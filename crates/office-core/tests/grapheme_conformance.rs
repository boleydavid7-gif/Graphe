//! Official Unicode 17 extended-grapheme conformance, including run boundaries.

use office_core::{
    GRAPHEME_UNICODE_VERSION, GraphemeOffset, Id, Paragraph, TextCoordinates, TextRun, Utf8Offset,
};

const FIXTURE: &str = include_str!("../../../tests/fixtures/unicode/GraphemeBreakTest.txt");

fn vector(line: &str) -> Option<(String, Vec<usize>)> {
    let content = line.split('#').next().unwrap().trim();
    if content.is_empty() {
        return None;
    }

    let mut text = String::new();
    let mut boundaries = Vec::new();
    let mut expect_marker = true;
    for token in content.split_whitespace() {
        if expect_marker {
            match token {
                "÷" => boundaries.push(text.len()),
                "×" => {}
                _ => panic!("invalid grapheme-boundary marker: {token}"),
            }
        } else {
            let scalar = u32::from_str_radix(token, 16).unwrap();
            text.push(char::from_u32(scalar).unwrap());
        }
        expect_marker = !expect_marker;
    }
    assert!(!expect_marker, "vector must end with a boundary marker");
    assert_eq!(boundaries.first(), Some(&0));
    assert_eq!(boundaries.last(), Some(&text.len()));
    Some((text, boundaries))
}

fn check(text: &str, boundaries: &[usize], runs: Vec<TextRun>, line: usize, layout: &str) {
    let paragraph = Paragraph::new(Id::new("conformance-paragraph").unwrap(), runs);
    let coordinates = TextCoordinates::new(&paragraph);
    assert_eq!(coordinates.text(), text, "fixture line {line}, {layout}");
    assert_eq!(
        coordinates.grapheme_len().get(),
        boundaries.len() - 1,
        "grapheme count at fixture line {line}, {layout}"
    );
    for (index, &byte) in boundaries.iter().enumerate() {
        let grapheme = GraphemeOffset::new(index);
        let utf8 = Utf8Offset::new(byte);
        assert_eq!(
            coordinates.grapheme_to_utf8(grapheme).unwrap(),
            utf8,
            "grapheme {index} at fixture line {line}, {layout}"
        );
        assert_eq!(coordinates.utf8_to_grapheme(utf8).unwrap(), grapheme);
        let utf16 = coordinates.utf8_to_utf16(utf8).unwrap();
        assert_eq!(coordinates.grapheme_to_utf16(grapheme).unwrap(), utf16);
        assert_eq!(coordinates.utf16_to_grapheme(utf16).unwrap(), grapheme);
        assert_eq!(coordinates.utf16_to_utf8(utf16).unwrap(), utf8);
    }

    // The Unicode vectors mark non-break scalar boundaries as well as breaks.
    // Confirm that the public cursor conversion rejects every such interior.
    for byte in text.char_indices().map(|(byte, _)| byte) {
        let actual = coordinates.utf8_to_grapheme(Utf8Offset::new(byte));
        assert_eq!(
            actual.is_ok(),
            boundaries.binary_search(&byte).is_ok(),
            "cursor admissibility at byte {byte}, fixture line {line}, {layout}"
        );
    }
}

#[test]
fn official_extended_grapheme_vectors_match_whole_and_split_runs() {
    // A library Unicode upgrade must update the official fixture deliberately.
    assert_eq!(GRAPHEME_UNICODE_VERSION, (17, 0, 0));
    assert!(FIXTURE.starts_with("# GraphemeBreakTest-17.0.0.txt\n"));
    assert!(FIXTURE.trim_end().ends_with("# EOF"));

    let mut count = 0;
    for (index, line) in FIXTURE.lines().enumerate() {
        let Some((text, boundaries)) = vector(line) else {
            continue;
        };
        count += 1;
        check(
            &text,
            &boundaries,
            vec![TextRun::new(&text)],
            index + 1,
            "single run",
        );
        check(
            &text,
            &boundaries,
            text.chars()
                .map(|scalar| TextRun::new(scalar.to_string()))
                .collect(),
            index + 1,
            "one scalar per run",
        );
    }
    // Unicode's source file declares this count; reject accidental truncation.
    assert_eq!(count, 766);
}
