//! New page-flow properties are persisted under schema 7, with exact schema-6 migration.
use office_core::{
    Block, Document, DocumentMetadata, DocumentSession, Id, Paragraph, ParagraphFormatPatch,
    PropertyChange, SCHEMA_VERSION, Section, TextRun, WRITER_FORMAT_VERSION, WriterFile,
    deserialize_writer_file, serialize_writer_file,
};
use serde_json::{Value, json};
fn id(s: &str) -> Id {
    Id::new(s).unwrap()
}
fn source() -> Document {
    let para = Paragraph::new(id("p"), vec![TextRun::new("A")]);
    Document::from_parts(
        id("d"),
        DocumentMetadata::default(),
        vec![Section::new(id("s"), vec![Block::Paragraph(para)])],
        vec![],
    )
    .unwrap()
}
#[test]
fn explicit_flags_are_atomic_and_undoable_and_roundtrip_in_current_native() {
    let mut session = DocumentSession::new(source());
    let mut tx = session.begin_transaction();
    let at = tx
        .position(&id("p"), office_core::GraphemeOffset::new(0))
        .unwrap();
    let selection = tx.selection(at.clone(), at).unwrap();
    tx.apply_paragraph_format(
        &selection,
        &ParagraphFormatPatch {
            keep_together: PropertyChange::Set(true),
            keep_with_next: PropertyChange::Set(false),
            ..Default::default()
        },
    )
    .unwrap();
    session.commit(tx).unwrap();
    let paragraph = session.document().paragraphs().next().unwrap();
    assert_eq!(paragraph.format().keep_together, Some(true));
    assert_eq!(paragraph.format().keep_with_next, Some(false));
    assert_eq!(SCHEMA_VERSION, 7);
    assert_eq!(WRITER_FORMAT_VERSION, 7);
    let native = serialize_writer_file(&WriterFile::new(session.document().clone())).unwrap();
    let file = deserialize_writer_file(&native).unwrap();
    assert_eq!(file.document(), session.document());
    session.undo().unwrap();
    assert_eq!(
        session
            .document()
            .paragraphs()
            .next()
            .unwrap()
            .format()
            .keep_together,
        None
    );
    session.redo().unwrap();
    assert_eq!(
        session
            .document()
            .paragraphs()
            .next()
            .unwrap()
            .format()
            .keep_together,
        Some(true)
    );
}
#[test]
fn schema_six_files_upgrade_without_accepting_new_properties_under_old_labels() {
    let native = serialize_writer_file(&WriterFile::new(source())).unwrap();
    let mut old: Value = serde_json::from_slice(&native).unwrap();
    old["format_version"] = json!(6);
    old["document"]["schema_version"] = json!(6);
    for block in old["document"]["sections"][0]["blocks"]
        .as_array_mut()
        .unwrap()
    {
        let format = block["content"]["format"].as_object_mut().unwrap();
        format.remove("keep_together");
        format.remove("keep_with_next");
    }
    let bytes = serde_json::to_vec(&old).unwrap();
    let migrated = deserialize_writer_file(&bytes).unwrap();
    assert_eq!(migrated.document(), &source());
    assert_eq!(migrated.document().schema_version(), 7);
    old["document"]["sections"][0]["blocks"][0]["content"]["format"]["keep_with_next"] =
        Value::Null;
    assert!(deserialize_writer_file(&serde_json::to_vec(&old).unwrap()).is_err());
    old["document"]["sections"][0]["blocks"][0]["content"]["format"]["keep_with_next"] =
        json!(true);
    assert!(deserialize_writer_file(&serde_json::to_vec(&old).unwrap()).is_err());
}
