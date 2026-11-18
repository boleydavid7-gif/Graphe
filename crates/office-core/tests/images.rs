//! Image resources are immutable, bounded and independent of text offsets.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use office_core::{
    Block, CharacterFormat, CharacterFormatPatch, Document, DocumentError, DocumentMetadata,
    DocumentSession, EditError, GraphemeOffset, Id, Image, ImageAlignment, ImageAsset, ImageError,
    MAX_DOCUMENT_IMAGES, MAX_IMAGE_ALT_TEXT_BYTES, MAX_IMAGE_BYTES, MAX_IMAGE_DIMENSION,
    OriginalSource, Paragraph, PropertyChange, RasterFormat, Section, Table, TableCell, TableRow,
    TextRun, Twips, WriterFile, deserialize_document, deserialize_writer_file, serialize_document,
    serialize_writer_file,
};
use serde_json::{Value, json};

const JPEG: &[u8] = include_bytes!("fixtures/images/asymmetric.jpg");
const PROGRESSIVE_JPEG: &[u8] = include_bytes!("fixtures/images/progressive.jpg");
const CMYK_JPEG: &[u8] = include_bytes!("fixtures/images/cmyk.jpg");
const PIXELS: &[u8] = &[
    1, 11, 21, 255, 2, 12, 22, 200, 3, 13, 23, 150, 4, 14, 24, 100, 5, 15, 25, 50, 6, 16, 26, 0,
];

fn id(value: &str) -> Id {
    Id::new(value).unwrap()
}
fn png_with(chunks: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 2, 3);
        encoder.set_color(png::ColorType::Rgba);
        let mut writer = encoder.write_header().unwrap();
        for (kind, payload) in chunks {
            writer
                .write_chunk(png::chunk::ChunkType(*kind), payload)
                .unwrap();
        }
        writer.write_image_data(PIXELS).unwrap();
        writer.finish().unwrap();
    }
    bytes
}
fn png_bytes() -> Vec<u8> {
    png_with(&[])
}
fn asset() -> ImageAsset {
    ImageAsset::from_bytes(&png_bytes()).unwrap()
}
fn image() -> Image {
    Image::new(id("image"), asset(), Twips(1440), Twips(2160))
        .with_alt_text("Diagram e\u{301} 😀\nSecond line\tcaption")
        .with_alignment(ImageAlignment::Center)
}
fn paragraph(name: &str, text: &str) -> Block {
    Block::Paragraph(Paragraph::new(id(name), vec![TextRun::new(text)]))
}
fn document(blocks: Vec<Block>) -> Document {
    Document::from_parts(
        id("document"),
        DocumentMetadata {
            title: "Illustrated notes".into(),
        },
        vec![Section::new(id("section"), blocks)],
        vec![],
    )
    .unwrap()
}
fn illustrated() -> Document {
    document(vec![
        paragraph("before", "e\u{301}😀"),
        Block::Image(image()),
        paragraph("after", "Next"),
    ])
}
fn tiff(orientation: u16) -> Vec<u8> {
    let mut bytes = b"II*\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0".to_vec();
    bytes.extend_from_slice(&orientation.to_le_bytes());
    bytes.extend_from_slice(&[0; 6]);
    bytes
}
fn jpeg_segment(marker: u8, payload: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0xff, marker];
    bytes.extend_from_slice(&u16::try_from(payload.len() + 2).unwrap().to_be_bytes());
    bytes.extend_from_slice(payload);
    bytes
}
fn jpeg_exif(value: u16) -> Vec<u8> {
    let mut payload = b"Exif\0\0".to_vec();
    payload.extend(tiff(value));
    jpeg_segment(0xe1, &payload)
}
fn with_jpeg_segment(bytes: &[u8], segment: &[u8]) -> Vec<u8> {
    [&bytes[..2], segment, &bytes[2..]].concat()
}
fn assert_canonical(asset: &ImageAsset) {
    let rendered = ImageAsset::from_bytes(asset.render_png()).unwrap();
    assert_eq!(rendered.rgba(), asset.rgba());
    assert_eq!(
        (rendered.width_px(), rendered.height_px()),
        (asset.width_px(), asset.height_px())
    );
    assert!(!rendered.has_orientation_transform());
    assert!(!rendered.has_color_approximation());
}

#[test]
fn png_and_jpeg_keep_exact_originals_and_share_derived_caches() {
    for bytes in [png_bytes(), JPEG.to_vec(), PROGRESSIVE_JPEG.to_vec()] {
        let asset = ImageAsset::from_bytes(&bytes).unwrap();
        assert_eq!(asset.original_bytes(), bytes);
        assert_eq!((asset.width_px(), asset.height_px()), (2, 3));
        let copy = asset.clone();
        assert!(std::ptr::eq(copy.original_bytes(), asset.original_bytes()));
        assert!(std::ptr::eq(copy.render_png(), asset.render_png()));
        assert!(std::ptr::eq(copy.rgba(), asset.rgba()));
        assert_canonical(&asset);
    }
    assert_eq!(asset().rgba(), PIXELS);
    assert_eq!(asset().format(), RasterFormat::Png);
    assert_eq!(
        ImageAsset::from_bytes(JPEG).unwrap().media_type(),
        "image/jpeg"
    );
}

#[test]
fn all_eight_png_and_jpeg_orientations_have_identical_rust_pixel_mappings() {
    let orders = [
        [0, 1, 2, 3, 4, 5],
        [1, 0, 3, 2, 5, 4],
        [5, 4, 3, 2, 1, 0],
        [4, 5, 2, 3, 0, 1],
        [0, 2, 4, 1, 3, 5],
        [4, 2, 0, 5, 3, 1],
        [5, 3, 1, 4, 2, 0],
        [1, 3, 5, 0, 2, 4],
    ];
    let jpeg_pixels = ImageAsset::from_bytes(JPEG).unwrap();
    for value in 1..=8_u16 {
        let png = png_with(&[(*b"eXIf", tiff(value))]);
        let jpeg = with_jpeg_segment(JPEG, &jpeg_exif(value));
        for (bytes, source) in [(&png[..], PIXELS), (&jpeg[..], jpeg_pixels.rgba())] {
            let asset = ImageAsset::from_bytes(bytes).unwrap();
            let expected: Vec<u8> = orders[usize::from(value - 1)]
                .iter()
                .flat_map(|i| source[i * 4..i * 4 + 4].iter().copied())
                .collect();
            assert_eq!(asset.rgba(), expected, "orientation {value}");
            assert_eq!(
                (asset.width_px(), asset.height_px()),
                if value < 5 { (2, 3) } else { (3, 2) }
            );
            assert_eq!(asset.has_orientation_transform(), value != 1);
            assert_eq!(asset.original_bytes(), bytes);
            assert_canonical(&asset);
        }
    }
}

#[test]
fn malformed_or_ambiguous_exif_is_never_silently_interpreted() {
    for value in [0, 9, u16::MAX] {
        assert!(ImageAsset::from_bytes(&png_with(&[(*b"eXIf", tiff(value))])).is_err());
        assert!(ImageAsset::from_bytes(&with_jpeg_segment(JPEG, &jpeg_exif(value))).is_err());
    }
    let mut duplicate = tiff(6);
    duplicate[8..10].copy_from_slice(&2_u16.to_le_bytes());
    let entry = duplicate[10..22].to_vec();
    duplicate.splice(22..22, entry);
    let mut wrong_type = tiff(6);
    wrong_type[12] = 4;
    let mut wrong_count = tiff(6);
    wrong_count[14] = 2;
    for raw in [duplicate, wrong_type, wrong_count, b"not a TIFF".to_vec()] {
        assert!(ImageAsset::from_bytes(&png_with(&[(*b"eXIf", raw.clone())])).is_err());
        let mut payload = b"Exif\0\0".to_vec();
        payload.extend(raw);
        assert!(
            ImageAsset::from_bytes(&with_jpeg_segment(JPEG, &jpeg_segment(0xe1, &payload)))
                .is_err()
        );
    }
    assert!(
        ImageAsset::from_bytes(&png_with(&[(*b"eXIf", tiff(1)), (*b"eXIf", tiff(6))])).is_err()
    );
    let duplicate = with_jpeg_segment(&with_jpeg_segment(JPEG, &jpeg_exif(1)), &jpeg_exif(6));
    assert!(matches!(
        ImageAsset::from_bytes(&duplicate),
        Err(ImageError::InvalidData { .. })
    ));
}

#[test]
fn repeated_exif_ifd_references_cannot_expand_fields_exponentially() {
    // Minimal repeated-IFD graph for GHSA-53gf-vqf6-qvp8. kamadak-exif
    // 0.6.2 rejects revisiting the same IFD before it can expand copied fields.
    let raw = b"MM\x00\x2a\x00\x00\x00\x08\x00\x02\x87\x69\x00\x04\x00\x00\x00\x01\x00\x00\x00\x26\x87\x69\x00\x04\x00\x00\x00\x01\x00\x00\x00\x26\x00\x00\x00\x00\x00\x01\x90\x00\x00\x07\x00\x00\x00\x040231\x00\x00\x00\x00";
    let png = png_with(&[(*b"eXIf", raw.to_vec())]);
    let mut payload = b"Exif\0\0".to_vec();
    payload.extend_from_slice(raw);
    let jpeg = with_jpeg_segment(JPEG, &jpeg_segment(0xe1, &payload));
    for bytes in [&png[..], &jpeg[..]] {
        assert!(
            matches!(ImageAsset::from_bytes(bytes), Err(ImageError::InvalidData { reason }) if reason.contains("Already visited IFD"))
        );
    }
}

#[test]
fn jpeg_container_rejects_late_metadata_trailing_bytes_and_truncated_markers() {
    let late = [
        &JPEG[..JPEG.len() - 2],
        &jpeg_exif(6),
        &JPEG[JPEG.len() - 2..],
    ]
    .concat();
    assert!(matches!(
        ImageAsset::from_bytes(&late),
        Err(ImageError::UnsupportedMetadata { .. })
    ));
    for bytes in [
        JPEG[..JPEG.len() - 2].to_vec(),
        [JPEG, &[0]].concat(),
        vec![0xff, 0xd8, 0xff, 0xe1, 0xff, 0xff, 0],
        vec![0xff, 0xd8, 0xff, 0xe1, 0, 1],
        vec![0xff, 0xd8, 0xff],
    ] {
        assert!(ImageAsset::from_bytes(&bytes).is_err());
    }
}

#[test]
fn source_color_limits_are_explicit_and_canonical_preview_keeps_alpha() {
    let icc = png_with(&[(*b"iCCP", b"retained-profile\0\0not-inflated".to_vec())]);
    let profile = ImageAsset::from_bytes(&icc).unwrap();
    assert!(profile.has_color_profile());
    assert!(profile.has_color_approximation());
    assert_eq!(profile.rgba(), PIXELS);
    assert_canonical(&profile);
    for (kind, payload) in [
        (*b"gAMA", 50000_u32.to_be_bytes().to_vec()),
        (*b"cICP", vec![9, 16, 0, 1]),
        (*b"mDCV", vec![0; 24]),
        (*b"cLLI", vec![0; 8]),
        (
            *b"cHRM",
            [31270_u32, 32900, 68000, 32000, 26500, 69000, 15000, 6000]
                .into_iter()
                .flat_map(u32::to_be_bytes)
                .collect(),
        ),
    ] {
        let bytes = png_with(&[(kind, payload)]);
        let asset = ImageAsset::from_bytes(&bytes).unwrap();
        assert!(asset.has_color_approximation());
        assert_canonical(&asset);
    }
    let cmyk = ImageAsset::from_bytes(CMYK_JPEG).unwrap();
    assert!(cmyk.has_color_approximation());
    assert_canonical(&cmyk);
}

#[test]
fn palette_transparency_grayscale_and_sixteen_bit_normalize_explicitly() {
    for (color, depth, source, expected) in [
        (
            png::ColorType::Grayscale,
            png::BitDepth::Eight,
            vec![42],
            vec![42, 42, 42, 255],
        ),
        (
            png::ColorType::GrayscaleAlpha,
            png::BitDepth::Eight,
            vec![42, 123],
            vec![42, 42, 42, 123],
        ),
        (
            png::ColorType::Grayscale,
            png::BitDepth::Sixteen,
            vec![42, 99],
            vec![42, 42, 42, 255],
        ),
        (
            png::ColorType::Indexed,
            png::BitDepth::Eight,
            vec![0],
            vec![11, 22, 33, 77],
        ),
    ] {
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
            encoder.set_color(color);
            encoder.set_depth(depth);
            if color == png::ColorType::Indexed {
                encoder.set_palette(vec![11, 22, 33]);
                encoder.set_trns(vec![77]);
            }
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&source).unwrap();
            writer.finish().unwrap();
        }
        let asset = ImageAsset::from_bytes(&bytes).unwrap();
        assert_eq!(asset.rgba(), expected);
        assert_eq!(
            asset.has_color_approximation(),
            depth == png::BitDepth::Sixteen
        );
        assert_canonical(&asset);
    }
}

#[test]
fn png_checksums_animation_and_complete_container_are_validated() {
    let original = png_with(&[(*b"tEXt", b"key\0value".to_vec())]);
    for marker in [b"IHDR", b"tEXt", b"IDAT"] {
        let mut damaged = original.clone();
        let at = damaged.windows(4).position(|s| s == marker).unwrap();
        damaged[at + 4] ^= 1;
        assert!(ImageAsset::from_bytes(&damaged).is_err(), "CRC {marker:?}");
    }
    for bytes in [
        original[..original.len() - 1].to_vec(),
        [&original[..], &[0]].concat(),
    ] {
        assert!(ImageAsset::from_bytes(&bytes).is_err());
    }
    let animation = png_with(&[(
        *b"acTL",
        [1_u32.to_be_bytes(), 0_u32.to_be_bytes()].concat(),
    )]);
    assert_eq!(
        ImageAsset::from_bytes(&animation),
        Err(ImageError::UnsupportedAnimation)
    );
}

#[test]
fn every_truncated_png_and_jpeg_prefix_fails_without_panicking() {
    for bytes in [png_bytes(), JPEG.to_vec(), PROGRESSIVE_JPEG.to_vec()] {
        for end in 0..bytes.len() {
            assert!(
                ImageAsset::from_bytes(&bytes[..end]).is_err(),
                "prefix {end}/{}",
                bytes.len()
            );
        }
    }
}

fn header_only(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    let encoder = png::Encoder::new(&mut bytes, width, height);
    drop(encoder.write_header().unwrap());
    bytes
}
#[test]
fn image_limits_are_checked_from_headers_before_raster_allocation() {
    assert!(matches!(
        ImageAsset::inspect_header(&header_only(MAX_IMAGE_DIMENSION + 1, 1)),
        Err(ImageError::LimitExceeded {
            resource: "dimension",
            ..
        })
    ));
    assert!(matches!(
        ImageAsset::inspect_header(&header_only(4001, 4000)),
        Err(ImageError::LimitExceeded {
            resource: "pixels",
            ..
        })
    ));
    assert!(matches!(
        ImageAsset::from_bytes(&vec![0; MAX_IMAGE_BYTES + 1]),
        Err(ImageError::LimitExceeded {
            resource: "bytes",
            ..
        })
    ));
    assert_eq!(
        ImageAsset::from_bytes(b"<svg/>"),
        Err(ImageError::UnsupportedFormat)
    );
    let header = ImageAsset::inspect_header(&header_only(4000, 4000)).unwrap();
    assert_eq!((header.width_px, header.height_px), (4000, 4000));
}

#[test]
fn native_roundtrip_retains_ids_unicode_originals_and_only_persists_original_asset() {
    let original = illustrated();
    let source = OriginalSource::new(
        "docx",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "original.docx",
        b"immutable source".as_slice(),
    )
    .unwrap();
    let file = WriterFile::with_original_source(original.clone(), source).unwrap();
    let bytes = serialize_writer_file(&file).unwrap();
    assert_eq!(deserialize_writer_file(&bytes).unwrap(), file);
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["format_version"], 7);
    assert_eq!(value["document"]["schema_version"], 7);
    assert_eq!(
        value["document"]["sections"][0]["blocks"][1]["content"]["asset"]
            .as_object()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        deserialize_document(&serialize_document(&original).unwrap()).unwrap(),
        original
    );
    let asset = original.images().next().unwrap().asset();
    assert_eq!(asset.original_bytes(), png_bytes());
    let restored: Document = serde_json::from_str(&original.to_json().unwrap()).unwrap();
    assert_eq!(restored, original);
}

fn image_wire(bytes: &[u8], name: &str) -> Value {
    json!({"kind":"image","content":{"id":name,"asset":{"format":"png","bytes_base64":STANDARD.encode(bytes)},"width":1440,"height":1440,"alt_text":"","alignment":"left"}})
}
#[test]
fn native_aggregate_pixel_budget_precedes_any_full_decode_even_with_malformed_rasters() {
    let mut value = serde_json::to_value(document(vec![])).unwrap();
    let bytes = header_only(4000, 4000); // IHDR is valid; IDAT is intentionally absent.
    value["sections"][0]["blocks"] = json!([image_wire(&bytes, "one"), image_wire(&bytes, "two")]);
    assert!(matches!(
        Document::from_json(&value.to_string()),
        Err(DocumentError::Image(ImageError::LimitExceeded {
            resource: "document pixels",
            ..
        }))
    ));
    assert!(
        serde_json::from_str::<Document>(&value.to_string())
            .unwrap_err()
            .to_string()
            .contains("document pixels")
    );
}

#[test]
fn native_image_count_format_base64_unknown_fields_and_duplicate_keys_are_strict() {
    let mut value = serde_json::to_value(document(vec![])).unwrap();
    value["sections"][0]["blocks"] = Value::Array(
        (0..=MAX_DOCUMENT_IMAGES)
            .map(|i| image_wire(&png_bytes(), &format!("image-{i}")))
            .collect(),
    );
    assert!(matches!(
        Document::from_json(&value.to_string()),
        Err(DocumentError::Image(ImageError::LimitExceeded {
            resource: "document count",
            ..
        }))
    ));
    let valid = illustrated().to_json().unwrap();
    for (old, replacement) in [
        ("\"format\": \"png\"", "\"format\": \"jpeg\""),
        (
            "\"alignment\": \"center\"",
            "\"alignment\": \"center\", \"unknown\": null",
        ),
        (
            "\"bytes_base64\":",
            "\"bytes_base64\": \"AAAA\", \"bytes_base64\":",
        ),
        ("\"sections\":", "\"sections\": [], \"sections\":"),
    ] {
        assert!(valid.contains(old));
        assert!(Document::from_json(&valid.replacen(old, replacement, 1)).is_err());
    }
    let mut value: Value = serde_json::from_str(&valid).unwrap();
    value["sections"][0]["blocks"][1]["content"]["asset"]["bytes_base64"] = json!("YQ");
    assert!(Document::from_json(&value.to_string()).is_err());
}

#[test]
fn images_cannot_be_smuggled_under_earlier_schema_or_envelope_labels() {
    for version in 1..6 {
        let mut value = serde_json::to_value(illustrated()).unwrap();
        value["schema_version"] = json!(version);
        assert!(
            Document::from_json(&value.to_string()).is_err(),
            "schema {version}"
        );
        let mut file: Value =
            serde_json::from_slice(&serialize_document(&illustrated()).unwrap()).unwrap();
        file["format_version"] = json!(version);
        file["document"]["schema_version"] = json!(version);
        assert!(
            deserialize_document(&serde_json::to_vec(&file).unwrap()).is_err(),
            "envelope {version}"
        );
    }
    let mut plain =
        serde_json::to_value(document(vec![paragraph("paragraph", "Old text")])).unwrap();
    plain["schema_version"] = json!(5);
    assert_eq!(
        Document::from_json(&plain.to_string())
            .unwrap()
            .schema_version(),
        office_core::SCHEMA_VERSION
    );
}

#[test]
fn strict_schema_five_migration_preserves_tables_links_and_retained_originals() {
    let linked = Paragraph::new(
        id("cell-paragraph"),
        vec![TextRun {
            text: "Reference 😀".into(),
            format: CharacterFormat {
                hyperlink: Some("https://example.com/notes".into()),
                ..Default::default()
            },
        }],
    );
    let expected = document(vec![Block::Table(
        Table::new(
            id("table"),
            vec![TableRow::new(
                id("row"),
                vec![TableCell::new(id("cell"), vec![linked])],
            )],
        )
        .with_column_widths(vec![Twips(2880)]),
    )]);
    let source = OriginalSource::new(
        "docx",
        "application/octet-stream",
        "original.docx",
        b"exact old original".as_slice(),
    )
    .unwrap();
    let file = WriterFile::with_original_source(expected.clone(), source).unwrap();
    let mut legacy: Value = serde_json::from_slice(&serialize_writer_file(&file).unwrap()).unwrap();
    legacy["format_version"] = json!(5);
    legacy["document"]["schema_version"] = json!(5);
    let restored = deserialize_writer_file(&serde_json::to_vec(&legacy).unwrap()).unwrap();
    assert_eq!(restored, file);
    assert_eq!(restored.document(), &expected);
    assert_eq!(
        restored.original_source().unwrap().bytes(),
        b"exact old original"
    );
}

#[test]
fn atomic_image_operations_have_exact_undo_and_preserve_shared_assets() {
    let original = document(vec![paragraph("before", "Text")]);
    let mut session = DocumentSession::new(original.clone());
    let supplied = image();
    let pointer = supplied.asset().rgba().as_ptr();
    let mut transaction = session.begin_transaction();
    transaction
        .insert_image_after(&id("before"), supplied)
        .unwrap();
    transaction
        .resize_image(&id("image"), Twips(2880), Twips(1000))
        .unwrap();
    transaction
        .set_image_alt_text(&id("image"), "Changed 😀".into())
        .unwrap();
    transaction
        .set_image_alignment(&id("image"), ImageAlignment::Right)
        .unwrap();
    session.commit(transaction).unwrap();
    let committed = session.document().clone();
    let current = committed.images().next().unwrap();
    assert_eq!(current.asset().rgba().as_ptr(), pointer);
    assert_eq!(current.width(), Twips(2880));
    assert_eq!(current.height(), Twips(1000));
    assert_eq!(current.alt_text(), "Changed 😀");
    assert_eq!(current.alignment(), ImageAlignment::Right);
    session.undo().unwrap();
    assert_eq!(session.document(), &original);
    session.redo().unwrap();
    assert_eq!(session.document(), &committed);
    assert_eq!(
        session
            .document()
            .images()
            .next()
            .unwrap()
            .asset()
            .rgba()
            .as_ptr(),
        pointer
    );
    let mut transaction = session.begin_transaction();
    transaction.remove_image(&id("image")).unwrap();
    session.commit(transaction).unwrap();
    assert_eq!(session.document(), &original);
    session.undo().unwrap();
    assert_eq!(session.document(), &committed);
}

#[test]
fn invalid_image_edits_and_no_ops_leave_draft_revision_and_document_unchanged() {
    let original = illustrated();
    let mut session = DocumentSession::new(original.clone());
    let revision = session.revision();
    let mut transaction = session.begin_transaction();
    assert!(
        !transaction
            .resize_image(&id("image"), image().width(), image().height())
            .unwrap()
    );
    assert!(
        !transaction
            .set_image_alt_text(&id("image"), image().alt_text().into())
            .unwrap()
    );
    assert!(
        transaction
            .resize_image(&id("image"), Twips(0), Twips(1))
            .is_err()
    );
    assert!(
        transaction
            .resize_image(&id("image"), Twips(1), Twips(-1))
            .is_err()
    );
    assert!(
        transaction
            .set_image_alt_text(&id("image"), "bad\0text".into())
            .is_err()
    );
    assert!(
        transaction
            .set_image_alt_text(&id("image"), "é".repeat(MAX_IMAGE_ALT_TEXT_BYTES / 2 + 1))
            .is_err()
    );
    assert!(
        transaction
            .insert_image_after(&id("before"), image())
            .is_err()
    );
    assert!(transaction.remove_image(&id("missing")).is_err());
    assert!(!session.commit(transaction).unwrap().changed());
    assert_eq!(session.revision(), revision);
    assert_eq!(session.document(), &original);
}

#[test]
fn text_edits_never_flatten_images_and_formatting_keeps_assets_intact() {
    let original = illustrated();
    let mut session = DocumentSession::new(original.clone());
    assert_eq!(
        original
            .paragraphs()
            .map(|p| p.id().as_str())
            .collect::<Vec<_>>(),
        ["before", "after"]
    );
    let mut transaction = session.begin_transaction();
    let start = transaction
        .position(&id("before"), GraphemeOffset::new(1))
        .unwrap();
    let end = transaction
        .position(&id("after"), GraphemeOffset::new(1))
        .unwrap();
    let selection = transaction.selection(start, end).unwrap();
    assert_eq!(
        transaction.replace_text(&selection, "X", CharacterFormat::default()),
        Err(EditError::CrossCellRange)
    );
    assert_eq!(
        transaction.delete_text(&selection),
        Err(EditError::CrossCellRange)
    );
    transaction
        .apply_character_format(
            &selection,
            &CharacterFormatPatch {
                bold: PropertyChange::Set(true),
                ..Default::default()
            },
        )
        .unwrap();
    session.commit(transaction).unwrap();
    let current = session.document().images().next().unwrap();
    let before = original.images().next().unwrap();
    assert_eq!(current, before);
    assert!(std::ptr::eq(current.asset().rgba(), before.asset().rgba()));
}

#[test]
fn an_image_only_document_can_gain_a_paragraph_without_replacing_the_image() {
    let mut session = DocumentSession::new(document(vec![Block::Image(image())]));
    let mut transaction = session.begin_transaction();
    transaction
        .insert_paragraph_after_image(&id("image"), Paragraph::new(id("new"), vec![]))
        .unwrap();
    session.commit(transaction).unwrap();
    assert_eq!(session.document().images().count(), 1);
    assert_eq!(
        session.document().paragraphs().next().unwrap().id(),
        &id("new")
    );
}

#[test]
fn drag_reordering_changes_image_flow_without_changing_pixels_and_is_atomic() {
    let original = document(vec![
        paragraph("before", "Before"),
        Block::Image(image()),
        paragraph("after", "After"),
    ]);
    let initial = original.images().next().unwrap().asset().rgba().as_ptr();
    let mut session = DocumentSession::new(original.clone());
    let mut tx = session.begin_transaction();
    assert!(
        tx.move_image_before(&id("image"), None, ImageAlignment::Right)
            .unwrap()
    );
    session.commit(tx).unwrap();
    let moved = session.document().clone();
    assert!(
        matches!(&moved.sections()[0].blocks()[2], Block::Image(i) if i.alignment() == ImageAlignment::Right)
    );
    assert_eq!(
        moved.images().next().unwrap().asset().rgba().as_ptr(),
        initial
    );
    session.undo().unwrap();
    assert_eq!(session.document(), &original);
    session.redo().unwrap();
    assert_eq!(session.document(), &moved);
    let mut rejected = session.begin_transaction();
    assert!(
        rejected
            .move_image_before(&id("image"), Some(&id("missing")), ImageAlignment::Left)
            .is_err()
    );
    assert_eq!(session.document(), &moved);
}
