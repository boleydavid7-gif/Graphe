//! The explicit ODF property subset representable by the shared model.

use super::{FO, STYLE, SVG, diagnostic};
use crate::{FormatDiagnostic, FormatError, ImportNotices};
use office_core::{
    Alignment, CharacterFormat, HalfPoints, Indentation, LineSpacing, PageMargins, PageOrientation,
    PageSize, ParagraphFormat, SectionProperties, TextColor, Twips, Underline,
};
use roxmltree::Node;

/// Deterministic decimal unit conversion, rounded to the nearest model twip.
pub(super) fn length(value: &str) -> Result<Twips, FormatError> {
    let (number, numerator, denominator) = if let Some(n) = value.strip_suffix("in") {
        (n, 1440_i128, 1_i128)
    } else if let Some(n) = value.strip_suffix("cm") {
        (n, 72000, 127)
    } else if let Some(n) = value.strip_suffix("mm") {
        (n, 7200, 127)
    } else if let Some(n) = value.strip_suffix("pt") {
        (n, 20, 1)
    } else if let Some(n) = value.strip_suffix("pc") {
        (n, 240, 1)
    } else {
        return Err(super::package::invalid(
            "unsupported or missing physical length unit",
        ));
    };
    let (n, d) = decimal(number)?;
    let divisor = d * denominator;
    let scaled = n
        .checked_mul(numerator)
        .ok_or_else(|| super::package::invalid("length overflow"))?;
    let rounded = if scaled < 0 {
        -((-scaled + divisor / 2) / divisor)
    } else {
        (scaled + divisor / 2) / divisor
    };
    Ok(Twips(
        i32::try_from(rounded).map_err(super::package::invalid)?,
    ))
}

fn decimal(value: &str) -> Result<(i128, i128), FormatError> {
    let (negative, value) = value
        .strip_prefix('-')
        .map_or((false, value), |v| (true, v));
    let value = value.strip_prefix('+').unwrap_or(value);
    let mut split = value.split('.');
    let whole = split.next().unwrap_or("");
    let fraction = split.next().unwrap_or("");
    if split.next().is_some()
        || whole.len() + fraction.len() > 15
        || fraction.len() > 9
        || (whole.is_empty() && fraction.is_empty())
        || !whole
            .bytes()
            .chain(fraction.bytes())
            .all(|b| b.is_ascii_digit())
    {
        return Err(super::package::invalid("invalid bounded decimal length"));
    }
    let divisor = 10_i128.pow(fraction.len() as u32);
    let integer = format!("{whole}{fraction}")
        .parse::<i128>()
        .map_err(super::package::invalid)?;
    Ok((if negative { -integer } else { integer }, divisor))
}

fn unsupported(
    report: &mut Vec<FormatDiagnostic>,
    part: &str,
    node: Node<'_, '_>,
    attribute: &str,
) {
    diagnostic(
        report,
        "unsupported_property",
        part,
        format!("{}/@{attribute}", node.tag_name().name()),
        "This ODF property has no complete shared-model representation.",
    );
}

pub(super) fn character(
    node: Node<'_, '_>,
    part: &str,
    fonts: &std::collections::BTreeMap<String, String>,
    report: &mut Vec<FormatDiagnostic>,
    notices: &mut ImportNotices,
) -> Result<CharacterFormat, FormatError> {
    super::container_text(node)?;
    let mut result = CharacterFormat::default();
    let mut named_family = None;
    let mut explicit_family = None;
    let mut automatic_color = false;
    let mut underline_style = None;
    let mut underline_type = None;
    for attribute in node.attributes() {
        let value = attribute.value();
        match (attribute.namespace(), attribute.name()) {
            (Some(FO), "font-weight") => {
                result.bold = match value {
                    "normal" | "400" => Some(false),
                    "bold" | "700" => Some(true),
                    _ => {
                        unsupported(report, part, node, attribute.name());
                        None
                    }
                }
            }
            (Some(FO), "font-style") => {
                result.italic = match value {
                    "normal" => Some(false),
                    "italic" => Some(true),
                    _ => {
                        unsupported(report, part, node, attribute.name());
                        None
                    }
                }
            }
            (Some(FO), "font-size") => {
                let twips = length(value)?;
                if twips.0 <= 0 {
                    return Err(super::package::invalid("font size must be positive"));
                }
                if twips.0 % 10 != 0 {
                    unsupported(report, part, node, "font-size (half-point precision)");
                } else {
                    result.font_size = Some(HalfPoints((twips.0 / 10) as u32));
                }
            }
            (Some(FO), "font-family") => explicit_family = Some(font_family(value)?),
            (Some(STYLE), "font-name-asian" | "font-name-complex")
                if !fonts.contains_key(value) =>
            {
                return Err(super::package::invalid(
                    "script font-name does not resolve to a font face",
                ));
            }
            (Some(STYLE), "font-name") => {
                named_family = Some(fonts.get(value).cloned().ok_or_else(|| {
                    super::package::invalid("font-name does not resolve to a font face")
                })?);
            }
            (Some(FO), "color") => {
                let rgb = value
                    .strip_prefix('#')
                    .filter(|s| s.len() == 6)
                    .ok_or_else(|| super::package::invalid("expected six-digit sRGB color"))?;
                let packed = u32::from_str_radix(rgb, 16).map_err(super::package::invalid)?;
                result.color = Some(TextColor::Rgb {
                    red: (packed >> 16) as u8,
                    green: (packed >> 8) as u8,
                    blue: packed as u8,
                });
            }
            (Some(STYLE), "use-window-font-color") if value == "true" => {
                automatic_color = true;
            }
            (Some(STYLE), "use-window-font-color") if value == "false" => {}
            (Some(STYLE), "text-underline-style") => underline_style = Some(value),
            (Some(STYLE), "text-underline-type") => underline_type = Some(value),
            (Some(STYLE), "text-underline-width") if value == "auto" => {}
            (Some(STYLE), "text-underline-color") if value == "font-color" => {}
            _ if super::compatibility::property(node, attribute, notices, part) => {}
            _ => unsupported(report, part, node, attribute.name()),
        }
    }
    if named_family
        .as_ref()
        .zip(explicit_family.as_ref())
        .is_some_and(|(a, b)| a != b)
    {
        unsupported(report, part, node, "conflicting font-family and font-name");
    }
    result.font_family = named_family.or(explicit_family);
    if automatic_color {
        result.color = Some(TextColor::Auto {});
    }
    if let Some(value) = underline_style {
        result.underline = match (value, underline_type.unwrap_or("single")) {
            ("none", _) => Some(Underline::None),
            ("solid", "single") => Some(Underline::Single),
            ("solid", "double") => Some(Underline::Double),
            _ => {
                unsupported(report, part, node, "underline");
                None
            }
        };
    } else if underline_type.is_some() {
        unsupported(report, part, node, "underline-type without style");
    }
    for child in node.children().filter(Node::is_element) {
        if super::compatibility::property_child(child) {
            continue;
        }
        diagnostic(
            report,
            "unsupported_element",
            part,
            child.tag_name().name(),
            "Nested character properties are unsupported.",
        );
    }
    Ok(result)
}

pub(super) fn paragraph(
    node: Node<'_, '_>,
    part: &str,
    report: &mut Vec<FormatDiagnostic>,
    notices: &mut ImportNotices,
) -> Result<ParagraphFormat, FormatError> {
    super::container_text(node)?;
    let mut result = ParagraphFormat::default();
    let mut indentation = Indentation::default();
    for attribute in node.attributes() {
        let value = attribute.value();
        match (attribute.namespace(), attribute.name()) {
            (Some(FO), "text-align") => {
                result.alignment = match value {
                    "left" => Some(Alignment::Left),
                    "right" => Some(Alignment::Right),
                    "center" => Some(Alignment::Center),
                    "justify" => Some(Alignment::Justify),
                    _ => {
                        unsupported(report, part, node, attribute.name());
                        None
                    }
                }
            }
            (Some(FO), "margin-top") => result.spacing_before = Some(length(value)?),
            (Some(FO), "margin-bottom") => result.spacing_after = Some(length(value)?),
            (Some(FO), "margin-left") => indentation.left = Some(length(value)?),
            (Some(FO), "margin-right") => indentation.right = Some(length(value)?),
            (Some(FO), "text-indent") => indentation.first_line = Some(length(value)?),
            (Some(FO), "line-height") => {
                if value == "normal" {
                    result.line_spacing = Some(LineSpacing::Multiple { value: 240 });
                } else if let Some(percent) = value.strip_suffix('%') {
                    let (n, d) = decimal(percent)?;
                    let value = u32::try_from((n * 240 + d * 50) / (d * 100))
                        .map_err(super::package::invalid)?;
                    result.line_spacing = Some(LineSpacing::Multiple { value });
                } else {
                    result.line_spacing = Some(LineSpacing::Exact {
                        height: length(value)?,
                    });
                }
            }
            (Some(STYLE), "line-height-at-least") => {
                result.line_spacing = Some(LineSpacing::AtLeast {
                    height: length(value)?,
                })
            }
            (Some(FO), "break-before") => {
                result.page_break_before = match value {
                    "page" => Some(true),
                    "auto" => Some(false),
                    _ => {
                        unsupported(report, part, node, attribute.name());
                        None
                    }
                }
            }
            (Some(FO), "keep-together") => {
                result.keep_together = match value {
                    "always" => Some(true),
                    "auto" => Some(false),
                    _ => { unsupported(report, part, node, attribute.name()); None }
                }
            }
            (Some(FO), "keep-with-next") => {
                result.keep_with_next = match value {
                    "always" => Some(true),
                    "auto" => Some(false),
                    _ => { unsupported(report, part, node, attribute.name()); None }
                }
            }
            _ if super::compatibility::property(node, attribute, notices, part) => {}
            _ => unsupported(report, part, node, attribute.name()),
        }
    }
    if node.attribute((FO, "line-height")).is_some()
        && node.attribute((STYLE, "line-height-at-least")).is_some()
    {
        unsupported(report, part, node, "combined line-height constraints");
    }
    if indentation != Indentation::default() {
        result.indentation = Some(indentation);
    }
    for child in node.children().filter(Node::is_element) {
        if super::compatibility::property_child(child) {
            continue;
        }
        diagnostic(
            report,
            "unsupported_element",
            part,
            child.tag_name().name(),
            "Nested paragraph properties are unsupported.",
        );
    }
    Ok(result)
}

pub(super) fn geometry(
    node: Node<'_, '_>,
    part: &str,
    report: &mut Vec<FormatDiagnostic>,
    notices: &mut ImportNotices,
) -> Result<SectionProperties, FormatError> {
    super::container_text(node)?;
    let mut width = None;
    let mut height = None;
    let mut orientation = None;
    let mut margins = PageMargins {
        top: Twips(0),
        right: Twips(0),
        bottom: Twips(0),
        left: Twips(0),
        header: Twips(0),
        footer: Twips(0),
        gutter: Twips(0),
    };
    let mut has_margins = false;
    if let Some(value) = node.attribute((FO, "margin")) {
        let value = length(value)?;
        margins.top = value;
        margins.right = value;
        margins.bottom = value;
        margins.left = value;
        has_margins = true;
    }
    for attribute in node.attributes() {
        let value = attribute.value();
        match (attribute.namespace(), attribute.name()) {
            (Some(FO), "page-width") => width = Some(length(value)?),
            (Some(FO), "page-height") => height = Some(length(value)?),
            (Some(STYLE), "print-orientation") => {
                orientation = match value {
                    "portrait" => Some(PageOrientation::Portrait),
                    "landscape" => Some(PageOrientation::Landscape),
                    _ => return Err(super::package::invalid("invalid page orientation")),
                }
            }
            (Some(FO), "margin") => {}
            (Some(FO), "margin-top") => {
                margins.top = length(value)?;
                has_margins = true;
            }
            (Some(FO), "margin-right") => {
                margins.right = length(value)?;
                has_margins = true;
            }
            (Some(FO), "margin-bottom") => {
                margins.bottom = length(value)?;
                has_margins = true;
            }
            (Some(FO), "margin-left") => {
                margins.left = length(value)?;
                has_margins = true;
            }
            _ if super::compatibility::property(node, attribute, notices, part) => {}
            _ => unsupported(report, part, node, attribute.name()),
        }
    }
    for child in node.children().filter(Node::is_element) {
        if super::compatibility::property_child(child) {
            continue;
        }
        diagnostic(
            report,
            "unsupported_element",
            part,
            child.tag_name().name(),
            "Page-layout child content is unsupported.",
        );
    }
    let page_size = match (width, height) {
        (Some(width), Some(height)) => Some(PageSize {
            width,
            height,
            orientation,
        }),
        (None, None) if orientation.is_none() => None,
        _ => {
            return Err(super::package::invalid(
                "page size requires width and height",
            ));
        }
    };
    Ok(SectionProperties {
        page_size,
        page_margins: has_margins.then_some(margins),
    })
}

pub(super) fn font_family(value: &str) -> Result<String, FormatError> {
    let value = value.trim();
    let value = value
        .strip_prefix('\'')
        .and_then(|v| v.strip_suffix('\''))
        .or_else(|| value.strip_prefix('"').and_then(|v| v.strip_suffix('"')))
        .unwrap_or(value);
    if value.is_empty() || value.contains([',', '\'', '"', '\\']) {
        return Err(super::package::invalid(
            "unsupported font family list or quoting",
        ));
    }
    Ok(value.to_owned())
}

pub(super) fn font_face(node: Node<'_, '_>) -> Result<(String, String), FormatError> {
    let name = node
        .attribute((STYLE, "name"))
        .ok_or_else(|| super::package::invalid("font face has no name"))?;
    let family = node
        .attribute((SVG, "font-family"))
        .ok_or_else(|| super::package::invalid("font face has no family"))?;
    Ok((name.to_owned(), font_family(family)?))
}

pub(super) fn overlay_paragraph(base: &mut ParagraphFormat, direct: &ParagraphFormat) {
    macro_rules! field {
        ($name:ident) => {
            if direct.$name.is_some() {
                base.$name = direct.$name;
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
    if let Some(value) = &direct.indentation {
        let indent = base.indentation.get_or_insert_default();
        if value.left.is_some() {
            indent.left = value.left;
        }
        if value.right.is_some() {
            indent.right = value.right;
        }
        if value.first_line.is_some() {
            indent.first_line = value.first_line;
        }
    }
}

pub(super) fn xml_escape(value: &str) -> Result<String, FormatError> {
    let mut result = String::new();
    for character in value.chars() {
        match character {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '"' => result.push_str("&quot;"),
            '\'' => result.push_str("&apos;"),
            '\t' => result.push_str("&#9;"),
            '\n' => result.push_str("&#10;"),
            '\r' => result.push_str("&#13;"),
            '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}' => {
                result.push(character)
            }
            _ => {
                return Err(FormatError::new(
                    "unsupported_odt_export",
                    "ODT cannot encode an illegal XML scalar.",
                ));
            }
        }
    }
    Ok(result)
}

pub(super) fn points(value: Twips) -> String {
    let absolute = i64::from(value.0).abs();
    let sign = if value.0 < 0 { "-" } else { "" };
    format!("{sign}{}.{:02}pt", absolute / 20, (absolute % 20) * 5)
}

pub(super) fn character_xml(format: &CharacterFormat) -> Result<String, FormatError> {
    let mut attributes = String::new();
    if let Some(v) = format.bold {
        attributes.push_str(&format!(
            " fo:font-weight=\"{}\"",
            if v { "bold" } else { "normal" }
        ));
    }
    if let Some(v) = format.italic {
        attributes.push_str(&format!(
            " fo:font-style=\"{}\"",
            if v { "italic" } else { "normal" }
        ));
    }
    if let Some(value) = format.underline {
        // ODF 1.3 Part 3 §20.391: underline-type is omitted when style is none.
        // LibreOffice treats an explicit single type as active even with none.
        attributes.push_str(match value {
            Underline::None => " style:text-underline-style=\"none\"",
            Underline::Single => {
                " style:text-underline-style=\"solid\" style:text-underline-type=\"single\""
            }
            Underline::Double => {
                " style:text-underline-style=\"solid\" style:text-underline-type=\"double\""
            }
        });
    }
    if let Some(v) = &format.font_family {
        font_family(v)?;
        attributes.push_str(&format!(" fo:font-family=\"{}\"", xml_escape(v)?));
    }
    if let Some(v) = format.font_size {
        attributes.push_str(&format!(
            " fo:font-size=\"{}.{:01}pt\"",
            v.0 / 2,
            (v.0 % 2) * 5
        ));
    }
    if let Some(v) = format.color {
        match v {
            TextColor::Auto {} => attributes.push_str(" style:use-window-font-color=\"true\""),
            TextColor::Rgb { red, green, blue } => {
                attributes.push_str(&format!(" fo:color=\"#{red:02x}{green:02x}{blue:02x}\""))
            }
        }
    }
    Ok(format!("<style:text-properties{attributes}/>"))
}

pub(super) fn paragraph_xml(format: &ParagraphFormat) -> String {
    let mut attributes = String::new();
    if let Some(v) = format.alignment {
        attributes.push_str(&format!(
            " fo:text-align=\"{}\"",
            match v {
                Alignment::Left => "left",
                Alignment::Center => "center",
                Alignment::Right => "right",
                Alignment::Justify => "justify",
            }
        ));
    }
    if let Some(v) = format.spacing_before {
        attributes.push_str(&format!(" fo:margin-top=\"{}\"", points(v)));
    }
    if let Some(v) = format.spacing_after {
        attributes.push_str(&format!(" fo:margin-bottom=\"{}\"", points(v)));
    }
    if let Some(v) = format.line_spacing {
        match v {
            LineSpacing::Multiple { value } => attributes.push_str(&format!(
                " fo:line-height=\"{}.{:06}%\"",
                u64::from(value) * 100 / 240,
                (u64::from(value) * 100 % 240) * 1_000_000 / 240
            )),
            LineSpacing::Exact { height } => {
                attributes.push_str(&format!(" fo:line-height=\"{}\"", points(height)))
            }
            LineSpacing::AtLeast { height } => attributes.push_str(&format!(
                " style:line-height-at-least=\"{}\"",
                points(height)
            )),
        }
    }
    if let Some(v) = &format.indentation {
        if let Some(v) = v.left {
            attributes.push_str(&format!(" fo:margin-left=\"{}\"", points(v)));
        }
        if let Some(v) = v.right {
            attributes.push_str(&format!(" fo:margin-right=\"{}\"", points(v)));
        }
        if let Some(v) = v.first_line {
            attributes.push_str(&format!(" fo:text-indent=\"{}\"", points(v)));
        }
    }
    if let Some(v) = format.page_break_before {
        attributes.push_str(&format!(
            " fo:break-before=\"{}\"",
            if v { "page" } else { "auto" }
        ));
    }
    if let Some(v) = format.keep_together {
        attributes.push_str(if v { " fo:keep-together=\"always\"" } else { " fo:keep-together=\"auto\"" });
    }
    if let Some(v) = format.keep_with_next {
        attributes.push_str(if v { " fo:keep-with-next=\"always\"" } else { " fo:keep-with-next=\"auto\"" });
    }
    format!("<style:paragraph-properties{attributes}/>")
}
