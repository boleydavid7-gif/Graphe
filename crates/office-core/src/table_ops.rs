//! Atomic structural edits for the initial rectangular-table profile.
use crate::{Block, Document, EditError, Id, Section, Table, TableCell, TableRow, Transaction};

fn invalid(reason: &'static str) -> EditError {
    EditError::InvalidTableOperation { reason }
}

fn section_with_blocks(section: &Section, blocks: Vec<Block>) -> Section {
    let current = Section::new(section.id().clone(), blocks);
    match section.properties() {
        Some(properties) => current.with_properties(properties.clone()),
        None => current,
    }
}

fn table_with_rows(table: &Table, rows: Vec<TableRow>) -> Table {
    let current = Table::new(table.id().clone(), rows);
    match table.column_widths() {
        Some(widths) => current.with_column_widths(widths.to_vec()),
        None => current,
    }
}

impl Transaction {
    /// Inserts a complete table after a body paragraph as one validated edit.
    ///
    /// Cell paragraphs cannot contain nested tables. Existing content and IDs are
    /// preserved; duplicate IDs or invalid dimensions reject the entire operation.
    pub fn insert_table_after(
        &mut self,
        paragraph_id: &Id,
        table: Table,
    ) -> Result<bool, EditError> {
        let before = self.candidate()?;
        let mut sections = before.sections().to_vec();
        let mut found = false;
        for section in &mut sections {
            if let Some(index) = section.blocks().iter().position(|block| {
                matches!(block, Block::Paragraph(paragraph) if paragraph.id() == paragraph_id)
            }) {
                let mut blocks = section.blocks().to_vec();
                blocks.insert(index + 1, Block::Table(table));
                *section = section_with_blocks(section, blocks);
                found = true;
                break;
            }
        }
        if !found {
            return Err(invalid("table insertion requires a body paragraph"));
        }
        self.accept_table_sections(before, sections)
    }

    /// Inserts a complete row before `index`, or appends at the current row count.
    /// Its cells must match the existing column count and use fresh unique IDs.
    pub fn insert_table_row(
        &mut self,
        table_id: &Id,
        index: usize,
        row: TableRow,
    ) -> Result<bool, EditError> {
        self.update_table(table_id, |table| {
            if index > table.rows().len() {
                return Err(invalid("row index is out of bounds"));
            }
            let mut rows = table.rows().to_vec();
            rows.insert(index, row);
            Ok(Some(table_with_rows(&table, rows)))
        })
    }

    /// Removes one row. Removing the final row is rejected; use `remove_table`.
    pub fn remove_table_row(&mut self, table_id: &Id, index: usize) -> Result<bool, EditError> {
        self.update_table(table_id, |table| {
            if index >= table.rows().len() {
                return Err(invalid("row index is out of bounds"));
            }
            if table.rows().len() == 1 {
                return Err(invalid("the final row cannot be removed"));
            }
            let mut rows = table.rows().to_vec();
            rows.remove(index);
            Ok(Some(table_with_rows(&table, rows)))
        })
    }

    /// Inserts a column from one fresh cell per row, before `index` or at the end.
    /// Explicit widths keep their exact total: the adjacent column is split in
    /// half. If it is only one twip wide, a splittable column is used instead.
    /// Insufficient total width rejects the operation without changing the draft.
    pub fn insert_table_column(
        &mut self,
        table_id: &Id,
        index: usize,
        cells: Vec<TableCell>,
    ) -> Result<bool, EditError> {
        self.update_table(table_id, |table| {
            let columns = table.rows()[0].cells().len();
            if index > columns {
                return Err(invalid("column index is out of bounds"));
            }
            if cells.len() != table.rows().len() {
                return Err(invalid("a new column requires one cell per row"));
            }
            let rows = table
                .rows()
                .iter()
                .zip(cells)
                .map(|(row, cell)| {
                    let mut cells = row.cells().to_vec();
                    cells.insert(index, cell);
                    TableRow::new(row.id().clone(), cells)
                })
                .collect();
            let mut result = Table::new(table.id().clone(), rows);
            if let Some(widths) = table.column_widths() {
                let mut widths = widths.to_vec();
                let adjacent = index.min(columns - 1);
                let donor = if widths[adjacent].0 >= 2 {
                    adjacent
                } else {
                    widths
                        .iter()
                        .position(|width| width.0 >= 2)
                        .ok_or_else(|| {
                            invalid("explicit table width cannot fit another positive-width column")
                        })?
                };
                let width = crate::Twips(widths[donor].0 / 2);
                widths[donor].0 -= width.0;
                widths.insert(index, width);
                result = result.with_column_widths(widths);
            }
            Ok(Some(result))
        })
    }

    /// Removes one column without changing retained cells or paragraph IDs.
    /// Explicit total width is preserved by assigning removed width to its next
    /// neighbor (or the preceding final column). Removing the last column is
    /// rejected; use `remove_table` instead.
    pub fn remove_table_column(&mut self, table_id: &Id, index: usize) -> Result<bool, EditError> {
        self.update_table(table_id, |table| {
            let columns = table.rows()[0].cells().len();
            if index >= columns {
                return Err(invalid("column index is out of bounds"));
            }
            if columns == 1 {
                return Err(invalid("the final column cannot be removed"));
            }
            let rows = table
                .rows()
                .iter()
                .map(|row| {
                    let mut cells = row.cells().to_vec();
                    cells.remove(index);
                    TableRow::new(row.id().clone(), cells)
                })
                .collect();
            let mut result = Table::new(table.id().clone(), rows);
            if let Some(widths) = table.column_widths() {
                let mut widths = widths.to_vec();
                let removed = widths.remove(index);
                let neighbor = index.min(widths.len() - 1);
                widths[neighbor].0 += removed.0;
                result = result.with_column_widths(widths);
            }
            Ok(Some(result))
        })
    }

    /// Sets validated physical column widths without replacing cells or paragraphs.
    pub fn set_table_column_widths(
        &mut self,
        table_id: &Id,
        widths: Vec<crate::Twips>,
    ) -> Result<bool, EditError> {
        self.update_table(table_id, |table| {
            Ok(Some(
                Table::new(table.id().clone(), table.rows().to_vec()).with_column_widths(widths),
            ))
        })
    }

    /// Removes the complete table as one undoable edit, preserving other blocks.
    pub fn remove_table(&mut self, table_id: &Id) -> Result<bool, EditError> {
        self.update_table(table_id, |_| Ok(None))
    }

    fn update_table(
        &mut self,
        table_id: &Id,
        update: impl FnOnce(Table) -> Result<Option<Table>, EditError>,
    ) -> Result<bool, EditError> {
        let before = self.candidate()?;
        let mut sections = before.sections().to_vec();
        for section in &mut sections {
            if let Some(index) = section
                .blocks()
                .iter()
                .position(|block| matches!(block, Block::Table(table) if table.id() == table_id))
            {
                let mut blocks = section.blocks().to_vec();
                let Block::Table(table) = blocks.remove(index) else {
                    unreachable!()
                };
                if let Some(replacement) = update(table)? {
                    blocks.insert(index, Block::Table(replacement));
                }
                *section = section_with_blocks(section, blocks);
                return self.accept_table_sections(before, sections);
            }
        }
        Err(invalid("table was not found"))
    }

    fn accept_table_sections(
        &mut self,
        before: Document,
        sections: Vec<Section>,
    ) -> Result<bool, EditError> {
        let after = Document::from_parts(
            before.id().clone(),
            before.metadata().clone(),
            sections,
            before.styles().to_vec(),
        )?;
        let changed = before != after;
        if changed {
            self.accept_candidate(after);
        }
        Ok(changed)
    }
}
