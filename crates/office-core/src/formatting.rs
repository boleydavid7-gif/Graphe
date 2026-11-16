//! Explicit formatting values stored independently from layout and rendering.

use serde::{Deserialize, Serialize};

use crate::ValidationError;

/// A length in twips: 1/1440 of an inch, or 1/20 of a point.
///
/// The signed representation allows negative indentation. Properties such as
/// paragraph spacing impose their own nonnegative or positive constraints when
/// the containing document is validated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Twips(
    /// The number of twips.
    pub i32,
);

/// A font size in half-points: a value of 24 means 12 points.
///
/// A document is invalid if it contains a font size of zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HalfPoints(
    /// The number of half-points.
    pub u32,
);

/// The requested physical orientation of a page, independent of its dimensions.
///
/// Importers preserve this explicit value without swapping width and height.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PageOrientation {
    /// Portrait printing orientation.
    Portrait,
    /// Landscape printing orientation.
    Landscape,
}

/// Explicit physical page dimensions, measured in twips.
///
/// Width and height must both be positive. Orientation is preserved separately;
/// it never changes or implicitly rotates these dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageSize {
    /// Physical page width, greater than zero.
    pub width: Twips,
    /// Physical page height, greater than zero.
    pub height: Twips,
    /// Optional explicit printing orientation.
    pub orientation: Option<PageOrientation>,
}

impl PageSize {
    pub(crate) fn validate(&self, path: &str) -> Result<(), ValidationError> {
        for (field, value) in [("width", self.width), ("height", self.height)] {
            if value.0 <= 0 {
                return Err(invalid(path, field, "page dimension must be positive"));
            }
        }
        Ok(())
    }
}

/// Explicit page margins and header/footer offsets, measured in twips.
///
/// All values must be nonnegative in this initial geometry profile. Negative
/// source top/bottom margins have distinct overlap semantics and require an
/// unsupported-content diagnostic at a format adapter, rather than clamping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageMargins {
    /// Distance from the physical top edge to the body text area.
    pub top: Twips,
    /// Distance from the physical right edge to the body text area.
    pub right: Twips,
    /// Distance from the physical bottom edge to the body text area.
    pub bottom: Twips,
    /// Distance from the physical left edge to the body text area.
    pub left: Twips,
    /// Distance from the top edge to header content, when supported.
    pub header: Twips,
    /// Distance from the bottom edge to footer content, when supported.
    pub footer: Twips,
    /// Additional binding margin, reducing the body's horizontal usable area.
    pub gutter: Twips,
}

impl PageMargins {
    pub(crate) fn validate(&self, path: &str) -> Result<(), ValidationError> {
        for (field, value) in [
            ("top", self.top),
            ("right", self.right),
            ("bottom", self.bottom),
            ("left", self.left),
            ("header", self.header),
            ("footer", self.footer),
            ("gutter", self.gutter),
        ] {
            if value.0 < 0 {
                return Err(invalid(path, field, "page margin must be nonnegative"));
            }
        }
        Ok(())
    }
}

/// An explicitly selected underline treatment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Underline {
    /// Explicitly disable underlining, including underlining inherited from a style.
    None,
    /// Draw a single underline.
    Single,
    /// Draw a double underline.
    Double,
}

/// A text color, without imposing a renderer or color-management policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TextColor {
    /// Request automatic text color; the eventual rendering policy determines it.
    Auto {},
    /// An explicit eight-bit-per-channel sRGB color.
    Rgb {
        /// The red channel, from 0 through 255.
        red: u8,
        /// The green channel, from 0 through 255.
        green: u8,
        /// The blue channel, from 0 through 255.
        blue: u8,
    },
}

/// Character formatting overrides for a text run or named style.
///
/// Every property defaults to unspecified (`None`). Unspecified values remain
/// available for inheritance, whereas `Some(false)` and
/// `Some(Underline::None)` explicitly disable an inherited treatment. This type
/// stores properties; it does not resolve styles or select a platform font.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CharacterFormat {
    /// Whether bold is explicitly enabled or disabled.
    pub bold: Option<bool>,
    /// Whether italic is explicitly enabled or disabled.
    pub italic: Option<bool>,
    /// An explicit underline treatment.
    pub underline: Option<Underline>,
    /// A font family name; whitespace-only names are invalid.
    pub font_family: Option<String>,
    /// A positive font size in half-points.
    pub font_size: Option<HalfPoints>,
    /// An explicit color or automatic color request.
    pub color: Option<TextColor>,
    /// Optional safe HTTP(S) link target on a text run (not inherited from HTML).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hyperlink: Option<String>,
}

impl CharacterFormat {
    pub(crate) fn validate(&self, path: &str) -> Result<(), ValidationError> {
        if self
            .font_family
            .as_ref()
            .is_some_and(|family| family.trim().is_empty())
        {
            return Err(invalid(
                path,
                "font_family",
                "font family must contain a non-whitespace character",
            ));
        }

        if let Some(url) = &self.hyperlink {
            if !valid_hyperlink(url) {
                return Err(invalid(path, "hyperlink",
                    "link must be an HTTP or HTTPS URL without credentials, controls, or whitespace"));
            }
        }

        if self.font_size.is_some_and(|size| size.0 == 0) {
            return Err(invalid(
                path,
                "font_size",
                "font size must be greater than zero half-points",
            ));
        }

        Ok(())
    }
}

/// Validate a bounded, navigation-safe HTTP(S) destination without network access.
/// Only the browser can decide whether to open a validated external link.
/// Credentials, backslashes, whitespace and control characters are forbidden.
#[must_use]
pub fn valid_hyperlink(url: &str) -> bool {
    if url.len() > 2048 || url.is_empty() || url.chars().any(|ch|
        ch.is_whitespace() || ch.is_control() || matches!(ch, '\\' | '<' | '>' | '"')
    ) {
        return false;
    }
    let Some(tail) = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://")) else {
        return false;
    };
    let authority = tail.split(&['/', '?', '#'][..]).next().unwrap_or("");
    !authority.is_empty() && !authority.contains('@')
        && !authority.starts_with(':') && !authority.ends_with(':')
}

/// Horizontal paragraph alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Alignment {
    /// Align against the left edge.
    Left,
    /// Center between the paragraph's left and right edges.
    Center,
    /// Align against the right edge.
    Right,
    /// Distribute eligible lines across the paragraph's available width.
    Justify,
}

/// A line-spacing rule expressed without floating-point measurements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum LineSpacing {
    /// A multiple of line height, in 1/240-line units.
    Multiple {
        /// A positive multiple; 240 is single spacing and 360 is 1.5 spacing.
        value: u32,
    },
    /// A fixed line height.
    Exact {
        /// A positive height in twips.
        height: Twips,
    },
    /// A minimum line height that may grow to accommodate content.
    AtLeast {
        /// A positive minimum height in twips.
        height: Twips,
    },
}

/// Paragraph indentation overrides, measured in twips.
///
/// All fields default to unspecified. Negative left or right indentation extends
/// beyond the corresponding paragraph boundary. A negative `first_line` value
/// represents a hanging indent; it is not a second, contradictory hanging field.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Indentation {
    /// An offset from the left paragraph boundary.
    pub left: Option<Twips>,
    /// An offset from the right paragraph boundary.
    pub right: Option<Twips>,
    /// The first-line offset relative to the remaining lines.
    pub first_line: Option<Twips>,
}

/// Paragraph formatting overrides for a paragraph or named style.
///
/// Every property defaults to unspecified (`None`) so inheritance is preserved.
/// This is a logical property set, independent of pagination or rendering.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ParagraphFormat {
    /// An explicit horizontal alignment.
    pub alignment: Option<Alignment>,
    /// Nonnegative spacing before the paragraph, in twips.
    pub spacing_before: Option<Twips>,
    /// Nonnegative spacing after the paragraph, in twips.
    pub spacing_after: Option<Twips>,
    /// An explicit line-spacing rule with a positive value.
    pub line_spacing: Option<LineSpacing>,
    /// Explicit indentation offsets.
    pub indentation: Option<Indentation>,
    /// Request or explicitly disable a page break before this paragraph.
    pub page_break_before: Option<bool>,
}

impl ParagraphFormat {
    pub(crate) fn validate(&self, path: &str) -> Result<(), ValidationError> {
        for (field, value) in [
            ("spacing_before", self.spacing_before),
            ("spacing_after", self.spacing_after),
        ] {
            if value.is_some_and(|spacing| spacing.0 < 0) {
                return Err(invalid(
                    path,
                    field,
                    "paragraph spacing must be nonnegative",
                ));
            }
        }

        let valid_line_spacing = match self.line_spacing {
            None => true,
            Some(LineSpacing::Multiple { value }) => value > 0,
            Some(LineSpacing::Exact { height } | LineSpacing::AtLeast { height }) => height.0 > 0,
        };

        if !valid_line_spacing {
            return Err(invalid(
                path,
                "line_spacing",
                "line spacing must be greater than zero",
            ));
        }

        Ok(())
    }
}

fn invalid(path: &str, field: &str, reason: &str) -> ValidationError {
    ValidationError::InvalidFormatting {
        path: format!("{path}.{field}"),
        reason: reason.to_owned(),
    }
}
