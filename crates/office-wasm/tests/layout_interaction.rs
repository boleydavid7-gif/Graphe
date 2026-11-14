//! Native browser-adapter interaction checks against the real Rust layout.

use office_wasm::BrowserEngine;
use serde_json::{Value, json};

fn snapshot(engine: &BrowserEngine) -> Value {
    serde_json::from_str(&engine.snapshot()).unwrap()
}

fn dispatch(engine: &mut BrowserEngine, command: Value) -> Value {
    let result: Value = serde_json::from_str(&engine.dispatch(&command.to_string())).unwrap();
    assert_eq!(result["ok"], true, "{result}");
    result["view"].clone()
}

fn action(engine: &mut BrowserEngine, command: Value) -> Value {
    let revision = snapshot(engine)["revision"].as_str().unwrap().to_owned();
    serde_json::from_str(
        &engine
            .layout_action("{}", &command.to_string(), Some(&revision))
            .unwrap(),
    )
    .unwrap()
}

fn geometry(engine: &mut BrowserEngine) -> Value {
    let revision = snapshot(engine)["revision"].as_str().unwrap().to_owned();
    serde_json::from_str(&engine.selection_geometry("{}", Some(&revision)).unwrap()).unwrap()
}

fn soft_break_document() -> BrowserEngine {
    let mut engine = BrowserEngine::new("interaction-soft-break").unwrap();
    dispatch(&mut engine, json!({"type":"replace_text","text":"one"}));
    dispatch(&mut engine, json!({"type":"insert_line_break"}));
    dispatch(&mut engine, json!({"type":"replace_text","text":"two"}));
    engine
}

#[test]
fn hit_testing_updates_rust_selection_without_content_history_or_revision_changes() {
    let mut engine = BrowserEngine::new("interaction-pointer").unwrap();
    dispatch(
        &mut engine,
        json!({"type":"replace_text","text":"Cafe\u{301} 👩🏽‍💻 שלום"}),
    );
    let before = snapshot(&engine);
    let bytes = engine.export_document().unwrap();
    let layout: Value = serde_json::from_str(&engine.layout_snapshot("{}", None).unwrap()).unwrap();
    let caret = &layout["layout"]["pages"][0]["lines"][0]["carets"][2];
    let hit = action(
        &mut engine,
        json!({"type":"hit","pageNumber":1,"x":caret["x"],"y":caret["y"]}),
    );
    assert_eq!(hit["view"]["selection"]["focus"]["utf16"], caret["utf16"]);
    assert_eq!(hit["geometry"]["focus"]["position"]["utf8"], caret["utf8"]);
    assert_eq!(hit["view"]["revision"], before["revision"]);
    assert_eq!(hit["view"]["canUndo"], before["canUndo"]);
    assert_eq!(hit["view"]["canRedo"], before["canRedo"]);
    assert_eq!(engine.export_document().unwrap(), bytes);
    assert_eq!(geometry(&mut engine)["geometry"], hit["geometry"]);
}

#[test]
fn native_navigation_and_extension_select_unicode_without_a_second_history() {
    let mut engine = BrowserEngine::new("interaction-keyboard").unwrap();
    dispatch(
        &mut engine,
        json!({"type":"replace_text","text":"e\u{301}👩🏽‍💻Z"}),
    );
    let before = engine.export_document().unwrap();
    action(
        &mut engine,
        json!({"type":"move","direction":"document_start"}),
    );
    let first = action(
        &mut engine,
        json!({"type":"move","direction":"right","extend":true}),
    );
    assert_eq!(first["view"]["selection"]["anchor"]["utf16"], 0);
    assert_eq!(first["view"]["selection"]["focus"]["utf16"], 2);
    assert_eq!(engine.selected_text().unwrap(), "e\u{301}");
    let second = action(
        &mut engine,
        json!({"type":"move","direction":"right","extend":true}),
    );
    assert_eq!(second["geometry"]["focus"]["position"]["grapheme"], 2);
    assert_eq!(second["geometry"]["focus"]["position"]["utf16"], 9);
    assert_eq!(engine.selected_text().unwrap(), "e\u{301}👩🏽‍💻");
    assert_eq!(engine.export_document().unwrap(), before);
    dispatch(&mut engine, json!({"type":"undo"}));
    assert_eq!(snapshot(&engine)["canUndo"], false);
    let empty = snapshot(&engine);
    assert!(
        empty["paragraphs"][0]["runs"]
            .as_array()
            .unwrap()
            .iter()
            .all(|run| run["text"].as_str().unwrap().is_empty())
    );
}

#[test]
fn mandatory_breaks_use_downstream_carets_and_preferred_x_is_runtime_state() {
    let mut engine = soft_break_document();
    action(
        &mut engine,
        json!({"type":"move","direction":"document_start"}),
    );
    action(&mut engine, json!({"type":"move","direction":"end"}));
    let after_break = action(&mut engine, json!({"type":"move","direction":"right"}));
    assert_eq!(after_break["geometry"]["focus"]["position"]["utf16"], 4);
    assert_eq!(
        after_break["geometry"]["focus"]["position"]["affinity"],
        "leading"
    );
    assert_eq!(after_break["geometry"]["focus"]["lineIndex"], 1);
    let up = action(&mut engine, json!({"type":"move","direction":"up"}));
    assert!(up["preferredX"].is_i64());
    assert_eq!(geometry(&mut engine)["preferredX"], up["preferredX"]);
    let selection = up["view"]["selection"].clone();
    dispatch(
        &mut engine,
        json!({"type":"set_selection","selection":selection}),
    );
    assert!(geometry(&mut engine)["preferredX"].is_null());
}

#[test]
fn select_all_and_collapsing_a_range_use_the_live_rust_selection() {
    let mut engine = BrowserEngine::new("interaction-select-all").unwrap();
    dispatch(
        &mut engine,
        json!({"type":"replace_text","text":"First\nSecond"}),
    );
    let all = action(&mut engine, json!({"type":"select_all"}));
    assert_eq!(engine.selected_text().unwrap(), "First\nSecond");
    assert!(all["geometry"]["rectangles"].as_array().unwrap().len() >= 2);
    let collapsed = action(&mut engine, json!({"type":"move","direction":"left"}));
    assert_eq!(
        collapsed["view"]["selection"]["anchor"],
        collapsed["view"]["selection"]["focus"]
    );
    assert_eq!(collapsed["view"]["selection"]["focus"]["utf16"], 0);
    assert_eq!(engine.selected_text().unwrap(), "");
}

#[test]
fn malformed_stale_and_excessive_interactions_leave_selection_and_content_intact() {
    let mut engine = soft_break_document();
    let old = snapshot(&engine)["revision"].as_str().unwrap().to_owned();
    dispatch(&mut engine, json!({"type":"replace_text","text":"!"}));
    action(&mut engine, json!({"type":"move","direction":"up"}));
    let before = engine.snapshot();
    let bytes = engine.export_document().unwrap();
    let before_geometry = geometry(&mut engine);
    let valid = "{\"type\":\"move\",\"direction\":\"right\"}";
    assert_eq!(
        engine
            .layout_action("{}", valid, Some(&old))
            .unwrap_err()
            .code,
        "stale_layout"
    );
    for invalid in [
        "{broken",
        "[]",
        "{\"type\":\"move\",\"direction\":\"future\"}",
        "{\"type\":\"move\",\"direction\":\"right\",\"future\":true}",
        "{\"type\":\"hit\",\"pageNumber\":0,\"x\":0,\"y\":0}",
        "{\"type\":\"hit\",\"pageNumber\":1,\"x\":9223372036854775807,\"y\":0}",
    ] {
        assert!(
            engine.layout_action("{}", invalid, None).is_err(),
            "{invalid}"
        );
        assert_eq!(engine.snapshot(), before);
        assert_eq!(geometry(&mut engine), before_geometry);
        assert_eq!(engine.export_document().unwrap(), bytes);
    }
    assert_eq!(
        engine
            .layout_action("{}", &" ".repeat(8193), None)
            .unwrap_err()
            .code,
        "invalid_layout_action"
    );
    dispatch(&mut engine, json!({"type":"new_document"}));
    assert_eq!(
        engine
            .layout_action("{}", valid, Some(&old))
            .unwrap_err()
            .code,
        "stale_layout"
    );
}
