//! Semantic first-level lists are versioned native structure, not text markers.

use office_core::{
    Block, Document, DocumentMetadata, Id, ListKind, Paragraph, SCHEMA_VERSION, Section, TextRun,
    WRITER_FORMAT_VERSION, deserialize_document, serialize_document,
};
use serde_json::{Value, json};

fn example() -> Document {
    let blocks = vec![
        Block::Paragraph(
            Paragraph::new(Id::new("p1").unwrap(), vec![TextRun::new("First")])
                .with_list(Some(ListKind::Numbered)),
        ),
        Block::Paragraph(
            Paragraph::new(Id::new("p2").unwrap(), vec![TextRun::new("Second")])
                .with_list(Some(ListKind::Numbered)),
        ),
    ];
    Document::from_parts(
        Id::new("doc").unwrap(),
        DocumentMetadata::default(),
        vec![Section::new(Id::new("section").unwrap(), blocks)],
        vec![],
    )
    .unwrap()
}

#[test]
fn native_round_trip_keeps_lists_separate_from_text() {
    assert_eq!(SCHEMA_VERSION, 5);
    assert_eq!(WRITER_FORMAT_VERSION, 5);
    let document = example();
    let bytes = serialize_document(&document).unwrap();
    let parsed = deserialize_document(&bytes).unwrap();
    assert_eq!(parsed, document);
    let saved: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(saved["format_version"], WRITER_FORMAT_VERSION);
    let p1 = &saved["document"]["sections"][0]["blocks"][0]["content"];
    assert_eq!(p1["list"], "numbered");
    assert_eq!(p1["runs"][0]["text"], "First");
}

#[test]
fn strict_version_two_migration_rejects_new_list_fields() {
    let current = example();
    let mut old: Value = serde_json::from_str(&current.to_json().unwrap()).unwrap();
    old["schema_version"] = json!(2);
    let unchanged = serde_json::to_string(&old).unwrap();
    assert!(Document::from_json(&unchanged).is_err());
    for block in old["sections"][0]["blocks"].as_array_mut().unwrap() {
        block["content"].as_object_mut().unwrap().remove("list");
        for run in block["content"]["runs"].as_array_mut().unwrap() {
            run["format"].as_object_mut().unwrap().remove("hyperlink");
        }
    }
    let migrated = Document::from_json(&serde_json::to_string(&old).unwrap()).unwrap();
    for block in migrated.sections()[0].blocks() {
        let Block::Paragraph(paragraph) = block else {
            panic!("expected paragraph");
        };
        assert_eq!(paragraph.list(), None);
    }
    assert_eq!(migrated.schema_version(), SCHEMA_VERSION);
}
