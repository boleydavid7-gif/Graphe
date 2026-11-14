//! Explicit WordprocessingML property mappings for the editable subset.

use std::collections::BTreeSet;

use office_core::{
    Alignment, CharacterFormat, HalfPoints, Indentation, LineSpacing, PageMargins, PageOrientation,
    PageSize, ParagraphFormat, SectionProperties, TextColor, Twips, Underline,
};
use roxmltree::Node;

use crate::conversion::{
    CompatibilityIssueKind as Issue, CompatibilityReport, ConversionError, invalid,
};

pub(crate) struct ParsedParagraphProperties {
    pub format: ParagraphFormat,
    pub style_id: Option<String>,
    pub section: Option<SectionProperties>,
}

/// Returns raw on/off values; style-toggle interpretation belongs to the caller.
pub(crate) fn parse_character(
    node: Node<'_, '_>,
    namespace: &str,
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<CharacterFormat, ConversionError> {
    attributes(node, namespace, &[], part, report);
    let mut format = CharacterFormat::default();
    let mut bold_cs = None;
    let mut italic_cs = None;
    let mut size_cs = None;
    let mut seen = BTreeSet::new();
    for property in property_children(node, namespace, part, report)? {
        if !unique(property, &mut seen, part, report) {
            continue;
        }
        match property.tag_name().name() {
            "b" => format.bold = Some(on_off(property, namespace, part, report)?),
            "i" => format.italic = Some(on_off(property, namespace, part, report)?),
            "bCs" => bold_cs = Some(on_off(property, namespace, part, report)?),
            "iCs" => italic_cs = Some(on_off(property, namespace, part, report)?),
            "sz" | "szCs" => {
                attributes(property, namespace, &["val"], part, report);
                leaf(property, part, report)?;
                let value = required(property, namespace, "val", part)?;
                let size = positive_u32(property, value, part, report)?.map(HalfPoints);
                if property.tag_name().name() == "sz" {
                    format.font_size = size;
                } else {
                    size_cs = size;
                }
            }
            "rFonts" => format.font_family = fonts(property, namespace, part, report)?,
            "u" => {
                attributes(property, namespace, &["val"], part, report);
                leaf(property, part, report)?;
                let value = property.attribute((namespace, "val")).unwrap_or("single");
                format.underline = match value {
                    "none" => Some(Underline::None),
                    "single" => Some(Underline::Single),
                    "double" => Some(Underline::Double),
                    _ => {
                        unsupported_value(property, part, report, "underline treatment");
                        None
                    }
                };
            }
            "color" => {
                attributes(property, namespace, &["val"], part, report);
                leaf(property, part, report)?;
                let value = required(property, namespace, "val", part)?;
                format.color = if value == "auto" {
                    Some(TextColor::Auto {})
                } else if value.len() == 6 && value.bytes().all(|b| b.is_ascii_hexdigit()) {
                    let component = |start| {
                        u8::from_str_radix(&value[start..start + 2], 16)
                            .expect("six ASCII hex digits checked")
                    };
                    Some(TextColor::Rgb {
                        red: component(0),
                        green: component(2),
                        blue: component(4),
                    })
                } else {
                    return Err(invalid(
                        part,
                        "color must be auto or six hexadecimal digits",
                    ));
                };
            }
            _ => unsupported_element(property, part, report),
        }
    }
    same_script_value("bCs", bold_cs, format.bold, part, report);
    same_script_value("iCs", italic_cs, format.italic, part, report);
    same_script_value("szCs", size_cs, format.font_size, part, report);
    Ok(format)
}

pub(crate) fn parse_paragraph(
    node: Node<'_, '_>,
    namespace: &str,
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<ParsedParagraphProperties, ConversionError> {
    attributes(node, namespace, &[], part, report);
    let mut result = ParsedParagraphProperties {
        format: ParagraphFormat::default(),
        style_id: None,
        section: None,
    };
    let mut seen = BTreeSet::new();
    for property in property_children(node, namespace, part, report)? {
        if !unique(property, &mut seen, part, report) {
            continue;
        }
        match property.tag_name().name() {
            "pStyle" => {
                attributes(property, namespace, &["val"], part, report);
                leaf(property, part, report)?;
                result.style_id = Some(required(property, namespace, "val", part)?.into());
            }
            "jc" => {
                attributes(property, namespace, &["val"], part, report);
                leaf(property, part, report)?;
                result.format.alignment = match required(property, namespace, "val", part)? {
                    "left" => Some(Alignment::Left),
                    "center" => Some(Alignment::Center),
                    "right" => Some(Alignment::Right),
                    "both" => Some(Alignment::Justify),
                    _ => {
                        unsupported_value(property, part, report, "paragraph alignment");
                        None
                    }
                };
            }
            "spacing" => spacing(property, namespace, part, report, &mut result.format)?,
            "ind" => {
                result.format.indentation = Some(indentation(property, namespace, part, report)?);
            }
            "pageBreakBefore" => {
                result.format.page_break_before = Some(on_off(property, namespace, part, report)?);
            }
            "sectPr" => {
                result.section = Some(parse_section(property, namespace, part, report)?);
            }
            _ => unsupported_element(property, part, report),
        }
    }
    Ok(result)
}

pub(crate) fn parse_section(
    node: Node<'_, '_>,
    namespace: &str,
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<SectionProperties, ConversionError> {
    attributes(node, namespace, &[], part, report);
    let mut result = SectionProperties::default();
    let mut seen = BTreeSet::new();
    for property in property_children(node, namespace, part, report)? {
        if !unique(property, &mut seen, part, report) {
            continue;
        }
        match property.tag_name().name() {
            "pgSz" => result.page_size = page_size(property, namespace, part, report)?,
            "pgMar" => result.page_margins = page_margins(property, namespace, part, report)?,
            "type" => {
                attributes(property, namespace, &["val"], part, report);
                leaf(property, part, report)?;
                if property.attribute((namespace, "val")).unwrap_or("nextPage") != "nextPage" {
                    unsupported_value(property, part, report, "section break type");
                }
            }
            _ => unsupported_element(property, part, report),
        }
    }
    Ok(result)
}

fn fonts(
    node: Node<'_, '_>,
    namespace: &str,
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<Option<String>, ConversionError> {
    let names = ["ascii", "hAnsi", "eastAsia", "cs"];
    attributes(node, namespace, &names, part, report);
    leaf(node, part, report)?;
    let values: Vec<_> = names
        .iter()
        .filter_map(|name| node.attribute((namespace, *name)))
        .collect();
    let Some(first) = values.first() else {
        return Ok(None);
    };
    if first.trim().is_empty() || values.iter().any(|value| value != first) {
        unsupported_value(
            node,
            part,
            report,
            "nonuniform or blank script-specific font families",
        );
        return Ok(None);
    }
    // Missing script families can inherit a different family. A universal native
    // family is faithful only when every script slot is explicitly uniform.
    if values.len() != names.len() {
        unsupported_value(
            node,
            part,
            report,
            "partially specified script-specific font families",
        );
    }
    Ok(Some((*first).into()))
}

fn same_script_value<T: PartialEq>(
    name: &str,
    complex: Option<T>,
    ordinary: Option<T>,
    part: &str,
    report: &mut CompatibilityReport,
) {
    if (complex.is_some() || ordinary.is_some()) && complex != ordinary {
        report.push(
            Issue::UnsupportedValue,
            part,
            format!("rPr/{name}"),
            "Complex-script formatting differs from the universal native property.",
        );
    }
}

fn spacing(
    node: Node<'_, '_>,
    namespace: &str,
    part: &str,
    report: &mut CompatibilityReport,
    format: &mut ParagraphFormat,
) -> Result<(), ConversionError> {
    attributes(
        node,
        namespace,
        &["before", "after", "line", "lineRule"],
        part,
        report,
    );
    leaf(node, part, report)?;
    if let Some(value) = node.attribute((namespace, "before")) {
        format.spacing_before = nonnegative_twips(node, value, part, report)?;
    }
    if let Some(value) = node.attribute((namespace, "after")) {
        format.spacing_after = nonnegative_twips(node, value, part, report)?;
    }
    let rule = node.attribute((namespace, "lineRule"));
    let Some(value) = node.attribute((namespace, "line")) else {
        if rule.is_some() {
            unsupported_value(node, part, report, "line rule without a line measurement");
        }
        return Ok(());
    };
    let Some(rule) = rule else {
        // Do not guess across consumers: LibreOffice inherits an ancestor's
        // exact/minimum rule here, whereas the standard describes an auto
        // default. The first editable profile requires the complete pair.
        positive_u32(node, value, part, report)?;
        unsupported_value(
            node,
            part,
            report,
            "line measurement without an explicit line rule",
        );
        return Ok(());
    };
    format.line_spacing = match rule {
        "auto" => {
            positive_u32(node, value, part, report)?.map(|value| LineSpacing::Multiple { value })
        }
        "exact" | "atLeast" => positive_twips(node, value, part, report)?.map(|height| {
            if rule == "exact" {
                LineSpacing::Exact { height }
            } else {
                LineSpacing::AtLeast { height }
            }
        }),
        _ => {
            unsupported_value(node, part, report, "line spacing rule");
            None
        }
    };
    Ok(())
}

fn indentation(
    node: Node<'_, '_>,
    namespace: &str,
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<Indentation, ConversionError> {
    attributes(
        node,
        namespace,
        &["left", "right", "firstLine", "hanging"],
        part,
        report,
    );
    leaf(node, part, report)?;
    let mut indent = Indentation::default();
    if let Some(value) = node.attribute((namespace, "left")) {
        indent.left = signed_twips(node, value, part, report)?;
    }
    if let Some(value) = node.attribute((namespace, "right")) {
        indent.right = signed_twips(node, value, part, report)?;
    }
    let first = node.attribute((namespace, "firstLine"));
    let hanging = node.attribute((namespace, "hanging"));
    if first.is_some() && hanging.is_some() {
        unsupported_value(
            node,
            part,
            report,
            "simultaneous first-line and hanging indentation",
        );
    } else if let Some(value) = first {
        indent.first_line = nonnegative_twips(node, value, part, report)?;
    } else if let Some(value) = hanging
        && let Some(value) =
            unsigned_number(node, value, u64::from(i32::MAX as u32) + 1, part, report)?
    {
        indent.first_line = Some(Twips(
            i32::try_from(-(value as i64)).expect("hanging bound checked"),
        ));
    }
    Ok(indent)
}

fn page_size(
    node: Node<'_, '_>,
    namespace: &str,
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<Option<PageSize>, ConversionError> {
    attributes(node, namespace, &["w", "h", "orient"], part, report);
    leaf(node, part, report)?;
    let orientation = match node.attribute((namespace, "orient")) {
        None => None,
        Some("portrait") => Some(PageOrientation::Portrait),
        Some("landscape") => Some(PageOrientation::Landscape),
        Some(_) => {
            return Err(invalid(
                part,
                "page orientation must be portrait or landscape",
            ));
        }
    };
    let (Some(width), Some(height)) = (
        node.attribute((namespace, "w")),
        node.attribute((namespace, "h")),
    ) else {
        unsupported_value(node, part, report, "partially specified page dimensions");
        return Ok(None);
    };
    let width = positive_twips(node, width, part, report)?;
    let height = positive_twips(node, height, part, report)?;
    Ok(width.zip(height).map(|(width, height)| PageSize {
        width,
        height,
        orientation,
    }))
}

fn page_margins(
    node: Node<'_, '_>,
    namespace: &str,
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<Option<PageMargins>, ConversionError> {
    let names = [
        "top", "right", "bottom", "left", "header", "footer", "gutter",
    ];
    attributes(node, namespace, &names, part, report);
    leaf(node, part, report)?;
    let mut values = [None; 7];
    for (index, name) in names.iter().enumerate() {
        let value = required(node, namespace, name, part)?;
        values[index] = if matches!(*name, "top" | "bottom") {
            signed_twips(node, value, part, report)?
        } else {
            nonnegative_twips(node, value, part, report)?
        };
    }
    let [
        Some(top),
        Some(right),
        Some(bottom),
        Some(left),
        Some(header),
        Some(footer),
        Some(gutter),
    ] = values
    else {
        return Ok(None);
    };
    if top.0 < 0 || bottom.0 < 0 {
        unsupported_value(node, part, report, "negative page-margin overlap semantics");
        return Ok(None);
    }
    Ok(Some(PageMargins {
        top,
        right,
        bottom,
        left,
        header,
        footer,
        gutter,
    }))
}

fn on_off(
    node: Node<'_, '_>,
    namespace: &str,
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<bool, ConversionError> {
    attributes(node, namespace, &["val"], part, report);
    leaf(node, part, report)?;
    match node.attribute((namespace, "val")) {
        None | Some("true" | "1" | "on") => Ok(true),
        Some("false" | "0" | "off") => Ok(false),
        Some(_) => Err(invalid(part, "on/off property has an invalid value")),
    }
}

fn positive_u32(
    node: Node<'_, '_>,
    value: &str,
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<Option<u32>, ConversionError> {
    let value = unsigned_number(node, value, u64::from(u32::MAX), part, report)?;
    if value == Some(0) {
        return Err(invalid(part, "measurement must be greater than zero"));
    }
    Ok(value.map(|value| value as u32))
}

fn positive_twips(
    node: Node<'_, '_>,
    value: &str,
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<Option<Twips>, ConversionError> {
    let value = nonnegative_twips(node, value, part, report)?;
    if value == Some(Twips(0)) {
        return Err(invalid(part, "measurement must be greater than zero"));
    }
    Ok(value)
}

fn nonnegative_twips(
    node: Node<'_, '_>,
    value: &str,
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<Option<Twips>, ConversionError> {
    Ok(unsigned_number(node, value, i32::MAX as u64, part, report)?
        .map(|value| Twips(value as i32)))
}

fn unsigned_number(
    node: Node<'_, '_>,
    value: &str,
    maximum: u64,
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<Option<u64>, ConversionError> {
    let value = value.trim_matches(|c: char| c.is_ascii_whitespace());
    let digits = value.strip_prefix('+').unwrap_or(value);
    if !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()) {
        match digits.parse::<u64>() {
            Ok(number) if number <= maximum => Ok(Some(number)),
            _ => {
                unsupported_value(
                    node,
                    part,
                    report,
                    "measurement outside the native numeric range",
                );
                Ok(None)
            }
        }
    } else if universal_measure(value) {
        unsupported_value(node, part, report, "unit-qualified measurement");
        Ok(None)
    } else {
        Err(invalid(part, "expected a nonnegative decimal integer"))
    }
}

fn signed_twips(
    node: Node<'_, '_>,
    value: &str,
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<Option<Twips>, ConversionError> {
    let value = value.trim_matches(|c: char| c.is_ascii_whitespace());
    let digits = value.strip_prefix(['+', '-']).unwrap_or(value);
    if !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()) {
        match value.parse::<i32>() {
            Ok(number) => Ok(Some(Twips(number))),
            Err(_) => {
                unsupported_value(
                    node,
                    part,
                    report,
                    "measurement outside the native numeric range",
                );
                Ok(None)
            }
        }
    } else if universal_measure(value) {
        unsupported_value(node, part, report, "unit-qualified measurement");
        Ok(None)
    } else {
        Err(invalid(part, "expected a signed decimal integer"))
    }
}

fn universal_measure(value: &str) -> bool {
    let value = value.strip_prefix(['+', '-']).unwrap_or(value);
    let Some(number) = ["mm", "cm", "in", "pt", "pc", "pi"]
        .iter()
        .find_map(|unit| value.strip_suffix(unit))
    else {
        return false;
    };
    let mut components = number.split('.');
    let whole = components.next().unwrap_or_default();
    let fraction = components.next();
    !whole.is_empty()
        && whole.bytes().all(|byte| byte.is_ascii_digit())
        && fraction
            .is_none_or(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
        && components.next().is_none()
}

fn required<'a>(
    node: Node<'a, '_>,
    namespace: &str,
    name: &str,
    part: &str,
) -> Result<&'a str, ConversionError> {
    node.attribute((namespace, name))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| invalid(part, format!("{} requires {name}", node.tag_name().name())))
}

fn attributes(
    node: Node<'_, '_>,
    namespace: &str,
    allowed: &[&str],
    part: &str,
    report: &mut CompatibilityReport,
) {
    for attribute in node.attributes() {
        if attribute.namespace() != Some(namespace) || !allowed.contains(&attribute.name()) {
            report.push(
                Issue::UnsupportedAttribute,
                part,
                format!("{}/@{}", node.tag_name().name(), attribute.name()),
                "This attribute is outside the editable property profile.",
            );
        }
    }
}

fn property_children<'a, 'input>(
    node: Node<'a, 'input>,
    namespace: &str,
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<Vec<Node<'a, 'input>>, ConversionError> {
    let mut result = Vec::new();
    for child in node.children() {
        if child.is_element() {
            if child.tag_name().namespace() == Some(namespace) {
                result.push(child);
            } else {
                unsupported_element(child, part, report);
            }
        } else if child.is_text() && child.text().is_some_and(|text| !xml_whitespace(text)) {
            return Err(invalid(part, "unexpected text in a property container"));
        } else if child.is_pi() {
            report.push(
                Issue::UnsupportedElement,
                part,
                node.tag_name().name(),
                "Processing instruction in property content.",
            );
        }
    }
    Ok(result)
}

fn leaf(
    node: Node<'_, '_>,
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<(), ConversionError> {
    for child in node.children() {
        if child.is_element() {
            unsupported_element(child, part, report);
        } else if child.is_text() && child.text().is_some_and(|text| !xml_whitespace(text)) {
            return Err(invalid(part, "unexpected text in a property element"));
        } else if child.is_pi() {
            report.push(
                Issue::UnsupportedElement,
                part,
                node.tag_name().name(),
                "Processing instruction in property content.",
            );
        }
    }
    Ok(())
}

fn unique<'a>(
    node: Node<'a, '_>,
    seen: &mut BTreeSet<&'a str>,
    part: &str,
    report: &mut CompatibilityReport,
) -> bool {
    if seen.insert(node.tag_name().name()) {
        true
    } else {
        report.push(
            Issue::UnsupportedElement,
            part,
            node.tag_name().name(),
            "Repeated properties are outside the editable profile.",
        );
        false
    }
}

fn xml_whitespace(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
}

fn unsupported_element(node: Node<'_, '_>, part: &str, report: &mut CompatibilityReport) {
    report.push(
        Issue::UnsupportedElement,
        part,
        node.tag_name().name(),
        "This element has no supported property mapping.",
    );
}

fn unsupported_value(
    node: Node<'_, '_>,
    part: &str,
    report: &mut CompatibilityReport,
    feature: &str,
) {
    report.push(
        Issue::UnsupportedValue,
        part,
        node.tag_name().name(),
        format!("Unsupported {feature}; the original remains read-only."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    const NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    const PART: &str = "word/document.xml";

    fn character(body: &str) -> Result<(CharacterFormat, CompatibilityReport), ConversionError> {
        let xml = format!("<w:rPr xmlns:w=\"{NS}\">{body}</w:rPr>");
        let document = crate::xml::parse_xml(PART, xml.as_bytes())?;
        let mut report = CompatibilityReport::default();
        let value = parse_character(document.root_element(), NS, PART, &mut report)?;
        Ok((value, report))
    }

    fn paragraph(
        body: &str,
    ) -> Result<(ParsedParagraphProperties, CompatibilityReport), ConversionError> {
        let xml = format!("<w:pPr xmlns:w=\"{NS}\">{body}</w:pPr>");
        let document = crate::xml::parse_xml(PART, xml.as_bytes())?;
        let mut report = CompatibilityReport::default();
        let value = parse_paragraph(document.root_element(), NS, PART, &mut report)?;
        Ok((value, report))
    }

    fn section(body: &str) -> Result<(SectionProperties, CompatibilityReport), ConversionError> {
        let xml = format!("<w:sectPr xmlns:w=\"{NS}\">{body}</w:sectPr>");
        let document = crate::xml::parse_xml(PART, xml.as_bytes())?;
        let mut report = CompatibilityReport::default();
        let value = parse_section(document.root_element(), NS, PART, &mut report)?;
        Ok((value, report))
    }

    #[test]
    fn maps_uniform_character_properties_without_losing_explicit_off_values() {
        let (format, report) = character(
            r#"<w:b/><w:bCs w:val="1"/><w:i w:val="false"/><w:iCs w:val="off"/>
            <w:rFonts w:ascii="Noto Sans &amp; Serif" w:hAnsi="Noto Sans &amp; Serif" w:eastAsia="Noto Sans &amp; Serif" w:cs="Noto Sans &amp; Serif"/>
            <w:sz w:val="25"/><w:szCs w:val="25"/><w:u w:val="double"/><w:color w:val="Ab12F0"/>"#,
        ).unwrap();
        assert!(report.is_supported());
        assert_eq!(format.bold, Some(true));
        assert_eq!(format.italic, Some(false));
        assert_eq!(format.font_family.as_deref(), Some("Noto Sans & Serif"));
        assert_eq!(format.font_size, Some(HalfPoints(25)));
        assert_eq!(format.underline, Some(Underline::Double));
        assert_eq!(
            format.color,
            Some(TextColor::Rgb {
                red: 0xab,
                green: 0x12,
                blue: 0xf0
            })
        );
    }

    #[test]
    fn preserves_raw_style_toggle_values_for_the_contextual_importer() {
        for (raw, expected) in [
            ("true", true),
            ("on", true),
            ("1", true),
            ("false", false),
            ("off", false),
            ("0", false),
        ] {
            let body = format!("<w:b w:val=\"{raw}\"/><w:bCs w:val=\"{raw}\"/>");
            let (format, report) = character(&body).unwrap();
            assert_eq!(format.bold, Some(expected));
            assert!(report.is_supported());
        }
    }

    #[test]
    fn partial_or_different_complex_script_properties_are_not_approximated() {
        for body in [
            r#"<w:b/>"#,
            r#"<w:bCs/>"#,
            r#"<w:b/><w:bCs w:val="false"/>"#,
            r#"<w:i/>"#,
            r#"<w:iCs/>"#,
            r#"<w:sz w:val="24"/>"#,
            r#"<w:szCs w:val="24"/>"#,
            r#"<w:sz w:val="24"/><w:szCs w:val="30"/>"#,
            r#"<w:rFonts w:ascii="Arial" w:hAnsi="Arial"/>"#,
            r#"<w:rFonts w:ascii="Arial" w:hAnsi="Arial" w:eastAsia="Arial" w:cs="Noto Sans Arabic"/>"#,
            r#"<w:rFonts w:asciiTheme="minorHAnsi"/>"#,
        ] {
            assert!(!character(body).unwrap().1.is_supported(), "{body}");
        }
    }

    #[test]
    fn malformed_recognized_character_values_are_structured_errors() {
        for body in [
            r#"<w:b w:val="TRUE"/>"#,
            r#"<w:sz w:val="0"/>"#,
            r#"<w:sz/>"#,
            r#"<w:color w:val="12345"/>"#,
            r#"<w:color w:val="xyzxyz"/>"#,
            r#"<w:b>unexpected text</w:b>"#,
        ] {
            assert!(
                matches!(character(body), Err(ConversionError::InvalidDocument { part, .. }) if part == PART),
                "{body}"
            );
        }
    }

    #[test]
    fn unknown_properties_attributes_and_repetitions_prevent_editable_import() {
        for body in [
            r#"<w:vanish/>"#,
            r#"<w:b w:future="1"/><w:bCs/>"#,
            r#"<w:b/><w:b/><w:bCs/>"#,
            r#"<w:u w:val="words"/>"#,
            r#"<w:color w:val="auto" w:themeColor="accent1"/>"#,
            r#"<x:feature xmlns:x="urn:future"/>"#,
        ] {
            assert!(!character(body).unwrap().1.is_supported(), "{body}");
        }
    }

    #[test]
    fn maps_paragraph_units_alignment_indentation_and_page_break() {
        let (properties, report) = paragraph(
            r#"<w:pStyle w:val="Heading1"/><w:jc w:val="both"/>
            <w:spacing w:before="120" w:after="240" w:line="276" w:lineRule="auto"/>
            <w:ind w:left="-120" w:right="360" w:hanging="480"/><w:pageBreakBefore w:val="false"/>"#,
        ).unwrap();
        assert!(report.is_supported());
        assert_eq!(properties.style_id.as_deref(), Some("Heading1"));
        assert!(properties.section.is_none());
        assert_eq!(properties.format.alignment, Some(Alignment::Justify));
        assert_eq!(properties.format.spacing_before, Some(Twips(120)));
        assert_eq!(properties.format.spacing_after, Some(Twips(240)));
        assert_eq!(
            properties.format.line_spacing,
            Some(LineSpacing::Multiple { value: 276 })
        );
        assert_eq!(properties.format.page_break_before, Some(false));
        assert_eq!(
            properties.format.indentation,
            Some(Indentation {
                left: Some(Twips(-120)),
                right: Some(Twips(360)),
                first_line: Some(Twips(-480))
            })
        );
    }

    #[test]
    fn exact_minimum_and_automatic_spacing_remain_distinct() {
        for (rule, expected) in [
            ("exact", LineSpacing::Exact { height: Twips(300) }),
            ("atLeast", LineSpacing::AtLeast { height: Twips(300) }),
            ("auto", LineSpacing::Multiple { value: 300 }),
        ] {
            let (properties, report) = paragraph(&format!(
                "<w:spacing w:line=\"300\" w:lineRule=\"{rule}\"/>"
            ))
            .unwrap();
            assert!(report.is_supported());
            assert_eq!(properties.format.line_spacing, Some(expected));
        }
    }

    #[test]
    fn partial_line_spacing_pairs_are_read_only_without_assuming_inheritance() {
        for body in [
            r#"<w:spacing w:line="300"/>"#,
            r#"<w:spacing w:lineRule="exact"/>"#,
            r#"<w:spacing w:lineRule="auto"/>"#,
        ] {
            let (properties, report) = paragraph(body).unwrap();
            assert_eq!(properties.format.line_spacing, None);
            assert!(!report.is_supported());
            assert!(report.diagnostics.iter().any(|diagnostic| {
                diagnostic.kind == Issue::UnsupportedValue && diagnostic.path == "spacing"
            }));
        }
        let (properties, report) =
            paragraph(r#"<w:spacing w:line="300" w:lineRule="auto"/>"#).unwrap();
        assert!(report.is_supported());
        assert_eq!(
            properties.format.line_spacing,
            Some(LineSpacing::Multiple { value: 300 })
        );
    }

    #[test]
    fn first_line_hanging_extremes_and_character_units_have_explicit_behavior() {
        let (properties, report) = paragraph(r#"<w:ind w:hanging="2147483648"/>"#).unwrap();
        assert!(report.is_supported());
        assert_eq!(
            properties.format.indentation.unwrap().first_line,
            Some(Twips(i32::MIN))
        );
        assert!(paragraph(r#"<w:ind w:firstLine="-1"/>"#).is_err());
        assert!(
            !paragraph(r#"<w:ind w:firstLine="120" w:hanging="240"/>"#)
                .unwrap()
                .1
                .is_supported()
        );
        assert!(
            !paragraph(r#"<w:ind w:leftChars="100"/>"#)
                .unwrap()
                .1
                .is_supported()
        );
        assert!(
            !paragraph(r#"<w:spacing w:beforeLines="100"/>"#)
                .unwrap()
                .1
                .is_supported()
        );
    }

    #[test]
    fn numeric_ranges_and_units_never_overflow_or_silently_convert() {
        let (properties, report) =
            paragraph(r#"<w:ind w:left=" -2147483648 " w:right="+000360"/>"#).unwrap();
        assert!(report.is_supported());
        assert_eq!(
            properties.format.indentation.unwrap().right,
            Some(Twips(360))
        );
        for body in [
            r#"<w:ind w:left="2147483648"/>"#,
            r#"<w:ind w:hanging="2147483649"/>"#,
            r#"<w:spacing w:before="2.54cm"/>"#,
            r#"<w:spacing w:line="999999999999999999999999999999999"/>"#,
        ] {
            assert!(!paragraph(body).unwrap().1.is_supported(), "{body}");
        }
        assert!(paragraph(r#"<w:spacing w:before="-1"/>"#).is_err());
        assert!(paragraph(r#"<w:spacing w:line="0"/>"#).is_err());
    }

    #[test]
    fn paragraph_mark_format_and_directional_alignment_remain_read_only() {
        for body in [
            r#"<w:rPr><w:b/></w:rPr>"#,
            r#"<w:jc w:val="start"/>"#,
            r#"<w:numPr/>"#,
            r#"<w:bidi/>"#,
        ] {
            assert!(!paragraph(body).unwrap().1.is_supported(), "{body}");
        }
    }

    #[test]
    fn maps_section_geometry_without_rotating_explicit_dimensions() {
        let (properties, report) = section(
            r#"<w:type w:val="nextPage"/><w:pgSz w:w="15840" w:h="12240" w:orient="landscape"/>
            <w:pgMar w:top="1440" w:right="1200" w:bottom="1440" w:left="1200" w:header="720" w:footer="720" w:gutter="240"/>"#,
        ).unwrap();
        assert!(report.is_supported());
        assert_eq!(
            properties.page_size,
            Some(PageSize {
                width: Twips(15840),
                height: Twips(12240),
                orientation: Some(PageOrientation::Landscape)
            })
        );
        assert_eq!(
            properties.page_margins,
            Some(PageMargins {
                top: Twips(1440),
                right: Twips(1200),
                bottom: Twips(1440),
                left: Twips(1200),
                header: Twips(720),
                footer: Twips(720),
                gutter: Twips(240)
            })
        );
    }

    #[test]
    fn paragraph_section_properties_remain_attached_to_that_paragraph() {
        let (properties, report) =
            paragraph(r#"<w:sectPr><w:pgSz w:w="12240" w:h="15840"/></w:sectPr>"#).unwrap();
        assert!(report.is_supported());
        assert_eq!(
            properties.section.unwrap().page_size.unwrap().width,
            Twips(12240)
        );
    }

    #[test]
    fn section_partial_geometry_and_unsupported_features_are_not_discarded() {
        for body in [
            r#"<w:pgSz w:w="12240"/>"#,
            r#"<w:pgSz w:w="210mm" w:h="297mm"/>"#,
            r#"<w:type w:val="continuous"/>"#,
            r#"<w:cols w:num="2"/>"#,
            r#"<w:headerReference/>"#,
            r#"<w:pgMar w:top="-1" w:right="1200" w:bottom="1440" w:left="1200" w:header="720" w:footer="720" w:gutter="0"/>"#,
        ] {
            assert!(!section(body).unwrap().1.is_supported(), "{body}");
        }
        assert!(section(r#"<w:pgSz w:w="0" w:h="100"/>"#).is_err());
        assert!(section(r#"<w:pgSz w:w="100" w:h="100" w:orient="reverse"/>"#).is_err());
        assert!(section(r#"<w:pgMar w:top="1" w:right="1" w:bottom="1" w:left="1"/>"#).is_err());
        assert_eq!(section("").unwrap().0, SectionProperties::default());
    }

    #[test]
    fn strict_namespaces_and_arbitrary_prefixes_use_expanded_names() {
        let strict = "http://purl.oclc.org/ooxml/wordprocessingml/main";
        let xml =
            format!("<s:rPr xmlns:s=\"{strict}\"><s:b s:val=\"0\"/><s:bCs s:val=\"0\"/></s:rPr>");
        let document = crate::xml::parse_xml(PART, xml.as_bytes()).unwrap();
        let mut report = CompatibilityReport::default();
        let format = parse_character(document.root_element(), strict, PART, &mut report).unwrap();
        assert_eq!(format.bold, Some(false));
        assert!(report.is_supported());
    }

    #[test]
    fn diagnostics_are_bounded_for_many_unsupported_properties() {
        let body = "<w:future/>".repeat(512);
        let (_, report) = character(&body).unwrap();
        assert_eq!(report.diagnostics.len(), 128);
        assert_eq!(report.omitted_diagnostics, 384);
        assert!(!report.is_supported());
    }
}
