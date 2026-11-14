//! Logical positions and strict Unicode coordinate conversion contracts.

use std::cmp::Ordering;

use office_core::{
    Block, CharacterFormat, Document, DocumentMetadata, GraphemeOffset, Id, NamedStyle, OffsetUnit,
    Paragraph, PositionError, Section, TextCoordinates, TextRun, Utf8Offset, Utf16Offset,
};

fn id(value: &str) -> Id {
    Id::new(value).unwrap()
}

fn paragraph(value: &str, text: &str) -> Paragraph {
    Paragraph::new(id(value), vec![TextRun::new(text)])
}

fn coordinates(text: &str) -> TextCoordinates {
    TextCoordinates::new(&paragraph("paragraph", text))
}

fn document(document_id: &str, sections: Vec<Section>) -> Document {
    Document::from_parts(
        id(document_id),
        DocumentMetadata::default(),
        sections,
        vec![],
    )
    .unwrap()
}

fn one_paragraph(document_id: &str, text: &str) -> Document {
    document(
        document_id,
        vec![Section::new(
            id("section"),
            vec![Block::Paragraph(paragraph("paragraph", text))],
        )],
    )
}

#[test]
fn explicit_offsets_keep_ascii_bmp_and_supplementary_units_distinct() {
    let map = coordinates("aé中😀");
    let bytes = [0, 1, 3, 6, 10];
    let units = [0, 1, 2, 3, 5];
    assert_eq!(map.text(), "aé中😀");
    assert_eq!(map.utf8_len(), Utf8Offset::new(10));
    assert_eq!(map.utf16_len(), Utf16Offset::new(5));
    assert_eq!(map.grapheme_len(), GraphemeOffset::new(4));
    for (index, (byte, unit)) in bytes.into_iter().zip(units).enumerate() {
        let grapheme = GraphemeOffset::new(index);
        assert_eq!(map.grapheme_to_utf8(grapheme), Ok(Utf8Offset::new(byte)));
        assert_eq!(map.grapheme_to_utf16(grapheme), Ok(Utf16Offset::new(unit)));
        assert_eq!(
            map.utf8_to_utf16(Utf8Offset::new(byte)),
            Ok(Utf16Offset::new(unit))
        );
        assert_eq!(
            map.utf16_to_utf8(Utf16Offset::new(unit)),
            Ok(Utf8Offset::new(byte))
        );
        assert_eq!(map.utf8_to_grapheme(Utf8Offset::new(byte)), Ok(grapheme));
        assert_eq!(map.utf16_to_grapheme(Utf16Offset::new(unit)), Ok(grapheme));
    }
}

#[test]
fn scalar_conversion_allows_combining_boundaries_but_cursor_conversion_rejects_them() {
    let map = coordinates("e\u{301}");
    assert_eq!(map.grapheme_len(), GraphemeOffset::new(1));
    assert_eq!(
        map.utf8_to_utf16(Utf8Offset::new(1)),
        Ok(Utf16Offset::new(1))
    );
    assert_eq!(
        map.utf16_to_utf8(Utf16Offset::new(1)),
        Ok(Utf8Offset::new(1))
    );
    assert_eq!(
        map.utf8_to_grapheme(Utf8Offset::new(1)),
        Err(PositionError::InvalidGraphemeBoundary {
            offset: Utf8Offset::new(1)
        })
    );
    assert_eq!(
        map.utf16_to_grapheme(Utf16Offset::new(1)),
        Err(PositionError::InvalidGraphemeBoundary {
            offset: Utf8Offset::new(1)
        })
    );
    assert_eq!(
        map.grapheme_to_utf8(GraphemeOffset::new(1)),
        Ok(Utf8Offset::new(3))
    );
    assert_eq!(
        map.grapheme_to_utf16(GraphemeOffset::new(1)),
        Ok(Utf16Offset::new(2))
    );
}

#[test]
fn interior_utf8_bytes_and_utf16_surrogates_are_never_snapped() {
    let map = coordinates("aé中😀");
    for byte in [2, 4, 5, 7, 8, 9] {
        let error = Err(PositionError::InvalidUtf8Boundary {
            offset: Utf8Offset::new(byte),
        });
        assert_eq!(map.utf8_to_utf16(Utf8Offset::new(byte)), error);
        assert_eq!(
            map.utf8_to_grapheme(Utf8Offset::new(byte)),
            Err(PositionError::InvalidUtf8Boundary {
                offset: Utf8Offset::new(byte)
            })
        );
    }
    assert_eq!(
        map.utf16_to_utf8(Utf16Offset::new(4)),
        Err(PositionError::InvalidUtf16Boundary {
            offset: Utf16Offset::new(4)
        })
    );
    assert_eq!(
        map.utf16_to_grapheme(Utf16Offset::new(4)),
        Err(PositionError::InvalidUtf16Boundary {
            offset: Utf16Offset::new(4)
        })
    );
}

#[test]
fn offsets_beyond_end_including_usize_max_return_structured_bounds_errors() {
    let map = coordinates("😀");
    for offset in [5, usize::MAX] {
        let error = PositionError::OutOfBounds {
            unit: OffsetUnit::Utf8,
            offset,
            max: 4,
        };
        assert_eq!(
            map.utf8_to_utf16(Utf8Offset::new(offset)),
            Err(error.clone())
        );
        assert_eq!(map.utf8_to_grapheme(Utf8Offset::new(offset)), Err(error));
    }
    for offset in [3, usize::MAX] {
        let error = PositionError::OutOfBounds {
            unit: OffsetUnit::Utf16,
            offset,
            max: 2,
        };
        assert_eq!(
            map.utf16_to_utf8(Utf16Offset::new(offset)),
            Err(error.clone())
        );
        assert_eq!(map.utf16_to_grapheme(Utf16Offset::new(offset)), Err(error));
    }
    for offset in [2, usize::MAX] {
        let error = PositionError::OutOfBounds {
            unit: OffsetUnit::Grapheme,
            offset,
            max: 1,
        };
        assert_eq!(
            map.grapheme_to_utf8(GraphemeOffset::new(offset)),
            Err(error.clone())
        );
        assert_eq!(
            map.grapheme_to_utf16(GraphemeOffset::new(offset)),
            Err(error)
        );
    }
}

#[test]
fn complex_clusters_have_hard_coded_boundaries_across_formatted_runs() {
    let cases: &[(&str, &[usize])] = &[
        ("", &[0]),
        ("e\u{301}", &[0, 3]),
        ("👩🏽‍💻", &[0, 15]),
        ("👨‍👩‍👧‍👦", &[0, 25]),
        ("🇺🇸🇨🇦🇯", &[0, 8, 16, 20]),
        ("1\u{fe0f}\u{20e3}", &[0, 7]),
        ("✈\u{fe0f}", &[0, 6]),
        ("\r\n", &[0, 2]),
        ("\u{1100}\u{1161}\u{11a8}", &[0, 9]),
        ("क्ष", &[0, 9]),
        ("שָׁ", &[0, 6]),
        ("\0", &[0, 1]),
        ("\u{301}\u{308}a", &[0, 4, 5]),
        ("🏳️‍🌈", &[0, 14]),
        ("\ra\nb", &[0, 1, 2, 3, 4]),
    ];
    for &(text, expected) in cases {
        let mut runs = vec![TextRun::new("")];
        for (index, scalar) in text.chars().enumerate() {
            runs.push(TextRun {
                text: scalar.to_string(),
                format: CharacterFormat {
                    bold: Some(index % 2 == 0),
                    ..Default::default()
                },
            });
            runs.push(TextRun::new(""));
        }
        let paragraph = Paragraph::new(id("paragraph"), runs);
        let map = TextCoordinates::new(&paragraph);
        assert_eq!(map.text().as_bytes(), text.as_bytes());
        assert_eq!(map.grapheme_len().get() + 1, expected.len(), "{text:?}");
        for (index, &byte) in expected.iter().enumerate() {
            assert_eq!(
                map.grapheme_to_utf8(GraphemeOffset::new(index)),
                Ok(Utf8Offset::new(byte)),
                "{text:?} boundary {index}"
            );
            assert_eq!(
                map.utf8_to_grapheme(Utf8Offset::new(byte)),
                Ok(GraphemeOffset::new(index))
            );
        }
        for byte in 0..=text.len() {
            let offset = Utf8Offset::new(byte);
            if !text.is_char_boundary(byte) {
                assert_eq!(
                    map.utf8_to_grapheme(offset),
                    Err(PositionError::InvalidUtf8Boundary { offset })
                );
            } else if !expected.contains(&byte) {
                assert_eq!(
                    map.utf8_to_grapheme(offset),
                    Err(PositionError::InvalidGraphemeBoundary { offset })
                );
            }
        }
        let mut scalar_boundaries = vec![(0, 0)];
        let (mut byte, mut unit) = (0, 0);
        for scalar in text.chars() {
            byte += scalar.len_utf8();
            unit += scalar.len_utf16();
            scalar_boundaries.push((byte, unit));
        }
        for unit in 0..=text.encode_utf16().count() {
            let offset = Utf16Offset::new(unit);
            let expected_conversion = match scalar_boundaries
                .iter()
                .find(|&&(_, scalar_unit)| scalar_unit == unit)
            {
                Some(&(byte, _)) => match expected.iter().position(|&boundary| boundary == byte) {
                    Some(index) => Ok(GraphemeOffset::new(index)),
                    None => Err(PositionError::InvalidGraphemeBoundary {
                        offset: Utf8Offset::new(byte),
                    }),
                },
                None => Err(PositionError::InvalidUtf16Boundary { offset }),
            };
            assert_eq!(
                map.utf16_to_grapheme(offset),
                expected_conversion,
                "{text:?}"
            );
        }
        let unsplit = coordinates(text);
        assert_eq!(map.grapheme_len(), unsplit.grapheme_len());
        assert_eq!(paragraph.runs().len(), 1 + 2 * text.chars().count());
    }
}

#[test]
fn every_scalar_boundary_round_trips_and_every_interior_encoding_offset_fails() {
    let text = "aé中😀e\u{301}👩🏽‍💻\r\nक्षالعربية\0";
    let map = coordinates(text);
    let mut scalar_boundaries = vec![(0, 0)];
    let mut bytes = 0;
    let mut utf16 = 0;
    for scalar in text.chars() {
        bytes += scalar.len_utf8();
        utf16 += scalar.len_utf16();
        scalar_boundaries.push((bytes, utf16));
    }
    for byte in 0..=bytes {
        let offset = Utf8Offset::new(byte);
        if let Some(&(_, units)) = scalar_boundaries
            .iter()
            .find(|&&(position, _)| position == byte)
        {
            assert_eq!(map.utf8_to_utf16(offset), Ok(Utf16Offset::new(units)));
            assert_eq!(map.utf16_to_utf8(Utf16Offset::new(units)), Ok(offset));
        } else {
            assert_eq!(
                map.utf8_to_utf16(offset),
                Err(PositionError::InvalidUtf8Boundary { offset })
            );
        }
    }
    for units in 0..=utf16 {
        let offset = Utf16Offset::new(units);
        if let Some(&(byte, _)) = scalar_boundaries
            .iter()
            .find(|&&(_, position)| position == units)
        {
            assert_eq!(map.utf16_to_utf8(offset), Ok(Utf8Offset::new(byte)));
        } else {
            assert_eq!(
                map.utf16_to_utf8(offset),
                Err(PositionError::InvalidUtf16Boundary { offset })
            );
        }
    }
    for index in 0..=map.grapheme_len().get() {
        let grapheme = GraphemeOffset::new(index);
        let utf8 = map.grapheme_to_utf8(grapheme).unwrap();
        let utf16 = map.grapheme_to_utf16(grapheme).unwrap();
        assert_eq!(map.utf8_to_grapheme(utf8), Ok(grapheme));
        assert_eq!(map.utf16_to_grapheme(utf16), Ok(grapheme));
    }
}

#[test]
fn empty_paragraphs_have_one_boundary_while_empty_documents_have_no_cursor_target() {
    let map = TextCoordinates::new(&Paragraph::new(id("paragraph"), vec![]));
    assert_eq!(map.text(), "");
    assert_eq!(map.utf8_len(), Utf8Offset::new(0));
    assert_eq!(map.utf16_len(), Utf16Offset::new(0));
    assert_eq!(map.grapheme_len(), GraphemeOffset::new(0));
    assert_eq!(
        map.grapheme_to_utf8(GraphemeOffset::new(0)),
        Ok(Utf8Offset::new(0))
    );
    assert_eq!(
        map.utf8_to_utf16(Utf8Offset::new(0)),
        Ok(Utf16Offset::new(0))
    );
    assert_eq!(
        map.utf16_to_grapheme(Utf16Offset::new(0)),
        Ok(GraphemeOffset::new(0))
    );
    let empty = Document::new(id("document")).unwrap();
    assert_eq!(
        empty.position(&id("paragraph"), GraphemeOffset::new(0)),
        Err(PositionError::ParagraphNotFound {
            id: id("paragraph")
        })
    );
    let document = one_paragraph("document", "");
    let position = document
        .position(&id("paragraph"), GraphemeOffset::new(0))
        .unwrap();
    assert_eq!(document.validate_position(&position), Ok(()));
    assert_eq!(
        document.position(&id("paragraph"), GraphemeOffset::new(1)),
        Err(PositionError::OutOfBounds {
            unit: OffsetUnit::Grapheme,
            offset: 1,
            max: 0
        })
    );
}

#[test]
fn positions_are_anchored_to_document_and_paragraph_ids() {
    let document = one_paragraph("document", "e\u{301}😀");
    let position = document
        .position(&id("paragraph"), GraphemeOffset::new(2))
        .unwrap();
    assert_eq!(position.document_id(), &id("document"));
    assert_eq!(position.paragraph_id(), &id("paragraph"));
    assert_eq!(position.offset(), GraphemeOffset::new(2));
    assert_eq!(document.validate_position(&position), Ok(()));
    for missing in ["missing", "document", "section"] {
        assert_eq!(
            document.position(&id(missing), GraphemeOffset::new(0)),
            Err(PositionError::ParagraphNotFound { id: id(missing) })
        );
        assert!(matches!(
            document.text_coordinates(&id(missing)),
            Err(PositionError::ParagraphNotFound { .. })
        ));
    }
    assert_eq!(
        document.position(&id("paragraph"), GraphemeOffset::new(usize::MAX)),
        Err(PositionError::OutOfBounds {
            unit: OffsetUnit::Grapheme,
            offset: usize::MAX,
            max: 2
        })
    );
}

#[test]
fn selection_order_follows_logical_structure_not_id_spelling_or_bidi_display() {
    let document = document(
        "document",
        vec![
            Section::new(
                id("z-section"),
                vec![
                    Block::Paragraph(paragraph("z-paragraph", "אב😀")),
                    Block::Paragraph(paragraph("m-paragraph", "")),
                ],
            ),
            Section::new(id("empty-section"), vec![]),
            Section::new(
                id("a-section"),
                vec![Block::Paragraph(paragraph("a-paragraph", "last"))],
            ),
        ],
    );
    let first = document
        .position(&id("z-paragraph"), GraphemeOffset::new(0))
        .unwrap();
    let later_in_first = document
        .position(&id("z-paragraph"), GraphemeOffset::new(2))
        .unwrap();
    let second = document
        .position(&id("m-paragraph"), GraphemeOffset::new(0))
        .unwrap();
    let last = document
        .position(&id("a-paragraph"), GraphemeOffset::new(0))
        .unwrap();
    assert_eq!(
        document.compare_positions(&first, &later_in_first),
        Ok(Ordering::Less)
    );
    assert_eq!(
        document.compare_positions(&later_in_first, &second),
        Ok(Ordering::Less)
    );
    assert_eq!(
        document.compare_positions(&second, &last),
        Ok(Ordering::Less)
    );
    assert_eq!(
        document.compare_positions(&first, &first),
        Ok(Ordering::Equal)
    );
    assert_eq!(
        document.compare_positions(&last, &first),
        Ok(Ordering::Greater)
    );
    let forward = document.selection(first.clone(), last.clone()).unwrap();
    let reverse = document.selection(last.clone(), first.clone()).unwrap();
    assert_eq!(
        forward.ordered_range(&document).unwrap(),
        first.clone()..last.clone()
    );
    assert_eq!(
        reverse.ordered_range(&document).unwrap(),
        first.clone()..last.clone()
    );
    assert_eq!(reverse.anchor(), &last);
    assert_eq!(reverse.focus(), &first);
    assert!(!reverse.is_collapsed());
    let collapsed = document.selection(second.clone(), second.clone()).unwrap();
    assert!(collapsed.is_collapsed());
    assert_eq!(
        collapsed.ordered_range(&document).unwrap(),
        second.clone()..second
    );
}

#[test]
fn paragraph_end_and_next_start_are_distinct_selection_boundaries() {
    let document = document(
        "document",
        vec![Section::new(
            id("section"),
            vec![
                Block::Paragraph(paragraph("first", "x")),
                Block::Paragraph(paragraph("second", "y")),
            ],
        )],
    );
    let end = document
        .position(&id("first"), GraphemeOffset::new(1))
        .unwrap();
    let start = document
        .position(&id("second"), GraphemeOffset::new(0))
        .unwrap();
    assert_ne!(end, start);
    assert_eq!(document.compare_positions(&end, &start), Ok(Ordering::Less));
    let selection = document.selection(end.clone(), start.clone()).unwrap();
    assert!(!selection.is_collapsed());
    assert_eq!(selection.ordered_range(&document).unwrap(), end..start);
}

#[test]
fn foreign_document_positions_are_rejected_at_every_document_boundary() {
    let document = one_paragraph("document", "abc");
    let foreign_document = one_paragraph("foreign", "abc");
    let local = document
        .position(&id("paragraph"), GraphemeOffset::new(0))
        .unwrap();
    let foreign = foreign_document
        .position(&id("paragraph"), GraphemeOffset::new(0))
        .unwrap();
    let error = PositionError::WrongDocument {
        expected: id("document"),
        actual: id("foreign"),
    };
    assert_eq!(document.validate_position(&foreign), Err(error.clone()));
    assert_eq!(
        document.compare_positions(&local, &foreign),
        Err(error.clone())
    );
    assert_eq!(
        document.compare_positions(&foreign, &local),
        Err(error.clone())
    );
    assert_eq!(
        document.selection(local.clone(), foreign.clone()),
        Err(error.clone())
    );
    assert_eq!(
        document.selection(foreign.clone(), local),
        Err(error.clone())
    );
    let foreign_selection = foreign_document
        .selection(foreign.clone(), foreign)
        .unwrap();
    assert_eq!(foreign_selection.ordered_range(&document), Err(error));
}

#[test]
fn revalidation_detects_missing_paragraphs_and_out_of_range_stale_offsets() {
    let original = one_paragraph("document", "abcdef");
    let start = original
        .position(&id("paragraph"), GraphemeOffset::new(0))
        .unwrap();
    let end = original
        .position(&id("paragraph"), GraphemeOffset::new(6))
        .unwrap();
    let selection = original.selection(start.clone(), end.clone()).unwrap();
    let shorter = one_paragraph("document", "x");
    let range_error = PositionError::OutOfBounds {
        unit: OffsetUnit::Grapheme,
        offset: 6,
        max: 1,
    };
    assert_eq!(shorter.validate_position(&end), Err(range_error.clone()));
    assert_eq!(
        shorter.compare_positions(&start, &end),
        Err(range_error.clone())
    );
    assert_eq!(
        shorter.selection(start, end.clone()),
        Err(range_error.clone())
    );
    assert_eq!(selection.ordered_range(&shorter), Err(range_error));
    let missing = Document::new(id("document")).unwrap();
    assert_eq!(
        missing.validate_position(&end),
        Err(PositionError::ParagraphNotFound {
            id: id("paragraph")
        })
    );
    // Same IDs and an in-range offset cannot detect changed meaning without a revision system.
    let changed = one_paragraph("document", "uvwxyz");
    assert_eq!(changed.validate_position(&end), Ok(()));
}

#[test]
fn stable_ids_survive_run_repartitioning_and_order_is_looked_up_again() {
    let original = document(
        "document",
        vec![Section::new(
            id("section"),
            vec![
                Block::Paragraph(paragraph("first", "e\u{301}😀")),
                Block::Paragraph(paragraph("second", "B")),
            ],
        )],
    );
    let first = original
        .position(&id("first"), GraphemeOffset::new(1))
        .unwrap();
    let second = original
        .position(&id("second"), GraphemeOffset::new(0))
        .unwrap();
    let repartitioned = Paragraph::new(
        id("first"),
        vec![
            TextRun::new("e"),
            TextRun::new(""),
            TextRun::new("\u{301}"),
            TextRun::new("😀"),
        ],
    );
    let reordered = document(
        "document",
        vec![Section::new(
            id("new-section"),
            vec![
                Block::Paragraph(paragraph("second", "B")),
                Block::Paragraph(repartitioned),
            ],
        )],
    );
    assert_eq!(reordered.validate_position(&first), Ok(()));
    assert_eq!(
        reordered.position(&id("first"), GraphemeOffset::new(1)),
        Ok(first.clone())
    );
    assert_eq!(
        original.compare_positions(&first, &second),
        Ok(Ordering::Less)
    );
    assert_eq!(
        reordered.compare_positions(&first, &second),
        Ok(Ordering::Greater)
    );
}

#[test]
fn document_style_ids_are_not_paragraph_targets() {
    let document = Document::from_parts(
        id("document"),
        DocumentMetadata::default(),
        vec![],
        vec![NamedStyle::new(id("style"), "Normal")],
    )
    .unwrap();
    assert_eq!(
        document.position(&id("style"), GraphemeOffset::new(0)),
        Err(PositionError::ParagraphNotFound { id: id("style") })
    );
}

#[test]
fn coordinate_queries_preserve_normalization_runs_and_document_schema() {
    let fixture = include_str!("../../../tests/fixtures/writer-sample-v1.json");
    let document = Document::from_json(fixture).unwrap();
    let before = document.to_json().unwrap();
    let map = document.text_coordinates(&id("intro")).unwrap();
    assert!(map.text().contains("e\u{301}"));
    let start = document
        .position(&id("intro"), GraphemeOffset::new(0))
        .unwrap();
    let end = document.position(&id("intro"), map.grapheme_len()).unwrap();
    let selection = document.selection(end, start).unwrap();
    selection.ordered_range(&document).unwrap();
    assert_eq!(document.to_json().unwrap(), before);
    assert_eq!(document.schema_version(), 1);
    let restored = Document::from_json(&before).unwrap();
    assert_eq!(restored, document);
    assert_eq!(
        selection.ordered_range(&restored),
        selection.ordered_range(&document)
    );
    let decomposed = coordinates("e\u{301}");
    let composed = coordinates("é");
    assert_ne!(decomposed.text().as_bytes(), composed.text().as_bytes());
    assert_eq!(decomposed.grapheme_len(), composed.grapheme_len());
}
