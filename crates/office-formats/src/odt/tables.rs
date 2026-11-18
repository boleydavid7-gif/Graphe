//! Bounded rectangular ODF table projection; unsupported table content is guarded.

use super::*;
use office_core::{Table, TableCell, TableRow};

pub(super) const TABLE: &str = "urn:oasis:names:tc:opendocument:xmlns:table:1.0";

impl Catalog {
    pub(super) fn read_table_style(
        &mut self,
        node: Node<'_, '_>,
        part: &str,
        report: &mut Vec<FormatDiagnostic>,
    ) -> Result<(), FormatError> {
        let family = node.attribute((STYLE, "family")).unwrap_or("");
        let name = node
            .attribute((STYLE, "name"))
            .ok_or_else(|| package::invalid("table style has no name"))?;
        attributes(
            node,
            part,
            &[
                (STYLE, "name"),
                (STYLE, "family"),
                (STYLE, "master-page-name"),
            ],
            report,
        );
        container_text(node)?;
        let mut width = None;
        let mut grid_border = false;
        let mut grid_padding = false;
        let mut found = false;
        for child in node.children().filter(Node::is_element) {
            container_text(child)?;
            if found || child.children().any(|node| node.is_element()) {
                diagnostic(
                    report,
                    "unsupported_table",
                    part,
                    name,
                    "Extended table styling is retained read-only.",
                );
            }
            found = true;
            if family == "table-column" && child.has_tag_name((STYLE, "table-column-properties")) {
                attributes(child, part, &[(STYLE, "column-width")], report);
                width = child
                    .attribute((STYLE, "column-width"))
                    .map(properties::length)
                    .transpose()?;
                if width.is_none_or(|w| w.0 <= 0) {
                    return Err(package::invalid("table column width must be positive"));
                }
            } else if family == "table-cell" && child.has_tag_name((STYLE, "table-cell-properties"))
            {
                grid_border = child.attribute((FO, "border")) == Some("0.5pt solid #808080");
                grid_padding = child.attribute((FO, "padding")) == Some("4pt");
                // The first model has a fixed grid and 4pt cell padding. A
                // different source appearance cannot silently become that grid.
                for attr in child.attributes() {
                    if !matches!(
                        (attr.namespace(), attr.name(), attr.value()),
                        (Some(FO), "border", "0.5pt solid #808080") | (Some(FO), "padding", "4pt")
                    ) {
                        diagnostic(
                            report,
                            "unsupported_table",
                            part,
                            name,
                            "This table border, padding or cell appearance is retained read-only.",
                        );
                    }
                }
            } else if family == "table" && child.has_tag_name((STYLE, "table-properties")) {
                if child.attribute((FO, "break-before")) == Some("page") {
                    self.table_breaks.insert(name.into());
                }
                for attr in child.attributes() {
                    if !matches!(
                        (attr.namespace(), attr.name(), attr.value()),
                        (Some(FO), "break-before", "page")
                    ) {
                        diagnostic(
                            report,
                            "unsupported_table",
                            part,
                            name,
                            "Unsupported table geometry is retained read-only.",
                        );
                    }
                }
            } else {
                diagnostic(
                    report,
                    "unsupported_table",
                    part,
                    name,
                    "Unsupported table style properties are retained read-only.",
                );
            }
        }
        if self.table_columns.len() + self.table_cells.len() + self.table_masters.len() >= 2048 {
            return Err(FormatError::new(
                "format_limit",
                "ODT has too many table styles.",
            ));
        }
        match family {
            "table-column" => {
                let width = width.ok_or_else(|| package::invalid("column style has no width"))?;
                if self.table_columns.insert(name.into(), width).is_some() {
                    return Err(package::invalid("duplicate column style"));
                }
            }
            "table-cell" => {
                if self
                    .table_cells
                    .insert(name.into(), grid_border && grid_padding)
                    .is_some()
                {
                    return Err(package::invalid("duplicate cell style"));
                }
            }
            "table" => {
                if self
                    .table_masters
                    .insert(
                        name.into(),
                        node.attribute((STYLE, "master-page-name"))
                            .map(str::to_owned),
                    )
                    .is_some()
                {
                    return Err(package::invalid("duplicate table style"));
                }
            }
            _ => unreachable!(),
        }
        Ok(())
    }
}

impl TextParser<'_> {
    pub(super) fn table(
        &mut self,
        node: Node<'_, '_>,
    ) -> Result<(Option<Table>, Option<SectionProperties>), FormatError> {
        attributes(
            node,
            "content.xml",
            &[(TABLE, "name"), (TABLE, "style-name")],
            self.report,
        );
        container_text(node)?;
        let mut geometry = node
            .attribute((TABLE, "style-name"))
            .map(|name| {
                self.catalog
                    .table_masters
                    .get(name)
                    .ok_or_else(|| package::invalid("unresolved table style"))?
                    .as_ref()
                    .map(|master| self.catalog.geometry(Some(master)))
                    .transpose()
                    .map(Option::flatten)
            })
            .transpose()?
            .flatten();
        if node
            .attribute((TABLE, "style-name"))
            .is_some_and(|name| self.catalog.table_breaks.contains(name))
            && geometry.is_none()
        {
            geometry = Some(self.catalog.geometry(None)?.unwrap_or_default());
        }
        self.table_count += 1;
        if self.table_count > 1024 {
            return Err(FormatError::new("format_limit", "ODT has too many tables."));
        }
        let base = format!(
            "{}:odt:table:{}",
            self.document_id.as_str(),
            self.table_count
        );
        let mut rows = Vec::new();
        let mut widths = Vec::new();
        let mut columns = 0;
        let mut all_widths = true;
        for child in node.children().filter(Node::is_element) {
            if child.has_tag_name((TABLE, "table-column")) && rows.is_empty() {
                attributes(child, "content.xml", &[(TABLE, "style-name")], self.report);
                container_text(child)?;
                if child.children().any(|node| node.is_element()) {
                    diagnostic(
                        self.report,
                        "unsupported_table",
                        "content.xml",
                        "table-column",
                        "Extended table columns are retained read-only.",
                    );
                }
                columns += 1;
                if columns > 32 {
                    return Err(FormatError::new("format_limit", "Too many table columns."));
                }
                if let Some(style) = child.attribute((TABLE, "style-name")) {
                    widths.push(
                        *self
                            .catalog
                            .table_columns
                            .get(style)
                            .ok_or_else(|| package::invalid("unresolved table column style"))?,
                    );
                } else {
                    all_widths = false;
                }
            } else if child.has_tag_name((TABLE, "table-row")) {
                attributes(child, "content.xml", &[], self.report);
                container_text(child)?;
                if rows.len() >= office_core::MAX_TABLE_ROWS {
                    return Err(FormatError::new("format_limit", "Too many table rows."));
                }
                let mut cells = Vec::new();
                for cell in child.children().filter(Node::is_element) {
                    if !cell.has_tag_name((TABLE, "table-cell")) {
                        diagnostic(
                            self.report,
                            "unsupported_table",
                            "content.xml",
                            cell.tag_name().name(),
                            "Merged, covered or extended cells are retained read-only.",
                        );
                        continue;
                    }
                    attributes(
                        cell,
                        "content.xml",
                        &[(TABLE, "style-name"), (OFFICE, "value-type")],
                        self.report,
                    );
                    if cell
                        .attribute((OFFICE, "value-type"))
                        .is_some_and(|value| value != "string")
                    {
                        diagnostic(
                            self.report,
                            "unsupported_table",
                            "content.xml",
                            "value-type",
                            "Formula or typed table values are retained read-only.",
                        );
                    }
                    let canonical_appearance =
                        if let Some(style) = cell.attribute((TABLE, "style-name")) {
                            *self
                                .catalog
                                .table_cells
                                .get(style)
                                .ok_or_else(|| package::invalid("unresolved table cell style"))?
                        } else {
                            false
                        };
                    if !canonical_appearance {
                        self.notices.record("content.xml","table-cell",
                            "ODF cells without complete explicit border and padding styling use Graphe's standard grid and padding; the original is preserved.",true);
                    }
                    container_text(cell)?;
                    let mut paragraphs = Vec::new();
                    for paragraph in cell.children().filter(Node::is_element) {
                        if paragraph.has_tag_name((TEXT, "p"))
                            || paragraph.has_tag_name((TEXT, "h"))
                        {
                            let (p, g) = self.paragraph(paragraph)?;
                            if g.is_some() {
                                diagnostic(
                                    self.report,
                                    "unsupported_table",
                                    "content.xml",
                                    "master-page-name",
                                    "Page geometry changes inside cells are retained read-only.",
                                );
                            }
                            paragraphs.push(p);
                        } else {
                            diagnostic(
                                self.report,
                                "unsupported_table",
                                "content.xml",
                                paragraph.tag_name().name(),
                                "Nested tables or unsupported cell content are retained read-only.",
                            );
                        }
                    }
                    if paragraphs.is_empty() {
                        diagnostic(
                            self.report,
                            "unsupported_table",
                            "content.xml",
                            "table-cell",
                            "Cells without a supported paragraph are retained read-only.",
                        );
                    } else {
                        cells.push(TableCell::new(
                            Id::new(format!("{base}:r:{}:c:{}", rows.len(), cells.len()))?,
                            paragraphs,
                        ));
                    }
                    if cells.len() > 32 {
                        return Err(FormatError::new("format_limit", "Too many table columns."));
                    }
                }
                if cells.is_empty() {
                    diagnostic(
                        self.report,
                        "unsupported_table",
                        "content.xml",
                        "table-row",
                        "Empty table rows are retained read-only.",
                    );
                } else {
                    rows.push(TableRow::new(
                        Id::new(format!("{base}:r:{}", rows.len()))?,
                        cells,
                    ));
                }
            } else {
                diagnostic(
                    self.report,
                    "unsupported_table",
                    "content.xml",
                    child.tag_name().name(),
                    "Nested, repeated or extended table structures are retained read-only.",
                );
            }
        }
        if rows.is_empty()
            || rows
                .iter()
                .any(|row| row.cells().len() != rows[0].cells().len())
            || (columns != 0 && columns != rows[0].cells().len())
        {
            diagnostic(
                self.report,
                "unsupported_table",
                "content.xml",
                "table",
                "A table must be nonempty and rectangular.",
            );
            return Ok((None, geometry));
        }
        if !all_widths && !widths.is_empty() {
            diagnostic(
                self.report,
                "unsupported_table",
                "content.xml",
                "table-column",
                "Partially specified column widths cannot be represented without changing the table.",
            );
        }
        let table = Table::new(Id::new(base)?, rows);
        let table = if columns != 0 && all_widths {
            table.with_column_widths(widths)
        } else {
            table
        };
        Ok((Some(table), geometry))
    }
}
