//! Exact schema-1 decoding followed by explicit migration into the current model.
//!
//! Keep the historical records separate: adding a new optional field to the
//! current model must never make that field valid inside a schema-1 payload.
//! Unchanged text-run, character-format and metadata types are reused today;
//! freeze their legacy definitions before adding fields to those current types.

use serde::Deserialize;

use super::{
    Block, Document, DocumentMetadata, Id, NamedStyle, Paragraph, Section, SectionProperties,
    TextRun,
};
use crate::{
    Alignment, CharacterFormat, DocumentError, Indentation, LineSpacing, ParagraphFormat, Twips,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyDocument {
    schema_version: u64,
    id: Id,
    metadata: DocumentMetadata,
    sections: Vec<LegacySection>,
    styles: Vec<LegacyStyle>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacySection {
    id: Id,
    blocks: Vec<LegacyBlock>,
    properties: Option<LegacySectionProperties>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacySectionProperties {}

#[derive(Deserialize)]
#[serde(
    tag = "kind",
    content = "content",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum LegacyBlock {
    Paragraph(LegacyParagraph),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyParagraph {
    id: Id,
    runs: Vec<TextRun>,
    style: Option<Id>,
    format: LegacyParagraphFormat,
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct LegacyParagraphFormat {
    alignment: Option<Alignment>,
    spacing_before: Option<Twips>,
    spacing_after: Option<Twips>,
    line_spacing: Option<LineSpacing>,
    indentation: Option<Indentation>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyStyle {
    id: Id,
    name: String,
    based_on: Option<Id>,
    paragraph_format: LegacyParagraphFormat,
    character_format: CharacterFormat,
}

impl From<LegacyParagraphFormat> for ParagraphFormat {
    fn from(value: LegacyParagraphFormat) -> Self {
        Self {
            alignment: value.alignment,
            spacing_before: value.spacing_before,
            spacing_after: value.spacing_after,
            line_spacing: value.line_spacing,
            indentation: value.indentation,
            page_break_before: None,
        }
    }
}

/// Decodes schema 1 without accepting any schema-2-only field.
pub(super) fn from_json(json: &str) -> Result<Document, DocumentError> {
    let legacy: LegacyDocument = serde_json::from_str(json)?;
    debug_assert_eq!(legacy.schema_version, 1);
    let sections = legacy
        .sections
        .into_iter()
        .map(|section| {
            let blocks = section
                .blocks
                .into_iter()
                .map(|block| match block {
                    LegacyBlock::Paragraph(paragraph) => {
                        let mut converted = Paragraph::new(paragraph.id, paragraph.runs)
                            .with_format(paragraph.format.into());
                        if let Some(style) = paragraph.style {
                            converted = converted.with_style(style);
                        }
                        Block::Paragraph(converted)
                    }
                })
                .collect();
            let mut converted = Section::new(section.id, blocks);
            if section.properties.is_some() {
                converted = converted.with_properties(SectionProperties::default());
            }
            converted
        })
        .collect();
    let styles = legacy
        .styles
        .into_iter()
        .map(|style| NamedStyle {
            id: style.id,
            name: style.name,
            based_on: style.based_on,
            paragraph_format: style.paragraph_format.into(),
            character_format: style.character_format,
        })
        .collect();
    Document::from_parts(legacy.id, legacy.metadata, sections, styles).map_err(DocumentError::from)
}
