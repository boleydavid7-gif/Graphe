//! External transport preserves the complete pinned catalog and native geometry.

use std::{fmt::Write, fs, path::Path};

use office_core::{
    Block, CharacterFormat, Document, DocumentMetadata, HalfPoints, Id, Paragraph, ParagraphFormat,
    Section, TextRun,
};
use office_layout::{
    FontCatalogBuilder, LayoutEngine, LayoutError, LayoutOptions, font_catalog_assets,
    font_catalog_id, font_catalog_manifest_sha256, font_catalog_ready, install_font_catalog,
    layout_document,
};
use sha2::{Digest, Sha256};

fn bytes(id: &str) -> Vec<u8> {
    fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("fonts").join(id)).unwrap()
}

fn digest(value: &[u8]) -> String {
    let mut result = String::with_capacity(64);
    for value in Sha256::digest(value) {
        write!(result, "{value:02x}").unwrap();
    }
    result
}

fn complete() -> FontCatalogBuilder {
    let mut builder = FontCatalogBuilder::new();
    // Arrival order must not become registration/fallback order.
    for asset in font_catalog_assets().iter().rev() {
        builder
            .register(asset.font_id, &bytes(asset.font_id))
            .unwrap();
    }
    builder
}

fn document() -> Document {
    let paragraphs = font_catalog_assets()
        .iter()
        .enumerate()
        .map(|(index, asset)| {
            Block::Paragraph(
                Paragraph::new(
                    Id::new(format!("paragraph-{index}")).unwrap(),
                    vec![
                        TextRun {
                            text: format!(
                                "{} — AV ffi Café e\u{301} العربية हिन्दी עברית 👩🏽‍💻\n",
                                asset.font_id
                            ),
                            format: CharacterFormat {
                                font_family: Some(asset.family.into()),
                                font_size: Some(HalfPoints(24 + (index % 3) as u32 * 4)),
                                italic: Some(asset.style == "italic"),
                                bold: Some(asset.weight_min >= 700),
                                ..Default::default()
                            },
                        },
                        TextRun::new("Unstyled fallback and mixed formatting."),
                    ],
                )
                .with_format(ParagraphFormat {
                    spacing_after: Some(office_core::Twips(180)),
                    ..Default::default()
                }),
            )
        })
        .collect();
    Document::from_parts(
        Id::new("external-fonts").unwrap(),
        DocumentMetadata::default(),
        vec![Section::new(Id::new("section").unwrap(), paragraphs)],
        vec![],
    )
    .unwrap()
}

#[test]
fn compiled_metadata_matches_the_manifest_without_changing_registration_order() {
    let manifest_bytes = include_bytes!("../fonts/manifest.json");
    let manifest: serde_json::Value = serde_json::from_slice(manifest_bytes).unwrap();
    assert_eq!(font_catalog_id(), "writer-fonts-v6");
    assert_eq!(font_catalog_manifest_sha256(), digest(manifest_bytes));
    let assets = font_catalog_assets();
    assert_eq!(assets.len(), 42);
    assert_eq!(
        assets.iter().map(|asset| asset.byte_length).sum::<usize>(),
        19_533_340
    );
    let expected_order: Vec<_> = include_str!("../fonts/registration-order.txt")
        .lines()
        .collect();
    assert_eq!(
        assets.iter().map(|asset| asset.font_id).collect::<Vec<_>>(),
        expected_order
    );
    // The historical registration order differs deliberately from manifest order.
    assert_eq!(assets[10].font_id, "Caladea-Regular.ttf");
    assert_eq!(assets[24].font_id, "Inter-Regular.ttf");
    for asset in assets {
        let entry = manifest["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["file"] == asset.font_id)
            .unwrap();
        assert_eq!(
            entry["byte_length"].as_u64().unwrap(),
            asset.byte_length as u64
        );
        assert_eq!(entry["sha256"], asset.sha256);
        assert_eq!(entry["family"], asset.family);
        assert_eq!(entry["style"], asset.style);
        assert!(
            asset.weight_min > 0
                && asset.weight_min <= asset.weight_max
                && asset.weight_max <= 1000
        );
    }
    let inter = assets
        .iter()
        .find(|asset| asset.font_id == "Inter-Italic.ttf")
        .unwrap();
    assert_eq!(
        (
            inter.family,
            inter.style,
            inter.weight_min,
            inter.weight_max
        ),
        ("Inter", "italic", 100, 900)
    );
    let tinos = assets
        .iter()
        .find(|asset| asset.font_id == "Tinos-Bold.ttf")
        .unwrap();
    assert_eq!((tinos.weight_min, tinos.weight_max), (700, 700));
}

#[test]
fn rejected_registration_leaves_slots_retryable_and_duplicate_ids_do_not_replace_them() {
    let mut builder = FontCatalogBuilder::default();
    let first = &font_catalog_assets()[0];
    let original = bytes(first.font_id);
    for (id, value) in [
        ("unknown.ttf", original.as_slice()),
        (first.font_id, &original[..original.len() - 1]),
        (first.font_id, &[]),
    ] {
        assert!(matches!(
            builder.register(id, value),
            Err(LayoutError::Font { .. })
        ));
        assert_eq!(builder.registered_count(), 0);
    }
    let mut corrupted = original.clone();
    let index = corrupted.len() / 2;
    corrupted[index] ^= 1;
    let error = builder.register(first.font_id, &corrupted).err().unwrap();
    assert!(error.to_string().contains("SHA-256"));
    assert_eq!(builder.registered_count(), 0);
    assert!(builder.register(&"x".repeat(1024), &original).is_err());
    assert!(!builder.is_registered(first.font_id));
    builder.register(first.font_id, &original).unwrap();
    assert!(builder.is_registered(first.font_id));
    assert!(!builder.is_registered("unknown.ttf"));
    assert_eq!(builder.registered_count(), 1);
    assert!(builder.register(first.font_id, &original).is_err());
    assert!(builder.register(first.font_id, &corrupted).is_err());
    assert_eq!(builder.registered_count(), 1);
    for _ in 0..2 {
        assert!(builder.finish().is_err());
        assert_eq!(builder.registered_count(), 1);
    }
    for asset in &font_catalog_assets()[1..] {
        builder
            .register(asset.font_id, &bytes(asset.font_id))
            .unwrap();
    }
    assert_eq!(builder.registered_count(), font_catalog_assets().len());
    assert!(builder.finish().is_ok());
}

#[test]
fn externally_supplied_fonts_preserve_native_glyphs_metrics_and_pagination_exactly() {
    let builder = complete();
    let catalog = builder.finish().unwrap();
    let source = document();
    let options = LayoutOptions::default();
    let expected = LayoutEngine::new()
        .unwrap()
        .layout_document(&source, &options)
        .unwrap();
    let mut external = LayoutEngine::with_font_catalog(&catalog).unwrap();
    let actual = external.layout_document(&source, &options).unwrap();
    assert!(actual.pages.len() > 1);
    assert_eq!(actual, expected);
    assert_eq!(
        serde_json::to_vec(&actual).unwrap(),
        serde_json::to_vec(&expected).unwrap()
    );
    assert_eq!(
        external.layout_document(&source, &options).unwrap(),
        expected
    );
    assert_eq!(layout_document(&source, &options).unwrap(), expected);
}

#[test]
fn publication_requires_a_complete_catalog_and_never_resets_ready_fonts() {
    assert!(!font_catalog_ready());
    let first = &font_catalog_assets()[0];
    let mut partial = FontCatalogBuilder::new();
    partial
        .register(first.font_id, &bytes(first.font_id))
        .unwrap();
    assert!(partial.finish().is_err());
    assert!(!font_catalog_ready());
    let catalog = complete().finish().unwrap();
    install_font_catalog(catalog.clone()).unwrap();
    assert!(font_catalog_ready());
    assert!(partial.register(first.font_id, &[]).is_err());
    assert!(partial.finish().is_err());
    assert!(font_catalog_ready());
    install_font_catalog(catalog).unwrap();
    assert!(font_catalog_ready());
    let source = document();
    assert_eq!(
        LayoutEngine::new()
            .unwrap()
            .layout_document(&source, &LayoutOptions::default())
            .unwrap(),
        layout_document(&source, &LayoutOptions::default()).unwrap(),
    );
    assert_eq!(
        LayoutError::FontCatalogNotReady.code(),
        "font_catalog_not_ready"
    );
}
