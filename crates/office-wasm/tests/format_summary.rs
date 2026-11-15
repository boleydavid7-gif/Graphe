//! Character summaries describe selected text without false empty-paragraph mixes.

use office_core::{
    Alignment, Block, CharacterFormat, Document, HalfPoints, Id, NamedStyle, Paragraph,
    ParagraphFormat, Section, TextRun, serialize_document,
};
use office_wasm::BrowserEngine;
use serde_json::{Value, json};

fn id(value: &str) -> Id {
    Id::new(value).unwrap()
}
fn execute(engine: &mut BrowserEngine, command: Value) -> Value {
    let result: Value = serde_json::from_str(&engine.dispatch(&command.to_string())).unwrap();
    assert_eq!(result["ok"], true, "{result}");
    result["view"].clone()
}
fn populated() -> BrowserEngine {
    let document = Document::from_parts(
        id("summary-document"),
        Default::default(),
        vec![Section::new(
            id("summary-section"),
            vec![
                Block::Paragraph(Paragraph::new(
                    id("text-start"),
                    vec![
                        TextRun {
                            text: "One".into(),
                            format: CharacterFormat {
                                bold: Some(true),
                                ..Default::default()
                            },
                        },
                        TextRun {
                            text: "".into(),
                            format: CharacterFormat {
                                bold: Some(false),
                                font_family: Some("Arial".into()),
                                font_size: Some(HalfPoints(16)),
                                ..Default::default()
                            },
                        },
                        TextRun::new(" two"),
                    ],
                )),
                Block::Paragraph(
                    Paragraph::new(id("empty-paragraph"), vec![]).with_style(id("empty-style")),
                ),
                Block::Paragraph(
                    Paragraph::new(id("empty-run-paragraph"), vec![TextRun::new("")])
                        .with_style(id("empty-style")),
                ),
                Block::Paragraph(Paragraph::new(id("text-end"), vec![TextRun::new("Three")])),
            ],
        )],
        vec![NamedStyle {
            id: id("empty-style"),
            name: "Empty paragraph style".into(),
            based_on: None,
            paragraph_format: ParagraphFormat {
                alignment: Some(Alignment::Right),
                ..Default::default()
            },
            character_format: CharacterFormat {
                bold: Some(false),
                italic: Some(false),
                font_family: Some("Tinos".into()),
                font_size: Some(HalfPoints(36)),
                ..Default::default()
            },
        }],
    )
    .unwrap();
    let mut engine = BrowserEngine::new("format-summary").unwrap();
    let opened: Value =
        serde_json::from_str(&engine.open_document(&serialize_document(&document).unwrap()))
            .unwrap();
    assert_eq!(opened["ok"], true, "{opened}");
    engine
}

#[test]
fn whole_text_formatting_ignores_empty_paragraph_and_zero_length_run_character_samples() {
    let mut engine = populated();
    let selected = execute(
        &mut engine,
        json!({"type":"set_selection", "selection":{
            "anchor":{"paragraphId":"text-start","utf16":0}, "focus":{"paragraphId":"text-end","utf16":5}
        }}),
    );
    assert!(selected["format"]["bold"].is_null());
    let formatted = execute(
        &mut engine,
        json!({"type":"format_character", "bold":true,"italic":true,"underline":true,"fontFamily":"Tinos","fontSizePt":14}),
    );
    assert!(
        formatted["paragraphs"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|p| p["runs"].as_array().unwrap())
            .filter(|run| !run["text"].as_str().unwrap().is_empty())
            .all(|run| run["format"]["bold"] == true && run["format"]["italic"] == true)
    );
    assert_eq!(formatted["format"]["bold"], true);
    assert_eq!(formatted["format"]["italic"], true);
    assert_eq!(formatted["format"]["underline"], true);
    assert_eq!(formatted["format"]["fontFamily"], "Tinos");
    assert_eq!(formatted["format"]["fontSizePt"], 14.0);
    // Empty paragraphs still participate in paragraph/style summaries.
    assert!(formatted["format"]["alignment"].is_null());
    assert_eq!(formatted["format"]["styleMixed"], true);
}

#[test]
fn blank_only_selection_uses_paragraph_defaults_and_collapsed_typing_remains_runtime_only() {
    let mut engine = populated();
    let selected = execute(
        &mut engine,
        json!({"type":"set_selection", "selection":{
            "anchor":{"paragraphId":"empty-paragraph","utf16":0}, "focus":{"paragraphId":"text-end","utf16":0}
        }}),
    );
    assert_eq!(selected["format"]["bold"], false);
    assert_eq!(selected["format"]["fontFamily"], "Tinos");
    assert_eq!(selected["format"]["fontSizePt"], 18.0);
    assert_eq!(selected["format"]["alignment"], "right");
    assert_eq!(selected["format"]["styleId"], "empty-style");
    execute(
        &mut engine,
        json!({"type":"set_selection", "selection":{
            "anchor":{"paragraphId":"empty-paragraph","utf16":0}, "focus":{"paragraphId":"empty-paragraph","utf16":0}
        }}),
    );
    let before = engine.export_document().unwrap();
    let typing = execute(
        &mut engine,
        json!({"type":"format_character","bold":true,"italic":true,"fontSizePt":20}),
    );
    assert_eq!(typing["format"]["bold"], true);
    assert_eq!(typing["format"]["italic"], true);
    assert_eq!(typing["format"]["fontSizePt"], 20.0);
    assert_eq!(engine.export_document().unwrap(), before);
}
