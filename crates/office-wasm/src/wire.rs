use office_core::{Alignment, ListKind};
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
    SetList {
        list: Option<ListKind>,
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
            | Self::ReplaceText {
                expected_revision, ..
            }
            | Self::ReplaceAll {
                expected_revision, ..
            }
            | Self::InsertParagraph {
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
            | Self::SetList {
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
    pub styles: Vec<StyleSummary>,
    pub selection: Option<BrowserSelection>,
    pub format: FormatSummary,
    pub can_undo: bool,
    pub can_redo: bool,
    pub word_count: usize,
    pub original_source: Option<OriginalSourceView>,
    pub import_warnings: CompatibilityReportView,
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
    pub style_id: Option<String>,
    pub runs: Vec<RunView>,
    pub list: Option<ListKind>,
    pub format: ParagraphFormatView,
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
    pub style_id: Option<String>,
    pub style_mixed: bool,
    pub list: Option<ListKind>,
    pub list_mixed: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct StyleSummary {
    pub id: String,
    pub name: String,
}
