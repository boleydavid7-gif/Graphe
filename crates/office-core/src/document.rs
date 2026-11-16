use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Deserializer, Serialize};

use crate::formatting::{PageMargins, PageSize};
use crate::{CharacterFormat, DocumentError, ParagraphFormat, ValidationError};

#[path = "legacy.rs"]
mod legacy;
#[path = "v2.rs"]
mod v2;
#[path = "v3.rs"]
mod v3;
#[path = "v4.rs"]
mod v4;

/// Current document JSON schema, independent of crate and native-file versions.
///
/// Schemas 1–4 migrate strictly into schema 5 with semantic table cells.
pub const SCHEMA_VERSION: u32 = 5;

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

/// A cell in a rectangular, single-paragraph-per-cell semantic table.
/// All consecutive cells with the same table ID are one table, in row-major
/// order. No merged cells, nested tables, or hidden HTML document are stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TableCell {
    /// Stable identifier for the table as a whole.
    pub table_id: Id,
    /// Zero-based row index.
    pub row: u16,
    /// Zero-based column index.
    pub column: u8,
    /// Total number of columns; identical for every cell in the table.
    pub columns: u8,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    table: Option<TableCell>,
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
            table: None,
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

    /// Assigns or removes the semantic table location of this paragraph.
    #[must_use]
    pub fn with_table(mut self, table: Option<TableCell>) -> Self {
        self.table = table;
        self
    }

    /// Returns the table cell location, if this paragraph belongs to a table.
    #[must_use]
    pub fn table(&self) -> Option<&TableCell> {
        self.table.as_ref()
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
    if !matches!(found, 1 | 2 | 3 | 4 | 5) {
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
            // Consecutive table cells must form a complete rectangular matrix.
            // This preserves text editing's paragraph-index coordinate model
            // without letting a malformed table corrupt layout or DOCX output.
            let mut table_state: Option<(Id, u16, u8, u8)> = None;
            let path = format!("sections[{section_index}]");
            register_id(&mut ids, &section.id, &format!("{path}.id"))?;
            if let Some(properties) = &section.properties {
                properties.validate(&format!("{path}.properties"))?;
            }
            for (block_index, block) in section.blocks.iter().enumerate() {
                let Block::Paragraph(paragraph) = block;
                let path = format!("{path}.blocks[{block_index}].content");
                register_id(&mut ids, &paragraph.id, &format!("{path}.id"))?;
                if let Some(table) = &paragraph.table {
                    if table.columns == 0 || table.columns > 8
                        || table.column >= table.columns || table.row >= 32
                        || paragraph.list.is_some()
                    {
                        return Err(ValidationError::InvalidFormatting {
                            path: format!("{path}.table"),
                            reason: "tables require 1–8 columns, at most 32 rows, and no list markers".into(),
                        });
                    }
                    let expected = match &table_state {
                        Some((id, row, column, columns)) if id == &table.table_id => {
                            if *column + 1 == *columns {
                                (row + 1, 0, *columns)
                            } else {
                                (*row, column + 1, *columns)
                            }
                        }
                        Some((_, _, previous_column, columns)) if *previous_column + 1 != *columns => {
                            return Err(ValidationError::InvalidFormatting {
                                path: format!("{path}.table"),
                                reason: "previous table ended with an incomplete row".into(),
                            });
                        }
                        _ => {
                            register_id(&mut ids, &table.table_id, &format!("{path}.table.table_id"))?;
                            (0, 0, table.columns)
                        }
                    };
                    if (table.row, table.column, table.columns) != expected {
                        return Err(ValidationError::InvalidFormatting {
                            path: format!("{path}.table"),
                            reason: "table cells must be consecutive, row-major, and rectangular".into(),
                        });
                    }
                    table_state = Some((table.table_id.clone(), table.row, table.column, table.columns));
                } else {
                    if let Some((_, _, column, columns)) = &table_state
                        && column + 1 != *columns
                    {
                        return Err(ValidationError::InvalidFormatting {
                            path: format!("{path}.table"),
                            reason: "table row cannot be incomplete".into(),
                        });
                    }
                    table_state = None;
                }
                if let Some(style) = &paragraph.style {
                    resolve(style, &format!("{path}.style"))?;
                }
                paragraph.format.validate(&format!("{path}.format"))?;
                for (run_index, run) in paragraph.runs.iter().enumerate() {
                    run.format
                        .validate(&format!("{path}.runs[{run_index}].format"))?;
                }
            }
            if let Some((_, _, column, columns)) = table_state
                && column + 1 != columns
            {
                return Err(ValidationError::InvalidFormatting {
                    path: format!("{path}.blocks"),
                    reason: "table must end with a complete row".into(),
                });
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
    /// error. Schema 1 is decoded using its exact former field definitions and
    /// migrated to schema 2 with new properties unspecified. Missing fields,
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
            _ => serde_json::from_str::<DocumentInput>(json)?.into_document(),
        }
    }
}
