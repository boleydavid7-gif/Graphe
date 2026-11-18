//! Strict semantic DOCX import with a preserved read-only outcome for gaps.

use std::collections::{BTreeMap, BTreeSet};

use office_core::{
    Block, CharacterFormat, Document, DocumentMetadata, Id, ListKind, NamedStyle, Paragraph,
    ParagraphFormat, Section, SectionProperties, TextRun, valid_hyperlink,
};
use roxmltree::{Document as XmlDocument, Node};

use crate::{
    CompatibilityIssueKind as Issue, CompatibilityReport, ConversionError, DocxConformance,
    DocxError, DocxImportOutcome, DocxSource, ImportedDocx,
    ancillary::AncillaryKind,
    conversion::{bounded_label, invalid},
    numbering::parse_numbering,
    opc::{parse_relationships, relationship_source, resolve_target},
    package::{Package, read_package},
    properties::{parse_character, parse_paragraph, parse_run_character},
    xml::parse_xml,
};

#[path = "table_import.rs"]
mod tables;

const WORD_TRANSITIONAL: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const WORD_STRICT: &str = "http://purl.oclc.org/ooxml/wordprocessingml/main";
const REL_TRANSITIONAL: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/";
const REL_STRICT: &str = "http://purl.oclc.org/ooxml/officeDocument/relationships/";
const CORE_RELATIONSHIP: &str =
    "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties";
const THUMBNAIL_RELATIONSHIP: &str =
    "http://schemas.openxmlformats.org/package/2006/relationships/metadata/thumbnail";
const CORE_NAMESPACE: &str =
    "http://schemas.openxmlformats.org/package/2006/metadata/core-properties";
const DC_NAMESPACE: &str = "http://purl.org/dc/elements/1.1/";
const DCTERMS_NAMESPACE: &str = "http://purl.org/dc/terms/";
const XSI_NAMESPACE: &str = "http://www.w3.org/2001/XMLSchema-instance";
const MC_NAMESPACE: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";
const XML_NAMESPACE: &str = "http://www.w3.org/XML/1998/namespace";
const WORD_2010_NAMESPACE: &str = "http://schemas.microsoft.com/office/word/2010/wordml";
const NUMBERING_CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml";
const STYLES_CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml";
const CORE_CONTENT_TYPE: &str = "application/vnd.openxmlformats-package.core-properties+xml";
const MAX_IMPORT_ID_BYTES: usize = 256;

pub(crate) struct PartGraph {
    pub(crate) styles: Option<String>,
    pub(crate) numbering: Option<String>,
    pub(crate) hyperlinks: BTreeMap<String, String>,
    pub(crate) core_properties: Option<String>,
    pub(crate) ancillary: BTreeMap<AncillaryKind, String>,
}

#[derive(Default)]
struct StyleBook {
    definitions: Vec<NamedStyle>,
    identifiers: BTreeMap<String, Id>,
    default_style: Option<Id>,
    effective_characters: BTreeMap<String, CharacterFormat>,
    character_styles: BTreeMap<String, CharacterFormat>,
    default_character: Option<CharacterFormat>,
}

#[derive(Clone, Copy)]
struct ImportContext<'a> {
    namespace: &'a str,
    part: &'a str,
    document_id: &'a Id,
    styles: &'a StyleBook,
    numbering: &'a BTreeMap<u32, ListKind>,
    hyperlinks: &'a BTreeMap<String, String>,
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
            &package.parts[&original.inspection().main_document_part],
            &mut report,
        )?,
        None => StyleBook::default(),
    };
    let metadata = match graph.core_properties {
        Some(ref part) => parse_metadata(part, &package.parts[part], &mut report)?,
        None => DocumentMetadata::default(),
    };
    let numbering = match graph.numbering {
        Some(ref part) => parse_numbering(part, &package.parts[part], namespace, &mut report)?,
        None => BTreeMap::new(),
    };
    let main_part = &original.inspection().main_document_part;
    let sections = parse_main_document(
        &package.parts[main_part],
        ImportContext {
            namespace,
            part: main_part,
            document_id: &document_id,
            styles: &styles,
            numbering: &numbering,
            hyperlinks: &graph.hyperlinks,
        },
        &mut report,
    )?;
    if styles.default_style.is_some() && sections.iter().all(|section| section.blocks().is_empty())
    {
        report.normalize(
            main_part,
            "body",
            "An empty body has no paragraph to retain its source default for future typing. Writer will use its own defaults; the source styles remain in the original.",
        );
    }
    if !report.is_supported() {
        return Ok(DocxImportOutcome::Preserved { original, report });
    }
    let document = Document::from_parts(document_id, metadata, sections, styles.definitions)?;
    Ok(DocxImportOutcome::Editable(ImportedDocx {
        document,
        original,
        report,
    }))
}

pub(crate) fn inspect_semantic_graph(
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
        numbering: None,
        hyperlinks: BTreeMap::new(),
        core_properties: None,
        ancillary: BTreeMap::new(),
    };
    let mut understood = BTreeSet::from([main_part.clone()]);
    for part in &original.inspection().parts {
        if !part.name.to_ascii_lowercase().ends_with(".rels") {
            continue;
        }
        understood.insert(part.name.clone());
        let source = relationship_source(&part.name)?;
        for relation in parse_relationships(&part.name, &package.parts[&part.name])? {
            let main_source = source
                .as_deref()
                .is_some_and(|value| value.eq_ignore_ascii_case(main_part));
            if relation.external {
                if main_source && relation_kind(&relation.relationship_type, "hyperlink") {
                    if valid_hyperlink(&relation.target) {
                        graph
                            .hyperlinks
                            .insert(relation.id.clone(), relation.target.clone());
                    } else {
                        unsupported_relationship(
                            &part.name,
                            &relation,
                            report,
                            "This external hyperlink target is not a safe HTTP(S) URL.",
                        );
                    }
                } else {
                    unsupported_relationship(
                        &part.name,
                        &relation,
                        report,
                        "External targets are never fetched and have no supported editable mapping.",
                    );
                }
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
            if main_source && relation_kind(&relation.relationship_type, "numbering") {
                if graph.numbering.replace((*target).into()).is_some() {
                    return Err(invalid(
                        &part.name,
                        "multiple numbering relationships are ambiguous",
                    ));
                }
                understood.insert((*target).into());
            } else if main_source && relation_kind(&relation.relationship_type, "styles") {
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
            } else if let Some(kind) =
                ancillary_relationship(&relation.relationship_type, source.is_none(), main_source)
            {
                if graph.ancillary.insert(kind, (*target).into()).is_some() {
                    return Err(invalid(
                        &part.name,
                        "multiple singleton ancillary relationships are ambiguous",
                    ));
                }
                understood.insert((*target).into());
            } else {
                unsupported_relationship(
                    &part.name,
                    &relation,
                    report,
                    "This type or source role has no supported document interpretation.",
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
        if graph.numbering.as_deref() == Some(part.name.as_str())
            && !part
                .content_type
                .eq_ignore_ascii_case(NUMBERING_CONTENT_TYPE)
        {
            return Err(invalid(
                &part.name,
                "numbering relationship has an incorrect content type",
            ));
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
    let namespace = match original.inspection().conformance {
        DocxConformance::Transitional => WORD_TRANSITIONAL,
        DocxConformance::Strict => WORD_STRICT,
    };
    for (kind, target) in &graph.ancillary {
        let inventory = original
            .inspection()
            .parts
            .iter()
            .find(|part| &part.name == target)
            .ok_or_else(|| {
                invalid(
                    target,
                    "ancillary target is absent from the package inventory",
                )
            })?;
        if !kind.accepts_content_type(&inventory.content_type) {
            return Err(invalid(
                target,
                "ancillary relationship has an incorrect content type",
            ));
        }
        kind.inspect(target, &package.parts[target], namespace, report)?;
    }
    Ok(graph)
}

fn unsupported_relationship(
    part: &str,
    relationship: &crate::opc::Relationship,
    report: &mut CompatibilityReport,
    reason: &str,
) {
    report.push(
        Issue::UnsupportedRelationship,
        part,
        format!("Relationship/{}", bounded_label(&relationship.id, 128)),
        format!(
            "Type={} Target={}. {reason}",
            bounded_label(&relationship.relationship_type, 512),
            bounded_label(&relationship.target, 512)
        ),
    );
}

fn ancillary_relationship(value: &str, root: bool, main: bool) -> Option<AncillaryKind> {
    if root {
        if value == THUMBNAIL_RELATIONSHIP {
            return Some(AncillaryKind::Thumbnail);
        }
        if value == format!("{REL_TRANSITIONAL}extended-properties")
            || value == format!("{REL_STRICT}extendedProperties")
        {
            return Some(AncillaryKind::ExtendedProperties);
        }
        if value == format!("{REL_TRANSITIONAL}custom-properties")
            || value == format!("{REL_STRICT}customProperties")
        {
            return Some(AncillaryKind::CustomProperties);
        }
    }
    if main {
        for (name, kind) in [
            ("theme", AncillaryKind::Theme),
            ("fontTable", AncillaryKind::FontTable),
            ("settings", AncillaryKind::Settings),
            ("webSettings", AncillaryKind::WebSettings),
        ] {
            if relation_kind(value, name) {
                return Some(kind);
            }
        }
    }
    None
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
    main_bytes: &[u8],
    report: &mut CompatibilityReport,
) -> Result<StyleBook, ConversionError> {
    let xml = parse_xml(part, bytes)?;
    let root = require_root(&xml, namespace, "styles", part)?;
    check_word_attributes(root, namespace, &[], part, report);
    let mut defaults = None;
    let mut declarations = BTreeMap::new();
    let mut ordered = Vec::new();
    let mut defaults_by_kind = BTreeSet::new();
    let mut latent_seen = false;
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
                &["type", "styleId", "default", "customStyle"],
                part,
                report,
            );
            let word_id = required_word_attribute(node, namespace, "styleId", part)?.to_owned();
            if declarations.insert(word_id.clone(), node).is_some() {
                return Err(invalid(part, "duplicate style identifiers"));
            }
            if let Some(value) = node.attribute((namespace, "customStyle")) {
                on_off(value, part)?;
            }
            let kind = required_word_attribute(node, namespace, "type", part)?;
            if !matches!(kind, "paragraph" | "character" | "table" | "numbering") {
                unsupported_element(
                    node,
                    part,
                    report,
                    "Unknown style types cannot be interpreted as ordinary formatting.",
                );
                continue;
            }
            if node
                .attribute((namespace, "default"))
                .map(|value| on_off(value, part))
                .transpose()?
                .unwrap_or(false)
                && !defaults_by_kind.insert(kind)
            {
                return Err(invalid(
                    part,
                    "multiple default styles of the same type are ambiguous",
                ));
            }
            ordered.push((word_id, kind, node));
        } else if node.has_tag_name((namespace, "latentStyles")) {
            if latent_seen {
                return Err(invalid(part, "duplicate latent-style metadata"));
            }
            latent_seen = true;
            inspect_latent_styles(node, namespace, part, report)?;
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
        // An unknown extension may supply declarations or defaults. Preserve
        // that source read-only rather than inventing a missing-style error.
        return Ok(StyleBook::default());
    }
    validate_style_inheritance(&declarations, namespace, part)?;
    let main = parse_xml("main document style references", main_bytes)?;
    let mut required_characters = BTreeSet::new();
    let mut required_paragraphs = BTreeSet::new();
    let mut required_tables = BTreeSet::new();
    for node in main
        .descendants()
        .filter(|node| node.has_tag_name((namespace, "tblStyle")))
    {
        required_tables.insert(required_word_attribute(node, namespace, "val", part)?.to_owned());
    }
    for id in &required_tables {
        let declaration = declarations
            .get(id)
            .ok_or_else(|| invalid(part, "table references a missing table style"))?;
        if declaration.attribute((namespace, "type")) != Some("table") {
            return Err(invalid(
                part,
                "table style reference has the wrong style type",
            ));
        }
    }
    for node in main
        .descendants()
        .filter(|node| node.has_tag_name((namespace, "rStyle")))
    {
        required_characters
            .insert(required_word_attribute(node, namespace, "val", part)?.to_owned());
    }
    for node in main
        .descendants()
        .filter(|node| node.has_tag_name((namespace, "pStyle")))
    {
        required_paragraphs
            .insert(required_word_attribute(node, namespace, "val", part)?.to_owned());
    }
    for (word_id, kind, node) in &ordered {
        if *kind == "character"
            && node
                .attribute((namespace, "default"))
                .map(|value| on_off(value, part))
                .transpose()?
                .unwrap_or(false)
        {
            required_characters.insert(word_id.clone());
        }
        if *kind == "paragraph"
            && node
                .attribute((namespace, "default"))
                .map(|value| on_off(value, part))
                .transpose()?
                .unwrap_or(false)
        {
            required_paragraphs.insert(word_id.clone());
        }
    }
    let mut pending: Vec<_> = required_characters.iter().cloned().collect();
    while let Some(id) = pending.pop() {
        let node = declarations
            .get(&id)
            .ok_or_else(|| invalid(part, "run references a missing character style"))?;
        if node.attribute((namespace, "type")) != Some("character") {
            return Err(invalid(
                part,
                "character-style reference has the wrong style type",
            ));
        }
        if let Some(parent) = style_parent(*node, namespace, part)?
            && required_characters.insert(parent.clone())
        {
            pending.push(parent);
        }
    }
    let mut pending: Vec<_> = required_paragraphs.iter().cloned().collect();
    while let Some(id) = pending.pop() {
        let node = declarations
            .get(&id)
            .ok_or_else(|| invalid(part, "paragraph references a missing style"))?;
        if node.attribute((namespace, "type")) != Some("paragraph") {
            return Err(invalid(
                part,
                "paragraph-style reference has the wrong style type",
            ));
        }
        if let Some(parent) = style_parent(*node, namespace, part)?
            && required_paragraphs.insert(parent.clone())
        {
            pending.push(parent);
        }
    }
    // Unused list-style libraries do not generate text in this document. Keep
    // those definitions (and unused descendants) in the original. A default or
    // referenced list style is never pruned: its numbering must still block.
    let mut children: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut pending = Vec::new();
    for (id, kind, node) in &ordered {
        if *kind != "paragraph" {
            continue;
        }
        if let Some(parent) = style_parent(*node, namespace, part)? {
            children.entry(parent).or_default().push(id.clone());
        }
        if !required_paragraphs.contains(id)
            && node
                .children()
                .filter(|child| child.has_tag_name((namespace, "pPr")))
                .any(|properties| {
                    properties
                        .children()
                        .any(|property| property.has_tag_name((namespace, "numPr")))
                })
        {
            pending.push(id.clone());
        }
    }
    let mut source_only = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if source_only.insert(id.clone()) {
            if required_paragraphs.contains(&id) {
                return Err(invalid(
                    part,
                    "referenced style cannot depend on a source-only definition",
                ));
            }
            if let Some(descendants) = children.get(&id) {
                pending.extend(descendants.iter().cloned());
            }
        }
    }
    let mut paragraphs = Vec::new();
    let mut characters = Vec::new();
    for (word_id, kind, node) in ordered {
        if kind == "paragraph" && !source_only.contains(&word_id) {
            paragraphs.push(parse_style(node, namespace, part, report, word_id)?);
        } else if kind == "character" && required_characters.contains(&word_id) {
            let style = parse_style(node, namespace, part, report, word_id)?;
            if style.paragraph != ParagraphFormat::default() {
                unsupported_element(
                    node,
                    part,
                    report,
                    "Character styles cannot introduce paragraph semantics.",
                );
            }
            characters.push(style);
        } else {
            inspect_unused_style(node, namespace, part, report)?;
            if kind == "table" && required_tables.contains(&word_id) {
                report.normalize(part, "tblPr/tblStyle", "Applied table-style formatting is normalized to Graphe's simple grid; the full style remains in the original source.");
            }
        }
    }
    if !report.is_supported() {
        // Unprocessed style extensions may contain the referenced definitions.
        // Do not classify those references as corrupt or invent replacements.
        // No semantic document will be published for this read-only outcome.
        return Ok(StyleBook::default());
    }
    let character_book = build_styles(characters, None, document_id, part, report)?;
    let mut book = build_styles(paragraphs, defaults, document_id, part, report)?;
    book.default_character = character_book
        .default_style
        .as_ref()
        .and_then(|id| character_book.effective_characters.get(id.as_str()))
        .cloned();
    for (word_id, id) in character_book.identifiers {
        book.character_styles.insert(
            word_id,
            character_book.effective_characters[id.as_str()].clone(),
        );
    }
    Ok(book)
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
                    if parsed.style_id.is_some()
                        || parsed.section.is_some()
                        || parsed.numbering_id.is_some()
                    {
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

fn style_parent(
    node: Node<'_, '_>,
    namespace: &str,
    part: &str,
) -> Result<Option<String>, ConversionError> {
    let mut parent = None;
    for child in node
        .children()
        .filter(|child| child.has_tag_name((namespace, "basedOn")))
    {
        if parent.is_some() {
            return Err(invalid(part, "duplicate style parent"));
        }
        parent = Some(required_word_attribute(child, namespace, "val", part)?.to_owned());
    }
    Ok(parent)
}

fn validate_style_inheritance(
    declarations: &BTreeMap<String, Node<'_, '_>>,
    namespace: &str,
    part: &str,
) -> Result<(), ConversionError> {
    let mut parents = BTreeMap::new();
    for (id, node) in declarations {
        if let Some(parent) = style_parent(*node, namespace, part)? {
            if !declarations.contains_key(&parent) {
                return Err(invalid(
                    part,
                    "style inheritance references a missing parent",
                ));
            }
            parents.insert(id.as_str(), parent);
        }
    }
    let mut resolved = BTreeSet::new();
    for id in declarations.keys() {
        let mut path = BTreeSet::new();
        let mut current = Some(id.as_str());
        while let Some(id) = current {
            if resolved.contains(id) {
                break;
            }
            if !path.insert(id) {
                return Err(invalid(part, "style inheritance contains a cycle"));
            }
            current = parents.get(id).map(String::as_str);
        }
        resolved.extend(path);
    }
    Ok(())
}

fn style_metadata(
    node: Node<'_, '_>,
    namespace: &str,
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<bool, ConversionError> {
    if node.tag_name().namespace() != Some(namespace) {
        return Ok(false);
    }
    let kind = node.tag_name().name();
    if !matches!(
        kind,
        "qFormat"
            | "semiHidden"
            | "unhideWhenUsed"
            | "hidden"
            | "locked"
            | "autoRedefine"
            | "personal"
            | "personalCompose"
            | "personalReply"
            | "uiPriority"
            | "rsid"
            | "next"
            | "link"
            | "aliases"
    ) {
        return Ok(false);
    }
    check_word_attributes(node, namespace, &["val"], part, report);
    check_leaf(node, part, report)?;
    match kind {
        "uiPriority" => {
            required_word_attribute(node, namespace, "val", part)?
                .parse::<u32>()
                .map_err(|_| invalid(part, "invalid style UI priority"))?;
        }
        "rsid" => {
            let value = required_word_attribute(node, namespace, "val", part)?;
            if value.len() != 8 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err(invalid(part, "invalid style revision session identifier"));
            }
        }
        "next" | "link" | "aliases" => {
            required_word_attribute(node, namespace, "val", part)?;
        }
        _ => {
            let value = node
                .attribute((namespace, "val"))
                .map(|value| on_off(value, part))
                .transpose()?
                .unwrap_or(true);
            if kind == "locked" && value {
                unsupported_element(
                    node,
                    part,
                    report,
                    "Locked style restrictions cannot be bypassed by formatting normalization.",
                );
                return Ok(true);
            }
        }
    }
    report.normalize(part, format!("style/{kind}"), "Style gallery, authoring or future-style behavior is retained in the original; Writer uses its own style controls.");
    Ok(true)
}

fn inspect_latent_styles(
    node: Node<'_, '_>,
    namespace: &str,
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<(), ConversionError> {
    check_word_attributes(
        node,
        namespace,
        &[
            "defLockedState",
            "defUIPriority",
            "defSemiHidden",
            "defUnhideWhenUsed",
            "defQFormat",
            "count",
        ],
        part,
        report,
    );
    let validate = |node: Node<'_, '_>, names: &[&str]| -> Result<(), ConversionError> {
        for name in names {
            if let Some(value) = node.attribute((namespace, *name)) {
                if matches!(*name, "defUIPriority" | "uiPriority" | "count") {
                    value
                        .parse::<u32>()
                        .map_err(|_| invalid(part, "invalid latent-style integer"))?;
                } else {
                    on_off(value, part)?;
                }
            }
        }
        Ok(())
    };
    validate(
        node,
        &[
            "defLockedState",
            "defUIPriority",
            "defSemiHidden",
            "defUnhideWhenUsed",
            "defQFormat",
            "count",
        ],
    )?;
    if node
        .attribute((namespace, "defLockedState"))
        .map(|value| on_off(value, part))
        .transpose()?
        .unwrap_or(false)
    {
        unsupported_element(
            node,
            part,
            report,
            "Latent style locking cannot be bypassed.",
        );
    }
    let mut names = BTreeSet::new();
    for child in node.children() {
        if !child.is_element() {
            check_whitespace(child, part)?;
            continue;
        }
        if !child.has_tag_name((namespace, "lsdException")) {
            unsupported_element(child, part, report, "Unknown latent-style metadata.");
            continue;
        }
        check_word_attributes(
            child,
            namespace,
            &[
                "name",
                "locked",
                "uiPriority",
                "semiHidden",
                "unhideWhenUsed",
                "qFormat",
            ],
            part,
            report,
        );
        check_leaf(child, part, report)?;
        if !names.insert(required_word_attribute(child, namespace, "name", part)?) {
            return Err(invalid(part, "duplicate latent-style name"));
        }
        validate(
            child,
            &[
                "locked",
                "uiPriority",
                "semiHidden",
                "unhideWhenUsed",
                "qFormat",
            ],
        )?;
        if child
            .attribute((namespace, "locked"))
            .map(|value| on_off(value, part))
            .transpose()?
            .unwrap_or(false)
        {
            unsupported_element(
                child,
                part,
                report,
                "Latent style locking cannot be bypassed.",
            );
        }
    }
    report.normalize(part, "latentStyles", "The latent style gallery is preserved in the original; it does not supply document text or instantiated formatting.");
    Ok(())
}

fn inspect_unused_style(
    node: Node<'_, '_>,
    namespace: &str,
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<(), ConversionError> {
    let mut seen = BTreeSet::new();
    for child in node.children() {
        if !child.is_element() {
            check_whitespace(child, part)?;
            continue;
        }
        if !seen.insert(child.tag_name().name()) {
            return Err(invalid(part, "duplicate unused-style property"));
        }
        if style_metadata(child, namespace, part, report)? {
            continue;
        }
        if child.tag_name().namespace() != Some(namespace) {
            unsupported_element(child, part, report, "Unknown unused-style namespace.");
            continue;
        }
        match child.tag_name().name() {
            "name" | "basedOn" => {
                check_word_attributes(child, namespace, &["val"], part, report);
                check_leaf(child, part, report)?;
                required_word_attribute(child, namespace, "val", part)?;
            }
            "rPr" | "pPr" | "tblPr" | "tcPr" | "trPr" | "tblStylePr" => {
                // Only unused formatting libraries may stay opaque. A reference
                // makes character styles go through the semantic parser; actual
                // tables and numbering still fail the body/relationship guards.
                for property in child.descendants() {
                    if property.is_element()
                        && (property.tag_name().namespace() != Some(namespace)
                            || matches!(
                                property.tag_name().name(),
                                "t" | "r"
                                    | "p"
                                    | "tbl"
                                    | "sdt"
                                    | "instrText"
                                    | "fldChar"
                                    | "drawing"
                                    | "pict"
                                    | "object"
                                    | "ins"
                                    | "del"
                                    | "moveFrom"
                                    | "moveTo"
                            ))
                    {
                        unsupported_element(
                            property,
                            part,
                            report,
                            "Content or foreign markup cannot be discarded as an unused formatting definition.",
                        );
                    } else if !property.is_element() {
                        check_whitespace(property, part)?;
                    }
                }
            }
            _ => unsupported_element(child, part, report, "Unknown unused-style construct."),
        }
    }
    report.normalize(part, "style/unused", "An unused character, table or numbering definition is retained in the original rather than added to Writer's paragraph-style model.");
    Ok(())
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
        if !seen.insert(kind) {
            return Err(invalid(part, "duplicate style property"));
        }
        if style_metadata(child, namespace, part, report)? {
            continue;
        }
        if !matches!(kind, "name" | "basedOn" | "pPr" | "rPr") {
            unsupported_element(child, part, report, "This style property is not supported.");
            continue;
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
                if parsed.style_id.is_some()
                    || parsed.section.is_some()
                    || parsed.numbering_id.is_some()
                {
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
        report.normalize(
            part,
            "style/name",
            "A style without a display name uses its stable source style identifier as its Writer display name.",
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
    mut raw: Vec<RawStyle>,
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
    let mut next_suffix = BTreeMap::new();
    for style in &mut raw {
        let base = if style.name.trim().is_empty() {
            style.word_id.clone()
        } else {
            style.name.clone()
        };
        let mut name = base.clone();
        let suffix = next_suffix.entry(base.clone()).or_insert(2usize);
        while !names.insert(name.clone()) {
            name = format!("{base} ({suffix})");
            *suffix += 1;
        }
        if name != style.name {
            report.normalize(part, "style/name", "A blank or repeated style display name was given a unique Writer display name; stable source references remain unchanged.");
            style.name = name;
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
    let mut effective: Vec<Option<CharacterFormat>> = vec![None; raw.len()];
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
            .and_then(|index| effective[index].clone())
            .unwrap_or_else(|| {
                defaults_id
                    .as_ref()
                    .map_or_else(CharacterFormat::default, |_| {
                        book.definitions[0].character_format.clone()
                    })
            });
        for index in chain.into_iter().rev() {
            let previous = inherited.clone();
            inherited = overlay_character(&inherited, &raw[index].character);
            if raw[index].character.bold == Some(true) {
                inherited.bold = Some(!previous.bold.unwrap_or(false));
            } else {
                inherited.bold = previous.bold;
            }
            if raw[index].character.italic == Some(true) {
                inherited.italic = Some(!previous.italic.unwrap_or(false));
            } else {
                inherited.italic = previous.italic;
            }
            effective[index] = Some(inherited.clone());
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
        let values = effective[index]
            .as_ref()
            .expect("all style chains were resolved");
        style.character_format.bold =
            (style.character_format.bold == Some(true)).then_some(values.bold.unwrap_or(false));
        style.character_format.italic =
            (style.character_format.italic == Some(true)).then_some(values.italic.unwrap_or(false));
        book.effective_characters
            .insert(style.id.as_str().to_owned(), values.clone());
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
    if let Some(id) = &book.default_style
        && !book.effective_characters.contains_key(id.as_str())
    {
        book.effective_characters.insert(
            id.as_str().to_owned(),
            book.definitions[0].character_format.clone(),
        );
    }
    Ok(book)
}

fn overlay_character(base: &CharacterFormat, direct: &CharacterFormat) -> CharacterFormat {
    CharacterFormat {
        bold: direct.bold.or(base.bold),
        italic: direct.italic.or(base.italic),
        underline: direct.underline.or(base.underline),
        font_family: direct
            .font_family
            .clone()
            .or_else(|| base.font_family.clone()),
        font_size: direct.font_size.or(base.font_size),
        color: direct.color.or(base.color),
        hyperlink: direct.hyperlink.clone().or_else(|| base.hyperlink.clone()),
    }
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
    let mut seen = BTreeSet::new();
    for node in root.children() {
        if !node.is_element() {
            check_whitespace(node, part)?;
        } else if core_metadata_field(node) {
            if !seen.insert((node.tag_name().namespace(), node.tag_name().name())) {
                return Err(invalid(part, "duplicate core metadata field"));
            }
            check_metadata_attributes(node, part, report);
            let content = simple_text(node, part, report)?;
            if !node.has_tag_name((DC_NAMESPACE, "title")) {
                // These standard descriptive/audit fields do not control body
                // formatting. Their exact values remain in the original rather
                // than forcing an otherwise complete document read-only.
                continue;
            }
            if title.is_some() {
                return Err(invalid(part, "duplicate document title"));
            }
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

fn core_metadata_field(node: Node<'_, '_>) -> bool {
    match node.tag_name().namespace() {
        Some(DC_NAMESPACE) => matches!(
            node.tag_name().name(),
            "title" | "creator" | "subject" | "description" | "identifier" | "language"
        ),
        Some(CORE_NAMESPACE) => matches!(
            node.tag_name().name(),
            "category"
                | "contentStatus"
                | "contentType"
                | "keywords"
                | "lastModifiedBy"
                | "lastPrinted"
                | "revision"
                | "version"
        ),
        Some(DCTERMS_NAMESPACE) => matches!(node.tag_name().name(), "created" | "modified"),
        _ => false,
    }
}

fn check_metadata_attributes(node: Node<'_, '_>, part: &str, report: &mut CompatibilityReport) {
    for attribute in node.attributes() {
        if attribute.namespace() == Some(XML_NAMESPACE)
            && matches!(attribute.name(), "space" | "lang")
        {
            continue;
        }
        if node.tag_name().namespace() == Some(DCTERMS_NAMESPACE)
            && attribute.namespace() == Some(XSI_NAMESPACE)
            && attribute.name() == "type"
            && attribute
                .value()
                .split_once(':')
                .is_some_and(|(prefix, local)| {
                    local == "W3CDTF"
                        && node.lookup_namespace_uri(Some(prefix)) == Some(DCTERMS_NAMESPACE)
                })
        {
            continue;
        }
        unsupported_attribute(node, attribute.name(), part, report);
    }
}

fn parse_main_document(
    bytes: &[u8],
    context: ImportContext<'_>,
    report: &mut CompatibilityReport,
) -> Result<Vec<Section>, ConversionError> {
    let ImportContext {
        namespace,
        part,
        document_id,
        ..
    } = context;
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
    inspect_bookmarks(body, namespace, part, report)?;
    let mut sections = Vec::new();
    let mut blocks = Vec::new();
    let mut paragraph_index = 0;
    let mut table_index = 0;
    let mut preceding_numbering: Option<(u32, ListKind)> = None;
    let mut used_numbering = BTreeSet::new();
    let mut final_properties = None;
    let mut ended_section = false;
    for node in body.children() {
        if !node.is_element() {
            check_whitespace(node, part)?;
            continue;
        }
        if bookmark_marker(node, namespace) {
            continue;
        }
        if final_properties.is_some() {
            return Err(invalid(part, "content follows final section properties"));
        }
        if node.has_tag_name((namespace, "p")) {
            let (paragraph, properties, numbering_id) =
                parse_paragraph_content(node, context, paragraph_index, report)?;
            paragraph_index += 1;
            if let Some(id) = numbering_id {
                if let Some(kind) = paragraph.list() {
                    if preceding_numbering != Some((id, kind)) && used_numbering.contains(&id) {
                        report.push(Issue::UnsupportedValue, part, "pPr/numPr",
                            "A Word numbering instance resumes after a break; Graphe would restart its sequence.");
                    }
                    if let Some((previous_id, previous_kind)) = preceding_numbering
                        && previous_id != id
                        && previous_kind == kind
                    {
                        report.push(Issue::UnsupportedValue, part, "pPr/numPr",
                            "Adjacent Word list groups with separate numbering instances must remain distinct.");
                    }
                    used_numbering.insert(id);
                    preceding_numbering = Some((id, kind));
                } else {
                    preceding_numbering = None;
                }
            } else {
                preceding_numbering = None;
            }
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
                preceding_numbering = None;
            }
        } else if node.has_tag_name((namespace, "tbl")) {
            let table = tables::parse_table(
                node,
                context,
                table_index,
                &mut paragraph_index,
                &mut used_numbering,
                report,
            )?;
            table_index += 1;
            blocks.push(Block::Table(table));
            preceding_numbering = None;
            ended_section = false;
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
    context: ImportContext<'_>,
    index: usize,
    report: &mut CompatibilityReport,
) -> Result<(Paragraph, Option<SectionProperties>, Option<u32>), ConversionError> {
    let ImportContext {
        namespace,
        part,
        document_id,
        styles,
        numbering,
        hyperlinks,
    } = context;
    check_word_attributes(node, namespace, &[], part, report);
    let mut properties = None;
    let mut runs = Vec::new();
    let mut character_styles = Vec::new();
    for child in node.children() {
        if !child.is_element() {
            check_whitespace(child, part)?;
        } else if child.has_tag_name((namespace, "pPr")) {
            if properties.is_some() || !runs.is_empty() {
                return Err(invalid(part, "duplicate or misplaced paragraph properties"));
            }
            properties = Some(parse_paragraph(child, namespace, part, report)?);
        } else if child.has_tag_name((namespace, "r")) {
            let (run, style) = parse_run(child, namespace, part, report)?;
            runs.push(run);
            character_styles.push(style);
        } else if child.has_tag_name((namespace, "hyperlink")) {
            // Only external relationship-backed links are editable. Anchors,
            // fields and unsupported hyperlink content remain preserved.
            let relation_ns = if namespace == WORD_STRICT {
                REL_STRICT.trim_end_matches('/')
            } else {
                REL_TRANSITIONAL.trim_end_matches('/')
            };
            let id = child.attribute((relation_ns, "id"));
            for attribute in child.attributes() {
                if attribute.namespace() != Some(relation_ns) || attribute.name() != "id" {
                    report.push(
                        Issue::UnsupportedAttribute,
                        part,
                        "w:hyperlink",
                        "Unsupported Word hyperlink attribute.",
                    );
                }
            }
            let destination = id.and_then(|id| hyperlinks.get(id));
            if destination.is_none() {
                report.push(
                    Issue::UnsupportedValue,
                    part,
                    "w:hyperlink",
                    "Hyperlink lacks a supported external relationship.",
                );
            }
            for nested in child.children() {
                if !nested.is_element() {
                    check_whitespace(nested, part)?;
                } else if nested.has_tag_name((namespace, "r")) {
                    let (mut run, style) = parse_run(nested, namespace, part, report)?;
                    run.format.hyperlink = destination.cloned();
                    runs.push(run);
                    character_styles.push(style);
                } else {
                    unsupported_element(
                        nested,
                        part,
                        report,
                        "Unsupported hyperlink child; only text runs are editable.",
                    );
                }
            }
        } else if bookmark_marker(child, namespace) {
            // Pairing and leaf/attribute integrity are checked once for the
            // whole document; these markers do not contain text.
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
    let mut numbering_id = None;
    let explicit_style = if let Some(properties) = properties {
        paragraph = paragraph.with_format(properties.format);
        numbering_id = properties.numbering_id;
        if let Some(id) = numbering_id {
            match numbering.get(&id) {
                Some(kind) => paragraph = paragraph.with_list(Some(*kind)),
                None => report.push(
                    Issue::UnsupportedValue,
                    part,
                    "pPr/numPr",
                    "List numbering instance has no supported first-level definition.",
                ),
            }
        }
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
    if let Some(style) = &style {
        paragraph = paragraph.with_style(style.clone());
    }
    let inherited = paragraph
        .style()
        .and_then(|id| styles.effective_characters.get(id.as_str()))
        .cloned()
        .unwrap_or_default();
    let mut materialized = Vec::with_capacity(paragraph.runs().len());
    for (run, word_style) in paragraph.runs().iter().zip(character_styles) {
        let character = if let Some(word_style) = word_style {
            match styles.character_styles.get(&word_style) {
                Some(format) => Some(format),
                None if !report.is_supported() => {
                    report.push(
                        Issue::UnsupportedValue,
                        part,
                        "rPr/rStyle",
                        "The character style cannot be resolved without unsupported content.",
                    );
                    None
                }
                None => {
                    return Err(invalid(
                        part,
                        "run references an unavailable character style",
                    ));
                }
            }
        } else {
            styles.default_character.as_ref()
        };
        let mut run = run.clone();
        if let Some(character) = character {
            let mut projected = character.clone();
            // OOXML character-style b/i values toggle the paragraph result.
            // The native run stores the materialized absolute override.
            projected.bold =
                (character.bold == Some(true)).then_some(!inherited.bold.unwrap_or(false));
            projected.italic =
                (character.italic == Some(true)).then_some(!inherited.italic.unwrap_or(false));
            if projected != CharacterFormat::default() {
                run.format = overlay_character(&projected, &run.format);
                report.normalize(part, "rPr/rStyle", "Character-style appearance was materialized into supported run formatting. The original retains its style definitions and future-style behavior.");
            }
        }
        materialized.push(run);
    }
    paragraph = Paragraph::new(paragraph.id().clone(), materialized)
        .with_format(paragraph.format().clone())
        .with_list(paragraph.list());
    if let Some(style) = style {
        paragraph = paragraph.with_style(style);
    }
    Ok((paragraph, section, numbering_id))
}

fn parse_run(
    node: Node<'_, '_>,
    namespace: &str,
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<(TextRun, Option<String>), ConversionError> {
    check_word_attributes(node, namespace, &[], part, report);
    let mut format = None;
    let mut text = String::new();
    let mut content_seen = false;
    let mut character_style = None;
    for child in node.children() {
        if !child.is_element() {
            check_whitespace(child, part)?;
            continue;
        }
        if child.has_tag_name((namespace, "rPr")) {
            if format.is_some() || content_seen {
                return Err(invalid(part, "duplicate or misplaced run properties"));
            }
            let (parsed, style) = parse_run_character(child, namespace, part, report)?;
            format = Some(parsed);
            character_style = style;
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
    Ok((
        TextRun {
            text,
            format: format.unwrap_or_default(),
        },
        character_style,
    ))
}

fn bookmark_marker(node: Node<'_, '_>, namespace: &str) -> bool {
    node.has_tag_name((namespace, "bookmarkStart")) || node.has_tag_name((namespace, "bookmarkEnd"))
}

fn inspect_bookmarks(
    node: Node<'_, '_>,
    namespace: &str,
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<(), ConversionError> {
    let mut starts = BTreeSet::new();
    let mut ends = BTreeSet::new();
    for marker in node
        .descendants()
        .filter(|marker| bookmark_marker(*marker, namespace))
    {
        let start = marker.has_tag_name((namespace, "bookmarkStart"));
        check_word_attributes(
            marker,
            namespace,
            if start { &["id", "name"] } else { &["id"] },
            part,
            report,
        );
        check_leaf(marker, part, report)?;
        let id = required_word_attribute(marker, namespace, "id", part)?
            .parse::<u32>()
            .map_err(|_| invalid(part, "invalid bookmark range identifier"))?;
        if start {
            required_word_attribute(marker, namespace, "name", part)?;
            if !starts.insert(id) {
                return Err(invalid(part, "duplicate bookmark start identifier"));
            }
        } else if !starts.contains(&id) || !ends.insert(id) {
            return Err(invalid(
                part,
                "unpaired or duplicate bookmark end identifier",
            ));
        }
    }
    if starts != ends {
        return Err(invalid(part, "bookmark range is not closed"));
    }
    if !starts.is_empty() {
        report.normalize(part, "bookmark", "Bookmark anchors are retained in the original; Writer preserves the enclosed text but does not export the original anchors.");
    }
    Ok(())
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
        if node.has_tag_name((namespace, "p"))
            && attribute.namespace() == Some(WORD_2010_NAMESPACE)
            && matches!(attribute.name(), "paraId" | "textId")
            && attribute.value().len() == 8
            && attribute
                .value()
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            continue;
        }
        // Session IDs describe authoring history, not inserted/deleted content.
        // Revision elements themselves still go through the semantic guards.
        if matches!(namespace, WORD_TRANSITIONAL | WORD_STRICT)
            && attribute.namespace() == Some(namespace)
            && matches!(
                attribute.name(),
                "rsidR" | "rsidRDefault" | "rsidRPr" | "rsidP" | "rsidSect" | "rsidDel"
            )
            && attribute.value().len() == 8
            && attribute
                .value()
                .bytes()
                .all(|value| value.is_ascii_hexdigit())
        {
            continue;
        }
        // This declaration does not grant permission to skip extensions: every
        // actual element/property below is still checked and may prevent edits.
        if attribute.namespace() == Some(MC_NAMESPACE)
            && attribute.name() == "Ignorable"
            && attribute
                .value()
                .split_whitespace()
                .all(|prefix| node.lookup_namespace_uri(Some(prefix)).is_some())
        {
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
