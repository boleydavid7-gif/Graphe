//! Native interaction regressions for Rust-authoritative page geometry.

use std::collections::BTreeSet;

use office_core::{
    Block, CharacterFormat, Document, DocumentMetadata, GraphemeOffset, Id, PageMargins, PageSize,
    Paragraph, Section, TextCoordinates, TextRun, Twips,
};
use office_layout::{
    CaretAffinity, DocumentLayout, LayoutEngine, LayoutError, LayoutOptions, LayoutPosition,
    NavigationDirection,
};

fn id(value: &str) -> Id {
    Id::new(value).unwrap()
}

fn paragraph(value: &str, text: &str) -> Paragraph {
    Paragraph::new(id(value), vec![TextRun::new(text)])
}

fn document(paragraphs: Vec<Paragraph>) -> Document {
    Document::from_parts(
        id("interaction-document"),
        DocumentMetadata::default(),
        vec![Section::new(
            id("interaction-section"),
            paragraphs.into_iter().map(Block::Paragraph).collect(),
        )],
        vec![],
    )
    .unwrap()
}

fn layout(document: &Document) -> DocumentLayout {
    LayoutEngine::new()
        .unwrap()
        .layout_document(document, &LayoutOptions::default())
        .unwrap()
}

fn position(paragraph: &str, utf16: usize) -> LayoutPosition {
    LayoutPosition {
        paragraph_id: paragraph.into(),
        utf16,
        affinity: CaretAffinity::Leading,
    }
}

fn compact_options() -> LayoutOptions {
    LayoutOptions {
        page_size: PageSize {
            width: Twips(3000),
            height: Twips(2400),
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
fn hit_testing_uses_native_caret_geometry_and_clamps_within_a_page() {
    let output = layout(&document(vec![paragraph("p", "Hello")]));
    let caret = output.caret_geometry(&position("p", 2)).unwrap();
    let hit = output
        .hit_test(
            caret.page_number,
            caret.rect.x,
            caret.rect.y + caret.rect.height / 2,
        )
        .unwrap()
        .unwrap();
    assert_eq!(hit.position.paragraph_id, "p");
    assert_eq!(hit.position.utf16, 2);
    assert_eq!(hit.position.utf8, 2);
    assert_eq!(hit.position.grapheme, 2);
    assert_eq!(hit.rect, caret.rect);

    let before = output.hit_test(1, -10_000, -10_000).unwrap().unwrap();
    assert_eq!(before.position.utf16, 0);
    let after = output.hit_test(1, 1_000_000, 1_000_000).unwrap().unwrap();
    assert_eq!(after.position.utf16, 5);
    assert!(output.hit_test(0, 0, 0).is_err());
    assert!(output.hit_test(2, 0, 0).is_err());
}

#[test]
fn a_sectionless_blank_document_has_no_synthetic_editing_position() {
    let source = Document::new(id("empty")).unwrap();
    let output = layout(&source);
    assert!(output.hit_test(1, 0, 0).unwrap().is_none());
    assert!(output.caret_geometry(&position("missing", 0)).is_err());
}

#[test]
fn extreme_hit_and_preferred_x_coordinates_return_structured_errors_without_overflow() {
    let output = layout(&document(vec![paragraph("p", "first\nsecond")]));
    for (x, y) in [(i64::MIN, 0), (i64::MAX, 0), (0, i64::MIN), (0, i64::MAX)] {
        assert!(matches!(
            output.hit_test(1, x, y),
            Err(LayoutError::InvalidPosition { .. })
        ));
    }
    for x in [i64::MIN, i64::MAX] {
        assert!(matches!(
            output.move_caret(&position("p", 0), NavigationDirection::Down, Some(x)),
            Err(LayoutError::InvalidPosition { .. })
        ));
    }
}

#[test]
fn combining_emoji_and_cross_run_boundaries_use_exact_core_coordinates() {
    let paragraph = Paragraph::new(
        id("unicode"),
        vec![
            TextRun::new("Ae"),
            TextRun {
                text: "\u{301}👩🏽‍💻🇺🇳Z".into(),
                format: CharacterFormat {
                    bold: Some(true),
                    ..Default::default()
                },
            },
        ],
    );
    let coordinates = TextCoordinates::new(&paragraph);
    let source = document(vec![paragraph]);
    let before = source.to_json().unwrap();
    let output = layout(&source);
    for index in 0..=coordinates.grapheme_len().get() {
        let grapheme = GraphemeOffset::new(index);
        let utf16 = coordinates.grapheme_to_utf16(grapheme).unwrap().get();
        let utf8 = coordinates.grapheme_to_utf8(grapheme).unwrap().get();
        let caret = output.caret_geometry(&position("unicode", utf16)).unwrap();
        assert_eq!(caret.position.grapheme, index);
        assert_eq!(caret.position.utf8, utf8);
        assert_eq!(caret.position.utf16, utf16);
    }
    for invalid in [2, 4, 5, 9, 11, 13, 16] {
        assert!(
            output
                .caret_geometry(&position("unicode", invalid))
                .is_err()
        );
    }
    assert_eq!(source.to_json().unwrap(), before);
}

#[test]
fn mandatory_line_break_defaults_to_downstream_and_end_stays_before_the_break() {
    let output = layout(&document(vec![paragraph("p", "a\nb")]));
    let downstream = output.caret_geometry(&position("p", 2)).unwrap();
    assert_eq!(downstream.line_index, 1);
    assert_eq!(downstream.position.affinity, CaretAffinity::Leading);
    let upstream = output
        .caret_geometry(&LayoutPosition {
            affinity: CaretAffinity::Trailing,
            ..position("p", 2)
        })
        .unwrap();
    assert_eq!(upstream.line_index, 0);
    assert!(upstream.rect.y < downstream.rect.y);

    let end = output
        .move_caret(&position("p", 0), NavigationDirection::End, None)
        .unwrap();
    assert_eq!(end.caret.position.utf16, 1);
    let right = output
        .move_caret(&position("p", 1), NavigationDirection::Right, None)
        .unwrap();
    assert_eq!(right.caret.position.utf16, 2);
    assert_eq!(right.caret.line_index, 1);
    assert_eq!(right.caret.position.affinity, CaretAffinity::Leading);
    let left = output
        .move_caret(&position("p", 2), NavigationDirection::Left, None)
        .unwrap();
    assert_eq!(left.caret.position.utf16, 1);
}

#[test]
fn crlf_and_unicode_separators_move_as_complete_core_graphemes() {
    for separator in ["\r\n", "\r", "\u{2028}", "\u{2029}"] {
        let text = format!("a{separator}b");
        let after = 1 + separator.encode_utf16().count();
        let output = layout(&document(vec![paragraph("p", &text)]));
        let end = output
            .move_caret(&position("p", 0), NavigationDirection::End, None)
            .unwrap();
        assert_eq!(end.caret.position.utf16, 1);
        let right = output
            .move_caret(&position("p", 1), NavigationDirection::Right, None)
            .unwrap();
        assert_eq!(right.caret.position.utf16, after);
        assert_eq!(right.caret.line_index, 1);
        let left = output
            .move_caret(&position("p", after), NavigationDirection::Left, None)
            .unwrap();
        assert_eq!(left.caret.position.utf16, 1);
    }
}

#[test]
fn a_selected_soft_break_has_a_visible_marker_on_its_source_line() {
    let output = layout(&document(vec![paragraph("p", "a\nb")]));
    let selection = output
        .selection_geometry(&position("p", 1), &position("p", 2))
        .unwrap();
    assert_eq!(selection.rectangles.len(), 1);
    let rectangle = &selection.rectangles[0];
    assert_eq!(rectangle.page_number, 1);
    assert_eq!(rectangle.line_index, 0);
    assert_eq!(rectangle.rect.width, 64);
    assert!(rectangle.rect.height > 0);
}

#[test]
fn horizontal_wrapped_navigation_has_no_duplicate_logical_stops() {
    let text = "word ".repeat(60);
    let source = document(vec![paragraph("p", &text)]);
    let output = LayoutEngine::new()
        .unwrap()
        .layout_document(&source, &compact_options())
        .unwrap();
    assert!(output.pages.len() > 1);
    let mut current = position("p", 0);
    let mut observed = BTreeSet::new();
    observed.insert(0);
    for expected in 1..=text.len() {
        let next = output
            .move_caret(&current, NavigationDirection::Right, None)
            .unwrap();
        assert_eq!(next.caret.position.utf16, expected);
        assert!(observed.insert(next.caret.position.utf16));
        current = position("p", next.caret.position.utf16);
        current.affinity = next.caret.position.affinity;
    }
    let end = output
        .move_caret(&current, NavigationDirection::Right, None)
        .unwrap();
    assert_eq!(end.caret.position.utf16, text.len());
    for expected in (0..text.len()).rev() {
        let next = output
            .move_caret(&current, NavigationDirection::Left, None)
            .unwrap();
        assert_eq!(next.caret.position.utf16, expected);
        current = position("p", expected);
        current.affinity = next.caret.position.affinity;
    }
}

#[test]
fn vertical_navigation_keeps_preferred_x_across_a_short_intermediate_line() {
    let output = layout(&document(vec![paragraph("p", "abcdefghij\nx\nabcdefghij")]));
    let start = output.caret_geometry(&position("p", 8)).unwrap();
    let down = output
        .move_caret(&position("p", 8), NavigationDirection::Down, None)
        .unwrap();
    assert_eq!(down.caret.line_index, 1);
    assert_eq!(down.preferred_x, Some(start.rect.x));
    let down_again = output
        .move_caret(
            &LayoutPosition {
                paragraph_id: down.caret.position.paragraph_id,
                utf16: down.caret.position.utf16,
                affinity: down.caret.position.affinity,
            },
            NavigationDirection::Down,
            down.preferred_x,
        )
        .unwrap();
    assert_eq!(down_again.caret.line_index, 2);
    assert_eq!(down_again.caret.position.utf16, 21);
    assert_eq!(down_again.preferred_x, Some(start.rect.x));
    let up = output
        .move_caret(
            &position("p", 21),
            NavigationDirection::Up,
            down_again.preferred_x,
        )
        .unwrap();
    assert_eq!(up.caret.line_index, 1);
    assert_eq!(up.preferred_x, Some(start.rect.x));
    let right = output
        .move_caret(
            &position("p", 8),
            NavigationDirection::Right,
            Some(start.rect.x),
        )
        .unwrap();
    assert_eq!(right.preferred_x, None);
}

#[test]
fn bidi_selection_keeps_disjoint_visual_rectangles_and_endpoint_direction() {
    let output = layout(&document(vec![paragraph("p", "abc שלום 123 العربية xyz")]));
    let forward = output
        .selection_geometry(&position("p", 3), &position("p", 5))
        .unwrap();
    assert_eq!(forward.anchor.position.utf16, 3);
    assert_eq!(forward.focus.position.utf16, 5);
    assert_eq!(forward.rectangles.len(), 2);
    let mut rectangles: Vec<_> = forward.rectangles.iter().map(|item| item.rect).collect();
    rectangles.sort_by_key(|rect| rect.x);
    assert!(rectangles[0].x + rectangles[0].width < rectangles[1].x);

    let reverse = output
        .selection_geometry(&position("p", 5), &position("p", 3))
        .unwrap();
    assert_eq!(reverse.anchor.position.utf16, 5);
    assert_eq!(reverse.focus.position.utf16, 3);
    assert_eq!(forward.rectangles, reverse.rectangles);
}

#[test]
fn bidi_affinities_retain_distinct_visual_caret_positions() {
    let output = layout(&document(vec![paragraph("p", "abc שלום 123 العربية xyz")]));
    let leading = output.caret_geometry(&position("p", 4)).unwrap();
    let trailing = output
        .caret_geometry(&LayoutPosition {
            affinity: CaretAffinity::Trailing,
            ..position("p", 4)
        })
        .unwrap();
    assert_eq!(leading.position.utf16, trailing.position.utf16);
    assert_ne!(leading.rect.x, trailing.rect.x);
    assert_eq!(leading.position.affinity, CaretAffinity::Leading);
    assert_eq!(trailing.position.affinity, CaretAffinity::Trailing);

    let right = output
        .move_caret(&position("p", 6), NavigationDirection::Right, None)
        .unwrap();
    assert_eq!(right.caret.position.utf16, 5);
    let left = output
        .move_caret(&position("p", 5), NavigationDirection::Left, None)
        .unwrap();
    assert_eq!(left.caret.position.utf16, 6);
}

#[test]
fn home_and_end_are_visual_edges_in_a_right_to_left_line() {
    let output = layout(&document(vec![paragraph("p", "שלום")]));
    let home = output
        .move_caret(&position("p", 2), NavigationDirection::Home, None)
        .unwrap();
    let end = output
        .move_caret(&position("p", 2), NavigationDirection::End, None)
        .unwrap();
    assert_eq!(home.caret.position.utf16, 4);
    assert_eq!(end.caret.position.utf16, 0);
    assert!(home.caret.rect.x < end.caret.rect.x);
}

#[test]
fn mixed_bidi_horizontal_navigation_terminates_without_alias_stops_or_cycles() {
    let text = "abc שלום 123 العربية xyz";
    let output = layout(&document(vec![paragraph("p", text)]));
    for (edge, direction) in [
        (NavigationDirection::Home, NavigationDirection::Right),
        (NavigationDirection::End, NavigationDirection::Left),
    ] {
        let mut current = output
            .move_caret(&position("p", 0), edge, None)
            .unwrap()
            .caret;
        let mut visited = BTreeSet::new();
        visited.insert((current.position.utf16, current.position.affinity));
        let mut terminated = false;
        for _ in 0..text.chars().count() * 3 {
            let next = output
                .move_caret(&current.position.endpoint(), direction, None)
                .unwrap()
                .caret;
            if next == current {
                terminated = true;
                break;
            }
            assert_ne!(next.position.utf16, current.position.utf16);
            assert!(visited.insert((next.position.utf16, next.position.affinity)));
            if direction == NavigationDirection::Right {
                assert!(next.rect.x >= current.rect.x);
            } else {
                assert!(next.rect.x <= current.rect.x);
            }
            current = next;
        }
        assert!(terminated, "visual navigation did not reach its line edge");
        let destination_edge = if direction == NavigationDirection::Right {
            NavigationDirection::End
        } else {
            NavigationDirection::Home
        };
        let destination = output
            .move_caret(&position("p", 0), destination_edge, None)
            .unwrap();
        assert_eq!(current.rect.x, destination.caret.rect.x);
        assert_eq!(visited.len(), text.chars().count() + 1);
    }
}

#[test]
fn selection_across_pages_and_sections_keeps_stable_source_identifiers() {
    let source = Document::from_parts(
        id("cross-section"),
        DocumentMetadata::default(),
        vec![
            Section::new(id("s1"), vec![Block::Paragraph(paragraph("p1", "first"))]),
            Section::new(id("s2"), vec![Block::Paragraph(paragraph("p2", "second"))]),
        ],
        vec![],
    )
    .unwrap();
    let before = source.to_json().unwrap();
    let output = layout(&source);
    let selected = output
        .selection_geometry(&position("p1", 2), &position("p2", 3))
        .unwrap();
    assert!(
        selected
            .rectangles
            .iter()
            .any(|rect| { rect.page_number == 1 && rect.paragraph_id == "p1" })
    );
    assert!(
        selected
            .rectangles
            .iter()
            .any(|rect| { rect.page_number == 2 && rect.paragraph_id == "p2" })
    );
    let start = output
        .move_caret(&position("p2", 2), NavigationDirection::DocumentStart, None)
        .unwrap();
    assert_eq!(start.caret.position.paragraph_id, "p1");
    assert_eq!(start.caret.position.utf16, 0);
    let end = output
        .move_caret(&position("p1", 2), NavigationDirection::DocumentEnd, None)
        .unwrap();
    assert_eq!(end.caret.position.paragraph_id, "p2");
    assert_eq!(end.caret.position.utf16, 6);
    let down = output
        .move_caret(&position("p1", 2), NavigationDirection::Down, None)
        .unwrap();
    assert_eq!(down.caret.page_number, 2);
    assert_eq!(down.caret.position.paragraph_id, "p2");
    let next = output
        .move_caret(&position("p1", 5), NavigationDirection::Right, None)
        .unwrap();
    assert_eq!(next.caret.position.paragraph_id, "p2");
    assert_eq!(next.caret.position.utf16, 0);
    let previous = output
        .move_caret(&position("p2", 0), NavigationDirection::Left, None)
        .unwrap();
    assert_eq!(previous.caret.position.paragraph_id, "p1");
    assert_eq!(previous.caret.position.utf16, 5);
    assert_eq!(source.to_json().unwrap(), before);
}

#[test]
fn collapsed_and_invalid_selections_do_not_produce_spurious_rectangles() {
    let output = layout(&document(vec![paragraph("p", "e\u{301} 👩🏽‍💻")]));
    let selected = output
        .selection_geometry(&position("p", 2), &position("p", 2))
        .unwrap();
    assert!(selected.rectangles.is_empty());
    assert!(
        output
            .selection_geometry(&position("p", 0), &position("p", 1))
            .is_err()
    );
    assert!(
        output
            .selection_geometry(&position("p", 0), &position("other", 0))
            .is_err()
    );
}

#[test]
fn position_transport_uses_explicit_utf16_affinity_and_rejects_unknown_fields() {
    let decoded: LayoutPosition =
        serde_json::from_str(r#"{"paragraphId":"p","utf16":2,"affinity":"leading"}"#).unwrap();
    assert_eq!(decoded, position("p", 2));
    for invalid in [
        r#"{"paragraphId":"p","utf16":2,"affinity":"leading","byte":2}"#,
        r#"{"paragraphId":"p","utf16":-1,"affinity":"leading"}"#,
        r#"{"paragraphId":"p","utf16":2,"affinity":"unknown"}"#,
    ] {
        assert!(serde_json::from_str::<LayoutPosition>(invalid).is_err());
    }
}
