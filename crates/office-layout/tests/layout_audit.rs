//! Independent layout integrity, coordinate and resource-boundary regressions.

use std::collections::{BTreeMap, BTreeSet};

use office_core::{
    Block, CharacterFormat, Document, DocumentMetadata, GraphemeOffset, HalfPoints, Id,
    Indentation, LineSpacing, NamedStyle, OriginalSource, PageMargins, PageSize, Paragraph,
    ParagraphFormat, Section, SectionProperties, TextCoordinates, TextRun, Twips, Underline,
    WriterFile, deserialize_writer_file, serialize_writer_file,
};
use office_layout::{CaretAffinity, DocumentLayout, LayoutEngine, LayoutError, LayoutOptions};

fn id(value: &str) -> Id {
    Id::new(value).unwrap()
}

fn document(paragraphs: Vec<Paragraph>) -> Document {
    Document::from_parts(
        id("audit-document"),
        DocumentMetadata::default(),
        vec![Section::new(
            id("audit-section"),
            paragraphs.into_iter().map(Block::Paragraph).collect(),
        )],
        vec![],
    )
    .unwrap()
}

fn paragraph(text: &str) -> Paragraph {
    Paragraph::new(id("audit-paragraph"), vec![TextRun::new(text)])
}

fn layout(document: &Document) -> DocumentLayout {
    LayoutEngine::new()
        .unwrap()
        .layout_document(document, &LayoutOptions::default())
        .unwrap()
}

#[test]
fn glyph_cache_distinguishes_variations_and_faces_but_reuses_font_unit_outlines() {
    let regular = CharacterFormat {
        font_family: Some("Arimo".into()),
        font_size: Some(HalfPoints(24)),
        ..Default::default()
    };
    let mut bold = regular.clone();
    bold.bold = Some(true);
    let mut larger = regular.clone();
    larger.font_size = Some(HalfPoints(48));
    let mut italic = regular.clone();
    italic.italic = Some(true);
    let runs = [regular.clone(), bold.clone(), larger, bold, italic, regular]
        .into_iter()
        .map(|format| TextRun {
            text: "A".into(),
            format,
        })
        .collect();
    let output = layout(&document(vec![Paragraph::new(id("letters"), runs)]));
    let glyphs = &output.pages[0].lines[0].glyphs;
    assert_eq!(glyphs.len(), 6);
    assert_eq!(glyphs[0].outline_id, glyphs[2].outline_id);
    assert_eq!(glyphs[0].outline_id, glyphs[5].outline_id);
    assert_eq!(glyphs[1].outline_id, glyphs[3].outline_id);
    assert_ne!(glyphs[0].outline_id, glyphs[1].outline_id);
    assert_ne!(glyphs[0].font_size, glyphs[2].font_size);
    assert_ne!(glyphs[0].transform, glyphs[2].transform);
    let ordinary = &output.outlines[glyphs[0].outline_id as usize];
    let bold = &output.outlines[glyphs[1].outline_id as usize];
    assert_eq!(ordinary.font_id, bold.font_id);
    assert_eq!(ordinary.glyph_id, bold.glyph_id);
    assert_ne!(ordinary.path, bold.path);
    assert_eq!(
        output.outlines[glyphs[4].outline_id as usize].font_id,
        "Arimo-Italic.ttf"
    );
}

#[test]
fn soft_breaks_and_cross_run_graphemes_keep_exact_source_and_complete_cursor_boundaries() {
    let paragraph = Paragraph::new(
        id("unicode-paragraph"),
        vec![
            TextRun::new("e"),
            TextRun::new("\u{301} 👩🏽"),
            TextRun::new("\u{200d}💻 \r\nשלום\n"),
        ],
    );
    let coordinates = TextCoordinates::new(&paragraph);
    let output = layout(&document(vec![paragraph]));
    let lines: Vec<_> = output.pages.iter().flat_map(|page| &page.lines).collect();
    assert_eq!(lines.len(), 3);
    assert_eq!(lines.last().unwrap().text, "");
    assert_eq!(
        lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<String>(),
        coordinates.text()
    );
    let mut observed = BTreeSet::new();
    for line in lines {
        assert_eq!(line.paragraph_id, "unicode-paragraph");
        for caret in &line.carets {
            let offset = GraphemeOffset::new(caret.grapheme);
            assert_eq!(
                coordinates.grapheme_to_utf8(offset).unwrap().get(),
                caret.utf8
            );
            assert_eq!(
                coordinates.grapheme_to_utf16(offset).unwrap().get(),
                caret.utf16
            );
            observed.insert(caret.grapheme);
        }
    }
    assert_eq!(observed, (0..=coordinates.grapheme_len().get()).collect());
}

#[test]
fn mixed_bidi_keeps_logical_boundaries_and_distinct_visual_affinities() {
    let source = paragraph("abc שלום 123 العربية xyz");
    let coordinates = TextCoordinates::new(&source);
    let output = layout(&document(vec![source]));
    let mut visual = BTreeMap::<usize, BTreeSet<i64>>::new();
    let mut observed = BTreeSet::new();
    for caret in output
        .pages
        .iter()
        .flat_map(|page| &page.lines)
        .flat_map(|line| &line.carets)
    {
        assert_eq!(
            coordinates
                .grapheme_to_utf8(GraphemeOffset::new(caret.grapheme))
                .unwrap()
                .get(),
            caret.utf8
        );
        visual.entry(caret.utf8).or_default().insert(caret.x);
        observed.insert(caret.grapheme);
    }
    assert_eq!(observed, (0..=coordinates.grapheme_len().get()).collect());
    assert!(visual.values().any(|positions| positions.len() > 1));
}

#[test]
fn leading_affinity_after_a_mandatory_break_is_on_the_following_line() {
    let output = layout(&document(vec![paragraph("a\nb")]));
    let lines = &output.pages[0].lines;
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].text, "a\n");
    assert_eq!(lines[1].text, "b");
    let after_break = lines[1]
        .carets
        .iter()
        .find(|caret| caret.utf8 == 2 && caret.affinity == CaretAffinity::Leading)
        .expect("the insertion side after a mandatory break is on the next line");
    assert_eq!(after_break.grapheme, 2);
    assert_eq!(after_break.utf16, 2);
    assert_eq!(after_break.y, lines[1].y);
    assert!(after_break.y > lines[0].y);
    // Neighbor-edge candidates can also describe the preceding break's edge.
    // Step 7 must resolve those affinities before treating them as hit-test or
    // keyboard insertion destinations; this read-only API does neither.
    for line in lines {
        assert!(line.carets.iter().all(|caret| caret.utf8 <= 3));
    }
}

#[test]
fn negative_first_line_indentation_extends_into_margin_without_clamping() {
    let source = paragraph(&"wrapping word ".repeat(25)).with_format(ParagraphFormat {
        indentation: Some(Indentation {
            left: Some(Twips(720)),
            right: Some(Twips(0)),
            first_line: Some(Twips(-1440)),
        }),
        ..Default::default()
    });
    let output = layout(&document(vec![source]));
    let page = &output.pages[0];
    assert!(page.lines.len() >= 2);
    assert_eq!(page.lines[0].x, 36 * 64);
    assert_eq!(page.lines[1].x, 108 * 64);
    assert!(page.lines[0].x < page.content_rect.x);
}

#[test]
fn paragraph_continuation_preserves_indexes_across_pages_and_sections() {
    let text = (0..20)
        .map(|index| format!("line {index}"))
        .collect::<Vec<_>>()
        .join("\n");
    let first = paragraph(&text).with_format(ParagraphFormat {
        line_spacing: Some(LineSpacing::Exact { height: Twips(360) }),
        ..Default::default()
    });
    let margins = PageMargins {
        top: Twips(200),
        right: Twips(200),
        bottom: Twips(200),
        left: Twips(200),
        header: Twips(0),
        footer: Twips(0),
        gutter: Twips(0),
    };
    let source = Document::from_parts(
        id("pages"),
        DocumentMetadata::default(),
        vec![
            Section::new(id("small-pages"), vec![Block::Paragraph(first)]).with_properties(
                SectionProperties {
                    page_size: Some(PageSize {
                        width: Twips(6000),
                        height: Twips(2000),
                        orientation: None,
                    }),
                    page_margins: Some(margins),
                },
            ),
            Section::new(
                id("landscape"),
                vec![Block::Paragraph(Paragraph::new(
                    id("last"),
                    vec![TextRun::new("after")],
                ))],
            )
            .with_properties(SectionProperties {
                page_size: Some(PageSize {
                    width: Twips(16838),
                    height: Twips(11906),
                    orientation: None,
                }),
                page_margins: Some(margins),
            }),
        ],
        vec![],
    )
    .unwrap();
    let output = layout(&source);
    assert_eq!(output.pages.len(), 6);
    assert!(
        output.pages[..5]
            .iter()
            .all(|page| page.section_id == "small-pages")
    );
    assert_eq!(output.pages[5].section_id, "landscape");
    assert_eq!(output.pages[5].width, 53882);
    assert_eq!(output.pages[5].height, 38099);
    let first_lines: Vec<_> = output.pages[..5]
        .iter()
        .flat_map(|page| &page.lines)
        .collect();
    assert_eq!(
        first_lines
            .iter()
            .map(|line| line.line_index)
            .collect::<Vec<_>>(),
        (0..20).collect::<Vec<_>>()
    );
    assert_eq!(
        first_lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<String>(),
        text
    );
    for page in &output.pages {
        for line in &page.lines {
            assert!(line.y >= page.content_rect.y);
            assert!(line.y + line.height <= page.content_rect.y + page.content_rect.height);
        }
    }
}

#[test]
fn paragraph_gap_uses_maximum_and_minimum_line_height_can_grow() {
    let exact = LineSpacing::Exact { height: Twips(400) };
    let first = paragraph("first").with_format(ParagraphFormat {
        spacing_after: Some(Twips(240)),
        line_spacing: Some(exact),
        ..Default::default()
    });
    let second =
        Paragraph::new(id("second"), vec![TextRun::new("second")]).with_format(ParagraphFormat {
            spacing_before: Some(Twips(80)),
            line_spacing: Some(exact),
            ..Default::default()
        });
    let output = layout(&document(vec![first, second]));
    let lines = &output.pages[0].lines;
    assert_eq!(lines[1].y - lines[0].y - lines[0].height, 12 * 64);

    let output = layout(&document(
        [
            LineSpacing::Multiple { value: 240 },
            LineSpacing::AtLeast { height: Twips(20) },
            LineSpacing::Exact { height: Twips(20) },
        ]
        .into_iter()
        .enumerate()
        .map(|(index, spacing)| {
            Paragraph::new(id(&format!("height-{index}")), vec![TextRun::new("X")]).with_format(
                ParagraphFormat {
                    line_spacing: Some(spacing),
                    ..Default::default()
                },
            )
        })
        .collect(),
    ));
    let lines = &output.pages[0].lines;
    assert_eq!(lines[0].height, lines[1].height);
    assert!(lines[0].height > 64);
    assert_eq!(lines[2].height, 64);
}

#[test]
fn unsupported_controls_and_valid_but_excessive_input_return_structured_errors() {
    let mut engine = LayoutEngine::new().unwrap();
    for text in ["a\tb", "a\0b"] {
        assert!(matches!(
            engine.layout_document(&document(vec![paragraph(text)]), &LayoutOptions::default()),
            Err(LayoutError::Unsupported { .. })
        ));
    }
    let source = document(vec![paragraph("valid text").with_format(ParagraphFormat {
        spacing_before: Some(Twips(i32::MAX)),
        ..Default::default()
    })]);
    source.validate().unwrap();
    assert!(matches!(
        engine.layout_document(&source, &LayoutOptions::default()),
        Err(LayoutError::LimitExceeded {
            resource: "absolute_twips",
            ..
        })
    ));
    let source = Document::from_parts(
        id("many-styles"),
        DocumentMetadata::default(),
        vec![],
        (0..129)
            .map(|index| NamedStyle::new(id(&format!("style-{index}")), format!("Style {index}")))
            .collect(),
    )
    .unwrap();
    assert!(matches!(
        engine.layout_document(&source, &LayoutOptions::default()),
        Err(LayoutError::LimitExceeded {
            resource: "styles",
            ..
        })
    ));
    let source = document(vec![Paragraph::new(
        id(&"p".repeat(1025)),
        vec![TextRun::new("tiny")],
    )]);
    assert!(matches!(
        engine.layout_document(&source, &LayoutOptions::default()),
        Err(LayoutError::LimitExceeded {
            resource: "paragraph_id_bytes",
            ..
        })
    ));
    let options = LayoutOptions {
        font_size: HalfPoints(0),
        ..Default::default()
    };
    assert!(matches!(
        engine.layout_document(&document(vec![]), &options),
        Err(LayoutError::InvalidOptions { .. })
    ));
}

#[test]
fn native_source_round_trip_reproduces_identical_layout_and_original_bytes() {
    let source = OriginalSource::new(
        "example",
        "application/octet-stream",
        "source.bin",
        b"immutable original".as_slice(),
    )
    .unwrap();
    let file =
        WriterFile::with_original_source(document(vec![paragraph("saved café 👩🏽‍💻 שלום")]), source)
            .unwrap();
    let bytes = serialize_writer_file(&file).unwrap();
    let restored = deserialize_writer_file(&bytes).unwrap();
    let mut engine = LayoutEngine::new().unwrap();
    assert_eq!(
        engine
            .layout_document(file.document(), &LayoutOptions::default())
            .unwrap(),
        engine
            .layout_document(restored.document(), &LayoutOptions::default())
            .unwrap()
    );
    assert_eq!(serialize_writer_file(&restored).unwrap(), bytes);
    assert_eq!(
        restored.original_source().unwrap().bytes(),
        b"immutable original"
    );
}

#[test]
fn pinned_font_underlines_are_below_the_baseline_in_downward_positive_page_coordinates() {
    let paragraphs = ["Arimo", "Tinos"]
        .into_iter()
        .enumerate()
        .map(|(index, family)| {
            Paragraph::new(
                id(&format!("underlined-{index}")),
                vec![TextRun {
                    text: "Underline".into(),
                    format: CharacterFormat {
                        font_family: Some(family.into()),
                        underline: Some(Underline::Double),
                        ..Default::default()
                    },
                }],
            )
        })
        .collect();
    let output = layout(&document(paragraphs));
    for line in &output.pages[0].lines {
        assert_eq!(line.decorations.len(), 2);
        assert!(
            line.decorations
                .iter()
                .all(|decoration| decoration.y > line.baseline)
        );
        assert!(line.decorations[1].y > line.decorations[0].y);
    }
}
