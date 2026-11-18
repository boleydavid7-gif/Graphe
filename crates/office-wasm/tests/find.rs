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

#[test]
fn replace_all_is_atomic_across_paragraphs_and_one_undo_step() {
    let mut engine = BrowserEngine::new("replace-all").unwrap();
    let original = command(
        &mut engine,
        json!({"type":"replace_text","text":"red red\nred"}),
    );
    let after = command(
        &mut engine,
        json!({
            "type":"replace_all", "query":"red", "text":"blue",
            "expectedRevision": original["revision"]
        }),
    );
    let texts: Vec<String> = after["paragraphs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|paragraph| {
            paragraph["runs"]
                .as_array()
                .unwrap()
                .iter()
                .map(|run| run["text"].as_str().unwrap())
                .collect::<String>()
        })
        .collect();
    assert_eq!(texts, vec!["blue blue", "blue"]);
    assert_ne!(after["revision"], original["revision"]);
    let undone = command(&mut engine, json!({"type":"undo"}));
    let original_text: Vec<String> = undone["paragraphs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|paragraph| {
            paragraph["runs"]
                .as_array()
                .unwrap()
                .iter()
                .map(|run| run["text"].as_str().unwrap())
                .collect::<String>()
        })
        .collect();
    assert_eq!(original_text, vec!["red red", "red"]);
    command(&mut engine, json!({"type":"redo"}));
    let red: Value = serde_json::from_str(&engine.find_text("red").unwrap()).unwrap();
    assert!(red["matches"].as_array().unwrap().is_empty());
    let blue: Value = serde_json::from_str(&engine.find_text("blue").unwrap()).unwrap();
    assert_eq!(blue["matches"].as_array().unwrap().len(), 3);
}

#[test]
fn replace_all_handles_overlapping_literal_hits_and_full_graphemes() {
    let mut engine = BrowserEngine::new("replace-complex").unwrap();
    command(
        &mut engine,
        json!({"type":"replace_text","text":"aaaa Ae\u{301} 👩🏽‍💻"}),
    );
    command(
        &mut engine,
        json!({"type":"replace_all","query":"aa","text":"b"}),
    );
    let text = |engine: &BrowserEngine| -> String {
        let snapshot: Value = serde_json::from_str(&engine.snapshot()).unwrap();
        snapshot["paragraphs"][0]["runs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|run| run["text"].as_str().unwrap())
            .collect()
    };
    assert_eq!(text(&engine), "bb Ae\u{301} 👩🏽‍💻");
    command(
        &mut engine,
        json!({"type":"replace_all","query":"\u{301}","text":"X"}),
    );
    assert_eq!(text(&engine), "bb AX 👩🏽‍💻");
    command(
        &mut engine,
        json!({"type":"replace_all","query":"💻","text":"Y"}),
    );
    assert_eq!(text(&engine), "bb AX Y");
}

#[test]
fn replace_all_rejects_stale_truncated_and_multiline_edits_without_changes() {
    let mut engine = BrowserEngine::new("replace-guard").unwrap();
    let before = command(
        &mut engine,
        json!({"type":"replace_text","text":"x ".repeat(1100)}),
    );
    let bytes = engine.export_document().unwrap();
    let snapshot = engine.snapshot();
    for attempted in [
        json!({"type":"replace_all","query":"x","text":"y","expectedRevision":"stale"}),
        json!({"type":"replace_all","query":"x","text":"y","expectedRevision":before["revision"]}),
        json!({"type":"replace_all","query":"x","text":"a\nb"}),
        json!({"type":"replace_all","query":"","text":"y"}),
    ] {
        let result: Value = serde_json::from_str(&engine.dispatch(&attempted.to_string())).unwrap();
        assert_eq!(result["ok"], false, "{attempted}");
        assert_eq!(engine.snapshot(), snapshot);
        assert_eq!(engine.export_document().unwrap(), bytes);
    }
}
