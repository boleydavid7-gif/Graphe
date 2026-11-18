use office_core::{CharacterFormat, Document, Paragraph, ParagraphFormat};

/// Inherited properties with paragraph direct overrides and no layout defaults.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedParagraphStyle {
    /// Named paragraph-style character defaults, before run overrides.
    pub character: CharacterFormat,
    /// Named style plus direct paragraph properties.
    pub paragraph: ParagraphFormat,
}

/// Applies specified character overrides, retaining unspecified inherited values.
pub fn overlay_character(base: &mut CharacterFormat, direct: &CharacterFormat) {
    macro_rules! field {
        ($name:ident) => {
            if direct.$name.is_some() {
                base.$name = direct.$name.clone();
            }
        };
    }
    field!(bold);
    field!(italic);
    field!(underline);
    field!(font_family);
    field!(font_size);
    field!(color);
    field!(hyperlink);
}

fn overlay_paragraph(base: &mut ParagraphFormat, direct: &ParagraphFormat) {
    macro_rules! field {
        ($name:ident) => {
            if direct.$name.is_some() {
                base.$name = direct.$name.clone();
            }
        };
    }
    field!(alignment);
    field!(spacing_before);
    field!(spacing_after);
    field!(line_spacing);
    field!(page_break_before);
    field!(keep_together);
    field!(keep_with_next);
    if let Some(override_indent) = &direct.indentation {
        let indent = base.indentation.get_or_insert_default();
        if override_indent.left.is_some() {
            indent.left = override_indent.left;
        }
        if override_indent.right.is_some() {
            indent.right = override_indent.right;
        }
        if override_indent.first_line.is_some() {
            indent.first_line = override_indent.first_line;
        }
    }
}

/// Resolves the existing absolute native style semantics without font defaults.
///
/// Documents are validated at construction/loading. Defensive termination also
/// keeps this helper total if a paragraph not belonging to the document is passed.
#[must_use]
pub fn resolve_paragraph_style(
    document: &Document,
    paragraph: &Paragraph,
) -> ResolvedParagraphStyle {
    let mut chain = Vec::new();
    let mut parent = paragraph.style();
    while let Some(id) = parent {
        let Some(style) = document.styles().iter().find(|style| style.id == *id) else {
            break;
        };
        if chain.len() >= document.styles().len() {
            break;
        }
        chain.push(style);
        parent = style.based_on.as_ref();
    }
    let mut character = CharacterFormat::default();
    let mut paragraph_format = ParagraphFormat::default();
    for style in chain.into_iter().rev() {
        overlay_character(&mut character, &style.character_format);
        overlay_paragraph(&mut paragraph_format, &style.paragraph_format);
    }
    overlay_paragraph(&mut paragraph_format, paragraph.format());
    ResolvedParagraphStyle {
        character,
        paragraph: paragraph_format,
    }
}
