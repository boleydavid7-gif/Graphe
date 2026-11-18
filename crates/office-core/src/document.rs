use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Deserializer, Serialize};

use crate::formatting::{PageMargins, PageSize};
use crate::{CharacterFormat, DocumentError, ParagraphFormat, ValidationError};
use crate::{
    Image, MAX_DOCUMENT_IMAGE_BYTES, MAX_DOCUMENT_IMAGE_PIXELS, MAX_DOCUMENT_IMAGES, Twips,
};

#[path = "legacy.rs"]
mod legacy;
#[path = "v2.rs"]
mod v2;
#[path = "v3.rs"]
mod v3;
#[path = "v4.rs"]
mod v4;
#[path = "v5.rs"]
mod v5;
#[path = "v6.rs"]
mod v6;

/// Current document JSON schema, independent of crate and native-file versions.
///
/// Schemas 1 through 6 migrate strictly into the pagination-aware schema 7.
pub const SCHEMA_VERSION: u32 = 7;

/// An opaque, caller-assigned identifier preserved across copies and serialization.
///
/// IDs are case-sensitive, nonempty, and must not have surrounding whitespace.
/// Constructors do not generate IDs; the application must allocate stable unique
/// values. Document validation checks uniqueness across all entity kinds.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Id(String);

impl Id {
    /// Creates an identifier, rejecting empty or whitespace-padded values.
    pub fn new(value: impl Into<String>) -> Result<Self, ValidationError> {
        let id = Self(value.into());
        id.validate("id")?;
        Ok(id)
    }

    /// Returns the identifier's exact string representation.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn validate(&self, path: &str) -> Result<(), ValidationError> {
        if self.0.is_empty() || self.0.trim() != self.0 {
            return Err(ValidationError::InvalidId { path: path.into() });
        }
        Ok(())
    }
}

/// Descriptive metadata. Text is preserved exactly without Unicode normalization.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentMetadata {
    /// The document title; an empty title represents an untitled document.
    pub title: String,
}

/// Optional format-neutral physical page properties.
///
/// An absent record and a present record with both fields unspecified remain
/// distinct. Geometry is never inferred, rotated, clamped, or normalized.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SectionProperties {
    /// Explicit physical page dimensions and optional printing orientation.
    pub page_size: Option<PageSize>,
    /// Explicit body margins, header/footer offsets and binding margin.
    pub page_margins: Option<PageMargins>,
}

impl SectionProperties {
    fn validate(&self, path: &str) -> Result<(), ValidationError> {
        if let Some(size) = self.page_size {
            size.validate(&format!("{path}.page_size"))?;
        }
        if let Some(margins) = self.page_margins {
            margins.validate(&format!("{path}.page_margins"))?;
        }
        if let (Some(size), Some(margins)) = (self.page_size, self.page_margins) {
            // i64 arithmetic covers the sum of every valid i32 twip field.
            let horizontal = i64::from(margins.left.0)
                + i64::from(margins.right.0)
                + i64::from(margins.gutter.0);
            let vertical = i64::from(margins.top.0) + i64::from(margins.bottom.0);
            if horizontal >= i64::from(size.width.0) || vertical >= i64::from(size.height.0) {
                return Err(ValidationError::InvalidFormatting {
                    path: path.to_owned(),
                    reason: "page dimensions and margins must leave a positive body area".into(),
                });
            }
        }
        Ok(())
    }
}

/// A UTF-8 text fragment with direct character formatting.
///
/// Runs may be empty and are never merged or normalized by model validation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextRun {
    /// UTF-8 text; byte indices are not cursor positions.
    pub text: String,
    /// Direct properties, with unspecified values eligible for inheritance.
    pub format: CharacterFormat,
}

impl TextRun {
    /// Constructs an unformatted run, preserving the supplied string exactly.
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            format: CharacterFormat::default(),
        }
    }
}

/// A one-level list classification stored independently from editable text.
/// Adjacent numbered paragraphs form a sequential group; unrelated paragraphs
/// restart numbering. Future nested lists require a separately versioned model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListKind {
    /// A bulleted paragraph.
    Bullet,
    /// A decimal-numbered paragraph.
    Numbered,
}

/// A paragraph containing an ordered sequence of text runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Paragraph {
    id: Id,
    runs: Vec<TextRun>,
    style: Option<Id>,
    format: ParagraphFormat,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    list: Option<ListKind>,
}

impl Paragraph {
    /// Assembles a paragraph. An empty run sequence is a valid empty paragraph.
    #[must_use]
    pub fn new(id: Id, runs: Vec<TextRun>) -> Self {
        Self {
            id,
            runs,
            style: None,
            format: ParagraphFormat::default(),
            list: None,
        }
    }

    /// Sets a named paragraph-style reference during construction.
    #[must_use]
    pub fn with_style(mut self, style: Id) -> Self {
        self.style = Some(style);
        self
    }

    /// Sets direct paragraph properties during construction.
    #[must_use]
    pub fn with_format(mut self, format: ParagraphFormat) -> Self {
        self.format = format;
        self
    }

    /// Sets or clears the paragraph's semantic list membership.
    #[must_use]
    pub fn with_list(mut self, list: Option<ListKind>) -> Self {
        self.list = list;
        self
    }

    /// Returns the semantic list classification, if any.
    #[must_use]
    pub fn list(&self) -> Option<ListKind> {
        self.list
    }

    /// Returns the stable paragraph identifier.
    #[must_use]
    pub fn id(&self) -> &Id {
        &self.id
    }

    /// Returns text runs in document order.
    #[must_use]
    pub fn runs(&self) -> &[TextRun] {
        &self.runs
    }

    /// Returns the optional named style reference.
    #[must_use]
    pub fn style(&self) -> Option<&Id> {
        self.style.as_ref()
    }

    /// Returns the paragraph's direct formatting properties.
    #[must_use]
    pub fn format(&self) -> &ParagraphFormat {
        &self.format
    }
}

/// Maximum number of rows in the initial rectangular-table profile.
pub const MAX_TABLE_ROWS: usize = 1_000;
/// Maximum number of columns in the initial rectangular-table profile.
pub const MAX_TABLE_COLUMNS: usize = 32;
/// Maximum total cells in one table, bounding model traversal and layout work.
pub const MAX_TABLE_CELLS: usize = 4_096;

/// A format-neutral rectangular table. Cells contain paragraphs, never nested tables.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Table {
    id: Id,
    rows: Vec<TableRow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    column_widths: Option<Vec<Twips>>,
}

impl Table {
    /// Assembles a table; document validation checks its dimensions and identities.
    #[must_use]
    pub fn new(id: Id, rows: Vec<TableRow>) -> Self {
        Self {
            id,
            rows,
            column_widths: None,
        }
    }
    /// Specifies positive physical column widths in twips, one per column.
    #[must_use]
    pub fn with_column_widths(mut self, widths: Vec<Twips>) -> Self {
        self.column_widths = Some(widths);
        self
    }
    /// Returns this table's stable identity.
    #[must_use]
    pub fn id(&self) -> &Id {
        &self.id
    }
    /// Returns rows in top-to-bottom logical order.
    #[must_use]
    pub fn rows(&self) -> &[TableRow] {
        &self.rows
    }
    /// Returns optional explicit column widths; absent widths divide usable space equally.
    #[must_use]
    pub fn column_widths(&self) -> Option<&[Twips]> {
        self.column_widths.as_deref()
    }
}

/// A table row with one cell for each table column.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TableRow {
    id: Id,
    cells: Vec<TableCell>,
}
impl TableRow {
    /// Assembles a row, retaining stable identity and logical cell order.
    #[must_use]
    pub fn new(id: Id, cells: Vec<TableCell>) -> Self {
        Self { id, cells }
    }
    /// Returns this row's stable identity.
    #[must_use]
    pub fn id(&self) -> &Id {
        &self.id
    }
    /// Returns cells in logical left-to-right column order.
    #[must_use]
    pub fn cells(&self) -> &[TableCell] {
        &self.cells
    }
}

/// A nonempty paragraph flow inside a table cell, with its own stable identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TableCell {
    id: Id,
    paragraphs: Vec<Paragraph>,
}
impl TableCell {
    /// Assembles a cell. Every valid cell contains at least one paragraph.
    #[must_use]
    pub fn new(id: Id, paragraphs: Vec<Paragraph>) -> Self {
        Self { id, paragraphs }
    }
    /// Returns this cell's stable identity.
    #[must_use]
    pub fn id(&self) -> &Id {
        &self.id
    }
    /// Returns the cell's paragraph flow in document order.
    #[must_use]
    pub fn paragraphs(&self) -> &[Paragraph] {
        &self.paragraphs
    }
}

/// Typed block content. Additional kinds require an explicit schema evolution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "content",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Block {
    /// A text paragraph.
    Paragraph(Paragraph),
    /// A rectangular table containing independent cell paragraph flows.
    Table(Table),
    /// A validated static raster image in the body flow.
    Image(Image),
}

impl Block {
    /// Visits paragraphs in logical row-major order, retaining cell boundaries in the model.
    pub fn paragraphs(&self) -> impl Iterator<Item = &Paragraph> {
        let (paragraph, table) = match self {
            Self::Paragraph(paragraph) => (Some(paragraph), None),
            Self::Table(table) => (None, Some(table)),
            Self::Image(_) => (None, None),
        };
        paragraph
            .into_iter()
            .chain(table.into_iter().flat_map(|table| {
                table
                    .rows
                    .iter()
                    .flat_map(|row| row.cells.iter())
                    .flat_map(|cell| cell.paragraphs.iter())
            }))
    }
}

/// An ordered section of document content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Section {
    id: Id,
    blocks: Vec<Block>,
    properties: Option<SectionProperties>,
}

impl Section {
    /// Assembles a section. An empty block sequence is valid.
    #[must_use]
    pub fn new(id: Id, blocks: Vec<Block>) -> Self {
        Self {
            id,
            blocks,
            properties: None,
        }
    }

    /// Supplies the reserved properties record during construction.
    #[must_use]
    pub fn with_properties(mut self, properties: SectionProperties) -> Self {
        self.properties = Some(properties);
        self
    }

    /// Returns the stable section identifier.
    #[must_use]
    pub fn id(&self) -> &Id {
        &self.id
    }

    /// Returns blocks in document order.
    #[must_use]
    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    /// Visits body and table-cell paragraphs in logical document order.
    pub fn paragraphs(&self) -> impl Iterator<Item = &Paragraph> {
        self.blocks.iter().flat_map(Block::paragraphs)
    }

    /// Returns the optional section properties record.
    #[must_use]
    pub fn properties(&self) -> Option<&SectionProperties> {
        self.properties.as_ref()
    }
}

/// A named paragraph style with optional inheritance and run defaults.
///
/// The model stores and validates inheritance but does not yet compute effective
/// properties. Direct formatting will override the nearest specified style value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NamedStyle {
    /// Stable identifier used in style references (not the display name).
    pub id: Id,
    /// Nonblank, unique, case-sensitive display name.
    pub name: String,
    /// Optional parent paragraph style; cycles are invalid.
    pub based_on: Option<Id>,
    /// Paragraph properties specified by this style.
    pub paragraph_format: ParagraphFormat,
    /// Default character properties specified by this style.
    pub character_format: CharacterFormat,
}

impl NamedStyle {
    /// Constructs a style with unspecified formatting and no parent.
    #[must_use]
    pub fn new(id: Id, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            based_on: None,
            paragraph_format: ParagraphFormat::default(),
            character_format: CharacterFormat::default(),
        }
    }
}

/// A validated structured document, independent of editing, layout, and rendering.
///
/// An empty document has no sections. Empty sections, paragraphs, and text runs
/// are valid; no layout content is synthesized. Construction and JSON loading
/// validate all references and properties. Inspection exposes no mutable access.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Document {
    schema_version: u32,
    id: Id,
    metadata: DocumentMetadata,
    sections: Vec<Section>,
    styles: Vec<NamedStyle>,
}

// Separate input record keeps schema version and model invariants out of the
// public construction API. Every path producing Document validates this record.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DocumentInput {
    schema_version: u64,
    id: Id,
    metadata: DocumentMetadata,
    sections: Vec<Section>,
    styles: Vec<NamedStyle>,
}

impl DocumentInput {
    fn into_document(self) -> Result<Document, DocumentError> {
        check_schema(self.schema_version)?;
        Document::from_parts(self.id, self.metadata, self.sections, self.styles)
            .map_err(DocumentError::from)
    }
}

impl<'de> Deserialize<'de> for Document {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Preserve the JSON object text: a Value intermediate would erase
        // duplicate fields before the strict schema-specific decoder sees them.
        let raw = Box::<serde_json::value::RawValue>::deserialize(deserializer)?;
        Self::from_json(raw.get()).map_err(serde::de::Error::custom)
    }
}

fn check_schema(found: u64) -> Result<(), DocumentError> {
    if !matches!(found, 1..=7) {
        return Err(DocumentError::UnsupportedSchemaVersion {
            found,
            supported: SCHEMA_VERSION,
        });
    }
    Ok(())
}

fn register_id<'a>(
    ids: &mut HashSet<&'a str>,
    id: &'a Id,
    path: &str,
) -> Result<(), ValidationError> {
    id.validate(path)?;
    if !ids.insert(id.as_str()) {
        return Err(ValidationError::DuplicateId {
            id: id.as_str().into(),
        });
    }
    Ok(())
}

impl Document {
    /// Creates an empty document with the current schema and default metadata.
    ///
    /// Validation is fallible because IDs deserialized independently can be invalid.
    pub fn new(id: Id) -> Result<Self, ValidationError> {
        Self::from_parts(id, DocumentMetadata::default(), Vec::new(), Vec::new())
    }

    /// Assembles and validates a document from complete structural parts.
    pub fn from_parts(
        id: Id,
        metadata: DocumentMetadata,
        sections: Vec<Section>,
        styles: Vec<NamedStyle>,
    ) -> Result<Self, ValidationError> {
        let document = Self {
            schema_version: SCHEMA_VERSION,
            id,
            metadata,
            sections,
            styles,
        };
        document.validate()?;
        Ok(document)
    }

    /// Returns the stable document identifier.
    #[must_use]
    pub fn id(&self) -> &Id {
        &self.id
    }

    /// Returns the document model schema version, independent of engine version.
    #[must_use]
    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }

    /// Returns document metadata.
    #[must_use]
    pub fn metadata(&self) -> &DocumentMetadata {
        &self.metadata
    }

    /// Returns sections in document order.
    #[must_use]
    pub fn sections(&self) -> &[Section] {
        &self.sections
    }

    /// Visits all paragraphs, including table cells, in logical row-major order.
    pub fn paragraphs(&self) -> impl Iterator<Item = &Paragraph> {
        self.sections.iter().flat_map(Section::paragraphs)
    }

    /// Finds the table and zero-based row/column containing a paragraph.
    #[must_use]
    pub fn table_for_paragraph(&self, paragraph_id: &Id) -> Option<(&Table, usize, usize)> {
        self.sections
            .iter()
            .flat_map(|section| section.blocks.iter())
            .find_map(|block| {
                let Block::Table(table) = block else {
                    return None;
                };
                table.rows.iter().enumerate().find_map(|(row_index, row)| {
                    row.cells.iter().enumerate().find_map(|(column, cell)| {
                        cell.paragraphs
                            .iter()
                            .any(|paragraph| paragraph.id() == paragraph_id)
                            .then_some((table, row_index, column))
                    })
                })
            })
    }

    /// Visits body image blocks in document order, without decoding their cached assets.
    pub fn images(&self) -> impl Iterator<Item = &Image> {
        self.sections
            .iter()
            .flat_map(|section| section.blocks.iter())
            .filter_map(|block| match block {
                Block::Image(image) => Some(image),
                _ => None,
            })
    }

    /// Returns style definitions in their preserved input order.
    #[must_use]
    pub fn styles(&self) -> &[NamedStyle] {
        &self.styles
    }

    /// Validates IDs, style references and inheritance, and all formatting values.
    ///
    /// Validation never repairs or normalizes content. It returns the first
    /// error encountered in structural traversal order, then style-graph order.
    pub fn validate(&self) -> Result<(), ValidationError> {
        let mut image_count = 0_usize;
        let mut image_bytes = 0_usize;
        let mut image_pixels = 0_u64;
        let mut ids = HashSet::new();
        register_id(&mut ids, &self.id, "id")?;
        let mut names = HashSet::new();
        let mut styles = HashMap::new();
        for (index, style) in self.styles.iter().enumerate() {
            let path = format!("styles[{index}]");
            register_id(&mut ids, &style.id, &format!("{path}.id"))?;
            if style.name.trim().is_empty() || !names.insert(style.name.as_str()) {
                return Err(ValidationError::InvalidStyleName {
                    path: format!("{path}.name"),
                });
            }
            style
                .character_format
                .validate(&format!("{path}.character_format"))?;
            style
                .paragraph_format
                .validate(&format!("{path}.paragraph_format"))?;
            if style.character_format.hyperlink.is_some() {
                return Err(ValidationError::InvalidFormatting {
                    path: format!("{path}.character_format.hyperlink"),
                    reason: "hyperlinks belong to text runs, not named style defaults".into(),
                });
            }
            styles.insert(style.id.as_str(), style);
        }
        let resolve = |id: &Id, path: &str| -> Result<(), ValidationError> {
            id.validate(path)?;
            if !styles.contains_key(id.as_str()) {
                return Err(ValidationError::MissingStyle {
                    path: path.into(),
                    id: id.as_str().into(),
                });
            }
            Ok(())
        };
        for (section_index, section) in self.sections.iter().enumerate() {
            let path = format!("sections[{section_index}]");
            register_id(&mut ids, &section.id, &format!("{path}.id"))?;
            if let Some(properties) = &section.properties {
                properties.validate(&format!("{path}.properties"))?;
            }
            for (block_index, block) in section.blocks.iter().enumerate() {
                let path = format!("{path}.blocks[{block_index}].content");
                if let Block::Image(image) = block {
                    register_id(&mut ids, image.id(), &format!("{path}.id"))?;
                    image.validate(&path)?;
                    image_count += 1;
                    image_bytes = image_bytes.saturating_add(image.asset().original_bytes().len());
                    image_pixels = image_pixels.saturating_add(
                        u64::from(image.asset().width_px()) * u64::from(image.asset().height_px()),
                    );
                    if image_count > MAX_DOCUMENT_IMAGES
                        || image_bytes > MAX_DOCUMENT_IMAGE_BYTES
                        || image_pixels > MAX_DOCUMENT_IMAGE_PIXELS
                    {
                        return Err(ValidationError::InvalidFormatting {
                            path: path.clone(),
                            reason: "document image count, byte or decoded pixel budget exceeded"
                                .into(),
                        });
                    }
                }
                if let Block::Table(table) = block {
                    register_id(&mut ids, &table.id, &format!("{path}.id"))?;
                    let columns = table.rows.first().map_or(0, |row| row.cells.len());
                    if table.rows.is_empty()
                        || table.rows.len() > MAX_TABLE_ROWS
                        || columns == 0
                        || columns > MAX_TABLE_COLUMNS
                        || table.rows.len().saturating_mul(columns) > MAX_TABLE_CELLS
                    {
                        return Err(ValidationError::InvalidFormatting {
                            path: path.clone(),
                            reason: "table dimensions exceed the nonempty rectangular profile"
                                .into(),
                        });
                    }
                    if let Some(widths) = &table.column_widths
                        && (widths.len() != columns
                            || widths.iter().any(|width| width.0 <= 0)
                            || widths.iter().map(|width| i64::from(width.0)).sum::<i64>()
                                > i64::from(i32::MAX))
                    {
                        return Err(ValidationError::InvalidFormatting {
                                path: format!("{path}.column_widths"),
                                reason: "column widths must be positive and match the column count within the twip range".into(),
                            });
                    }
                    for (row_index, row) in table.rows.iter().enumerate() {
                        let row_path = format!("{path}.rows[{row_index}]");
                        register_id(&mut ids, &row.id, &format!("{row_path}.id"))?;
                        if row.cells.len() != columns {
                            return Err(ValidationError::InvalidFormatting {
                                path: row_path,
                                reason: "all table rows must have the same number of cells".into(),
                            });
                        }
                        for (column, cell) in row.cells.iter().enumerate() {
                            let cell_path = format!("{row_path}.cells[{column}]");
                            register_id(&mut ids, &cell.id, &format!("{cell_path}.id"))?;
                            if cell.paragraphs.is_empty() {
                                return Err(ValidationError::InvalidFormatting {
                                    path: cell_path,
                                    reason: "a table cell must contain at least one paragraph"
                                        .into(),
                                });
                            }
                        }
                    }
                }
                for (paragraph_index, paragraph) in block.paragraphs().enumerate() {
                    let paragraph_path = format!("{path}.paragraphs[{paragraph_index}]");
                    register_id(&mut ids, &paragraph.id, &format!("{paragraph_path}.id"))?;
                    if let Some(style) = &paragraph.style {
                        resolve(style, &format!("{paragraph_path}.style"))?;
                    }
                    paragraph
                        .format
                        .validate(&format!("{paragraph_path}.format"))?;
                    for (run_index, run) in paragraph.runs.iter().enumerate() {
                        run.format
                            .validate(&format!("{paragraph_path}.runs[{run_index}].format"))?;
                    }
                }
            }
        }
        for (index, style) in self.styles.iter().enumerate() {
            if let Some(parent) = &style.based_on {
                resolve(parent, &format!("styles[{index}].based_on"))?;
            }
        }
        // Iterative graph walk avoids recursion and visits each style once.
        let mut complete = HashSet::new();
        for style in &self.styles {
            let mut path = HashSet::new();
            let mut current = Some(style);
            while let Some(style) = current {
                let id = style.id.as_str();
                if complete.contains(id) {
                    break;
                }
                if !path.insert(id) {
                    return Err(ValidationError::StyleCycle { id: id.into() });
                }
                current = style
                    .based_on
                    .as_ref()
                    .map(|parent| styles[parent.as_str()]);
            }
            complete.extend(path);
        }
        Ok(())
    }

    /// Serializes a validated document to pretty-printed schema-versioned JSON.
    pub fn to_json(&self) -> Result<String, DocumentError> {
        self.validate()?;
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Parses JSON and validates it without silently dropping unknown fields.
    ///
    /// The version is checked before decoding model fields, so a future schema
    /// with a different payload still returns an explicit unsupported-version
    /// error. Schemas 1–6 are decoded using exact former field definitions and
    /// migrated to the current schema with new properties unspecified. Missing fields,
    /// fields introduced only in newer schemas, and duplicate keys are rejected.
    pub fn from_json(json: &str) -> Result<Self, DocumentError> {
        #[derive(Deserialize)]
        struct Header {
            schema_version: u64,
        }
        let header: Header = serde_json::from_str(json)?;
        check_schema(header.schema_version)?;
        match header.schema_version {
            1 => legacy::from_json(json),
            2 => v2::from_json(json),
            3 => v3::from_json(json),
            4 => v4::from_json(json),
            5 => v5::from_json(json),
            6 => v6::from_json(json),
            _ => {
                crate::image::preflight_document(json)?;
                serde_json::from_str::<DocumentInput>(json)?.into_document()
            }
        }
    }
}
