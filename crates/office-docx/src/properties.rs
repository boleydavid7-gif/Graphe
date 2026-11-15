//! Explicit WordprocessingML property mappings for the editable subset.

use std::collections::BTreeSet;

use office_core::{
    Alignment, CharacterFormat, HalfPoints, Indentation, LineSpacing, PageMargins, PageOrientation,
    PageSize, ParagraphFormat, SectionProperties, TextColor, Twips, Underline,
};
use roxmltree::Node;

const WORD_2010: &str = "http://schemas.microsoft.com/office/word/2010/wordml";

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
    parse_character_properties(node, namespace, part, report, false).map(|(format, _)| format)
}

/// Extracts a validated run-style reference for the importer to resolve before
/// publishing text. Styles/defaults do not accept this run-only property.
pub(crate) fn parse_run_character(
    node: Node<'_, '_>,
    namespace: &str,
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<(CharacterFormat, Option<String>), ConversionError> {
    parse_character_properties(node, namespace, part, report, true)
}

fn parse_character_properties(
    node: Node<'_, '_>,
    namespace: &str,
    part: &str,
    report: &mut CompatibilityReport,
    allow_run_style: bool,
) -> Result<(CharacterFormat, Option<String>), ConversionError> {
    attributes(node, namespace, &[], part, report);
    let mut format = CharacterFormat::default();
    let mut style = None;
    let mut bold_cs = None;
    let mut italic_cs = None;
    let mut size_cs = None;
    let mut seen = BTreeSet::new();
    for property in property_children(node, namespace, part, report)? {
        if !unique(property, &mut seen, part, report) {
            continue;
        }
        match property.tag_name().name() {
            "rStyle" if allow_run_style => {
                attributes(property, namespace, &["val"], part, report);
                leaf(property, part, report)?;
                style = Some(required(property, namespace, "val", part)?.to_owned());
            }
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
                    "wavyDouble" => {
                        normalized(
                            property,
                            part,
                            report,
                            "Double-wave underlining is normalized to double underline; the original treatment is retained.",
                        );
                        Some(Underline::Double)
                    }
                    "words" | "thick" | "dotted" | "dottedHeavy" | "dash" | "dashedHeavy"
                    | "dashLong" | "dashLongHeavy" | "dotDash" | "dashDotHeavy" | "dotDotDash"
                    | "dashDotDotHeavy" | "wave" | "wavyHeavy" => {
                        normalized(
                            property,
                            part,
                            report,
                            "This underline treatment is normalized to single underline; the original treatment is retained.",
                        );
                        Some(Underline::Single)
                    }
                    _ => return Err(invalid(part, "unknown underline treatment")),
                };
            }
            "color" => format.color = color(property, namespace, part, report)?,
            _ => {
                if !normalize_character_property(property, namespace, part, report)? {
                    unsupported_element(property, part, report);
                }
            }
        }
    }
    format.bold = normalized_script_value("bCs", bold_cs, format.bold, part, report);
    format.italic = normalized_script_value("iCs", italic_cs, format.italic, part, report);
    format.font_size = normalized_script_value("szCs", size_cs, format.font_size, part, report);
    Ok((format, style))
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
                    "start" | "end" => {
                        normalized(
                            property,
                            part,
                            report,
                            "Logical start/end alignment is normalized to left/right in Writer's horizontal paragraph flow; the source retains the original alignment.",
                        );
                        Some(if required(property, namespace, "val", part)? == "start" {
                            Alignment::Left
                        } else {
                            Alignment::Right
                        })
                    }
                    _ => {
                        unsupported_value(property, part, report, "paragraph alignment");
                        None
                    }
                };
            }
            "spacing" => spacing(property, namespace, part, report, &mut result.format)?,
            "pBdr" => paragraph_borders(property, namespace, part, report)?,
            "ind" => {
                result.format.indentation = Some(indentation(property, namespace, part, report)?);
            }
            "pageBreakBefore" => {
                result.format.page_break_before = Some(on_off(property, namespace, part, report)?);
            }
            "sectPr" => {
                result.section = Some(parse_section(property, namespace, part, report)?);
            }
            "rPr" => {
                // Paragraph-mark formatting is not text. Parse it through the same
                // guards before retaining its source-only future-typing intent.
                let _ = parse_character(property, namespace, part, report)?;
                normalized(
                    property,
                    part,
                    report,
                    "Paragraph-mark formatting uses Writer's current paragraph defaults.",
                );
            }
            "keepNext"
            | "keepLines"
            | "widowControl"
            | "contextualSpacing"
            | "suppressAutoHyphens"
            | "suppressLineNumbers"
            | "snapToGrid"
            | "adjustRightInd"
            | "autoSpaceDE"
            | "autoSpaceDN"
            | "wordWrap"
            | "overflowPunct"
            | "topLinePunct"
            | "kinsoku" => {
                let _ = on_off(property, namespace, part, report)?;
                normalized(
                    property,
                    part,
                    report,
                    "Paragraph pagination and typography preferences use Writer defaults.",
                );
            }
            "bidi" => {
                if on_off(property, namespace, part, report)? {
                    unsupported_value(
                        property,
                        part,
                        report,
                        "forced right-to-left paragraph direction",
                    );
                }
            }
            "outlineLvl" => {
                numeric_leaf(property, namespace, part, report, false)?;
                if required(property, namespace, "val", part)?
                    .parse::<u32>()
                    .is_ok_and(|v| v > 9)
                {
                    return Err(invalid(
                        part,
                        "paragraph outline level must be between zero and nine",
                    ));
                }
                normalized(
                    property,
                    part,
                    report,
                    "Outline level remains in the original; Writer edits the paragraph as ordinary text.",
                );
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
                match property.attribute((namespace, "val")).unwrap_or("nextPage") {
                    "nextPage" => {}
                    "nextColumn" | "continuous" | "evenPage" | "oddPage" => normalized(
                        property,
                        part,
                        report,
                        "The section break is normalized to Writer's next-page section flow; original section-break preferences are retained.",
                    ),
                    _ => return Err(invalid(part, "unknown section break type")),
                }
            }
            "cols" => single_column(property, namespace, part, report)?,
            "docGrid" => document_grid(property, namespace, part, report)?,
            "pgNumType" => page_numbering(property, namespace, part, report)?,
            "formProt" => {
                if on_off(property, namespace, part, report)? {
                    unsupported_value(property, part, report, "protected form section");
                }
            }
            "textDirection" => {
                attributes(property, namespace, &["val"], part, report);
                leaf(property, part, report)?;
                match required(property, namespace, "val", part)? {
                    "lrTb" => {}
                    "tbRl" | "btLr" | "lrTbV" | "tbRlV" | "tbLrV" | "tb" | "rl" | "lr" | "tbV"
                    | "rlV" | "lrV" => normalized(
                        property,
                        part,
                        report,
                        "Vertical or rotated text flow is normalized to Writer's horizontal flow in source XML text order; original flow formatting is retained.",
                    ),
                    _ => return Err(invalid(part, "unknown text flow direction")),
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
    let names = ["ascii", "hAnsi", "cs", "eastAsia"];
    let themes = ["asciiTheme", "hAnsiTheme", "cstheme", "eastAsiaTheme"];
    attributes(
        node,
        namespace,
        &[
            "ascii",
            "hAnsi",
            "cs",
            "eastAsia",
            "asciiTheme",
            "hAnsiTheme",
            "cstheme",
            "eastAsiaTheme",
            "hint",
        ],
        part,
        report,
    );
    leaf(node, part, report)?;
    let values: Vec<_> = names
        .iter()
        .filter_map(|name| node.attribute((namespace, *name)))
        .collect();
    if values.iter().any(|name| name.trim().is_empty()) {
        return Err(invalid(part, "font names cannot be blank"));
    }
    let mut has_theme = false;
    for name in themes {
        if let Some(value) = node.attribute((namespace, name)) {
            has_theme = true;
            if !matches!(
                value,
                "majorAscii"
                    | "majorHAnsi"
                    | "majorEastAsia"
                    | "majorBidi"
                    | "minorAscii"
                    | "minorHAnsi"
                    | "minorEastAsia"
                    | "minorBidi"
            ) {
                return Err(invalid(part, "unknown theme font slot"));
            }
        }
    }
    if let Some(hint) = node.attribute((namespace, "hint")) {
        if !matches!(hint, "default" | "eastAsia" | "cs") {
            return Err(invalid(part, "unknown script font hint"));
        }
        normalized(
            node,
            part,
            report,
            "Script font hints use Writer's Unicode shaping and fallback policy.",
        );
    }
    if has_theme
        || (!values.is_empty()
            && (values.len() != names.len() || values.iter().any(|value| value != &values[0])))
    {
        normalized(
            node,
            part,
            report,
            "Script/theme fonts are normalized to the first explicit ASCII, high-ANSI, complex-script or East Asian family, or inherited Writer defaults when no concrete family exists. Appearance may differ; original formatting is preserved in the source.",
        );
    }
    // Full inherited properties are cached and may later be copied into many
    // runs. Bound the chosen family before allocating it so a small source
    // cannot amplify one huge attribute into gigabytes of inherited strings.
    if values.first().is_some_and(|value| value.len() > 1024) {
        normalized(
            node,
            part,
            report,
            "A font-family name exceeding the 1,024-byte import limit uses inherited Writer defaults. Its exact value remains in the original source.",
        );
        return Ok(None);
    }
    Ok(values.first().map(|value| (*value).into()))
}

fn normalized_script_value<T: PartialEq>(
    name: &str,
    complex: Option<T>,
    ordinary: Option<T>,
    part: &str,
    report: &mut CompatibilityReport,
) -> Option<T> {
    if (complex.is_some() || ordinary.is_some()) && complex != ordinary {
        report.normalize(part, format!("rPr/{name}"), "Script-specific formatting is normalized to the ordinary value, or the complex-script value when ordinary formatting is absent. The original retains every script-specific value.");
    }
    ordinary.or(complex)
}

fn color(
    node: Node<'_, '_>,
    namespace: &str,
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<Option<TextColor>, ConversionError> {
    attributes(
        node,
        namespace,
        &["val", "themeColor", "themeTint", "themeShade"],
        part,
        report,
    );
    leaf(node, part, report)?;
    let theme = node.attribute((namespace, "themeColor"));
    if let Some(theme) = theme
        && !theme_color_slot(theme)
    {
        return Err(invalid(part, "unknown theme color slot"));
    }
    for name in ["themeTint", "themeShade"] {
        if let Some(value) = node.attribute((namespace, name))
            && (value.len() != 2 || !value.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err(invalid(
                part,
                "theme tint and shade require one hexadecimal byte",
            ));
        }
    }
    if theme.is_some()
        || node.attribute((namespace, "themeTint")).is_some()
        || node.attribute((namespace, "themeShade")).is_some()
    {
        normalized(
            node,
            part,
            report,
            "Theme color/tint/shade uses the cached explicit color, or inherited Writer color when absent; theme changes are not reproduced. The source preserves the original theme formatting.",
        );
    }
    let Some(value) = node.attribute((namespace, "val")) else {
        if theme.is_some() {
            return Ok(None);
        }
        return Err(invalid(
            part,
            "color requires an explicit value or theme slot",
        ));
    };
    if value == "auto" {
        return Ok(Some(TextColor::Auto {}));
    }
    if value.len() != 6 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(invalid(
            part,
            "color must be auto or six hexadecimal digits",
        ));
    }
    let component = |start| {
        u8::from_str_radix(&value[start..start + 2], 16).expect("six ASCII hex digits checked")
    };
    Ok(Some(TextColor::Rgb {
        red: component(0),
        green: component(2),
        blue: component(4),
    }))
}

fn theme_color_slot(value: &str) -> bool {
    matches!(
        value,
        "dark1"
            | "light1"
            | "dark2"
            | "light2"
            | "accent1"
            | "accent2"
            | "accent3"
            | "accent4"
            | "accent5"
            | "accent6"
            | "hyperlink"
            | "followedHyperlink"
            | "none"
            | "background1"
            | "text1"
            | "background2"
            | "text2"
    )
}

fn paragraph_borders(
    node: Node<'_, '_>,
    namespace: &str,
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<(), ConversionError> {
    attributes(node, namespace, &[], part, report);
    let mut seen = BTreeSet::new();
    for border in property_children(node, namespace, part, report)? {
        if !matches!(
            border.tag_name().name(),
            "top" | "left" | "bottom" | "right" | "between" | "bar"
        ) {
            unsupported_element(border, part, report);
            continue;
        }
        if !unique(border, &mut seen, part, report) {
            continue;
        }
        attributes(
            border,
            namespace,
            &[
                "val",
                "sz",
                "space",
                "color",
                "themeColor",
                "themeTint",
                "themeShade",
                "frame",
                "shadow",
            ],
            part,
            report,
        );
        leaf(border, part, report)?;
        if !matches!(
            required(border, namespace, "val", part)?,
            "nil"
                | "none"
                | "single"
                | "thick"
                | "double"
                | "dotted"
                | "dashed"
                | "dotDash"
                | "dotDotDash"
                | "triple"
                | "thinThickSmallGap"
                | "thickThinSmallGap"
                | "thinThickThinSmallGap"
                | "thinThickMediumGap"
                | "thickThinMediumGap"
                | "thinThickThinMediumGap"
                | "thinThickLargeGap"
                | "thickThinLargeGap"
                | "thinThickThinLargeGap"
                | "wave"
                | "doubleWave"
                | "dashSmallGap"
                | "dashDotStroked"
                | "threeDEmboss"
                | "threeDEngrave"
                | "outset"
                | "inset"
        ) {
            unsupported_value(border, part, report, "paragraph border treatment");
        }
        for name in ["sz", "space"] {
            if let Some(value) = border.attribute((namespace, name))
                && value.parse::<u32>().is_err()
            {
                return Err(invalid(part, "malformed paragraph border measurement"));
            }
        }
        for name in ["frame", "shadow"] {
            if let Some(value) = border.attribute((namespace, name)) {
                boolean(value, part)?;
            }
        }
        if let Some(value) = border.attribute((namespace, "color"))
            && value != "auto"
            && (value.len() != 6 || !value.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err(invalid(
                part,
                "paragraph border color requires auto or six hexadecimal digits",
            ));
        }
        if let Some(value) = border.attribute((namespace, "themeColor"))
            && !theme_color_slot(value)
        {
            return Err(invalid(part, "unknown paragraph border theme color slot"));
        }
        for name in ["themeTint", "themeShade"] {
            if let Some(value) = border.attribute((namespace, name))
                && (value.len() != 2 || !value.bytes().all(|b| b.is_ascii_hexdigit()))
            {
                return Err(invalid(
                    part,
                    "paragraph border theme tint/shade requires one hexadecimal byte",
                ));
            }
        }
    }
    normalized(
        node,
        part,
        report,
        "Paragraph borders are retained in the original and omitted from Writer's editable presentation; text and supported paragraph formatting are unchanged.",
    );
    Ok(())
}

fn normalize_character_property(
    node: Node<'_, '_>,
    namespace: &str,
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<bool, ConversionError> {
    match node.tag_name().name() {
        "kern" | "w" => numeric_leaf(node, namespace, part, report, false)?,
        "spacing" | "position" => numeric_leaf(node, namespace, part, report, true)?,
        "lang" => {
            attributes(node, namespace, &["val", "eastAsia", "bidi"], part, report);
            leaf(node, part, report)?;
            for value in node
                .attributes()
                .filter(|a| a.namespace() == Some(namespace))
                .map(|a| a.value())
            {
                if value.is_empty()
                    || value.len() > 128
                    || !value.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
                {
                    return Err(invalid(part, "invalid language identifier"));
                }
            }
        }
        "caps" | "smallCaps" | "noProof" | "snapToGrid" | "outline" | "shadow" | "emboss"
        | "imprint" | "strike" | "dstrike" => {
            let _ = on_off(node, namespace, part, report)?;
        }
        "vanish" | "webHidden" | "rtl" | "specVanish" => {
            if on_off(node, namespace, part, report)? {
                unsupported_value(node, part, report, "hidden text or forced run direction");
            }
            return Ok(true);
        }
        "vertAlign" => {
            attributes(node, namespace, &["val"], part, report);
            leaf(node, part, report)?;
            if !matches!(
                required(node, namespace, "val", part)?,
                "baseline" | "superscript" | "subscript"
            ) {
                return Err(invalid(part, "unknown vertical alignment"));
            }
        }
        "ligatures" if node.tag_name().namespace() == Some(WORD_2010) => {
            attributes(node, WORD_2010, &["val"], part, report);
            leaf(node, part, report)?;
            if !matches!(
                required(node, WORD_2010, "val", part)?,
                "none"
                    | "standard"
                    | "contextual"
                    | "historical"
                    | "discretional"
                    | "standardContextual"
                    | "standardHistorical"
                    | "contextualHistorical"
                    | "standardDiscretional"
                    | "contextualDiscretional"
                    | "historicalDiscretional"
                    | "standardContextualHistorical"
                    | "standardContextualDiscretional"
                    | "standardHistoricalDiscretional"
                    | "contextualHistoricalDiscretional"
                    | "all"
            ) {
                return Err(invalid(part, "unknown ligature selection"));
            }
        }
        _ => return Ok(false),
    }
    normalized(
        node,
        part,
        report,
        "This character typography preference uses Writer defaults; the original formatting is retained in the source.",
    );
    Ok(true)
}

fn normalized(node: Node<'_, '_>, part: &str, report: &mut CompatibilityReport, message: &str) {
    report.normalize(part, node.tag_name().name(), message);
}

fn numeric_leaf(
    node: Node<'_, '_>,
    namespace: &str,
    part: &str,
    report: &mut CompatibilityReport,
    signed: bool,
) -> Result<(), ConversionError> {
    attributes(node, namespace, &["val"], part, report);
    leaf(node, part, report)?;
    let value = required(node, namespace, "val", part)?;
    if if signed {
        value.parse::<i32>().is_err()
    } else {
        value.parse::<u32>().is_err()
    } {
        return Err(invalid(part, "malformed typography measurement"));
    }
    Ok(())
}

fn single_column(
    node: Node<'_, '_>,
    namespace: &str,
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<(), ConversionError> {
    attributes(
        node,
        namespace,
        &["space", "num", "equalWidth", "sep"],
        part,
        report,
    );
    if let Some(value) = node.attribute((namespace, "space")) {
        nonnegative_twips(node, value, part, report)?;
    }
    let count = node
        .attribute((namespace, "num"))
        .map(|v| v.parse::<u16>())
        .transpose()
        .map_err(|_| invalid(part, "malformed column count"))?
        .unwrap_or(1);
    if count == 0 || count > i16::MAX as u16 {
        return Err(invalid(
            part,
            "column count is outside the supported positive short range",
        ));
    }
    let equal = node
        .attribute((namespace, "equalWidth"))
        .map(|v| boolean(v, part))
        .transpose()?
        .unwrap_or(true);
    if let Some(value) = node.attribute((namespace, "sep")) {
        boolean(value, part)?;
    }
    let mut columns = 0;
    for child in property_children(node, namespace, part, report)? {
        if !child.has_tag_name((namespace, "col")) {
            unsupported_element(child, part, report);
            continue;
        }
        columns += 1;
        attributes(child, namespace, &["w", "space"], part, report);
        leaf(child, part, report)?;
        positive_twips(child, required(child, namespace, "w", part)?, part, report)?;
        if let Some(value) = child.attribute((namespace, "space")) {
            nonnegative_twips(child, value, part, report)?;
        }
    }
    if !equal && columns == 0 {
        return Err(invalid(
            part,
            "unequal columns require explicit column definitions",
        ));
    }
    if count != 1 || columns > 0 || !equal {
        normalized(
            node,
            part,
            report,
            "Column formatting is normalized to a single Writer text flow in source XML order. All text and original column definitions are retained.",
        );
    }
    Ok(())
}

fn document_grid(
    node: Node<'_, '_>,
    namespace: &str,
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<(), ConversionError> {
    attributes(
        node,
        namespace,
        &["type", "linePitch", "charSpace"],
        part,
        report,
    );
    leaf(node, part, report)?;
    if let Some(value) = node.attribute((namespace, "type"))
        && !matches!(value, "default" | "lines" | "linesAndChars" | "snapToChars")
    {
        return Err(invalid(part, "unknown document grid type"));
    }
    if let Some(value) = node.attribute((namespace, "linePitch")) {
        nonnegative_twips(node, value, part, report)?;
    }
    // LibreOffice writes the signed 32-bit pitch bit pattern as an unsigned
    // decimal in an inactive default grid. It is retained, never reinterpreted.
    if let Some(value) = node.attribute((namespace, "charSpace"))
        && value.parse::<i32>().is_err()
        && value.parse::<u32>().is_err()
    {
        return Err(invalid(part, "malformed document grid character pitch"));
    }
    normalized(
        node,
        part,
        report,
        "Document grid spacing uses Writer's typography and line spacing. The original grid settings are retained.",
    );
    Ok(())
}

fn page_numbering(
    node: Node<'_, '_>,
    namespace: &str,
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<(), ConversionError> {
    attributes(
        node,
        namespace,
        &["fmt", "start", "chapStyle", "chapSep"],
        part,
        report,
    );
    leaf(node, part, report)?;
    if let Some(value) = node.attribute((namespace, "fmt"))
        && !matches!(
            value,
            "decimal" | "upperRoman" | "lowerRoman" | "upperLetter" | "lowerLetter" | "none"
        )
    {
        unsupported_value(node, part, report, "page number format");
    }
    for name in ["start", "chapStyle"] {
        if let Some(value) = node.attribute((namespace, name)) {
            let value = value
                .parse::<u32>()
                .map_err(|_| invalid(part, "malformed page numbering value"))?;
            if name == "chapStyle" && !(1..=9).contains(&value) {
                return Err(invalid(
                    part,
                    "page-number chapter style must be between one and nine",
                ));
            }
        }
    }
    if let Some(value) = node.attribute((namespace, "chapSep"))
        && !matches!(value, "hyphen" | "period" | "colon" | "emDash" | "enDash")
    {
        return Err(invalid(part, "unknown page number chapter separator"));
    }
    normalized(
        node,
        part,
        report,
        "Page-number presentation uses Writer's page count; original numbering preferences are retained. Fields still require a supported content mapping.",
    );
    Ok(())
}

fn boolean(value: &str, part: &str) -> Result<bool, ConversionError> {
    match value {
        "true" | "1" | "on" => Ok(true),
        "false" | "0" | "off" => Ok(false),
        _ => Err(invalid(part, "on/off property has an invalid value")),
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
        &[
            "before",
            "after",
            "line",
            "lineRule",
            "beforeAutospacing",
            "afterAutospacing",
            "beforeLines",
            "afterLines",
        ],
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
    for name in ["beforeAutospacing", "afterAutospacing"] {
        if let Some(value) = node.attribute((namespace, name))
            && boolean(value, part)?
        {
            normalized(
                node,
                part,
                report,
                "Automatic paragraph spacing uses explicit spacing or Writer defaults; the source retains its automatic-spacing preference.",
            );
        }
    }
    for name in ["beforeLines", "afterLines"] {
        if let Some(value) = node.attribute((namespace, name)) {
            if value.parse::<u32>().is_err() {
                return Err(invalid(part, "malformed line-based paragraph spacing"));
            }
            normalized(
                node,
                part,
                report,
                "Line-based paragraph spacing uses explicit twips or Writer defaults; the original line-based value is retained.",
            );
        }
    }
    let rule = node.attribute((namespace, "lineRule"));
    if rule.is_some_and(|value| !matches!(value, "auto" | "exact" | "atLeast")) {
        return Err(invalid(part, "unknown line spacing rule"));
    }
    let Some(value) = node.attribute((namespace, "line")) else {
        if rule.is_some() {
            normalized(
                node,
                part,
                report,
                "A line rule without a measurement uses inherited Writer line spacing; the source retains its original rule.",
            );
        }
        return Ok(());
    };
    let rule = rule.unwrap_or_else(|| {
        // Consumers differ in how this partial pair inherits. Deliberately choose
        // auto rather than claiming a faithful interpretation of that ambiguity.
        normalized(node, part, report, "A line measurement without an explicit rule is normalized to automatic line spacing; consumer-specific inheritance remains in the original.");
        "auto"
    });
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
        if node.tag_name().name() == "sectPr"
            && attribute.namespace() == Some(namespace)
            && matches!(
                attribute.name(),
                "rsidR" | "rsidRPr" | "rsidSect" | "rsidDel"
            )
            && attribute.value().len() == 8
            && attribute.value().bytes().all(|c| c.is_ascii_hexdigit())
        {
            continue;
        }
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
            if child.tag_name().namespace() == Some(namespace)
                || (node.tag_name().name() == "rPr" && child.has_tag_name((WORD_2010, "ligatures")))
            {
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
    fn partial_or_different_complex_script_properties_are_normalized_with_notices() {
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
            let (_, report) = character(body).unwrap();
            assert!(report.is_supported(), "{body}");
            assert!(!report.normalizations.is_empty(), "{body}");
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
            r#"<w:u w:val="single" w:future="1"/>"#,
            r#"<w:color w:val="auto" w:unknownThemeColor="accent1"/>"#,
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
    fn partial_line_spacing_pairs_are_normalized_explicitly_without_claiming_inheritance() {
        let (properties, report) = paragraph(r#"<w:spacing w:line="300"/>"#).unwrap();
        assert_eq!(
            properties.format.line_spacing,
            Some(LineSpacing::Multiple { value: 300 })
        );
        assert!(report.is_supported());
        assert!(!report.normalizations.is_empty());
        for body in [
            r#"<w:spacing w:lineRule="exact"/>"#,
            r#"<w:spacing w:lineRule="auto"/>"#,
        ] {
            let (properties, report) = paragraph(body).unwrap();
            assert_eq!(properties.format.line_spacing, None);
            assert!(report.is_supported());
            assert!(!report.normalizations.is_empty());
        }
        let (properties, report) =
            paragraph(r#"<w:spacing w:line="300" w:lineRule="auto"/>"#).unwrap();
        assert!(report.is_supported());
        assert!(report.normalizations.is_empty());
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
        let (_, report) = paragraph(r#"<w:spacing w:beforeLines="100"/>"#).unwrap();
        assert!(report.is_supported());
        assert!(!report.normalizations.is_empty());
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
    fn paragraph_mark_unknown_styles_and_directional_alignment_remain_read_only() {
        for body in [
            r#"<w:rPr><w:rStyle w:val="unknown"/></w:rPr>"#,
            r#"<w:jc w:val="start" w:future="1"/>"#,
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
            r#"<w:type w:val="continuous" w:future="1"/>"#,
            r#"<w:cols w:num="2"><w:future/></w:cols>"#,
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
