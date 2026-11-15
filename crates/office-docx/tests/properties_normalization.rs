//! Explicit formatting approximations must not discard text or weaken guards.

use std::{
    collections::BTreeMap,
    io::{Cursor, Read, Write},
};

use office_core::{Block, HalfPoints, Id, TextColor, Twips};
use office_docx::{DocxImportOutcome, ImportedDocx, export_docx, import_docx};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

const BASIC: &[u8] = include_bytes!("../../../tests/fixtures/docx/supported-basic.docx");
const TEXT: &str = "DOCX hello — café 👩🏽‍💻";

fn package(bytes: &[u8], update: impl FnOnce(&mut BTreeMap<String, Vec<u8>>)) -> Vec<u8> {
    let mut input = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut parts = BTreeMap::new();
    for index in 0..input.len() {
        let mut file = input.by_index(index).unwrap();
        let name = file.name().unwrap().to_string();
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        parts.insert(name, bytes);
    }
    update(&mut parts);
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in parts {
        writer
            .start_file(name, SimpleFileOptions::default())
            .unwrap();
        writer.write_all(&bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn source(run: &str, paragraph: &str, section: &str) -> Vec<u8> {
    package(BASIC, |parts| {
        let document = String::from_utf8(parts["document.xml"].clone())
            .unwrap()
            .replace("<w:r>", &format!("<w:r><w:rPr>{run}</w:rPr>"))
            .replace("<w:p>", &format!("<w:p><w:pPr>{paragraph}</w:pPr>"))
            .replace(
                "</w:body>",
                &format!("<w:sectPr>{section}</w:sectPr></w:body>"),
            );
        parts.insert("document.xml".into(), document.into_bytes());
    })
}

fn editable(bytes: &[u8]) -> ImportedDocx {
    match import_docx(bytes, Id::new("format-normalization").unwrap()).unwrap() {
        DocxImportOutcome::Editable(imported) => {
            assert_eq!(imported.original().original_bytes(), bytes);
            imported.document().validate().unwrap();
            let Block::Paragraph(paragraph) = &imported.document().sections()[0].blocks()[0];
            assert_eq!(
                paragraph
                    .runs()
                    .iter()
                    .map(|run| run.text.as_str())
                    .collect::<String>(),
                TEXT
            );
            imported
        }
        DocxImportOutcome::Preserved { report, .. } => {
            panic!("safe formatting became read-only: {report:?}")
        }
    }
}

#[test]
fn partial_and_different_script_fonts_have_a_deterministic_documented_choice() {
    for (property, expected) in [
        (
            r#"<w:rFonts w:ascii="Arimo" w:hAnsi="Tinos" w:cs="Noto Sans"/>"#,
            Some("Arimo"),
        ),
        (
            r#"<w:rFonts w:hAnsi="Tinos" w:eastAsia="Noto Serif"/>"#,
            Some("Tinos"),
        ),
        (r#"<w:rFonts w:cs="Noto Sans"/>"#, Some("Noto Sans")),
        (
            r#"<w:rFonts w:asciiTheme="minorHAnsi" w:cstheme="minorBidi"/>"#,
            None,
        ),
    ] {
        let bytes = source(property, "", "");
        let imported = editable(&bytes);
        let Block::Paragraph(paragraph) = &imported.document().sections()[0].blocks()[0];
        assert_eq!(paragraph.runs()[0].format.font_family.as_deref(), expected);
        assert!(
            imported
                .report()
                .normalizations
                .iter()
                .any(|notice| notice.path == "rFonts")
        );
    }
}

#[test]
fn complex_script_values_fall_back_only_when_the_ordinary_value_is_absent() {
    let bytes = source(
        r#"<w:b w:val="0"/><w:bCs/><w:iCs/><w:sz w:val="22"/><w:szCs w:val="30"/>"#,
        "",
        "",
    );
    let imported = editable(&bytes);
    let Block::Paragraph(paragraph) = &imported.document().sections()[0].blocks()[0];
    let format = &paragraph.runs()[0].format;
    assert_eq!(format.bold, Some(false));
    assert_eq!(format.italic, Some(true));
    assert_eq!(format.font_size, Some(HalfPoints(22)));
    assert_eq!(imported.report().normalizations.len(), 3);
    let current_copy = export_docx(imported.document()).unwrap();
    let reopened = editable(&current_copy);
    let Block::Paragraph(paragraph) = &reopened.document().sections()[0].blocks()[0];
    assert_eq!(&paragraph.runs()[0].format, format);
}

#[test]
fn cached_theme_colors_are_kept_and_theme_only_colors_use_inheritance_with_notice() {
    let bytes = source(
        r#"<w:color w:val="1234AB" w:themeColor="accent1" w:themeTint="FF" w:themeShade="BF"/>"#,
        "",
        "",
    );
    let imported = editable(&bytes);
    let Block::Paragraph(paragraph) = &imported.document().sections()[0].blocks()[0];
    assert_eq!(
        paragraph.runs()[0].format.color,
        Some(TextColor::Rgb {
            red: 0x12,
            green: 0x34,
            blue: 0xab
        })
    );
    assert!(
        imported
            .report()
            .normalizations
            .iter()
            .any(|notice| notice.path == "color")
    );
    let bytes = source(r#"<w:color w:themeColor="text1"/>"#, "", "");
    let imported = editable(&bytes);
    let Block::Paragraph(paragraph) = &imported.document().sections()[0].blocks()[0];
    assert_eq!(paragraph.runs()[0].format.color, None);
    assert!(!imported.report().normalizations.is_empty());
}

#[test]
fn ordinary_word_and_libreoffice_paragraph_preferences_keep_explicit_geometry() {
    let bytes = source(
        r#"<w:caps w:val="0"/><w:smallCaps w:val="false"/><w:noProof w:val="0"/><w:kern w:val="2"/><w:lang w:val="en-US" w:eastAsia="zh-CN"/>"#,
        r#"<w:keepNext/><w:widowControl w:val="false"/><w:bidi w:val="0"/><w:rPr/><w:spacing w:before="240" w:after="120" w:beforeAutospacing="off" w:afterAutospacing="0"/>"#,
        r#"<w:pgSz w:w="12240" w:h="15840"/><w:cols w:space="720"/><w:docGrid w:type="default" w:linePitch="100" w:charSpace="4294963199"/><w:pgNumType w:fmt="decimal"/><w:formProt w:val="false"/><w:textDirection w:val="lrTb"/>"#,
    );
    let imported = editable(&bytes);
    let section = &imported.document().sections()[0];
    assert_eq!(
        section.properties().unwrap().page_size.unwrap().width,
        Twips(12240)
    );
    let Block::Paragraph(paragraph) = &section.blocks()[0];
    assert_eq!(paragraph.format().spacing_before, Some(Twips(240)));
    assert_eq!(paragraph.format().spacing_after, Some(Twips(120)));
    assert!(imported.report().normalizations.len() >= 5);
}

#[test]
fn normalization_does_not_hide_unknown_attributes_nested_text_or_protection() {
    for (run, paragraph, section) in [
        (r#"<w:kern w:val="2" w:future="1"/>"#, "", ""),
        (
            r#"<w:kern w:val="2"><w:t>Do not discard</w:t></w:kern>"#,
            "",
            "",
        ),
        (
            r#"<w:color w:val="000000" w:unknownTheme="accent1"/>"#,
            "",
            "",
        ),
        (r#"<w:vanish/>"#, "", ""),
        (r#"<w:webHidden/>"#, "", ""),
        (r#"<w:kern w:val="2"/><w:kern w:val="3"/>"#, "", ""),
        ("", r#"<w:numPr><w:numId w:val="1"/></w:numPr>"#, ""),
        ("", r#"<w:bidi/>"#, ""),
        ("", "", r#"<w:formProt/>"#),
        (
            "",
            "",
            r#"<w:cols w:num="2"><w:col w:w="100"><w:t>Do not discard</w:t></w:col></w:cols>"#,
        ),
        ("", "", r#"<w:textDirection w:val="tbRl" w:future="1"/>"#),
        ("", "", r#"<w:docGrid w:type="default" w:future="1"/>"#),
    ] {
        let bytes = source(run, paragraph, section);
        match import_docx(&bytes, Id::new("unsafe-format").unwrap()).unwrap() {
            DocxImportOutcome::Preserved { original, report } => {
                assert_eq!(original.original_bytes(), bytes);
                assert!(!report.is_supported());
            }
            other => {
                panic!("unsafe property became editable: {run} {paragraph} {section}: {other:?}")
            }
        }
    }
}

#[test]
fn malformed_known_normalization_values_are_explicit_errors() {
    for (run, paragraph, section) in [
        (r#"<w:kern w:val="not-a-number"/>"#, "", ""),
        (r#"<w:caps w:val="yes"/>"#, "", ""),
        (r#"<w:rFonts w:ascii="  "/>"#, "", ""),
        (r#"<w:rFonts w:asciiTheme="unknown"/>"#, "", ""),
        (r#"<w:color w:val="000000" w:themeTint="100"/>"#, "", ""),
        ("", r#"<w:spacing w:beforeAutospacing="no"/>"#, ""),
        ("", r#"<w:outlineLvl w:val="10"/>"#, ""),
        ("", "", r#"<w:cols w:num="0"/>"#),
        ("", "", r#"<w:docGrid w:charSpace="4294967296"/>"#),
        ("", "", r#"<w:formProt w:val="no"/>"#),
        (r#"<w:u w:val="unknown"/>"#, "", ""),
        ("", "", r#"<w:cols w:num="2"><w:col w:w="0"/></w:cols>"#),
        ("", "", r#"<w:textDirection w:val="reverse"/>"#),
        ("", r#"<w:spacing w:lineRule="unknown"/>"#, ""),
    ] {
        let bytes = source(run, paragraph, section);
        assert!(
            import_docx(&bytes, Id::new("malformed-format").unwrap()).is_err(),
            "{run} {paragraph} {section}"
        );
    }
}

#[test]
fn known_underline_and_logical_alignment_variants_have_named_approximations() {
    for (value, expected) in [
        ("words", office_core::Underline::Single),
        ("dashedHeavy", office_core::Underline::Single),
        ("wave", office_core::Underline::Single),
        ("wavyDouble", office_core::Underline::Double),
    ] {
        let bytes = source(&format!(r#"<w:u w:val="{value}"/>"#), "", "");
        let imported = editable(&bytes);
        let Block::Paragraph(paragraph) = &imported.document().sections()[0].blocks()[0];
        assert_eq!(paragraph.runs()[0].format.underline, Some(expected));
        assert!(!imported.report().normalizations.is_empty());
    }
    for (value, expected) in [
        ("start", office_core::Alignment::Left),
        ("end", office_core::Alignment::Right),
    ] {
        let bytes = source(
            "",
            &format!(r#"<w:jc w:val="{value}"/><w:spacing w:line="276"/>"#),
            "",
        );
        let imported = editable(&bytes);
        let Block::Paragraph(paragraph) = &imported.document().sections()[0].blocks()[0];
        assert_eq!(paragraph.format().alignment, Some(expected));
        assert_eq!(
            paragraph.format().line_spacing,
            Some(office_core::LineSpacing::Multiple { value: 276 })
        );
        assert_eq!(imported.report().normalizations.len(), 2);
    }
}

#[test]
fn columns_rotation_and_non_page_section_breaks_normalize_to_writer_flow() {
    for layout in [
        r#"<w:type w:val="continuous"/><w:cols w:num="2" w:space="720"/>"#,
        r#"<w:type w:val="oddPage"/><w:cols w:equalWidth="0"><w:col w:w="2000" w:space="720"/><w:col w:w="4000"/></w:cols>"#,
        r#"<w:type w:val="nextColumn"/><w:textDirection w:val="tbRl"/>"#,
        r#"<w:type w:val="evenPage"/><w:textDirection w:val="lrTbV"/>"#,
    ] {
        let bytes = source("", "", layout);
        let imported = editable(&bytes);
        assert_eq!(imported.report().normalizations.len(), 2);
        let copy = export_docx(imported.document()).unwrap();
        let reopened = editable(&copy);
        assert!(reopened.report().normalizations.is_empty());
    }
}

#[test]
fn paragraph_borders_are_visual_only_but_unknown_or_malformed_payloads_stay_guarded() {
    for side in ["top", "left", "bottom", "right", "between", "bar"] {
        let paragraph = format!(
            r#"<w:pBdr><w:{side} w:val="single" w:sz="4" w:space="10" w:color="0F4761" w:themeColor="accent1" w:themeShade="BF" w:frame="0" w:shadow="1"/></w:pBdr>"#
        );
        let bytes = source("", &paragraph, "");
        let imported = editable(&bytes);
        assert!(
            imported
                .report()
                .normalizations
                .iter()
                .any(|notice| notice.path == "pBdr")
        );
    }
    for paragraph in [
        r#"<w:pBdr><w:top w:val="single" w:future="1"/></w:pBdr>"#,
        r#"<w:pBdr><w:top w:val="single"><w:t>Do not discard</w:t></w:top></w:pBdr>"#,
        r#"<w:pBdr><x:top xmlns:x="urn:future" w:val="single"/></w:pBdr>"#,
        r#"<w:pBdr><w:top w:val="single"/><w:top w:val="double"/></w:pBdr>"#,
    ] {
        let bytes = source("", paragraph, "");
        assert!(matches!(
            import_docx(&bytes, Id::new("unsafe-border").unwrap()).unwrap(),
            DocxImportOutcome::Preserved { .. }
        ));
    }
    for paragraph in [
        r#"<w:pBdr><w:top w:val="single" w:sz="no"/></w:pBdr>"#,
        r#"<w:pBdr><w:top w:val="single" w:space="-1"/></w:pBdr>"#,
        r#"<w:pBdr><w:top w:val="single" w:color="not-rgb"/></w:pBdr>"#,
        r#"<w:pBdr><w:top w:val="single" w:shadow="yes"/></w:pBdr>"#,
        r#"<w:pBdr><w:top w:val="single" w:themeTint="100"/></w:pBdr>"#,
    ] {
        let bytes = source("", paragraph, "");
        assert!(import_docx(&bytes, Id::new("malformed-border").unwrap()).is_err());
    }
}

#[test]
fn many_duplicate_style_names_have_deterministic_unique_native_names() {
    const SAFE: &[u8] =
        include_bytes!("../../../tests/fixtures/docx/relationships/supported-auxiliary-parts.docx");
    let bytes = package(SAFE, |parts| {
        let extra: String = (0..1000).map(|index| format!(r#"<w:style w:type="paragraph" w:styleId="Duplicate{index}"><w:name w:val="Repeated style"/></w:style>"#)).collect();
        let styles = String::from_utf8(parts["word/styles.xml"].clone())
            .unwrap()
            .replace("</w:styles>", &format!("{extra}</w:styles>"));
        parts.insert("word/styles.xml".into(), styles.into_bytes());
    });
    let imported = editable(&bytes);
    let styles = imported.document().styles();
    let names: std::collections::BTreeSet<_> = styles.iter().map(|style| &style.name).collect();
    assert_eq!(styles.len(), names.len());
    assert!(names.iter().any(|name| name.as_str() == "Repeated style"));
    assert!(
        names
            .iter()
            .any(|name| name.as_str() == "Repeated style (1000)")
    );
    assert_eq!(imported.document(), editable(&bytes).document());
}

#[test]
fn oversized_inherited_font_names_cannot_amplify_through_style_caches_or_runs() {
    const SAFE: &[u8] =
        include_bytes!("../../../tests/fixtures/docx/relationships/supported-auxiliary-parts.docx");
    let bytes = package(SAFE, |parts| {
        let font = "F".repeat(64 * 1024);
        let mut extra = format!(
            r#"<w:style w:type="paragraph" w:styleId="LargeParagraphBase"><w:name w:val="Large paragraph base"/><w:rPr><w:rFonts w:ascii="{font}"/></w:rPr></w:style><w:style w:type="character" w:styleId="LargeCharacterBase"><w:name w:val="Large character base"/><w:rPr><w:rFonts w:ascii="{font}"/></w:rPr></w:style>"#
        );
        let mut body = String::new();
        for index in 0..128 {
            extra.push_str(&format!(r#"<w:style w:type="paragraph" w:styleId="LargeParagraph{index}"><w:name w:val="Large paragraph {index}"/><w:basedOn w:val="LargeParagraphBase"/></w:style><w:style w:type="character" w:styleId="LargeCharacter{index}"><w:name w:val="Large character {index}"/><w:basedOn w:val="LargeCharacterBase"/></w:style>"#));
            body.push_str(&format!(r#"<w:p><w:pPr><w:pStyle w:val="LargeParagraph{index}"/></w:pPr><w:r><w:rPr><w:rStyle w:val="LargeCharacter{index}"/></w:rPr><w:t>{TEXT}</w:t></w:r></w:p>"#));
        }
        let styles = String::from_utf8(parts["word/styles.xml"].clone())
            .unwrap()
            .replace("</w:styles>", &format!("{extra}</w:styles>"));
        parts.insert("word/styles.xml".into(), styles.into_bytes());
        parts.insert("word/document.xml".into(), format!(r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}</w:body></w:document>"#).into_bytes());
    });
    let imported = editable(&bytes);
    assert!(imported.document().styles().iter().all(|style| {
        style
            .character_format
            .font_family
            .as_ref()
            .is_none_or(|family| family.len() <= 1024)
    }));
    let mut text = String::new();
    for section in imported.document().sections() {
        for block in section.blocks() {
            let Block::Paragraph(paragraph) = block;
            for run in paragraph.runs() {
                assert!(
                    run.format
                        .font_family
                        .as_ref()
                        .is_none_or(|family| family.len() <= 1024)
                );
                text.push_str(&run.text);
            }
        }
    }
    assert_eq!(text, TEXT.repeat(128));
    assert!(
        imported
            .report()
            .normalizations
            .iter()
            .filter(|notice| notice.path == "rFonts")
            .count()
            >= 2
    );
}
