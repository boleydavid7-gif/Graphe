//! Forced run direction is disclosed formatting, not permission to omit text.

use std::{
    collections::BTreeMap,
    io::{Cursor, Read, Write},
};

use office_core::{
    Block, CharacterFormat, Document, DocumentSession, GraphemeOffset, HalfPoints, Id,
    OriginalSource, WriterFile, deserialize_writer_file, serialize_writer_file,
};
use office_docx::{
    DOCX_MEDIA_TYPE, DocxImportOutcome, ImportedDocx, assess_docx_copy, export_docx_with_source,
    import_docx,
};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

const WORD: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const SAFE: &[u8] =
    include_bytes!("../../../tests/fixtures/docx/relationships/supported-auxiliary-parts.docx");
const LIBREOFFICE: &[u8] =
    include_bytes!("../../../tests/fixtures/docx/relationships/phase2-libreoffice-unicode.docx");
const MIXED: &str = "Latin e\u{301} 👩🏽‍💻 — العَرَبِيَّة (١٢٣), שָׁלוֹם 42.";

fn changed(bytes: &[u8], part: &str, update: impl FnOnce(String) -> String) -> Vec<u8> {
    let mut input = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut entries = BTreeMap::new();
    for index in 0..input.len() {
        let mut file = input.by_index(index).unwrap();
        let name = file.name().unwrap().to_string();
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        entries.insert(name, bytes);
    }
    let xml = String::from_utf8(entries[part].clone()).unwrap();
    entries.insert(part.to_owned(), update(xml).into_bytes());
    let mut output = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        output
            .start_file(name, SimpleFileOptions::default())
            .unwrap();
        output.write_all(&bytes).unwrap();
    }
    output.finish().unwrap().into_inner()
}

fn body(bytes: &[u8], content: &str) -> Vec<u8> {
    changed(bytes, "word/document.xml", |_| {
        format!("<w:document xmlns:w=\"{WORD}\"><w:body>{content}</w:body></w:document>")
    })
}

fn editable(bytes: &[u8]) -> ImportedDocx {
    match import_docx(bytes, Id::new("rtl-normalization").unwrap()).unwrap() {
        DocxImportOutcome::Editable(value) => {
            assert!(value.report().is_supported());
            assert_eq!(value.original().original_bytes(), bytes);
            value.document().validate().unwrap();
            value
        }
        DocxImportOutcome::Preserved { report, .. } => panic!("unexpected blocker: {report:?}"),
    }
}

fn paragraphs(document: &Document) -> Vec<String> {
    document
        .sections()
        .iter()
        .flat_map(|section| section.blocks())
        .map(|block| {
            let Block::Paragraph(paragraph) = block else {
                panic!("expected paragraph")
            };
            paragraph
                .runs()
                .iter()
                .map(|run| run.text.as_str())
                .collect()
        })
        .collect()
}

#[test]
fn valid_forced_run_direction_preserves_logical_unicode_without_inserting_controls() {
    for direction in ["<w:rtl/>", r#"<w:rtl w:val="true"/>"#] {
        let source = body(
            SAFE,
            &format!(
                "<w:p><w:r><w:rPr>{direction}</w:rPr><w:t>{MIXED}</w:t></w:r><w:r><w:t xml:space=\"preserve\"> Ordinary run.</w:t></w:r></w:p>"
            ),
        );
        let value = editable(&source);
        assert_eq!(
            paragraphs(value.document()),
            [format!("{MIXED} Ordinary run.")]
        );
        let notice = value
            .report()
            .normalizations
            .iter()
            .find(|notice| notice.path == "rPr/rtl")
            .unwrap();
        assert!(notice.message.contains("Punctuation"));
        assert!(notice.message.contains("inherited or mixed-script"));
        assert!(value.report().has_formatting_normalizations);
        assert!(
            assess_docx_copy(value.document(), value.original())
                .unwrap()
                .formatting_may_differ
        );
    }
}

#[test]
fn direct_rtl_complex_script_appearance_is_order_independent_and_false_uses_ordinary_values() {
    let properties = r#"<w:b w:val="0"/><w:bCs/><w:i/><w:iCs w:val="0"/><w:sz w:val="22"/><w:szCs w:val="30"/><w:rFonts w:ascii="Arimo" w:hAnsi="Arimo" w:cs="Amiri" w:eastAsia="Arimo"/>"#;
    for (direction, complex) in [("<w:rtl/>", true), (r#"<w:rtl w:val="0"/>"#, false)] {
        for properties in [
            format!("{direction}{properties}"),
            format!("{properties}{direction}"),
        ] {
            let source = body(
                SAFE,
                &format!("<w:p><w:r><w:rPr>{properties}</w:rPr><w:t>{MIXED}</w:t></w:r></w:p>"),
            );
            let value = editable(&source);
            let Block::Paragraph(paragraph) = &value.document().sections()[0].blocks()[0] else {
                panic!("expected paragraph")
            };
            let format = &paragraph.runs()[0].format;
            assert_eq!(format.bold, Some(complex));
            assert_eq!(format.italic, Some(!complex));
            assert_eq!(
                format.font_size,
                Some(HalfPoints(if complex { 30 } else { 22 }))
            );
            assert_eq!(
                format.font_family.as_deref(),
                Some(if complex { "Amiri" } else { "Arimo" })
            );
            assert_eq!(
                value
                    .report()
                    .normalizations
                    .iter()
                    .any(|notice| notice.path == "rPr/rtl"),
                complex
            );
            let output = export_docx_with_source(value.document(), value.original()).unwrap();
            let reopened = editable(&output);
            let Block::Paragraph(paragraph) = &reopened.document().sections()[0].blocks()[0] else {
                panic!("expected paragraph")
            };
            assert_eq!(&paragraph.runs()[0].format, format);
            assert_eq!(paragraphs(reopened.document()), [MIXED]);
        }
    }
}

#[test]
fn inherited_rtl_is_disclosed_and_explicit_false_retains_existing_ordinary_style_projection() {
    let styled = changed(SAFE, "word/styles.xml", |xml| {
        xml.replace("</w:styles>", r#"<w:style w:type="character" w:styleId="Directional"><w:name w:val="Directional"/><w:rPr><w:rtl/><w:sz w:val="22"/><w:szCs w:val="30"/><w:rFonts w:ascii="Arimo" w:hAnsi="Arimo" w:cs="Amiri" w:eastAsia="Arimo"/></w:rPr></w:style></w:styles>"#)
    });
    let source = body(
        &styled,
        &format!(
            r#"<w:p><w:r><w:rPr><w:rStyle w:val="Directional"/></w:rPr><w:t>{MIXED}</w:t></w:r><w:r><w:rPr><w:rStyle w:val="Directional"/><w:rtl w:val="false"/></w:rPr><w:t xml:space="preserve"> LTR override.</w:t></w:r></w:p>"#
        ),
    );
    let value = editable(&source);
    let Block::Paragraph(paragraph) = &value.document().sections()[0].blocks()[0] else {
        panic!("expected paragraph")
    };
    for run in paragraph.runs() {
        assert_eq!(run.format.font_size, Some(HalfPoints(22)));
        assert_eq!(run.format.font_family.as_deref(), Some("Arimo"));
    }
    assert_eq!(
        paragraphs(value.document()),
        [format!("{MIXED} LTR override.")]
    );
    assert!(
        value
            .report()
            .normalizations
            .iter()
            .any(|notice| { notice.part == "word/styles.xml" && notice.path == "rPr/rtl" })
    );
    assert!(
        assess_docx_copy(value.document(), value.original())
            .unwrap()
            .formatting_may_differ
    );
}

#[test]
fn rtl_normalization_never_accepts_hidden_content_unknown_markup_or_paragraph_direction() {
    for content in [
        r#"<w:rPr><w:rtl/><w:vanish/></w:rPr>"#,
        r#"<w:rPr><w:rtl/><w:webHidden/></w:rPr>"#,
        r#"<w:rPr><w:rtl/><w:specVanish/></w:rPr>"#,
        r#"<w:rPr><w:rtl w:future="1"/></w:rPr>"#,
        r#"<w:rPr><w:rtl/><w:rtl/></w:rPr>"#,
        r#"<w:rPr><w:rtl><w:t>Do not discard</w:t></w:rtl></w:rPr>"#,
        r#"<w:rPr><w:rtl/><w:unknownProperty/></w:rPr>"#,
        r#"<w:rPr><w:rtl/><x:rtl xmlns:x="urn:unknown"/></w:rPr>"#,
    ] {
        assert_noneditable(&body(
            SAFE,
            &format!("<w:p><w:r>{content}<w:t>{MIXED}</w:t></w:r></w:p>"),
        ));
    }
    for properties in ["<w:bidi/>", "<w:numPr/>"] {
        assert_noneditable(&body(
            SAFE,
            &format!(
                "<w:p><w:pPr>{properties}</w:pPr><w:r><w:rPr><w:rtl/></w:rPr><w:t>{MIXED}</w:t></w:r></w:p>"
            ),
        ));
    }
    for value in ["yes", "2", "", "TRUE"] {
        let source = body(
            SAFE,
            &format!(
                "<w:p><w:r><w:rPr><w:rtl w:val=\"{value}\"/></w:rPr><w:t>{MIXED}</w:t></w:r></w:p>"
            ),
        );
        assert!(import_docx(&source, Id::new("invalid-rtl").unwrap()).is_err());
    }
}

fn assert_noneditable(source: &[u8]) {
    match import_docx(source, Id::new("guarded-rtl").unwrap()) {
        Ok(DocxImportOutcome::Preserved { original, report }) => {
            assert_eq!(original.original_bytes(), source);
            assert!(!report.is_supported());
        }
        Err(_) => {}
        Ok(other) => panic!("unsafe RTL normalization: {other:?}"),
    }
}

#[test]
fn truncated_inert_notices_cannot_hide_meaningful_rtl_or_later_content_blockers() {
    let gallery = (0..150).map(|index| format!(
        "<w:style w:type=\"paragraph\" w:styleId=\"Gallery{index}\"><w:name w:val=\"Gallery {index}\"/><w:qFormat/><w:uiPriority w:val=\"1\"/></w:style>"
    )).collect::<String>();
    let source = changed(SAFE, "word/styles.xml", |xml| {
        xml.replace("</w:styles>", &format!("{gallery}</w:styles>"))
    });
    assert!(!editable(&source).report().has_formatting_normalizations);
    let source = body(
        &source,
        &format!("<w:p><w:r><w:rPr><w:rtl/></w:rPr><w:t>{MIXED}</w:t></w:r></w:p>"),
    );
    let value = editable(&source);
    assert_eq!(value.report().normalizations.len(), 128);
    assert!(value.report().omitted_normalizations > 0);
    assert!(
        !value
            .report()
            .normalizations
            .iter()
            .any(|notice| notice.path == "rPr/rtl")
    );
    assert!(value.report().has_formatting_normalizations);
    assert!(
        assess_docx_copy(value.document(), value.original())
            .unwrap()
            .formatting_may_differ
    );
    let tracked = changed(&source, "word/document.xml", |xml| {
        xml.replace("</w:body>", "<w:p><w:ins w:id=\"1\" w:author=\"Synthetic\"><w:r><w:t>Tracked content</w:t></w:r></w:ins></w:p></w:body>")
    });
    assert_noneditable(&tracked);
}

#[test]
fn actual_libreoffice_resave_edits_and_reopens_without_changing_original_or_supported_content() {
    let value = editable(LIBREOFFICE);
    let expected = [
        "Project notes",
        "The next review will focus on dependable editing, accurate document layout, and reliable local saving. Writer keeps the document engine in Rust.",
        "Review priorities",
        "Confirm Unicode behavior — Café, e\u{301}, 👩🏽‍💻 — and preserve formatting across native save and reopen cycles.",
        "Next steps",
        "Compare the saved document with the current revision, check keyboard navigation, and retain an exported backup. Audit edit e\u{301} 👩🏽‍💻 العربية नमस्ते שלום.",
    ];
    assert_eq!(paragraphs(value.document()), expected);
    assert_eq!(
        value
            .report()
            .normalizations
            .iter()
            .filter(|notice| notice.path == "rPr/rtl")
            .count(),
        2
    );
    let mut session = DocumentSession::new(value.document().clone());
    let Block::Paragraph(first) = &session.document().sections()[0].blocks()[0] else {
        panic!("expected paragraph")
    };
    let id = first.id().clone();
    let mut transaction = session.begin_transaction();
    let at = transaction.position(&id, GraphemeOffset::new(0)).unwrap();
    transaction
        .insert_text(&at, "Revised — ", CharacterFormat::default())
        .unwrap();
    session.commit(transaction).unwrap();
    session.undo().unwrap();
    assert_eq!(session.document(), value.document());
    session.redo().unwrap();
    let edited = session.document().clone();
    let native = WriterFile::with_original_source(
        edited.clone(),
        OriginalSource::new("docx", DOCX_MEDIA_TYPE, "audit.docx", LIBREOFFICE.to_vec()).unwrap(),
    )
    .unwrap();
    let reopened = deserialize_writer_file(&serialize_writer_file(&native).unwrap()).unwrap();
    assert_eq!(reopened.document(), &edited);
    assert_eq!(reopened.original_source().unwrap().bytes(), LIBREOFFICE);
    let output = export_docx_with_source(reopened.document(), value.original()).unwrap();
    let copy = editable(&output);
    assert_eq!(paragraphs(copy.document()), paragraphs(&edited));
    assert_eq!(copy.document().styles(), edited.styles());
    assert_eq!(copy.document().metadata(), edited.metadata());
    for (before, after) in edited.sections().iter().zip(copy.document().sections()) {
        assert_eq!(before.properties(), after.properties());
        for (before, after) in before.blocks().iter().zip(after.blocks()) {
            let (Block::Paragraph(before), Block::Paragraph(after)) = (before, after) else {
                panic!("expected paragraphs")
            };
            assert_eq!(before.runs(), after.runs());
            assert_eq!(before.format(), after.format());
        }
    }
    assert_eq!(value.original().original_bytes(), LIBREOFFICE);
}
