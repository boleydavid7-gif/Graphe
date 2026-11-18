//! Bounded literal search over Rust document text, with core cursor boundaries.

use office_core::{GraphemeOffset, TextCoordinates};
use serde::Serialize;

use crate::{
    BrowserEngine, EngineError,
    wire::{BrowserSelection, Point},
};

const MAX_QUERY_BYTES: usize = 1024;
const MAX_MATCHES: usize = 1000;

#[derive(Serialize)]
pub(super) struct FindMatch {
    pub(super) selection: BrowserSelection,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct FindSnapshot<'a> {
    session_id: &'a str,
    document_id: &'a str,
    revision: String,
    pub(super) matches: Vec<FindMatch>,
    pub(super) truncated: bool,
}

impl BrowserEngine {
    /// Finds exact, case-sensitive text within paragraphs without changing state.
    ///
    /// No Unicode normalization or locale-dependent folding is applied. Matches
    /// inside a combining/emoji cluster select its complete core grapheme, and
    /// duplicate expanded ranges are returned once. Paragraph separators are not
    /// synthesized into searchable text. At most 1,000 matches are returned.
    /// The response's session, document and revision must still match before a
    /// caller applies a result as its live selection.
    pub(super) fn find_snapshot(&self, query: &str) -> Result<FindSnapshot<'_>, EngineError> {
        if query.len() > MAX_QUERY_BYTES {
            return Err(EngineError::new(
                "invalid_find",
                "Find text is limited to 1,024 UTF-8 bytes.",
            ));
        }
        let mut response = FindSnapshot {
            session_id: &self.session_id,
            document_id: self.session.document().id().as_str(),
            revision: self.revision(),
            matches: Vec::new(),
            truncated: false,
        };
        if !query.is_empty() {
            'sections: for section in self.session.document().sections() {
                for paragraph in section.paragraphs() {
                    let coordinates = TextCoordinates::new(paragraph);
                    // Build each paragraph's byte/UTF-16 cursor table once;
                    // repeated prefix conversion for every hit would be quadratic.
                    let mut boundaries = Vec::with_capacity(coordinates.grapheme_len().get() + 1);
                    let mut scalars = coordinates.text().char_indices().peekable();
                    let mut utf16 = 0;
                    for index in 0..=coordinates.grapheme_len().get() {
                        let byte = coordinates
                            .grapheme_to_utf8(GraphemeOffset::new(index))?
                            .get();
                        while scalars.peek().is_some_and(|(at, _)| *at < byte) {
                            if let Some((_, scalar)) = scalars.next() {
                                utf16 += scalar.len_utf16();
                            }
                        }
                        boundaries.push((byte, utf16));
                    }
                    for (start, text) in coordinates.text().match_indices(query) {
                        let first = boundaries
                            .partition_point(|(byte, _)| *byte <= start)
                            .saturating_sub(1);
                        let last =
                            boundaries.partition_point(|(byte, _)| *byte < start + text.len());
                        let selection = BrowserSelection {
                            anchor: Point {
                                paragraph_id: paragraph.id().as_str().into(),
                                utf16: boundaries[first].1,
                            },
                            focus: Point {
                                paragraph_id: paragraph.id().as_str().into(),
                                utf16: boundaries[last].1,
                            },
                        };
                        if response
                            .matches
                            .last()
                            .is_some_and(|previous| previous.selection == selection)
                        {
                            continue;
                        }
                        if response.matches.len() == MAX_MATCHES {
                            response.truncated = true;
                            break 'sections;
                        }
                        response.matches.push(FindMatch { selection });
                    }
                }
            }
        }
        Ok(response)
    }

    /// Returns a JSON snapshot of case-sensitive literal matches for the current document.
    /// The snapshot is bounded and contains Rust-validated grapheme selections.
    pub fn find_text(&self, query: &str) -> Result<String, EngineError> {
        serde_json::to_string(&self.find_snapshot(query)?)
            .map_err(|error| EngineError::new("find_failed", error.to_string()))
    }
}
