//! Ancillary parts that do not supply editable body semantics in this profile.

use crate::{
    CompatibilityIssueKind as Issue, CompatibilityReport, ConversionError, conversion::invalid,
    xml::parse_xml,
};
use roxmltree::Node;
use std::collections::BTreeSet;

const EXT: [&str; 2] = [
    "http://schemas.openxmlformats.org/officeDocument/2006/extended-properties",
    "http://purl.oclc.org/ooxml/officeDocument/extendedProperties",
];
const CUSTOM: [&str; 2] = [
    "http://schemas.openxmlformats.org/officeDocument/2006/custom-properties",
    "http://purl.oclc.org/ooxml/officeDocument/customProperties",
];
const TYPES: [&str; 2] = [
    "http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes",
    "http://purl.oclc.org/ooxml/officeDocument/docPropsVTypes",
];
const DRAWING: [&str; 2] = [
    "http://schemas.openxmlformats.org/drawingml/2006/main",
    "http://purl.oclc.org/ooxml/drawingml/main",
];
const MC: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";
const MATH: [&str; 2] = [
    "http://schemas.openxmlformats.org/officeDocument/2006/math",
    "http://purl.oclc.org/ooxml/officeDocument/math",
];
const W14: &str = "http://schemas.microsoft.com/office/word/2010/wordml";
const W15: &str = "http://schemas.microsoft.com/office/word/2012/wordml";
const OFFICE: &str = "urn:schemas-microsoft-com:office:office";
const VML: &str = "urn:schemas-microsoft-com:vml";

/// Known roles, never inferred from a filename alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum AncillaryKind {
    ExtendedProperties,
    CustomProperties,
    Theme,
    FontTable,
    Settings,
    WebSettings,
    Thumbnail,
}

impl AncillaryKind {
    pub(crate) const fn expected_content_type(self) -> &'static str {
        match self {
            Self::ExtendedProperties => {
                "application/vnd.openxmlformats-officedocument.extended-properties+xml"
            }
            Self::CustomProperties => {
                "application/vnd.openxmlformats-officedocument.custom-properties+xml"
            }
            Self::Theme => "application/vnd.openxmlformats-officedocument.theme+xml",
            Self::FontTable => {
                "application/vnd.openxmlformats-officedocument.wordprocessingml.fontTable+xml"
            }
            Self::Settings => {
                "application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"
            }
            Self::WebSettings => {
                "application/vnd.openxmlformats-officedocument.wordprocessingml.webSettings+xml"
            }
            Self::Thumbnail => "image/jpeg",
        }
    }

    pub(crate) fn accepts_content_type(self, value: &str) -> bool {
        if self == Self::Thumbnail {
            [
                "image/jpeg",
                "image/png",
                "image/gif",
                "image/tiff",
                "image/bmp",
                "image/x-emf",
                "image/x-wmf",
                "image/x-ms-bmp",
                "image/svg+xml",
            ]
            .iter()
            .any(|media| value.eq_ignore_ascii_case(media))
        } else {
            value.eq_ignore_ascii_case(self.expected_content_type())
        }
    }

    pub(crate) fn inspect(
        self,
        part: &str,
        bytes: &[u8],
        word_namespace: &str,
        report: &mut CompatibilityReport,
    ) -> Result<(), ConversionError> {
        // A package-root cached preview is retained as opaque bytes. It is
        // never rendered, decoded or used as editable body image content.
        if self == Self::Thumbnail {
            return Ok(());
        }
        let xml = parse_xml(part, bytes)?;
        let root = xml.root_element();
        let namespaces: &[&str] = match self {
            Self::ExtendedProperties => &EXT,
            Self::CustomProperties => &CUSTOM,
            Self::Theme => &DRAWING,
            _ => std::slice::from_ref(&word_namespace),
        };
        let name = match self {
            Self::ExtendedProperties | Self::CustomProperties => "Properties",
            Self::Theme => "theme",
            Self::FontTable => "fonts",
            Self::Settings => "settings",
            Self::WebSettings => "webSettings",
            Self::Thumbnail => unreachable!(),
        };
        if root.tag_name().name() != name
            || !namespaces
                .iter()
                .any(|ns| root.tag_name().namespace() == Some(*ns))
        {
            return Err(invalid(
                part,
                "ancillary relationship target has an incorrect root or namespace",
            ));
        }
        // Theme values are never interpreted here. Theme-dependent body/style
        // properties still pass through the semantic normalization and content
        // guards; relationships out of the Theme part still traverse the graph.
        if self == Self::Theme {
            return Ok(());
        }
        attrs(root, None, &[], part, report);
        whitespace(root, part)?;
        match self {
            Self::ExtendedProperties => extended(root, part, report),
            Self::CustomProperties => custom(root, part, report),
            Self::FontTable => fonts(root, word_namespace, part, report),
            Self::Settings | Self::WebSettings => settings(
                root,
                word_namespace,
                self == Self::WebSettings,
                part,
                report,
            ),
            Self::Theme | Self::Thumbnail => unreachable!(),
        }
    }
}

fn unsupported(node: Node<'_, '_>, part: &str, report: &mut CompatibilityReport, message: &str) {
    report.push(
        Issue::UnsupportedElement,
        part,
        node.tag_name().name(),
        message,
    );
}

fn attrs(
    node: Node<'_, '_>,
    ns: Option<&str>,
    allowed: &[&str],
    part: &str,
    report: &mut CompatibilityReport,
) {
    for attr in node.attributes() {
        if attr.namespace() == Some(MC)
            && attr.name() == "Ignorable"
            && attr.value().split_whitespace().next().is_some()
            && attr
                .value()
                .split_whitespace()
                .all(|prefix| node.lookup_namespace_uri(Some(prefix)).is_some())
        {
            continue;
        }
        if attr.namespace() != ns || !allowed.contains(&attr.name()) {
            report.push(
                Issue::UnsupportedAttribute,
                part,
                format!("{}/@{}", node.tag_name().name(), attr.name()),
                "Unknown ancillary attributes cannot be discarded as inert metadata.",
            );
        }
    }
}

fn whitespace(node: Node<'_, '_>, part: &str) -> Result<(), ConversionError> {
    if node.children().any(|n| {
        n.is_text()
            && n.text()
                .is_some_and(|t| !t.chars().all(|c| matches!(c, ' ' | '\t' | '\r' | '\n')))
    }) {
        return Err(invalid(
            part,
            "unexpected text in an ancillary container or setting",
        ));
    }
    Ok(())
}

fn leaf(node: Node<'_, '_>, part: &str, report: &mut CompatibilityReport) {
    for child in node.children().filter(Node::is_element) {
        unsupported(
            child,
            part,
            report,
            "Ancillary leaf data cannot contain document content or unknown nested markup.",
        );
    }
}

fn hex(value: &str, length: usize) -> bool {
    value.len() == length && value.bytes().all(|c| c.is_ascii_hexdigit())
}
fn boolean(value: &str) -> bool {
    matches!(value, "true" | "false" | "0" | "1" | "on" | "off")
}
fn checked(value: bool, part: &str) -> Result<(), ConversionError> {
    if value {
        Ok(())
    } else {
        Err(invalid(part, "malformed known ancillary value"))
    }
}
fn val<'a>(node: Node<'a, 'a>, ns: &str, part: &str) -> Result<&'a str, ConversionError> {
    node.attribute((ns, "val"))
        .ok_or_else(|| invalid(part, "missing ancillary value"))
}

fn extended(
    root: Node<'_, '_>,
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<(), ConversionError> {
    let ns = root.tag_name().namespace();
    let mut seen = BTreeSet::new();
    for node in root.children().filter(Node::is_element) {
        if node.tag_name().namespace() != ns {
            unsupported(node, part, report, "Unknown extended-property namespace.");
            continue;
        }
        let name = node.tag_name().name();
        if !seen.insert(name) {
            return Err(invalid(part, "duplicate extended property"));
        }
        attrs(node, None, &[], part, report);
        if matches!(name, "HeadingPairs" | "TitlesOfParts" | "HLinks") {
            whitespace(node, part)?;
            for child in node.children().filter(Node::is_element) {
                typed_value(child, part, report, 0)?;
            }
        } else if matches!(
            name,
            "Template"
                | "Manager"
                | "Company"
                | "PresentationFormat"
                | "Application"
                | "AppVersion"
                | "HyperlinkBase"
                | "TotalTime"
                | "Pages"
                | "Words"
                | "Characters"
                | "Lines"
                | "Paragraphs"
                | "Slides"
                | "Notes"
                | "HiddenSlides"
                | "MMClips"
                | "DocSecurity"
                | "CharactersWithSpaces"
                | "ScaleCrop"
                | "LinksUpToDate"
                | "SharedDoc"
                | "HyperlinksChanged"
        ) {
            leaf(node, part, report);
        } else {
            unsupported(
                node,
                part,
                report,
                "This extended property is outside the inert metadata profile.",
            );
        }
    }
    Ok(())
}

fn custom(
    root: Node<'_, '_>,
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<(), ConversionError> {
    let ns = root.tag_name().namespace();
    let mut ids = BTreeSet::new();
    let mut names = BTreeSet::new();
    for node in root.children().filter(Node::is_element) {
        if node.tag_name().namespace() != ns || node.tag_name().name() != "property" {
            unsupported(node, part, report, "Unknown custom-property content.");
            continue;
        }
        attrs(node, None, &["fmtid", "pid", "name"], part, report);
        whitespace(node, part)?;
        let pid = node
            .attribute("pid")
            .and_then(|v| v.parse::<u32>().ok())
            .filter(|n| *n >= 2)
            .ok_or_else(|| invalid(part, "invalid custom-property identifier"))?;
        let name = node
            .attribute("name")
            .filter(|v| !v.trim().is_empty())
            .ok_or_else(|| invalid(part, "missing custom-property name"))?;
        if !ids.insert(pid) || !names.insert(name) {
            return Err(invalid(
                part,
                "duplicate custom-property identifier or name",
            ));
        }
        if !node.attribute("fmtid").is_some_and(|value| {
            value.eq_ignore_ascii_case("{D5CDD505-2E9C-101B-9397-08002B2CF9AE}")
        }) {
            report.push(
                Issue::UnsupportedValue,
                part,
                "property/@fmtid",
                "Only the standard user-defined custom-property format is inert in this profile.",
            );
        }
        let children: Vec<_> = node.children().filter(Node::is_element).collect();
        if children.len() != 1 {
            return Err(invalid(
                part,
                "a custom property requires exactly one typed value",
            ));
        }
        typed_value(children[0], part, report, 0)?;
    }
    Ok(())
}

fn typed_value(
    node: Node<'_, '_>,
    part: &str,
    report: &mut CompatibilityReport,
    depth: usize,
) -> Result<(), ConversionError> {
    if depth > 32 {
        return Err(invalid(
            part,
            "custom-property nesting exceeds the supported profile",
        ));
    }
    if !TYPES
        .iter()
        .any(|ns| node.tag_name().namespace() == Some(*ns))
    {
        unsupported(
            node,
            part,
            report,
            "Only known property-value namespaces are inert metadata.",
        );
        return Ok(());
    }
    let name = node.tag_name().name();
    if matches!(name, "vector" | "variant") {
        attrs(
            node,
            None,
            if name == "vector" {
                &["baseType", "size"]
            } else {
                &[]
            },
            part,
            report,
        );
        whitespace(node, part)?;
        let children: Vec<_> = node.children().filter(Node::is_element).collect();
        if name == "variant" {
            checked(children.len() == 1, part)?;
        } else {
            let size = node
                .attribute("size")
                .and_then(|v| v.parse::<usize>().ok())
                .ok_or_else(|| invalid(part, "invalid property vector size"))?;
            checked(size == children.len(), part)?;
            let base = node
                .attribute("baseType")
                .ok_or_else(|| invalid(part, "missing property vector base type"))?;
            if children.iter().any(|c| c.tag_name().name() != base) {
                return Err(invalid(
                    part,
                    "property vector entries differ from its base type",
                ));
            }
        }
        for child in children {
            typed_value(child, part, report, depth + 1)?;
        }
    } else if matches!(
        name,
        "lpstr"
            | "lpwstr"
            | "bstr"
            | "i1"
            | "i2"
            | "i4"
            | "i8"
            | "int"
            | "ui1"
            | "ui2"
            | "ui4"
            | "ui8"
            | "uint"
            | "r4"
            | "r8"
            | "decimal"
            | "bool"
            | "date"
            | "filetime"
            | "clsid"
            | "empty"
            | "null"
    ) {
        attrs(node, None, &[], part, report);
        leaf(node, part, report);
        let text: String = node
            .children()
            .filter(Node::is_text)
            .filter_map(|n| n.text())
            .collect();
        match name {
            "i1" | "i2" | "i4" | "i8" | "int" => checked(text.parse::<i64>().is_ok(), part)?,
            "ui1" | "ui2" | "ui4" | "ui8" | "uint" => checked(text.parse::<u64>().is_ok(), part)?,
            "r4" | "r8" | "decimal" => {
                checked(text.parse::<f64>().is_ok_and(f64::is_finite), part)?
            }
            "bool" => checked(matches!(text.as_str(), "true" | "false" | "1" | "0"), part)?,
            "empty" | "null" => checked(text.trim().is_empty(), part)?,
            _ => {}
        }
    } else {
        unsupported(
            node,
            part,
            report,
            "This property-value form is not inert supported metadata.",
        );
    }
    Ok(())
}

fn fonts(
    root: Node<'_, '_>,
    ns: &str,
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<(), ConversionError> {
    let mut names = BTreeSet::new();
    for font in root.children().filter(Node::is_element) {
        if !font.has_tag_name((ns, "font")) {
            unsupported(font, part, report, "Unknown font-table content.");
            continue;
        }
        attrs(font, Some(ns), &["name"], part, report);
        whitespace(font, part)?;
        let name = font
            .attribute((ns, "name"))
            .filter(|n| !n.trim().is_empty())
            .ok_or_else(|| invalid(part, "missing font-table font name"))?;
        if !names.insert(name) {
            return Err(invalid(part, "duplicate font-table font name"));
        }
        let mut seen = BTreeSet::new();
        for node in font.children().filter(Node::is_element) {
            let name = node.tag_name().name();
            if node.tag_name().namespace() != Some(ns)
                || !matches!(
                    name,
                    "charset" | "family" | "pitch" | "sig" | "panose1" | "notTrueType" | "altName"
                )
            {
                unsupported(
                    node,
                    part,
                    report,
                    "Embedded fonts and unknown font semantics require preservation read-only.",
                );
                continue;
            }
            if !seen.insert(name) {
                return Err(invalid(part, "duplicate font descriptor"));
            }
            leaf(node, part, report);
            whitespace(node, part)?;
            if name == "altName" {
                attrs(node, Some(ns), &["val"], part, report);
                checked(!val(node, ns, part)?.trim().is_empty(), part)?;
                report.normalize(part, "font/altName", "Font alternate-name substitution hints are not applied. Writer uses its pinned font catalog and reports unavailable requested families.");
                continue;
            }
            if name == "sig" {
                attrs(
                    node,
                    Some(ns),
                    &["usb0", "usb1", "usb2", "usb3", "csb0", "csb1"],
                    part,
                    report,
                );
                for attr in node.attributes().filter(|a| a.namespace() == Some(ns)) {
                    checked(hex(attr.value(), 8), part)?;
                }
            } else {
                attrs(node, Some(ns), &["val"], part, report);
                let value = node.attribute((ns, "val"));
                checked(
                    match name {
                        "charset" => value.is_some_and(|v| hex(v, 2)),
                        "panose1" => value.is_some_and(|v| hex(v, 20)),
                        "family" => value.is_some_and(|v| {
                            matches!(
                                v,
                                "auto" | "decorative" | "modern" | "roman" | "script" | "swiss"
                            )
                        }),
                        "pitch" => {
                            value.is_some_and(|v| matches!(v, "default" | "fixed" | "variable"))
                        }
                        _ => value.is_none_or(boolean),
                    },
                    part,
                )?;
            }
        }
    }
    Ok(())
}

fn settings(
    root: Node<'_, '_>,
    ns: &str,
    web: bool,
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<(), ConversionError> {
    let mut seen = BTreeSet::new();
    for node in root.children().filter(Node::is_element) {
        if !web && foreign_setting(node, part, report)? {
            if !seen.insert((node.tag_name().namespace(), node.tag_name().name())) {
                return Err(invalid(part, "duplicate document setting"));
            }
            continue;
        }
        if node.tag_name().namespace() != Some(ns) {
            unsupported(
                node,
                part,
                report,
                "Unknown settings extension is not an inert preference.",
            );
            continue;
        }
        let name = node.tag_name().name();
        if !seen.insert((Some(ns), name)) {
            return Err(invalid(part, "duplicate document setting"));
        }
        if !web && normalized_setting(node, ns, part, report)? {
            continue;
        }
        let flag = if web {
            matches!(
                name,
                "allowPNG"
                    | "optimizeForBrowser"
                    | "doNotSaveAsSingleFile"
                    | "doNotOrganizeInFolder"
                    | "doNotUseLongFileNames"
                    | "relyOnVML"
                    | "doNotRelyOnCSS"
            )
        } else {
            matches!(
                name,
                "hideSpellingErrors"
                    | "hideGrammaticalErrors"
                    | "doNotDisplayPageBoundaries"
                    | "savePreviewPicture"
            )
        };
        if flag {
            attrs(node, Some(ns), &["val"], part, report);
            leaf(node, part, report);
            whitespace(node, part)?;
            checked(node.attribute((ns, "val")).is_none_or(boolean), part)?;
            continue;
        }
        if !web && name == "rsids" {
            attrs(node, Some(ns), &[], part, report);
            whitespace(node, part)?;
            let mut root_seen = false;
            for child in node.children().filter(Node::is_element) {
                if child.has_tag_name((ns, "rsid")) || child.has_tag_name((ns, "rsidRoot")) {
                    if child.has_tag_name((ns, "rsidRoot")) {
                        checked(!root_seen, part)?;
                        root_seen = true;
                    }
                    attrs(child, Some(ns), &["val"], part, report);
                    leaf(child, part, report);
                    whitespace(child, part)?;
                    checked(hex(val(child, ns, part)?, 8), part)?;
                } else {
                    unsupported(child, part, report, "Unknown revision-session metadata.");
                }
            }
            continue;
        }
        if !web
            && matches!(
                name,
                "zoom" | "view" | "proofState" | "stylePaneSortMethod" | "stylePaneFormatFilter"
            )
        {
            leaf(node, part, report);
            whitespace(node, part)?;
            match name {
                "zoom" => {
                    attrs(node, Some(ns), &["val", "percent"], part, report);
                    if let Some(v) = node.attribute((ns, "percent")) {
                        checked(v.parse::<u32>().is_ok(), part)?;
                    }
                    if let Some(v) = node.attribute((ns, "val")) {
                        checked(
                            matches!(v, "none" | "fullPage" | "bestFit" | "textFit"),
                            part,
                        )?;
                    }
                }
                "view" => {
                    attrs(node, Some(ns), &["val"], part, report);
                    checked(
                        matches!(
                            val(node, ns, part)?,
                            "none" | "print" | "outline" | "masterPages" | "normal" | "web"
                        ),
                        part,
                    )?;
                }
                "proofState" => {
                    attrs(node, Some(ns), &["spelling", "grammar"], part, report);
                    for name in ["spelling", "grammar"] {
                        if let Some(v) = node.attribute((ns, name)) {
                            checked(matches!(v, "clean" | "dirty"), part)?;
                        }
                    }
                }
                "stylePaneFormatFilter" => {
                    attrs(node, Some(ns), &["val"], part, report);
                    if let Some(v) = node.attribute((ns, "val")) {
                        checked(hex(v, 4), part)?;
                    }
                }
                _ => {
                    attrs(node, Some(ns), &["val"], part, report);
                    let _ = val(node, ns, part)?;
                }
            }
        } else {
            unsupported(
                node,
                part,
                report,
                "This setting may affect formatting, layout, content or editing protection; it is not ignored as a harmless relationship.",
            );
        }
    }
    Ok(())
}

fn normalized_setting(
    node: Node<'_, '_>,
    ns: &str,
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<bool, ConversionError> {
    let name = node.tag_name().name();
    if name == "trackRevisions" {
        attrs(node, Some(ns), &["val"], part, report);
        leaf(node, part, report);
        whitespace(node, part)?;
        // An explicitly disabled feature does not request tracking. Missing,
        // true and unknown values are still handled by the blocking path.
        return Ok(node
            .attribute((ns, "val"))
            .is_some_and(|v| matches!(v, "false" | "0" | "off")));
    }
    let message = match name {
        "defaultTabStop" | "hyphenationZone" | "consecutiveHyphenLimit" => {
            attrs(node, Some(ns), &["val"], part, report);
            leaf(node, part, report);
            whitespace(node, part)?;
            let value = val(node, ns, part)?
                .parse::<u32>()
                .map_err(|_| invalid(part, "invalid nonnegative settings dimension"))?;
            checked(
                value <= i32::MAX as u32 && (name != "defaultTabStop" || value <= 32767),
                part,
            )?;
            "Document tab and hyphenation defaults are not applied; Writer uses its supported text and layout profile."
        }
        "autoHyphenation" | "doNotHyphenateCaps" => {
            attrs(node, Some(ns), &["val"], part, report);
            leaf(node, part, report);
            whitespace(node, part)?;
            checked(node.attribute((ns, "val")).is_none_or(boolean), part)?;
            "Automatic hyphenation preferences are not applied; line wrapping uses Writer's layout profile."
        }
        "characterSpacingControl" => {
            attrs(node, Some(ns), &["val"], part, report);
            leaf(node, part, report);
            whitespace(node, part)?;
            checked(
                matches!(
                    val(node, ns, part)?,
                    "doNotCompress" | "compressPunctuation" | "compressPunctuationAndJapaneseKana"
                ),
                part,
            )?;
            "Character-spacing compression preferences are normalized to Writer's shaping and spacing profile."
        }
        "compat" => {
            compatibility(node, ns, part, report)?;
            "Word compatibility layout switches are normalized to Writer's supported paragraph and page layout profile."
        }
        "themeFontLang" => {
            attrs(node, Some(ns), &["val", "eastAsia", "bidi"], part, report);
            leaf(node, part, report);
            whitespace(node, part)?;
            for key in ["val", "eastAsia", "bidi"] {
                if let Some(value) = node.attribute((ns, key)) {
                    checked(
                        value.is_empty()
                            || (value.len() <= 85
                                && value.split('-').all(|piece| {
                                    !piece.is_empty()
                                        && piece.bytes().all(|b| b.is_ascii_alphanumeric())
                                })),
                        part,
                    )?;
                }
            }
            "Theme font-language preferences are not applied; supported requested fonts and Writer's pinned fallback profile determine display."
        }
        "clrSchemeMapping" => {
            let slots = [
                "bg1",
                "t1",
                "bg2",
                "t2",
                "accent1",
                "accent2",
                "accent3",
                "accent4",
                "accent5",
                "accent6",
                "hyperlink",
                "followedHyperlink",
            ];
            attrs(node, Some(ns), &slots, part, report);
            leaf(node, part, report);
            whitespace(node, part)?;
            for attribute in node
                .attributes()
                .filter(|a| a.namespace() == Some(ns) && slots.contains(&a.name()))
            {
                checked(
                    matches!(
                        attribute.value(),
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
                    ),
                    part,
                )?;
            }
            "Document-level theme color remapping is not applied; supported run color normalization is used instead."
        }
        "shapeDefaults" => {
            shape_defaults(node, part, report)?;
            "Unused legacy shape defaults are not applied. Actual drawing content still requires a supported content mapping."
        }
        "decimalSymbol" | "listSeparator" => {
            attrs(node, Some(ns), &["val"], part, report);
            leaf(node, part, report);
            whitespace(node, part)?;
            checked(val(node, ns, part)?.chars().count() == 1, part)?;
            "Field-evaluation separator preferences are retained only in the original. Writer does not execute fields."
        }
        _ => return Ok(false),
    };
    report.normalize(part, format!("settings/{name}"), message);
    Ok(true)
}

fn compatibility(
    node: Node<'_, '_>,
    ns: &str,
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<(), ConversionError> {
    attrs(node, Some(ns), &[], part, report);
    whitespace(node, part)?;
    let mut seen = BTreeSet::new();
    for child in node.children().filter(Node::is_element) {
        if child.tag_name().namespace() != Some(ns) {
            unsupported(
                child,
                part,
                report,
                "Unknown compatibility namespace is not safely normalized.",
            );
            continue;
        }
        let kind = child.tag_name().name();
        if kind == "compatSetting" {
            attrs(child, Some(ns), &["name", "uri", "val"], part, report);
            leaf(child, part, report);
            whitespace(child, part)?;
            let name = child
                .attribute((ns, "name"))
                .ok_or_else(|| invalid(part, "missing compatibility-setting name"))?;
            if !seen.insert(name) {
                return Err(invalid(part, "duplicate compatibility setting"));
            }
            let uri = child.attribute((ns, "uri"));
            if uri != Some("http://schemas.microsoft.com/office/word")
                || !matches!(
                    name,
                    "compatibilityMode"
                        | "overrideTableStyleFontSizeAndJustification"
                        | "enableOpenTypeFeatures"
                        | "doNotFlipMirrorIndents"
                        | "differentiateMultirowTableHeaders"
                        | "useWord2013TrackBottomHyphenation"
                )
            {
                unsupported(
                    child,
                    part,
                    report,
                    "Unknown compatibility names or vendors are not safely normalized.",
                );
                continue;
            }
            let value = val(child, ns, part)?;
            checked(
                if name == "compatibilityMode" {
                    value.parse::<u32>().is_ok_and(|v| (11..=16).contains(&v))
                } else {
                    boolean(value)
                },
                part,
            )?;
        } else if kind == "adjustLineHeightInTable" {
            if !seen.insert(kind) {
                return Err(invalid(part, "duplicate compatibility setting"));
            }
            attrs(child, Some(ns), &["val"], part, report);
            leaf(child, part, report);
            whitespace(child, part)?;
            checked(child.attribute((ns, "val")).is_none_or(boolean), part)?;
        } else {
            unsupported(
                child,
                part,
                report,
                "This compatibility behavior is outside the safe normalization profile.",
            );
        }
    }
    Ok(())
}

fn foreign_setting(
    node: Node<'_, '_>,
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<bool, ConversionError> {
    let namespace = node.tag_name().namespace();
    let name = node.tag_name().name();
    if MATH.iter().any(|ns| namespace == Some(*ns)) && name == "mathPr" {
        math_defaults(node, namespace.expect("known namespace"), part, report)?;
        report.normalize(part,"settings/mathPr","Unused equation formatting defaults are not applied. Actual equation content is not silently converted to plain text.");
        return Ok(true);
    }
    if (namespace == Some(W14) || namespace == Some(W15)) && name == "docId" {
        let ns = namespace.expect("known namespace");
        attrs(node, Some(ns), &["val"], part, report);
        leaf(node, part, report);
        whitespace(node, part)?;
        let value = val(node, ns, part)?;
        let guid = value.strip_prefix('{').and_then(|v| v.strip_suffix('}'));
        checked(
            if ns == W14 {
                hex(value, 8)
            } else {
                guid.is_some_and(|v| {
                    v.len() == 36
                        && v.bytes().enumerate().all(|(i, b)| {
                            if [8, 13, 18, 23].contains(&i) {
                                b == b'-'
                            } else {
                                b.is_ascii_hexdigit()
                            }
                        })
                })
            },
            part,
        )?;
        return Ok(true);
    }
    if namespace == Some(W15) && name == "chartTrackingRefBased" {
        attrs(node, Some(W15), &["val"], part, report);
        leaf(node, part, report);
        whitespace(node, part)?;
        checked(node.attribute((W15, "val")).is_none_or(boolean), part)?;
        report.normalize(part,"settings/chartTrackingRefBased","Unused chart-reference preferences are not applied. Actual charts remain unsupported document content.");
        return Ok(true);
    }
    Ok(false)
}

fn math_defaults(
    node: Node<'_, '_>,
    ns: &str,
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<(), ConversionError> {
    attrs(node, Some(ns), &[], part, report);
    whitespace(node, part)?;
    let mut seen = BTreeSet::new();
    for child in node.children().filter(Node::is_element) {
        let name = child.tag_name().name();
        if child.tag_name().namespace() != Some(ns)
            || !matches!(
                name,
                "mathFont"
                    | "brkBin"
                    | "brkBinSub"
                    | "smallFrac"
                    | "dispDef"
                    | "lMargin"
                    | "rMargin"
                    | "defJc"
                    | "wrapIndent"
                    | "intLim"
                    | "naryLim"
                    | "interSp"
                    | "intraSp"
                    | "preSp"
                    | "postSp"
                    | "wrapRight"
            )
        {
            unsupported(
                child,
                part,
                report,
                "Unknown math defaults or nested equation content are not safely normalized.",
            );
            continue;
        }
        if !seen.insert(name) {
            return Err(invalid(part, "duplicate math default"));
        }
        attrs(child, Some(ns), &["val"], part, report);
        leaf(child, part, report);
        whitespace(child, part)?;
        let value = child.attribute((ns, "val"));
        let valid = match name {
            "mathFont" => value.is_some_and(|v| !v.trim().is_empty()),
            "brkBin" => value.is_some_and(|v| matches!(v, "before" | "after" | "repeat")),
            "brkBinSub" => value.is_some_and(|v| matches!(v, "--" | "-+" | "+-")),
            "smallFrac" | "dispDef" | "wrapRight" => value.is_none_or(boolean),
            "defJc" => {
                value.is_some_and(|v| matches!(v, "centerGroup" | "center" | "left" | "right"))
            }
            "intLim" | "naryLim" => value.is_some_and(|v| matches!(v, "subSup" | "undOvr")),
            _ => value.is_some_and(|v| v.parse::<u32>().is_ok_and(|n| n <= i32::MAX as u32)),
        };
        checked(valid, part)?;
    }
    Ok(())
}

fn shape_defaults(
    node: Node<'_, '_>,
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<(), ConversionError> {
    attrs(node, node.tag_name().namespace(), &[], part, report);
    whitespace(node, part)?;
    let mut seen = BTreeSet::new();
    for child in node.children().filter(Node::is_element) {
        let name = child.tag_name().name();
        if child.tag_name().namespace() != Some(OFFICE)
            || !matches!(name, "shapedefaults" | "shapelayout")
        {
            unsupported(
                child,
                part,
                report,
                "Unknown legacy shape content is not safely normalized.",
            );
            continue;
        }
        if !seen.insert(name) {
            return Err(invalid(part, "duplicate legacy shape defaults"));
        }
        shape_attributes(
            child,
            if name == "shapedefaults" {
                "spidmax"
            } else {
                ""
            },
            part,
            report,
        )?;
        whitespace(child, part)?;
        if name == "shapedefaults" {
            leaf(child, part, report);
        } else {
            let mut map = false;
            for entry in child.children().filter(Node::is_element) {
                if entry.has_tag_name((OFFICE, "idmap")) {
                    checked(!map, part)?;
                    map = true;
                    shape_attributes(entry, "data", part, report)?;
                    leaf(entry, part, report);
                    whitespace(entry, part)?;
                } else {
                    unsupported(
                        entry,
                        part,
                        report,
                        "Only inert shape identifier maps are normalized.",
                    );
                }
            }
        }
    }
    Ok(())
}

fn shape_attributes(
    node: Node<'_, '_>,
    numeric: &str,
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<(), ConversionError> {
    for attribute in node.attributes() {
        if attribute.namespace() == Some(VML) && attribute.name() == "ext" {
            checked(
                matches!(attribute.value(), "edit" | "view" | "backward"),
                part,
            )?;
        } else if attribute.namespace().is_none()
            && attribute.name() == numeric
            && !numeric.is_empty()
        {
            checked(
                attribute
                    .value()
                    .split(',')
                    .all(|v| v.parse::<u32>().is_ok()),
                part,
            )?;
        } else {
            report.push(
                Issue::UnsupportedAttribute,
                part,
                format!("{}/@{}", node.tag_name().name(), attribute.name()),
                "Unknown legacy shape attributes are not safely normalized.",
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod normalization_tests {
    use super::*;

    const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    fn inspect(body: &str) -> Result<CompatibilityReport, ConversionError> {
        let xml = format!(
            "<w:settings xmlns:w=\"{W}\" xmlns:m=\"{}\" xmlns:o=\"{OFFICE}\" xmlns:v=\"{VML}\" xmlns:w14=\"{W14}\" xmlns:w15=\"{W15}\" xmlns:mc=\"{MC}\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">{body}</w:settings>",
            MATH[0]
        );
        let mut report = CompatibilityReport::default();
        AncillaryKind::Settings.inspect("word/settings.xml", xml.as_bytes(), W, &mut report)?;
        Ok(report)
    }

    #[test]
    fn known_layout_preferences_are_normalized_with_nonblocking_notices() {
        for body in [
            "<w:defaultTabStop w:val=\"720\"/>",
            "<w:autoHyphenation w:val=\"true\"/>",
            "<w:hyphenationZone w:val=\"0\"/>",
            "<w:consecutiveHyphenLimit w:val=\"2\"/>",
            "<w:characterSpacingControl w:val=\"doNotCompress\"/>",
            "<w:themeFontLang w:val=\"en-US\" w:eastAsia=\"\"/>",
            "<w:clrSchemeMapping w:bg1=\"light1\" w:t1=\"dark1\"/>",
            "<w:decimalSymbol w:val=\".\"/>",
            "<w:listSeparator w:val=\",\"/>",
            "<w:compat><w:adjustLineHeightInTable/><w:compatSetting w:name=\"compatibilityMode\" w:uri=\"http://schemas.microsoft.com/office/word\" w:val=\"15\"/><w:compatSetting w:name=\"enableOpenTypeFeatures\" w:uri=\"http://schemas.microsoft.com/office/word\" w:val=\"1\"/></w:compat>",
        ] {
            let report = inspect(body).unwrap();
            assert!(report.is_supported(), "{body}: {report:?}");
            assert!(!report.normalizations.is_empty());
            assert!(report.normalizations.iter().all(|n|n.kind==Issue::NormalizedFormatting&&n.part=="word/settings.xml"));
        }
    }

    #[test]
    fn disabled_tracking_is_distinct_from_active_missing_or_unknown_tracking() {
        for value in ["false", "0", "off"] {
            let report = inspect(&format!("<w:trackRevisions w:val=\"{value}\"/>")).unwrap();
            assert!(report.is_supported());
            assert!(report.normalizations.is_empty());
        }
        for body in [
            "<w:trackRevisions/>",
            "<w:trackRevisions w:val=\"true\"/>",
            "<w:trackRevisions w:val=\"1\"/>",
            "<w:trackRevisions w:val=\"on\"/>",
            "<w:trackRevisions w:val=\"unknown\"/>",
        ] {
            assert!(!inspect(body).unwrap().is_supported(), "{body}");
        }
    }

    #[test]
    fn invalid_known_dimensions_and_enumerations_are_not_silently_repaired() {
        for body in [
            "<w:defaultTabStop w:val=\"-1\"/>",
            "<w:defaultTabStop w:val=\"32768\"/>",
            "<w:hyphenationZone w:val=\"2147483648\"/>",
            "<w:autoHyphenation w:val=\"maybe\"/>",
            "<w:characterSpacingControl w:val=\"unknown\"/>",
            "<w:themeFontLang w:val=\"bad--language\"/>",
            "<w:clrSchemeMapping w:t1=\"unknown\"/>",
            "<w:compat><w:compatSetting w:name=\"compatibilityMode\" w:uri=\"http://schemas.microsoft.com/office/word\" w:val=\"999\"/></w:compat>",
            "<m:mathPr><m:lMargin m:val=\"-1\"/></m:mathPr>",
        ] {
            assert!(inspect(body).is_err(), "{body}");
        }
    }

    #[test]
    fn normalization_never_hides_unknown_attributes_or_nested_content() {
        for body in [
            "<w:defaultTabStop w:val=\"720\" w:unknown=\"1\"/>",
            "<w:defaultTabStop w:val=\"720\"><w:r><w:t>content</w:t></w:r></w:defaultTabStop>",
            "<w:compat><w:compatSetting w:name=\"unknown\" w:uri=\"http://schemas.microsoft.com/office/word\" w:val=\"1\"/></w:compat>",
            "<w:compat><w:compatSetting w:name=\"compatibilityMode\" w:uri=\"urn:unknown\" w:val=\"15\"/></w:compat>",
            "<m:mathPr><m:oMath/></m:mathPr>",
            "<w:shapeDefaults><v:shape/></w:shapeDefaults>",
            "<future:setting xmlns:future=\"urn:future\"/>",
        ] {
            assert!(!inspect(body).unwrap().is_supported(), "{body}");
        }
    }

    #[test]
    fn protection_and_external_or_content_changing_settings_remain_blocking() {
        for body in [
            "<w:documentProtection w:enforcement=\"true\"/>",
            "<w:writeProtection/>",
            "<w:mailMerge/>",
            "<w:attachedTemplate r:id=\"template\"/>",
            "<w:linkStyles/>",
            "<w:updateFields w:val=\"true\"/>",
            "<w:saveThroughXslt r:id=\"transform\"/>",
            "<w:trackRevisions w:val=\"true\"/>",
            "<w:doNotTrackMoves/>",
            "<w:unknownSetting/>",
        ] {
            assert!(!inspect(body).unwrap().is_supported(), "{body}");
        }
    }

    #[test]
    fn exact_app_identifiers_and_bound_ignorable_declarations_do_not_skip_extensions() {
        let identifiers = "<w14:docId w14:val=\"1234ABCD\"/><w15:docId w15:val=\"{12345678-ABCD-1234-ABCD-123456789ABC}\"/><w15:chartTrackingRefBased/>";
        let report = inspect(identifiers).unwrap();
        assert!(report.is_supported());
        assert_eq!(report.normalizations.len(), 1);
        assert!(inspect("<w14:docId w14:val=\"bad\"/>").is_err());
        assert!(
            inspect("<w14:docId w14:val=\"1234ABCD\"/><w14:docId w14:val=\"1234ABCD\"/>").is_err()
        );
        for (attribute, body, supported) in [
            ("mc:Ignorable=\"w14 w15\"", identifiers, true),
            ("mc:Ignorable=\"w14\"", "<w14:unknown/>", false),
            ("mc:Ignorable=\"unbound\"", "", false),
        ] {
            let xml = format!(
                "<w:settings xmlns:w=\"{W}\" xmlns:w14=\"{W14}\" xmlns:w15=\"{W15}\" xmlns:mc=\"{MC}\" {attribute}>{body}</w:settings>"
            );
            let mut report = CompatibilityReport::default();
            AncillaryKind::Settings
                .inspect("word/settings.xml", xml.as_bytes(), W, &mut report)
                .unwrap();
            assert_eq!(report.is_supported(), supported);
        }
    }

    #[test]
    fn unused_math_and_legacy_shape_defaults_have_explicit_normalizations() {
        let body = "<m:mathPr><m:mathFont m:val=\"Cambria Math\"/><m:brkBin m:val=\"before\"/><m:brkBinSub m:val=\"--\"/><m:smallFrac m:val=\"0\"/><m:dispDef/><m:lMargin m:val=\"0\"/><m:rMargin m:val=\"0\"/><m:defJc m:val=\"centerGroup\"/><m:wrapIndent m:val=\"1440\"/><m:intLim m:val=\"subSup\"/><m:naryLim m:val=\"undOvr\"/></m:mathPr><w:shapeDefaults><o:shapedefaults v:ext=\"edit\" spidmax=\"1026\"/><o:shapelayout v:ext=\"edit\"><o:idmap v:ext=\"edit\" data=\"1\"/></o:shapelayout></w:shapeDefaults>";
        let report = inspect(body).unwrap();
        assert!(report.is_supported());
        assert_eq!(report.normalizations.len(), 2);
    }

    #[test]
    fn font_alias_normalization_does_not_enable_embedded_fonts_or_hidden_text() {
        for (body, supported) in [
            ("<w:altName w:val=\"Times\"/>", true),
            ("<w:embedRegular/>", false),
            (
                "<w:altName w:val=\"Times\"><w:r><w:t>hidden</w:t></w:r></w:altName>",
                false,
            ),
        ] {
            let xml = format!(
                "<w:fonts xmlns:w=\"{W}\"><w:font w:name=\"Times New Roman\">{body}</w:font></w:fonts>"
            );
            let mut report = CompatibilityReport::default();
            AncillaryKind::FontTable
                .inspect("word/fontTable.xml", xml.as_bytes(), W, &mut report)
                .unwrap();
            assert_eq!(report.is_supported(), supported);
            if supported {
                assert_eq!(report.normalizations.len(), 1);
            }
        }
    }
}
