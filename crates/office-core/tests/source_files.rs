//! Full native-file source retention, version boundaries and resource contracts.

use std::sync::Arc;

use office_core::{
    Block, CharacterFormat, Document, DocumentError, DocumentMetadata, DocumentSession,
    GraphemeOffset, Id, MAX_ORIGINAL_SOURCE_BYTES, MAX_SOURCE_FILENAME_BYTES,
    MAX_SOURCE_FORMAT_BYTES, MAX_SOURCE_MEDIA_TYPE_BYTES, MAX_WRITER_FILE_BYTES, OriginalSource,
    Paragraph, PersistenceError, SCHEMA_VERSION, Section, TextRun, WRITER_FORMAT_VERSION,
    WriterFile, deserialize_document, deserialize_writer_file, serialize_document,
    serialize_writer_file,
};
use serde_json::{Value, json};

const LEGACY: &[u8] = include_bytes!("../../../tests/fixtures/native/writer-v1.writer");
const ORIGINAL: &[u8] =
    include_bytes!("../../../tests/fixtures/docx/writer-source-preservation.docx");
const MIME: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.document";

fn id(value: &str) -> Id {
    Id::new(value).unwrap()
}

fn document() -> Document {
    Document::from_parts(
        id("document"),
        DocumentMetadata {
            title: "Source retention — 世界".into(),
        },
        vec![Section::new(
            id("section"),
            vec![Block::Paragraph(Paragraph::new(
                id("paragraph"),
                vec![TextRun::new("Original text — e\u{301} 👩🏽‍💻")],
            ))],
        )],
        vec![],
    )
    .unwrap()
}

fn source() -> OriginalSource {
    OriginalSource::new("docx", MIME, "原稿 — e\u{301} — 📝.docx", ORIGINAL).unwrap()
}

fn file() -> WriterFile {
    WriterFile::with_original_source(document(), source()).unwrap()
}

fn payload() -> Value {
    serde_json::from_slice(&serialize_writer_file(&file()).unwrap()).unwrap()
}

fn decode(value: &Value) -> Result<WriterFile, PersistenceError> {
    deserialize_writer_file(&serde_json::to_vec(value).unwrap())
}

#[test]
fn complete_file_preserves_supported_model_metadata_and_exact_original() {
    let original = file();
    let bytes = serialize_writer_file(&original).unwrap();
    let restored = deserialize_writer_file(&bytes).unwrap();
    assert_eq!(restored, original);
    assert_eq!(restored.document(), &document());
    let source = restored.original_source().unwrap();
    assert_eq!(source.format(), "docx");
    assert_eq!(source.media_type(), MIME);
    assert_eq!(source.filename(), "原稿 — e\u{301} — 📝.docx");
    assert_eq!(source.bytes(), ORIGINAL);
    assert_eq!(serialize_writer_file(&restored).unwrap(), bytes);
    assert_eq!(restored.into_parts(), (document(), Some(self::source())));
}

#[test]
fn current_geometry_and_explicit_page_break_values_survive_full_file_round_trip() {
    for page_break_before in [false, true] {
        let mut value = payload();
        value["document"]["sections"][0]["properties"] = json!({
            "page_size": { "width": 12240, "height": 15840, "orientation": "portrait" },
            "page_margins": { "top": 1440, "right": 1440, "bottom": 1440,
                "left": 1440, "header": 720, "footer": 720, "gutter": 0 }
        });
        value["document"]["sections"][0]["blocks"][0]["content"]["format"]["page_break_before"] =
            json!(page_break_before);
        let file = decode(&value).unwrap();
        let serialized = serialize_writer_file(&file).unwrap();
        assert_eq!(serde_json::from_slice::<Value>(&serialized).unwrap(), value);
        let restored = deserialize_writer_file(&serialized).unwrap();
        assert_eq!(restored, file);
        assert_eq!(restored.original_source().unwrap().bytes(), ORIGINAL);
    }
}

#[test]
fn current_envelope_is_explicit_without_adding_source_state_to_the_model() {
    let value = payload();
    assert_eq!(value["format_version"], WRITER_FORMAT_VERSION);
    assert_eq!(value["document"]["schema_version"], SCHEMA_VERSION);
    assert_eq!(value["source"].as_object().unwrap().len(), 4);
    assert!(value["source"]["bytes_base64"].as_str().unwrap().is_ascii());
    assert!(value["document"].get("source").is_none());
    assert!(value.get("history").is_none());
    assert!(value.get("revision").is_none());
    let native_model = document().to_json().unwrap();
    assert!(!native_model.contains("bytes_base64"));
    let without_source = WriterFile::new(document());
    assert_eq!(
        serialize_writer_file(&without_source).unwrap(),
        serialize_document(&document()).unwrap()
    );
    assert!(
        serde_json::from_slice::<Value>(&serialize_writer_file(&without_source).unwrap())
            .unwrap()
            .get("source")
            .is_none()
    );
}

#[test]
fn legacy_file_migrates_explicitly_and_is_only_rewritten_when_serialized() {
    let original_bytes = LEGACY.to_vec();
    let loaded = deserialize_writer_file(LEGACY).unwrap();
    assert!(loaded.original_source().is_none());
    assert_eq!(loaded.document().schema_version(), SCHEMA_VERSION);
    assert_eq!(loaded.document(), &deserialize_document(LEGACY).unwrap());
    assert_eq!(original_bytes, LEGACY);
    let rewritten = serialize_writer_file(&loaded).unwrap();
    let value: Value = serde_json::from_slice(&rewritten).unwrap();
    assert_eq!(value["format_version"], WRITER_FORMAT_VERSION);
    assert_eq!(value["document"]["schema_version"], SCHEMA_VERSION);
    assert_eq!(deserialize_writer_file(&rewritten).unwrap(), loaded);
}

#[test]
fn envelope_and_model_versions_cannot_be_mixed_or_reinterpreted() {
    let mut current = payload();
    current.as_object_mut().unwrap().remove("source");
    current["format_version"] = json!(1);
    assert!(matches!(
        decode(&current),
        Err(PersistenceError::IncompatibleModelSchema {
            format_version: 1,
            found: 5,
            expected: 1
        })
    ));
    let mut legacy: Value = serde_json::from_slice(LEGACY).unwrap();
    legacy["format_version"] = json!(2);
    assert!(matches!(
        decode(&legacy),
        Err(PersistenceError::IncompatibleModelSchema {
            format_version: 2,
            found: 1,
            expected: 2
        })
    ));
    let mut legacy: Value = serde_json::from_slice(LEGACY).unwrap();
    legacy["source"] = payload()["source"].clone();
    assert!(matches!(decode(&legacy), Err(PersistenceError::Json(_))));
}

#[test]
fn document_only_loader_explicitly_refuses_to_discard_retained_original() {
    let bytes = serialize_writer_file(&file()).unwrap();
    assert!(matches!(
        deserialize_document(&bytes),
        Err(PersistenceError::SourcePresent)
    ));
    assert_eq!(
        deserialize_writer_file(&bytes)
            .unwrap()
            .original_source()
            .unwrap()
            .bytes(),
        ORIGINAL
    );
}

#[test]
fn source_clones_share_immutable_owned_bytes_without_debug_content() {
    let original: Arc<[u8]> = Arc::from(b"private original payload".as_slice());
    let source = OriginalSource::new(
        "example",
        "application/octet-stream",
        "source.bin",
        original.clone(),
    )
    .unwrap();
    let clone = source.clone();
    assert_eq!(source.bytes().as_ptr(), original.as_ptr());
    assert_eq!(clone.bytes().as_ptr(), source.bytes().as_ptr());
    assert!(!format!("{source:?}").contains("private original payload"));
    let mut bytes = b"independent input".to_vec();
    let source = OriginalSource::new(
        "example",
        "application/octet-stream",
        "source.bin",
        bytes.as_slice(),
    )
    .unwrap();
    bytes.fill(0);
    assert_eq!(source.bytes(), b"independent input");
}

#[test]
fn custom_input_conversion_cannot_bypass_retained_byte_invariants() {
    struct InconsistentInput(Arc<[u8]>);

    impl AsRef<[u8]> for InconsistentInput {
        fn as_ref(&self) -> &[u8] {
            b"x"
        }
    }

    impl From<InconsistentInput> for Arc<[u8]> {
        fn from(input: InconsistentInput) -> Self {
            input.0
        }
    }

    for actual in [b"".as_slice(), b"xy".as_slice()] {
        let input = InconsistentInput(Arc::from(actual));
        assert!(matches!(
            OriginalSource::new("example", "application/octet-stream", "source.bin", input),
            Err(PersistenceError::InvalidSourceMetadata { field: "bytes", .. })
        ));
    }
}

#[test]
fn source_is_format_neutral_without_inventing_a_second_format_adapter() {
    let source = OriginalSource::new(
        "opaque-example",
        "application/octet-stream",
        "sample.bin",
        [0_u8, 255, 12, 13].as_slice(),
    )
    .unwrap();
    let original = WriterFile::with_original_source(document(), source).unwrap();
    assert_eq!(
        deserialize_writer_file(&serialize_writer_file(&original).unwrap()).unwrap(),
        original
    );
}

#[test]
fn edits_and_undo_history_do_not_mutate_the_retained_original() {
    let (document, original) = file().into_parts();
    let original = original.unwrap();
    let mut session = DocumentSession::new(document.clone());
    let mut transaction = session.begin_transaction();
    let at = transaction
        .position(&id("paragraph"), GraphemeOffset::new(0))
        .unwrap();
    transaction
        .insert_text(&at, "Revised — ", CharacterFormat::default())
        .unwrap();
    session.commit(transaction).unwrap();
    let edited = session.document().clone();
    let bytes = serialize_writer_file(
        &WriterFile::with_original_source(edited.clone(), original.clone()).unwrap(),
    )
    .unwrap();
    let loaded = deserialize_writer_file(&bytes).unwrap();
    assert_eq!(loaded.document(), &edited);
    assert_eq!(loaded.original_source().unwrap().bytes(), ORIGINAL);
    session.undo().unwrap();
    assert_eq!(session.document(), &document);
    assert_eq!(original.bytes(), ORIGINAL);
    session.redo().unwrap();
    assert_eq!(session.document(), &edited);
    assert_eq!(original.bytes(), ORIGINAL);
}

#[test]
fn source_metadata_has_explicit_validation_without_silent_normalization() {
    for format in ["", "DOCX", " docx", "xml/docx", "日本語", "-invalid"] {
        assert!(matches!(
            OriginalSource::new(format, MIME, "file.docx", b"bytes".as_slice()),
            Err(PersistenceError::InvalidSourceMetadata {
                field: "format",
                ..
            })
        ));
    }
    for media_type in [
        "",
        "application",
        "/docx",
        "application/",
        "application/xml;charset=utf-8",
        "application/日本語",
        "application/!name",
        "a/b/c",
    ] {
        assert!(matches!(
            OriginalSource::new("docx", media_type, "file.docx", b"bytes".as_slice()),
            Err(PersistenceError::InvalidSourceMetadata {
                field: "media_type",
                ..
            })
        ));
    }
    for filename in [
        "",
        "  ",
        ".",
        "..",
        "../file.docx",
        "folder/file.docx",
        "C:\\file.docx",
        "nul\0.docx",
        "tab\t.docx",
    ] {
        assert!(matches!(
            OriginalSource::new("docx", MIME, filename, b"bytes".as_slice()),
            Err(PersistenceError::InvalidSourceMetadata {
                field: "filename",
                ..
            })
        ));
    }
    assert!(matches!(
        OriginalSource::new("docx", MIME, "file.docx", [].as_slice()),
        Err(PersistenceError::InvalidSourceMetadata { field: "bytes", .. })
    ));
}

#[test]
fn metadata_limits_are_checked_in_bytes() {
    for (format, mime, filename, resource, limit) in [
        (
            "x".repeat(MAX_SOURCE_FORMAT_BYTES + 1),
            MIME.into(),
            "f.docx".into(),
            "source_format_bytes",
            MAX_SOURCE_FORMAT_BYTES,
        ),
        (
            "docx".into(),
            format!("application/{}", "x".repeat(MAX_SOURCE_MEDIA_TYPE_BYTES)),
            "f.docx".into(),
            "source_media_type_bytes",
            MAX_SOURCE_MEDIA_TYPE_BYTES,
        ),
        (
            "docx".into(),
            MIME.into(),
            "界".repeat(MAX_SOURCE_FILENAME_BYTES / 3 + 1),
            "source_filename_bytes",
            MAX_SOURCE_FILENAME_BYTES,
        ),
    ] {
        assert!(
            matches!(OriginalSource::new(format, mime, filename, b"bytes".as_slice()), Err(PersistenceError::LimitExceeded { resource: actual_resource, limit: actual_limit, actual }) if actual_resource == resource && actual_limit == limit && actual > limit)
        );
    }
}

#[test]
fn unknown_duplicate_and_missing_source_fields_are_rejected() {
    for field in ["format", "media_type", "filename", "bytes_base64"] {
        let mut value = payload();
        value["source"].as_object_mut().unwrap().remove(field);
        assert!(matches!(decode(&value), Err(PersistenceError::Json(_))));
        let source_raw = serde_json::to_string(&payload()["source"]).unwrap();
        let raw = serde_json::to_string(&payload()).unwrap();
        let needle = format!("\"{field}\":");
        let value_raw = serde_json::to_string(&payload()["source"][field]).unwrap();
        let duplicate_source =
            source_raw.replacen(&needle, &format!("{needle}{value_raw},{needle}"), 1);
        let duplicate = raw.replacen(&source_raw, &duplicate_source, 1);
        assert!(
            matches!(
                deserialize_writer_file(duplicate.as_bytes()),
                Err(PersistenceError::Json(_))
            ),
            "{field}"
        );
    }
    let mut unknown = payload();
    unknown["source"]["unsupported"] = json!(true);
    assert!(matches!(decode(&unknown), Err(PersistenceError::Json(_))));
    let raw = String::from_utf8(serde_json::to_vec(&payload()).unwrap()).unwrap();
    let duplicate = raw.replacen("\"source\":", "\"source\":null,\"source\":", 1);
    assert!(matches!(
        deserialize_writer_file(duplicate.as_bytes()),
        Err(PersistenceError::Json(_))
    ));
}

#[test]
fn base64_requires_standard_alphabet_padding_and_zero_unused_bits() {
    for encoded in [
        "Zg", "Zg=", "Zg===", "Zh==", "Zm9=", "Zm 8=", "Zm8=\n", "_w==", "?g==", "====", "A===",
        "AA=A",
    ] {
        let mut value = payload();
        value["source"]["bytes_base64"] = json!(encoded);
        assert!(
            matches!(
                decode(&value),
                Err(PersistenceError::InvalidSourceEncoding { .. })
            ),
            "{encoded:?}"
        );
    }
    for (encoded, expected) in [
        ("Zg==", b"f".as_slice()),
        ("Zm8=", b"fo".as_slice()),
        ("Zm9v", b"foo".as_slice()),
    ] {
        let mut value = payload();
        value["source"]["bytes_base64"] = json!(encoded);
        assert_eq!(
            decode(&value).unwrap().original_source().unwrap().bytes(),
            expected
        );
    }
}

#[test]
fn source_record_syntax_does_not_override_model_validation() {
    let mut value = payload();
    value["document"]["sections"][0]["blocks"][0]["content"]["runs"][0]["format"]["font_size"] =
        json!(0);
    assert!(matches!(
        decode(&value),
        Err(PersistenceError::Document(DocumentError::Validation(_)))
    ));
    let mut value = payload();
    value["document"]["schema_version"] = json!(u64::MAX);
    assert!(matches!(
        decode(&value),
        Err(PersistenceError::Document(
            DocumentError::UnsupportedSchemaVersion {
                found: u64::MAX,
                ..
            }
        ))
    ));
    let mut value = payload();
    value["format_version"] = json!(u64::MAX);
    value["document"] = json!({"future":true});
    assert!(matches!(
        decode(&value),
        Err(PersistenceError::UnsupportedFormatVersion {
            found: u64::MAX,
            ..
        })
    ));
}

#[test]
fn maximum_source_bytes_round_trip_and_larger_sources_are_rejected() {
    let bytes: Arc<[u8]> = vec![73; MAX_ORIGINAL_SOURCE_BYTES].into();
    let source =
        OriginalSource::new("example", "application/octet-stream", "source.bin", bytes).unwrap();
    let file = WriterFile::with_original_source(document(), source).unwrap();
    let encoded = serialize_writer_file(&file).unwrap();
    let decoded = deserialize_writer_file(&encoded).unwrap();
    assert_eq!(decoded, file);
    let oversized = vec![0; MAX_ORIGINAL_SOURCE_BYTES + 1];
    assert!(matches!(
        OriginalSource::new(
            "example",
            "application/octet-stream",
            "source.bin",
            oversized
        ),
        Err(PersistenceError::LimitExceeded {
            resource: "original_source_bytes",
            ..
        })
    ));
}

#[test]
fn base64_expansion_budget_is_rejected_before_decode_allocation() {
    let max_encoded = MAX_ORIGINAL_SOURCE_BYTES.div_ceil(3) * 4;
    let mut value = payload();
    value["source"]["bytes_base64"] = json!("A".repeat(max_encoded));
    assert!(
        matches!(decode(&value), Err(PersistenceError::LimitExceeded { resource: "original_source_bytes", actual, .. }) if actual > MAX_ORIGINAL_SOURCE_BYTES)
    );
    value["source"]["bytes_base64"] = json!("A".repeat(max_encoded + 4));
    assert!(matches!(
        decode(&value),
        Err(PersistenceError::LimitExceeded {
            resource: "source_base64_bytes",
            ..
        })
    ));
}

#[test]
fn whole_input_and_escaped_output_have_enforced_file_budgets() {
    let oversized = vec![b' '; MAX_WRITER_FILE_BYTES + 1];
    assert!(
        matches!(deserialize_writer_file(&oversized), Err(PersistenceError::LimitExceeded { resource: "writer_file_bytes", actual, .. }) if actual == oversized.len())
    );
    let document = Document::from_parts(
        id("large"),
        DocumentMetadata::default(),
        vec![Section::new(
            id("section"),
            vec![Block::Paragraph(Paragraph::new(
                id("paragraph"),
                vec![TextRun::new("\0".repeat(MAX_WRITER_FILE_BYTES / 6 + 1))],
            ))],
        )],
        vec![],
    )
    .unwrap();
    assert!(
        matches!(serialize_document(&document), Err(PersistenceError::LimitExceeded { resource: "writer_file_bytes", actual, .. }) if actual > MAX_WRITER_FILE_BYTES)
    );
}
