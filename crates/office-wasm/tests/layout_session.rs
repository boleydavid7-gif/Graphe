//! Layout reads share the browser session's revision without entering history.

use office_wasm::BrowserEngine;
use serde_json::{Value, json};

fn view(engine: &BrowserEngine) -> Value {
    serde_json::from_str(&engine.snapshot()).unwrap()
}

fn dispatch(engine: &mut BrowserEngine, command: Value) -> Value {
    let response: Value = serde_json::from_str(&engine.dispatch(&command.to_string())).unwrap();
    assert_eq!(response["ok"], true, "{response}");
    response["view"].clone()
}

fn layout(engine: &mut BrowserEngine, options: &str) -> Value {
    let revision = view(engine)["revision"].as_str().unwrap().to_owned();
    serde_json::from_str(&engine.layout_snapshot(options, Some(&revision)).unwrap()).unwrap()
}

#[test]
fn layout_is_revision_bound_read_only_and_reused_for_selection_changes() {
    let mut engine = BrowserEngine::new("layout-session").unwrap();
    let inserted = dispatch(
        &mut engine,
        json!({"type":"replace_text","text":"Cafe\u{301} 👩🏽‍💻 العربية नमस्ते"}),
    );
    let before_view = engine.snapshot();
    let before_bytes = engine.export_document().unwrap();
    let first = layout(&mut engine, "{}");
    assert_eq!(first["sessionId"], inserted["sessionId"]);
    assert_eq!(first["revision"], inserted["revision"]);
    assert_eq!(first["documentId"], inserted["documentId"]);
    assert_eq!(first["layout"]["documentId"], inserted["documentId"]);
    assert_eq!(first["layout"]["unitsPerPoint"], 64);
    assert!(first["layout"]["fontCatalogId"].is_string());
    assert!(!first["layout"]["pages"].as_array().unwrap().is_empty());
    assert!(!first["layout"]["outlines"].as_array().unwrap().is_empty());
    assert_eq!(engine.snapshot(), before_view);
    assert_eq!(engine.export_document().unwrap(), before_bytes);
    assert_eq!(layout(&mut engine, "{}"), first);

    let paragraph = inserted["paragraphs"][0]["id"].as_str().unwrap();
    let selected = dispatch(
        &mut engine,
        json!({"type":"set_selection","selection":{
            "anchor":{"paragraphId":paragraph,"utf16":0},
            "focus":{"paragraphId":paragraph,"utf16":1}
        }}),
    );
    assert_eq!(selected["revision"], inserted["revision"]);
    assert_eq!(layout(&mut engine, "{}"), first);
    assert_eq!(engine.export_document().unwrap(), before_bytes);
    dispatch(&mut engine, json!({"type":"undo"}));
    assert_eq!(view(&engine)["canUndo"], false);
}

#[test]
fn stale_or_invalid_layout_requests_preserve_content_cursor_and_history() {
    let mut engine = BrowserEngine::new("layout-rejections").unwrap();
    let old_revision = view(&engine)["revision"].as_str().unwrap().to_owned();
    dispatch(
        &mut engine,
        json!({"type":"replace_text","text":"Unchanged"}),
    );
    let before_view = engine.snapshot();
    let before_bytes = engine.export_document().unwrap();
    let error = engine
        .layout_snapshot("{}", Some(&old_revision))
        .unwrap_err();
    assert_eq!(error.code, "stale_layout");
    for options in [
        "{broken",
        "[]",
        "{\"unknown\":true}",
        "{\"fontSize\":0}",
        "{\"pageSize\":{\"width\":0,\"height\":1440}}",
    ] {
        assert!(engine.layout_snapshot(options, None).is_err(), "{options}");
        assert_eq!(engine.snapshot(), before_view);
        assert_eq!(engine.export_document().unwrap(), before_bytes);
    }
    let error = engine
        .layout_snapshot(&" ".repeat(8 * 1024 + 1), None)
        .unwrap_err();
    assert_eq!(error.code, "invalid_layout_options");
    assert_eq!(engine.snapshot(), before_view);
    dispatch(&mut engine, json!({"type":"undo"}));
    assert_eq!(view(&engine)["canUndo"], false);
}

#[test]
fn options_and_new_sessions_cannot_reuse_another_documents_layout() {
    let mut engine = BrowserEngine::new("layout-origins").unwrap();
    dispatch(
        &mut engine,
        json!({"type":"replace_text","text":"Paragraph"}),
    );
    let first = layout(&mut engine, "{}");
    let custom = layout(
        &mut engine,
        "{\"pageSize\":{\"width\":11906,\"height\":16838,\"orientation\":\"portrait\"}}",
    );
    assert_eq!(custom["revision"], first["revision"]);
    assert_ne!(
        custom["layout"]["pages"][0]["width"],
        first["layout"]["pages"][0]["width"]
    );
    assert_eq!(layout(&mut engine, "{}"), first);

    let old_revision = first["revision"].as_str().unwrap();
    dispatch(&mut engine, json!({"type":"new_document"}));
    assert_eq!(
        engine
            .layout_snapshot("{}", Some(old_revision))
            .unwrap_err()
            .code,
        "stale_layout"
    );
    let fresh = layout(&mut engine, "{}");
    assert_ne!(fresh["sessionId"], first["sessionId"]);
    assert_ne!(fresh["documentId"], first["documentId"]);
    for page in fresh["layout"]["pages"].as_array().unwrap() {
        for line in page["lines"].as_array().unwrap() {
            assert_eq!(line["text"], "");
        }
    }
}

#[test]
fn native_reopen_preserves_geometry_while_starting_a_new_runtime_origin() {
    let mut first = BrowserEngine::new("layout-save").unwrap();
    dispatch(
        &mut first,
        json!({"type":"replace_text","text":"ffi Cafe\u{301} 👩‍💻 العربية\nSecond paragraph"}),
    );
    let first_layout = layout(&mut first, "{}");
    let bytes = first.export_document().unwrap();
    let mut reopened = BrowserEngine::new("layout-reopen").unwrap();
    let result: Value = serde_json::from_str(&reopened.open_document(&bytes)).unwrap();
    assert_eq!(result["ok"], true);
    let reopened_layout = layout(&mut reopened, "{}");
    assert_eq!(reopened_layout["layout"], first_layout["layout"]);
    assert_ne!(reopened_layout["sessionId"], first_layout["sessionId"]);
    assert_eq!(reopened.export_document().unwrap(), bytes);
}

#[test]
fn layout_limits_do_not_redefine_valid_native_identifiers() {
    use office_core::{
        Block, Document, DocumentMetadata, Id, Paragraph, Section, TextRun, serialize_document,
    };
    let document = Document::from_parts(
        Id::new("document").unwrap(),
        DocumentMetadata::default(),
        vec![Section::new(
            Id::new("section").unwrap(),
            vec![Block::Paragraph(Paragraph::new(
                Id::new("p".repeat(70_000)).unwrap(),
                vec![TextRun::new("Valid native content")],
            ))],
        )],
        vec![],
    )
    .unwrap();
    let bytes = serialize_document(&document).unwrap();
    let mut engine = BrowserEngine::new("layout-bounds").unwrap();
    let opened: Value = serde_json::from_str(&engine.open_document(&bytes)).unwrap();
    assert_eq!(opened["ok"], true);
    let before = engine.snapshot();
    let native = engine.export_document().unwrap();
    let error = engine.layout_snapshot("{}", None).unwrap_err();
    assert_eq!(error.code, "layout_too_large");
    assert_eq!(engine.snapshot(), before);
    assert_eq!(engine.export_document().unwrap(), native);
}
