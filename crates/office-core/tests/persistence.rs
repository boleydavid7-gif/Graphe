//! Native bytes-format compatibility, preservation, and strict loading contracts.

use std::error::Error;

use office_core::{
    Alignment, Block, CharacterFormat, CharacterFormatPatch, Document, DocumentError,
    DocumentMetadata, DocumentSession, GraphemeOffset, HalfPoints, Id, Indentation, LineSpacing,
    NamedStyle, Paragraph, ParagraphFormat, PersistenceError, PropertyChange, SCHEMA_VERSION,
    Section, SectionProperties, TextColor, TextRun, Twips, Underline, ValidationError,
    WRITER_EXTENSION, WRITER_FORMAT_NAME, WRITER_FORMAT_VERSION, deserialize_document,
    serialize_document,
};
use serde_json::{Value, json};

const NATIVE_FIXTURE: &[u8] = include_bytes!("../../../tests/fixtures/native/writer-v1.writer");
const MODEL_FIXTURE: &str = include_str!("../../../tests/fixtures/writer-sample-v1.json");

fn id(value: &str) -> Id {
    Id::new(value).unwrap()
}

fn fixture() -> Document {
    deserialize_document(NATIVE_FIXTURE).unwrap()
}

fn simple_document() -> Document {
    Document::from_parts(
        id("document"),
        DocumentMetadata {
            title: "Title".into(),
        },
        vec![Section::new(
            id("section"),
            vec![Block::Paragraph(Paragraph::new(
                id("paragraph"),
                vec![TextRun::new("A")],
            ))],
        )],
        vec![],
    )
    .unwrap()
}

fn payload() -> Value {
    serde_json::from_slice(&serialize_document(&simple_document()).unwrap()).unwrap()
}

fn load(value: Value) -> Result<Document, PersistenceError> {
    deserialize_document(&serde_json::to_vec(&value).unwrap())
}

fn first_paragraph(document: &Document) -> &Paragraph {
    let Block::Paragraph(paragraph) = &document.sections()[0].blocks()[0] else {
        panic!("expected paragraph");
    };
    paragraph
}

#[test]
fn empty_document_round_trip_preserves_identity_and_does_not_synthesize_content() {
    let document = Document::new(id("empty-document")).unwrap();
    let bytes = serialize_document(&document).unwrap();
    let restored = deserialize_document(&bytes).unwrap();
    assert_eq!(restored, document);
    assert_eq!(restored.schema_version(), SCHEMA_VERSION);
    assert!(restored.sections().is_empty());
    assert!(restored.styles().is_empty());
    assert_eq!(restored.metadata().title, "");
}

#[test]
fn native_envelope_version_is_explicit_and_separate_from_model_and_engine_versions() {
    let document = simple_document();
    let bytes = serialize_document(&document).unwrap();
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(WRITER_EXTENSION, "writer");
    assert_eq!(value["format"], WRITER_FORMAT_NAME);
    assert_eq!(value["format_version"], WRITER_FORMAT_VERSION);
    assert_eq!(value["document"]["schema_version"], SCHEMA_VERSION);
    assert_eq!(value.as_object().unwrap().len(), 3);
    for field in [
        "application_version",
        "engine_version",
        "timestamp",
        "revision",
        "history",
    ] {
        assert!(value.get(field).is_none());
    }
}

#[test]
fn serialization_is_deterministic_pretty_utf8_with_one_final_newline() {
    let document = fixture();
    let first = serialize_document(&document).unwrap();
    let second = serialize_document(&document).unwrap();
    assert_eq!(first, second);
    assert!(first.starts_with(
        b"{\n  \"format\": \"office-writer\",\n  \"format_version\": 7,\n  \"document\": {"
    ));
    assert!(first.ends_with(b"}\n"));
    assert!(!first.ends_with(b"\n\n"));
    let text = std::str::from_utf8(&first).unwrap();
    assert!(text.contains("👩🏽‍💻"));
    assert!(text.contains("ملخص البحث"));
    let restored = deserialize_document(&first).unwrap();
    assert_eq!(serialize_document(&restored).unwrap(), first);
}

#[test]
fn json_whitespace_and_escaped_format_marker_keep_the_same_document() {
    let document = simple_document();
    let canonical = serialize_document(&document).unwrap();
    let mut spaced = b"\n \t".to_vec();
    spaced.extend_from_slice(&canonical);
    spaced.extend_from_slice(b" \r\n\t");
    assert_eq!(deserialize_document(&spaced).unwrap(), document);

    let escaped = String::from_utf8(canonical.clone()).unwrap().replacen(
        "\"office-writer\"",
        "\"\\u006fffice-writer\"",
        1,
    );
    let restored = deserialize_document(escaped.as_bytes()).unwrap();
    assert_eq!(restored, document);
    assert_eq!(serialize_document(&restored).unwrap(), canonical);
}

#[test]
fn realistic_native_fixture_matches_the_existing_model_without_information_loss() {
    let document = fixture();
    let model = Document::from_json(MODEL_FIXTURE).unwrap();
    assert_eq!(document, model);
    assert_eq!(document.metadata().title, "Quarterly research report");
    assert_eq!(document.sections().len(), 2);
    assert_eq!(document.sections()[0].blocks().len(), 2);
    assert_eq!(document.sections()[1].blocks().len(), 1);
    assert_eq!(
        document.styles()[1].based_on.as_ref().unwrap().as_str(),
        "normal"
    );
    assert_eq!(document.sections()[0].id().as_str(), "executive-summary");
    assert_eq!(
        document.sections()[1].id().as_str(),
        "international-findings"
    );
    let restored = deserialize_document(&serialize_document(&document).unwrap()).unwrap();
    assert_eq!(restored, model);
}

#[test]
fn all_supported_character_and_paragraph_values_round_trip_with_order_and_defaults() {
    let mut normal = NamedStyle::new(id("normal"), "Normal");
    normal.character_format.font_family = Some("Noto Serif".into());
    let mut heading = NamedStyle::new(id("heading"), "Heading");
    heading.based_on = Some(id("normal"));
    heading.paragraph_format.spacing_after = Some(Twips(160));
    let text = "e\u{301} 👩🏽‍💻 🇺🇸 العربية 中文\0\r\n";
    let mut paragraphs = Vec::new();
    for (index, (alignment, line_spacing, underline)) in [
        (
            Alignment::Left,
            LineSpacing::Multiple { value: 360 },
            Underline::None,
        ),
        (
            Alignment::Center,
            LineSpacing::Exact { height: Twips(240) },
            Underline::Single,
        ),
        (
            Alignment::Right,
            LineSpacing::AtLeast { height: Twips(180) },
            Underline::Double,
        ),
        (
            Alignment::Justify,
            LineSpacing::Multiple { value: 240 },
            Underline::Single,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let character = CharacterFormat {
            bold: Some(index % 2 == 0),
            italic: Some(index % 2 != 0),
            underline: Some(underline),
            font_family: Some("Noto Sans CJK 日本語".into()),
            font_size: Some(HalfPoints(23)),
            color: Some(if index == 0 {
                TextColor::Auto {}
            } else {
                TextColor::Rgb {
                    red: 0,
                    green: 128,
                    blue: 255,
                }
            }),
            hyperlink: None,
        };
        let format = ParagraphFormat {
            page_break_before: Some(index % 2 == 0),
            alignment: Some(alignment),
            spacing_before: Some(Twips(0)),
            spacing_after: Some(Twips(120)),
            line_spacing: Some(line_spacing),
            indentation: Some(Indentation {
                left: Some(Twips(720)),
                right: Some(Twips(-120)),
                first_line: Some(Twips(-360)),
            }),
            ..Default::default()
        };
        paragraphs.push(Block::Paragraph(
            Paragraph::new(
                id(&format!("paragraph-{index}")),
                vec![
                    TextRun {
                        text: text.into(),
                        format: character,
                    },
                    TextRun::new(""),
                    TextRun::new("plain"),
                ],
            )
            .with_style(id("heading"))
            .with_format(format),
        ));
    }
    let document = Document::from_parts(
        id("formatting-document"),
        DocumentMetadata {
            title: "研究 — Cafe\u{301} 👩🏽‍💻".into(),
        },
        vec![
            Section::new(id("first-section"), paragraphs)
                .with_properties(SectionProperties::default()),
            Section::new(id("empty-section"), vec![]),
        ],
        vec![heading, normal],
    )
    .unwrap();
    let before = document.clone();
    let restored = deserialize_document(&serialize_document(&document).unwrap()).unwrap();
    assert_eq!(restored, before);
    assert_eq!(document, before);
    assert!(restored.sections()[0].properties().is_some());
    assert!(restored.sections()[1].properties().is_none());
    assert_eq!(
        first_paragraph(&restored).runs()[0].text.as_bytes(),
        text.as_bytes()
    );
    assert_eq!(
        first_paragraph(&restored).runs()[1].format,
        CharacterFormat::default()
    );
    assert_eq!(restored.validate(), Ok(()));
}

#[test]
fn persistence_of_edited_documents_leaves_live_revisions_endpoints_and_history_intact() {
    let mut session = DocumentSession::new(fixture());
    let original = session.document().clone();
    let mut transaction = session.begin_transaction();
    let at = transaction
        .position(&id("intro"), GraphemeOffset::new(0))
        .unwrap();
    transaction
        .insert_text(&at, "Updated 👨‍👩‍👧‍👦 — ", CharacterFormat::default())
        .unwrap();
    let start = transaction
        .position(&id("intro"), GraphemeOffset::new(0))
        .unwrap();
    let end = transaction
        .position(&id("intro"), GraphemeOffset::new(1))
        .unwrap();
    let range = transaction.selection(start, end).unwrap();
    transaction
        .apply_character_format(
            &range,
            &CharacterFormatPatch {
                italic: PropertyChange::Set(true),
                ..Default::default()
            },
        )
        .unwrap();
    session.commit(transaction).unwrap();
    let edited = session.document().clone();
    let revision = session.revision();
    let mut pending = session.begin_transaction();
    let endpoint = pending
        .position(&id("intro"), GraphemeOffset::new(0))
        .unwrap();
    let bytes = serialize_document(session.document()).unwrap();
    let restored = deserialize_document(&bytes).unwrap();
    assert_eq!(restored, edited);
    assert_eq!(session.document(), &edited);
    assert_eq!(session.revision(), revision);
    assert!(session.can_undo());
    assert!(!session.can_redo());
    assert_eq!(
        pending.insert_text(&endpoint, "", CharacterFormat::default()),
        Ok(false)
    );
    assert!(!session.commit(pending).unwrap().changed());
    let reopened = DocumentSession::new(restored);
    assert!(!reopened.can_undo());
    assert!(!reopened.can_redo());
    assert_eq!(reopened.revision().number(), 0);
    session.undo().unwrap();
    assert_eq!(session.document(), &original);
    session.redo().unwrap();
    assert_eq!(session.document(), &edited);
}

#[test]
fn corrupt_truncated_non_object_or_trailing_json_is_rejected() {
    for bytes in [
        b"".as_slice(),
        b"{",
        b"[]",
        b"null",
        b"{}",
        b"{\"format\":",
        b"true",
    ] {
        assert!(matches!(
            deserialize_document(bytes),
            Err(PersistenceError::Json(_))
        ));
    }
    let mut bytes = serialize_document(&simple_document()).unwrap();
    bytes.extend_from_slice(b"garbage");
    assert!(matches!(
        deserialize_document(&bytes),
        Err(PersistenceError::Json(_))
    ));
}

#[test]
fn invalid_utf8_and_a_byte_order_mark_are_not_silently_repaired() {
    let error = deserialize_document(&[b'{', 0xff, b'}']).unwrap_err();
    assert!(matches!(error, PersistenceError::InvalidUtf8(_)));
    assert!(error.source().is_some());
    let mut bytes = vec![0xef, 0xbb, 0xbf];
    bytes.extend(serialize_document(&simple_document()).unwrap());
    assert!(matches!(
        deserialize_document(&bytes),
        Err(PersistenceError::Json(_))
    ));
}

#[test]
fn unknown_format_names_are_distinct_from_unsupported_versions() {
    let error =
        load(json!({"format":"some-other-file", "format_version":999, "future":true})).unwrap_err();
    assert!(
        matches!(error, PersistenceError::UnknownFormat { found } if found == "some-other-file")
    );
}

#[test]
fn unsupported_native_versions_are_checked_before_current_payload_shape() {
    for version in [0, 7, u64::MAX] {
        let error = load(
            json!({"format":"office-writer", "format_version":version, "future_payload":true}),
        )
        .unwrap_err();
        assert!(
            matches!(error, PersistenceError::UnsupportedFormatVersion { found, supported }
            if found == version && supported == WRITER_FORMAT_VERSION)
        );
    }
}

#[test]
fn unsupported_model_schema_versions_retain_the_existing_structured_error() {
    for version in [0, 7, u64::MAX] {
        let error = load(json!({"format":"office-writer", "format_version":1,
            "document":{"schema_version":version,"future_payload":true}}))
        .unwrap_err();
        assert!(
            matches!(error, PersistenceError::Document(DocumentError::UnsupportedSchemaVersion { found, supported })
            if found == version && supported == SCHEMA_VERSION)
        );
    }
}

#[test]
fn bare_model_json_is_not_implicitly_reinterpreted_as_a_native_file() {
    let model = simple_document().to_json().unwrap();
    assert!(matches!(
        deserialize_document(model.as_bytes()),
        Err(PersistenceError::Json(_))
    ));
}

#[test]
fn missing_required_envelope_and_model_fields_are_rejected() {
    for field in ["format", "format_version", "document"] {
        let mut value = payload();
        value.as_object_mut().unwrap().remove(field);
        assert!(
            matches!(load(value), Err(PersistenceError::Json(_))),
            "missing {field}"
        );
    }
    for field in ["id", "metadata", "sections", "styles"] {
        let mut value = payload();
        value["document"].as_object_mut().unwrap().remove(field);
        assert!(
            matches!(
                load(value),
                Err(PersistenceError::Document(DocumentError::Json(_)))
            ),
            "missing {field}"
        );
    }
}

#[test]
fn unknown_envelope_or_supported_model_fields_are_rejected_instead_of_discarded() {
    let mut value = payload();
    value["unknown"] = json!(true);
    assert!(matches!(load(value), Err(PersistenceError::Json(_))));
    for pointer in [
        "/document",
        "/document/metadata",
        "/document/sections/0",
        "/document/sections/0/blocks/0/content",
        "/document/sections/0/blocks/0/content/runs/0/format",
    ] {
        let mut value = payload();
        value
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unsupported".into(), json!(true));
        assert!(
            matches!(
                load(value),
                Err(PersistenceError::Document(DocumentError::Json(_)))
            ),
            "{pointer}"
        );
    }
    let mut value = payload();
    value["document"]["sections"][0]["blocks"][0]["kind"] = json!("image");
    assert!(matches!(
        load(value),
        Err(PersistenceError::Document(DocumentError::Json(_)))
    ));
    let mut value = payload();
    value["document"]["sections"][0]["blocks"][0]["content"]["runs"][0]["format"]["color"] =
        json!({"kind":"auto","theme":"accent1"});
    assert!(matches!(
        load(value),
        Err(PersistenceError::Document(DocumentError::Json(_)))
    ));
}

#[test]
fn duplicate_object_keys_are_rejected_without_value_based_overwriting() {
    let valid = String::from_utf8(serialize_document(&simple_document()).unwrap()).unwrap();
    for duplicate in [
        valid.replacen(
            "\"format\": \"office-writer\"",
            "\"format\": \"office-writer\", \"format\": \"office-writer\"",
            1,
        ),
        valid.replacen(
            "\"format_version\": 7",
            "\"format_version\": 7, \"format_version\": 7",
            1,
        ),
        valid.replacen("\"document\": {", "\"document\": {}, \"document\": {", 1),
    ] {
        assert!(matches!(
            deserialize_document(duplicate.as_bytes()),
            Err(PersistenceError::Json(_))
        ));
    }
    for duplicate in [
        valid.replacen(
            "\"schema_version\": 7",
            "\"schema_version\": 7, \"schema_version\": 7",
            1,
        ),
        valid.replacen(
            "\"title\": \"Title\"",
            "\"title\": \"Title\", \"title\": \"Other\"",
            1,
        ),
        valid.replacen("\"bold\": null", "\"bold\": null, \"bold\": false", 1),
    ] {
        assert!(matches!(
            deserialize_document(duplicate.as_bytes()),
            Err(PersistenceError::Document(DocumentError::Json(_)))
        ));
    }
}

#[test]
fn invalid_identifiers_and_duplicate_entities_cannot_bypass_model_validation() {
    for invalid in ["", " ", " padded"] {
        let mut value = payload();
        value["document"]["sections"][0]["blocks"][0]["content"]["id"] = json!(invalid);
        assert!(matches!(
            load(value),
            Err(PersistenceError::Document(DocumentError::Validation(
                ValidationError::InvalidId { .. }
            )))
        ));
    }
    let mut value = payload();
    value["document"]["sections"][0]["id"] = json!("document");
    assert!(
        matches!(load(value), Err(PersistenceError::Document(DocumentError::Validation(ValidationError::DuplicateId { id }))) if id == "document")
    );
}

#[test]
fn unresolved_styles_and_style_cycles_are_rejected_after_decoding() {
    let mut missing = payload();
    missing["document"]["sections"][0]["blocks"][0]["content"]["style"] = json!("missing");
    assert!(
        matches!(load(missing), Err(PersistenceError::Document(DocumentError::Validation(ValidationError::MissingStyle { id, .. }))) if id == "missing")
    );
    let mut cyclic = payload();
    let mut style = NamedStyle::new(id("style"), "Style");
    style.based_on = Some(id("style"));
    cyclic["document"]["styles"] = json!([style]);
    assert!(
        matches!(load(cyclic), Err(PersistenceError::Document(DocumentError::Validation(ValidationError::StyleCycle { id }))) if id == "style")
    );
}

#[test]
fn invalid_supported_formatting_returns_validation_errors_with_original_paths() {
    for (pointer, invalid, field) in [
        (
            "/document/sections/0/blocks/0/content/runs/0/format/font_size",
            json!(0),
            "font_size",
        ),
        (
            "/document/sections/0/blocks/0/content/runs/0/format/font_family",
            json!(" \t"),
            "font_family",
        ),
        (
            "/document/sections/0/blocks/0/content/format/spacing_before",
            json!(-1),
            "spacing_before",
        ),
        (
            "/document/sections/0/blocks/0/content/format/line_spacing",
            json!({"kind":"exact","height":0}),
            "line_spacing",
        ),
    ] {
        let mut value = payload();
        *value.pointer_mut(pointer).unwrap() = invalid;
        let error = load(value).unwrap_err();
        assert!(
            matches!(error, PersistenceError::Document(DocumentError::Validation(ValidationError::InvalidFormatting { path, .. })) if path.ends_with(field))
        );
    }
}

#[test]
fn numeric_or_enum_shapes_outside_supported_types_are_json_errors() {
    for invalid in [json!(-1), json!(1.5), json!(u64::MAX)] {
        let mut value = payload();
        value["document"]["sections"][0]["blocks"][0]["content"]["runs"][0]["format"]["font_size"] =
            invalid;
        assert!(matches!(
            load(value),
            Err(PersistenceError::Document(DocumentError::Json(_)))
        ));
    }
    for version in [json!(-1), json!(1.5), json!("1"), Value::Null] {
        let mut value = payload();
        value["format_version"] = version;
        assert!(matches!(load(value), Err(PersistenceError::Json(_))));
    }
}
