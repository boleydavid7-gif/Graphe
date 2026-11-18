//! Step 8C.3 page-flow regression tests using the real pinned Rust layout engine.
use office_core::{
    Block, Document, DocumentMetadata, Id, LineSpacing, PageMargins, PageSize, Paragraph,
    ParagraphFormat, Section, TextRun, Twips,
};
use office_layout::{DocumentLayout, LayoutOptions, layout_document};

fn id(s: &str) -> Id {
    Id::new(s).unwrap()
}
fn para(name: &str, text: &str, together: bool, with_next: bool) -> Paragraph {
    Paragraph::new(id(name), vec![TextRun::new(text)]).with_format(ParagraphFormat {
        line_spacing: Some(LineSpacing::Exact { height: Twips(360) }),
        keep_together: Some(together),
        keep_with_next: Some(with_next),
        ..Default::default()
    })
}
fn layout(paragraphs: Vec<Paragraph>) -> DocumentLayout {
    let doc = Document::from_parts(
        id("pagination-test"),
        DocumentMetadata::default(),
        vec![Section::new(
            id("s"),
            paragraphs.into_iter().map(Block::Paragraph).collect(),
        )],
        vec![],
    )
    .unwrap();
    layout_document(
        &doc,
        &LayoutOptions {
            page_size: PageSize {
                width: Twips(7_200),
                height: Twips(2_000),
                orientation: None,
            },
            page_margins: PageMargins {
                top: Twips(0),
                right: Twips(0),
                bottom: Twips(0),
                left: Twips(0),
                header: Twips(0),
                footer: Twips(0),
                gutter: Twips(0),
            },
            ..Default::default()
        },
    )
    .unwrap()
}
fn pages_for(layout: &DocumentLayout, id: &str) -> Vec<usize> {
    layout
        .pages
        .iter()
        .enumerate()
        .filter(|(_, page)| page.lines.iter().any(|line| line.paragraph_id == id))
        .map(|(index, _)| index)
        .collect()
}

#[test]
fn keep_together_moves_a_fitting_paragraph_before_any_lines_are_emitted() {
    let normal = layout(vec![
        para("first", "A\nB", false, false),
        para("second", "D\nE\nF\nG", false, false),
    ]);
    let kept = layout(vec![
        para("first", "A\nB", false, false),
        para("second", "D\nE\nF\nG", true, false),
    ]);
    assert!(
        pages_for(&normal, "second").len() > 1,
        "baseline must split: {:?}",
        normal.pages
    );
    assert_eq!(pages_for(&kept, "second"), vec![1]);
    assert_eq!(
        kept.pages[1]
            .lines
            .iter()
            .filter(|line| line.paragraph_id == "second")
            .count(),
        4
    );
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
    assert_eq!(
        kept.pages
            .iter()
            .flat_map(|page| &page.lines)
            .filter(|line| line.paragraph_id == "body")
            .count(),
        1
    );
}

#[test]
fn impossible_keep_together_preserves_all_lines_and_reports_fallback() {
    let tall = layout(vec![para("tall", "1\n2\n3\n4\n5\n6\n7", true, false)]);
    assert!(tall.pages.len() > 1);
    assert_eq!(tall.pages.iter().flat_map(|page| &page.lines).count(), 7);
    assert!(
        tall.diagnostics
            .iter()
            .any(|diag| diag.kind == "keep_together_too_tall")
    );
}

#[test]
fn default_widow_control_moves_the_first_two_lines_from_a_nearly_full_page() {
    let output = layout(vec![
        para("first", "A\nB\nC\nD", false, false),
        para("second", "D\nE\nF\nG", false, false),
    ]);
    assert_eq!(pages_for(&output, "first"), vec![0]);
    // Four lines plus spacing leave room for only one subsequent line.
    // The second paragraph begins on a new page with at least two lines.
    assert_eq!(pages_for(&output, "second"), vec![1]);
    assert_eq!(
        output.pages[1]
            .lines
            .iter()
            .filter(|line| line.paragraph_id == "second")
            .count(),
        4
    );
}

#[test]
fn last_two_lines_move_together_instead_of_orphaning_a_final_line() {
    let output = layout(vec![
        para("first", "A\nB", false, false),
        para("second", "D\nE\nF\nG", false, false),
    ]);
    assert_eq!(pages_for(&output, "second"), vec![0, 1]);
    let fragments: Vec<_> = output
        .pages
        .iter()
        .map(|page| {
            page.lines
                .iter()
                .filter(|line| line.paragraph_id == "second")
                .count()
        })
        .collect();
    assert_eq!(fragments, vec![2, 2]);
    assert_eq!(
        output
            .pages
            .iter()
            .flat_map(|page| &page.lines)
            .filter(|line| line.paragraph_id == "second")
            .count(),
        4
    );
}

#[test]
fn two_or_three_line_paragraphs_that_fit_page_are_never_split() {
    for lines in ["D\nE", "D\nE\nF"] {
        let output = layout(vec![
            para("first", "A\nB\nC\nD", false, false),
            para("short", lines, false, false),
        ]);
        assert_eq!(pages_for(&output, "short"), vec![1], "{lines:?}");
    }
}

#[test]
fn overlong_paragraph_uses_bounded_fallback_without_losing_source_lines() {
    let text = "1\n2\n3\n4\n5\n6\n7\n8\n9";
    let output = layout(vec![para("long", text, false, false)]);
    let lines: Vec<_> = output.pages.iter().flat_map(|page| &page.lines).collect();
    assert_eq!(lines.len(), 9);
    assert_eq!(
        lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<String>(),
        text
    );
    assert!(output.pages.len() > 1);
}
