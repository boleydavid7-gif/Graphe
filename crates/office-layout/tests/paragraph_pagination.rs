//! Step 8C.3 page-flow regression tests using the real pinned Rust layout engine.
use office_core::{
    Block, Document, DocumentMetadata, Id, LineSpacing, PageMargins, PageSize,
    Paragraph, ParagraphFormat, Section, TextRun, Twips,
};
use office_layout::{layout_document, DocumentLayout, LayoutOptions};

fn id(s: &str) -> Id { Id::new(s).unwrap() }
fn para(name: &str, text: &str, together: bool, with_next: bool) -> Paragraph {
    Paragraph::new(id(name), vec![TextRun::new(text)]).with_format(ParagraphFormat {
        line_spacing: Some(LineSpacing::Exact { height: Twips(360) }),
        keep_together: Some(together),
        keep_with_next: Some(with_next),
        ..Default::default()
    })
}
fn layout(paragraphs: Vec<Paragraph>) -> DocumentLayout {
    let doc = Document::from_parts(id("pagination-test"), DocumentMetadata::default(),
        vec![Section::new(id("s"), paragraphs.into_iter().map(Block::Paragraph).collect())],
        vec![]).unwrap();
    layout_document(&doc, &LayoutOptions {
        page_size: PageSize { width: Twips(7_200), height: Twips(2_000), orientation: None },
        page_margins: PageMargins { top: Twips(0), right: Twips(0), bottom: Twips(0),
            left: Twips(0), header: Twips(0), footer: Twips(0), gutter: Twips(0) },
        ..Default::default()
    }).unwrap()
}
fn pages_for(layout: &DocumentLayout, id: &str) -> Vec<usize> {
    layout.pages.iter().enumerate().filter(|(_, page)| {
        page.lines.iter().any(|line| line.paragraph_id == id)
    }).map(|(index, _)| index).collect()
}

#[test]
fn keep_together_moves_a_fitting_paragraph_before_any_lines_are_emitted() {
    let normal = layout(vec![
        para("first", "A\nB\nC", false, false),
        para("second", "D\nE\nF", false, false),
    ]);
    let kept = layout(vec![
        para("first", "A\nB\nC", false, false),
        para("second", "D\nE\nF", true, false),
    ]);
    assert!(pages_for(&normal, "second").len() > 1, "baseline must split: {:?}", normal.pages);
    assert_eq!(pages_for(&kept, "second"), vec![1]);
    assert_eq!(kept.pages[1].lines.iter().filter(|line| line.paragraph_id == "second").count(), 3);
}

#[test]
fn keep_with_next_prevents_a_stranded_heading_on_the_previous_page() {
    let normal = layout(vec![
        para("first", "A\nB\nC", false, false),
        para("heading", "Heading", false, false),
        para("body", "Body", false, false),
    ]);
    let kept = layout(vec![
        para("first", "A\nB\nC", false, false),
        para("heading", "Heading", false, true),
        para("body", "Body", false, false),
    ]);
    assert_eq!(pages_for(&normal, "heading"), vec![0]);
    assert_eq!(pages_for(&kept, "heading"), vec![1]);
    assert_eq!(pages_for(&kept, "body"), vec![1]);
    // One shaping pass per paragraph: no duplicate line placement.
    assert_eq!(kept.pages.iter().flat_map(|page| &page.lines)
        .filter(|line| line.paragraph_id == "body").count(), 1);
}

#[test]
fn impossible_keep_together_preserves_all_lines_and_reports_fallback() {
    let tall = layout(vec![para("tall", "1\n2\n3\n4\n5\n6\n7", true, false)]);
    assert!(tall.pages.len() > 1);
    assert_eq!(tall.pages.iter().flat_map(|page| &page.lines).count(), 7);
    assert!(tall.diagnostics.iter().any(|diag| diag.kind == "keep_together_too_tall"));
}
