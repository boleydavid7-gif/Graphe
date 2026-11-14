//! Generic page geometry and paragraph pagination-property regression tests.

use office_core::{
    Block, Document, DocumentMetadata, DocumentSession, GraphemeOffset, Id, PageMargins,
    PageOrientation, PageSize, Paragraph, ParagraphFormat, ParagraphFormatPatch, PropertyChange,
    Section, SectionProperties, TextRun, Twips, ValidationError,
};

fn id(value: &str) -> Id {
    Id::new(value).unwrap()
}

fn letter() -> PageSize {
    PageSize {
        width: Twips(12_240),
        height: Twips(15_840),
        orientation: None,
    }
}

fn margins() -> PageMargins {
    PageMargins {
        top: Twips(1_440),
        right: Twips(1_440),
        bottom: Twips(1_440),
        left: Twips(1_440),
        header: Twips(720),
        footer: Twips(720),
        gutter: Twips(0),
    }
}

fn document(properties: SectionProperties) -> Result<Document, ValidationError> {
    Document::from_parts(
        id("document"),
        DocumentMetadata::default(),
        vec![
            Section::new(
                id("section"),
                vec![Block::Paragraph(Paragraph::new(
                    id("paragraph"),
                    vec![TextRun::new("Geometry 🌍")],
                ))],
            )
            .with_properties(properties),
        ],
        vec![],
    )
}

#[test]
fn page_size_margins_and_explicit_orientation_are_preserved_without_rotation() {
    for orientation in [
        None,
        Some(PageOrientation::Portrait),
        Some(PageOrientation::Landscape),
    ] {
        let size = PageSize {
            orientation,
            ..letter()
        };
        let properties = SectionProperties {
            page_size: Some(size),
            page_margins: Some(margins()),
        };
        let original = document(properties.clone()).unwrap();
        let restored = Document::from_json(&original.to_json().unwrap()).unwrap();
        assert_eq!(restored, original);
        assert_eq!(restored.sections()[0].properties(), Some(&properties));
        assert_eq!(
            restored.sections()[0]
                .properties()
                .unwrap()
                .page_size
                .unwrap()
                .width,
            Twips(12_240)
        );
    }
}

#[test]
fn absent_geometry_and_independently_unspecified_records_remain_distinct() {
    for properties in [
        SectionProperties::default(),
        SectionProperties {
            page_size: Some(letter()),
            page_margins: None,
        },
        SectionProperties {
            page_size: None,
            page_margins: Some(margins()),
        },
    ] {
        let original = document(properties.clone()).unwrap();
        assert_eq!(original.sections()[0].properties(), Some(&properties));
        assert_eq!(
            Document::from_json(&original.to_json().unwrap()).unwrap(),
            original
        );
    }
    let absent = Document::from_parts(
        id("document"),
        Default::default(),
        vec![Section::new(id("section"), vec![])],
        vec![],
    )
    .unwrap();
    assert!(absent.sections()[0].properties().is_none());
}

#[test]
fn nonpositive_dimensions_are_rejected_at_model_validation() {
    for invalid in [0, -1, i32::MIN] {
        for size in [
            PageSize {
                width: Twips(invalid),
                ..letter()
            },
            PageSize {
                height: Twips(invalid),
                ..letter()
            },
        ] {
            assert!(
                matches!(document(SectionProperties { page_size: Some(size), page_margins: None }), Err(ValidationError::InvalidFormatting { path, .. }) if path.starts_with("sections[0].properties.page_size."))
            );
        }
    }
}

#[test]
fn negative_page_margin_profile_is_rejected_without_clamping() {
    let original = serde_json::to_value(margins()).unwrap();
    for field in [
        "top", "right", "bottom", "left", "header", "footer", "gutter",
    ] {
        let mut value = original.clone();
        value[field] = serde_json::json!(-1);
        let margins = serde_json::from_value(value).unwrap();
        assert!(
            matches!(document(SectionProperties { page_size: None, page_margins: Some(margins) }), Err(ValidationError::InvalidFormatting { path, .. }) if path == format!("sections[0].properties.page_margins.{field}"))
        );
    }
}

#[test]
fn usable_body_area_must_be_positive_and_uses_wide_arithmetic() {
    let zero = PageMargins {
        top: Twips(0),
        right: Twips(0),
        bottom: Twips(0),
        left: Twips(0),
        header: Twips(0),
        footer: Twips(0),
        gutter: Twips(0),
    };
    for invalid in [
        PageMargins {
            left: Twips(12_240),
            ..zero
        },
        PageMargins {
            gutter: Twips(12_240),
            ..zero
        },
        PageMargins {
            top: Twips(15_840),
            ..zero
        },
        PageMargins {
            bottom: Twips(15_840),
            ..zero
        },
        PageMargins {
            left: Twips(i32::MAX),
            right: Twips(i32::MAX),
            gutter: Twips(i32::MAX),
            ..zero
        },
        PageMargins {
            top: Twips(i32::MAX),
            bottom: Twips(i32::MAX),
            ..zero
        },
    ] {
        assert!(
            matches!(document(SectionProperties { page_size: Some(letter()), page_margins: Some(invalid) }), Err(ValidationError::InvalidFormatting { path, .. }) if path == "sections[0].properties")
        );
    }
    let one_twip = PageMargins {
        left: Twips(12_239),
        top: Twips(15_839),
        ..zero
    };
    assert!(
        document(SectionProperties {
            page_size: Some(letter()),
            page_margins: Some(one_twip)
        })
        .is_ok()
    );
    assert!(
        document(SectionProperties {
            page_size: Some(PageSize {
                width: Twips(i32::MAX),
                height: Twips(i32::MAX),
                orientation: None
            }),
            page_margins: Some(zero)
        })
        .is_ok()
    );
}

#[test]
fn page_break_before_has_explicit_keep_set_false_clear_and_history_semantics() {
    let original = document(SectionProperties::default()).unwrap();
    let mut session = DocumentSession::new(original.clone());
    for (change, expected) in [
        (PropertyChange::Set(true), Some(true)),
        (PropertyChange::Keep, Some(true)),
        (PropertyChange::Set(false), Some(false)),
        (PropertyChange::Clear, None),
    ] {
        let mut transaction = session.begin_transaction();
        let start = transaction
            .position(&id("paragraph"), GraphemeOffset::new(0))
            .unwrap();
        let selection = transaction.selection(start.clone(), start).unwrap();
        transaction
            .apply_paragraph_format(
                &selection,
                &ParagraphFormatPatch {
                    page_break_before: change,
                    ..Default::default()
                },
            )
            .unwrap();
        session.commit(transaction).unwrap();
        let Block::Paragraph(paragraph) = &session.document().sections()[0].blocks()[0];
        assert_eq!(paragraph.format().page_break_before, expected);
    }
    session.undo().unwrap();
    let Block::Paragraph(paragraph) = &session.document().sections()[0].blocks()[0];
    assert_eq!(paragraph.format().page_break_before, Some(false));
    session.redo().unwrap();
    assert_eq!(session.document(), &original);
}

#[test]
fn geometry_is_preserved_by_edit_transactions_and_exact_history() {
    let original = document(SectionProperties {
        page_size: Some(letter()),
        page_margins: Some(margins()),
    })
    .unwrap();
    let mut session = DocumentSession::new(original.clone());
    let mut transaction = session.begin_transaction();
    transaction.draft_mut().metadata.title = "Edited title".into();
    session.commit(transaction).unwrap();
    let edited = session.document().clone();
    assert_eq!(
        edited.sections()[0].properties(),
        original.sections()[0].properties()
    );
    session.undo().unwrap();
    assert_eq!(session.document(), &original);
    session.redo().unwrap();
    assert_eq!(session.document(), &edited);
    assert_eq!(
        Document::from_json(&edited.to_json().unwrap()).unwrap(),
        edited
    );
}

#[test]
fn page_break_before_round_trips_in_direct_and_named_paragraph_properties() {
    let style = office_core::NamedStyle {
        paragraph_format: ParagraphFormat {
            page_break_before: Some(false),
            ..Default::default()
        },
        ..office_core::NamedStyle::new(id("style"), "Body")
    };
    let paragraph = Paragraph::new(id("paragraph"), vec![])
        .with_style(style.id.clone())
        .with_format(ParagraphFormat {
            page_break_before: Some(true),
            ..Default::default()
        });
    let original = Document::from_parts(
        id("document"),
        Default::default(),
        vec![Section::new(
            id("section"),
            vec![Block::Paragraph(paragraph)],
        )],
        vec![style],
    )
    .unwrap();
    assert_eq!(
        Document::from_json(&original.to_json().unwrap()).unwrap(),
        original
    );
}

#[test]
fn geometry_json_rejects_unknown_missing_duplicate_and_invalid_typed_properties() {
    let original = document(SectionProperties {
        page_size: Some(letter()),
        page_margins: Some(margins()),
    })
    .unwrap();
    let serialized = original.to_json().unwrap();
    let json: serde_json::Value = serde_json::from_str(&serialized).unwrap();
    for (field, value) in [
        ("orientation", serde_json::json!("sideways")),
        ("width", serde_json::json!(u64::MAX)),
        ("width", serde_json::json!(1.5)),
        ("unknown", serde_json::json!(0)),
    ] {
        let mut malformed = json.clone();
        malformed["sections"][0]["properties"]["page_size"][field] = value;
        assert!(Document::from_json(&malformed.to_string()).is_err());
        assert!(serde_json::from_value::<Document>(malformed).is_err());
    }
    let mut missing = json.clone();
    missing["sections"][0]["properties"]["page_margins"]
        .as_object_mut()
        .unwrap()
        .remove("gutter");
    assert!(Document::from_json(&missing.to_string()).is_err());
    let duplicate = serialized.replace("\"width\": 12240", "\"width\": 12240, \"width\": 1");
    assert_ne!(duplicate, serialized);
    assert!(Document::from_json(&duplicate).is_err());
    assert!(serde_json::from_str::<Document>(&duplicate).is_err());
}
