//! Tables preserve semantic cell boundaries through edits, native files and history.
use office_core::{
    Block, CharacterFormat, CharacterFormatPatch, Document, DocumentMetadata, DocumentSession,
    EditError, GraphemeOffset, Id, OriginalSource, Paragraph, PropertyChange, SCHEMA_VERSION,
    Section, Table, TableCell, TableRow, TextRun, Twips, ValidationError, WRITER_FORMAT_VERSION,
    WriterFile, deserialize_writer_file, serialize_writer_file,
};
use serde_json::json;

fn id(value: &str) -> Id {
    Id::new(value).unwrap()
}
fn paragraph(name: &str, text: &str) -> Paragraph {
    Paragraph::new(id(name), vec![TextRun::new(text)])
}
fn cell(name: &str, text: &str) -> TableCell {
    TableCell::new(id(name), vec![paragraph(&format!("{name}-p"), text)])
}
fn table() -> Table {
    Table::new(
        id("table"),
        vec![
            TableRow::new(
                id("row-a"),
                vec![cell("a1", "Cafe\u{301} 👨‍👩‍👧‍👦"), cell("a2", "second")],
            ),
            TableRow::new(id("row-b"), vec![cell("b1", "العربية"), cell("b2", "last")]),
        ],
    )
}
fn document_with(table: Table) -> Result<Document, ValidationError> {
    Document::from_parts(
        id("doc"),
        DocumentMetadata {
            title: "Table notes".into(),
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
}
fn document() -> Document {
    document_with(table()).unwrap()
}
fn text(document: &Document, name: &str) -> String {
    document.text_coordinates(&id(name)).unwrap().text().into()
}
fn selected(
    tx: &office_core::Transaction,
    a: (&str, usize),
    b: (&str, usize),
) -> office_core::EditSelection {
    let a = tx.position(&id(a.0), GraphemeOffset::new(a.1)).unwrap();
    let b = tx.position(&id(b.0), GraphemeOffset::new(b.1)).unwrap();
    tx.selection(a, b).unwrap()
}

#[test]
fn table_native_roundtrip_retains_all_ids_unicode_formatting_and_exact_original() {
    let mut value = serde_json::to_value(document()).unwrap();
    value["sections"][0]["blocks"][1]["content"]["rows"][0]["cells"][0]["paragraphs"][0]["runs"]
        [0]["format"]["bold"] = json!(true);
    let document = Document::from_json(&value.to_string()).unwrap();
    let source = OriginalSource::new(
        "docx",
        "application/octet-stream",
        "original.docx",
        b"retained exact source".as_slice(),
    )
    .unwrap();
    let file = WriterFile::with_original_source(document, source).unwrap();
    let bytes = serialize_writer_file(&file).unwrap();
    assert_eq!(deserialize_writer_file(&bytes).unwrap(), file);
    assert_eq!(serialize_writer_file(&file).unwrap(), bytes);
    assert_eq!(SCHEMA_VERSION, 6);
    assert_eq!(WRITER_FORMAT_VERSION, 6);
}

#[test]
fn schema_four_migration_keeps_links_but_rejects_tables_under_old_version() {
    let mut value = serde_json::to_value(document()).unwrap();
    value["schema_version"] = json!(4);
    assert!(Document::from_json(&value.to_string()).is_err());
    value["sections"][0]["blocks"]
        .as_array_mut()
        .unwrap()
        .remove(1);
    value["sections"][0]["blocks"][0]["content"]["runs"][0]["format"]["hyperlink"] =
        json!("https://example.com");
    let migrated = Document::from_json(&value.to_string()).unwrap();
    assert_eq!(migrated.schema_version(), SCHEMA_VERSION);
    assert_eq!(
        migrated.paragraphs().next().unwrap().runs()[0]
            .format
            .hyperlink
            .as_deref(),
        Some("https://example.com")
    );
}

#[test]
fn old_native_envelopes_cannot_smuggle_new_table_schema() {
    let bytes = serialize_writer_file(&WriterFile::new(document())).unwrap();
    let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    value["format_version"] = json!(4);
    assert!(deserialize_writer_file(value.to_string().as_bytes()).is_err());
    value["document"]["schema_version"] = json!(4);
    assert!(deserialize_writer_file(value.to_string().as_bytes()).is_err());
}

#[test]
fn table_dimensions_empty_cells_and_widths_are_validated_without_repair() {
    let invalid = [
        Table::new(id("t"), vec![]),
        Table::new(id("t"), vec![TableRow::new(id("r"), vec![])]),
        Table::new(
            id("t"),
            vec![TableRow::new(
                id("r"),
                vec![TableCell::new(id("c"), vec![])],
            )],
        ),
        Table::new(
            id("t"),
            vec![
                TableRow::new(id("r"), vec![cell("c", "")]),
                TableRow::new(id("r2"), vec![cell("c2", ""), cell("c3", "")]),
            ],
        ),
        table().with_column_widths(vec![Twips(1440)]),
        table().with_column_widths(vec![Twips(1440), Twips(0)]),
        table().with_column_widths(vec![Twips(-1), Twips(1440)]),
        table().with_column_widths(vec![Twips(i32::MAX), Twips(i32::MAX)]),
    ];
    for invalid in invalid {
        assert!(document_with(invalid).is_err());
    }
    assert!(document_with(table().with_column_widths(vec![Twips(1440), Twips(2880)])).is_ok());
}

#[test]
fn table_cell_count_and_row_column_budgets_are_bounded() {
    let too_many_columns = Table::new(
        id("t"),
        vec![TableRow::new(
            id("r"),
            (0..33)
                .map(|column| cell(&format!("c{column}"), ""))
                .collect(),
        )],
    );
    assert!(document_with(too_many_columns).is_err());
    let too_many_rows = Table::new(
        id("t"),
        (0..1001)
            .map(|row| TableRow::new(id(&format!("r{row}")), vec![cell(&format!("c{row}"), "")]))
            .collect(),
    );
    assert!(document_with(too_many_rows).is_err());
    let too_many_cells = Table::new(
        id("t"),
        (0..129)
            .map(|row| {
                TableRow::new(
                    id(&format!("r{row}")),
                    (0..32)
                        .map(|column| cell(&format!("c{row}-{column}"), ""))
                        .collect(),
                )
            })
            .collect(),
    );
    assert!(document_with(too_many_cells).is_err());
}

#[test]
fn table_entities_share_the_document_wide_unique_identifier_namespace() {
    let value = serde_json::to_value(document()).unwrap();
    for pointer in [
        "/sections/0/blocks/1/content/id",
        "/sections/0/blocks/1/content/rows/0/id",
        "/sections/0/blocks/1/content/rows/0/cells/0/id",
        "/sections/0/blocks/1/content/rows/0/cells/0/paragraphs/0/id",
    ] {
        let mut invalid = value.clone();
        *invalid.pointer_mut(pointer).unwrap() = json!("doc");
        assert!(matches!(
            Document::from_json(&invalid.to_string()),
            Err(office_core::DocumentError::Validation(
                ValidationError::DuplicateId { .. }
            ))
        ));
    }
}

#[test]
fn table_paragraphs_obey_existing_strict_styles_properties_and_unknown_fields() {
    let value = serde_json::to_value(document()).unwrap();
    for (field, property) in [("style", json!("missing-style")), ("unknown", json!(true))] {
        let mut invalid = value.clone();
        invalid["sections"][0]["blocks"][1]["content"]["rows"][0]["cells"][0]["paragraphs"][0]
            [field] = property;
        assert!(Document::from_json(&invalid.to_string()).is_err());
    }
}

#[test]
fn paragraph_traversal_and_positions_follow_logical_row_major_order() {
    let document = document();
    let ids: Vec<_> = document.paragraphs().map(|p| p.id().as_str()).collect();
    assert_eq!(ids, ["before", "a1-p", "a2-p", "b1-p", "b2-p", "after"]);
    let (table, row, column) = document.table_for_paragraph(&id("b1-p")).unwrap();
    assert_eq!((table.id().as_str(), row, column), ("table", 1, 0));
    assert!(document.table_for_paragraph(&id("before")).is_none());
    for pair in ids.windows(2) {
        let a = document
            .position(&id(pair[0]), GraphemeOffset::new(0))
            .unwrap();
        let b = document
            .position(&id(pair[1]), GraphemeOffset::new(0))
            .unwrap();
        assert!(document.compare_positions(&a, &b).unwrap().is_lt());
    }
}

#[test]
fn unicode_text_edits_and_paragraph_breaks_work_within_cell_and_undo_exactly() {
    let initial = document();
    let mut session = DocumentSession::new(initial.clone());
    let mut tx = session.begin_transaction();
    let at = tx.position(&id("a1-p"), GraphemeOffset::new(4)).unwrap();
    tx.insert_text(&at, "!", CharacterFormat::default())
        .unwrap();
    let selection = selected(&tx, ("a1-p", 5), ("a1-p", 5));
    tx.insert_paragraph_break(&selection, id("new-cell-p"))
        .unwrap();
    session.commit(tx).unwrap();
    assert_eq!(text(session.document(), "a1-p"), "Cafe\u{301}!");
    assert_eq!(text(session.document(), "new-cell-p"), " 👨‍👩‍👧‍👦");
    assert_eq!(
        session
            .document()
            .table_for_paragraph(&id("new-cell-p"))
            .unwrap()
            .1,
        0
    );
    session.undo().unwrap();
    assert_eq!(session.document(), &initial);
    session.redo().unwrap();
    assert_eq!(text(session.document(), "new-cell-p"), " 👨‍👩‍👧‍👦");
}

#[test]
fn text_replacement_cannot_erase_cell_boundaries_or_intervening_tables() {
    let initial = document();
    for (a, b) in [
        (("a1-p", 0), ("a2-p", 2)),
        (("before", 0), ("after", 1)),
        (("before", 0), ("a1-p", 0)),
        (("b2-p", 0), ("after", 1)),
    ] {
        let mut session = DocumentSession::new(initial.clone());
        let mut tx = session.begin_transaction();
        let selection = selected(&tx, a, b);
        assert_eq!(tx.delete_text(&selection), Err(EditError::CrossCellRange));
        assert_eq!(
            tx.replace_text(&selection, "replacement", CharacterFormat::default()),
            Err(EditError::CrossCellRange)
        );
        assert!(!session.commit(tx).unwrap().changed());
        assert_eq!(session.document(), &initial);
        assert!(!session.can_undo());
    }
}

#[test]
fn paragraph_merge_within_cell_keeps_cell_and_table_structure() {
    let mut value = serde_json::to_value(document()).unwrap();
    let second = serde_json::to_value(paragraph("cell-second", "tail")).unwrap();
    value["sections"][0]["blocks"][1]["content"]["rows"][0]["cells"][0]["paragraphs"]
        .as_array_mut()
        .unwrap()
        .push(second);
    let mut session = DocumentSession::new(Document::from_json(&value.to_string()).unwrap());
    let mut tx = session.begin_transaction();
    let selection = selected(&tx, ("a1-p", 4), ("cell-second", 2));
    tx.delete_text(&selection).unwrap();
    session.commit(tx).unwrap();
    assert_eq!(text(session.document(), "a1-p"), "Cafe\u{301}il");
    let (table, _, _) = session.document().table_for_paragraph(&id("a1-p")).unwrap();
    assert_eq!(table.rows()[0].cells()[0].paragraphs().len(), 1);
    assert_eq!(table.rows().len(), 2);
}

#[test]
fn formatting_across_cells_changes_text_properties_without_changing_structure() {
    let mut session = DocumentSession::new(document());
    let mut tx = session.begin_transaction();
    let selection = selected(&tx, ("a1-p", 0), ("b2-p", 4));
    tx.apply_character_format(
        &selection,
        &CharacterFormatPatch {
            bold: PropertyChange::Set(true),
            ..Default::default()
        },
    )
    .unwrap();
    session.commit(tx).unwrap();
    for p in session
        .document()
        .paragraphs()
        .filter(|p| p.id().as_str() != "before" && p.id().as_str() != "after")
    {
        assert!(p.runs().iter().all(|run| run.format.bold == Some(true)));
    }
    assert_eq!(session.document().sections()[0].blocks().len(), 3);
}

#[test]
fn insertion_row_column_removal_and_table_delete_are_atomic_history_steps() {
    let initial = document();
    let mut session = DocumentSession::new(initial.clone());
    let mut tx = session.begin_transaction();
    tx.insert_table_row(
        &id("table"),
        1,
        TableRow::new(id("row-new"), vec![cell("n1", "new"), cell("n2", "")]),
    )
    .unwrap();
    tx.insert_table_column(
        &id("table"),
        1,
        vec![cell("new-a", ""), cell("new-n", ""), cell("new-b", "")],
    )
    .unwrap();
    session.commit(tx).unwrap();
    let changed = session.document().clone();
    let (table, _, _) = changed.table_for_paragraph(&id("a1-p")).unwrap();
    assert_eq!((table.rows().len(), table.rows()[0].cells().len()), (3, 3));
    session.undo().unwrap();
    assert_eq!(session.document(), &initial);
    session.redo().unwrap();
    assert_eq!(session.document(), &changed);
    let mut tx = session.begin_transaction();
    tx.remove_table_column(&id("table"), 1).unwrap();
    tx.remove_table_row(&id("table"), 1).unwrap();
    session.commit(tx).unwrap();
    assert_eq!(session.document(), &initial);
    let mut tx = session.begin_transaction();
    tx.remove_table(&id("table")).unwrap();
    session.commit(tx).unwrap();
    assert_eq!(session.document().paragraphs().count(), 2);
    session.undo().unwrap();
    assert_eq!(session.document(), &initial);
}

#[test]
fn failed_structural_commands_preserve_candidate_endpoints_and_history() {
    let initial = document();
    let mut session = DocumentSession::new(initial.clone());
    let mut tx = session.begin_transaction();
    let original = tx.draft().clone();
    let at = tx.position(&id("a1-p"), GraphemeOffset::new(0)).unwrap();
    assert!(
        tx.insert_table_row(
            &id("table"),
            0,
            TableRow::new(id("duplicate-row"), vec![cell("bad", "")])
        )
        .is_err()
    );
    assert_eq!(tx.draft(), &original);
    assert!(
        tx.insert_table_after(&id("a1-p"), Table::new(id("nested"), vec![]))
            .is_err()
    );
    assert!(tx.remove_table_row(&id("table"), usize::MAX).is_err());
    assert!(tx.remove_table_column(&id("missing"), 0).is_err());
    assert!(tx.insert_table_column(&id("table"), 0, vec![]).is_err());
    tx.insert_text(&at, "", CharacterFormat::default()).unwrap();
    assert!(!session.commit(tx).unwrap().changed());
    assert!(!session.can_undo());
}

#[test]
fn insertion_after_body_paragraph_and_duplicate_identity_rejection() {
    let initial = Document::from_parts(
        id("doc"),
        DocumentMetadata::default(),
        vec![Section::new(
            id("section"),
            vec![Block::Paragraph(paragraph("before", "body"))],
        )],
        vec![],
    )
    .unwrap();
    let mut session = DocumentSession::new(initial.clone());
    let mut tx = session.begin_transaction();
    tx.insert_table_after(&id("before"), table()).unwrap();
    session.commit(tx).unwrap();
    assert_eq!(session.document().sections()[0].blocks().len(), 2);
    let mut tx = session.begin_transaction();
    assert!(tx.insert_table_after(&id("before"), table()).is_err());
    let p = selected(&tx, ("a1-p", 0), ("a1-p", 0));
    assert!(tx.insert_paragraph_break(&p, id("row-a")).is_err());
    assert!(!session.commit(tx).unwrap().changed());
    session.undo().unwrap();
    assert_eq!(session.document(), &initial);
}

#[test]
fn final_row_and_column_need_explicit_table_deletion() {
    let single = Table::new(
        id("single"),
        vec![TableRow::new(id("single-r"), vec![cell("single-c", "")])],
    );
    let mut session = DocumentSession::new(document_with(single).unwrap());
    let mut tx = session.begin_transaction();
    assert!(tx.remove_table_row(&id("single"), 0).is_err());
    assert!(tx.remove_table_column(&id("single"), 0).is_err());
    assert!(!session.commit(tx).unwrap().changed());
}

#[test]
fn column_edits_retain_existing_explicit_widths_and_other_cells() {
    let mut session = DocumentSession::new(
        document_with(table().with_column_widths(vec![Twips(1000), Twips(2000)])).unwrap(),
    );
    let mut tx = session.begin_transaction();
    tx.insert_table_column(&id("table"), 1, vec![cell("new-a", ""), cell("new-b", "")])
        .unwrap();
    session.commit(tx).unwrap();
    let (table, _, _) = session.document().table_for_paragraph(&id("a1-p")).unwrap();
    assert_eq!(
        table.column_widths().unwrap(),
        [Twips(1000), Twips(1000), Twips(1000)]
    );
    let mut tx = session.begin_transaction();
    tx.remove_table_column(&id("table"), 1).unwrap();
    session.commit(tx).unwrap();
    let (table, _, _) = session.document().table_for_paragraph(&id("a1-p")).unwrap();
    assert_eq!(table.column_widths().unwrap(), [Twips(1000), Twips(2000)]);
}

#[test]
fn adding_a_column_to_full_width_table_preserves_physical_total_and_undo() {
    let initial =
        document_with(table().with_column_widths(vec![Twips(4680), Twips(4680)])).unwrap();
    let mut session = DocumentSession::new(initial.clone());
    let mut tx = session.begin_transaction();
    tx.insert_table_column(
        &id("table"),
        1,
        vec![cell("wide-a", ""), cell("wide-b", "")],
    )
    .unwrap();
    session.commit(tx).unwrap();
    let (table, _, _) = session.document().table_for_paragraph(&id("a1-p")).unwrap();
    let widths = table.column_widths().unwrap();
    assert!(widths.iter().all(|width| width.0 > 0));
    assert_eq!(widths.iter().map(|width| width.0).sum::<i32>(), 9360);
    session.undo().unwrap();
    assert_eq!(session.document(), &initial);
}

#[test]
fn one_twip_columns_cannot_lose_width_or_be_split_to_zero() {
    let initial = document_with(table().with_column_widths(vec![Twips(1), Twips(1)])).unwrap();
    let mut session = DocumentSession::new(initial.clone());
    let mut tx = session.begin_transaction();
    assert!(
        tx.insert_table_column(
            &id("table"),
            0,
            vec![cell("tiny-a", ""), cell("tiny-b", "")]
        )
        .is_err()
    );
    assert!(!session.commit(tx).unwrap().changed());
    assert_eq!(session.document(), &initial);
    let mut session = DocumentSession::new(
        document_with(table().with_column_widths(vec![Twips(1), Twips(3)])).unwrap(),
    );
    let mut tx = session.begin_transaction();
    tx.insert_table_column(
        &id("table"),
        0,
        vec![cell("tiny-a", ""), cell("tiny-b", "")],
    )
    .unwrap();
    session.commit(tx).unwrap();
    let (table, _, _) = session.document().table_for_paragraph(&id("a1-p")).unwrap();
    assert_eq!(
        table.column_widths().unwrap(),
        [Twips(1), Twips(1), Twips(2)]
    );
}
