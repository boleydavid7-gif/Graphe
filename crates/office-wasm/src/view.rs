use crate::wire::{CharacterView, ParagraphFormatView, ParagraphView, RunView};
use office_core::{
    Alignment, Block, CharacterFormat, Document, LineSpacing, Paragraph, ParagraphFormat,
    TextColor, Underline,
};

// Shared Rust inheritance policy. Browser defaults below remain presentation
// values; page geometry and font measurement come from office-layout.
pub(crate) fn overlay_character(base: &mut CharacterFormat, direct: &CharacterFormat) {
    office_layout::overlay_character(base, direct);
}

pub(crate) fn inherited(
    document: &Document,
    paragraph: &Paragraph,
) -> (CharacterFormat, ParagraphFormat) {
    let resolved = office_layout::resolve_paragraph_style(document, paragraph);
    (resolved.character, resolved.paragraph)
}

pub(crate) fn character_view(format: &CharacterFormat) -> CharacterView {
    CharacterView {
        bold: format.bold.unwrap_or(false),
        italic: format.italic.unwrap_or(false),
        underline: match format.underline {
            Some(Underline::Single) => "single",
            Some(Underline::Double) => "double",
            None if format.hyperlink.is_some() => "single",
            _ => "none",
        },
        font_family: format.font_family.clone().unwrap_or_else(|| "Arimo".into()),
        font_size_pt: format.font_size.map_or(12., |size| f64::from(size.0) / 2.),
        hyperlink: format.hyperlink.clone(),
        color: match format.color {
            Some(TextColor::Rgb { red, green, blue }) => format!("#{red:02x}{green:02x}{blue:02x}"),
            _ if format.hyperlink.is_some() => "#1762a6".into(),
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
                style_id: paragraph.style().map(|id| id.as_str().into()),
                runs,
                list: paragraph.list(),
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
