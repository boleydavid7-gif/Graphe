//! Revision-bound layout transport and a single cache outside document history.

use std::io::{self, Write};

use office_core::{Document, GraphemeOffset, Id, TextCoordinates, Utf8Offset};
use office_layout::{
    CaretAffinity, CaretGeometry, DocumentLayout, LayoutEngine, LayoutOptions, LayoutPosition,
    NavigationDirection, SelectionGeometry,
};
use serde::{Deserialize, Serialize};
use unicode_segmentation::UnicodeSegmentation;

use crate::{
    BrowserEngine, Command, EngineError,
    wire::{BrowserSelection, Point},
};

const MAX_LAYOUT_REQUEST_BYTES: usize = 8 * 1024;
const MAX_LAYOUT_RESPONSE_BYTES: usize = 32 * 1024 * 1024;

#[derive(Default)]
pub(crate) struct LayoutState {
    engine: Option<LayoutEngine>,
    cached: Option<CachedLayout>,
    visual: Option<VisualSelection>,
}

struct CachedLayout {
    revision: String,
    options: String,
    json: String,
    layout: DocumentLayout,
}

struct VisualSelection {
    selection: BrowserSelection,
    anchor: CaretAffinity,
    focus: CaretAffinity,
    preferred_x: Option<i64>,
}

#[derive(Deserialize)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum LayoutAction {
    Hit {
        page_number: u32,
        x: i64,
        y: i64,
        #[serde(default)]
        extend: bool,
    },
    SelectWord {
        page_number: u32,
        x: i64,
        y: i64,
    },
    DragEndpoint {
        endpoint: SelectionEndpoint,
        page_number: u32,
        x: i64,
        y: i64,
    },
    Move {
        direction: NavigationDirection,
        #[serde(default)]
        extend: bool,
    },
    SelectAll {},
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SelectionEndpoint {
    Anchor,
    Focus,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GeometryView<'a> {
    session_id: &'a str,
    revision: String,
    document_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    view: Option<crate::View>,
    geometry: Option<SelectionGeometry>,
    preferred_x: Option<i64>,
}

impl LayoutState {
    pub(crate) fn reset_visual(&mut self) {
        self.visual = None;
    }

    pub(crate) fn invalidate(&mut self) {
        // Keep the immutable font catalog and shaping context, but release the
        // old document's potentially large geometry after a session replacement.
        self.cached = None;
        self.visual = None;
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LayoutView<'a> {
    session_id: &'a str,
    revision: &'a str,
    document_id: &'a str,
    layout: &'a DocumentLayout,
}

impl BrowserEngine {
    /// Returns bounded page, line, glyph and cursor geometry computed in Rust.
    ///
    /// Options are a strict JSON object; `{}` selects the pinned layout profile.
    /// If supplied, the revision must match this live session. The response is
    /// always bound to its session, document and current opaque revision token.
    /// Reading layout never creates an edit/history entry or changes selection.
    /// The font catalog initializes once on demand and survives document changes;
    /// one response is cached by revision and normalized options. Selection-only
    /// commands reuse that geometry and ordinary presentation snapshots contain
    /// no glyph outlines.
    pub fn layout_snapshot(
        &mut self,
        options_json: &str,
        expected_revision: Option<&str>,
    ) -> Result<String, EngineError> {
        self.ensure_layout(options_json, expected_revision)?;
        Ok(self
            .layout_state
            .cached
            .as_ref()
            .expect("layout prepared above")
            .json
            .clone())
    }

    fn ensure_layout(
        &mut self,
        options_json: &str,
        expected_revision: Option<&str>,
    ) -> Result<(), EngineError> {
        let revision = self.revision();
        if expected_revision.is_some_and(|expected| expected != revision) {
            return Err(EngineError::new(
                "stale_layout",
                "The requested layout revision no longer identifies the active document.",
            ));
        }
        if options_json.len() > MAX_LAYOUT_REQUEST_BYTES {
            return Err(EngineError::new(
                "invalid_layout_options",
                "Layout options exceed the 8 KiB request limit.",
            ));
        }
        if !options_json.trim_start().starts_with('{') {
            return Err(EngineError::new(
                "invalid_layout_options",
                "Layout options must be a JSON object.",
            ));
        }
        let options: LayoutOptions = serde_json::from_str(options_json)
            .map_err(|error| EngineError::new("invalid_layout_options", error.to_string()))?;
        let normalized = serde_json::to_string(&options)
            .map_err(|error| EngineError::new("invalid_layout_options", error.to_string()))?;
        if let Some(cached) = &self.layout_state.cached
            && cached.revision == revision
            && cached.options == normalized
        {
            return Ok(());
        }

        if self.layout_state.engine.is_none() {
            self.layout_state.engine = Some(
                LayoutEngine::new()
                    .map_err(|error| EngineError::new(error.code(), error.to_string()))?,
            );
        }
        let engine = self
            .layout_state
            .engine
            .as_mut()
            .expect("the layout engine was initialized above");
        let layout = engine
            .layout_document(self.session.document(), &options)
            .map_err(|error| EngineError::new(error.code(), error.to_string()))?;
        let view = LayoutView {
            session_id: &self.session_id,
            revision: &revision,
            document_id: self.session.document().id().as_str(),
            layout: &layout,
        };
        let mut output = LayoutOutput::default();
        serde_json::to_writer(&mut output, &view).map_err(|error| {
            if output.exceeded {
                EngineError::new(
                    "layout_too_large",
                    "The computed layout exceeds the 32 MiB response limit.",
                )
            } else {
                EngineError::new("layout_failed", error.to_string())
            }
        })?;
        let json =
            String::from_utf8(output.bytes).expect("the JSON serializer produces valid UTF-8");
        self.layout_state.cached = Some(CachedLayout {
            revision,
            options: normalized,
            json,
            layout,
        });
        Ok(())
    }

    /// Returns native caret and selection geometry without changing selection.
    ///
    /// The immutable layout is reused without parsing its serialized glyph data.
    /// Ordinary selection updates default to leading/downstream affinity; visual
    /// actions retain explicit affinity and vertical preferred x outside history.
    pub fn selection_geometry(
        &mut self,
        options_json: &str,
        expected_revision: Option<&str>,
    ) -> Result<String, EngineError> {
        self.ensure_layout(options_json, expected_revision)?;
        self.geometry_response(false)
    }

    /// Applies a revision-bound pointer or keyboard selection intention in Rust.
    ///
    /// Pointer actions use one-based pages and integer 1/64-point coordinates.
    /// `select_word` expands the touched shaper cell using pinned Unicode word
    /// boundaries; nonword cells select one whole grapheme. `drag_endpoint`
    /// moves the named logical endpoint without swapping it when handles cross.
    /// `move` addresses visual graphemes/lines; `select_all` spans all paragraphs.
    /// These actions do not edit content, advance revisions, or add undo entries.
    pub fn layout_action(
        &mut self,
        options_json: &str,
        action_json: &str,
        expected_revision: Option<&str>,
    ) -> Result<String, EngineError> {
        if action_json.len() > MAX_LAYOUT_REQUEST_BYTES
            || !action_json.trim_start().starts_with('{')
        {
            return Err(EngineError::new(
                "invalid_layout_action",
                "Layout action must be a JSON object within 8 KiB.",
            ));
        }
        let action: LayoutAction = serde_json::from_str(action_json)
            .map_err(|error| EngineError::new("invalid_layout_action", error.to_string()))?;
        self.ensure_layout(options_json, expected_revision)?;
        let layout = &self
            .layout_state
            .cached
            .as_ref()
            .expect("layout prepared above")
            .layout;
        let visual = self
            .layout_state
            .visual
            .as_ref()
            .filter(|visual| Some(&visual.selection) == self.selection.as_ref());
        let anchor_affinity = visual.map_or(CaretAffinity::Leading, |visual| visual.anchor);
        let focus_affinity = visual.map_or(CaretAffinity::Leading, |visual| visual.focus);
        let preferred = visual.and_then(|visual| visual.preferred_x);
        let initial = self.selection.clone().or_else(|| first_selection(layout));
        let mut selected = initial.clone();
        let mut anchor = anchor_affinity;
        let mut focus = focus_affinity;
        let mut preferred_x = None;
        match action {
            LayoutAction::Hit {
                page_number,
                x,
                y,
                extend,
            } => {
                if let Some(target) = layout.hit_test(page_number, x, y).map_err(layout_error)? {
                    selected = Some(selection_at(initial.as_ref(), &target, extend));
                    focus = target.position.affinity;
                    if !extend || initial.is_none() {
                        anchor = focus;
                    }
                }
            }
            LayoutAction::SelectWord { page_number, x, y } => {
                if let Some(target) = layout.hit_test(page_number, x, y).map_err(layout_error)? {
                    selected = Some(word_selection(self.session.document(), layout, &target, x)?);
                    anchor = CaretAffinity::Leading;
                    focus = CaretAffinity::Trailing;
                }
            }
            LayoutAction::DragEndpoint {
                endpoint: moving,
                page_number,
                x,
                y,
            } => {
                let mut selection = self.selection.clone().ok_or_else(|| {
                    EngineError::new(
                        "invalid_selection",
                        "Select text before dragging a selection endpoint.",
                    )
                })?;
                if let Some(target) = layout.hit_test(page_number, x, y).map_err(layout_error)? {
                    let point = Point {
                        paragraph_id: target.position.paragraph_id,
                        utf16: target.position.utf16,
                    };
                    match moving {
                        SelectionEndpoint::Anchor => {
                            selection.anchor = point;
                            anchor = target.position.affinity;
                        }
                        SelectionEndpoint::Focus => {
                            selection.focus = point;
                            focus = target.position.affinity;
                        }
                    }
                    selected = Some(selection);
                }
            }
            LayoutAction::Move { direction, extend } => {
                if let Some(current) = &initial {
                    let mut point = endpoint(&current.focus, focus);
                    let collapsed = current.anchor == current.focus;
                    let horizontal = matches!(
                        direction,
                        NavigationDirection::Left | NavigationDirection::Right
                    );
                    let target = if !extend && !collapsed && horizontal {
                        let first = layout
                            .caret_geometry(&endpoint(&current.anchor, anchor))
                            .map_err(layout_error)?;
                        let last = layout.caret_geometry(&point).map_err(layout_error)?;
                        let first_is_left = (first.page_number, first.rect.y, first.rect.x)
                            <= (last.page_number, last.rect.y, last.rect.x);
                        if first_is_left == (direction == NavigationDirection::Left) {
                            first
                        } else {
                            last
                        }
                    } else {
                        let moved = layout
                            .move_caret(&point, direction, preferred)
                            .map_err(layout_error)?;
                        preferred_x = moved.preferred_x;
                        moved.caret
                    };
                    point.affinity = target.position.affinity;
                    selected = Some(selection_at(Some(current), &target, extend));
                    focus = point.affinity;
                    if !extend {
                        anchor = focus;
                    }
                }
            }
            LayoutAction::SelectAll {} => {
                if let (Some(first), Some(last)) = (
                    layout.pages.iter().flat_map(|page| &page.lines).next(),
                    layout.pages.iter().flat_map(|page| &page.lines).last(),
                ) {
                    selected = Some(BrowserSelection {
                        anchor: Point {
                            paragraph_id: first.paragraph_id.clone(),
                            utf16: first.text_range.start_utf16,
                        },
                        focus: Point {
                            paragraph_id: last.paragraph_id.clone(),
                            utf16: last.text_range.end_utf16,
                        },
                    });
                    anchor = CaretAffinity::Leading;
                    focus = CaretAffinity::Trailing;
                }
            }
        }
        // Check all geometry before committing even a selection-only update.
        if let Some(selection) = &selected {
            layout
                .selection_geometry(
                    &endpoint(&selection.anchor, anchor),
                    &endpoint(&selection.focus, focus),
                )
                .map_err(layout_error)?;
        }
        let previous_selection = self.selection.clone();
        let previous_typing = self.typing.clone();
        let previous_visual = self.layout_state.visual.take();
        let result = self.execute(Command::SetSelection {
            selection: selected.clone(),
            affinity: None,
            expected_revision: None,
        });
        if let Err(error) = result {
            self.selection = previous_selection;
            self.typing = previous_typing;
            self.layout_state.visual = previous_visual;
            return Err(error);
        }
        self.layout_state.visual = selected.map(|selection| VisualSelection {
            selection,
            anchor,
            focus,
            preferred_x,
        });
        let response = self.geometry_response(true);
        if response.is_err() {
            // A bounded transport failure is still a failed intention. Retain
            // the previous input format, selection, and visual runtime state.
            self.selection = previous_selection;
            self.typing = previous_typing;
            self.layout_state.visual = previous_visual;
        }
        response
    }

    fn geometry_response(&self, include_view: bool) -> Result<String, EngineError> {
        let layout = &self
            .layout_state
            .cached
            .as_ref()
            .expect("layout prepared above")
            .layout;
        let visual = self
            .layout_state
            .visual
            .as_ref()
            .filter(|visual| Some(&visual.selection) == self.selection.as_ref());
        let geometry = self
            .selection
            .as_ref()
            .map(|selection| {
                layout
                    .selection_geometry(
                        &endpoint(
                            &selection.anchor,
                            visual.map_or(CaretAffinity::Leading, |visual| visual.anchor),
                        ),
                        &endpoint(
                            &selection.focus,
                            visual.map_or(CaretAffinity::Leading, |visual| visual.focus),
                        ),
                    )
                    .map_err(layout_error)
            })
            .transpose()?;
        let response = GeometryView {
            session_id: &self.session_id,
            revision: self.revision(),
            document_id: self.session.document().id().as_str(),
            view: include_view.then(|| self.make_view()),
            geometry,
            preferred_x: visual.and_then(|visual| visual.preferred_x),
        };
        let mut output = LayoutOutput::default();
        serde_json::to_writer(&mut output, &response).map_err(|error| {
            EngineError::new(
                if output.exceeded {
                    "layout_too_large"
                } else {
                    "layout_failed"
                },
                error.to_string(),
            )
        })?;
        String::from_utf8(output.bytes)
            .map_err(|error| EngineError::new("layout_failed", error.to_string()))
    }
}

fn layout_error(error: office_layout::LayoutError) -> EngineError {
    EngineError::new(error.code(), error.to_string())
}

fn endpoint(point: &Point, affinity: CaretAffinity) -> LayoutPosition {
    LayoutPosition {
        paragraph_id: point.paragraph_id.clone(),
        utf16: point.utf16,
        affinity,
    }
}

fn selection_at(
    current: Option<&BrowserSelection>,
    target: &CaretGeometry,
    extend: bool,
) -> BrowserSelection {
    let focus = Point {
        paragraph_id: target.position.paragraph_id.clone(),
        utf16: target.position.utf16,
    };
    let anchor = current
        .filter(|_| extend)
        .map_or_else(|| focus.clone(), |selection| selection.anchor.clone());
    BrowserSelection { anchor, focus }
}

/// Select a UAX #29 word from the complete paragraph, including across runs.
///
/// Geometry chooses a whole grapheme cell before word segmentation, so the right
/// half of a word's final glyph does not accidentally select following space.
/// Emoji, punctuation and whitespace outside Unicode words select one whole
/// extended grapheme. A blank paragraph stays collapsed. No text is normalized.
fn word_selection(
    document: &Document,
    layout: &DocumentLayout,
    target: &CaretGeometry,
    x: i64,
) -> Result<BrowserSelection, EngineError> {
    let coordinates = document.text_coordinates(&Id::new(&target.position.paragraph_id)?)?;
    let count = coordinates.grapheme_len().get();
    let boundary = coordinates
        .utf8_to_grapheme(Utf8Offset::new(target.position.utf8))?
        .get();
    let selected = if count == 0 {
        (0, 0)
    } else {
        let index = touched_grapheme(layout, target, &coordinates, boundary, x);
        let start = coordinates
            .grapheme_to_utf8(GraphemeOffset::new(index))?
            .get();
        let end = coordinates
            .grapheme_to_utf8(GraphemeOffset::new(index + 1))?
            .get();
        let (start, end) = coordinates
            .text()
            .unicode_word_indices()
            .find(|(word_start, word)| *word_start <= start && start < word_start + word.len())
            .map_or((start, end), |(start, word)| (start, start + word.len()));
        enclosing_graphemes(&coordinates, start, end)?
    };
    let point = |index| -> Result<Point, EngineError> {
        Ok(Point {
            paragraph_id: target.position.paragraph_id.clone(),
            utf16: coordinates
                .grapheme_to_utf16(GraphemeOffset::new(index))?
                .get(),
        })
    };
    Ok(BrowserSelection {
        anchor: point(selected.0)?,
        focus: point(selected.1)?,
    })
}

/// Pick between the two logical grapheme cells adjoining the nearest caret.
/// Both sides use existing Rust leading/trailing geometry, including bidi.
/// Only a cell on the actual hit line/page can contain this pointer position.
fn touched_grapheme(
    layout: &DocumentLayout,
    target: &CaretGeometry,
    coordinates: &TextCoordinates,
    boundary: usize,
    x: i64,
) -> usize {
    let count = coordinates.grapheme_len().get();
    let fallback = if target.position.affinity == CaretAffinity::Trailing || boundary == count {
        boundary.saturating_sub(1)
    } else {
        boundary
    };
    [
        boundary.checked_sub(1),
        (boundary < count).then_some(boundary),
    ]
    .into_iter()
    .flatten()
    .filter_map(|index| {
        let position = |index, affinity| {
            Some(LayoutPosition {
                paragraph_id: target.position.paragraph_id.clone(),
                utf16: coordinates
                    .grapheme_to_utf16(GraphemeOffset::new(index))
                    .ok()?
                    .get(),
                affinity,
            })
        };
        let start = layout
            .caret_geometry(&position(index, CaretAffinity::Leading)?)
            .ok()?;
        let end = layout
            .caret_geometry(&position(index + 1, CaretAffinity::Trailing)?)
            .ok()?;
        if start.page_number != target.page_number
            || end.page_number != target.page_number
            || start.line_index != target.line_index
            || end.line_index != target.line_index
        {
            return None;
        }
        let left = start.rect.x.min(end.rect.x);
        let right = start.rect.x.max(end.rect.x);
        if x < left || x > right {
            return None;
        }
        let center_distance = (i128::from(x) * 2 - i128::from(left) - i128::from(right)).abs();
        Some((center_distance, index != fallback, index))
    })
    .min()
    .map_or(fallback, |(_, _, index)| index)
}

/// UAX word spans expand outward if they meet an extended grapheme interior.
/// This is a selection rule, never a repair or modification of source text.
fn enclosing_graphemes(
    coordinates: &TextCoordinates,
    start: usize,
    end: usize,
) -> Result<(usize, usize), EngineError> {
    let mut first = 0;
    for index in 0..=coordinates.grapheme_len().get() {
        let byte = coordinates
            .grapheme_to_utf8(GraphemeOffset::new(index))?
            .get();
        if byte <= start {
            first = index;
        }
        if byte >= end {
            return Ok((first, index));
        }
    }
    unreachable!("word segmentation spans cannot exceed the source paragraph")
}

fn first_selection(layout: &DocumentLayout) -> Option<BrowserSelection> {
    let first = layout.pages.iter().flat_map(|page| &page.lines).next()?;
    let point = Point {
        paragraph_id: first.paragraph_id.clone(),
        utf16: first.text_range.start_utf16,
    };
    Some(BrowserSelection {
        anchor: point.clone(),
        focus: point,
    })
}

#[derive(Default)]
struct LayoutOutput {
    bytes: Vec<u8>,
    exceeded: bool,
}

impl Write for LayoutOutput {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.bytes.len().saturating_add(bytes.len()) > MAX_LAYOUT_RESPONSE_BYTES {
            self.exceeded = true;
            return Err(io::Error::other("layout response byte budget exceeded"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
