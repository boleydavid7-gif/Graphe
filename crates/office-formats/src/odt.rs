//! ODF text packages with an explicit, loss-refusing editable profile.

use crate::{
    DocumentFormat, FormatDiagnostic, FormatError, ImportNotices, ImportOutcome, ImportReport,
};
use office_core::{
    Block, CharacterFormat, Document, DocumentMetadata, Id, LineSpacing, NamedStyle, Paragraph,
    ParagraphFormat, Section, SectionProperties, TextRun,
};
use roxmltree::{Document as XmlDocument, Node};
use std::collections::{BTreeMap, BTreeSet};

mod compatibility;
mod export;
mod images;
mod package;
mod properties;
mod tables;
use tables::TABLE;

pub(super) const MEDIA_TYPE: &str = "application/vnd.oasis.opendocument.text";
const OFFICE: &str = "urn:oasis:names:tc:opendocument:xmlns:office:1.0";
const STYLE: &str = "urn:oasis:names:tc:opendocument:xmlns:style:1.0";
const TEXT: &str = "urn:oasis:names:tc:opendocument:xmlns:text:1.0";
const FO: &str = "urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0";
const SVG: &str = "urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0";
const MANIFEST: &str = "urn:oasis:names:tc:opendocument:xmlns:manifest:1.0";
const DC: &str = "http://purl.org/dc/elements/1.1/";

pub(crate) fn export_document(document: &Document) -> Result<Vec<u8>, FormatError> {
    export::export_document(document)
}

fn diagnostic(
    report: &mut Vec<FormatDiagnostic>,
    code: &str,
    part: &str,
    path: impl Into<String>,
    message: &str,
) {
    if report.len() < 128 {
        report.push(FormatDiagnostic::new(code, part, path, message));
    }
}

fn attributes(
    node: Node<'_, '_>,
    part: &str,
    allowed: &[(&str, &str)],
    report: &mut Vec<FormatDiagnostic>,
) {
    for attr in node.attributes() {
        if !allowed.contains(&(attr.namespace().unwrap_or(""), attr.name())) {
            diagnostic(
                report,
                "unsupported_attribute",
                part,
                format!("{}/@{}", node.tag_name().name(), attr.name()),
                "This ODF attribute cannot be preserved in the editable model.",
            );
        }
    }
}

fn container_text(node: Node<'_, '_>) -> Result<(), FormatError> {
    if node
        .children()
        .any(|child| child.is_text() && child.text().is_some_and(|text| !text.trim().is_empty()))
    {
        return Err(package::invalid(
            "character data outside a permitted text element",
        ));
    }
    Ok(())
}

fn validate_containers(
    document: &XmlDocument<'_>,
    part: &str,
    report: &mut Vec<FormatDiagnostic>,
    notices: &mut ImportNotices,
) -> Result<(), FormatError> {
    attributes(
        document.root_element(),
        part,
        &[(OFFICE, "version")],
        report,
    );
    container_text(document.root_element())?;
    for container in document.root_element().children().filter(Node::is_element) {
        let allowed: &[(&str, &str)] = if container.has_tag_name((OFFICE, "styles")) {
            &[(STYLE, "style"), (STYLE, "default-style")]
        } else if container.has_tag_name((OFFICE, "automatic-styles")) {
            &[(STYLE, "style"), (STYLE, "page-layout")]
        } else if container.has_tag_name((OFFICE, "master-styles")) {
            &[(STYLE, "master-page")]
        } else if container.has_tag_name((OFFICE, "font-face-decls")) {
            &[(STYLE, "font-face")]
        } else if container.has_tag_name((OFFICE, "body")) {
            &[(OFFICE, "text")]
        } else if container.has_tag_name((OFFICE, "scripts")) {
            &[]
        } else if container.has_tag_name((OFFICE, "meta")) {
            continue;
        } else {
            diagnostic(
                report,
                "unsupported_element",
                part,
                container.tag_name().name(),
                "Unsupported ODF document component is retained read-only.",
            );
            continue;
        };
        attributes(container, part, &[], report);
        container_text(container)?;
        for child in container.children().filter(Node::is_element) {
            if container.has_tag_name((OFFICE, "styles")) && compatibility::scaffolding(child) {
                notices.record(
                    part,
                    child.tag_name().name(),
                    "Inactive ODF authoring defaults are retained in the original file.",
                    false,
                );
                continue;
            }
            if !allowed.iter().any(|tag| child.has_tag_name(*tag)) {
                diagnostic(
                    report,
                    "unsupported_element",
                    part,
                    child.tag_name().name(),
                    "Unsupported ODF container content is retained read-only.",
                );
            }
            if child.has_tag_name((STYLE, "default-style"))
                && child.attribute((STYLE, "family")) != Some("paragraph")
            {
                diagnostic(
                    report,
                    "unsupported_style",
                    part,
                    "default-style",
                    "Only paragraph-family defaults are represented by this profile.",
                );
            }
            if child.has_tag_name((STYLE, "font-face")) {
                attributes(
                    child,
                    part,
                    &[
                        (STYLE, "name"),
                        (SVG, "font-family"),
                        (STYLE, "font-family-generic"),
                        (STYLE, "font-pitch"),
                    ],
                    report,
                );
                if child.has_children() {
                    diagnostic(
                        report,
                        "unsupported_font",
                        part,
                        "font-face",
                        "Embedded or extended font faces are retained read-only.",
                    );
                }
            }
        }
    }
    Ok(())
}

#[derive(Clone)]
struct StyleDefinition {
    name: String,
    display: String,
    family: String,
    parent: Option<String>,
    master: Option<String>,
    automatic: bool,
    paragraph: ParagraphFormat,
    character: CharacterFormat,
}

#[derive(Default)]
struct Catalog {
    fonts: BTreeMap<String, String>,
    styles: BTreeMap<(String, String), StyleDefinition>,
    default_paragraph: ParagraphFormat,
    default_character: CharacterFormat,
    pages: BTreeMap<String, SectionProperties>,
    masters: BTreeMap<String, String>,
    first_master: Option<String>,
    section_styles: BTreeSet<String>,
    drawing_styles: BTreeSet<String>,
    table_columns: BTreeMap<String, office_core::Twips>,
    table_cells: BTreeMap<String, bool>,
    table_masters: BTreeMap<String, Option<String>>,
    table_breaks: BTreeSet<String>,
}

impl Catalog {
    fn read_fonts(&mut self, document: &XmlDocument<'_>) -> Result<(), FormatError> {
        let mut seen = BTreeSet::new();
        for node in document
            .descendants()
            .filter(|n| n.has_tag_name((STYLE, "font-face")))
        {
            let (name, family) = properties::font_face(node)?;
            if !seen.insert(name.clone()) {
                return Err(package::invalid("duplicate font-face declaration"));
            }
            if self
                .fonts
                .get(&name)
                .is_some_and(|previous| *previous != family)
            {
                return Err(package::invalid("conflicting font declarations"));
            }
            self.fonts.insert(name, family);
        }
        Ok(())
    }

    fn read(
        &mut self,
        document: &XmlDocument<'_>,
        part: &str,
        report: &mut Vec<FormatDiagnostic>,
        notices: &mut ImportNotices,
    ) -> Result<(), FormatError> {
        for node in document.descendants().filter(Node::is_element) {
            if node.has_tag_name((STYLE, "default-style"))
                && node.attribute((STYLE, "family")) == Some("paragraph")
            {
                attributes(node, part, &[(STYLE, "family")], report);
                let (paragraph, character) = self.read_properties(node, part, report, notices)?;
                properties::overlay_paragraph(&mut self.default_paragraph, &paragraph);
                office_layout::overlay_character(&mut self.default_character, &character);
            } else if node.has_tag_name((STYLE, "style")) {
                let family = node
                    .attribute((STYLE, "family"))
                    .ok_or_else(|| package::invalid("style has no family"))?;
                if family == "section" && compatibility::section_style(node) {
                    let name = node
                        .attribute((STYLE, "name"))
                        .ok_or_else(|| package::invalid("section style has no name"))?;
                    if !self.section_styles.insert(name.into()) {
                        return Err(package::invalid("duplicate section style"));
                    }
                    notices.record(
                        part,
                        name,
                        "Unprotected single-column section defaults are retained in the original.",
                        false,
                    );
                    continue;
                }
                if family == "drawing-page" && compatibility::drawing_style(node) {
                    let name = node
                        .attribute((STYLE, "name"))
                        .ok_or_else(|| package::invalid("drawing style has no name"))?;
                    if !self.drawing_styles.insert(name.into()) {
                        return Err(package::invalid("duplicate drawing page style"));
                    }
                    continue;
                }
                if matches!(family, "table" | "table-column" | "table-cell") {
                    self.read_table_style(node, part, report)?;
                    continue;
                }
                if !matches!(family, "paragraph" | "text") {
                    diagnostic(
                        report,
                        "unsupported_style",
                        part,
                        family,
                        "Only paragraph and character styles fit the editable ODF profile.",
                    );
                    continue;
                }
                attributes(
                    node,
                    part,
                    &[
                        (STYLE, "name"),
                        (STYLE, "family"),
                        (STYLE, "display-name"),
                        (STYLE, "parent-style-name"),
                        (STYLE, "master-page-name"),
                        (STYLE, "class"),
                    ],
                    report,
                );
                let name = node
                    .attribute((STYLE, "name"))
                    .ok_or_else(|| package::invalid("style has no name"))?;
                let (paragraph, character) = self.read_properties(node, part, report, notices)?;
                let definition = StyleDefinition {
                    name: name.into(),
                    display: node
                        .attribute((STYLE, "display-name"))
                        .unwrap_or(name)
                        .into(),
                    family: family.into(),
                    parent: node
                        .attribute((STYLE, "parent-style-name"))
                        .map(str::to_owned),
                    master: node
                        .attribute((STYLE, "master-page-name"))
                        .map(str::to_owned),
                    automatic: node
                        .parent()
                        .is_some_and(|n| n.has_tag_name((OFFICE, "automatic-styles"))),
                    paragraph,
                    character,
                };
                if self
                    .styles
                    .insert((family.into(), name.into()), definition)
                    .is_some()
                {
                    return Err(package::invalid("duplicate style declaration"));
                }
                if self.styles.len() > 2048 {
                    return Err(FormatError::new("format_limit", "ODT has too many styles."));
                }
            } else if node.has_tag_name((STYLE, "page-layout")) {
                attributes(node, part, &[(STYLE, "name")], report);
                container_text(node)?;
                let name = node
                    .attribute((STYLE, "name"))
                    .ok_or_else(|| package::invalid("page layout has no name"))?;
                let mut properties = SectionProperties::default();
                for child in node.children().filter(Node::is_element) {
                    if child.has_tag_name((STYLE, "page-layout-properties")) {
                        properties = properties::geometry(child, part, report, notices)?;
                    } else if !((child.has_tag_name((STYLE, "header-style"))
                        || child.has_tag_name((STYLE, "footer-style")))
                        && child.children().all(|n| {
                            !n.is_element() && n.text().is_none_or(|t| t.trim().is_empty())
                        }))
                    {
                        diagnostic(
                            report,
                            "unsupported_page_layout",
                            part,
                            child.tag_name().name(),
                            "This page-layout component cannot be represented.",
                        );
                    }
                }
                if self.pages.insert(name.into(), properties).is_some() {
                    return Err(package::invalid("duplicate page layout"));
                }
            } else if node.has_tag_name((STYLE, "master-page")) {
                container_text(node)?;
                attributes(
                    node,
                    part,
                    &[
                        (STYLE, "name"),
                        (STYLE, "display-name"),
                        (STYLE, "page-layout-name"),
                        (compatibility::DRAW, "style-name"),
                    ],
                    report,
                );
                if let Some(name) = node.attribute((compatibility::DRAW, "style-name"))
                    && !self.drawing_styles.contains(name)
                {
                    return Err(package::invalid(
                        "unresolved or unsupported master drawing style",
                    ));
                }
                let name = node
                    .attribute((STYLE, "name"))
                    .ok_or_else(|| package::invalid("master page has no name"))?;
                let page = node
                    .attribute((STYLE, "page-layout-name"))
                    .ok_or_else(|| package::invalid("master page has no page layout"))?;
                if self.first_master.is_none() {
                    self.first_master = Some(name.into());
                }
                if self.masters.insert(name.into(), page.into()).is_some() {
                    return Err(package::invalid("duplicate master page"));
                }
                for child in node.children().filter(Node::is_element) {
                    diagnostic(
                        report,
                        "unsupported_header_footer",
                        part,
                        child.tag_name().name(),
                        "Headers, footers and master-page content are retained read-only.",
                    );
                }
            }
        }
        Ok(())
    }

    fn read_properties(
        &self,
        node: Node<'_, '_>,
        part: &str,
        report: &mut Vec<FormatDiagnostic>,
        notices: &mut ImportNotices,
    ) -> Result<(ParagraphFormat, CharacterFormat), FormatError> {
        let mut paragraph = ParagraphFormat::default();
        let mut character = CharacterFormat::default();
        let mut seen = BTreeSet::new();
        container_text(node)?;
        for child in node.children().filter(Node::is_element) {
            if !seen.insert((child.tag_name().namespace(), child.tag_name().name())) {
                return Err(package::invalid("duplicate style property element"));
            }
            if child.has_tag_name((STYLE, "paragraph-properties")) {
                paragraph = properties::paragraph(child, part, report, notices)?;
            } else if child.has_tag_name((STYLE, "text-properties")) {
                character = properties::character(child, part, &self.fonts, report, notices)?;
            } else {
                diagnostic(
                    report,
                    "unsupported_style",
                    part,
                    child.tag_name().name(),
                    "This style component has no shared-model representation.",
                );
            }
        }
        Ok((paragraph, character))
    }

    fn chain(&self, family: &str, name: &str) -> Result<Vec<&StyleDefinition>, FormatError> {
        let mut chain = Vec::new();
        let mut current = Some(name);
        while let Some(name) = current {
            if chain.len() >= self.styles.len() {
                return Err(package::invalid("cyclic style inheritance"));
            }
            let style = self
                .styles
                .get(&(family.into(), name.into()))
                .ok_or_else(|| package::invalid("unresolved style reference"))?;
            chain.push(style);
            current = style.parent.as_deref();
        }
        chain.reverse();
        Ok(chain)
    }

    fn geometry(&self, master: Option<&str>) -> Result<Option<SectionProperties>, FormatError> {
        let Some(master) = master.or(self.first_master.as_deref()) else {
            return Ok(None);
        };
        let page = self
            .masters
            .get(master)
            .ok_or_else(|| package::invalid("unresolved master-page reference"))?;
        Ok(Some(
            self.pages
                .get(page)
                .ok_or_else(|| package::invalid("unresolved page-layout reference"))?
                .clone(),
        ))
    }
}

pub(crate) fn import_document_with_warnings(
    bytes: &[u8],
    document_id: Id,
    filename: &str,
) -> Result<ImportReport, FormatError> {
    let mut notices = ImportNotices::default();
    let outcome = import_document(bytes, document_id, filename, &mut notices)?;
    Ok(notices.finish(outcome))
}

fn import_document(
    bytes: &[u8],
    document_id: Id,
    filename: &str,
    notices: &mut ImportNotices,
) -> Result<ImportOutcome, FormatError> {
    let parts = package::read(bytes)?;
    let manifest = package::xml(
        "META-INF/manifest.xml",
        parts
            .get("META-INF/manifest.xml")
            .ok_or_else(|| package::invalid("missing manifest"))?,
    )?;
    let mut report = Vec::new();
    validate_manifest(&manifest, &parts, &mut report)?;
    if manifest
        .descendants()
        .any(|n| n.has_tag_name((MANIFEST, "encryption-data")))
    {
        return preserved(bytes, filename, report);
    }
    let content = package::xml(
        "content.xml",
        parts
            .get("content.xml")
            .ok_or_else(|| package::invalid("missing content.xml"))?,
    )?;
    validate_root(&content, "document-content")?;
    validate_containers(&content, "content.xml", &mut report, notices)?;
    let styles = parts
        .get("styles.xml")
        .map(|b| package::xml("styles.xml", b))
        .transpose()?;
    if let Some(styles) = &styles {
        validate_root(styles, "document-styles")?;
        validate_containers(styles, "styles.xml", &mut report, notices)?;
    }
    let meta = parts
        .get("meta.xml")
        .map(|b| package::xml("meta.xml", b))
        .transpose()?;
    if let Some(meta) = &meta {
        validate_root(meta, "document-meta")?;
        validate_containers(meta, "meta.xml", &mut report, notices)?;
    }
    let mut catalog = Catalog::default();
    catalog.default_paragraph.line_spacing = Some(LineSpacing::Multiple { value: 240 });
    catalog.default_paragraph.spacing_before = Some(office_core::Twips(0));
    catalog.default_paragraph.spacing_after = Some(office_core::Twips(0));
    if let Some(styles) = &styles {
        catalog.read_fonts(styles)?;
    }
    catalog.read_fonts(&content)?;
    if let Some(styles) = &styles {
        catalog.read(styles, "styles.xml", &mut report, notices)?;
    }
    catalog.read(&content, "content.xml", &mut report, notices)?;
    for style in catalog.styles.values() {
        catalog.chain(&style.family, &style.name)?;
    }
    for master in catalog.masters.keys() {
        catalog.geometry(Some(master))?;
    }
    let mut title = String::new();
    if let Some(meta) = &meta {
        for container in meta.root_element().children().filter(Node::is_element) {
            if !container.has_tag_name((OFFICE, "meta")) {
                diagnostic(
                    &mut report,
                    "unsupported_metadata",
                    "meta.xml",
                    container.tag_name().name(),
                    "Unknown metadata container is retained read-only.",
                );
                continue;
            }
            let mut title_seen = false;
            container_text(container)?;
            for node in container.children().filter(Node::is_element) {
                if node.has_tag_name((DC, "title")) {
                    attributes(node, "meta.xml", &[], &mut report);
                    if node.children().any(|n| n.is_element()) {
                        diagnostic(
                            &mut report,
                            "unsupported_metadata",
                            "meta.xml",
                            "title",
                            "Structured title metadata is retained read-only.",
                        );
                    }
                    if title_seen {
                        return Err(package::invalid("duplicate document title"));
                    }
                    title_seen = true;
                    title = node
                        .children()
                        .filter(Node::is_text)
                        .filter_map(|child| child.text())
                        .collect();
                } else if compatibility::generated_metadata(node) {
                    notices.record(
                        "meta.xml",
                        node.tag_name().name(),
                        "Generated application metadata is retained in the original file.",
                        false,
                    );
                } else {
                    diagnostic(
                        &mut report,
                        "unsupported_metadata",
                        "meta.xml",
                        node.tag_name().name(),
                        "This metadata is not represented by the current model.",
                    );
                }
            }
        }
    }
    let image_media = manifest
        .descendants()
        .filter(|node| node.has_tag_name((MANIFEST, "file-entry")))
        .filter_map(|node| {
            Some((
                node.attribute((MANIFEST, "full-path"))?.to_owned(),
                node.attribute((MANIFEST, "media-type"))?.to_owned(),
            ))
        })
        .collect::<BTreeMap<_, _>>();
    images::preflight(&content, &parts, &image_media)?;
    let mut named = Vec::new();
    let named_ids: BTreeMap<_, _> = catalog
        .styles
        .values()
        .filter(|s| s.family == "paragraph" && !s.automatic)
        .enumerate()
        .map(|(index, s)| {
            Ok((
                s.name.clone(),
                Id::new(format!("{}:odt:style:{index}", document_id.as_str()))?,
            ))
        })
        .collect::<Result<_, office_core::ValidationError>>()?;
    for definition in catalog
        .styles
        .values()
        .filter(|s| s.family == "paragraph" && !s.automatic)
    {
        let mut style = NamedStyle::new(
            named_ids[&definition.name].clone(),
            definition.display.clone(),
        );
        style.based_on = definition
            .parent
            .as_ref()
            .map(|parent| {
                named_ids.get(parent).cloned().ok_or_else(|| {
                    package::invalid("named paragraph style inherits an automatic style")
                })
            })
            .transpose()?;
        if style.based_on.is_none() {
            style.paragraph_format = catalog.default_paragraph.clone();
            style.character_format = catalog.default_character.clone();
        }
        properties::overlay_paragraph(&mut style.paragraph_format, &definition.paragraph);
        office_layout::overlay_character(&mut style.character_format, &definition.character);
        named.push(style);
    }
    let bodies: Vec<_> = content
        .root_element()
        .children()
        .filter(|n| n.has_tag_name((OFFICE, "body")))
        .collect();
    if bodies.len() != 1 {
        return Err(package::invalid("expected one document body"));
    }
    let texts: Vec<_> = bodies[0]
        .children()
        .filter(|n| n.has_tag_name((OFFICE, "text")))
        .collect();
    if texts.len() != 1 {
        return Err(package::invalid("expected one text body"));
    }
    let mut parser = TextParser {
        catalog: &catalog,
        report: &mut report,
        named_ids: &named_ids,
        document_id: &document_id,
        paragraph_count: 0,
        table_count: 0,
        image_count: 0,
        image_bytes: 0,
        image_pixels: 0,
        image_cache: BTreeMap::new(),
        image_parts: BTreeSet::new(),
        parts: &parts,
        image_media: &image_media,
        notices,
        section_count: 0,
        text_bytes: 0,
        run_count: 0,
    };
    let sections = parser.body(texts[0])?;
    let consumed_images = std::mem::take(&mut parser.image_parts);
    drop(parser);
    for name in parts.keys() {
        if !matches!(
            name.as_str(),
            "mimetype" | "META-INF/manifest.xml" | "content.xml" | "styles.xml" | "meta.xml"
        ) && !consumed_images.contains(name)
            && !compatibility::package_part(name, &parts, &manifest, notices)?
        {
            diagnostic(
                &mut report,
                "unsupported_part",
                name,
                "package",
                "Additional ODF package content is retained read-only.",
            );
        }
    }
    for child in content.root_element().children().filter(Node::is_element) {
        if child.has_tag_name((OFFICE, "scripts"))
            && child
                .children()
                .all(|n| !n.is_element() && n.text().is_none_or(|v| v.trim().is_empty()))
        {
            continue;
        }
        if ![
            (OFFICE, "font-face-decls"),
            (OFFICE, "automatic-styles"),
            (OFFICE, "body"),
        ]
        .iter()
        .any(|tag| child.has_tag_name(*tag))
        {
            diagnostic(
                &mut report,
                "unsupported_element",
                "content.xml",
                child.tag_name().name(),
                "This ODF document component is retained read-only.",
            );
        }
    }
    if !report.is_empty() {
        return preserved(bytes, filename, report);
    }
    let document = Document::from_parts(document_id, DocumentMetadata { title }, sections, named)?;
    Ok(ImportOutcome::Editable(crate::retain_source(
        DocumentFormat::Odt,
        document,
        bytes,
        filename,
    )?))
}

fn preserved(
    bytes: &[u8],
    filename: &str,
    report: Vec<FormatDiagnostic>,
) -> Result<ImportOutcome, FormatError> {
    Ok(ImportOutcome::Preserved {
        original: crate::original_source(DocumentFormat::Odt, bytes, filename)?
            .ok_or_else(|| package::invalid("empty original"))?,
        report,
    })
}

fn validate_root(document: &XmlDocument<'_>, name: &str) -> Result<(), FormatError> {
    let root = document.root_element();
    if !root.has_tag_name((OFFICE, name))
        || !matches!(
            root.attribute((OFFICE, "version")),
            Some("1.2" | "1.3" | "1.4")
        )
    {
        return Err(package::invalid("unsupported document root or ODF version"));
    }
    Ok(())
}

fn validate_manifest(
    document: &XmlDocument<'_>,
    parts: &BTreeMap<String, Vec<u8>>,
    report: &mut Vec<FormatDiagnostic>,
) -> Result<(), FormatError> {
    let root = document.root_element();
    if !root.has_tag_name((MANIFEST, "manifest"))
        || !matches!(
            root.attribute((MANIFEST, "version")),
            Some("1.2" | "1.3" | "1.4")
        )
    {
        return Err(package::invalid("unsupported manifest root or ODF version"));
    }
    let mut names = BTreeSet::new();
    for node in root.children().filter(Node::is_element) {
        if !node.has_tag_name((MANIFEST, "file-entry")) {
            diagnostic(
                report,
                "unsupported_package",
                "META-INF/manifest.xml",
                node.tag_name().name(),
                "Unsupported manifest component.",
            );
            continue;
        }
        let path = node
            .attribute((MANIFEST, "full-path"))
            .ok_or_else(|| package::invalid("manifest path missing"))?;
        let media = node
            .attribute((MANIFEST, "media-type"))
            .ok_or_else(|| package::invalid("manifest media type missing"))?;
        if !names.insert(path) || path == "mimetype" || path == "META-INF/manifest.xml" {
            return Err(package::invalid("duplicate or forbidden manifest entry"));
        }
        if path == "/" {
            if media != MEDIA_TYPE {
                return Err(package::invalid(
                    "manifest media type differs from mimetype",
                ));
            }
        } else if !parts.contains_key(path) {
            return Err(package::invalid("manifest refers to missing package entry"));
        }
        for child in node.children().filter(Node::is_element) {
            diagnostic(
                report,
                "unsupported_package",
                "META-INF/manifest.xml",
                child.tag_name().name(),
                "Encrypted or extended ODF manifests are retained read-only.",
            );
        }
    }
    if !names.contains("/")
        || parts.keys().any(|name| {
            name != "mimetype" && !name.starts_with("META-INF/") && !names.contains(name.as_str())
        })
    {
        return Err(package::invalid(
            "manifest does not inventory the complete package",
        ));
    }
    Ok(())
}

struct TextParser<'a> {
    catalog: &'a Catalog,
    report: &'a mut Vec<FormatDiagnostic>,
    named_ids: &'a BTreeMap<String, Id>,
    document_id: &'a Id,
    paragraph_count: usize,
    table_count: usize,
    image_count: usize,
    image_bytes: usize,
    image_pixels: u64,
    image_cache: BTreeMap<String, office_core::ImageAsset>,
    image_parts: BTreeSet<String>,
    parts: &'a BTreeMap<String, Vec<u8>>,
    image_media: &'a BTreeMap<String, String>,
    notices: &'a mut ImportNotices,
    section_count: usize,
    text_bytes: usize,
    run_count: usize,
}

#[derive(Default)]
struct Whitespace {
    seen_content: bool,
    pending: Option<CharacterFormat>,
}

impl TextParser<'_> {
    fn section(
        &mut self,
        blocks: Vec<Block>,
        properties: Option<SectionProperties>,
    ) -> Result<Section, FormatError> {
        self.section_count += 1;
        let section = Section::new(
            Id::new(format!(
                "{}:odt:section:{}",
                self.document_id.as_str(),
                self.section_count
            ))?,
            blocks,
        );
        Ok(match properties {
            Some(p) => section.with_properties(p),
            None => section,
        })
    }

    fn body(&mut self, node: Node<'_, '_>) -> Result<Vec<Section>, FormatError> {
        attributes(node, "content.xml", &[], self.report);
        container_text(node)?;
        let mut result = Vec::new();
        let mut current = Vec::new();
        let mut geometry = self.catalog.geometry(None)?;
        for child in node.children().filter(Node::is_element) {
            if child.has_tag_name((TEXT, "section")) {
                if !current.is_empty() {
                    result.push(self.section(std::mem::take(&mut current), geometry.take())?);
                }
                attributes(
                    child,
                    "content.xml",
                    &[(TEXT, "name"), (TEXT, "style-name")],
                    self.report,
                );
                if let Some(style) = child.attribute((TEXT, "style-name"))
                    && !self.catalog.section_styles.contains(style)
                {
                    return Err(package::invalid("unresolved or unsupported section style"));
                }
                container_text(child)?;
                let mut blocks = Vec::new();
                let mut section_geometry = self.catalog.geometry(None)?;
                for paragraph in child.children().filter(Node::is_element) {
                    if paragraph.has_tag_name((TEXT, "p")) || paragraph.has_tag_name((TEXT, "h")) {
                        let (p, g) = self.body_paragraph(paragraph)?;
                        if blocks.is_empty() && g.is_some() {
                            section_geometry = g;
                        } else if g.is_some() {
                            diagnostic(
                                self.report,
                                "unsupported_section",
                                "content.xml",
                                "master-page-name",
                                "A master-page change within a text section requires an unsupported section split.",
                            );
                        }
                        if let Some(p) = p {
                            blocks.push(p);
                        }
                    } else if paragraph.has_tag_name((TABLE, "table")) {
                        let (table, g) = self.table(paragraph)?;
                        if blocks.is_empty() && g.is_some() {
                            section_geometry = g;
                        } else if g.is_some() {
                            diagnostic(
                                self.report,
                                "unsupported_section",
                                "content.xml",
                                "table",
                                "Page changes within a section are retained read-only.",
                            );
                        }
                        if let Some(table) = table {
                            blocks.push(Block::Table(table));
                        }
                    } else {
                        diagnostic(
                            self.report,
                            "unsupported_element",
                            "content.xml",
                            paragraph.tag_name().name(),
                            "Unsupported section content is retained read-only.",
                        );
                    }
                }
                result.push(self.section(blocks, section_geometry)?);
            } else if child.has_tag_name((TEXT, "p")) || child.has_tag_name((TEXT, "h")) {
                let (p, g) = self.body_paragraph(child)?;
                if let Some(next) = g {
                    if !current.is_empty() {
                        result.push(self.section(std::mem::take(&mut current), geometry.take())?);
                    }
                    geometry = Some(next);
                }
                if let Some(p) = p {
                    current.push(p);
                }
            } else if child.has_tag_name((TABLE, "table")) {
                let (table, g) = self.table(child)?;
                if let Some(next) = g {
                    if !current.is_empty() {
                        result.push(self.section(std::mem::take(&mut current), geometry.take())?);
                    }
                    geometry = Some(next);
                }
                if let Some(table) = table {
                    current.push(Block::Table(table));
                }
            } else if compatibility::sequence_declarations(child) {
                // Actual sequence fields remain unsupported in runs().
            } else {
                diagnostic(
                    self.report,
                    "unsupported_element",
                    "content.xml",
                    child.tag_name().name(),
                    "Only paragraphs and plain text sections fit the editable ODF profile.",
                );
            }
        }
        if !current.is_empty() {
            result.push(self.section(current, geometry)?);
        }
        Ok(result)
    }

    fn paragraph(
        &mut self,
        node: Node<'_, '_>,
    ) -> Result<(Paragraph, Option<SectionProperties>), FormatError> {
        attributes(node, "content.xml", &[(TEXT, "style-name")], self.report);
        if node.has_tag_name((TEXT, "h")) {
            diagnostic(
                self.report,
                "unsupported_heading",
                "content.xml",
                "text:h",
                "Heading outline semantics are not represented by the current model.",
            );
        }
        self.paragraph_count += 1;
        if self.paragraph_count > 4096 {
            return Err(FormatError::new(
                "format_limit",
                "ODT has too many paragraphs.",
            ));
        }
        let mut direct = ParagraphFormat::default();
        let mut character = CharacterFormat::default();
        let mut named = None;
        let mut master = None;
        if let Some(style) = node.attribute((TEXT, "style-name")) {
            let chain = self.catalog.chain("paragraph", style)?;
            if !chain.iter().any(|style| !style.automatic) {
                direct = self.catalog.default_paragraph.clone();
                character = self.catalog.default_character.clone();
            }
            for style in chain {
                if style.automatic {
                    properties::overlay_paragraph(&mut direct, &style.paragraph);
                    office_layout::overlay_character(&mut character, &style.character);
                } else {
                    named = Some(
                        self.named_ids
                            .get(&style.name)
                            .ok_or_else(|| package::invalid("unresolved paragraph style"))?
                            .clone(),
                    );
                }
                if let Some(value) = &style.master {
                    master = Some(value.as_str());
                }
            }
        } else {
            direct = self.catalog.default_paragraph.clone();
            character = self.catalog.default_character.clone();
        }
        let mut runs = Vec::new();
        self.runs(node, &character, &mut runs, &mut Whitespace::default(), 0)?;
        let paragraph = Paragraph::new(
            Id::new(format!(
                "{}:odt:paragraph:{}",
                self.document_id.as_str(),
                self.paragraph_count
            ))?,
            runs,
        )
        .with_format(direct);
        let paragraph = match named {
            Some(style) => paragraph.with_style(style),
            None => paragraph,
        };
        Ok((
            paragraph,
            master
                .map(|m| self.catalog.geometry(Some(m)))
                .transpose()?
                .flatten(),
        ))
    }

    fn runs(
        &mut self,
        node: Node<'_, '_>,
        format: &CharacterFormat,
        runs: &mut Vec<TextRun>,
        whitespace: &mut Whitespace,
        depth: usize,
    ) -> Result<(), FormatError> {
        if depth > 64 {
            return Err(FormatError::new(
                "format_limit",
                "ODT text nesting is too deep.",
            ));
        }
        for child in node.children() {
            if child.is_text() {
                let text = child.text().unwrap_or("");
                // ODF collapses ordinary XML whitespace. Explicit text:s/tab/
                // line-break nodes below preserve significant source spacing.
                let mut chunk = String::new();
                for c in text.chars() {
                    if matches!(c, ' ' | '\t' | '\r' | '\n') {
                        if !chunk.is_empty() {
                            self.push(runs, std::mem::take(&mut chunk), format)?;
                            whitespace.seen_content = true;
                        }
                        if whitespace.seen_content && whitespace.pending.is_none() {
                            whitespace.pending = Some(format.clone());
                        }
                    } else {
                        self.flush_space(runs, whitespace)?;
                        chunk.push(c);
                    }
                }
                if !chunk.is_empty() {
                    self.push(runs, chunk, format)?;
                    whitespace.seen_content = true;
                }
            } else if child.is_element() {
                if child.has_tag_name((TEXT, "span")) {
                    attributes(child, "content.xml", &[(TEXT, "style-name")], self.report);
                    let mut next = format.clone();
                    if let Some(name) = child.attribute((TEXT, "style-name")) {
                        for style in self.catalog.chain("text", name)? {
                            office_layout::overlay_character(&mut next, &style.character);
                        }
                    }
                    if !child.has_children() {
                        self.push(runs, String::new(), &next)?;
                    } else {
                        self.runs(child, &next, runs, whitespace, depth + 1)?;
                    }
                } else if child.has_tag_name((TEXT, "s")) {
                    if child.has_children() {
                        return Err(package::invalid("text:s must be empty"));
                    }
                    attributes(child, "content.xml", &[(TEXT, "c")], self.report);
                    let count = child
                        .attribute((TEXT, "c"))
                        .unwrap_or("1")
                        .parse::<usize>()
                        .map_err(package::invalid)?;
                    if count == 0 || count > 1_000_000 {
                        return Err(FormatError::new(
                            "format_limit",
                            "ODT repeated spaces exceed the supported limit.",
                        ));
                    }
                    self.flush_space(runs, whitespace)?;
                    self.push(runs, " ".repeat(count), format)?;
                    whitespace.seen_content = true;
                } else if child.has_tag_name((TEXT, "tab"))
                    || child.has_tag_name((TEXT, "line-break"))
                {
                    if child.has_children() {
                        return Err(package::invalid("text tab/line-break must be empty"));
                    }
                    attributes(child, "content.xml", &[], self.report);
                    self.flush_space(runs, whitespace)?;
                    self.push(
                        runs,
                        if child.has_tag_name((TEXT, "tab")) {
                            "\t"
                        } else {
                            "\n"
                        }
                        .into(),
                        format,
                    )?;
                    whitespace.seen_content = true;
                } else {
                    diagnostic(
                        self.report,
                        "unsupported_element",
                        "content.xml",
                        child.tag_name().name(),
                        "Unsupported inline content is retained read-only.",
                    );
                }
            }
        }
        Ok(())
    }

    fn flush_space(
        &mut self,
        runs: &mut Vec<TextRun>,
        whitespace: &mut Whitespace,
    ) -> Result<(), FormatError> {
        if let Some(format) = whitespace.pending.take() {
            self.push(runs, " ".into(), &format)?;
        }
        Ok(())
    }

    fn push(
        &mut self,
        runs: &mut Vec<TextRun>,
        text: String,
        format: &CharacterFormat,
    ) -> Result<(), FormatError> {
        self.text_bytes += text.len();
        if !text.is_empty()
            && let Some(last) = runs.last_mut()
            && !last.text.is_empty()
            && last.format == *format
        {
            if self.text_bytes > 4 * 1024 * 1024 {
                return Err(FormatError::new(
                    "format_limit",
                    "ODT text exceeds the supported resource limit.",
                ));
            }
            last.text.push_str(&text);
            return Ok(());
        }
        self.run_count += 1;
        if self.run_count > 20_000 || self.text_bytes > 4 * 1024 * 1024 {
            return Err(FormatError::new(
                "format_limit",
                "ODT text exceeds the supported resource limit.",
            ));
        }
        runs.push(TextRun {
            text,
            format: format.clone(),
        });
        Ok(())
    }
}
