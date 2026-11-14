//! Strict semantic DOCX import with a preserved read-only outcome for gaps.

use std::collections::{BTreeMap, BTreeSet};

use office_core::{
    Block, CharacterFormat, Document, DocumentMetadata, Id, NamedStyle, Paragraph, ParagraphFormat,
    Section, SectionProperties, TextRun,
};
use roxmltree::{Document as XmlDocument, Node};

use crate::{
    CompatibilityIssueKind as Issue, CompatibilityReport, ConversionError, DocxConformance,
    DocxError, DocxImportOutcome, DocxSource, ImportedDocx,
    conversion::{bounded_label, invalid},
    opc::{parse_relationships, relationship_source, resolve_target},
    package::{Package, read_package},
    properties::{parse_character, parse_paragraph},
    xml::parse_xml,
};

const WORD_TRANSITIONAL: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const WORD_STRICT: &str = "http://purl.oclc.org/ooxml/wordprocessingml/main";
const REL_TRANSITIONAL: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/";
const REL_STRICT: &str = "http://purl.oclc.org/ooxml/officeDocument/relationships/";
const CORE_RELATIONSHIP: &str =
    "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties";
const CORE_NAMESPACE: &str =
    "http://schemas.openxmlformats.org/package/2006/metadata/core-properties";
const DC_NAMESPACE: &str = "http://purl.org/dc/elements/1.1/";
const XML_NAMESPACE: &str = "http://www.w3.org/XML/1998/namespace";
const STYLES_CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml";
const CORE_CONTENT_TYPE: &str = "application/vnd.openxmlformats-package.core-properties+xml";
const MAX_IMPORT_ID_BYTES: usize = 256;

struct PartGraph {
    styles: Option<String>,
    core_properties: Option<String>,
}

#[derive(Default)]
struct StyleBook {
    definitions: Vec<NamedStyle>,
    identifiers: BTreeMap<String, Id>,
    default_style: Option<Id>,
}

struct RawStyle {
    word_id: String,
    name: String,
    parent: Option<String>,
    paragraph: ParagraphFormat,
    character: CharacterFormat,
    is_default: bool,
}

pub(crate) fn import_document(
    bytes: &[u8],
    document_id: Id,
) -> Result<DocxImportOutcome, ConversionError> {
    if document_id.as_str().len() > MAX_IMPORT_ID_BYTES {
        return Err(DocxError::LimitExceeded {
            resource: "import_document_id_bytes",
            limit: MAX_IMPORT_ID_BYTES as u64,
            actual: document_id.as_str().len() as u64,
        }
        .into());
    }
    let original = DocxSource::open(bytes)?;
    let package = read_package(bytes)?;
    let mut report = CompatibilityReport::default();
    let graph = inspect_semantic_graph(&package, &original, &mut report)?;
    let namespace = match original.inspection().conformance {
        DocxConformance::Transitional => WORD_TRANSITIONAL,
        DocxConformance::Strict => WORD_STRICT,
    };
    let styles = match graph.styles {
        Some(ref part) => parse_styles(
            part,
            &package.parts[part],
            namespace,
            &document_id,
            &mut report,
        )?,
        None => StyleBook::default(),
    };
    let metadata = match graph.core_properties {
        Some(ref part) => parse_metadata(part, &package.parts[part], &mut report)?,
        None => DocumentMetadata::default(),
    };
    let main_part = &original.inspection().main_document_part;
    let sections = parse_main_document(
        main_part,
        &package.parts[main_part],
        namespace,
        &document_id,
        &styles,
        &mut report,
    )?;
    if styles.default_style.is_some() && sections.iter().all(|section| section.blocks().is_empty())
    {
        report.push(
            Issue::UnsupportedValue,
            main_part,
            "body",
            "An empty body with document/default-style formatting has no native paragraph to retain that default for future typing.",
        );
    }
    if !report.is_supported() {
        return Ok(DocxImportOutcome::Preserved { original, report });
    }
    let document = Document::from_parts(document_id, metadata, sections, styles.definitions)?;
    Ok(DocxImportOutcome::Editable(ImportedDocx {
        document,
        original,
    }))
}

fn inspect_semantic_graph(
    package: &Package,
    original: &DocxSource,
    report: &mut CompatibilityReport,
) -> Result<PartGraph, ConversionError> {
    let main_part = &original.inspection().main_document_part;
    let lookup: BTreeMap<_, _> = package
        .parts
        .keys()
        .map(|part| (part.to_ascii_lowercase(), part.as_str()))
        .collect();
    let mut graph = PartGraph {
        styles: None,
        core_properties: None,
    };
    let mut understood = BTreeSet::from([main_part.clone()]);
    for part in &original.inspection().parts {
        if !part.name.to_ascii_lowercase().ends_with(".rels") {
            continue;
        }
        understood.insert(part.name.clone());
        let source = relationship_source(&part.name)?;
        for relation in parse_relationships(&part.name, &package.parts[&part.name])? {
            if relation.external {
                report.push(
                    Issue::UnsupportedRelationship,
                    &part.name,
                    "Relationship",
                    "External relationships have no editable semantic mapping.",
                );
                continue;
            }
            let resolved = resolve_target(source.as_deref(), &relation.target, &part.name)?;
            let target = lookup
                .get(&resolved.to_ascii_lowercase())
                .ok_or_else(|| invalid(&part.name, "relationship target is missing"))?;
            let main_source = source
                .as_deref()
                .is_some_and(|value| value.eq_ignore_ascii_case(main_part));
            if source.is_none() && relation_kind(&relation.relationship_type, "officeDocument") {
                continue;
            }
            if main_source && relation_kind(&relation.relationship_type, "styles") {
                if graph.styles.replace((*target).into()).is_some() {
                    return Err(invalid(
                        &part.name,
                        "multiple styles relationships are ambiguous",
                    ));
                }
                understood.insert((*target).into());
            } else if source.is_none() && relation.relationship_type == CORE_RELATIONSHIP {
                if graph.core_properties.replace((*target).into()).is_some() {
                    return Err(invalid(
                        &part.name,
                        "multiple core-properties relationships are ambiguous",
                    ));
                }
                understood.insert((*target).into());
            } else {
                report.push(
                    Issue::UnsupportedRelationship,
                    &part.name,
                    "Relationship",
                    "This relationship requires unsupported document semantics.",
                );
            }
        }
    }
    for part in &original.inspection().parts {
        if !understood.contains(&part.name) {
            report.push(
                Issue::UnsupportedPart,
                &part.name,
                "/",
                "This package part has no supported semantic mapping.",
            );
        }
        if graph.styles.as_deref() == Some(part.name.as_str())
            && !part.content_type.eq_ignore_ascii_case(STYLES_CONTENT_TYPE)
        {
            return Err(invalid(
                &part.name,
                "styles relationship has an incorrect content type",
            ));
        }
        if graph.core_properties.as_deref() == Some(part.name.as_str())
            && !part.content_type.eq_ignore_ascii_case(CORE_CONTENT_TYPE)
        {
            return Err(invalid(
                &part.name,
                "core-properties relationship has an incorrect content type",
            ));
        }
    }
    Ok(graph)
}

fn relation_kind(value: &str, kind: &str) -> bool {
    value.strip_prefix(REL_TRANSITIONAL) == Some(kind)
        || value.strip_prefix(REL_STRICT) == Some(kind)
}

fn parse_styles(
    part: &str,
    bytes: &[u8],
    namespace: &str,
    document_id: &Id,
    report: &mut CompatibilityReport,
) -> Result<StyleBook, ConversionError> {
    let xml = parse_xml(part, bytes)?;
    let root = require_root(&xml, namespace, "styles", part)?;
    check_word_attributes(root, namespace, &[], part, report);
    let mut defaults = None;
    let mut raw = Vec::new();
    let mut identifiers = BTreeSet::new();
    for node in root.children() {
        if !node.is_element() {
            check_whitespace(node, part)?;
        } else if node.has_tag_name((namespace, "docDefaults")) {
            if defaults.is_some() {
                return Err(invalid(part, "duplicate document defaults"));
            }
            defaults = Some(parse_defaults(node, namespace, part, report)?);
        } else if node.has_tag_name((namespace, "style")) {
            check_word_attributes(
                node,
                namespace,
                &["type", "styleId", "default"],
                part,
                report,
            );
            let word_id = required_word_attribute(node, namespace, "styleId", part)?.to_owned();
            if !identifiers.insert(word_id.clone()) {
                return Err(invalid(part, "duplicate style identifiers"));
            }
            if node.attribute((namespace, "type")) != Some("paragraph") {
                unsupported_element(
                    node,
                    part,
                    report,
                    "Only paragraph style definitions are supported.",
                );
                continue;
            }
            raw.push(parse_style(node, namespace, part, report, word_id)?);
        } else {
            unsupported_element(
                node,
                part,
                report,
                "This styles construct is not supported.",
            );
        }
    }
    if !report.is_supported() {
        // Unprocessed style extensions may contain the referenced definitions.
        // Do not classify those references as corrupt or invent replacements.
        // No semantic document will be published for this read-only outcome.
        return Ok(StyleBook::default());
    }
    build_styles(raw, defaults, document_id, part, report)
}

fn parse_defaults(
    node: Node<'_, '_>,
    namespace: &str,
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<(CharacterFormat, ParagraphFormat), ConversionError> {
    check_word_attributes(node, namespace, &[], part, report);
    let mut character = CharacterFormat::default();
    let mut paragraph = ParagraphFormat::default();
    let mut seen = BTreeSet::new();
    for wrapper in node.children() {
        if !wrapper.is_element() {
            check_whitespace(wrapper, part)?;
            continue;
        }
        let kind = if wrapper.has_tag_name((namespace, "rPrDefault")) {
            "rPr"
        } else if wrapper.has_tag_name((namespace, "pPrDefault")) {
            "pPr"
        } else {
            unsupported_element(wrapper, part, report, "Unknown document-default construct.");
            continue;
        };
        if !seen.insert(kind) {
            return Err(invalid(part, "duplicate document-default property wrapper"));
        }
        check_word_attributes(wrapper, namespace, &[], part, report);
        let mut properties_seen = false;
        for properties in wrapper.children() {
            if !properties.is_element() {
                check_whitespace(properties, part)?;
            } else if properties.has_tag_name((namespace, kind)) {
                if properties_seen {
                    return Err(invalid(part, "duplicate document-default property record"));
                }
                properties_seen = true;
                if kind == "rPr" {
                    character = parse_character(properties, namespace, part, report)?;
                } else {
                    let parsed = parse_paragraph(properties, namespace, part, report)?;
                    if parsed.style_id.is_some() || parsed.section.is_some() {
                        unsupported_element(
                            properties,
                            part,
                            report,
                            "Style references and section boundaries are not paragraph defaults.",
                        );
                    }
                    paragraph = parsed.format;
                }
            } else {
                unsupported_element(
                    properties,
                    part,
                    report,
                    "Unexpected document-default child.",
                );
            }
        }
    }
    Ok((character, paragraph))
}

fn parse_style(
    node: Node<'_, '_>,
    namespace: &str,
    part: &str,
    report: &mut CompatibilityReport,
    word_id: String,
) -> Result<RawStyle, ConversionError> {
    let is_default = node
        .attribute((namespace, "default"))
        .map(|value| on_off(value, part))
        .transpose()?
        .unwrap_or(false);
    let mut name = None;
    let mut parent = None;
    let mut paragraph = ParagraphFormat::default();
    let mut character = CharacterFormat::default();
    let mut seen = BTreeSet::new();
    for child in node.children() {
        if !child.is_element() {
            check_whitespace(child, part)?;
            continue;
        }
        if child.tag_name().namespace() != Some(namespace) {
            unsupported_element(child, part, report, "Unknown style extension.");
            continue;
        }
        let kind = child.tag_name().name();
        if !matches!(kind, "name" | "basedOn" | "pPr" | "rPr") {
            unsupported_element(child, part, report, "This style property is not supported.");
            continue;
        }
        if !seen.insert(kind) {
            return Err(invalid(part, "duplicate style property"));
        }
        match kind {
            "name" | "basedOn" => {
                check_word_attributes(child, namespace, &["val"], part, report);
                check_leaf(child, part, report)?;
                let value = required_word_attribute(child, namespace, "val", part)?.to_owned();
                if kind == "name" {
                    name = Some(value);
                } else {
                    parent = Some(value);
                }
            }
            "rPr" => character = parse_character(child, namespace, part, report)?,
            "pPr" => {
                let parsed = parse_paragraph(child, namespace, part, report)?;
                if parsed.style_id.is_some() || parsed.section.is_some() {
                    unsupported_element(
                        child,
                        part,
                        report,
                        "Paragraph styles cannot carry a paragraph style reference or section boundary.",
                    );
                }
                paragraph = parsed.format;
            }
            _ => unreachable!(),
        }
    }
    let name = name.unwrap_or_else(|| {
        report.push(
            Issue::UnsupportedValue,
            part,
            "style/name",
            "A named style without an explicit display name is outside the editable profile.",
        );
        word_id.clone()
    });
    Ok(RawStyle {
        word_id,
        name,
        parent,
        paragraph,
        character,
        is_default,
    })
}

fn build_styles(
    raw: Vec<RawStyle>,
    defaults: Option<(CharacterFormat, ParagraphFormat)>,
    document_id: &Id,
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<StyleBook, ConversionError> {
    let defaults = defaults.filter(|(character, paragraph)| {
        *character != CharacterFormat::default() || *paragraph != ParagraphFormat::default()
    });
    let mut book = StyleBook::default();
    let mut names = BTreeSet::new();
    for style in &raw {
        if style.name.trim().is_empty() || !names.insert(style.name.clone()) {
            report.push(
                Issue::UnsupportedValue,
                part,
                "style/name",
                "Blank or duplicate style display names cannot be represented in the native model.",
            );
        }
    }
    let defaults_id = if let Some((character, paragraph)) = defaults {
        let id = generated_id(document_id, "style", 0)?;
        let mut name = "DOCX Document Defaults".to_owned();
        let mut suffix = 1;
        while names.contains(&name) {
            name = format!("DOCX Document Defaults {suffix}");
            suffix += 1;
        }
        let mut style = NamedStyle::new(id.clone(), name);
        style.character_format = character;
        style.paragraph_format = paragraph;
        book.definitions.push(style);
        Some(id)
    } else {
        None
    };
    let offset = usize::from(defaults_id.is_some());
    let indices: BTreeMap<_, _> = raw
        .iter()
        .enumerate()
        .map(|(index, style)| (style.word_id.as_str(), index))
        .collect();
    for (index, style) in raw.iter().enumerate() {
        let id = generated_id(document_id, "style", index + offset)?;
        book.identifiers.insert(style.word_id.clone(), id);
    }
    let mut effective = vec![None; raw.len()];
    for start in 0..raw.len() {
        if effective[start].is_some() {
            continue;
        }
        let mut chain = Vec::new();
        let mut path = BTreeSet::new();
        let mut current = Some(start);
        while let Some(index) = current {
            if effective[index].is_some() {
                break;
            }
            if !path.insert(index) {
                return Err(invalid(
                    part,
                    "paragraph-style inheritance contains a cycle",
                ));
            }
            chain.push(index);
            current = raw[index]
                .parent
                .as_deref()
                .map(|parent| {
                    indices
                        .get(parent)
                        .copied()
                        .ok_or_else(|| invalid(part, "paragraph style has an unresolved parent"))
                })
                .transpose()?;
        }
        let mut inherited = current
            .and_then(|index| effective[index])
            .unwrap_or_else(|| {
                defaults_id.as_ref().map_or((false, false), |_| {
                    let defaults = &book.definitions[0].character_format;
                    (
                        defaults.bold.unwrap_or(false),
                        defaults.italic.unwrap_or(false),
                    )
                })
            });
        for index in chain.into_iter().rev() {
            if raw[index].character.bold == Some(true) {
                inherited.0 = !inherited.0;
            }
            if raw[index].character.italic == Some(true) {
                inherited.1 = !inherited.1;
            }
            effective[index] = Some(inherited);
        }
    }
    for (index, raw_style) in raw.into_iter().enumerate() {
        let id = book.identifiers[&raw_style.word_id].clone();
        let mut style = NamedStyle::new(id.clone(), raw_style.name);
        style.based_on = raw_style
            .parent
            .as_ref()
            .map(|parent| book.identifiers[parent].clone())
            .or_else(|| defaults_id.clone());
        style.paragraph_format = raw_style.paragraph;
        style.character_format = raw_style.character;
        // In styles, true toggles inherited state while false is a no-op. The
        // shared model stores an explicit current absolute override instead.
        let values = effective[index].expect("all style chains were resolved");
        style.character_format.bold =
            (style.character_format.bold == Some(true)).then_some(values.0);
        style.character_format.italic =
            (style.character_format.italic == Some(true)).then_some(values.1);
        if raw_style.is_default && book.default_style.replace(id).is_some() {
            return Err(invalid(
                part,
                "multiple default paragraph styles are ambiguous",
            ));
        }
        book.definitions.push(style);
    }
    if book.default_style.is_none() {
        book.default_style = defaults_id;
    }
    Ok(book)
}

fn parse_metadata(
    part: &str,
    bytes: &[u8],
    report: &mut CompatibilityReport,
) -> Result<DocumentMetadata, ConversionError> {
    let xml = parse_xml(part, bytes)?;
    let root = require_root(&xml, CORE_NAMESPACE, "coreProperties", part)?;
    check_word_attributes(root, CORE_NAMESPACE, &[], part, report);
    let mut title = None;
    for node in root.children() {
        if !node.is_element() {
            check_whitespace(node, part)?;
        } else if node.has_tag_name((DC_NAMESPACE, "title")) {
            if title.is_some() {
                return Err(invalid(part, "duplicate document title"));
            }
            check_word_attributes(node, DC_NAMESPACE, &[], part, report);
            let content = simple_text(node, part, report)?;
            if content.contains('\r') {
                report.push(Issue::UnsupportedValue, part, "coreProperties/title",
                    "Literal carriage returns in title metadata are outside the editable conversion profile.");
            }
            title = Some(content);
        } else {
            unsupported_element(
                node,
                part,
                report,
                "This metadata field is preserved only in the original source.",
            );
        }
    }
    Ok(DocumentMetadata {
        title: title.unwrap_or_default(),
    })
}

fn parse_main_document(
    part: &str,
    bytes: &[u8],
    namespace: &str,
    document_id: &Id,
    styles: &StyleBook,
    report: &mut CompatibilityReport,
) -> Result<Vec<Section>, ConversionError> {
    let xml = parse_xml(part, bytes)?;
    let root = require_root(&xml, namespace, "document", part)?;
    check_word_attributes(root, namespace, &[], part, report);
    let mut body = None;
    for node in root.children() {
        if !node.is_element() {
            check_whitespace(node, part)?;
        } else if node.has_tag_name((namespace, "body")) {
            if body.replace(node).is_some() {
                return Err(invalid(part, "duplicate document body"));
            }
        } else {
            unsupported_element(node, part, report, "Unsupported document-level content.");
        }
    }
    let body = body.ok_or_else(|| invalid(part, "document body is missing"))?;
    check_word_attributes(body, namespace, &[], part, report);
    let mut sections = Vec::new();
    let mut blocks = Vec::new();
    let mut paragraph_index = 0;
    let mut final_properties = None;
    let mut ended_section = false;
    for node in body.children() {
        if !node.is_element() {
            check_whitespace(node, part)?;
            continue;
        }
        if final_properties.is_some() {
            return Err(invalid(part, "content follows final section properties"));
        }
        if node.has_tag_name((namespace, "p")) {
            let (paragraph, properties) = parse_paragraph_content(
                node,
                namespace,
                part,
                document_id,
                paragraph_index,
                styles,
                report,
            )?;
            paragraph_index += 1;
            blocks.push(Block::Paragraph(paragraph));
            ended_section = false;
            if let Some(properties) = properties {
                append_section(
                    &mut sections,
                    std::mem::take(&mut blocks),
                    Some(properties),
                    document_id,
                )?;
                ended_section = true;
            }
        } else if node.has_tag_name((namespace, "sectPr")) {
            final_properties = Some(crate::properties::parse_section(
                node, namespace, part, report,
            )?);
        } else {
            unsupported_element(
                node,
                part,
                report,
                "This block content has no editable native representation.",
            );
        }
    }
    if !blocks.is_empty() || final_properties.is_some() || ended_section {
        append_section(&mut sections, blocks, final_properties, document_id)?;
    }
    Ok(sections)
}

fn append_section(
    sections: &mut Vec<Section>,
    blocks: Vec<Block>,
    properties: Option<SectionProperties>,
    document_id: &Id,
) -> Result<(), ConversionError> {
    let mut section = Section::new(
        generated_id(document_id, "section", sections.len())?,
        blocks,
    );
    if let Some(properties) = properties {
        section = section.with_properties(properties);
    }
    sections.push(section);
    Ok(())
}

fn parse_paragraph_content(
    node: Node<'_, '_>,
    namespace: &str,
    part: &str,
    document_id: &Id,
    index: usize,
    styles: &StyleBook,
    report: &mut CompatibilityReport,
) -> Result<(Paragraph, Option<SectionProperties>), ConversionError> {
    check_word_attributes(node, namespace, &[], part, report);
    let mut properties = None;
    let mut runs = Vec::new();
    for child in node.children() {
        if !child.is_element() {
            check_whitespace(child, part)?;
        } else if child.has_tag_name((namespace, "pPr")) {
            if properties.is_some() || !runs.is_empty() {
                return Err(invalid(part, "duplicate or misplaced paragraph properties"));
            }
            properties = Some(parse_paragraph(child, namespace, part, report)?);
        } else if child.has_tag_name((namespace, "r")) {
            runs.push(parse_run(child, namespace, part, report)?);
        } else {
            unsupported_element(
                child,
                part,
                report,
                "This inline content has no supported editable representation.",
            );
        }
    }
    let mut paragraph = Paragraph::new(generated_id(document_id, "paragraph", index)?, runs);
    let mut section = None;
    let explicit_style = if let Some(properties) = properties {
        paragraph = paragraph.with_format(properties.format);
        section = properties.section;
        properties.style_id
    } else {
        None
    };
    let style = if let Some(word_id) = explicit_style {
        match styles.identifiers.get(&word_id) {
            Some(id) => Some(id.clone()),
            None if !report.is_supported() => {
                report.push(Issue::UnsupportedValue, part, "pPr/pStyle",
                    "The style reference cannot be resolved without unsupported content processing.");
                None
            }
            None => {
                return Err(invalid(
                    part,
                    "paragraph references a missing paragraph style",
                ));
            }
        }
    } else {
        styles.default_style.clone()
    };
    if let Some(style) = style {
        paragraph = paragraph.with_style(style);
    }
    Ok((paragraph, section))
}

fn parse_run(
    node: Node<'_, '_>,
    namespace: &str,
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<TextRun, ConversionError> {
    check_word_attributes(node, namespace, &[], part, report);
    let mut format = None;
    let mut text = String::new();
    let mut content_seen = false;
    for child in node.children() {
        if !child.is_element() {
            check_whitespace(child, part)?;
            continue;
        }
        if child.has_tag_name((namespace, "rPr")) {
            if format.is_some() || content_seen {
                return Err(invalid(part, "duplicate or misplaced run properties"));
            }
            format = Some(parse_character(child, namespace, part, report)?);
            continue;
        }
        content_seen = true;
        if child.has_tag_name((namespace, "t")) {
            for attribute in child.attributes() {
                if attribute.namespace() == Some(XML_NAMESPACE) && attribute.name() == "space" {
                    if !matches!(attribute.value(), "default" | "preserve") {
                        return Err(invalid(part, "invalid xml:space value"));
                    }
                } else {
                    unsupported_attribute(child, attribute.name(), part, report);
                }
            }
            let content = simple_text(child, part, report)?;
            if content.contains(['\r', '\n', '\t']) {
                report.push(
                    Issue::UnsupportedValue,
                    part,
                    "r/t",
                    "Literal carriage returns, line feeds and tabs in text literals have application-dependent processing; use explicit break/tab elements.",
                );
            }
            if !space_is_preserved(child)
                && (content.chars().next().is_some_and(is_xml_whitespace)
                    || content.chars().next_back().is_some_and(is_xml_whitespace))
            {
                report.push(
                    Issue::UnsupportedValue,
                    part,
                    "r/t",
                    "Boundary whitespace requires effective xml:space=preserve; the original remains unchanged rather than trimming text.",
                );
            }
            text.push_str(&content);
        } else if child.has_tag_name((namespace, "tab")) {
            check_word_attributes(child, namespace, &[], part, report);
            check_leaf(child, part, report)?;
            text.push('\t');
        } else if child.has_tag_name((namespace, "br")) {
            check_word_attributes(child, namespace, &["type", "clear"], part, report);
            check_leaf(child, part, report)?;
            if !matches!(
                child.attribute((namespace, "type")),
                None | Some("textWrapping")
            ) || !matches!(child.attribute((namespace, "clear")), None | Some("none"))
            {
                unsupported_element(
                    child,
                    part,
                    report,
                    "Only ordinary soft line breaks are supported.",
                );
            } else {
                text.push('\n');
            }
        } else if child.has_tag_name((namespace, "cr")) {
            check_word_attributes(child, namespace, &[], part, report);
            check_leaf(child, part, report)?;
            text.push('\n');
        } else if child.has_tag_name((namespace, "noBreakHyphen"))
            || child.has_tag_name((namespace, "softHyphen"))
        {
            check_word_attributes(child, namespace, &[], part, report);
            check_leaf(child, part, report)?;
            text.push(if child.tag_name().name() == "softHyphen" {
                '\u{00ad}'
            } else {
                '\u{2011}'
            });
        } else {
            unsupported_element(
                child,
                part,
                report,
                "This run content has no supported editable representation.",
            );
        }
    }
    Ok(TextRun {
        text,
        format: format.unwrap_or_default(),
    })
}

fn generated_id(document_id: &Id, kind: &str, index: usize) -> Result<Id, ConversionError> {
    Ok(Id::new(format!(
        "{}:docx:{kind}:{index}",
        document_id.as_str()
    ))?)
}

fn require_root<'a, 'input>(
    xml: &'a XmlDocument<'input>,
    namespace: &str,
    name: &str,
    part: &str,
) -> Result<Node<'a, 'input>, ConversionError> {
    let root = xml.root_element();
    if !root.has_tag_name((namespace, name)) {
        return Err(invalid(part, "unexpected semantic part root or namespace"));
    }
    for node in root.descendants().filter(Node::is_element) {
        if node
            .attribute((XML_NAMESPACE, "space"))
            .is_some_and(|value| !matches!(value, "default" | "preserve"))
        {
            return Err(invalid(part, "invalid xml:space value"));
        }
    }
    Ok(root)
}

fn check_word_attributes(
    node: Node<'_, '_>,
    namespace: &str,
    allowed: &[&str],
    part: &str,
    report: &mut CompatibilityReport,
) {
    for attribute in node.attributes() {
        if attribute.namespace() == Some(XML_NAMESPACE) && attribute.name() == "space" {
            continue;
        }
        if attribute.namespace() != Some(namespace) || !allowed.contains(&attribute.name()) {
            unsupported_attribute(node, attribute.name(), part, report);
        }
    }
}

fn space_is_preserved(node: Node<'_, '_>) -> bool {
    node.ancestors()
        .find_map(|ancestor| ancestor.attribute((XML_NAMESPACE, "space")))
        == Some("preserve")
}

fn is_xml_whitespace(character: char) -> bool {
    matches!(character, ' ' | '\t' | '\r' | '\n')
}

fn required_word_attribute<'a>(
    node: Node<'a, '_>,
    namespace: &str,
    name: &str,
    part: &str,
) -> Result<&'a str, ConversionError> {
    node.attribute((namespace, name))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| invalid(part, "required semantic attribute is missing or empty"))
}

fn on_off(value: &str, part: &str) -> Result<bool, ConversionError> {
    match value {
        "true" | "on" | "1" => Ok(true),
        "false" | "off" | "0" => Ok(false),
        _ => Err(invalid(part, "invalid on/off property value")),
    }
}

fn simple_text(
    node: Node<'_, '_>,
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<String, ConversionError> {
    let mut text = String::new();
    for child in node.children() {
        if child.is_text() {
            text.push_str(child.text().unwrap_or_default());
        } else if child.is_element() {
            unsupported_element(
                child,
                part,
                report,
                "Nested markup is not a supported text literal.",
            );
        }
    }
    Ok(text)
}

fn check_leaf(
    node: Node<'_, '_>,
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<(), ConversionError> {
    for child in node.children() {
        if child.is_element() {
            unsupported_element(
                child,
                part,
                report,
                "This property must not contain nested semantic content.",
            );
        } else {
            check_whitespace(child, part)?;
        }
    }
    Ok(())
}

fn check_whitespace(node: Node<'_, '_>, part: &str) -> Result<(), ConversionError> {
    if node.is_text()
        && node.text().is_some_and(|text| {
            !text
                .bytes()
                .all(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
        })
    {
        return Err(invalid(
            part,
            "unexpected text outside a text-bearing element",
        ));
    }
    Ok(())
}

fn unsupported_element(
    node: Node<'_, '_>,
    part: &str,
    report: &mut CompatibilityReport,
    message: &str,
) {
    report.push(
        Issue::UnsupportedElement,
        part,
        node.tag_name().name(),
        message,
    );
}

fn unsupported_attribute(
    node: Node<'_, '_>,
    name: &str,
    part: &str,
    report: &mut CompatibilityReport,
) {
    report.push(
        Issue::UnsupportedAttribute,
        part,
        format!(
            "{}/@{}",
            bounded_label(node.tag_name().name(), 128),
            bounded_label(name, 128)
        ),
        "This attribute has no supported semantic mapping.",
    );
}
