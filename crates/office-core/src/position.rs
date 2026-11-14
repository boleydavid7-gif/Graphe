use std::cmp::Ordering;
use std::error::Error;
use std::fmt;
use std::ops::Range;

use crate::{
    Block, Document, GraphemeOffset, Id, Paragraph, TextCoordinates, Utf8Offset, Utf16Offset,
};

/// The coordinate system used by an offset validation error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OffsetUnit {
    /// UTF-8 bytes from the start of a logical paragraph.
    Utf8,
    /// UTF-16 code units from the start of a logical paragraph.
    Utf16,
    /// Extended grapheme clusters from the start of a logical paragraph.
    Grapheme,
}

/// An invalid text coordinate, document position, or selection endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PositionError {
    /// The offset exceeds the inclusive end boundary of the text.
    OutOfBounds {
        /// Coordinate system of the supplied offset.
        unit: OffsetUnit,
        /// Rejected offset.
        offset: usize,
        /// Largest valid offset in that coordinate system.
        max: usize,
    },
    /// A byte offset falls inside a UTF-8 encoded Unicode scalar.
    InvalidUtf8Boundary {
        /// Rejected paragraph-relative byte offset.
        offset: Utf8Offset,
    },
    /// A UTF-16 offset falls between the two code units of a surrogate pair.
    InvalidUtf16Boundary {
        /// Rejected paragraph-relative code-unit offset.
        offset: Utf16Offset,
    },
    /// A scalar boundary falls inside an extended grapheme cluster.
    InvalidGraphemeBoundary {
        /// Rejected paragraph-relative byte offset.
        offset: Utf8Offset,
    },
    /// The position belongs to a different document identity.
    WrongDocument {
        /// Identity of the document used for validation.
        expected: Id,
        /// Identity stored in the position.
        actual: Id,
    },
    /// The identifier does not identify a paragraph in this document.
    ParagraphNotFound {
        /// Missing paragraph identifier (or an identifier of another entity kind).
        id: Id,
    },
}

impl fmt::Display for PositionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfBounds { unit, offset, max } => {
                write!(f, "{unit:?} offset {offset} exceeds paragraph end {max}")
            }
            Self::InvalidUtf8Boundary { offset } => {
                write!(f, "UTF-8 offset {} splits a Unicode scalar", offset.get())
            }
            Self::InvalidUtf16Boundary { offset } => {
                write!(f, "UTF-16 offset {} splits a surrogate pair", offset.get())
            }
            Self::InvalidGraphemeBoundary { offset } => {
                write!(f, "UTF-8 offset {} splits a grapheme cluster", offset.get())
            }
            Self::WrongDocument { expected, actual } => write!(
                f,
                "position belongs to document {}, expected {}",
                actual.as_str(),
                expected.as_str()
            ),
            Self::ParagraphNotFound { id } => write!(f, "paragraph not found: {}", id.as_str()),
        }
    }
}

impl Error for PositionError {}

/// A logical cursor boundary anchored to a document and paragraph by stable IDs.
///
/// The offset counts extended grapheme clusters across the paragraph's complete
/// text, independent of formatting runs. Zero and the paragraph end are valid.
/// Create positions through [`Document::position`]. Positions have no intrinsic
/// ordering; compare them against document order with [`Document::compare_positions`].
/// They are runtime values, not part of the document serialization schema.
///
/// Identity is stable, but offsets are not automatically rebased. A position is
/// valid against the content used for validation. Bare positions do not carry
/// revisions. When using [`crate::DocumentSession`], retain the originating
/// [`crate::Revision`] and validate it before reuse. No automatic rebasing or
/// editing affinity is provided.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DocumentPosition {
    document_id: Id,
    paragraph_id: Id,
    offset: GraphemeOffset,
}

impl DocumentPosition {
    /// Returns the stable document identity.
    #[must_use]
    pub fn document_id(&self) -> &Id {
        &self.document_id
    }

    /// Returns the stable paragraph identity, independent of its array index.
    #[must_use]
    pub fn paragraph_id(&self) -> &Id {
        &self.paragraph_id
    }

    /// Returns the paragraph-relative extended-grapheme offset.
    #[must_use]
    pub fn offset(&self) -> GraphemeOffset {
        self.offset
    }
}

/// A directed logical selection, with an anchor and an independently moving focus.
///
/// Both endpoints are cursor boundaries. Equal endpoints form a collapsed
/// selection. Ordering follows paragraph order in the document, never ID spelling
/// or visual bidirectional layout. Create selections through [`Document::selection`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    anchor: DocumentPosition,
    focus: DocumentPosition,
}

impl Selection {
    /// Returns the fixed endpoint, preserving the selection's direction.
    #[must_use]
    pub fn anchor(&self) -> &DocumentPosition {
        &self.anchor
    }

    /// Returns the moving endpoint, preserving the selection's direction.
    #[must_use]
    pub fn focus(&self) -> &DocumentPosition {
        &self.focus
    }

    /// Reports whether the two endpoints identify exactly the same boundary.
    #[must_use]
    pub fn is_collapsed(&self) -> bool {
        self.anchor == self.focus
    }

    /// Revalidates both endpoints and returns their logical half-open range.
    ///
    /// Paragraph end and the next paragraph's start remain distinct positions.
    /// No flattened paragraph separators, selected text, or visual geometry are
    /// synthesized. The returned range does not change anchor/focus direction.
    pub fn ordered_range(
        &self,
        document: &Document,
    ) -> Result<Range<DocumentPosition>, PositionError> {
        if document.compare_positions(&self.anchor, &self.focus)? == Ordering::Greater {
            Ok(self.focus.clone()..self.anchor.clone())
        } else {
            Ok(self.anchor.clone()..self.focus.clone())
        }
    }
}

impl Document {
    /// Builds a text-coordinate snapshot for a paragraph identified by stable ID.
    ///
    /// Runs are concatenated without separators, and grapheme segmentation can
    /// cross run boundaries. The existing document and run boundaries are preserved.
    pub fn text_coordinates(&self, paragraph_id: &Id) -> Result<TextCoordinates, PositionError> {
        let (paragraph, _) = self.locate_paragraph(paragraph_id)?;
        Ok(TextCoordinates::new(paragraph))
    }

    /// Creates a checked paragraph-relative grapheme cursor position.
    ///
    /// Empty paragraphs have one valid boundary at offset zero. Empty documents
    /// and sections have no paragraph cursor positions; no content is synthesized.
    pub fn position(
        &self,
        paragraph_id: &Id,
        offset: GraphemeOffset,
    ) -> Result<DocumentPosition, PositionError> {
        self.text_coordinates(paragraph_id)?
            .grapheme_to_utf8(offset)?;
        Ok(DocumentPosition {
            document_id: self.id().clone(),
            paragraph_id: paragraph_id.clone(),
            offset,
        })
    }

    /// Checks document identity, paragraph membership, and the grapheme offset.
    ///
    /// This detects missing elements and out-of-range offsets against the supplied
    /// document. It does not detect a changed text meaning under the same IDs.
    pub fn validate_position(&self, position: &DocumentPosition) -> Result<(), PositionError> {
        self.resolve_position(position).map(|_| ())
    }

    /// Compares checked positions in logical section, block, then grapheme order.
    ///
    /// Positions must belong to this document. Visual RTL order, layout affinity,
    /// and cross-paragraph navigation are outside this milestone.
    pub fn compare_positions(
        &self,
        first: &DocumentPosition,
        second: &DocumentPosition,
    ) -> Result<Ordering, PositionError> {
        let first = self.resolve_position(first)?;
        let second = self.resolve_position(second)?;
        Ok(first.cmp(&second))
    }

    /// Constructs a directed selection after validating both endpoints.
    pub fn selection(
        &self,
        anchor: DocumentPosition,
        focus: DocumentPosition,
    ) -> Result<Selection, PositionError> {
        self.compare_positions(&anchor, &focus)?;
        Ok(Selection { anchor, focus })
    }

    fn locate_paragraph(
        &self,
        paragraph_id: &Id,
    ) -> Result<(&Paragraph, (usize, usize)), PositionError> {
        for (section_index, section) in self.sections().iter().enumerate() {
            for (block_index, block) in section.blocks().iter().enumerate() {
                let Block::Paragraph(paragraph) = block;
                if paragraph.id() == paragraph_id {
                    return Ok((paragraph, (section_index, block_index)));
                }
            }
        }
        Err(PositionError::ParagraphNotFound {
            id: paragraph_id.clone(),
        })
    }

    fn resolve_position(
        &self,
        position: &DocumentPosition,
    ) -> Result<(usize, usize, GraphemeOffset), PositionError> {
        if position.document_id != *self.id() {
            return Err(PositionError::WrongDocument {
                expected: self.id().clone(),
                actual: position.document_id.clone(),
            });
        }
        let (paragraph, (section, block)) = self.locate_paragraph(&position.paragraph_id)?;
        TextCoordinates::new(paragraph).grapheme_to_utf8(position.offset)?;
        Ok((section, block, position.offset))
    }
}
