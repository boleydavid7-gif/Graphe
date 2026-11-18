//! Real raster resources and loss boundaries across external-format adapters.

use std::io::{Cursor, Read, Write};

use office_core::{
    Block, Document, DocumentMetadata, DocumentSession, Id, Image, ImageAlignment, ImageAsset,
    Paragraph, Section, TextRun, Twips, WriterFile, deserialize_writer_file, serialize_writer_file,
};
use office_formats::{
    DocumentFormat as Format, ImportOutcome, assess_export, export_document, export_file,
    import_document, import_document_with_warnings,
};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

const PNG: &[u8] = include_bytes!("fixtures/images/quadrants.png");
const JPEG: &[u8] = include_bytes!("fixtures/images/quadrants.jpg");
const ORIENTED: &[u8] = include_bytes!("fixtures/images/oriented.jpg");
const COLOR_APPROXIMATION: &[u8] = include_bytes!("fixtures/images/color-approx.png");
const ALT: &str = "Project picture — Café e\u{301} 👩🏽‍💻";

fn id(value: &str) -> Id {
    Id::new(value).unwrap()
}
fn sample(bytes: &[u8]) -> Document {
    Document::from_parts(
        id("private-image-document"),
        DocumentMetadata {
            title: "Image review".into(),
        },
        vec![Section::new(
            id("section"),
            vec![
                Block::Paragraph(Paragraph::new(
                    id("before"),
                    vec![TextRun::new("Before the picture")],
                )),
                Block::Image(
                    Image::new(
                        id("private-picture"),
                        ImageAsset::from_bytes(bytes).unwrap(),
                        Twips(2880),
                        Twips(1920),
                    )
                    .with_alt_text(ALT)
                    .with_alignment(ImageAlignment::Center),
                ),
                Block::Paragraph(Paragraph::new(
                    id("after"),
                    vec![TextRun::new("After the picture — العربية")],
                )),
            ],
        )],
        vec![],
    )
    .unwrap()
}
fn image(document: &Document) -> &Image {
    document
        .sections()
        .iter()
        .flat_map(|section| section.blocks())
        .find_map(|block| match block {
            Block::Image(image) => Some(image),
            _ => None,
        })
        .unwrap()
}
fn editable(format: Format, bytes: &[u8]) -> WriterFile {
    match import_document(
        format,
        bytes,
        id("imported"),
        &format!("sample.{}", format.extension()),
    )
    .unwrap()
    {
        ImportOutcome::Editable(file) => file,
        ImportOutcome::Preserved { report, .. } => panic!("Unexpected preservation: {report:?}"),
    }
}
fn part(bytes: &[u8], name: &str) -> Vec<u8> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut data = Vec::new();
    archive
        .by_name(name)
        .unwrap()
        .read_to_end(&mut data)
        .unwrap();
    data
}
fn change(bytes: &[u8], name: &str, edit: impl Fn(&[u8]) -> Vec<u8>) -> Vec<u8> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut output = ZipWriter::new(Cursor::new(Vec::new()));
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).unwrap();
        let path = file.name().unwrap().into_owned();
        let mut data = Vec::new();
        file.read_to_end(&mut data).unwrap();
        if path == name {
            data = edit(&data);
        }
        output
            .start_file(
                &path,
                SimpleFileOptions::default().compression_method(if path == "mimetype" {
                    CompressionMethod::Stored
                } else {
                    CompressionMethod::Deflated
                }),
            )
            .unwrap();
        output.write_all(&data).unwrap();
    }
    output.finish().unwrap().into_inner()
}

#[test]
fn odt_png_and_jpeg_resources_roundtrip_exact_bytes_size_alignment_alt_and_text_order() {
    for bytes in [PNG, JPEG] {
        let original = sample(bytes);
        let encoded = export_document(Format::Odt, &original).unwrap();
        let restored = editable(Format::Odt, &encoded);
        let picture = image(restored.document());
        assert_eq!(picture.asset().original_bytes(), bytes);
        assert_eq!(picture.asset().rgba(), image(&original).asset().rgba());
        assert_eq!(
            (picture.width(), picture.height()),
            (Twips(2880), Twips(1920))
        );
        assert_eq!(picture.alignment(), ImageAlignment::Center);
        assert_eq!(picture.alt_text(), ALT);
        assert_eq!(restored.original_source().unwrap().bytes(), encoded);
        assert_eq!(
            restored
                .document()
                .paragraphs()
                .map(|p| p.runs()[0].text.as_str())
                .collect::<Vec<_>>(),
            ["Before the picture", "After the picture — العربية"]
        );
        let manifest = String::from_utf8(part(&encoded, "META-INF/manifest.xml")).unwrap();
        assert!(manifest.contains(picture.asset().media_type()));
        assert!(
            !assess_export(Format::Odt, &original)
                .unwrap()
                .warnings
                .iter()
                .any(|issue| issue.code == "image_normalization")
        );
    }
}

#[test]
fn odt_picture_edits_native_recovery_external_reopen_and_undo_preserve_immutable_originals() {
    let encoded = export_document(Format::Odt, &sample(PNG)).unwrap();
    let imported = editable(Format::Odt, &encoded);
    let picture_id = image(imported.document()).id().clone();
    let original = imported.original_source().cloned().unwrap();
    let before = imported.document().clone();
    let mut session = DocumentSession::new(before.clone());
    let mut transaction = session.begin_transaction();
    transaction
        .resize_image(&picture_id, Twips(3600), Twips(2400))
        .unwrap();
    transaction
        .set_image_alt_text(&picture_id, "Edited description 👋".into())
        .unwrap();
    transaction
        .set_image_alignment(&picture_id, ImageAlignment::Right)
        .unwrap();
    session.commit(transaction).unwrap();
    let file = WriterFile::with_original_source(session.document().clone(), original).unwrap();
    let restored = deserialize_writer_file(&serialize_writer_file(&file).unwrap()).unwrap();
    assert_eq!(restored, file);
    assert_eq!(image(restored.document()).id(), &picture_id);
    assert_eq!(restored.original_source().unwrap().bytes(), encoded);
    assert_eq!(image(restored.document()).asset().original_bytes(), PNG);
    let copy = export_file(Format::Odt, &restored).unwrap();
    let reopened = editable(Format::Odt, &copy);
    let picture = image(reopened.document());
    assert_eq!(picture.alt_text(), "Edited description 👋");
    assert_eq!(picture.alignment(), ImageAlignment::Right);
    assert_eq!(
        (picture.width(), picture.height()),
        (Twips(3600), Twips(2400))
    );
    assert_eq!(picture.asset().original_bytes(), PNG);
    assert_eq!(image(session.document()).id(), &picture_id);
    session.undo().unwrap();
    assert_eq!(session.document(), &before);
}

#[test]
fn orientation_normalization_is_disclosed_and_exports_the_same_oriented_pixels() {
    let model = sample(ORIENTED);
    let source = image(&model).asset();
    assert!(source.has_orientation_transform());
    assert_eq!((source.width_px(), source.height_px()), (16, 24));
    assert!(
        assess_export(Format::Odt, &model)
            .unwrap()
            .warnings
            .iter()
            .any(|w| w.code == "image_normalization")
    );
    let bytes = export_document(Format::Odt, &model).unwrap();
    let file = editable(Format::Odt, &bytes);
    let normalized = image(file.document()).asset();
    assert_eq!(normalized.media_type(), "image/png");
    assert_eq!(normalized.rgba(), source.rgba());
    assert_eq!(source.original_bytes(), ORIENTED);
}

#[test]
fn color_approximation_is_disclosed_for_external_rasters_and_pdf_but_native_stays_exact() {
    let model = sample(COLOR_APPROXIMATION);
    assert!(image(&model).asset().has_color_approximation());
    for format in [Format::Odt, Format::Pdf] {
        let code = if format == Format::Pdf {
            "image_color_approximation"
        } else {
            "image_normalization"
        };
        assert!(
            assess_export(format, &model)
                .unwrap()
                .warnings
                .iter()
                .any(|warning| warning.code == code)
        );
        assert!(!export_document(format, &model).unwrap().is_empty());
    }
    let bytes = export_document(Format::Odt, &model).unwrap();
    let copy = editable(Format::Odt, &bytes);
    assert_eq!(
        image(copy.document()).asset().rgba(),
        image(&model).asset().rgba()
    );
    assert!(!image(copy.document()).asset().has_color_approximation());
    let file = WriterFile::new(model);
    let native = deserialize_writer_file(&serialize_writer_file(&file).unwrap()).unwrap();
    assert_eq!(
        image(native.document()).asset().original_bytes(),
        COLOR_APPROXIMATION
    );
}

#[test]
fn odt_picture_titles_multiline_descriptions_and_fixed_spacing_normalize_with_a_notice() {
    let source = export_document(Format::Odt, &sample(PNG)).unwrap();
    let changed = change(&source, "content.xml", |bytes| {
        String::from_utf8(bytes.to_vec())
            .unwrap()
            .replace(
                "<svg:desc>",
                "<svg:title>Separate title</svg:title><svg:desc>Line one&#10;",
            )
            .replace(
                "fo:margin-top=\"0pt\"",
                "fo:line-height=\"20pt\" fo:margin-top=\"0pt\"",
            )
            .into_bytes()
    });
    let report =
        import_document_with_warnings(Format::Odt, &changed, id("descriptions"), "picture.odt")
            .unwrap();
    assert!(report.has_formatting_normalizations);
    assert!(!report.warnings.is_empty());
    let ImportOutcome::Editable(file) = report.outcome else {
        panic!("Known description normalization became read-only")
    };
    assert_eq!(
        image(file.document()).alt_text(),
        format!("Separate title — Line one\n{ALT}")
    );
    assert_eq!(file.original_source().unwrap().bytes(), changed);
}

#[test]
fn realistic_image_fixture_survives_native_odt_copy_and_reopen_with_two_page_layout() {
    let bytes = include_bytes!("fixtures/project-review-images.writer");
    let file = deserialize_writer_file(bytes).unwrap();
    let layout =
        office_layout::layout_document(file.document(), &office_layout::LayoutOptions::default())
            .unwrap();
    assert_eq!(layout.pages.len(), 2);
    assert_eq!(
        layout
            .pages
            .iter()
            .map(|page| page.images.len())
            .sum::<usize>(),
        3
    );
    let reopened = editable(Format::Odt, &export_file(Format::Odt, &file).unwrap());
    assert_eq!(reopened.document().sections().len(), 2);
    assert_eq!(
        reopened
            .document()
            .paragraphs()
            .map(|p| p.runs().iter().map(|r| r.text.as_str()).collect::<String>())
            .collect::<Vec<_>>(),
        file.document()
            .paragraphs()
            .map(|p| p.runs().iter().map(|r| r.text.as_str()).collect::<String>())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        serialize_writer_file(&file).unwrap(),
        serialize_writer_file(
            &deserialize_writer_file(&serialize_writer_file(&file).unwrap()).unwrap()
        )
        .unwrap()
    );
}

#[test]
fn odt_unknown_wrapped_mixed_or_external_pictures_are_preserved_read_only() {
    let source = export_document(Format::Odt, &sample(PNG)).unwrap();
    for (needle, replacement) in [
        (
            "text:anchor-type=\"as-char\"",
            "text:anchor-type=\"paragraph\"",
        ),
        ("<draw:frame ", "Extra text<draw:frame "),
        (
            "<draw:frame ",
            "<draw:frame draw:transform=\"rotate(0.5)\" ",
        ),
        (
            "xlink:href=\"Pictures/image0_1.png\"",
            "xlink:href=\"https://example.invalid/picture.png\"",
        ),
        ("<draw:image ", "<draw:image draw:filter-name=\"unknown\" "),
    ] {
        let changed = change(&source, "content.xml", |xml| {
            String::from_utf8(xml.to_vec())
                .unwrap()
                .replace(needle, replacement)
                .into_bytes()
        });
        match import_document(Format::Odt, &changed, id("rejected"), "unsupported.odt").unwrap() {
            ImportOutcome::Preserved { original, report } => {
                assert_eq!(original.bytes(), changed);
                assert!(!report.is_empty());
            }
            ImportOutcome::Editable(_) => panic!("accepted unsupported {replacement}"),
        }
    }
}

#[test]
fn odt_image_signature_media_mismatch_and_corrupted_payload_are_rejected() {
    let source = export_document(Format::Odt, &sample(PNG)).unwrap();
    let mismatched = change(&source, "META-INF/manifest.xml", |xml| {
        String::from_utf8(xml.to_vec())
            .unwrap()
            .replace(
                "manifest:media-type=\"image/png\"",
                "manifest:media-type=\"image/jpeg\"",
            )
            .into_bytes()
    });
    assert!(import_document(Format::Odt, &mismatched, id("bad-media"), "bad.odt").is_err());
    let corrupted = change(&source, "Pictures/image0_1.png", |bytes| {
        bytes[..bytes.len() - 4].to_vec()
    });
    assert!(import_document(Format::Odt, &corrupted, id("bad-image"), "bad.odt").is_err());
}

#[test]
fn unsupported_animation_and_late_jpeg_metadata_preserve_the_complete_original_read_only() {
    let mut animated = PNG[..33].to_vec();
    let mut animation = b"acTL".to_vec();
    animation.extend_from_slice(&2_u32.to_be_bytes());
    animation.extend_from_slice(&0_u32.to_be_bytes());
    let mut crc = flate2::Crc::new();
    crc.update(&animation);
    animated.extend_from_slice(&8_u32.to_be_bytes());
    animated.extend_from_slice(&animation);
    animated.extend_from_slice(&crc.sum().to_be_bytes());
    animated.extend_from_slice(&PNG[33..]);
    let mut late = JPEG[..JPEG.len() - 2].to_vec();
    late.extend_from_slice(b"\xff\xfe\x00\x05abc\xff\xd9");
    for (original, name, replacement) in [
        (PNG, "Pictures/image0_1.png", animated),
        (JPEG, "Pictures/image0_1.jpg", late),
    ] {
        let source = export_document(Format::Odt, &sample(original)).unwrap();
        let changed = change(&source, name, |_| replacement.clone());
        match import_document(
            Format::Odt,
            &changed,
            id("unsupported-raster"),
            "unsupported.odt",
        )
        .unwrap()
        {
            ImportOutcome::Preserved { original, report } => {
                assert_eq!(original.bytes(), changed);
                assert!(report.iter().any(|issue| issue.code == "unsupported_image"));
            }
            _ => panic!("Unsupported raster content became editable or disappeared"),
        }
    }
}

#[test]
fn odt_aggregate_image_budget_is_checked_before_a_large_second_decode() {
    let source = export_document(Format::Odt, &sample(PNG)).unwrap();
    // A declared 4000x4000 PNG reaches the entire document pixel budget. The
    // intentionally truncated raster must never be decoded: duplicate image
    // references first trip the cheap-header aggregate preflight.
    let mut header = PNG[..33].to_vec();
    header[16..20].copy_from_slice(&4000_u32.to_be_bytes());
    header[20..24].copy_from_slice(&4000_u32.to_be_bytes());
    let mut crc = flate2::Crc::new();
    crc.update(&header[12..29]);
    header[29..33].copy_from_slice(&crc.sum().to_be_bytes());
    let changed = change(&source, "Pictures/image0_1.png", |_| header.clone());
    let repeated = change(&changed, "content.xml", |xml| {
        let text = String::from_utf8(xml.to_vec()).unwrap();
        let start = text.find("<text:p text:style-name=\"I0_1\">").unwrap();
        let end = start + text[start..].find("</text:p>").unwrap() + "</text:p>".len();
        text.replace(
            "</office:text>",
            &format!("{}</office:text>", &text[start..end]),
        )
        .into_bytes()
    });
    let error = import_document(Format::Odt, &repeated, id("budget"), "large.odt").unwrap_err();
    assert_eq!(error.code, "format_limit");
}

#[test]
fn pdf_contains_rgb_image_soft_alpha_and_the_exact_rust_page_geometry() {
    let model = sample(PNG);
    let bytes = export_document(Format::Pdf, &model).unwrap();
    let text = String::from_utf8_lossy(&bytes).replace([' ', '\n', '\r', '\t'], "");
    assert_eq!(text.matches("/Subtype/Image").count(), 2);
    assert!(text.contains("/SMask"));
    assert!(text.contains("/DeviceRGB"));
    assert!(text.contains("/DeviceGray"));
    assert!(text.contains("/Width24"));
    assert!(text.contains("/Height16"));
    assert!(!text.contains("private-image-document"));
    assert!(!text.contains("private-picture"));
    assert_eq!(bytes, export_document(Format::Pdf, &model).unwrap());
    let layout =
        office_layout::layout_document(&model, &office_layout::LayoutOptions::default()).unwrap();
    assert_eq!(layout.pages[0].images.len(), 1);
    let rect = layout.pages[0].images[0].rect;
    assert_eq!((rect.width, rect.height), (2880 * 64 / 20, 1920 * 64 / 20));
}

#[test]
fn plain_text_requires_image_loss_disclosure_and_rtf_refuses_to_drop_pictures() {
    let model = sample(PNG);
    assert!(
        assess_export(Format::Txt, &model)
            .unwrap()
            .warnings
            .iter()
            .any(|warning| warning.code == "images_as_text")
    );
    let text = String::from_utf8(export_document(Format::Txt, &model).unwrap()).unwrap();
    assert_eq!(
        text,
        format!("Before the picture\n[Image: {ALT}]\nAfter the picture — العربية")
    );
    assert_eq!(
        assess_export(Format::Rtf, &model).unwrap_err().code,
        "unsupported_rtf_export"
    );
    assert_eq!(
        export_document(Format::Rtf, &model).unwrap_err().code,
        "unsupported_rtf_export"
    );
}
