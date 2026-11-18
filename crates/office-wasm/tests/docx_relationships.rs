//! Black-box DOCX relationship policy, exact sources and browser-session integrity.

use std::{fs, path::Path};

use office_core::{OriginalSource, WriterFile, deserialize_writer_file, serialize_writer_file};
use office_docx::{DOCX_MEDIA_TYPE, DocxImportOutcome};
use office_wasm::BrowserEngine;
use serde_json::{Value, json};

const SAFE: &str = "supported-auxiliary-parts.docx";
const EDITED: &str = "Edited Café e\u{301} 👩🏽‍💻 — العربية नमस्ते שלום";
const UNSUPPORTED: &[&str] = &[
    "unsupported-image.docx",
    "unsupported-header.docx",
    "unsupported-footer.docx",
    "unsupported-revision.docx",
    "unsupported-altchunk.docx",
    "unsupported-external-relationship.docx",
    "misplaced-root-font-table.docx",
    "misplaced-main-app-properties.docx",
    "unknown-theme-relationship.docx",
    "unsupported-font-embedding.docx",
    "unsupported-auxiliary-external-chain.docx",
    "unsupported-main-image-same-thumbnail.docx",
    "unsupported-malformed-default-session-id.docx",
    "unsupported-active-document-protection.docx",
    "unsupported-active-track-revisions.docx",
    "unsupported-normalization-flood.docx",
];
const MALFORMED: &[&str] = &[
    "mismatched-auxiliary-mime.docx",
    "mismatched-auxiliary-root.docx",
    "duplicate-settings-relationship.docx",
];

fn fixture(name: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/docx/relationships")
            .join(name),
    )
    .unwrap_or_else(|error| panic!("DOCX relationship fixture {name}: {error}"))
}

fn accept(json: &str) -> Value {
    let response: Value = serde_json::from_str(json).unwrap();
    assert_eq!(response["ok"], true, "{response}");
    response["view"].clone()
}

fn execute(engine: &mut BrowserEngine, input: Value) -> Value {
    accept(&engine.dispatch(&input.to_string()))
}

fn import(engine: &mut BrowserEngine, bytes: &[u8], generic: bool) -> Value {
    let json = if generic {
        engine.import_format(bytes, "docx", "元の文書.docx")
    } else {
        engine.import_docx(bytes, "元の文書.docx")
    };
    serde_json::from_str(&json).unwrap()
}

fn text(paragraph: &Value) -> String {
    paragraph["runs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|run| run["text"].as_str().unwrap())
        .collect()
}

fn auxiliary_content_type(value: &str) -> bool {
    matches!(
        value,
        "application/vnd.openxmlformats-officedocument.theme+xml"
            | "application/vnd.openxmlformats-officedocument.wordprocessingml.fontTable+xml"
            | "application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"
            | "application/vnd.openxmlformats-officedocument.wordprocessingml.webSettings+xml"
            | "application/vnd.openxmlformats-officedocument.extended-properties+xml"
            | "application/vnd.openxmlformats-officedocument.custom-properties+xml"
    )
}

fn requires_relationship_diagnostic(name: &str) -> bool {
    matches!(
        name,
        "unsupported-image.docx"
            | "unsupported-header.docx"
            | "unsupported-footer.docx"
            | "unsupported-altchunk.docx"
            | "unsupported-external-relationship.docx"
            | "misplaced-root-font-table.docx"
            | "misplaced-main-app-properties.docx"
            | "unknown-theme-relationship.docx"
            | "unsupported-auxiliary-external-chain.docx"
            | "unsupported-main-image-same-thumbnail.docx"
    )
}

#[test]
fn harmless_parts_survive_copy_export_without_replacing_current_text_or_mutating_the_source() {
    for name in [
        SAFE,
        "supported-authoring-metadata.docx",
        "supported-strict-auxiliary-parts.docx",
        "supported-strict-authoring-metadata.docx",
    ] {
        let source = fixture(name);
        assert!(
            office_docx::inspect_docx(&source)
                .unwrap()
                .parts
                .iter()
                .any(|part| auxiliary_content_type(&part.content_type))
        );
        for generic in [false, true] {
            let mut engine =
                BrowserEngine::new(if generic { "aux-generic" } else { "aux-legacy" }).unwrap();
            let imported = import(&mut engine, &source, generic);
            assert_eq!(imported["ok"], true, "{imported}");
            let initial = imported["view"].clone();
            assert_eq!(initial["originalSource"]["filename"], "元の文書.docx");
            assert_eq!(engine.export_original().unwrap(), source);
            let native = engine.export_document().unwrap();
            let file = deserialize_writer_file(&native).unwrap();
            assert_eq!(file.original_source().unwrap().bytes(), source);
            let mut reopened = BrowserEngine::new(if generic {
                "aux-generic-reopened"
            } else {
                "aux-legacy-reopened"
            })
            .unwrap();
            let opened = accept(
                &reopened.open_document_verified(&native, initial["documentId"].as_str().unwrap()),
            );
            assert_eq!(opened["paragraphs"], initial["paragraphs"]);
            assert_eq!(opened["documentId"], initial["documentId"]);
            assert_eq!(reopened.export_document().unwrap(), native);
            assert_eq!(reopened.export_original().unwrap(), source);

            let first = &opened["paragraphs"][0];
            let original_text = text(first);
            let paragraph_id = first["id"].clone();
            let edited = execute(
                &mut reopened,
                json!({"type":"replace_text","text":EDITED,"selection":{
                    "anchor":{"paragraphId":paragraph_id,"utf16":0},
                    "focus":{"paragraphId":paragraph_id,"utf16":original_text.encode_utf16().count()}
                }}),
            );
            assert_eq!(text(&edited["paragraphs"][0]), EDITED);
            let formatted = execute(
                &mut reopened,
                json!({"type":"format_character","bold":true,"italic":true,"underline":true,"fontFamily":"Tinos","fontSizePt":14,"selection":{
                    "anchor":{"paragraphId":paragraph_id,"utf16":0},
                    "focus":{"paragraphId":paragraph_id,"utf16":EDITED.encode_utf16().count()}
                }}),
            );
            assert_eq!(text(&formatted["paragraphs"][0]), EDITED);
            execute(&mut reopened, json!({"type":"undo"}));
            let restored = execute(&mut reopened, json!({"type":"undo"}));
            assert_eq!(text(&restored["paragraphs"][0]), original_text);
            execute(&mut reopened, json!({"type":"redo"}));
            execute(&mut reopened, json!({"type":"redo"}));
            let current = execute(
                &mut reopened,
                json!({"type":"set_title","title":"Current DOCX copy"}),
            );
            let before_snapshot = reopened.snapshot();
            let before_native = reopened.export_document().unwrap();
            let assessment: Value = serde_json::from_str(
                &reopened
                    .export_assessment("docx", Some(current["revision"].as_str().unwrap()))
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(assessment["warnings"], json!([]), "{name}: {assessment}");
            let copied = reopened
                .export_format("docx", false, Some(current["revision"].as_str().unwrap()))
                .unwrap();
            assert_eq!(reopened.snapshot(), before_snapshot, "{name}");
            assert_eq!(reopened.export_document().unwrap(), before_native, "{name}");
            let inspection = office_docx::inspect_docx(&copied).unwrap();
            let original_inspection = office_docx::inspect_docx(&source).unwrap();
            for part in original_inspection
                .parts
                .iter()
                .filter(|part| auxiliary_content_type(&part.content_type))
            {
                assert!(
                    inspection
                        .parts
                        .iter()
                        .any(|copied_part| copied_part.content_type == part.content_type),
                    "{name}: omitted safe {}",
                    part.content_type
                );
            }
            let DocxImportOutcome::Editable(copy) = office_docx::import_docx(
                &copied,
                office_core::Id::new("regenerated-current-copy").unwrap(),
            )
            .unwrap() else {
                panic!("the current-content DOCX copy must remain editable");
            };
            let office_core::Block::Paragraph(paragraph) =
                &copy.document().sections()[0].blocks()[0]
            else {
                panic!("expected paragraph")
            };
            assert_eq!(copy.document().metadata().title, "Current DOCX copy");
            assert_eq!(
                paragraph
                    .runs()
                    .iter()
                    .map(|run| run.text.as_str())
                    .collect::<String>(),
                EDITED
            );
            assert!(
                paragraph
                    .runs()
                    .iter()
                    .filter(|run| !run.text.is_empty())
                    .all(|run| run.format.bold == Some(true) && run.format.italic == Some(true))
            );
            assert_eq!(reopened.export_original().unwrap(), source);
            assert_eq!(
                deserialize_writer_file(&reopened.export_document().unwrap())
                    .unwrap()
                    .original_source()
                    .unwrap()
                    .bytes(),
                source
            );
            let undone = execute(&mut reopened, json!({"type":"undo"}));
            assert_eq!(undone["title"], initial["title"]);
            assert_eq!(text(&undone["paragraphs"][0]), EDITED);
            let redone = execute(&mut reopened, json!({"type":"redo"}));
            assert_eq!(redone["title"], current["title"]);
            assert_eq!(redone["paragraphs"], current["paragraphs"]);
            assert_eq!(redone["selection"], current["selection"]);
        }
    }
}

#[test]
fn unsafe_relationships_still_preserve_live_content_history_and_original() {
    let safe = fixture(SAFE);
    for generic in [false, true] {
        let mut engine = BrowserEngine::new(if generic {
            "unsafe-generic"
        } else {
            "unsafe-legacy"
        })
        .unwrap();
        let opened = import(&mut engine, &safe, generic);
        assert_eq!(opened["ok"], true, "{opened}");
        let title = opened["view"]["title"].clone();
        execute(
            &mut engine,
            json!({"type":"set_title","title":"Unsaved relationship review"}),
        );
        let before = engine.snapshot();
        let native = engine.export_document().unwrap();
        for name in UNSUPPORTED {
            let response = import(&mut engine, &fixture(name), generic);
            assert_eq!(response["ok"], false, "{name}: {response}");
            assert_eq!(
                response["error"]["code"],
                if generic {
                    "unsupported_format"
                } else {
                    "unsupported_docx"
                },
                "{name}: {response}"
            );
            let diagnostics = response["error"]["compatibility"]["diagnostics"]
                .as_array()
                .unwrap();
            assert!(!diagnostics.is_empty(), "{name}: {response}");
            if requires_relationship_diagnostic(name) {
                assert!(
                    diagnostics
                        .iter()
                        .any(|diagnostic| diagnostic["kind"] == "unsupported_relationship"),
                    "{name}: {response}"
                );
            }
            assert_eq!(engine.snapshot(), before, "{name}");
            assert_eq!(engine.export_document().unwrap(), native, "{name}");
            assert_eq!(engine.export_original().unwrap(), safe, "{name}");
        }
        assert_eq!(execute(&mut engine, json!({"type":"undo"}))["title"], title);
    }
}

#[test]
fn unsafe_originals_in_native_files_are_revalidated_before_replacement() {
    let safe = fixture(SAFE);
    let mut engine = BrowserEngine::new("revalidate-relationship-source").unwrap();
    let response = import(&mut engine, &safe, true);
    assert_eq!(response["ok"], true, "{response}");
    execute(
        &mut engine,
        json!({"type":"set_title","title":"Keep this session"}),
    );
    let before = engine.snapshot();
    let native = engine.export_document().unwrap();
    let (document, _) = deserialize_writer_file(&native).unwrap().into_parts();
    for name in UNSUPPORTED {
        let source = OriginalSource::new("docx", DOCX_MEDIA_TYPE, *name, fixture(name)).unwrap();
        let file = WriterFile::with_original_source(document.clone(), source).unwrap();
        let response: Value =
            serde_json::from_str(&engine.open_document(&serialize_writer_file(&file).unwrap()))
                .unwrap();
        assert_eq!(response["ok"], false, "{name}: {response}");
        assert_eq!(
            response["error"]["code"], "unsupported_docx",
            "{name}: {response}"
        );
        assert_eq!(engine.snapshot(), before);
        assert_eq!(engine.export_document().unwrap(), native);
        assert_eq!(engine.export_original().unwrap(), safe);
    }
}

#[test]
fn ordinary_docx_copies_do_not_warn_about_preserved_auxiliaries() {
    let mut engine = BrowserEngine::new("docx-warning-scope").unwrap();
    let report: Value =
        serde_json::from_str(&engine.export_assessment("docx", None).unwrap()).unwrap();
    assert_eq!(report["warnings"], json!([]));
    let response = import(&mut engine, &fixture(SAFE), true);
    assert_eq!(response["ok"], true, "{response}");
    let before = engine.export_document().unwrap();
    let report: Value =
        serde_json::from_str(&engine.export_assessment("docx", None).unwrap()).unwrap();
    assert_eq!(report["warnings"], json!([]));
    assert!(
        !engine
            .export_format("docx", false, None)
            .unwrap()
            .is_empty()
    );
    for format in ["writer", "odt", "rtf", "txt"] {
        let report: Value =
            serde_json::from_str(&engine.export_assessment(format, None).unwrap()).unwrap();
        assert!(
            !report["warnings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|warning| warning["code"] == "docx_original_parts")
        );
    }
    assert_eq!(engine.export_document().unwrap(), before);
}

#[test]
fn active_source_formatting_changes_still_require_review_without_changing_current_state() {
    let source = fixture("unsupported-theme-font-reference.docx");
    let mut engine = BrowserEngine::new("meaningful-docx-formatting").unwrap();
    let response = import(&mut engine, &source, true);
    assert_eq!(response["ok"], true, "{response}");
    let current = execute(
        &mut engine,
        json!({"type":"set_title","title":"Edited with a normalized font"}),
    );
    let before = engine.snapshot();
    let native = engine.export_document().unwrap();
    let report: Value =
        serde_json::from_str(&engine.export_assessment("docx", None).unwrap()).unwrap();
    assert!(
        report["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| warning["code"] == "docx_formatting_changes"),
        "{report}"
    );
    assert_eq!(
        engine.export_format("docx", false, None).unwrap_err().code,
        "export_confirmation_required"
    );
    assert_eq!(engine.snapshot(), before);
    assert_eq!(engine.export_document().unwrap(), native);
    let bytes = engine.export_format("docx", true, None).unwrap();
    let mut copy = BrowserEngine::new("normalized-copy-reopened").unwrap();
    let opened = import(&mut copy, &bytes, true);
    assert_eq!(opened["ok"], true, "{opened}");
    assert_eq!(opened["view"]["title"], current["title"]);
    assert_eq!(
        text(&opened["view"]["paragraphs"][0]),
        text(&current["paragraphs"][0])
    );
    assert_eq!(engine.snapshot(), before);
    assert_eq!(engine.export_document().unwrap(), native);
    assert_eq!(engine.export_original().unwrap(), source);
}

#[test]
fn malformed_or_ambiguous_auxiliary_parts_cannot_replace_the_current_session() {
    let safe = fixture(SAFE);
    for generic in [false, true] {
        let mut engine = BrowserEngine::new(if generic {
            "malformed-aux-generic"
        } else {
            "malformed-aux-legacy"
        })
        .unwrap();
        assert_eq!(import(&mut engine, &safe, generic)["ok"], true);
        execute(
            &mut engine,
            json!({"type":"set_title","title":"Preserve this version"}),
        );
        let before = engine.snapshot();
        let native = engine.export_document().unwrap();
        for name in MALFORMED {
            let result = import(&mut engine, &fixture(name), generic);
            assert_eq!(result["ok"], false, "{name}: {result}");
            assert!(
                matches!(
                    result["error"]["code"].as_str(),
                    Some("invalid_docx" | "unsupported_docx" | "unsupported_format")
                ),
                "{name}: {result}"
            );
            assert_eq!(engine.snapshot(), before);
            assert_eq!(engine.export_document().unwrap(), native);
            assert_eq!(engine.export_original().unwrap(), safe);
        }
    }
}

#[test]
fn package_thumbnail_is_preserved_but_the_same_image_is_not_admitted_as_document_content() {
    let source = fixture("supported-root-thumbnail.docx");
    let unsafe_source = fixture("unsupported-main-image-same-thumbnail.docx");
    for generic in [false, true] {
        let mut engine = BrowserEngine::new(if generic {
            "thumbnail-generic"
        } else {
            "thumbnail-legacy"
        })
        .unwrap();
        let result = import(&mut engine, &source, generic);
        assert_eq!(result["ok"], true, "{result}");
        assert_eq!(engine.export_original().unwrap(), source);
        let native = engine.export_document().unwrap();
        let mut reopened = BrowserEngine::new("thumbnail-native-reopen").unwrap();
        accept(&reopened.open_document(&native));
        assert_eq!(reopened.export_document().unwrap(), native);
        let before = reopened.snapshot();
        let blocked = import(&mut reopened, &unsafe_source, generic);
        assert_eq!(blocked["ok"], false, "{blocked}");
        assert_eq!(reopened.snapshot(), before);
        assert_eq!(reopened.export_original().unwrap(), source);
        let current_copy = reopened.export_format("docx", true, None).unwrap();
        assert!(
            !office_docx::inspect_docx(&current_copy)
                .unwrap()
                .parts
                .iter()
                .any(|part| part.content_type.starts_with("image/"))
        );
    }
}
