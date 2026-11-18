//! Bounded rectangular Word tables, using existing paragraph/run conversion.

use office_core::{Table, TableCell, TableRow, Twips};
use roxmltree::Node;
use std::collections::BTreeSet;

use super::{
    ImportContext, bookmark_marker, check_leaf, check_whitespace, check_word_attributes,
    generated_id, on_off, parse_paragraph_content, required_word_attribute, unsupported_element,
};
use crate::{
    CompatibilityIssueKind as Issue, CompatibilityReport, ConversionError, conversion::invalid,
};

pub(super) fn parse_table(
    node: Node<'_, '_>,
    context: ImportContext<'_>,
    index: usize,
    paragraph_index: &mut usize,
    numbering_flows: &mut BTreeSet<u32>,
    report: &mut CompatibilityReport,
) -> Result<Table, ConversionError> {
    let ImportContext {
        namespace,
        part,
        document_id,
        ..
    } = context;
    check_word_attributes(node, namespace, &[], part, report);
    let mut rows = Vec::new();
    let mut grid = None;
    let mut properties_seen = false;
    for child in node.children() {
        if !child.is_element() {
            check_whitespace(child, part)?;
        } else if child.has_tag_name((namespace, "tblPr")) {
            if properties_seen || grid.is_some() || !rows.is_empty() {
                return Err(invalid(part, "duplicate or misplaced table properties"));
            }
            properties_seen = true;
            parse_properties(child, namespace, part, "tblPr", report)?;
        } else if child.has_tag_name((namespace, "tblGrid")) {
            if grid.is_some() || !rows.is_empty() {
                return Err(invalid(part, "duplicate or misplaced table grid"));
            }
            check_word_attributes(child, namespace, &[], part, report);
            let mut columns = Vec::new();
            for column in child.children() {
                if !column.is_element() {
                    check_whitespace(column, part)?;
                } else if column.has_tag_name((namespace, "gridCol")) {
                    check_word_attributes(column, namespace, &["w"], part, report);
                    check_leaf(column, part, report)?;
                    let width = positive_twips(
                        required_word_attribute(column, namespace, "w", part)?,
                        part,
                    )?;
                    columns.push(Twips(width));
                } else {
                    unsupported_element(column, part, report, "Unsupported table grid content.");
                }
            }
            grid = Some(columns);
        } else if child.has_tag_name((namespace, "tr")) {
            check_word_attributes(child, namespace, &[], part, report);
            let row_index = rows.len();
            let mut cells = Vec::new();
            let mut row_properties = false;
            for row_child in child.children() {
                if !row_child.is_element() {
                    check_whitespace(row_child, part)?;
                } else if row_child.has_tag_name((namespace, "trPr")) {
                    if row_properties || !cells.is_empty() {
                        return Err(invalid(part, "duplicate or misplaced table row properties"));
                    }
                    row_properties = true;
                    parse_properties(row_child, namespace, part, "trPr", report)?;
                } else if row_child.has_tag_name((namespace, "tc")) {
                    check_word_attributes(row_child, namespace, &[], part, report);
                    let column_index = cells.len();
                    let mut paragraphs = Vec::new();
                    let mut cell_properties = false;
                    let mut preceding_numbering = None;
                    let mut used_numbering = BTreeSet::new();
                    for content in row_child.children() {
                        if !content.is_element() {
                            check_whitespace(content, part)?;
                        } else if bookmark_marker(content, namespace) {
                            continue;
                        } else if content.has_tag_name((namespace, "tcPr")) {
                            if cell_properties || !paragraphs.is_empty() {
                                return Err(invalid(
                                    part,
                                    "duplicate or misplaced table cell properties",
                                ));
                            }
                            cell_properties = true;
                            parse_properties(content, namespace, part, "tcPr", report)?;
                        } else if content.has_tag_name((namespace, "p")) {
                            let (paragraph, section, numbering_id) = parse_paragraph_content(
                                content,
                                context,
                                *paragraph_index,
                                report,
                            )?;
                            *paragraph_index += 1;
                            if section.is_some() {
                                report.push(Issue::UnsupportedElement, part, "tbl/tc/p/sectPr",
                                    "A section boundary inside a table cell cannot be represented safely.");
                            }
                            if let Some(id) = numbering_id {
                                let kind = paragraph.list();
                                if (preceding_numbering != Some((id, kind))
                                    && used_numbering.contains(&id))
                                    || (!used_numbering.contains(&id)
                                        && numbering_flows.contains(&id))
                                {
                                    report.push(Issue::UnsupportedValue, part, "tbl/tc/p/numPr",
                                        "A list resumes across a table flow boundary; its numbering would change.");
                                }
                                if let Some((previous_id, previous_kind)) = preceding_numbering
                                    && previous_id != id
                                    && previous_kind == kind
                                {
                                    report.push(Issue::UnsupportedValue, part, "tbl/tc/p/numPr",
                                        "Adjacent independent list instances inside a cell must remain distinct.");
                                }
                                used_numbering.insert(id);
                                preceding_numbering = Some((id, kind));
                            } else {
                                preceding_numbering = None;
                            }
                            paragraphs.push(paragraph);
                        } else {
                            unsupported_element(
                                content,
                                part,
                                report,
                                "Only ordinary paragraphs are editable inside table cells; nested tables and unknown content remain preserved read-only.",
                            );
                        }
                    }
                    numbering_flows.extend(used_numbering);
                    if paragraphs.is_empty() {
                        if report.is_supported() {
                            return Err(invalid(
                                part,
                                "table cell must contain at least one paragraph",
                            ));
                        }
                        // This draft is never published when unsupported cell content exists.
                        paragraphs.push(office_core::Paragraph::new(
                            generated_id(document_id, "paragraph", *paragraph_index)?,
                            Vec::new(),
                        ));
                        *paragraph_index += 1;
                    }
                    cells.push(TableCell::new(
                        generated_id(
                            document_id,
                            &format!("table:{index}:row:{row_index}:cell"),
                            column_index,
                        )?,
                        paragraphs,
                    ));
                } else {
                    unsupported_element(row_child, part, report, "Unsupported table row content.");
                }
            }
            if cells.is_empty() {
                return Err(invalid(part, "table row has no cells"));
            }
            rows.push(TableRow::new(
                generated_id(document_id, &format!("table:{index}:row"), row_index)?,
                cells,
            ));
        } else {
            unsupported_element(child, part, report, "Unsupported table content.");
        }
    }
    if rows.is_empty() {
        return Err(invalid(part, "table has no rows"));
    }
    let count = rows[0].cells().len();
    if rows.iter().any(|row| row.cells().len() != count) {
        // Word permits gridBefore/gridAfter, spans and merging, but those have
        // blockers. Never publish a rectangular partial projection for them.
        if report.is_supported() {
            return Err(invalid(part, "table rows have inconsistent cell counts"));
        }
        return Ok(Table::new(
            generated_id(document_id, "table", index)?,
            vec![rows.remove(0)],
        ));
    }
    let table_properties = node
        .children()
        .find(|value| value.has_tag_name((namespace, "tblPr")));
    for (property, message) in [
        (
            "tblLayout",
            "Word's default automatic table fitting is normalized to the fixed rectangular grid.",
        ),
        (
            "tblBorders",
            "The source's implicit table border defaults are normalized to a visible simple grid.",
        ),
        (
            "tblCellMar",
            "The source's implicit table cell margins are normalized to four points on each side.",
        ),
    ] {
        if !table_properties.is_some_and(|value| {
            value
                .children()
                .any(|child| child.has_tag_name((namespace, property)))
        }) {
            report.normalize(part, format!("tblPr/{property}"), message);
        }
    }
    if let Some(widths) = grid.as_ref().filter(|widths| !widths.is_empty()) {
        let total: i64 = widths.iter().map(|width| i64::from(width.0)).sum();
        let preferred = table_properties.and_then(|value| {
            value
                .children()
                .find(|child| child.has_tag_name((namespace, "tblW")))
        });
        check_preferred_width(preferred, total, namespace, part, "tblPr/tblW", report);
        for row in node
            .children()
            .filter(|value| value.has_tag_name((namespace, "tr")))
        {
            for (index, cell) in row
                .children()
                .filter(|value| value.has_tag_name((namespace, "tc")))
                .enumerate()
            {
                let preferred = cell
                    .children()
                    .find(|value| value.has_tag_name((namespace, "tcPr")))
                    .and_then(|value| {
                        value
                            .children()
                            .find(|child| child.has_tag_name((namespace, "tcW")))
                    });
                if let Some(width) = widths.get(index) {
                    check_preferred_width(
                        preferred,
                        i64::from(width.0),
                        namespace,
                        part,
                        "tcPr/tcW",
                        report,
                    );
                }
            }
        }
    } else if node.descendants().any(|value| {
        value.has_tag_name((namespace, "tcW")) || value.has_tag_name((namespace, "tblW"))
    }) {
        report.normalize(part, "tblPr/tblW", "Without an explicit column grid, preferred table/cell widths are normalized to equal-width columns.");
    }
    for row in node
        .children()
        .filter(|value| value.has_tag_name((namespace, "tr")))
    {
        let cant_split = row
            .children()
            .find(|value| value.has_tag_name((namespace, "trPr")))
            .and_then(|value| {
                value
                    .children()
                    .find(|value| value.has_tag_name((namespace, "cantSplit")))
            });
        if cant_split.is_none() {
            report.normalize(
                part,
                "trPr/cantSplit",
                "Table rows stay together during pagination; Word may split a row across pages.",
            );
        }
    }
    let mut table = Table::new(generated_id(document_id, "table", index)?, rows);
    if let Some(grid) = grid.filter(|grid| !grid.is_empty()) {
        if grid.len() != count {
            if report.is_supported() {
                return Err(invalid(part, "table grid does not match its cell columns"));
            }
        } else {
            table = table.with_column_widths(grid);
        }
    }
    Ok(table)
}

fn check_preferred_width(
    node: Option<Node<'_, '_>>,
    expected: i64,
    namespace: &str,
    part: &str,
    path: &str,
    report: &mut CompatibilityReport,
) {
    if let Some(node) = node
        && node.attribute((namespace, "type")) == Some("dxa")
        && node
            .attribute((namespace, "w"))
            .and_then(|value| value.parse::<i64>().ok())
            != Some(expected)
    {
        report.normalize(part, path, "Preferred table/cell widths differ from the explicit column grid. The grid widths are retained; original preferences remain in the source.");
    }
}

fn positive_twips(value: &str, part: &str) -> Result<i32, ConversionError> {
    let width: i32 = value
        .parse()
        .map_err(|_| invalid(part, "invalid table width"))?;
    if width <= 0 {
        return Err(invalid(part, "table width must be positive"));
    }
    Ok(width)
}

fn parse_properties(
    node: Node<'_, '_>,
    namespace: &str,
    part: &str,
    role: &str,
    report: &mut CompatibilityReport,
) -> Result<(), ConversionError> {
    check_word_attributes(node, namespace, &[], part, report);
    let mut seen = BTreeSet::new();
    for property in node.children() {
        if !property.is_element() {
            check_whitespace(property, part)?;
            continue;
        }
        if !seen.insert(property.tag_name().name()) {
            return Err(invalid(part, "duplicate table formatting property"));
        }
        if property.tag_name().namespace() != Some(namespace) {
            unsupported_element(
                property,
                part,
                report,
                "Unknown table formatting namespace.",
            );
            continue;
        }
        let name = property.tag_name().name();
        let mut normalized = false;
        match (role, name) {
            ("tblPr", "tblW")
            | ("tcPr", "tcW")
            | ("tblPr", "tblInd")
            | ("tblPr" | "trPr", "tblCellSpacing") => {
                check_word_attributes(property, namespace, &["w", "type"], part, report);
                check_leaf(property, part, report)?;
                let value = required_word_attribute(property, namespace, "w", part)?;
                let width: i32 = value
                    .parse()
                    .map_err(|_| invalid(part, "invalid table preferred width"))?;
                let kind = required_word_attribute(property, namespace, "type", part)?;
                if !matches!(kind, "dxa" | "auto" | "nil" | "pct") || width < 0 {
                    return Err(invalid(part, "invalid table preferred width type or value"));
                }
                normalized = match name {
                    "tblInd" | "tblCellSpacing" => width != 0,
                    "tblW" | "tcW" => kind == "pct",
                    _ => false,
                };
            }
            ("tblPr", "tblLayout") => {
                check_word_attributes(property, namespace, &["type"], part, report);
                check_leaf(property, part, report)?;
                let kind = required_word_attribute(property, namespace, "type", part)?;
                if !matches!(kind, "fixed" | "autofit") {
                    return Err(invalid(part, "invalid table layout kind"));
                }
                normalized = kind == "autofit";
            }
            ("tblPr", "tblStyle") | ("tblPr", "tblCaption" | "tblDescription") => {
                check_word_attributes(property, namespace, &["val"], part, report);
                check_leaf(property, part, report)?;
                required_word_attribute(property, namespace, "val", part)?;
                normalized = true;
            }
            ("tblPr", "tblLook") => {
                check_word_attributes(
                    property,
                    namespace,
                    &[
                        "val",
                        "firstRow",
                        "lastRow",
                        "firstColumn",
                        "lastColumn",
                        "noHBand",
                        "noVBand",
                    ],
                    part,
                    report,
                );
                check_leaf(property, part, report)?;
                for attribute in property.attributes() {
                    if attribute.namespace() == Some(namespace) {
                        if attribute.name() == "val" {
                            if attribute.value().len() != 4
                                || !attribute
                                    .value()
                                    .bytes()
                                    .all(|byte| byte.is_ascii_hexdigit())
                            {
                                return Err(invalid(part, "invalid table look mask"));
                            }
                        } else {
                            on_off(attribute.value(), part)?;
                        }
                    }
                }
                normalized = true;
            }
            ("tblPr", "jc") | ("tcPr", "vAlign") => {
                check_word_attributes(property, namespace, &["val"], part, report);
                check_leaf(property, part, report)?;
                let value = required_word_attribute(property, namespace, "val", part)?;
                let accepted = if name == "jc" {
                    matches!(value, "left" | "center" | "right" | "start" | "end")
                } else {
                    matches!(value, "top" | "center" | "bottom" | "both")
                };
                if !accepted {
                    return Err(invalid(part, "invalid table alignment"));
                }
                normalized = !matches!(value, "left" | "start" | "top");
            }
            ("tblPr", "tblBorders" | "tblCellMar") | ("tcPr", "tcBorders" | "tcMar") => {
                normalized = parse_edges(property, namespace, part, report)?;
            }
            ("tcPr", "gridSpan") | ("trPr", "gridBefore" | "gridAfter") => {
                check_word_attributes(property, namespace, &["val"], part, report);
                check_leaf(property, part, report)?;
                let value: u32 = required_word_attribute(property, namespace, "val", part)?
                    .parse()
                    .map_err(|_| invalid(part, "invalid table span or omitted-grid count"))?;
                if (name == "gridSpan" && value != 1) || (name != "gridSpan" && value != 0) {
                    report.push(Issue::UnsupportedValue, part, format!("{role}/{name}"),
                        "Merged cells and omitted grid cells are not editable in the rectangular table profile.");
                }
            }
            ("tcPr", "vMerge" | "hMerge") => {
                check_word_attributes(property, namespace, &["val"], part, report);
                check_leaf(property, part, report)?;
                report.push(
                    Issue::UnsupportedValue,
                    part,
                    format!("{role}/{name}"),
                    "Merged cells remain preserved read-only.",
                );
            }
            ("trPr", "cantSplit" | "tblHeader")
            | ("tcPr", "noWrap" | "tcFitText")
            | ("tblPr", "bidiVisual") => {
                check_word_attributes(property, namespace, &["val"], part, report);
                check_leaf(property, part, report)?;
                let value = property
                    .attribute((namespace, "val"))
                    .map(|value| on_off(value, part))
                    .transpose()?
                    .unwrap_or(true);
                normalized = if name == "cantSplit" { !value } else { value };
                if name == "bidiVisual" && value {
                    report.push(
                        Issue::UnsupportedValue,
                        part,
                        "tblPr/bidiVisual",
                        "Reversed table column ordering remains preserved read-only.",
                    );
                    normalized = false;
                }
            }
            ("trPr", "trHeight") => {
                check_word_attributes(property, namespace, &["val", "hRule"], part, report);
                check_leaf(property, part, report)?;
                let height: u32 = required_word_attribute(property, namespace, "val", part)?
                    .parse()
                    .map_err(|_| invalid(part, "invalid table row height"))?;
                if property
                    .attribute((namespace, "hRule"))
                    .is_some_and(|value| !matches!(value, "auto" | "atLeast" | "exact"))
                {
                    return Err(invalid(part, "invalid table row height rule"));
                }
                normalized = height != 0;
            }
            ("tcPr", "textDirection") => {
                check_word_attributes(property, namespace, &["val"], part, report);
                check_leaf(property, part, report)?;
                if required_word_attribute(property, namespace, "val", part)? != "lrTb" {
                    report.push(
                        Issue::UnsupportedValue,
                        part,
                        "tcPr/textDirection",
                        "Non-horizontal cell text remains preserved read-only.",
                    );
                }
            }
            ("tcPr" | "tblPr", "shd") => {
                check_word_attributes(property, namespace, &["val", "color", "fill"], part, report);
                check_leaf(property, part, report)?;
                if let Some(value) = property.attribute((namespace, "val"))
                    && !matches!(
                        value,
                        "clear"
                            | "nil"
                            | "solid"
                            | "pct5"
                            | "pct10"
                            | "pct12"
                            | "pct15"
                            | "pct20"
                            | "pct25"
                            | "pct30"
                            | "pct35"
                            | "pct37"
                            | "pct40"
                            | "pct45"
                            | "pct50"
                            | "pct55"
                            | "pct60"
                            | "pct62"
                            | "pct65"
                            | "pct70"
                            | "pct75"
                            | "pct80"
                            | "pct85"
                            | "pct87"
                            | "pct90"
                            | "pct95"
                            | "horzStripe"
                            | "vertStripe"
                            | "reverseDiagStripe"
                            | "diagStripe"
                            | "horzCross"
                            | "diagCross"
                            | "thinHorzStripe"
                            | "thinVertStripe"
                            | "thinReverseDiagStripe"
                            | "thinDiagStripe"
                            | "thinHorzCross"
                            | "thinDiagCross"
                    )
                {
                    report.push(
                        Issue::UnsupportedValue,
                        part,
                        format!("{role}/{name}"),
                        "Unknown table shading treatment remains preserved read-only.",
                    );
                }
                for attribute in ["color", "fill"] {
                    if let Some(value) = property.attribute((namespace, attribute)) {
                        validate_color(value, part)?;
                    }
                }
                normalized = true;
            }
            _ => unsupported_element(
                property,
                part,
                report,
                "Unknown or content-bearing table property remains preserved read-only.",
            ),
        }
        if normalized {
            report.normalize(part, format!("{role}/{name}"),
                "This table appearance or layout preference uses Graphe's simple grid defaults. The exact source setting remains in the preserved original.");
        }
    }
    Ok(())
}

fn parse_edges(
    node: Node<'_, '_>,
    namespace: &str,
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<bool, ConversionError> {
    check_word_attributes(node, namespace, &[], part, report);
    let borders = matches!(node.tag_name().name(), "tblBorders" | "tcBorders");
    let mut seen = BTreeSet::new();
    let mut normalized = false;
    for edge in node.children() {
        if !edge.is_element() {
            check_whitespace(edge, part)?;
            continue;
        }
        if !seen.insert(edge.tag_name().name()) {
            return Err(invalid(part, "duplicate table border or margin edge"));
        }
        if edge.tag_name().namespace() != Some(namespace)
            || !matches!(
                edge.tag_name().name(),
                "top" | "left" | "right" | "bottom" | "start" | "end" | "insideH" | "insideV"
            )
        {
            unsupported_element(edge, part, report, "Unknown table border or margin edge.");
            continue;
        }
        check_leaf(edge, part, report)?;
        if borders {
            check_word_attributes(
                edge,
                namespace,
                &[
                    "val",
                    "sz",
                    "color",
                    "space",
                    "shadow",
                    "frame",
                    "themeColor",
                    "themeTint",
                    "themeShade",
                ],
                part,
                report,
            );
            let kind = required_word_attribute(edge, namespace, "val", part)?;
            if !matches!(
                kind,
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
                report.push(
                    Issue::UnsupportedValue,
                    part,
                    "tbl/borders",
                    "Unknown or artwork table border remains preserved read-only.",
                );
            }
            for attribute in ["sz", "space"] {
                if let Some(value) = edge.attribute((namespace, attribute)) {
                    value
                        .parse::<u32>()
                        .map_err(|_| invalid(part, "invalid table border dimension"))?;
                }
            }
            for attribute in ["shadow", "frame"] {
                if let Some(value) = edge.attribute((namespace, attribute)) {
                    on_off(value, part)?;
                }
            }
            if let Some(value) = edge.attribute((namespace, "color")) {
                validate_color(value, part)?;
            }
            normalized |= kind != "single"
                || edge.attribute((namespace, "sz")) != Some("4")
                || edge.attribute((namespace, "color")) != Some("808080")
                || edge
                    .attributes()
                    .any(|attribute| !matches!(attribute.name(), "val" | "sz" | "color"));
        } else {
            check_word_attributes(edge, namespace, &["w", "type"], part, report);
            let value = required_word_attribute(edge, namespace, "w", part)?;
            let width: u32 = value
                .parse()
                .map_err(|_| invalid(part, "invalid table cell margin"))?;
            let kind = required_word_attribute(edge, namespace, "type", part)?;
            if !matches!(kind, "dxa" | "nil") {
                return Err(invalid(part, "invalid cell margin unit"));
            }
            normalized |= width != 80 || kind != "dxa";
        }
    }
    let required = if borders { 6 } else { 4 };
    normalized |= seen.len() != required;
    Ok(normalized)
}

fn validate_color(value: &str, part: &str) -> Result<(), ConversionError> {
    if value != "auto" && (value.len() != 6 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()))
    {
        return Err(invalid(part, "invalid table color"));
    }
    Ok(())
}
