//! ODF underline interop: inactive style must not carry an active line type.

use std::io::{Cursor, Read};

use office_core::{
    Block, CharacterFormat, Document, DocumentMetadata, Id, NamedStyle, Paragraph, Section,
    TextRun, Underline, WriterFile, deserialize_writer_file, serialize_writer_file,
};
use office_formats::{DocumentFormat, ImportOutcome, export_file, import_document};

const STYLE: &str = "urn:oasis:names:tc:opendocument:xmlns:style:1.0";

fn id(value: &str) -> Id {
    Id::new(value).unwrap()
}

fn document() -> Document {
    let mut parent = NamedStyle::new(id("parent"), "Underlined parent");
    parent.character_format.underline = Some(Underline::Double);
    let mut clear = NamedStyle::new(id("clear"), "Cleared child");
    clear.based_on = Some(parent.id.clone());
    clear.character_format.underline = Some(Underline::None);
    let run = |text: &str, underline| TextRun {
        text: text.into(),
        format: CharacterFormat {
            underline,
            ..Default::default()
        },
    };
    Document::from_parts(
        id("underline-document"),
        DocumentMetadata::default(),
        vec![Section::new(
            id("section"),
            vec![
                Block::Paragraph(
                    Paragraph::new(
                        id("inherits-double"),
                        vec![
                            run("Inherited double", None),
                            run("explicit clear", Some(Underline::None)),
                        ],
                    )
                    .with_style(parent.id.clone()),
                ),
                Block::Paragraph(
                    Paragraph::new(
                        id("clears-inheritance"),
                        vec![
                            run("Clear Café e\u{301} 👩🏽‍💻", None),
                            run("single", Some(Underline::Single)),
                            run("double", Some(Underline::Double)),
                            run("clear", Some(Underline::None)),
                        ],
                    )
                    .with_style(clear.id.clone()),
                ),
                Block::Paragraph(Paragraph::new(
                    id("default"),
                    vec![TextRun::new("Default clear")],
                )),
            ],
        )],
        vec![parent, clear],
    )
    .unwrap()
}

fn xml_parts(bytes: &[u8]) -> Vec<String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    ["styles.xml", "content.xml"]
        .into_iter()
        .map(|name| {
            let mut xml = String::new();
            archive
                .by_name(name)
                .unwrap()
                .read_to_string(&mut xml)
                .unwrap();
            xml
        })
        .collect()
}

fn assert_standard_underline_properties(bytes: &[u8]) {
    let mut none = 0;
    let mut single = 0;
    let mut double = 0;
    for part in xml_parts(bytes) {
        let xml = roxmltree::Document::parse(&part).unwrap();
        for node in xml
            .descendants()
            .filter(|node| node.has_tag_name((STYLE, "text-properties")))
        {
            match node.attribute((STYLE, "text-underline-style")) {
                Some("none") => {
                    // ODF 1.3 Part 3 §20.391: no underline-type when style is
                    // none. LibreOffice otherwise imported the old single type
                    // as an active underline, despite Writer's self-roundtrip.
                    assert_eq!(node.attribute((STYLE, "text-underline-type")), None);
                    none += 1;
                }
                Some("solid") => match node.attribute((STYLE, "text-underline-type")) {
                    Some("single") => single += 1,
                    Some("double") => double += 1,
                    other => panic!("Invalid active underline type: {other:?}"),
                },
                None => assert_eq!(node.attribute((STYLE, "text-underline-type")), None),
                other => panic!("Unexpected underline style: {other:?}"),
            }
        }
    }
    assert!(
        none >= 3,
        "default, inherited clear and direct clear are encoded"
    );
    assert!(single > 0);
    assert!(
        double >= 2,
        "named and direct double underlines are retained"
    );
}

fn open(bytes: &[u8]) -> WriterFile {
    match import_document(DocumentFormat::Odt, bytes, id("imported"), "underline.odt").unwrap() {
        ImportOutcome::Editable(file) => file,
        other => panic!("Expected editable supported ODT: {other:?}"),
    }
}

fn effective_runs(document: &Document) -> Vec<Vec<(String, Underline)>> {
    document
        .paragraphs()
        .map(|paragraph| {
            let inherited = office_layout::resolve_paragraph_style(document, paragraph).character;
            paragraph
                .runs()
                .iter()
                .filter(|run| !run.text.is_empty())
                .map(|run| {
                    let mut format = inherited.clone();
                    office_layout::overlay_character(&mut format, &run.format);
                    (
                        run.text.clone(),
                        format.underline.unwrap_or(Underline::None),
                    )
                })
                .collect()
        })
        .collect()
}

#[test]
fn inactive_underline_omits_type_in_default_named_and_direct_properties() {
    let bytes = export_file(DocumentFormat::Odt, &WriterFile::new(document())).unwrap();
    assert_standard_underline_properties(&bytes);
}

#[test]
fn inherited_and_direct_underlines_survive_native_recovery_and_odt_regeneration() {
    let document = document();
    let expected = effective_runs(&document);
    let bytes = export_file(DocumentFormat::Odt, &WriterFile::new(document)).unwrap();
    let file = open(&bytes);
    assert_eq!(file.original_source().unwrap().bytes(), bytes);
    assert_eq!(effective_runs(file.document()), expected);
    let native = serialize_writer_file(&file).unwrap();
    let reopened = deserialize_writer_file(&native).unwrap();
    assert_eq!(reopened, file);
    let regenerated = export_file(DocumentFormat::Odt, &reopened).unwrap();
    assert_standard_underline_properties(&regenerated);
    assert_eq!(effective_runs(open(&regenerated).document()), expected);
    assert_eq!(serialize_writer_file(&file).unwrap(), native);
}
