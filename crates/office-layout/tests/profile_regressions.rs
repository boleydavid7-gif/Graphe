//! Independent persistence, geometry and resource-bound regressions.

use office_core::{
    Block, CharacterFormat, Document, DocumentMetadata, HalfPoints, Id, Indentation, PageMargins,
    PageOrientation, PageSize, Paragraph, ParagraphFormat, Section, SectionProperties, TextRun,
    Twips, deserialize_document, serialize_document,
};
use office_layout::{LayoutEngine, LayoutOptions, UNITS_PER_POINT};

fn id(value: &str) -> Id {
    Id::new(value).unwrap()
}

fn document(paragraphs: Vec<Paragraph>, properties: Option<SectionProperties>) -> Document {
    let mut section = Section::new(
        id("section"),
        paragraphs.into_iter().map(Block::Paragraph).collect(),
    );
    if let Some(properties) = properties {
        section = section.with_properties(properties);
    }
    Document::from_parts(
        id("document"),
        DocumentMetadata::default(),
        vec![section],
        vec![],
    )
    .unwrap()
}

#[test]
fn legacy_supported_model_and_native_reopen_reproduce_layout_without_mutation() {
    let original = Document::from_json(include_str!(
        "../../../tests/fixtures/writer-sample-v1.json"
    ))
    .unwrap();
    let bytes = serialize_document(&original).unwrap();
    let reopened = deserialize_document(&bytes).unwrap();
    let mut engine = LayoutEngine::new().unwrap();
    let initial = engine
        .layout_document(&original, &LayoutOptions::default())
        .unwrap();
    let next = engine
        .layout_document(&reopened, &LayoutOptions::default())
        .unwrap();
    assert_eq!(initial, next);
    assert_eq!(serialize_document(&original).unwrap(), bytes);
    assert_eq!(serialize_document(&reopened).unwrap(), bytes);
}

#[test]
fn explicit_orientation_does_not_rotate_custom_dimensions_and_gutter_reduces_body() {
    let size = PageSize {
        width: Twips(12_240),
        height: Twips(15_840),
        orientation: Some(PageOrientation::Landscape),
    };
    let margins = PageMargins {
        gutter: Twips(720),
        ..LayoutOptions::default().page_margins
    };
    let source = document(
        vec![Paragraph::new(
            id("paragraph"),
            vec![TextRun::new("content")],
        )],
        Some(SectionProperties {
            page_size: Some(size),
            page_margins: Some(margins),
        }),
    );
    let layout = LayoutEngine::new()
        .unwrap()
        .layout_document(&source, &LayoutOptions::a4())
        .unwrap();
    let page = &layout.pages[0];
    assert_eq!(page.width, 612 * UNITS_PER_POINT);
    assert_eq!(page.height, 792 * UNITS_PER_POINT);
    assert_eq!(page.content_rect.x, 108 * UNITS_PER_POINT);
    assert_eq!(page.content_rect.width, 432 * UNITS_PER_POINT);
}

#[test]
fn hanging_indent_has_distinct_first_line_and_continuation_origins() {
    let paragraph = Paragraph::new(id("paragraph"), vec![TextRun::new("word ".repeat(80))])
        .with_format(ParagraphFormat {
            indentation: Some(Indentation {
                left: Some(Twips(720)),
                right: Some(Twips(0)),
                first_line: Some(Twips(-360)),
            }),
            ..ParagraphFormat::default()
        });
    let source = document(vec![paragraph], None);
    let mut options = LayoutOptions::default();
    options.page_size.width = Twips(5_760);
    let layout = LayoutEngine::new()
        .unwrap()
        .layout_document(&source, &options)
        .unwrap();
    let lines: Vec<_> = layout.pages.iter().flat_map(|page| &page.lines).collect();
    assert!(lines.len() > 1);
    assert_eq!(lines[0].x, 90 * UNITS_PER_POINT);
    assert_eq!(lines[1].x, 108 * UNITS_PER_POINT);
}

#[test]
fn adjacent_paragraph_before_after_spacing_uses_the_larger_gap() {
    let first =
        Paragraph::new(id("first"), vec![TextRun::new("one")]).with_format(ParagraphFormat {
            spacing_after: Some(Twips(240)),
            ..ParagraphFormat::default()
        });
    let second =
        Paragraph::new(id("second"), vec![TextRun::new("two")]).with_format(ParagraphFormat {
            spacing_before: Some(Twips(80)),
            ..ParagraphFormat::default()
        });
    let source = document(vec![first, second], None);
    let layout = LayoutEngine::new()
        .unwrap()
        .layout_document(&source, &LayoutOptions::default())
        .unwrap();
    let lines = &layout.pages[0].lines;
    assert_eq!(lines.len(), 2);
    assert_eq!(
        lines[1].y - lines[0].y - lines[0].height,
        12 * UNITS_PER_POINT
    );
}

#[test]
fn large_valid_font_size_is_a_layout_error_and_does_not_change_model() {
    let mut run = TextRun::new("safe");
    run.format = CharacterFormat {
        font_size: Some(HalfPoints(u32::MAX)),
        ..CharacterFormat::default()
    };
    let source = document(vec![Paragraph::new(id("paragraph"), vec![run])], None);
    source.validate().unwrap();
    let before = source.to_json().unwrap();
    assert!(
        LayoutEngine::new()
            .unwrap()
            .layout_document(&source, &LayoutOptions::default())
            .is_err()
    );
    assert_eq!(source.to_json().unwrap(), before);
}

#[test]
fn valid_native_ids_cannot_amplify_unbounded_layout_output() {
    let source = document(
        vec![Paragraph::new(
            id(&"p".repeat(2_048)),
            vec![TextRun::new("safe")],
        )],
        None,
    );
    source.validate().unwrap();
    assert!(
        LayoutEngine::new()
            .unwrap()
            .layout_document(&source, &LayoutOptions::default())
            .is_err()
    );
}

#[test]
fn tiny_positive_body_reports_overflow_and_extreme_page_geometry_fails() {
    let source = document(
        vec![Paragraph::new(id("paragraph"), vec![TextRun::new("safe")])],
        Some(SectionProperties {
            page_size: Some(PageSize {
                width: Twips(2_881),
                height: Twips(15_840),
                orientation: None,
            }),
            page_margins: None,
        }),
    );
    let layout = LayoutEngine::new()
        .unwrap()
        .layout_document(&source, &LayoutOptions::default())
        .unwrap();
    assert!(layout.pages[0].content_rect.width > 0);
    assert!(
        layout
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.kind == "line_overflow")
    );
    let oversized = document(
        vec![],
        Some(SectionProperties {
            page_size: Some(PageSize {
                width: Twips(i32::MAX),
                height: Twips(i32::MAX),
                orientation: None,
            }),
            page_margins: None,
        }),
    );
    assert!(
        LayoutEngine::new()
            .unwrap()
            .layout_document(&oversized, &LayoutOptions::default())
            .is_err()
    );
    source.validate().unwrap();
    oversized.validate().unwrap();
}

#[test]
fn extreme_negative_indentation_is_rejected_without_clamping_source() {
    let source = document(
        vec![
            Paragraph::new(id("paragraph"), vec![TextRun::new("safe")]).with_format(
                ParagraphFormat {
                    indentation: Some(Indentation {
                        left: Some(Twips(i32::MIN)),
                        ..Indentation::default()
                    }),
                    ..ParagraphFormat::default()
                },
            ),
        ],
        None,
    );
    let before = source.to_json().unwrap();
    assert!(
        LayoutEngine::new()
            .unwrap()
            .layout_document(&source, &LayoutOptions::default())
            .is_err()
    );
    assert_eq!(source.to_json().unwrap(), before);
}
