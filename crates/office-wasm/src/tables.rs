//! Small table commands backed by the existing atomic core transactions.

use office_core::{Block, Id, Paragraph, Section, Table, TableCell, TableRow};

use crate::{
    BrowserEngine, EngineError,
    wire::{BrowserSelection, Direction, Point, TableAction},
};

const MAX_TABLE_DIMENSION: usize = 20;

fn caret(paragraph: &Id) -> BrowserSelection {
    let point = Point {
        paragraph_id: paragraph.as_str().into(),
        utf16: 0,
    };
    BrowserSelection {
        anchor: point.clone(),
        focus: point,
    }
}

impl BrowserEngine {
    fn empty_cell(&mut self) -> Result<TableCell, EngineError> {
        Ok(TableCell::new(
            self.allocate_id()?,
            vec![Paragraph::new(self.allocate_id()?, vec![])],
        ))
    }

    /// Insert a block table after the current body paragraph, preserving its text.
    pub(super) fn insert_table(
        &mut self,
        rows: usize,
        columns: usize,
        supplied: Option<BrowserSelection>,
    ) -> Result<(), EngineError> {
        if !(1..=MAX_TABLE_DIMENSION).contains(&rows)
            || !(1..=MAX_TABLE_DIMENSION).contains(&columns)
        {
            return Err(EngineError::new(
                "invalid_table",
                "Choose 1 to 20 rows and columns.",
            ));
        }
        let selected = self.current_selection(supplied)?.ok_or_else(|| {
            EngineError::new(
                "missing_selection",
                "Choose a paragraph before inserting a table.",
            )
        })?;
        if selected.anchor != selected.focus {
            return Err(EngineError::new(
                "invalid_table",
                "Place the caret before inserting a table.",
            ));
        }
        let at = Id::new(&selected.focus.paragraph_id)?;
        if self.session.document().table_for_paragraph(&at).is_some() {
            return Err(EngineError::new(
                "invalid_table",
                "Nested tables are not supported.",
            ));
        }
        let table_id = self.allocate_id()?;
        let mut table_rows = Vec::with_capacity(rows);
        for _ in 0..rows {
            let row_id = self.allocate_id()?;
            let mut cells = Vec::with_capacity(columns);
            for _ in 0..columns {
                cells.push(self.empty_cell()?);
            }
            table_rows.push(TableRow::new(row_id, cells));
        }
        let table = Table::new(table_id.clone(), table_rows);
        let after = caret(table.rows()[0].cells()[0].paragraphs()[0].id());
        let trailing_id = self.allocate_id()?;
        let mut transaction = self.session.begin_transaction();
        transaction.insert_table_after(&at, table)?;
        // Keep an ordinary body position after a table, including a newly
        // inserted last block, so Tab and pointer editing can leave the table.
        for section in &mut transaction.draft_mut().sections {
            if let Some(index) = section
                .blocks()
                .iter()
                .position(|block| matches!(block, Block::Table(table) if table.id() == &table_id))
            {
                if !matches!(section.blocks().get(index + 1), Some(Block::Paragraph(_))) {
                    let mut blocks = section.blocks().to_vec();
                    blocks.insert(
                        index + 1,
                        Block::Paragraph(Paragraph::new(trailing_id, vec![])),
                    );
                    let mut replacement = Section::new(section.id().clone(), blocks);
                    if let Some(properties) = section.properties() {
                        replacement = replacement.with_properties(properties.clone());
                    }
                    *section = replacement;
                }
                break;
            }
        }
        self.commit_from(transaction, Some(selected), Some(after))?;
        self.typing = None;
        Ok(())
    }

    fn selected_cell(
        &self,
        supplied: Option<BrowserSelection>,
    ) -> Result<(BrowserSelection, Table, usize, usize), EngineError> {
        let selected = self
            .current_selection(supplied)?
            .ok_or_else(|| EngineError::new("missing_selection", "Choose a table cell first."))?;
        let document = self.session.document();
        let at = Id::new(&selected.focus.paragraph_id)?;
        let anchor = Id::new(&selected.anchor.paragraph_id)?;
        let (table, row, column) = document
            .table_for_paragraph(&at)
            .ok_or_else(|| EngineError::new("invalid_table", "Choose a table cell first."))?;
        let Some((anchor_table, anchor_row, anchor_column)) = document.table_for_paragraph(&anchor)
        else {
            return Err(EngineError::new(
                "invalid_table",
                "Choose text within one table cell.",
            ));
        };
        if anchor_table.id() != table.id() || anchor_row != row || anchor_column != column {
            return Err(EngineError::new(
                "invalid_table",
                "Choose text within one table cell.",
            ));
        }
        Ok((selected, table.clone(), row, column))
    }

    pub(super) fn table_action(
        &mut self,
        action: TableAction,
        supplied: Option<BrowserSelection>,
    ) -> Result<(), EngineError> {
        let (selected, table, row, column) = self.selected_cell(supplied)?;
        let mut transaction = self.session.begin_transaction();
        let mut target_row = row;
        let mut target_column = column;
        match action {
            TableAction::InsertRowAbove | TableAction::InsertRowBelow => {
                if table.rows().len() >= MAX_TABLE_DIMENSION {
                    return Err(EngineError::new(
                        "invalid_table",
                        "This table already has 20 rows.",
                    ));
                }
                let index = row + usize::from(matches!(action, TableAction::InsertRowBelow));
                let row_id = self.allocate_id()?;
                let mut cells = Vec::with_capacity(table.rows()[0].cells().len());
                for _ in table.rows()[0].cells() {
                    cells.push(self.empty_cell()?);
                }
                transaction.insert_table_row(table.id(), index, TableRow::new(row_id, cells))?;
                target_row = index;
            }
            TableAction::InsertColumnBefore | TableAction::InsertColumnAfter => {
                if table.rows()[0].cells().len() >= MAX_TABLE_DIMENSION {
                    return Err(EngineError::new(
                        "invalid_table",
                        "This table already has 20 columns.",
                    ));
                }
                let index = column + usize::from(matches!(action, TableAction::InsertColumnAfter));
                let mut cells = Vec::with_capacity(table.rows().len());
                for _ in table.rows() {
                    cells.push(self.empty_cell()?);
                }
                transaction.insert_table_column(table.id(), index, cells)?;
                target_column = index;
            }
            TableAction::DeleteRow => {
                transaction.remove_table_row(table.id(), row)?;
                target_row = row.min(table.rows().len().saturating_sub(2));
            }
            TableAction::DeleteColumn => {
                transaction.remove_table_column(table.id(), column)?;
                target_column = column.min(table.rows()[0].cells().len().saturating_sub(2));
            }
            TableAction::DeleteTable => {
                transaction.remove_table(table.id())?;
            }
        }
        let candidate = crate::transaction_document(&transaction)?;
        let after = candidate
            .sections()
            .iter()
            .flat_map(|section| section.blocks())
            .find_map(|block| match block {
                Block::Table(current) if current.id() == table.id() => Some(caret(
                    current.rows()[target_row].cells()[target_column].paragraphs()[0].id(),
                )),
                _ => None,
            })
            .or_else(|| crate::first_cursor(&candidate));
        self.commit_from(transaction, Some(selected), after)?;
        self.typing = None;
        Ok(())
    }

    /// Table traversal changes only Rust selection, never revision or history.
    pub(super) fn move_table_cell(
        &mut self,
        direction: Direction,
        supplied: Option<BrowserSelection>,
    ) -> Result<(), EngineError> {
        let (_, table, row, column) = self.selected_cell(supplied)?;
        let columns = table.rows()[0].cells().len();
        let index = row * columns + column;
        let target = match direction {
            Direction::Backward => index.checked_sub(1),
            Direction::Forward => index
                .checked_add(1)
                .filter(|at| *at < table.rows().len() * columns),
        };
        let paragraph = if let Some(target) = target {
            Some(
                table.rows()[target / columns].cells()[target % columns].paragraphs()[0]
                    .id()
                    .clone(),
            )
        } else {
            self.session
                .document()
                .sections()
                .iter()
                .find_map(|section| {
                    let index = section.blocks().iter().position(|block|
                    matches!(block, Block::Table(current) if current.id() == table.id()))?;
                    let neighbor = match direction {
                        Direction::Backward => {
                            index.checked_sub(1).and_then(|at| section.blocks().get(at))
                        }
                        Direction::Forward => section.blocks().get(index + 1),
                    };
                    match neighbor {
                        Some(Block::Paragraph(paragraph)) => Some(paragraph.id().clone()),
                        _ => None,
                    }
                })
        };
        if let Some(paragraph) = paragraph {
            self.selection = Some(caret(&paragraph));
            self.typing = None;
            self.layout_state.reset_visual();
        }
        Ok(())
    }
}
