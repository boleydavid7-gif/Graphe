//! Interaction over immutable shaper geometry, without DOM text measurement.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{Caret, CaretAffinity, DocumentLayout, LayoutError, LineLayout, Rect, UNITS_PER_POINT};

const MAX_COORDINATE: i64 = 64_000_000;

/// A transient paragraph endpoint; its revision belongs to the calling session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LayoutPosition {
    /// Stable source paragraph identifier.
    pub paragraph_id: String,
    /// Exact core UTF-16 grapheme boundary, never a scalar approximation.
    pub utf16: usize,
    /// Leading prefers the following grapheme and the downstream wrapped line.
    pub affinity: CaretAffinity,
}

/// A checked source endpoint with each coordinate system represented explicitly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualPosition {
    /// Stable source paragraph identifier.
    pub paragraph_id: String,
    /// Paragraph-relative UTF-8 byte boundary.
    pub utf8: usize,
    /// Paragraph-relative UTF-16 code-unit boundary.
    pub utf16: usize,
    /// Paragraph-relative core grapheme boundary ordinal.
    pub grapheme: usize,
    /// The actual visual neighbor used for this caret.
    pub affinity: CaretAffinity,
}

impl VisualPosition {
    /// Retains the exact UTF-16 endpoint and affinity for a later interaction.
    #[must_use]
    pub fn endpoint(&self) -> LayoutPosition {
        LayoutPosition {
            paragraph_id: self.paragraph_id.clone(),
            utf16: self.utf16,
            affinity: self.affinity,
        }
    }
}

/// A one-point caret rectangle in page-local fixed units.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaretGeometry {
    /// Checked logical source position.
    pub position: VisualPosition,
    /// One-based physical page number.
    pub page_number: u32,
    /// Zero-based paragraph line ordinal, including page continuation.
    pub line_index: u32,
    /// Position and extent in 1/64-point units.
    pub rect: Rect,
}

/// One contiguous visual portion of a logical selection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionRectangle {
    /// One-based physical page number.
    pub page_number: u32,
    /// Stable source paragraph identifier.
    pub paragraph_id: String,
    /// Zero-based source paragraph line ordinal.
    pub line_index: u32,
    /// Page-local highlight geometry in fixed units.
    pub rect: Rect,
}

/// Directed endpoint geometry and logically selected, possibly disjoint areas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionGeometry {
    /// Fixed endpoint, retaining selection direction.
    pub anchor: CaretGeometry,
    /// Moving endpoint.
    pub focus: CaretGeometry,
    /// Visual highlights in page and line order; bidi ranges may be disjoint.
    pub rectangles: Vec<SelectionRectangle>,
}

/// Supported movement intentions; no platform key interpretation occurs here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NavigationDirection {
    /// Previous visual grapheme, traversing line and paragraph boundaries.
    Left,
    /// Next visual grapheme, traversing line and paragraph boundaries.
    Right,
    /// Previous physical line, retaining the preferred page-local x coordinate.
    Up,
    /// Next physical line, retaining the preferred page-local x coordinate.
    Down,
    /// Left edge of the current physical line.
    Home,
    /// Right edge of the current physical line before its mandatory break.
    End,
    /// First logical paragraph endpoint in the laid-out document.
    DocumentStart,
    /// Last logical paragraph endpoint in the laid-out document.
    DocumentEnd,
}

/// A movement destination and vertical-navigation state outside document history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NavigationResult {
    /// The new checked caret placement.
    pub caret: CaretGeometry,
    /// Retained x for repeated up/down; absent after other movement.
    pub preferred_x: Option<i64>,
}

#[derive(Clone, Copy)]
struct PhysicalLine<'a> {
    page: u32,
    line: &'a LineLayout,
}

#[derive(Clone, Copy)]
struct Span<'a> {
    start: &'a Caret,
    end: &'a Caret,
}

impl<'a> Span<'a> {
    fn left(self) -> i64 {
        self.start.x.min(self.end.x)
    }
    fn right(self) -> i64 {
        self.start.x.max(self.end.x)
    }
    fn edge(self, right: bool) -> &'a Caret {
        if (self.start.x <= self.end.x) == right {
            self.end
        } else {
            self.start
        }
    }
}

impl DocumentLayout {
    /// Finds the nearest line and core cursor boundary on a particular page.
    ///
    /// Coordinates may lie in page margins, but must fit the interaction budget.
    /// Empty pages return `None`; content is never synthesized. Equal-distance
    /// bidi placements prefer a leading caret, then the lower logical offset.
    pub fn hit_test(
        &self,
        page_number: u32,
        x: i64,
        y: i64,
    ) -> Result<Option<CaretGeometry>, LayoutError> {
        coordinate(x)?;
        coordinate(y)?;
        let page = self
            .pages
            .iter()
            .find(|page| page.number == page_number)
            .ok_or_else(|| invalid("page number does not identify this layout"))?;
        // Images have their own stable object selection, not a fabricated text
        // cursor. A point on an image must not jump to a nearby paragraph.
        if page.images.iter().any(|image| contains(image.rect, x, y)) {
            return Ok(None);
        }
        // Multiple table cells share a physical y range. Restrict a point in a
        // cell to that cell's source paragraphs, including its blank padding.
        let cell = page
            .table_cells
            .iter()
            .find(|cell| contains(cell.rect, x, y));
        let Some(line) = page
            .lines
            .iter()
            .filter(|line| cell.is_none_or(|cell| cell.paragraph_ids.contains(&line.paragraph_id)))
            .min_by_key(|line| {
                (
                    distance_to_interval(y, line.y, i128::from(line.y) + i128::from(line.height)),
                    horizontal_distance(line, x),
                )
            })
        else {
            return Ok(None);
        };
        let physical = PhysicalLine {
            page: page.number,
            line,
        };
        Ok(nearest(physical, x).map(|caret| geometry(physical, caret)))
    }

    /// Resolves an exact endpoint, preserving bidi and wrapped-line affinity.
    ///
    /// Leading prefers the downstream line at a wrap or mandatory break. An
    /// unavailable affinity falls back to the same exact logical boundary.
    pub fn caret_geometry(&self, position: &LayoutPosition) -> Result<CaretGeometry, LayoutError> {
        let lines = physical_lines(self);
        let (index, caret) = resolve(&lines, position)?;
        Ok(geometry(lines[index], caret))
    }

    /// Projects a directed logical range into visual rectangles, including bidi.
    ///
    /// Grapheme spans use their leading start and trailing end. Selected zero-
    /// advance text and paragraph separators receive a one-point marker rather
    /// than an invented font advance. No document content is changed.
    pub fn selection_geometry(
        &self,
        anchor: &LayoutPosition,
        focus: &LayoutPosition,
    ) -> Result<SelectionGeometry, LayoutError> {
        let lines = physical_lines(self);
        let (anchor_line, anchor_caret) = resolve(&lines, anchor)?;
        let (focus_line, focus_caret) = resolve(&lines, focus)?;
        let mut output = SelectionGeometry {
            anchor: geometry(lines[anchor_line], anchor_caret),
            focus: geometry(lines[focus_line], focus_caret),
            rectangles: Vec::new(),
        };
        if same_position(anchor, focus) {
            return Ok(output);
        }
        let order = paragraph_order(&lines);
        let anchor_order = order[anchor.paragraph_id.as_str()];
        let focus_order = order[focus.paragraph_id.as_str()];
        let (start, end, start_order, end_order) =
            if (anchor_order, anchor.utf16) <= (focus_order, focus.utf16) {
                (anchor, focus, anchor_order, focus_order)
            } else {
                (focus, anchor, focus_order, anchor_order)
            };
        for (index, &physical) in lines.iter().enumerate() {
            let paragraph = order[physical.line.paragraph_id.as_str()];
            if paragraph < start_order || paragraph > end_order {
                continue;
            }
            let from = if paragraph == start_order {
                start.utf16
            } else {
                0
            };
            let to = if paragraph == end_order {
                end.utf16
            } else {
                usize::MAX
            };
            let mut intervals = Vec::new();
            for span in spans(physical.line) {
                if span.start.utf16 >= from && span.end.utf16 <= to {
                    let right = if span.left() == span.right() {
                        span.right().saturating_add(UNITS_PER_POINT)
                    } else {
                        span.right()
                    };
                    intervals.push((span.left(), right));
                }
            }
            // Paragraph separators have no semantic text byte or glyph. Show a
            // marker when this range continues to a later paragraph.
            if paragraph < end_order
                && lines
                    .get(index + 1)
                    .is_none_or(|next| next.line.paragraph_id != physical.line.paragraph_id)
                && let Some(caret) = physical.line.carets.iter().max_by_key(|caret| caret.utf16)
            {
                intervals.push((caret.x, caret.x.saturating_add(UNITS_PER_POINT)));
            }
            intervals.sort_unstable();
            let mut merged: Vec<(i64, i64)> = Vec::new();
            for (left, right) in intervals {
                if let Some(last) = merged.last_mut()
                    && left <= last.1
                {
                    last.1 = last.1.max(right);
                } else {
                    merged.push((left, right));
                }
            }
            for (left, right) in merged {
                output.rectangles.push(SelectionRectangle {
                    page_number: physical.page,
                    paragraph_id: physical.line.paragraph_id.clone(),
                    line_index: physical.line.line_index,
                    rect: Rect {
                        x: left,
                        y: physical.line.y,
                        width: right.saturating_sub(left),
                        height: physical.line.height,
                    },
                });
            }
        }
        Ok(output)
    }

    /// Moves among shaper-derived visual grapheme edges, without editing text.
    ///
    /// Horizontal movement crosses a grapheme rather than visiting aliases of
    /// the same logical boundary twice. At wrapped boundaries its destination
    /// prefers the downstream line. Up/down retain a preferred x independently
    /// from short intervening lines. Home/end address physical visual edges.
    pub fn move_caret(
        &self,
        position: &LayoutPosition,
        direction: NavigationDirection,
        preferred_x: Option<i64>,
    ) -> Result<NavigationResult, LayoutError> {
        if let Some(x) = preferred_x {
            coordinate(x)?;
        }
        let lines = physical_lines(self);
        let (index, caret) = resolve(&lines, position)?;
        let current = geometry(lines[index], caret);
        let mut preferred = None;
        let destination = match direction {
            NavigationDirection::Left | NavigationDirection::Right => horizontal(
                &lines,
                index,
                caret,
                direction == NavigationDirection::Right,
            )
            .unwrap_or(current),
            NavigationDirection::Up | NavigationDirection::Down => {
                let x = preferred_x.unwrap_or(caret.x);
                preferred = Some(x);
                vertical(
                    self,
                    &lines,
                    index,
                    x,
                    direction == NavigationDirection::Down,
                )
                .unwrap_or(current)
            }
            NavigationDirection::Home | NavigationDirection::End => {
                let right = direction == NavigationDirection::End;
                edge(lines[index], right)
                    .map(|caret| geometry(lines[index], caret))
                    .unwrap_or(current)
            }
            NavigationDirection::DocumentStart | NavigationDirection::DocumentEnd => {
                let last = direction == NavigationDirection::DocumentEnd;
                let line = if last { lines.last() } else { lines.first() };
                line.and_then(|line| {
                    let caret = if last {
                        line.line.carets.iter().max_by_key(|caret| caret.utf16)
                    } else {
                        line.line.carets.iter().min_by_key(|caret| caret.utf16)
                    }?;
                    Some(geometry(*line, caret))
                })
                .unwrap_or(current)
            }
        };
        Ok(NavigationResult {
            caret: destination,
            preferred_x: preferred,
        })
    }
}

fn contains(rect: Rect, x: i64, y: i64) -> bool {
    x >= rect.x
        && i128::from(x) < i128::from(rect.x) + i128::from(rect.width)
        && y >= rect.y
        && i128::from(y) < i128::from(rect.y) + i128::from(rect.height)
}

fn horizontal_distance(line: &LineLayout, x: i64) -> i128 {
    let left = line
        .carets
        .iter()
        .map(|caret| caret.x)
        .min()
        .unwrap_or(line.x);
    let right = line
        .carets
        .iter()
        .map(|caret| caret.x)
        .max()
        .unwrap_or(line.x);
    distance_to_interval(x, left, i128::from(right))
}

fn vertical(
    layout: &DocumentLayout,
    lines: &[PhysicalLine<'_>],
    index: usize,
    x: i64,
    down: bool,
) -> Option<CaretGeometry> {
    let current = lines[index];
    let cell = layout
        .pages
        .iter()
        .find(|page| page.number == current.page)
        .and_then(|page| {
            page.table_cells
                .iter()
                .find(|cell| cell.paragraph_ids.contains(&current.line.paragraph_id))
        });
    // Within a cell, stay in its text flow before moving to the adjacent row.
    // Traversing the row-major vector alone would send Down into the next cell.
    let same_cell = cell.and_then(|cell| {
        lines
            .iter()
            .copied()
            .filter(|line| {
                line.page == current.page
                    && cell.paragraph_ids.contains(&line.line.paragraph_id)
                    && if down {
                        line.line.y > current.line.y
                    } else {
                        line.line.y < current.line.y
                    }
            })
            .min_by_key(|line| {
                (
                    distance(line.line.y, current.line.y),
                    horizontal_distance(line.line, x),
                )
            })
    });
    let target = same_cell.or_else(|| {
        let boundary = cell.map_or(i128::from(current.line.y), |cell| {
            if down {
                i128::from(cell.rect.y) + i128::from(cell.rect.height) - 1
            } else {
                i128::from(cell.rect.y)
            }
        });
        lines
            .iter()
            .copied()
            .filter(|line| {
                if line.page != current.page {
                    if down {
                        line.page > current.page
                    } else {
                        line.page < current.page
                    }
                } else if down {
                    i128::from(line.line.y) > boundary
                } else {
                    i128::from(line.line.y) < boundary
                }
            })
            .min_by_key(|line| {
                let page_distance = line.page.abs_diff(current.page);
                let y = if line.page != current.page {
                    if down {
                        i128::from(line.line.y)
                    } else {
                        -i128::from(line.line.y)
                    }
                } else {
                    (i128::from(line.line.y) - boundary).abs()
                };
                (page_distance, y, horizontal_distance(line.line, x))
            })
    })?;
    nearest(target, x).map(|caret| geometry(target, caret))
}

fn physical_lines(layout: &DocumentLayout) -> Vec<PhysicalLine<'_>> {
    layout
        .pages
        .iter()
        .flat_map(|page| {
            page.lines.iter().map(|line| PhysicalLine {
                page: page.number,
                line,
            })
        })
        .collect()
}

fn paragraph_order<'a>(lines: &[PhysicalLine<'a>]) -> BTreeMap<&'a str, usize> {
    let mut order = BTreeMap::new();
    for physical in lines {
        let next = order.len();
        order
            .entry(physical.line.paragraph_id.as_str())
            .or_insert(next);
    }
    order
}

fn resolve<'a>(
    lines: &[PhysicalLine<'a>],
    position: &LayoutPosition,
) -> Result<(usize, &'a Caret), LayoutError> {
    if position.paragraph_id.len() > 1024 {
        return Err(invalid("paragraph identifier exceeds the layout budget"));
    }
    let mut fallback = None;
    for (index, physical) in lines.iter().enumerate() {
        if physical.line.paragraph_id != position.paragraph_id {
            continue;
        }
        for caret in &physical.line.carets {
            if caret.utf16 != position.utf16 {
                continue;
            }
            if caret.affinity == position.affinity {
                return Ok((index, caret));
            }
            fallback.get_or_insert((index, caret));
        }
    }
    fallback.ok_or_else(|| invalid("endpoint is not a core cursor boundary in this layout"))
}

fn geometry(physical: PhysicalLine<'_>, caret: &Caret) -> CaretGeometry {
    CaretGeometry {
        position: VisualPosition {
            paragraph_id: physical.line.paragraph_id.clone(),
            utf8: caret.utf8,
            utf16: caret.utf16,
            grapheme: caret.grapheme,
            affinity: caret.affinity,
        },
        page_number: physical.page,
        line_index: physical.line.line_index,
        rect: Rect {
            x: caret.x,
            y: caret.y,
            width: UNITS_PER_POINT,
            height: caret.height,
        },
    }
}

fn nearest(physical: PhysicalLine<'_>, x: i64) -> Option<&Caret> {
    physical.line.carets.iter().min_by_key(|caret| {
        (
            distance(x, caret.x),
            caret.affinity != CaretAffinity::Leading,
            caret.utf16,
        )
    })
}

fn edge(physical: PhysicalLine<'_>, right: bool) -> Option<&Caret> {
    let mut carets: Vec<_> = physical.line.carets.iter().collect();
    // The position after a mandatory newline belongs to the downstream line.
    if physical
        .line
        .text
        .ends_with(['\n', '\r', '\u{2028}', '\u{2029}'])
    {
        carets.retain(|caret| caret.utf16 != physical.line.text_range.end_utf16);
    }
    carets.into_iter().min_by_key(|caret| {
        let x = i128::from(caret.x);
        (
            if right { -x } else { x },
            caret.affinity != CaretAffinity::Leading,
            caret.utf16,
        )
    })
}

fn spans(line: &LineLayout) -> Vec<Span<'_>> {
    let mut boundaries = BTreeMap::<usize, (Option<&Caret>, Option<&Caret>)>::new();
    for caret in &line.carets {
        let pair = boundaries.entry(caret.utf16).or_default();
        match caret.affinity {
            CaretAffinity::Leading => pair.0 = Some(caret),
            CaretAffinity::Trailing => pair.1 = Some(caret),
        }
    }
    let points: Vec<_> = boundaries.into_values().collect();
    points
        .windows(2)
        .filter_map(|pair| {
            let start = pair[0].0.or(pair[0].1)?;
            let end = pair[1].1.or(pair[1].0)?;
            (start.grapheme.checked_add(1) == Some(end.grapheme)).then_some(Span { start, end })
        })
        .collect()
}

fn horizontal(
    lines: &[PhysicalLine<'_>],
    index: usize,
    current: &Caret,
    right: bool,
) -> Option<CaretGeometry> {
    let physical = lines[index];
    let mut boxes = spans(physical.line);
    boxes.sort_by_key(|span| (span.left(), span.right(), span.start.utf16));
    let associated = boxes.iter().position(|span| {
        if current.affinity == CaretAffinity::Leading {
            span.start.utf16 == current.utf16
        } else {
            span.end.utf16 == current.utf16
        }
    });
    if let Some(associated) = associated {
        let span = boxes[associated];
        let far = span.edge(right);
        let target = if far.utf16 != current.utf16 {
            Some(far)
        } else {
            let next = if right {
                associated.checked_add(1)
            } else {
                associated.checked_sub(1)
            };
            next.and_then(|next| boxes.get(next))
                .map(|span| span.edge(right))
        };
        if let Some(target) = target
            && target.utf16 != current.utf16
        {
            return Some(downstream(lines, index, target));
        }
    }
    // Visual horizontal direction and logical line progression differ in RTL.
    // Infer flow from the shaper's logical endpoint placements, not source text
    // heuristics or a second bidi implementation.
    let forward = right != line_is_rtl(physical.line);
    let mut next = if forward {
        index.checked_add(1)
    } else {
        index.checked_sub(1)
    };
    while let Some(target_index) = next.filter(|next| *next < lines.len()) {
        let target = lines[target_index];
        let boundary = if forward {
            target.line.text_range.start_utf16
        } else {
            target.line.text_range.end_utf16
        };
        let affinity = if forward {
            CaretAffinity::Leading
        } else {
            CaretAffinity::Trailing
        };
        let candidate = target
            .line
            .carets
            .iter()
            .filter(|caret| caret.utf16 == boundary)
            .min_by_key(|caret| caret.affinity != affinity)?;
        if target.line.paragraph_id != physical.line.paragraph_id
            || candidate.utf16 != current.utf16
        {
            return Some(geometry(target, candidate));
        }
        let mut target_boxes = spans(target.line);
        target_boxes.sort_by_key(|span| (span.left(), span.right(), span.start.utf16));
        let candidate = if right {
            target_boxes.first()
        } else {
            target_boxes.last()
        }
        .map(|span| span.edge(right));
        if let Some(candidate) = candidate
            && candidate.utf16 != current.utf16
        {
            return Some(downstream(lines, target_index, candidate));
        }
        next = if forward {
            target_index.checked_add(1)
        } else {
            target_index.checked_sub(1)
        };
    }
    None
}

fn line_is_rtl(line: &LineLayout) -> bool {
    let start = line
        .carets
        .iter()
        .filter(|caret| caret.utf16 == line.text_range.start_utf16)
        .min_by_key(|caret| caret.affinity != CaretAffinity::Leading);
    let end = line
        .carets
        .iter()
        .filter(|caret| caret.utf16 == line.text_range.end_utf16)
        .min_by_key(|caret| caret.affinity != CaretAffinity::Trailing);
    match (start, end) {
        (Some(start), Some(end)) if start.x != end.x => start.x > end.x,
        _ => spans(line)
            .first()
            .is_some_and(|span| span.start.x > span.end.x),
    }
}

fn downstream(lines: &[PhysicalLine<'_>], index: usize, caret: &Caret) -> CaretGeometry {
    let current = lines[index];
    if let Some(next) = lines.get(index + 1)
        && next.line.paragraph_id == current.line.paragraph_id
        && let Some(leading) = next.line.carets.iter().find(|candidate| {
            candidate.utf16 == caret.utf16 && candidate.affinity == CaretAffinity::Leading
        })
    {
        geometry(*next, leading)
    } else {
        geometry(current, caret)
    }
}

fn same_position(first: &LayoutPosition, second: &LayoutPosition) -> bool {
    first.paragraph_id == second.paragraph_id && first.utf16 == second.utf16
}

fn distance(first: i64, second: i64) -> i128 {
    (i128::from(first) - i128::from(second)).abs()
}

fn distance_to_interval(value: i64, start: i64, end: i128) -> i128 {
    let value = i128::from(value);
    (i128::from(start) - value).max(value - end).max(0)
}

fn coordinate(value: i64) -> Result<(), LayoutError> {
    if value.unsigned_abs() > MAX_COORDINATE as u64 {
        Err(invalid("coordinate exceeds the fixed interaction budget"))
    } else {
        Ok(())
    }
}

fn invalid(message: &str) -> LayoutError {
    LayoutError::InvalidPosition {
        message: message.into(),
    }
}
