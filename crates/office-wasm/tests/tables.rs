//! Table editing uses stable Rust identities, validated transactions and history.

use office_core::{Block, deserialize_writer_file};
use office_wasm::BrowserEngine;
use serde_json::{Value, json};

fn command(engine: &mut BrowserEngine, input: Value) -> Value {
    let response: Value = serde_json::from_str(&engine.dispatch(&input.to_string())).unwrap();
    assert_eq!(response["ok"], true, "{response}");
    response["view"].clone()
}

fn view(engine: &BrowserEngine) -> Value {
    serde_json::from_str(&engine.snapshot()).unwrap()
}
fn selected(paragraph: &str, utf16: usize) -> Value {
    json!({"anchor":{"paragraphId":paragraph,"utf16":utf16},"focus":{"paragraphId":paragraph,"utf16":utf16}})
}

#[test]
fn table_typing_navigation_history_and_native_reopen_are_exact() {
    let mut engine = BrowserEngine::new("table-editing").unwrap();
    command(
        &mut engine,
        json!({"type":"replace_text","text":"Before table"}),
    );
    let created = command(
        &mut engine,
        json!({"type":"insert_table","rows":2,"columns":2}),
    );
    assert_eq!(created["paragraphs"].as_array().unwrap().len(), 6);
    assert_eq!(created["table"]["rowCount"], 2);
    assert_eq!(created["table"]["columnCount"], 2);
    assert_eq!(created["table"]["row"], 0);
    assert_eq!(created["table"]["column"], 0);
    let edited = command(&mut engine, json!({"type":"replace_text","text":"é👩🏽‍💻"}));
    let first = edited["selection"]["focus"]["paragraphId"]
        .as_str()
        .unwrap()
        .to_owned();
    command(
        &mut engine,
        json!({"type":"format_character","bold":true,"selection":{
        "anchor":{"paragraphId":first,"utf16":0},"focus":{"paragraphId":first,"utf16":9}}}),
    );
    let before = view(&engine);
    let moved = command(
        &mut engine,
        json!({"type":"move_table_cell","direction":"forward"}),
    );
    assert_eq!(moved["revision"], before["revision"]);
    assert_eq!(moved["table"]["column"], 1);
    command(
        &mut engine,
        json!({"type":"replace_text","text":"Second cell"}),
    );
    let undone = command(&mut engine, json!({"type":"undo"}));
    assert!(
        undone["paragraphs"][2]["runs"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let redone = command(&mut engine, json!({"type":"redo"}));
    assert_eq!(redone["paragraphs"][2]["runs"][0]["text"], "Second cell");
    let bytes = engine.export_document().unwrap();
    let file = deserialize_writer_file(&bytes).unwrap();
    let Block::Table(table) = &file.document().sections()[0].blocks()[1] else {
        panic!("missing table")
    };
    assert_eq!(
        table.rows()[0].cells()[0].paragraphs()[0].id().as_str(),
        first
    );
    assert_eq!(
        table.rows()[0].cells()[0].paragraphs()[0].runs()[0]
            .format
            .bold,
        Some(true)
    );
    let opened: Value = serde_json::from_str(&engine.open_document(&bytes)).unwrap();
    assert_eq!(opened["ok"], true);
    assert_eq!(engine.export_document().unwrap(), bytes);
}

#[test]
fn table_structure_commands_are_single_atomic_history_steps_and_stale_safe() {
    let mut engine = BrowserEngine::new("table-structure").unwrap();
    let initial = engine.export_document().unwrap();
    let created = command(
        &mut engine,
        json!({"type":"insert_table","rows":2,"columns":3}),
    );
    let table_id = created["table"]["tableId"].clone();
    let before = engine.export_document().unwrap();
    let expanded = command(
        &mut engine,
        json!({"type":"table_action","action":"insert_row_below"}),
    );
    assert_eq!(expanded["table"]["rowCount"], 3);
    assert_eq!(expanded["table"]["row"], 1);
    assert_eq!(expanded["table"]["tableId"], table_id);
    command(&mut engine, json!({"type":"undo"}));
    assert_eq!(engine.export_document().unwrap(), before);
    command(&mut engine, json!({"type":"redo"}));
    let wider = command(
        &mut engine,
        json!({"type":"table_action","action":"insert_column_after"}),
    );
    assert_eq!(wider["table"]["columnCount"], 4);
    command(
        &mut engine,
        json!({"type":"table_action","action":"delete_column"}),
    );
    let fewer = command(
        &mut engine,
        json!({"type":"table_action","action":"delete_row"}),
    );
    assert_eq!(fewer["table"]["rowCount"], 2);
    assert_eq!(fewer["table"]["columnCount"], 3);
    let snapshot = engine.snapshot();
    let rejected: Value = serde_json::from_str(
        &engine.dispatch(
            &json!({
                "type":"table_action","action":"delete_table","expectedRevision":created["revision"]
            })
            .to_string(),
        ),
    )
    .unwrap();
    assert_eq!(rejected["error"]["code"], "stale_revision");
    assert_eq!(engine.snapshot(), snapshot);
    command(
        &mut engine,
        json!({"type":"table_action","action":"delete_table"}),
    );
    assert!(
        view(&engine)["paragraphs"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["tableCell"].is_null())
    );
    // The explicit trailing body paragraph is kept after deleting a table.
    assert_ne!(engine.export_document().unwrap(), initial);
}

#[test]
fn cell_boundaries_never_merge_and_cross_cell_replace_is_rejected() {
    let mut engine = BrowserEngine::new("table-boundaries").unwrap();
    let created = command(
        &mut engine,
        json!({"type":"insert_table","rows":1,"columns":2}),
    );
    let first = created["selection"]["focus"]["paragraphId"]
        .as_str()
        .unwrap()
        .to_owned();
    let second = created["paragraphs"][2]["id"].as_str().unwrap().to_owned();
    command(
        &mut engine,
        json!({"type":"replace_text","text":"Cell one"}),
    );
    command(
        &mut engine,
        json!({"type":"set_selection","selection":selected(&first,0)}),
    );
    let before = engine.export_document().unwrap();
    command(&mut engine, json!({"type":"delete","direction":"backward"}));
    assert_eq!(engine.export_document().unwrap(), before);
    let rejected: Value = serde_json::from_str(&engine.dispatch(&json!({"type":"replace_text","text":"bad",
        "selection":{"anchor":{"paragraphId":first,"utf16":0},"focus":{"paragraphId":second,"utf16":0}}
    }).to_string())).unwrap();
    assert_eq!(rejected["ok"], false);
    assert_eq!(engine.export_document().unwrap(), before);
    // A body-to-body selection enclosing a table is also structural: it must
    // never flatten or silently remove the cells between its endpoints.
    let before_body = created["paragraphs"][0]["id"].as_str().unwrap();
    let after_body = created["paragraphs"].as_array().unwrap().last().unwrap()["id"]
        .as_str()
        .unwrap();
    let across: Value = serde_json::from_str(&engine.dispatch(&json!({"type":"replace_text","text":"bad",
        "selection":{"anchor":{"paragraphId":before_body,"utf16":0},"focus":{"paragraphId":after_body,"utf16":0}}
    }).to_string())).unwrap();
    assert_eq!(across["ok"], false);
    assert_eq!(engine.export_document().unwrap(), before);
    let invalid: Value = serde_json::from_str(
        &engine.dispatch(&json!({"type":"insert_table","rows":21,"columns":1}).to_string()),
    )
    .unwrap();
    assert_eq!(invalid["error"]["code"], "invalid_table");
    assert_eq!(engine.export_document().unwrap(), before);
}

#[test]
fn enter_splits_inside_cell_and_tab_leaves_last_cell_without_adding_rows() {
    let mut engine = BrowserEngine::new("table-enter").unwrap();
    command(
        &mut engine,
        json!({"type":"insert_table","rows":1,"columns":1}),
    );
    command(
        &mut engine,
        json!({"type":"replace_text","text":"First\nSecond"}),
    );
    let file = deserialize_writer_file(&engine.export_document().unwrap()).unwrap();
    let Block::Table(table) = &file.document().sections()[0].blocks()[1] else {
        panic!("missing table")
    };
    assert_eq!(table.rows()[0].cells()[0].paragraphs().len(), 2);
    let before = view(&engine);
    let exited = command(
        &mut engine,
        json!({"type":"move_table_cell","direction":"forward"}),
    );
    assert!(exited["table"].is_null());
    assert_eq!(exited["revision"], before["revision"]);
    assert_eq!(
        engine.export_document().unwrap(),
        office_core::serialize_writer_file(&file).unwrap()
    );
}

#[test]
fn physical_column_resizing_preserves_total_and_reopens_with_exact_history() {
    let mut engine = BrowserEngine::new("column-resize").unwrap();
    command(
        &mut engine,
        json!({"type":"insert_table","rows":2,"columns":2}),
    );
    command(
        &mut engine,
        json!({"type":"table_action","action":"widen_column"}),
    );
    let bytes = engine.export_document().unwrap();
    let file = deserialize_writer_file(&bytes).unwrap();
    let Block::Table(table) = &file.document().sections()[0].blocks()[1] else {
        panic!("table missing")
    };
    let widths = table.column_widths().unwrap();
    assert_eq!(widths[0].0 - widths[1].0, 480);
    let total: i32 = widths.iter().map(|w| w.0).sum();
    command(
        &mut engine,
        json!({"type":"table_action","action":"equalize_columns"}),
    );
    let equal = engine.export_document().unwrap();
    let file = deserialize_writer_file(&equal).unwrap();
    let Block::Table(table) = &file.document().sections()[0].blocks()[1] else {
        panic!("table missing")
    };
    assert_eq!(
        table
            .column_widths()
            .unwrap()
            .iter()
            .map(|w| w.0)
            .sum::<i32>(),
        total
    );
    command(&mut engine, json!({"type":"undo"}));
    assert_eq!(engine.export_document().unwrap(), bytes);
    command(&mut engine, json!({"type":"redo"}));
    assert_eq!(engine.export_document().unwrap(), equal);
    let mut reopened = BrowserEngine::new("resize-reopened").unwrap();
    assert!(
        serde_json::from_str::<Value>(&reopened.open_document(&equal)).unwrap()["ok"]
            .as_bool()
            .unwrap()
    );
    assert_eq!(reopened.export_document().unwrap(), equal);
}
