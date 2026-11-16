//! Hyperlinks are Rust-owned formatting, not browser-generated HTML.

use office_wasm::BrowserEngine;
use serde_json::{Value, json};

fn command(engine: &mut BrowserEngine, value: Value) -> Value {
    let result: Value = serde_json::from_str(&engine.dispatch(&value.to_string())).unwrap();
    assert_eq!(result["ok"], true, "{result}");
    result["view"].clone()
}

fn url(view: &Value) -> Option<&str> {
    view["paragraphs"][0]["runs"][0]["format"]["hyperlink"].as_str()
}

#[test]
fn insert_edit_remove_and_undo_a_real_link() {
    let mut engine = BrowserEngine::new("linked-document").unwrap();
    let inserted = command(&mut engine, json!({
        "type":"set_hyperlink", "url":"https://example.com/start",
        "text":"Example"
    }));
    assert_eq!(inserted["wordCount"], 1);
    assert_eq!(url(&inserted), Some("https://example.com/start"));
    assert_eq!(inserted["paragraphs"][0]["runs"][0]["text"], "Example");
    let edited = command(&mut engine, json!({
        "type":"set_hyperlink", "url":"https://example.com/updated"
    }));
    assert_eq!(url(&edited), Some("https://example.com/updated"));
    let removed = command(&mut engine, json!({
        "type":"set_hyperlink", "url":null
    }));
    assert_eq!(url(&removed), None);
    assert_eq!(removed["paragraphs"][0]["runs"][0]["text"], "Example");
    assert_eq!(url(&command(&mut engine, json!({"type":"undo"}))),
        Some("https://example.com/updated"));
    assert_eq!(url(&command(&mut engine, json!({"type":"undo"}))),
        Some("https://example.com/start"));
}

#[test]
fn unsafe_destinations_are_rejected_without_touching_bytes() {
    let mut engine = BrowserEngine::new("unsafe-link").unwrap();
    command(&mut engine, json!({"type":"replace_text","text":"Unlinked"}));
    let before = engine.export_document().unwrap();
    let snapshot = engine.snapshot();
    for url in ["javascript:alert(1)", "file:///etc/passwd", "https://a@evil.test/",
        "http://", "https://site.test/a b", "https://site.test\\@evil.test"] {
        let response: Value = serde_json::from_str(&engine.dispatch(
            &json!({"type":"set_hyperlink","url":url,"text":"Click"}).to_string(),
        )).unwrap();
        assert_eq!(response["ok"], false, "{url}");
        assert_eq!(engine.export_document().unwrap(), before);
        assert_eq!(engine.snapshot(), snapshot);
    }
}

#[test]
fn link_is_preserved_across_native_document_serialization() {
    let mut engine = BrowserEngine::new("save-hyperlink").unwrap();
    command(&mut engine, json!({"type":"set_hyperlink","url":"https://example.com",
        "text":"Example"}));
    let bytes = engine.export_document().unwrap();
    let saved = office_core::deserialize_writer_file(&bytes).unwrap();
    let office_core::Block::Paragraph(paragraph) = &saved.document().sections()[0].blocks()[0];
    assert_eq!(paragraph.runs()[0].format.hyperlink.as_deref(),
        Some("https://example.com"));
    assert_eq!(saved.document().schema_version(), 4);
}
