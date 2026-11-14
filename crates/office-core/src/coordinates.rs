//! Explicit coordinate units and Unicode boundaries for a logical paragraph.

use unicode_segmentation::UnicodeSegmentation;

use crate::{OffsetUnit, Paragraph, PositionError};

/// Unicode version used by the extended grapheme-cluster segmentation algorithm.
///
/// Updating the segmentation dependency can change cursor boundaries. Such an
/// update must be reviewed alongside the matching Unicode conformance tests.
pub const GRAPHEME_UNICODE_VERSION: (u64, u64, u64) = unicode_segmentation::UNICODE_VERSION;

/// A zero-based UTF-8 byte offset, measured from the start of a paragraph.
///
/// This unit alone does not guarantee a scalar or cursor boundary. Conversion
/// through [`TextCoordinates`] validates the relevant boundary explicitly.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Utf8Offset(usize);

impl Utf8Offset {
    /// Wraps a byte count without claiming it is valid for any particular text.
    #[must_use]
    pub const fn new(value: usize) -> Self {
        Self(value)
    }

    /// Returns the UTF-8 byte count.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }
}

/// A zero-based UTF-16 code-unit offset from the start of a paragraph.
///
/// An offset inside a surrogate pair is not a scalar boundary. This type is a
/// numeric request; [`TextCoordinates`] performs validation against actual text.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Utf16Offset(usize);

impl Utf16Offset {
    /// Wraps a code-unit count without claiming it is a valid text boundary.
    #[must_use]
    pub const fn new(value: usize) -> Self {
        Self(value)
    }

    /// Returns the UTF-16 code-unit count.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }
}

/// A zero-based extended grapheme-cluster boundary index within a paragraph.
///
/// For N grapheme clusters the valid cursor indices are 0 through N inclusive.
/// This value is validated against text before it becomes a document position.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GraphemeOffset(usize);

impl GraphemeOffset {
    /// Wraps a grapheme boundary index without validating a particular text.
    #[must_use]
    pub const fn new(value: usize) -> Self {
        Self(value)
    }

    /// Returns the grapheme boundary index.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }
}

/// An owned coordinate snapshot of a paragraph's exact logical text.
///
/// Text runs are concatenated without separators before applying Unicode
/// extended grapheme segmentation. A formatting boundary, including an empty
/// run, never creates a cursor boundary inside a combining or emoji sequence.
/// Paragraph boundaries are outside this local coordinate space.
///
/// Raw UTF-8/UTF-16 conversions accept any Unicode scalar boundary. Converting
/// either unit to a grapheme index additionally requires a cursor boundary.
/// Invalid offsets return errors; no conversion snaps, clamps, or normalizes.
///
/// ```
/// use office_core::{GraphemeOffset, Id, Paragraph, TextCoordinates, TextRun, Utf16Offset};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// // The combining accent belongs to the preceding run's "e".
/// let paragraph = Paragraph::new(
///     Id::new("paragraph")?,
///     vec![TextRun::new("e"), TextRun::new("\u{301}😀")],
/// );
/// let coordinates = TextCoordinates::new(&paragraph);
/// assert_eq!(coordinates.grapheme_len(), GraphemeOffset::new(2));
/// assert_eq!(coordinates.utf16_to_grapheme(Utf16Offset::new(2))?, GraphemeOffset::new(1));
/// assert!(coordinates.utf16_to_grapheme(Utf16Offset::new(1)).is_err());
/// assert!(coordinates.utf16_to_utf8(Utf16Offset::new(3)).is_err());
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextCoordinates {
    text: String,
    grapheme_boundaries: Vec<usize>,
}

impl TextCoordinates {
    /// Builds a coordinate snapshot without modifying the paragraph or its runs.
    #[must_use]
    pub fn new(paragraph: &Paragraph) -> Self {
        let text: String = paragraph
            .runs()
            .iter()
            .map(|run| run.text.as_str())
            .collect();
        let mut grapheme_boundaries: Vec<usize> =
            text.grapheme_indices(true).map(|(byte, _)| byte).collect();
        // Even an empty paragraph has the single valid cursor boundary at zero.
        grapheme_boundaries.push(text.len());
        Self {
            text,
            grapheme_boundaries,
        }
    }

    /// Returns the exact logical text used for this coordinate snapshot.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Returns the UTF-8 length, also the valid end byte offset.
    #[must_use]
    pub fn utf8_len(&self) -> Utf8Offset {
        Utf8Offset::new(self.text.len())
    }

    /// Returns the UTF-16 length, also the valid end code-unit offset.
    #[must_use]
    pub fn utf16_len(&self) -> Utf16Offset {
        Utf16Offset::new(self.text.encode_utf16().count())
    }

    /// Returns the number of grapheme clusters, also the valid end cursor index.
    #[must_use]
    pub fn grapheme_len(&self) -> GraphemeOffset {
        GraphemeOffset::new(self.grapheme_boundaries.len() - 1)
    }

    /// Converts a UTF-8 scalar boundary to an exact UTF-16 code-unit offset.
    ///
    /// A scalar boundary inside a grapheme is permitted by this raw conversion.
    pub fn utf8_to_utf16(&self, offset: Utf8Offset) -> Result<Utf16Offset, PositionError> {
        self.check_utf8(offset)?;
        Ok(Utf16Offset::new(
            self.text[..offset.get()].encode_utf16().count(),
        ))
    }

    /// Converts a UTF-16 scalar boundary to an exact UTF-8 byte offset.
    ///
    /// Offsets inside surrogate pairs are rejected, even when in range.
    pub fn utf16_to_utf8(&self, offset: Utf16Offset) -> Result<Utf8Offset, PositionError> {
        let max = self.utf16_len().get();
        if offset.get() > max {
            return Err(PositionError::OutOfBounds {
                unit: OffsetUnit::Utf16,
                offset: offset.get(),
                max,
            });
        }
        let mut units = 0;
        for (byte, scalar) in self.text.char_indices() {
            if units == offset.get() {
                return Ok(Utf8Offset::new(byte));
            }
            units += scalar.len_utf16();
            if units > offset.get() {
                return Err(PositionError::InvalidUtf16Boundary { offset });
            }
        }
        Ok(self.utf8_len())
    }

    /// Converts a grapheme cursor index to its exact UTF-8 byte offset.
    pub fn grapheme_to_utf8(&self, offset: GraphemeOffset) -> Result<Utf8Offset, PositionError> {
        self.grapheme_boundaries
            .get(offset.get())
            .copied()
            .map(Utf8Offset::new)
            .ok_or(PositionError::OutOfBounds {
                unit: OffsetUnit::Grapheme,
                offset: offset.get(),
                max: self.grapheme_len().get(),
            })
    }

    /// Converts a UTF-8 offset only when it is an extended grapheme boundary.
    pub fn utf8_to_grapheme(&self, offset: Utf8Offset) -> Result<GraphemeOffset, PositionError> {
        self.check_utf8(offset)?;
        self.grapheme_boundaries
            .binary_search(&offset.get())
            .map(GraphemeOffset::new)
            .map_err(|_| PositionError::InvalidGraphemeBoundary { offset })
    }

    /// Converts a grapheme cursor index to its exact UTF-16 code-unit offset.
    pub fn grapheme_to_utf16(&self, offset: GraphemeOffset) -> Result<Utf16Offset, PositionError> {
        self.utf8_to_utf16(self.grapheme_to_utf8(offset)?)
    }

    /// Converts UTF-16 only when it identifies an extended grapheme boundary.
    ///
    /// This first rejects an offset inside a surrogate pair, then rejects valid
    /// scalar boundaries that lie inside a grapheme cluster.
    pub fn utf16_to_grapheme(&self, offset: Utf16Offset) -> Result<GraphemeOffset, PositionError> {
        self.utf8_to_grapheme(self.utf16_to_utf8(offset)?)
    }

    fn check_utf8(&self, offset: Utf8Offset) -> Result<(), PositionError> {
        if offset.get() > self.text.len() {
            return Err(PositionError::OutOfBounds {
                unit: OffsetUnit::Utf8,
                offset: offset.get(),
                max: self.text.len(),
            });
        }
        if !self.text.is_char_boundary(offset.get()) {
            return Err(PositionError::InvalidUtf8Boundary { offset });
        }
        Ok(())
    }
}
