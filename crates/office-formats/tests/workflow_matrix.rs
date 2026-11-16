//! End-to-end native-save and source-format-reopen audit across all editable adapters.

use office_core::{
    Alignment, Block, CharacterFormat, Document, DocumentMetadata, DocumentSession, GraphemeOffset,
    HalfPoints, Id, NamedStyle, Paragraph, ParagraphFormat, Section, TextColor, TextCoordinates,
    TextRun, Underline, WriterFile,
};
use office_formats::{DocumentFormat, ImportOutcome, assess_file, export_file, import_document};
use office_layout::{LayoutOptions, layout_document, overlay_character, resolve_paragraph_style};

fn id(value: impl Into<String>) -> Id {
    Id::new(value).unwrap()
}

fn sample() -> Document {
    let mut heading = NamedStyle::new(id("heading"), "Review heading");
    heading.character_format = CharacterFormat {
        font_family: Some("Tinos".into()),
        font_size: Some(HalfPoints(36)),
        bold: Some(true),
        ..Default::default()
    };
    heading.paragraph_format.alignment = Some(Alignment::Center);
    let mut blocks = vec![Block::Paragraph(
        Paragraph::new(
            id("title"),
            vec![TextRun::new("Compatibility review — Café")],
        )
        .with_style(id("heading")),
    )];
    for index in 0..84 {
        blocks.push(Block::Paragraph(
            Paragraph::new(
                id(format!("paragraph-{index}")),
                vec![
                    TextRun::new(format!("Review item {index}: preserve text and ")),
                    TextRun {
                        text: "mixed formatting".into(),
                        format: CharacterFormat {
                            bold: Some(true),
                            italic: Some(true),
                            underline: Some(Underline::Single),
                            font_family: Some("Arimo".into()),
                            font_size: Some(HalfPoints(26)),
                            color: Some(TextColor::Rgb {
                                red: 40,
                                green: 70,
                                blue: 110,
                            }),
                            hyperlink: None,
                        },
                    },
                    TextRun::new(". Unicode e\u{301} 👩🏽‍💻 العربية नमस्ते שלום."),
                ],
            )
            .with_format(ParagraphFormat {
                alignment: Some(Alignment::Left),
                ..Default::default()
            }),
        ));
    }
    Document::from_parts(
        id("compatibility-audit"),
        DocumentMetadata {
            title: "Compatibility review".into(),
        },
        vec![Section::new(id("section"), blocks)],
        vec![heading],
    )
    .unwrap()
}

fn open(format: DocumentFormat, bytes: &[u8]) -> WriterFile {
    match import_document(format, bytes, id("reopened"), "audit-document").unwrap() {
        ImportOutcome::Editable(file) => file,
        ImportOutcome::Preserved { report, .. } => panic!("{format:?} cannot reopen: {report:?}"),
    }
}

fn paragraphs(document: &Document) -> Vec<&Paragraph> {
    document
        .sections()
        .iter()
        .flat_map(Section::blocks)
        .map(|block| {
            let Block::Paragraph(paragraph) = block;
            paragraph
        })
        .collect()
}

fn text(document: &Document) -> Vec<String> {
    paragraphs(document)
        .into_iter()
        .map(|paragraph| {
            paragraph
                .runs()
                .iter()
                .map(|run| run.text.as_str())
                .collect()
        })
        .collect()
}

// External formats may flatten inheritance or repartition runs. Compare effective
// supported character properties per scalar without assuming glyph/cursor offsets.
fn character_projection(document: &Document) -> Vec<Vec<(char, CharacterFormat)>> {
    paragraphs(document)
        .into_iter()
        .map(|paragraph| {
            let inherited = resolve_paragraph_style(document, paragraph).character;
            paragraph
                .runs()
                .iter()
                .flat_map(|run| {
                    let mut format = inherited.clone();
                    overlay_character(&mut format, &run.format);
                    format.bold = Some(format.bold.unwrap_or(false));
                    format.italic = Some(format.italic.unwrap_or(false));
                    format.underline = Some(format.underline.unwrap_or(Underline::None));
                    format.font_family.get_or_insert_with(|| "Arimo".into());
                    format.font_size.get_or_insert(HalfPoints(24));
                    format.color.get_or_insert(TextColor::Auto {});
                    run.text
                        .chars()
                        .map(move |character| (character, format.clone()))
                })
                .collect()
        })
        .collect()
}

#[test]
fn each_editable_format_survives_import_edit_native_save_reload_export_and_reopen() {
    let document = sample();
    assert!(
        layout_document(&document, &LayoutOptions::default())
            .unwrap()
            .pages
            .len()
            >= 4
    );
    for format in [
        DocumentFormat::Writer,
        DocumentFormat::Docx,
        DocumentFormat::Odt,
        DocumentFormat::Rtf,
        DocumentFormat::Txt,
    ] {
        let initial = WriterFile::new(document.clone());
        let bytes = export_file(format, &initial).unwrap();
        let imported = open(format, &bytes);
        assert_eq!(text(imported.document()), text(&document), "{format:?}");
        if format == DocumentFormat::Txt {
            assert!(
                assess_file(format, &initial)
                    .unwrap()
                    .warnings
                    .iter()
                    .any(|warning| warning.code == "plain_text_only")
            );
        } else {
            assert_eq!(
                character_projection(imported.document()),
                character_projection(&document),
                "{format:?} initial supported formatting"
            );
        }
        let (model, original) = imported.into_parts();
        let last = paragraphs(&model).last().unwrap().id().clone();
        let end = TextCoordinates::new(paragraphs(&model).last().unwrap()).grapheme_len();
        let mut session = DocumentSession::new(model);
        let mut transaction = session.begin_transaction();
        let at = transaction
            .position(&last, GraphemeOffset::new(end.get()))
            .unwrap();
        transaction
            .insert_text(&at, " Edited e\u{301} 🦀.", CharacterFormat::default())
            .unwrap();
        session.commit(transaction).unwrap();
        let edited = match original {
            Some(source) => {
                WriterFile::with_original_source(session.document().clone(), source).unwrap()
            }
            None => WriterFile::new(session.document().clone()),
        };
        let native_bytes = office_core::serialize_writer_file(&edited).unwrap();
        let loaded = office_core::deserialize_writer_file(&native_bytes).unwrap();
        assert_eq!(
            loaded, edited,
            "{format:?} native persistence must retain identifiers and sources"
        );
        assert_eq!(
            office_core::serialize_writer_file(&loaded).unwrap(),
            native_bytes,
            "{format:?} deterministic native reopening"
        );
        let before = loaded.clone();
        let output = export_file(format, &loaded).unwrap();
        let reopened = open(format, &output);
        assert_eq!(
            text(reopened.document()),
            text(loaded.document()),
            "{format:?}"
        );
        if format != DocumentFormat::Txt {
            assert_eq!(
                character_projection(reopened.document()),
                character_projection(loaded.document()),
                "{format:?} supported formatting after editing and native reload"
            );
            let alignments = |document: &Document| {
                paragraphs(document)
                    .into_iter()
                    .map(|paragraph| {
                        resolve_paragraph_style(document, paragraph)
                            .paragraph
                            .alignment
                            .unwrap_or(Alignment::Left)
                    })
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                alignments(reopened.document()),
                alignments(loaded.document())
            );
        }
        assert_eq!(
            loaded, before,
            "{format:?} copying must not mutate native content or original"
        );
        assert!(session.can_undo());
        session.undo().unwrap();
        assert_eq!(
            text(session.document()),
            text(open(format, &bytes).document())
        );
    }
}
