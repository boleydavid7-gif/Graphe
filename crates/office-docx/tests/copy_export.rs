//! Source-aware copy preservation never reinstates original body content.

use std::{
    collections::BTreeMap,
    io::{Cursor, Read, Write},
};

use office_core::{
    Alignment, Block, CharacterFormat, Document, DocumentMetadata, HalfPoints, Id, Paragraph,
    ParagraphFormat, Section, TextRun,
};
use office_docx::{
    DocxImportOutcome, DocxSource, ImportedDocx, assess_docx_copy, export_docx,
    export_docx_with_source, import_docx, inspect_docx,
};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

const SAFE: &[u8] =
    include_bytes!("../../../tests/fixtures/docx/relationships/supported-auxiliary-parts.docx");
const STRICT: &[u8] = include_bytes!(
    "../../../tests/fixtures/docx/relationships/supported-strict-auxiliary-parts.docx"
);
const WORD: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const STRICT_CUSTOM: &str = "http://purl.oclc.org/ooxml/officeDocument/customProperties";

fn id(value: impl Into<String>) -> Id {
    Id::new(value).unwrap()
}
fn editable(bytes: &[u8]) -> ImportedDocx {
    match import_docx(bytes, id("copy-test")).unwrap() {
        DocxImportOutcome::Editable(value) => value,
        DocxImportOutcome::Preserved { report, .. } => panic!("not editable: {report:?}"),
    }
}
fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/docx/relationships")
            .join(name),
    )
    .unwrap()
}
fn parts(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
    (0..archive.len())
        .map(|index| {
            let mut file = archive.by_index(index).unwrap();
            let name = file.name().unwrap().into_owned();
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes).unwrap();
            (name, bytes)
        })
        .collect()
}
fn package(parts: BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (name, bytes) in parts {
        zip.start_file(name, options).unwrap();
        zip.write_all(&bytes).unwrap();
    }
    zip.finish().unwrap().into_inner()
}
fn changed(bytes: &[u8], part: &str, change: impl FnOnce(String) -> String) -> Vec<u8> {
    let mut values = parts(bytes);
    let text = String::from_utf8(values.remove(part).unwrap()).unwrap();
    values.insert(part.into(), change(text).into_bytes());
    package(values)
}
fn xml(values: &BTreeMap<String, Vec<u8>>, part: &str) -> String {
    String::from_utf8(values[part].clone()).unwrap()
}
fn current(source: &Document) -> Document {
    let paragraph = Paragraph::new(
        id("edited-paragraph"),
        vec![TextRun {
            text: "Current Café e\u{301} 👩🏽‍💻 only".into(),
            format: CharacterFormat {
                bold: Some(true),
                font_family: Some("Tinos".into()),
                font_size: Some(HalfPoints(28)),
                ..CharacterFormat::default()
            },
        }],
    )
    .with_format(ParagraphFormat {
        alignment: Some(Alignment::Right),
        ..ParagraphFormat::default()
    });
    Document::from_parts(
        source.id().clone(),
        DocumentMetadata {
            title: "Current title & <safe>".into(),
        },
        vec![Section::new(
            id("edited-section"),
            vec![Block::Paragraph(paragraph)],
        )],
        source.styles().to_vec(),
    )
    .unwrap()
}

#[test]
fn copy_preserves_known_properties_auxiliaries_and_current_projection_without_mutation() {
    let imported = editable(SAFE);
    let current = current(imported.document());
    let snapshot = current.clone();
    let output = export_docx_with_source(&current, imported.original()).unwrap();
    assert_eq!(
        output,
        export_docx_with_source(&current, imported.original()).unwrap()
    );
    assert_eq!(current, snapshot);
    assert_eq!(imported.original().original_bytes(), SAFE);
    assert_eq!(
        editable(&output).document(),
        editable(&export_docx(&current).unwrap()).document()
    );
    let values = parts(&output);
    let core = xml(&values, "docProps/core.xml");
    assert!(core.contains("Fixture Author"));
    assert!(core.contains("Current title &amp; &lt;safe&gt;"));
    assert!(!core.contains("lastModifiedBy"));
    assert!(!core.contains(":modified"));
    for part in [
        "docProps/app.xml",
        "docProps/custom.xml",
        "word/theme/theme1.xml",
        "word/fontTable.xml",
        "word/settings.xml",
        "word/webSettings.xml",
    ] {
        assert!(values.contains_key(part), "missing {part}");
    }
    assert!(xml(&values, "docProps/custom.xml").contains("Relationship regression"));
    assert!(!xml(&values, "word/document.xml").contains("DOCX hello"));
    inspect_docx(&output).unwrap();
    let assessment = assess_docx_copy(&current, imported.original()).unwrap();
    assert!(!assessment.formatting_may_differ);
    assert!(!assessment.bookmarks_omitted);
    assert!(assessment.has_document_properties);
}

#[test]
fn strict_parts_convert_only_namespace_declarations_not_property_values() {
    let source = changed(STRICT, "docProps/custom.xml", |text| {
        text.replace("Relationship regression", STRICT_CUSTOM)
    });
    let imported = editable(&source);
    let output = export_docx_with_source(imported.document(), imported.original()).unwrap();
    let values = parts(&output);
    for part in [
        "docProps/app.xml",
        "docProps/custom.xml",
        "word/theme/theme1.xml",
        "word/fontTable.xml",
        "word/settings.xml",
        "word/webSettings.xml",
    ] {
        let text = xml(&values, part);
        let document = roxmltree::Document::parse(&text).unwrap();
        for node in document.descendants().filter(|node| node.is_element()) {
            assert!(
                !node
                    .tag_name()
                    .namespace()
                    .is_some_and(|ns| ns.starts_with("http://purl.oclc.org/ooxml/")),
                "{part}"
            );
        }
    }
    assert!(xml(&values, "docProps/custom.xml").contains(STRICT_CUSTOM));
    assert_eq!(imported.original().original_bytes(), source);
    assert_eq!(
        editable(&output).document(),
        editable(&export_docx(imported.document()).unwrap()).document()
    );
}

#[test]
fn copied_roles_do_not_assume_original_part_paths() {
    let mut values = parts(SAFE);
    let moved = values.remove("word/fontTable.xml").unwrap();
    values.insert("other/fonts.xml".into(), moved);
    let content =
        xml(&values, "[Content_Types].xml").replace("/word/fontTable.xml", "/other/fonts.xml");
    values.insert("[Content_Types].xml".into(), content.into_bytes());
    let relations = xml(&values, "word/_rels/document.xml.rels")
        .replace("Target=\"fontTable.xml\"", "Target=\"../other/fonts.xml\"");
    values.insert(
        "word/_rels/document.xml.rels".into(),
        relations.into_bytes(),
    );
    let source = package(values);
    let imported = editable(&source);
    let output = export_docx_with_source(imported.document(), imported.original()).unwrap();
    assert!(parts(&output).contains_key("word/fontTable.xml"));
    assert!(!parts(&output).contains_key("other/fonts.xml"));
    assert_eq!(imported.original().original_bytes(), source);
}

#[test]
fn stale_preview_counts_heading_caches_and_xml_comments_are_not_reintroduced() {
    let source = fixture("supported-root-thumbnail.docx");
    let source = changed(&source, "docProps/app.xml", |text| {
        text.replace("</Properties>",
        "<Company>Kept company</Company><Words>99999</Words><TitlesOfParts><vt:vector xmlns:vt=\"http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes\" size=\"1\" baseType=\"lpstr\"><vt:lpstr>REMOVED-CACHE</vt:lpstr></vt:vector></TitlesOfParts></Properties>")
    });
    let source = changed(&source, "word/theme/theme1.xml", |text| {
        text.replace(
            "</a:theme>",
            "<!--REMOVED-COMMENT--><?old-data REMOVED-PI?></a:theme>",
        )
    });
    let source = changed(&source, "docProps/custom.xml", |text| {
        text.replace(
            "</Properties>",
            "<!--REMOVED-CUSTOM-COMMENT--></Properties>",
        )
    });
    let imported = editable(&source);
    let output =
        export_docx_with_source(&current(imported.document()), imported.original()).unwrap();
    let values = parts(&output);
    assert!(!values.keys().any(|name| name.contains("thumbnail")));
    let all = values
        .values()
        .flat_map(|bytes| bytes.iter().copied())
        .collect::<Vec<_>>();
    let text = String::from_utf8_lossy(&all);
    assert!(!text.contains("REMOVED-"));
    assert!(!text.contains("99999"));
    assert!(xml(&values, "docProps/app.xml").contains("Kept company"));
    assert_eq!(imported.original().original_bytes(), source);
}

#[test]
fn active_normalized_settings_are_not_grafted_over_current_formatting() {
    let source = fixture("unsupported-settings-default-tabs.docx");
    let imported = editable(&source);
    let assessment = assess_docx_copy(imported.document(), imported.original()).unwrap();
    assert!(assessment.formatting_may_differ);
    let output = export_docx_with_source(imported.document(), imported.original()).unwrap();
    assert!(!parts(&output).contains_key("word/settings.xml"));
    assert_eq!(
        editable(&output).document(),
        editable(&export_docx(imported.document()).unwrap()).document()
    );
}

#[test]
fn unsafe_theme_payloads_are_explicitly_reported_and_never_attached_to_copies() {
    for payload in [
        r#"<a:blip xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" r:embed="missing"/>"#,
        r#"<a:blip xmlns:r="http://purl.oclc.org/ooxml/officeDocument/relationships" r:link="missing"/>"#,
        r#"<foreign:body xmlns:foreign="urn:unrecognized-content">Hidden content</foreign:body>"#,
        r#"<a:blip xmlns:foreign="urn:unrecognized-content" foreign:src="remote-resource"/>"#,
        r#"<a:hlinkClick action="unrecognized-active-action"/>"#,
    ] {
        let source = changed(SAFE, "word/theme/theme1.xml", |text| {
            text.replace("</a:theme>", &format!("{payload}</a:theme>"))
        });
        // The source-only importer can retain opaque unused theme XML, but the
        // copy profile must not attach that unchecked payload to a new package.
        let imported = editable(&source);
        assert!(
            assess_docx_copy(imported.document(), imported.original())
                .unwrap()
                .theme_omitted
        );
        let output = export_docx_with_source(imported.document(), imported.original()).unwrap();
        assert!(!parts(&output).contains_key("word/theme/theme1.xml"));
        assert_eq!(
            editable(&output).document(),
            editable(&export_docx(imported.document()).unwrap()).document()
        );
        assert_eq!(imported.original().original_bytes(), source);
    }
}

#[test]
fn unsupported_semantic_sources_never_enable_partial_source_copy_export() {
    let bytes = fixture("unsupported-image.docx");
    let source = DocxSource::open(&bytes).unwrap();
    let document = Document::new(id("native")).unwrap();
    assert!(assess_docx_copy(&document, &source).is_err());
    assert!(export_docx_with_source(&document, &source).is_err());
    assert_eq!(source.original_bytes(), bytes);
}

#[test]
fn meaningful_bookmarks_are_distinct_from_word_cursor_cache_and_metadata() {
    for (name, meaningful) in [
        ("_GoBack", false),
        ("User anchor", true),
        ("_TocHeading", true),
    ] {
        let source = changed(SAFE, "word/document.xml", |text| {
            text.replace("<w:p>",
            &format!("<w:p><w:bookmarkStart w:id=\"1\" w:name=\"{name}\"/><w:bookmarkEnd w:id=\"1\"/>"))
        });
        let imported = editable(&source);
        let report = assess_docx_copy(imported.document(), imported.original()).unwrap();
        assert_eq!(report.bookmarks_omitted, meaningful);
        assert!(!report.formatting_may_differ);
        let output = export_docx_with_source(imported.document(), imported.original()).unwrap();
        assert!(!xml(&parts(&output), "word/document.xml").contains("bookmarkStart"));
    }
}

#[test]
fn native_identifier_length_does_not_become_an_external_import_limit() {
    let imported = editable(SAFE);
    let document = Document::from_parts(
        id("x".repeat(1024)),
        imported.document().metadata().clone(),
        imported.document().sections().to_vec(),
        imported.document().styles().to_vec(),
    )
    .unwrap();
    assert!(export_docx_with_source(&document, imported.original()).is_ok());
}

#[test]
fn ordinary_word_and_libreoffice_sources_keep_safe_copy_support() {
    for name in [
        "microsoft-sdk-hello-world.docx",
        "libreoffice-basic.docx",
        "normalizable-wordweb-profile.docx",
    ] {
        let source = fixture(name);
        let imported = editable(&source);
        let output = export_docx_with_source(imported.document(), imported.original()).unwrap();
        assert_eq!(
            editable(&output).document(),
            editable(&export_docx(imported.document()).unwrap()).document()
        );
        assert_eq!(imported.original().original_bytes(), source);
    }
}

#[test]
fn minimal_empty_core_properties_can_receive_current_title_without_invalid_xml() {
    let source = changed(SAFE, "docProps/core.xml", |_| {
        "<cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\"/>".into()
    });
    let imported = editable(&source);
    let output =
        export_docx_with_source(&current(imported.document()), imported.original()).unwrap();
    assert_eq!(
        editable(&output).document().metadata().title,
        "Current title & <safe>"
    );
    let core = xml(&parts(&output), "docProps/core.xml");
    assert!(core.contains("xmlns:dc="));
    assert!(!core.contains(WORD));
}

#[test]
fn notice_budget_cannot_hide_active_settings_from_the_copy_decision() {
    let fonts = (0..150)
        .map(|index| {
            format!(
                "<w:font w:name=\"Font{index}\"><w:altName w:val=\"Alternate{index}\"/></w:font>"
            )
        })
        .collect::<String>();
    let source = changed(SAFE, "word/fontTable.xml", |_| {
        format!("<w:fonts xmlns:w=\"{WORD}\">{fonts}</w:fonts>")
    });
    let source = changed(&source, "word/settings.xml", |text| {
        text.replace(
            "</w:settings>",
            "<w:defaultTabStop w:val=\"720\"/></w:settings>",
        )
    });
    let imported = editable(&source);
    assert!(imported.report().omitted_normalizations > 0);
    assert!(
        !imported
            .report()
            .normalizations
            .iter()
            .any(|notice| notice.part == "word/settings.xml")
    );
    assert!(
        assess_docx_copy(imported.document(), imported.original())
            .unwrap()
            .formatting_may_differ
    );
    let output = export_docx_with_source(imported.document(), imported.original()).unwrap();
    assert!(!parts(&output).contains_key("word/settings.xml"));
    assert_eq!(imported.original().original_bytes(), source);
}

#[test]
fn metadata_presence_distinguishes_user_properties_from_title_and_producer_caches() {
    let source = changed(SAFE, "docProps/core.xml", |_| {
        "<cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:title>Ordinary title</dc:title><cp:revision>1</cp:revision></cp:coreProperties>".into()
    });
    let source = changed(&source, "docProps/custom.xml", |_| {
        "<Properties xmlns=\"http://schemas.openxmlformats.org/officeDocument/2006/custom-properties\"/>".into()
    });
    let imported = editable(&source);
    let report = assess_docx_copy(imported.document(), imported.original()).unwrap();
    assert!(!report.has_document_properties);
    assert!(!report.formatting_may_differ);
    let source = changed(&source, "docProps/app.xml", |text| {
        text.replace(
            "</Properties>",
            "<Company>Meaningful company</Company></Properties>",
        )
    });
    let imported = editable(&source);
    assert!(
        assess_docx_copy(imported.document(), imported.original())
            .unwrap()
            .has_document_properties
    );
    let output = export_docx_with_source(imported.document(), imported.original()).unwrap();
    assert!(xml(&parts(&output), "docProps/app.xml").contains("Meaningful company"));
    assert!(!xml(&parts(&output), "docProps/core.xml").contains("revision"));
}

#[test]
fn bounded_gallery_notices_never_create_false_loss_or_hide_later_active_formatting() {
    let gallery = (0..150).map(|index| format!(
        "<w:style w:type=\"paragraph\" w:styleId=\"Gallery{index}\"><w:name w:val=\"Gallery {index}\"/><w:qFormat/><w:uiPriority w:val=\"1\"/></w:style>"
    )).collect::<String>();
    let source = changed(SAFE, "word/styles.xml", |text| {
        text.replace("</w:styles>", &format!("{gallery}</w:styles>"))
    });
    let imported = editable(&source);
    assert!(imported.report().omitted_normalizations > 0);
    assert!(!imported.report().has_formatting_normalizations);
    assert!(
        !assess_docx_copy(imported.document(), imported.original())
            .unwrap()
            .formatting_may_differ
    );

    let source = changed(&source, "word/document.xml", |text| {
        text.replace("<w:r>", "<w:r><w:rPr><w:kern w:val=\"2\"/></w:rPr>")
    });
    let imported = editable(&source);
    assert!(imported.report().has_formatting_normalizations);
    assert!(
        !imported
            .report()
            .normalizations
            .iter()
            .any(|notice| notice.part == "word/document.xml")
    );
    assert!(
        assess_docx_copy(imported.document(), imported.original())
            .unwrap()
            .formatting_may_differ
    );
    let output = export_docx_with_source(imported.document(), imported.original()).unwrap();
    assert_eq!(
        editable(&output).document(),
        editable(&export_docx(imported.document()).unwrap()).document()
    );
}
