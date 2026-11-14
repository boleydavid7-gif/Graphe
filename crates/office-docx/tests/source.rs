//! End-to-end preservation tests with deterministic handcrafted OPC fixtures.

use office_docx::{DocxConformance, DocxError, DocxSource, inspect_docx};

const SOURCE: &[u8] =
    include_bytes!("../../../tests/fixtures/docx/writer-source-preservation.docx");
const STRICT: &[u8] = include_bytes!("../../../tests/fixtures/docx/strict-relocated.docx");

#[test]
fn unsupported_semantics_and_package_details_are_retained_in_exact_original_bytes() {
    let source = DocxSource::open(SOURCE).expect("valid handcrafted package");
    let inventory = source.inspection();
    assert_eq!(inventory.main_document_part, "word/document.xml");
    assert_eq!(inventory.conformance, DocxConformance::Transitional);
    assert_eq!(inventory.parts.len(), 7);
    assert_eq!(inventory.relationship_count, 6);
    assert_eq!(inventory.external_relationship_count, 1);
    assert!(
        inventory
            .parts
            .iter()
            .any(|part| part.name == "customXml/item1.xml")
    );
    assert!(
        inventory
            .parts
            .iter()
            .any(|part| part.name == "word/media/image1.png" && part.content_type == "image/png")
    );
    assert_eq!(source.original_bytes(), SOURCE);
    assert_eq!(source.into_original_bytes(), SOURCE);
}

#[test]
fn source_owns_its_input_without_exposing_a_mutable_or_partial_projection() {
    let mut input = SOURCE.to_vec();
    let source = DocxSource::open(&input).expect("valid source");
    input.fill(0);
    assert_eq!(source.original_bytes(), SOURCE);
    assert_eq!(source.inspection(), &inspect_docx(SOURCE).unwrap());
    assert!(!format!("{source:?}").contains("Unsupported table content"));
}

#[test]
fn strict_namespace_and_relocated_main_are_discovered_from_package_metadata() {
    let source = DocxSource::open(STRICT).expect("valid Strict package");
    assert_eq!(source.inspection().conformance, DocxConformance::Strict);
    assert_eq!(
        source.inspection().main_document_part,
        "documents/text/main.xml"
    );
    assert_eq!(source.inspection().parts.len(), 2);
    assert_eq!(source.original_bytes(), STRICT);
}

#[test]
fn invalid_input_cannot_create_a_validated_source() {
    assert!(matches!(
        DocxSource::open(b"not a ZIP package"),
        Err(DocxError::InvalidArchive { .. })
    ));
}
