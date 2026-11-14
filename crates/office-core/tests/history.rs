//! Bounded snapshot history, atomic restoration, and revision-lineage contracts.

use office_core::{
    Block, DEFAULT_HISTORY_LIMIT, Document, DocumentMetadata, DocumentSession, HistoryError, Id,
    Paragraph, SCHEMA_VERSION, Section, TextRun, TransactionError, ValidationError,
};

fn id(value: &str) -> Id {
    Id::new(value).unwrap()
}

fn document() -> Document {
    Document::from_parts(
        id("document"),
        DocumentMetadata { title: "0".into() },
        vec![Section::new(
            id("section"),
            vec![Block::Paragraph(Paragraph::new(
                id("paragraph"),
                vec![TextRun::new("e"), TextRun::new("\u{301}👩🏽‍💻")],
            ))],
        )],
        vec![],
    )
    .unwrap()
}

fn commit_title(session: &mut DocumentSession, title: &str) {
    let mut transaction = session.begin_transaction();
    transaction.draft_mut().metadata.title = title.into();
    assert!(session.commit(transaction).unwrap().changed());
}

#[test]
fn fresh_history_has_explicit_errors_without_changing_document_or_revision() {
    let original = document();
    let mut session = DocumentSession::new(original.clone());
    let origin = session.revision();
    assert_eq!(session.history_limit(), DEFAULT_HISTORY_LIMIT);
    assert_eq!(DEFAULT_HISTORY_LIMIT, 100);
    assert!(!session.can_undo());
    assert!(!session.can_redo());
    assert_eq!(session.undo(), Err(HistoryError::NothingToUndo));
    assert_eq!(session.redo(), Err(HistoryError::NothingToRedo));
    assert_eq!(session.document(), &original);
    assert_eq!(session.revision(), origin);
}

#[test]
fn an_entire_compound_commit_is_one_exact_undo_and_redo_step() {
    let original = document();
    let mut session = DocumentSession::new(original.clone());
    let mut transaction = session.begin_transaction();
    transaction.draft_mut().metadata.title = "Compound candidate".into();
    transaction.draft_mut().sections.push(Section::new(
        id("appendix"),
        vec![Block::Paragraph(Paragraph::new(
            id("appendix-paragraph"),
            vec![TextRun::new("Additional content")],
        ))],
    ));
    let committed = session.commit(transaction).unwrap();
    assert_eq!(committed.revision().number(), 1);
    let changed = session.document().clone();
    assert_ne!(changed, original);
    assert!(session.can_undo());
    assert!(!session.can_redo());

    let undo = session.undo().unwrap();
    assert!(undo.changed());
    assert_eq!(undo.revision(), &session.revision());
    assert_eq!(undo.revision().number(), 2);
    assert_eq!(session.document(), &original);
    assert!(!session.can_undo());
    assert!(session.can_redo());
    assert_eq!(session.undo(), Err(HistoryError::NothingToUndo));

    let redo = session.redo().unwrap();
    assert!(redo.changed());
    assert_eq!(redo.revision().number(), 3);
    assert_eq!(session.document(), &changed);
    assert!(session.can_undo());
    assert!(!session.can_redo());
    assert_eq!(session.redo(), Err(HistoryError::NothingToRedo));
}

#[test]
fn multiple_history_steps_follow_commit_order_in_both_directions() {
    let mut session = DocumentSession::new(document());
    let mut snapshots = vec![session.document().clone()];
    for title in ["First", "Second", "Third"] {
        commit_title(&mut session, title);
        snapshots.push(session.document().clone());
    }
    for (index, expected) in snapshots[..3].iter().rev().enumerate() {
        let outcome = session.undo().unwrap();
        assert_eq!(outcome.revision().number(), 4 + index as u64);
        assert_eq!(session.document(), expected);
    }
    assert!(!session.can_undo());
    for (index, expected) in snapshots[1..].iter().enumerate() {
        let outcome = session.redo().unwrap();
        assert_eq!(outcome.revision().number(), 7 + index as u64);
        assert_eq!(session.document(), expected);
    }
    assert!(!session.can_redo());
}

#[test]
fn default_history_keeps_the_latest_hundred_changes_and_prunes_the_oldest() {
    let mut session = DocumentSession::new(document());
    for number in 1..=105 {
        commit_title(&mut session, &number.to_string());
    }
    for expected in (5..=104).rev() {
        session.undo().unwrap();
        assert_eq!(session.document().metadata().title, expected.to_string());
    }
    assert!(!session.can_undo());
    assert_eq!(session.undo(), Err(HistoryError::NothingToUndo));
    assert_eq!(session.document().metadata().title, "5");
    for expected in 6..=105 {
        session.redo().unwrap();
        assert_eq!(session.document().metadata().title, expected.to_string());
    }
    assert!(!session.can_redo());
    assert_eq!(session.revision().number(), 305);
}

#[test]
fn a_custom_limit_is_preserved_when_undo_and_redo_transfer_snapshots() {
    let mut session = DocumentSession::with_history_limit(document(), 2);
    assert_eq!(session.history_limit(), 2);
    for title in ["A", "B", "C"] {
        commit_title(&mut session, title);
    }
    for expected in ["B", "A"] {
        session.undo().unwrap();
        assert_eq!(session.document().metadata().title, expected);
    }
    assert_eq!(session.undo(), Err(HistoryError::NothingToUndo));
    for expected in ["B", "C"] {
        session.redo().unwrap();
        assert_eq!(session.document().metadata().title, expected);
    }
    // Redo must not expand the retained window beyond its original bound.
    for expected in ["B", "A"] {
        session.undo().unwrap();
        assert_eq!(session.document().metadata().title, expected);
    }
    assert_eq!(session.undo(), Err(HistoryError::NothingToUndo));
}

#[test]
fn a_zero_limit_disables_history_without_disabling_commits_or_revisions() {
    let mut session = DocumentSession::with_history_limit(document(), 0);
    for (index, title) in ["A", "B", "C"].into_iter().enumerate() {
        commit_title(&mut session, title);
        assert_eq!(session.revision().number(), index as u64 + 1);
        assert_eq!(session.document().metadata().title, title);
        assert!(!session.can_undo());
        assert!(!session.can_redo());
    }
    let unchanged = session.document().clone();
    let token = session.revision();
    assert_eq!(session.undo(), Err(HistoryError::NothingToUndo));
    assert_eq!(session.redo(), Err(HistoryError::NothingToRedo));
    assert_eq!(session.document(), &unchanged);
    assert_eq!(session.revision(), token);
}

#[test]
fn a_no_op_after_undo_preserves_redo_tokens_and_other_pending_transactions() {
    let mut session = DocumentSession::new(document());
    commit_title(&mut session, "A");
    commit_title(&mut session, "B");
    session.undo().unwrap();
    let origin = session.revision();
    let pending = session.begin_transaction();

    let mut no_op = session.begin_transaction();
    no_op.draft_mut().metadata.title = "Temporary".into();
    no_op.draft_mut().metadata.title = "A".into();
    assert!(!session.commit(no_op).unwrap().changed());
    assert_eq!(session.revision(), origin);
    assert_eq!(session.validate_revision(&origin), Ok(()));
    assert!(session.can_undo());
    assert!(session.can_redo());
    assert!(!session.commit(pending).unwrap().changed());
    session.redo().unwrap();
    assert_eq!(session.document().metadata().title, "B");
    session.undo().unwrap();
    session.undo().unwrap();
    assert_eq!(session.document().metadata().title, "0");
    assert!(!session.can_undo());
}

#[test]
fn a_new_changed_commit_after_undo_discards_only_the_redo_branch() {
    let mut session = DocumentSession::new(document());
    for title in ["A", "B", "C"] {
        commit_title(&mut session, title);
    }
    session.undo().unwrap();
    session.undo().unwrap();
    assert_eq!(session.document().metadata().title, "A");
    assert!(session.can_redo());
    commit_title(&mut session, "D");
    assert!(!session.can_redo());
    assert_eq!(session.redo(), Err(HistoryError::NothingToRedo));
    session.undo().unwrap();
    assert_eq!(session.document().metadata().title, "A");
    session.undo().unwrap();
    assert_eq!(session.document().metadata().title, "0");
    for expected in ["A", "D"] {
        session.redo().unwrap();
        assert_eq!(session.document().metadata().title, expected);
    }
    assert!(!session.can_redo());
}

#[test]
fn invalid_commits_preserve_the_document_revision_and_both_history_stacks() {
    let mut session = DocumentSession::new(document());
    commit_title(&mut session, "A");
    commit_title(&mut session, "B");
    session.undo().unwrap();
    let current = session.document().clone();
    let token = session.revision();
    let mut invalid = session.begin_transaction();
    invalid.draft_mut().metadata.title = "Must not leak".into();
    invalid
        .draft_mut()
        .sections
        .push(Section::new(id("paragraph"), vec![]));
    assert!(matches!(
        session.commit(invalid),
        Err(TransactionError::InvalidDocument(
            ValidationError::DuplicateId { .. }
        ))
    ));
    assert_eq!(session.document(), &current);
    assert_eq!(session.revision(), token);
    assert!(session.can_undo());
    assert!(session.can_redo());
    session.redo().unwrap();
    assert_eq!(session.document().metadata().title, "B");
    session.undo().unwrap();
    session.undo().unwrap();
    assert_eq!(session.document().metadata().title, "0");
}

#[test]
fn undo_and_redo_never_revive_old_tokens_or_allow_stale_drafts_to_overwrite() {
    let original = document();
    let mut session = DocumentSession::new(original.clone());
    let origin = session.revision();
    let stale_before_commit = session.begin_transaction();
    commit_title(&mut session, "A");
    let at_a = session.revision();
    let stale_before_undo = session.begin_transaction();
    session.undo().unwrap();
    assert_eq!(session.document(), &original);
    assert_eq!(session.revision().number(), 2);
    assert!(matches!(
        session.validate_revision(&origin),
        Err(TransactionError::StaleRevision {
            expected: 2,
            actual: 0
        })
    ));
    assert!(matches!(
        session.commit(stale_before_commit),
        Err(TransactionError::StaleRevision { .. })
    ));
    assert!(session.can_redo());
    session.redo().unwrap();
    assert_eq!(session.document().metadata().title, "A");
    assert_eq!(session.revision().number(), 3);
    assert!(matches!(
        session.validate_revision(&at_a),
        Err(TransactionError::StaleRevision {
            expected: 3,
            actual: 1
        })
    ));
    assert!(matches!(
        session.commit(stale_before_undo),
        Err(TransactionError::StaleRevision { .. })
    ));
    assert!(!session.can_redo());
    session.undo().unwrap();
    assert_eq!(session.document(), &original);
}

#[test]
fn foreign_transactions_cannot_change_history_even_when_the_document_ids_match() {
    let mut local = DocumentSession::new(document());
    commit_title(&mut local, "A");
    commit_title(&mut local, "B");
    local.undo().unwrap();
    let foreign = DocumentSession::new(document());
    let mut transaction = foreign.begin_transaction();
    transaction.draft_mut().metadata.title = "Foreign".into();
    let before = local.document().clone();
    let token = local.revision();
    assert_eq!(
        local.commit(transaction),
        Err(TransactionError::WrongSession)
    );
    assert_eq!(local.document(), &before);
    assert_eq!(local.revision(), token);
    assert!(local.can_undo());
    assert!(local.can_redo());
    local.redo().unwrap();
    assert_eq!(local.document().metadata().title, "B");
}

#[test]
fn clearing_history_preserves_content_revision_and_pending_transaction_validity() {
    let mut session = DocumentSession::new(document());
    commit_title(&mut session, "A");
    commit_title(&mut session, "B");
    session.undo().unwrap();
    let before = session.document().clone();
    let token = session.revision();
    let mut pending = session.begin_transaction();
    pending.draft_mut().metadata.title = "After clear".into();

    session.clear_history();
    assert!(!session.can_undo());
    assert!(!session.can_redo());
    assert_eq!(session.document(), &before);
    assert_eq!(session.revision(), token);
    assert_eq!(session.validate_revision(&token), Ok(()));
    assert_eq!(session.undo(), Err(HistoryError::NothingToUndo));
    assert_eq!(session.redo(), Err(HistoryError::NothingToRedo));
    assert!(session.commit(pending).unwrap().changed());
    session.undo().unwrap();
    assert_eq!(session.document(), &before);
    assert!(!session.can_undo());
}

#[test]
fn history_restores_exact_unicode_runs_styles_ids_and_properties_from_the_fixture() {
    let original = Document::from_json(include_str!(
        "../../../tests/fixtures/writer-sample-v1.json"
    ))
    .unwrap();
    let original_json = original.to_json().unwrap();
    let mut session = DocumentSession::new(original.clone());
    let mut candidate = session.begin_transaction();
    candidate.draft_mut().metadata.title = "Updated".into();
    candidate.draft_mut().sections.reverse();
    candidate.draft_mut().styles[0].character_format.bold = Some(true);
    session.commit(candidate).unwrap();
    let changed = session.document().clone();
    session.undo().unwrap();
    assert_eq!(session.document(), &original);
    assert_eq!(session.document().to_json().unwrap(), original_json);
    assert_eq!(
        session
            .document()
            .text_coordinates(&id("intro"))
            .unwrap()
            .text(),
        "Cafe\u{301} research — 👩🏽‍💻 team"
    );
    session.redo().unwrap();
    assert_eq!(session.document(), &changed);
    assert_eq!(session.document().schema_version(), SCHEMA_VERSION);
}

#[test]
fn a_run_boundary_only_change_is_recorded_and_restored_exactly() {
    let original = document();
    let mut session = DocumentSession::new(original.clone());
    let mut candidate = session.begin_transaction();
    candidate.draft_mut().sections = vec![Section::new(
        id("section"),
        vec![Block::Paragraph(Paragraph::new(
            id("paragraph"),
            vec![TextRun::new("e\u{301}👩🏽‍💻")],
        ))],
    )];
    assert!(session.commit(candidate).unwrap().changed());
    let repartitioned = session.document().clone();
    assert_eq!(
        session
            .document()
            .text_coordinates(&id("paragraph"))
            .unwrap()
            .text(),
        original.text_coordinates(&id("paragraph")).unwrap().text()
    );
    session.undo().unwrap();
    assert_eq!(session.document(), &original);
    session.redo().unwrap();
    assert_eq!(session.document(), &repartitioned);
}

#[test]
fn saving_and_reopening_preserve_the_model_but_start_with_empty_independent_history() {
    let mut original = DocumentSession::new(document());
    commit_title(&mut original, "Saved");
    let token = original.revision();
    let saved = original.document().to_json().unwrap();
    let payload: serde_json::Value = serde_json::from_str(&saved).unwrap();
    for runtime_field in ["undo", "redo", "history", "revision"] {
        assert!(payload.get(runtime_field).is_none());
    }
    let mut reopened = DocumentSession::new(Document::from_json(&saved).unwrap());
    assert_eq!(reopened.document(), original.document());
    assert_eq!(reopened.revision().number(), 0);
    assert_eq!(
        reopened.validate_revision(&token),
        Err(TransactionError::WrongSession)
    );
    assert_eq!(reopened.undo(), Err(HistoryError::NothingToUndo));
    assert_eq!(reopened.redo(), Err(HistoryError::NothingToRedo));
    original.undo().unwrap();
    assert_eq!(original.document().metadata().title, "0");
    assert_eq!(reopened.document().metadata().title, "Saved");
    assert!(!reopened.can_undo());
}
