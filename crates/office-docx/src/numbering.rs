//! Bounded, conservative first-level OOXML numbering parser.
//! Unsupported definitions remain in the exact original and block editable import.

use std::collections::BTreeMap;

use office_core::ListKind;
use roxmltree::Node;

use crate::{
    CompatibilityIssueKind as Issue, CompatibilityReport, ConversionError,
    conversion::invalid, xml::parse_xml,
};

fn field(
    node: Node<'_, '_>,
    ns: &str,
    attr: &str,
    part: &str,
) -> Result<String, ConversionError> {
    node.attribute((ns, attr)).map(str::to_owned)
        .ok_or_else(|| invalid(part, "missing required numbering attribute"))
}

fn unsupported(report: &mut CompatibilityReport, part: &str, path: &str) {
    report.push(Issue::UnsupportedElement, part, path,
        "This Word numbering feature is outside Graphe's first-level list profile.");
}

/// Maps Word numbering instance IDs to supported one-level semantic list kinds.
pub(crate) fn parse_numbering(
    part: &str,
    bytes: &[u8],
    namespace: &str,
    report: &mut CompatibilityReport,
) -> Result<BTreeMap<u32, ListKind>, ConversionError> {
    let xml = parse_xml(part, bytes)?;
    let root = xml.root_element();
    if !root.has_tag_name((namespace, "numbering")) {
        return Err(invalid(part, "numbering part has the wrong root namespace"));
    }
    let mut abstracts = BTreeMap::new();
    let mut instances = BTreeMap::new();
    for node in root.children().filter(|node| node.is_element()) {
        if node.tag_name().namespace() != Some(namespace) {
            unsupported(report, part, "numbering/extension");
            continue;
        }
        match node.tag_name().name() {
            "abstractNum" => {
                let id: u32 = field(node, namespace, "abstractNumId", part)?
                    .parse().map_err(|_| invalid(part, "invalid abstract numbering id"))?;
                if abstracts.contains_key(&id) {
                    return Err(invalid(part, "duplicate abstract numbering id"));
                }
                let mut kind = None;
                let mut level_seen = false;
                let mut valid_level = true;
                for child in node.children().filter(|node| node.is_element()) {
                    if child.tag_name().namespace() != Some(namespace) {
                        unsupported(report, part, "abstractNum/extension");
                        continue;
                    }
                    match child.tag_name().name() {
                        "multiLevelType" => {
                            if field(child, namespace, "val", part)? != "singleLevel" {
                                unsupported(report, part, "abstractNum/multiLevelType");
                            }
                        }
                        "lvl" => {
                            if level_seen {
                                unsupported(report, part, "abstractNum/multipleLevels");
                                continue;
                            }
                            level_seen = true;
                            if field(child, namespace, "ilvl", part)? != "0" {
                                valid_level = false;
                                unsupported(report, part, "abstractNum/level");
                            }
                            let mut fmt = None;
                            let mut label = None;
                            let mut start = None;
                            for property in child.children().filter(|node| node.is_element()) {
                                if property.tag_name().namespace() != Some(namespace) {
                                    unsupported(report, part, "lvl/extension");
                                    continue;
                                }
                                match property.tag_name().name() {
                                    "numFmt" => fmt = Some(field(property, namespace, "val", part)?),
                                    "lvlText" => label = Some(field(property, namespace, "val", part)?),
                                    "start" => start = Some(field(property, namespace, "val", part)?),
                                    "lvlJc" if field(property, namespace, "val", part)? == "left" => {}
                                    _ => unsupported(report, part, "lvl/unsupportedProperty"),
                                }
                            }
                            if start.as_deref() != Some("1") {
                                valid_level = false;
                                unsupported(report, part, "lvl/start");
                            }
                            kind = match (fmt.as_deref(), label.as_deref()) {
                                (Some("bullet"), Some("•")) => Some(ListKind::Bullet),
                                (Some("decimal"), Some("%1.")) => Some(ListKind::Numbered),
                                _ => {
                                    unsupported(report, part, "lvl/format");
                                    None
                                }
                            };
                        }
                        _ => unsupported(report, part, "abstractNum/unsupportedProperty"),
                    }
                }
                if !level_seen || !valid_level {
                    unsupported(report, part, "abstractNum/level");
                }
                abstracts.insert(id, kind);
            }
            "num" => {
                let id: u32 = field(node, namespace, "numId", part)?
                    .parse().map_err(|_| invalid(part, "invalid numbering instance id"))?;
                if id == 0 || instances.contains_key(&id) {
                    return Err(invalid(part, "invalid or repeated numbering instance id"));
                }
                let children = node.children().filter(|node| node.is_element()).collect::<Vec<_>>();
                if children.len() != 1 || !children[0].has_tag_name((namespace, "abstractNumId")) {
                    unsupported(report, part, "num/overrides");
                    continue;
                }
                let abstract_id: u32 = field(children[0], namespace, "val", part)?
                    .parse().map_err(|_| invalid(part, "invalid numbering definition reference"))?;
                instances.insert(id, abstract_id);
            }
            _ => unsupported(report, part, "numbering/unsupportedDefinition"),
        }
    }
    let mut result = BTreeMap::new();
    for (instance, definition) in instances {
        match abstracts.get(&definition).copied().flatten() {
            Some(kind) => { result.insert(instance, kind); }
            None => unsupported(report, part, "num/missingDefinition"),
        }
    }
    Ok(result)
}
