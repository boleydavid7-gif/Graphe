use crate::wire::{CharacterView, ParagraphFormatView, ParagraphView, RunView};
use office_core::{
    Alignment, Block, CharacterFormat, Document, LineSpacing, Paragraph, ParagraphFormat,
    TextColor, Underline,
};

// Browser display policy only. Native properties remain exact and unresolved.
pub(crate) fn overlay_character(base: &mut CharacterFormat, direct: &CharacterFormat) {
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
    if let Some(direct_indent) = &direct.indentation {
        let indent = base.indentation.get_or_insert_default();
        if direct_indent.left.is_some() {
            indent.left = direct_indent.left;
        }
        if direct_indent.right.is_some() {
            indent.right = direct_indent.right;
        }
        if direct_indent.first_line.is_some() {
            indent.first_line = direct_indent.first_line;
        }
    }
}

pub(crate) fn inherited(
    document: &Document,
    paragraph: &Paragraph,
) -> (CharacterFormat, ParagraphFormat) {
    let mut chain = Vec::new();
    let mut parent = paragraph.style();
    while let Some(id) = parent {
        let style = document
            .styles()
            .iter()
            .find(|style| style.id == *id)
            .expect("validated style reference");
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
    (character, paragraph_format)
}

pub(crate) fn character_view(format: &CharacterFormat) -> CharacterView {
    CharacterView {
        bold: format.bold.unwrap_or(false),
        italic: format.italic.unwrap_or(false),
        underline: match format.underline {
            Some(Underline::Single) => "single",
            Some(Underline::Double) => "double",
            _ => "none",
        },
        font_family: format.font_family.clone().unwrap_or_else(|| "Arial".into()),
        font_size_pt: format.font_size.map_or(12., |size| f64::from(size.0) / 2.),
        color: match format.color {
            Some(TextColor::Rgb { red, green, blue }) => format!("#{red:02x}{green:02x}{blue:02x}"),
            _ => "#24211d".into(),
        },
    }
}

pub(crate) fn paragraphs(document: &Document) -> Vec<ParagraphView> {
    let mut result = Vec::new();
    for section in document.sections() {
        for block in section.blocks() {
            let Block::Paragraph(paragraph) = block;
            let (base, format) = inherited(document, paragraph);
            let indent = format.indentation.unwrap_or_default();
            let runs = paragraph
                .runs()
                .iter()
                .map(|run| {
                    let mut effective = base.clone();
                    overlay_character(&mut effective, &run.format);
                    RunView {
                        text: run.text.clone(),
                        format: character_view(&effective),
                    }
                })
                .collect();
            result.push(ParagraphView {
                id: paragraph.id().as_str().into(),
                section_id: section.id().as_str().into(),
                runs,
                format: ParagraphFormatView {
                    alignment: format.alignment.unwrap_or(Alignment::Left),
                    spacing_before_pt: format.spacing_before.map_or(0., |v| f64::from(v.0) / 20.),
                    spacing_after_pt: format.spacing_after.map_or(8., |v| f64::from(v.0) / 20.),
                    line_height: match format.line_spacing {
                        None => "1.5".into(),
                        Some(LineSpacing::Multiple { value }) => {
                            (f64::from(value) / 240.).to_string()
                        }
                        Some(LineSpacing::Exact { height } | LineSpacing::AtLeast { height }) => {
                            format!("{}pt", f64::from(height.0) / 20.)
                        }
                    },
                    indent_left_pt: indent.left.map_or(0., |v| f64::from(v.0) / 20.),
                    indent_right_pt: indent.right.map_or(0., |v| f64::from(v.0) / 20.),
                    first_line_pt: indent.first_line.map_or(0., |v| f64::from(v.0) / 20.),
                },
            });
        }
    }
    result
}
