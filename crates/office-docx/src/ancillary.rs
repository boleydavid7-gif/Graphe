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
        // Theme values are never interpreted here. Every theme-dependent body,
        // style/default property remains rejected by the semantic parser, and
        // all relationships out of the Theme part still traverse the graph.
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
                    "charset" | "family" | "pitch" | "sig" | "panose1" | "notTrueType"
                )
            {
                unsupported(
                    node,
                    part,
                    report,
                    "Alternate names, embedded fonts and unknown font semantics require preservation read-only.",
                );
                continue;
            }
            if !seen.insert(name) {
                return Err(invalid(part, "duplicate font descriptor"));
            }
            leaf(node, part, report);
            whitespace(node, part)?;
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
        if !seen.insert(name) {
            return Err(invalid(part, "duplicate document setting"));
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
