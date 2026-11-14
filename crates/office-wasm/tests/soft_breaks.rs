//! Soft line breaks preserve paragraph identity and use the existing undo engine.

use office_core::{Document, Id, serialize_document};
use office_wasm::BrowserEngine;
use serde_json::{Value, json};

fn snapshot(engine: &BrowserEngine) -> Value {
    serde_json::from_str(&engine.snapshot()).unwrap()
}

fn command(engine: &mut BrowserEngine, value: Value) -> Value {
    let result: Value = serde_json::from_str(&engine.dispatch(&value.to_string())).unwrap();
    assert_eq!(result["ok"], true, "{result}");
    result["view"].clone()
}

fn text(view: &Value) -> Vec<String> {
    view["paragraphs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|paragraph| {
            paragraph["runs"]
                .as_array()
                .unwrap()
                .iter()
                .map(|run| run["text"].as_str().unwrap())
                .collect()
        })
        .collect()
}

#[test]
fn soft_break_keeps_unicode_paragraph_identity_and_is_one_undo_step() {
    let mut engine = BrowserEngine::new("soft-break").unwrap();
    let original = command(
        &mut engine,
        json!({"type":"replace_text","text":"Cafe\u{301} 👩‍💻"}),
    );
    let paragraph = original["paragraphs"][0]["id"].as_str().unwrap();
    let selection = json!({"anchor":{"paragraphId":paragraph,"utf16":5},"focus":{"paragraphId":paragraph,"utf16":5}});
    command(
        &mut engine,
        json!({"type":"set_selection","selection":selection}),
    );
    let before = engine.export_document().unwrap();
    let inserted = command(&mut engine, json!({"type":"insert_line_break"}));
    assert_eq!(text(&inserted), vec!["Cafe\u{301}\n 👩‍💻"]);
    assert_eq!(inserted["paragraphs"][0]["id"], paragraph);
    assert_eq!(inserted["selection"]["focus"]["utf16"], 6);
    let after = engine.export_document().unwrap();
    command(&mut engine, json!({"type":"undo"}));
    assert_eq!(engine.export_document().unwrap(), before);
    command(&mut engine, json!({"type":"redo"}));
    assert_eq!(engine.export_document().unwrap(), after);
}

#[test]
fn soft_break_replaces_a_range_across_paragraphs_atomically() {
    let mut engine = BrowserEngine::new("soft-range").unwrap();
    let original = command(
        &mut engine,
        json!({"type":"replace_text","text":"one\ntwo"}),
    );
    let first = original["paragraphs"][0]["id"].as_str().unwrap();
    let last = original["paragraphs"][1]["id"].as_str().unwrap();
    let before = engine.export_document().unwrap();
    let changed = command(
        &mut engine,
        json!({"type":"insert_line_break","selection":{
            "anchor":{"paragraphId":first,"utf16":1},"focus":{"paragraphId":last,"utf16":1}
        }}),
    );
    assert_eq!(text(&changed), vec!["o\nwo"]);
    assert_eq!(changed["paragraphs"][0]["id"], first);
    assert_eq!(changed["selection"]["focus"]["utf16"], 2);
    command(&mut engine, json!({"type":"undo"}));
    assert_eq!(engine.export_document().unwrap(), before);
}

#[test]
fn soft_break_in_empty_native_document_materializes_only_through_the_edit() {
    let original =
        serialize_document(&Document::new(Id::new("empty-native").unwrap()).unwrap()).unwrap();
    let mut engine = BrowserEngine::new("soft-empty").unwrap();
    let result: Value = serde_json::from_str(&engine.open_document(&original)).unwrap();
    assert_eq!(result["ok"], true);
    assert!(
        snapshot(&engine)["paragraphs"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let changed = command(&mut engine, json!({"type":"insert_line_break"}));
    assert_eq!(text(&changed), vec!["\n"]);
    command(&mut engine, json!({"type":"undo"}));
    assert_eq!(engine.export_document().unwrap(), original);
}

#[test]
fn stale_and_invalid_soft_breaks_leave_current_content_and_history_intact() {
    let mut engine = BrowserEngine::new("soft-invalid").unwrap();
    let stale = snapshot(&engine)["revision"].clone();
    command(&mut engine, json!({"type":"replace_text","text":"Current"}));
    let before = engine.snapshot();
    for request in [
        json!({"type":"insert_line_break","expectedRevision":stale}),
        json!({"type":"insert_line_break","selection":{
            "anchor":{"paragraphId":"missing","utf16":0},"focus":{"paragraphId":"missing","utf16":0}
        }}),
        json!({"type":"insert_line_break","unknown":true}),
    ] {
        let result: Value = serde_json::from_str(&engine.dispatch(&request.to_string())).unwrap();
        assert_eq!(result["ok"], false);
        assert_eq!(engine.snapshot(), before);
    }
    command(&mut engine, json!({"type":"undo"}));
    assert_eq!(text(&snapshot(&engine)), vec![""]);
}
