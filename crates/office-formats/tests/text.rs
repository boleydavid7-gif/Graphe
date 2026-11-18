//! Unicode text files preserve source bytes while edits use native transactions.

use office_core::{Block, CharacterFormat, DocumentSession, GraphemeOffset, Id, TextRun};
use office_formats::{
    DocumentFormat, ImportOutcome, assess_export, export_document, import_document,
};

fn read(bytes: &[u8], filename: &str) -> office_core::WriterFile {
    let outcome = import_document(
        DocumentFormat::Txt,
        bytes,
        Id::new("text-document").unwrap(),
        filename,
    )
    .unwrap();
    let ImportOutcome::Editable(file) = outcome else {
        panic!("plain Unicode text must be editable")
    };
    file
}

fn paragraphs(file: &office_core::WriterFile) -> Vec<String> {
    file.document()
        .sections()
        .iter()
        .flat_map(|s| s.blocks())
        .map(|block| {
            let Block::Paragraph(p) = block else {
                panic!("expected paragraph fixture")
            };
            p.runs().iter().map(|r| r.text.as_str()).collect()
        })
        .collect()
}

#[test]
fn empty_text_is_editable_without_inventing_original_bytes() {
    let file = read(b"", "blank.txt");
    assert_eq!(paragraphs(&file), [""]);
    assert!(file.original_source().is_none());
    assert_eq!(
        export_document(DocumentFormat::Txt, file.document()).unwrap(),
        b""
    );
}

#[test]
fn utf8_roundtrip_preserves_combining_emoji_and_interior_bom() {
    let source = "Café e\u{301} 👩🏽‍💻\nالعربية नमस्ते שלום\n\u{feff}retained\n";
    let file = read(source.as_bytes(), "国際 notes.txt");
    assert_eq!(file.document().metadata().title, "国際 notes");
    assert_eq!(
        paragraphs(&file),
        [
            "Café e\u{301} 👩🏽‍💻",
            "العربية नमस्ते שלום",
            "\u{feff}retained",
            ""
        ]
    );
    assert_eq!(file.original_source().unwrap().bytes(), source.as_bytes());
    assert_eq!(
        export_document(DocumentFormat::Txt, file.document()).unwrap(),
        source.as_bytes()
    );
}

#[test]
fn newline_normalization_is_explicit_and_original_is_exact() {
    let source = b"first\r\nsecond\rthird\n\n";
    let file = read(source, "lines.txt");
    assert_eq!(paragraphs(&file), ["first", "second", "third", "", ""]);
    assert_eq!(file.original_source().unwrap().bytes(), source);
    assert_eq!(
        export_document(DocumentFormat::Txt, file.document()).unwrap(),
        b"first\nsecond\nthird\n\n"
    );
}

#[test]
fn utf8_bom_is_transport_data_only_at_start() {
    let file = read("\u{feff}one\u{feff}two".as_bytes(), "bom.txt");
    assert_eq!(paragraphs(&file), ["one\u{feff}two"]);
}

#[test]
fn bom_marked_utf16_both_orders_decode_without_replacement() {
    let text = "e\u{301} 👩🏽‍💻\r\nשלום";
    for little in [true, false] {
        let mut source = if little {
            vec![0xff, 0xfe]
        } else {
            vec![0xfe, 0xff]
        };
        for unit in text.encode_utf16() {
            source.extend(if little {
                unit.to_le_bytes()
            } else {
                unit.to_be_bytes()
            });
        }
        let file = read(&source, "utf16.txt");
        assert_eq!(paragraphs(&file), ["e\u{301} 👩🏽‍💻", "שלום"]);
        assert_eq!(file.original_source().unwrap().bytes(), source);
    }
}

#[test]
fn malformed_and_unsupported_encodings_are_rejected() {
    for source in [
        vec![0x80],
        vec![0xff, 0xfe, 0x61],
        vec![0xff, 0xfe, 0, 0xd8],
        vec![0xfe, 0xff, 0xdc, 0],
        vec![0xff, 0xfe, 0, 0, 0x61, 0, 0, 0],
        b"embedded\0nul".to_vec(),
    ] {
        assert!(
            import_document(
                DocumentFormat::Txt,
                &source,
                Id::new("d").unwrap(),
                "bad.txt"
            )
            .is_err()
        );
    }
}

#[test]
fn paragraph_budget_rejects_small_newline_bombs() {
    let source = vec![b'\n'; 10_000];
    assert_eq!(
        import_document(
            DocumentFormat::Txt,
            &source,
            Id::new("d").unwrap(),
            "many.txt"
        )
        .unwrap_err()
        .code,
        "file_too_large"
    );
}

#[test]
fn empty_source_still_validates_filename() {
    for filename in ["", "../bad.txt", "dir/file.txt", "bad\\file.txt", "\0.txt"] {
        assert!(
            import_document(DocumentFormat::Txt, b"", Id::new("d").unwrap(), filename).is_err()
        );
    }
}

#[test]
fn ordinary_plain_text_conversion_needs_no_loss_warning() {
    let file = read(b"sample", "sample.txt");
    let report = assess_export(DocumentFormat::Txt, file.document()).unwrap();
    assert!(report.warnings.is_empty());
    assert!(
        assess_export(DocumentFormat::Writer, file.document())
            .unwrap()
            .warnings
            .is_empty()
    );
}

#[test]
fn imported_text_edits_export_and_reopen_without_changing_original() {
    let file = read("e\u{301} 👩🏽‍💻".as_bytes(), "edit.txt");
    let original = file.original_source().unwrap().clone();
    let paragraph_id = file.document().sections()[0]
        .blocks()
        .first()
        .map(|block| {
            let Block::Paragraph(p) = block else {
                panic!("expected paragraph fixture")
            };
            p.id().clone()
        })
        .unwrap();
    let mut session = DocumentSession::new(file.document().clone());
    let mut tx = session.begin_transaction();
    let position = tx.position(&paragraph_id, GraphemeOffset::new(0)).unwrap();
    tx.insert_text(&position, "Edited ", CharacterFormat::default())
        .unwrap();
    session.commit(tx).unwrap();
    let bytes = export_document(DocumentFormat::Txt, session.document()).unwrap();
    assert_eq!(
        paragraphs(&read(&bytes, "saved.txt")),
        ["Edited e\u{301} 👩🏽‍💻"]
    );
    assert_eq!(original.bytes(), "e\u{301} 👩🏽‍💻".as_bytes());
}

#[test]
fn pdf_is_never_an_editable_input() {
    assert_eq!(
        import_document(
            DocumentFormat::Pdf,
            b"%PDF-1.7",
            Id::new("d").unwrap(),
            "p.pdf"
        )
        .unwrap_err()
        .code,
        "export_only"
    );
}

fn native_runs(runs: Vec<TextRun>) -> office_core::Document {
    office_core::Document::from_parts(
        Id::new("d").unwrap(),
        Default::default(),
        vec![office_core::Section::new(
            Id::new("s").unwrap(),
            vec![Block::Paragraph(office_core::Paragraph::new(
                Id::new("p").unwrap(),
                runs,
            ))],
        )],
        vec![],
    )
    .unwrap()
}

#[test]
fn exporting_nul_is_refused_before_save_review_or_output() {
    let document = native_runs(vec![TextRun::new("A\0B")]);
    assert_eq!(
        assess_export(DocumentFormat::Txt, &document)
            .unwrap_err()
            .code,
        "unsupported_txt_export"
    );
    assert_eq!(
        export_document(DocumentFormat::Txt, &document)
            .unwrap_err()
            .code,
        "unsupported_txt_export"
    );
}

#[test]
fn export_normalizes_crlf_across_runs_without_extra_paragraph_breaks() {
    let document = native_runs(vec![
        TextRun::new("A\r"),
        TextRun::new("\nB\rC\r"),
        TextRun::new("\r\nD"),
    ]);
    let bytes = export_document(DocumentFormat::Txt, &document).unwrap();
    assert_eq!(bytes, b"A\nB\nC\n\nD");
    assert_eq!(
        paragraphs(&read(&bytes, "copy.txt")),
        ["A", "B", "C", "", "D"]
    );
}

#[test]
fn semantic_leading_bom_survives_export_and_reopen() {
    let document = native_runs(vec![TextRun::new("\u{feff}retained\u{feff}")]);
    let bytes = export_document(DocumentFormat::Txt, &document).unwrap();
    assert!(bytes.starts_with("\u{feff}\u{feff}".as_bytes()));
    assert_eq!(
        paragraphs(&read(&bytes, "copy.txt")),
        ["\u{feff}retained\u{feff}"]
    );
}
