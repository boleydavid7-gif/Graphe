use office_core::{HalfPoints, LineSpacing, PageMargins, PageOrientation, PageSize, Twips};
use serde::{Deserialize, Serialize};

/// Explicit defaults used only when the source does not specify a property.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct LayoutOptions {
    /// Default page dimensions; explicit section dimensions override them.
    pub page_size: PageSize,
    /// Default margins; explicit section margins override them.
    pub page_margins: PageMargins,
    /// Default registered font family, without host font discovery.
    pub font_family: String,
    /// Default font size in half-points.
    pub font_size: HalfPoints,
    /// Default paragraph line spacing.
    pub line_spacing: LineSpacing,
    /// Default space after paragraphs, in twips.
    pub spacing_after: Twips,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        Self {
            page_size: PageSize {
                width: Twips(12_240),
                height: Twips(15_840),
                orientation: Some(PageOrientation::Portrait),
            },
            page_margins: PageMargins {
                top: Twips(1_440),
                right: Twips(1_440),
                bottom: Twips(1_440),
                left: Twips(1_440),
                header: Twips(720),
                footer: Twips(720),
                gutter: Twips(0),
            },
            font_family: "Arimo".into(),
            font_size: HalfPoints(24),
            line_spacing: LineSpacing::Multiple { value: 360 },
            spacing_after: Twips(160),
        }
    }
}

impl LayoutOptions {
    /// Letter pages with one-inch margins and the documented typography defaults.
    #[must_use]
    pub fn letter() -> Self {
        Self::default()
    }

    /// Nominal A4 pages rounded to 11,906 by 16,838 twips, with one-inch margins.
    #[must_use]
    pub fn a4() -> Self {
        Self {
            page_size: PageSize {
                width: Twips(11_906),
                height: Twips(16_838),
                orientation: Some(PageOrientation::Portrait),
            },
            ..Self::default()
        }
    }
}
