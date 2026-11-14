//! Structural paragraph boundaries preserve Unicode, formatting, and history.

use office_core::{
    Alignment, Block, CharacterFormat, Document, DocumentMetadata, DocumentSession, EditError,
    EditSelection, GraphemeOffset, Id, NamedStyle, Paragraph, ParagraphFormat, Section,
    SectionProperties, TextRun, Transaction, Twips, Underline, ValidationError,
};

fn id(value: &str) -> Id {
    Id::new(value).unwrap()
}

fn paragraph(value: &str, text: &str) -> Paragraph {
    Paragraph::new(id(value), vec![TextRun::new(text)])
}

fn document(paragraphs: Vec<Paragraph>) -> Document {
    Document::from_parts(
        id("document"),
        DocumentMetadata {
            title: "Paragraph boundaries".into(),
        },
        vec![
            Section::new(
                id("section"),
                paragraphs.into_iter().map(Block::Paragraph).collect(),
            )
            .with_properties(SectionProperties::default()),
        ],
        vec![
            NamedStyle::new(id("normal"), "Normal"),
            NamedStyle::new(id("heading"), "Heading"),
        ],
    )
    .unwrap()
}

fn selected(transaction: &Transaction, start: (&str, usize), end: (&str, usize)) -> EditSelection {
    let anchor = transaction
        .position(&id(start.0), GraphemeOffset::new(start.1))
        .unwrap();
    let focus = transaction
        .position(&id(end.0), GraphemeOffset::new(end.1))
        .unwrap();
    transaction.selection(anchor, focus).unwrap()
}

fn paragraphs(transaction: &Transaction) -> Vec<&Paragraph> {
    transaction
        .draft()
        .sections
        .iter()
        .flat_map(Section::blocks)
        .map(|block| match block {
            Block::Paragraph(paragraph) => paragraph,
        })
        .collect()
}

fn text(paragraph: &Paragraph) -> String {
    paragraph
        .runs()
        .iter()
        .map(|run| run.text.as_str())
        .collect()
}

fn bold() -> CharacterFormat {
    CharacterFormat {
        bold: Some(true),
        ..Default::default()
    }
}

fn italic() -> CharacterFormat {
    CharacterFormat {
        italic: Some(true),
        ..Default::default()
    }
}

#[test]
fn splitting_an_empty_paragraph_creates_two_distinct_empty_paragraphs() {
    let original = document(vec![Paragraph::new(id("p"), vec![])]);
    let mut session = DocumentSession::new(original.clone());
    let mut transaction = session.begin_transaction();
    let selection = selected(&transaction, ("p", 0), ("p", 0));
    assert_eq!(
        transaction.insert_paragraph_break(&selection, id("new")),
        Ok(true)
    );
    let result = paragraphs(&transaction);
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].id(), &id("p"));
    assert_eq!(result[1].id(), &id("new"));
    assert!(result.iter().all(|paragraph| paragraph.runs().is_empty()));
    assert_eq!(session.document(), &original);
    assert!(session.commit(transaction).unwrap().changed());
    assert_eq!(session.document().metadata(), original.metadata());
    assert_eq!(session.document().styles(), original.styles());
    assert_eq!(session.document().id(), original.id());
    assert_eq!(
        session.document().schema_version(),
        original.schema_version()
    );
    assert_eq!(session.document().sections()[0].id(), &id("section"));
    assert_eq!(
        session.document().sections()[0].properties(),
        original.sections()[0].properties()
    );
}

#[test]
fn paragraph_start_and_end_are_valid_structural_boundaries() {
    for (initial, count) in [("abc", 3), ("😀e\u{301}", 2), ("", 0)] {
        for (offset, expected) in [(0, ["", initial]), (count, [initial, ""])] {
            let session = DocumentSession::new(document(vec![paragraph("p", initial)]));
            let mut transaction = session.begin_transaction();
            let selection = selected(&transaction, ("p", offset), ("p", offset));
            assert_eq!(
                transaction.insert_paragraph_break(&selection, id("new")),
                Ok(true)
            );
            let result = paragraphs(&transaction);
            assert_eq!([text(result[0]), text(result[1])], expected);
            assert_eq!(result[0].id(), &id("p"));
            assert_eq!(result[1].id(), &id("new"));
        }
    }
}

#[test]
fn splitting_mixed_runs_inherits_paragraph_properties_and_preserves_run_properties() {
    let format = ParagraphFormat {
        alignment: Some(Alignment::Center),
        spacing_before: Some(Twips(120)),
        spacing_after: Some(Twips(240)),
        ..Default::default()
    };
    let original = Paragraph::new(
        id("p"),
        vec![
            TextRun {
                text: "ab".into(),
                format: bold(),
            },
            TextRun {
                text: "cdef".into(),
                format: italic(),
            },
            TextRun::new("GH"),
        ],
    )
    .with_style(id("heading"))
    .with_format(format.clone());
    let session = DocumentSession::new(document(vec![original]));
    let mut transaction = session.begin_transaction();
    let selection = selected(&transaction, ("p", 4), ("p", 4));
    assert!(
        transaction
            .insert_paragraph_break(&selection, id("new"))
            .unwrap()
    );
    let result = paragraphs(&transaction);
    assert_eq!(
        result[0].runs(),
        &[
            TextRun {
                text: "ab".into(),
                format: bold(),
            },
            TextRun {
                text: "cd".into(),
                format: italic(),
            },
        ]
    );
    assert_eq!(
        result[1].runs(),
        &[
            TextRun {
                text: "ef".into(),
                format: italic(),
            },
            TextRun::new("GH"),
        ]
    );
    for paragraph in result {
        assert_eq!(paragraph.style(), Some(&id("heading")));
        assert_eq!(paragraph.format(), &format);
    }
}

#[test]
fn empty_runs_at_the_cut_belong_only_to_the_suffix() {
    let runs = vec![
        TextRun {
            text: String::new(),
            format: bold(),
        },
        TextRun::new("ab"),
        TextRun {
            text: String::new(),
            format: italic(),
        },
        TextRun::new("cd"),
        TextRun {
            text: String::new(),
            format: CharacterFormat {
                underline: Some(Underline::Single),
                ..Default::default()
            },
        },
    ];
    for (cut, run_cut) in [(0, 0), (2, 2), (4, 4)] {
        let session = DocumentSession::new(document(vec![Paragraph::new(id("p"), runs.clone())]));
        let mut transaction = session.begin_transaction();
        let selection = selected(&transaction, ("p", cut), ("p", cut));
        transaction
            .insert_paragraph_break(&selection, id("new"))
            .unwrap();
        let result = paragraphs(&transaction);
        assert_eq!(result[0].runs(), &runs[..run_cut]);
        assert_eq!(result[1].runs(), &runs[run_cut..]);
    }
}

#[test]
fn grapheme_boundaries_span_runs_combining_marks_zwj_emoji_and_flags() {
    let runs = vec![
        TextRun {
            text: "e".into(),
            format: bold(),
        },
        TextRun {
            text: "\u{301}".into(),
            format: italic(),
        },
        TextRun::new("👩\u{200d}"),
        TextRun {
            text: "👩\u{200d}👧\u{200d}👦".into(),
            format: bold(),
        },
        TextRun::new("🇺🇳Q"),
    ];
    let expected = [
        (1, "e\u{301}", "👩\u{200d}👩\u{200d}👧\u{200d}👦🇺🇳Q"),
        (2, "e\u{301}👩\u{200d}👩\u{200d}👧\u{200d}👦", "🇺🇳Q"),
        (3, "e\u{301}👩\u{200d}👩\u{200d}👧\u{200d}👦🇺🇳", "Q"),
    ];
    for (offset, prefix, suffix) in expected {
        let session = DocumentSession::new(document(vec![Paragraph::new(id("p"), runs.clone())]));
        let mut transaction = session.begin_transaction();
        let selection = selected(&transaction, ("p", offset), ("p", offset));
        transaction
            .insert_paragraph_break(&selection, id("new"))
            .unwrap();
        let result = paragraphs(&transaction);
        assert_eq!(text(result[0]), prefix);
        assert_eq!(text(result[1]), suffix);
        assert_eq!(&result[0].runs()[..2], &runs[..2]);
    }
}

#[test]
fn a_selected_same_paragraph_range_is_replaced_with_a_break_in_either_direction() {
    let session = DocumentSession::new(document(vec![paragraph("p", "abcdef")]));
    let mut forward = session.begin_transaction();
    let mut reverse = session.begin_transaction();
    for (transaction, endpoints) in [
        (&mut forward, [("p", 2), ("p", 4)]),
        (&mut reverse, [("p", 4), ("p", 2)]),
    ] {
        let selection = selected(transaction, endpoints[0], endpoints[1]);
        transaction
            .insert_paragraph_break(&selection, id("new"))
            .unwrap();
        let result = paragraphs(transaction);
        assert_eq!([text(result[0]), text(result[1])], ["ab", "ef"]);
    }
    assert_eq!(forward.draft(), reverse.draft());
}

#[test]
fn selecting_all_text_leaves_two_empty_paragraphs() {
    let session = DocumentSession::new(document(vec![paragraph("p", "e\u{301}😀")]));
    let mut transaction = session.begin_transaction();
    let selection = selected(&transaction, ("p", 0), ("p", 2));
    assert!(
        transaction
            .insert_paragraph_break(&selection, id("new"))
            .unwrap()
    );
    let result = paragraphs(&transaction);
    assert_eq!(result.len(), 2);
    assert!(result.iter().all(|paragraph| paragraph.runs().is_empty()));
}

#[test]
fn multi_paragraph_replacement_keeps_start_properties_and_end_run_formatting() {
    let format = ParagraphFormat {
        alignment: Some(Alignment::Right),
        ..Default::default()
    };
    let end = Paragraph::new(
        id("end"),
        vec![TextRun {
            text: "ending".into(),
            format: italic(),
        }],
    )
    .with_style(id("normal"));
    let original = document(vec![
        paragraph("before", "before"),
        paragraph("start", "beginning")
            .with_style(id("heading"))
            .with_format(format.clone()),
        paragraph("middle", "discarded"),
        end,
        paragraph("after", "after"),
    ]);
    let session = DocumentSession::new(original.clone());
    let mut forward = session.begin_transaction();
    let mut reverse = session.begin_transaction();
    for (transaction, endpoints) in [
        (&mut forward, [("start", 2), ("end", 3)]),
        (&mut reverse, [("end", 3), ("start", 2)]),
    ] {
        let selection = selected(transaction, endpoints[0], endpoints[1]);
        transaction
            .insert_paragraph_break(&selection, id("new"))
            .unwrap();
        let result = paragraphs(transaction);
        assert_eq!(
            result
                .iter()
                .map(|paragraph| paragraph.id())
                .collect::<Vec<_>>(),
            [&id("before"), &id("start"), &id("new"), &id("after")]
        );
        assert_eq!(text(result[1]), "be");
        assert_eq!(text(result[2]), "ing");
        assert_eq!(result[2].runs()[0].format, italic());
        for paragraph in &result[1..3] {
            assert_eq!(paragraph.format(), &format);
            assert_eq!(paragraph.style(), Some(&id("heading")));
        }
        assert_eq!(
            transaction.draft().sections[0].properties(),
            Some(&SectionProperties::default())
        );
        assert_eq!(
            transaction.draft().sections[0].blocks()[0],
            original.sections()[0].blocks()[0]
        );
        assert_eq!(
            transaction.draft().sections[0].blocks()[3],
            original.sections()[0].blocks()[4]
        );
    }
    assert_eq!(forward.draft(), reverse.draft());
}

#[test]
fn retained_prefix_and_suffix_never_join_before_the_break() {
    let session = DocumentSession::new(document(vec![
        paragraph("start", "eX"),
        paragraph("end", "\u{301}Y"),
    ]));
    let mut transaction = session.begin_transaction();
    let selection = selected(&transaction, ("start", 1), ("end", 0));
    transaction
        .insert_paragraph_break(&selection, id("new"))
        .unwrap();
    let result = paragraphs(&transaction);
    assert_eq!([text(result[0]), text(result[1])], ["e", "\u{301}Y"]);
}

#[test]
fn new_ids_cannot_reuse_any_original_entity_even_a_removed_paragraph() {
    let session = DocumentSession::new(document(vec![
        paragraph("start", "abc"),
        paragraph("middle", "def"),
        paragraph("end", "ghi"),
    ]));
    let mut transaction = session.begin_transaction();
    let selection = selected(&transaction, ("start", 1), ("end", 1));
    let before = transaction.draft().clone();
    for duplicate in [
        "document", "section", "normal", "heading", "start", "middle", "end",
    ] {
        assert_eq!(
            transaction.insert_paragraph_break(&selection, id(duplicate)),
            Err(EditError::InvalidDocument(ValidationError::DuplicateId {
                id: duplicate.into(),
            }))
        );
        assert_eq!(transaction.draft(), &before);
    }
    // Every failure kept the same endpoint stamp, so the original selection works.
    assert_eq!(
        transaction.insert_paragraph_break(&selection, id("new")),
        Ok(true)
    );
}

#[test]
fn invalid_deserialized_ids_fail_without_changing_draft_or_endpoints() {
    let session = DocumentSession::new(document(vec![paragraph("p", "abc")]));
    let mut transaction = session.begin_transaction();
    let selection = selected(&transaction, ("p", 1), ("p", 1));
    let before = transaction.draft().clone();
    for encoded in ["\"\"", "\" padded \""] {
        let invalid: Id = serde_json::from_str(encoded).unwrap();
        assert!(matches!(
            transaction.insert_paragraph_break(&selection, invalid),
            Err(EditError::InvalidDocument(
                ValidationError::InvalidId { .. }
            ))
        ));
        assert_eq!(transaction.draft(), &before);
    }
    assert_eq!(
        transaction.insert_paragraph_break(&selection, id("new")),
        Ok(true)
    );
}

#[test]
fn cross_section_breaks_fail_without_changing_content_or_endpoints() {
    let original = Document::from_parts(
        id("document"),
        DocumentMetadata::default(),
        vec![
            Section::new(id("first"), vec![Block::Paragraph(paragraph("p", "abc"))]),
            Section::new(id("second"), vec![Block::Paragraph(paragraph("q", "def"))])
                .with_properties(SectionProperties::default()),
        ],
        vec![],
    )
    .unwrap();
    let session = DocumentSession::new(original.clone());
    let mut transaction = session.begin_transaction();
    let before = transaction.draft().clone();
    for endpoints in [[("p", 1), ("q", 1)], [("q", 1), ("p", 1)]] {
        let selection = selected(&transaction, endpoints[0], endpoints[1]);
        assert_eq!(
            transaction.insert_paragraph_break(&selection, id("new")),
            Err(EditError::CrossSectionRange)
        );
        assert_eq!(transaction.draft(), &before);
        assert_eq!(
            transaction.delete_text(&selection),
            Err(EditError::CrossSectionRange)
        );
    }
    let selection = selected(&transaction, ("p", 1), ("p", 1));
    transaction
        .insert_paragraph_break(&selection, id("new"))
        .unwrap();
    assert_eq!(&transaction.draft().sections[1], &original.sections()[1]);
}

#[test]
fn successful_breaks_invalidate_old_endpoints_and_foreign_selections_are_rejected() {
    let session = DocumentSession::new(document(vec![paragraph("p", "abc")]));
    let mut transaction = session.begin_transaction();
    let foreign = session.begin_transaction();
    let foreign_selection = selected(&foreign, ("p", 1), ("p", 1));
    let before = transaction.draft().clone();
    assert_eq!(
        transaction.insert_paragraph_break(&foreign_selection, id("new")),
        Err(EditError::WrongContext)
    );
    assert_eq!(transaction.draft(), &before);
    let selection = selected(&transaction, ("p", 1), ("p", 1));
    transaction
        .insert_paragraph_break(&selection, id("new"))
        .unwrap();
    let after = transaction.draft().clone();
    assert_eq!(
        transaction.insert_paragraph_break(&selection, id("another")),
        Err(EditError::WrongContext)
    );
    assert_eq!(transaction.draft(), &after);
    let selection = selected(&transaction, ("new", 0), ("new", 0));
    assert_eq!(
        transaction.insert_paragraph_break(&selection, id("another")),
        Ok(true)
    );
}

#[test]
fn one_commit_undoes_and_redoes_breaks_and_text_edits_exactly() {
    let original = document(vec![
        Paragraph::new(
            id("p"),
            vec![
                TextRun {
                    text: "e".into(),
                    format: bold(),
                },
                TextRun {
                    text: "\u{301}😀end".into(),
                    format: italic(),
                },
            ],
        )
        .with_style(id("heading")),
    ]);
    let mut session = DocumentSession::new(original.clone());
    let mut transaction = session.begin_transaction();
    let selection = selected(&transaction, ("p", 1), ("p", 2));
    transaction
        .insert_paragraph_break(&selection, id("new"))
        .unwrap();
    let at = transaction
        .position(&id("new"), GraphemeOffset::new(0))
        .unwrap();
    transaction.insert_text(&at, "😀", bold()).unwrap();
    let selection = selected(&transaction, ("new", 1), ("new", 1));
    transaction
        .insert_paragraph_break(&selection, id("last"))
        .unwrap();
    assert_eq!(session.document(), &original);
    assert_eq!(session.revision().number(), 0);
    assert!(session.commit(transaction).unwrap().changed());
    assert_eq!(session.revision().number(), 1);
    let edited = session.document().clone();
    assert_ne!(edited, original);
    assert!(session.undo().unwrap().changed());
    assert_eq!(session.document(), &original);
    assert!(!session.can_undo());
    assert!(session.redo().unwrap().changed());
    assert_eq!(session.document(), &edited);
    assert!(!session.can_redo());
}

#[test]
fn literal_newlines_in_text_remain_text_in_the_original_low_level_operation() {
    let session = DocumentSession::new(document(vec![paragraph("p", "ab")]));
    let mut transaction = session.begin_transaction();
    let at = transaction
        .position(&id("p"), GraphemeOffset::new(1))
        .unwrap();
    transaction
        .insert_text(&at, "\r\n", CharacterFormat::default())
        .unwrap();
    let result = paragraphs(&transaction);
    assert_eq!(result.len(), 1);
    assert_eq!(text(result[0]), "a\r\nb");
}
