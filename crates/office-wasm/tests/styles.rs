//! Existing named-style assignment is exposed without rewriting source formats.

use office_wasm::BrowserEngine;
use serde_json::{Value, json};

fn populated() -> BrowserEngine {
    let mut engine = BrowserEngine::new("styles").unwrap();
    let document: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/writer-sample-v1.json"
    ))
    .unwrap();
    let bytes = serde_json::to_vec(
        &json!({"format":"office-writer","format_version":1,"document":document}),
    )
    .unwrap();
    let opened: Value = serde_json::from_str(&engine.open_document(&bytes)).unwrap();
    assert_eq!(opened["ok"], true, "{opened}");
    engine
}

fn command(engine: &mut BrowserEngine, command: Value) -> Value {
    let result: Value = serde_json::from_str(&engine.dispatch(&command.to_string())).unwrap();
    assert_eq!(result["ok"], true, "{result}");
    result["view"].clone()
}

#[test]
fn default_display_family_matches_the_pinned_layout_without_rewriting_unspecified_data() {
    let mut engine = BrowserEngine::new("font-default").unwrap();
    let view = command(&mut engine, json!({"type":"replace_text","text":"Writer"}));
    assert_eq!(view["format"]["fontFamily"], "Arimo");
    let file: Value = serde_json::from_slice(&engine.export_document().unwrap()).unwrap();
    assert!(
        file["document"]["sections"][0]["blocks"][0]["content"]["runs"][0]["format"]["font_family"]
            .is_null()
    );
    let explicit = command(
        &mut engine,
        json!({"type":"format_character","fontFamily":"Arial","selection":{
            "anchor":{"paragraphId":view["paragraphs"][0]["id"],"utf16":0},
            "focus":{"paragraphId":view["paragraphs"][0]["id"],"utf16":6}
        }}),
    );
    assert_eq!(explicit["format"]["fontFamily"], "Arial");
}

#[test]
fn named_styles_are_available_and_assignment_is_one_undoable_core_edit() {
    let mut engine = populated();
    let view: Value = serde_json::from_str(&engine.snapshot()).unwrap();
    assert_eq!(view["styles"].as_array().unwrap().len(), 2);
    assert_eq!(view["format"]["styleId"], "heading");
    assert_eq!(view["format"]["styleMixed"], false);
    let before = engine.export_document().unwrap();
    let view = command(
        &mut engine,
        json!({"type":"assign_style","styleId":"normal"}),
    );
    assert_eq!(view["paragraphs"][0]["styleId"], "normal");
    assert_eq!(view["format"]["styleId"], "normal");
    command(&mut engine, json!({"type":"undo"}));
    assert_eq!(engine.export_document().unwrap(), before);
    let view = command(&mut engine, json!({"type":"redo"}));
    assert_eq!(view["paragraphs"][0]["styleId"], "normal");
}

#[test]
fn mixed_style_selection_and_clearing_preserve_direct_formatting() {
    let mut engine = populated();
    let selection = json!({"anchor":{"paragraphId":"report-heading","utf16":0},"focus":{"paragraphId":"intro","utf16":1}});
    let selected = command(
        &mut engine,
        json!({"type":"set_selection","selection":selection}),
    );
    assert_eq!(selected["format"]["styleMixed"], true);
    let before: Value = serde_json::from_slice(&engine.export_document().unwrap()).unwrap();
    let view = command(&mut engine, json!({"type":"assign_style","styleId":null}));
    assert_eq!(view["format"]["styleMixed"], false);
    assert!(view["format"]["styleId"].is_null());
    let after: Value = serde_json::from_slice(&engine.export_document().unwrap()).unwrap();
    for index in 0..2 {
        let first = &before["document"]["sections"][0]["blocks"][index]["content"];
        let last = &after["document"]["sections"][0]["blocks"][index]["content"];
        assert!(last["style"].is_null());
        assert_eq!(last["runs"], first["runs"]);
        assert_eq!(last["format"], first["format"]);
        assert_eq!(last["id"], first["id"]);
    }
}

#[test]
fn unknown_style_and_stale_assignment_leave_document_cursor_and_history_intact() {
    let mut engine = populated();
    let before = engine.snapshot();
    let bytes = engine.export_document().unwrap();
    for input in [
        json!({"type":"assign_style","styleId":"missing"}),
        json!({"type":"assign_style","styleId":"normal","expectedRevision":"stale"}),
    ] {
        let failed: Value = serde_json::from_str(&engine.dispatch(&input.to_string())).unwrap();
        assert_eq!(failed["ok"], false);
        assert!(failed["error"]["code"].is_string());
        assert_eq!(engine.snapshot(), before);
        assert_eq!(engine.export_document().unwrap(), bytes);
    }
}

#[test]
fn final_paragraph_at_offset_zero_is_not_given_the_preceding_paragraph_style() {
    let mut engine = populated();
    let selection = json!({"anchor":{"paragraphId":"report-heading","utf16":0},"focus":{"paragraphId":"intro","utf16":0}});
    let selected = command(
        &mut engine,
        json!({"type":"set_selection","selection":selection}),
    );
    assert_eq!(selected["format"]["styleId"], "heading");
    let view = command(&mut engine, json!({"type":"assign_style","styleId":null}));
    assert!(view["paragraphs"][0]["styleId"].is_null());
    assert_eq!(view["paragraphs"][1]["styleId"], "normal");
}
