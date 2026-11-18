//! Single-level lists must survive WordprocessingML export and import.

use office_core::{Block, Document, DocumentMetadata, Id, ListKind, Paragraph, Section, TextRun};
use office_docx::{
    DocxImportOutcome, DocxSource, export_docx, export_docx_with_source, import_docx,
};

fn document() -> Document {
    let paragraphs = [
        ("One", Some(ListKind::Numbered)),
        ("Two", Some(ListKind::Numbered)),
        ("Paragraph", None),
        ("Three", Some(ListKind::Numbered)),
        ("Bullet", Some(ListKind::Bullet)),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (text, list))| {
        Block::Paragraph(
            Paragraph::new(
                Id::new(format!("para-{index}")).unwrap(),
                vec![TextRun::new(text)],
            )
            .with_list(list),
        )
    })
    .collect();
    Document::from_parts(
        Id::new("source").unwrap(),
        DocumentMetadata::default(),
        vec![Section::new(Id::new("section").unwrap(), paragraphs)],
        vec![],
    )
    .unwrap()
}

#[test]
fn word_numbering_is_structural_and_round_trips() {
    let file = export_docx(&document()).unwrap();
    let imported = import_docx(&file, Id::new("imported").unwrap()).unwrap();
    let DocxImportOutcome::Editable(imported) = imported else {
        panic!("Graphe's own exported first-level lists must import as editable");
    };
    let lists = imported.document().sections()[0]
        .blocks()
        .iter()
        .map(|block| {
            let Block::Paragraph(paragraph) = block else {
                panic!("expected paragraph")
            };
            paragraph.list()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        lists,
        vec![
            Some(ListKind::Numbered),
            Some(ListKind::Numbered),
            None,
            Some(ListKind::Numbered),
            Some(ListKind::Bullet),
        ]
    );
    assert!(imported.report().is_supported());
}

#[test]
fn source_aware_word_copy_retains_structural_numbering() {
    let current = document();
    let bytes = export_docx(&current).unwrap();
    let source = DocxSource::open(&bytes).unwrap();
    let copied = export_docx_with_source(&current, &source).unwrap();
    let imported = import_docx(&copied, Id::new("reopened-copy").unwrap()).unwrap();
    let DocxImportOutcome::Editable(imported) = imported else {
        panic!("The regenerated, source-aware Word copy lost list editability");
    };
    let lists = imported.document().sections()[0]
        .blocks()
        .iter()
        .map(|block| {
            let Block::Paragraph(paragraph) = block else {
                panic!("expected paragraph")
            };
            paragraph.list()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        lists,
        vec![
            Some(ListKind::Numbered),
            Some(ListKind::Numbered),
            None,
            Some(ListKind::Numbered),
            Some(ListKind::Bullet),
        ]
    );
}
