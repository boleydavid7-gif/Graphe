//! Section boundaries preserve all content and page geometry in atomic Rust edits.
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
fn reject(engine: &mut BrowserEngine, command: Value, code: &str) {
    let before = engine.snapshot();
    let native = engine.export_document().unwrap();
    let response: Value = serde_json::from_str(&engine.dispatch(&command.to_string())).unwrap();
    assert_eq!(response["ok"], false, "{response}");
    assert_eq!(response["error"]["code"], code, "{response}");
    assert_eq!(engine.snapshot(), before);
    assert_eq!(engine.export_document().unwrap(), native);
}
fn caret(id: &str, utf16: usize) -> Value {
    json!({"anchor":{"paragraphId":id,"utf16":utf16},
        "focus":{"paragraphId":id,"utf16":utf16}})
}
fn setup(section: &str, revision: &str) -> Value {
    json!({"type":"set_page_setup", "sectionId":section,
        "pageSize":{"width":20160,"height":12240,"orientation":"landscape"},
        "pageMargins":{"top":720,"right":1440,"bottom":720,"left":1440,
            "header":720,"footer":720,"gutter":0},
        "expectedRevision":revision})
}
fn saved(engine: &BrowserEngine) -> Value {
    serde_json::from_slice(&engine.export_document().unwrap()).unwrap()
}

#[test]
fn section_break_splits_body_text_and_preserves_ids_and_geometry_across_history() {
    let mut engine = BrowserEngine::new("8c2-sections").unwrap();
    let first = dispatch(&mut engine, json!({"type":"replace_text","text":"Alpha beta"}));
    let id = first["paragraphs"][0]["id"].as_str().unwrap().to_owned();
    let first_section = first["pageSetup"]["sectionId"].as_str().unwrap().to_owned();
    let broken = dispatch(&mut engine, json!({"type":"insert_section_break",
        "selection":caret(&id, 5),"expectedRevision":first["revision"]}));
    assert_eq!(broken["pageSetup"]["sectionIndex"], 1);
    assert_eq!(broken["pageSetup"]["sectionCount"], 2);
    assert_ne!(broken["pageSetup"]["sectionId"], first_section);
    assert_eq!(broken["paragraphs"][0]["id"], id);
    assert_eq!(broken["paragraphs"][0]["runs"][0]["text"], "Alpha");
    assert_eq!(broken["paragraphs"][1]["runs"][0]["text"], " beta");
    assert_eq!(saved(&engine)["document"]["sections"].as_array().unwrap().len(), 2);
    let pages: Value = serde_json::from_str(
        &engine.layout_snapshot("{}", broken["revision"].as_str()).unwrap()).unwrap();
    assert!(pages["layout"]["pages"].as_array().unwrap().len() >= 2);

    let second = broken["pageSetup"]["sectionId"].as_str().unwrap().to_owned();
    let altered = dispatch(&mut engine, setup(&second, broken["revision"].as_str().unwrap()));
    assert_eq!(altered["pageSetup"]["pageSize"]["width"], 20160);
    assert_eq!(altered["pageSetup"]["pageMargins"]["top"], 720);
    assert_eq!(saved(&engine)["document"]["sections"][0]["properties"], Value::Null);
    assert_eq!(saved(&engine)["document"]["sections"][1]["properties"]["page_size"]["width"], 20160);
    let pages: Value = serde_json::from_str(
        &engine.layout_snapshot("{}", altered["revision"].as_str()).unwrap()).unwrap();
    let page_list = pages["layout"]["pages"].as_array().unwrap();
    assert_eq!(page_list[0]["width"], 12240 * 64 / 20);
    assert_eq!(page_list[1]["width"], 20160 * 64 / 20);

    dispatch(&mut engine, json!({"type":"undo"}));
    assert_eq!(view(&engine)["pageSetup"]["pageSize"]["width"], 12240);
    dispatch(&mut engine, json!({"type":"undo"}));
    assert_eq!(view(&engine)["pageSetup"]["sectionCount"], 1);
    assert_eq!(view(&engine)["paragraphs"][0]["runs"][0]["text"], "Alpha beta");
    dispatch(&mut engine, json!({"type":"redo"}));
    assert_eq!(view(&engine)["pageSetup"]["sectionCount"], 2);
    assert_eq!(view(&engine)["paragraphs"][1]["runs"][0]["text"], " beta");
}

#[test]
fn removing_a_break_requires_layout_acknowledgement_and_retains_all_blocks() {
    let mut engine = BrowserEngine::new("8c2-removal").unwrap();
    let first = dispatch(&mut engine, json!({"type":"replace_text","text":"Before after"}));
    let id = first["paragraphs"][0]["id"].as_str().unwrap();
    let broken = dispatch(&mut engine, json!({"type":"insert_section_break",
        "selection":caret(id, 6)}));
    let second_id = broken["pageSetup"]["sectionId"].as_str().unwrap();
    let updated = dispatch(&mut engine, setup(second_id, broken["revision"].as_str().unwrap()));
    let revision = updated["revision"].as_str().unwrap();
    reject(&mut engine, json!({"type":"remove_section_break",
        "sectionId":second_id,"expectedRevision":revision}), "section_layout_confirmation_required");
    let joined = dispatch(&mut engine, json!({"type":"remove_section_break",
        "sectionId":second_id,"confirmLayoutChange":true,"expectedRevision":revision}));
    assert_eq!(joined["pageSetup"]["sectionCount"], 1);
    assert_eq!(joined["paragraphs"][0]["runs"][0]["text"], "Before");
    assert_eq!(joined["paragraphs"][1]["runs"][0]["text"], " after");
    assert_eq!(joined["pageSetup"]["pageSize"]["width"], 12240);
    assert_eq!(saved(&engine)["document"]["sections"].as_array().unwrap().len(), 1);
    dispatch(&mut engine, json!({"type":"undo"}));
    assert_eq!(view(&engine)["pageSetup"]["sectionCount"], 2);
    assert_eq!(view(&engine)["pageSetup"]["pageSize"]["width"], 20160);
}

#[test]
fn break_at_body_paragraph_start_moves_existing_paragraph_without_inserting_empty_text() {
    let mut engine = BrowserEngine::new("8c2-boundary").unwrap();
    let first = dispatch(&mut engine, json!({"type":"replace_text","text":"One\nTwo"}));
    assert_eq!(first["paragraphs"].as_array().unwrap().len(), 2);
    let first_id = first["paragraphs"][0]["id"].as_str().unwrap().to_owned();
    let second_id = first["paragraphs"][1]["id"].as_str().unwrap().to_owned();
    let next = dispatch(&mut engine, json!({"type":"insert_section_break",
        "selection":caret(&second_id, 0)}));
    assert_eq!(next["paragraphs"].as_array().unwrap().len(), 2);
    assert_eq!(next["paragraphs"][0]["id"], first_id);
    assert_eq!(next["paragraphs"][1]["id"], second_id);
    assert_eq!(next["pageSetup"]["sectionCount"], 2);
}

#[test]
fn refused_section_operations_leave_content_selection_and_history_unchanged() {
    let mut engine = BrowserEngine::new("8c2-refusal").unwrap();
    let initial = dispatch(&mut engine, json!({"type":"replace_text","text":"Hello"}));
    let id = initial["paragraphs"][0]["id"].as_str().unwrap();
    reject(&mut engine, json!({"type":"insert_section_break",
        "selection":caret(id, 0)}), "invalid_section_break");
    reject(&mut engine, json!({"type":"insert_section_break",
        "selection":{"anchor":{"paragraphId":id,"utf16":0},
            "focus":{"paragraphId":id,"utf16":2}}}), "invalid_section_break");
    reject(&mut engine, json!({"type":"remove_section_break",
        "sectionId":initial["pageSetup"]["sectionId"],
        "confirmLayoutChange":true}), "invalid_section_break");
    dispatch(&mut engine, json!({"type":"undo"}));
    assert_eq!(view(&engine)["canUndo"], false);
}
