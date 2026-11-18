//! Exact schema-5 table model; images cannot be smuggled under an older label.
use super::{
    Block, Document, DocumentMetadata, Id, NamedStyle, Paragraph, Section, SectionProperties, Table,
};
use crate::DocumentError;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct V5Document {
    schema_version: u64,
    id: Id,
    metadata: DocumentMetadata,
    sections: Vec<V5Section>,
    styles: Vec<NamedStyle>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct V5Section {
    id: Id,
    blocks: Vec<V5Block>,
    properties: Option<SectionProperties>,
}
#[derive(Deserialize)]
#[serde(
    tag = "kind",
    content = "content",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum V5Block {
    Paragraph(Paragraph),
    Table(Table),
}
pub(super) fn from_json(json: &str) -> Result<Document, DocumentError> {
    let old: V5Document = serde_json::from_str(json)?;
    debug_assert_eq!(old.schema_version, 5);
    let sections = old
        .sections
        .into_iter()
        .map(|s| {
            let blocks = s
                .blocks
                .into_iter()
                .map(|b| match b {
                    V5Block::Paragraph(p) => Block::Paragraph(p),
                    V5Block::Table(t) => Block::Table(t),
                })
                .collect();
            let current = Section::new(s.id, blocks);
            match s.properties {
                Some(p) => current.with_properties(p),
                None => current,
            }
        })
        .collect();
    Document::from_parts(old.id, old.metadata, sections, old.styles).map_err(Into::into)
}
