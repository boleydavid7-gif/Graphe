//! Deterministic typography, paragraph formatting and pagination contracts.

use office_core::{
    Alignment, Block, CharacterFormat, Document, DocumentMetadata, HalfPoints, Id, Indentation,
    LineSpacing, PageMargins, PageSize, Paragraph, ParagraphFormat, Section, SectionProperties,
    TextColor, TextRun, Twips, Underline,
};
use office_layout::{DocumentLayout, LayoutEngine, LayoutError, LayoutOptions, layout_document};

fn id(value: &str) -> Id {
    Id::new(value).unwrap()
}
fn paragraph(text: &str) -> Paragraph {
    Paragraph::new(id("paragraph"), vec![TextRun::new(text)])
}
fn document(paragraphs: Vec<Paragraph>) -> Document {
    Document::from_parts(
        id("document"),
        DocumentMetadata::default(),
        vec![Section::new(
            id("section"),
            paragraphs.into_iter().map(Block::Paragraph).collect(),
        )],
        vec![],
    )
    .unwrap()
}
fn render(text: &str) -> DocumentLayout {
    layout_document(&document(vec![paragraph(text)]), &LayoutOptions::default()).unwrap()
}
fn line(layout: &DocumentLayout) -> &office_layout::LineLayout {
    &layout.pages[0].lines[0]
}

#[test]
fn blank_documents_and_empty_paragraphs_have_page_and_caret_geometry() {
    let blank = Document::new(id("blank")).unwrap();
    let layout = layout_document(&blank, &LayoutOptions::default()).unwrap();
    assert_eq!(layout.pages.len(), 1);
    assert_eq!(
        (layout.pages[0].width, layout.pages[0].height),
        (612 * 64, 792 * 64)
    );
    let empty = render("");
    assert_eq!(empty.pages[0].lines.len(), 1);
    assert_eq!(line(&empty).carets.len(), 1);
    assert_eq!(line(&empty).carets[0].grapheme, 0);
    assert!(line(&empty).height > 0);
}

#[test]
fn pinned_font_kerning_and_size_affect_measured_advance() {
    let pair = render("AV");
    assert!(line(&pair).width < line(&render("A")).width + line(&render("V")).width);
    let split_pair = layout_document(
        &document(vec![Paragraph::new(
            id("paragraph"),
            vec![TextRun::new("A"), TextRun::new("V")],
        )]),
        &LayoutOptions::default(),
    )
    .unwrap();
    assert_eq!(line(&pair).width, line(&split_pair).width);
    let options = LayoutOptions {
        font_size: HalfPoints(48),
        ..Default::default()
    };
    let doubled = layout_document(&document(vec![paragraph("AV")]), &options).unwrap();
    assert!((line(&doubled).width - line(&pair).width * 2).abs() <= 1);
    let serif = layout_document(
        &document(vec![paragraph("Words measure differently")]),
        &LayoutOptions {
            font_family: "Tinos".into(),
            ..Default::default()
        },
    )
    .unwrap();
    assert_ne!(
        line(&serif).width,
        line(&render("Words measure differently")).width
    );
    assert!(
        serif
            .outlines
            .iter()
            .all(|outline| outline.font_id.starts_with("Tinos"))
    );
}

#[test]
fn genuine_latin_weight_and_italic_faces_use_distinct_outlines() {
    let normal = render("A");
    let bold = layout_document(
        &document(vec![Paragraph::new(
            id("paragraph"),
            vec![TextRun {
                text: "A".into(),
                format: CharacterFormat {
                    bold: Some(true),
                    ..Default::default()
                },
            }],
        )]),
        &LayoutOptions::default(),
    )
    .unwrap();
    assert_ne!(normal.outlines[0].path, bold.outlines[0].path);
    let italic = layout_document(
        &document(vec![Paragraph::new(
            id("paragraph"),
            vec![TextRun {
                text: "A".into(),
                format: CharacterFormat {
                    italic: Some(true),
                    ..Default::default()
                },
            }],
        )]),
        &LayoutOptions::default(),
    )
    .unwrap();
    assert_eq!(italic.outlines[0].font_id, "Arimo-Italic.ttf");
    assert!(
        !italic
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.kind == "synthetic_italic")
    );
}

#[test]
fn unicode_ligatures_and_pinned_complex_script_fallback_are_real_shaping() {
    // A supported emoji ZWJ ligature is three Unicode scalars, one glyph.
    // Latin discretionary ligatures are not enabled just to force a count.
    let ligature = render("👩‍💻");
    assert!(
        line(&ligature).glyphs.len() < 3,
        "{:?}",
        ligature
            .outlines
            .iter()
            .map(|outline| (&outline.font_id, outline.glyph_id))
            .collect::<Vec<_>>()
    );
    let isolated = render("ب");
    let joined = render("بب");
    assert!(joined.outlines.iter().any(|outline| {
        outline.font_id == "NotoSansArabic.ttf"
            && !isolated
                .outlines
                .iter()
                .any(|other| outline.font_id == other.font_id && outline.glyph_id == other.glyph_id)
    }));
    let mixed = render("Cafe\u{301} العربية नमस्ते שלום 😀");
    assert!(
        !mixed
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.kind == "missing_glyph")
    );
    for expected in [
        "NotoSansArabic.ttf",
        "NotoSansDevanagari.ttf",
        "NotoEmoji.ttf",
    ] {
        assert!(
            mixed
                .outlines
                .iter()
                .any(|outline| outline.font_id == expected),
            "missing fallback {expected}"
        );
    }
    assert!(
        mixed
            .outlines
            .iter()
            .any(|outline| !outline.path.is_empty())
    );
}

#[test]
fn combining_and_emoji_cursor_mappings_use_core_boundaries_across_runs() {
    let text = "Ae\u{301}👩🏽‍💻🇺🇳Z";
    let source = document(vec![Paragraph::new(
        id("paragraph"),
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
    )]);
    let before = source.to_json().unwrap();
    let layout = layout_document(&source, &LayoutOptions::default()).unwrap();
    let coordinates = source.text_coordinates(&id("paragraph")).unwrap();
    let mut expected = Vec::new();
    for index in 0..=coordinates.grapheme_len().get() {
        expected.push((
            coordinates
                .grapheme_to_utf8(office_core::GraphemeOffset::new(index))
                .unwrap()
                .get(),
            coordinates
                .grapheme_to_utf16(office_core::GraphemeOffset::new(index))
                .unwrap()
                .get(),
            index,
        ));
    }
    let actual: std::collections::BTreeSet<_> = line(&layout)
        .carets
        .iter()
        .map(|caret| (caret.utf8, caret.utf16, caret.grapheme))
        .collect();
    assert_eq!(actual, expected.into_iter().collect());
    assert_eq!(line(&layout).text, text);
    assert_eq!(source.to_json().unwrap(), before);
}

#[test]
fn explicit_newlines_crlf_and_final_empty_lines_keep_logical_source_offsets() {
    for text in [
        "one\ntwo",
        "one\r\ntwo",
        "one\rtwo",
        "one\u{2028}two",
        "one\u{2029}two",
    ] {
        let layout = render(text);
        assert_eq!(layout.pages[0].lines.len(), 2, "{text:?}");
        assert_eq!(
            layout.pages[0]
                .lines
                .iter()
                .map(|line| line.text.as_str())
                .collect::<String>(),
            text
        );
        assert_eq!(
            layout.pages[0].lines.last().unwrap().text_range.end_utf8,
            text.len()
        );
    }
    let trailing = render("one\n");
    assert_eq!(trailing.pages[0].lines.len(), 2);
    assert_eq!(trailing.pages[0].lines[1].text, "");
    assert_eq!(trailing.pages[0].lines[1].carets[0].utf8, 4);
}

#[test]
fn alignment_and_justification_use_measured_pinned_advances() {
    let mut xs = Vec::new();
    for alignment in [Alignment::Left, Alignment::Center, Alignment::Right] {
        let source = document(vec![paragraph("Aligned").with_format(ParagraphFormat {
            alignment: Some(alignment),
            ..Default::default()
        })]);
        xs.push(line(&layout_document(&source, &LayoutOptions::default()).unwrap()).x);
    }
    assert!(xs[0] < xs[1] && xs[1] < xs[2]);
    let source = document(vec![
        paragraph(&"Words for justified wrapping. ".repeat(25)).with_format(ParagraphFormat {
            alignment: Some(Alignment::Justify),
            ..Default::default()
        }),
    ]);
    let layout = layout_document(&source, &LayoutOptions::default()).unwrap();
    let lines = &layout.pages[0].lines;
    assert!(lines.len() > 2);
    assert!((lines[0].width - layout.pages[0].content_rect.width).abs() <= 2);
    assert!(lines.last().unwrap().width < lines[0].width);
}

#[test]
fn first_line_hanging_and_right_indents_control_the_available_wrap_area() {
    for first_line in [360, -360] {
        let source = document(vec![
            paragraph(&"Indented words wrap carefully. ".repeat(12)).with_format(ParagraphFormat {
                indentation: Some(Indentation {
                    left: Some(Twips(720)),
                    right: Some(Twips(720)),
                    first_line: Some(Twips(first_line)),
                }),
                ..Default::default()
            }),
        ]);
        let layout = layout_document(&source, &LayoutOptions::default()).unwrap();
        assert!(layout.pages[0].lines.len() > 1);
        let lines = &layout.pages[0].lines;
        assert_eq!(lines[0].x - lines[1].x, i64::from(first_line) * 64 / 20);
        assert_eq!(lines[1].x, (72 + 36) * 64);
    }
}

#[test]
fn exact_minimum_and_multiple_line_spacing_remain_distinct() {
    let source = |spacing| {
        document(vec![paragraph("Line spacing").with_format(
            ParagraphFormat {
                line_spacing: Some(spacing),
                ..Default::default()
            },
        )])
    };
    let natural = layout_document(
        &source(LineSpacing::Multiple { value: 240 }),
        &LayoutOptions::default(),
    )
    .unwrap();
    let double = layout_document(
        &source(LineSpacing::Multiple { value: 480 }),
        &LayoutOptions::default(),
    )
    .unwrap();
    assert!((line(&double).height - line(&natural).height * 2).abs() <= 1);
    let exact = layout_document(
        &source(LineSpacing::Exact { height: Twips(20) }),
        &LayoutOptions::default(),
    )
    .unwrap();
    let minimum = layout_document(
        &source(LineSpacing::AtLeast { height: Twips(20) }),
        &LayoutOptions::default(),
    )
    .unwrap();
    assert_eq!(line(&exact).height, 64);
    assert!(
        exact
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.kind == "exact_line_ink_overflow" })
    );
    assert_eq!(line(&minimum).height, line(&natural).height);
    let large_minimum = layout_document(
        &source(LineSpacing::AtLeast { height: Twips(800) }),
        &LayoutOptions::default(),
    )
    .unwrap();
    assert_eq!(line(&large_minimum).height, 40 * 64);
}

#[test]
fn subresolution_line_spacing_is_rejected_instead_of_clamped() {
    let error = layout_document(
        &document(vec![paragraph("tiny")]),
        &LayoutOptions {
            font_size: HalfPoints(1),
            line_spacing: LineSpacing::Multiple { value: 1 },
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(matches!(error, LayoutError::Unsupported { .. }));
    assert!(error.to_string().contains("resolution"));
}

#[test]
fn spacing_suppressed_at_page_boundary_is_reported() {
    let source = document(vec![paragraph("first").with_format(ParagraphFormat {
        spacing_before: Some(Twips(20_000)),
        ..Default::default()
    })]);
    let layout = layout_document(&source, &LayoutOptions::default()).unwrap();
    assert_eq!(layout.pages.len(), 1);
    assert_eq!(line(&layout).y, layout.pages[0].content_rect.y);
    assert!(
        layout
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.kind == "page_spacing_suppressed" })
    );
}

#[test]
fn diagnostic_budget_reports_truncation_and_deduplicates_repeated_findings() {
    let options = LayoutOptions {
        font_family: "Uninstalled font".into(),
        ..Default::default()
    };
    let repeated = document(vec![Paragraph::new(
        id("paragraph"),
        (0..150).map(|_| TextRun::new("word ")).collect(),
    )]);
    let layout = layout_document(&repeated, &options).unwrap();
    assert_eq!(layout.diagnostics.len(), 1);
    let many = document(
        (0..130)
            .map(|index| {
                Paragraph::new(
                    id(&format!("paragraph-{index}")),
                    vec![TextRun::new("word")],
                )
            })
            .collect(),
    );
    let layout = layout_document(&many, &options).unwrap();
    assert_eq!(layout.diagnostics.len(), 128);
    let marker = layout.diagnostics.last().unwrap();
    assert_eq!(marker.kind, "diagnostics_truncated");
    assert_eq!(marker.paragraph_id, None);
}

#[test]
fn paragraph_spacing_collapses_to_maximum_instead_of_adding() {
    let source = document(vec![
        paragraph("First").with_format(ParagraphFormat {
            spacing_after: Some(Twips(240)),
            ..Default::default()
        }),
        Paragraph::new(id("second"), vec![TextRun::new("Second")]).with_format(ParagraphFormat {
            spacing_before: Some(Twips(80)),
            ..Default::default()
        }),
    ]);
    let layout = layout_document(&source, &LayoutOptions::default()).unwrap();
    let lines = &layout.pages[0].lines;
    assert_eq!(lines[1].y - lines[0].y - lines[0].height, 12 * 64);
}

#[test]
fn mixed_color_and_double_underline_are_renderable_without_browser_text_layout() {
    let source = document(vec![Paragraph::new(
        id("paragraph"),
        vec![TextRun {
            text: "Underlined".into(),
            format: CharacterFormat {
                underline: Some(Underline::Double),
                color: Some(TextColor::Rgb {
                    red: 12,
                    green: 34,
                    blue: 56,
                }),
                ..Default::default()
            },
        }],
    )]);
    let layout = layout_document(&source, &LayoutOptions::default()).unwrap();
    assert_eq!(line(&layout).decorations.len(), 2);
    assert!(
        line(&layout)
            .decorations
            .iter()
            .all(|decoration| decoration.y >= line(&layout).baseline
                && decoration.color == "#0c2238")
    );
    assert!(
        line(&layout)
            .glyphs
            .iter()
            .all(|glyph| glyph.color == "#0c2238"
                && glyph.transform.contains("translate(")
                && glyph.transform.contains("scale("))
    );
}

#[test]
fn pagination_preserves_paragraph_continuations_and_section_dimensions() {
    let source = document(vec![
        paragraph(&"Long document words. ".repeat(1500)),
        Paragraph::new(id("break"), vec![TextRun::new("Forced page")]).with_format(
            ParagraphFormat {
                page_break_before: Some(true),
                ..Default::default()
            },
        ),
    ]);
    let layout = layout_document(&source, &LayoutOptions::default()).unwrap();
    assert!(layout.pages.len() > 3);
    let continuation: Vec<_> = layout
        .pages
        .iter()
        .flat_map(|page| page.lines.iter())
        .filter(|line| line.paragraph_id == "paragraph")
        .collect();
    assert!(
        continuation
            .iter()
            .enumerate()
            .all(|(index, line)| line.line_index as usize == index)
    );
    assert_eq!(layout.pages.last().unwrap().lines[0].paragraph_id, "break");
    for page in &layout.pages {
        for line in &page.lines {
            assert!(
                line.y >= page.content_rect.y
                    && line.y + line.height <= page.content_rect.y + page.content_rect.height
            );
        }
    }
    let a4 = layout_document(&document(vec![paragraph("A4")]), &LayoutOptions::a4()).unwrap();
    assert_eq!(a4.pages[0].width, (11_906_i64 * 64 + 10) / 20);
    let custom = Document::from_parts(
        id("custom"),
        Default::default(),
        vec![
            Section::new(id("section"), vec![]).with_properties(SectionProperties {
                page_size: Some(PageSize {
                    width: Twips(6000),
                    height: Twips(9000),
                    orientation: None,
                }),
                page_margins: Some(PageMargins {
                    top: Twips(600),
                    right: Twips(600),
                    bottom: Twips(600),
                    left: Twips(600),
                    header: Twips(0),
                    footer: Twips(0),
                    gutter: Twips(0),
                }),
            }),
        ],
        vec![],
    )
    .unwrap();
    let layout = layout_document(&custom, &LayoutOptions::default()).unwrap();
    assert_eq!(
        (layout.pages[0].width, layout.pages[0].height),
        (300 * 64, 450 * 64)
    );
}

#[test]
fn repeated_layout_is_exact_with_reused_and_fresh_contexts_and_after_reopen() {
    let source = document(vec![paragraph("Stable AV العربية 👩🏽‍💻 and words")]);
    let mut engine = LayoutEngine::new().unwrap();
    let options = LayoutOptions::default();
    let first = engine.layout_document(&source, &options).unwrap();
    assert_eq!(engine.layout_document(&source, &options).unwrap(), first);
    assert_eq!(layout_document(&source, &options).unwrap(), first);
    let restored =
        office_core::deserialize_document(&office_core::serialize_document(&source).unwrap())
            .unwrap();
    assert_eq!(layout_document(&restored, &options).unwrap(), first);
}

#[test]
fn missing_fonts_glyphs_and_unimplemented_controls_are_explicit() {
    let substituted = layout_document(
        &document(vec![paragraph("Unknown font")]),
        &LayoutOptions {
            font_family: "Not Installed".into(),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        substituted
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.kind == "font_substitution")
    );
    assert!(
        render("中文")
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.kind == "missing_glyph")
    );
    for text in ["tab\tstop", "nul\0control", "page\u{000c}break"] {
        assert!(matches!(
            layout_document(&document(vec![paragraph(text)]), &LayoutOptions::default()),
            Err(LayoutError::Unsupported { .. })
        ));
    }
}
