//! Explicit paragraph pagination properties survive supported format round-trips.
use office_core::{
    Block, Document, DocumentMetadata, Id, Paragraph, ParagraphFormat, Section, TextRun,
};
use office_formats::{
    DocumentFormat, ImportOutcome, assess_export, export_document, import_document,
};

fn sample() -> Document {
    let first = Paragraph::new(
        Id::new("heading").unwrap(),
        vec![TextRun::new("Section heading")],
    )
    .with_format(ParagraphFormat {
        keep_with_next: Some(true),
        ..Default::default()
    });
    let second = Paragraph::new(
        Id::new("body").unwrap(),
        vec![TextRun::new(
            "The paragraph must stay on one page when possible.",
        )],
    )
    .with_format(ParagraphFormat {
        keep_together: Some(true),
        ..Default::default()
    });
    Document::from_parts(
        Id::new("pagination-roundtrip").unwrap(),
        DocumentMetadata::default(),
        vec![Section::new(
            Id::new("section").unwrap(),
            vec![Block::Paragraph(first), Block::Paragraph(second)],
        )],
        vec![],
    )
    .unwrap()
}

#[test]
fn docx_odt_and_rtf_retain_explicit_keep_settings() {
    let original = sample();
    for format in [
        DocumentFormat::Docx,
        DocumentFormat::Odt,
        DocumentFormat::Rtf,
    ] {
        let bytes = export_document(format, &original).unwrap();
        let reopened =
            import_document(format, &bytes, Id::new("reopened").unwrap(), "copy").unwrap();
        let ImportOutcome::Editable(file) = reopened else {
            panic!("{format:?} must roundtrip complete native pagination flags");
        };
        let paragraphs: Vec<_> = file.document().paragraphs().collect();
        assert_eq!(paragraphs.len(), 2, "{format:?}");
        assert_eq!(
            paragraphs[0].format().keep_with_next,
            Some(true),
            "{format:?}"
        );
        assert_eq!(
            paragraphs[1].format().keep_together,
            Some(true),
            "{format:?}"
        );
    }
}

#[test]
fn plain_text_warns_about_losing_pagination_flags() {
    let report = assess_export(DocumentFormat::Txt, &sample()).unwrap();
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.code == "plain_text_only")
    );
}
