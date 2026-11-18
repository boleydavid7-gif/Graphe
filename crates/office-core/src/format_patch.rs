use crate::{
    Alignment, CharacterFormat, HalfPoints, Indentation, LineSpacing, ParagraphFormat, TextColor,
    Twips, Underline, ValidationError,
};

/// An explicit change to one optional formatting property.
///
/// Clearing restores the unspecified value eligible for inheritance. It differs
/// from setting `false` or [`Underline::None`], which are explicit overrides.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum PropertyChange<T> {
    /// Preserve the property's current value.
    #[default]
    Keep,
    /// Set an explicit value.
    Set(T),
    /// Remove the direct value, restoring inheritance.
    Clear,
}

impl<T: Clone> PropertyChange<T> {
    fn apply(&self, original: &Option<T>) -> Option<T> {
        match self {
            Self::Keep => original.clone(),
            Self::Set(value) => Some(value.clone()),
            Self::Clear => None,
        }
    }
}

/// Changes to selected text's direct character properties.
///
/// Every property defaults to [`PropertyChange::Keep`]. Patches do not resolve
/// named styles or alter unselected text.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CharacterFormatPatch {
    /// Change the direct bold override.
    pub bold: PropertyChange<bool>,
    /// Change the direct italic override.
    pub italic: PropertyChange<bool>,
    /// Change the direct underline override.
    pub underline: PropertyChange<Underline>,
    /// Change the direct font-family override.
    pub font_family: PropertyChange<String>,
    /// Change the direct font size in positive half-points.
    pub font_size: PropertyChange<HalfPoints>,
    /// Change the direct text color.
    pub color: PropertyChange<TextColor>,
    /// Set or clear the validated link destination.
    pub hyperlink: PropertyChange<String>,
}

impl CharacterFormatPatch {
    pub(crate) fn apply(&self, original: &CharacterFormat) -> CharacterFormat {
        CharacterFormat {
            bold: self.bold.apply(&original.bold),
            italic: self.italic.apply(&original.italic),
            underline: self.underline.apply(&original.underline),
            font_family: self.font_family.apply(&original.font_family),
            font_size: self.font_size.apply(&original.font_size),
            color: self.color.apply(&original.color),
            hyperlink: self.hyperlink.apply(&original.hyperlink),
        }
    }

    pub(crate) fn validate(&self) -> Result<(), ValidationError> {
        self.apply(&CharacterFormat::default())
            .validate("character_patch")
    }
}

/// Changes to direct paragraph properties in the selected paragraph range.
///
/// Every property defaults to [`PropertyChange::Keep`]. Setting indentation
/// replaces its complete override record; clearing removes that record.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParagraphFormatPatch {
    /// Change direct alignment.
    pub alignment: PropertyChange<Alignment>,
    /// Change nonnegative spacing before the paragraph.
    pub spacing_before: PropertyChange<Twips>,
    /// Change nonnegative spacing after the paragraph.
    pub spacing_after: PropertyChange<Twips>,
    /// Change the positive line-spacing rule.
    pub line_spacing: PropertyChange<LineSpacing>,
    /// Replace or remove the complete indentation override record.
    pub indentation: PropertyChange<Indentation>,
    /// Change the direct page-break-before override.
    pub page_break_before: PropertyChange<bool>,
    /// Control paragraph-internal page splitting.
    pub keep_together: PropertyChange<bool>,
    /// Keep this paragraph with its following body paragraph.
    pub keep_with_next: PropertyChange<bool>,
}

impl ParagraphFormatPatch {
    pub(crate) fn apply(&self, original: &ParagraphFormat) -> ParagraphFormat {
        ParagraphFormat {
            alignment: self.alignment.apply(&original.alignment),
            spacing_before: self.spacing_before.apply(&original.spacing_before),
            spacing_after: self.spacing_after.apply(&original.spacing_after),
            line_spacing: self.line_spacing.apply(&original.line_spacing),
            indentation: self.indentation.apply(&original.indentation),
            page_break_before: self.page_break_before.apply(&original.page_break_before),
            keep_together: self.keep_together.apply(&original.keep_together),
            keep_with_next: self.keep_with_next.apply(&original.keep_with_next),
        }
    }

    pub(crate) fn validate(&self) -> Result<(), ValidationError> {
        self.apply(&ParagraphFormat::default())
            .validate("paragraph_patch")
    }
}

/// Independently changes concrete paragraph spacing and indentation overrides.
///
/// Unlike `ParagraphFormatPatch::indentation`, this preserves every unmodified
/// indentation component on each selected paragraph, including inherited
/// information and existing page-break, alignment and named-style metadata.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParagraphMetricsPatch {
    /// Spacing before the paragraph in twips; unchanged when absent.
    pub spacing_before: Option<Twips>,
    /// Spacing after the paragraph in twips; unchanged when absent.
    pub spacing_after: Option<Twips>,
    /// Line spacing, expressed as one of the native, existing rules.
    pub line_spacing: Option<LineSpacing>,
    /// Left paragraph indent in twips.
    pub indent_left: Option<Twips>,
    /// Right paragraph indent in twips.
    pub indent_right: Option<Twips>,
    /// First-line offset relative to the remaining lines; negative hangs.
    pub first_line: Option<Twips>,
}

impl ParagraphMetricsPatch {
    pub(crate) fn apply(&self, original: &ParagraphFormat) -> ParagraphFormat {
        let mut result = original.clone();
        if let Some(before) = self.spacing_before {
            result.spacing_before = Some(before);
        }
        if let Some(after) = self.spacing_after {
            result.spacing_after = Some(after);
        }
        if let Some(spacing) = self.line_spacing {
            result.line_spacing = Some(spacing);
        }
        if self.indent_left.is_some() || self.indent_right.is_some() || self.first_line.is_some() {
            let indentation = result.indentation.get_or_insert_default();
            if let Some(left) = self.indent_left {
                indentation.left = Some(left);
            }
            if let Some(right) = self.indent_right {
                indentation.right = Some(right);
            }
            if let Some(first) = self.first_line {
                indentation.first_line = Some(first);
            }
        }
        result
    }

    pub(crate) fn validate(&self) -> Result<(), ValidationError> {
        self.apply(&ParagraphFormat::default())
            .validate("paragraph_metrics")
    }

    /// Whether the patch requests any edits.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.spacing_before.is_none()
            && self.spacing_after.is_none()
            && self.line_spacing.is_none()
            && self.indent_left.is_none()
            && self.indent_right.is_none()
            && self.first_line.is_none()
    }
}
