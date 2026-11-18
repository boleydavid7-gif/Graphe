//! Links are native semantic metadata, not part of editable text.

use office_core::{
    Block, Document, DocumentMetadata, Id, Paragraph, SCHEMA_VERSION, Section, TextRun,
    WRITER_FORMAT_VERSION, valid_hyperlink,
};
use serde_json::{Value, json};

fn document() -> Document {
    let mut run = TextRun::new("Website");
    run.format.hyperlink = Some("https://example.com".into());
    Document::from_parts(
        Id::new("link-doc").unwrap(),
        DocumentMetadata::default(),
        vec![Section::new(
            Id::new("s").unwrap(),
            vec![Block::Paragraph(Paragraph::new(
                Id::new("p").unwrap(),
                vec![run],
            ))],
        )],
        vec![],
    )
    .unwrap()
}

#[test]
fn links_need_bounded_web_addresses_only() {
    for value in [
        "https://example.com",
        "http://localhost:8080/path",
        "https://xn--bcher-kva.example/path?q=1#part",
    ] {
        assert!(valid_hyperlink(value), "{value}");
    }
    for value in [
        "",
        "ftp://example.com",
        "javascript:alert(1)",
        "https://user:pass@example.com",
        "https://example.com/a b",
        "https://example.com\\evil.com",
        "https://",
        "https://example.com\nx",
    ] {
        assert!(!valid_hyperlink(value), "{value}");
    }
}

#[test]
fn schema_four_links_round_trip_and_schema_three_remains_strict() {
    let current = document();
    assert_eq!(SCHEMA_VERSION, 6);
    assert_eq!(WRITER_FORMAT_VERSION, 6);
    let encoded = current.to_json().unwrap();
    let value: Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(
        value["sections"][0]["blocks"][0]["content"]["runs"][0]["format"]["hyperlink"],
        "https://example.com"
    );
    assert_eq!(Document::from_json(&encoded).unwrap(), current);

    let mut old = value;
    old["schema_version"] = json!(3);
    let old_json = serde_json::to_string(&old).unwrap();
    assert!(
        Document::from_json(&old_json).is_err(),
        "Schema 3 must reject new hyperlink fields even when syntax is valid"
    );
    old["sections"][0]["blocks"][0]["content"]["runs"][0]["format"]
        .as_object_mut()
        .unwrap()
        .remove("hyperlink");
    let migrated = Document::from_json(&old.to_string()).unwrap();
    let Block::Paragraph(p) = &migrated.sections()[0].blocks()[0] else {
        panic!("expected paragraph");
    };
    assert!(p.runs()[0].format.hyperlink.is_none());
    assert_eq!(migrated.schema_version(), SCHEMA_VERSION);
}
