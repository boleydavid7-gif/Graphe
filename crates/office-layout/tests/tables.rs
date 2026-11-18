//! Rust-owned rectangular table geometry and source-position regressions.

use office_core::{
    Block, CharacterFormat, Document, DocumentMetadata, Id, PageMargins, PageSize, Paragraph,
    Section, Table, TableCell, TableRow, TextCoordinates, TextRun, Twips, deserialize_document,
    serialize_document,
};
use office_layout::{
    CaretAffinity, LayoutEngine, LayoutError, LayoutOptions, LayoutPosition, NavigationDirection,
    UNITS_PER_POINT,
};

fn id(value: impl Into<String>) -> Id {
    Id::new(value).unwrap()
}

fn table(rows: usize, columns: usize, text: &str) -> Table {
    Table::new(
        id("table"),
        (0..rows)
            .map(|row| {
                TableRow::new(
                    id(format!("row-{row}")),
                    (0..columns)
                        .map(|column| {
                            TableCell::new(
                                id(format!("cell-{row}-{column}")),
                                vec![Paragraph::new(
                                    id(format!("paragraph-{row}-{column}")),
                                    vec![TextRun::new(text)],
                                )],
                            )
                        })
                        .collect(),
                )
            })
            .collect(),
    )
}

fn document(table: Table) -> Document {
    Document::from_parts(
        id("document"),
        DocumentMetadata::default(),
        vec![Section::new(id("section"), vec![Block::Table(table)])],
        vec![],
    )
    .unwrap()
}

fn endpoint(row: usize, column: usize, utf16: usize) -> LayoutPosition {
    LayoutPosition {
        paragraph_id: format!("paragraph-{row}-{column}"),
        utf16,
        affinity: CaretAffinity::Leading,
    }
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

#[test]
fn equal_width_table_borders_wrap_and_contain_rust_shaped_lines() {
    let source = document(table(
        2,
        2,
        "Words wrapping in a real cell with useful content. ",
    ));
    let output = LayoutEngine::new()
        .unwrap()
        .layout_document(&source, &small_pages())
        .unwrap();
    let page = &output.pages[0];
    assert_eq!(page.table_cells.len(), 4);
    let first = &page.table_cells[0];
    let second = &page.table_cells[1];
    assert_eq!(first.rect.x, page.content_rect.x);
    assert_eq!(
        first.rect.width + second.rect.width,
        page.content_rect.width
    );
    assert_eq!(first.rect.x + first.rect.width, second.rect.x);
    assert_eq!(first.rect.y, second.rect.y);
    assert_eq!(first.rect.height, second.rect.height);
    assert_eq!(page.table_cells[2].rect.y, first.rect.y + first.rect.height);
    for cell in &page.table_cells {
        let lines: Vec<_> = page
            .lines
            .iter()
            .filter(|line| cell.paragraph_ids.contains(&line.paragraph_id))
            .collect();
        assert!(
            lines.len() > 1,
            "cell should wrap through the existing shaper"
        );
        for line in lines {
            assert!(line.x >= cell.rect.x + 4 * UNITS_PER_POINT);
            assert!(line.y >= cell.rect.y + 4 * UNITS_PER_POINT);
            assert!(line.y + line.height <= cell.rect.y + cell.rect.height);
        }
    }
}

#[test]
fn empty_cell_hit_testing_uses_cell_identity_even_below_its_text() {
    let source = document(table(2, 3, ""));
    let output = LayoutEngine::new()
        .unwrap()
        .layout_document(&source, &LayoutOptions::default())
        .unwrap();
    let page = &output.pages[0];
    for cell in &page.table_cells {
        let hit = output
            .hit_test(
                page.number,
                cell.rect.x + cell.rect.width / 2,
                cell.rect.y + cell.rect.height - 1,
            )
            .unwrap()
            .unwrap();
        assert_eq!(hit.position.paragraph_id, cell.paragraph_ids[0]);
        assert_eq!(hit.position.utf16, 0);
        let geometry = output.caret_geometry(&hit.position.endpoint()).unwrap();
        assert_eq!(geometry.page_number, page.number);
    }
}

#[test]
fn table_unicode_mixed_runs_retain_exact_core_grapheme_boundaries() {
    let text = "Ae\u{301}👩🏽‍💻 العربية नमस्ते שלום";
    let paragraph = Paragraph::new(
        id("paragraph-0-0"),
        vec![
            TextRun::new("Ae"),
            TextRun {
                text: text[2..].into(),
                format: CharacterFormat {
                    bold: Some(true),
                    ..Default::default()
                },
            },
        ],
    );
    let source = document(Table::new(
        id("table"),
        vec![TableRow::new(
            id("row-0"),
            vec![TableCell::new(id("cell-0-0"), vec![paragraph.clone()])],
        )],
    ));
    let output = LayoutEngine::new()
        .unwrap()
        .layout_document(&source, &LayoutOptions::default())
        .unwrap();
    assert!(
        !output
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.kind == "missing_glyph")
    );
    let coordinates = TextCoordinates::new(&paragraph);
    for grapheme in 0..=coordinates.grapheme_len().get() {
        let utf16 = coordinates
            .grapheme_to_utf16(office_core::GraphemeOffset::new(grapheme))
            .unwrap()
            .get();
        let caret = output.caret_geometry(&endpoint(0, 0, utf16)).unwrap();
        assert_eq!(caret.position.grapheme, grapheme);
    }
    assert!(
        output.caret_geometry(&endpoint(0, 0, 2)).is_err(),
        "combining sequence interior is not a cursor"
    );
}

#[test]
fn explicit_twip_widths_are_preserved_and_invalid_geometry_is_not_scaled() {
    let source =
        document(table(1, 2, "content").with_column_widths(vec![Twips(1500), Twips(3000)]));
    let mut engine = LayoutEngine::new().unwrap();
    let output = engine.layout_document(&source, &small_pages()).unwrap();
    assert_eq!(
        output.pages[0].table_cells[0].rect.width,
        75 * UNITS_PER_POINT
    );
    assert_eq!(
        output.pages[0].table_cells[1].rect.width,
        150 * UNITS_PER_POINT
    );
    for widths in [vec![Twips(4000), Twips(4000)], vec![Twips(80), Twips(80)]] {
        let invalid = document(table(1, 2, "preserved").with_column_widths(widths));
        let before = invalid.to_json().unwrap();
        assert!(matches!(
            engine.layout_document(&invalid, &small_pages()),
            Err(LayoutError::InvalidOptions { .. })
        ));
        assert_eq!(invalid.to_json().unwrap(), before);
    }
}

#[test]
fn table_rows_paginate_intact_and_selection_spans_pages_in_source_order() {
    let source = document(table(35, 2, "A row of content."));
    let output = LayoutEngine::new()
        .unwrap()
        .layout_document(&source, &small_pages())
        .unwrap();
    assert!(output.pages.len() > 2);
    assert_eq!(
        output
            .pages
            .iter()
            .map(|page| page.table_cells.len())
            .sum::<usize>(),
        70
    );
    for page in &output.pages {
        assert_eq!(page.table_cells.len() % 2, 0);
        for pair in page.table_cells.as_chunks::<2>().0 {
            assert_eq!(pair[0].row_id, pair[1].row_id);
            assert_eq!(pair[0].rect.y, pair[1].rect.y);
            assert!(pair[0].rect.y >= page.content_rect.y);
            assert!(
                pair[0].rect.y + pair[0].rect.height
                    <= page.content_rect.y + page.content_rect.height
            );
        }
    }
    let range = output
        .selection_geometry(&endpoint(0, 0, 0), &endpoint(34, 1, 1))
        .unwrap();
    assert!(range.focus.page_number > range.anchor.page_number);
    assert!(
        range
            .rectangles
            .iter()
            .any(|rect| rect.page_number == range.focus.page_number)
    );
}

#[test]
fn vertical_navigation_keeps_column_while_horizontal_navigation_remains_logical() {
    let source = document(table(2, 2, "abc"));
    let output = LayoutEngine::new()
        .unwrap()
        .layout_document(&source, &LayoutOptions::default())
        .unwrap();
    for column in 0..2 {
        let next = output
            .move_caret(&endpoint(0, column, 1), NavigationDirection::Down, None)
            .unwrap();
        assert_eq!(
            next.caret.position.paragraph_id,
            format!("paragraph-1-{column}")
        );
        let previous = output
            .move_caret(
                &next.caret.position.endpoint(),
                NavigationDirection::Up,
                next.preferred_x,
            )
            .unwrap();
        assert_eq!(
            previous.caret.position.paragraph_id,
            format!("paragraph-0-{column}")
        );
    }
    let right = output
        .move_caret(&endpoint(0, 0, 3), NavigationDirection::Right, None)
        .unwrap();
    assert_eq!(right.caret.position.paragraph_id, "paragraph-0-1");
}

#[test]
fn table_native_save_reopen_reproduces_identical_geometry_and_identifiers() {
    let source = document(table(4, 3, "Cafe\u{301} 👩‍💻 and several words to wrap."));
    let bytes = serialize_document(&source).unwrap();
    let reopened = deserialize_document(&bytes).unwrap();
    let mut engine = LayoutEngine::new().unwrap();
    assert_eq!(
        engine.layout_document(&source, &small_pages()).unwrap(),
        engine.layout_document(&reopened, &small_pages()).unwrap()
    );
    assert_eq!(serialize_document(&reopened).unwrap(), bytes);
}

#[test]
fn oversized_rows_fail_explicitly_and_leave_document_unchanged() {
    let source = document(table(
        1,
        2,
        &"A long table cell that cannot fit on one page. ".repeat(200),
    ));
    let before = source.to_json().unwrap();
    let error = LayoutEngine::new()
        .unwrap()
        .layout_document(&source, &small_pages())
        .unwrap_err();
    assert!(matches!(error, LayoutError::Unsupported { .. }));
    assert!(error.to_string().contains("table row is taller"));
    assert_eq!(source.to_json().unwrap(), before);
}

#[test]
fn unequal_cell_content_sets_one_row_height_and_body_flow_resumes_after_it() {
    let source = Document::from_parts(id("document"), DocumentMetadata::default(),
        vec![Section::new(id("section"), vec![
            Block::Paragraph(Paragraph::new(id("before"), vec![TextRun::new("Before table")])),
            Block::Table(Table::new(id("table"), vec![TableRow::new(id("row"), vec![
                TableCell::new(id("left"), vec![
                    Paragraph::new(id("first-left"), vec![TextRun::new("A short paragraph")]),
                    Paragraph::new(id("second-left"), vec![TextRun::new("A longer paragraph that wraps over several physical lines. ".repeat(3))]),
                ]),
                TableCell::new(id("right"), vec![Paragraph::new(id("empty"), vec![])]),
            ])])),
            Block::Paragraph(Paragraph::new(id("after"), vec![TextRun::new("After table")])),
        ])], vec![]).unwrap();
    let output = LayoutEngine::new()
        .unwrap()
        .layout_document(&source, &LayoutOptions::default())
        .unwrap();
    let page = &output.pages[0];
    let left = &page.table_cells[0];
    let right = &page.table_cells[1];
    assert_eq!(left.rect.height, right.rect.height);
    let before = page
        .lines
        .iter()
        .find(|line| line.paragraph_id == "before")
        .unwrap();
    let after = page
        .lines
        .iter()
        .find(|line| line.paragraph_id == "after")
        .unwrap();
    assert!(before.y + before.height < left.rect.y);
    assert!(after.y >= left.rect.y + left.rect.height);
    let first = page
        .lines
        .iter()
        .find(|line| line.paragraph_id == "first-left")
        .unwrap();
    let second = page
        .lines
        .iter()
        .find(|line| line.paragraph_id == "second-left")
        .unwrap();
    assert!(second.y > first.y + first.height);
    let hit = output
        .hit_test(1, right.rect.x + 10, right.rect.y + right.rect.height - 1)
        .unwrap()
        .unwrap();
    assert_eq!(hit.position.paragraph_id, "empty");
}

#[test]
fn vertical_navigation_stays_in_a_wrapped_cell_before_moving_to_next_row() {
    let source = document(table(
        2,
        2,
        "Text in a cell that wraps to several lines before the next row. "
            .repeat(2)
            .as_str(),
    ));
    let output = LayoutEngine::new()
        .unwrap()
        .layout_document(&source, &small_pages())
        .unwrap();
    let first = output
        .move_caret(&endpoint(0, 1, 0), NavigationDirection::Down, None)
        .unwrap();
    assert_eq!(first.caret.position.paragraph_id, "paragraph-0-1");
    assert!(first.caret.line_index > 0);
    let back = output
        .move_caret(
            &first.caret.position.endpoint(),
            NavigationDirection::Up,
            first.preferred_x,
        )
        .unwrap();
    assert_eq!(back.caret.position.paragraph_id, "paragraph-0-1");
    assert_eq!(back.caret.line_index, 0);
}
