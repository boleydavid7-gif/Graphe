//! Step 8C.1 page geometry and manual breaks use the existing transaction/history model.
use office_wasm::BrowserEngine;
use serde_json::{Value, json};

fn view(engine: &BrowserEngine) -> Value {
    serde_json::from_str(&engine.snapshot()).unwrap()
}
fn send(engine: &mut BrowserEngine, command: Value) -> Value {
    let response: Value = serde_json::from_str(&engine.dispatch(&command.to_string())).unwrap();
    assert_eq!(response["ok"], true, "{response}");
    response["view"].clone()
}
fn reject(engine: &mut BrowserEngine, command: Value) {
    let before = engine.snapshot();
    let native = engine.export_document().unwrap();
    let response: Value = serde_json::from_str(&engine.dispatch(&command.to_string())).unwrap();
    assert_eq!(response["ok"], false, "{response}");
    assert_eq!(engine.snapshot(), before);
    assert_eq!(engine.export_document().unwrap(), native);
}
fn setup(section_id: &str, width: i32, height: i32, revision: &str) -> Value {
    json!({
        "type": "set_page_setup",
        "sectionId": section_id,
        "pageSize": { "width": width, "height": height, "orientation": "landscape" },
        "pageMargins": { "top": 720, "right": 1080, "bottom": 900,
            "left": 1080, "header": 720, "footer": 720, "gutter": 0 },
        "expectedRevision": revision
    })
}

#[test]
fn page_settings_use_atomic_undo_and_preserve_native_geometry() {
    let mut engine = BrowserEngine::new("8c-page-settings").unwrap();
    let original = view(&engine);
    assert_eq!(original["pageSetup"]["pageSize"]["width"], 12240);
    assert_eq!(original["pageSetup"]["pageMargins"]["top"], 1440);
    let section = original["pageSetup"]["sectionId"].as_str().unwrap();
    let revision = original["revision"].as_str().unwrap();
    let updated = send(&mut engine, setup(section, 15840, 12240, revision));
    assert_eq!(updated["pageSetup"]["pageSize"]["orientation"], "landscape");
    assert_eq!(updated["pageSetup"]["pageSize"]["width"], 15840);
    assert_eq!(updated["pageSetup"]["pageMargins"]["top"], 720);
    let persisted: Value = serde_json::from_slice(&engine.export_document().unwrap()).unwrap();
    assert_eq!(
        persisted["document"]["sections"][0]["properties"]["page_size"]["width"],
        15840
    );
    assert_eq!(
        persisted["document"]["sections"][0]["properties"]["page_margins"]["left"],
        1080
    );

    send(&mut engine, json!({"type":"undo"}));
    assert_eq!(
        view(&engine)["pageSetup"]["pageSize"],
        original["pageSetup"]["pageSize"]
    );
    send(&mut engine, json!({"type":"redo"}));
    assert_eq!(view(&engine)["pageSetup"]["pageSize"]["width"], 15840);
    let stable = view(&engine);
    reject(
        &mut engine,
        setup(section, 3_000, 5_000, stable["revision"].as_str().unwrap()),
    );
    reject(
        &mut engine,
        setup(
            "absent-section",
            15840,
            12240,
            stable["revision"].as_str().unwrap(),
        ),
    );
    reject(&mut engine, setup(section, 15840, 12240, revision));
}

#[test]
fn manual_page_break_splits_at_caret_in_one_history_entry() {
    let mut engine = BrowserEngine::new("8c-manual-break").unwrap();
    let inserted = send(
        &mut engine,
        json!({"type":"replace_text", "text":"Alpha beta"}),
    );
    let para_id = inserted["paragraphs"][0]["id"].as_str().unwrap();
    let selection = json!({"anchor":{"paragraphId":para_id,"utf16":5},
        "focus":{"paragraphId":para_id,"utf16":5}});
    let broken = send(
        &mut engine,
        json!({"type":"insert_page_break",
        "selection":selection, "expectedRevision":inserted["revision"]}),
    );
    assert_eq!(broken["paragraphs"].as_array().unwrap().len(), 2);
    assert_eq!(broken["paragraphs"][0]["runs"][0]["text"], "Alpha");
    assert_eq!(broken["paragraphs"][1]["runs"][0]["text"], " beta");
    assert_eq!(broken["paragraphs"][1]["format"]["pageBreakBefore"], true);
    assert_eq!(broken["selection"]["focus"]["utf16"], 0);
    let revision = broken["revision"].as_str().unwrap();
    let layout: Value =
        serde_json::from_str(&engine.layout_snapshot("{}", Some(revision)).unwrap()).unwrap();
    assert!(layout["layout"]["pages"].as_array().unwrap().len() >= 2);

    send(&mut engine, json!({"type":"undo"}));
    assert_eq!(view(&engine)["paragraphs"].as_array().unwrap().len(), 1);
    assert_eq!(
        view(&engine)["paragraphs"][0]["runs"][0]["text"],
        "Alpha beta"
    );
    send(&mut engine, json!({"type":"redo"}));
    assert_eq!(
        view(&engine)["paragraphs"][1]["format"]["pageBreakBefore"],
        true
    );
    let second = view(&engine)["paragraphs"][1]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    send(
        &mut engine,
        json!({"type":"set_page_break_before", "enabled":false,
        "selection":{"anchor":{"paragraphId":second,"utf16":0},
            "focus":{"paragraphId":second,"utf16":0}}}),
    );
    assert_eq!(
        view(&engine)["paragraphs"][1]["format"]["pageBreakBefore"],
        false
    );
}

#[test]
fn invalid_page_break_selection_is_nonmutating() {
    let mut engine = BrowserEngine::new("8c-page-break-rejection").unwrap();
    let before = send(&mut engine, json!({"type":"replace_text","text":"Hello"}));
    let id = before["paragraphs"][0]["id"].as_str().unwrap();
    reject(
        &mut engine,
        json!({"type":"insert_page_break",
        "selection":{"anchor":{"paragraphId":id,"utf16":0},
            "focus":{"paragraphId":id,"utf16":2}}}),
    );
}
