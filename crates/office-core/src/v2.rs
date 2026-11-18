//! Strict schema-2 reader. Old files cannot smuggle list metadata into the
//! older format, even though list membership is optional in schema 3.

use serde::Deserialize;

use super::v3::{V3NamedStyle, V3TextRun};
use super::{
    Block, Document, DocumentMetadata, Id, ListKind, Paragraph, Section, SectionProperties,
};
use crate::{DocumentError, ParagraphFormat};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct V2Document {
    schema_version: u64,
    id: Id,
    metadata: DocumentMetadata,
    sections: Vec<V2Section>,
    styles: Vec<V3NamedStyle>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct V2Section {
    id: Id,
    blocks: Vec<V2Block>,
    properties: Option<SectionProperties>,
}

#[derive(Deserialize)]
#[serde(
    tag = "kind",
    content = "content",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum V2Block {
    Paragraph(V2Paragraph),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct V2Paragraph {
    id: Id,
    runs: Vec<V3TextRun>,
    style: Option<Id>,
    format: ParagraphFormat,
}

/// Decode exactly the former schema-2 paragraph shape and migrate with no lists.
pub(super) fn from_json(json: &str) -> Result<Document, DocumentError> {
    let value: V2Document = serde_json::from_str(json)?;
    debug_assert_eq!(value.schema_version, 2);
    let sections = value
        .sections
        .into_iter()
        .map(|section| {
            let blocks = section
                .blocks
                .into_iter()
                .map(|block| match block {
                    V2Block::Paragraph(old) => {
                        let mut paragraph =
                            Paragraph::new(old.id, old.runs.into_iter().map(Into::into).collect())
                                .with_format(old.format)
                                .with_list(None::<ListKind>);
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
        value.id,
        value.metadata,
        sections,
        value.styles.into_iter().map(Into::into).collect(),
    )
    .map_err(DocumentError::from)
}
