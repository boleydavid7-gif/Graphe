//! Table format roundtrips and explicit loss/preservation boundaries.
use office_core::{
    Block, CharacterFormat, Document, DocumentMetadata, Id, Paragraph, Section, Table, TableCell,
    TableRow, TextRun, Twips, Underline,
};
use office_formats::{
    DocumentFormat, ImportOutcome, assess_export, export_document, import_document,
};
use std::io::{Cursor, Read, Write};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

fn id(value: &str) -> Id {
    Id::new(value).unwrap()
}
fn sample() -> Document {
    let paragraphs = |prefix: &str, texts: &[&str]| {
        texts
            .iter()
            .enumerate()
            .map(|(n, text)| {
                let mut run = TextRun::new(*text);
                run.format = CharacterFormat {
                    bold: Some(n == 0),
                    italic: Some(true),
                    underline: Some(Underline::Single),
                    ..CharacterFormat::default()
                };
                Paragraph::new(id(&format!("{prefix}-p{n}")), vec![run])
            })
            .collect()
    };
    let table = Table::new(
        id("table"),
        vec![
            TableRow::new(
                id("row0"),
                vec![
                    TableCell::new(
                        id("cell00"),
                        paragraphs("a", &["Heading 👩🏽‍💻", "e\u{301} / العربية"]),
                    ),
                    TableCell::new(id("cell01"), paragraphs("b", &["Second"])),
                ],
            ),
            TableRow::new(
                id("row1"),
                vec![
                    TableCell::new(id("cell10"), paragraphs("c", &["Third"])),
                    TableCell::new(id("cell11"), paragraphs("d", &[""])),
                ],
            ),
        ],
    )
    .with_column_widths(vec![Twips(3000), Twips(3000)]);
    Document::from_parts(
        id("document"),
        DocumentMetadata {
            title: "Tables & Unicode".into(),
        },
        vec![Section::new(
            id("section"),
            vec![
                Block::Table(table),
                Block::Paragraph(Paragraph::new(
                    id("after"),
                    vec![TextRun::new("After the table")],
                )),
            ],
        )],
        vec![],
    )
    .unwrap()
}
fn editable(bytes: &[u8]) -> Document {
    match import_document(DocumentFormat::Odt, bytes, id("imported"), "table.odt").unwrap() {
        ImportOutcome::Editable(file) => {
            assert_eq!(file.original_source().unwrap().bytes(), bytes);
            file.into_parts().0
        }
        other => panic!("expected editable: {other:?}"),
    }
}
fn change_content(bytes: &[u8], f: impl FnOnce(String) -> String) -> Vec<u8> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut entries = Vec::new();
    for n in 0..archive.len() {
        let mut file = archive.by_index(n).unwrap();
        let mut body = Vec::new();
        file.read_to_end(&mut body).unwrap();
        entries.push((file.name().unwrap().to_string(), body));
    }
    let content = entries
        .iter_mut()
        .find(|(name, _)| name == "content.xml")
        .unwrap();
    content.1 = f(String::from_utf8(std::mem::take(&mut content.1)).unwrap()).into_bytes();
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, body) in entries {
        writer
            .start_file(
                &name,
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(&body).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

#[test]
fn odt_tables_preserve_rows_columns_widths_cell_paragraphs_runs_and_unicode() {
    let original = sample();
    let bytes = export_document(DocumentFormat::Odt, &original).unwrap();
    assert_eq!(
        bytes,
        export_document(DocumentFormat::Odt, &original).unwrap()
    );
    let restored = editable(&bytes);
    let Block::Table(table) = &restored.sections()[0].blocks()[0] else {
        panic!("table missing")
    };
    assert_eq!(table.rows().len(), 2);
    assert_eq!(table.rows()[0].cells().len(), 2);
    assert_eq!(table.rows()[0].cells()[0].paragraphs().len(), 2);
    assert_eq!(
        table.column_widths(),
        Some([Twips(3000), Twips(3000)].as_slice())
    );
    assert_eq!(restored.metadata(), original.metadata());
    let text = |document: &Document| {
        document
            .paragraphs()
            .map(|p| p.runs().iter().map(|r| r.text.as_str()).collect::<String>())
            .collect::<Vec<_>>()
    };
    assert_eq!(text(&restored), text(&original));
    for (actual, expected) in restored.paragraphs().zip(original.paragraphs()) {
        let actual = office_layout::resolve_paragraph_style(&restored, actual);
        let expected = office_layout::resolve_paragraph_style(&original, expected);
        assert_eq!(actual.paragraph.alignment, expected.paragraph.alignment);
    }
    for (actual, expected) in restored.paragraphs().zip(original.paragraphs()) {
        for (actual, expected) in actual.runs().iter().zip(expected.runs()) {
            assert_eq!(actual.text, expected.text);
            assert_eq!(
                actual.format.bold.unwrap_or(false),
                expected.format.bold.unwrap_or(false)
            );
            assert_eq!(
                actual.format.italic.unwrap_or(false),
                expected.format.italic.unwrap_or(false)
            );
            assert_eq!(
                actual.format.underline.unwrap_or(Underline::None),
                expected.format.underline.unwrap_or(Underline::None)
            );
        }
    }
    let second = editable(&export_document(DocumentFormat::Odt, &restored).unwrap());
    assert_eq!(text(&second), text(&original));
}

#[test]
fn odt_merged_nested_formulas_and_unknown_table_content_remain_preserved() {
    let bytes = export_document(DocumentFormat::Odt, &sample()).unwrap();
    for (before, after) in [
        (
            "<table:table-cell table:style-name=\"GridCell\"",
            "<table:table-cell table:number-columns-spanned=\"2\" table:style-name=\"GridCell\"",
        ),
        (
            "<table:table-cell table:style-name=\"GridCell\"",
            "<table:table-cell table:formula=\"of:=1+1\" table:style-name=\"GridCell\"",
        ),
        (
            "</table:table-cell>",
            "<table:table><table:table-row/></table:table></table:table-cell>",
        ),
        (
            "<table:table-row>",
            "<table:table-row table:number-rows-repeated=\"2\">",
        ),
    ] {
        let changed = change_content(&bytes, |text| text.replacen(before, after, 1));
        match import_document(
            DocumentFormat::Odt,
            &changed,
            id("guarded"),
            "unsupported.odt",
        )
        .unwrap()
        {
            ImportOutcome::Preserved { original, report } => {
                assert_eq!(original.bytes(), changed);
                assert!(!report.is_empty());
            }
            other => panic!("unsupported table became editable: {other:?}"),
        }
    }
}

#[test]
fn text_table_conversion_is_deliberate_and_warned_and_rtf_is_loss_refusing() {
    let document = sample();
    assert!(
        assess_export(DocumentFormat::Txt, &document)
            .unwrap()
            .warnings
            .iter()
            .any(|w| w.code == "tables_as_text")
    );
    let bytes = export_document(DocumentFormat::Txt, &document).unwrap();
    assert_eq!(
        String::from_utf8(bytes).unwrap(),
        "Heading 👩🏽‍💻\ne\u{301} / العربية\tSecond\nThird\t\nAfter the table"
    );
    assert_eq!(
        assess_export(DocumentFormat::Rtf, &document)
            .unwrap_err()
            .code,
        "unsupported_rtf_export"
    );
    assert_eq!(
        export_document(DocumentFormat::Rtf, &document)
            .unwrap_err()
            .code,
        "unsupported_rtf_export"
    );
}

#[test]
fn pdf_has_table_cell_border_rectangles_and_same_page_count_as_rust() {
    let document = sample();
    let layout =
        office_layout::layout_document(&document, &office_layout::LayoutOptions::default())
            .unwrap();
    assert_eq!(
        layout
            .pages
            .iter()
            .map(|p| p.table_cells.len())
            .sum::<usize>(),
        4
    );
    let bytes = export_document(DocumentFormat::Pdf, &document).unwrap();
    assert_eq!(
        bytes,
        export_document(DocumentFormat::Pdf, &document).unwrap()
    );
    assert!(bytes.windows(3).any(|part| part == b" re"));
    assert!(bytes.windows(2).any(|part| part == b"\nS"));
    let count = bytes
        .windows(11)
        .filter(|part| part.starts_with(b"/Type/Page") && part.get(10) != Some(&b's'))
        .count();
    // Use the page object reference marker (Pages uses a trailing s).
    assert_eq!(count, layout.pages.len());
}

#[test]
fn realistic_table_import_edit_native_save_export_reopen_preserves_content_and_original() {
    use office_core::{
        DocumentSession, GraphemeOffset, WriterFile, deserialize_writer_file, serialize_writer_file,
    };
    let native =
        deserialize_writer_file(include_bytes!("fixtures/project-review-table.writer")).unwrap();
    for format in [DocumentFormat::Docx, DocumentFormat::Odt] {
        let bytes = export_document(format, native.document()).unwrap();
        let file = match import_document(
            format,
            &bytes,
            id("workflow-import"),
            &format!("review.{}", format.extension()),
        )
        .unwrap()
        {
            ImportOutcome::Editable(file) => file,
            other => panic!("expected editable table: {other:?}"),
        };
        let original = file.original_source().unwrap().clone();
        let table = file.document().sections()[0]
            .blocks()
            .iter()
            .find_map(|b| {
                if let Block::Table(t) = b {
                    Some(t)
                } else {
                    None
                }
            })
            .unwrap();
        let paragraph_id = table.rows()[1].cells()[0].paragraphs()[0].id().clone();
        let mut session = DocumentSession::new(file.document().clone());
        let mut transaction = session.begin_transaction();
        let start = transaction
            .position(&paragraph_id, GraphemeOffset::new(0))
            .unwrap();
        transaction
            .insert_text(&start, "Reviewed ", CharacterFormat::default())
            .unwrap();
        session.commit(transaction).unwrap();
        let edited =
            WriterFile::with_original_source(session.document().clone(), original.clone()).unwrap();
        let saved = serialize_writer_file(&edited).unwrap();
        let reopened = deserialize_writer_file(&saved).unwrap();
        assert_eq!(
            reopened, edited,
            "native identities and all structure persist"
        );
        assert_eq!(reopened.original_source().unwrap(), &original);
        assert_eq!(original.bytes(), bytes);
        let external = export_document(format, reopened.document()).unwrap();
        let restored = match import_document(
            format,
            &external,
            id("workflow-reopen"),
            &format!("edited.{}", format.extension()),
        )
        .unwrap()
        {
            ImportOutcome::Editable(file) => file,
            other => panic!("edited table not editable: {other:?}"),
        };
        let texts = |d: &Document| {
            d.paragraphs()
                .map(|p| p.runs().iter().map(|r| r.text.as_str()).collect::<String>())
                .collect::<Vec<_>>()
        };
        assert_eq!(texts(restored.document()), texts(reopened.document()));
        let tables = restored
            .document()
            .sections()
            .iter()
            .flat_map(|s| s.blocks())
            .filter_map(|b| {
                if let Block::Table(t) = b {
                    Some(t)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].rows().len(), 36);
        assert!(tables[0].rows().iter().all(|r| r.cells().len() == 2));
        assert!(
            texts(restored.document())
                .iter()
                .any(|text| text.starts_with("Reviewed Phase 01"))
        );
    }
}

#[test]
fn odt_empty_named_cell_styles_disclose_grid_and_padding_normalization() {
    let bytes = export_document(DocumentFormat::Odt, &sample()).unwrap();
    for replacement in [
        "",
        "<style:table-cell-properties/>",
        "<style:table-cell-properties fo:border=\"0.5pt solid #808080\"/>",
    ] {
        let changed = change_content(&bytes, |text| {
            text.replace("<style:table-cell-properties fo:border=\"0.5pt solid #808080\" fo:padding=\"4pt\"/>",replacement)
        });
        let report = office_formats::import_document_with_warnings(
            DocumentFormat::Odt,
            &changed,
            id("normalized-table"),
            "normalized.odt",
        )
        .unwrap();
        assert!(report.has_formatting_normalizations);
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.message.contains("grid and padding"))
        );
        match report.outcome {
            ImportOutcome::Editable(file) => {
                assert_eq!(file.original_source().unwrap().bytes(), changed)
            }
            other => panic!("inert appearance defaults blocked: {other:?}"),
        }
    }
}
