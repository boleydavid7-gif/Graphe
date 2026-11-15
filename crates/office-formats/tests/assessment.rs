//! Conversion warnings are bounded and report meaningful visible losses.

use office_core::{
    Block, Document, Id, OriginalSource, Paragraph, Section, SectionProperties, TextRun,
    WriterFile, deserialize_writer_file, serialize_writer_file,
};
use office_formats::{
    DocumentFormat, FormatDiagnostic, ImportOutcome, assess_export, assess_file, export_document,
    export_file, import_document, import_document_with_warnings,
};

#[test]
fn diagnostic_limits_preserve_unicode_boundaries() {
    let diagnostic = FormatDiagnostic::new(
        "é".repeat(100),
        "👩🏽‍💻".repeat(100),
        "界".repeat(400),
        "e\u{301}👩🏽‍💻".repeat(500),
    );
    for (text, limit) in [
        (&diagnostic.code, 64),
        (&diagnostic.part, 512),
        (&diagnostic.path, 512),
        (&diagnostic.message, 2048),
    ] {
        assert!(text.len() <= limit);
        assert!(text.ends_with('…'));
        assert!(!text.contains('\u{fffd}'));
    }
    let error = office_formats::FormatError::new("é".repeat(100), "👩🏽‍💻".repeat(1000));
    assert!(error.code.len() <= 64 && error.message.len() <= 4096);
    assert!(error.message.ends_with('…'));
}

const EXTERNAL_FORMATS: [DocumentFormat; 5] = [
    DocumentFormat::Docx,
    DocumentFormat::Odt,
    DocumentFormat::Rtf,
    DocumentFormat::Txt,
    DocumentFormat::Pdf,
];

fn assert_source_formatting_warning_survives_native_reopen(file: &WriterFile) {
    let native = serialize_writer_file(file).unwrap();
    let reopened = deserialize_writer_file(&native).unwrap();
    assert_eq!(&reopened, file);
    for candidate in [file, &reopened] {
        assert!(
            assess_file(DocumentFormat::Writer, candidate)
                .unwrap()
                .warnings
                .is_empty()
        );
        for format in EXTERNAL_FORMATS {
            let expected = if format == DocumentFormat::Docx
                && file.original_source().unwrap().format() == "docx"
            {
                "docx_formatting_changes"
            } else {
                "original_formatting_changes"
            };
            assert!(
                assess_file(format, candidate)
                    .unwrap()
                    .warnings
                    .iter()
                    .any(|warning| warning.code == expected),
                "{} source to {format:?}",
                file.original_source().unwrap().format()
            );
        }
    }
    assert_eq!(serialize_writer_file(file).unwrap(), native);
}

#[test]
fn normalized_docx_original_warns_for_every_external_copy_and_not_native_saving() {
    let original = include_bytes!(
        "../../../tests/fixtures/docx/relationships/phase2-libreoffice-unicode.docx"
    );
    let report = import_document_with_warnings(
        DocumentFormat::Docx,
        original,
        Id::new("rtl-cross-format-assessment").unwrap(),
        "original.docx",
    )
    .unwrap();
    assert!(report.has_formatting_normalizations);
    assert!(
        report
            .warnings
            .iter()
            .any(|notice| notice.path == "rPr/rtl")
    );
    let ImportOutcome::Editable(file) = report.outcome else {
        panic!("known DOCX formatting must remain editable")
    };
    assert_source_formatting_warning_survives_native_reopen(&file);
    assert_eq!(file.original_source().unwrap().bytes(), original);
}

#[test]
fn truncated_rtf_notices_remain_meaningful_for_external_copies_after_native_reopen() {
    let source = format!("{{\\rtf1 {}\\kerning1 text}}", "\\lang1033 ".repeat(140));
    let report = import_document_with_warnings(
        DocumentFormat::Rtf,
        source.as_bytes(),
        Id::new("rtf-warning-budget").unwrap(),
        "original.rtf",
    )
    .unwrap();
    assert_eq!(report.warnings.len(), 128);
    assert!(report.omitted_warnings >= 13);
    assert!(report.has_formatting_normalizations);
    assert!(
        !report
            .warnings
            .iter()
            .any(|notice| notice.path == "kerning")
    );
    let ImportOutcome::Editable(file) = report.outcome else {
        panic!("known RTF formatting must remain editable")
    };
    assert_source_formatting_warning_survives_native_reopen(&file);
    assert_eq!(file.original_source().unwrap().bytes(), source.as_bytes());
}

#[test]
fn ordinary_source_imports_do_not_create_original_formatting_warnings() {
    let document = formatted_document(Default::default(), Default::default());
    for source_format in [
        DocumentFormat::Docx,
        DocumentFormat::Odt,
        DocumentFormat::Rtf,
    ] {
        let source = export_document(source_format, &document).unwrap();
        let report = import_document_with_warnings(
            source_format,
            &source,
            Id::new("ordinary-assessment").unwrap(),
            &format!("ordinary.{}", source_format.extension()),
        )
        .unwrap();
        assert!(!report.has_formatting_normalizations, "{source_format:?}");
        let ImportOutcome::Editable(file) = report.outcome else {
            panic!("ordinary generated source must be editable")
        };
        for format in EXTERNAL_FORMATS {
            assert!(
                !assess_file(format, &file)
                    .unwrap()
                    .warnings
                    .iter()
                    .any(|warning| matches!(
                        warning.code.as_str(),
                        "original_formatting_changes" | "docx_formatting_changes"
                    )),
                "{source_format:?} source to {format:?}"
            );
        }
    }
}

fn assessment_xml_change(
    source: &[u8],
    part: &str,
    update: impl FnOnce(String) -> String,
) -> Vec<u8> {
    use std::io::{Cursor, Read, Write};
    let mut input = zip::ZipArchive::new(Cursor::new(source)).unwrap();
    let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let mut update = Some(update);
    // Preserve package order and an uncompressed first ODT mimetype member.
    for index in 0..input.len() {
        let mut file = input.by_index(index).unwrap();
        let name = file.name().unwrap().to_string();
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        if name == part {
            bytes = update.take().unwrap()(String::from_utf8(bytes).unwrap()).into_bytes();
        }
        output
            .start_file(
                name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
        output.write_all(&bytes).unwrap();
    }
    assert!(update.is_none(), "requested XML part exists");
    output.finish().unwrap().into_inner()
}

#[test]
fn odt_meaningful_normalization_after_inert_notice_truncation_survives_native_reopen() {
    let document = formatted_document(Default::default(), Default::default());
    let original = export_document(DocumentFormat::Odt, &document).unwrap();
    let gallery = (0..70)
        .map(|index| format!(
            "<style:style style:name=\"Budget{index}\" style:family=\"paragraph\"><style:paragraph-properties style:contextual-spacing=\"false\" style:font-independent-line-spacing=\"false\"/></style:style>"
        ))
        .collect::<String>();
    let inert = assessment_xml_change(&original, "styles.xml", |xml| {
        xml.replace("</office:styles>", &format!("{gallery}</office:styles>"))
    });
    let inert_report = import_document_with_warnings(
        DocumentFormat::Odt,
        &inert,
        Id::new("odt-inert-warning-budget").unwrap(),
        "original.odt",
    )
    .unwrap();
    assert_eq!(inert_report.warnings.len(), 128);
    assert!(inert_report.omitted_warnings >= 12);
    assert!(!inert_report.has_formatting_normalizations);
    let ImportOutcome::Editable(inert_file) = inert_report.outcome else {
        panic!("known inactive preferences must remain editable")
    };
    assert!(
        !assess_file(DocumentFormat::Docx, &inert_file)
            .unwrap()
            .warnings
            .iter()
            .any(|notice| notice.code == "original_formatting_changes")
    );
    let source = assessment_xml_change(&inert, "styles.xml", |xml| {
        xml.replace("</office:styles>", "<style:style style:name=\"LanguageStyle\" style:family=\"paragraph\"><style:text-properties fo:language=\"en\"/></style:style></office:styles>")
    });
    let report = import_document_with_warnings(
        DocumentFormat::Odt,
        &source,
        Id::new("odt-meaningful-warning-budget").unwrap(),
        "original.odt",
    )
    .unwrap();
    assert_eq!(report.warnings.len(), 128);
    assert!(report.omitted_warnings >= 13);
    assert!(report.has_formatting_normalizations);
    assert!(
        !report
            .warnings
            .iter()
            .any(|notice| notice.path.contains("language"))
    );
    let ImportOutcome::Editable(file) = report.outcome else {
        panic!("known language formatting must remain editable")
    };
    assert_source_formatting_warning_survives_native_reopen(&file);
    assert_eq!(file.original_source().unwrap().bytes(), source);
}

#[test]
fn unusable_odt_and_rtf_originals_cannot_be_assessed_as_safe_external_copies() {
    let document = formatted_document(Default::default(), Default::default());
    let odt = export_document(DocumentFormat::Odt, &document).unwrap();
    let unsupported_odt = assessment_xml_change(&odt, "content.xml", |xml| {
        xml.replace(
            "</office:text>",
            "<text:p><text:note>Unsupported meaningful note</text:note></text:p></office:text>",
        )
    });
    for (source_format, source) in [
        (DocumentFormat::Odt, unsupported_odt.as_slice()),
        (DocumentFormat::Rtf, b"{\\rtf1\\unknown42 text}".as_slice()),
    ] {
        for (media_type, expected) in [
            ("application/zip", "invalid_original_source"),
            (source_format.media_type(), "unsupported_original_source"),
        ] {
            let original = OriginalSource::new(
                source_format.extension(),
                media_type,
                format!("original.{}", source_format.extension()),
                source,
            )
            .unwrap();
            let file = WriterFile::with_original_source(document.clone(), original).unwrap();
            let before = serialize_writer_file(&file).unwrap();
            assert!(
                assess_file(DocumentFormat::Writer, &file)
                    .unwrap()
                    .warnings
                    .is_empty()
            );
            for format in EXTERNAL_FORMATS {
                assert_eq!(
                    assess_file(format, &file).unwrap_err().code,
                    expected,
                    "{source_format:?} source to {format:?}"
                );
            }
            assert_eq!(serialize_writer_file(&file).unwrap(), before);
            assert_eq!(file.original_source().unwrap().bytes(), source);
        }
    }
}

#[test]
fn unused_odt_header_footer_offsets_do_not_warn_about_visible_loss() {
    fn document(offset: i32) -> Document {
        let mut margins = office_layout::LayoutOptions::default().page_margins;
        margins.header.0 = offset;
        margins.footer.0 = 0;
        Document::from_parts(
            Id::new("document").unwrap(),
            Default::default(),
            vec![
                Section::new(
                    Id::new("section").unwrap(),
                    vec![Block::Paragraph(Paragraph::new(
                        Id::new("paragraph").unwrap(),
                        vec![TextRun::new("Body")],
                    ))],
                )
                .with_properties(SectionProperties {
                    page_size: None,
                    page_margins: Some(margins),
                }),
            ],
            vec![],
        )
        .unwrap()
    }
    let has_warning = |document: &Document| {
        assess_export(DocumentFormat::Odt, document)
            .unwrap()
            .warnings
            .iter()
            .any(|warning| warning.code == "odt_header_footer_offsets")
    };
    assert!(!has_warning(&document(720)));
    assert!(!has_warning(&document(0)));
}

fn formatted_document(
    character: office_core::CharacterFormat,
    paragraph: office_core::ParagraphFormat,
) -> Document {
    Document::from_parts(
        Id::new("document").unwrap(),
        Default::default(),
        vec![Section::new(
            Id::new("section").unwrap(),
            vec![Block::Paragraph(
                Paragraph::new(
                    Id::new("paragraph").unwrap(),
                    vec![TextRun {
                        text: "Café e\u{301} 👩🏽‍💻".into(),
                        format: character,
                    }],
                )
                .with_format(paragraph),
            )],
        )],
        vec![],
    )
    .unwrap()
}

fn text_warns(document: &Document) -> bool {
    let before = document.clone();
    let report = assess_export(DocumentFormat::Txt, document).unwrap();
    assert_eq!(document, &before);
    report
        .warnings
        .iter()
        .any(|warning| warning.code == "plain_text_only")
}

#[test]
fn supported_rich_saves_do_not_warn_about_internal_identifiers_or_property_distinctions() {
    let document = formatted_document(
        office_core::CharacterFormat {
            bold: Some(true),
            font_family: Some("Tinos".into()),
            ..Default::default()
        },
        office_core::ParagraphFormat {
            alignment: Some(office_core::Alignment::Right),
            ..Default::default()
        },
    );
    for format in [
        DocumentFormat::Writer,
        DocumentFormat::Docx,
        DocumentFormat::Odt,
        DocumentFormat::Rtf,
    ] {
        assert!(
            assess_export(format, &document)
                .unwrap()
                .warnings
                .is_empty(),
            "{format:?}"
        );
    }
}

#[test]
fn text_warning_tracks_effective_visible_character_changes_not_explicit_defaults() {
    use office_core::{CharacterFormat, HalfPoints, TextColor, Underline};
    let default = CharacterFormat {
        bold: Some(false),
        italic: Some(false),
        underline: Some(Underline::None),
        font_family: Some("Arimo".into()),
        font_size: Some(HalfPoints(24)),
        color: Some(TextColor::Rgb {
            red: 36,
            green: 33,
            blue: 29,
        }),
    };
    assert!(!text_warns(&formatted_document(
        Default::default(),
        Default::default()
    )));
    assert!(!text_warns(&formatted_document(
        default,
        Default::default()
    )));
    for format in [
        CharacterFormat {
            bold: Some(true),
            ..Default::default()
        },
        CharacterFormat {
            italic: Some(true),
            ..Default::default()
        },
        CharacterFormat {
            underline: Some(Underline::Single),
            ..Default::default()
        },
        CharacterFormat {
            font_family: Some("Tinos".into()),
            ..Default::default()
        },
        CharacterFormat {
            font_size: Some(HalfPoints(28)),
            ..Default::default()
        },
        CharacterFormat {
            color: Some(TextColor::Rgb {
                red: 128,
                green: 0,
                blue: 0,
            }),
            ..Default::default()
        },
    ] {
        assert!(text_warns(&formatted_document(format, Default::default())));
    }
}

#[test]
fn text_warning_tracks_visible_paragraph_formatting_and_breaks() {
    use office_core::{Alignment, Indentation, LineSpacing, ParagraphFormat, Twips};
    let default = ParagraphFormat {
        alignment: Some(Alignment::Left),
        spacing_before: Some(Twips(0)),
        spacing_after: Some(Twips(160)),
        line_spacing: Some(LineSpacing::Multiple { value: 360 }),
        page_break_before: Some(false),
        indentation: Some(Indentation {
            left: Some(Twips(0)),
            right: Some(Twips(0)),
            first_line: Some(Twips(0)),
        }),
    };
    assert!(!text_warns(&formatted_document(
        Default::default(),
        default
    )));
    for format in [
        ParagraphFormat {
            alignment: Some(Alignment::Center),
            ..Default::default()
        },
        ParagraphFormat {
            spacing_before: Some(Twips(20)),
            ..Default::default()
        },
        ParagraphFormat {
            spacing_after: Some(Twips(240)),
            ..Default::default()
        },
        ParagraphFormat {
            line_spacing: Some(LineSpacing::Exact { height: Twips(360) }),
            ..Default::default()
        },
        ParagraphFormat {
            page_break_before: Some(true),
            ..Default::default()
        },
        ParagraphFormat {
            indentation: Some(Indentation {
                first_line: Some(Twips(-240)),
                ..Default::default()
            }),
            ..Default::default()
        },
    ] {
        assert!(text_warns(&formatted_document(Default::default(), format)));
    }
    let document = Document::from_parts(
        Id::new("break-document").unwrap(),
        Default::default(),
        vec![Section::new(
            Id::new("break-section").unwrap(),
            vec![Block::Paragraph(Paragraph::new(
                Id::new("break-paragraph").unwrap(),
                vec![TextRun::new("A\nB")],
            ))],
        )],
        vec![],
    )
    .unwrap();
    assert!(text_warns(&document));
}

#[test]
fn text_warning_resolves_used_styles_and_ignores_unused_style_libraries() {
    use office_core::{CharacterFormat, NamedStyle};
    let mut style = NamedStyle::new(Id::new("emphasis").unwrap(), "Emphasis");
    style.character_format.bold = Some(true);
    let document = |used: bool, reset: bool| {
        Document::from_parts(
            Id::new("style-document").unwrap(),
            Default::default(),
            vec![Section::new(
                Id::new("style-section").unwrap(),
                vec![Block::Paragraph({
                    let paragraph = Paragraph::new(
                        Id::new("style-paragraph").unwrap(),
                        vec![TextRun {
                            text: "Body".into(),
                            format: CharacterFormat {
                                bold: reset.then_some(false),
                                ..Default::default()
                            },
                        }],
                    );
                    if used {
                        paragraph.with_style(style.id.clone())
                    } else {
                        paragraph
                    }
                })],
            )],
            vec![style.clone()],
        )
        .unwrap()
    };
    assert!(!text_warns(&document(false, false)));
    assert!(text_warns(&document(true, false)));
    assert!(!text_warns(&document(true, true)));
}

#[test]
fn text_warning_tracks_page_and_section_structure_but_not_unused_offsets() {
    let defaults = office_layout::LayoutOptions::default();
    let document = |properties: SectionProperties, second: bool| {
        let mut sections = vec![
            Section::new(
                Id::new("page-section").unwrap(),
                vec![Block::Paragraph(Paragraph::new(
                    Id::new("page-paragraph").unwrap(),
                    vec![TextRun::new("Body")],
                ))],
            )
            .with_properties(properties),
        ];
        if second {
            sections.push(Section::new(Id::new("second-section").unwrap(), vec![]));
        }
        Document::from_parts(
            Id::new("page-document").unwrap(),
            Default::default(),
            sections,
            vec![],
        )
        .unwrap()
    };
    let mut margins = defaults.page_margins;
    margins.header.0 = 0;
    margins.footer.0 = 0;
    assert!(!text_warns(&document(
        SectionProperties {
            page_size: Some(defaults.page_size),
            page_margins: Some(margins)
        },
        false
    )));
    assert!(text_warns(&document(Default::default(), true)));
    assert!(text_warns(&document(
        SectionProperties {
            page_size: Some(office_layout::LayoutOptions::a4().page_size),
            page_margins: None
        },
        false
    )));
    margins.left.0 += 120;
    assert!(text_warns(&document(
        SectionProperties {
            page_size: None,
            page_margins: Some(margins)
        },
        false
    )));
}

#[test]
fn model_only_writer_export_cannot_silently_drop_a_retained_source() {
    let ImportOutcome::Editable(file) = import_document(
        DocumentFormat::Txt,
        b"Original",
        Id::new("d").unwrap(),
        "original.txt",
    )
    .unwrap() else {
        panic!("TXT is supported")
    };
    assert_eq!(
        export_document(DocumentFormat::Writer, file.document())
            .unwrap_err()
            .code,
        "source_aware_native_required"
    );
}

#[test]
fn full_file_export_preserves_native_content_and_original_exactly() {
    let ImportOutcome::Editable(file) = import_document(
        DocumentFormat::Txt,
        b"Original\r\n",
        Id::new("d").unwrap(),
        "original.txt",
    )
    .unwrap() else {
        panic!("TXT is supported")
    };
    let bytes = export_file(DocumentFormat::Writer, &file).unwrap();
    let ImportOutcome::Editable(reopened) = import_document(
        DocumentFormat::Writer,
        &bytes,
        Id::new("unused").unwrap(),
        "copy.writer",
    )
    .unwrap() else {
        panic!("native file is supported")
    };
    assert_eq!(reopened, file);
    assert_eq!(reopened.original_source().unwrap().bytes(), b"Original\r\n");
}

#[test]
fn full_file_docx_copy_keeps_safe_source_information_and_current_content_without_mutation() {
    use std::io::{Cursor, Read};
    let original =
        include_bytes!("../../../tests/fixtures/docx/relationships/supported-auxiliary-parts.docx");
    let ImportOutcome::Editable(imported) = import_document(
        DocumentFormat::Docx,
        original,
        Id::new("copy-document").unwrap(),
        "original.docx",
    )
    .unwrap() else {
        panic!("supported DOCX fixture must be editable")
    };
    let current = Document::from_parts(
        imported.document().id().clone(),
        office_core::DocumentMetadata {
            title: "Current title".into(),
        },
        vec![Section::new(
            Id::new("current-section").unwrap(),
            vec![Block::Paragraph(Paragraph::new(
                Id::new("current-paragraph").unwrap(),
                vec![TextRun::new("Updated copy")],
            ))],
        )],
        vec![],
    )
    .unwrap();
    let file = office_core::WriterFile::with_original_source(
        current,
        imported.original_source().unwrap().clone(),
    )
    .unwrap();
    let before = office_core::serialize_writer_file(&file).unwrap();
    assert!(
        assess_file(DocumentFormat::Docx, &file)
            .unwrap()
            .warnings
            .is_empty()
    );
    let output = export_file(DocumentFormat::Docx, &file).unwrap();
    assert_eq!(office_core::serialize_writer_file(&file).unwrap(), before);
    assert_eq!(file.original_source().unwrap().bytes(), original);
    let mut package = zip::ZipArchive::new(Cursor::new(output)).unwrap();
    let mut part = |name: &str| {
        let mut text = String::new();
        package
            .by_name(name)
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        text
    };
    let document = part("word/document.xml");
    assert!(document.contains("Updated copy"));
    assert!(!document.contains("DOCX relationship fixture"));
    let core = part("docProps/core.xml");
    assert!(core.contains("Current title"));
    assert!(core.contains("Fixture Author"));
    assert!(part("docProps/custom.xml").contains("Relationship regression"));
    assert!(part("docProps/app.xml").contains("Writer relationship fixture"));
}

#[test]
fn unusable_docx_originals_fail_explicitly_instead_of_falling_back_to_lossy_export() {
    let document = formatted_document(Default::default(), Default::default());
    let original =
        include_bytes!("../../../tests/fixtures/docx/relationships/supported-auxiliary-parts.docx");
    for (media, bytes, code) in [
        (
            "application/zip",
            original.as_slice(),
            "invalid_docx_source",
        ),
        (
            office_docx::DOCX_MEDIA_TYPE,
            b"not a ZIP".as_slice(),
            "invalid_docx_source",
        ),
    ] {
        let source =
            office_core::OriginalSource::new("docx", media, "original.docx", bytes).unwrap();
        let file = office_core::WriterFile::with_original_source(document.clone(), source).unwrap();
        let before = office_core::serialize_writer_file(&file).unwrap();
        assert_eq!(
            assess_file(DocumentFormat::Docx, &file).unwrap_err().code,
            code
        );
        assert_eq!(
            export_file(DocumentFormat::Docx, &file).unwrap_err().code,
            code
        );
        assert_eq!(office_core::serialize_writer_file(&file).unwrap(), before);
    }
}

#[test]
fn cross_format_copies_disclose_proven_document_property_loss_but_ordinary_copies_do_not() {
    fn imported(bytes: &[u8]) -> office_core::WriterFile {
        let ImportOutcome::Editable(file) = import_document(
            DocumentFormat::Docx,
            bytes,
            Id::new("properties").unwrap(),
            "original.docx",
        )
        .unwrap() else {
            panic!("supported DOCX must be editable")
        };
        file
    }
    let rich = imported(include_bytes!(
        "../../../tests/fixtures/docx/relationships/supported-auxiliary-parts.docx"
    ));
    let ordinary = imported(include_bytes!(
        "../../../tests/fixtures/docx/supported-basic.docx"
    ));
    let before = office_core::serialize_writer_file(&rich).unwrap();
    for format in [
        DocumentFormat::Odt,
        DocumentFormat::Rtf,
        DocumentFormat::Txt,
        DocumentFormat::Pdf,
    ] {
        assert!(
            assess_file(format, &rich)
                .unwrap()
                .warnings
                .iter()
                .any(|warning| warning.code == "document_properties_omitted"),
            "{format:?}"
        );
        assert!(
            !assess_file(format, &ordinary)
                .unwrap()
                .warnings
                .iter()
                .any(|warning| warning.code == "document_properties_omitted"),
            "{format:?}"
        );
    }
    for format in [DocumentFormat::Writer, DocumentFormat::Docx] {
        assert!(
            !assess_file(format, &rich)
                .unwrap()
                .warnings
                .iter()
                .any(|warning| warning.code == "document_properties_omitted"),
            "{format:?}"
        );
    }
    assert_eq!(office_core::serialize_writer_file(&rich).unwrap(), before);
}

#[test]
fn meaningful_bookmark_loss_is_disclosed_across_formats_but_cursor_caches_are_not() {
    use std::io::{Cursor, Read, Write};
    let document = formatted_document(Default::default(), Default::default());
    for name in ["Important", "_GoBack"] {
        let original = office_docx::export_docx(&document).unwrap();
        let mut archive = zip::ZipArchive::new(Cursor::new(original)).unwrap();
        let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for index in 0..archive.len() {
            let mut part = archive.by_index(index).unwrap();
            let part_name = part.name().unwrap().into_owned();
            let mut bytes = Vec::new();
            part.read_to_end(&mut bytes).unwrap();
            if part_name == "word/document.xml" {
                let xml = String::from_utf8(bytes).unwrap();
                let marker = format!(
                    "<w:bookmarkStart w:id=\"1\" w:name=\"{name}\"/><w:bookmarkEnd w:id=\"1\"/></w:p>"
                );
                bytes = xml.replacen("</w:p>", &marker, 1).into_bytes();
            }
            output
                .start_file(part_name, zip::write::SimpleFileOptions::default())
                .unwrap();
            output.write_all(&bytes).unwrap();
        }
        let original = output.finish().unwrap().into_inner();
        let ImportOutcome::Editable(file) = import_document(
            DocumentFormat::Docx,
            &original,
            Id::new("bookmarks").unwrap(),
            "original.docx",
        )
        .unwrap() else {
            panic!("bookmark document must be editable")
        };
        for format in [
            DocumentFormat::Docx,
            DocumentFormat::Odt,
            DocumentFormat::Rtf,
            DocumentFormat::Txt,
            DocumentFormat::Pdf,
        ] {
            let warned = assess_file(format, &file)
                .unwrap()
                .warnings
                .iter()
                .any(|warning| warning.code.contains("bookmarks_omitted"));
            assert_eq!(warned, name == "Important", "{format:?}: {name}");
        }
        assert!(
            assess_file(DocumentFormat::Writer, &file)
                .unwrap()
                .warnings
                .is_empty()
        );
        assert_eq!(file.original_source().unwrap().bytes(), original);
    }
}

#[test]
fn uncopiable_theme_payload_does_not_block_other_formats_or_modify_the_original() {
    use std::io::{Cursor, Read, Write};
    let fixture =
        include_bytes!("../../../tests/fixtures/docx/relationships/supported-auxiliary-parts.docx");
    for payload in [
        "<foreign:payload xmlns:foreign=\"urn:unmodeled\">Source only</foreign:payload>",
        "<a:objectDefaults xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" r:embed=\"rIdHidden\"/>",
    ] {
        let mut archive = zip::ZipArchive::new(Cursor::new(fixture)).unwrap();
        let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for index in 0..archive.len() {
            let mut part = archive.by_index(index).unwrap();
            let name = part.name().unwrap().into_owned();
            let mut bytes = Vec::new();
            part.read_to_end(&mut bytes).unwrap();
            if name == "word/theme/theme1.xml" {
                bytes = String::from_utf8(bytes)
                    .unwrap()
                    .replacen("</a:theme>", &format!("{payload}</a:theme>"), 1)
                    .into_bytes();
            }
            output
                .start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            output.write_all(&bytes).unwrap();
        }
        let original = output.finish().unwrap().into_inner();
        let ImportOutcome::Editable(file) = import_document(
            DocumentFormat::Docx,
            &original,
            Id::new("theme").unwrap(),
            "original.docx",
        )
        .unwrap() else {
            panic!("source-only theme payload must not block semantic import")
        };
        let before = office_core::serialize_writer_file(&file).unwrap();
        assert!(
            assess_file(DocumentFormat::Docx, &file)
                .unwrap()
                .warnings
                .iter()
                .any(|warning| warning.code == "docx_theme_omitted")
        );
        let copied = export_file(DocumentFormat::Docx, &file).unwrap();
        let mut package = zip::ZipArchive::new(Cursor::new(copied)).unwrap();
        assert!(package.by_name("word/theme/theme1.xml").is_err());
        for format in [
            DocumentFormat::Odt,
            DocumentFormat::Rtf,
            DocumentFormat::Txt,
            DocumentFormat::Pdf,
        ] {
            let report = assess_file(format, &file).unwrap();
            assert!(
                !report
                    .warnings
                    .iter()
                    .any(|warning| warning.code == "docx_theme_omitted"),
                "{format:?}"
            );
            assert!(
                !export_file(format, &file).unwrap().is_empty(),
                "{format:?}"
            );
        }
        assert_eq!(office_core::serialize_writer_file(&file).unwrap(), before);
        assert_eq!(file.original_source().unwrap().bytes(), original);
    }
}

#[test]
fn exports_outside_their_own_editable_profiles_are_not_published() {
    fn document(title: String, format: office_core::CharacterFormat) -> Document {
        Document::from_parts(
            Id::new("d").unwrap(),
            office_core::DocumentMetadata { title },
            vec![Section::new(
                Id::new("s").unwrap(),
                vec![Block::Paragraph(Paragraph::new(
                    Id::new("p").unwrap(),
                    vec![TextRun {
                        text: "Body".into(),
                        format,
                    }],
                ))],
            )],
            vec![],
        )
        .unwrap()
    }
    let cases = [
        (
            DocumentFormat::Rtf,
            document(
                String::new(),
                office_core::CharacterFormat {
                    font_family: Some("F".repeat(257)),
                    ..Default::default()
                },
            ),
        ),
        (
            DocumentFormat::Rtf,
            document("t".repeat(16 * 1024 + 1), Default::default()),
        ),
        (
            DocumentFormat::Odt,
            document(
                String::new(),
                office_core::CharacterFormat {
                    font_size: Some(office_core::HalfPoints(u32::MAX)),
                    ..Default::default()
                },
            ),
        ),
    ];
    for (format, document) in cases {
        let before = office_core::serialize_document(&document).unwrap();
        assert_eq!(
            export_document(format, &document).unwrap_err().code,
            "unsupported_export"
        );
        assert_eq!(office_core::serialize_document(&document).unwrap(), before);
    }
}
