//! ODF semantic, package, Unicode and preservation regressions.

use office_core::{
    Alignment, Block, CharacterFormat, Document, DocumentMetadata, DocumentSession, GraphemeOffset,
    HalfPoints, Id, Indentation, LineSpacing, NamedStyle, PageMargins, PageOrientation, PageSize,
    Paragraph, ParagraphFormat, Section, SectionProperties, TextColor, TextCoordinates, TextRun,
    Twips, Underline, WriterFile,
};
use office_formats::{
    DocumentFormat, ImportOutcome, export_document, import_document, import_document_with_warnings,
};
use std::{
    collections::BTreeMap,
    io::{Cursor, Read, Write},
};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

const ODT: DocumentFormat = DocumentFormat::Odt;
const NS: &str = "xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" xmlns:style=\"urn:oasis:names:tc:opendocument:xmlns:style:1.0\" xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" xmlns:fo=\"urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0\" office:version=\"1.3\"";
const MIME: &str = "application/vnd.oasis.opendocument.text";

fn id(value: &str) -> Id {
    Id::new(value).unwrap()
}
fn editable(bytes: &[u8]) -> WriterFile {
    match import_document(ODT, bytes, id("imported"), "sample.odt").unwrap() {
        ImportOutcome::Editable(file) => file,
        other => panic!("expected complete editable ODF projection, got {other:?}"),
    }
}
fn paragraphs(document: &Document) -> Vec<&Paragraph> {
    document
        .sections()
        .iter()
        .flat_map(|s| s.blocks())
        .map(|b| {
            let Block::Paragraph(p) = b else {
                panic!("expected paragraph fixture")
            };
            p
        })
        .collect()
}
fn text(document: &Document) -> String {
    paragraphs(document)
        .iter()
        .map(|p| p.runs().iter().map(|r| r.text.as_str()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}
fn document(text: &str) -> Document {
    Document::from_parts(
        id("d"),
        DocumentMetadata {
            title: "ODF test".into(),
        },
        vec![Section::new(
            id("s"),
            vec![Block::Paragraph(Paragraph::new(
                id("p"),
                vec![TextRun::new(text)],
            ))],
        )],
        vec![],
    )
    .unwrap()
}
fn output(document: &Document) -> Vec<u8> {
    export_document(ODT, document).unwrap()
}
fn content(body: &str) -> String {
    format!(
        "<office:document-content {NS}><office:body><office:text>{body}</office:text></office:body></office:document-content>"
    )
}
fn parts(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let mut zip = ZipArchive::new(Cursor::new(bytes)).unwrap();
    (0..zip.len())
        .map(|i| {
            let mut file = zip.by_index(i).unwrap();
            let name = file.name().unwrap().to_string();
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes).unwrap();
            (name, bytes)
        })
        .collect()
}
fn zip_parts(parts: BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default();
    writer
        .start_file(
            "mimetype",
            options.compression_method(CompressionMethod::Stored),
        )
        .unwrap();
    writer
        .write_all(
            parts
                .get("mimetype")
                .map(Vec::as_slice)
                .unwrap_or(MIME.as_bytes()),
        )
        .unwrap();
    for (name, bytes) in parts {
        if name == "mimetype" {
            continue;
        }
        writer
            .start_file(
                name,
                options.compression_method(CompressionMethod::Deflated),
            )
            .unwrap();
        writer.write_all(&bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}
fn package(content: String, styles: Option<String>) -> Vec<u8> {
    let mut parts = BTreeMap::new();
    parts.insert("content.xml".into(), content.into_bytes());
    if let Some(s) = styles {
        parts.insert("styles.xml".into(), s.into_bytes());
    }
    let entries=parts.keys().map(|name|format!("<manifest:file-entry manifest:full-path=\"{name}\" manifest:media-type=\"text/xml\"/>")).collect::<String>();
    parts.insert("META-INF/manifest.xml".into(),format!("<manifest:manifest xmlns:manifest=\"urn:oasis:names:tc:opendocument:xmlns:manifest:1.0\" manifest:version=\"1.3\"><manifest:file-entry manifest:full-path=\"/\" manifest:media-type=\"{MIME}\"/>{entries}</manifest:manifest>").into_bytes());
    zip_parts(parts)
}

#[test]
fn empty_document_is_a_valid_blank_odf_and_retains_exact_original() {
    let original = output(&Document::new(id("empty")).unwrap());
    let file = editable(&original);
    assert_eq!(text(file.document()), "");
    assert_eq!(paragraphs(file.document()).len(), 1);
    assert_eq!(file.original_source().unwrap().bytes(), original);
    assert_eq!(file.original_source().unwrap().format(), "odt");
    assert_eq!(original, output(&Document::new(id("empty")).unwrap()));
}

#[test]
fn multiple_sections_mixed_runs_paragraph_properties_geometry_and_title_roundtrip() {
    let character = CharacterFormat {
        bold: Some(true),
        italic: Some(true),
        underline: Some(Underline::Double),
        font_family: Some("Tinos".into()),
        font_size: Some(HalfPoints(29)),
        color: Some(TextColor::Rgb {
            red: 13,
            green: 127,
            blue: 239,
        }),
        hyperlink: None,
    };
    let format = ParagraphFormat {
        alignment: Some(Alignment::Justify),
        spacing_before: Some(Twips(120)),
        spacing_after: Some(Twips(75)),
        line_spacing: Some(LineSpacing::Multiple { value: 273 }),
        indentation: Some(Indentation {
            left: Some(Twips(300)),
            right: Some(Twips(80)),
            first_line: Some(Twips(-160)),
        }),
        page_break_before: Some(false),
        ..Default::default()
    };
    let geometry = SectionProperties {
        page_size: Some(PageSize {
            width: Twips(9000),
            height: Twips(12000),
            orientation: Some(PageOrientation::Portrait),
        }),
        page_margins: Some(PageMargins {
            top: Twips(600),
            right: Twips(700),
            bottom: Twips(800),
            left: Twips(900),
            header: Twips(0),
            footer: Twips(0),
            gutter: Twips(0),
        }),
    };
    let native = Document::from_parts(
        id("d"),
        DocumentMetadata {
            title: "Project & research".into(),
        },
        vec![
            Section::new(
                id("one"),
                vec![Block::Paragraph(
                    Paragraph::new(
                        id("p1"),
                        vec![
                            TextRun {
                                text: " A  café e\u{301} 👩🏽‍💻\nالعربية\tनमस्ते ".into(),
                                format: character.clone(),
                            },
                            TextRun::new("plain"),
                        ],
                    )
                    .with_format(format.clone()),
                )],
            )
            .with_properties(geometry.clone()),
            Section::new(
                id("two"),
                vec![Block::Paragraph(Paragraph::new(
                    id("p2"),
                    vec![TextRun::new("Next section")],
                ))],
            ),
        ],
        vec![],
    )
    .unwrap();
    let bytes = output(&native);
    let imported = editable(&bytes);
    let doc = imported.document();
    assert_eq!(doc.metadata(), native.metadata());
    assert_eq!(text(doc), text(&native));
    assert_eq!(doc.sections().len(), 2);
    assert_eq!(doc.sections()[0].properties(), Some(&geometry));
    assert_eq!(paragraphs(doc)[0].format(), &format);
    assert_eq!(paragraphs(doc)[0].runs()[0].format, character);
    assert_eq!(paragraphs(doc)[1].format().page_break_before, Some(true));
}

#[test]
fn named_styles_and_inheritance_do_not_grow_artificial_default_styles() {
    let mut base = NamedStyle::new(id("base"), "Body & base");
    base.character_format.font_family = Some("Arimo".into());
    base.character_format.font_size = Some(HalfPoints(22));
    let mut heading = NamedStyle::new(id("heading"), "Heading");
    heading.based_on = Some(id("base"));
    heading.character_format.bold = Some(true);
    heading.paragraph_format.alignment = Some(Alignment::Center);
    let mut native = Document::from_parts(
        id("d"),
        DocumentMetadata::default(),
        vec![Section::new(
            id("s"),
            vec![Block::Paragraph(
                Paragraph::new(id("p"), vec![TextRun::new("styled")]).with_style(id("heading")),
            )],
        )],
        vec![base, heading],
    )
    .unwrap();
    for _ in 0..3 {
        let bytes = output(&native);
        native = editable(&bytes).into_parts().0;
        assert_eq!(native.styles().len(), 2);
        let paragraph = paragraphs(&native)[0];
        let style = native
            .styles()
            .iter()
            .find(|s| Some(&s.id) == paragraph.style())
            .unwrap();
        assert_eq!(style.name, "Heading");
        assert!(style.based_on.is_some());
        let resolved = office_layout::resolve_paragraph_style(&native, paragraph);
        assert_eq!(resolved.character.bold, Some(true));
        assert_eq!(resolved.character.font_size, Some(HalfPoints(22)));
        assert_eq!(resolved.paragraph.alignment, Some(Alignment::Center));
    }
}

#[test]
fn odf_whitespace_collapses_across_spans_but_explicit_spaces_and_breaks_survive() {
    let bytes = package(
        content(
            "<text:p>  A <text:span> \n B  </text:span> C  <text:s text:c=\"3\"/><text:tab/><text:line-break/> D  </text:p>",
        ),
        None,
    );
    assert_eq!(text(editable(&bytes).document()), "A B C    \t\n D");
    let significant = "  leading and  repeated \n trailing  ";
    assert_eq!(
        text(editable(&output(&document(significant))).document()),
        significant
    );
}

#[test]
fn imported_document_edits_export_and_reopen_through_core_transactions() {
    let original = output(&document("Café e\u{301} 👩🏽‍💻"));
    let imported = editable(&original);
    let (doc, source) = imported.into_parts();
    let paragraph = paragraphs(&doc)[0];
    let pid = paragraph.id().clone();
    let end = TextCoordinates::new(paragraph).grapheme_len();
    let mut session = DocumentSession::new(doc);
    let mut tx = session.begin_transaction();
    let position = tx.position(&pid, GraphemeOffset::new(end.get())).unwrap();
    tx.insert_text(
        &position,
        " — edited",
        CharacterFormat {
            italic: Some(true),
            ..Default::default()
        },
    )
    .unwrap();
    session.commit(tx).unwrap();
    let reopened = editable(&output(session.document()));
    assert_eq!(text(reopened.document()), "Café e\u{301} 👩🏽‍💻 — edited");
    assert_eq!(
        paragraphs(reopened.document())[0]
            .runs()
            .last()
            .unwrap()
            .format
            .italic,
        Some(true)
    );
    assert_eq!(source.unwrap().bytes(), original);
}

#[test]
fn native_save_reopen_retains_odf_source_independently_of_current_content() {
    let original = output(&document("Original"));
    let file = editable(&original);
    let serialized = office_core::serialize_writer_file(&file).unwrap();
    let reopened = office_core::deserialize_writer_file(&serialized).unwrap();
    assert_eq!(file, reopened);
    assert_eq!(reopened.original_source().unwrap().bytes(), original);
}

#[test]
fn title_text_survives_comments_processing_instructions_and_cdata_boundaries() {
    let source = package(content("<text:p>body</text:p>"), None);
    let mut p = parts(&source);
    p.insert("meta.xml".into(), format!("<office:document-meta {NS}><office:meta><dc:title xmlns:dc=\"http://purl.org/dc/elements/1.1/\">A<!--comment-->B<?writer note?><![CDATA[C]]> e\u{301} 👩🏽‍💻</dc:title></office:meta></office:document-meta>").into_bytes());
    let manifest = String::from_utf8(p["META-INF/manifest.xml"].clone()).unwrap().replace("</manifest:manifest>", "<manifest:file-entry manifest:full-path=\"meta.xml\" manifest:media-type=\"text/xml\"/></manifest:manifest>");
    p.insert("META-INF/manifest.xml".into(), manifest.into_bytes());
    let imported = editable(&zip_parts(p));
    assert_eq!(imported.document().metadata().title, "ABC e\u{301} 👩🏽‍💻");
    assert_eq!(
        editable(&output(imported.document()))
            .document()
            .metadata()
            .title,
        "ABC e\u{301} 👩🏽‍💻"
    );
}

#[test]
fn title_and_style_labels_preserve_tabs_newlines_and_carriage_returns() {
    let title = "Title\tcell\nline\rcarriage";
    let style_name = "Heading\tcell\nline\rcarriage";
    let native = Document::from_parts(
        id("whitespace-metadata"),
        DocumentMetadata {
            title: title.into(),
        },
        vec![Section::new(
            id("s"),
            vec![Block::Paragraph(
                Paragraph::new(id("p"), vec![TextRun::new("Body stays ordinary")])
                    .with_style(id("style")),
            )],
        )],
        vec![NamedStyle::new(id("style"), style_name)],
    )
    .unwrap();
    let imported = editable(&output(&native));
    assert_eq!(imported.document().metadata().title, title);
    assert_eq!(imported.document().styles()[0].name, style_name);
}

#[test]
fn unsupported_tables_images_links_and_properties_preserve_original_read_only() {
    for body in [
        "<text:p><text:a xmlns:xlink=\"http://www.w3.org/1999/xlink\" xlink:href=\"https://example.invalid\">link</text:a></text:p>",
        "<table:table xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\"/>",
        "<text:p><draw:frame xmlns:draw=\"urn:oasis:names:tc:opendocument:xmlns:drawing:1.0\"/></text:p>",
        "<text:p text:style-name=\"missing\">x</text:p>",
    ] {
        let bytes = package(content(body), None);
        let outcome = import_document(ODT, &bytes, id("d"), "source.odt");
        if body.contains("missing") {
            assert!(outcome.is_err());
            continue;
        }
        match outcome.unwrap() {
            ImportOutcome::Preserved { original, report } => {
                assert_eq!(original.bytes(), bytes);
                assert!(!report.is_empty());
            }
            _ => panic!("unsupported content became editable"),
        }
    }
    let styles = format!(
        "<office:document-styles {NS}><office:styles><style:default-style style:family=\"paragraph\"><style:text-properties fo:language=\"en\"/></style:default-style></office:styles></office:document-styles>"
    );
    let bytes = package(content("<text:p>language metadata</text:p>"), Some(styles));
    let report = import_document_with_warnings(ODT, &bytes, id("d"), "source.odt").unwrap();
    assert!(matches!(report.outcome, ImportOutcome::Editable(_)));
    assert!(report.has_formatting_normalizations);
    assert!(!report.warnings.is_empty());
}

#[test]
fn independent_libreoffice_documents_normalize_known_defaults_and_preserve_originals() {
    let bytes = include_bytes!("../../../tests/fixtures/formats/libreoffice-audit-resaved.odt");
    let report = import_document_with_warnings(ODT, bytes, id("lo"), "libreoffice.odt").unwrap();
    assert!(report.has_formatting_normalizations);
    assert!(!report.warnings.is_empty());
    match report.outcome {
        ImportOutcome::Editable(file) => {
            assert_eq!(file.original_source().unwrap().bytes(), bytes);
        }
        other => panic!("ordinary LibreOffice document remained guarded: {other:?}"),
    }
}

#[test]
fn malformed_packages_mimetype_manifest_and_crc_are_rejected() {
    let valid = output(&document("valid"));
    for bytes in [&valid[..valid.len() - 8], b"not a zip"] {
        assert!(import_document(ODT, bytes, id("d"), "bad.odt").is_err());
    }
    let mut p = parts(&valid);
    p.insert("mimetype".into(), b"application/zip".to_vec());
    assert!(import_document(ODT, &zip_parts(p), id("d"), "bad.odt").is_err());
    let mut p = parts(&valid);
    p.remove("META-INF/manifest.xml");
    assert!(import_document(ODT, &zip_parts(p), id("d"), "bad.odt").is_err());
    let mut corrupt = valid.clone();
    corrupt[38] ^= 1;
    assert!(import_document(ODT, &corrupt, id("d"), "bad.odt").is_err());
}

#[test]
fn dtd_invalid_references_invalid_structure_and_future_versions_are_rejected() {
    for body in [
        content("<text:p>&#xD800;</text:p>"),
        content("hidden<text:p>visible</text:p>"),
        format!(
            "<!DOCTYPE x [<!ENTITY secret SYSTEM 'file:///etc/passwd'>]>{}",
            content("<text:p>&secret;</text:p>")
        ),
        content("<text:p>future</text:p>")
            .replace("office:version=\"1.3\"", "office:version=\"99\""),
        content("<text:p><text:s>hidden</text:s></text:p>"),
    ] {
        assert!(import_document(ODT, &package(body, None), id("d"), "bad.odt").is_err());
    }
}

#[test]
fn unresolved_or_cyclic_styles_and_invalid_formatting_do_not_become_editable() {
    for definitions in [
        "<style:style style:name=\"P\" style:family=\"paragraph\" style:parent-style-name=\"P\"/>",
        "<style:style style:name=\"P\" style:family=\"paragraph\" style:parent-style-name=\"Missing\"/>",
        "<style:style style:name=\"P\" style:family=\"paragraph\"><style:text-properties fo:font-size=\"0pt\"/></style:style>",
        "<style:style style:name=\"P\" style:family=\"paragraph\"><style:paragraph-properties fo:margin-top=\"-1pt\"/></style:style>",
    ] {
        let styles = format!(
            "<office:document-styles {NS}><office:styles>{definitions}</office:styles></office:document-styles>"
        );
        let bytes = package(
            content("<text:p text:style-name=\"P\">x</text:p>"),
            Some(styles),
        );
        assert!(import_document(ODT, &bytes, id("d"), "bad.odt").is_err());
    }
}

#[test]
fn repeated_spaces_depth_and_package_counts_are_bounded() {
    let bytes = package(
        content("<text:p><text:s text:c=\"1000000000\"/></text:p>"),
        None,
    );
    assert!(import_document(ODT, &bytes, id("d"), "big.odt").is_err());
    let deep = format!(
        "<text:p>{}x{}</text:p>",
        "<text:span>".repeat(65),
        "</text:span>".repeat(65)
    );
    assert!(import_document(ODT, &package(content(&deep), None), id("d"), "deep.odt").is_err());
}

#[test]
fn export_rejects_unrepresentable_controls_carriage_returns_gutters_and_empty_sections() {
    for text in ["NUL\0", "CR\r", "vertical\u{b}"] {
        assert!(export_document(ODT, &document(text)).is_err());
    }
    let empty = Document::from_parts(
        id("d"),
        DocumentMetadata::default(),
        vec![Section::new(id("s"), vec![])],
        vec![],
    )
    .unwrap();
    assert_eq!(
        export_document(ODT, &empty).unwrap_err().code,
        "unsupported_odt_export"
    );
    let mut geometry = office_layout::LayoutOptions::default().page_margins;
    geometry.gutter = Twips(100);
    let gutter = Document::from_parts(
        id("d"),
        DocumentMetadata::default(),
        vec![
            Section::new(
                id("s"),
                vec![Block::Paragraph(Paragraph::new(
                    id("p"),
                    vec![TextRun::new("x")],
                ))],
            )
            .with_properties(SectionProperties {
                page_margins: Some(geometry),
                ..Default::default()
            }),
        ],
        vec![],
    )
    .unwrap();
    assert!(export_document(ODT, &gutter).is_err());
}

#[test]
fn mimetype_is_first_uncompressed_and_manifest_lists_all_parts() {
    let bytes = output(&document("ordinary"));
    assert_eq!(&bytes[30..38], b"mimetype");
    assert_eq!(&bytes[38..38 + MIME.len()], MIME.as_bytes());
    assert_eq!(&bytes[8..10], &[0, 0]);
    let p = parts(&bytes);
    let manifest = std::str::from_utf8(&p["META-INF/manifest.xml"]).unwrap();
    for name in ["content.xml", "styles.xml", "meta.xml"] {
        assert!(manifest.contains(name));
    }
}

#[test]
fn shorthand_margins_and_automatic_color_are_independent_of_attribute_order() {
    for attrs in [
        "fo:margin-left=\"2in\" fo:margin=\"1in\"",
        "fo:margin=\"1in\" fo:margin-left=\"2in\"",
    ] {
        let styles = format!(
            "<office:document-styles {NS}><office:automatic-styles><style:page-layout style:name=\"L\"><style:page-layout-properties fo:page-width=\"8.5in\" fo:page-height=\"11in\" {attrs}/></style:page-layout></office:automatic-styles><office:master-styles><style:master-page style:name=\"M\" style:page-layout-name=\"L\"/></office:master-styles></office:document-styles>"
        );
        let imported = editable(&package(content("<text:p>margins</text:p>"), Some(styles)));
        let margins = imported.document().sections()[0]
            .properties()
            .unwrap()
            .page_margins
            .unwrap();
        assert_eq!(margins.left, Twips(2880));
        assert_eq!(margins.top, Twips(1440));
    }
    for attrs in [
        "fo:color=\"#ff0000\" style:use-window-font-color=\"true\"",
        "style:use-window-font-color=\"true\" fo:color=\"#ff0000\"",
    ] {
        let source = format!(
            "<office:document-content {NS}><office:automatic-styles><style:style style:name=\"T\" style:family=\"text\"><style:text-properties {attrs}/></style:style></office:automatic-styles><office:body><office:text><text:p><text:span text:style-name=\"T\">auto</text:span></text:p></office:text></office:body></office:document-content>"
        );
        let imported = editable(&package(source, None));
        assert_eq!(
            paragraphs(imported.document())[0].runs()[0].format.color,
            Some(TextColor::Auto {})
        );
    }
}

#[test]
fn duplicate_or_reordered_xml_declaration_attributes_are_rejected() {
    for declaration in [
        "<?xml version='1.0' version='1.0'?>",
        "<?xml encoding='UTF-8' version='1.0'?>",
        "<?xml version='1.0' standalone='yes' encoding='UTF-8'?>",
        "<?xml version='1.1'?>",
    ] {
        let bytes = package(
            format!("{declaration}{}", content("<text:p>data</text:p>")),
            None,
        );
        assert!(import_document(ODT, &bytes, id("d"), "bad.odt").is_err());
    }
}

#[test]
fn explicit_master_page_transitions_preserve_same_geometry_page_boundaries() {
    let styles = format!(
        "<office:document-styles {NS}><office:automatic-styles><style:page-layout style:name=\"L\"><style:page-layout-properties fo:page-width=\"8.5in\" fo:page-height=\"11in\" fo:margin=\"1in\"/></style:page-layout><style:style style:name=\"P1\" style:family=\"paragraph\" style:master-page-name=\"M1\"/><style:style style:name=\"P2\" style:family=\"paragraph\" style:master-page-name=\"M2\"/></office:automatic-styles><office:master-styles><style:master-page style:name=\"M1\" style:page-layout-name=\"L\"/><style:master-page style:name=\"M2\" style:page-layout-name=\"L\"/></office:master-styles></office:document-styles>"
    );
    let imported = editable(&package(
        content(
            "<text:p text:style-name=\"P1\">one</text:p><text:p text:style-name=\"P2\">two</text:p>",
        ),
        Some(styles),
    ));
    assert_eq!(imported.document().sections().len(), 2);
    assert_eq!(
        office_layout::layout_document(
            imported.document(),
            &office_layout::LayoutOptions::default()
        )
        .unwrap()
        .pages
        .len(),
        2
    );
}

const LIBREOFFICE_AUDIT: &[u8] =
    include_bytes!("../../../tests/fixtures/formats/libreoffice-audit-resaved.odt");

fn changed_part(source: &[u8], name: &str, change: impl FnOnce(String) -> String) -> Vec<u8> {
    let mut members = parts(source);
    let before = String::from_utf8(members[name].clone()).unwrap();
    let after = change(before.clone());
    assert_ne!(before, after, "fixture mutation must exercise its target");
    members.insert(name.into(), after.into_bytes());
    zip_parts(members)
}

fn assert_preserved(bytes: &[u8]) {
    match import_document(ODT, bytes, id("guarded"), "source.odt").unwrap() {
        ImportOutcome::Preserved { original, report } => {
            assert_eq!(original.bytes(), bytes);
            assert!(!report.is_empty());
        }
        other => panic!("unsupported content became editable: {other:?}"),
    }
}

#[test]
fn libreoffice_resave_edits_native_saves_exports_and_reopens_without_changing_source() {
    let file = editable(LIBREOFFICE_AUDIT);
    assert_eq!(file.document().metadata().title, "Project notes");
    assert_eq!(paragraphs(file.document()).len(), 6);
    let original_text = text(file.document());
    assert!(original_text.contains("Audit edit e\u{301} 👩🏽‍💻 العربية नमस्ते שלום."));
    let emphasis = paragraphs(file.document())[1]
        .runs()
        .iter()
        .find(|r| r.text == "dependable editing")
        .unwrap();
    assert_eq!(emphasis.format.bold, Some(true));
    assert_eq!(emphasis.format.italic, Some(true));
    assert_eq!(emphasis.format.underline, Some(Underline::Single));
    assert_eq!(emphasis.format.font_size, Some(HalfPoints(28)));
    assert_eq!(emphasis.format.font_family.as_deref(), Some("Tinos"));
    let (document, source) = file.into_parts();
    let paragraph = paragraphs(&document).last().unwrap().id().clone();
    let mut session = DocumentSession::new(document);
    let mut tx = session.begin_transaction();
    let position = tx.position(&paragraph, GraphemeOffset::new(0)).unwrap();
    tx.insert_text(&position, "Reviewed — ", CharacterFormat::default())
        .unwrap();
    session.commit(tx).unwrap();
    let edited =
        WriterFile::with_original_source(session.document().clone(), source.unwrap()).unwrap();
    let native = office_core::serialize_writer_file(&edited).unwrap();
    let reopened = office_core::deserialize_writer_file(&native).unwrap();
    assert_eq!(reopened, edited);
    assert_eq!(
        reopened.original_source().unwrap().bytes(),
        LIBREOFFICE_AUDIT
    );
    let exported = output(reopened.document());
    let reimported = editable(&exported);
    assert_eq!(text(reimported.document()), text(reopened.document()));
    assert_eq!(paragraphs(reimported.document()).len(), 6);
    assert_eq!(
        reimported.document().sections()[0].properties(),
        reopened.document().sections()[0].properties()
    );
    assert_eq!(
        reimported.document().metadata(),
        reopened.document().metadata()
    );
    for (before, after) in paragraphs(reopened.document())
        .iter()
        .zip(paragraphs(reimported.document()))
    {
        // ODF export may merge adjacent runs with equal formatting. Native
        // persistence above preserves exact structure; an external copy must
        // preserve text and supported formatting without requiring run splits.
        let characters = |document: &Document, paragraph: &Paragraph| {
            let inherited = office_layout::resolve_paragraph_style(document, paragraph).character;
            paragraph
                .runs()
                .iter()
                .flat_map(|run| {
                    let mut format = inherited.clone();
                    office_layout::overlay_character(&mut format, &run.format);
                    run.text
                        .chars()
                        .map(move |character| (character, format.clone()))
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            characters(reopened.document(), before),
            characters(reimported.document(), after)
        );
        assert_eq!(
            office_layout::resolve_paragraph_style(reopened.document(), before),
            office_layout::resolve_paragraph_style(reimported.document(), after)
        );
    }
    assert_eq!(
        reopened.original_source().unwrap().bytes(),
        LIBREOFFICE_AUDIT
    );
}

#[test]
fn other_libreoffice_profiles_remain_guarded_when_unknown_preferences_remain() {
    // This separate producer fixture includes additional hyphenation, font,
    // default-page-layout and application settings outside the bounded profile.
    assert_preserved(include_bytes!(
        "../../../tests/fixtures/formats/libreoffice-basic.odt"
    ));
}

#[test]
fn active_notes_lists_and_headings_remain_preserved() {
    assert_preserved(include_bytes!(
        "../../../tests/fixtures/formats/unsupported-active-note.odt"
    ));
    for body in [
        "<text:list><text:list-item><text:p>Important list item</text:p></text:list-item></text:list>",
        "<text:h text:outline-level=\"1\">Important heading semantics</text:h>",
        "<text:p><text:sequence text:name=\"Figure\">1</text:sequence></text:p>",
    ] {
        assert_preserved(&package(content(body), None));
    }
}

#[test]
fn active_or_default_enabled_numbering_is_not_misclassified_as_inactive() {
    for (from, to) in [
        ("text:number-lines=\"false\"", "text:number-lines=\"true\""),
        ("text:number-lines=\"false\"", ""),
        ("style:num-format=\"\"", "style:num-format=\"1\""),
    ] {
        assert_preserved(&changed_part(LIBREOFFICE_AUDIT, "styles.xml", |s| {
            s.replacen(from, to, 1)
        }));
    }
}

#[test]
fn content_inside_inactive_authoring_configurations_is_not_ignored() {
    for (from, to) in [
        (
            "<text:notes-configuration text:note-class=\"endnote\" style:num-format=\"i\" text:start-value=\"0\"/>",
            "<text:notes-configuration text:note-class=\"endnote\" style:num-format=\"i\" text:start-value=\"0\"><text:p>Important hidden note</text:p></text:notes-configuration>",
        ),
        (
            "<style:table-properties table:border-model=\"separating\"/>",
            "<style:table-properties table:border-model=\"separating\"><text:p>Important hidden table</text:p></style:table-properties>",
        ),
        (
            "<loext:theme-colors loext:name=\"LibreOffice\">",
            "<loext:theme-colors loext:name=\"LibreOffice\"><text:p>Important extension content</text:p>",
        ),
    ] {
        assert_preserved(&changed_part(LIBREOFFICE_AUDIT, "styles.xml", |s| {
            s.replacen(from, to, 1)
        }));
    }
}

#[test]
fn protected_or_unknown_settings_do_not_become_editable() {
    for (from, to) in [
        (
            "config:name=\"LoadReadonly\" config:type=\"boolean\">false",
            "config:name=\"LoadReadonly\" config:type=\"boolean\">true",
        ),
        (
            "config:name=\"ProtectForm\" config:type=\"boolean\">false",
            "config:name=\"ProtectForm\" config:type=\"boolean\">true",
        ),
        (
            "config:name=\"RedlineProtectionKey\" config:type=\"base64Binary\"/>",
            "config:name=\"RedlineProtectionKey\" config:type=\"base64Binary\">AQ==</config:config-item>",
        ),
        (
            "config:name=\"SaveThumbnail\"",
            "config:name=\"UnknownFutureSetting\"",
        ),
        (
            "config:name=\"SaveThumbnail\" config:type=\"boolean\"",
            "config:name=\"SaveThumbnail\" config:type=\"string\"",
        ),
    ] {
        assert_preserved(&changed_part(LIBREOFFICE_AUDIT, "settings.xml", |s| {
            s.replacen(from, to, 1)
        }));
    }
}

#[test]
fn typed_image_preferences_respect_signed_odf_integer_bounds() {
    let preference = "config:name=\"ImagePreferredDPI\" config:type=\"int\">0";
    for value in ["0", "300", "2147483647"] {
        let bytes = if value == "0" {
            LIBREOFFICE_AUDIT.to_vec()
        } else {
            changed_part(LIBREOFFICE_AUDIT, "settings.xml", |source| {
                source.replace(
                    preference,
                    &format!("config:name=\"ImagePreferredDPI\" config:type=\"int\">{value}"),
                )
            })
        };
        let report = import_document_with_warnings(ODT, &bytes, id("image"), "source.odt").unwrap();
        assert!(matches!(report.outcome, ImportOutcome::Editable(_)));
    }
    for value in ["-1", "2147483648", "4294967295", "not-an-integer"] {
        assert_preserved(&changed_part(LIBREOFFICE_AUDIT, "settings.xml", |source| {
            source.replace(
                preference,
                &format!("config:name=\"ImagePreferredDPI\" config:type=\"int\">{value}"),
            )
        }));
    }
}

#[test]
fn settings_validate_container_names_namespaces_duplicates_and_shapes() {
    for change in [
        (
            "<config:config-item-set ",
            "<config:unknown-set ",
            "</config:config-item-set>",
            "</config:unknown-set>",
        ),
        (
            "<config:config-item-set ",
            "<office:config-item-set ",
            "</config:config-item-set>",
            "</office:config-item-set>",
        ),
    ] {
        assert_preserved(&changed_part(LIBREOFFICE_AUDIT, "settings.xml", |s| {
            s.replace(change.0, change.1).replace(change.2, change.3)
        }));
    }
    assert_preserved(&changed_part(LIBREOFFICE_AUDIT, "settings.xml", |s| {
        s.replace(
            "</office:document-settings>",
            "<office:settings/></office:document-settings>",
        )
    }));
    assert_preserved(&changed_part(LIBREOFFICE_AUDIT, "settings.xml", |s| {
        s.replace(
            "</office:settings>",
            "<config:config-item-set config:name=\"ooo:view-settings\"/></office:settings>",
        )
    }));
}

#[test]
fn undefined_script_font_references_are_rejected() {
    for name in ["font-name-asian", "font-name-complex"] {
        let styles = format!(
            "<office:document-styles {NS}><office:styles><style:default-style style:family=\"paragraph\"><style:text-properties style:{name}=\"Missing font\"/></style:default-style></office:styles></office:document-styles>"
        );
        let bytes = package(content("<text:p>keep source</text:p>"), Some(styles));
        assert_eq!(
            import_document(ODT, &bytes, id("d"), "bad.odt")
                .unwrap_err()
                .code,
            "invalid_odt"
        );
    }
}

#[test]
fn malformed_text_in_property_elements_is_not_silently_discarded() {
    for property in [
        "text-properties",
        "paragraph-properties",
        "page-layout-properties",
    ] {
        let styles = if property == "page-layout-properties" {
            format!(
                "<office:document-styles {NS}><office:automatic-styles><style:page-layout style:name=\"page\"><style:{property}>Important content</style:{property}></style:page-layout></office:automatic-styles></office:document-styles>"
            )
        } else {
            format!(
                "<office:document-styles {NS}><office:styles><style:default-style style:family=\"paragraph\"><style:{property}>Important content</style:{property}></style:default-style></office:styles></office:document-styles>"
            )
        };
        let bytes = package(content("<text:p>keep source</text:p>"), Some(styles));
        assert_eq!(
            import_document(ODT, &bytes, id("d"), "bad.odt")
                .unwrap_err()
                .code,
            "invalid_odt"
        );
    }
    for declaration in [
        "<office:automatic-styles><style:page-layout style:name=\"page\">Important content</style:page-layout></office:automatic-styles>",
        "<office:automatic-styles><style:page-layout style:name=\"page\"/></office:automatic-styles><office:master-styles><style:master-page style:name=\"master\" style:page-layout-name=\"page\">Important content</style:master-page></office:master-styles>",
    ] {
        let styles = format!("<office:document-styles {NS}>{declaration}</office:document-styles>");
        let bytes = package(content("<text:p>keep source</text:p>"), Some(styles));
        assert_eq!(
            import_document(ODT, &bytes, id("d"), "bad.odt")
                .unwrap_err()
                .code,
            "invalid_odt"
        );
    }
}

#[test]
fn known_property_children_in_the_wrong_context_remain_guarded() {
    for (property, child) in [
        ("text-properties", "<style:tab-stops/>"),
        ("paragraph-properties", "<style:footnote-sep/>"),
        ("page-layout-properties", "<style:tab-stops/>"),
    ] {
        let styles = if property == "page-layout-properties" {
            format!(
                "<office:document-styles {NS}><office:automatic-styles><style:page-layout style:name=\"page\"><style:{property}>{child}</style:{property}></style:page-layout></office:automatic-styles></office:document-styles>"
            )
        } else {
            format!(
                "<office:document-styles {NS}><office:styles><style:default-style style:family=\"paragraph\"><style:{property}>{child}</style:{property}></style:default-style></office:styles></office:document-styles>"
            )
        };
        assert_preserved(&package(
            content("<text:p>keep source</text:p>"),
            Some(styles),
        ));
    }
}

#[test]
fn malformed_or_authored_auxiliary_data_is_not_dismissed_as_generated_metadata() {
    assert_preserved(&changed_part(LIBREOFFICE_AUDIT, "meta.xml", |s| {
        s.replace(
            "</office:meta>",
            "<dc:creator>Important author</dc:creator></office:meta>",
        )
    }));
    assert_preserved(&changed_part(LIBREOFFICE_AUDIT, "manifest.rdf", |s| {
        s.replace("</rdf:RDF>","<rdf:Description rdf:about=\"\"><rdf:value>Important custom metadata</rdf:value></rdf:Description></rdf:RDF>")
    }));
    assert_preserved(&changed_part(
        LIBREOFFICE_AUDIT,
        "META-INF/manifest.xml",
        |s| {
            s.replace("manifest:full-path=\"Thumbnails/thumbnail.png\" manifest:media-type=\"image/png\"","manifest:full-path=\"Thumbnails/thumbnail.png\" manifest:media-type=\"application/octet-stream\"")
        },
    ));
}

#[test]
fn normalization_notice_saturation_cannot_hide_late_unsupported_content() {
    let definitions=(0..140).map(|i|format!("<style:style style:name=\"S{i}\" style:family=\"paragraph\"><style:text-properties fo:language=\"en\"/></style:style>")).collect::<String>();
    let styles = format!(
        "<office:document-styles {NS}><office:styles>{definitions}</office:styles></office:document-styles>"
    );
    let bytes = package(
        content(
            "<text:p>body</text:p><text:p><text:note><text:note-body><text:p>Important note</text:p></text:note-body></text:note></text:p>",
        ),
        Some(styles),
    );
    let report = import_document_with_warnings(ODT, &bytes, id("d"), "source.odt").unwrap();
    assert_eq!(report.warnings.len(), 128);
    assert!(report.omitted_warnings > 0);
    assert!(report.has_formatting_normalizations);
    match report.outcome {
        ImportOutcome::Preserved { original, report } => {
            assert_eq!(original.bytes(), bytes);
            assert!(report.iter().any(|d| d.path == "note"));
        }
        other => {
            panic!("late unsupported content escaped the independent blocking report: {other:?}")
        }
    }
}
