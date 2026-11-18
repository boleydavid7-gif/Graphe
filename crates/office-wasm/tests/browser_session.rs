//! End-to-end native tests of the browser command and snapshot boundary.

use office_core::{
    Block, CharacterFormat, Document, DocumentMetadata, Id, NamedStyle, Paragraph, Section,
    TextRun, Underline, deserialize_document, serialize_document,
};
use office_wasm::BrowserEngine;
use serde_json::{Value, json};

fn view(engine: &BrowserEngine) -> Value {
    serde_json::from_str(&engine.snapshot()).unwrap()
}
fn execute(engine: &mut BrowserEngine, command: Value) -> Value {
    let result: Value = serde_json::from_str(&engine.dispatch(&command.to_string())).unwrap();
    assert_eq!(result["ok"], true, "{result}");
    result["view"].clone()
}
fn text(view: &Value) -> Vec<String> {
    view["paragraphs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            p["runs"]
                .as_array()
                .unwrap()
                .iter()
                .map(|run| run["text"].as_str().unwrap())
                .collect()
        })
        .collect()
}
fn point(id: &str, offset: usize) -> Value {
    json!({"paragraphId":id,"utf16":offset})
}
fn selection(id: &str, anchor: usize, focus: usize) -> Value {
    json!({"anchor":point(id,anchor),"focus":point(id,focus)})
}
fn first_id(engine: &BrowserEngine) -> String {
    view(engine)["paragraphs"][0]["id"].as_str().unwrap().into()
}
fn id(value: &str) -> Id {
    Id::new(value).unwrap()
}

#[test]
fn creation_revision_transport_and_single_transaction_multiline_history() {
    let mut engine = BrowserEngine::new("test").unwrap();
    let original = engine.export_document().unwrap();
    let initial = view(&engine);
    assert!(initial["revision"].is_string());
    assert!(initial["sessionId"].is_string());
    let inserted = execute(
        &mut engine,
        json!({"type":"replace_text","text":"one\r\ntwo\rthree"}),
    );
    assert_eq!(text(&inserted), vec!["one", "two", "three"]);
    assert_eq!(inserted["selection"]["focus"]["utf16"], 5);
    execute(&mut engine, json!({"type":"undo"}));
    assert_eq!(engine.export_document().unwrap(), original);
    assert_eq!(view(&engine)["canUndo"], false);
    execute(&mut engine, json!({"type":"redo"}));
    assert_eq!(text(&view(&engine)), vec!["one", "two", "three"]);
}

#[test]
fn stale_invalid_and_unknown_commands_preserve_document_and_history() {
    let mut engine = BrowserEngine::new("test").unwrap();
    let old = view(&engine)["revision"].clone();
    execute(&mut engine, json!({"type":"replace_text","text":"hello"}));
    let before = engine.snapshot();
    for command in [
        json!({"type":"replace_text","text":"bad","expectedRevision":old}),
        json!({"type":"undo","secret":true}),
        json!({"type":"new_document","secret":true}),
        json!({"type":"replace_text","text":"bad","selection":selection("missing",0,0)}),
        json!({"type":"format_character","fontSizePt":0}),
        json!({"type":"format_character","fontFamily":" "}),
    ] {
        let response: Value = serde_json::from_str(&engine.dispatch(&command.to_string())).unwrap();
        assert_eq!(response["ok"], false, "{response}");
        assert_eq!(engine.snapshot(), before);
    }
}

#[test]
fn grapheme_deletion_caret_affinity_and_scalar_boundaries() {
    let mut engine = BrowserEngine::new("unicode").unwrap();
    let id = first_id(&engine);
    execute(
        &mut engine,
        json!({"type":"replace_text","text":"e\u{301}👩🏽‍💻🇺🇳"}),
    );
    assert_eq!(view(&engine)["selection"]["focus"]["utf16"], 13);
    execute(&mut engine, json!({"type":"delete","direction":"backward"}));
    assert_eq!(text(&view(&engine)), vec!["e\u{301}👩🏽‍💻"]);
    execute(&mut engine, json!({"type":"delete","direction":"backward"}));
    assert_eq!(text(&view(&engine)), vec!["e\u{301}"]);
    execute(
        &mut engine,
        json!({"type":"set_selection","selection":selection(&id,1,1)}),
    );
    assert_eq!(view(&engine)["selection"]["focus"]["utf16"], 2);
    execute(&mut engine, json!({"type":"replace_text","text":"👩"}));
    let before = engine.snapshot();
    let result: Value = serde_json::from_str(
        &engine
            .dispatch(&json!({"type":"set_selection","selection":selection(&id,3,3)}).to_string()),
    )
    .unwrap();
    assert_eq!(result["ok"], false);
    assert_eq!(engine.snapshot(), before);
    execute(
        &mut engine,
        json!({"type":"set_selection","selection":selection(&id,2,2)}),
    );
    execute(&mut engine, json!({"type":"replace_text","text":"\u{301}"}));
    assert_eq!(view(&engine)["selection"]["focus"]["utf16"], 3);
}

#[test]
fn directional_selection_snaps_outward_and_formatting_is_mixed() {
    let mut engine = BrowserEngine::new("test").unwrap();
    let id = first_id(&engine);
    execute(
        &mut engine,
        json!({"type":"replace_text","text":"Ae\u{301}Z"}),
    );
    let selected = execute(
        &mut engine,
        json!({"type":"set_selection","selection":selection(&id,3,2)}),
    );
    assert_eq!(selected["selection"]["anchor"]["utf16"], 3);
    assert_eq!(selected["selection"]["focus"]["utf16"], 1);
    execute(
        &mut engine,
        json!({"type":"format_character","bold":true,"italic":true,"underline":true,"fontFamily":"Georgia","fontSizePt":14.5}),
    );
    let all = execute(
        &mut engine,
        json!({"type":"set_selection","selection":selection(&id,0,4)}),
    );
    assert!(all["format"]["bold"].is_null());
    execute(
        &mut engine,
        json!({"type":"format_paragraph","alignment":"center"}),
    );
    assert_eq!(
        view(&engine)["paragraphs"][0]["format"]["alignment"],
        "center"
    );
    let bytes = engine.export_document().unwrap();
    let model = deserialize_document(&bytes).unwrap();
    let Block::Paragraph(paragraph) = &model.sections()[0].blocks()[0] else {
        panic!("expected paragraph")
    };
    assert!(paragraph.runs().iter().any(
        |run| run.format.bold == Some(true) && run.format.underline == Some(Underline::Single)
    ));
}

#[test]
fn typing_format_is_runtime_state_until_text_is_inserted() {
    let mut engine = BrowserEngine::new("test").unwrap();
    let original = engine.export_document().unwrap();
    let initial = view(&engine)["revision"].clone();
    execute(
        &mut engine,
        json!({"type":"format_character","bold":true,"italic":true}),
    );
    assert_eq!(view(&engine)["revision"], initial);
    assert_eq!(engine.export_document().unwrap(), original);
    execute(&mut engine, json!({"type":"replace_text","text":"Bold"}));
    assert_eq!(
        view(&engine)["paragraphs"][0]["runs"][0]["format"]["bold"],
        true
    );
    execute(&mut engine, json!({"type":"undo"}));
    assert_eq!(engine.export_document().unwrap(), original);
}

#[test]
fn paragraph_creation_join_and_unicode_split_are_atomic() {
    let mut engine = BrowserEngine::new("test").unwrap();
    let id = first_id(&engine);
    execute(&mut engine, json!({"type":"replace_text","text":"A👩🏽‍💻B"}));
    execute(
        &mut engine,
        json!({"type":"insert_paragraph","selection":selection(&id,1,1)}),
    );
    assert_eq!(text(&view(&engine)), vec!["A", "👩🏽‍💻B"]);
    execute(&mut engine, json!({"type":"delete","direction":"backward"}));
    assert_eq!(text(&view(&engine)), vec!["A👩🏽‍💻B"]);
    execute(&mut engine, json!({"type":"undo"}));
    let second = view(&engine)["paragraphs"][1]["id"]
        .as_str()
        .unwrap()
        .to_string();
    execute(
        &mut engine,
        json!({"type":"delete","direction":"forward","selection":selection(&id,1,1)}),
    );
    assert_eq!(text(&view(&engine)), vec!["A👩🏽‍💻B"]);
    assert_ne!(second, id);
}

#[test]
fn empty_import_is_exact_until_first_edit_and_failure_preserves_active_session() {
    let mut engine = BrowserEngine::new("test").unwrap();
    let empty = serialize_document(&Document::new(id("empty")).unwrap()).unwrap();
    let result: Value = serde_json::from_str(&engine.open_document(&empty)).unwrap();
    assert_eq!(result["ok"], true);
    assert_eq!(view(&engine)["paragraphs"], json!([]));
    assert_eq!(engine.export_document().unwrap(), empty);
    execute(&mut engine, json!({"type":"replace_text","text":"first"}));
    execute(&mut engine, json!({"type":"undo"}));
    assert_eq!(engine.export_document().unwrap(), empty);
    let before = engine.snapshot();
    for bytes in [
        b"{bad".as_slice(),
        b"{\"format\":\"writer\",\"format_version\":99}".as_slice(),
    ] {
        let response: Value = serde_json::from_str(&engine.open_document(bytes)).unwrap();
        assert_eq!(response["ok"], false);
        assert_eq!(engine.snapshot(), before);
    }
    let response: Value =
        serde_json::from_str(&engine.open_document_verified(&empty, "wrong")).unwrap();
    assert_eq!(response["error"]["code"], "document_mismatch");
    assert_eq!(engine.snapshot(), before);
}

#[test]
fn imported_ids_are_opaque_and_style_display_preserves_native_semantics() {
    let mut engine = BrowserEngine::new("test").unwrap();
    let mut base = NamedStyle::new(id("base"), "Base");
    base.character_format.bold = Some(true);
    base.character_format.font_family = Some("Georgia".into());
    let mut child = NamedStyle::new(id("child"), "Child");
    child.based_on = Some(base.id.clone());
    child.character_format.italic = Some(true);
    let weird = r#"paragraph'"] [data-p="x"]"#;
    let paragraph = Paragraph::new(
        id(weird),
        vec![TextRun {
            text: "text".into(),
            format: CharacterFormat {
                bold: Some(false),
                ..Default::default()
            },
        }],
    )
    .with_style(child.id.clone());
    let document = Document::from_parts(
        id("test:e4"),
        DocumentMetadata {
            title: "Imported".into(),
        },
        vec![Section::new(
            id("section"),
            vec![Block::Paragraph(paragraph)],
        )],
        vec![base, child],
    )
    .unwrap();
    let bytes = serialize_document(&document).unwrap();
    engine.open_document(&bytes);
    assert_eq!(engine.export_document().unwrap(), bytes);
    let snapshot = view(&engine);
    assert_eq!(snapshot["paragraphs"][0]["id"], weird);
    assert_eq!(
        snapshot["paragraphs"][0]["runs"][0]["format"]["bold"],
        false
    );
    assert_eq!(
        snapshot["paragraphs"][0]["runs"][0]["format"]["italic"],
        true
    );
    assert_eq!(
        snapshot["paragraphs"][0]["runs"][0]["format"]["fontFamily"],
        "Georgia"
    );
    execute(&mut engine, json!({"type":"insert_paragraph"}));
    assert_ne!(view(&engine)["paragraphs"][1]["id"], "test:e4");
}

#[test]
fn paste_splits_before_insertion_so_combining_suffix_cannot_obstruct_break() {
    let mut engine = BrowserEngine::new("test").unwrap();
    execute(&mut engine, json!({"type":"replace_text","text":"\u{301}"}));
    let id = first_id(&engine);
    execute(
        &mut engine,
        json!({"type":"replace_text","text":"a\nb","selection":selection(&id,0,0)}),
    );
    assert_eq!(text(&view(&engine)), vec!["a", "b\u{301}"]);
    assert_eq!(view(&engine)["selection"]["focus"]["utf16"], 2);
}

#[test]
fn cross_section_delete_is_rejected_and_formatting_crosses_sections() {
    let document = Document::from_parts(
        id("doc"),
        Default::default(),
        vec![
            Section::new(
                id("s1"),
                vec![Block::Paragraph(Paragraph::new(
                    id("p1"),
                    vec![TextRun::new("a")],
                ))],
            ),
            Section::new(
                id("s2"),
                vec![Block::Paragraph(Paragraph::new(
                    id("p2"),
                    vec![TextRun::new("b")],
                ))],
            ),
        ],
        vec![],
    )
    .unwrap();
    let mut engine = BrowserEngine::new("test").unwrap();
    engine.open_document(&serialize_document(&document).unwrap());
    let before = engine.export_document().unwrap();
    let result: Value = serde_json::from_str(&engine.dispatch(
        &json!({"type":"delete","direction":"forward","selection":selection("p1",1,1)}).to_string(),
    ))
    .unwrap();
    assert_eq!(result["error"]["code"], "cross_section_edit");
    assert_eq!(engine.export_document().unwrap(), before);
    execute(
        &mut engine,
        json!({"type":"format_character","bold":true,"selection":{"anchor":point("p1",0),"focus":point("p2",1)}}),
    );
    assert_eq!(
        view(&engine)["paragraphs"][1]["runs"][0]["format"]["bold"],
        true
    );
}

#[test]
fn copy_changes_document_id_and_session_but_preserves_supported_content() {
    let mut engine = BrowserEngine::new("test").unwrap();
    execute(&mut engine, json!({"type":"replace_text","text":"copy😀"}));
    let before = view(&engine);
    execute(&mut engine, json!({"type":"fork_document"}));
    let after = view(&engine);
    assert_ne!(before["documentId"], after["documentId"]);
    assert_ne!(before["sessionId"], after["sessionId"]);
    assert_eq!(before["paragraphs"], after["paragraphs"]);
    assert_eq!(after["canUndo"], false);
}

#[test]
fn excessive_input_fails_without_publishing_partial_multiline_edits() {
    let mut engine = BrowserEngine::new("test").unwrap();
    let original = engine.snapshot();
    for input in ["x".repeat(200_001), "a\n".repeat(2_001)] {
        let response: Value = serde_json::from_str(
            &engine.dispatch(&json!({"type":"replace_text","text":input}).to_string()),
        )
        .unwrap();
        assert_eq!(response["error"]["code"], "document_too_large");
        assert_eq!(engine.snapshot(), original);
    }
}

#[test]
fn missing_selection_cannot_replace_existing_content_or_another_section() {
    let mut engine = BrowserEngine::new("test").unwrap();
    execute(
        &mut engine,
        json!({"type":"replace_text","text":"first\nsecond"}),
    );
    execute(
        &mut engine,
        json!({"type":"set_selection","selection":null}),
    );
    let before = engine.snapshot();
    for command in [
        json!({"type":"replace_text","text":"replacement"}),
        json!({"type":"format_paragraph","alignment":"center"}),
    ] {
        let result: Value = serde_json::from_str(&engine.dispatch(&command.to_string())).unwrap();
        assert_eq!(result["error"]["code"], "missing_selection");
        assert_eq!(engine.snapshot(), before);
    }
    let document = Document::from_parts(
        id("doc"),
        Default::default(),
        vec![
            Section::new(id("empty-section"), vec![]),
            Section::new(
                id("content-section"),
                vec![Block::Paragraph(Paragraph::new(
                    id("p"),
                    vec![TextRun::new("preserve")],
                ))],
            ),
        ],
        vec![],
    )
    .unwrap();
    engine.open_document(&serialize_document(&document).unwrap());
    execute(
        &mut engine,
        json!({"type":"set_selection","selection":null}),
    );
    let original = engine.export_document().unwrap();
    let result: Value =
        serde_json::from_str(&engine.dispatch(r#"{"type":"replace_text","text":"discard?"}"#))
            .unwrap();
    assert_eq!(result["error"]["code"], "missing_selection");
    assert_eq!(engine.export_document().unwrap(), original);
}

#[test]
fn explicit_new_location_does_not_reuse_previous_caret_typing_format() {
    let mut engine = BrowserEngine::new("test").unwrap();
    execute(&mut engine, json!({"type":"replace_text","text":"ab"}));
    let paragraph = first_id(&engine);
    execute(&mut engine, json!({"type":"format_character","bold":true}));
    execute(
        &mut engine,
        json!({"type":"replace_text","text":"z","selection":selection(&paragraph,0,0)}),
    );
    assert_eq!(
        view(&engine)["paragraphs"][0]["runs"][0]["format"]["bold"],
        false
    );
    execute(&mut engine, json!({"type":"undo"}));
    assert_eq!(view(&engine)["selection"]["focus"]["utf16"], 0);
}

#[test]
fn navigation_affinity_handles_split_runs_and_shrinking_reverse_selections() {
    for (runs, inside, end) in [
        (vec![TextRun::new("Ae"), TextRun::new("\u{301}")], 2, 3),
        (vec![TextRun::new("A👩"), TextRun::new("🏽‍💻")], 3, 8),
    ] {
        let document = Document::from_parts(
            id("doc"),
            Default::default(),
            vec![Section::new(
                id("s"),
                vec![Block::Paragraph(Paragraph::new(id("p"), runs))],
            )],
            vec![],
        )
        .unwrap();
        let mut engine = BrowserEngine::new("test").unwrap();
        engine.open_document(&serialize_document(&document).unwrap());
        let leading = execute(
            &mut engine,
            json!({"type":"set_selection","selection":selection("p",inside,inside),"affinity":"leading"}),
        );
        assert_eq!(leading["selection"]["focus"]["utf16"], 1);
        let trailing = execute(
            &mut engine,
            json!({"type":"set_selection","selection":selection("p",inside,inside),"affinity":"trailing"}),
        );
        assert_eq!(trailing["selection"]["focus"]["utf16"], end);
        let reverse = execute(
            &mut engine,
            json!({"type":"set_selection","selection":selection("p",end,inside),"affinity":"trailing"}),
        );
        assert_eq!(reverse["selection"]["focus"]["utf16"], end);
        let expanded = execute(
            &mut engine,
            json!({"type":"set_selection","selection":selection("p",end,inside),"affinity":"leading"}),
        );
        assert_eq!(expanded["selection"]["focus"]["utf16"], 1);
        assert_eq!(
            engine.export_document().unwrap(),
            serialize_document(&document).unwrap()
        );
    }
}

#[test]
fn resource_bounds_reject_large_style_graphs_run_sets_and_sections_before_publication() {
    let mut engine = BrowserEngine::new("test").unwrap();
    execute(
        &mut engine,
        json!({"type":"replace_text","text":"unsaved work"}),
    );
    let original = engine.snapshot();
    let broad_styles = (0..129)
        .map(|index| NamedStyle::new(id(&format!("style-{index}")), format!("Style {index}")))
        .collect();
    let deep_styles = (0..33)
        .map(|index| {
            let mut style =
                NamedStyle::new(id(&format!("style-{index}")), format!("Style {index}"));
            if index > 0 {
                style.based_on = Some(id(&format!("style-{}", index - 1)));
            }
            style
        })
        .collect();
    let documents = [
        Document::from_parts(id("broad"), Default::default(), vec![], broad_styles).unwrap(),
        Document::from_parts(id("deep"), Default::default(), vec![], deep_styles).unwrap(),
        Document::from_parts(
            id("runs"),
            Default::default(),
            vec![Section::new(
                id("s"),
                vec![Block::Paragraph(Paragraph::new(
                    id("p"),
                    vec![TextRun::new(""); 20_001],
                ))],
            )],
            vec![],
        )
        .unwrap(),
        Document::from_parts(
            id("sections"),
            Default::default(),
            (0..2001)
                .map(|index| Section::new(id(&format!("section-{index}")), vec![]))
                .collect(),
            vec![],
        )
        .unwrap(),
    ];
    for document in documents {
        let result: Value =
            serde_json::from_str(&engine.open_document(&serialize_document(&document).unwrap()))
                .unwrap();
        assert_eq!(result["error"]["code"], "document_too_large");
        assert_eq!(engine.snapshot(), original);
    }
    let exported = engine.export_document().unwrap();
    let result: Value = serde_json::from_str(&engine.open_document(&exported)).unwrap();
    assert_eq!(result["ok"], true);
    assert_eq!(engine.export_document().unwrap(), exported);
}

#[test]
fn paste_preflight_accounts_for_existing_paragraphs_and_selected_replacements() {
    let document = Document::from_parts(
        id("doc"),
        Default::default(),
        vec![Section::new(
            id("s"),
            (0..2000)
                .map(|index| {
                    Block::Paragraph(Paragraph::new(
                        id(&format!("p-{index}")),
                        vec![TextRun::new("a")],
                    ))
                })
                .collect(),
        )],
        vec![],
    )
    .unwrap();
    let mut engine = BrowserEngine::new("test").unwrap();
    engine.open_document(&serialize_document(&document).unwrap());
    let original = engine.snapshot();
    let result: Value =
        serde_json::from_str(&engine.dispatch(r#"{"type":"replace_text","text":"\n"}"#)).unwrap();
    assert_eq!(result["error"]["code"], "document_too_large");
    assert_eq!(engine.snapshot(), original);
    execute(
        &mut engine,
        json!({"type":"replace_text","text":"first\nlast","selection":{"anchor":point("p-0",0),"focus":point("p-1999",1)}}),
    );
    assert_eq!(text(&view(&engine)), vec!["first", "last"]);
}

#[test]
fn text_limit_counts_content_without_structural_paragraph_separators() {
    let mut engine = BrowserEngine::new("test").unwrap();
    let content = format!("{}\n{}", "a".repeat(100_000), "b".repeat(100_000));
    execute(&mut engine, json!({"type":"replace_text","text":content}));
    assert_eq!(view(&engine)["paragraphs"].as_array().unwrap().len(), 2);
    let bytes = engine.export_document().unwrap();
    let response: Value = serde_json::from_str(&engine.open_document(&bytes)).unwrap();
    assert_eq!(response["ok"], true);
    assert_eq!(engine.export_document().unwrap(), bytes);
}

#[test]
fn selected_text_preserves_split_run_graphemes_and_reverse_selection_without_mutation() {
    let document = Document::from_parts(
        id("doc"),
        Default::default(),
        vec![Section::new(
            id("s"),
            vec![Block::Paragraph(Paragraph::new(
                id("p"),
                vec![
                    TextRun::new("Ae"),
                    TextRun {
                        text: "\u{301}".into(),
                        format: CharacterFormat {
                            bold: Some(true),
                            ..Default::default()
                        },
                    },
                    TextRun::new("👩"),
                    TextRun::new("🏽‍💻"),
                    TextRun::new("Z"),
                ],
            ))],
        )],
        vec![],
    )
    .unwrap();
    let original = serialize_document(&document).unwrap();
    let mut engine = BrowserEngine::new("clipboard").unwrap();
    let opened: Value = serde_json::from_str(&engine.open_document(&original)).unwrap();
    assert_eq!(opened["ok"], true);
    execute(
        &mut engine,
        json!({"type":"set_title","title":"Clipboard sample"}),
    );
    for (anchor, focus) in [(1, 10), (10, 1)] {
        execute(
            &mut engine,
            json!({"type":"set_selection","selection":selection("p",anchor,focus)}),
        );
        let snapshot = engine.snapshot();
        let bytes = engine.export_document().unwrap();
        assert_eq!(engine.selected_text().unwrap(), "e\u{301}👩🏽‍💻");
        assert_eq!(engine.snapshot(), snapshot);
        assert_eq!(engine.export_document().unwrap(), bytes);
    }
    execute(&mut engine, json!({"type":"undo"}));
    assert_eq!(engine.export_document().unwrap(), original);
    execute(&mut engine, json!({"type":"redo"}));
    assert_eq!(view(&engine)["title"], "Clipboard sample");
}

#[test]
fn selected_text_adds_one_lf_per_paragraph_boundary_and_preserves_literal_crlf() {
    let document = Document::from_parts(
        id("doc"),
        Default::default(),
        vec![
            Section::new(
                id("s1"),
                vec![
                    Block::Paragraph(Paragraph::new(id("p1"), vec![TextRun::new("first")])),
                    Block::Paragraph(Paragraph::new(id("p2"), vec![])),
                    Block::Paragraph(Paragraph::new(
                        id("p3"),
                        vec![TextRun::new("line\r\ninside")],
                    )),
                ],
            ),
            Section::new(
                id("s2"),
                vec![Block::Paragraph(Paragraph::new(
                    id("p4"),
                    vec![TextRun::new("last")],
                ))],
            ),
        ],
        vec![],
    )
    .unwrap();
    let mut engine = BrowserEngine::new("clipboard").unwrap();
    let opened: Value =
        serde_json::from_str(&engine.open_document(&serialize_document(&document).unwrap()))
            .unwrap();
    assert_eq!(opened["ok"], true);
    for (anchor, focus) in [
        (point("p1", 1), point("p4", 3)),
        (point("p4", 3), point("p1", 1)),
    ] {
        execute(
            &mut engine,
            json!({"type":"set_selection","selection":{"anchor":anchor,"focus":focus}}),
        );
        let snapshot = engine.snapshot();
        assert_eq!(
            engine.selected_text().unwrap(),
            "irst\n\nline\r\ninside\nlas"
        );
        assert_eq!(engine.snapshot(), snapshot);
    }
    execute(
        &mut engine,
        json!({"type":"set_selection","selection":{"anchor":point("p1",5),"focus":point("p2",0)}}),
    );
    assert_eq!(engine.selected_text().unwrap(), "\n");
    assert_eq!(
        engine.export_document().unwrap(),
        serialize_document(&document).unwrap()
    );
}

#[test]
fn selected_text_of_absent_or_collapsed_selection_is_empty_without_materializing() {
    let mut engine = BrowserEngine::new("clipboard").unwrap();
    for text in ["", "content"] {
        execute(&mut engine, json!({"type":"replace_text","text":text}));
        let snapshot = engine.snapshot();
        assert_eq!(engine.selected_text().unwrap(), "");
        assert_eq!(engine.snapshot(), snapshot);
    }
    execute(
        &mut engine,
        json!({"type":"set_selection","selection":null}),
    );
    let snapshot = engine.snapshot();
    assert_eq!(engine.selected_text().unwrap(), "");
    assert_eq!(engine.snapshot(), snapshot);
    let empty = serialize_document(&Document::new(id("empty")).unwrap()).unwrap();
    let opened: Value = serde_json::from_str(&engine.open_document(&empty)).unwrap();
    assert_eq!(opened["ok"], true);
    let snapshot = engine.snapshot();
    assert_eq!(engine.selected_text().unwrap(), "");
    assert_eq!(engine.snapshot(), snapshot);
    assert_eq!(engine.export_document().unwrap(), empty);
}

#[test]
fn word_count_tracks_committed_multiword_typing_formatting_and_history() {
    let mut engine = BrowserEngine::new("live-word-count").unwrap();
    assert_eq!(view(&engine)["wordCount"], 0);

    let first = execute(&mut engine, json!({"type":"replace_text","text":"one"}));
    assert_eq!(first["wordCount"], 1);

    let second = execute(&mut engine, json!({"type":"replace_text","text":" two"}));
    assert_eq!(text(&second), vec!["one two"]);
    assert_eq!(second["wordCount"], 2);

    let third = execute(&mut engine, json!({"type":"replace_text","text":" three"}));
    assert_eq!(text(&third), vec!["one two three"]);
    assert_eq!(third["wordCount"], 3);

    // Splitting a word across styled runs must not count two words.
    let paragraph_id = third["paragraphs"][0]["id"].as_str().unwrap();
    let selected = execute(
        &mut engine,
        json!({"type":"set_selection",
        "selection":selection(paragraph_id,0,2)}),
    );
    assert_eq!(selected["wordCount"], 3);
    let formatted = execute(&mut engine, json!({"type":"format_character","bold":true}));
    assert_eq!(formatted["wordCount"], 3);
    assert_eq!(text(&formatted), vec!["one two three"]);

    let paragraph_end = execute(
        &mut engine,
        json!({"type":"set_selection",
        "selection":selection(paragraph_id,13,13)}),
    );
    assert_eq!(paragraph_end["wordCount"], 3);
    let newline = execute(
        &mut engine,
        json!({"type":"replace_text","text":"\nfour five"}),
    );
    assert_eq!(text(&newline), vec!["one two three", "four five"]);
    assert_eq!(newline["wordCount"], 5);
    assert_eq!(view(&engine)["wordCount"], 5);

    let undo = execute(&mut engine, json!({"type":"undo"}));
    assert_eq!(undo["wordCount"], 3);
    let redo = execute(&mut engine, json!({"type":"redo"}));
    assert_eq!(redo["wordCount"], 5);
}
