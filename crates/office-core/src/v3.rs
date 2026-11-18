//! Strict schema-3 migration. Older files cannot contain hyperlink metadata
//! even if a future schema adds more optional character properties.

use super::{
    Block, Document, DocumentMetadata, Id, ListKind, NamedStyle, Paragraph, Section,
    SectionProperties, TextRun,
};
use crate::{CharacterFormat, DocumentError, HalfPoints, TextColor, Underline};
use super::v6::V6ParagraphFormat;
use serde::Deserialize;

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct V3CharacterFormat {
    bold: Option<bool>,
    italic: Option<bool>,
    underline: Option<Underline>,
    font_family: Option<String>,
    font_size: Option<HalfPoints>,
    color: Option<TextColor>,
}

impl From<V3CharacterFormat> for CharacterFormat {
    fn from(old: V3CharacterFormat) -> Self {
        CharacterFormat {
            bold: old.bold,
            italic: old.italic,
            underline: old.underline,
            font_family: old.font_family,
            font_size: old.font_size,
            color: old.color,
            hyperlink: None,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct V3TextRun {
    text: String,
    format: V3CharacterFormat,
}

impl From<V3TextRun> for TextRun {
    fn from(old: V3TextRun) -> Self {
        Self {
            text: old.text,
            format: old.format.into(),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct V3NamedStyle {
    id: Id,
    name: String,
    based_on: Option<Id>,
    paragraph_format: V6ParagraphFormat,
    character_format: V3CharacterFormat,
}

impl From<V3NamedStyle> for NamedStyle {
    fn from(old: V3NamedStyle) -> Self {
        NamedStyle {
            id: old.id,
            name: old.name,
            based_on: old.based_on,
            paragraph_format: old.paragraph_format.into(),
            character_format: old.character_format.into(),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct V3Document {
    schema_version: u64,
    id: Id,
    metadata: DocumentMetadata,
    sections: Vec<V3Section>,
    styles: Vec<V3NamedStyle>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct V3Section {
    id: Id,
    blocks: Vec<V3Block>,
    properties: Option<SectionProperties>,
}

#[derive(Deserialize)]
#[serde(
    tag = "kind",
    content = "content",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum V3Block {
    Paragraph(V3Paragraph),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct V3Paragraph {
    id: Id,
    runs: Vec<V3TextRun>,
    style: Option<Id>,
    format: V6ParagraphFormat,
    #[serde(default)]
    list: Option<ListKind>,
}

/// Decode the exact schema-3 character and list shape, rejecting new hyperlink fields.
pub(super) fn from_json(json: &str) -> Result<Document, DocumentError> {
    let old: V3Document = serde_json::from_str(json)?;
    debug_assert_eq!(old.schema_version, 3);
    let sections = old
        .sections
        .into_iter()
        .map(|section| {
            let blocks = section
                .blocks
                .into_iter()
                .map(|block| match block {
                    V3Block::Paragraph(old) => {
                        let mut paragraph =
                            Paragraph::new(old.id, old.runs.into_iter().map(Into::into).collect())
                                .with_format(old.format.into())
                                .with_list(old.list);
                        if let Some(style) = old.style {
                            paragraph = paragraph.with_style(style);
                        }
                        Block::Paragraph(paragraph)
                    }
                })
                .collect();
            let mut current = Section::new(section.id, blocks);
            if let Some(properties) = section.properties {
                current = current.with_properties(properties);
            }
            current
        })
        .collect();
    Document::from_parts(
        old.id,
        old.metadata,
        sections,
        old.styles.into_iter().map(Into::into).collect(),
    )
    .map_err(DocumentError::from)
}
