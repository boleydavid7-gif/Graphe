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
        }
    }

    pub(crate) fn validate(&self) -> Result<(), ValidationError> {
        self.apply(&ParagraphFormat::default())
            .validate("paragraph_patch")
    }
}
