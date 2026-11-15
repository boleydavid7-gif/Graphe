//! Bounded recognition of ordinary producer defaults. This is deliberately not
//! an ignore-unknown policy: body content and unfamiliar shapes remain guarded.

use super::{FO, OFFICE, STYLE, SVG, TEXT, package, properties};
use crate::{FormatError, ImportNotices};
use roxmltree::{Attribute, Node};
use std::collections::{BTreeMap, BTreeSet};

pub(super) const DRAW: &str = "urn:oasis:names:tc:opendocument:xmlns:drawing:1.0";
const CONFIG: &str = "urn:oasis:names:tc:opendocument:xmlns:config:1.0";
const META: &str = "urn:oasis:names:tc:opendocument:xmlns:meta:1.0";
const TABLE: &str = "urn:oasis:names:tc:opendocument:xmlns:table:1.0";
const LOEXT: &str = "urn:org:documentfoundation:names:experimental:office:xmlns:loext:1.0";
const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
const PKG: &str = "http://docs.oasis-open.org/ns/office/1.2/meta/pkg#";

fn attrs(node: Node<'_, '_>, names: &[(&str, &str)]) -> bool {
    node.attributes()
        .all(|a| names.contains(&(a.namespace().unwrap_or(""), a.name())))
}
fn blank(node: Node<'_, '_>) -> bool {
    node.children()
        .all(|n| !n.is_element() && n.text().is_none_or(|t| t.trim().is_empty()))
}
fn no_text(node: Node<'_, '_>) -> bool {
    node.children()
        .all(|n| !n.is_text() || n.text().is_none_or(|t| t.trim().is_empty()))
}
fn boolean(s: &str) -> bool {
    matches!(s, "true" | "false")
}
fn color(s: &str) -> bool {
    s.strip_prefix('#')
        .is_some_and(|s| s.len() == 6 && s.bytes().all(|b| b.is_ascii_hexdigit()))
}
fn positive_length(s: &str) -> bool {
    properties::length(s).is_ok_and(|n| n.0 > 0)
}
fn token(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}

/// Only formatting properties whose semantics are known may be normalized.
/// The meaningful flag is conservative for active paragraph/text defaults.
pub(super) fn property(
    node: Node<'_, '_>,
    attr: Attribute<'_, '_>,
    notices: &mut ImportNotices,
    part: &str,
) -> bool {
    let value = attr.value();
    let kind = match (node.tag_name().name(), attr.namespace(), attr.name()) {
        ("text-properties", Some(FO), "language" | "country") if token(value) => Some(true),
        ("text-properties", Some(STYLE), "font-name-asian" | "font-name-complex")
            if !value.is_empty() =>
        {
            Some(true)
        }
        (
            "text-properties",
            Some(STYLE),
            "language-asian" | "country-asian" | "language-complex" | "country-complex",
        ) if token(value) => Some(true),
        ("text-properties", Some(STYLE), "font-size-asian" | "font-size-complex")
            if positive_length(value) =>
        {
            Some(true)
        }
        ("text-properties", Some(LOEXT), "opacity")
            if value == "100%"
                || (value == "0%"
                    && node.attribute((STYLE, "use-window-font-color")) == Some("true")) =>
        {
            Some(false)
        }
        (
            "paragraph-properties",
            Some(STYLE),
            "contextual-spacing" | "font-independent-line-spacing",
        ) if value == "false" => Some(false),
        ("paragraph-properties", Some(STYLE), "text-autospace")
            if matches!(value, "none" | "ideograph-alpha") =>
        {
            Some(value != "none")
        }
        ("paragraph-properties", Some(STYLE), "punctuation-wrap")
            if matches!(value, "simple" | "hanging") =>
        {
            Some(true)
        }
        ("paragraph-properties", Some(STYLE), "line-break")
            if matches!(value, "normal" | "strict") =>
        {
            Some(true)
        }
        ("paragraph-properties" | "page-layout-properties", Some(STYLE), "writing-mode")
            if matches!(value, "lr-tb" | "page") =>
        {
            Some(true)
        }
        ("paragraph-properties", Some(STYLE), "page-number") if value == "auto" => Some(false),
        ("page-layout-properties", Some(STYLE), "num-format") if value == "1" => Some(false),
        ("page-layout-properties", Some(STYLE), "footnote-max-height")
            if properties::length(value).is_ok_and(|n| n.0 == 0) =>
        {
            Some(false)
        }
        ("page-layout-properties", Some(LOEXT), "margin-gutter")
            if properties::length(value).is_ok_and(|n| n.0 == 0) =>
        {
            Some(false)
        }
        ("page-layout-properties", Some(STYLE), "layout-grid-mode") if value == "none" => {
            Some(false)
        }
        ("page-layout-properties", Some(STYLE), name)
            if node.attribute((STYLE, "layout-grid-mode")) == Some("none") =>
        {
            match name {
                "layout-grid-color" if color(value) => Some(false),
                "layout-grid-lines" if value.parse::<u16>().is_ok() => Some(false),
                "layout-grid-base-height" | "layout-grid-ruby-height" if positive_length(value) => {
                    Some(false)
                }
                "layout-grid-ruby-below" | "layout-grid-print" | "layout-grid-display"
                    if boolean(value) =>
                {
                    Some(false)
                }
                _ => None,
            }
        }
        _ => None,
    };
    if let Some(meaningful) = kind {
        notices.record(part, format!("{}/@{}", node.tag_name().name(), attr.name()), if meaningful {
            "Known ODF language, script-font, direction or typography preferences use Writer's supported formatting; appearance may differ. The exact original is retained."
        } else {
            "An inactive ODF formatting default is retained in the exact original."
        }, meaningful);
        true
    } else {
        false
    }
}

pub(super) fn property_child(node: Node<'_, '_>) -> bool {
    if node.has_tag_name((STYLE, "tab-stops")) {
        return node
            .parent()
            .is_some_and(|n| n.has_tag_name((STYLE, "paragraph-properties")))
            && node.attributes().len() == 0
            && blank(node);
    }
    node.has_tag_name((STYLE, "footnote-sep"))
        && node
            .parent()
            .is_some_and(|n| n.has_tag_name((STYLE, "page-layout-properties")))
        && blank(node)
        && attrs(
            node,
            &[
                (STYLE, "width"),
                (STYLE, "distance-before-sep"),
                (STYLE, "distance-after-sep"),
                (STYLE, "line-style"),
                (STYLE, "adjustment"),
                (STYLE, "rel-width"),
                (STYLE, "color"),
            ],
        )
        && node.attributes().all(|a| match a.name() {
            "width" | "distance-before-sep" | "distance-after-sep" => {
                properties::length(a.value()).is_ok_and(|n| n.0 >= 0)
            }
            "line-style" => matches!(a.value(), "none" | "solid"),
            "adjustment" => matches!(a.value(), "left" | "center" | "right"),
            "rel-width" => a
                .value()
                .strip_suffix('%')
                .and_then(|v| v.parse::<u8>().ok())
                .is_some_and(|n| n <= 100),
            "color" => color(a.value()),
            _ => false,
        })
}

fn inactive_default(node: Node<'_, '_>) -> bool {
    let Some(family) = node.attribute((STYLE, "family")) else {
        return false;
    };
    if !attrs(node, &[(STYLE, "family")]) || !no_text(node) {
        return false;
    }
    match family {
        "table" => node.children().filter(Node::is_element).all(|n| {
            n.has_tag_name((STYLE, "table-properties"))
                && attrs(n, &[(TABLE, "border-model")])
                && n.attribute((TABLE, "border-model"))
                    .is_none_or(|v| matches!(v, "collapsing" | "separating"))
                && blank(n)
        }),
        "table-row" => node.children().filter(Node::is_element).all(|n| {
            n.has_tag_name((STYLE, "table-row-properties"))
                && attrs(n, &[(FO, "keep-together")])
                && n.attribute((FO, "keep-together"))
                    .is_none_or(|v| v == "auto")
                && blank(n)
        }),
        "graphic" => node.children().filter(Node::is_element).all(|n| {
            if !no_text(n) {
                return false;
            }
            if n.has_tag_name((STYLE, "graphic-properties")) {
                return blank(n)
                    && attrs(
                        n,
                        &[
                            (SVG, "stroke-color"),
                            (DRAW, "fill-color"),
                            (FO, "wrap-option"),
                            (DRAW, "shadow-offset-x"),
                            (DRAW, "shadow-offset-y"),
                            (DRAW, "start-line-spacing-horizontal"),
                            (DRAW, "start-line-spacing-vertical"),
                            (DRAW, "end-line-spacing-horizontal"),
                            (DRAW, "end-line-spacing-vertical"),
                            (STYLE, "flow-with-text"),
                        ],
                    )
                    && n.attributes().all(|a| match a.name() {
                        "stroke-color" | "fill-color" => color(a.value()),
                        "wrap-option" => a.value() == "no-wrap",
                        "flow-with-text" => a.value() == "false",
                        _ => properties::length(a.value()).is_ok(),
                    });
            }
            if !n.has_tag_name((STYLE, "text-properties"))
                && !n.has_tag_name((STYLE, "paragraph-properties"))
            {
                return false;
            }
            // The same strict property parser checks these unused defaults, but
            // their notices do not describe active body formatting.
            let mut report = Vec::new();
            let mut notices = ImportNotices::default();
            let result = if n.has_tag_name((STYLE, "paragraph-properties")) {
                properties::paragraph(n, "styles.xml", &mut report, &mut notices).map(|_| ())
            } else {
                let fonts: BTreeMap<_, _> = n
                    .document()
                    .descendants()
                    .filter(|x| x.has_tag_name((STYLE, "font-face")))
                    .filter_map(|x| properties::font_face(x).ok())
                    .collect();
                properties::character(n, "styles.xml", &fonts, &mut report, &mut notices)
                    .map(|_| ())
            };
            result.is_ok() && report.is_empty()
        }),
        _ => false,
    }
}

pub(super) fn scaffolding(node: Node<'_, '_>) -> bool {
    if node.has_tag_name((STYLE, "default-style")) {
        return inactive_default(node);
    }
    if node.has_tag_name((TEXT, "notes-configuration")) {
        return blank(node)
            && attrs(
                node,
                &[
                    (TEXT, "note-class"),
                    (STYLE, "num-format"),
                    (TEXT, "start-value"),
                    (TEXT, "footnotes-position"),
                    (TEXT, "start-numbering-at"),
                ],
            )
            && matches!(
                node.attribute((TEXT, "note-class")),
                Some("footnote" | "endnote")
            )
            && node
                .attribute((STYLE, "num-format"))
                .is_none_or(|v| matches!(v, "1" | "i" | "I" | "a" | "A" | ""))
            && node
                .attribute((TEXT, "start-value"))
                .is_none_or(|v| v.parse::<u32>().is_ok())
            && node
                .attribute((TEXT, "footnotes-position"))
                .is_none_or(|v| v == "page")
            && node
                .attribute((TEXT, "start-numbering-at"))
                .is_none_or(|v| v == "document");
    }
    if node.has_tag_name((TEXT, "linenumbering-configuration")) {
        return node.attribute((TEXT, "number-lines")) == Some("false")
            && blank(node)
            && attrs(
                node,
                &[
                    (TEXT, "number-lines"),
                    (TEXT, "offset"),
                    (STYLE, "num-format"),
                    (TEXT, "number-position"),
                    (TEXT, "increment"),
                ],
            )
            && node
                .attribute((TEXT, "offset"))
                .is_none_or(|v| properties::length(v).is_ok_and(|n| n.0 >= 0))
            && node
                .attribute((TEXT, "increment"))
                .is_none_or(|v| v.parse::<u16>().is_ok_and(|n| n > 0))
            && node
                .attribute((TEXT, "number-position"))
                .is_none_or(|v| matches!(v, "left" | "right" | "inner" | "outer"))
            && node
                .attribute((STYLE, "num-format"))
                .is_none_or(|v| matches!(v, "1" | "i" | "I" | "a" | "A"));
    }
    if node.has_tag_name((TEXT, "outline-style")) {
        let mut levels = BTreeSet::new();
        return attrs(node, &[(STYLE, "name")])
            && no_text(node)
            && node.children().filter(Node::is_element).all(|n| {
                n.has_tag_name((TEXT, "outline-level-style"))
                    && attrs(n, &[(TEXT, "level"), (STYLE, "num-format")])
                    && n.attribute((STYLE, "num-format")) == Some("")
                    && n.attribute((TEXT, "level"))
                        .and_then(|v| v.parse::<u8>().ok())
                        .is_some_and(|n| (1..=10).contains(&n) && levels.insert(n))
                    && no_text(n)
                    && n.children().filter(Node::is_element).all(|p| {
                        p.has_tag_name((STYLE, "list-level-properties"))
                            && attrs(p, &[(TEXT, "list-level-position-and-space-mode")])
                            && p.attribute((TEXT, "list-level-position-and-space-mode"))
                                == Some("label-alignment")
                            && no_text(p)
                            && p.children().filter(Node::is_element).all(|a| {
                                a.has_tag_name((STYLE, "list-level-label-alignment"))
                                    && attrs(a, &[(TEXT, "label-followed-by")])
                                    && a.attribute((TEXT, "label-followed-by")) == Some("nothing")
                                    && blank(a)
                            })
                    })
            });
    }
    if node.has_tag_name((LOEXT, "theme")) {
        return attrs(node, &[(LOEXT, "name")])
            && no_text(node)
            && node.children().filter(Node::is_element).all(|n| {
                n.has_tag_name((LOEXT, "theme-colors"))
                    && attrs(n, &[(LOEXT, "name")])
                    && no_text(n)
                    && n.children().filter(Node::is_element).all(|c| {
                        c.has_tag_name((LOEXT, "color"))
                            && attrs(c, &[(LOEXT, "name"), (LOEXT, "color")])
                            && c.attribute((LOEXT, "color")).is_some_and(color)
                            && c.attribute((LOEXT, "name")).is_some_and(|s| {
                                matches!(
                                    s,
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
                                        | "followed-hyperlink"
                                )
                            })
                            && blank(c)
                    })
            });
    }
    false
}

pub(super) fn section_style(node: Node<'_, '_>) -> bool {
    attrs(node, &[(STYLE, "name"), (STYLE, "family")])
        && no_text(node)
        && node.children().filter(Node::is_element).all(|p| {
            p.has_tag_name((STYLE, "section-properties"))
                && attrs(p, &[(STYLE, "editable")])
                && p.attribute((STYLE, "editable")).is_none_or(boolean)
                && no_text(p)
                && p.children().filter(Node::is_element).count() <= 1
                && p.children().filter(Node::is_element).all(|c| {
                    c.has_tag_name((STYLE, "columns"))
                        && attrs(c, &[(FO, "column-count"), (FO, "column-gap")])
                        && c.attribute((FO, "column-count")) == Some("1")
                        && c.attribute((FO, "column-gap"))
                            .is_none_or(|v| properties::length(v).is_ok_and(|n| n.0 == 0))
                        && blank(c)
                })
        })
}
pub(super) fn drawing_style(node: Node<'_, '_>) -> bool {
    attrs(node, &[(STYLE, "name"), (STYLE, "family")])
        && no_text(node)
        && node.children().filter(Node::is_element).all(|p| {
            p.has_tag_name((STYLE, "drawing-page-properties"))
                && attrs(p, &[(DRAW, "background-size")])
                && p.attribute((DRAW, "background-size"))
                    .is_none_or(|v| v == "full")
                && blank(p)
        })
}
pub(super) fn sequence_declarations(node: Node<'_, '_>) -> bool {
    let mut names = BTreeSet::new();
    node.has_tag_name((TEXT, "sequence-decls"))
        && node.attributes().len() == 0
        && no_text(node)
        && node.children().filter(Node::is_element).all(|n| {
            n.has_tag_name((TEXT, "sequence-decl"))
                && attrs(n, &[(TEXT, "name"), (TEXT, "display-outline-level")])
                && n.attribute((TEXT, "name"))
                    .is_some_and(|s| !s.is_empty() && names.insert(s))
                && n.attribute((TEXT, "display-outline-level"))
                    .is_none_or(|s| s == "0")
                && blank(n)
        })
}
pub(super) fn generated_metadata(node: Node<'_, '_>) -> bool {
    if node.has_tag_name((META, "generator")) {
        return node.attributes().len() == 0 && !node.children().any(|n| n.is_element());
    }
    node.has_tag_name((META, "document-statistic"))
        && blank(node)
        && node.attributes().all(|a| {
            a.namespace() == Some(META)
                && matches!(
                    a.name(),
                    "table-count"
                        | "image-count"
                        | "object-count"
                        | "page-count"
                        | "paragraph-count"
                        | "word-count"
                        | "character-count"
                        | "non-whitespace-character-count"
                )
                && a.value().parse::<u64>().is_ok()
        })
}

pub(super) fn package_part(
    name: &str,
    parts: &BTreeMap<String, Vec<u8>>,
    manifest: &roxmltree::Document<'_>,
    notices: &mut ImportNotices,
) -> Result<bool, FormatError> {
    let bytes = &parts[name];
    let media = manifest
        .descendants()
        .find(|n| {
            n.has_tag_name((super::MANIFEST, "file-entry"))
                && n.attribute((super::MANIFEST, "full-path")) == Some(name)
        })
        .and_then(|n| n.attribute((super::MANIFEST, "media-type")));
    let expected = match name {
        "Configurations2/" => "application/vnd.sun.xml.ui.configuration",
        "Thumbnails/thumbnail.png" => "image/png",
        "manifest.rdf" => "application/rdf+xml",
        "settings.xml" => "text/xml",
        _ => return Ok(false),
    };
    if media != Some(expected) {
        return Ok(false);
    }
    let recognized = match name {
        "Configurations2/" => bytes.is_empty(),
        "Thumbnails/thumbnail.png" => {
            bytes.starts_with(b"\x89PNG\r\n\x1a\n")
                && bytes.len() >= 20
                && bytes.ends_with(b"IEND\xaeB`\x82")
        }
        "manifest.rdf" => rdf(&package::xml(name, bytes)?),
        "settings.xml" => settings(&package::xml(name, bytes)?, notices),
        _ => false,
    };
    if recognized {
        notices.record(name,"package","Known application preferences, package descriptors and previews are retained in the exact original.",false);
    }
    Ok(recognized)
}
fn rdf(document: &roxmltree::Document<'_>) -> bool {
    let root = document.root_element();
    root.has_tag_name((RDF, "RDF"))
        && root.attributes().len() == 0
        && no_text(root)
        && root.children().filter(Node::is_element).all(|n| {
            n.has_tag_name((RDF, "Description"))
                && attrs(n, &[(RDF, "about")])
                && matches!(
                    n.attribute((RDF, "about")),
                    Some("" | "content.xml" | "styles.xml")
                )
                && no_text(n)
                && n.children().filter(Node::is_element).all(|c| {
                    blank(c)
                        && attrs(c, &[(RDF, "resource")])
                        && matches!(
                            (
                                n.attribute((RDF, "about")),
                                c.tag_name().namespace(),
                                c.tag_name().name(),
                                c.attribute((RDF, "resource"))
                            ),
                            (
                                Some("content.xml"),
                                Some(RDF),
                                "type",
                                Some(
                                    "http://docs.oasis-open.org/ns/office/1.2/meta/odf#ContentFile"
                                )
                            ) | (
                                Some("styles.xml"),
                                Some(RDF),
                                "type",
                                Some(
                                    "http://docs.oasis-open.org/ns/office/1.2/meta/odf#StylesFile"
                                )
                            ) | (
                                Some(""),
                                Some(RDF),
                                "type",
                                Some("http://docs.oasis-open.org/ns/office/1.2/meta/pkg#Document")
                            ) | (
                                Some(""),
                                Some(PKG),
                                "hasPart",
                                Some("content.xml" | "styles.xml")
                            )
                        )
                })
        })
}

fn settings(document: &roxmltree::Document<'_>, notices: &mut ImportNotices) -> bool {
    let root = document.root_element();
    if !root.has_tag_name((OFFICE, "document-settings"))
        || !attrs(root, &[(OFFICE, "version")])
        || !matches!(
            root.attribute((OFFICE, "version")),
            Some("1.2" | "1.3" | "1.4")
        )
        || !no_text(root)
    {
        return false;
    }
    let mut containers = root.children().filter(Node::is_element);
    let Some(container) = containers.next() else {
        return false;
    };
    if containers.next().is_some()
        || !container.has_tag_name((OFFICE, "settings"))
        || container.attributes().len() != 0
        || !no_text(container)
    {
        return false;
    }
    let mut sets = BTreeSet::new();
    for set in container.children().filter(Node::is_element) {
        if !set.has_tag_name((CONFIG, "config-item-set")) {
            return false;
        }
        let Some(name) = set.attribute((CONFIG, "name")) else {
            return false;
        };
        if !matches!(name, "ooo:view-settings" | "ooo:configuration-settings")
            || !sets.insert(name)
            || !attrs(set, &[(CONFIG, "name")])
            || !no_text(set)
        {
            return false;
        }
        let mut names = BTreeSet::new();
        for item in set.children().filter(Node::is_element) {
            if !item.has_tag_name((CONFIG, "config-item"))
                || !attrs(item, &[(CONFIG, "name"), (CONFIG, "type")])
                || item.children().any(|node| node.is_element())
            {
                return false;
            }
            let Some(key) = item.attribute((CONFIG, "name")) else {
                return false;
            };
            if !names.insert(key) {
                return false;
            }
            let value = item
                .children()
                .filter(Node::is_text)
                .filter_map(|node| node.text())
                .collect::<String>();
            let Some(meaningful) = setting(
                key,
                item.attribute((CONFIG, "type")).unwrap_or(""),
                &value,
                name == "ooo:view-settings",
            ) else {
                return false;
            };
            let message = if meaningful {
                "Known LibreOffice layout preferences use Writer's layout behavior; pagination or spacing may differ. The exact original is retained."
            } else {
                "Known LibreOffice application preferences are retained in the exact original."
            };
            notices.record("settings.xml", key, message, meaningful);
        }
    }
    true
}

fn setting(key: &str, kind: &str, value: &str, view: bool) -> Option<bool> {
    if view {
        return match (key, kind) {
            ("ViewAreaTop" | "ViewAreaLeft" | "ViewAreaWidth" | "ViewAreaHeight", "long")
                if value.parse::<i64>().is_ok() =>
            {
                Some(false)
            }
            ("ShowRedlineChanges" | "InBrowseMode", "boolean") if boolean(value) => Some(false),
            _ => None,
        };
    }
    match (key, kind) {
        ("LoadReadonly" | "ProtectForm" | "ProtectFields" | "ProtectBookmarks", "boolean")
            if value == "false" =>
        {
            Some(false)
        }
        ("RedlineProtectionKey" | "PrinterSetup", "base64Binary") if value.is_empty() => {
            Some(false)
        }
        (
            "CurrentDatabaseDataSource" | "CurrentDatabaseCommand" | "EmbeddedDatabaseName",
            "string",
        ) if value.is_empty() => Some(false),
        ("CurrentDatabaseCommandType", "int") if value == "0" => Some(false),
        ("PrinterName" | "PrintFaxName", "string") => Some(false),
        ("PrinterIndependentLayout", "string")
            if matches!(value, "high-resolution" | "low-resolution" | "disabled") =>
        {
            Some(true)
        }
        ("Rsid" | "RsidRoot", "int") if value.parse::<i32>().is_ok() => Some(false),
        ("PrintAnnotationMode" | "LinkUpdateMode" | "CharacterCompressionType", "short")
            if value.parse::<i16>().is_ok() =>
        {
            Some(true)
        }
        ("ImagePreferredDPI", "int") if value.parse::<i32>().is_ok_and(|dpi| dpi >= 0) => {
            Some(false)
        }
        (_, "boolean")
            if boolean(value) && BOOLEAN_SETTINGS.split_whitespace().any(|s| s == key) =>
        {
            Some(!INERT_SETTINGS.split_whitespace().any(|s| s == key))
        }
        _ => None,
    }
}

// Exact known LibreOffice preferences, not a namespace-wide ignore list.
const BOOLEAN_SETTINGS: &str = "
PrintProspectRTL PrintPageBackground PrintDrawings PrintReversed PrintHiddenText PrintEmptyPages
ListLabelAlignmentIgnoresDirection AdjustTableLineHeightsToGridHeight
BalanceSpacesAndIdeographicSpaces MsWordCompGridMetrics PaintHellOverHeaderFooter
ApplyParagraphMarkFormatToEmptyLineAtEndOfParagraph PrintGraphics NoNumberingShowFollowBy
MinRowHeightInclBorder HyphenateURLs FootnoteInColumnToPageEnd GutterAtTop ContinuousEndnotes
MsWordUlTrailSpace FrameAutowidthWithMorePara SubtractFlysAnchoredAtFlys SurroundTextWrapSmall
PrintControls TreatSingleColumnBreakAsPageBreak TabOverSpacing
AutoFirstLineIndentDisregardLineSpace TabOverMargin EmbedComplexScriptFonts
EmbedAsianScriptFonts PrintTextPlaceholder ApplyTextAttrToEmptyLineAtEndOfParagraph
EmbedSystemFonts EmbedFonts NoClippingWithWrapPolygon DisableOffPagePositioning
BackgroundParaOverDrawings TabOverflow SmallCapsPercentage66 AllowPrintJobCancel
AddVerticalFrameOffsets AddFrameOffsets PrintBlackFonts TableRowKeep
ApplyParagraphMarkFormatToNumbering JustifyLinesWithShrinking PrintProspect
CollapseEmptyCellPara PrintPaperFromSetup SaveThumbnail UnbreakableNumberings
SaveGlobalDocumentLinks MsWordCompTrailingBlanks PrintTables PrintLeftPages AddParaTableSpacing
PrinterPaperFromSetup ChartAutoUpdate OutlineLevelYieldsNumbering
ForceTopAlignmentInCellWithFloatingAnchor UseOldNumbering AddParaSpacingToTableCells
FieldAutoUpdate PropLineSpacingShrinksFirstLine TabAtLeftIndentForParagraphsInList
PrintRightPages DoNotCaptureDrawObjsOnPage HeaderSpacingBelowLastPara SaveVersionOnClose
UseFormerLineSpacing AddParaLineSpacingToTableCells IsKernAsianPunctuation
DoNotJustifyLinesWithManualBreak IsLabelDocument EmbedLatinScriptFonts UseVariableWidthNBSP
EmbedOnlyUsedFonts ApplyUserData AddParaTableSpacingAtStart InvertBorderSpacing
AddExternalLeading UseFormerObjectPositioning UnxForceZeroExtLeading UseFormerTextWrapping
ConsiderTextWrapOnObjPos StylesNoDefault IgnoreFirstLineIndentInNumbering NoGapAfterNoteNumber
EmptyDbFieldHidesPara DoNotResetParaAttrsForNumFont HiddenParagraphMarkPerLineProperties
IgnoreTabsAndBlanksForLineCalculation ClippedPictures MathBaselineAlignment AlignTabStopPosition
IgnoreHiddenCharsForLineCalculation ClipAsCharacterAnchoredWriterFlyFrames DropCapPunctuation
UseOldPrinterMetrics TabsRelativeToIndent UpdateFromTemplate MsWordCompMinLineHeightByFly
";
const INERT_SETTINGS: &str = "
PrintDrawings PrintGraphics PrintControls EmbedComplexScriptFonts EmbedAsianScriptFonts
EmbedLatinScriptFonts EmbedSystemFonts EmbedFonts EmbedOnlyUsedFonts AllowPrintJobCancel
PrintPaperFromSetup PrinterPaperFromSetup SaveThumbnail SaveGlobalDocumentLinks ChartAutoUpdate
FieldAutoUpdate SaveVersionOnClose ApplyUserData UpdateFromTemplate
";
