//! Real OPC/WordprocessingML round trips for the rectangular table profile.

use office_core::{
    Block, CharacterFormat, Document, DocumentMetadata, DocumentSession, GraphemeOffset, Id,
    Paragraph, Section, Table, TableCell, TableRow, TextRun, Twips, Underline,
};
use office_docx::{ConversionError, DocxImportOutcome, ImportedDocx, export_docx, import_docx};
use std::{
    collections::BTreeMap,
    io::{Cursor, Read, Write},
};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

fn id(value: &str) -> Id {
    Id::new(value).unwrap()
}
fn paragraph(name: &str, text: &str) -> Paragraph {
    Paragraph::new(id(name), vec![TextRun::new(text)])
}
fn document() -> Document {
    let mut mixed = TextRun::new("e\u{301} 👩🏽‍💻 — العربية");
    mixed.format = CharacterFormat {
        bold: Some(true),
        italic: Some(true),
        underline: Some(Underline::Single),
        hyperlink: Some("https://example.org/tables".into()),
        ..CharacterFormat::default()
    };
    let table = Table::new(
        id("table"),
        vec![
            TableRow::new(
                id("row1"),
                vec![
                    TableCell::new(id("cell1"), vec![paragraph("p1", "Heading")]),
                    TableCell::new(
                        id("cell2"),
                        vec![Paragraph::new(
                            id("p2"),
                            vec![mixed, TextRun::new(" plain")],
                        )],
                    ),
                ],
            ),
            TableRow::new(
                id("row2"),
                vec![
                    TableCell::new(
                        id("cell3"),
                        vec![paragraph("p3", "Line one"), paragraph("p4", "Line two")],
                    ),
                    TableCell::new(id("cell4"), vec![paragraph("p5", "")]),
                ],
            ),
        ],
    )
    .with_column_widths(vec![Twips(3000), Twips(3600)]);
    Document::from_parts(
        id("table-document"),
        DocumentMetadata {
            title: "Table — café".into(),
        },
        vec![Section::new(
            id("section"),
            vec![
                Block::Paragraph(paragraph("before", "Before")),
                Block::Table(table),
                Block::Paragraph(paragraph("after", "After")),
            ],
        )],
        vec![],
    )
    .unwrap()
}
fn editable(bytes: &[u8]) -> ImportedDocx {
    match import_docx(bytes, id("opened-table")).unwrap() {
        DocxImportOutcome::Editable(value) => value,
        other => panic!("expected editable table, got {other:?}"),
    }
}
fn xml(bytes: &[u8], name: &str) -> String {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut result = String::new();
    archive
        .by_name(name)
        .unwrap()
        .read_to_string(&mut result)
        .unwrap();
    result
}
fn replace(bytes: &[u8], name: &str, update: impl FnOnce(String) -> String) -> Vec<u8> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut parts = BTreeMap::new();
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).unwrap();
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        parts.insert(file.name().unwrap().to_string(), bytes);
    }
    let current = String::from_utf8(parts.remove(name).unwrap()).unwrap();
    parts.insert(name.into(), update(current).into_bytes());
    let mut output = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in parts {
        output
            .start_file(name, SimpleFileOptions::default())
            .unwrap();
        output.write_all(&bytes).unwrap();
    }
    output.finish().unwrap().into_inner()
}

#[test]
fn mixed_unicode_table_round_trip_keeps_cells_paragraph_order_widths_and_links() {
    let original = document();
    let bytes = export_docx(&original).unwrap();
    assert_eq!(bytes, export_docx(&original).unwrap());
    let text = xml(&bytes, "word/document.xml");
    assert!(text.contains("<w:tbl>"));
    assert_eq!(text.matches("<w:tc>").count(), 4);
    let imported = editable(&bytes);
    assert_eq!(imported.original().original_bytes(), bytes);
    let Block::Table(table) = &imported.document().sections()[0].blocks()[1] else {
        panic!("table missing")
    };
    assert_eq!(
        table.column_widths(),
        Some([Twips(3000), Twips(3600)].as_slice())
    );
    assert_eq!(table.rows().len(), 2);
    assert_eq!(table.rows()[1].cells()[0].paragraphs().len(), 2);
    let expected: Vec<_> = original
        .paragraphs()
        .map(|paragraph| (paragraph.runs(), paragraph.format()))
        .collect();
    let actual: Vec<_> = imported
        .document()
        .paragraphs()
        .map(|paragraph| (paragraph.runs(), paragraph.format()))
        .collect();
    assert_eq!(actual, expected);
    assert!(
        !imported.report().has_formatting_normalizations,
        "canonical output should not disclose nonexistent formatting loss"
    );
    let regenerated = export_docx(imported.document()).unwrap();
    let second = editable(&regenerated);
    assert_eq!(
        second
            .document()
            .paragraphs()
            .map(|paragraph| paragraph.runs())
            .collect::<Vec<_>>(),
        imported
            .document()
            .paragraphs()
            .map(|paragraph| paragraph.runs())
            .collect::<Vec<_>>()
    );
}

#[test]
fn cell_edits_use_existing_unicode_transactions_and_export_the_current_content() {
    let bytes = export_docx(&document()).unwrap();
    let imported = editable(&bytes);
    let mut session = DocumentSession::new(imported.document().clone());
    let cell_paragraph = session.document().paragraphs().nth(2).unwrap().id().clone();
    let mut transaction = session.begin_transaction();
    let at = transaction
        .position(&cell_paragraph, GraphemeOffset::new(1))
        .unwrap();
    transaction
        .insert_text(&at, " inserted 👨‍👩‍👧‍👦 ", CharacterFormat::default())
        .unwrap();
    session.commit(transaction).unwrap();
    let output = export_docx(session.document()).unwrap();
    let reopened = editable(&output);
    assert_eq!(
        session
            .document()
            .paragraphs()
            .map(|paragraph| paragraph.runs())
            .collect::<Vec<_>>(),
        reopened
            .document()
            .paragraphs()
            .map(|paragraph| paragraph.runs())
            .collect::<Vec<_>>()
    );
    assert_eq!(imported.original().original_bytes(), bytes);
}

#[test]
fn merged_nested_and_unknown_table_content_never_yields_an_editable_partial_document() {
    let bytes = export_docx(&document()).unwrap();
    for replacement in [
        "<w:gridSpan w:val=\"2\"/>",
        "<w:vMerge w:val=\"restart\"/>",
        "<w:unknownContent/>",
        "<w:cellIns w:id=\"1\"/>",
    ] {
        let input = replace(&bytes, "word/document.xml", |text| {
            text.replacen("<w:tcPr>", &format!("<w:tcPr>{replacement}"), 1)
        });
        match import_docx(&input, id("blocked")).unwrap() {
            DocxImportOutcome::Preserved { original, report } => {
                assert_eq!(original.original_bytes(), input);
                assert!(!report.is_supported());
            }
            other => panic!("unsupported cell property became editable: {replacement}: {other:?}"),
        }
    }
    let nested = replace(&bytes, "word/document.xml", |text| {
        text.replacen("</w:tcPr>",
        "</w:tcPr><w:tbl><w:tr><w:tc><w:p><w:r><w:t>Nested content</w:t></w:r></w:p></w:tc></w:tr></w:tbl>", 1)
    });
    assert!(matches!(
        import_docx(&nested, id("nested")).unwrap(),
        DocxImportOutcome::Preserved { .. }
    ));
}

#[test]
fn malformed_table_structure_and_duplicate_properties_are_rejected_explicitly() {
    let bytes = export_docx(&document()).unwrap();
    for input in [
        replace(&bytes, "word/document.xml", |text| {
            text.replacen("<w:tcPr>", "<w:tcPr/><w:tcPr>", 1)
        }),
        replace(&bytes, "word/document.xml", |text| {
            text.replacen("<w:gridCol w:w=\"3000\"/>", "<w:gridCol w:w=\"0\"/>", 1)
        }),
        replace(&bytes, "word/document.xml", |text| {
            text.replacen("<w:tblGrid>", "<w:tblGrid><w:gridCol w:w=\"800\"/>", 1)
        }),
    ] {
        assert!(matches!(
            import_docx(&input, id("invalid")),
            Err(ConversionError::InvalidDocument { .. })
        ));
    }
}

#[test]
fn known_table_appearance_is_disclosed_without_discarding_supported_cell_content() {
    let bytes = export_docx(&document()).unwrap();
    let input = replace(&bytes, "word/document.xml", |text| {
        text.replace("w:color=\"808080\"", "w:color=\"FF0000\"")
    });
    let imported = editable(&input);
    assert!(imported.report().has_formatting_normalizations);
    assert!(
        imported
            .report()
            .normalizations
            .iter()
            .any(|item| item.path == "tblPr/tblBorders")
    );
    assert_eq!(
        imported
            .document()
            .paragraphs()
            .map(|paragraph| paragraph.runs())
            .collect::<Vec<_>>(),
        document()
            .paragraphs()
            .map(|paragraph| paragraph.runs())
            .collect::<Vec<_>>()
    );
    assert_eq!(imported.original().original_bytes(), input);
}

#[test]
fn nonfinal_section_ending_at_a_table_returns_an_export_error_without_added_content() {
    let mut sections = document().sections().to_vec();
    let first = sections.remove(0);
    let blocks = first.blocks()[..2].to_vec();
    let value = Document::from_parts(
        id("boundary"),
        DocumentMetadata::default(),
        vec![
            Section::new(id("first"), blocks),
            Section::new(
                id("second"),
                vec![Block::Paragraph(paragraph("final", "Final section"))],
            ),
        ],
        vec![],
    )
    .unwrap();
    assert!(matches!(
        export_docx(&value),
        Err(ConversionError::UnsupportedExport { .. })
    ));
    value.validate().unwrap();
}

#[test]
fn unresolved_huge_margins_and_tiny_body_widths_fail_without_panicking_or_zero_width_columns() {
    use office_core::{PageMargins, PageSize, SectionProperties};
    let original = document();
    let Block::Table(table) = &original.sections()[0].blocks()[1] else {
        panic!("table missing")
    };
    let automatic = Table::new(table.id().clone(), table.rows().to_vec());
    let huge = SectionProperties {
        page_size: None,
        page_margins: Some(PageMargins {
            top: Twips(0),
            bottom: Twips(0),
            left: Twips(i32::MAX),
            right: Twips(i32::MAX),
            gutter: Twips(i32::MAX),
            header: Twips(0),
            footer: Twips(0),
        }),
    };
    let tiny = SectionProperties {
        page_size: Some(PageSize {
            width: Twips(3),
            height: Twips(1000),
            orientation: None,
        }),
        page_margins: Some(PageMargins {
            top: Twips(1),
            bottom: Twips(1),
            left: Twips(1),
            right: Twips(1),
            gutter: Twips(0),
            header: Twips(0),
            footer: Twips(0),
        }),
    };
    for properties in [huge, tiny] {
        let value = Document::from_parts(
            id("geometry"),
            DocumentMetadata::default(),
            vec![
                Section::new(
                    id("geometry-section"),
                    vec![Block::Table(automatic.clone())],
                )
                .with_properties(properties),
            ],
            vec![],
        )
        .unwrap();
        let result = std::panic::catch_unwind(|| export_docx(&value));
        assert!(matches!(
            result,
            Ok(Err(ConversionError::UnsupportedExport { .. }))
        ));
        value.validate().unwrap();
    }
}

#[test]
fn table_numbering_instances_restart_per_cell_and_cannot_resume_across_cells() {
    use office_core::ListKind;
    let value = Document::from_parts(
        id("lists"),
        DocumentMetadata::default(),
        vec![Section::new(
            id("list-section"),
            vec![Block::Table(Table::new(
                id("list-table"),
                vec![TableRow::new(
                    id("list-row"),
                    vec![
                        TableCell::new(
                            id("list-cell-a"),
                            vec![
                                paragraph("list-a", "First cell")
                                    .with_list(Some(ListKind::Numbered)),
                            ],
                        ),
                        TableCell::new(
                            id("list-cell-b"),
                            vec![
                                paragraph("list-b", "Second cell")
                                    .with_list(Some(ListKind::Numbered)),
                            ],
                        ),
                    ],
                )],
            ))],
        )],
        vec![],
    )
    .unwrap();
    let bytes = export_docx(&value).unwrap();
    let imported = editable(&bytes);
    assert!(
        imported
            .document()
            .paragraphs()
            .all(|paragraph| paragraph.list() == Some(ListKind::Numbered))
    );
    let resumed = replace(&bytes, "word/document.xml", |text| {
        text.replace("<w:numId w:val=\"2\"/>", "<w:numId w:val=\"1\"/>")
    });
    assert!(matches!(
        import_docx(&resumed, id("resumed")).unwrap(),
        DocxImportOutcome::Preserved { .. }
    ));
}
