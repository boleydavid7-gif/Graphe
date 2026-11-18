//! Collapsed typing grows edited runs without normalizing imported partitions.

use office_core::{
    Block, CharacterFormat, Document, DocumentSession, GraphemeOffset, Id, Paragraph, Section,
    TextCoordinates, TextRun, Underline, Utf16Offset,
};

fn id(value: &str) -> Id {
    Id::new(value).unwrap()
}

fn document(runs: Vec<TextRun>) -> Document {
    Document::from_parts(
        id("typing-document"),
        Default::default(),
        vec![Section::new(
            id("typing-section"),
            vec![Block::Paragraph(Paragraph::new(
                id("typing-paragraph"),
                runs,
            ))],
        )],
        vec![],
    )
    .unwrap()
}

fn runs(document: &Document) -> &[TextRun] {
    let Block::Paragraph(paragraph) = &document.sections()[0].blocks()[0] else {
        panic!("expected paragraph");
    };
    paragraph.runs()
}

fn insert(session: &mut DocumentSession, grapheme: usize, text: &str, format: CharacterFormat) {
    let mut transaction = session.begin_transaction();
    let at = transaction
        .position(&id("typing-paragraph"), GraphemeOffset::new(grapheme))
        .unwrap();
    assert!(transaction.insert_text(&at, text, format).unwrap());
    assert!(session.commit(transaction).unwrap().changed());
}

#[test]
fn one_thousand_ordinary_keystrokes_do_not_consume_one_thousand_runs() {
    let mut session = DocumentSession::with_history_limit(document(vec![]), 2);
    for index in 0..1000 {
        insert(&mut session, index, "a", CharacterFormat::default());
    }
    assert_eq!(runs(session.document()).len(), 1);
    assert_eq!(runs(session.document())[0].text, "a".repeat(1000));
    session.undo().unwrap();
    assert_eq!(runs(session.document())[0].text.len(), 999);
    session.redo().unwrap();
    assert_eq!(runs(session.document())[0].text.len(), 1000);
}

#[test]
fn exact_direct_format_equality_preserves_explicit_override_boundaries() {
    let explicit_false = CharacterFormat {
        bold: Some(false),
        ..Default::default()
    };
    let mut session = DocumentSession::new(document(vec![TextRun::new("a")]));
    insert(&mut session, 1, "b", explicit_false.clone());
    insert(&mut session, 2, "c", explicit_false.clone());
    assert_eq!(
        runs(session.document()),
        &[
            TextRun::new("a"),
            TextRun {
                text: "bc".into(),
                format: explicit_false
            },
        ]
    );
    let italic = CharacterFormat {
        italic: Some(true),
        ..Default::default()
    };
    insert(&mut session, 3, "d", italic.clone());
    insert(&mut session, 4, "e", italic.clone());
    assert_eq!(runs(session.document()).len(), 3);
    assert_eq!(
        runs(session.document())[2],
        TextRun {
            text: "de".into(),
            format: italic
        }
    );
}

#[test]
fn imported_partition_and_empty_suffix_runs_are_not_coalesced() {
    let leading = TextRun {
        text: "".into(),
        format: CharacterFormat {
            bold: Some(true),
            ..Default::default()
        },
    };
    let at_caret = TextRun {
        text: "".into(),
        format: CharacterFormat {
            italic: Some(true),
            ..Default::default()
        },
    };
    let trailing = TextRun {
        text: "".into(),
        format: CharacterFormat {
            underline: Some(Underline::Single),
            ..Default::default()
        },
    };
    let initial = document(vec![
        leading.clone(),
        TextRun::new("ab"),
        at_caret.clone(),
        TextRun::new("cd"),
        TextRun::new("ef"),
        trailing.clone(),
    ]);
    let mut session = DocumentSession::new(initial.clone());
    insert(&mut session, 2, "X", CharacterFormat::default());
    assert_eq!(
        runs(session.document()),
        &[
            leading,
            TextRun::new("abX"),
            at_caret,
            TextRun::new("cd"),
            TextRun::new("ef"),
            trailing,
        ]
    );
    session.undo().unwrap();
    assert_eq!(session.document(), &initial);
    session.redo().unwrap();
    assert_eq!(runs(session.document())[1].text, "abX");
}

#[test]
fn insertion_at_the_start_does_not_absorb_the_unchanged_right_run() {
    let empty = TextRun {
        text: "".into(),
        format: CharacterFormat {
            bold: Some(true),
            ..Default::default()
        },
    };
    let mut session = DocumentSession::new(document(vec![empty.clone(), TextRun::new("ab")]));
    insert(&mut session, 0, "X", CharacterFormat::default());
    insert(&mut session, 1, "Y", CharacterFormat::default());
    assert_eq!(
        runs(session.document()),
        &[TextRun::new("XY"), empty, TextRun::new("ab")]
    );
}

#[test]
fn insertion_inside_a_unicode_run_preserves_core_offsets_and_right_partition() {
    let mut session = DocumentSession::new(document(vec![
        TextRun::new("e"),
        TextRun::new("\u{301}😀Z"),
    ]));
    insert(&mut session, 1, "👩🏽‍💻", CharacterFormat::default());
    assert_eq!(
        runs(session.document()),
        &[
            TextRun::new("e"),
            TextRun::new("\u{301}👩🏽‍💻"),
            TextRun::new("😀Z")
        ]
    );
    let coordinates = session
        .document()
        .text_coordinates(&id("typing-paragraph"))
        .unwrap();
    assert_eq!(coordinates.text(), "e\u{301}👩🏽‍💻😀Z");
    assert_eq!(coordinates.grapheme_len().get(), 4);
    assert_eq!(
        coordinates
            .grapheme_to_utf16(GraphemeOffset::new(2))
            .unwrap()
            .get(),
        9
    );
    assert!(coordinates.utf16_to_grapheme(Utf16Offset::new(1)).is_err());
    let mut joined = DocumentSession::new(document(vec![TextRun::new("e")]));
    insert(&mut joined, 1, "\u{301}", CharacterFormat::default());
    assert_eq!(runs(joined.document()), &[TextRun::new("e\u{301}")]);
    let Block::Paragraph(paragraph) = &joined.document().sections()[0].blocks()[0] else {
        panic!("expected paragraph");
    };
    assert_eq!(TextCoordinates::new(paragraph).grapheme_len().get(), 1);
}

#[test]
fn no_op_paths_preserve_identical_imported_partitions_and_empty_runs() {
    let initial = document(vec![TextRun::new("a"), TextRun::new(""), TextRun::new("b")]);
    let session = DocumentSession::new(initial.clone());
    let mut transaction = session.begin_transaction();
    let first = transaction
        .position(&id("typing-paragraph"), GraphemeOffset::new(0))
        .unwrap();
    assert!(
        !transaction
            .insert_text(&first, "", CharacterFormat::default())
            .unwrap()
    );
    let end = transaction
        .position(&id("typing-paragraph"), GraphemeOffset::new(2))
        .unwrap();
    let selected = transaction.selection(first, end).unwrap();
    assert!(
        !transaction
            .replace_text(&selected, "ab", CharacterFormat::default())
            .unwrap()
    );
    assert_eq!(transaction.draft().sections, initial.sections());
}

#[test]
fn noncollapsed_replacement_retains_its_existing_partition_policy() {
    let session = DocumentSession::new(document(vec![
        TextRun::new("ab"),
        TextRun::new("cd"),
        TextRun::new(""),
        TextRun::new("ef"),
    ]));
    let mut transaction = session.begin_transaction();
    let start = transaction
        .position(&id("typing-paragraph"), GraphemeOffset::new(1))
        .unwrap();
    let end = transaction
        .position(&id("typing-paragraph"), GraphemeOffset::new(3))
        .unwrap();
    let selected = transaction.selection(start, end).unwrap();
    assert!(
        transaction
            .replace_text(&selected, "X", CharacterFormat::default())
            .unwrap()
    );
    let Block::Paragraph(paragraph) = &transaction.draft().sections[0].blocks()[0] else {
        panic!("expected paragraph");
    };
    assert_eq!(
        paragraph.runs(),
        &[
            TextRun::new("a"),
            TextRun::new("X"),
            TextRun::new("d"),
            TextRun::new(""),
            TextRun::new("ef")
        ]
    );
}

#[test]
fn collapsed_replace_and_insert_share_the_same_targeted_typing_policy() {
    let session = DocumentSession::new(document(vec![TextRun::new("ab"), TextRun::new("cd")]));
    let mut transaction = session.begin_transaction();
    let at = transaction
        .position(&id("typing-paragraph"), GraphemeOffset::new(2))
        .unwrap();
    let selected = transaction.selection(at.clone(), at).unwrap();
    assert!(
        transaction
            .replace_text(&selected, "X", CharacterFormat::default())
            .unwrap()
    );
    let Block::Paragraph(paragraph) = &transaction.draft().sections[0].blocks()[0] else {
        panic!("expected paragraph");
    };
    assert_eq!(paragraph.runs(), &[TextRun::new("abX"), TextRun::new("cd")]);
}
