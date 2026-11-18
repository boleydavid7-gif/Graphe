//! Strict pre-pagination schema-6 model. Never allow new properties under an old label.
use super::{
    Block, Document, DocumentMetadata, Id, ListKind, NamedStyle, Paragraph, Section,
    SectionProperties, Table, TableCell, TableRow, TextRun,
};
use crate::{
    Alignment, CharacterFormat, DocumentError, Image, Indentation, LineSpacing, ParagraphFormat,
    Twips,
};
use serde::Deserialize;

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct V6ParagraphFormat {
    alignment: Option<Alignment>,
    spacing_before: Option<Twips>,
    spacing_after: Option<Twips>,
    line_spacing: Option<LineSpacing>,
    indentation: Option<Indentation>,
    page_break_before: Option<bool>,
}
impl From<V6ParagraphFormat> for ParagraphFormat {
    fn from(old: V6ParagraphFormat) -> Self {
        Self {
            alignment: old.alignment,
            spacing_before: old.spacing_before,
            spacing_after: old.spacing_after,
            line_spacing: old.line_spacing,
            indentation: old.indentation,
            page_break_before: old.page_break_before,
            keep_together: None,
            keep_with_next: None,
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct V6Paragraph {
    id: Id,
    runs: Vec<TextRun>,
    style: Option<Id>,
    format: V6ParagraphFormat,
    #[serde(default)]
    list: Option<ListKind>,
}
impl From<V6Paragraph> for Paragraph {
    fn from(old: V6Paragraph) -> Self {
        let mut paragraph = Paragraph::new(old.id, old.runs)
            .with_format(old.format.into())
            .with_list(old.list);
        if let Some(style) = old.style {
            paragraph = paragraph.with_style(style);
        }
        paragraph
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct V6NamedStyle {
    id: Id,
    name: String,
    based_on: Option<Id>,
    paragraph_format: V6ParagraphFormat,
    character_format: CharacterFormat,
}
impl From<V6NamedStyle> for NamedStyle {
    fn from(old: V6NamedStyle) -> Self {
        Self {
            id: old.id,
            name: old.name,
            based_on: old.based_on,
            paragraph_format: old.paragraph_format.into(),
            character_format: old.character_format,
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct V6TableCell {
    id: Id,
    paragraphs: Vec<V6Paragraph>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct V6TableRow {
    id: Id,
    cells: Vec<V6TableCell>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct V6Table {
    id: Id,
    rows: Vec<V6TableRow>,
    #[serde(default)]
    column_widths: Option<Vec<Twips>>,
}
impl From<V6Table> for Table {
    fn from(old: V6Table) -> Self {
        let rows = old
            .rows
            .into_iter()
            .map(|row| {
                TableRow::new(
                    row.id,
                    row.cells
                        .into_iter()
                        .map(|cell| {
                            TableCell::new(
                                cell.id,
                                cell.paragraphs.into_iter().map(Into::into).collect(),
                            )
                        })
                        .collect(),
                )
            })
            .collect();
        let result = Table::new(old.id, rows);
        match old.column_widths {
            Some(widths) => result.with_column_widths(widths),
            None => result,
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct V6Document {
    schema_version: u64,
    id: Id,
    metadata: DocumentMetadata,
    sections: Vec<V6Section>,
    styles: Vec<V6NamedStyle>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct V6Section {
    id: Id,
    blocks: Vec<V6Block>,
    properties: Option<SectionProperties>,
}
#[derive(Deserialize)]
#[serde(
    tag = "kind",
    content = "content",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum V6Block {
    Paragraph(V6Paragraph),
    Table(V6Table),
    Image(Image),
}

pub(super) fn from_json(json: &str) -> Result<Document, DocumentError> {
    crate::image::preflight_document(json)?;
    let old: V6Document = serde_json::from_str(json)?;
    debug_assert_eq!(old.schema_version, 6);
    let sections = old
        .sections
        .into_iter()
        .map(|section| {
            let blocks = section
                .blocks
                .into_iter()
                .map(|block| match block {
                    V6Block::Paragraph(paragraph) => Block::Paragraph(paragraph.into()),
                    V6Block::Table(table) => Block::Table(table.into()),
                    V6Block::Image(image) => Block::Image(image),
                })
                .collect();
            let current = Section::new(section.id, blocks);
            match section.properties {
                Some(properties) => current.with_properties(properties),
                None => current,
            }
        })
        .collect();
    Document::from_parts(
        old.id,
        old.metadata,
        sections,
        old.styles.into_iter().map(Into::into).collect(),
    )
    .map_err(Into::into)
}
