//! Independent bidi, page-transition and selection interaction regressions.

use office_core::{
    Block, Document, DocumentMetadata, Id, PageMargins, PageSize, Paragraph, Section, TextRun,
    Twips,
};
use office_layout::{
    CaretAffinity, DocumentLayout, LayoutEngine, LayoutOptions, LayoutPosition, NavigationDirection,
};

fn id(value: &str) -> Id {
    Id::new(value).unwrap()
}

fn source(text: &str) -> Document {
    Document::from_parts(
        id("quality-document"),
        DocumentMetadata::default(),
        vec![Section::new(
            id("quality-section"),
            vec![Block::Paragraph(Paragraph::new(
                id("quality-paragraph"),
                vec![TextRun::new(text)],
            ))],
        )],
        vec![],
    )
    .unwrap()
}

fn render(text: &str, options: &LayoutOptions) -> DocumentLayout {
    LayoutEngine::new()
        .unwrap()
        .layout_document(&source(text), options)
        .unwrap()
}

fn endpoint(utf16: usize, affinity: CaretAffinity) -> LayoutPosition {
    LayoutPosition {
        paragraph_id: "quality-paragraph".into(),
        utf16,
        affinity,
    }
}

fn small_pages() -> LayoutOptions {
    LayoutOptions {
        page_size: PageSize {
            width: Twips(3000),
            height: Twips(1800),
            orientation: None,
        },
        page_margins: PageMargins {
            top: Twips(360),
            right: Twips(360),
            bottom: Twips(360),
            left: Twips(360),
            header: Twips(0),
            footer: Twips(0),
            gutter: Twips(0),
        },
        ..Default::default()
    }
}

#[test]
fn pure_rtl_wraps_walk_forward_left_and_backward_right_without_losing_text_positions() {
    let text = "שלום עולם ".repeat(14);
    let output = render(&text, &small_pages());
    assert!(output.pages.len() > 1);
    let length = text.encode_utf16().count();
    let mut current = endpoint(0, CaretAffinity::Leading);
    let previous = output
        .move_caret(&current, NavigationDirection::Right, None)
        .unwrap();
    assert_eq!(previous.caret.position.utf16, 0);
    for expected in 1..=length {
        let next = output
            .move_caret(&current, NavigationDirection::Left, None)
            .unwrap();
        assert_eq!(next.caret.position.utf16, expected);
        current = next.caret.position.endpoint();
    }
    let after = output
        .move_caret(&current, NavigationDirection::Left, None)
        .unwrap();
    assert_eq!(after.caret.position.utf16, length);
    for expected in (0..length).rev() {
        let next = output
            .move_caret(&current, NavigationDirection::Right, None)
            .unwrap();
        assert_eq!(next.caret.position.utf16, expected);
        current = next.caret.position.endpoint();
    }
}

#[test]
fn pointer_chosen_upstream_rtl_wrap_edge_can_continue_in_the_reading_direction() {
    let output = render(&"שלום עולם ".repeat(10), &small_pages());
    let first = &output.pages[0].lines[0];
    let utf16 = first.text_range.end_utf16;
    let upstream = output
        .caret_geometry(&endpoint(utf16, CaretAffinity::Trailing))
        .unwrap();
    assert_eq!(upstream.line_index, 0);
    let hit = output
        .hit_test(
            upstream.page_number,
            upstream.rect.x,
            upstream.rect.y + upstream.rect.height / 2,
        )
        .unwrap()
        .unwrap();
    assert_eq!(hit.position.utf16, utf16);
    let next = output
        .move_caret(&hit.position.endpoint(), NavigationDirection::Left, None)
        .unwrap();
    assert_eq!(next.caret.position.utf16, utf16 + 1);
    assert!(next.caret.line_index > upstream.line_index);
}

#[test]
fn a_mixed_bidi_logical_range_keeps_unselected_visual_gap_unhighlighted() {
    let output = render("abc אבג DEF", &LayoutOptions::default());
    let forward = output
        .selection_geometry(
            &endpoint(2, CaretAffinity::Leading),
            &endpoint(6, CaretAffinity::Leading),
        )
        .unwrap();
    // Logical c/space/aleph/bet leaves visually preceding gimel unselected.
    assert_eq!(forward.rectangles.len(), 2);
    let first = &forward.rectangles[0].rect;
    let second = &forward.rectangles[1].rect;
    assert!(first.width > 0 && second.width > 0);
    assert!(first.x + first.width < second.x);
    let reversed = output
        .selection_geometry(
            &endpoint(6, CaretAffinity::Leading),
            &endpoint(2, CaretAffinity::Leading),
        )
        .unwrap();
    assert_eq!(forward.rectangles, reversed.rectangles);
    assert_eq!(forward.anchor, reversed.focus);
    assert_eq!(forward.focus, reversed.anchor);
}

#[test]
fn crlf_is_one_core_break_and_downstream_geometry_can_cross_a_page() {
    let mut options = small_pages();
    options.page_size.height = Twips(1440);
    let output = render("a\r\nb", &options);
    assert_eq!(output.pages.len(), 2);
    assert!(
        output
            .caret_geometry(&endpoint(2, CaretAffinity::Leading))
            .is_err()
    );
    let before = output
        .caret_geometry(&endpoint(1, CaretAffinity::Leading))
        .unwrap();
    let after = output
        .caret_geometry(&endpoint(3, CaretAffinity::Leading))
        .unwrap();
    assert_eq!(before.page_number, 1);
    assert_eq!(after.page_number, 2);
    let right = output
        .move_caret(
            &before.position.endpoint(),
            NavigationDirection::Right,
            None,
        )
        .unwrap();
    assert_eq!(right.caret.position.utf16, 3);
    assert_eq!(right.caret.page_number, 2);
    let left = output
        .move_caret(
            &right.caret.position.endpoint(),
            NavigationDirection::Left,
            None,
        )
        .unwrap();
    assert_eq!(left.caret.position.utf16, 1);
    assert_eq!(left.caret.page_number, 1);
    let selection = output
        .selection_geometry(&before.position.endpoint(), &after.position.endpoint())
        .unwrap();
    assert_eq!(selection.rectangles.len(), 1);
    assert_eq!(selection.rectangles[0].page_number, 1);
    assert_eq!(selection.rectangles[0].rect.width, 64);
}

#[test]
fn extreme_public_coordinates_are_errors_and_layout_output_is_unchanged() {
    let output = render("abc אבג\nlast", &LayoutOptions::default());
    let before = output.clone();
    for value in [i64::MIN, i64::MAX] {
        assert!(output.hit_test(1, value, 0).is_err());
        assert!(output.hit_test(1, 0, value).is_err());
        assert!(
            output
                .move_caret(
                    &endpoint(0, CaretAffinity::Leading),
                    NavigationDirection::Down,
                    Some(value)
                )
                .is_err()
        );
    }
    for point in [
        endpoint(usize::MAX, CaretAffinity::Leading),
        LayoutPosition {
            paragraph_id: "missing".into(),
            ..endpoint(0, CaretAffinity::Leading)
        },
    ] {
        assert!(output.caret_geometry(&point).is_err());
        assert!(
            output
                .selection_geometry(&point, &endpoint(0, CaretAffinity::Leading))
                .is_err()
        );
    }
    assert_eq!(output, before);
}
