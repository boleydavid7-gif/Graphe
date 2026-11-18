//! Public document-model contract and schema compatibility regression tests.

use office_core::{
    Alignment, Block, CharacterFormat, Document, DocumentError, DocumentMetadata, HalfPoints, Id,
    Indentation, LineSpacing, NamedStyle, Paragraph, ParagraphFormat, SCHEMA_VERSION, Section,
    SectionProperties, TextColor, TextRun, Twips, Underline, ValidationError,
};
use serde_json::{Value, json};

fn id(value: &str) -> Id {
    Id::new(value).unwrap()
}

fn assemble(sections: Vec<Section>, styles: Vec<NamedStyle>) -> Result<Document, ValidationError> {
    Document::from_parts(
        id("document"),
        DocumentMetadata::default(),
        sections,
        styles,
    )
}

fn paragraph_document(paragraph: Paragraph) -> Result<Document, ValidationError> {
    assemble(
        vec![Section::new(
            id("section"),
            vec![Block::Paragraph(paragraph)],
        )],
        vec![],
    )
}

fn simple_json() -> Value {
    serde_json::to_value(
        paragraph_document(Paragraph::new(id("paragraph"), vec![TextRun::new("Hello")])).unwrap(),
    )
    .unwrap()
}

fn load_json(value: Value) -> Result<Document, DocumentError> {
    Document::from_json(&serde_json::to_string(&value).unwrap())
}

fn first_paragraph(document: &Document) -> &Paragraph {
    let Block::Paragraph(paragraph) = &document.sections()[0].blocks()[0] else {
        panic!("expected paragraph");
    };
    paragraph
}

#[test]
fn empty_document_has_stable_identity_schema_and_no_synthesized_content() {
    let document = Document::new(id("blank")).unwrap();
    assert_eq!(document.id().as_str(), "blank");
    assert_eq!(document.schema_version(), SCHEMA_VERSION);
    assert_eq!(document.metadata().title, "");
    assert!(document.sections().is_empty());
    assert!(document.styles().is_empty());
    assert_eq!(document.validate(), Ok(()));
    assert_eq!(
        Document::from_json(&document.to_json().unwrap()).unwrap(),
        document
    );
}

#[test]
fn ordered_sections_blocks_and_runs_survive_serialization() {
    let sections = vec![
        Section::new(
            id("opening"),
            vec![
                Block::Paragraph(Paragraph::new(
                    id("first"),
                    vec![TextRun::new("First "), TextRun::new("paragraph")],
                )),
                Block::Paragraph(Paragraph::new(id("empty-paragraph"), vec![])),
            ],
        )
        .with_properties(SectionProperties::default()),
        Section::new(id("closing"), vec![]),
    ];
    let document = Document::from_parts(
        id("document"),
        DocumentMetadata {
            title: "Report".into(),
        },
        sections,
        vec![],
    )
    .unwrap();
    assert_eq!(document.sections()[0].id().as_str(), "opening");
    assert_eq!(document.sections()[1].id().as_str(), "closing");
    assert_eq!(document.sections()[0].blocks().len(), 2);
    assert_eq!(first_paragraph(&document).runs()[0].text, "First ");
    assert_eq!(first_paragraph(&document).runs()[1].text, "paragraph");
    assert!(document.sections()[0].properties().is_some());
    assert!(document.sections()[1].properties().is_none());
    assert_eq!(
        Document::from_json(&document.to_json().unwrap()).unwrap(),
        document
    );
}

#[test]
fn all_character_and_paragraph_properties_round_trip_without_resolving_defaults() {
    let character = CharacterFormat {
        bold: Some(false),
        italic: Some(true),
        underline: Some(Underline::Double),
        font_family: Some("Noto Serif".into()),
        font_size: Some(HalfPoints(23)),
        color: Some(TextColor::Rgb {
            red: 12,
            green: 34,
            blue: 255,
        }),
        hyperlink: None,
    };
    let format = ParagraphFormat {
        alignment: Some(Alignment::Justify),
        spacing_before: Some(Twips(120)),
        spacing_after: Some(Twips(0)),
        line_spacing: Some(LineSpacing::Multiple { value: 360 }),
        indentation: Some(Indentation {
            left: Some(Twips(720)),
            right: Some(Twips(-120)),
            first_line: Some(Twips(-360)),
        }),
        page_break_before: Some(false),
    };
    let paragraph = Paragraph::new(
        id("paragraph"),
        vec![
            TextRun {
                text: "Formatted".into(),
                format: character.clone(),
            },
            TextRun::new(""),
        ],
    )
    .with_format(format.clone());
    let document = paragraph_document(paragraph).unwrap();
    let decoded = Document::from_json(&document.to_json().unwrap()).unwrap();
    assert_eq!(first_paragraph(&decoded).format(), &format);
    assert_eq!(first_paragraph(&decoded).runs()[0].format, character);
    assert_eq!(first_paragraph(&decoded).runs()[0].format.bold, Some(false));
    assert_eq!(first_paragraph(&decoded).runs()[1].format.bold, None);
    assert_eq!(decoded, document);
}

#[test]
fn typed_formatting_variants_preserve_explicit_choices() {
    for alignment in [
        Alignment::Left,
        Alignment::Center,
        Alignment::Right,
        Alignment::Justify,
    ] {
        for spacing in [
            LineSpacing::Multiple { value: 240 },
            LineSpacing::Exact { height: Twips(240) },
            LineSpacing::AtLeast { height: Twips(200) },
        ] {
            let paragraph = Paragraph::new(id("paragraph"), vec![]).with_format(ParagraphFormat {
                alignment: Some(alignment),
                line_spacing: Some(spacing),
                ..Default::default()
            });
            let document = paragraph_document(paragraph).unwrap();
            assert_eq!(
                Document::from_json(&document.to_json().unwrap()).unwrap(),
                document
            );
        }
    }
    for underline in [Underline::None, Underline::Single, Underline::Double] {
        let document = paragraph_document(Paragraph::new(
            id("paragraph"),
            vec![TextRun {
                text: "Underline".into(),
                format: CharacterFormat {
                    underline: Some(underline),
                    color: Some(TextColor::Auto {}),
                    ..Default::default()
                },
            }],
        ))
        .unwrap();
        assert_eq!(
            Document::from_json(&document.to_json().unwrap()).unwrap(),
            document
        );
    }
}

#[test]
fn named_style_references_and_inheritance_are_preserved() {
    let mut normal = NamedStyle::new(id("normal"), "Normal");
    normal.character_format.bold = Some(true);
    let mut heading = NamedStyle::new(id("heading"), "Heading 1");
    heading.based_on = Some(id("normal"));
    heading.paragraph_format.alignment = Some(Alignment::Center);
    let paragraph =
        Paragraph::new(id("paragraph"), vec![TextRun::new("Heading")]).with_style(id("heading"));
    // Forward style references are valid; order does not impose inheritance order.
    let document = assemble(
        vec![Section::new(
            id("section"),
            vec![Block::Paragraph(paragraph)],
        )],
        vec![heading, normal],
    )
    .unwrap();
    assert_eq!(
        first_paragraph(&document).style().unwrap().as_str(),
        "heading"
    );
    assert_eq!(
        document.styles()[0].based_on.as_ref().unwrap().as_str(),
        "normal"
    );
    assert_eq!(
        Document::from_json(&document.to_json().unwrap()).unwrap(),
        document
    );
}

#[test]
fn unresolved_paragraph_and_parent_styles_return_structured_errors() {
    let result =
        paragraph_document(Paragraph::new(id("paragraph"), vec![]).with_style(id("missing")));
    assert!(
        matches!(result, Err(ValidationError::MissingStyle { id, path })
        if id == "missing" && path.ends_with(".style"))
    );

    let mut style = NamedStyle::new(id("style"), "Body");
    style.based_on = Some(id("missing-parent"));
    assert!(
        matches!(assemble(vec![], vec![style]), Err(ValidationError::MissingStyle { id, path })
        if id == "missing-parent" && path == "styles[0].based_on")
    );
}

#[test]
fn cyclic_and_self_referencing_style_graphs_are_invalid() {
    let mut first = NamedStyle::new(id("first"), "First");
    first.based_on = Some(id("second"));
    let mut second = NamedStyle::new(id("second"), "Second");
    second.based_on = Some(id("first"));
    assert!(matches!(
        assemble(vec![], vec![first, second]),
        Err(ValidationError::StyleCycle { .. })
    ));
    let mut own_parent = NamedStyle::new(id("self"), "Self");
    own_parent.based_on = Some(id("self"));
    assert!(
        matches!(assemble(vec![], vec![own_parent]), Err(ValidationError::StyleCycle { id }) if id == "self")
    );
}

#[test]
fn duplicate_ids_are_rejected_across_entity_kinds_and_sections() {
    let invalid_documents = [
        assemble(vec![Section::new(id("document"), vec![])], vec![]),
        assemble(
            vec![
                Section::new(id("shared"), vec![]),
                Section::new(id("shared"), vec![]),
            ],
            vec![],
        ),
        assemble(
            vec![Section::new(
                id("shared"),
                vec![Block::Paragraph(Paragraph::new(id("shared"), vec![]))],
            )],
            vec![],
        ),
        assemble(
            vec![Section::new(
                id("section"),
                vec![Block::Paragraph(Paragraph::new(id("style"), vec![]))],
            )],
            vec![NamedStyle::new(id("style"), "Style")],
        ),
        assemble(vec![], vec![NamedStyle::new(id("document"), "Style")]),
        assemble(
            vec![],
            vec![
                NamedStyle::new(id("style"), "First"),
                NamedStyle::new(id("style"), "Second"),
            ],
        ),
    ];
    for result in invalid_documents {
        assert!(matches!(result, Err(ValidationError::DuplicateId { .. })));
    }
}

#[test]
fn style_names_must_be_nonblank_and_unique() {
    assert!(matches!(
        assemble(vec![], vec![NamedStyle::new(id("style"), " \t")]),
        Err(ValidationError::InvalidStyleName { .. })
    ));
    assert!(matches!(
        assemble(
            vec![],
            vec![
                NamedStyle::new(id("a"), "Body"),
                NamedStyle::new(id("b"), "Body")
            ]
        ),
        Err(ValidationError::InvalidStyleName { .. })
    ));
}

#[test]
fn invalid_font_sizes_and_family_names_are_rejected_with_property_paths() {
    for (format, field) in [
        (
            CharacterFormat {
                font_size: Some(HalfPoints(0)),
                ..Default::default()
            },
            "font_size",
        ),
        (
            CharacterFormat {
                font_family: Some(" \t".into()),
                ..Default::default()
            },
            "font_family",
        ),
    ] {
        let result = paragraph_document(Paragraph::new(
            id("paragraph"),
            vec![TextRun {
                text: "Bad format".into(),
                format,
            }],
        ));
        assert!(
            matches!(result, Err(ValidationError::InvalidFormatting { path, reason })
            if path.ends_with(field) && path.contains("runs[0]") && !reason.is_empty())
        );
    }
}

#[test]
fn negative_spacing_and_nonpositive_line_heights_are_rejected() {
    for format in [
        ParagraphFormat {
            spacing_before: Some(Twips(-1)),
            ..Default::default()
        },
        ParagraphFormat {
            spacing_after: Some(Twips(-1)),
            ..Default::default()
        },
        ParagraphFormat {
            line_spacing: Some(LineSpacing::Multiple { value: 0 }),
            ..Default::default()
        },
        ParagraphFormat {
            line_spacing: Some(LineSpacing::Exact { height: Twips(0) }),
            ..Default::default()
        },
        ParagraphFormat {
            line_spacing: Some(LineSpacing::Exact { height: Twips(-1) }),
            ..Default::default()
        },
        ParagraphFormat {
            line_spacing: Some(LineSpacing::AtLeast { height: Twips(0) }),
            ..Default::default()
        },
        ParagraphFormat {
            line_spacing: Some(LineSpacing::AtLeast { height: Twips(-1) }),
            ..Default::default()
        },
    ] {
        assert!(matches!(
            paragraph_document(Paragraph::new(id("paragraph"), vec![]).with_format(format)),
            Err(ValidationError::InvalidFormatting { .. })
        ));
    }
}

#[test]
fn invalid_style_formatting_is_validated_even_when_no_paragraph_uses_it() {
    let mut style = NamedStyle::new(id("style"), "Bad style");
    style.character_format.font_size = Some(HalfPoints(0));
    assert!(
        matches!(assemble(vec![], vec![style]), Err(ValidationError::InvalidFormatting { path, .. })
        if path == "styles[0].character_format.font_size")
    );
    let mut style = NamedStyle::new(id("style"), "Bad style");
    style.paragraph_format.spacing_after = Some(Twips(-1));
    assert!(
        matches!(assemble(vec![], vec![style]), Err(ValidationError::InvalidFormatting { path, .. })
        if path == "styles[0].paragraph_format.spacing_after")
    );
}

#[test]
fn unicode_is_preserved_without_normalization_or_byte_position_assumptions() {
    let text = "Cafe\u{301} / Café / 👩🏽‍💻 / 👨‍👩‍👧‍👦 / العربية / 中文";
    let document =
        paragraph_document(Paragraph::new(id("paragraph"), vec![TextRun::new(text)])).unwrap();
    let decoded = Document::from_json(&document.to_json().unwrap()).unwrap();
    let actual = &first_paragraph(&decoded).runs()[0].text;
    assert_eq!(actual.as_bytes(), text.as_bytes());
    assert!(actual.len() > actual.chars().count());
    assert!(actual.contains("e\u{301}"));
    assert!(actual.contains('é'));
}

#[test]
fn future_and_unsupported_old_schema_versions_are_rejected_before_reading_model_shape() {
    for version in [0, 7, u64::MAX] {
        let error =
            load_json(json!({ "schema_version": version, "future_payload": true })).unwrap_err();
        assert!(
            matches!(error, DocumentError::UnsupportedSchemaVersion { found, supported }
            if found == version && supported == SCHEMA_VERSION)
        );
    }
}

#[test]
fn malformed_json_and_missing_required_fields_are_rejected() {
    for malformed in ["{", "[]", "null", "{}", "{\"schema_version\":1}"] {
        assert!(matches!(
            Document::from_json(malformed),
            Err(DocumentError::Json(_))
        ));
    }
    for field in ["id", "metadata", "sections", "styles"] {
        let mut value = simple_json();
        value.as_object_mut().unwrap().remove(field);
        assert!(
            matches!(load_json(value), Err(DocumentError::Json(_))),
            "missing {field}"
        );
    }
}

#[test]
fn unknown_fields_and_block_kinds_are_rejected_instead_of_lost() {
    let pointers = [
        "",
        "/metadata",
        "/sections/0",
        "/sections/0/blocks/0",
        "/sections/0/blocks/0/content",
        "/sections/0/blocks/0/content/format",
        "/sections/0/blocks/0/content/runs/0",
        "/sections/0/blocks/0/content/runs/0/format",
    ];
    for pointer in pointers {
        let mut value = simple_json();
        value
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unknown".into(), json!(true));
        assert!(
            matches!(load_json(value), Err(DocumentError::Json(_))),
            "unknown field at {pointer}"
        );
    }
    let mut value = simple_json();
    value["sections"][0]["blocks"][0]["kind"] = json!("table");
    assert!(matches!(load_json(value), Err(DocumentError::Json(_))));
    let mut value = simple_json();
    value["sections"][0]["properties"] = json!({ "page_width": 12240 });
    assert!(matches!(load_json(value), Err(DocumentError::Json(_))));

    for color in [
        json!({ "kind": "auto", "theme": "accent1" }),
        json!({ "kind": "rgb", "red": 0, "green": 0, "blue": 0, "theme": "accent1" }),
    ] {
        let mut value = simple_json();
        value["sections"][0]["blocks"][0]["content"]["runs"][0]["format"]["color"] = color;
        assert!(matches!(load_json(value), Err(DocumentError::Json(_))));
    }
    for line_spacing in [
        json!({ "kind": "multiple", "value": 240, "extra": true }),
        json!({ "kind": "exact", "height": 240, "extra": true }),
        json!({ "kind": "at_least", "height": 240, "extra": true }),
    ] {
        let mut value = simple_json();
        value["sections"][0]["blocks"][0]["content"]["format"]["line_spacing"] = line_spacing;
        assert!(matches!(load_json(value), Err(DocumentError::Json(_))));
    }
    let mut value = simple_json();
    value["sections"][0]["blocks"][0]["content"]["format"]["indentation"] = json!({"unknown": 100});
    assert!(matches!(load_json(value), Err(DocumentError::Json(_))));
    let mut value = simple_json();
    let mut style = serde_json::to_value(NamedStyle::new(id("style"), "Style")).unwrap();
    style["unknown"] = json!(true);
    value["styles"] = json!([style]);
    assert!(matches!(load_json(value), Err(DocumentError::Json(_))));
}

#[test]
fn duplicate_json_fields_are_not_silently_overwritten() {
    let duplicate_schema = "{\"schema_version\":1,\"schema_version\":2}";
    assert!(matches!(
        Document::from_json(duplicate_schema),
        Err(DocumentError::Json(_))
    ));
    let duplicate_title = "{\"schema_version\":1,\"id\":\"doc\",\"metadata\":{\"title\":\"one\",\"title\":\"two\"},\"sections\":[],\"styles\":[]}";
    assert!(matches!(
        Document::from_json(duplicate_title),
        Err(DocumentError::Json(_))
    ));
    let document = paragraph_document(Paragraph::new(
        id("paragraph"),
        vec![TextRun {
            text: "Duplicate property".into(),
            format: CharacterFormat {
                bold: Some(true),
                ..Default::default()
            },
        }],
    ))
    .unwrap();
    let duplicate_run_format = document
        .to_json()
        .unwrap()
        .replace("\"bold\": true", "\"bold\": true, \"bold\": false");
    assert!(matches!(
        Document::from_json(&duplicate_run_format),
        Err(DocumentError::Json(_))
    ));
    let duplicate_paragraph_format = simple_json().to_string().replace(
        "\"alignment\":null",
        "\"alignment\":null,\"alignment\":\"left\"",
    );
    assert!(matches!(
        Document::from_json(&duplicate_paragraph_format),
        Err(DocumentError::Json(_))
    ));
}

#[test]
fn invalid_numeric_ranges_and_typed_enum_values_are_rejected() {
    for (field, invalid) in [
        ("font_size", json!(-1)),
        ("font_size", json!(1.5)),
        ("font_size", json!(u64::MAX)),
        (
            "color",
            json!({ "kind": "rgb", "red": 256, "green": 0, "blue": 0 }),
        ),
        ("underline", json!("wavy")),
    ] {
        let mut value = simple_json();
        value["sections"][0]["blocks"][0]["content"]["runs"][0]["format"][field] = invalid;
        assert!(
            matches!(load_json(value), Err(DocumentError::Json(_))),
            "invalid {field}"
        );
    }
}

#[test]
fn json_loading_returns_structured_validation_errors_and_serde_also_validates() {
    let mut duplicate = simple_json();
    duplicate["sections"][0]["id"] = json!("document");
    assert!(
        matches!(load_json(duplicate.clone()), Err(DocumentError::Validation(ValidationError::DuplicateId { id }))
        if id == "document")
    );
    assert!(serde_json::from_value::<Document>(duplicate).is_err());

    let mut bad_format = simple_json();
    bad_format["sections"][0]["blocks"][0]["content"]["runs"][0]["format"]["font_size"] = json!(0);
    assert!(matches!(
        load_json(bad_format.clone()),
        Err(DocumentError::Validation(
            ValidationError::InvalidFormatting { .. }
        ))
    ));
    assert!(serde_json::from_value::<Document>(bad_format).is_err());

    let valid = simple_json();
    let loaded = serde_json::from_value::<Document>(valid).unwrap();
    assert_eq!(loaded.validate(), Ok(()));
}

#[test]
fn identifiers_are_validated_at_construction_and_document_boundaries() {
    for invalid in ["", " ", " padded", "padded ", "\nline"] {
        assert!(matches!(
            Id::new(invalid),
            Err(ValidationError::InvalidId { .. })
        ));
        let independently_decoded: Id = serde_json::from_value(json!(invalid)).unwrap();
        assert!(matches!(
            Document::new(independently_decoded),
            Err(ValidationError::InvalidId { .. })
        ));
        let mut value = simple_json();
        value["sections"][0]["blocks"][0]["content"]["id"] = json!(invalid);
        assert!(matches!(
            load_json(value),
            Err(DocumentError::Validation(ValidationError::InvalidId { .. }))
        ));
    }
    let preserved = Id::new("报告:α-1").unwrap();
    assert_eq!(preserved.as_str(), "报告:α-1");
}

#[test]
fn realistic_fixture_retains_sections_styles_formatting_and_unicode() {
    let fixture = include_str!("../../../tests/fixtures/writer-sample-v1.json");
    let document = Document::from_json(fixture).unwrap();
    assert_eq!(document.metadata().title, "Quarterly research report");
    assert_eq!(document.sections().len(), 2);
    assert_eq!(document.styles().len(), 2);
    let intro = document.sections()[0]
        .blocks()
        .iter()
        .find_map(|block| match block {
            Block::Paragraph(paragraph) if paragraph.id().as_str() == "intro" => Some(paragraph),
            _ => None,
        })
        .unwrap();
    assert_eq!(intro.style().unwrap().as_str(), "normal");
    assert_eq!(intro.runs()[0].text, "Cafe\u{301} research — ");
    assert_eq!(intro.runs()[0].format.bold, Some(false));
    assert_eq!(intro.runs()[1].text, "👩🏽‍💻 team");
    assert_eq!(intro.runs()[1].format.bold, Some(true));
    assert_eq!(
        document.styles()[1].based_on.as_ref().unwrap().as_str(),
        "normal"
    );
    assert_eq!(
        Document::from_json(&document.to_json().unwrap()).unwrap(),
        document
    );
}
