//! Editing contracts: Unicode boundaries, formatting preservation, and draft provenance.

use office_core::{
    Alignment, Block, CharacterFormat, CharacterFormatPatch, Document, DocumentMetadata,
    DocumentSession, EditError, EditSelection, GraphemeOffset, HalfPoints, Id, Indentation,
    LineSpacing, NamedStyle, Paragraph, ParagraphFormat, ParagraphFormatPatch, PositionError,
    PropertyChange, Section, SectionProperties, TextColor, TextRun, Transaction, TransactionError,
    Twips, Underline, ValidationError,
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
            title: "Report".into(),
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

fn session(text: &str) -> DocumentSession {
    DocumentSession::new(document(vec![paragraph("p", text)]))
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

fn paragraph_in<'a>(transaction: &'a Transaction, value: &str) -> &'a Paragraph {
    transaction
        .draft()
        .sections
        .iter()
        .flat_map(|section| section.blocks())
        .map(|block| match block {
            Block::Paragraph(paragraph) => paragraph,
            Block::Table(_) => panic!("expected paragraph"),
        })
        .find(|paragraph| paragraph.id() == &id(value))
        .unwrap()
}

fn text(transaction: &Transaction, value: &str) -> String {
    transaction
        .text_coordinates(&id(value))
        .unwrap()
        .text()
        .into()
}

fn bold() -> CharacterFormat {
    CharacterFormat {
        bold: Some(true),
        ..Default::default()
    }
}

#[test]
fn a_failed_operation_preserves_prior_staging_until_the_caller_commits_or_aborts() {
    let mut session = session("A");
    let original = session.document().clone();
    let mut transaction = session.begin_transaction();
    let at = transaction
        .position(&id("p"), GraphemeOffset::new(1))
        .unwrap();
    assert!(
        transaction
            .insert_text(&at, "B", CharacterFormat::default())
            .unwrap()
    );
    let selection = selected(&transaction, ("p", 0), ("p", 2));
    let before_failure = transaction.draft().clone();
    assert!(matches!(
        transaction.apply_character_format(
            &selection,
            &CharacterFormatPatch {
                font_size: PropertyChange::Set(HalfPoints(0)),
                ..Default::default()
            }
        ),
        Err(EditError::InvalidDocument(
            ValidationError::InvalidFormatting { .. }
        ))
    ));
    assert_eq!(transaction.draft(), &before_failure);
    assert_eq!(session.document(), &original);
    assert_eq!(
        transaction.apply_character_format(&selection, &CharacterFormatPatch::default()),
        Ok(false)
    );
    assert!(session.commit(transaction).unwrap().changed());
    assert_eq!(
        session
            .document()
            .text_coordinates(&id("p"))
            .unwrap()
            .text(),
        "AB"
    );
    session.undo().unwrap();
    assert_eq!(session.document(), &original);
}

#[test]
fn insertion_preserves_prefix_suffix_properties_and_unpublished_state() {
    let italic = CharacterFormat {
        italic: Some(true),
        ..Default::default()
    };
    let paragraph = Paragraph::new(
        id("p"),
        vec![
            TextRun {
                text: "abcd".into(),
                format: bold(),
            },
            TextRun {
                text: "EF".into(),
                format: italic.clone(),
            },
        ],
    )
    .with_style(id("normal"))
    .with_format(ParagraphFormat {
        alignment: Some(Alignment::Center),
        ..Default::default()
    });
    let original = document(vec![paragraph]);
    let mut session = DocumentSession::new(original.clone());
    let mut transaction = session.begin_transaction();
    let at = transaction
        .position(&id("p"), GraphemeOffset::new(2))
        .unwrap();
    let inserted = CharacterFormat {
        underline: Some(Underline::Single),
        ..Default::default()
    };
    assert_eq!(
        transaction.insert_text(&at, "XYZ", inserted.clone()),
        Ok(true)
    );
    assert_eq!(text(&transaction, "p"), "abXYZcdEF");
    let result = paragraph_in(&transaction, "p");
    assert_eq!(result.id(), &id("p"));
    assert_eq!(result.style(), Some(&id("normal")));
    assert_eq!(result.format().alignment, Some(Alignment::Center));
    assert_eq!(
        result.runs(),
        &[
            TextRun {
                text: "ab".into(),
                format: bold()
            },
            TextRun {
                text: "XYZ".into(),
                format: inserted
            },
            TextRun {
                text: "cd".into(),
                format: bold()
            },
            TextRun {
                text: "EF".into(),
                format: italic
            },
        ]
    );
    assert_eq!(session.document(), &original);
    assert!(session.commit(transaction).unwrap().changed());
    assert_eq!(session.document().id(), original.id());
    assert_eq!(
        session.document().sections()[0].properties(),
        original.sections()[0].properties()
    );
}

#[test]
fn start_end_and_empty_paragraph_insertion_keep_exact_text() {
    for initial in ["", "abc", "😀"] {
        let session = session(initial);
        let mut transaction = session.begin_transaction();
        let at = transaction
            .position(&id("p"), GraphemeOffset::new(0))
            .unwrap();
        assert!(
            transaction
                .insert_text(&at, "前", CharacterFormat::default())
                .unwrap()
        );
        let end = transaction
            .text_coordinates(&id("p"))
            .unwrap()
            .grapheme_len();
        let at = transaction.position(&id("p"), end).unwrap();
        assert!(
            transaction
                .insert_text(&at, "後", CharacterFormat::default())
                .unwrap()
        );
        assert_eq!(text(&transaction, "p"), format!("前{initial}後"));
    }
    let empty = document(vec![Paragraph::new(id("p"), vec![])]);
    let session = DocumentSession::new(empty);
    let mut transaction = session.begin_transaction();
    let at = transaction
        .position(&id("p"), GraphemeOffset::new(0))
        .unwrap();
    assert!(transaction.insert_text(&at, "Text", bold()).unwrap());
    assert_eq!(
        paragraph_in(&transaction, "p").runs(),
        &[TextRun {
            text: "Text".into(),
            format: bold()
        }]
    );
}

#[test]
fn deletion_respects_graphemes_that_span_formatted_runs() {
    let paragraph = Paragraph::new(
        id("p"),
        vec![
            TextRun::new("e"),
            TextRun {
                text: "\u{301}👩".into(),
                format: bold(),
            },
            TextRun::new("🏽‍💻!"),
        ],
    );
    let session = DocumentSession::new(document(vec![paragraph]));
    let mut transaction = session.begin_transaction();
    assert_eq!(
        transaction
            .text_coordinates(&id("p"))
            .unwrap()
            .grapheme_len()
            .get(),
        3
    );
    let selection = selected(&transaction, ("p", 0), ("p", 1));
    assert!(transaction.delete_text(&selection).unwrap());
    assert_eq!(text(&transaction, "p"), "👩🏽‍💻!");
    let selection = selected(&transaction, ("p", 0), ("p", 1));
    assert!(transaction.delete_text(&selection).unwrap());
    assert_eq!(text(&transaction, "p"), "!");
}

#[test]
fn reversed_replacement_uses_the_ordered_half_open_range() {
    let session = session("abcdef");
    let mut forward = session.begin_transaction();
    let mut reverse = session.begin_transaction();
    let first = selected(&forward, ("p", 1), ("p", 4));
    let second = selected(&reverse, ("p", 4), ("p", 1));
    assert!(second.selection().anchor().offset() > second.selection().focus().offset());
    forward.replace_text(&first, "X", bold()).unwrap();
    reverse.replace_text(&second, "X", bold()).unwrap();
    assert_eq!(forward.draft(), reverse.draft());
    assert_eq!(text(&forward, "p"), "aXef");
}

#[test]
fn new_text_adjacency_can_merge_clusters_and_requires_fresh_endpoints() {
    let initial_session = session("e");
    let mut transaction = initial_session.begin_transaction();
    let old_end = transaction
        .position(&id("p"), GraphemeOffset::new(1))
        .unwrap();
    transaction
        .insert_text(&old_end, "\u{301}", CharacterFormat::default())
        .unwrap();
    let coordinates = transaction.text_coordinates(&id("p")).unwrap();
    assert_eq!(coordinates.text(), "e\u{301}");
    assert_eq!(coordinates.grapheme_len().get(), 1);
    assert_eq!(
        transaction.insert_text(&old_end, "x", bold()),
        Err(EditError::WrongContext)
    );
    let fresh = transaction
        .position(&id("p"), GraphemeOffset::new(1))
        .unwrap();
    transaction
        .insert_text(&fresh, "!", CharacterFormat::default())
        .unwrap();
    assert_eq!(text(&transaction, "p"), "e\u{301}!");

    let session = session("👩💻");
    let mut transaction = session.begin_transaction();
    let between = transaction
        .position(&id("p"), GraphemeOffset::new(1))
        .unwrap();
    transaction
        .insert_text(&between, "‍", CharacterFormat::default())
        .unwrap();
    assert_eq!(text(&transaction, "p"), "👩‍💻");
    assert_eq!(
        transaction
            .text_coordinates(&id("p"))
            .unwrap()
            .grapheme_len()
            .get(),
        1
    );
}

#[test]
fn deletion_can_join_regional_indicators_into_one_new_grapheme() {
    let session = session("🇺x🇸");
    let mut transaction = session.begin_transaction();
    let selection = selected(&transaction, ("p", 1), ("p", 2));
    transaction.delete_text(&selection).unwrap();
    assert_eq!(text(&transaction, "p"), "🇺🇸");
    assert_eq!(
        transaction
            .text_coordinates(&id("p"))
            .unwrap()
            .grapheme_len()
            .get(),
        1
    );
}

#[test]
fn inserted_combining_marks_newlines_and_nul_are_not_normalized_or_restructured() {
    let mut session = session("é");
    let mut transaction = session.begin_transaction();
    let at = transaction
        .position(&id("p"), GraphemeOffset::new(1))
        .unwrap();
    let inserted = "e\u{301}\r\n中文\n\0";
    transaction
        .insert_text(&at, inserted, CharacterFormat::default())
        .unwrap();
    assert_eq!(
        text(&transaction, "p").as_bytes(),
        format!("é{inserted}").as_bytes()
    );
    assert_eq!(transaction.draft().sections[0].blocks().len(), 1);
    session.commit(transaction).unwrap();
    assert_eq!(
        Document::from_json(&session.document().to_json().unwrap()).unwrap(),
        *session.document()
    );
}

#[test]
fn deleting_all_text_keeps_the_paragraph_identity_and_properties() {
    let original = paragraph("p", "Hello, 世界!😀")
        .with_style(id("heading"))
        .with_format(ParagraphFormat {
            spacing_after: Some(Twips(240)),
            ..Default::default()
        });
    let session = DocumentSession::new(document(vec![original.clone()]));
    let mut transaction = session.begin_transaction();
    let count = transaction
        .text_coordinates(&id("p"))
        .unwrap()
        .grapheme_len()
        .get();
    let selection = selected(&transaction, ("p", 0), ("p", count));
    transaction.delete_text(&selection).unwrap();
    assert_eq!(text(&transaction, "p"), "");
    let result = paragraph_in(&transaction, "p");
    assert_eq!(result.id(), original.id());
    assert_eq!(result.style(), original.style());
    assert_eq!(result.format(), original.format());
    assert!(
        transaction
            .position(&id("p"), GraphemeOffset::new(0))
            .is_ok()
    );
}

#[test]
fn multi_paragraph_replacement_keeps_start_identity_and_end_suffix_format() {
    let start = Paragraph::new(
        id("first"),
        vec![TextRun {
            text: "First".into(),
            format: bold(),
        }],
    )
    .with_style(id("heading"))
    .with_format(ParagraphFormat {
        alignment: Some(Alignment::Center),
        ..Default::default()
    });
    let suffix_format = CharacterFormat {
        italic: Some(true),
        ..Default::default()
    };
    let end = Paragraph::new(
        id("last"),
        vec![TextRun {
            text: "Ending".into(),
            format: suffix_format.clone(),
        }],
    )
    .with_style(id("normal"));
    let original = document(vec![
        start.clone(),
        paragraph("middle", "Covered"),
        end,
        paragraph("after", "Unchanged"),
    ]);
    let session = DocumentSession::new(original.clone());
    let mut transaction = session.begin_transaction();
    let selection = selected(&transaction, ("last", 2), ("first", 2));
    transaction
        .replace_text(&selection, " + ", CharacterFormat::default())
        .unwrap();
    assert_eq!(text(&transaction, "first"), "Fi + ding");
    let result = paragraph_in(&transaction, "first");
    assert_eq!(result.style(), start.style());
    assert_eq!(result.format(), start.format());
    assert_eq!(result.runs().last().unwrap().format, suffix_format);
    assert_eq!(transaction.draft().sections[0].blocks().len(), 2);
    assert_eq!(
        paragraph_in(&transaction, "after").runs(),
        &[TextRun::new("Unchanged")]
    );
    for removed in ["middle", "last"] {
        assert!(matches!(
            transaction.position(&id(removed), GraphemeOffset::new(0)),
            Err(EditError::Position(PositionError::ParagraphNotFound { .. }))
        ));
    }
    assert_eq!(
        transaction.draft().sections[0].properties(),
        original.sections()[0].properties()
    );
}

#[test]
fn deleting_only_a_paragraph_boundary_joins_paragraphs_and_keeps_unicode_exact() {
    let session = DocumentSession::new(document(vec![
        paragraph("a", "e"),
        paragraph("b", "\u{301}😀"),
    ]));
    let mut transaction = session.begin_transaction();
    let selection = selected(&transaction, ("a", 1), ("b", 0));
    transaction.delete_text(&selection).unwrap();
    assert_eq!(text(&transaction, "a"), "e\u{301}😀");
    assert_eq!(
        transaction
            .text_coordinates(&id("a"))
            .unwrap()
            .grapheme_len()
            .get(),
        2
    );
    assert_eq!(transaction.draft().sections[0].blocks().len(), 1);
}

fn two_sections() -> DocumentSession {
    DocumentSession::new(
        Document::from_parts(
            id("document"),
            DocumentMetadata::default(),
            vec![
                Section::new(id("s1"), vec![Block::Paragraph(paragraph("a", "abc"))]),
                Section::new(id("s2"), vec![Block::Paragraph(paragraph("b", "xyz"))])
                    .with_properties(SectionProperties::default()),
            ],
            vec![
                NamedStyle::new(id("normal"), "Normal"),
                NamedStyle::new(id("heading"), "Heading"),
            ],
        )
        .unwrap(),
    )
}

#[test]
fn cross_section_text_replacement_is_rejected_without_invalidating_endpoints() {
    let session = two_sections();
    let mut transaction = session.begin_transaction();
    let selection = selected(&transaction, ("a", 1), ("b", 2));
    let before = transaction.draft().clone();
    assert_eq!(
        transaction.replace_text(&selection, "X", bold()),
        Err(EditError::CrossSectionRange)
    );
    assert_eq!(transaction.draft(), &before);
    assert_eq!(
        transaction.delete_text(&selection),
        Err(EditError::CrossSectionRange)
    );
    assert!(
        transaction
            .apply_character_format(
                &selection,
                &CharacterFormatPatch {
                    bold: PropertyChange::Set(true),
                    ..Default::default()
                }
            )
            .unwrap()
    );
}

#[test]
fn character_formatting_splits_only_selected_text_and_preserves_other_properties() {
    let original_format = CharacterFormat {
        italic: Some(true),
        font_family: Some("Noto Sans".into()),
        ..Default::default()
    };
    let session = DocumentSession::new(document(vec![Paragraph::new(
        id("p"),
        vec![TextRun {
            text: "abcdef".into(),
            format: original_format.clone(),
        }],
    )]));
    let mut transaction = session.begin_transaction();
    let selection = selected(&transaction, ("p", 1), ("p", 4));
    let patch = CharacterFormatPatch {
        bold: PropertyChange::Set(true),
        ..Default::default()
    };
    transaction
        .apply_character_format(&selection, &patch)
        .unwrap();
    let runs = paragraph_in(&transaction, "p").runs();
    assert_eq!(
        runs.iter().map(|run| run.text.as_str()).collect::<Vec<_>>(),
        ["a", "bcd", "ef"]
    );
    assert_eq!(runs[0].format, original_format);
    assert_eq!(runs[2].format, original_format);
    assert_eq!(runs[1].format.bold, Some(true));
    assert_eq!(runs[1].format.italic, Some(true));
    assert_eq!(runs[1].format.font_family.as_deref(), Some("Noto Sans"));
}

#[test]
fn formatting_a_cross_run_grapheme_affects_every_scalar_in_the_cluster() {
    let session = DocumentSession::new(document(vec![Paragraph::new(
        id("p"),
        vec![
            TextRun::new("e"),
            TextRun::new("\u{301}!"),
            TextRun::new(""),
        ],
    )]));
    let mut transaction = session.begin_transaction();
    let selection = selected(&transaction, ("p", 0), ("p", 1));
    transaction
        .apply_character_format(
            &selection,
            &CharacterFormatPatch {
                bold: PropertyChange::Set(true),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(text(&transaction, "p"), "e\u{301}!");
    let runs = paragraph_in(&transaction, "p").runs();
    assert_eq!(
        runs[0],
        TextRun {
            text: "e".into(),
            format: bold()
        }
    );
    assert_eq!(
        runs[1],
        TextRun {
            text: "\u{301}".into(),
            format: bold()
        }
    );
    assert_eq!(runs[2], TextRun::new("!"));
    assert_eq!(runs[3], TextRun::new(""));
}

#[test]
fn character_patches_distinguish_keep_clear_and_explicit_false() {
    let initial = CharacterFormat {
        bold: Some(true),
        italic: Some(true),
        underline: Some(Underline::Single),
        font_family: Some("Noto Serif".into()),
        font_size: Some(HalfPoints(24)),
        color: Some(TextColor::Rgb {
            red: 1,
            green: 2,
            blue: 3,
        }),
        hyperlink: None,
    };
    let session = DocumentSession::new(document(vec![Paragraph::new(
        id("p"),
        vec![TextRun {
            text: "A".into(),
            format: initial,
        }],
    )]));
    let mut transaction = session.begin_transaction();
    let selection = selected(&transaction, ("p", 0), ("p", 1));
    let patch = CharacterFormatPatch {
        bold: PropertyChange::Set(false),
        italic: PropertyChange::Clear,
        underline: PropertyChange::Set(Underline::None),
        font_family: PropertyChange::Keep,
        font_size: PropertyChange::Set(HalfPoints(30)),
        color: PropertyChange::Clear,
        hyperlink: PropertyChange::Keep,
    };
    transaction
        .apply_character_format(&selection, &patch)
        .unwrap();
    assert_eq!(
        paragraph_in(&transaction, "p").runs()[0].format,
        CharacterFormat {
            bold: Some(false),
            italic: None,
            underline: Some(Underline::None),
            font_family: Some("Noto Serif".into()),
            font_size: Some(HalfPoints(30)),
            color: None,
            hyperlink: None,
        }
    );
}

#[test]
fn character_formatting_can_span_sections_without_changing_section_properties() {
    let session = two_sections();
    let mut transaction = session.begin_transaction();
    let original_properties = transaction.draft().sections[1].properties().cloned();
    let selection = selected(&transaction, ("b", 2), ("a", 1));
    transaction
        .apply_character_format(
            &selection,
            &CharacterFormatPatch {
                italic: PropertyChange::Set(true),
                ..Default::default()
            },
        )
        .unwrap();
    for (value, expected) in [("a", vec!["a", "bc"]), ("b", vec!["xy", "z"])] {
        assert_eq!(
            paragraph_in(&transaction, value)
                .runs()
                .iter()
                .map(|run| run.text.as_str())
                .collect::<Vec<_>>(),
            expected
        );
    }
    assert_eq!(
        paragraph_in(&transaction, "a").runs()[0].format.italic,
        None
    );
    assert_eq!(
        paragraph_in(&transaction, "a").runs()[1].format.italic,
        Some(true)
    );
    assert_eq!(
        paragraph_in(&transaction, "b").runs()[0].format.italic,
        Some(true)
    );
    assert_eq!(
        paragraph_in(&transaction, "b").runs()[1].format.italic,
        None
    );
    assert_eq!(
        transaction.draft().sections[1].properties(),
        original_properties.as_ref()
    );
}

#[test]
fn paragraph_formatting_obeys_collapsed_and_half_open_endpoint_rules() {
    let session = DocumentSession::new(document(vec![
        paragraph("a", "A"),
        paragraph("empty", ""),
        paragraph("b", "B"),
    ]));
    let mut transaction = session.begin_transaction();
    let selection = selected(&transaction, ("a", 1), ("b", 0));
    transaction
        .apply_paragraph_format(
            &selection,
            &ParagraphFormatPatch {
                alignment: PropertyChange::Set(Alignment::Right),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(
        paragraph_in(&transaction, "a").format().alignment,
        Some(Alignment::Right)
    );
    assert_eq!(
        paragraph_in(&transaction, "empty").format().alignment,
        Some(Alignment::Right)
    );
    assert_eq!(paragraph_in(&transaction, "b").format().alignment, None);
    let collapsed = selected(&transaction, ("b", 0), ("b", 0));
    transaction
        .apply_paragraph_format(
            &collapsed,
            &ParagraphFormatPatch {
                spacing_after: PropertyChange::Set(Twips(240)),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(
        paragraph_in(&transaction, "b").format().spacing_after,
        Some(Twips(240))
    );
}

#[test]
fn paragraph_patches_cover_every_property_and_clear_inherited_overrides() {
    let session = session("A");
    let mut transaction = session.begin_transaction();
    let selection = selected(&transaction, ("p", 0), ("p", 0));
    transaction
        .apply_paragraph_format(
            &selection,
            &ParagraphFormatPatch {
                alignment: PropertyChange::Set(Alignment::Justify),
                spacing_before: PropertyChange::Set(Twips(100)),
                spacing_after: PropertyChange::Set(Twips(200)),
                line_spacing: PropertyChange::Set(LineSpacing::Multiple { value: 360 }),
                indentation: PropertyChange::Set(Indentation {
                    left: Some(Twips(720)),
                    right: None,
                    first_line: Some(Twips(-360)),
                }),
                ..Default::default()
            },
        )
        .unwrap();
    let format = paragraph_in(&transaction, "p").format();
    assert_eq!(format.alignment, Some(Alignment::Justify));
    assert_eq!(format.spacing_before, Some(Twips(100)));
    assert_eq!(format.spacing_after, Some(Twips(200)));
    assert_eq!(
        format.line_spacing,
        Some(LineSpacing::Multiple { value: 360 })
    );
    assert_eq!(
        format.indentation.as_ref().unwrap().first_line,
        Some(Twips(-360))
    );
    let selection = selected(&transaction, ("p", 0), ("p", 1));
    transaction
        .apply_paragraph_format(
            &selection,
            &ParagraphFormatPatch {
                alignment: PropertyChange::Clear,
                spacing_before: PropertyChange::Clear,
                spacing_after: PropertyChange::Clear,
                line_spacing: PropertyChange::Clear,
                indentation: PropertyChange::Clear,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(
        paragraph_in(&transaction, "p").format(),
        &ParagraphFormat::default()
    );
}

#[test]
fn paragraph_style_assignment_and_clear_preserve_direct_formatting() {
    let session = session("A");
    let mut transaction = session.begin_transaction();
    let selection = selected(&transaction, ("p", 0), ("p", 0));
    transaction
        .apply_paragraph_format(
            &selection,
            &ParagraphFormatPatch {
                alignment: PropertyChange::Set(Alignment::Center),
                ..Default::default()
            },
        )
        .unwrap();
    let selection = selected(&transaction, ("p", 0), ("p", 0));
    transaction
        .set_paragraph_style(&selection, Some(&id("heading")))
        .unwrap();
    assert_eq!(
        paragraph_in(&transaction, "p").style(),
        Some(&id("heading"))
    );
    assert_eq!(
        paragraph_in(&transaction, "p").format().alignment,
        Some(Alignment::Center)
    );
    let selection = selected(&transaction, ("p", 0), ("p", 1));
    transaction.set_paragraph_style(&selection, None).unwrap();
    assert_eq!(paragraph_in(&transaction, "p").style(), None);
    assert_eq!(
        paragraph_in(&transaction, "p").format().alignment,
        Some(Alignment::Center)
    );
}

#[test]
fn identical_formatting_and_collapsed_text_edits_preserve_runs_and_endpoints() {
    let original = document(vec![Paragraph::new(
        id("p"),
        vec![
            TextRun {
                text: "abcd".into(),
                format: bold(),
            },
            TextRun::new(""),
        ],
    )]);
    let mut session = DocumentSession::new(original.clone());
    let token = session.revision();
    let mut transaction = session.begin_transaction();
    let at = transaction
        .position(&id("p"), GraphemeOffset::new(2))
        .unwrap();
    let selection = selected(&transaction, ("p", 1), ("p", 3));
    assert_eq!(
        transaction.insert_text(&at, "", CharacterFormat::default()),
        Ok(false)
    );
    assert_eq!(
        transaction.apply_character_format(
            &selection,
            &CharacterFormatPatch {
                bold: PropertyChange::Set(true),
                ..Default::default()
            }
        ),
        Ok(false)
    );
    assert_eq!(
        transaction.apply_character_format(&selection, &CharacterFormatPatch::default()),
        Ok(false)
    );
    let collapsed = transaction.selection(at.clone(), at.clone()).unwrap();
    assert_eq!(transaction.delete_text(&collapsed), Ok(false));
    assert_eq!(
        transaction.apply_character_format(
            &collapsed,
            &CharacterFormatPatch {
                italic: PropertyChange::Set(true),
                ..Default::default()
            }
        ),
        Ok(false)
    );
    assert_eq!(transaction.draft().sections, original.sections());
    let outcome = session.commit(transaction).unwrap();
    assert!(!outcome.changed());
    assert_eq!(session.revision(), token);
    assert!(!session.can_undo());
}

#[test]
fn identical_text_and_format_replacement_is_a_structural_no_op() {
    let original = document(vec![Paragraph::new(
        id("p"),
        vec![
            TextRun {
                text: "ab".into(),
                format: bold(),
            },
            TextRun {
                text: "cd".into(),
                format: bold(),
            },
        ],
    )]);
    let session = DocumentSession::new(original.clone());
    let mut transaction = session.begin_transaction();
    let selection = selected(&transaction, ("p", 1), ("p", 3));
    assert_eq!(
        transaction.replace_text(&selection, "bc", bold()),
        Ok(false)
    );
    assert_eq!(transaction.draft().sections, original.sections());
    assert_eq!(
        transaction.apply_character_format(&selection, &CharacterFormatPatch::default()),
        Ok(false)
    );
}

#[test]
fn invalid_requested_properties_are_rejected_even_for_empty_operations() {
    let session = session("A");
    let mut transaction = session.begin_transaction();
    let at = transaction
        .position(&id("p"), GraphemeOffset::new(0))
        .unwrap();
    let collapsed = transaction.selection(at.clone(), at.clone()).unwrap();
    let before = transaction.draft().clone();
    for format in [
        CharacterFormat {
            font_size: Some(HalfPoints(0)),
            ..Default::default()
        },
        CharacterFormat {
            font_family: Some(" ".into()),
            ..Default::default()
        },
    ] {
        assert!(matches!(
            transaction.insert_text(&at, "", format),
            Err(EditError::InvalidDocument(
                ValidationError::InvalidFormatting { .. }
            ))
        ));
    }
    assert!(matches!(
        transaction.apply_character_format(
            &collapsed,
            &CharacterFormatPatch {
                font_size: PropertyChange::Set(HalfPoints(0)),
                ..Default::default()
            }
        ),
        Err(EditError::InvalidDocument(
            ValidationError::InvalidFormatting { .. }
        ))
    ));
    assert!(matches!(
        transaction.apply_paragraph_format(
            &collapsed,
            &ParagraphFormatPatch {
                spacing_after: PropertyChange::Set(Twips(-1)),
                ..Default::default()
            }
        ),
        Err(EditError::InvalidDocument(
            ValidationError::InvalidFormatting { .. }
        ))
    ));
    assert!(matches!(
        transaction.set_paragraph_style(&collapsed, Some(&id("missing"))),
        Err(EditError::InvalidDocument(
            ValidationError::MissingStyle { .. }
        ))
    ));
    assert_eq!(transaction.draft(), &before);
    assert!(
        transaction
            .insert_text(&at, "Valid", CharacterFormat::default())
            .unwrap()
    );
}

#[test]
fn endpoints_are_rejected_after_changed_operations_and_raw_mutable_access() {
    let session = session("abc");
    let mut transaction = session.begin_transaction();
    let start = transaction
        .position(&id("p"), GraphemeOffset::new(0))
        .unwrap();
    let end = transaction
        .position(&id("p"), GraphemeOffset::new(3))
        .unwrap();
    let selection = transaction.selection(start.clone(), end.clone()).unwrap();
    transaction
        .insert_text(&start, "X", CharacterFormat::default())
        .unwrap();
    assert_eq!(
        transaction.insert_text(&end, "Y", bold()),
        Err(EditError::WrongContext)
    );
    assert_eq!(
        transaction.delete_text(&selection),
        Err(EditError::WrongContext)
    );
    assert_eq!(
        transaction.selection(start, end),
        Err(EditError::WrongContext)
    );
    let at = transaction
        .position(&id("p"), GraphemeOffset::new(0))
        .unwrap();
    let _ = transaction.draft_mut();
    assert_eq!(
        transaction.insert_text(&at, "Z", bold()),
        Err(EditError::WrongContext)
    );
}

#[test]
fn endpoint_provenance_is_distinct_between_transactions_at_the_same_session_revision() {
    let session = session("abc");
    let first = session.begin_transaction();
    let mut second = session.begin_transaction();
    let foreign = first.position(&id("p"), GraphemeOffset::new(0)).unwrap();
    let local = second.position(&id("p"), GraphemeOffset::new(0)).unwrap();
    assert_eq!(foreign.position(), local.position());
    assert_ne!(foreign, local);
    assert_eq!(
        second.selection(foreign.clone(), local),
        Err(EditError::WrongContext)
    );
    assert_eq!(
        second.insert_text(&foreign, "X", bold()),
        Err(EditError::WrongContext)
    );
    let foreign_range = selected(&first, ("p", 0), ("p", 1));
    assert_eq!(
        second.delete_text(&foreign_range),
        Err(EditError::WrongContext)
    );
    assert_eq!(text(&second, "p"), "abc");
}

#[test]
fn invalid_drafts_and_out_of_bounds_positions_produce_structured_errors() {
    let session = session("😀");
    let mut transaction = session.begin_transaction();
    assert!(matches!(
        transaction.position(&id("missing"), GraphemeOffset::new(0)),
        Err(EditError::Position(PositionError::ParagraphNotFound { .. }))
    ));
    assert!(matches!(
        transaction.position(&id("p"), GraphemeOffset::new(usize::MAX)),
        Err(EditError::Position(PositionError::OutOfBounds { .. }))
    ));
    transaction
        .draft_mut()
        .styles
        .push(NamedStyle::new(id("p"), "Duplicate entity"));
    assert!(matches!(
        transaction.position(&id("p"), GraphemeOffset::new(0)),
        Err(EditError::InvalidDocument(
            ValidationError::DuplicateId { .. }
        ))
    ));
    assert!(matches!(
        transaction.text_coordinates(&id("p")),
        Err(EditError::InvalidDocument(
            ValidationError::DuplicateId { .. }
        ))
    ));
}

#[test]
fn multiple_operations_form_one_commit_and_one_exact_undo_redo_unit() {
    let original = document(vec![paragraph("p", "e\u{301}😀")]);
    let mut session = DocumentSession::new(original.clone());
    let mut transaction = session.begin_transaction();
    let at = transaction
        .position(&id("p"), GraphemeOffset::new(2))
        .unwrap();
    transaction
        .insert_text(&at, "!", CharacterFormat::default())
        .unwrap();
    let selection = selected(&transaction, ("p", 0), ("p", 2));
    transaction
        .apply_character_format(
            &selection,
            &CharacterFormatPatch {
                bold: PropertyChange::Set(true),
                ..Default::default()
            },
        )
        .unwrap();
    let collapsed = selected(&transaction, ("p", 0), ("p", 0));
    transaction
        .apply_paragraph_format(
            &collapsed,
            &ParagraphFormatPatch {
                alignment: PropertyChange::Set(Alignment::Right),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(session.document(), &original);
    assert_eq!(session.commit(transaction).unwrap().revision().number(), 1);
    let edited = session.document().clone();
    assert!(session.undo().unwrap().changed());
    assert_eq!(session.document(), &original);
    assert!(!session.can_undo());
    session.redo().unwrap();
    assert_eq!(session.document(), &edited);
    assert_eq!(session.revision().number(), 3);
}

#[test]
fn replacement_partitioning_preserves_every_unicode_prefix_and_suffix() {
    for input in ["", "e\u{301}😀x", "🇺🇸a👩🏽‍💻", "क्ष中文", "a\r\nb"] {
        let runs = input
            .chars()
            .enumerate()
            .flat_map(|(index, scalar)| {
                [
                    TextRun {
                        text: scalar.to_string(),
                        format: CharacterFormat {
                            bold: Some(index % 2 == 0),
                            ..Default::default()
                        },
                    },
                    TextRun::new(""),
                ]
            })
            .collect();
        let original = document(vec![Paragraph::new(id("p"), runs)]);
        let coordinates = original.text_coordinates(&id("p")).unwrap();
        let count = coordinates.grapheme_len().get();
        for start in 0..=count {
            for end in start..=count {
                let start_byte = coordinates
                    .grapheme_to_utf8(GraphemeOffset::new(start))
                    .unwrap()
                    .get();
                let end_byte = coordinates
                    .grapheme_to_utf8(GraphemeOffset::new(end))
                    .unwrap()
                    .get();
                let expected = format!("{}Ω{}", &input[..start_byte], &input[end_byte..]);
                let session = DocumentSession::new(original.clone());
                let mut transaction = session.begin_transaction();
                let selection = selected(&transaction, ("p", end), ("p", start));
                transaction
                    .replace_text(&selection, "Ω", CharacterFormat::default())
                    .unwrap();
                assert_eq!(
                    text(&transaction, "p"),
                    expected,
                    "{input:?} range {start}..{end}"
                );
            }
        }
    }
}

#[test]
fn empty_runs_at_replacement_cuts_are_partitioned_without_duplication() {
    let leading = TextRun {
        text: "".into(),
        format: bold(),
    };
    let middle = TextRun {
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
    let original = document(vec![Paragraph::new(
        id("p"),
        vec![
            leading.clone(),
            TextRun::new("ab"),
            middle.clone(),
            TextRun::new("cd"),
            trailing.clone(),
        ],
    )]);
    let session = DocumentSession::new(original);
    let mut insertion = session.begin_transaction();
    let at = insertion
        .position(&id("p"), GraphemeOffset::new(2))
        .unwrap();
    insertion
        .insert_text(&at, "X", CharacterFormat::default())
        .unwrap();
    assert_eq!(
        paragraph_in(&insertion, "p").runs(),
        &[
            leading.clone(),
            TextRun::new("abX"),
            middle,
            TextRun::new("cd"),
            trailing.clone(),
        ]
    );
    let mut replacement = session.begin_transaction();
    let selection = selected(&replacement, ("p", 1), ("p", 3));
    replacement
        .replace_text(&selection, "X", CharacterFormat::default())
        .unwrap();
    assert_eq!(
        paragraph_in(&replacement, "p").runs(),
        &[
            leading,
            TextRun::new("a"),
            TextRun::new("X"),
            TextRun::new("d"),
            trailing,
        ]
    );
}

#[test]
fn paragraph_formatting_and_styles_can_span_sections_with_half_open_endpoints() {
    let session = two_sections();
    let mut transaction = session.begin_transaction();
    let selection = selected(&transaction, ("a", 1), ("b", 2));
    transaction
        .apply_paragraph_format(
            &selection,
            &ParagraphFormatPatch {
                alignment: PropertyChange::Set(Alignment::Right),
                ..Default::default()
            },
        )
        .unwrap();
    for value in ["a", "b"] {
        assert_eq!(
            paragraph_in(&transaction, value).format().alignment,
            Some(Alignment::Right)
        );
    }
    let selection = selected(&transaction, ("a", 3), ("b", 0));
    transaction
        .set_paragraph_style(&selection, Some(&id("heading")))
        .unwrap();
    assert_eq!(
        paragraph_in(&transaction, "a").style(),
        Some(&id("heading"))
    );
    assert_eq!(paragraph_in(&transaction, "b").style(), None);
    assert_eq!(transaction.draft().sections.len(), 2);
    assert_eq!(
        transaction.draft().sections[1].properties(),
        Some(&SectionProperties::default())
    );
}

#[test]
fn editing_a_detached_draft_does_not_bypass_stale_commit_checks() {
    let mut session = session("abc");
    let mut first = session.begin_transaction();
    let mut second = session.begin_transaction();
    let first_at = first.position(&id("p"), GraphemeOffset::new(0)).unwrap();
    first
        .insert_text(&first_at, "Winner", CharacterFormat::default())
        .unwrap();
    session.commit(first).unwrap();
    let second_at = second.position(&id("p"), GraphemeOffset::new(0)).unwrap();
    second
        .insert_text(&second_at, "Loser", CharacterFormat::default())
        .unwrap();
    let committed = session.document().clone();
    assert!(matches!(
        session.commit(second),
        Err(TransactionError::StaleRevision {
            expected: 1,
            actual: 0
        })
    ));
    assert_eq!(session.document(), &committed);
    session.undo().unwrap();
    assert_eq!(
        session
            .document()
            .text_coordinates(&id("p"))
            .unwrap()
            .text(),
        "abc"
    );
}
