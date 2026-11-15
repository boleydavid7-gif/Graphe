//! Single-level lists must survive WordprocessingML export and import.

use office_core::{
    Block, Document, DocumentMetadata, Id, ListKind, Paragraph, Section, TextRun,
};
use office_docx::{DocxImportOutcome, export_docx, import_docx};

fn document() -> Document {
    let paragraphs = [
        ("One", Some(ListKind::Numbered)),
        ("Two", Some(ListKind::Numbered)),
        ("Paragraph", None),
        ("Three", Some(ListKind::Numbered)),
        ("Bullet", Some(ListKind::Bullet)),
    ].into_iter().enumerate().map(|(index, (text, list))| {
        Block::Paragraph(Paragraph::new(
            Id::new(format!("para-{index}")).unwrap(),
            vec![TextRun::new(text)],
        ).with_list(list))
    }).collect();
    Document::from_parts(Id::new("source").unwrap(),
        DocumentMetadata::default(),
        vec![Section::new(Id::new("section").unwrap(), paragraphs)], vec![]).unwrap()
}

#[test]
fn word_numbering_is_structural_and_round_trips() {
    let file = export_docx(&document()).unwrap();
    let imported = import_docx(&file, Id::new("imported").unwrap()).unwrap();
    let DocxImportOutcome::Editable(imported) = imported else {
        panic!("Graphe's own exported first-level lists must import as editable");
    };
    let lists = imported.document().sections()[0].blocks().iter().map(|block| {
        let Block::Paragraph(paragraph) = block;
        paragraph.list()
    }).collect::<Vec<_>>();
    assert_eq!(lists, vec![
        Some(ListKind::Numbered),
        Some(ListKind::Numbered),
        None,
        Some(ListKind::Numbered),
        Some(ListKind::Bullet),
    ]);
    assert!(imported.report().is_supported());
}
