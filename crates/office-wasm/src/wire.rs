use office_core::{Alignment, ImageAlignment, ListKind, PageMargins, PageSize};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Point {
    pub paragraph_id: String,
    pub utf16: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BrowserSelection {
    pub anchor: Point,
    pub focus: Point,
}

#[derive(Deserialize)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(crate) enum Command {
    NewDocument {},
    ForkDocument {},
    SetTitle {
        title: String,
        expected_revision: Option<String>,
    },
    SetSelection {
        selection: Option<BrowserSelection>,
        affinity: Option<Affinity>,
        expected_revision: Option<String>,
    },
    SelectImage {
        image_id: Option<String>,
        expected_revision: Option<String>,
    },
    ResizeImage {
        width_pt: Option<f64>,
        height_pt: Option<f64>,
        #[serde(default = "locked_aspect")]
        lock_aspect: bool,
        expected_revision: Option<String>,
    },
    UpdateImage {
        width_pt: Option<f64>,
        height_pt: Option<f64>,
        #[serde(default = "locked_aspect")]
        lock_aspect: bool,
        alt_text: Option<String>,
        alignment: Option<ImageAlignment>,
        expected_revision: Option<String>,
    },
    SetImageAltText {
        alt_text: String,
        expected_revision: Option<String>,
    },
    SetImageAlignment {
        alignment: ImageAlignment,
        expected_revision: Option<String>,
    },
    DeleteImage {
        expected_revision: Option<String>,
    },
    MoveImageCursor {
        direction: Direction,
        expected_revision: Option<String>,
    },
    ReplaceText {
        text: String,
        selection: Option<BrowserSelection>,
        expected_revision: Option<String>,
    },
    ReplaceAll {
        query: String,
        text: String,
        expected_revision: Option<String>,
    },
    InsertParagraph {
        selection: Option<BrowserSelection>,
        expected_revision: Option<String>,
    },
    InsertTable {
        rows: usize,
        columns: usize,
        selection: Option<BrowserSelection>,
        expected_revision: Option<String>,
    },
    TableAction {
        action: TableAction,
        selection: Option<BrowserSelection>,
        expected_revision: Option<String>,
    },
    MoveTableCell {
        direction: Direction,
        selection: Option<BrowserSelection>,
        expected_revision: Option<String>,
    },
    InsertLineBreak {
        selection: Option<BrowserSelection>,
        expected_revision: Option<String>,
    },
    Delete {
        direction: Direction,
        selection: Option<BrowserSelection>,
        expected_revision: Option<String>,
    },
    FormatCharacter {
        bold: Option<bool>,
        italic: Option<bool>,
        underline: Option<bool>,
        font_family: Option<String>,
        font_size_pt: Option<f64>,
        selection: Option<BrowserSelection>,
        expected_revision: Option<String>,
    },
    FormatParagraph {
        alignment: Alignment,
        selection: Option<BrowserSelection>,
        expected_revision: Option<String>,
    },
    SetPageSetup {
        section_id: String,
        page_size: PageSize,
        page_margins: PageMargins,
        expected_revision: Option<String>,
    },
    InsertSectionBreak {
        selection: Option<BrowserSelection>,
        expected_revision: Option<String>,
    },
    RemoveSectionBreak {
        section_id: String,
        #[serde(default)]
        confirm_layout_change: bool,
        expected_revision: Option<String>,
    },
    InsertPageBreak {
        selection: Option<BrowserSelection>,
        expected_revision: Option<String>,
    },
    SetParagraphPagination {
        keep_together: Option<bool>,
        keep_with_next: Option<bool>,
        selection: Option<BrowserSelection>,
        expected_revision: Option<String>,
    },
    SetPageBreakBefore {
        enabled: bool,
        selection: Option<BrowserSelection>,
        expected_revision: Option<String>,
    },
    SetParagraphMetrics {
        spacing_before_pt: Option<f64>,
        spacing_after_pt: Option<f64>,
        line_spacing_multiple: Option<f64>,
        indent_left_in: Option<f64>,
        indent_right_in: Option<f64>,
        first_line_in: Option<f64>,
        selection: Option<BrowserSelection>,
        expected_revision: Option<String>,
    },
    SetList {
        list: Option<ListKind>,
        selection: Option<BrowserSelection>,
        expected_revision: Option<String>,
    },
    SetHyperlink {
        url: Option<String>,
        text: Option<String>,
        selection: Option<BrowserSelection>,
        expected_revision: Option<String>,
    },
    AssignStyle {
        style_id: Option<String>,
        selection: Option<BrowserSelection>,
        expected_revision: Option<String>,
    },
    Undo {
        expected_revision: Option<String>,
    },
    Redo {
        expected_revision: Option<String>,
    },
}

fn locked_aspect() -> bool {
    true
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TableAction {
    InsertRowAbove,
    InsertRowBelow,
    DeleteRow,
    InsertColumnBefore,
    InsertColumnAfter,
    DeleteColumn,
    DeleteTable,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Direction {
    Backward,
    Forward,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Affinity {
    Leading,
    Trailing,
}

impl Command {
    pub fn expected_revision(&self) -> Option<&str> {
        match self {
            Self::NewDocument {} | Self::ForkDocument {} => None,
            Self::SetTitle {
                expected_revision, ..
            }
            | Self::SetSelection {
                expected_revision, ..
            }
            | Self::SelectImage {
                expected_revision, ..
            }
            | Self::ResizeImage {
                expected_revision, ..
            }
            | Self::UpdateImage {
                expected_revision, ..
            }
            | Self::SetImageAltText {
                expected_revision, ..
            }
            | Self::SetImageAlignment {
                expected_revision, ..
            }
            | Self::DeleteImage { expected_revision }
            | Self::MoveImageCursor {
                expected_revision, ..
            }
            | Self::ReplaceText {
                expected_revision, ..
            }
            | Self::ReplaceAll {
                expected_revision, ..
            }
            | Self::InsertParagraph {
                expected_revision, ..
            }
            | Self::InsertTable {
                expected_revision, ..
            }
            | Self::TableAction {
                expected_revision, ..
            }
            | Self::MoveTableCell {
                expected_revision, ..
            }
            | Self::InsertLineBreak {
                expected_revision, ..
            }
            | Self::Delete {
                expected_revision, ..
            }
            | Self::FormatCharacter {
                expected_revision, ..
            }
            | Self::FormatParagraph {
                expected_revision, ..
            }
            | Self::SetPageSetup {
                expected_revision, ..
            }
            | Self::InsertSectionBreak {
                expected_revision, ..
            }
            | Self::RemoveSectionBreak {
                expected_revision, ..
            }
            | Self::InsertPageBreak {
                expected_revision, ..
            }
            | Self::SetPageBreakBefore {
                expected_revision, ..
            }
            | Self::SetParagraphPagination {
                expected_revision, ..
            }
            | Self::SetParagraphMetrics {
                expected_revision, ..
            }
            | Self::SetList {
                expected_revision, ..
            }
            | Self::SetHyperlink {
                expected_revision, ..
            }
            | Self::AssignStyle {
                expected_revision, ..
            }
            | Self::Undo { expected_revision }
            | Self::Redo { expected_revision } => expected_revision.as_deref(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct View {
    pub session_id: String,
    pub revision: String,
    pub document_id: String,
    pub title: String,
    pub paragraphs: Vec<ParagraphView>,
    pub body_blocks: Vec<BodyBlockView>,
    pub styles: Vec<StyleSummary>,
    pub selection: Option<BrowserSelection>,
    pub images: Vec<ImageView>,
    pub selected_image_id: Option<String>,
    pub table: Option<TableCellView>,
    pub page_setup: Option<PageSetupView>,
    pub format: FormatSummary,
    pub can_undo: bool,
    pub can_redo: bool,
    pub word_count: usize,
    pub original_source: Option<OriginalSourceView>,
    pub import_warnings: CompatibilityReportView,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PageSetupView {
    pub section_id: String,
    pub section_index: usize,
    pub section_count: usize,
    pub page_size: PageSize,
    pub page_margins: PageMargins,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BodyBlockView {
    pub kind: &'static str,
    pub id: String,
    pub section_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImageView {
    pub id: String,
    pub section_id: String,
    pub width_pt: f64,
    pub height_pt: f64,
    pub pixel_width: u32,
    pub pixel_height: u32,
    pub color_approximation: bool,
    pub orientation_applied: bool,
    pub alt_text: String,
    pub alignment: ImageAlignment,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OriginalSourceView {
    pub format: String,
    pub filename: String,
    pub byte_length: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
/// Bounded blocking diagnostics or non-blocking import normalization notices.
pub struct CompatibilityReportView {
    /// Concrete issues in deterministic traversal order.
    pub diagnostics: Vec<CompatibilityDiagnosticView>,
    /// Count of further issues omitted from the bounded transport.
    pub omitted_diagnostics: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
/// One interface-independent compatibility issue; text is human-readable.
pub struct CompatibilityDiagnosticView {
    /// Stable issue category.
    pub kind: String,
    /// Bounded package-part label.
    pub part: String,
    /// Bounded element/property path.
    pub path: String,
    /// Explanation of the unsupported construct.
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ParagraphView {
    pub id: String,
    pub section_id: String,
    pub table_cell: Option<TableCellView>,
    pub style_id: Option<String>,
    pub runs: Vec<RunView>,
    pub list: Option<ListKind>,
    pub format: ParagraphFormatView,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TableCellView {
    pub table_id: String,
    pub row_id: String,
    pub cell_id: String,
    pub row: usize,
    pub column: usize,
    pub row_count: usize,
    pub column_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RunView {
    pub text: String,
    pub format: CharacterView,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CharacterView {
    pub bold: bool,
    pub italic: bool,
    pub underline: &'static str,
    pub font_family: String,
    pub font_size_pt: f64,
    pub color: String,
    pub hyperlink: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ParagraphFormatView {
    pub alignment: Alignment,
    pub spacing_before_pt: f64,
    pub spacing_after_pt: f64,
    pub line_height: String,
    pub indent_left_pt: f64,
    pub indent_right_pt: f64,
    pub first_line_pt: f64,
    pub page_break_before: bool,
    pub keep_together: bool,
    pub keep_with_next: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FormatSummary {
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub font_family: Option<String>,
    pub font_size_pt: Option<f64>,
    pub alignment: Option<Alignment>,
    pub spacing_before_pt: Option<f64>,
    pub spacing_after_pt: Option<f64>,
    pub line_spacing_multiple: Option<f64>,
    pub indent_left_in: Option<f64>,
    pub indent_right_in: Option<f64>,
    pub first_line_in: Option<f64>,
    pub style_id: Option<String>,
    pub style_mixed: bool,
    pub list: Option<ListKind>,
    pub list_mixed: bool,
    pub hyperlink: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct StyleSummary {
    pub id: String,
    pub name: String,
}
