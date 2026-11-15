//! Successful import notices remain runtime state while originals stay exact.

use std::{fs, path::Path};

use office_core::{Id, WriterFile, deserialize_writer_file, serialize_writer_file};
use office_docx::DocxImportOutcome;
use office_formats::{DocumentFormat, ImportOutcome};
use office_wasm::BrowserEngine;
use serde_json::{Value, json};

const PROFILE: &str = "normalizable-wordweb-profile.docx";

fn fixture(name: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/docx/relationships")
            .join(name),
    )
    .unwrap()
}
fn accept(response: &str) -> Value {
    let value: Value = serde_json::from_str(response).unwrap();
    assert_eq!(value["ok"], true, "{value}");
    value["view"].clone()
}
fn execute(engine: &mut BrowserEngine, value: Value) -> Value {
    accept(&engine.dispatch(&value.to_string()))
}
fn paragraph_text(value: &Value) -> String {
    value["runs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|run| run["text"].as_str().unwrap())
        .collect()
}
fn warnings(value: &Value) -> &Value {
    &value["importWarnings"]
}
fn assert_warned(value: &Value) {
    assert!(
        !warnings(value)["diagnostics"]
            .as_array()
            .unwrap()
            .is_empty(),
        "{value}"
    );
    assert!(
        warnings(value)["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .all(|notice| notice["kind"] == "normalized_formatting")
    );
}

#[test]
fn format_facade_returns_the_same_single_import_model_and_normalization_report() {
    let source = fixture(PROFILE);
    let document_id = Id::new("normalized-facade").unwrap();
    let DocxImportOutcome::Editable(imported) =
        office_docx::import_docx(&source, document_id.clone()).unwrap()
    else {
        panic!("approved normalization must return an editable document");
    };
    assert!(imported.report().is_supported());
    assert!(!imported.report().normalizations.is_empty());
    let reported = office_formats::import_document_with_warnings(
        DocumentFormat::Docx,
        &source,
        document_id,
        "source.docx",
    )
    .unwrap();
    assert_eq!(
        reported.warnings.len(),
        imported.report().normalizations.len()
    );
    assert_eq!(
        reported.omitted_warnings,
        imported.report().omitted_normalizations
    );
    for (actual, expected) in reported
        .warnings
        .iter()
        .zip(&imported.report().normalizations)
    {
        assert_eq!(actual.code, expected.kind.code());
        assert_eq!(actual.part, expected.part);
        assert_eq!(actual.path, expected.path);
        assert_eq!(actual.message, expected.message);
    }
    let ImportOutcome::Editable(file) = reported.outcome else {
        panic!("normalization remains editable");
    };
    assert_eq!(file.document(), imported.document());
    assert_eq!(file.original_source().unwrap().bytes(), source);
}

#[test]
fn both_browser_import_apis_expose_warnings_and_native_reopen_preserves_edited_content() {
    let source = fixture(PROFILE);
    for generic in [false, true] {
        let mut engine = BrowserEngine::new(if generic {
            "normalized-generic"
        } else {
            "normalized-legacy"
        })
        .unwrap();
        let response = if generic {
            engine.import_format(&source, "docx", "source.docx")
        } else {
            engine.import_docx(&source, "source.docx")
        };
        let opened = accept(&response);
        assert_warned(&opened);
        assert_eq!(opened["paragraphs"].as_array().unwrap().len(), 9);
        assert_eq!(engine.export_original().unwrap(), source);
        let first = &opened["paragraphs"][0];
        let edited = execute(
            &mut engine,
            json!({"type":"replace_text", "text":"Edited Café e\u{301} 👩🏽‍💻", "selection": {
                "anchor":{"paragraphId":first["id"],"utf16":0},
                "focus":{"paragraphId":first["id"],"utf16":paragraph_text(first).encode_utf16().count()}
            }}),
        );
        assert_eq!(warnings(&edited), warnings(&opened));
        let native = engine.export_document().unwrap();
        let file = deserialize_writer_file(&native).unwrap();
        assert_eq!(file.original_source().unwrap().bytes(), source);
        let envelope: Value = serde_json::from_slice(&native).unwrap();
        assert!(envelope.get("importWarnings").is_none());
        assert!(envelope["document"].get("importWarnings").is_none());
        let mut reopened = BrowserEngine::new("normalized-native-reopen").unwrap();
        let restored = accept(
            &reopened.open_document_verified(&native, opened["documentId"].as_str().unwrap()),
        );
        assert_eq!(restored["paragraphs"], edited["paragraphs"]);
        assert_eq!(warnings(&restored), warnings(&opened));
        assert_eq!(reopened.export_document().unwrap(), native);
        assert_eq!(reopened.export_original().unwrap(), source);
        let copy = reopened.export_format("docx", true, None).unwrap();
        let mut converted = BrowserEngine::new("normalized-current-copy").unwrap();
        let result = accept(&converted.import_format(&copy, "docx", "edited.docx"));
        assert_eq!(
            paragraph_text(&result["paragraphs"][0]),
            "Edited Café e\u{301} 👩🏽‍💻"
        );
    }
}

#[test]
fn normalization_notices_follow_source_lifecycle_and_never_bypass_atomic_rejection() {
    let source = fixture(PROFILE);
    let mut engine = BrowserEngine::new("normalization-lifecycle").unwrap();
    let opened = accept(&engine.import_format(&source, "docx", "source.docx"));
    assert_warned(&opened);
    let expected = warnings(&opened).clone();
    for operation in [
        json!({"type":"set_title","title":"Edited"}),
        json!({"type":"undo"}),
        json!({"type":"redo"}),
        json!({"type":"fork_document"}),
    ] {
        assert_eq!(warnings(&execute(&mut engine, operation)), &expected);
    }
    let before = engine.snapshot();
    let native = engine.export_document().unwrap();
    for name in [
        "unsupported-image.docx",
        "unsupported-active-document-protection.docx",
        "unsupported-active-track-revisions.docx",
        "unsupported-normalization-flood.docx",
    ] {
        let result: Value =
            serde_json::from_str(&engine.import_format(&fixture(name), "docx", name)).unwrap();
        assert_eq!(result["ok"], false, "{name}: {result}");
        assert!(
            !result["error"]["compatibility"]["diagnostics"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_eq!(engine.snapshot(), before);
        assert_eq!(engine.export_document().unwrap(), native);
        assert_eq!(engine.export_original().unwrap(), source);
    }
    let text = accept(&engine.import_format(b"New plain text", "txt", "new.txt"));
    assert_eq!(warnings(&text)["diagnostics"], json!([]));
    assert_eq!(warnings(&text)["omittedDiagnostics"], 0);
    let fresh = execute(&mut engine, json!({"type":"new_document"}));
    assert_eq!(warnings(&fresh)["diagnostics"], json!([]));
    assert_eq!(warnings(&fresh)["omittedDiagnostics"], 0);
    let (document, _) = deserialize_writer_file(&native).unwrap().into_parts();
    let without_source = serialize_writer_file(&WriterFile::new(document)).unwrap();
    let restored = accept(&engine.open_document(&without_source));
    assert_eq!(warnings(&restored)["diagnostics"], json!([]));
}

#[test]
fn genuine_public_word_and_libreoffice_files_are_editable_with_disclosed_approximations() {
    for name in [
        "microsoft-sdk-hello-world.docx",
        "microsoft-sdk-plain.docx",
        "libreoffice-basic.docx",
    ] {
        let source = fixture(name);
        let mut engine = BrowserEngine::new(name).unwrap();
        let opened = accept(&engine.import_format(&source, "docx", name));
        assert_warned(&opened);
        assert!(!opened["paragraphs"].as_array().unwrap().is_empty());
        assert_eq!(engine.export_original().unwrap(), source);
        let native = engine.export_document().unwrap();
        let mut reopened = BrowserEngine::new(&format!("native-{name}")).unwrap();
        let restored = accept(&reopened.open_document(&native));
        assert_eq!(restored["paragraphs"], opened["paragraphs"]);
        assert_eq!(warnings(&restored), warnings(&opened));
        assert_eq!(reopened.export_original().unwrap(), source);
    }
}

#[test]
fn rejected_legacy_and_generic_imports_do_not_consume_later_identifiers() {
    let source = fixture(PROFILE);
    let protected = fixture("unsupported-active-document-protection.docx");
    for generic in [false, true] {
        let mut rejected = BrowserEngine::new("docx-import-id-rollback").unwrap();
        let mut untouched = BrowserEngine::new("docx-import-id-rollback").unwrap();
        for engine in [&mut rejected, &mut untouched] {
            let response = if generic {
                engine.import_format(&source, "docx", "source.docx")
            } else {
                engine.import_docx(&source, "source.docx")
            };
            assert_warned(&accept(&response));
        }
        let response = if generic {
            rejected.import_format(&protected, "docx", "protected.docx")
        } else {
            rejected.import_docx(&protected, "protected.docx")
        };
        assert_eq!(
            serde_json::from_str::<Value>(&response).unwrap()["ok"],
            false
        );
        assert_eq!(rejected.snapshot(), untouched.snapshot());
        let next = execute(&mut rejected, json!({"type":"new_document"}));
        let expected = execute(&mut untouched, json!({"type":"new_document"}));
        assert_eq!(next, expected);
    }
}

#[test]
fn empty_paragraphs_do_not_make_selected_text_formatting_appear_mixed() {
    let mut engine = BrowserEngine::new("blank-paragraph-summary").unwrap();
    let inserted = execute(
        &mut engine,
        json!({"type":"replace_text","text":"first\n\nlast"}),
    );
    let paragraphs = inserted["paragraphs"].as_array().unwrap();
    assert_eq!(paragraphs.len(), 3);
    let first = paragraphs[0]["id"].clone();
    let blank = paragraphs[1]["id"].clone();
    let last = paragraphs[2]["id"].clone();
    let selected =
        json!({"anchor":{"paragraphId":first,"utf16":0},"focus":{"paragraphId":last,"utf16":4}});
    let formatted = execute(
        &mut engine,
        json!({"type":"format_character","bold":true,"italic":true,"fontFamily":"Tinos","fontSizePt":14,"selection":selected}),
    );
    assert_eq!(formatted["format"]["bold"], true);
    assert_eq!(formatted["format"]["italic"], true);
    assert_eq!(formatted["format"]["fontFamily"], "Tinos");
    assert_eq!(formatted["format"]["fontSizePt"], 14.0);
    let aligned = execute(
        &mut engine,
        json!({"type":"format_paragraph","alignment":"center","selection":selected}),
    );
    assert_eq!(aligned["format"]["alignment"], "center");
    assert_eq!(aligned["format"]["bold"], true);
    let blank_only = execute(
        &mut engine,
        json!({"type":"set_selection","selection":{"anchor":{"paragraphId":first,"utf16":5},"focus":{"paragraphId":last,"utf16":0}}}),
    );
    assert_eq!(blank_only["format"]["bold"], false);
    assert_eq!(blank_only["format"]["fontFamily"], "Arimo");
    let collapsed = execute(
        &mut engine,
        json!({"type":"set_selection","selection":{"anchor":{"paragraphId":blank,"utf16":0},"focus":{"paragraphId":blank,"utf16":0}}}),
    );
    assert_eq!(collapsed["format"]["bold"], false);
    assert_eq!(collapsed["format"]["fontFamily"], "Arimo");
    assert_eq!(
        collapsed["paragraphs"]
            .as_array()
            .unwrap()
            .iter()
            .map(paragraph_text)
            .collect::<Vec<_>>(),
        ["first", "", "last"]
    );
}
