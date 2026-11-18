//! First-level external links survive the DOCX relationship graph.

use office_core::{Block, Document, DocumentMetadata, Id, Paragraph, Section, TextRun};
use office_docx::{
    DocxImportOutcome, DocxSource, export_docx, export_docx_with_source, import_docx,
};

fn document() -> Document {
    let mut run = TextRun::new("Example website");
    run.format.hyperlink = Some("https://example.com/article?x=1&y=2".into());
    let paragraph = Paragraph::new(
        Id::new("p").unwrap(),
        vec![TextRun::new("Before "), run, TextRun::new(" after")],
    );
    Document::from_parts(
        Id::new("doc").unwrap(),
        DocumentMetadata::default(),
        vec![Section::new(
            Id::new("s").unwrap(),
            vec![Block::Paragraph(paragraph)],
        )],
        vec![],
    )
    .unwrap()
}

fn assert_link(document: &Document) {
    let Block::Paragraph(paragraph) = &document.sections()[0].blocks()[0] else {
        panic!("expected paragraph")
    };
    assert_eq!(paragraph.runs().len(), 3);
    assert_eq!(paragraph.runs()[1].text, "Example website");
    assert_eq!(
        paragraph.runs()[1].format.hyperlink.as_deref(),
        Some("https://example.com/article?x=1&y=2")
    );
    assert_eq!(paragraph.runs()[0].format.hyperlink, None);
    assert_eq!(paragraph.runs()[2].format.hyperlink, None);
}

#[test]
fn docx_link_relationship_round_trip_keeps_selected_text_and_url() {
    let bytes = export_docx(&document()).unwrap();
    let opened = import_docx(&bytes, Id::new("reopened").unwrap()).unwrap();
    let DocxImportOutcome::Editable(opened) = opened else {
        panic!("Exported hyperlinks should reopen as editable");
    };
    assert_link(opened.document());
    let source = DocxSource::open(&bytes).unwrap();
    let copied = export_docx_with_source(&document(), &source).unwrap();
    let reopened = import_docx(&copied, Id::new("copy").unwrap()).unwrap();
    let DocxImportOutcome::Editable(reopened) = reopened else {
        panic!("Source-aware Word copy lost hyperlink semantics");
    };
    assert_link(reopened.document());
}
