//! Editing and exact history restoration over every official Unicode 17 vector.

use office_core::{
    Block, CharacterFormat, Document, DocumentMetadata, DocumentSession, GRAPHEME_UNICODE_VERSION,
    GraphemeOffset, Id, Paragraph, Section, TextRun,
};

const FIXTURE: &str = include_str!("../../../tests/fixtures/unicode/GraphemeBreakTest.txt");

fn vector(line: &str) -> Option<(String, Vec<usize>)> {
    let content = line.split('#').next().unwrap().trim();
    if content.is_empty() {
        return None;
    }
    let mut text = String::new();
    let mut boundaries = Vec::new();
    let mut marker = true;
    for token in content.split_whitespace() {
        if marker {
            match token {
                "÷" => boundaries.push(text.len()),
                "×" => {}
                _ => panic!("unexpected boundary marker: {token}"),
            }
        } else {
            let scalar = u32::from_str_radix(token, 16).unwrap();
            text.push(char::from_u32(scalar).unwrap());
        }
        marker = !marker;
    }
    assert!(!marker, "vector must end in a boundary marker");
    assert_eq!(boundaries.first(), Some(&0));
    assert_eq!(boundaries.last(), Some(&text.len()));
    Some((text, boundaries))
}

fn original(text: &str, split: bool) -> Document {
    let runs = if split {
        text.chars()
            .enumerate()
            .map(|(index, scalar)| TextRun {
                text: scalar.to_string(),
                format: CharacterFormat {
                    bold: Some(index % 2 == 0),
                    ..Default::default()
                },
            })
            .collect()
    } else {
        vec![TextRun {
            text: text.into(),
            format: CharacterFormat {
                bold: Some(true),
                ..Default::default()
            },
        }]
    };
    Document::from_parts(
        Id::new("document").unwrap(),
        DocumentMetadata::default(),
        vec![Section::new(
            Id::new("section").unwrap(),
            vec![Block::Paragraph(Paragraph::new(
                Id::new("paragraph").unwrap(),
                runs,
            ))],
        )],
        vec![],
    )
    .unwrap()
}

fn paragraph(document: &Document) -> &Paragraph {
    let Block::Paragraph(paragraph) = &document.sections()[0].blocks()[0] else {
        panic!("expected paragraph");
    };
    paragraph
}

fn formatted_scalars(document: &Document) -> Vec<(char, CharacterFormat)> {
    paragraph(document)
        .runs()
        .iter()
        .flat_map(|run| run.text.chars().map(|scalar| (scalar, run.format.clone())))
        .collect()
}

#[test]
fn every_official_grapheme_range_edits_without_losing_text_formatting_or_history() {
    assert_eq!(GRAPHEME_UNICODE_VERSION, (17, 0, 0));
    assert!(FIXTURE.starts_with("# GraphemeBreakTest-17.0.0.txt\n"));
    let paragraph_id = Id::new("paragraph").unwrap();
    let mut vectors = 0;
    let mut edits = 0;
    for (line_index, line) in FIXTURE.lines().enumerate() {
        let Some((text, boundaries)) = vector(line) else {
            continue;
        };
        vectors += 1;
        for split in [false, true] {
            let original = original(&text, split);
            let scalars = formatted_scalars(&original);
            for (start, &start_byte) in boundaries.iter().enumerate() {
                for (end, &end_byte) in boundaries.iter().enumerate().skip(start) {
                    for replacement in ["X", ""] {
                        edits += 1;
                        let context = format!(
                            "line {}, split={split}, range={start}..{end}, replacement={replacement:?}",
                            line_index + 1
                        );
                        let expected_text = format!(
                            "{}{}{}",
                            &text[..start_byte],
                            replacement,
                            &text[end_byte..]
                        );
                        // Derive retained character properties from the original
                        // scalar stream, independently of formatting-run slicing.
                        let prefix_count = text[..start_byte].chars().count();
                        let suffix_start = text[..end_byte].chars().count();
                        let expected_scalars: Vec<_> = scalars[..prefix_count]
                            .iter()
                            .cloned()
                            .chain(
                                replacement
                                    .chars()
                                    .map(|scalar| (scalar, CharacterFormat::default())),
                            )
                            .chain(scalars[suffix_start..].iter().cloned())
                            .collect();

                        let mut session = DocumentSession::with_history_limit(original.clone(), 1);
                        let origin = session.revision();
                        let mut transaction = session.begin_transaction();
                        let anchor = transaction
                            .position(&paragraph_id, GraphemeOffset::new(start))
                            .unwrap();
                        let focus = transaction
                            .position(&paragraph_id, GraphemeOffset::new(end))
                            .unwrap();
                        let selection = transaction.selection(anchor, focus).unwrap();
                        let changed = if replacement.is_empty() {
                            transaction.delete_text(&selection).unwrap()
                        } else {
                            transaction
                                .replace_text(&selection, replacement, CharacterFormat::default())
                                .unwrap()
                        };
                        assert_eq!(
                            changed,
                            start != end || !replacement.is_empty(),
                            "{context}"
                        );
                        let coordinates = transaction.text_coordinates(&paragraph_id).unwrap();
                        assert_eq!(coordinates.text(), expected_text, "{context}");
                        // A splice can join adjacent clusters. Ask the updated
                        // draft for its boundaries instead of reusing old offsets.
                        transaction
                            .position(&paragraph_id, coordinates.grapheme_len())
                            .unwrap();
                        assert_eq!(session.document(), &original, "{context}");

                        let outcome = session.commit(transaction).unwrap();
                        assert_eq!(outcome.changed(), changed, "{context}");
                        assert_eq!(session.document().validate(), Ok(()), "{context}");
                        assert_eq!(
                            paragraph(session.document()).id(),
                            &paragraph_id,
                            "{context}"
                        );
                        assert_eq!(
                            formatted_scalars(session.document()),
                            expected_scalars,
                            "{context}"
                        );
                        let edited = session.document().clone();
                        if changed {
                            assert!(session.undo().unwrap().changed(), "{context}");
                            assert_eq!(session.document(), &original, "{context}");
                            assert!(session.redo().unwrap().changed(), "{context}");
                            assert_eq!(session.document(), &edited, "{context}");
                            assert_eq!(session.revision().number(), 3, "{context}");
                        } else {
                            assert_eq!(session.revision(), origin, "{context}");
                            assert!(!session.can_undo(), "{context}");
                        }
                    }
                }
            }
        }
    }
    assert_eq!(vectors, 766);
    assert_eq!(edits, 16_944);
}
