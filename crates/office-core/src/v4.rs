//! Strict schema-4 migration: paragraph-only blocks cannot contain schema-5 tables.
use super::v6::{V6NamedStyle, V6Paragraph};
use super::{Block, Document, DocumentMetadata, Id, Section, SectionProperties};
use crate::DocumentError;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct V4Document {
    schema_version: u64,
    id: Id,
    metadata: DocumentMetadata,
    sections: Vec<V4Section>,
    styles: Vec<V6NamedStyle>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct V4Section {
    id: Id,
    blocks: Vec<V4Block>,
    properties: Option<SectionProperties>,
}
#[derive(Deserialize)]
#[serde(
    tag = "kind",
    content = "content",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum V4Block {
    Paragraph(V6Paragraph),
}

pub(super) fn from_json(json: &str) -> Result<Document, DocumentError> {
    let old: V4Document = serde_json::from_str(json)?;
    debug_assert_eq!(old.schema_version, 4);
    let sections = old
        .sections
        .into_iter()
        .map(|section| {
            let blocks = section
                .blocks
                .into_iter()
                .map(|block| match block {
                    V4Block::Paragraph(p) => Block::Paragraph(p.into()),
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
    .map_err(DocumentError::from)
}
