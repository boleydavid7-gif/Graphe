//! Deterministic RTF writer for the same explicit semantic profile.

use std::{collections::BTreeMap, fmt::Write};

use office_core::{
    Alignment, Block, CharacterFormat, Document, LineSpacing, PageOrientation, ParagraphFormat,
    TextColor, Underline,
};

use crate::{FormatError, MAX_OUTPUT_BYTES};

fn unsupported(message: impl Into<String>) -> FormatError {
    FormatError::new("unsupported_rtf_export", message)
}

fn bounded(output: &str) -> Result<(), FormatError> {
    if output.len() > MAX_OUTPUT_BYTES {
        Err(unsupported("RTF output exceeds the 32 MiB limit."))
    } else {
        Ok(())
    }
}

fn escaped(output: &mut String, text: &str) -> Result<(), FormatError> {
    for scalar in text.chars() {
        match scalar {
            '\\' | '{' | '}' => {
                output.push('\\');
                output.push(scalar);
            }
            '\n' => output.push_str("\\line "),
            '\t' => output.push_str("\\tab "),
            // Semicolons delimit table names; escaping them as Unicode also
            // keeps font/style/title labels unambiguous to external readers.
            ' '..='~' if scalar != ';' => output.push(scalar),
            _ if scalar.is_control() => {
                return Err(unsupported(
                    "RTF cannot safely represent this control character; keep the native Writer file.",
                ));
            }
            _ => {
                for unit in scalar.encode_utf16(&mut [0; 2]).iter() {
                    write!(output, "\\u{}?", *unit as i16)
                        .expect("writing into String cannot fail");
                }
            }
        }
        if output.len() > MAX_OUTPUT_BYTES {
            return Err(unsupported("RTF output exceeds the 32 MiB limit."));
        }
    }
    Ok(())
}

fn escaped_label(output: &mut String, text: &str) -> Result<(), FormatError> {
    if text.chars().any(char::is_control) {
        return Err(unsupported(
            "RTF font, style and title labels cannot contain control characters.",
        ));
    }
    escaped(output, text)
}

struct Tables {
    fonts: Vec<String>,
    colors: Vec<TextColor>,
    styles: BTreeMap<String, i32>,
    character_defaults: BTreeMap<String, CharacterFormat>,
    paragraph_defaults: BTreeMap<String, ParagraphFormat>,
}

impl Tables {
    fn new(document: &Document) -> Result<Self, FormatError> {
        let mut result = Self {
            fonts: vec!["Arimo".into()],
            colors: Vec::new(),
            styles: BTreeMap::new(),
            character_defaults: BTreeMap::new(),
            paragraph_defaults: BTreeMap::new(),
        };
        for (number, style) in document.styles().iter().enumerate() {
            result.styles.insert(
                style.id.as_str().into(),
                i32::try_from(number).map_err(|_| unsupported("Too many RTF styles."))?,
            );
            result.collect(&style.character_format)?;
        }
        for section in document.sections() {
            for paragraph in section.paragraphs() {
                for run in paragraph.runs() {
                    result.collect(&run.format)?;
                }
            }
        }
        let by_id: BTreeMap<&str, &office_core::NamedStyle> = document
            .styles()
            .iter()
            .map(|style| (style.id.as_str(), style))
            .collect();
        for style in document.styles() {
            let mut character = CharacterFormat::default();
            let mut paragraph = ParagraphFormat::default();
            let mut current = Some(style);
            let mut depth = 0;
            while let Some(value) = current {
                if depth >= super::MAX_DEPTH {
                    return Err(unsupported(
                        "RTF style inheritance exceeds the 64-level profile.",
                    ));
                }
                merge_character(&mut character, &value.character_format);
                merge_paragraph(&mut paragraph, &value.paragraph_format);
                current = value
                    .based_on
                    .as_ref()
                    .and_then(|id| by_id.get(id.as_str()).copied());
                depth += 1;
            }
            result
                .character_defaults
                .insert(style.id.as_str().into(), character);
            result
                .paragraph_defaults
                .insert(style.id.as_str().into(), paragraph);
        }
        Ok(result)
    }

    fn collect(&mut self, format: &CharacterFormat) -> Result<(), FormatError> {
        if let Some(font) = &format.font_family
            && !self.fonts.contains(font)
        {
            self.fonts.push(font.clone());
        }
        if let Some(color @ TextColor::Rgb { .. }) = format.color
            && !self.colors.contains(&color)
        {
            self.colors.push(color);
        }
        if self.fonts.len() > super::MAX_TABLE_ENTRIES
            || self.colors.len() > super::MAX_TABLE_ENTRIES
            || self.styles.len() > super::MAX_TABLE_ENTRIES
        {
            return Err(unsupported(
                "RTF font, color or style table exceeds its resource limit.",
            ));
        }
        Ok(())
    }

    fn character(&self, output: &mut String, format: &CharacterFormat) -> Result<(), FormatError> {
        for (name, value) in [("b", format.bold), ("i", format.italic)] {
            if let Some(value) = value {
                write!(output, "\\{name}{}", i32::from(value)).expect("String write");
            }
        }
        if let Some(value) = format.underline {
            output.push_str(match value {
                Underline::None => "\\ulnone",
                Underline::Single => "\\ul",
                Underline::Double => "\\uldb",
            });
        }
        if let Some(name) = &format.font_family {
            let index = self
                .fonts
                .iter()
                .position(|font| font == name)
                .ok_or_else(|| unsupported("RTF font table is inconsistent."))?;
            write!(output, "\\f{index}").expect("String write");
        }
        if let Some(size) = format.font_size {
            let size = i32::try_from(size.0)
                .map_err(|_| unsupported("RTF font size exceeds the supported numeric range."))?;
            write!(output, "\\fs{size}").expect("String write");
        }
        if let Some(color) = format.color {
            let index = match color {
                TextColor::Auto {} => 0,
                _ => {
                    self.colors
                        .iter()
                        .position(|value| value == &color)
                        .ok_or_else(|| unsupported("RTF color table is inconsistent."))?
                        + 1
                }
            };
            write!(output, "\\cf{index}").expect("String write");
        }
        Ok(())
    }
}

fn paragraph(output: &mut String, format: &ParagraphFormat) -> Result<(), FormatError> {
    if let Some(alignment) = format.alignment {
        output.push_str(match alignment {
            Alignment::Left => "\\ql",
            Alignment::Center => "\\qc",
            Alignment::Right => "\\qr",
            Alignment::Justify => "\\qj",
        });
    }
    for (name, value) in [("sb", format.spacing_before), ("sa", format.spacing_after)] {
        if let Some(value) = value {
            write!(output, "\\{name}{}", value.0).expect("String write");
        }
    }
    if let Some(indentation) = &format.indentation {
        for (name, value) in [
            ("li", indentation.left),
            ("ri", indentation.right),
            ("fi", indentation.first_line),
        ] {
            if let Some(value) = value {
                write!(output, "\\{name}{}", value.0).expect("String write");
            }
        }
    }
    if let Some(spacing) = format.line_spacing {
        let (height, multiple) = match spacing {
            LineSpacing::Multiple { value } => (
                i32::try_from(value).map_err(|_| {
                    unsupported("RTF line-spacing multiple exceeds the numeric range.")
                })?,
                1,
            ),
            LineSpacing::Exact { height } => (-height.0, 0),
            LineSpacing::AtLeast { height } => (height.0, 0),
        };
        write!(output, "\\sl{height}\\slmult{multiple}").expect("String write");
    }
    if let Some(value) = format.page_break_before {
        write!(output, "\\pagebb{}", i32::from(value)).expect("String write");
    }
    if let Some(value) = format.keep_together {
        write!(output, "\\keep{}", i32::from(value)).expect("String write");
    }
    if let Some(value) = format.keep_with_next {
        write!(output, "\\keepn{}", i32::from(value)).expect("String write");
    }
    Ok(())
}

fn merge_character(result: &mut CharacterFormat, parent: &CharacterFormat) {
    result.bold = result.bold.or(parent.bold);
    result.italic = result.italic.or(parent.italic);
    result.underline = result.underline.or(parent.underline);
    result.font_size = result.font_size.or(parent.font_size);
    result.color = result.color.or(parent.color);
    if result.font_family.is_none() {
        result.font_family = parent.font_family.clone();
    }
}

fn merge_paragraph(result: &mut ParagraphFormat, parent: &ParagraphFormat) {
    result.alignment = result.alignment.or(parent.alignment);
    result.spacing_before = result.spacing_before.or(parent.spacing_before);
    result.spacing_after = result.spacing_after.or(parent.spacing_after);
    result.line_spacing = result.line_spacing.or(parent.line_spacing);
    result.page_break_before = result.page_break_before.or(parent.page_break_before);
    if let Some(parent) = &parent.indentation {
        let indent = result.indentation.get_or_insert_with(Default::default);
        indent.left = indent.left.or(parent.left);
        indent.right = indent.right.or(parent.right);
        indent.first_line = indent.first_line.or(parent.first_line);
    }
}

fn inherited_paragraph(
    tables: &Tables,
    style: Option<&office_core::Id>,
    format: &ParagraphFormat,
) -> ParagraphFormat {
    let mut result = format.clone();
    if let Some(parent) = style.and_then(|id| tables.paragraph_defaults.get(id.as_str())) {
        merge_paragraph(&mut result, parent);
    }
    result.spacing_after.get_or_insert(office_core::Twips(160));
    result
        .line_spacing
        .get_or_insert(LineSpacing::Multiple { value: 360 });
    result
}

fn inherited_character(
    tables: &Tables,
    style: Option<&office_core::Id>,
    format: &CharacterFormat,
) -> CharacterFormat {
    let mut result = format.clone();
    if let Some(parent) = style.and_then(|id| tables.character_defaults.get(id.as_str())) {
        merge_character(&mut result, parent);
    }
    result
}

pub(crate) fn export_document(document: &Document) -> Result<Vec<u8>, FormatError> {
    document.validate()?;
    if document
        .sections()
        .iter()
        .flat_map(|section| section.blocks())
        .any(|block| matches!(block, Block::Table(_) | Block::Image(_)))
    {
        return Err(unsupported(
            "RTF table and image export is not supported yet. Keep a native Graphe, DOCX, ODT or PDF copy.",
        ));
    }
    let tables = Tables::new(document)?;
    let mut output = String::from("{\\rtf1\\ansi\\ansicpg1252\\uc1\\deff0\n{\\fonttbl");
    for (index, font) in tables.fonts.iter().enumerate() {
        write!(output, "{{\\f{index}\\fnil\\fcharset0 ").expect("String write");
        escaped_label(&mut output, font)?;
        output.push_str(";}");
    }
    output.push_str("}\n{\\colortbl;");
    for color in &tables.colors {
        if let TextColor::Rgb { red, green, blue } = color {
            write!(output, "\\red{red}\\green{green}\\blue{blue};").expect("String write");
        }
    }
    output.push_str("}\n");
    if !document.styles().is_empty() {
        output.push_str("{\\stylesheet");
        for style in document.styles() {
            let index = tables.styles[style.id.as_str()];
            write!(output, "{{\\s{index}").expect("String write");
            if let Some(parent) = &style.based_on {
                write!(output, "\\sbasedon{}", tables.styles[parent.as_str()])
                    .expect("String write");
            }
            tables.character(&mut output, &style.character_format)?;
            paragraph(&mut output, &style.paragraph_format)?;
            output.push(' ');
            escaped_label(&mut output, &style.name)?;
            output.push_str(";}");
        }
        output.push_str("}\n");
    }
    if !document.metadata().title.is_empty() {
        output.push_str("{\\info{\\title ");
        escaped_label(&mut output, &document.metadata().title)?;
        output.push_str("}}\n");
    }
    if document.sections().is_empty() {
        output.push_str("{\\pard\\sa160\\sl360\\slmult1 \\par}\n");
    }
    for (section_index, section) in document.sections().iter().enumerate() {
        if section_index > 0 {
            output.push_str("\\sect\n");
        }
        output.push_str("\\sectd");
        if let Some(properties) = section.properties() {
            if let Some(size) = properties.page_size {
                write!(output, "\\pgwsxn{}\\pghsxn{}", size.width.0, size.height.0)
                    .expect("String write");
                if let Some(orientation) = size.orientation {
                    write!(
                        output,
                        "\\lndscpsxn{}",
                        i32::from(orientation == PageOrientation::Landscape)
                    )
                    .expect("String write");
                }
            }
            if let Some(margins) = properties.page_margins {
                for (name, value) in [
                    ("margtsxn", margins.top),
                    ("margrsxn", margins.right),
                    ("margbsxn", margins.bottom),
                    ("marglsxn", margins.left),
                    ("headery", margins.header),
                    ("footery", margins.footer),
                    ("guttersxn", margins.gutter),
                ] {
                    write!(output, "\\{name}{}", value.0).expect("String write");
                }
            }
        }
        output.push('\n');
        if section.blocks().is_empty() {
            output.push_str("{\\pard\\sa160\\sl360\\slmult1 \\par}\n");
        }
        for block in section.blocks() {
            let Block::Paragraph(source) = block else {
                return Err(unsupported("RTF table export is not supported yet."));
            };
            output.push_str("{\\pard");
            if let Some(style) = source.style() {
                write!(output, "\\s{}", tables.styles[style.as_str()]).expect("String write");
            }
            // RTF requires materialized properties alongside \sN. Preserve
            // named definitions but encode resolved paragraph metrics as well.
            paragraph(
                &mut output,
                &inherited_paragraph(&tables, source.style(), source.format()),
            )?;
            output.push(' ');
            for run in source.runs() {
                output.push('{');
                let before = output.len();
                tables.character(
                    &mut output,
                    &inherited_character(&tables, source.style(), &run.format),
                )?;
                if output.len() > before {
                    output.push(' ');
                }
                escaped(&mut output, &run.text)?;
                output.push('}');
            }
            output.push_str("\\par}\n");
            bounded(&output)?;
        }
    }
    output.push('}');
    bounded(&output)?;
    Ok(output.into_bytes())
}
