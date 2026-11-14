use std::{error::Error, fmt};

/// Structured layout failure; source content and formatting are never changed.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum LayoutError {
    /// An interaction coordinate or endpoint does not identify this layout.
    InvalidPosition {
        /// Explanation of the rejected transient position.
        message: String,
    },
    /// Options or effective geometry cannot produce a usable layout.
    InvalidOptions {
        /// Explanation of the rejected setting.
        message: String,
    },
    /// Existing document validation rejected the input.
    InvalidDocument {
        /// Core model diagnostic.
        message: String,
    },
    /// A recognized source feature has no deliberate implementation yet.
    Unsupported {
        /// Feature requiring a future layout policy.
        feature: String,
        /// Relevant source paragraph, when known.
        paragraph_id: Option<String>,
    },
    /// A fixed prototype resource budget would be exceeded.
    LimitExceeded {
        /// Stable resource name.
        resource: &'static str,
        /// Maximum accepted value.
        limit: usize,
        /// Requested or observed value.
        actual: usize,
    },
    /// A pinned font could not be parsed, registered or outlined.
    Font {
        /// Catalog filename identifying the affected font.
        font_id: String,
        /// Human-readable diagnostic.
        message: String,
    },
}

impl LayoutError {
    /// Returns a stable category for native or browser transports.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InvalidPosition { .. } => "invalid_layout_position",
            Self::InvalidOptions { .. } => "invalid_layout_options",
            Self::InvalidDocument { .. } => "invalid_document",
            Self::Unsupported { .. } => "unsupported_layout",
            Self::LimitExceeded { .. } => "layout_too_large",
            Self::Font { .. } => "layout_font_error",
        }
    }
}

impl fmt::Display for LayoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPosition { message } => write!(f, "invalid layout position: {message}"),
            Self::InvalidOptions { message } => write!(f, "invalid layout options: {message}"),
            Self::InvalidDocument { message } => write!(f, "invalid layout document: {message}"),
            Self::Unsupported { feature, .. } => write!(f, "unsupported layout: {feature}"),
            Self::LimitExceeded {
                resource,
                limit,
                actual,
            } => write!(f, "layout {resource} limit {limit} exceeded by {actual}"),
            Self::Font { font_id, message } => write!(f, "layout font {font_id}: {message}"),
        }
    }
}

impl Error for LayoutError {}
