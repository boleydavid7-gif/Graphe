//! Word stores paragraph metrics as native pPr spacing/ind properties.

use office_core::{
    Block, Document, DocumentMetadata, Id, Indentation, LineSpacing, Paragraph, ParagraphFormat,
    Section, TextRun, Twips,
};
use office_docx::{DocxImportOutcome, export_docx, import_docx};

#[test]
fn paragraph_spacing_and_hanging_indent_survive_word_round_trip() {
    let format = ParagraphFormat {
        spacing_before: Some(Twips(80)),
        spacing_after: Some(Twips(240)),
        line_spacing: Some(LineSpacing::Multiple { value: 276 }),
        indentation: Some(Indentation {
            left: Some(Twips(720)),
            right: Some(Twips(360)),
            first_line: Some(Twips(-360)),
        }),
        ..Default::default()
    };
    let paragraph = Paragraph::new(
        Id::new("paragraph").unwrap(),
        vec![TextRun::new("Text with a hanging indent")],
    )
    .with_format(format.clone());
    let original = Document::from_parts(
        Id::new("document").unwrap(),
        DocumentMetadata::default(),
        vec![Section::new(
            Id::new("section").unwrap(),
            vec![Block::Paragraph(paragraph)],
        )],
        vec![],
    )
    .unwrap();
    let word_bytes = export_docx(&original).unwrap();
    let outcome = import_docx(&word_bytes, Id::new("reopened").unwrap()).unwrap();
    let DocxImportOutcome::Editable(imported) = outcome else {
        panic!("Word paragraph spacing should remain editable after export/import");
    };
    let Block::Paragraph(restored) = &imported.document().sections()[0].blocks()[0] else {
        panic!("expected paragraph")
    };
    assert_eq!(restored.format(), &format);
}
