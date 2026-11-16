//! RTF adapter profile, preservation and real engine editing regressions.

use office_core::{
    Alignment, Block, CharacterFormat, Document, DocumentMetadata, DocumentSession, GraphemeOffset,
    HalfPoints, Id, Paragraph, ParagraphFormat, Section, TextColor, TextCoordinates, TextRun,
    Underline, WriterFile,
};
use office_formats::{
    DocumentFormat, ImportOutcome, assess_export, export_document, import_document,
    import_document_with_warnings,
};

fn id(value: &str) -> Id {
    Id::new(value).unwrap()
}
fn open(bytes: &[u8]) -> WriterFile {
    match import_document(
        DocumentFormat::Rtf,
        bytes,
        id("rtf-reference"),
        "reference.rtf",
    )
    .unwrap()
    {
        ImportOutcome::Editable(file) => file,
        ImportOutcome::Preserved { report, .. } => panic!("Expected editable profile: {report:?}"),
    }
}
fn paragraphs(document: &Document) -> Vec<&Paragraph> {
    document
        .sections()
        .iter()
        .flat_map(|section| section.blocks())
        .map(|block| {
            let Block::Paragraph(paragraph) = block;
            paragraph
        })
        .collect()
}
fn text(document: &Document) -> Vec<String> {
    paragraphs(document)
        .iter()
        .map(|paragraph| {
            paragraph
                .runs()
                .iter()
                .map(|run| run.text.as_str())
                .collect()
        })
        .collect()
}
fn native(content: &str) -> Document {
    Document::from_parts(
        id("native"),
        DocumentMetadata::default(),
        vec![Section::new(
            id("section"),
            vec![Block::Paragraph(Paragraph::new(
                id("paragraph"),
                vec![TextRun::new(content)],
            ))],
        )],
        vec![],
    )
    .unwrap()
}

#[test]
fn empty_rtf_is_editable_and_retains_exact_original() {
    let bytes = br"{\rtf1\ansi}";
    let file = open(bytes);
    assert_eq!(text(file.document()), vec![""]);
    assert_eq!(file.original_source().unwrap().bytes(), bytes);
    assert_eq!(file.original_source().unwrap().format(), "rtf");
    assert_eq!(
        open(&export_document(DocumentFormat::Rtf, file.document()).unwrap())
            .document()
            .sections()
            .len(),
        1
    );
}

#[test]
fn mixed_character_properties_and_paragraph_formatting_survive_conversion() {
    let bytes=br"{\rtf1\ansi\deff0{\fonttbl{\f0\fnil Arimo;}{\f1\froman Tinos;}}{\colortbl;\red34\green68\blue102;}\pard\qr\sb120\sa240\li360\ri180\fi-120\sl360\slmult1 plain {\b\i\uldb\f1\fs28\cf1 mixed} tail\par}";
    let file = open(bytes);
    let paragraph = paragraphs(file.document())[0];
    assert_eq!(text(file.document()), vec!["plain mixed tail"]);
    let run = paragraph
        .runs()
        .iter()
        .find(|run| run.text == "mixed")
        .unwrap();
    assert_eq!(run.format.bold, Some(true));
    assert_eq!(run.format.italic, Some(true));
    assert_eq!(run.format.underline, Some(Underline::Double));
    assert_eq!(run.format.font_family.as_deref(), Some("Tinos"));
    assert_eq!(run.format.font_size, Some(HalfPoints(28)));
    assert_eq!(
        run.format.color,
        Some(TextColor::Rgb {
            red: 34,
            green: 68,
            blue: 102
        })
    );
    assert_eq!(paragraph.format().alignment, Some(Alignment::Right));
    let encoded = export_document(DocumentFormat::Rtf, file.document()).unwrap();
    let reopened = open(&encoded);
    assert_eq!(text(reopened.document()), text(file.document()));
    assert_eq!(
        paragraphs(reopened.document())[0].format(),
        paragraph.format()
    );
    let again = paragraphs(reopened.document())[0]
        .runs()
        .iter()
        .find(|run| run.text == "mixed")
        .unwrap();
    assert_eq!(again.format, run.format);
    assert_eq!(
        encoded,
        export_document(DocumentFormat::Rtf, file.document()).unwrap()
    );
}

#[test]
fn all_supported_unicode_scripts_and_emoji_are_preserved_exactly() {
    let original = "Cafe\u{301} 👩🏽‍💻 العربية नमस्ते שלום { braces } \\ slash\nsoft\tbreak";
    let native = native(original);
    let bytes = export_document(DocumentFormat::Rtf, &native).unwrap();
    assert!(bytes.is_ascii());
    assert_eq!(text(open(&bytes).document()), vec![original]);
}

#[test]
fn fallback_count_is_scoped_and_escaped_fallback_tokens_are_counted_once() {
    let bytes = br"{\rtf1\ansi\uc1 {\uc0\u65}\u66?C\uc2\u233\'3f\~X}";
    assert_eq!(text(open(bytes).document()), vec!["ABCéX"]);
    assert_eq!(
        text(open(br"{\rtf1\ansi\uc2\u233?{X}Y}").document()),
        vec!["éXY"]
    );
    assert_eq!(
        text(open(b"{\\rtf1\\ansi\\u233\r\n?X}").document()),
        vec!["éX"]
    );
}

#[test]
fn fallback_controls_do_not_change_formatting_and_binary_counts_as_one() {
    let file = open(br"{\rtf1\ansi\b\u233\b0X}");
    assert_eq!(text(file.document()), vec!["éX"]);
    assert!(
        paragraphs(file.document())[0]
            .runs()
            .iter()
            .all(|run| run.format.bold == Some(true))
    );
    let bytes = b"{\\rtf1\\ansi\\u233\\bin3 {}\0X}";
    assert_eq!(text(open(bytes).document()), vec!["éX"]);
}

#[test]
fn surrogate_pairs_must_be_complete_and_cannot_split_formatting() {
    assert_eq!(
        text(open(br"{\rtf1\ansi\u-10179?\u-8704?}").document()),
        vec!["😀"]
    );
    for bytes in [
        br"{\rtf1\u-10179?}".as_slice(),
        br"{\rtf1\u-8704?}",
        br"{\rtf1\u-10179?X}",
        br"{\rtf1\u-10179?\b\u-8704?}",
        br"{\rtf1\u65535?}",
    ] {
        assert!(
            import_document(DocumentFormat::Rtf, bytes, id("bad"), "bad.rtf").is_err(),
            "{bytes:?}"
        );
    }
}

#[test]
fn raw_and_hex_codepage_bytes_use_the_declared_encoding_without_replacement() {
    let file = open(b"{\\rtf1\\ansi\\ansicpg1252 caf\xe9 \\'97}");
    assert_eq!(text(file.document()), vec!["café —"]);
    for bytes in [
        br"{\rtf1\ansi\ansicpg932 text}".as_slice(),
        br"{\rtf1\ansi\'81}",
    ] {
        assert!(matches!(
            import_document(DocumentFormat::Rtf, bytes, id("unsupported"), "source.rtf").unwrap(),
            ImportOutcome::Preserved { .. }
        ));
    }
}

#[test]
fn named_styles_inherit_and_remain_available_after_save_reopen() {
    let bytes=br"{\rtf1\ansi{\fonttbl{\f0\fnil Arimo;}}{\stylesheet{\s0\fs24 Normal;}{\s7\sbasedon0\b\fs32\qc Heading;}}\pard\s7 Title\par\pard\s0 Body\par}";
    let file = open(bytes);
    assert_eq!(file.document().styles().len(), 2);
    assert_eq!(file.document().styles()[1].name, "Heading");
    assert_eq!(
        file.document().styles()[1].based_on.as_ref(),
        Some(&file.document().styles()[0].id)
    );
    assert_eq!(
        paragraphs(file.document())[0].style(),
        Some(&file.document().styles()[1].id)
    );
    let reopened = open(&export_document(DocumentFormat::Rtf, file.document()).unwrap());
    assert_eq!(reopened.document().styles()[1].name, "Heading");
    assert_eq!(text(reopened.document()), vec!["Title", "Body"]);
    assert_eq!(
        paragraphs(reopened.document())[0].runs()[0].format.bold,
        Some(true)
    );
    assert_eq!(
        paragraphs(reopened.document())[0].format().alignment,
        Some(Alignment::Center)
    );
}

#[test]
fn realistic_fixture_supports_edit_export_reopen_and_native_original_retention() {
    let bytes = include_bytes!("../../../tests/fixtures/formats/project-notes.rtf");
    let file = open(bytes);
    assert_eq!(file.document().sections().len(), 2);
    assert_eq!(file.document().metadata().title, "Project notes; café 😀");
    assert_eq!(paragraphs(file.document()).len(), 3);
    let last = paragraphs(file.document())[2];
    let paragraph_id = last.id().clone();
    let end = TextCoordinates::new(last).grapheme_len();
    let (document, original) = file.into_parts();
    let mut session = DocumentSession::new(document);
    let mut transaction = session.begin_transaction();
    let at = transaction.position(&paragraph_id, end).unwrap();
    transaction
        .insert_text(
            &at,
            " — edited e\u{301} 👩🏽‍💻",
            CharacterFormat {
                italic: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
    session.commit(transaction).unwrap();
    let expected = text(session.document());
    let encoded = export_document(DocumentFormat::Rtf, session.document()).unwrap();
    let reopened = open(&encoded);
    assert_eq!(text(reopened.document()), expected);
    assert_eq!(reopened.document().sections().len(), 2);
    assert_eq!(
        reopened.document().metadata().title,
        "Project notes; café 😀"
    );
    let native =
        WriterFile::with_original_source(session.document().clone(), original.unwrap()).unwrap();
    let loaded =
        office_core::deserialize_writer_file(&office_core::serialize_writer_file(&native).unwrap())
            .unwrap();
    assert_eq!(loaded.document(), session.document());
    assert_eq!(loaded.original_source().unwrap().bytes(), bytes);
    session.undo().unwrap();
    assert_ne!(text(session.document()), expected);
    session.redo().unwrap();
    assert_eq!(text(session.document()), expected);
}

#[test]
fn metadata_title_and_unicode_table_names_do_not_treat_escaped_semicolons_as_delimiters() {
    let bytes =
        br"{\rtf1\ansi{\fonttbl{\f0\fnil Family\u59?Name;}}{\info{\title Notes; review}}\f0 Text}";
    let file = open(bytes);
    assert_eq!(file.document().metadata().title, "Notes; review");
    assert_eq!(
        paragraphs(file.document())[0].runs()[0]
            .format
            .font_family
            .as_deref(),
        Some("Family;Name")
    );
    let reopened = open(&export_document(DocumentFormat::Rtf, file.document()).unwrap());
    assert_eq!(reopened.document().metadata().title, "Notes; review");
    assert_eq!(
        paragraphs(reopened.document())[0].runs()[0]
            .format
            .font_family
            .as_deref(),
        Some("Family;Name")
    );
}

#[test]
fn known_formatting_fixture_reports_normalizations_and_survives_edit_export_reopen() {
    let bytes = include_bytes!("../../../tests/fixtures/formats/rtf-known-formatting.rtf");
    let report = import_document_with_warnings(
        DocumentFormat::Rtf,
        bytes,
        id("normalized"),
        "compatibility.rtf",
    )
    .unwrap();
    assert!(report.has_formatting_normalizations);
    assert!(report.warnings.len() >= 10);
    assert_eq!(report.omitted_warnings, 0);
    assert!(report.warnings.iter().all(|warning| {
        warning.code == "normalized_formatting"
            && warning.part == "rtf"
            && warning.path.starts_with("byte:")
    }));
    let ImportOutcome::Editable(file) = report.outcome else {
        panic!("Known formatting must not block the complete supported content")
    };
    assert_eq!(file.original_source().unwrap().bytes(), bytes);
    assert_eq!(file.document().metadata().title, "Compatibility notes");
    assert_eq!(file.document().styles().len(), 2);
    assert_eq!(
        text(file.document()),
        vec![
            "Compatibility notes",
            "Writer keeps logical Unicode e\u{301} and 😀 with bold, italic and underlined text.",
        ]
    );
    let runs = paragraphs(file.document())[1].runs();
    for (word, expected) in [("bold", 0), ("italic", 1), ("underlined", 2)] {
        let run = runs.iter().find(|run| run.text == word).unwrap();
        match expected {
            0 => assert_eq!(run.format.bold, Some(true)),
            1 => assert_eq!(run.format.italic, Some(true)),
            _ => assert_eq!(run.format.underline, Some(Underline::Single)),
        }
    }
    let last = paragraphs(file.document())[1];
    let paragraph_id = last.id().clone();
    let end = TextCoordinates::new(last).grapheme_len();
    let (document, original) = file.into_parts();
    let mut session = DocumentSession::new(document);
    let mut transaction = session.begin_transaction();
    let position = transaction.position(&paragraph_id, end).unwrap();
    transaction
        .insert_text(&position, " Edited 👩🏽‍💻.", Default::default())
        .unwrap();
    session.commit(transaction).unwrap();
    let saved =
        WriterFile::with_original_source(session.document().clone(), original.unwrap()).unwrap();
    let reopened_native =
        office_core::deserialize_writer_file(&office_core::serialize_writer_file(&saved).unwrap())
            .unwrap();
    assert_eq!(reopened_native.document(), session.document());
    assert_eq!(reopened_native.original_source().unwrap().bytes(), bytes);
    let encoded = export_document(DocumentFormat::Rtf, reopened_native.document()).unwrap();
    let reopened = open(&encoded);
    assert_eq!(text(reopened.document()), text(session.document()));
    assert_eq!(
        reopened
            .document()
            .styles()
            .iter()
            .map(|style| style.name.as_str())
            .collect::<Vec<_>>(),
        ["Heading", "Body"]
    );
    assert_eq!(saved.original_source().unwrap().bytes(), bytes);
}

#[test]
fn validated_language_and_layout_controls_disclose_their_approximation() {
    for property in [
        "adeflang0",
        "adeflang65535",
        "deflang1033",
        "deflangfe2052",
        "lang1033",
        "langfe2052",
        "alang1025",
        "nowidctlpar",
        "widctlpar",
        "noline",
        "hyphpar",
        "hyphpar0",
        "hyphauto1",
        "kerning0",
        "kerning1",
        "kerning32767",
    ] {
        let bytes = format!("{{\\rtf1 \\{property} complete text}}");
        let report = import_document_with_warnings(
            DocumentFormat::Rtf,
            bytes.as_bytes(),
            id("formatting"),
            "formatting.rtf",
        )
        .unwrap();
        assert!(report.has_formatting_normalizations, "{property}");
        assert_eq!(report.warnings.len(), 1, "{property}");
        let ImportOutcome::Editable(file) = report.outcome else {
            panic!("Known formatting control was blocked: {property}")
        };
        assert_eq!(text(file.document()), ["complete text"], "{property}");
        assert_eq!(file.original_source().unwrap().bytes(), bytes.as_bytes());
    }
}

#[test]
fn invalid_known_formatting_values_and_next_style_references_are_rejected() {
    for property in [
        "adeflang",
        "adeflang-1",
        "deflang65536",
        "deflangfe-1",
        "lang-1",
        "langfe65536",
        "alang65536",
        "nowidctlpar2",
        "widctlpar-1",
        "noline2",
        "hyphpar2",
        "hyphauto-1",
        "kerning",
        "kerning-1",
        "kerning32768",
    ] {
        let bytes = format!("{{\\rtf1 \\{property} text}}");
        let error = import_document(DocumentFormat::Rtf, bytes.as_bytes(), id("bad"), "bad.rtf")
            .unwrap_err();
        assert_eq!(error.code, "invalid_rtf", "{property}");
    }
    for bytes in [
        br"{\rtf1{\stylesheet{\s0\snext1 Normal;}}text}".as_slice(),
        br"{\rtf1{\stylesheet{\s0\snext-1 Normal;}}text}",
    ] {
        assert!(import_document(DocumentFormat::Rtf, bytes, id("bad"), "bad.rtf").is_err());
    }
}

#[test]
fn automatic_next_style_is_disclosed_without_changing_existing_paragraph_styles() {
    let bytes = br"{\rtf1{\stylesheet{\s0 Normal;}{\s1\snext0 Heading;}}\s1 text}";
    let report =
        import_document_with_warnings(DocumentFormat::Rtf, bytes, id("next"), "next.rtf").unwrap();
    assert!(report.has_formatting_normalizations);
    assert_eq!(report.warnings.len(), 1);
    assert!(report.warnings[0].message.contains("next-paragraph"));
    let ImportOutcome::Editable(file) = report.outcome else {
        panic!("Known automatic next-style editing preference must be disclosed")
    };
    assert_eq!(text(file.document()), ["text"]);
    assert_eq!(
        paragraphs(file.document())[0].style(),
        Some(&file.document().styles()[1].id)
    );
    assert_eq!(file.original_source().unwrap().bytes(), bytes);
}

#[test]
fn exact_alternate_font_labels_keep_primary_font_and_scoped_unicode_fallback() {
    let bytes = br"{\rtf1\ansi\deff0{\fonttbl{\f0\fnil Primary{\*\falt Fallback\u-10179?\u-8704?};}}\f0\uc2\u233??\u-10179??\u-8704??}";
    let report =
        import_document_with_warnings(DocumentFormat::Rtf, bytes, id("font"), "font.rtf").unwrap();
    assert!(report.has_formatting_normalizations);
    let ImportOutcome::Editable(file) = report.outcome else {
        panic!("An exact alternate font label should be normalizable")
    };
    assert_eq!(text(file.document()), ["é😀"]);
    assert_eq!(
        paragraphs(file.document())[0].runs()[0]
            .format
            .font_family
            .as_deref(),
        Some("Primary")
    );
    assert_eq!(file.original_source().unwrap().bytes(), bytes);
    for bytes in [
        br"{\rtf1{\fonttbl{\f0 Primary{\*\falt };}}text}".as_slice(),
        br"{\rtf1{\fonttbl{\f0 Primary{\*\falt1 Alternate};}}text}",
        br"{\rtf1{\fonttbl{\f0 Primary{\*\falt \u-10179?};}}text}",
        br"{\rtf1{\fonttbl{\f0 Primary;{\*\falt Alternate}}}text}",
        br"{\rtf1{\fonttbl{\f0 Primary{\*\falt Alternate}{\*\falt Other};}}text}",
        br"{\rtf1{\fonttbl{\f0 Primary{\*\falt Alternate} trailing;}}text}",
    ] {
        assert!(import_document(DocumentFormat::Rtf, bytes, id("bad"), "bad.rtf").is_err());
    }
}

#[test]
fn late_unknown_content_remains_read_only_after_normalization_notice_saturation() {
    for suffix in [
        r"\unknown42 text",
        r"{\*\future retained}",
        r"{\field{\*\fldinst PAGE}{\fldrslt 1}}",
        r"\rtlch directional text",
        r"{\footnote note}",
        r"{\*\listtable{\list\listid1}}",
    ] {
        let bytes = format!(
            "{{\\rtf1 {}complete text {suffix}}}",
            "\\lang1033 ".repeat(140)
        );
        let report = import_document_with_warnings(
            DocumentFormat::Rtf,
            bytes.as_bytes(),
            id("blocked"),
            "blocked.rtf",
        )
        .unwrap();
        assert!(report.has_formatting_normalizations);
        assert_eq!(report.warnings.len(), 128);
        assert_eq!(report.omitted_warnings, 12);
        let ImportOutcome::Preserved { original, report } = report.outcome else {
            panic!("Late unsupported source became an editable partial document: {suffix}")
        };
        assert_eq!(original.bytes(), bytes.as_bytes());
        assert!(!report.is_empty());
    }
}

#[test]
fn alternate_font_support_does_not_allow_embedded_or_unknown_font_content() {
    for metadata in [
        r"{\*\fontemb\fttruetype\bin3 abc}",
        r"{\*\fontfile external.ttf}",
        r"{\*\future font data}",
        r"{\*\falt Alternate{\*\future hidden}}",
        r"{\*\falt Alternate\unknown42}",
    ] {
        let bytes = format!("{{\\rtf1{{\\fonttbl{{\\f0 Primary{metadata};}}}}\\f0 text}}");
        let ImportOutcome::Preserved { original, report } = import_document(
            DocumentFormat::Rtf,
            bytes.as_bytes(),
            id("font"),
            "font.rtf",
        )
        .unwrap() else {
            panic!("Unsupported font payload became editable: {metadata}")
        };
        assert_eq!(original.bytes(), bytes.as_bytes());
        assert!(!report.is_empty());
    }
}

#[test]
fn unsupported_destinations_properties_and_objects_remain_exact_read_only_originals() {
    for bytes in [
        br"{\rtf1 text{\pict\pngblip abc}}".as_slice(),
        br"{\rtf1 text{\*\future data}}",
        br"{\rtf1\unknown42 text}",
        br"{\rtf1\trowd cell\cell\row}",
        br"{\rtf1 text{\object\bin3 {x}}}",
    ] {
        let outcome =
            import_document(DocumentFormat::Rtf, bytes, id("source"), "source.rtf").unwrap();
        let ImportOutcome::Preserved { original, report } = outcome else {
            panic!("unsupported source became editable")
        };
        assert_eq!(original.bytes(), bytes);
        assert!(!report.is_empty());
    }
}

#[test]
fn genuine_libreoffice_rtf_with_unsupported_script_and_list_properties_is_preserved() {
    let bytes = include_bytes!("../../../tests/fixtures/formats/rtf-libreoffice.rtf");
    let ImportOutcome::Preserved { original, report } = import_document(
        DocumentFormat::Rtf,
        bytes,
        id("external"),
        "libreoffice.rtf",
    )
    .unwrap() else {
        panic!("The strict profile must not drop rich script/list properties")
    };
    assert_eq!(original.bytes(), bytes);
    assert!(report.iter().any(|item| item.message.contains("listtable")));
}

#[test]
fn truncated_unbalanced_trailing_and_invalid_property_values_fail_explicitly() {
    for bytes in [
        b"".as_slice(),
        br"not RTF",
        br"{\rtf2 text}",
        br"{\rtf10 text}",
        br"{\rtf1 text",
        br"{\rtf1}junk",
        br"{\rtf1}{x}",
        br"{\rtf1\'z0}",
        br"{\rtf1\fs0 text}",
        br"{\rtf1\fs2147483648 text}",
        br"{\rtf1\fs999999999999999999999 text}",
        br"{\rtf1\uc-1 text}",
        br"{\rtf1\bin-1 x}",
        br"{\rtf1\bin5 x}",
        br"{\rtf1\sl-2147483648 text}",
        b"{\\rtf1 A\0B}",
        br"{\rtf1{\fonttbl{\f0 Arimo;}{\f0 Tinos;}}}",
        br"{\rtf1{\colortbl;\red256;} text}",
    ] {
        assert!(
            import_document(DocumentFormat::Rtf, bytes, id("bad"), "bad.rtf").is_err(),
            "accepted invalid source {bytes:?}"
        );
    }
}

#[test]
fn nested_groups_and_style_chains_are_bounded_without_recursion_overflow() {
    let nested = format!("{{\\rtf1 {}x{}}}", "{".repeat(64), "}".repeat(64));
    assert!(
        import_document(
            DocumentFormat::Rtf,
            nested.as_bytes(),
            id("depth"),
            "depth.rtf"
        )
        .is_err()
    );
    let mut style = String::from("{\\rtf1{\\stylesheet{\\s0 Normal;}");
    for number in 1..65 {
        style.push_str(&format!(
            "{{\\s{number}\\sbasedon{} Style {number};}}",
            number - 1
        ));
    }
    style.push_str("}\\s64 Text}");
    assert!(
        import_document(
            DocumentFormat::Rtf,
            style.as_bytes(),
            id("depth"),
            "depth.rtf"
        )
        .is_err()
    );
}

#[test]
fn export_rejects_unrepresentable_controls_but_plain_rich_output_needs_no_warning() {
    let document = native("safe\0unsafe");
    let before = document.clone();
    let error = export_document(DocumentFormat::Rtf, &document).unwrap_err();
    assert_eq!(error.code, "unsupported_rtf_export");
    assert_eq!(document, before);
    assert!(
        assess_export(DocumentFormat::Rtf, &native("plain"))
            .unwrap()
            .warnings
            .is_empty()
    );
}

#[test]
fn native_rich_formatting_and_page_break_survive_rtf_edit_roundtrip() {
    let document = Document::from_parts(
        id("rich"),
        Default::default(),
        vec![Section::new(
            id("s"),
            vec![Block::Paragraph(
                Paragraph::new(
                    id("p"),
                    vec![TextRun {
                        text: "bold Arabic العربية नमस्ते שלום".into(),
                        format: CharacterFormat {
                            bold: Some(true),
                            italic: Some(true),
                            underline: Some(Underline::Single),
                            font_family: Some("Arimo".into()),
                            font_size: Some(HalfPoints(28)),
                            color: Some(TextColor::Rgb {
                                red: 34,
                                green: 68,
                                blue: 102,
                            }),
                            hyperlink: None,
                        },
                    }],
                )
                .with_format(ParagraphFormat {
                    alignment: Some(Alignment::Right),
                    page_break_before: Some(true),
                    ..Default::default()
                }),
            )],
        )],
        vec![],
    )
    .unwrap();
    let file = open(&export_document(DocumentFormat::Rtf, &document).unwrap());
    let paragraph = paragraphs(file.document())[0];
    assert_eq!(paragraph.format().page_break_before, Some(true));
    assert_eq!(paragraph.format().alignment, Some(Alignment::Right));
    assert_eq!(
        paragraph.runs()[0].format,
        paragraphs(&document)[0].runs()[0].format
    );
    let mut session = DocumentSession::new(file.document().clone());
    let mut transaction = session.begin_transaction();
    let at = transaction
        .position(paragraph.id(), GraphemeOffset::new(0))
        .unwrap();
    transaction
        .insert_text(&at, "Edited ", CharacterFormat::default())
        .unwrap();
    session.commit(transaction).unwrap();
    assert!(
        text(open(&export_document(DocumentFormat::Rtf, session.document()).unwrap()).document())
            [0]
        .starts_with("Edited ")
    );
}

#[test]
fn metadata_inherits_unicode_fallback_and_document_codepage_without_reordering_surrogates() {
    let file = open(br"{\rtf1\ansi\uc0{\fonttbl{\f0 Family \u233;}}{\info{\title \u233X}}Text}");
    assert_eq!(file.document().metadata().title, "éX");
    assert_eq!(
        paragraphs(file.document())[0].runs()[0]
            .format
            .font_family
            .as_deref(),
        Some("Family é")
    );
    let latin = open(b"{\\rtf1\\ansi\\ansicpg28591{\\info{\\title \\'80}}Text}");
    assert_eq!(latin.document().metadata().title, "\u{80}");
    for bytes in [
        br"{\rtf1{\info{\title \u-10179?X\u-8704?}}Text}".as_slice(),
        br"{\rtf1{\info{\title A}{\title B}}Text}",
        br"{\rtf1\deff4 Text}",
    ] {
        assert!(import_document(DocumentFormat::Rtf, bytes, id("bad"), "bad.rtf").is_err());
    }
}

#[test]
fn unsupported_default_font_and_nested_metadata_are_not_silently_dropped() {
    for bytes in [
        br"{\rtf1\deff0{\fonttbl{\f0\fcharset2 Symbol;}}text}".as_slice(),
        br"{\rtf1{\info{\creatim{\pict data}}}text}",
        br"{\rtf1{\*\generator{\object text}}text}",
    ] {
        let ImportOutcome::Preserved { original, report } =
            import_document(DocumentFormat::Rtf, bytes, id("source"), "source.rtf").unwrap()
        else {
            panic!("unsupported metadata/font became editable")
        };
        assert_eq!(original.bytes(), bytes);
        assert!(!report.is_empty());
    }
}

#[test]
fn legacy_flat_font_tables_and_automatic_single_line_spacing_are_supported() {
    let file = open(br"{\rtf1\deff0{\fonttbl\f0 Arimo;\f1 Tinos;}\sl0\slmult0\f1 Text}");
    let paragraph = paragraphs(file.document())[0];
    assert_eq!(
        paragraph.runs()[0].format.font_family.as_deref(),
        Some("Tinos")
    );
    assert_eq!(
        paragraph.format().line_spacing,
        Some(office_core::LineSpacing::Multiple { value: 240 })
    );
}
