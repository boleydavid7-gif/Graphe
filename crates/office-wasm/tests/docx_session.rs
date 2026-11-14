//! DOCX conversion, immutable-original retention and browser-session atomicity.

use office_core::{
    Id, OriginalSource, PersistenceError, WriterFile, deserialize_document,
    deserialize_writer_file, serialize_document, serialize_writer_file,
};
use office_docx::{DOCX_MEDIA_TYPE, DocxImportOutcome, import_docx};
use office_wasm::{BrowserEngine, MAX_DOCUMENT_BYTES};
use serde_json::{Value, json};

const BASIC: &[u8] = include_bytes!("../../../tests/fixtures/docx/supported-basic.docx");
const FORMATTED: &[u8] = include_bytes!("../../../tests/fixtures/docx/supported-formatting.docx");
const UNSUPPORTED: &[u8] =
    include_bytes!("../../../tests/fixtures/docx/writer-source-preservation.docx");
const PARTIAL_SPACING: &[u8] =
    include_bytes!("../../../tests/fixtures/docx/unsupported-partial-line-spacing.docx");

fn view(engine: &BrowserEngine) -> Value {
    serde_json::from_str(&engine.snapshot()).unwrap()
}

fn execute(engine: &mut BrowserEngine, command: Value) -> Value {
    let response: Value = serde_json::from_str(&engine.dispatch(&command.to_string())).unwrap();
    assert_eq!(response["ok"], true, "{response}");
    response["view"].clone()
}

fn open_docx(engine: &mut BrowserEngine, bytes: &[u8]) -> Value {
    let response: Value = serde_json::from_str(&engine.import_docx(bytes, "原本.docx")).unwrap();
    assert_eq!(response["ok"], true, "{response}");
    response["view"].clone()
}

#[test]
fn supported_import_retains_exact_original_and_geometry_across_native_reopen() {
    let mut engine = BrowserEngine::new("docx-import").unwrap();
    let imported = open_docx(&mut engine, FORMATTED);
    assert_eq!(imported["originalSource"]["filename"], "原本.docx");
    assert_eq!(imported["canUndo"], false);
    assert_eq!(engine.export_original().unwrap(), FORMATTED);

    let bytes = engine.export_document().unwrap();
    assert!(matches!(
        deserialize_document(&bytes),
        Err(PersistenceError::SourcePresent)
    ));
    let file = deserialize_writer_file(&bytes).unwrap();
    assert_eq!(file.document().sections().len(), 2);
    assert_eq!(file.original_source().unwrap().bytes(), FORMATTED);
    assert_eq!(
        file.document().sections()[0]
            .properties()
            .unwrap()
            .page_size
            .unwrap()
            .width
            .0,
        12240
    );
    assert_eq!(
        file.document().sections()[1]
            .properties()
            .unwrap()
            .page_size
            .unwrap()
            .width
            .0,
        11906
    );

    let mut reopened = BrowserEngine::new("docx-reopen").unwrap();
    let result: Value = serde_json::from_str(&reopened.open_document(&bytes)).unwrap();
    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(reopened.export_document().unwrap(), bytes);
    assert_eq!(reopened.export_original().unwrap(), FORMATTED);
    assert_ne!(view(&reopened)["sessionId"], imported["sessionId"]);
}

#[test]
fn unsupported_and_corrupt_docx_preserve_existing_content_history_and_original() {
    let mut engine = BrowserEngine::new("atomic-docx").unwrap();
    open_docx(&mut engine, BASIC);
    execute(
        &mut engine,
        json!({"type":"set_title", "title":"Unsaved work"}),
    );
    let before = engine.snapshot();
    let bytes = engine.export_document().unwrap();
    for (input, code) in [
        (UNSUPPORTED, "unsupported_docx"),
        (PARTIAL_SPACING, "unsupported_docx"),
        (b"broken".as_slice(), "invalid_docx"),
    ] {
        let response: Value =
            serde_json::from_str(&engine.import_docx(input, "next.docx")).unwrap();
        assert_eq!(response["ok"], false);
        assert_eq!(response["error"]["code"], code);
        if code == "unsupported_docx" {
            assert!(
                !response["error"]["compatibility"]["diagnostics"]
                    .as_array()
                    .unwrap()
                    .is_empty()
            );
        }
        assert_eq!(engine.snapshot(), before);
        assert_eq!(engine.export_document().unwrap(), bytes);
        assert_eq!(engine.export_original().unwrap(), BASIC);
    }
    execute(&mut engine, json!({"type":"undo"}));
    assert_eq!(view(&engine)["title"], "");
}

#[test]
fn deleted_text_is_absent_from_regenerated_docx_and_retained_only_in_original_backup() {
    let mut engine = BrowserEngine::new("edited-copy").unwrap();
    let imported = open_docx(&mut engine, BASIC);
    let paragraph = &imported["paragraphs"][0];
    let text: String = paragraph["runs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|run| run["text"].as_str().unwrap())
        .collect();
    let id = paragraph["id"].as_str().unwrap();
    execute(
        &mut engine,
        json!({"type":"replace_text", "text":"Edited copy 👩🏽‍💻", "selection":{
            "anchor":{"paragraphId":id,"utf16":0},
            "focus":{"paragraphId":id,"utf16":text.encode_utf16().count()}
        }}),
    );
    let exported = engine.export_docx().unwrap();
    let DocxImportOutcome::Editable(restored) =
        import_docx(&exported, Id::new("edited-reimport").unwrap()).unwrap()
    else {
        panic!("regenerated supported content must remain editable");
    };
    let all_text: String = restored
        .document()
        .sections()
        .iter()
        .flat_map(|section| section.blocks())
        .flat_map(|block| {
            let office_core::Block::Paragraph(paragraph) = block;
            paragraph.runs()
        })
        .map(|run| run.text.as_str())
        .collect();
    assert_eq!(all_text, "Edited copy 👩🏽‍💻");
    assert!(!all_text.contains("DOCX hello"));
    assert_eq!(engine.export_original().unwrap(), BASIC);
    let file = deserialize_writer_file(&engine.export_document().unwrap()).unwrap();
    assert_eq!(file.original_source().unwrap().bytes(), BASIC);
}

#[test]
fn history_and_fork_retain_source_while_new_clears_it() {
    let mut engine = BrowserEngine::new("source-lifecycle").unwrap();
    let initial = open_docx(&mut engine, BASIC);
    execute(&mut engine, json!({"type":"set_title", "title":"Edited"}));
    execute(&mut engine, json!({"type":"undo"}));
    assert_eq!(engine.export_original().unwrap(), BASIC);
    execute(&mut engine, json!({"type":"redo"}));
    let fork = execute(&mut engine, json!({"type":"fork_document"}));
    assert_ne!(fork["documentId"], initial["documentId"]);
    assert_eq!(engine.export_original().unwrap(), BASIC);
    execute(&mut engine, json!({"type":"new_document"}));
    assert_eq!(view(&engine)["originalSource"], Value::Null);
    assert_eq!(engine.export_original().unwrap_err().code, "no_original");
    assert!(
        deserialize_writer_file(&engine.export_document().unwrap())
            .unwrap()
            .original_source()
            .is_none()
    );
}

#[test]
fn source_bearing_native_input_is_revalidated_before_session_replacement() {
    let mut engine = BrowserEngine::new("source-validation").unwrap();
    open_docx(&mut engine, BASIC);
    let initial = engine.export_document().unwrap();
    let document = deserialize_writer_file(&initial).unwrap().into_parts().0;
    for source in [UNSUPPORTED, b"invalid ZIP".as_slice()] {
        let file = WriterFile::with_original_source(
            document.clone(),
            OriginalSource::new("docx", DOCX_MEDIA_TYPE, "forged.docx", source).unwrap(),
        )
        .unwrap();
        let response: Value =
            serde_json::from_str(&engine.open_document(&serialize_writer_file(&file).unwrap()))
                .unwrap();
        assert_eq!(response["ok"], false);
        assert_eq!(engine.export_document().unwrap(), initial);
        assert_eq!(engine.export_original().unwrap(), BASIC);
    }
    let source = OriginalSource::new(
        "docx",
        DOCX_MEDIA_TYPE.to_ascii_uppercase(),
        "uppercase-mime.docx",
        BASIC,
    )
    .unwrap();
    let file = WriterFile::with_original_source(document, source).unwrap();
    let response: Value =
        serde_json::from_str(&engine.open_document(&serialize_writer_file(&file).unwrap()))
            .unwrap();
    assert_eq!(
        response["ok"], true,
        "MIME type matching is case insensitive: {response}"
    );
    assert_eq!(engine.export_original().unwrap(), BASIC);
}

#[test]
fn candidate_size_budget_includes_retained_source_before_committing_edits() {
    let mut engine = BrowserEngine::new("source-budget").unwrap();
    open_docx(&mut engine, BASIC);
    let original = engine.export_document().unwrap();
    let file = deserialize_writer_file(&original).unwrap();
    let plain = serialize_document(file.document()).unwrap().len();
    let overhead = original.len() - plain;
    assert!(overhead > 100);
    let title_length = MAX_DOCUMENT_BYTES - plain - overhead / 2;
    let response: Value = serde_json::from_str(
        &engine.dispatch(
            &json!({
                "type":"set_title", "title":"x".repeat(title_length)
            })
            .to_string(),
        ),
    )
    .unwrap();
    assert_eq!(response["ok"], false);
    assert_eq!(response["error"]["code"], "document_too_large");
    assert_eq!(engine.export_document().unwrap(), original);
    assert_eq!(engine.export_original().unwrap(), BASIC);
}
