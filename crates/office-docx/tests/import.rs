//! Semantic import regressions for the declared editable DOCX profile.

use std::{
    collections::BTreeMap,
    io::{Cursor, Read, Write},
};

use office_core::{
    Alignment, Block, Id, LineSpacing, PageOrientation, TextColor, Twips, Underline,
};
use office_docx::{
    CompatibilityIssueKind, ConversionError, DocxImportOutcome, ImportedDocx, import_docx,
};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

const BASIC: &[u8] = include_bytes!("../../../tests/fixtures/docx/supported-basic.docx");
const FORMATTING: &[u8] = include_bytes!("../../../tests/fixtures/docx/supported-formatting.docx");
const STRICT: &[u8] = include_bytes!("../../../tests/fixtures/docx/strict-relocated.docx");
const UNSUPPORTED: &[u8] =
    include_bytes!("../../../tests/fixtures/docx/writer-source-preservation.docx");
const CARRIAGE_RETURN: &[u8] =
    include_bytes!("../../../tests/fixtures/docx/unsupported-carriage-return.docx");

fn editable(bytes: &[u8]) -> ImportedDocx {
    match import_docx(bytes, Id::new("import-test").unwrap()).expect("valid package") {
        DocxImportOutcome::Editable(imported) => imported,
        outcome => panic!("expected complete editable import, got {outcome:?}"),
    }
}

fn modified(bytes: &[u8], part: &str, update: impl FnOnce(String) -> String) -> Vec<u8> {
    let mut input = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut parts = BTreeMap::new();
    for index in 0..input.len() {
        let mut file = input.by_index(index).unwrap();
        let name = file.name().unwrap().to_string();
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        parts.insert(name, bytes);
    }
    let original = String::from_utf8(parts[part].clone()).unwrap();
    parts.insert(part.into(), update(original).into_bytes());
    let mut output = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in parts {
        output
            .start_file(
                name,
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
        output.write_all(&bytes).unwrap();
    }
    output.finish().unwrap().into_inner()
}

#[test]
fn imports_basic_unicode_content_with_stable_native_identifiers_and_exact_original() {
    let imported = editable(BASIC);
    let document = imported.document();
    assert_eq!(document.id().as_str(), "import-test");
    document.validate().unwrap();
    assert_eq!(document.sections().len(), 1);
    let Block::Paragraph(paragraph) = &document.sections()[0].blocks()[0];
    assert_eq!(paragraph.runs()[0].text, "DOCX hello — café 👩🏽‍💻");
    assert_eq!(imported.original().original_bytes(), BASIC);
    assert_eq!(document, editable(BASIC).document());
    let second_namespace = match import_docx(BASIC, Id::new("other-document").unwrap()).unwrap() {
        DocxImportOutcome::Editable(value) => value,
        value => panic!("{value:?}"),
    };
    assert_ne!(
        document.sections()[0].id(),
        second_namespace.document().sections()[0].id()
    );
}

#[test]
fn discovers_relocated_parts_and_preserves_sections_geometry_and_paragraph_properties() {
    let imported = editable(FORMATTING);
    let document = imported.document();
    assert_eq!(document.metadata().title, "Supported Writer DOCX – café");
    assert_eq!(document.sections().len(), 2);
    assert_eq!(document.sections()[0].blocks().len(), 2);
    assert_eq!(document.sections()[1].blocks().len(), 1);
    let first = document.sections()[0].properties().unwrap();
    assert_eq!(first.page_size.as_ref().unwrap().width, Twips(12240));
    assert_eq!(first.page_size.as_ref().unwrap().height, Twips(15840));
    assert_eq!(
        first.page_size.as_ref().unwrap().orientation,
        Some(PageOrientation::Portrait)
    );
    assert_eq!(first.page_margins.as_ref().unwrap().left, Twips(1440));
    assert_eq!(first.page_margins.as_ref().unwrap().header, Twips(720));
    let second = document.sections()[1].properties().unwrap();
    assert_eq!(second.page_size.as_ref().unwrap().width, Twips(11906));
    assert_eq!(second.page_margins.as_ref().unwrap().top, Twips(1134));
    let Block::Paragraph(paragraph) = &document.sections()[0].blocks()[0];
    assert_eq!(paragraph.format().alignment, Some(Alignment::Center));
    assert_eq!(paragraph.format().spacing_before, Some(Twips(240)));
    assert_eq!(paragraph.format().spacing_after, Some(Twips(120)));
    assert_eq!(
        paragraph.format().line_spacing,
        Some(LineSpacing::Exact { height: Twips(360) })
    );
    assert_eq!(
        paragraph.format().indentation.as_ref().unwrap().first_line,
        Some(Twips(360))
    );
    assert_eq!(paragraph.format().page_break_before, Some(false));
    let Block::Paragraph(paragraph) = &document.sections()[1].blocks()[0];
    assert_eq!(
        paragraph.format().line_spacing,
        Some(LineSpacing::Multiple { value: 360 })
    );
    assert_eq!(
        paragraph.format().indentation.as_ref().unwrap().first_line,
        Some(Twips(-360))
    );
    assert_eq!(paragraph.format().page_break_before, Some(true));
}

#[test]
fn maps_style_toggles_separately_from_absolute_direct_formatting_and_defaults() {
    let imported = editable(FORMATTING);
    let document = imported.document();
    assert_eq!(document.styles().len(), 3);
    let defaults = document
        .styles()
        .iter()
        .find(|style| style.name == "DOCX Document Defaults")
        .unwrap();
    let normal = document
        .styles()
        .iter()
        .find(|style| style.name == "Normal")
        .unwrap();
    let title = document
        .styles()
        .iter()
        .find(|style| style.name == "Title")
        .unwrap();
    assert_eq!(
        defaults.character_format.font_family.as_deref(),
        Some("Arimo")
    );
    assert_eq!(normal.based_on.as_ref(), Some(&defaults.id));
    assert_eq!(normal.character_format.bold, Some(true));
    assert_eq!(title.based_on.as_ref(), Some(&normal.id));
    assert_eq!(title.character_format.bold, Some(false));
    assert_eq!(title.character_format.italic, Some(true));
    let Block::Paragraph(first) = &document.sections()[0].blocks()[0];
    assert_eq!(first.style(), Some(&title.id));
    let Block::Paragraph(second) = &document.sections()[0].blocks()[1];
    assert_eq!(second.style(), Some(&normal.id));
    assert_eq!(first.runs()[0].format.bold, Some(true));
    assert_eq!(first.runs()[0].format.underline, Some(Underline::Double));
    assert_eq!(first.runs()[0].format.font_size.unwrap().0, 28);
    assert_eq!(
        first.runs()[0].format.color,
        Some(TextColor::Rgb {
            red: 0x11,
            green: 0x22,
            blue: 0x33
        })
    );
    assert_eq!(first.runs()[1].format.bold, Some(false));
    assert_eq!(first.runs()[1].format.italic, Some(false));
}

#[test]
fn preserves_mixed_runs_combining_marks_emoji_whitespace_tabs_and_soft_breaks() {
    let imported = editable(FORMATTING);
    let Block::Paragraph(paragraph) = &imported.document().sections()[0].blocks()[0];
    assert_eq!(paragraph.runs().len(), 2);
    assert_eq!(paragraph.runs()[0].text, "  Cafe\u{301} 👩🏽‍💻 ");
    assert_eq!(
        paragraph.runs()[1].text,
        "日本語 & <XML> \nSecond line\ttabbed"
    );
}

#[test]
fn accepts_strict_namespaces_with_relocated_main_part() {
    let imported = editable(STRICT);
    let Block::Paragraph(paragraph) = &imported.document().sections()[0].blocks()[0];
    assert_eq!(
        paragraph.runs()[0].text,
        "Strict namespace, relocated main part."
    );
    assert_eq!(imported.original().original_bytes(), STRICT);
}

#[test]
fn unsupported_source_is_preserved_without_returning_a_partial_editable_document() {
    match import_docx(UNSUPPORTED, Id::new("unsupported-source").unwrap()).unwrap() {
        DocxImportOutcome::Preserved { original, report } => {
            assert_eq!(original.original_bytes(), UNSUPPORTED);
            assert!(!report.is_supported());
            assert!(
                report
                    .diagnostics
                    .iter()
                    .any(|issue| issue.kind == CompatibilityIssueKind::UnsupportedElement)
            );
            assert!(
                report
                    .diagnostics
                    .iter()
                    .any(|issue| issue.kind == CompatibilityIssueKind::UnsupportedPart)
            );
        }
        value => panic!("unsupported source must remain read-only: {value:?}"),
    }
}

#[test]
fn unknown_attributes_inline_page_breaks_and_metadata_prevent_editable_import() {
    let unknown_attribute = modified(BASIC, "document.xml", |text| {
        text.replace("<w:p>", "<w:p w:unrecognizedAttribute=\"12345678\">")
    });
    let page_break = modified(BASIC, "document.xml", |text| {
        text.replace("</w:t>", "</w:t><w:br w:type=\"page\"/>")
    });
    let metadata = modified(FORMATTING, "metadata/core.xml", |text| {
        text.replace(
            "</cp:coreProperties>",
            "<dc:unrecognizedMetadata>Alice</dc:unrecognizedMetadata></cp:coreProperties>",
        )
    });
    for input in [unknown_attribute, page_break, metadata] {
        match import_docx(&input, Id::new("read-only").unwrap()).unwrap() {
            DocxImportOutcome::Preserved { original, report } => {
                assert_eq!(original.original_bytes(), input);
                assert!(!report.is_supported());
            }
            value => panic!("must preserve unrepresented semantics: {value:?}"),
        }
    }
}

#[test]
fn missing_style_references_duplicate_styles_and_cycles_are_explicit_errors() {
    let missing = modified(BASIC, "document.xml", |text| {
        text.replace("<w:p>", "<w:p><w:pPr><w:pStyle w:val=\"missing\"/></w:pPr>")
    });
    let duplicate = modified(FORMATTING, "formatting/paragraphs.xml", |text| {
        text.replace("w:styleId=\"Title\"", "w:styleId=\"Normal\"")
    });
    let cycle = modified(FORMATTING, "formatting/paragraphs.xml", |text| {
        text.replace(
            "<w:basedOn w:val=\"Normal\"/>",
            "<w:basedOn w:val=\"Title\"/>",
        )
    });
    for input in [missing, duplicate, cycle] {
        assert!(matches!(
            import_docx(&input, Id::new("invalid-reference").unwrap()),
            Err(ConversionError::InvalidDocument { .. })
        ));
    }
}

#[test]
fn duplicate_or_misplaced_properties_are_not_silently_repaired() {
    let duplicate = modified(BASIC, "document.xml", |text| {
        text.replace("<w:r>", "<w:r><w:rPr/><w:rPr/>")
    });
    let misplaced = modified(BASIC, "document.xml", |text| {
        text.replace("</w:r>", "<w:rPr/></w:r>")
    });
    let final_properties = modified(BASIC, "document.xml", |text| {
        text.replace("<w:body>", "<w:body><w:sectPr/>")
    });
    for input in [duplicate, misplaced, final_properties] {
        assert!(matches!(
            import_docx(&input, Id::new("invalid-structure").unwrap()),
            Err(ConversionError::InvalidDocument { .. })
        ));
    }
}

#[test]
fn reports_are_bounded_without_losing_the_read_only_decision() {
    let repeated = modified(BASIC, "document.xml", |text| {
        text.replace(
            "</w:p>",
            &format!("{}</w:p>", "<w:unimplementedContent/>".repeat(200)),
        )
    });
    match import_docx(&repeated, Id::new("many-issues").unwrap()).unwrap() {
        DocxImportOutcome::Preserved { report, .. } => {
            assert_eq!(report.diagnostics.len(), 128);
            assert_eq!(report.omitted_diagnostics, 72);
        }
        value => panic!("{value:?}"),
    }
}

#[test]
fn invalid_or_unbounded_import_inputs_never_create_a_document() {
    assert!(matches!(
        import_docx(b"corrupt", Id::new("valid-id").unwrap()),
        Err(ConversionError::Package(_))
    ));
    assert!(matches!(
        import_docx(BASIC, Id::new("x".repeat(257)).unwrap()),
        Err(ConversionError::Package(_))
    ));
}

#[test]
fn unresolved_style_references_inside_unsupported_extensions_remain_read_only() {
    let unsupported_styles = modified(FORMATTING, "formatting/paragraphs.xml", |text| {
        text.replace(
            "<w:docDefaults>",
            "<x:styles xmlns:x=\"urn:unsupported-styles\"><w:docDefaults>",
        )
        .replace("</w:styles>", "</x:styles></w:styles>")
    });
    match import_docx(
        &unsupported_styles,
        Id::new("extended-style-source").unwrap(),
    )
    .unwrap()
    {
        DocxImportOutcome::Preserved { original, report } => {
            assert_eq!(original.original_bytes(), unsupported_styles);
            assert!(!report.is_supported());
        }
        value => panic!("unknown style processing must not enable a partial document: {value:?}"),
    }
}

#[test]
fn false_style_toggle_is_a_no_op_while_direct_false_remains_absolute() {
    let source = modified(FORMATTING, "formatting/paragraphs.xml", |text| {
        text.replace(
            "<w:b/><w:bCs/><w:i/><w:iCs/>",
            "<w:b w:val=\"0\"/><w:bCs w:val=\"0\"/><w:i/><w:iCs/>",
        )
    });
    let imported = editable(&source);
    let normal = imported
        .document()
        .styles()
        .iter()
        .find(|style| style.name == "Normal")
        .unwrap();
    let title = imported
        .document()
        .styles()
        .iter()
        .find(|style| style.name == "Title")
        .unwrap();
    assert_eq!(normal.character_format.bold, Some(true));
    assert_eq!(title.character_format.bold, None);
    assert_eq!(title.based_on.as_ref(), Some(&normal.id));
    let Block::Paragraph(paragraph) = &imported.document().sections()[0].blocks()[0];
    assert_eq!(paragraph.runs()[1].format.bold, Some(false));
}

#[test]
fn document_default_bold_is_absolute_before_paragraph_style_toggles() {
    let source = modified(FORMATTING, "formatting/paragraphs.xml", |text| {
        text.replace(
            "<w:docDefaults><w:rPrDefault><w:rPr>",
            "<w:docDefaults><w:rPrDefault><w:rPr><w:b/><w:bCs/>",
        )
    });
    let imported = editable(&source);
    let defaults = imported
        .document()
        .styles()
        .iter()
        .find(|style| style.name == "DOCX Document Defaults")
        .unwrap();
    let normal = imported
        .document()
        .styles()
        .iter()
        .find(|style| style.name == "Normal")
        .unwrap();
    let title = imported
        .document()
        .styles()
        .iter()
        .find(|style| style.name == "Title")
        .unwrap();
    assert_eq!(defaults.character_format.bold, Some(true));
    assert_eq!(normal.character_format.bold, Some(false));
    assert_eq!(title.character_format.bold, Some(true));
}

#[test]
fn literal_carriage_returns_remain_byte_exact_read_only_sources() {
    match import_docx(CARRIAGE_RETURN, Id::new("carriage-return-source").unwrap()).unwrap() {
        DocxImportOutcome::Preserved { original, report } => {
            assert_eq!(original.original_bytes(), CARRIAGE_RETURN);
            assert!(report.diagnostics.iter().any(|issue| {
                issue.kind == CompatibilityIssueKind::UnsupportedValue && issue.path == "r/t"
            }));
        }
        value => panic!("Office-ambiguous carriage return must not become editable: {value:?}"),
    }
}

#[test]
fn boundary_whitespace_requires_effective_preserve_without_trimming() {
    for setting in ["", " xml:space=\"default\"", " xml:space=\"preserve\""] {
        let input = modified(BASIC, "document.xml", |text| {
            text.replace("<w:t>", &format!("<w:t{setting}>  "))
                .replace("</w:t>", "  </w:t>")
        });
        let result = import_docx(&input, Id::new("whitespace-source").unwrap()).unwrap();
        if setting.contains("preserve") {
            let DocxImportOutcome::Editable(imported) = result else {
                panic!("{result:?}");
            };
            let Block::Paragraph(paragraph) = &imported.document().sections()[0].blocks()[0];
            assert_eq!(paragraph.runs()[0].text, "  DOCX hello — café 👩🏽‍💻  ");
        } else {
            let DocxImportOutcome::Preserved { original, report } = result else {
                panic!("{result:?}");
            };
            assert_eq!(original.original_bytes(), input);
            assert!(
                report
                    .diagnostics
                    .iter()
                    .any(|issue| issue.kind == CompatibilityIssueKind::UnsupportedValue)
            );
        }
    }
}

#[test]
fn xml_space_inheritance_and_local_default_override_are_deliberate() {
    let inherited = modified(BASIC, "document.xml", |text| {
        text.replace("<w:document ", "<w:document xml:space=\"preserve\" ")
            .replace("<w:t>", "<w:t>  ")
            .replace("</w:t>", "  </w:t>")
    });
    let imported = editable(&inherited);
    let Block::Paragraph(paragraph) = &imported.document().sections()[0].blocks()[0];
    assert_eq!(paragraph.runs()[0].text, "  DOCX hello — café 👩🏽‍💻  ");
    let overridden = modified(&inherited, "document.xml", |text| {
        text.replace("<w:t>", "<w:t xml:space=\"default\">")
    });
    assert!(matches!(
        import_docx(&overridden, Id::new("overridden-space").unwrap()).unwrap(),
        DocxImportOutcome::Preserved { .. }
    ));
}

#[test]
fn literal_line_feeds_tabs_and_carriage_returns_are_not_approximated_as_elements() {
    for controls in ["a\nb", "a\tb", "a&#13;b", "a\rb"] {
        for space in ["", " xml:space=\"preserve\""] {
            let input = modified(BASIC, "document.xml", |text| {
                text.replace(
                    "<w:t>DOCX hello — café 👩🏽‍💻</w:t>",
                    &format!("<w:t{space}>{controls}</w:t>"),
                )
            });
            match import_docx(&input, Id::new("literal-control-source").unwrap()).unwrap() {
                DocxImportOutcome::Preserved { original, report } => {
                    assert_eq!(original.original_bytes(), input);
                    assert!(
                        report
                            .diagnostics
                            .iter()
                            .any(|issue| issue.kind == CompatibilityIssueKind::UnsupportedValue)
                    );
                }
                value => {
                    panic!("literal controls require a separate compatibility policy: {value:?}")
                }
            }
        }
    }
}

#[test]
fn invalid_whitespace_declarations_are_errors_even_when_inherited() {
    for input in [
        modified(BASIC, "document.xml", |text| {
            text.replace("<w:t>", "<w:t xml:space=\"invalid\">")
        }),
        modified(BASIC, "document.xml", |text| {
            text.replace("<w:document ", "<w:document xml:space=\"invalid\" ")
        }),
    ] {
        assert!(matches!(
            import_docx(&input, Id::new("invalid-whitespace").unwrap()),
            Err(ConversionError::InvalidDocument { .. })
        ));
    }
}

#[test]
fn empty_body_defaults_are_disclosed_as_future_typing_normalization_and_retained_in_original() {
    let replace_body = |text: String| {
        let start = text.find("<w:body>").unwrap();
        let end = text.find("</w:body>").unwrap() + "</w:body>".len();
        format!("{}<w:body/>{}", &text[..start], &text[end..])
    };
    let with_defaults = modified(FORMATTING, "documents/main.xml", replace_body);
    let normalized = editable(&with_defaults);
    assert!(normalized.document().sections().is_empty());
    assert_eq!(normalized.original().original_bytes(), with_defaults);
    assert!(normalized.report().is_supported());
    assert!(normalized.report().normalizations.iter().any(|notice| {
        notice.kind == CompatibilityIssueKind::NormalizedFormatting && notice.path == "body"
    }));
    let without_defaults = modified(BASIC, "document.xml", replace_body);
    let imported = editable(&without_defaults);
    assert!(imported.document().sections().is_empty());
}

#[test]
fn explicit_carriage_return_and_hyphen_elements_have_defined_text_mappings() {
    let source = modified(BASIC, "document.xml", |text| {
        text.replace(
            "</w:t>",
            "</w:t><w:cr/><w:t>after</w:t><w:noBreakHyphen/><w:softHyphen/>",
        )
    });
    let imported = editable(&source);
    let Block::Paragraph(paragraph) = &imported.document().sections()[0].blocks()[0];
    assert_eq!(
        paragraph.runs()[0].text,
        "DOCX hello — café 👩🏽‍💻\nafter\u{2011}\u{00ad}"
    );
}
