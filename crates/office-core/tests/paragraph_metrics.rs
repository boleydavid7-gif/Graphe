//! Paragraph metrics are partial, semantic, transactional and Unicode-safe.

use office_core::{
    Alignment, Block, Document, DocumentMetadata, DocumentSession, GraphemeOffset, Id, Indentation,
    LineSpacing, Paragraph, ParagraphFormat, ParagraphMetricsPatch, Section, TextRun, Twips,
};

fn id(value: &str) -> Id {
    Id::new(value).unwrap()
}

fn first() -> Paragraph {
    Paragraph::new(id("one"), vec![TextRun::new("One")]).with_format(ParagraphFormat {
        alignment: Some(Alignment::Right),
        spacing_before: Some(Twips(40)),
        spacing_after: Some(Twips(160)),
        line_spacing: Some(LineSpacing::Multiple { value: 360 }),
        indentation: Some(Indentation {
            left: Some(Twips(720)),
            right: Some(Twips(200)),
            first_line: Some(Twips(-240)),
        }),
        ..Default::default()
    })
}

fn document() -> Document {
    Document::from_parts(
        id("document"),
        DocumentMetadata::default(),
        vec![Section::new(
            id("section"),
            vec![
                Block::Paragraph(first()),
                Block::Paragraph(
                    Paragraph::new(id("two"), vec![TextRun::new("Two")]).with_format(
                        ParagraphFormat {
                            indentation: Some(Indentation {
                                left: Some(Twips(180)),
                                right: Some(Twips(120)),
                                first_line: Some(Twips(0)),
                            }),
                            ..Default::default()
                        },
                    ),
                ),
                Block::Paragraph(Paragraph::new(id("three"), vec![TextRun::new("Three")])),
            ],
        )],
        vec![],
    )
    .unwrap()
}

fn p(document: &Document, index: usize) -> &Paragraph {
    let Block::Paragraph(paragraph) = &document.sections()[0].blocks()[index] else {
        panic!("expected paragraph");
    };
    paragraph
}

#[test]
fn selected_paragraphs_change_only_specified_metrics_in_one_undo_step() {
    let initial = document();
    let mut session = DocumentSession::new(initial.clone());
    let mut edit = session.begin_transaction();
    let start = edit.position(&id("one"), GraphemeOffset::new(0)).unwrap();
    let end = edit.position(&id("two"), GraphemeOffset::new(3)).unwrap();
    let selection = edit.selection(start, end).unwrap();
    let patch = ParagraphMetricsPatch {
        spacing_after: Some(Twips(260)),
        indent_left: Some(Twips(1440)),
        line_spacing: Some(LineSpacing::Multiple { value: 480 }),
        ..Default::default()
    };
    assert_eq!(edit.apply_paragraph_metrics(&selection, &patch), Ok(true));
    assert!(session.commit(edit).unwrap().changed());
    let updated = session.document();
    assert_eq!(p(updated, 0).format().alignment, Some(Alignment::Right));
    assert_eq!(p(updated, 0).format().spacing_before, Some(Twips(40)));
    assert_eq!(p(updated, 0).format().spacing_after, Some(Twips(260)));
    assert_eq!(
        p(updated, 0).format().line_spacing,
        Some(LineSpacing::Multiple { value: 480 })
    );
    assert_eq!(p(updated, 1).format().spacing_after, Some(Twips(260)));
    assert_eq!(
        p(updated, 0).format().indentation.as_ref().unwrap().right,
        Some(Twips(200))
    );
    assert_eq!(
        p(updated, 1).format().indentation.as_ref().unwrap().right,
        Some(Twips(120))
    );
    assert_eq!(
        p(updated, 0)
            .format()
            .indentation
            .as_ref()
            .unwrap()
            .first_line,
        Some(Twips(-240))
    );
    assert_eq!(
        p(updated, 1)
            .format()
            .indentation
            .as_ref()
            .unwrap()
            .first_line,
        Some(Twips(0))
    );
    assert_eq!(
        p(updated, 0).format().indentation.as_ref().unwrap().left,
        Some(Twips(1440))
    );
    assert_eq!(
        p(updated, 1).format().indentation.as_ref().unwrap().left,
        Some(Twips(1440))
    );
    assert_eq!(p(updated, 2), p(&initial, 2));
    assert_eq!(p(updated, 0).runs(), p(&initial, 0).runs());
    session.undo().unwrap();
    assert_eq!(session.document(), &initial);
    session.redo().unwrap();
    assert_eq!(
        p(session.document(), 0).format().spacing_after,
        Some(Twips(260))
    );
}

#[test]
fn invalid_paragraph_metrics_do_not_mutate_a_transaction() {
    let session = DocumentSession::new(document());
    let mut edit = session.begin_transaction();
    let point = edit.position(&id("one"), GraphemeOffset::new(0)).unwrap();
    let selection = edit.selection(point.clone(), point).unwrap();
    let before = edit.draft().clone();
    assert!(
        edit.apply_paragraph_metrics(
            &selection,
            &ParagraphMetricsPatch {
                spacing_before: Some(Twips(-20)),
                ..Default::default()
            }
        )
        .is_err()
    );
    assert_eq!(edit.draft(), &before);
    assert_eq!(
        edit.apply_paragraph_metrics(&selection, &ParagraphMetricsPatch::default()),
        Ok(false)
    );
    assert_eq!(edit.draft(), &before);
}
