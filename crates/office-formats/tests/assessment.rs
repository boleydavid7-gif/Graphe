//! Conversion warnings are bounded and disclose unrepresented native properties.

use office_core::{Block, Document, Id, Paragraph, Section, SectionProperties, TextRun};
use office_formats::{
    DocumentFormat, FormatDiagnostic, ImportOutcome, assess_export, export_document, export_file,
    import_document,
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
fn odt_offset_warning_is_specific_to_explicit_unrepresented_offsets() {
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
    assert!(has_warning(&document(720)));
    assert!(!has_warning(&document(0)));
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
