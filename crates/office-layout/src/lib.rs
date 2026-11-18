//! Rust-authoritative pinned-font shaping, paragraph geometry and pagination.
//!
//! Layout is derived runtime state: it never edits or normalizes a document.
//! Positions use fixed 1/64-point units; outlines are unhinted font design units.
//! The registered font catalog is explicit and identical on native and WASM.

mod dto;
mod engine;
mod error;
mod interaction;
mod options;
mod style;

pub use dto::{
    Caret, CaretAffinity, Decoration, DocumentLayout, GlyphOutline, ImageLayout, LayoutDiagnostic,
    LineLayout, PageLayout, PositionedGlyph, Rect, TableCellLayout, TextRange, UNITS_PER_POINT,
};
pub use engine::{LayoutEngine, layout_document};
pub use error::LayoutError;
pub use interaction::{
    CaretGeometry, LayoutPosition, NavigationDirection, NavigationResult, SelectionGeometry,
    SelectionRectangle, VisualPosition,
};
pub use options::LayoutOptions;
pub use style::{ResolvedParagraphStyle, overlay_character, resolve_paragraph_style};
