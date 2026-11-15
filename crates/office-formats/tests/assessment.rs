//! Conversion warnings are bounded and report meaningful visible losses.

use office_core::{Block, Document, Id, Paragraph, Section, SectionProperties, TextRun};
use office_formats::{
    DocumentFormat, FormatDiagnostic, ImportOutcome, assess_export, assess_file, export_document,
    export_file, import_document,
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
