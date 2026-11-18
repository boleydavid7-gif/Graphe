//! Approved formatting approximation never authorizes dropping document content.

use std::{
    collections::BTreeMap,
    io::{Cursor, Read, Write},
};

use office_core::{
    Block, CharacterFormat, Document, DocumentSession, GraphemeOffset, Id, OriginalSource,
    WriterFile, deserialize_writer_file, serialize_writer_file,
};
use office_docx::{
    CompatibilityIssueKind as Issue, ConversionError, DOCX_MEDIA_TYPE, DocxImportOutcome,
    ImportedDocx, export_docx, import_docx,
};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

const SAFE: &[u8] =
    include_bytes!("../../../tests/fixtures/docx/relationships/supported-auxiliary-parts.docx");
const PROFILE: &[u8] =
    include_bytes!("../../../tests/fixtures/docx/relationships/normalizable-wordweb-profile.docx");
const WORD_NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

fn imported(bytes: &[u8]) -> ImportedDocx {
    match import_docx(bytes, Id::new("normalized-import").unwrap()).expect("valid package") {
        DocxImportOutcome::Editable(value) => {
            assert!(value.report().is_supported());
            value.document().validate().unwrap();
            assert_eq!(value.original().original_bytes(), bytes);
            value
        }
        DocxImportOutcome::Preserved { report, .. } => panic!(
            "expected editable import; blockers={:?}; normalization counts={}/{} omitted",
            report.diagnostics,
            report.normalizations.len(),
            report.omitted_normalizations
        ),
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

fn parts(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
    (0..archive.len())
        .map(|index| {
            let mut part = archive.by_index(index).unwrap();
            let mut content = Vec::new();
            part.read_to_end(&mut content).unwrap();
            (part.name().unwrap().into_owned(), content)
        })
        .collect()
}

fn modified(bytes: &[u8], part: &str, update: impl FnOnce(String) -> String) -> Vec<u8> {
    let mut entries = parts(bytes);
    let xml = String::from_utf8(entries[part].clone()).unwrap();
    entries.insert(part.into(), update(xml).into_bytes());
    let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, content) in entries {
        archive
            .start_file(
                name,
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
        archive.write_all(&content).unwrap();
    }
    archive.finish().unwrap().into_inner()
}

fn source_paragraphs(bytes: &[u8]) -> Vec<String> {
    let entries = parts(bytes);
    let xml = std::str::from_utf8(&entries["word/document.xml"]).unwrap();
    let document = roxmltree::Document::parse(xml).unwrap();
    document
        .descendants()
        .filter(|node| node.has_tag_name((WORD_NS, "p")))
        .map(|paragraph| {
            paragraph
                .descendants()
                .filter(|node| node.has_tag_name((WORD_NS, "t")))
                .flat_map(|node| node.children().filter(roxmltree::Node::is_text))
                .filter_map(|node| node.text())
                .collect()
        })
        .collect()
}

fn assert_noneditable(bytes: &[u8]) {
    match import_docx(bytes, Id::new("must-remain-protected").unwrap()) {
        Ok(DocxImportOutcome::Preserved { original, report }) => {
            assert_eq!(original.original_bytes(), bytes);
            assert!(!report.is_supported());
        }
        Err(_) => {}
        Ok(outcome) => panic!("normalization accepted unsafe or malformed content: {outcome:?}"),
    }
}

#[test]
fn synthetic_wordweb_formatting_preserves_unicode_order_and_supported_mixed_runs() {
    let value = imported(PROFILE);
    assert!(!value.report().normalizations.is_empty());
    let expected: Vec<_> = (1..=9)
        .map(|index| {
            if index == 1 {
                "A synthetic writing sample.".to_owned()
            } else {
                format!("Paragraph {index} — Café e\u{301} 👩🏽‍💻")
            }
        })
        .collect();
    assert_eq!(paragraphs(value.document()), expected);
    assert_eq!(paragraphs(value.document()), source_paragraphs(PROFILE));
    let blocks = value.document().sections()[0].blocks();
    assert_eq!(blocks.len(), 9);
    assert_eq!(
        blocks
            .iter()
            .map(|block| {
                let Block::Paragraph(p) = block else {
                    panic!("expected paragraph")
                };
                p.runs().len()
            })
            .sum::<usize>(),
        17
    );
    let Block::Paragraph(first) = &blocks[0] else {
        panic!("expected paragraph")
    };
    assert_eq!(first.runs()[0].format.bold, Some(true));
    assert_eq!(first.runs()[0].format.font_family.as_deref(), Some("Arimo"));
    assert_eq!(first.runs()[0].format.font_size.unwrap().0, 24);
    let Block::Paragraph(second) = &blocks[1] else {
        panic!("expected paragraph")
    };
    assert_eq!(second.runs()[1].format.italic, Some(true));
}

#[test]
fn real_word_and_libreoffice_imports_preserve_each_original_paragraph_without_editing_the_fixture()
{
    for bytes in [
        include_bytes!("../../../tests/fixtures/docx/relationships/microsoft-sdk-plain.docx")
            .as_slice(),
        include_bytes!("../../../tests/fixtures/docx/relationships/libreoffice-basic.docx")
            .as_slice(),
        include_bytes!("../../../tests/fixtures/docx/relationships/microsoft-sdk-hello-world.docx")
            .as_slice(),
    ] {
        let value = imported(bytes);
        assert!(!value.report().normalizations.is_empty());
        assert_eq!(paragraphs(value.document()), source_paragraphs(bytes));
        let file = WriterFile::with_original_source(
            value.document().clone(),
            OriginalSource::new("docx", DOCX_MEDIA_TYPE, "producer.docx", bytes.to_vec()).unwrap(),
        )
        .unwrap();
        let native = serialize_writer_file(&file).unwrap();
        let reopened = deserialize_writer_file(&native).unwrap();
        assert_eq!(reopened.document(), value.document());
        assert_eq!(reopened.original_source().unwrap().bytes(), bytes);
        let output = export_docx(reopened.document()).unwrap();
        let copy = imported(&output);
        assert_eq!(paragraphs(copy.document()), paragraphs(value.document()));
        assert_eq!(copy.document().metadata(), value.document().metadata());
    }
}

#[test]
fn normalized_documents_still_use_atomic_edit_history_and_lossless_native_reopening() {
    let value = imported(PROFILE);
    let original_document = value.document().clone();
    let Block::Paragraph(first) = &original_document.sections()[0].blocks()[0] else {
        panic!("expected paragraph")
    };
    let first_id = first.id().clone();
    let mut session = DocumentSession::new(original_document.clone());
    let mut transaction = session.begin_transaction();
    let at = transaction
        .position(&first_id, GraphemeOffset::new(0))
        .unwrap();
    transaction
        .insert_text(&at, "Revised e\u{301} 👩🏽‍💻 — ", CharacterFormat::default())
        .unwrap();
    session.commit(transaction).unwrap();
    let edited = session.document().clone();
    session.undo().unwrap();
    assert_eq!(session.document(), &original_document);
    session.redo().unwrap();
    assert_eq!(session.document(), &edited);
    let file = WriterFile::with_original_source(
        edited.clone(),
        OriginalSource::new("docx", DOCX_MEDIA_TYPE, "profile.docx", PROFILE.to_vec()).unwrap(),
    )
    .unwrap();
    let bytes = serialize_writer_file(&file).unwrap();
    let reopened = deserialize_writer_file(&bytes).unwrap();
    assert_eq!(reopened.document(), &edited);
    assert_eq!(reopened.original_source().unwrap().bytes(), PROFILE);
    let output = export_docx(reopened.document()).unwrap();
    assert_eq!(
        paragraphs(imported(&output).document()),
        paragraphs(&edited)
    );
}

#[test]
fn style_behavior_metadata_can_be_normalized_without_becoming_a_second_style_engine() {
    let bytes = modified(SAFE, "word/styles.xml", |xml| {
        xml.replace("<w:rPr>",
            "<w:qFormat/><w:uiPriority w:val=\"9\"/><w:semiHidden/><w:unhideWhenUsed/><w:rsid w:val=\"00000001\"/><w:next w:val=\"FixtureNormal\"/><w:rPr>")
            .replace("<w:style ", "<w:latentStyles w:defLockedState=\"0\" w:defUIPriority=\"99\" w:defSemiHidden=\"1\" w:defUnhideWhenUsed=\"1\" w:defQFormat=\"0\" w:count=\"1\"><w:lsdException w:name=\"Heading 1\" w:semiHidden=\"0\" w:uiPriority=\"9\" w:unhideWhenUsed=\"0\" w:qFormat=\"1\"/></w:latentStyles><w:style ")
    });
    let value = imported(&bytes);
    assert_eq!(
        paragraphs(value.document()),
        paragraphs(imported(SAFE).document())
    );
    assert!(
        value
            .report()
            .normalizations
            .iter()
            .any(|notice| notice.part == "word/styles.xml")
    );
}

#[test]
fn duplicate_style_ids_unresolved_parents_and_inheritance_cycles_are_not_normalized() {
    for bytes in [
        modified(SAFE, "word/styles.xml", |xml| {
            xml.replace("</w:styles>", "<w:style w:type=\"paragraph\" w:styleId=\"FixtureNormal\"><w:name w:val=\"Duplicate\"/></w:style></w:styles>")
        }),
        modified(SAFE, "word/styles.xml", |xml| {
            xml.replace("<w:rPr>", "<w:basedOn w:val=\"missing-parent\"/><w:rPr>")
        }),
        modified(SAFE, "word/styles.xml", |xml| {
            xml.replace("<w:rPr>", "<w:basedOn w:val=\"FixtureNormal\"/><w:rPr>")
        }),
    ] {
        assert!(matches!(
            import_docx(&bytes, Id::new("invalid-styles").unwrap()),
            Err(ConversionError::InvalidDocument { .. })
        ));
    }
}

fn unused_numbering_styles() -> Vec<u8> {
    modified(SAFE, "word/styles.xml", |xml| {
        xml.replace("</w:styles>",
            "<w:style w:type=\"paragraph\" w:styleId=\"ListUnused\"><w:name w:val=\"Unused list\"/><w:pPr><w:numPr><w:numId w:val=\"42\"/></w:numPr></w:pPr></w:style><w:style w:type=\"paragraph\" w:styleId=\"ListChildUnused\"><w:name w:val=\"Unused list child\"/><w:basedOn w:val=\"ListUnused\"/></w:style></w:styles>")
    })
}

#[test]
fn unused_numbering_paragraph_styles_and_their_unused_descendants_stay_only_in_the_original() {
    let bytes = unused_numbering_styles();
    let value = imported(&bytes);
    assert_eq!(
        paragraphs(value.document()),
        paragraphs(imported(SAFE).document())
    );
    assert!(
        value
            .document()
            .styles()
            .iter()
            .all(|style| { style.name != "Unused list" && style.name != "Unused list child" })
    );
    assert!(
        value
            .report()
            .normalizations
            .iter()
            .any(|notice| { notice.part == "word/styles.xml" && notice.path.contains("unused") })
    );
}

#[test]
fn numbering_styles_that_are_default_referenced_or_inherited_by_used_styles_are_not_pruned() {
    let library = unused_numbering_styles();
    for id in ["ListUnused", "ListChildUnused"] {
        let used = modified(&library, "word/document.xml", |xml| {
            xml.replace(
                "<w:p>",
                &format!("<w:p><w:pPr><w:pStyle w:val=\"{id}\"/></w:pPr>"),
            )
        });
        assert_noneditable(&used);
    }
    let default = modified(&library, "word/styles.xml", |xml| {
        xml.replace(
            "w:styleId=\"FixtureNormal\" w:default=\"1\"",
            "w:styleId=\"FixtureNormal\"",
        )
        .replace(
            "w:styleId=\"ListUnused\"",
            "w:styleId=\"ListUnused\" w:default=\"1\"",
        )
    });
    assert_noneditable(&default);
}

fn character_style(properties: &str) -> Vec<u8> {
    modified(SAFE, "word/styles.xml", |xml| {
        xml.replace("</w:styles>", &format!(
            "<w:style w:type=\"character\" w:styleId=\"Emphasis\"><w:name w:val=\"Emphasis\"/><w:rPr>{properties}</w:rPr></w:style></w:styles>"
        ))
    })
}

#[test]
fn supported_character_styles_are_materialized_with_direct_formatting_precedence() {
    let styled = character_style("<w:b/><w:bCs/><w:i/><w:iCs/>");
    let bytes = modified(&styled, "word/document.xml", |xml| {
        xml.replace("<w:r>", "<w:r><w:rPr><w:rStyle w:val=\"Emphasis\"/><w:b w:val=\"0\"/><w:bCs w:val=\"0\"/></w:rPr>")
    });
    let value = imported(&bytes);
    let Block::Paragraph(paragraph) = &value.document().sections()[0].blocks()[0] else {
        panic!("expected paragraph")
    };
    assert_eq!(paragraph.runs()[0].format.bold, Some(false));
    assert_eq!(paragraph.runs()[0].format.italic, Some(true));
    assert_eq!(
        paragraphs(value.document()),
        paragraphs(imported(SAFE).document())
    );
    assert!(
        value
            .report()
            .normalizations
            .iter()
            .any(|notice| { notice.part == "word/styles.xml" || notice.path.contains("rStyle") })
    );
    let output = export_docx(value.document()).unwrap();
    let reopened = imported(&output);
    let Block::Paragraph(paragraph) = &reopened.document().sections()[0].blocks()[0] else {
        panic!("expected paragraph")
    };
    assert_eq!(paragraph.runs()[0].format.bold, Some(false));
    assert_eq!(paragraph.runs()[0].format.italic, Some(true));
}

#[test]
fn character_style_toggle_parity_combines_with_the_effective_paragraph_style() {
    for (properties, parent_toggle, expected_run) in [
        ("<w:b/><w:bCs/>", false, Some(false)),
        ("<w:b w:val=\"0\"/><w:bCs w:val=\"0\"/>", false, None),
        ("<w:b/><w:bCs/>", true, None),
    ] {
        let library = character_style(properties);
        let paragraph_bold = modified(&library, "word/styles.xml", |xml| {
            let xml = xml.replacen("<w:rPr>", "<w:rPr><w:b/><w:bCs/>", 1);
            if parent_toggle {
                xml.replace("<w:name w:val=\"Emphasis\"/>",
                    "<w:name w:val=\"Emphasis\"/><w:basedOn w:val=\"EmphasisParent\"/>")
                    .replace("</w:styles>", "<w:style w:type=\"character\" w:styleId=\"EmphasisParent\"><w:name w:val=\"Emphasis parent\"/><w:rPr><w:b/><w:bCs/></w:rPr></w:style></w:styles>")
            } else {
                xml
            }
        });
        let bytes = modified(&paragraph_bold, "word/document.xml", |xml| {
            xml.replace(
                "<w:r>",
                "<w:r><w:rPr><w:rStyle w:val=\"Emphasis\"/></w:rPr>",
            )
        });
        let value = imported(&bytes);
        let Block::Paragraph(paragraph) = &value.document().sections()[0].blocks()[0] else {
            panic!("expected paragraph")
        };
        let base = value
            .document()
            .styles()
            .iter()
            .find(|style| Some(&style.id) == paragraph.style())
            .unwrap();
        assert_eq!(base.character_format.bold, Some(true));
        assert_eq!(paragraph.runs()[0].format.bold, expected_run);
    }
}

#[test]
fn character_style_normalization_never_unhides_text_or_accepts_unknown_properties() {
    for properties in ["<w:vanish/>", "<w:unrecognizedCharacterProperty/>"] {
        let styled = character_style(properties);
        let bytes = modified(&styled, "word/document.xml", |xml| {
            xml.replace(
                "<w:r>",
                "<w:r><w:rPr><w:rStyle w:val=\"Emphasis\"/></w:rPr>",
            )
        });
        assert_noneditable(&bytes);
    }
}

#[test]
fn missing_character_style_references_and_character_inheritance_cycles_are_errors() {
    let bytes = modified(SAFE, "word/document.xml", |xml| {
        xml.replace(
            "<w:r>",
            "<w:r><w:rPr><w:rStyle w:val=\"missing-character-style\"/></w:rPr>",
        )
    });
    assert!(matches!(
        import_docx(&bytes, Id::new("missing-character-style").unwrap()),
        Err(ConversionError::InvalidDocument { .. })
    ));
    let cyclic = modified(
        &character_style("<w:i/><w:iCs/>"),
        "word/styles.xml",
        |xml| {
            xml.replace(
                "<w:name w:val=\"Emphasis\"/>",
                "<w:name w:val=\"Emphasis\"/><w:basedOn w:val=\"Emphasis\"/>",
            )
        },
    );
    assert!(matches!(
        import_docx(&cyclic, Id::new("character-style-cycle").unwrap()),
        Err(ConversionError::InvalidDocument { .. })
    ));
}

fn warning_flood() -> Vec<u8> {
    modified(SAFE, "word/document.xml", |_| {
        format!(
            "<w:document xmlns:w=\"{WORD_NS}\"><w:body>{}</w:body></w:document>",
            (0..160)
                .map(|index| format!(
                    "<w:p><w:r><w:rPr><w:lang w:val=\"en-US\"/></w:rPr><w:t>Paragraph {index}</w:t></w:r></w:p>"
                ))
                .collect::<String>()
        )
    })
}

#[test]
fn normalization_notice_limits_do_not_change_a_supported_import_into_read_only() {
    let bytes = warning_flood();
    let value = imported(&bytes);
    assert_eq!(value.report().normalizations.len(), 128);
    assert!(value.report().omitted_normalizations >= 32);
    assert!(value.report().diagnostics.is_empty());
    assert_eq!(paragraphs(value.document()).len(), 160);
}

#[test]
fn normalization_notice_flood_never_hides_a_later_actual_revision() {
    let bytes = modified(&warning_flood(), "word/document.xml", |xml| {
        xml.replace("</w:body>", "<w:p><w:ins w:id=\"1\" w:author=\"Synthetic\"><w:r><w:t>Tracked text</w:t></w:r></w:ins></w:p></w:body>")
    });
    match import_docx(&bytes, Id::new("flood-plus-revision").unwrap()).unwrap() {
        DocxImportOutcome::Preserved { original, report } => {
            assert_eq!(original.original_bytes(), bytes);
            assert_eq!(report.normalizations.len(), 128);
            assert!(report.omitted_normalizations >= 32);
            assert!(report.diagnostics.iter().any(|issue| {
                issue.kind == Issue::UnsupportedElement && issue.path.contains("ins")
            }));
        }
        outcome => panic!("warning budget must never suppress revision integrity: {outcome:?}"),
    }
}

#[test]
fn normalization_notice_flood_never_disables_editing_protection() {
    let bytes = modified(&warning_flood(), "word/settings.xml", |xml| {
        xml.replace(
            "</w:settings>",
            "<w:documentProtection w:edit=\"readOnly\" w:enforcement=\"1\"/></w:settings>",
        )
    });
    match import_docx(&bytes, Id::new("flood-plus-protection").unwrap()).unwrap() {
        DocxImportOutcome::Preserved { original, report } => {
            assert_eq!(original.original_bytes(), bytes);
            assert_eq!(report.normalizations.len(), 128);
            assert!(
                report
                    .diagnostics
                    .iter()
                    .any(|issue| { issue.path.contains("documentProtection") })
            );
        }
        outcome => panic!("formatting policy never disables protection: {outcome:?}"),
    }
}

#[test]
fn style_normalization_does_not_accept_spoofed_namespaces_unknown_content_or_nested_markup() {
    for child in [
        "<x:qFormat xmlns:x=\"urn:unknown-style-properties\"/>",
        "<w:qFormat><w:unexpectedContent/></w:qFormat>",
        "<w:unrecognizedStyleProperty/>",
    ] {
        let bytes = modified(SAFE, "word/styles.xml", |xml| {
            xml.replace("<w:rPr>", &format!("{child}<w:rPr>"))
        });
        assert_noneditable(&bytes);
    }
}
