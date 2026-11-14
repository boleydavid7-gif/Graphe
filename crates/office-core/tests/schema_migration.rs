//! Strict schema-1 migration and schema-2 decoding regression tests.

use office_core::{Block, Document, DocumentError, SCHEMA_VERSION, ValidationError};
use serde_json::{Value, json};

const LEGACY: &str = include_str!("../../../tests/fixtures/writer-sample-v1.json");

fn load(value: &Value) -> Result<Document, DocumentError> {
    Document::from_json(&value.to_string())
}

fn omit_unspecified_fields(value: &mut Value) {
    match value {
        Value::Object(object) => {
            object.retain(|_, value| !value.is_null());
            for value in object.values_mut() {
                omit_unspecified_fields(value);
            }
        }
        Value::Array(array) => {
            for value in array {
                omit_unspecified_fields(value);
            }
        }
        _ => {}
    }
}

#[test]
fn legacy_fixture_migrates_all_supported_values_and_stable_identities() {
    let mut before: Value = serde_json::from_str(LEGACY).unwrap();
    let document = Document::from_json(LEGACY).unwrap();
    assert_eq!(SCHEMA_VERSION, 2);
    assert_eq!(document.schema_version(), 2);
    let mut after = serde_json::to_value(&document).unwrap();
    after["schema_version"] = json!(1);
    for section in after["sections"].as_array_mut().unwrap() {
        if let Some(properties) = section["properties"].as_object_mut() {
            properties.remove("page_size");
            properties.remove("page_margins");
        }
        for block in section["blocks"].as_array_mut().unwrap() {
            assert!(
                block["content"]["format"]
                    .as_object_mut()
                    .unwrap()
                    .remove("page_break_before")
                    .unwrap()
                    .is_null()
            );
        }
    }
    for style in after["styles"].as_array_mut().unwrap() {
        assert!(
            style["paragraph_format"]
                .as_object_mut()
                .unwrap()
                .remove("page_break_before")
                .unwrap()
                .is_null()
        );
    }
    // Missing optional properties and explicit null both represented None in
    // schema 1. Compare their supported values rather than JSON field spelling.
    omit_unspecified_fields(&mut before);
    omit_unspecified_fields(&mut after);
    assert_eq!(after, before);
    assert_eq!(
        Document::from_json(&document.to_json().unwrap()).unwrap(),
        document
    );
}

#[test]
fn direct_serde_and_value_deserialization_use_the_same_migration_and_validation() {
    let expected = Document::from_json(LEGACY).unwrap();
    assert_eq!(serde_json::from_str::<Document>(LEGACY).unwrap(), expected);
    assert_eq!(
        serde_json::from_value::<Document>(serde_json::from_str::<Value>(LEGACY).unwrap()).unwrap(),
        expected
    );
    let current = expected.to_json().unwrap();
    assert_eq!(
        serde_json::from_str::<Document>(&current).unwrap(),
        expected
    );
}

#[test]
fn new_properties_remain_illegal_inside_legacy_schema_even_when_null() {
    for (pointer, field, value) in [
        ("/sections/0/properties", "page_size", Value::Null),
        ("/sections/0/properties", "page_margins", Value::Null),
        (
            "/sections/0/blocks/0/content/format",
            "page_break_before",
            Value::Null,
        ),
        (
            "/styles/0/paragraph_format",
            "page_break_before",
            json!(false),
        ),
    ] {
        let mut source: Value = serde_json::from_str(LEGACY).unwrap();
        source
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert(field.into(), value);
        assert!(
            matches!(load(&source), Err(DocumentError::Json(_))),
            "legacy new field {pointer}/{field}"
        );
        assert!(serde_json::from_str::<Document>(&source.to_string()).is_err());
    }
}

#[test]
fn duplicate_fields_still_fail_in_legacy_and_current_decoders() {
    for text in [
        r#"{"schema_version":1,"schema_version":2}"#,
        r#"{"schema_version":1,"id":"document","metadata":{"title":"one","title":"two"},"sections":[],"styles":[]}"#,
        r#"{"schema_version":2,"id":"document","metadata":{"title":"one","title":"two"},"sections":[],"styles":[]}"#,
    ] {
        assert!(matches!(
            Document::from_json(text),
            Err(DocumentError::Json(_))
        ));
        assert!(serde_json::from_str::<Document>(text).is_err());
    }
    let duplicate = LEGACY.replace(
        "\"spacing_before\": 0",
        "\"spacing_before\": 0, \"spacing_before\": 1",
    );
    assert_ne!(duplicate, LEGACY, "fixture contains replacement token");
    assert!(Document::from_json(&duplicate).is_err());
    assert!(serde_json::from_str::<Document>(&duplicate).is_err());
}

#[test]
fn invalid_legacy_references_and_formatting_are_not_repaired_by_migration() {
    let mut source: Value = serde_json::from_str(LEGACY).unwrap();
    source["sections"][0]["blocks"][0]["content"]["style"] = json!("missing");
    assert!(matches!(
        load(&source),
        Err(DocumentError::Validation(
            ValidationError::MissingStyle { .. }
        ))
    ));
    let mut source: Value = serde_json::from_str(LEGACY).unwrap();
    source["sections"][0]["blocks"][0]["content"]["format"]["spacing_before"] = json!(-1);
    assert!(matches!(
        load(&source),
        Err(DocumentError::Validation(
            ValidationError::InvalidFormatting { .. }
        ))
    ));
    let mut source: Value = serde_json::from_str(LEGACY).unwrap();
    source["sections"][0]["blocks"][0]["content"]["id"] = source["id"].clone();
    assert!(matches!(
        load(&source),
        Err(DocumentError::Validation(
            ValidationError::DuplicateId { .. }
        ))
    ));
}

#[test]
fn old_and_future_unsupported_versions_are_rejected_before_payload_interpretation() {
    for version in [0, 3, u64::MAX] {
        let source = json!({"schema_version":version,"incompatible_future_payload":true});
        assert!(
            matches!(load(&source), Err(DocumentError::UnsupportedSchemaVersion {found,supported:2}) if found==version)
        );
        let error = serde_json::from_str::<Document>(&source.to_string()).unwrap_err();
        assert!(error.to_string().contains("unsupported document schema"));
    }
}

#[test]
fn legacy_absent_and_present_empty_section_properties_remain_distinct() {
    let document = Document::from_json(LEGACY).unwrap();
    assert!(document.sections()[0].properties().is_some());
    assert!(document.sections()[1].properties().is_none());
    for section in document.sections() {
        if let Some(properties) = section.properties() {
            assert!(properties.page_size.is_none());
            assert!(properties.page_margins.is_none());
        }
        for block in section.blocks() {
            let Block::Paragraph(paragraph) = block;
            assert!(paragraph.format().page_break_before.is_none());
        }
    }
}
