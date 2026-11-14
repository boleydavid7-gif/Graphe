//! Fixed-page PDF export preserves the Rust geometry and exposes its limits.

use office_core::{
    Block, CharacterFormat, Document, DocumentMetadata, DocumentSession, GraphemeOffset,
    HalfPoints, Id, Paragraph, ParagraphFormat, Section, TextColor, TextRun, Underline,
};
use office_formats::{DocumentFormat, assess_export, export_document, import_document};
use office_layout::{LayoutOptions, layout_document};

fn id(value: &str) -> Id {
    Id::new(value).unwrap()
}

fn document(text: &str) -> Document {
    Document::from_parts(
        id("private-native-document-identifier"),
        DocumentMetadata {
            title: "PDF reference".into(),
        },
        vec![Section::new(
            id("section"),
            vec![Block::Paragraph(Paragraph::new(
                id("paragraph"),
                vec![TextRun::new(text)],
            ))],
        )],
        vec![],
    )
    .unwrap()
}

fn pdf(document: &Document) -> Vec<u8> {
    export_document(DocumentFormat::Pdf, document).unwrap()
}

fn page_count(bytes: &[u8]) -> usize {
    let token = b"/Type/Page";
    bytes
        .windows(token.len() + 1)
        .filter(|window| &window[..token.len()] == token && window[token.len()] != b's')
        .count()
}

fn contains(bytes: &[u8], text: &str) -> bool {
    bytes
        .windows(text.len())
        .any(|window| window == text.as_bytes())
}

#[test]
fn an_empty_document_produces_one_complete_deterministic_page() {
    let document = Document::new(id("empty")).unwrap();
    let bytes = pdf(&document);
    assert!(bytes.starts_with(b"%PDF-1.7"));
    assert!(bytes.ends_with(b"%%EOF"));
    assert_eq!(page_count(&bytes), 1);
    assert!(contains(&bytes, "/MediaBox[0 0 612 792]"));
    assert!(contains(&bytes, "xref"));
    assert_eq!(bytes, pdf(&document));
}

#[test]
fn paragraphs_continue_over_the_same_pages_as_native_layout() {
    let file = office_core::deserialize_writer_file(include_bytes!(
        "../../../tests/fixtures/layout/layout-v1.writer"
    ))
    .unwrap();
    let layout = layout_document(file.document(), &LayoutOptions::default()).unwrap();
    let bytes = pdf(file.document());
    assert_eq!(layout.pages.len(), 4);
    assert_eq!(page_count(&bytes), layout.pages.len());
    assert!(contains(&bytes, "/MediaBox[0 0 300 225]"));
    assert!(contains(&bytes, "/MediaBox[0 0 595.2969 841.90625]"));
    assert!(contains(&bytes, "/Subtype/Form"));
    assert_eq!(bytes, pdf(file.document()));
}

#[test]
fn mixed_fonts_color_underline_and_unicode_use_the_native_vector_outlines() {
    let document = Document::from_parts(
        id("mixed"),
        DocumentMetadata::default(),
        vec![Section::new(
            id("s"),
            vec![Block::Paragraph(Paragraph::new(
                id("p"),
                vec![
                    TextRun::new("Cafe\u{301} 👩🏽‍💻 العربية नमस्ते שלום "),
                    TextRun {
                        text: "mixed".into(),
                        format: CharacterFormat {
                            bold: Some(true),
                            italic: Some(true),
                            underline: Some(Underline::Double),
                            font_family: Some("Tinos".into()),
                            font_size: Some(HalfPoints(36)),
                            color: Some(TextColor::Rgb {
                                red: 255,
                                green: 0,
                                blue: 0,
                            }),
                        },
                    },
                ],
            ))],
        )],
        vec![],
    )
    .unwrap();
    let before = office_core::serialize_document(&document).unwrap();
    let layout = layout_document(&document, &LayoutOptions::default()).unwrap();
    assert!(
        !layout
            .diagnostics
            .iter()
            .any(|warning| warning.kind == "missing_glyph")
    );
    let bytes = pdf(&document);
    assert!(contains(&bytes, "1 0 0 rg"));
    assert!(contains(&bytes, " re"));
    assert_eq!(
        bytes
            .windows(b"/Subtype/Form".len())
            .filter(|window| *window == b"/Subtype/Form")
            .count(),
        layout.outlines.len()
    );
    assert!(!contains(&bytes, "/FontFile"));
    assert!(!contains(&bytes, " BT"));
    assert_eq!(before, office_core::serialize_document(&document).unwrap());
}

#[test]
fn export_does_not_publish_native_identifiers_or_make_pdf_an_import_format() {
    let document = document("ordinary text");
    let bytes = pdf(&document);
    assert!(!contains(&bytes, document.id().as_str()));
    let error = import_document(DocumentFormat::Pdf, &bytes, id("import"), "file.pdf").unwrap_err();
    assert_eq!(error.code, "export_only");
}

#[test]
fn export_assessment_discloses_outlines_and_keeps_layout_warnings() {
    let ordinary = assess_export(DocumentFormat::Pdf, &document("text")).unwrap();
    assert!(
        ordinary
            .warnings
            .iter()
            .any(|warning| warning.code == "outlined_pdf")
    );
    assert!(
        ordinary
            .warnings
            .iter()
            .any(|warning| warning.code == "native_information")
    );
    let missing = assess_export(DocumentFormat::Pdf, &document("漢字")).unwrap();
    assert!(
        missing
            .warnings
            .iter()
            .any(|warning| warning.code == "pdf_layout_missing_glyph")
    );
}

#[test]
fn unsupported_layout_is_an_error_rather_than_browser_generated_pdf() {
    let document = document("tab\tstop");
    assert!(assess_export(DocumentFormat::Pdf, &document).is_err());
    assert!(export_document(DocumentFormat::Pdf, &document).is_err());
}

#[test]
fn pdf_export_reflects_committed_editing_and_undo_without_mutating_history() {
    let mut session = DocumentSession::new(document("Before"));
    let before = pdf(session.document());
    let mut transaction = session.begin_transaction();
    let at = transaction
        .position(&id("paragraph"), GraphemeOffset::new(6))
        .unwrap();
    transaction
        .insert_text(&at, " after 👩🏽‍💻", CharacterFormat::default())
        .unwrap();
    session.commit(transaction).unwrap();
    let revision = session.revision();
    let edited = pdf(session.document());
    assert_ne!(before, edited);
    assert_eq!(revision, session.revision());
    session.undo().unwrap();
    assert_eq!(before, pdf(session.document()));
    session.redo().unwrap();
    assert_eq!(edited, pdf(session.document()));
}

#[test]
fn explicit_page_breaks_add_pages_using_the_existing_layout_rule() {
    let document = Document::from_parts(
        id("breaks"),
        DocumentMetadata::default(),
        vec![Section::new(
            id("s"),
            vec![
                Block::Paragraph(Paragraph::new(id("a"), vec![TextRun::new("Page one")])),
                Block::Paragraph(
                    Paragraph::new(id("b"), vec![TextRun::new("Page two")]).with_format(
                        ParagraphFormat {
                            page_break_before: Some(true),
                            ..Default::default()
                        },
                    ),
                ),
            ],
        )],
        vec![],
    )
    .unwrap();
    assert_eq!(page_count(&pdf(&document)), 2);
}
