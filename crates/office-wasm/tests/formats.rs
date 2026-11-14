//! Format adapters, guarded Save As, retained originals and atomic browser state.

use office_core::{
    Alignment, Block, CharacterFormat, Document, DocumentMetadata, HalfPoints, Id, OriginalSource,
    Paragraph, ParagraphFormat, Section, TextColor, TextRun, Underline, WriterFile,
    deserialize_writer_file, serialize_writer_file,
};
use office_formats::DocumentFormat;
use office_wasm::BrowserEngine;
use serde_json::{Value, json};

fn id(value: &str) -> Id {
    Id::new(value).unwrap()
}

fn view(engine: &BrowserEngine) -> Value {
    serde_json::from_str(&engine.snapshot()).unwrap()
}

fn dispatch(engine: &mut BrowserEngine, command: Value) -> Value {
    let response: Value = serde_json::from_str(&engine.dispatch(&command.to_string())).unwrap();
    assert_eq!(response["ok"], true, "{response}");
    response["view"].clone()
}

fn source_model() -> Document {
    Document::from_parts(
        id("format-source"),
        DocumentMetadata {
            title: "Formats Café".into(),
        },
        vec![Section::new(
            id("format-section"),
            vec![
                Block::Paragraph(
                    Paragraph::new(
                        id("format-paragraph"),
                        vec![
                            TextRun {
                                text: "Café e\u{301} 👩🏽‍💻 ".into(),
                                format: CharacterFormat {
                                    bold: Some(true),
                                    italic: Some(true),
                                    underline: Some(Underline::Single),
                                    font_family: Some("Arimo".into()),
                                    font_size: Some(HalfPoints(28)),
                                    color: Some(TextColor::Rgb {
                                        red: 34,
                                        green: 68,
                                        blue: 102,
                                    }),
                                },
                            },
                            TextRun::new("العربية नमस्ते שלום"),
                        ],
                    )
                    .with_format(ParagraphFormat {
                        alignment: Some(Alignment::Right),
                        ..Default::default()
                    }),
                ),
                Block::Paragraph(Paragraph::new(
                    id("second-paragraph"),
                    vec![TextRun::new("Second paragraph")],
                )),
            ],
        )],
        vec![],
    )
    .unwrap()
}

fn formatted_engine(namespace: &str) -> BrowserEngine {
    let mut engine = BrowserEngine::new(namespace).unwrap();
    let bytes = serialize_writer_file(&WriterFile::new(source_model())).unwrap();
    let opened: Value = serde_json::from_str(&engine.open_document(&bytes)).unwrap();
    assert_eq!(opened["ok"], true, "{opened}");
    engine
}

fn paragraph_text(paragraph: &Value) -> String {
    paragraph["runs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|run| run["text"].as_str().unwrap())
        .collect()
}

#[test]
fn all_editable_external_formats_retain_original_edit_save_and_reopen() {
    let original = formatted_engine("external-export");
    for format in ["docx", "odt", "rtf", "txt"] {
        let bytes = original.export_format(format, true, None).unwrap();
        let mut imported = BrowserEngine::new(&format!("import-{format}")).unwrap();
        let result: Value = serde_json::from_str(&imported.import_format(
            &bytes,
            format,
            &format!("原本.{format}"),
        ))
        .unwrap();
        assert_eq!(result["ok"], true, "{format}: {result}");
        let initial = result["view"].clone();
        assert_eq!(initial["originalSource"]["format"], format);
        assert_eq!(
            initial["originalSource"]["filename"],
            format!("原本.{format}")
        );
        assert_eq!(imported.export_original().unwrap(), bytes);
        assert_eq!(
            paragraph_text(&initial["paragraphs"][0]),
            "Café e\u{301} 👩🏽‍💻 العربية नमस्ते שלום"
        );
        if format != "txt" {
            assert_eq!(
                initial["paragraphs"][0]["runs"][0]["format"]["bold"], true,
                "{format}: {initial}"
            );
            assert_eq!(initial["paragraphs"][0]["format"]["alignment"], "right");
        }
        let paragraph = initial["paragraphs"][0]["id"].clone();
        let edited = dispatch(
            &mut imported,
            json!({ "type":"replace_text", "text":"Edited ", "selection": {
                "anchor":{"paragraphId":paragraph,"utf16":0}, "focus":{"paragraphId":paragraph,"utf16":0}
            }}),
        );
        assert!(paragraph_text(&edited["paragraphs"][0]).starts_with("Edited Café"));
        let native = imported.export_document().unwrap();
        let retained = deserialize_writer_file(&native).unwrap();
        assert_eq!(retained.original_source().unwrap().bytes(), bytes);
        let mut native_reopened = BrowserEngine::new(&format!("native-{format}")).unwrap();
        let opened: Value = serde_json::from_str(&native_reopened.open_document(&native)).unwrap();
        assert_eq!(opened["ok"], true, "{format}: {opened}");
        assert_eq!(native_reopened.export_document().unwrap(), native);
        assert_eq!(native_reopened.export_original().unwrap(), bytes);

        let converted = imported
            .export_format(format, true, Some(edited["revision"].as_str().unwrap()))
            .unwrap();
        let mut reopened = BrowserEngine::new(&format!("reopened-{format}")).unwrap();
        let response: Value = serde_json::from_str(&reopened.import_format(
            &converted,
            format,
            &format!("edited.{format}"),
        ))
        .unwrap();
        assert_eq!(response["ok"], true, "{format}: {response}");
        assert_eq!(
            paragraph_text(&response["view"]["paragraphs"][0]),
            paragraph_text(&edited["paragraphs"][0])
        );
        assert_eq!(reopened.export_original().unwrap(), converted);
    }
}

#[test]
fn export_warnings_require_acknowledgement_and_stale_confirmation_is_rejected() {
    let mut engine = formatted_engine("loss-guard");
    let before = engine.snapshot();
    let revision = view(&engine)["revision"].as_str().unwrap().to_owned();
    for format in ["docx", "odt", "rtf", "txt", "pdf"] {
        let report: Value =
            serde_json::from_str(&engine.export_assessment(format, Some(&revision)).unwrap())
                .unwrap();
        assert_eq!(report["format"], format);
        assert_eq!(report["revision"], revision);
        assert!(!report["warnings"].as_array().unwrap().is_empty());
        assert_eq!(
            engine
                .export_format(format, false, Some(&revision))
                .unwrap_err()
                .code,
            "export_confirmation_required"
        );
        assert_eq!(engine.snapshot(), before);
    }
    let native_report: Value =
        serde_json::from_str(&engine.export_assessment("writer", Some(&revision)).unwrap())
            .unwrap();
    assert_eq!(native_report["warnings"], json!([]));
    assert_eq!(
        engine
            .export_format("writer", false, Some(&revision))
            .unwrap(),
        engine.export_document().unwrap()
    );
    dispatch(
        &mut engine,
        json!({"type":"set_title", "title":"New revision"}),
    );
    let after = engine.snapshot();
    assert_eq!(
        engine
            .export_assessment("txt", Some(&revision))
            .unwrap_err()
            .code,
        "stale_revision"
    );
    assert_eq!(
        engine
            .export_format("txt", true, Some(&revision))
            .unwrap_err()
            .code,
        "stale_revision"
    );
    assert_eq!(engine.snapshot(), after);
}

#[test]
fn native_save_as_discloses_retained_original_without_changing_internal_saving() {
    let mut engine = BrowserEngine::new("retained-warning").unwrap();
    let opened: Value =
        serde_json::from_str(&engine.import_format(b"Sensitive original", "txt", "original.txt"))
            .unwrap();
    assert_eq!(opened["ok"], true);
    let report: Value =
        serde_json::from_str(&engine.export_assessment("writer", None).unwrap()).unwrap();
    assert!(
        report["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| warning["code"] == "retained_original")
    );
    assert_eq!(
        engine
            .export_format("writer", false, None)
            .unwrap_err()
            .code,
        "export_confirmation_required"
    );
    let native = engine.export_document().unwrap();
    assert_eq!(engine.export_format("writer", true, None).unwrap(), native);
    assert_eq!(
        deserialize_writer_file(&native)
            .unwrap()
            .original_source()
            .unwrap()
            .bytes(),
        b"Sensitive original"
    );
}

#[test]
fn empty_plain_text_has_no_invalid_empty_source_and_stays_editable() {
    let mut engine = BrowserEngine::new("empty-text").unwrap();
    let opened: Value =
        serde_json::from_str(&engine.import_format(b"", "txt", "empty.txt")).unwrap();
    assert_eq!(opened["ok"], true, "{opened}");
    assert!(opened["view"]["originalSource"].is_null());
    assert_eq!(engine.export_original().unwrap_err().code, "no_original");
    dispatch(
        &mut engine,
        json!({"type":"replace_text", "text":"First words 👩🏽‍💻"}),
    );
    assert_eq!(
        engine.export_format("txt", true, None).unwrap(),
        "First words 👩🏽‍💻".as_bytes()
    );
}

#[test]
fn corrupt_unknown_and_pdf_imports_leave_document_history_selection_and_original_unchanged() {
    let mut engine = BrowserEngine::new("atomic-formats").unwrap();
    let imported: Value =
        serde_json::from_str(&engine.import_format(b"Original", "txt", "original.txt")).unwrap();
    assert_eq!(imported["ok"], true);
    dispatch(
        &mut engine,
        json!({"type":"replace_text", "text":"edited "}),
    );
    let before = engine.snapshot();
    let native = engine.export_document().unwrap();
    for (format, bytes) in [
        ("odt", b"broken ZIP".as_slice()),
        ("rtf", b"not RTF".as_slice()),
        ("txt", &[0xff][..]),
        ("pdf", b"%PDF-1.7".as_slice()),
        ("unknown", b"test".as_slice()),
    ] {
        let response: Value =
            serde_json::from_str(&engine.import_format(bytes, format, &format!("next.{format}")))
                .unwrap();
        assert_eq!(response["ok"], false, "{format}: {response}");
        if format == "pdf" {
            assert_eq!(response["error"]["code"], "export_only_format");
        }
        assert_eq!(engine.snapshot(), before);
        assert_eq!(engine.export_document().unwrap(), native);
        assert_eq!(engine.export_original().unwrap(), b"Original");
    }
    dispatch(&mut engine, json!({"type":"undo"}));
    assert_eq!(paragraph_text(&view(&engine)["paragraphs"][0]), "Original");
}

#[test]
fn source_bearing_native_files_revalidate_new_adapter_sources_before_replacing_state() {
    let mut engine = formatted_engine("source-validation-new");
    let initial = engine.export_document().unwrap();
    let document = deserialize_writer_file(&initial).unwrap().into_parts().0;
    for format in [
        DocumentFormat::Odt,
        DocumentFormat::Rtf,
        DocumentFormat::Txt,
    ] {
        let malformed = if format == DocumentFormat::Txt {
            &[0xff][..]
        } else {
            b"invalid source".as_slice()
        };
        for (media_type, bytes) in [
            (format.media_type(), malformed),
            ("application/x-wrong", b"Original".as_slice()),
        ] {
            let source = OriginalSource::new(
                format.extension(),
                media_type,
                format!("original.{}", format.extension()),
                bytes,
            )
            .unwrap();
            let file = WriterFile::with_original_source(document.clone(), source).unwrap();
            let result: Value =
                serde_json::from_str(&engine.open_document(&serialize_writer_file(&file).unwrap()))
                    .unwrap();
            assert_eq!(result["ok"], false, "{}: {result}", format.extension());
            assert_eq!(engine.export_document().unwrap(), initial);
        }
    }
    let source = OriginalSource::new(
        "txt",
        "TEXT/PLAIN",
        "original.txt",
        b"Text original".as_slice(),
    )
    .unwrap();
    let file = WriterFile::with_original_source(document, source).unwrap();
    let result: Value =
        serde_json::from_str(&engine.open_document(&serialize_writer_file(&file).unwrap()))
            .unwrap();
    assert_eq!(
        result["ok"], true,
        "Media type matching is case-insensitive: {result}"
    );
}

#[test]
fn outlined_pdf_exports_from_the_existing_rust_layout_and_never_mutates_state() {
    let engine = formatted_engine("pdf-export");
    let before = engine.snapshot();
    let bytes = engine.export_format("pdf", true, None).unwrap();
    assert!(bytes.starts_with(b"%PDF-"));
    assert!(bytes.windows(5).any(|chunk| chunk == b"%%EOF"));
    assert_eq!(engine.snapshot(), before);
}

#[test]
fn rejected_conversion_does_not_consume_identifiers_for_the_next_document() {
    let mut rejected = BrowserEngine::new("rejected-id-allocation").unwrap();
    let mut untouched = BrowserEngine::new("rejected-id-allocation").unwrap();
    let response: Value =
        serde_json::from_str(&rejected.import_format(b"invalid ZIP", "odt", "broken.odt")).unwrap();
    assert_eq!(response["ok"], false);
    let next = dispatch(&mut rejected, json!({"type":"new_document"}));
    let expected = dispatch(&mut untouched, json!({"type":"new_document"}));
    assert_eq!(next, expected);
}

#[test]
fn unsupported_rich_import_returns_diagnostics_without_publishing_partial_text() {
    let mut engine = BrowserEngine::new("preserved-rich-import").unwrap();
    dispatch(
        &mut engine,
        json!({"type":"replace_text", "text":"Keep my work"}),
    );
    let before = engine.snapshot();
    let bytes = engine.export_document().unwrap();
    let source = br"{\rtf1\ansi Supported prefix {\pict\pngblip 00} suffix\par}";
    let result: Value =
        serde_json::from_str(&engine.import_format(source, "rtf", "unsupported.rtf")).unwrap();
    assert_eq!(result["ok"], false, "{result}");
    assert_eq!(result["error"]["code"], "unsupported_format");
    let report = result["error"]["compatibility"]["diagnostics"]
        .as_array()
        .unwrap();
    assert!(!report.is_empty());
    assert!(
        report
            .iter()
            .all(|diagnostic| diagnostic["message"].as_str().unwrap().len() <= 2048)
    );
    assert_eq!(engine.snapshot(), before);
    assert_eq!(engine.export_document().unwrap(), bytes);
    dispatch(&mut engine, json!({"type":"undo"}));
    assert_eq!(paragraph_text(&view(&engine)["paragraphs"][0]), "");
}
