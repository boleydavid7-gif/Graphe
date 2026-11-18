//! Standalone image flow uses Rust geometry without deriving text positions.

use office_core::{
    Block, Document, DocumentMetadata, Id, Image, ImageAlignment, ImageAsset, PageMargins,
    PageSize, Paragraph, ParagraphFormat, Section, Table, TableCell, TableRow, TextRun, Twips,
    deserialize_document, serialize_document,
};
use office_layout::{
    CaretAffinity, LayoutEngine, LayoutError, LayoutOptions, LayoutPosition, NavigationDirection,
    UNITS_PER_POINT,
};

const PIXELS: &[u8] = include_bytes!("fixtures/images/two-pixels.png");

fn id(value: &str) -> Id {
    Id::new(value).unwrap()
}

fn image(name: &str, width: i32, height: i32) -> Image {
    Image::new(
        id(name),
        ImageAsset::from_bytes(PIXELS).unwrap(),
        Twips(width),
        Twips(height),
    )
    .with_alt_text(String::from("Diagram — Café e\u{301} 👩🏽‍💻 العربية"))
}

fn paragraph(name: &str, text: &str) -> Paragraph {
    Paragraph::new(id(name), vec![TextRun::new(text)])
}

fn source(blocks: Vec<Block>) -> Document {
    Document::from_parts(
        id("document"),
        DocumentMetadata::default(),
        vec![Section::new(id("section"), blocks)],
        vec![],
    )
    .unwrap()
}

fn small_pages() -> LayoutOptions {
    LayoutOptions {
        page_size: PageSize {
            width: Twips(6000),
            height: Twips(4000),
            orientation: None,
        },
        page_margins: PageMargins {
            top: Twips(300),
            right: Twips(300),
            bottom: Twips(300),
            left: Twips(300),
            header: Twips(0),
            footer: Twips(0),
            gutter: Twips(0),
        },
        ..Default::default()
    }
}

fn endpoint(paragraph_id: &str, utf16: usize) -> LayoutPosition {
    LayoutPosition {
        paragraph_id: paragraph_id.into(),
        utf16,
        affinity: CaretAffinity::Leading,
    }
}

#[test]
fn image_between_paragraphs_uses_source_size_and_preserves_text_coordinates() {
    let document = source(vec![
        Block::Paragraph(paragraph("before", "Before")),
        Block::Image(image("picture", 1200, 1000)),
        Block::Paragraph(paragraph("after", "After")),
    ]);
    let before = serialize_document(&document).unwrap();
    let output = LayoutEngine::new()
        .unwrap()
        .layout_document(&document, &small_pages())
        .unwrap();
    assert_eq!(output.pages.len(), 1);
    let page = &output.pages[0];
    let placed = &page.images[0];
    assert_eq!(placed.image_id, "picture");
    assert_eq!(placed.rect.x, page.content_rect.x);
    assert_eq!(placed.rect.width, 60 * UNITS_PER_POINT);
    assert_eq!(placed.rect.height, 50 * UNITS_PER_POINT);
    let first = &page.lines[0];
    let last = &page.lines[1];
    assert_eq!(first.paragraph_id, "before");
    assert_eq!(last.paragraph_id, "after");
    assert_eq!(placed.rect.y, first.y + first.height + 8 * UNITS_PER_POINT);
    assert_eq!(last.y, placed.rect.y + placed.rect.height);
    let selected = output
        .selection_geometry(&endpoint("before", 0), &endpoint("after", 5))
        .unwrap();
    assert_eq!(selected.anchor.position.paragraph_id, "before");
    assert_eq!(selected.focus.position.paragraph_id, "after");
    assert!(selected.rectangles.iter().all(|rectangle| {
        rectangle.rect.y + rectangle.rect.height <= placed.rect.y
            || rectangle.rect.y >= placed.rect.y + placed.rect.height
    }));
    assert_eq!(serialize_document(&document).unwrap(), before);
}

#[test]
fn image_alignment_is_measured_in_the_usable_page_not_the_browser() {
    let document = source(vec![
        Block::Image(image("left", 1200, 500)),
        Block::Image(image("center", 1200, 500).with_alignment(ImageAlignment::Center)),
        Block::Image(image("right", 1200, 500).with_alignment(ImageAlignment::Right)),
    ]);
    let output = LayoutEngine::new()
        .unwrap()
        .layout_document(&document, &small_pages())
        .unwrap();
    let page = &output.pages[0];
    assert_eq!(page.images.len(), 3);
    let width = page.images[0].rect.width;
    assert_eq!(page.images[0].rect.x, page.content_rect.x);
    assert_eq!(
        page.images[1].rect.x,
        page.content_rect.x + (page.content_rect.width - width) / 2
    );
    assert_eq!(
        page.images[2].rect.x,
        page.content_rect.x + page.content_rect.width - width
    );
    for adjacent in page.images.windows(2) {
        assert_eq!(
            adjacent[1].rect.y,
            adjacent[0].rect.y + adjacent[0].rect.height
        );
    }
}

#[test]
fn neighboring_paragraph_spacing_remains_explicit_around_an_image() {
    let document = source(vec![
        Block::Paragraph(paragraph("before", "Before").with_format(ParagraphFormat {
            spacing_after: Some(Twips(600)),
            ..Default::default()
        })),
        Block::Image(image("picture", 1200, 1000)),
        Block::Paragraph(paragraph("after", "After").with_format(ParagraphFormat {
            spacing_before: Some(Twips(340)),
            ..Default::default()
        })),
    ]);
    let output = LayoutEngine::new()
        .unwrap()
        .layout_document(&document, &small_pages())
        .unwrap();
    let page = &output.pages[0];
    let placed = &page.images[0];
    assert_eq!(
        placed.rect.y,
        page.lines[0].y + page.lines[0].height + 30 * UNITS_PER_POINT
    );
    assert_eq!(
        page.lines[1].y,
        placed.rect.y + placed.rect.height + 17 * UNITS_PER_POINT
    );
}

#[test]
fn image_only_pages_are_nonempty_and_images_paginate_without_overlap() {
    let document = source(vec![
        Block::Image(image("first", 1000, 3400)),
        Block::Image(image("second", 1000, 3400)),
        Block::Paragraph(paragraph("after", "After full-page images")),
    ]);
    let output = LayoutEngine::new()
        .unwrap()
        .layout_document(&document, &small_pages())
        .unwrap();
    assert_eq!(output.pages.len(), 3);
    assert_eq!(output.pages[0].images[0].image_id, "first");
    assert_eq!(output.pages[1].images[0].image_id, "second");
    assert!(output.pages[0].lines.is_empty());
    assert!(output.pages[1].lines.is_empty());
    assert_eq!(output.pages[2].lines[0].paragraph_id, "after");
    for (index, page) in output.pages.iter().enumerate() {
        assert_eq!(page.number as usize, index + 1);
        for picture in &page.images {
            assert_eq!(picture.rect.y, page.content_rect.y);
            assert_eq!(picture.rect.height, page.content_rect.height);
        }
    }
}

#[test]
fn image_moves_intact_after_text_when_the_remaining_space_is_insufficient() {
    let document = source(vec![
        Block::Paragraph(paragraph("before", "Before")),
        Block::Image(image("picture", 2000, 3300)),
        Block::Paragraph(paragraph("after", "After")),
    ]);
    let output = LayoutEngine::new()
        .unwrap()
        .layout_document(&document, &small_pages())
        .unwrap();
    assert_eq!(output.pages.len(), 3);
    assert!(output.pages[0].images.is_empty());
    assert_eq!(
        output.pages[1].images[0].rect.y,
        output.pages[1].content_rect.y
    );
    assert_eq!(output.pages[1].images[0].rect.height, 165 * UNITS_PER_POINT);
    assert_eq!(output.pages[2].lines[0].paragraph_id, "after");
    assert!(output.diagnostics.iter().any(|issue| {
        issue.kind == "page_spacing_suppressed" && issue.paragraph_id.as_deref() == Some("before")
    }));
    let moved = output
        .move_caret(&endpoint("before", 6), NavigationDirection::Down, None)
        .unwrap();
    assert_eq!(moved.caret.position.paragraph_id, "after");
    assert_eq!(moved.caret.page_number, 3);
}

#[test]
fn an_explicit_paragraph_page_break_after_an_image_creates_a_new_page() {
    let document = source(vec![
        Block::Image(image("picture", 1000, 500)),
        Block::Paragraph(paragraph("after", "Break").with_format(ParagraphFormat {
            page_break_before: Some(true),
            ..Default::default()
        })),
    ]);
    let output = LayoutEngine::new()
        .unwrap()
        .layout_document(&document, &small_pages())
        .unwrap();
    assert_eq!(output.pages.len(), 2);
    assert_eq!(output.pages[0].images.len(), 1);
    assert_eq!(output.pages[1].lines[0].paragraph_id, "after");
}

#[test]
fn image_only_page_before_table_cannot_be_reused_as_a_blank_page() {
    let table = Table::new(
        id("table"),
        vec![TableRow::new(
            id("row"),
            vec![TableCell::new(
                id("cell"),
                vec![paragraph("cell-p", "Cell")],
            )],
        )],
    );
    let document = source(vec![
        Block::Image(image("picture", 1000, 3400)),
        Block::Table(table),
    ]);
    let output = LayoutEngine::new()
        .unwrap()
        .layout_document(&document, &small_pages())
        .unwrap();
    assert_eq!(output.pages.len(), 2);
    assert_eq!(output.pages[0].images.len(), 1);
    assert!(output.pages[0].table_cells.is_empty());
    assert!(output.pages[1].images.is_empty());
    assert_eq!(output.pages[1].table_cells.len(), 1);
    assert_eq!(
        output.pages[1].table_cells[0].rect.y,
        output.pages[1].content_rect.y
    );
}

#[test]
fn a_picture_has_no_fabricated_text_caret_even_when_paragraphs_share_its_page() {
    let document = source(vec![
        Block::Paragraph(paragraph("before", "Before")),
        Block::Image(image("picture", 1000, 1000)),
        Block::Paragraph(paragraph("after", "After")),
    ]);
    let output = LayoutEngine::new()
        .unwrap()
        .layout_document(&document, &small_pages())
        .unwrap();
    let placed = &output.pages[0].images[0];
    assert!(
        output
            .hit_test(1, placed.rect.x + 1, placed.rect.y + 1)
            .unwrap()
            .is_none()
    );
    let at = output.caret_geometry(&endpoint("after", 0)).unwrap();
    assert_eq!(
        output
            .hit_test(1, at.rect.x, at.rect.y + 1)
            .unwrap()
            .unwrap()
            .position
            .paragraph_id,
        "after"
    );
    let only = source(vec![Block::Image(image("only", 1000, 1000))]);
    let only = LayoutEngine::new()
        .unwrap()
        .layout_document(&only, &small_pages())
        .unwrap();
    assert!(only.hit_test(1, 1000, 1000).unwrap().is_none());
}

#[test]
fn images_are_not_implicitly_resized_or_removed_when_the_page_cannot_contain_them() {
    for (width, height) in [(5401, 500), (500, 3401)] {
        let document = source(vec![Block::Image(image("picture", width, height))]);
        let before = serialize_document(&document).unwrap();
        let failure = LayoutEngine::new()
            .unwrap()
            .layout_document(&document, &small_pages())
            .unwrap_err();
        assert!(matches!(failure, LayoutError::Unsupported { .. }));
        assert_eq!(serialize_document(&document).unwrap(), before);
    }
}

#[test]
fn native_image_reopen_preserves_layout_and_fixed_unit_rounding() {
    let document = source(vec![Block::Image(
        image("opaque:[id]🌟", 21, 31).with_alignment(ImageAlignment::Right),
    )]);
    let restored = deserialize_document(&serialize_document(&document).unwrap()).unwrap();
    let mut engine = LayoutEngine::new().unwrap();
    let original = engine.layout_document(&document, &small_pages()).unwrap();
    assert_eq!(
        original,
        engine.layout_document(&restored, &small_pages()).unwrap()
    );
    assert_eq!(original.pages[0].images[0].rect.width, 67);
    assert_eq!(original.pages[0].images[0].rect.height, 99);
    assert_eq!(original.pages[0].images[0].image_id, "opaque:[id]🌟");
    let encoded = serde_json::to_string(&original).unwrap();
    assert!(encoded.contains("\"images\":["));
    assert!(!encoded.contains("bytes_base64"));
    let text_only = source(vec![Block::Paragraph(paragraph("plain", "Plain"))]);
    let text_only = engine.layout_document(&text_only, &small_pages()).unwrap();
    assert!(
        !serde_json::to_string(&text_only)
            .unwrap()
            .contains("\"images\"")
    );
}
