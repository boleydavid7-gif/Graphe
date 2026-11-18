//! Browser/WASM paragraph geometry is Rust-owned and revision guarded.

use office_wasm::BrowserEngine;
use serde_json::{Value, json};

fn apply(engine: &mut BrowserEngine, command: Value) -> Value {
    let value: Value = serde_json::from_str(&engine.dispatch(&command.to_string())).unwrap();
    assert_eq!(value["ok"], true, "{value}");
    value["view"].clone()
}

#[test]
fn paragraph_spacing_and_hanging_indent_keep_text_and_undo() {
    let mut engine = BrowserEngine::new("paragraph-controls").unwrap();
    let initial = apply(
        &mut engine,
        json!({
            "type": "replace_text", "text": "First\nSecond\nThird"
        }),
    );
    let paragraph_id = |index: usize| initial["paragraphs"][index]["id"].as_str().unwrap();
    let range = json!({
        "anchor": {"paragraphId": paragraph_id(0), "utf16": 0},
        "focus": {"paragraphId": paragraph_id(1), "utf16": 6},
    });
    let word_count = initial["wordCount"].clone();
    let updated = apply(
        &mut engine,
        json!({
            "type": "set_paragraph_metrics",
            "spacingBeforePt": 4,
            "spacingAfterPt": 12,
            "lineSpacingMultiple": 1.15,
            "indentLeftIn": 0.5,
            "indentRightIn": 0.25,
            "firstLineIn": -0.25,
            "selection": range,
        }),
    );
    assert_eq!(updated["wordCount"], word_count);
    assert_eq!(updated["format"]["spacingBeforePt"], 4.0);
    assert_eq!(updated["format"]["spacingAfterPt"], 12.0);
    assert_eq!(updated["format"]["lineSpacingMultiple"], 1.15);
    assert_eq!(updated["format"]["indentLeftIn"], 0.5);
    assert_eq!(updated["format"]["indentRightIn"], 0.25);
    assert_eq!(updated["format"]["firstLineIn"], -0.25);
    for index in [0, 1] {
        let format = &updated["paragraphs"][index]["format"];
        assert_eq!(format["spacingBeforePt"], 4.0);
        assert_eq!(format["spacingAfterPt"], 12.0);
        assert_eq!(format["lineHeight"], "1.15");
        assert_eq!(format["indentLeftPt"], 36.0);
        assert_eq!(format["indentRightPt"], 18.0);
        assert_eq!(format["firstLinePt"], -18.0);
    }
    assert_eq!(updated["paragraphs"][2]["format"]["spacingAfterPt"], 8.0);
    let undo = apply(&mut engine, json!({"type":"undo"}));
    assert_eq!(undo["paragraphs"][0]["format"]["spacingAfterPt"], 8.0);
    let redo = apply(&mut engine, json!({"type":"redo"}));
    assert_eq!(redo["paragraphs"][0]["format"]["spacingAfterPt"], 12.0);
}

#[test]
fn partial_update_does_not_reset_unrelated_indent_or_paragraph_values() {
    let mut engine = BrowserEngine::new("mixed-paragraph").unwrap();
    let initial = apply(
        &mut engine,
        json!({"type":"replace_text","text":"First\nSecond"}),
    );
    let first = initial["paragraphs"][0]["id"].as_str().unwrap();
    let second = initial["paragraphs"][1]["id"].as_str().unwrap();
    let first_caret = json!({
        "anchor":{"paragraphId":first,"utf16":0},
        "focus":{"paragraphId":first,"utf16":0}
    });
    apply(
        &mut engine,
        json!({"type":"set_paragraph_metrics",
        "indentRightIn":0.5,"selection":first_caret}),
    );
    let selection = json!({
        "anchor":{"paragraphId":first,"utf16":0},
        "focus":{"paragraphId":second,"utf16":6}
    });
    let mixed = apply(
        &mut engine,
        json!({"type":"set_selection","selection":selection}),
    );
    assert!(mixed["format"]["indentRightIn"].is_null());
    let updated = apply(
        &mut engine,
        json!({"type":"set_paragraph_metrics",
        "spacingAfterPt":18}),
    );
    assert_eq!(updated["paragraphs"][0]["format"]["indentRightPt"], 36.0);
    assert_eq!(updated["paragraphs"][1]["format"]["indentRightPt"], 0.0);
    assert!(updated["format"]["indentRightIn"].is_null());
}

#[test]
fn invalid_inputs_and_stale_edits_preserve_serialized_document() {
    let mut engine = BrowserEngine::new("bad-paragraph").unwrap();
    apply(&mut engine, json!({"type":"replace_text","text":"Words"}));
    let old = engine.export_document().unwrap();
    let before: Value = serde_json::from_str(&engine.snapshot()).unwrap();
    for input in [
        json!({"type":"set_paragraph_metrics","spacingBeforePt":-1}),
        json!({"type":"set_paragraph_metrics","lineSpacingMultiple":0}),
        json!({"type":"set_paragraph_metrics","indentLeftIn":999}),
        json!({"type":"set_paragraph_metrics","firstLineIn":-3}),
    ] {
        let failed: Value = serde_json::from_str(&engine.dispatch(&input.to_string())).unwrap();
        assert_eq!(failed["ok"], false);
        assert_eq!(engine.export_document().unwrap(), old);
    }
    let stale: Value = serde_json::from_str(
        &engine.dispatch(
            &json!({
                "type":"set_paragraph_metrics","spacingAfterPt":10,
                "expectedRevision":"stale-revision"
            })
            .to_string(),
        ),
    )
    .unwrap();
    assert_eq!(stale["ok"], false);
    let after: Value = serde_json::from_str(&engine.snapshot()).unwrap();
    assert_eq!(before, after);
}
