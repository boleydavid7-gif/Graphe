//! Deterministic supported-profile WordprocessingML generation.

use std::{
    collections::BTreeMap,
    io::{self, Cursor, Write},
};

use office_core::{
    Alignment, Block, CharacterFormat, Document, LineSpacing, ListKind, NamedStyle,
    PageOrientation, Paragraph, ParagraphFormat, SectionProperties, Table, TextColor, Underline,
};
use quick_xml::{
    Writer,
    events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event, attributes::Attribute},
};
use zip::{CompressionMethod, DateTime, System, ZipWriter, write::SimpleFileOptions};

use crate::{ConversionError, DocxError, MAX_INPUT_BYTES, MAX_XML_BYTES, inspect_docx};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const REL: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const OFFICE_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const CT: &str = "http://schemas.openxmlformats.org/package/2006/content-types";
const CP: &str = "http://schemas.openxmlformats.org/package/2006/metadata/core-properties";
const DC: &str = "http://purl.org/dc/elements/1.1/";

pub(crate) fn export_document(document: &Document) -> Result<Vec<u8>, ConversionError> {
    document.validate()?;
    if document
        .sections()
        .iter()
        .take(document.sections().len().saturating_sub(1))
        .any(|section| section.blocks().is_empty())
    {
        return Err(unsupported(
            "an empty nonfinal section has no paragraph for its section boundary",
        ));
    }

    let styles = StyleMap::new(document.styles());
    let (list_ids, list_instances) = list_numbering(document);
    let links = document_links(document);
    // Fixed order and timestamp prevent application/session state entering ZIP
    // output. The creator system is explicit for native/WASM consistency.
    let has_numbering = !list_instances.is_empty();
    let mut parts = vec![
        ("[Content_Types].xml", content_types(has_numbering)?),
        ("_rels/.rels", package_relationships()?),
        (
            "word/document.xml",
            document_xml(document, &styles, &list_ids, &links)?,
        ),
        (
            "word/_rels/document.xml.rels",
            document_relationships(has_numbering, &links)?,
        ),
        ("word/styles.xml", styles_xml(document.styles(), &styles)?),
        (
            "docProps/core.xml",
            core_properties(&document.metadata().title)?,
        ),
    ];
    if has_numbering {
        parts.push(("word/numbering.xml", numbering_xml(&list_instances)?));
    }
    write_parts(parts)
}

pub(crate) fn write_parts<N: AsRef<str>>(
    parts: impl IntoIterator<Item = (N, Vec<u8>)>,
) -> Result<Vec<u8>, ConversionError> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default()
        .system(System::Unix)
        .compression_method(CompressionMethod::Deflated)
        .compression_level(Some(6))
        .last_modified_time(
            DateTime::from_date_and_time(2020, 1, 1, 0, 0, 0)
                .expect("the fixed ZIP timestamp is valid"),
        )
        .unix_permissions(0o644);
    let mut expanded = 0_usize;
    for (index, (name, bytes)) in parts.into_iter().enumerate() {
        if index >= crate::MAX_PACKAGE_ENTRIES {
            return Err(limit(
                "package_entries",
                crate::MAX_PACKAGE_ENTRIES,
                index + 1,
            ));
        }
        if bytes.len() > crate::MAX_PART_BYTES {
            return Err(limit("part_bytes", crate::MAX_PART_BYTES, bytes.len()));
        }
        expanded = expanded.saturating_add(bytes.len());
        if expanded > crate::MAX_EXPANDED_BYTES {
            return Err(limit("expanded_bytes", crate::MAX_EXPANDED_BYTES, expanded));
        }
        writer
            .start_file(name.as_ref(), options)
            .map_err(zip_error)?;
        writer.write_all(&bytes).map_err(zip_error)?;
    }
    let bytes = writer.finish().map_err(zip_error)?.into_inner();
    if bytes.len() > MAX_INPUT_BYTES {
        return Err(limit("compressed_bytes", MAX_INPUT_BYTES, bytes.len()));
    }
    // Generated output obeys the same public package/XML profile as imported
    // data, including semantic-independent node/attribute expansion budgets.
    inspect_docx(&bytes)?;
    Ok(bytes)
}

struct StyleMap<'a> {
    indices: BTreeMap<&'a str, usize>,
    names: Vec<String>,
    effective: Vec<(bool, bool)>,
}

impl<'a> StyleMap<'a> {
    fn new(styles: &'a [NamedStyle]) -> Self {
        let indices: BTreeMap<_, _> = styles
            .iter()
            .enumerate()
            .map(|(index, style)| (style.id.as_str(), index))
            .collect();
        let mut resolved = vec![None; styles.len()];
        // A validated graph is acyclic. Resolve each node once rather than
        // walking every ancestor again for deep style chains.
        for start in 0..styles.len() {
            if resolved[start].is_some() {
                continue;
            }
            let mut pending = Vec::new();
            let mut current = Some(start);
            let mut inherited = (false, false);
            while let Some(index) = current {
                if let Some(value) = resolved[index] {
                    inherited = value;
                    break;
                }
                pending.push(index);
                current = styles[index]
                    .based_on
                    .as_ref()
                    .map(|id| indices[id.as_str()]);
            }
            for index in pending.into_iter().rev() {
                let format = &styles[index].character_format;
                inherited = (
                    format.bold.unwrap_or(inherited.0),
                    format.italic.unwrap_or(inherited.1),
                );
                resolved[index] = Some(inherited);
            }
        }
        Self {
            indices,
            names: (0..styles.len()).map(|index| format!("s{index}")).collect(),
            effective: resolved
                .into_iter()
                .map(|value| value.expect("validated style graph was resolved"))
                .collect(),
        }
    }

    fn name(&self, id: &office_core::Id) -> &str {
        &self.names[self.indices[id.as_str()]]
    }

    fn parent_format(&self, style: &NamedStyle) -> (bool, bool) {
        style
            .based_on
            .as_ref()
            .map(|id| self.effective[self.indices[id.as_str()]])
            .unwrap_or((false, false))
    }
}

fn content_types(has_numbering: bool) -> Result<Vec<u8>, ConversionError> {
    let mut xml = Xml::new();
    xml.start("Types", &[("xmlns", CT)])?;
    xml.empty(
        "Default",
        &[
            ("Extension", "rels"),
            (
                "ContentType",
                "application/vnd.openxmlformats-package.relationships+xml",
            ),
        ],
    )?;
    for (name, content_type) in [
        (
            "/word/document.xml",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
        ),
        (
            "/word/styles.xml",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml",
        ),
        (
            "/docProps/core.xml",
            "application/vnd.openxmlformats-package.core-properties+xml",
        ),
    ] {
        xml.empty(
            "Override",
            &[("PartName", name), ("ContentType", content_type)],
        )?;
    }
    if has_numbering {
        xml.empty(
            "Override",
            &[
                ("PartName", "/word/numbering.xml"),
                (
                    "ContentType",
                    "application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml",
                ),
            ],
        )?;
    }
    xml.end("Types")?;
    Ok(xml.finish())
}

/// Assign stable package-local IDs to distinct external destinations.
fn document_links(document: &Document) -> BTreeMap<String, String> {
    let mut links = BTreeMap::new();
    for paragraph in document.paragraphs() {
        for run in paragraph.runs() {
            if let Some(address) = &run.format.hyperlink
                && !links.contains_key(address)
            {
                let number = links.len() + 1;
                links.insert(address.clone(), format!("rIdLink{number}"));
            }
        }
    }
    links
}

/// Assign each contiguous list an independent OOXML instance so decimal
/// numbering restarts after a normal paragraph or a different list kind.
fn list_numbering(document: &Document) -> (BTreeMap<String, u32>, Vec<(u32, ListKind)>) {
    let mut paragraphs = BTreeMap::new();
    let mut instances = Vec::new();
    let mut current = 0_u32;
    fn assign<'a>(
        flow: impl IntoIterator<Item = &'a Paragraph>,
        paragraphs: &mut BTreeMap<String, u32>,
        instances: &mut Vec<(u32, ListKind)>,
        current: &mut u32,
        previous: &mut Option<ListKind>,
    ) {
        for paragraph in flow {
            let kind = paragraph.list();
            if kind != *previous
                && let Some(kind) = kind
            {
                *current += 1;
                instances.push((*current, kind));
            }
            if kind.is_some() {
                paragraphs.insert(paragraph.id().as_str().into(), *current);
            }
            *previous = kind;
        }
    }
    for section in document.sections() {
        let mut previous = None;
        for block in section.blocks() {
            match block {
                Block::Paragraph(paragraph) => assign(
                    std::iter::once(paragraph),
                    &mut paragraphs,
                    &mut instances,
                    &mut current,
                    &mut previous,
                ),
                Block::Table(table) => {
                    previous = None;
                    for row in table.rows() {
                        for cell in row.cells() {
                            assign(
                                cell.paragraphs(),
                                &mut paragraphs,
                                &mut instances,
                                &mut current,
                                &mut None,
                            );
                        }
                    }
                }
            }
        }
    }
    (paragraphs, instances)
}

/// Well-formed, level-zero OOXML numbering definitions for first-level lists.
fn numbering_xml(instances: &[(u32, ListKind)]) -> Result<Vec<u8>, ConversionError> {
    let mut xml = Xml::new();
    xml.start("w:numbering", &[("xmlns:w", W)])?;
    for (abstract_id, format, value) in [(1, "bullet", "•"), (2, "decimal", "%1.")] {
        let id = abstract_id.to_string();
        xml.start("w:abstractNum", &[("w:abstractNumId", &id)])?;
        xml.empty("w:multiLevelType", &[("w:val", "singleLevel")])?;
        xml.start("w:lvl", &[("w:ilvl", "0")])?;
        xml.empty("w:start", &[("w:val", "1")])?;
        xml.empty("w:numFmt", &[("w:val", format)])?;
        xml.empty("w:lvlText", &[("w:val", value)])?;
        xml.empty("w:lvlJc", &[("w:val", "left")])?;
        xml.end("w:lvl")?;
        xml.end("w:abstractNum")?;
    }
    for (id, kind) in instances {
        let number = id.to_string();
        let abstract_id = match kind {
            ListKind::Bullet => "1",
            ListKind::Numbered => "2",
        };
        xml.start("w:num", &[("w:numId", &number)])?;
        xml.empty("w:abstractNumId", &[("w:val", abstract_id)])?;
        xml.end("w:num")?;
    }
    xml.end("w:numbering")?;
    Ok(xml.finish())
}

fn package_relationships() -> Result<Vec<u8>, ConversionError> {
    let mut xml = Xml::new();
    xml.start("Relationships", &[("xmlns", REL)])?;
    let main_type = format!("{OFFICE_REL}/officeDocument");
    xml.empty(
        "Relationship",
        &[
            ("Id", "rIdDocument"),
            ("Type", &main_type),
            ("Target", "word/document.xml"),
        ],
    )?;
    xml.empty(
        "Relationship",
        &[
            ("Id", "rIdCore"),
            (
                "Type",
                "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties",
            ),
            ("Target", "docProps/core.xml"),
        ],
    )?;
    xml.end("Relationships")?;
    Ok(xml.finish())
}

fn document_relationships(
    has_numbering: bool,
    links: &BTreeMap<String, String>,
) -> Result<Vec<u8>, ConversionError> {
    let mut xml = Xml::new();
    xml.start("Relationships", &[("xmlns", REL)])?;
    let relationship_type = format!("{OFFICE_REL}/styles");
    xml.empty(
        "Relationship",
        &[
            ("Id", "rIdStyles"),
            ("Type", &relationship_type),
            ("Target", "styles.xml"),
        ],
    )?;
    if has_numbering {
        xml.empty(
            "Relationship",
            &[
                ("Id", "rIdNumbering"),
                ("Type", &format!("{OFFICE_REL}/numbering")),
                ("Target", "numbering.xml"),
            ],
        )?;
    }
    for (address, id) in links {
        xml.empty(
            "Relationship",
            &[
                ("Id", id.as_str()),
                ("Type", &format!("{OFFICE_REL}/hyperlink")),
                ("Target", address.as_str()),
                ("TargetMode", "External"),
            ],
        )?;
    }
    xml.end("Relationships")?;
    Ok(xml.finish())
}

fn core_properties(title: &str) -> Result<Vec<u8>, ConversionError> {
    let mut xml = Xml::new();
    xml.start("cp:coreProperties", &[("xmlns:cp", CP), ("xmlns:dc", DC)])?;
    xml.start("dc:title", &[])?;
    xml.text(title)?;
    xml.end("dc:title")?;
    xml.end("cp:coreProperties")?;
    Ok(xml.finish())
}

fn document_xml(
    document: &Document,
    styles: &StyleMap<'_>,
    list_ids: &BTreeMap<String, u32>,
    links: &BTreeMap<String, String>,
) -> Result<Vec<u8>, ConversionError> {
    let mut xml = Xml::new();
    xml.start("w:document", &[("xmlns:w", W), ("xmlns:r", OFFICE_REL)])?;
    xml.start("w:body", &[])?;
    let last_section = document.sections().len().checked_sub(1);
    for (section_index, section) in document.sections().iter().enumerate() {
        let final_section = Some(section_index) == last_section;
        for (block_index, block) in section.blocks().iter().enumerate() {
            let section_boundary = !final_section && block_index + 1 == section.blocks().len();
            match block {
                Block::Paragraph(paragraph) => write_paragraph(
                    &mut xml,
                    paragraph,
                    styles,
                    list_ids.get(paragraph.id().as_str()).copied(),
                    links,
                    section_boundary.then_some(section.properties()),
                )?,
                Block::Table(table) => {
                    if section_boundary {
                        return Err(unsupported(
                            "a nonfinal section ending in a table needs a paragraph section-boundary carrier",
                        ));
                    }
                    write_table(
                        &mut xml,
                        table,
                        section.properties(),
                        styles,
                        list_ids,
                        links,
                    )?;
                }
            }
        }
        if final_section {
            write_section(&mut xml, section.properties())?;
        }
    }
    xml.end("w:body")?;
    xml.end("w:document")?;
    Ok(xml.finish())
}

fn write_table(
    xml: &mut Xml,
    table: &Table,
    section: Option<&SectionProperties>,
    styles: &StyleMap<'_>,
    list_ids: &BTreeMap<String, u32>,
    links: &BTreeMap<String, String>,
) -> Result<(), ConversionError> {
    let columns = table.rows()[0].cells().len();
    let widths: Vec<i32> = if let Some(widths) = table.column_widths() {
        widths.iter().map(|value| value.0).collect()
    } else {
        let page_width = section
            .and_then(|value| value.page_size)
            .map_or(12240_i64, |size| i64::from(size.width.0));
        let body_width =
            section
                .and_then(|value| value.page_margins)
                .map_or(page_width - 2880, |margins| {
                    page_width
                        - i64::from(margins.left.0)
                        - i64::from(margins.right.0)
                        - i64::from(margins.gutter.0)
                });
        if body_width <= 0 {
            return Err(unsupported("table requires a positive section body width"));
        }
        let body_width = i32::try_from(body_width)
            .map_err(|_| unsupported("table body width exceeds the supported range"))?;
        let count = i32::try_from(columns).map_err(|_| unsupported("too many table columns"))?;
        if body_width < count {
            return Err(unsupported("table columns cannot have zero width"));
        }
        let mut values = vec![body_width / count; columns];
        values[columns - 1] += body_width % count;
        values
    };
    xml.start("w:tbl", &[])?;
    xml.start("w:tblPr", &[])?;
    let total: i64 = widths.iter().map(|value| i64::from(*value)).sum();
    xml.empty("w:tblW", &[("w:w", &total.to_string()), ("w:type", "dxa")])?;
    xml.empty("w:tblLayout", &[("w:type", "fixed")])?;
    xml.start("w:tblBorders", &[])?;
    for edge in [
        "w:top",
        "w:left",
        "w:bottom",
        "w:right",
        "w:insideH",
        "w:insideV",
    ] {
        xml.empty(
            edge,
            &[("w:val", "single"), ("w:sz", "4"), ("w:color", "808080")],
        )?;
    }
    xml.end("w:tblBorders")?;
    xml.start("w:tblCellMar", &[])?;
    for edge in ["w:top", "w:left", "w:bottom", "w:right"] {
        xml.empty(edge, &[("w:w", "80"), ("w:type", "dxa")])?;
    }
    xml.end("w:tblCellMar")?;
    xml.end("w:tblPr")?;
    xml.start("w:tblGrid", &[])?;
    for width in &widths {
        xml.empty("w:gridCol", &[("w:w", &width.to_string())])?;
    }
    xml.end("w:tblGrid")?;
    for row in table.rows() {
        xml.start("w:tr", &[])?;
        xml.start("w:trPr", &[])?;
        xml.empty("w:cantSplit", &[])?;
        xml.end("w:trPr")?;
        for (column, cell) in row.cells().iter().enumerate() {
            xml.start("w:tc", &[])?;
            xml.start("w:tcPr", &[])?;
            xml.empty(
                "w:tcW",
                &[("w:w", &widths[column].to_string()), ("w:type", "dxa")],
            )?;
            xml.end("w:tcPr")?;
            for paragraph in cell.paragraphs() {
                write_paragraph(
                    xml,
                    paragraph,
                    styles,
                    list_ids.get(paragraph.id().as_str()).copied(),
                    links,
                    None,
                )?;
            }
            xml.end("w:tc")?;
        }
        xml.end("w:tr")?;
    }
    xml.end("w:tbl")
}

fn write_paragraph(
    xml: &mut Xml,
    paragraph: &Paragraph,
    styles: &StyleMap<'_>,
    list_id: Option<u32>,
    links: &BTreeMap<String, String>,
    boundary: Option<Option<&SectionProperties>>,
) -> Result<(), ConversionError> {
    xml.start("w:p", &[])?;
    write_paragraph_properties(
        xml,
        paragraph.format(),
        paragraph.style().map(|id| styles.name(id)),
        list_id,
        boundary,
    )?;
    for run in paragraph.runs() {
        if let Some(address) = run.format.hyperlink.as_deref() {
            let id = links
                .get(address)
                .ok_or_else(|| unsupported("missing hyperlink relationship"))?;
            xml.start("w:hyperlink", &[("r:id", id.as_str())])?;
        }
        xml.start("w:r", &[])?;
        let mut properties = run.format.clone();
        properties.hyperlink = None;
        write_character_properties(xml, &properties, None)?;
        let mut start = 0;
        for (offset, character) in run.text.char_indices() {
            let element = match character {
                '\t' => "w:tab",
                '\n' => "w:br",
                _ => continue,
            };
            if offset > start {
                write_run_text(xml, &run.text[start..offset])?;
            }
            xml.empty(element, &[])?;
            start = offset + character.len_utf8();
        }
        if start < run.text.len() || run.text.is_empty() {
            write_run_text(xml, &run.text[start..])?;
        }
        xml.end("w:r")?;
        if run.format.hyperlink.is_some() {
            xml.end("w:hyperlink")?;
        }
    }
    xml.end("w:p")?;
    Ok(())
}

fn write_run_text(xml: &mut Xml, text: &str) -> Result<(), ConversionError> {
    xml.start("w:t", &[("xml:space", "preserve")])?;
    xml.text(text)?;
    xml.end("w:t")
}

fn styles_xml(styles: &[NamedStyle], mapping: &StyleMap<'_>) -> Result<Vec<u8>, ConversionError> {
    let mut xml = Xml::new();
    xml.start("w:styles", &[("xmlns:w", W)])?;
    for style in styles {
        xml.start(
            "w:style",
            &[
                ("w:type", "paragraph"),
                ("w:styleId", mapping.name(&style.id)),
            ],
        )?;
        xml.empty("w:name", &[("w:val", &style.name)])?;
        if let Some(parent) = &style.based_on {
            xml.empty("w:basedOn", &[("w:val", mapping.name(parent))])?;
        }
        write_paragraph_properties(&mut xml, &style.paragraph_format, None, None, None)?;
        write_character_properties(
            &mut xml,
            &style.character_format,
            Some(mapping.parent_format(style)),
        )?;
        xml.end("w:style")?;
    }
    xml.end("w:styles")?;
    Ok(xml.finish())
}

fn write_character_properties(
    xml: &mut Xml,
    format: &CharacterFormat,
    style_parent: Option<(bool, bool)>,
) -> Result<(), ConversionError> {
    if format == &CharacterFormat::default() {
        return Ok(());
    }
    xml.start("w:rPr", &[])?;
    if let Some(family) = &format.font_family {
        xml.empty(
            "w:rFonts",
            &[
                ("w:ascii", family),
                ("w:hAnsi", family),
                ("w:eastAsia", family),
                ("w:cs", family),
            ],
        )?;
    }
    for (value, parent, normal, complex) in [
        (
            format.bold,
            style_parent.map(|value| value.0),
            "w:b",
            "w:bCs",
        ),
        (
            format.italic,
            style_parent.map(|value| value.1),
            "w:i",
            "w:iCs",
        ),
    ] {
        if let Some(value) = value {
            // True toggles a style's inherited value. In direct properties the
            // same tokens are absolute. False style tokens leave the parent.
            let token = parent.map_or(value, |parent| value != parent);
            write_on_off(xml, normal, token)?;
            write_on_off(xml, complex, token)?;
        }
    }
    if let Some(color) = format.color {
        let color = match color {
            TextColor::Auto {} => "auto".into(),
            TextColor::Rgb { red, green, blue } => format!("{red:02X}{green:02X}{blue:02X}"),
        };
        xml.empty("w:color", &[("w:val", &color)])?;
    }
    if let Some(size) = format.font_size {
        let size = size.0.to_string();
        xml.empty("w:sz", &[("w:val", &size)])?;
        xml.empty("w:szCs", &[("w:val", &size)])?;
    }
    if let Some(underline) = format.underline {
        let value = match underline {
            Underline::None => "none",
            Underline::Single => "single",
            Underline::Double => "double",
        };
        xml.empty("w:u", &[("w:val", value)])?;
    }
    xml.end("w:rPr")
}

fn write_on_off(xml: &mut Xml, name: &str, value: bool) -> Result<(), ConversionError> {
    xml.empty(name, &[("w:val", if value { "1" } else { "0" })])
}

fn write_paragraph_properties(
    xml: &mut Xml,
    format: &ParagraphFormat,
    style: Option<&str>,
    list_id: Option<u32>,
    section_boundary: Option<Option<&SectionProperties>>,
) -> Result<(), ConversionError> {
    if format == &ParagraphFormat::default()
        && style.is_none()
        && list_id.is_none()
        && section_boundary.is_none()
    {
        return Ok(());
    }
    xml.start("w:pPr", &[])?;
    if let Some(style) = style {
        xml.empty("w:pStyle", &[("w:val", style)])?;
    }
    if let Some(list_id) = list_id {
        xml.start("w:numPr", &[])?;
        xml.empty("w:ilvl", &[("w:val", "0")])?;
        xml.empty("w:numId", &[("w:val", &list_id.to_string())])?;
        xml.end("w:numPr")?;
    }
    if let Some(value) = format.page_break_before {
        write_on_off(xml, "w:pageBreakBefore", value)?;
    }
    let mut attributes = Vec::new();
    if let Some(value) = format.spacing_before {
        attributes.push(("w:before", value.0.to_string()));
    }
    if let Some(value) = format.spacing_after {
        attributes.push(("w:after", value.0.to_string()));
    }
    if let Some(value) = format.line_spacing {
        let (height, rule) = match value {
            LineSpacing::Multiple { value } => (value.to_string(), "auto"),
            LineSpacing::Exact { height } => (height.0.to_string(), "exact"),
            LineSpacing::AtLeast { height } => (height.0.to_string(), "atLeast"),
        };
        attributes.push(("w:line", height));
        attributes.push(("w:lineRule", rule.into()));
    }
    if !attributes.is_empty() {
        xml.empty_owned("w:spacing", &attributes)?;
    }
    if let Some(indentation) = &format.indentation {
        let mut attributes = Vec::new();
        if let Some(value) = indentation.left {
            attributes.push(("w:left", value.0.to_string()));
        }
        if let Some(value) = indentation.right {
            attributes.push(("w:right", value.0.to_string()));
        }
        if let Some(value) = indentation.first_line {
            let (name, value) = if value.0 < 0 {
                ("w:hanging", value.0.unsigned_abs())
            } else {
                ("w:firstLine", value.0 as u32)
            };
            attributes.push((name, value.to_string()));
        }
        // Keep Some(empty) a documented native-only distinction; an empty ind
        // element is semantically equivalent and never invents an indent.
        xml.empty_owned("w:ind", &attributes)?;
    }
    if let Some(alignment) = format.alignment {
        let value = match alignment {
            Alignment::Left => "left",
            Alignment::Center => "center",
            Alignment::Right => "right",
            Alignment::Justify => "both",
        };
        xml.empty("w:jc", &[("w:val", value)])?;
    }
    if let Some(properties) = section_boundary {
        write_section(xml, properties)?;
    }
    xml.end("w:pPr")
}

fn write_section(
    xml: &mut Xml,
    properties: Option<&SectionProperties>,
) -> Result<(), ConversionError> {
    let Some(properties) = properties else {
        return xml.empty("w:sectPr", &[]);
    };
    xml.start("w:sectPr", &[])?;
    if let Some(size) = &properties.page_size {
        let mut attributes = vec![
            ("w:w", size.width.0.to_string()),
            ("w:h", size.height.0.to_string()),
        ];
        if let Some(orientation) = size.orientation {
            attributes.push((
                "w:orient",
                match orientation {
                    PageOrientation::Portrait => "portrait",
                    PageOrientation::Landscape => "landscape",
                }
                .into(),
            ));
        }
        xml.empty_owned("w:pgSz", &attributes)?;
    }
    if let Some(margins) = &properties.page_margins {
        let attributes = [
            ("w:top", margins.top.0.to_string()),
            ("w:right", margins.right.0.to_string()),
            ("w:bottom", margins.bottom.0.to_string()),
            ("w:left", margins.left.0.to_string()),
            ("w:header", margins.header.0.to_string()),
            ("w:footer", margins.footer.0.to_string()),
            ("w:gutter", margins.gutter.0.to_string()),
        ];
        xml.empty_owned("w:pgMar", &attributes)?;
    }
    xml.end("w:sectPr")
}

struct Xml {
    writer: Writer<XmlBytes>,
}

impl Xml {
    fn new() -> Self {
        let mut result = Self {
            writer: Writer::new(XmlBytes::default()),
        };
        result
            .event(Event::Decl(BytesDecl::new(
                "1.0",
                Some("UTF-8"),
                Some("yes"),
            )))
            .expect("a fixed declaration fits the XML output budget");
        result
    }

    fn start(&mut self, name: &str, attributes: &[(&str, &str)]) -> Result<(), ConversionError> {
        let element = self.element(name, attributes)?;
        self.event(Event::Start(element))
    }

    fn empty(&mut self, name: &str, attributes: &[(&str, &str)]) -> Result<(), ConversionError> {
        let element = self.element(name, attributes)?;
        self.event(Event::Empty(element))
    }

    fn empty_owned(
        &mut self,
        name: &str,
        attributes: &[(&str, String)],
    ) -> Result<(), ConversionError> {
        self.empty(
            name,
            &attributes
                .iter()
                .map(|(name, value)| (*name, value.as_str()))
                .collect::<Vec<_>>(),
        )
    }

    fn element<'a>(
        &self,
        name: &'a str,
        attributes: &'a [(&'a str, &'a str)],
    ) -> Result<BytesStart<'a>, ConversionError> {
        let mut element = BytesStart::new(name);
        for &(key, value) in attributes {
            validate_xml_string(value)?;
            // quick-xml 0.42 escapes CR/LF/TAB in attributes as character
            // references, in addition to ordinary XML reserved characters.
            let attribute = Attribute::from((key, value));
            let actual = self.writer.get_ref().bytes.len()
                + element.as_ref().len()
                + key.len()
                + attribute.value.len()
                + 7;
            if actual > MAX_XML_BYTES {
                return Err(limit("xml_bytes", MAX_XML_BYTES, actual));
            }
            element.push_attribute(attribute);
        }
        Ok(element)
    }

    fn end(&mut self, name: &str) -> Result<(), ConversionError> {
        self.event(Event::End(BytesEnd::new(name)))
    }

    fn text(&mut self, text: &str) -> Result<(), ConversionError> {
        validate_xml_string(text)?;
        if text.contains('\r') {
            // Character references preserve XML scalar values, but an
            // independent application may reinterpret CR as a paragraph
            // boundary. Do not export that structural change silently.
            return Err(unsupported(
                "literal carriage-return text requires an explicit cross-application mapping",
            ));
        }
        let mut start = 0;
        while start < text.len() {
            let mut end = (start + 8_192).min(text.len());
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            self.event(Event::Text(BytesText::new(&text[start..end])))?;
            start = end;
        }
        Ok(())
    }

    fn event(&mut self, event: Event<'_>) -> Result<(), ConversionError> {
        self.writer.write_event(event).map_err(|error| {
            self.writer.get_ref().exceeded.map_or_else(
                || unsupported(format!("XML output failed: {error}")),
                |actual| limit("xml_bytes", MAX_XML_BYTES, actual),
            )
        })
    }

    fn finish(self) -> Vec<u8> {
        self.writer.into_inner().bytes
    }
}

#[derive(Default)]
struct XmlBytes {
    bytes: Vec<u8>,
    exceeded: Option<usize>,
}

impl Write for XmlBytes {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let actual = self.bytes.len().saturating_add(bytes.len());
        if actual > MAX_XML_BYTES {
            self.exceeded = Some(actual);
            return Err(io::Error::other("DOCX XML part budget exceeded"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn validate_xml_string(value: &str) -> Result<(), ConversionError> {
    if value.len() > MAX_XML_BYTES {
        return Err(limit("xml_bytes", MAX_XML_BYTES, value.len()));
    }
    if let Some(character) = value.chars().find(|&character| {
        !matches!(character, '\t' | '\n' | '\r' | '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}')
    }) {
        return Err(unsupported(format!(
            "XML 1.0 cannot represent character U+{:04X}",
            character as u32
        )));
    }
    Ok(())
}

fn zip_error(error: impl std::fmt::Display) -> ConversionError {
    unsupported(format!("ZIP generation failed: {error}"))
}

fn unsupported(feature: impl Into<String>) -> ConversionError {
    ConversionError::UnsupportedExport {
        feature: feature.into(),
    }
}

fn limit(resource: &'static str, limit: usize, actual: usize) -> ConversionError {
    DocxError::LimitExceeded {
        resource,
        limit: limit as u64,
        actual: actual as u64,
    }
    .into()
}

#[cfg(test)]
mod tests {
    use office_core::{
        DocumentMetadata, HalfPoints, Id, Indentation, PageMargins, PageSize, Section, TextRun,
        Twips,
    };

    use super::*;

    fn id(value: &str) -> Id {
        Id::new(value).unwrap()
    }

    fn document(paragraphs: Vec<Paragraph>) -> Document {
        Document::from_parts(
            id("native document:任意"),
            DocumentMetadata {
                title: "Title & <safe> 👩‍💻".into(),
            },
            vec![Section::new(
                id("section"),
                paragraphs.into_iter().map(Block::Paragraph).collect(),
            )],
            vec![],
        )
        .unwrap()
    }

    fn part(bytes: &[u8], name: &str) -> String {
        let package = crate::package::read_package(bytes).unwrap();
        String::from_utf8(package.parts[name].clone()).unwrap()
    }

    fn parsed_text(xml: &str) -> String {
        let document = roxmltree::Document::parse(xml).unwrap();
        let mut text = String::new();
        for element in document.descendants().filter(|node| node.is_element()) {
            match element.tag_name().name() {
                "t" => text.push_str(element.text().unwrap_or_default()),
                "br" => text.push('\n'),
                "tab" => text.push('\t'),
                _ => {}
            }
        }
        text
    }

    #[test]
    fn export_is_a_deterministic_inspectable_docx_with_fixed_part_order() {
        let document = document(vec![Paragraph::new(
            id("paragraph"),
            vec![TextRun::new("Hello, 世界!")],
        )]);
        let first = export_document(&document).unwrap();
        assert_eq!(first, export_document(&document).unwrap());
        let inventory = inspect_docx(&first).unwrap();
        assert_eq!(inventory.main_document_part, "word/document.xml");
        assert_eq!(inventory.parts.len(), 5);
        assert_eq!(inventory.relationship_count, 3);
        let mut zip = zip::ZipArchive::new(Cursor::new(&first)).unwrap();
        assert_eq!(
            zip.by_index(0).unwrap().name().unwrap(),
            "[Content_Types].xml"
        );
        assert_eq!(zip.by_index(1).unwrap().name().unwrap(), "_rels/.rels");
        assert_eq!(
            zip.by_index(2).unwrap().name().unwrap(),
            "word/document.xml"
        );
    }

    #[test]
    fn unicode_reserved_characters_tabs_and_soft_breaks_survive_xml_encoding() {
        let text = "  A & <tag> \"quoted\" 'apostrophe' B\nC\tCafe\u{301} 👩‍💻  ";
        let document = document(vec![Paragraph::new(id("p"), vec![TextRun::new(text)])]);
        let bytes = export_document(&document).unwrap();
        let xml = part(&bytes, "word/document.xml");
        assert!(xml.contains("xml:space=\"preserve\""));
        assert_eq!(parsed_text(&xml), text);
        let core = part(&bytes, "docProps/core.xml");
        let core = roxmltree::Document::parse(&core).unwrap();
        assert_eq!(
            core.descendants()
                .find(|node| node.has_tag_name((DC, "title")))
                .unwrap()
                .text(),
            Some(document.metadata().title.as_str())
        );
    }

    #[test]
    fn attribute_whitespace_is_encoded_without_xml_normalization_loss() {
        let family = "Font\rFamily\nWith\tWhitespace & \"quote\"";
        let mut run = TextRun::new("x");
        run.format.font_family = Some(family.into());
        let bytes = export_document(&document(vec![Paragraph::new(id("p"), vec![run])])).unwrap();
        let xml = part(&bytes, "word/document.xml");
        assert!(xml.contains("&#13;"));
        assert!(xml.contains("&#10;"));
        assert!(xml.contains("&#9;"));
        let parsed = roxmltree::Document::parse(&xml).unwrap();
        let fonts = parsed
            .descendants()
            .find(|node| node.has_tag_name((W, "rFonts")))
            .unwrap();
        for attribute in ["ascii", "hAnsi", "eastAsia", "cs"] {
            assert_eq!(fonts.attribute((W, attribute)), Some(family));
        }
    }

    #[test]
    fn common_font_compatibility_names_are_preserved_in_docx_xml() {
        for name in ["Times New Roman", "Arial", "Calibri", "Cambria"] {
            let mut run = TextRun::new("Word compatibility");
            run.format.font_family = Some(name.into());
            let bytes =
                export_document(&document(vec![Paragraph::new(id("p"), vec![run])])).unwrap();
            let xml = part(&bytes, "word/document.xml");
            let parsed = roxmltree::Document::parse(&xml).unwrap();
            let fonts = parsed
                .descendants()
                .find(|node| node.has_tag_name((W, "rFonts")))
                .unwrap();
            for attribute in ["ascii", "hAnsi", "eastAsia", "cs"] {
                assert_eq!(fonts.attribute((W, attribute)), Some(name));
            }
        }
    }

    #[test]
    fn mixed_character_and_paragraph_properties_have_explicit_units() {
        let mut first = TextRun::new("Bold");
        first.format = CharacterFormat {
            bold: Some(true),
            italic: Some(false),
            underline: Some(Underline::Double),
            font_family: Some("Example Font".into()),
            font_size: Some(HalfPoints(35)),
            color: Some(TextColor::Rgb {
                red: 0x12,
                green: 0xAB,
                blue: 0xCD,
            }),
            hyperlink: None,
        };
        let paragraph = Paragraph::new(id("p"), vec![first, TextRun::new(" plain")]).with_format(
            ParagraphFormat {
                alignment: Some(Alignment::Justify),
                spacing_before: Some(Twips(80)),
                spacing_after: Some(Twips(120)),
                line_spacing: Some(LineSpacing::AtLeast { height: Twips(300) }),
                indentation: Some(Indentation {
                    left: Some(Twips(720)),
                    right: Some(Twips(-10)),
                    first_line: Some(Twips(-360)),
                }),
                page_break_before: Some(true),
            },
        );
        let xml = part(
            &export_document(&document(vec![paragraph])).unwrap(),
            "word/document.xml",
        );
        assert!(xml.contains("w:line=\"300\" w:lineRule=\"atLeast\""));
        assert!(xml.contains("w:hanging=\"360\""));
        assert!(xml.contains("w:pageBreakBefore w:val=\"1\""));
        assert!(xml.contains("w:jc w:val=\"both\""));
        assert!(xml.contains("w:color w:val=\"12ABCD\""));
        for (name, value) in [("b", "1"), ("bCs", "1"), ("i", "0"), ("iCs", "0")] {
            assert!(xml.contains(&format!("w:{name} w:val=\"{value}\"")));
        }
        assert!(xml.contains("w:sz w:val=\"35\""));
        assert!(xml.contains("w:szCs w:val=\"35\""));
        assert_eq!(parsed_text(&xml), "Bold plain");
    }

    #[test]
    fn exact_and_multiple_spacing_keep_distinct_ooxml_rules() {
        for (line_spacing, height, rule) in [
            (LineSpacing::Multiple { value: 276 }, "276", "auto"),
            (LineSpacing::Exact { height: Twips(240) }, "240", "exact"),
        ] {
            let paragraph = Paragraph::new(id("p"), vec![]).with_format(ParagraphFormat {
                line_spacing: Some(line_spacing),
                ..Default::default()
            });
            let xml = part(
                &export_document(&document(vec![paragraph])).unwrap(),
                "word/document.xml",
            );
            let parsed = roxmltree::Document::parse(&xml).unwrap();
            let spacing = parsed
                .descendants()
                .find(|node| node.has_tag_name((W, "spacing")))
                .unwrap();
            assert_eq!(spacing.attribute((W, "line")), Some(height));
            assert_eq!(spacing.attribute((W, "lineRule")), Some(rule));
        }
    }

    #[test]
    fn style_absolute_bold_and_italic_are_translated_into_contextual_toggles() {
        let mut parent = NamedStyle::new(id("not:an xml ID"), "Parent\rName");
        parent.character_format.bold = Some(true);
        parent.character_format.italic = Some(true);
        let mut child = NamedStyle::new(id("child"), "Child");
        child.based_on = Some(parent.id.clone());
        child.character_format.bold = Some(false);
        child.character_format.italic = Some(true);
        let paragraph =
            Paragraph::new(id("p"), vec![TextRun::new("Styled")]).with_style(child.id.clone());
        let document = Document::from_parts(
            id("doc"),
            DocumentMetadata::default(),
            vec![Section::new(id("s"), vec![Block::Paragraph(paragraph)])],
            vec![parent, child],
        )
        .unwrap();
        let bytes = export_document(&document).unwrap();
        let xml = part(&bytes, "word/styles.xml");
        let parsed = roxmltree::Document::parse(&xml).unwrap();
        let styles: Vec<_> = parsed
            .descendants()
            .filter(|node| node.has_tag_name((W, "style")))
            .collect();
        assert_eq!(styles[0].attribute((W, "styleId")), Some("s0"));
        assert_eq!(styles[1].attribute((W, "styleId")), Some("s1"));
        let property = |style: usize, name| {
            styles[style]
                .descendants()
                .find(|node| node.has_tag_name((W, name)))
                .unwrap()
                .attribute((W, "val"))
        };
        assert_eq!(property(0, "b"), Some("1"));
        assert_eq!(property(1, "b"), Some("1")); // Turn inherited bold off.
        assert_eq!(property(1, "i"), Some("0")); // Keep inherited italic on.
        assert_eq!(property(1, "basedOn"), Some("s0"));
        assert_eq!(property(0, "name"), Some("Parent\rName"));
    }

    #[test]
    fn nonfinal_section_geometry_is_on_its_ending_paragraph_and_final_geometry_is_on_body() {
        let properties = SectionProperties {
            page_size: Some(PageSize {
                width: Twips(15_840),
                height: Twips(12_240),
                orientation: Some(PageOrientation::Landscape),
            }),
            page_margins: Some(PageMargins {
                top: Twips(1_440),
                right: Twips(1_440),
                bottom: Twips(1_440),
                left: Twips(1_440),
                header: Twips(720),
                footer: Twips(720),
                gutter: Twips(0),
            }),
        };
        let document = Document::from_parts(
            id("doc"),
            DocumentMetadata::default(),
            vec![
                Section::new(
                    id("s1"),
                    vec![Block::Paragraph(Paragraph::new(
                        id("p1"),
                        vec![TextRun::new("First")],
                    ))],
                )
                .with_properties(properties.clone()),
                Section::new(
                    id("s2"),
                    vec![Block::Paragraph(Paragraph::new(
                        id("p2"),
                        vec![TextRun::new("Last")],
                    ))],
                )
                .with_properties(properties),
            ],
            vec![],
        )
        .unwrap();
        let xml = part(&export_document(&document).unwrap(), "word/document.xml");
        let parsed = roxmltree::Document::parse(&xml).unwrap();
        let sections: Vec<_> = parsed
            .descendants()
            .filter(|node| node.has_tag_name((W, "sectPr")))
            .collect();
        assert_eq!(sections.len(), 2);
        assert!(sections[0].parent().unwrap().has_tag_name((W, "pPr")));
        assert!(sections[1].parent().unwrap().has_tag_name((W, "body")));
        for section in sections {
            let size = section
                .children()
                .find(|node| node.has_tag_name((W, "pgSz")))
                .unwrap();
            assert_eq!(size.attribute((W, "w")), Some("15840"));
            assert_eq!(size.attribute((W, "h")), Some("12240"));
            assert_eq!(size.attribute((W, "orient")), Some("landscape"));
        }
    }

    #[test]
    fn empty_documents_and_final_empty_sections_are_represented_without_inventing_text() {
        let empty = Document::new(id("doc")).unwrap();
        let xml = part(&export_document(&empty).unwrap(), "word/document.xml");
        let parsed = roxmltree::Document::parse(&xml).unwrap();
        assert_eq!(
            parsed
                .descendants()
                .filter(|node| node.has_tag_name((W, "p")))
                .count(),
            0
        );
        assert_eq!(
            parsed
                .descendants()
                .filter(|node| node.has_tag_name((W, "sectPr")))
                .count(),
            0
        );
        let final_empty = document(vec![]);
        let xml = part(&export_document(&final_empty).unwrap(), "word/document.xml");
        let parsed = roxmltree::Document::parse(&xml).unwrap();
        assert_eq!(
            parsed
                .descendants()
                .filter(|node| node.has_tag_name((W, "p")))
                .count(),
            0
        );
        assert_eq!(
            parsed
                .descendants()
                .filter(|node| node.has_tag_name((W, "sectPr")))
                .count(),
            1
        );
    }

    #[test]
    fn empty_nonfinal_sections_are_explicitly_unrepresentable() {
        let document = Document::from_parts(
            id("doc"),
            DocumentMetadata::default(),
            vec![
                Section::new(id("s1"), vec![]),
                Section::new(id("s2"), vec![]),
            ],
            vec![],
        )
        .unwrap();
        assert!(matches!(
            export_document(&document),
            Err(ConversionError::UnsupportedExport { .. })
        ));
    }

    #[test]
    fn illegal_xml_values_fail_without_partial_output() {
        for illegal in ['\0', '\u{8}', '\u{b}', '\u{c}', '\u{fffe}', '\u{ffff}'] {
            let document = document(vec![Paragraph::new(
                id("p"),
                vec![TextRun::new(format!("before{illegal}after"))],
            )]);
            assert!(matches!(
                export_document(&document),
                Err(ConversionError::UnsupportedExport { .. })
            ));
        }
        let mut run = TextRun::new("valid");
        run.format.font_family = Some("bad\0font".into());
        assert!(export_document(&document(vec![Paragraph::new(id("p"), vec![run])])).is_err());
    }

    #[test]
    fn literal_carriage_returns_cannot_silently_change_paragraph_structure() {
        for text in ["before\rafter", "trailing\r", "\r\n"] {
            let source = document(vec![Paragraph::new(id("p"), vec![TextRun::new(text)])]);
            assert!(matches!(
                export_document(&source),
                Err(ConversionError::UnsupportedExport { feature })
                    if feature.contains("carriage-return")
            ));
            let Block::Paragraph(paragraph) = &source.sections()[0].blocks()[0] else {
                panic!("expected paragraph")
            };
            assert_eq!(paragraph.runs()[0].text, text);
        }
        let source = Document::from_parts(
            id("d"),
            DocumentMetadata {
                title: "title\rtext".into(),
            },
            vec![],
            vec![],
        )
        .unwrap();
        assert!(matches!(
            export_document(&source),
            Err(ConversionError::UnsupportedExport { .. })
        ));
    }

    #[test]
    fn valid_extreme_integer_values_are_encoded_without_overflow() {
        let mut run = TextRun::new("x");
        run.format.font_size = Some(HalfPoints(u32::MAX));
        let paragraph = Paragraph::new(id("p"), vec![run]).with_format(ParagraphFormat {
            indentation: Some(Indentation {
                left: Some(Twips(i32::MIN)),
                right: Some(Twips(i32::MAX)),
                first_line: Some(Twips(i32::MIN)),
            }),
            ..Default::default()
        });
        let xml = part(
            &export_document(&document(vec![paragraph])).unwrap(),
            "word/document.xml",
        );
        assert!(xml.contains("w:sz w:val=\"4294967295\""));
        assert!(xml.contains("w:hanging=\"2147483648\""));
        assert!(xml.contains("w:left=\"-2147483648\""));
    }

    #[test]
    fn xml_output_and_structural_budgets_are_enforced() {
        let oversized = document(vec![Paragraph::new(
            id("p"),
            vec![TextRun::new("&".repeat(MAX_XML_BYTES))],
        )]);
        assert!(matches!(
            export_document(&oversized),
            Err(ConversionError::Package(DocxError::LimitExceeded {
                resource: "xml_bytes",
                ..
            }))
        ));
        let paragraphs = (0..10_000)
            .map(|index| Paragraph::new(id(&format!("p{index}")), vec![TextRun::new("x")]))
            .collect();
        assert!(matches!(
            export_document(&document(paragraphs)),
            Err(ConversionError::Package(DocxError::LimitExceeded {
                resource: "xml_nodes",
                ..
            }))
        ));
    }

    #[test]
    fn generated_docx_reimports_editably_with_current_supported_content_and_geometry() {
        let mut parent = NamedStyle::new(id("parent"), "Heading parent");
        parent.character_format = CharacterFormat {
            bold: Some(true),
            italic: Some(true),
            font_family: Some("Example Font".into()),
            font_size: Some(HalfPoints(32)),
            ..Default::default()
        };
        let mut child = NamedStyle::new(id("child"), "Heading child");
        child.based_on = Some(parent.id.clone());
        child.character_format.bold = Some(false);
        let mut run = TextRun::new("  Cafe\u{301}\n日本語 👩‍💻\t ");
        run.format.italic = Some(false);
        run.format.underline = Some(Underline::Single);
        run.format.color = Some(TextColor::Auto {});
        let paragraph = Paragraph::new(id("p"), vec![run.clone()])
            .with_style(child.id.clone())
            .with_format(ParagraphFormat {
                alignment: Some(Alignment::Right),
                line_spacing: Some(LineSpacing::Exact { height: Twips(280) }),
                page_break_before: Some(true),
                ..Default::default()
            });
        let properties = SectionProperties {
            page_size: Some(PageSize {
                width: Twips(11_906),
                height: Twips(16_838),
                orientation: Some(PageOrientation::Portrait),
            }),
            page_margins: Some(PageMargins {
                top: Twips(1_440),
                right: Twips(1_440),
                bottom: Twips(1_440),
                left: Twips(1_440),
                header: Twips(720),
                footer: Twips(720),
                gutter: Twips(0),
            }),
        };
        let document = Document::from_parts(
            id("doc"),
            DocumentMetadata {
                title: "Roundtrip title".into(),
            },
            vec![
                Section::new(id("s"), vec![Block::Paragraph(paragraph.clone())])
                    .with_properties(properties.clone()),
            ],
            vec![parent, child],
        )
        .unwrap();
        let bytes = export_document(&document).unwrap();
        let outcome = crate::import_docx(&bytes, id("reopened")).unwrap();
        let crate::DocxImportOutcome::Editable(imported) = outcome else {
            panic!("generated supported content must be editable: {outcome:?}");
        };
        assert_eq!(imported.original().original_bytes(), bytes);
        let restored = imported.document();
        assert_eq!(restored.metadata(), document.metadata());
        assert_eq!(restored.sections()[0].properties(), Some(&properties));
        let Block::Paragraph(restored_paragraph) = &restored.sections()[0].blocks()[0] else {
            panic!("expected paragraph")
        };
        assert_eq!(restored_paragraph.runs(), &[run]);
        assert_eq!(restored_paragraph.format(), paragraph.format());
        assert_eq!(restored.styles().len(), 2);
        assert_eq!(restored.styles()[0].name, "Heading parent");
        assert_eq!(restored.styles()[1].name, "Heading child");
        assert_eq!(
            restored.styles()[1].based_on.as_ref(),
            Some(&restored.styles()[0].id)
        );
        assert_eq!(restored.styles()[0].character_format.bold, Some(true));
        assert_eq!(restored.styles()[1].character_format.bold, Some(false));
        assert_eq!(restored_paragraph.style(), Some(&restored.styles()[1].id));
    }

    #[test]
    fn supported_independent_fixture_survives_export_without_style_growth() {
        let source = include_bytes!("../../../tests/fixtures/docx/supported-formatting.docx");
        let crate::DocxImportOutcome::Editable(first) =
            crate::import_docx(source, id("first")).unwrap()
        else {
            panic!("positive fixture must be editable");
        };
        let first_bytes = export_document(first.document()).unwrap();
        let crate::DocxImportOutcome::Editable(second) =
            crate::import_docx(&first_bytes, id("second")).unwrap()
        else {
            panic!("supported export must remain editable");
        };
        let second_bytes = export_document(second.document()).unwrap();
        assert_eq!(first_bytes, second_bytes);
        assert_eq!(first.document().metadata(), second.document().metadata());
        assert_eq!(
            first.document().sections().len(),
            second.document().sections().len()
        );
        assert_eq!(
            first.document().styles().len(),
            second.document().styles().len()
        );
        for (first, second) in first
            .document()
            .sections()
            .iter()
            .zip(second.document().sections())
        {
            assert_eq!(first.properties(), second.properties());
            assert_eq!(first.blocks().len(), second.blocks().len());
            for (first_block, second_block) in first.blocks().iter().zip(second.blocks()) {
                let (Block::Paragraph(first), Block::Paragraph(second)) =
                    (first_block, second_block)
                else {
                    panic!("expected paragraphs")
                };
                assert_eq!(first.runs(), second.runs());
                assert_eq!(first.format(), second.format());
            }
        }
    }
}
