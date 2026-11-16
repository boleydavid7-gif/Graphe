//! Validated source metadata carried around regenerated current document content.

use std::collections::BTreeMap;

use office_core::{Document, Id};
use quick_xml::{
    Reader, Writer, XmlVersion,
    events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event},
};

use crate::{
    CompatibilityReport, ConversionError, DocxImportOutcome, DocxSource,
    ancillary::AncillaryKind,
    conversion::invalid,
    export::{export_document, write_parts},
    import::{PartGraph, inspect_semantic_graph},
    import_docx,
    package::{Package, read_package},
    xml::parse_xml,
};

const WORD: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const WORD_STRICT: &str = "http://purl.oclc.org/ooxml/wordprocessingml/main";
const OFFICE_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const PACKAGE_REL: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const CONTENT_TYPES: &str = "http://schemas.openxmlformats.org/package/2006/content-types";
const DC: &str = "http://purl.org/dc/elements/1.1/";

/// Meaningful source features that a supported current-content copy cannot retain.
///
/// Source-only style galleries, package caches and non-content authoring data are
/// not current document content/formatting loss. Exact source recovery remains a
/// separate operation; this assessment never authorizes partial unsupported import.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DocxCopyAssessment {
    /// Active imported formatting or layout preferences were approximated.
    pub formatting_may_differ: bool,
    /// Source bookmark anchors have no current editable-model representation.
    pub bookmarks_omitted: bool,
    /// Descriptive/user-defined source properties beyond title and audit caches.
    pub has_document_properties: bool,
    /// An opaque source theme cannot be attached under the safe copy profile.
    pub theme_omitted: bool,
}

struct CopySource {
    package: Package,
    graph: PartGraph,
    report: CompatibilityReport,
    theme_omitted: bool,
}

fn copy_source(document: &Document, source: &DocxSource) -> Result<CopySource, ConversionError> {
    document.validate()?;
    // Package inspection alone does not establish semantic editability. Never
    // attach selected metadata to a partial projection of unsupported content.
    let imported = match import_docx(source.original_bytes(), validation_id())? {
        DocxImportOutcome::Editable(imported) => imported,
        DocxImportOutcome::Preserved { .. } => {
            return Err(ConversionError::UnsupportedExport {
                feature: "source-preserving export requires a completely supported source import"
                    .into(),
            });
        }
    };
    let package = read_package(source.original_bytes())?;
    let graph = inspect_semantic_graph(&package, source, &mut CompatibilityReport::default())?;
    let mut theme_omitted = false;
    for (&kind, part) in &graph.ancillary {
        if kind != AncillaryKind::Thumbnail {
            match validate_auxiliary_copy(kind, part, &package.parts[part]) {
                Err(ConversionError::UnsupportedExport { .. }) if kind == AncillaryKind::Theme => {
                    // This was safe to retain opaquely on import, but cannot
                    // be attached to a fresh package. Report the omission;
                    // never turn it into a dangling or active relationship.
                    theme_omitted = true;
                }
                value => value?,
            }
        }
    }
    Ok(CopySource {
        package,
        graph,
        report: imported.report().clone(),
        theme_omitted,
    })
}

/// Assesses a copy using the same validated source profile as its exporter.
///
/// No original, document, editing history or storage association is modified.
pub fn assess_docx_copy(
    document: &Document,
    source: &DocxSource,
) -> Result<DocxCopyAssessment, ConversionError> {
    let input = copy_source(document, source)?;
    let mut result = assessment(&input.report);
    let namespace = word_namespace(source);
    let main = &source.inspection().main_document_part;
    let xml = parse_xml(main, &input.package.parts[main])?;
    // `_GoBack` is Word's known cursor-position cache, not a user anchor. Do
    // not broadly ignore underscore-prefixed named bookmarks or rely on the
    // bounded normalization list to detect lost navigation features.
    result.bookmarks_omitted = xml.descendants().any(|node| {
        node.has_tag_name((namespace, "bookmarkStart"))
            && node.attribute((namespace, "name")) != Some("_GoBack")
    });
    result.has_document_properties = has_properties(&input)?;
    result.theme_omitted = input.theme_omitted;
    Ok(result)
}

fn assessment(report: &CompatibilityReport) -> DocxCopyAssessment {
    DocxCopyAssessment {
        formatting_may_differ: report.has_formatting_normalizations,
        bookmarks_omitted: false,
        has_document_properties: false,
        theme_omitted: false,
    }
}

pub(crate) fn meaningful_normalization(path: &str) -> bool {
    !matches!(
        path,
        "bookmark"
            | "latentStyles"
            | "style/unused"
            | "style/name"
            | "rPr/rStyle"
            | "font/altName"
            | "settings/mathPr"
            | "settings/chartTrackingRefBased"
            | "settings/shapeDefaults"
            | "settings/decimalSymbol"
            | "settings/listSeparator"
            | "style/qFormat"
            | "style/semiHidden"
            | "style/unhideWhenUsed"
            | "style/hidden"
            | "style/locked"
            | "style/autoRedefine"
            | "style/personal"
            | "style/personalCompose"
            | "style/personalReply"
            | "style/uiPriority"
            | "style/rsid"
            | "style/next"
            | "style/link"
            | "style/aliases"
    )
}

/// Exports current supported content while retaining validated source metadata.
///
/// The body and styles are always regenerated from `document`. Known core/custom
/// metadata, static extended properties, theme/font descriptors and inert settings
/// are carried through by exact relationship roles and validated payloads. Source
/// thumbnails, computed caches and normalized active settings are not copied.
/// Strict sources are emitted as the exporter's Transitional package profile.
/// The complete output is reinspected and semantically validated before return.
pub fn export_docx_with_source(
    document: &Document,
    source: &DocxSource,
) -> Result<Vec<u8>, ConversionError> {
    let input = copy_source(document, source)?;
    let generated = export_document(document)?;
    let expected = editable_projection(&generated)?;
    let mut parts = read_package(&generated)?.parts;
    let has_numbering = parts.contains_key("word/numbering.xml");
    let mut extra = Vec::new();
    if let Some(part) = &input.graph.core_properties {
        let metadata = metadata(
            part,
            &input.package.parts[part],
            Some(&document.metadata().title),
        )?;
        parts.insert(
            "docProps/core.xml".into(),
            normalize_namespace_declarations(part, &metadata)?,
        );
    }
    for (&kind, part) in &input.graph.ancillary {
        if kind == AncillaryKind::Thumbnail || (kind == AncillaryKind::Theme && input.theme_omitted)
        {
            // A cached image can contain text deleted from the edited body.
            continue;
        }
        if matches!(kind, AncillaryKind::Settings | AncillaryKind::WebSettings) {
            // Assess each settings part independently: a large style library
            // may already have exhausted the source report's notice budget.
            let mut settings = CompatibilityReport::default();
            kind.inspect(
                part,
                &input.package.parts[part],
                word_namespace(source),
                &mut settings,
            )?;
            if !settings.is_supported()
                || !settings.normalizations.is_empty()
                || settings.omitted_normalizations > 0
            {
                // Reattaching active compatibility/layout defaults could
                // override the current model's regenerated formatting in Word.
                continue;
            }
        }
        let bytes = match kind {
            AncillaryKind::ExtendedProperties => metadata(part, &input.package.parts[part], None)?,
            _ => input.package.parts[part].clone(),
        };
        let bytes = normalize_namespace_declarations(part, &bytes)?;
        let destination = destination(kind);
        parts.insert(destination.into(), bytes);
        extra.push((kind, destination));
    }
    parts.insert("_rels/.rels".into(), relationships(true, has_numbering, &extra)?);
    parts.insert(
        "word/_rels/document.xml.rels".into(),
        relationships(false, has_numbering, &extra)?,
    );
    parts.insert("[Content_Types].xml".into(), content_types(has_numbering, &extra)?);
    let bytes = write_parts(parts)?;
    // Original styles/defaults and opaque ancillary relationships never enter
    // the regenerated semantic graph. Keep every copied construct within the
    // established editable profile; no partially valid bytes are published.
    if editable_projection(&bytes)? != expected {
        return Err(ConversionError::UnsupportedExport {
            feature: "preserved metadata would change current document content or formatting"
                .into(),
        });
    }
    Ok(bytes)
}

fn validation_id() -> Id {
    // Imported comparison IDs are internal; callers may use longer native IDs
    // than the import API's external identifier budget.
    Id::new("docx-copy-validation").expect("fixed validation identifier is valid")
}

fn editable_projection(bytes: &[u8]) -> Result<Document, ConversionError> {
    match import_docx(bytes, validation_id())? {
        DocxImportOutcome::Editable(imported) => Ok(imported.document().clone()),
        DocxImportOutcome::Preserved { .. } => Err(ConversionError::UnsupportedExport {
            feature: "preserved metadata would leave the supported editable profile".into(),
        }),
    }
}

fn destination(kind: AncillaryKind) -> &'static str {
    match kind {
        AncillaryKind::ExtendedProperties => "docProps/app.xml",
        AncillaryKind::CustomProperties => "docProps/custom.xml",
        AncillaryKind::Theme => "word/theme/theme1.xml",
        AncillaryKind::FontTable => "word/fontTable.xml",
        AncillaryKind::Settings => "word/settings.xml",
        AncillaryKind::WebSettings => "word/webSettings.xml",
        AncillaryKind::Thumbnail => {
            unreachable!("cached previews are never carried into edited copies")
        }
    }
}

fn relationship_name(kind: AncillaryKind) -> &'static str {
    match kind {
        AncillaryKind::ExtendedProperties => "extended-properties",
        AncillaryKind::CustomProperties => "custom-properties",
        AncillaryKind::Theme => "theme",
        AncillaryKind::FontTable => "fontTable",
        AncillaryKind::Settings => "settings",
        AncillaryKind::WebSettings => "webSettings",
        AncillaryKind::Thumbnail => unreachable!("cached previews have no generated relationship"),
    }
}

fn is_root(kind: AncillaryKind) -> bool {
    matches!(
        kind,
        AncillaryKind::ExtendedProperties | AncillaryKind::CustomProperties
    )
}

fn relationships(
    root: bool, has_numbering: bool, extra: &[(AncillaryKind, &str)],
) -> Result<Vec<u8>, ConversionError> {
    let mut xml = Writer::new(Vec::new());
    let mut start = BytesStart::new("Relationships");
    start.push_attribute(("xmlns", PACKAGE_REL));
    event(&mut xml, Event::Start(start))?;
    let mut required = if root {
        vec![
            ("rIdDocument", format!("{OFFICE_REL}/officeDocument"), "word/document.xml"),
            ("rIdCore", "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties".into(), "docProps/core.xml"),
        ]
    } else {
        vec![("rIdStyles", format!("{OFFICE_REL}/styles"), "styles.xml")]
    };
    if !root && has_numbering {
        required.push(("rIdNumbering", format!("{OFFICE_REL}/numbering"), "numbering.xml"));
    }
    for (id, kind, target) in required {
        relation(&mut xml, id, &kind, target)?;
    }
    for (index, &(kind, target)) in extra.iter().enumerate() {
        if is_root(kind) != root {
            continue;
        }
        let relationship = format!("{OFFICE_REL}/{}", relationship_name(kind));
        relation(
            &mut xml,
            &format!("rIdSource{index}"),
            &relationship,
            if root {
                target
            } else {
                target
                    .strip_prefix("word/")
                    .expect("main auxiliaries are in word/")
            },
        )?;
    }
    event(&mut xml, Event::End(BytesEnd::new("Relationships")))?;
    Ok(xml.into_inner())
}

fn relation(
    xml: &mut Writer<Vec<u8>>,
    id: &str,
    kind: &str,
    target: &str,
) -> Result<(), ConversionError> {
    let mut element = BytesStart::new("Relationship");
    element.push_attribute(("Id", id));
    element.push_attribute(("Type", kind));
    element.push_attribute(("Target", target));
    event(xml, Event::Empty(element))
}

fn content_types(
    has_numbering: bool, extra: &[(AncillaryKind, &str)],
) -> Result<Vec<u8>, ConversionError> {
    let mut xml = Writer::new(Vec::new());
    let mut start = BytesStart::new("Types");
    start.push_attribute(("xmlns", CONTENT_TYPES));
    event(&mut xml, Event::Start(start))?;
    let mut rels = BytesStart::new("Default");
    rels.push_attribute(("Extension", "rels"));
    rels.push_attribute((
        "ContentType",
        "application/vnd.openxmlformats-package.relationships+xml",
    ));
    event(&mut xml, Event::Empty(rels))?;
    let mut types = BTreeMap::from([
        (
            "word/document.xml",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
        ),
        (
            "word/styles.xml",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml",
        ),
        (
            "docProps/core.xml",
            "application/vnd.openxmlformats-package.core-properties+xml",
        ),
    ]);
    if has_numbering {
        types.insert("word/numbering.xml",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml");
    }
    for &(kind, target) in extra {
        types.insert(target, kind.expected_content_type());
    }
    for (part, kind) in types {
        let mut element = BytesStart::new("Override");
        let name = format!("/{part}");
        element.push_attribute(("PartName", name.as_str()));
        element.push_attribute(("ContentType", kind));
        event(&mut xml, Event::Empty(element))?;
    }
    event(&mut xml, Event::End(BytesEnd::new("Types")))?;
    Ok(xml.into_inner())
}

fn metadata(part: &str, bytes: &[u8], title: Option<&str>) -> Result<Vec<u8>, ConversionError> {
    parse_xml(part, bytes)?;
    let mut reader = Reader::from_reader(bytes);
    let mut xml = Writer::new(Vec::new());
    event(
        &mut xml,
        Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)),
    )?;
    let mut depth = 0_usize;
    let mut keep = false;
    let mut title_written = false;
    loop {
        let value = reader
            .read_event()
            .map_err(|error| invalid(part, error.to_string()))?;
        let empty = matches!(value, Event::Empty(_));
        match value {
            Event::Start(start) | Event::Empty(start) => {
                if depth == 0 {
                    let end = start.to_end().into_owned();
                    event(&mut xml, Event::Start(start))?;
                    if empty {
                        if let Some(title) = title {
                            new_title(&mut xml, title)?;
                        }
                        event(&mut xml, Event::End(end))?;
                        break;
                    }
                } else if depth == 1 {
                    let name = start.local_name();
                    let name = name.as_ref();
                    keep = if let Some(title) = title {
                        if name == "title" {
                            let end = start.to_end().into_owned();
                            event(&mut xml, Event::Start(start.to_owned()))?;
                            event(&mut xml, Event::Text(BytesText::new(title)))?;
                            event(&mut xml, Event::End(end))?;
                            title_written = true;
                            false
                        } else {
                            !matches!(
                                name,
                                "modified" | "revision" | "lastPrinted" | "lastModifiedBy"
                            )
                        }
                    } else {
                        // Heading/title caches, statistics, hyperlinks,
                        // security flags and counters can expose stale text.
                        matches!(
                            name,
                            "Application"
                                | "AppVersion"
                                | "Company"
                                | "Manager"
                                | "Template"
                                | "PresentationFormat"
                                | "HyperlinkBase"
                        )
                    };
                    if keep {
                        event(
                            &mut xml,
                            if empty {
                                Event::Empty(start)
                            } else {
                                Event::Start(start)
                            },
                        )?;
                    }
                } else if keep {
                    event(
                        &mut xml,
                        if empty {
                            Event::Empty(start)
                        } else {
                            Event::Start(start)
                        },
                    )?;
                }
                if !empty {
                    depth += 1;
                }
            }
            Event::End(end) => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    if let Some(title) = title
                        && !title_written
                    {
                        new_title(&mut xml, title)?;
                    }
                    event(&mut xml, Event::End(end))?;
                    break;
                } else if keep {
                    event(&mut xml, Event::End(end))?;
                }
            }
            Event::Text(_) | Event::CData(_) | Event::GeneralRef(_) if keep && depth > 1 => {
                event(&mut xml, value)?
            }
            Event::Eof => return Err(invalid(part, "missing metadata root end")),
            _ => {} // Outside whitespace, XML comments and PIs are not metadata values.
        }
    }
    let bytes = xml.into_inner();
    parse_xml(part, &bytes)?;
    Ok(bytes)
}

fn new_title(xml: &mut Writer<Vec<u8>>, title: &str) -> Result<(), ConversionError> {
    let mut element = BytesStart::new("dc:title");
    element.push_attribute(("xmlns:dc", DC));
    event(xml, Event::Start(element))?;
    event(xml, Event::Text(BytesText::new(title)))?;
    event(xml, Event::End(BytesEnd::new("dc:title")))
}

fn word_namespace(source: &DocxSource) -> &'static str {
    match source.inspection().conformance {
        crate::DocxConformance::Transitional => WORD,
        crate::DocxConformance::Strict => WORD_STRICT,
    }
}

fn has_properties(source: &CopySource) -> Result<bool, ConversionError> {
    if let Some(part) = &source.graph.core_properties {
        let xml = parse_xml(part, &source.package.parts[part])?;
        if xml
            .root_element()
            .children()
            .filter(roxmltree::Node::is_element)
            .any(|node| {
                !matches!(
                    node.tag_name().name(),
                    "title"
                        | "created"
                        | "modified"
                        | "revision"
                        | "lastPrinted"
                        | "lastModifiedBy"
                ) && node.text().is_some_and(|text| !text.trim().is_empty())
            })
        {
            return Ok(true);
        }
    }
    for (&kind, part) in &source.graph.ancillary {
        if !matches!(
            kind,
            AncillaryKind::CustomProperties | AncillaryKind::ExtendedProperties
        ) {
            continue;
        }
        let xml = parse_xml(part, &source.package.parts[part])?;
        if xml
            .root_element()
            .children()
            .filter(roxmltree::Node::is_element)
            .any(|node| {
                if kind == AncillaryKind::CustomProperties {
                    node.tag_name().name() == "property"
                } else {
                    matches!(
                        node.tag_name().name(),
                        "Company" | "Manager" | "Template" | "PresentationFormat" | "HyperlinkBase"
                    ) && node.text().is_some_and(|text| !text.trim().is_empty())
                }
            })
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn validate_auxiliary_copy(
    kind: AncillaryKind,
    part: &str,
    bytes: &[u8],
) -> Result<(), ConversionError> {
    let xml = parse_xml(part, bytes)?;
    for node in xml.descendants().filter(roxmltree::Node::is_element) {
        if node.attributes().any(|attribute| {
            matches!(
                attribute.namespace(),
                Some(OFFICE_REL | "http://purl.oclc.org/ooxml/officeDocument/relationships")
            )
        }) {
            return Err(ConversionError::UnsupportedExport {
                feature: format!(
                    "source metadata in {part} contains a relationship reference that cannot be safely carried into this copy"
                ),
            });
        }
        if kind != AncillaryKind::Theme {
            continue;
        }
        let drawing = matches!(
            node.tag_name().namespace(),
            Some(
                "http://schemas.openxmlformats.org/drawingml/2006/main"
                    | "http://purl.oclc.org/ooxml/drawingml/main"
            )
        );
        // The existing import retains theme XML opaquely. The copy profile is
        // narrower: DrawingML theme data and Microsoft's known inert family
        // descriptor, with no referenced resources or foreign active payload.
        let family = node.has_tag_name((
            "http://schemas.microsoft.com/office/thememl/2012/main",
            "themeFamily",
        )) && !node.children().any(|child| child.is_element())
            && node
                .children()
                .filter(roxmltree::Node::is_text)
                .all(|child| child.text().is_none_or(|text| text.trim().is_empty()))
            && node.attributes().all(|attribute| {
                attribute.namespace().is_none() && matches!(attribute.name(), "name" | "id" | "vid")
            });
        let inert_attributes = node.attributes().all(|attribute| {
            attribute.namespace().is_none()
                || (attribute.namespace() == Some("http://www.w3.org/XML/1998/namespace")
                    && matches!(attribute.name(), "space" | "lang"))
                || (attribute.namespace()
                    == Some("http://schemas.openxmlformats.org/markup-compatibility/2006")
                    && attribute.name() == "Ignorable"
                    && attribute
                        .value()
                        .split_whitespace()
                        .all(|prefix| node.lookup_namespace_uri(Some(prefix)).is_some()))
        });
        let active_drawing = matches!(node.tag_name().name(), "hlinkClick" | "hlinkMouseOver");
        if (!drawing && !family) || !inert_attributes || active_drawing {
            return Err(ConversionError::UnsupportedExport {
                feature: format!(
                    "source theme extension in {part} cannot be safely carried into this copy"
                ),
            });
        }
    }
    Ok(())
}

fn normalize_namespace_declarations(part: &str, bytes: &[u8]) -> Result<Vec<u8>, ConversionError> {
    let mut reader = Reader::from_reader(bytes);
    let mut xml = Writer::new(Vec::new());
    loop {
        let value = reader
            .read_event()
            .map_err(|error| invalid(part, error.to_string()))?;
        let empty = matches!(value, Event::Empty(_));
        match value {
            Event::Start(start) | Event::Empty(start) => {
                let mut output = start.to_owned();
                output.clear_attributes();
                for attr in start.attributes().with_checks(false) {
                    let attr = attr.map_err(|error| invalid(part, error.to_string()))?;
                    let declaration =
                        attr.key.as_ref() == "xmlns" || attr.key.as_ref().starts_with("xmlns:");
                    let value = attr
                        .normalized_value(XmlVersion::Explicit1_0)
                        .map_err(|error| invalid(part, error.to_string()))?;
                    if let Some(namespace) = declaration
                        .then(|| transitional_namespace(value.as_ref()))
                        .flatten()
                    {
                        output.push_attribute((attr.key.as_ref(), namespace));
                    } else {
                        output.push_attribute(attr);
                    }
                }
                event(
                    &mut xml,
                    if empty {
                        Event::Empty(output)
                    } else {
                        Event::Start(output)
                    },
                )?;
            }
            Event::Comment(_) | Event::PI(_) => {}
            Event::Eof => break,
            value => event(&mut xml, value)?,
        }
    }
    let bytes = xml.into_inner();
    parse_xml(part, &bytes)?;
    Ok(bytes)
}

fn transitional_namespace(namespace: &str) -> Option<&'static str> {
    match namespace {
        WORD_STRICT => Some(WORD),
        "http://purl.oclc.org/ooxml/officeDocument/relationships" => Some(OFFICE_REL),
        "http://purl.oclc.org/ooxml/officeDocument/extendedProperties" => {
            Some("http://schemas.openxmlformats.org/officeDocument/2006/extended-properties")
        }
        "http://purl.oclc.org/ooxml/officeDocument/customProperties" => {
            Some("http://schemas.openxmlformats.org/officeDocument/2006/custom-properties")
        }
        "http://purl.oclc.org/ooxml/officeDocument/docPropsVTypes" => {
            Some("http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes")
        }
        "http://purl.oclc.org/ooxml/drawingml/main" => {
            Some("http://schemas.openxmlformats.org/drawingml/2006/main")
        }
        "http://purl.oclc.org/ooxml/officeDocument/math" => {
            Some("http://schemas.openxmlformats.org/officeDocument/2006/math")
        }
        _ => None,
    }
}

fn event(xml: &mut Writer<Vec<u8>>, value: Event<'_>) -> Result<(), ConversionError> {
    xml.write_event(value)
        .map_err(|error| invalid("DOCX copy generation", error.to_string()))
}
