//! Native checks for Rust-authoritative touch word selection and endpoint drag.

use office_core::{
    Block, CharacterFormat, Document, DocumentMetadata, Id, Paragraph, Section, TextRun,
    serialize_document,
};
use office_layout::{CaretAffinity, DocumentLayout, LayoutPosition};
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

fn layout(engine: &mut BrowserEngine) -> DocumentLayout {
    let value: Value = serde_json::from_str(&engine.layout_snapshot("{}", None).unwrap()).unwrap();
    serde_json::from_value(value["layout"].clone()).unwrap()
}

fn position(paragraph_id: &str, utf16: usize, affinity: CaretAffinity) -> LayoutPosition {
    LayoutPosition {
        paragraph_id: paragraph_id.into(),
        utf16,
        affinity,
    }
}

fn open_document(document: &Document) -> BrowserEngine {
    let mut engine = BrowserEngine::new("touch-selection-native").unwrap();
    let result: Value =
        serde_json::from_str(&engine.open_document(&serialize_document(document).unwrap()))
            .unwrap();
    assert_eq!(result["ok"], true, "{result}");
    engine
}

fn paragraph_engine(runs: Vec<TextRun>) -> BrowserEngine {
    let document = Document::from_parts(
        Id::new("touch-document").unwrap(),
        DocumentMetadata::default(),
        vec![Section::new(
            Id::new("touch-section").unwrap(),
            vec![Block::Paragraph(Paragraph::new(
                Id::new("p").unwrap(),
                runs,
            ))],
        )],
        vec![],
    )
    .unwrap();
    open_document(&document)
}

/// A point inside an actual shaped grapheme cell, including right-to-left cells.
fn cell(layout: &DocumentLayout, paragraph: &str, start: usize, end: usize, fourths: i64) -> Value {
    let start = layout
        .caret_geometry(&position(paragraph, start, CaretAffinity::Leading))
        .unwrap();
    let end = layout
        .caret_geometry(&position(paragraph, end, CaretAffinity::Trailing))
        .unwrap();
    assert_eq!(start.page_number, end.page_number);
    assert_eq!(start.line_index, end.line_index);
    json!({
        "pageNumber": start.page_number,
        "x": start.rect.x + (end.rect.x - start.rect.x) * fourths / 4,
        "y": start.rect.y + start.rect.height / 2,
    })
}

fn select_cell(engine: &mut BrowserEngine, start: usize, end: usize, fourths: i64) -> Value {
    let mut point = cell(&layout(engine), "p", start, end, fourths);
    point["type"] = json!("select_word");
    action(engine, point)
}

fn assert_read_only(engine: &BrowserEngine, before: &Value, bytes: &[u8]) {
    let after = snapshot(engine);
    for field in [
        "sessionId",
        "documentId",
        "revision",
        "canUndo",
        "canRedo",
        "paragraphs",
    ] {
        assert_eq!(after[field], before[field], "selection changed {field}");
    }
    assert_eq!(engine.export_document().unwrap(), bytes);
}

#[test]
fn word_selection_uses_complete_paragraphs_across_runs_and_combining_marks() {
    let mut engine = paragraph_engine(vec![
        TextRun::new("Ca"),
        TextRun {
            text: "fe".into(),
            format: CharacterFormat {
                bold: Some(true),
                ..Default::default()
            },
        },
        TextRun::new("\u{301} don't l’été 12.5 office"),
    ]);
    let before = snapshot(&engine);
    let bytes = engine.export_document().unwrap();
    let result = select_cell(&mut engine, 3, 5, 1);
    assert_eq!(engine.selected_text().unwrap(), "Cafe\u{301}");
    assert_eq!(result["view"]["selection"]["anchor"]["utf16"], 0);
    assert_eq!(result["view"]["selection"]["focus"]["utf16"], 5);
    assert_eq!(result["geometry"]["focus"]["position"]["grapheme"], 4);
    for (start, end, expected) in [
        (7, 8, "don't"),
        (13, 14, "l’été"),
        (19, 20, "12.5"),
        (24, 25, "office"),
    ] {
        select_cell(&mut engine, start, end, 2);
        assert_eq!(engine.selected_text().unwrap(), expected);
    }
    assert_read_only(&engine, &before, &bytes);
}

#[test]
fn word_final_glyph_selects_the_same_word_on_both_sides_of_its_midpoint() {
    let mut engine = paragraph_engine(vec![TextRun::new("last next")]);
    for fourths in [1, 3] {
        select_cell(&mut engine, 3, 4, fourths);
        assert_eq!(engine.selected_text().unwrap(), "last");
    }
    select_cell(&mut engine, 4, 5, 2);
    assert_eq!(engine.selected_text().unwrap(), " ");
}

#[test]
fn emoji_punctuation_and_whitespace_fallback_select_one_whole_grapheme() {
    let mut engine = paragraph_engine(vec![TextRun::new("A 👩🏽‍💻!  e\u{301}🇺🇳")]);
    let before = engine.export_document().unwrap();
    for (start, end, expected) in [
        (2, 9, "👩🏽‍💻"),
        (9, 10, "!"),
        (10, 11, " "),
        (11, 12, " "),
        (12, 14, "e\u{301}"),
        (14, 18, "🇺🇳"),
    ] {
        for fourths in [1, 3] {
            let selected = select_cell(&mut engine, start, end, fourths);
            assert_eq!(engine.selected_text().unwrap(), expected, "{start}..{end}");
            assert_eq!(selected["view"]["selection"]["anchor"]["utf16"], start);
            assert_eq!(selected["view"]["selection"]["focus"]["utf16"], end);
        }
    }
    assert_eq!(engine.export_document().unwrap(), before);
}

#[test]
fn arabic_hebrew_devanagari_and_cjk_follow_pinned_unicode_word_rules() {
    for (text, start, end, expected) in [
        ("العربية", 1, 2, "العربية"),
        ("שלום", 1, 2, "שלום"),
        ("नमस्ते", 0, 1, "नमस्ते"),
        ("中文", 1, 2, "文"),
    ] {
        let mut engine = paragraph_engine(vec![TextRun::new(text)]);
        let bytes = engine.export_document().unwrap();
        select_cell(&mut engine, start, end, 2);
        assert_eq!(engine.selected_text().unwrap(), expected);
        assert_eq!(engine.export_document().unwrap(), bytes);
    }
}

#[test]
fn bidi_endpoint_drags_preserve_the_fixed_logical_endpoint_and_affinity() {
    let mut engine = paragraph_engine(vec![TextRun::new("Hello שלום world")]);
    let selected = select_cell(&mut engine, 7, 8, 2);
    assert_eq!(engine.selected_text().unwrap(), "שלום");
    let bytes = engine.export_document().unwrap();
    let before = snapshot(&engine);
    let mut target = cell(&layout(&mut engine), "p", 12, 13, 2);
    target["type"] = json!("drag_endpoint");
    target["endpoint"] = json!("anchor");
    let dragged = action(&mut engine, target);
    assert_eq!(
        dragged["view"]["selection"]["focus"],
        selected["view"]["selection"]["focus"]
    );
    assert_eq!(dragged["geometry"]["focus"], selected["geometry"]["focus"]);
    assert!(
        dragged["view"]["selection"]["anchor"]["utf16"]
            .as_u64()
            .unwrap()
            > 10
    );
    assert_eq!(
        dragged["geometry"]["anchor"]["position"]["utf16"],
        dragged["view"]["selection"]["anchor"]["utf16"]
    );
    assert_read_only(&engine, &before, &bytes);
}

#[test]
fn dragging_both_endpoints_can_cross_pages_sections_and_each_other_without_swapping() {
    let mut engine = BrowserEngine::new("touch-multiple-pages").unwrap();
    let response: Value = serde_json::from_str(&engine.open_document(include_bytes!(
        "../../../tests/fixtures/layout/layout-v1.writer"
    )))
    .unwrap();
    assert_eq!(response["ok"], true);
    let layout = layout(&mut engine);
    assert_eq!(layout.pages.len(), 4);
    let first = &layout.pages[0].lines[0].carets[0];
    action(
        &mut engine,
        json!({ "type":"select_word", "pageNumber":1, "x":first.x, "y":first.y }),
    );
    let initial = snapshot(&engine);
    let bytes = engine.export_document().unwrap();
    let last = layout.pages[3]
        .lines
        .last()
        .unwrap()
        .carets
        .iter()
        .max_by_key(|c| c.utf16)
        .unwrap();
    let extended = action(
        &mut engine,
        json!({
            "type":"drag_endpoint", "endpoint":"focus", "pageNumber":4, "x":last.x, "y":last.y,
        }),
    );
    assert_eq!(
        extended["view"]["selection"]["anchor"],
        initial["selection"]["anchor"]
    );
    let pages: std::collections::BTreeSet<_> = extended["geometry"]["rectangles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|rectangle| rectangle["pageNumber"].as_u64().unwrap())
        .collect();
    assert_eq!(pages, [1, 2, 3, 4].into_iter().collect());
    assert!(engine.selected_text().unwrap().contains("A4 section"));
    let collapsed = action(
        &mut engine,
        json!({
            "type":"drag_endpoint", "endpoint":"anchor", "pageNumber":4, "x":last.x, "y":last.y,
        }),
    );
    assert_eq!(
        collapsed["view"]["selection"]["anchor"],
        collapsed["view"]["selection"]["focus"]
    );
    assert_eq!(collapsed["geometry"]["rectangles"], json!([]));
    let reverse = action(
        &mut engine,
        json!({
            "type":"drag_endpoint", "endpoint":"focus", "pageNumber":1, "x":first.x, "y":first.y,
        }),
    );
    assert_eq!(
        reverse["view"]["selection"]["anchor"],
        collapsed["view"]["selection"]["anchor"]
    );
    assert_eq!(
        reverse["geometry"]["anchor"],
        collapsed["geometry"]["anchor"]
    );
    assert!(engine.selected_text().unwrap().contains("A4 section"));
    assert_read_only(&engine, &initial, &bytes);
}

#[test]
fn word_selection_resolves_whole_paragraph_offsets_on_page_continuations() {
    let text = "continuation ".repeat(1_000);
    let mut engine = paragraph_engine(vec![TextRun::new(&text)]);
    let layout = layout(&mut engine);
    assert!(layout.pages.len() > 1);
    let line = &layout.pages[1].lines[0];
    let letter = line.text_range.start_utf16 + line.text.find('o').unwrap();
    let mut point = cell(&layout, "p", letter, letter + 1, 2);
    point["type"] = json!("select_word");
    let selected = action(&mut engine, point);
    assert_eq!(engine.selected_text().unwrap(), "continuation");
    assert!(
        selected["view"]["selection"]["anchor"]["utf16"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert_eq!(selected["geometry"]["anchor"]["pageNumber"], 2);
    assert_eq!(selected["geometry"]["focus"]["pageNumber"], 2);
}

#[test]
fn empty_documents_and_empty_paragraphs_do_not_invent_text() {
    let empty = Document::new(Id::new("empty-touch-document").unwrap()).unwrap();
    let mut engine = open_document(&empty);
    let bytes = engine.export_document().unwrap();
    let result = action(
        &mut engine,
        json!({"type":"select_word", "pageNumber":1, "x":0, "y":0}),
    );
    assert!(result["view"]["selection"].is_null());
    assert!(result["geometry"].is_null());
    assert_eq!(engine.export_document().unwrap(), bytes);
    let mut engine = paragraph_engine(vec![]);
    let bytes = engine.export_document().unwrap();
    let result = action(
        &mut engine,
        json!({"type":"select_word", "pageNumber":1, "x":0, "y":0}),
    );
    assert_eq!(result["view"]["selection"]["anchor"]["utf16"], 0);
    assert_eq!(
        result["view"]["selection"]["anchor"],
        result["view"]["selection"]["focus"]
    );
    assert_eq!(engine.export_document().unwrap(), bytes);
}

#[test]
fn word_and_endpoint_actions_leave_existing_redo_history_available() {
    let mut engine = paragraph_engine(vec![TextRun::new("One two")]);
    dispatch(&mut engine, json!({"type":"replace_text", "text":"!"}));
    dispatch(&mut engine, json!({"type":"undo"}));
    let before = snapshot(&engine);
    assert_eq!(before["canRedo"], true);
    let bytes = engine.export_document().unwrap();
    select_cell(&mut engine, 1, 2, 2);
    assert_eq!(engine.selected_text().unwrap(), "One");
    let end = layout(&mut engine)
        .caret_geometry(&position("p", 7, CaretAffinity::Trailing))
        .unwrap();
    action(
        &mut engine,
        json!({"type":"drag_endpoint", "endpoint":"focus", "pageNumber":end.page_number,
            "x":end.rect.x, "y":end.rect.y}),
    );
    assert_eq!(engine.selected_text().unwrap(), "One two");
    assert_read_only(&engine, &before, &bytes);
    let redone = dispatch(&mut engine, json!({"type":"redo"}));
    assert_eq!(redone["canRedo"], false);
    let text: String = redone["paragraphs"][0]["runs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|run| run["text"].as_str().unwrap())
        .collect();
    assert_eq!(text, "!One two");
}

#[test]
fn malformed_stale_and_missing_selection_actions_preserve_content_and_runtime_geometry() {
    let mut engine = paragraph_engine(vec![TextRun::new("Safe 👩🏽‍💻 selected text")]);
    let old = snapshot(&engine)["revision"].as_str().unwrap().to_owned();
    dispatch(&mut engine, json!({"type":"replace_text", "text":"!"}));
    let selected = select_cell(&mut engine, 1, 2, 2);
    let before = engine.snapshot();
    let bytes = engine.export_document().unwrap();
    let geometry = engine.selection_geometry("{}", None).unwrap();
    let valid = json!({"type":"drag_endpoint", "endpoint":"anchor", "pageNumber":1, "x":0, "y":0});
    assert_eq!(
        engine
            .layout_action("{}", &valid.to_string(), Some(&old))
            .unwrap_err()
            .code,
        "stale_layout"
    );
    for invalid in [
        json!({"type":"select_word", "pageNumber":0, "x":0, "y":0}),
        json!({"type":"select_word", "pageNumber":1, "x":i64::MAX, "y":0}),
        json!({"type":"select_word", "pageNumber":1, "x":0, "y":0, "future":true}),
        json!({"type":"drag_endpoint", "endpoint":"start", "pageNumber":1, "x":0, "y":0}),
        json!({"type":"drag_endpoint", "endpoint":"focus", "pageNumber":1, "x":0, "y":i64::MIN}),
        json!({"type":"drag_endpoint", "endpoint":"focus", "pageNumber":1, "x":0.5, "y":0}),
    ] {
        assert!(
            engine
                .layout_action("{}", &invalid.to_string(), None)
                .is_err()
        );
        assert_eq!(engine.snapshot(), before);
        assert_eq!(engine.export_document().unwrap(), bytes);
        assert_eq!(engine.selection_geometry("{}", None).unwrap(), geometry);
    }
    assert_eq!(
        snapshot(&engine)["selection"],
        selected["view"]["selection"]
    );
    dispatch(
        &mut engine,
        json!({"type":"set_selection", "selection":null}),
    );
    let no_selection = engine.snapshot();
    assert_eq!(
        engine
            .layout_action("{}", &valid.to_string(), None)
            .unwrap_err()
            .code,
        "invalid_selection"
    );
    assert_eq!(engine.snapshot(), no_selection);
    assert_eq!(engine.export_document().unwrap(), bytes);
}
