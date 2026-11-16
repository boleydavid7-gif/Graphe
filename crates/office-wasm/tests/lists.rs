//! Native/WASM semantic list behavior, history and paragraph structure.

use office_core::{ListKind, deserialize_writer_file};
use office_wasm::BrowserEngine;
use serde_json::{Value, json};

fn command(engine: &mut BrowserEngine, input: Value) -> Value {
    let result: Value = serde_json::from_str(&engine.dispatch(&input.to_string())).unwrap();
    assert_eq!(result["ok"], true, "{result}");
    result["view"].clone()
}

fn paragraphs(view: &Value) -> Vec<(String, Option<String>)> {
    view["paragraphs"].as_array().unwrap().iter().map(|paragraph| {
        let text = paragraph["runs"].as_array().unwrap().iter()
            .map(|run| run["text"].as_str().unwrap()).collect::<String>();
        (text, paragraph["list"].as_str().map(str::to_string))
    }).collect()
}

#[test]
fn bullet_enter_inherits_and_empty_enter_exits_with_one_undo() {
    let mut engine = BrowserEngine::new("bullet-lists").unwrap();
    command(&mut engine, json!({"type":"set_list","list":"bullet"}));
    command(&mut engine, json!({"type":"replace_text","text":"First"}));
    let split = command(&mut engine, json!({"type":"insert_paragraph"}));
    assert_eq!(paragraphs(&split), vec![
        ("First".into(), Some("bullet".into())),
        ("".into(), Some("bullet".into())),
    ]);
    command(&mut engine, json!({"type":"replace_text","text":"Second"}));
    let split = command(&mut engine, json!({"type":"insert_paragraph"}));
    assert_eq!(split["format"]["list"], "bullet");
    let exited = command(&mut engine, json!({"type":"insert_paragraph"}));
    assert_eq!(paragraphs(&exited), vec![
        ("First".into(), Some("bullet".into())),
        ("Second".into(), Some("bullet".into())),
        ("".into(), None),
    ]);
    let undone = command(&mut engine, json!({"type":"undo"}));
    assert_eq!(paragraphs(&undone)[2].1.as_deref(), Some("bullet"));
    let redone = command(&mut engine, json!({"type":"redo"}));
    assert_eq!(paragraphs(&redone)[2].1, None);
    let saved = engine.export_document().unwrap();
    let restored = deserialize_writer_file(&saved).unwrap();
    let lists: Vec<_> = restored.document().sections()[0].blocks().iter()
        .map(|block| { let office_core::Block::Paragraph(p) = block; p.list() })
        .collect();
    assert_eq!(lists, vec![Some(ListKind::Bullet), Some(ListKind::Bullet), None]);
}

#[test]
fn selected_paragraphs_toggle_numbering_without_changing_words() {
    let mut engine = BrowserEngine::new("numbered-lists").unwrap();
    let created = command(&mut engine, json!({"type":"replace_text","text":"one\ntwo\nthree"}));
    let first = created["paragraphs"][0]["id"].as_str().unwrap();
    let second = created["paragraphs"][1]["id"].as_str().unwrap();
    let selected = json!({"anchor":{"paragraphId":first,"utf16":0},
        "focus":{"paragraphId":second,"utf16":3}});
    let before_count = created["wordCount"];
    let changed = command(&mut engine, json!({
        "type":"set_list","list":"numbered","selection":selected
    }));
    assert_eq!(changed["wordCount"], before_count);
    assert_eq!(paragraphs(&changed), vec![
        ("one".into(), Some("numbered".into())),
        ("two".into(), Some("numbered".into())),
        ("three".into(), None),
    ]);
    command(&mut engine, json!({"type":"undo"}));
    assert_eq!(paragraphs(&serde_json::from_str::<Value>(&engine.snapshot()).unwrap())[0].1, None);
}
