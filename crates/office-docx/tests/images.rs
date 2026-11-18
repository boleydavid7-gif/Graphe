//! Actual OPC media relationships, image editing, and preservation boundaries.
use office_core::{
    Block, Document, DocumentMetadata, DocumentSession, Id, Image, ImageAlignment, ImageAsset,
    OriginalSource, Paragraph, Section, TextRun, Twips, WriterFile, deserialize_writer_file,
    serialize_writer_file,
};
use office_docx::{
    DOCX_MEDIA_TYPE, DocxImportOutcome, ImportedDocx, export_docx, export_docx_with_source,
    import_docx, inspect_docx,
};
use std::{
    collections::BTreeMap,
    io::{Cursor, Read, Write},
};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

const PNG: &[u8] = include_bytes!("fixtures/images/transparent.png");
const JPEG: &[u8] = include_bytes!("fixtures/images/ordinary.jpg");
const ORIENTED: &[u8] = include_bytes!("fixtures/images/orientation6.jpg");
fn id(value: &str) -> Id {
    Id::new(value).unwrap()
}
fn image(name: &str, bytes: &[u8], alignment: ImageAlignment) -> Image {
    Image::new(
        id(name),
        ImageAsset::from_bytes(bytes).unwrap(),
        Twips(2400),
        Twips(1440),
    )
    .with_alt_text("A <picture> & café e\u{301} 👩🏽‍💻 العربية\nsecond\tline")
    .with_alignment(alignment)
}
fn document() -> Document {
    Document::from_parts(
        id("pictures"),
        DocumentMetadata {
            title: "Pictures — 世界".into(),
        },
        vec![Section::new(
            id("section"),
            vec![
                Block::Paragraph(Paragraph::new(
                    id("before"),
                    vec![TextRun::new("Before — e\u{301} 👩🏽‍💻")],
                )),
                Block::Image(image("png", PNG, ImageAlignment::Center)),
                Block::Paragraph(Paragraph::new(id("between"), vec![TextRun::new("Between")])),
                Block::Image(image("jpeg", JPEG, ImageAlignment::Right)),
                Block::Paragraph(Paragraph::new(id("after"), vec![TextRun::new("After")])),
            ],
        )],
        vec![],
    )
    .unwrap()
}
fn editable(bytes: &[u8]) -> ImportedDocx {
    match import_docx(bytes, id("opened-pictures")).unwrap() {
        DocxImportOutcome::Editable(imported) => imported,
        other => panic!("expected editable picture document, got {other:?}"),
    }
}
fn images(document: &Document) -> Vec<&Image> {
    document
        .sections()
        .iter()
        .flat_map(|section| section.blocks())
        .filter_map(|block| {
            if let Block::Image(image) = block {
                Some(image)
            } else {
                None
            }
        })
        .collect()
}
fn parts(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
    (0..archive.len())
        .map(|index| {
            let mut part = archive.by_index(index).unwrap();
            let mut bytes = vec![];
            part.read_to_end(&mut bytes).unwrap();
            (part.name().unwrap().into(), bytes)
        })
        .collect()
}
fn package(parts: BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(vec![]));
    for (name, bytes) in parts {
        writer
            .start_file(name, SimpleFileOptions::default())
            .unwrap();
        writer.write_all(&bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}
fn replace(bytes: &[u8], name: &str, update: impl FnOnce(String) -> String) -> Vec<u8> {
    let mut entries = parts(bytes);
    let value = String::from_utf8(entries.remove(name).unwrap()).unwrap();
    let updated = update(value.clone());
    assert_ne!(value, updated, "test mutation must actually change input");
    entries.insert(name.into(), updated.into_bytes());
    package(entries)
}
fn preserved(bytes: &[u8]) {
    match import_docx(bytes, id("blocked")).unwrap() {
        DocxImportOutcome::Preserved { original, report } => {
            assert_eq!(original.original_bytes(), bytes);
            assert!(!report.diagnostics.is_empty());
        }
        other => panic!("unsupported picture became an editable partial document: {other:?}"),
    }
}

#[test]
fn deterministic_png_jpeg_packages_preserve_pixels_originals_alt_and_geometry() {
    let original = document();
    let bytes = export_docx(&original).unwrap();
    assert_eq!(bytes, export_docx(&original).unwrap());
    let entries = parts(&bytes);
    assert_eq!(entries["word/media/image1.png"], PNG);
    assert_eq!(entries["word/media/image2.jpg"], JPEG);
    let inventory = inspect_docx(&bytes).unwrap();
    assert!(
        inventory
            .parts
            .iter()
            .any(|part| part.name == "word/media/image1.png" && part.content_type == "image/png")
    );
    let imported = editable(&bytes);
    assert_eq!(imported.original().original_bytes(), bytes);
    assert!(
        !imported.report().has_formatting_normalizations,
        "{:?}",
        imported.report()
    );
    let before = images(&original);
    let after = images(imported.document());
    assert_eq!(after.len(), 2);
    for (a, b) in before.into_iter().zip(after) {
        assert_eq!(a.asset(), b.asset());
        assert_eq!(a.alt_text(), b.alt_text());
        assert_eq!(
            (a.width(), a.height(), a.alignment()),
            (b.width(), b.height(), b.alignment())
        );
    }
    assert_eq!(
        original.paragraphs().map(|p| p.runs()).collect::<Vec<_>>(),
        imported
            .document()
            .paragraphs()
            .map(|p| p.runs())
            .collect::<Vec<_>>()
    );
}

#[test]
fn image_edits_native_reopen_and_source_aware_copy_keep_current_state_and_original() {
    let source = export_docx(&document()).unwrap();
    let imported = editable(&source);
    let mut session = DocumentSession::new(imported.document().clone());
    let image_id = images(session.document())[0].id().clone();
    let mut edit = session.begin_transaction();
    edit.resize_image(&image_id, Twips(1800), Twips(1080))
        .unwrap();
    edit.set_image_alt_text(&image_id, "Edited — 🖼️".into())
        .unwrap();
    edit.set_image_alignment(&image_id, ImageAlignment::Left)
        .unwrap();
    session.commit(edit).unwrap();
    let file = WriterFile::with_original_source(
        session.document().clone(),
        OriginalSource::new("docx", DOCX_MEDIA_TYPE, "pictures.docx", source.as_slice()).unwrap(),
    )
    .unwrap();
    let reopened = deserialize_writer_file(&serialize_writer_file(&file).unwrap()).unwrap();
    assert_eq!(reopened.document(), session.document());
    assert_eq!(reopened.original_source().unwrap().bytes(), source);
    let copy = export_docx_with_source(reopened.document(), imported.original()).unwrap();
    let restored = editable(&copy);
    let picture = images(restored.document())[0];
    assert_eq!(
        (picture.width(), picture.height()),
        (Twips(1800), Twips(1080))
    );
    assert_eq!(picture.alt_text(), "Edited — 🖼️");
    assert_eq!(picture.alignment(), ImageAlignment::Left);
    assert_eq!(picture.asset().original_bytes(), PNG);
    assert_eq!(imported.original().original_bytes(), source);
}

#[test]
fn deleting_images_does_not_graft_original_media_back_into_source_aware_copy() {
    let source = export_docx(&document()).unwrap();
    let imported = editable(&source);
    let mut session = DocumentSession::new(imported.document().clone());
    let ids = images(session.document())
        .iter()
        .map(|image| image.id().clone())
        .collect::<Vec<_>>();
    let mut edit = session.begin_transaction();
    for id in ids {
        edit.remove_image(&id).unwrap();
    }
    session.commit(edit).unwrap();
    let bytes = export_docx_with_source(session.document(), imported.original()).unwrap();
    let entries = parts(&bytes);
    assert!(!entries.keys().any(|name| name.starts_with("word/media/")));
    assert!(!String::from_utf8_lossy(&entries["word/_rels/document.xml.rels"]).contains("/image"));
    assert!(images(editable(&bytes).document()).is_empty());
    assert_eq!(imported.original().original_bytes(), source);
}

#[test]
fn oriented_jpeg_external_copy_uses_same_normalized_pixels_as_preview() {
    let asset = ImageAsset::from_bytes(ORIENTED).unwrap();
    assert!(asset.has_orientation_transform());
    assert_eq!((asset.width_px(), asset.height_px()), (3, 6));
    let document = Document::from_parts(
        id("oriented"),
        DocumentMetadata::default(),
        vec![Section::new(
            id("section"),
            vec![Block::Image(image(
                "rotated",
                ORIENTED,
                ImageAlignment::Left,
            ))],
        )],
        vec![],
    )
    .unwrap();
    let bytes = export_docx(&document).unwrap();
    let entries = parts(&bytes);
    assert_eq!(entries["word/media/image1.png"], asset.render_png());
    assert!(!entries.contains_key("word/media/image1.jpg"));
    let imported = editable(&bytes);
    assert_eq!(images(imported.document())[0].asset().rgba(), asset.rgba());
    assert_eq!(images(&document)[0].asset().original_bytes(), ORIENTED);
    // A source whose raw JPEG carries orientation also remains editable with a
    // meaningful normalization notice, while the exact source is recoverable.
    let mut raw = entries;
    raw.insert("word/media/image1.png".into(), ORIENTED.into());
    let ct = String::from_utf8(raw.remove("[Content_Types].xml").unwrap())
        .unwrap()
        .replace("image/png", "image/jpeg");
    raw.insert("[Content_Types].xml".into(), ct.into_bytes());
    let source = package(raw);
    let imported = editable(&source);
    assert!(imported.report().has_formatting_normalizations);
    assert_eq!(
        images(imported.document())[0].asset().original_bytes(),
        ORIENTED
    );
    assert_eq!(imported.original().original_bytes(), source);
}

#[test]
fn floating_mixed_cropped_rotated_effects_and_unknown_extensions_remain_read_only() {
    let source = export_docx(&document()).unwrap();
    for (from, to) in [
        ("wp:inline", "wp:anchor"),
        ("<w:drawing>", "<w:t>Mixed image text</w:t><w:drawing>"),
        ("<a:stretch>", "<a:srcRect l=\"100\"/><a:stretch>"),
        ("<a:xfrm>", "<a:xfrm rot=\"60000\">"),
        (
            "<pic:spPr>",
            "<pic:spPr><a:effectLst><a:blur rad=\"100\"/></a:effectLst>",
        ),
        ("<a:stretch><a:fillRect/></a:stretch>", "<a:tile/>"),
        (
            "<pic:cNvPicPr/>",
            "<pic:cNvPicPr><a:hlinkClick r:id=\"rIdLink\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"/></pic:cNvPicPr>",
        ),
        (
            "<a:stretch>",
            "<a:extLst><a:ext uri=\"unknown\"><svgBlip xmlns=\"http://schemas.microsoft.com/office/drawing/2016/SVG/main\"/></a:ext></a:extLst><a:stretch>",
        ),
    ] {
        let bytes = replace(&source, "word/document.xml", |xml| {
            if from == "wp:inline" {
                xml.replace(from, to)
            } else {
                xml.replacen(from, to, 1)
            }
        });
        preserved(&bytes);
    }
}

#[test]
fn image_relationships_require_consumed_main_body_semantics_not_harmless_filenames() {
    let source = export_docx(&document()).unwrap();
    let external = replace(&source, "word/_rels/document.xml.rels", |xml| {
        xml.replacen(
            "Target=\"media/image1.png\"",
            "Target=\"https://example.invalid/picture.png\" TargetMode=\"External\"",
            1,
        )
    });
    preserved(&external);
    let unknown = replace(&source, "word/_rels/document.xml.rels", |xml| {
        xml.replacen("/relationships/image", "/relationships/oleObject", 1)
    });
    preserved(&unknown);
    let unused = replace(&source, "word/_rels/document.xml.rels", |xml| {
        xml.replacen("</Relationships>","<Relationship Id=\"unusedImage\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"media/image1.png\"/></Relationships>",1)
    });
    preserved(&unused);
    let table = replace(&source, "word/document.xml", |xml| {
        let start = xml.find("<w:p><w:pPr><w:spacing").unwrap();
        let end = start + xml[start..].find("</w:p>").unwrap() + 6;
        format!(
            "{}<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"4000\"/></w:tblGrid><w:tr><w:tc><w:tcPr/>{}</w:tc></w:tr></w:tbl>{}",
            &xml[..start],
            &xml[start..end],
            &xml[end..]
        )
    });
    preserved(&table);
}

#[test]
fn malformed_payloads_identifiers_and_dimensions_are_rejected() {
    let source = export_docx(&document()).unwrap();
    for (from, to) in [
        ("cx=\"1524000\"", "cx=\"0\""),
        ("cy=\"914400\"", "cy=\"no\""),
        ("<wp:docPr id=\"2\"", "<wp:docPr id=\"1\""),
        ("<pic:cNvPr id=\"0\"", "<pic:cNvPr id=\"4294967296\""),
        (
            "<w:drawing>",
            "<w:rPr><w:rStyle w:val=\"missing-image-style\"/></w:rPr><w:drawing>",
        ),
    ] {
        let bytes = replace(&source, "word/document.xml", |xml| {
            xml.replacen(from, to, 1)
        });
        assert!(import_docx(&bytes, id("invalid")).is_err(), "accepted {to}");
    }
    let mismatch = replace(&source, "[Content_Types].xml", |xml| {
        xml.replacen("ContentType=\"image/png\"", "ContentType=\"image/jpeg\"", 1)
    });
    assert!(import_docx(&mismatch, id("mismatch")).is_err());
    let mut invalid = parts(&source);
    invalid.insert("word/media/image1.png".into(), b"not a picture".to_vec());
    assert!(import_docx(&package(invalid), id("corrupt")).is_err());
    let mut missing = parts(&source);
    missing.remove("word/media/image1.png");
    assert!(import_docx(&package(missing), id("missing")).is_err());
}

#[test]
fn fractional_emu_extents_normalize_explicitly_and_final_image_section_is_not_synthesized() {
    let source = export_docx(&document()).unwrap();
    let fractional = replace(&source, "word/document.xml", |xml| {
        xml.replace("1524000", "1524001")
    });
    let imported = editable(&fractional);
    assert!(imported.report().has_formatting_normalizations);
    assert_eq!(images(imported.document())[0].width(), Twips(2400));
    let doc = Document::from_parts(
        id("sections"),
        DocumentMetadata::default(),
        vec![
            Section::new(
                id("first"),
                vec![Block::Image(image("picture", PNG, ImageAlignment::Left))],
            ),
            Section::new(id("second"), vec![]),
        ],
        vec![],
    )
    .unwrap();
    assert!(export_docx(&doc).is_err());
}

#[test]
fn strict_namespace_family_and_known_word_local_dpi_extension_are_supported() {
    let source = export_docx(&document()).unwrap();
    let dpi = replace(&source, "word/document.xml", |xml| {
        xml.replacen("r:embed=\"rIdImage1\"/>", "r:embed=\"rIdImage1\"><a:extLst><a:ext uri=\"{28A0092B-C50C-407E-A947-70E740481C1C}\"><a14:useLocalDpi xmlns:a14=\"http://schemas.microsoft.com/office/drawing/2010/main\" val=\"0\"/></a:ext></a:extLst></a:blip>", 1)
    });
    assert_eq!(images(editable(&dpi).document()).len(), 2);
    let mut strict = parts(&source);
    for (name, bytes) in &mut strict {
        if name.ends_with(".xml") || name.ends_with(".rels") {
            let xml = String::from_utf8(bytes.clone()).unwrap();
            *bytes = xml
                .replace(
                    "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
                    "http://purl.oclc.org/ooxml/wordprocessingml/main",
                )
                .replace(
                    "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
                    "http://purl.oclc.org/ooxml/officeDocument/relationships",
                )
                .replace(
                    "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing",
                    "http://purl.oclc.org/ooxml/drawingml/wordprocessingDrawing",
                )
                .replace(
                    "http://schemas.openxmlformats.org/drawingml/2006/main",
                    "http://purl.oclc.org/ooxml/drawingml/main",
                )
                .replace(
                    "http://schemas.openxmlformats.org/drawingml/2006/picture",
                    "http://purl.oclc.org/ooxml/drawingml/picture",
                )
                .into_bytes();
        }
    }
    assert_eq!(images(editable(&package(strict)).document()).len(), 2);
}

#[test]
fn unsupported_image_carrier_breaks_and_formatting_are_never_silently_dropped() {
    let source = export_docx(&document()).unwrap();
    for property in [
        "<w:pageBreakBefore/>",
        "<w:spacing w:line=\"120\" w:lineRule=\"exact\"/>",
    ] {
        let bytes = replace(&source, "word/document.xml", |xml| {
            xml.replacen("<w:spacing w:before=\"0\" w:after=\"0\"/>", property, 1)
        });
        preserved(&bytes);
    }
    let normalized = replace(&source, "word/document.xml", |xml| {
        xml.replacen(
            "<w:spacing w:before=\"0\" w:after=\"0\"/>",
            "<w:spacing w:before=\"120\" w:after=\"240\"/><w:ind w:left=\"720\" w:right=\"360\"/>",
            1,
        )
    });
    let imported = editable(&normalized);
    assert!(imported.report().has_formatting_normalizations);
    assert_eq!(
        images(imported.document())[0].alignment(),
        ImageAlignment::Center
    );
    let run_format = replace(&source, "word/document.xml", |xml| {
        xml.replacen(
            "<w:drawing>",
            "<w:rPr><w:b/><w:sz w:val=\"28\"/></w:rPr><w:drawing>",
            1,
        )
    });
    assert!(editable(&run_format).report().has_formatting_normalizations);
    let oversized_alt = replace(&source, "word/document.xml", |xml| {
        xml.replacen("descr=\"", &format!("descr=\"{}", "x".repeat(8193)), 1)
    });
    preserved(&oversized_alt);
}

#[test]
fn repeated_media_relationships_count_each_image_against_document_resource_limits() {
    let source = export_docx(&document()).unwrap();
    let mut entries = parts(&source);
    let xml = String::from_utf8(entries.remove("word/document.xml").unwrap()).unwrap();
    let start = xml.find("<w:p><w:pPr><w:spacing").unwrap();
    let end = start + xml[start..].find("</w:p>").unwrap() + 6;
    let paragraph = &xml[start..end];
    let copies = (1..=33)
        .map(|index| paragraph.replace("<wp:docPr id=\"1\"", &format!("<wp:docPr id=\"{index}\"")))
        .collect::<String>();
    entries.insert(
        "word/document.xml".into(),
        format!("{}{}{}", &xml[..start], copies, &xml[end..]).into_bytes(),
    );
    assert!(import_docx(&package(entries), id("too-many")).is_err());
}

#[test]
fn aggregate_pixel_budget_is_checked_before_decoding_a_repeated_image() {
    let source = export_docx(&document()).unwrap();
    let mut entries = parts(&source);
    entries.insert(
        "word/media/image1.png".into(),
        include_bytes!("fixtures/images/pixel-budget.png").to_vec(),
    );
    let xml = String::from_utf8(entries.remove("word/document.xml").unwrap()).unwrap();
    let start = xml.find("<w:p><w:pPr><w:spacing").unwrap();
    let end = start + xml[start..].find("</w:p>").unwrap() + 6;
    let duplicate = xml[start..end].replace("<wp:docPr id=\"1\"", "<wp:docPr id=\"3\"");
    entries.insert(
        "word/document.xml".into(),
        format!("{}{}{}", &xml[..start], duplicate, &xml[start..]).into_bytes(),
    );
    let error = import_docx(&package(entries), id("pixel-budget")).unwrap_err();
    assert!(
        matches!(error, office_docx::ConversionError::InvalidDocument { reason, .. } if reason.contains("aggregate image pixels"))
    );
}

#[test]
fn jpeg_metadata_after_first_scan_stays_preserved_instead_of_being_ignored() {
    let source = export_docx(&document()).unwrap();
    let mut entries = parts(&source);
    let mut late = JPEG[..JPEG.len() - 2].to_vec();
    late.extend_from_slice(&[0xff, 0xfe, 0, 6, b'l', b'a', b't', b'e', 0xff, 0xd9]);
    entries.insert("word/media/image2.jpg".into(), late);
    preserved(&package(entries));
}

#[test]
fn realistic_native_and_independent_libreoffice_packages_keep_all_three_pictures() {
    let raw_libreoffice = include_bytes!("fixtures/images/libreoffice-review.docx");
    // The unmodified consumer output still has pre-existing unsupported
    // compatibility switches and an explicit run-level page break. Keep that
    // whole file read-only; testing its picture syntax never grants editability.
    let DocxImportOutcome::Preserved { original, report } =
        import_docx(raw_libreoffice, id("raw-libreoffice")).unwrap()
    else {
        panic!("unsupported page break must remain protected")
    };
    assert_eq!(original.original_bytes(), raw_libreoffice);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|issue| issue.part == "word/settings.xml" && issue.path == "compatSetting")
    );
    assert!(report.diagnostics.iter().any(|issue| issue.path == "br"));
    // An explicitly modified test projection isolates LibreOffice's real image
    // syntax from those unrelated pre-existing profile restrictions. This is
    // test construction, not an importer normalization or permission bypass.
    let known_settings = replace(raw_libreoffice, "word/settings.xml", |xml| {
        let start = xml.find("<w:compat>").unwrap();
        let end = xml.find("</w:compat>").unwrap() + "</w:compat>".len();
        format!("{}{}", &xml[..start], &xml[end..])
    });
    let picture_profile = replace(&known_settings, "word/document.xml", |xml| {
        xml.replace("<w:br w:type=\"page\"/>", "")
    });
    for source in [
        include_bytes!("fixtures/images/project-review.docx").as_slice(),
        picture_profile.as_slice(),
    ] {
        let imported = editable(source);
        assert_eq!(images(imported.document()).len(), 3);
        assert_eq!(imported.original().original_bytes(), source);
        let bytes = export_docx_with_source(imported.document(), imported.original()).unwrap();
        let reopened = editable(&bytes);
        for (expected, actual) in images(imported.document())
            .into_iter()
            .zip(images(reopened.document()))
        {
            assert_eq!(actual.alt_text(), expected.alt_text());
            assert_eq!(actual.asset().rgba(), expected.asset().rgba());
            assert_eq!(actual.width(), expected.width());
            assert_eq!(actual.height(), expected.height());
            assert_eq!(actual.alignment(), expected.alignment());
        }
    }
}
