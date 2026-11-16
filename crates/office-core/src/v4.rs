//! Strict schema-4 reader: old files cannot smuggle table-cell metadata
//! under the previous document version, even when the field is optional.

use serde::Deserialize;
use super::{
    Block, Document, DocumentMetadata, Id, ListKind, NamedStyle, Paragraph,
    Section, SectionProperties, TextRun,
};
use crate::{DocumentError, ParagraphFormat};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct V4Document {
    schema_version: u64,
    id: Id,
    metadata: DocumentMetadata,
    sections: Vec<V4Section>,
    styles: Vec<NamedStyle>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct V4Section {
    id: Id,
    blocks: Vec<V4Block>,
    properties: Option<SectionProperties>,
}

#[derive(Deserialize)]
#[serde(tag = "kind", content = "content", rename_all = "snake_case", deny_unknown_fields)]
enum V4Block {
    Paragraph(V4Paragraph),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct V4Paragraph {
    id: Id,
    runs: Vec<TextRun>,
    style: Option<Id>,
    format: ParagraphFormat,
    #[serde(default)]
    list: Option<ListKind>,
}

/// Parse the exact schema-four paragraphs, migrating with no table membership.
pub(super) fn from_json(json: &str) -> Result<Document, DocumentError> {
    let old: V4Document = serde_json::from_str(json)?;
    debug_assert_eq!(old.schema_version, 4);
    let sections = old.sections.into_iter().map(|section| {
        let blocks = section.blocks.into_iter().map(|block| match block {
            V4Block::Paragraph(old) => {
                let mut paragraph = Paragraph::new(old.id, old.runs)
                    .with_format(old.format).with_list(old.list);
                if let Some(style) = old.style {
                    paragraph = paragraph.with_style(style);
                }
                Block::Paragraph(paragraph)
            }
        }).collect();
        let mut section_out = Section::new(section.id, blocks);
        if let Some(properties) = section.properties {
            section_out = section_out.with_properties(properties);
        }
        section_out
    }).collect();
    Document::from_parts(old.id, old.metadata, sections, old.styles)
        .map_err(DocumentError::from)
}
