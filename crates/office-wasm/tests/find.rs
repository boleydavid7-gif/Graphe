//! Literal document search preserves exact content and core Unicode positions.

use office_wasm::BrowserEngine;
use serde_json::{Value, json};

fn command(engine: &mut BrowserEngine, command: Value) -> Value {
    let response: Value = serde_json::from_str(&engine.dispatch(&command.to_string())).unwrap();
    assert_eq!(response["ok"], true, "{response}");
    response["view"].clone()
}

#[test]
fn literal_search_is_read_only_and_case_sensitive_across_multiple_paragraphs() {
    let mut engine = BrowserEngine::new("find").unwrap();
    let view = command(
        &mut engine,
        json!({"type":"replace_text","text":"Writer writer\nwriter"}),
    );
    let before = engine.snapshot();
    let bytes = engine.export_document().unwrap();
    let result: Value = serde_json::from_str(&engine.find_text("writer").unwrap()).unwrap();
    assert_eq!(result["sessionId"], view["sessionId"]);
    assert_eq!(result["documentId"], view["documentId"]);
    assert_eq!(result["revision"], view["revision"]);
    assert_eq!(result["matches"].as_array().unwrap().len(), 2);
    assert_ne!(
        result["matches"][0]["selection"]["anchor"]["paragraphId"],
        result["matches"][1]["selection"]["anchor"]["paragraphId"]
    );
    assert_eq!(engine.snapshot(), before);
    assert_eq!(engine.export_document().unwrap(), bytes);
    command(&mut engine, json!({"type":"undo"}));
    let view: Value = serde_json::from_str(&engine.snapshot()).unwrap();
    assert_eq!(view["paragraphs"][0]["runs"].as_array().unwrap().len(), 0);
}

#[test]
fn partial_emoji_and_combining_hits_select_complete_core_graphemes() {
    let mut engine = BrowserEngine::new("unicode-find").unwrap();
    command(
        &mut engine,
        json!({"type":"replace_text","text":"Ae\u{301}\u{301} 👩🏽‍💻Z"}),
    );
    let marks: Value = serde_json::from_str(&engine.find_text("\u{301}").unwrap()).unwrap();
    assert_eq!(marks["matches"].as_array().unwrap().len(), 1);
    assert_eq!(marks["matches"][0]["selection"]["anchor"]["utf16"], 1);
    assert_eq!(marks["matches"][0]["selection"]["focus"]["utf16"], 4);
    let emoji: Value = serde_json::from_str(&engine.find_text("💻").unwrap()).unwrap();
    assert_eq!(emoji["matches"][0]["selection"]["anchor"]["utf16"], 5);
    assert_eq!(emoji["matches"][0]["selection"]["focus"]["utf16"], 12);
    command(
        &mut engine,
        json!({"type":"set_selection","selection":emoji["matches"][0]["selection"]}),
    );
    assert_eq!(engine.selected_text().unwrap(), "👩🏽‍💻");
}

#[test]
fn search_is_bounded_and_empty_queries_are_not_matches_at_every_boundary() {
    let mut engine = BrowserEngine::new("bounded-find").unwrap();
    command(
        &mut engine,
        json!({"type":"replace_text","text":"x ".repeat(1100)}),
    );
    let result: Value = serde_json::from_str(&engine.find_text("x").unwrap()).unwrap();
    assert_eq!(result["matches"].as_array().unwrap().len(), 1000);
    assert_eq!(result["truncated"], true);
    let empty: Value = serde_json::from_str(&engine.find_text("").unwrap()).unwrap();
    assert!(empty["matches"].as_array().unwrap().is_empty());
    let before = engine.snapshot();
    assert_eq!(
        engine.find_text(&"x".repeat(1025)).unwrap_err().code,
        "invalid_find"
    );
    assert_eq!(engine.snapshot(), before);
}

#[test]
fn query_does_not_normalize_unicode_or_invent_paragraph_separator_matches() {
    let mut engine = BrowserEngine::new("exact-find").unwrap();
    command(
        &mut engine,
        json!({"type":"replace_text","text":"e\u{301}\nsecond"}),
    );
    for query in ["é", "\u{301}\nsecond"] {
        let result: Value = serde_json::from_str(&engine.find_text(query).unwrap()).unwrap();
        assert!(result["matches"].as_array().unwrap().is_empty());
    }
}
