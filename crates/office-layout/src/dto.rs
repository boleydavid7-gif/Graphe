//! Serializable render geometry, independent of a frontend or drawing API.

use serde::{Deserialize, Serialize};

/// Number of fixed output units per typographic point.
pub const UNITS_PER_POINT: i64 = 64;

/// A fixed-unit rectangle in a page's top-left, downward-positive coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rect {
    /// Horizontal position.
    pub x: i64,
    /// Vertical position.
    pub y: i64,
    /// Width.
    pub width: i64,
    /// Height.
    pub height: i64,
}

/// Exact logical source boundaries for a laid-out line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextRange {
    /// Inclusive paragraph-relative UTF-8 byte boundary.
    pub start_utf8: usize,
    /// Exclusive paragraph-relative UTF-8 byte boundary.
    pub end_utf8: usize,
    /// Inclusive UTF-16 code-unit boundary.
    pub start_utf16: usize,
    /// Exclusive UTF-16 code-unit boundary.
    pub end_utf16: usize,
    /// Inclusive core grapheme boundary ordinal.
    pub start_grapheme: usize,
    /// Exclusive core grapheme boundary ordinal.
    pub end_grapheme: usize,
}

/// Which logical neighbor contributes a visual caret at a bidi/wrap boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaretAffinity {
    /// The edge of the text after the position.
    Leading,
    /// The edge of the text before the position.
    Trailing,
}

/// A visual caret attached to an exact core logical boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Caret {
    /// Paragraph-relative UTF-8 byte offset.
    pub utf8: usize,
    /// Paragraph-relative UTF-16 code-unit offset.
    pub utf16: usize,
    /// Paragraph-relative core grapheme boundary ordinal.
    pub grapheme: usize,
    /// Page-local horizontal position in fixed units.
    pub x: i64,
    /// Page-local top position in fixed units.
    pub y: i64,
    /// Caret height in fixed units.
    pub height: i64,
    /// Logical neighbor determining this visual placement.
    pub affinity: CaretAffinity,
}

/// An unhinted outline shared by all occurrences of the same font/location/glyph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlyphOutline {
    /// Stable ordinal within this document layout.
    pub id: u32,
    /// Pinned catalog filename identifying the font face.
    pub font_id: String,
    /// Font-local glyph identifier, distinct from a Unicode character.
    pub glyph_id: u32,
    /// Font design units in one em.
    pub units_per_em: u16,
    /// SVG-compatible path commands in font units, with upward-positive y.
    pub path: String,
}

/// A fully positioned glyph; renderers must use the supplied transform directly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionedGlyph {
    /// Outline dictionary ordinal.
    pub outline_id: u32,
    /// Page-local horizontal origin in fixed units.
    pub x: i64,
    /// Page-local baseline origin in fixed units.
    pub y: i64,
    /// Font size in fixed units.
    pub font_size: i64,
    /// Complete SVG translate/scale transform, including the font-y-axis flip.
    pub transform: String,
    /// Resolved sRGB CSS hex color.
    pub color: String,
}

/// A filled underline rectangle in page-local fixed units.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Decoration {
    /// Horizontal start.
    pub x: i64,
    /// Vertical top.
    pub y: i64,
    /// Extent across the corresponding glyph run.
    pub width: i64,
    /// Rectangle height.
    pub thickness: i64,
    /// Resolved sRGB CSS hex color.
    pub color: String,
}

/// One physical line of a logical paragraph, including source mappings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineLayout {
    /// Stable source paragraph identifier.
    pub paragraph_id: String,
    /// Zero-based line index within the paragraph, across page continuations.
    pub line_index: u32,
    /// Exact source substring, for nonvisual alternatives rather than reshaping.
    pub text: String,
    /// Logical source range, retaining all coordinate systems explicitly.
    pub text_range: TextRange,
    /// Horizontal line origin in fixed units.
    pub x: i64,
    /// Top of its line box in fixed units.
    pub y: i64,
    /// Measured advance excluding hanging trailing whitespace, including justification.
    pub width: i64,
    /// Line-box height; ink can extend outside an exact-height line box.
    pub height: i64,
    /// Page-local baseline position.
    pub baseline: i64,
    /// Fully positioned shaped glyphs in visual order.
    pub glyphs: Vec<PositionedGlyph>,
    /// Core cursor boundaries with visual affinity.
    pub carets: Vec<Caret>,
    /// Underline rectangles, including two rectangles for double underline.
    pub decorations: Vec<Decoration>,
}

/// One physical page. Sections start on a new page in this first layout profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageLayout {
    /// One-based page number in the document.
    pub number: u32,
    /// Stable source section identifier, empty for a sectionless blank document.
    pub section_id: String,
    /// Physical page width in fixed units.
    pub width: i64,
    /// Physical page height in fixed units.
    pub height: i64,
    /// Usable body area in page-local fixed units.
    pub content_rect: Rect,
    /// Lines placed by Rust; frontend text measurement is unnecessary.
    pub lines: Vec<LineLayout>,
}

/// A supported layout completed with a clearly reported fidelity limitation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutDiagnostic {
    /// Stable diagnostic category.
    pub kind: String,
    /// Relevant paragraph when the finding is local to source content.
    pub paragraph_id: Option<String>,
    /// Human-readable explanation, not a machine-readable identifier.
    pub message: String,
}

/// Complete deterministic page/glyph geometry for an immutable document snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentLayout {
    /// Stable source document identifier.
    pub document_id: String,
    /// Versioned identity of the pinned font catalog and typography profile.
    pub font_catalog_id: String,
    /// Fixed output resolution; currently 64 units per point.
    pub units_per_point: i64,
    /// Physical pages in reading order.
    pub pages: Vec<PageLayout>,
    /// Shared glyph outline dictionary in deterministic first-use order.
    pub outlines: Vec<GlyphOutline>,
    /// Bounded warnings; no source document is repaired or mutated.
    pub diagnostics: Vec<LayoutDiagnostic>,
}
