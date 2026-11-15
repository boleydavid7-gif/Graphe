//! Relationship-role regressions, including unmodified producer packages.

use std::io::{Cursor, Read};

use office_core::{
    Block, Document, Id, OriginalSource, WriterFile, deserialize_writer_file, serialize_writer_file,
};
use office_docx::{
    CompatibilityIssueKind as Issue, CompatibilityReport, DOCX_MEDIA_TYPE, DocxConformance,
    DocxImportOutcome, ImportedDocx, export_docx, import_docx, inspect_docx,
};
use zip::ZipArchive;

const AUXILIARY: &[u8] =
    include_bytes!("../../../tests/fixtures/docx/relationships/supported-auxiliary-parts.docx");
const AUTHORING: &[u8] =
    include_bytes!("../../../tests/fixtures/docx/relationships/supported-authoring-metadata.docx");

fn editable(bytes: &[u8]) -> ImportedDocx {
    match import_docx(bytes, Id::new("relationship-test").unwrap()).expect("valid package") {
        DocxImportOutcome::Editable(imported) => imported,
        other => panic!("expected complete editable import, got {other:?}"),
    }
}

fn text(document: &Document) -> String {
    document
        .sections()
        .iter()
        .flat_map(|section| section.blocks())
        .map(|block| {
            let Block::Paragraph(paragraph) = block;
            paragraph
                .runs()
                .iter()
                .map(|run| run.text.as_str())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn preserved(bytes: &[u8]) -> CompatibilityReport {
    match import_docx(bytes, Id::new("relationship-test").unwrap()).expect("valid package") {
        DocxImportOutcome::Preserved { original, report } => {
            assert_eq!(original.original_bytes(), bytes);
            assert!(!report.is_supported());
            report
        }
        other => panic!("unsupported semantics became editable: {other:?}"),
    }
}

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/docx/relationships")
            .join(name),
    )
    .expect("checked-in regression fixture")
}

fn xml_part(bytes: &[u8], name: &str) -> String {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut xml = String::new();
    archive
        .by_name(name)
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    xml
}

#[test]
fn imports_inert_ancillary_parts_without_discarding_original_or_supported_body() {
    let imported = editable(AUXILIARY);
    imported.document().validate().unwrap();
    assert_eq!(text(imported.document()), "DOCX hello — café 👩🏽‍💻");
    assert_eq!(
        imported.document().metadata().title,
        "Relationship fixture e\u{301} 👩🏽‍💻"
    );
    assert!(imported.document().styles().iter().any(|style| {
        style.name == "Fixture Normal"
            && style.character_format.font_family.as_deref() == Some("Arimo")
            && style
                .character_format
                .font_size
                .is_some_and(|size| size.0 == 24)
    }));
    let Block::Paragraph(paragraph) = &imported.document().sections()[0].blocks()[0];
    assert!(paragraph.style().is_some());
    assert_eq!(imported.original().original_bytes(), AUXILIARY);

    let file = WriterFile::with_original_source(
        imported.document().clone(),
        OriginalSource::new(
            "docx",
            DOCX_MEDIA_TYPE,
            "auxiliary.docx",
            AUXILIARY.to_vec(),
        )
        .unwrap(),
    )
    .unwrap();
    let reopened = deserialize_writer_file(&serialize_writer_file(&file).unwrap()).unwrap();
    assert_eq!(reopened.document(), imported.document());
    assert_eq!(reopened.original_source().unwrap().bytes(), AUXILIARY);
}

#[test]
fn current_content_export_and_exact_original_remain_distinct_operations() {
    let imported = editable(AUXILIARY);
    let output = export_docx(imported.document()).unwrap();
    let reconstructed = editable(&output);
    assert_eq!(text(reconstructed.document()), text(imported.document()));
    assert_eq!(
        reconstructed.document().metadata(),
        imported.document().metadata()
    );
    assert_eq!(
        reconstructed.document().styles()[0].character_format,
        imported.document().styles()[0].character_format
    );
    let mut archive = ZipArchive::new(Cursor::new(output)).unwrap();
    assert!(archive.by_name("word/theme/theme1.xml").is_err());
    assert!(archive.by_name("docProps/custom.xml").is_err());
    let mut body = String::new();
    archive
        .by_name("word/document.xml")
        .unwrap()
        .read_to_string(&mut body)
        .unwrap();
    assert!(body.contains("DOCX hello — café 👩🏽‍💻"));
    assert_eq!(imported.original().original_bytes(), AUXILIARY);
}

#[test]
fn harmless_session_metadata_and_resolved_ignorable_declarations_do_not_block_editing() {
    let strict = fixture("supported-strict-authoring-metadata.docx");
    assert_eq!(
        inspect_docx(&strict).unwrap().conformance,
        DocxConformance::Strict
    );
    for bytes in [AUTHORING, strict.as_slice()] {
        let imported = editable(bytes);
        assert_eq!(
            text(imported.document()),
            text(editable(AUXILIARY).document())
        );
        assert_eq!(imported.original().original_bytes(), bytes);
    }
}

#[test]
fn a_cached_package_thumbnail_does_not_enable_body_images_to_the_same_target() {
    let bytes = fixture("supported-root-thumbnail.docx");
    let imported = editable(&bytes);
    assert_eq!(
        text(imported.document()),
        text(editable(AUXILIARY).document())
    );
    assert_eq!(imported.original().original_bytes(), bytes);
    let report = preserved(&fixture("unsupported-main-image-same-thumbnail.docx"));
    assert!(report.diagnostics.iter().any(|issue| {
        issue.kind == Issue::UnsupportedRelationship
            && issue.part == "word/_rels/document.xml.rels"
            && issue.message.contains("/image")
            && issue.message.contains("../docProps/thumbnail.png")
    }));
}

#[test]
fn ignorable_declarations_never_hide_unknown_extensions_or_revision_content() {
    for (name, path) in [
        ("unsupported-unbound-ignorable.docx", "Ignorable"),
        (
            "unsupported-ignorable-extension.docx",
            "unrecognizedLayoutProperty",
        ),
        ("unsupported-revision.docx", "ins"),
        ("unsupported-malformed-session-id.docx", "rsidR"),
        (
            "unsupported-malformed-default-session-id.docx",
            "rsidRDefault",
        ),
    ] {
        let bytes = fixture(name);
        let report = preserved(&bytes);
        assert!(
            report
                .diagnostics
                .iter()
                .any(|issue| { issue.part == "word/document.xml" && issue.path.contains(path) }),
            "{name}: {report:?}"
        );
    }
}

#[test]
fn strict_ancillary_relationships_use_exact_roles_and_strict_xml_namespaces() {
    let bytes = fixture("supported-strict-auxiliary-parts.docx");
    let inspection = inspect_docx(&bytes).unwrap();
    assert_eq!(inspection.conformance, DocxConformance::Strict);
    let root_relationships = xml_part(&bytes, "_rels/.rels");
    for role in ["extendedProperties", "customProperties"] {
        assert!(root_relationships.contains(&format!(
            "http://purl.oclc.org/ooxml/officeDocument/relationships/{role}"
        )));
    }
    for (part, namespace) in [
        (
            "word/document.xml",
            "http://purl.oclc.org/ooxml/wordprocessingml/main",
        ),
        (
            "word/settings.xml",
            "http://purl.oclc.org/ooxml/wordprocessingml/main",
        ),
        (
            "word/fontTable.xml",
            "http://purl.oclc.org/ooxml/wordprocessingml/main",
        ),
        (
            "word/webSettings.xml",
            "http://purl.oclc.org/ooxml/wordprocessingml/main",
        ),
        (
            "word/styles.xml",
            "http://purl.oclc.org/ooxml/wordprocessingml/main",
        ),
        (
            "word/theme/theme1.xml",
            "http://purl.oclc.org/ooxml/drawingml/main",
        ),
        (
            "docProps/app.xml",
            "http://purl.oclc.org/ooxml/officeDocument/extendedProperties",
        ),
        (
            "docProps/custom.xml",
            "http://purl.oclc.org/ooxml/officeDocument/customProperties",
        ),
    ] {
        let xml = xml_part(&bytes, part);
        let document = roxmltree::Document::parse(&xml).unwrap();
        assert_eq!(
            document.root_element().tag_name().namespace(),
            Some(namespace),
            "{part}"
        );
        if part == "docProps/custom.xml" {
            assert!(document.descendants().any(|node| node.has_tag_name((
                "http://purl.oclc.org/ooxml/officeDocument/docPropsVTypes",
                "lpwstr"
            ))));
        }
    }
    let imported = editable(&bytes);
    imported.document().validate().unwrap();
    assert_eq!(imported.document(), editable(AUXILIARY).document());
    assert_eq!(imported.original().original_bytes(), bytes);
}

#[test]
fn strict_theme_relationships_do_not_allow_active_body_or_style_theme_references() {
    for (name, expected_part, expected_path) in [
        (
            "unsupported-strict-theme-font-reference.docx",
            "word/document.xml",
            "rFonts",
        ),
        (
            "unsupported-strict-theme-color-reference.docx",
            "word/document.xml",
            "color",
        ),
        (
            "unsupported-strict-theme-style-reference.docx",
            "word/styles.xml",
            "rFonts",
        ),
    ] {
        let bytes = fixture(name);
        assert_eq!(
            inspect_docx(&bytes).unwrap().conformance,
            DocxConformance::Strict
        );
        let report = preserved(&bytes);
        assert!(
            report
                .diagnostics
                .iter()
                .any(|issue| { issue.part == expected_part && issue.path.contains(expected_path) }),
            "{name}: {report:?}"
        );
        assert!(
            report.diagnostics.iter().all(|issue| {
                !matches!(
                    issue.kind,
                    Issue::UnsupportedRelationship | Issue::UnsupportedPart
                )
            }),
            "The theme property, not a known Strict auxiliary edge, must block editing: {name}: {report:?}"
        );
    }
}

#[test]
fn each_unsupported_content_relationship_remains_read_only_independently() {
    for (name, relationship, target) in [
        ("unsupported-image.docx", "/image", "image.png"),
        ("unsupported-header.docx", "/header", "header.xml"),
        ("unsupported-footer.docx", "/footer", "footer.xml"),
        ("unsupported-altchunk.docx", "/aFChunk", "altchunk.html"),
        (
            "unsupported-hyperlink.docx",
            "/hyperlink",
            "https://example.invalid/hyperlink",
        ),
        (
            "unsupported-external-relationship.docx",
            "/extended-properties",
            "https://example.invalid/app.xml",
        ),
        (
            "unknown-theme-relationship.docx",
            "https://example.invalid/relationships/theme",
            "theme/theme1.xml",
        ),
        (
            "unsupported-auxiliary-external-chain.docx",
            "/image",
            "https://example.invalid/hidden.png",
        ),
    ] {
        let bytes = fixture(name);
        let report = preserved(&bytes);
        assert!(
            report.diagnostics.iter().any(|issue| {
                issue.kind == Issue::UnsupportedRelationship
                    && issue.message.contains(relationship)
                    && issue.message.contains(target)
            }),
            "{name}: {report:?}"
        );
    }
}

#[test]
fn relationship_role_is_not_inferred_from_the_type_suffix_or_target_filename() {
    for (name, relationship) in [
        ("misplaced-root-font-table.docx", "/fontTable"),
        ("misplaced-main-app-properties.docx", "/extended-properties"),
    ] {
        let report = preserved(&fixture(name));
        assert!(
            report.diagnostics.iter().any(|issue| {
                issue.kind == Issue::UnsupportedRelationship && issue.message.contains(relationship)
            }),
            "{name}: {report:?}"
        );
    }
}

#[test]
fn correct_relationship_types_cannot_hide_wrong_mime_roots_or_singleton_ambiguity() {
    for name in [
        "mismatched-auxiliary-mime.docx",
        "mismatched-auxiliary-root.docx",
        "duplicate-settings-relationship.docx",
    ] {
        let bytes = fixture(name);
        assert!(
            import_docx(&bytes, Id::new("invalid-relationship").unwrap()).is_err(),
            "{name} must not publish a partial editable document"
        );
    }
}

#[test]
fn recognized_ancillary_edges_do_not_hide_active_settings_embedded_fonts_or_theme_properties() {
    for (name, expected_part, expected_path) in [
        (
            "unsupported-settings-compat.docx",
            "word/settings.xml",
            "compat",
        ),
        (
            "unsupported-settings-default-tabs.docx",
            "word/settings.xml",
            "defaultTabStop",
        ),
        (
            "unsupported-font-embedding.docx",
            "word/fontTable.xml",
            "embedRegular",
        ),
        (
            "unsupported-theme-font-reference.docx",
            "word/document.xml",
            "rFonts",
        ),
        (
            "unsupported-theme-color-reference.docx",
            "word/document.xml",
            "color",
        ),
        (
            "unsupported-theme-style-reference.docx",
            "word/styles.xml",
            "rFonts",
        ),
    ] {
        let report = preserved(&fixture(name));
        assert!(
            report
                .diagnostics
                .iter()
                .any(|issue| { issue.part == expected_part && issue.path.contains(expected_path) }),
            "{name}: {report:?}"
        );
        assert!(
            !report.diagnostics.iter().any(|issue| {
                issue.kind == Issue::UnsupportedRelationship
                    && issue.part == "word/_rels/document.xml.rels"
            }),
            "The known ancillary relationship itself is not the unsupported feature: {name}: {report:?}"
        );
    }
}

#[test]
fn unmodified_microsoft_and_libreoffice_fixtures_report_real_semantic_limits_not_boilerplate_edges()
{
    for name in [
        "microsoft-sdk-hello-world.docx",
        "microsoft-sdk-plain.docx",
        "libreoffice-basic.docx",
    ] {
        let report = preserved(&fixture(name));
        assert!(
            report.diagnostics.iter().all(|issue| {
                !matches!(
                    issue.kind,
                    Issue::UnsupportedRelationship | Issue::UnsupportedPart
                )
            }),
            "{name}: {report:?}"
        );
        assert!(
            report.diagnostics.iter().any(|issue| {
                issue.part == "word/styles.xml" || issue.part == "word/settings.xml"
            }),
            "The original producer still uses unrepresented styles/settings: {name}: {report:?}"
        );
    }
}
