use crate::{
    BrowserEngine, EngineError, transaction_document, view,
    wire::{BrowserSelection, Direction, FormatSummary, ParagraphView, Point},
};
use office_core::{
    Alignment, Block, CharacterFormat, CharacterFormatPatch, GraphemeOffset, Id, ListKind, Paragraph,
    ParagraphFormatPatch, PropertyChange, Section, TextCoordinates, Transaction, Utf16Offset,
    valid_hyperlink,
};

fn paragraph<'a>(
    document: &'a office_core::Document,
    id: &str,
) -> Result<(&'a Paragraph, usize, usize), EngineError> {
    let mut index = 0;
    for (section_index, section) in document.sections().iter().enumerate() {
        for block in section.blocks() {
            let Block::Paragraph(paragraph) = block;
            if paragraph.id().as_str() == id {
                return Ok((paragraph, section_index, index));
            }
            index += 1;
        }
    }
    Err(EngineError::new(
        "invalid_selection",
        "The selected paragraph does not exist.",
    ))
}

fn snap_utf16(
    document: &office_core::Document,
    point: &Point,
    trailing: bool,
) -> Result<Point, EngineError> {
    let (paragraph, _, _) = paragraph(document, &point.paragraph_id)?;
    let coordinates = TextCoordinates::new(paragraph);
    // Reject offsets inside surrogate pairs; grapheme affinity is separate.
    let byte = coordinates
        .utf16_to_utf8(Utf16Offset::new(point.utf16))?
        .get();
    snap_byte(paragraph, byte, trailing)
}

fn snap_byte(paragraph: &Paragraph, byte: usize, trailing: bool) -> Result<Point, EngineError> {
    let coordinates = TextCoordinates::new(paragraph);
    let mut chosen = 0;
    for index in 0..=coordinates.grapheme_len().get() {
        let boundary = coordinates
            .grapheme_to_utf8(GraphemeOffset::new(index))?
            .get();
        if boundary == byte || (trailing && boundary > byte) {
            chosen = index;
            break;
        }
        if boundary < byte {
            chosen = index;
        }
    }
    Ok(Point {
        paragraph_id: paragraph.id().as_str().into(),
        utf16: coordinates
            .grapheme_to_utf16(GraphemeOffset::new(chosen))?
            .get(),
    })
}

fn ordered<'a>(
    document: &office_core::Document,
    selection: &'a BrowserSelection,
) -> Result<(&'a Point, &'a Point), EngineError> {
    let (_, _, anchor) = paragraph(document, &selection.anchor.paragraph_id)?;
    let (_, _, focus) = paragraph(document, &selection.focus.paragraph_id)?;
    if (anchor, selection.anchor.utf16) <= (focus, selection.focus.utf16) {
        Ok((&selection.anchor, &selection.focus))
    } else {
        Ok((&selection.focus, &selection.anchor))
    }
}

fn edit_selection(
    transaction: &Transaction,
    selection: &BrowserSelection,
) -> Result<office_core::EditSelection, EngineError> {
    let endpoint = |point: &Point| -> Result<office_core::EditPosition, EngineError> {
        let id = Id::new(&point.paragraph_id)?;
        let coordinates = transaction.text_coordinates(&id)?;
        let offset = coordinates.utf16_to_grapheme(Utf16Offset::new(point.utf16))?;
        Ok(transaction.position(&id, offset)?)
    };
    Ok(transaction.selection(endpoint(&selection.anchor)?, endpoint(&selection.focus)?)?)
}

fn caret(point: Point) -> BrowserSelection {
    BrowserSelection {
        anchor: point.clone(),
        focus: point,
    }
}

impl BrowserEngine {
    /// Reads selected plain text without changing the document, cursor, or history.
    ///
    /// Logical paragraph boundaries contribute exactly one LF, including across
    /// sections. Literal text, including any CR/LF inside runs, is preserved
    /// exactly. A collapsed or absent selection returns an empty string.
    pub fn selected_text(&self) -> Result<String, EngineError> {
        let Some(selection) = self.current_selection(None)? else {
            return Ok(String::new());
        };
        if selection.anchor == selection.focus {
            return Ok(String::new());
        }
        let document = self.session.document();
        let (start, end) = ordered(document, &selection)?;
        let (_, _, first) = paragraph(document, &start.paragraph_id)?;
        let (_, _, last) = paragraph(document, &end.paragraph_id)?;
        let mut result = String::new();
        for (index, block) in document
            .sections()
            .iter()
            .flat_map(|section| section.blocks())
            .enumerate()
            .take(last + 1)
            .skip(first)
        {
            let Block::Paragraph(paragraph) = block;
            let coordinates = TextCoordinates::new(paragraph);
            let from = if index == first {
                coordinates
                    .utf16_to_utf8(Utf16Offset::new(start.utf16))?
                    .get()
            } else {
                0
            };
            let to = if index == last {
                coordinates
                    .utf16_to_utf8(Utf16Offset::new(end.utf16))?
                    .get()
            } else {
                coordinates.text().len()
            };
            if index != first {
                result.push('\n');
            }
            result.push_str(&coordinates.text()[from..to]);
        }
        Ok(result)
    }

    pub(super) fn normalize_selection(
        &self,
        selection: &BrowserSelection,
    ) -> Result<BrowserSelection, EngineError> {
        self.normalize_selection_with_affinity(selection, None)
    }

    pub(super) fn normalize_selection_with_affinity(
        &self,
        selection: &BrowserSelection,
        trailing: Option<bool>,
    ) -> Result<BrowserSelection, EngineError> {
        let document = self.session.document();
        if selection.anchor == selection.focus {
            return Ok(caret(snap_utf16(
                document,
                &selection.anchor,
                trailing.unwrap_or(true),
            )?));
        }
        let (start, _) = ordered(document, selection)?;
        let anchor_first = std::ptr::eq(start, &selection.anchor);
        Ok(BrowserSelection {
            anchor: snap_utf16(document, &selection.anchor, !anchor_first)?,
            // Explicit native navigation affinity follows the moving focus,
            // including shrinking a reverse selection through a split EGC.
            focus: snap_utf16(document, &selection.focus, trailing.unwrap_or(anchor_first))?,
        })
    }

    fn current_selection(
        &self,
        supplied: Option<BrowserSelection>,
    ) -> Result<Option<BrowserSelection>, EngineError> {
        supplied
            .or_else(|| self.selection.clone())
            .map(|selection| self.normalize_selection(&selection))
            .transpose()
    }

    fn materialize(
        &mut self,
        transaction: &mut Transaction,
    ) -> Result<BrowserSelection, EngineError> {
        if transaction
            .draft()
            .sections
            .iter()
            .any(|section| !section.blocks().is_empty())
        {
            return Err(EngineError::new(
                "missing_selection",
                "Choose an editing position before changing this document.",
            ));
        }
        let id = self.allocate_id()?;
        let block = Block::Paragraph(Paragraph::new(id.clone(), vec![]));
        if let Some(section) = transaction.draft().sections.first() {
            let mut replacement = Section::new(section.id().clone(), vec![block]);
            if let Some(properties) = section.properties() {
                replacement = replacement.with_properties(properties.clone());
            }
            transaction.draft_mut().sections[0] = replacement;
        } else {
            let section_id = self.allocate_id()?;
            transaction
                .draft_mut()
                .sections
                .push(Section::new(section_id, vec![block]));
        }
        Ok(caret(Point {
            paragraph_id: id.as_str().into(),
            utf16: 0,
        }))
    }

    fn typing_format(&self, selection: &BrowserSelection) -> Result<CharacterFormat, EngineError> {
        if self.selection.as_ref() == Some(selection)
            && let Some(typing) = &self.typing
        {
            return Ok(typing.clone());
        }
        let (start, _) = ordered(self.session.document(), selection)?;
        let (paragraph, _, _) = paragraph(self.session.document(), &start.paragraph_id)?;
        let coordinates = TextCoordinates::new(paragraph);
        let byte = coordinates
            .utf16_to_utf8(Utf16Offset::new(start.utf16))?
            .get();
        let mut offset = 0;
        for run in paragraph.runs() {
            let end = offset + run.text.len();
            if byte <= end && (byte > offset || offset == 0) {
                return Ok(run.format.clone());
            }
            offset = end;
        }
        Ok(CharacterFormat::default())
    }

    pub(super) fn replace_text(
        &mut self,
        text: &str,
        supplied: Option<BrowserSelection>,
    ) -> Result<(), EngineError> {
        self.replace_text_with_break_policy(text, supplied, true)
    }

    /// Replace bounded, non-overlapping literal matches in one core transaction.
    /// Search and UTF-16/grapheme selection remain Rust-owned. Reject unbounded
    /// results rather than silently editing only the first 1,000 occurrences.
    pub(super) fn replace_all(
        &mut self,
        query: &str,
        replacement: &str,
    ) -> Result<(), EngineError> {
        if query.is_empty() {
            return Err(EngineError::new("invalid_replace", "Enter text to find first."));
        }
        // The first release replaces within paragraphs only, so this command
        // never inserts paragraph breaks or changes document structure.
        if replacement.len() > 1024 || replacement.contains('\r') || replacement.contains('\n') {
            return Err(EngineError::new(
                "invalid_replace",
                "Replacement text must be a single line of at most 1,024 UTF-8 bytes.",
            ));
        }
        let snapshot = self.find_snapshot(query)?;
        if snapshot.truncated {
            return Err(EngineError::new(
                "too_many_matches",
                "More than 1,000 matches were found. Narrow the search before replacing all.",
            ));
        }
        // Search returns expanded full-grapheme selections in document order.
        // Overlapping matches cannot both be replaced: choose the earliest
        // non-overlapping occurrence, matching conventional replace-all behavior.
        let mut targets: Vec<BrowserSelection> = Vec::new();
        for item in snapshot.matches {
            let selection = item.selection;
            if targets.last().is_some_and(|previous|
                previous.focus.paragraph_id == selection.anchor.paragraph_id
                && selection.anchor.utf16 < previous.focus.utf16
            ) {
                continue;
            }
            targets.push(selection);
        }
        if targets.is_empty() {
            return Ok(());
        }
        let mut prepared = Vec::with_capacity(targets.len());
        for selection in &targets {
            prepared.push((selection.clone(), self.typing_format(selection)?));
        }
        let after = targets.first().map(|selection| caret(selection.anchor.clone()));
        let mut transaction = self.session.begin_transaction();
        // Right-to-left keeps all earlier Rust UTF-16/grapheme offsets stable.
        for (selection, format) in prepared.into_iter().rev() {
            let range = edit_selection(&transaction, &selection)?;
            transaction.replace_text(&range, replacement, format)?;
        }
        self.commit_from(transaction, self.selection.clone(), after)?;
        self.typing = None;
        Ok(())
    }

    pub(super) fn insert_line_break(
        &mut self,
        supplied: Option<BrowserSelection>,
    ) -> Result<(), EngineError> {
        self.replace_text_with_break_policy("\n", supplied, false)
    }

    fn replace_text_with_break_policy(
        &mut self,
        text: &str,
        supplied: Option<BrowserSelection>,
        paragraph_breaks: bool,
    ) -> Result<(), EngineError> {
        if text.len() > crate::MAX_DOCUMENT_BYTES {
            return Err(crate::limit_error());
        }
        let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
        let segments: Vec<&str> = if paragraph_breaks {
            normalized.split('\n').collect()
        } else {
            vec![text]
        };
        if segments.len() > crate::MAX_PARAGRAPHS
            || segments
                .iter()
                .map(|segment| segment.encode_utf16().count())
                .sum::<usize>()
                > crate::MAX_UTF16
        {
            return Err(crate::limit_error());
        }
        let selected = self.current_selection(supplied)?;
        if selected.is_none() && text.is_empty() {
            return Ok(());
        }
        self.preflight_text(selected.as_ref(), &segments)?;
        let format = selected
            .as_ref()
            .map(|selection| self.typing_format(selection))
            .transpose()?
            .unwrap_or_else(|| self.typing.clone().unwrap_or_default());
        let mut transaction = self.session.begin_transaction();
        let selection = match selected {
            Some(selection) => selection,
            None => self.materialize(&mut transaction)?,
        };
        let cursor_before = Some(selection.clone());
        let typing_after = format.clone();
        let before = transaction_document(&transaction)?;
        let (start, _) = ordered(&before, &selection)?;
        let start_id = start.paragraph_id.clone();
        let start_byte = before
            .text_coordinates(&Id::new(&start_id)?)?
            .utf16_to_utf8(Utf16Offset::new(start.utf16))?
            .get();
        // Clipboard/paragraph newline policy belongs to this browser adapter.
        // The explicit soft-break command uses one literal LF through the same
        // validated text replacement instead of creating another paragraph.
        let after_point = if segments.len() == 1 {
            let range = edit_selection(&transaction, &selection)?;
            transaction.replace_text(&range, text, format)?;
            let candidate = transaction_document(&transaction)?;
            let (paragraph, _, _) = paragraph(&candidate, &start_id)?;
            // Insertion/deletion can join adjacent EGCs. Trailing affinity is
            // deliberate and never weakens the strict core offset conversion.
            snap_byte(paragraph, start_byte + text.len(), true)?
        } else {
            // Split before inserting text: inserting first could merge a suffix
            // combining mark and make the intended structural cut invalid.
            let suffix_id = self.allocate_id()?;
            let range = edit_selection(&transaction, &selection)?;
            transaction.insert_paragraph_break(&range, suffix_id.clone())?;
            let coordinates = transaction.text_coordinates(&Id::new(&start_id)?)?;
            let at = transaction.position(&Id::new(&start_id)?, coordinates.grapheme_len())?;
            transaction.insert_text(&at, segments[0], format.clone())?;
            let mut suffix_id = suffix_id;
            for segment in &segments[1..segments.len() - 1] {
                let next = self.allocate_id()?;
                let at = transaction.position(&suffix_id, GraphemeOffset::new(0))?;
                let range = transaction.selection(at.clone(), at)?;
                transaction.insert_paragraph_break(&range, next.clone())?;
                let at = transaction.position(&suffix_id, GraphemeOffset::new(0))?;
                transaction.insert_text(&at, segment, format.clone())?;
                suffix_id = next;
            }
            let last = segments[segments.len() - 1];
            let at = transaction.position(&suffix_id, GraphemeOffset::new(0))?;
            transaction.insert_text(&at, last, format)?;
            let candidate = transaction_document(&transaction)?;
            let (paragraph, _, _) = paragraph(&candidate, suffix_id.as_str())?;
            snap_byte(paragraph, last.len(), true)?
        };
        self.commit_from(transaction, cursor_before, Some(caret(after_point)))?;
        self.typing = Some(typing_after);
        Ok(())
    }

    // Reject oversized paste before staging repeated core operations. Final
    // validated-byte checks still run before publication for every command.
    fn preflight_text(
        &self,
        selection: Option<&BrowserSelection>,
        segments: &[&str],
    ) -> Result<(), EngineError> {
        let document = self.session.document();
        let all: Vec<&Paragraph> = document
            .sections()
            .iter()
            .flat_map(|section| section.blocks())
            .map(|block| {
                let Block::Paragraph(paragraph) = block;
                paragraph
            })
            .collect();
        if selection.is_none() && !all.is_empty() {
            return Err(EngineError::new(
                "missing_selection",
                "Choose an editing position before changing this document.",
            ));
        }
        let mut removed_paragraphs = 0;
        let mut removed_units = 0;
        if let Some(selection) = selection {
            let (start, end) = ordered(document, selection)?;
            let (_, _, first) = paragraph(document, &start.paragraph_id)?;
            let (_, _, last) = paragraph(document, &end.paragraph_id)?;
            removed_paragraphs = last - first;
            for (index, paragraph) in all.iter().enumerate().take(last + 1).skip(first) {
                let from = if index == first { start.utf16 } else { 0 };
                let to = if index == last {
                    end.utf16
                } else {
                    TextCoordinates::new(paragraph).utf16_len().get()
                };
                removed_units += to - from;
            }
        }
        let current_units: usize = all
            .iter()
            .flat_map(|paragraph| paragraph.runs())
            .map(|run| run.text.encode_utf16().count())
            .sum();
        let inserted_units: usize = segments
            .iter()
            .map(|segment| segment.encode_utf16().count())
            .sum();
        let paragraph_count = all.len().max(1) - removed_paragraphs + segments.len() - 1;
        if paragraph_count > crate::MAX_PARAGRAPHS
            || current_units - removed_units + inserted_units > crate::MAX_UTF16
        {
            return Err(crate::limit_error());
        }
        Ok(())
    }

    pub(super) fn delete(
        &mut self,
        direction: Direction,
        supplied: Option<BrowserSelection>,
    ) -> Result<(), EngineError> {
        let Some(mut selected) = self.current_selection(supplied)? else {
            return Ok(());
        };
        if selected.anchor == selected.focus {
            let document = self.session.document();
            let point = &selected.focus;
            let (current, section_index, current_index) = paragraph(document, &point.paragraph_id)?;
            let coordinates = TextCoordinates::new(current);
            let index = coordinates
                .utf16_to_grapheme(Utf16Offset::new(point.utf16))?
                .get();
            let max = coordinates.grapheme_len().get();
            let target = match direction {
                Direction::Backward if index > 0 => Point {
                    paragraph_id: point.paragraph_id.clone(),
                    utf16: coordinates
                        .grapheme_to_utf16(GraphemeOffset::new(index - 1))?
                        .get(),
                },
                Direction::Forward if index < max => Point {
                    paragraph_id: point.paragraph_id.clone(),
                    utf16: coordinates
                        .grapheme_to_utf16(GraphemeOffset::new(index + 1))?
                        .get(),
                },
                Direction::Backward | Direction::Forward => {
                    let all: Vec<_> = document
                        .sections()
                        .iter()
                        .enumerate()
                        .flat_map(|(index, section)| {
                            section.blocks().iter().map(move |block| (index, block))
                        })
                        .collect();
                    let neighbor = match direction {
                        Direction::Backward => {
                            current_index.checked_sub(1).and_then(|i| all.get(i))
                        }
                        Direction::Forward => all.get(current_index + 1),
                    };
                    let Some((neighbor_section, Block::Paragraph(neighbor))) = neighbor else {
                        return Ok(());
                    };
                    if *neighbor_section != section_index {
                        return Err(EngineError::new(
                            "cross_section_edit",
                            "Joining section boundaries is not supported.",
                        ));
                    }
                    Point {
                        paragraph_id: neighbor.id().as_str().into(),
                        utf16: match direction {
                            Direction::Backward => TextCoordinates::new(neighbor).utf16_len().get(),
                            Direction::Forward => 0,
                        },
                    }
                }
            };
            selected.anchor = target;
        }
        self.replace_text("", Some(selected))
    }

    pub(super) fn format_character(
        &mut self,
        format: CharacterFormat,
        supplied: Option<BrowserSelection>,
    ) -> Result<(), EngineError> {
        let selection = self.current_selection(supplied)?;
        if selection
            .as_ref()
            .is_none_or(|selection| selection.anchor == selection.focus)
        {
            let mut typing = selection
                .as_ref()
                .map(|selection| self.typing_format(selection))
                .transpose()?
                .unwrap_or_default();
            view::overlay_character(&mut typing, &format);
            self.typing = Some(typing);
            self.selection = selection;
            return Ok(());
        }
        let selection = selection.expect("noncollapsed selection exists");
        let change = |value: Option<bool>| value.map_or(PropertyChange::Keep, PropertyChange::Set);
        let patch = CharacterFormatPatch {
            bold: change(format.bold),
            italic: change(format.italic),
            underline: format
                .underline
                .map_or(PropertyChange::Keep, PropertyChange::Set),
            font_family: format
                .font_family
                .map_or(PropertyChange::Keep, PropertyChange::Set),
            font_size: format
                .font_size
                .map_or(PropertyChange::Keep, PropertyChange::Set),
            color: PropertyChange::Keep,
            hyperlink: PropertyChange::Keep,
        };
        let mut transaction = self.session.begin_transaction();
        let range = edit_selection(&transaction, &selection)?;
        transaction.apply_character_format(&range, &patch)?;
        self.commit_from(transaction, Some(selection.clone()), Some(selection))?;
        self.typing = None;
        Ok(())
    }

    /// Insert a new link or apply/edit/remove an existing link in one transaction.
    /// Link boundaries remain Rust grapheme positions, not browser DOM offsets.
    pub(super) fn set_hyperlink(
        &mut self,
        url: Option<String>,
        text: Option<String>,
        supplied: Option<BrowserSelection>,
    ) -> Result<(), EngineError> {
        if url.as_ref().is_some_and(|value| !valid_hyperlink(value)) {
            return Err(EngineError::new("invalid_link",
                "Use an HTTP or HTTPS address without spaces or credentials."));
        }
        let selected = self.current_selection(supplied)?;
        if let Some(label) = text {
            if url.is_none() || label.is_empty() || label.len() > 2048
                || label.contains('\n') || label.contains('\r')
            {
                return Err(EngineError::new("invalid_link",
                    "A new link needs a single-line label and a safe web address."));
            }
            self.preflight_text(selected.as_ref(), &[label.as_str()])?;
            let mut format = selected.as_ref()
                .map(|selection| self.typing_format(selection))
                .transpose()?.unwrap_or_default();
            format.hyperlink = url;
            let mut transaction = self.session.begin_transaction();
            let selection = match selected {
                Some(selection) => selection,
                None => self.materialize(&mut transaction)?,
            };
            let original = transaction_document(&transaction)?;
            let (start, _) = ordered(&original, &selection)?;
            let id = start.paragraph_id.clone();
            let at = original.text_coordinates(&Id::new(&id)?)?
                .utf16_to_utf8(Utf16Offset::new(start.utf16))?.get();
            let range = edit_selection(&transaction, &selection)?;
            transaction.replace_text(&range, &label, format)?;
            let candidate = transaction_document(&transaction)?;
            let (paragraph, _, _) = paragraph(&candidate, &id)?;
            let after = snap_byte(paragraph, at + label.len(), true)?;
            self.commit_from(transaction, Some(selection), Some(caret(after)))?;
            self.typing = None;
            return Ok(());
        }

        let Some(mut selection) = selected else {
            return Err(EngineError::new("missing_selection", "Select text to link."));
        };
        if selection.anchor == selection.focus {
            // Allow editing/removing a link with the caret inside an existing
            // linked run, even when no editable text is highlighted.
            let (paragraph, _, _) = paragraph(self.session.document(),
                &selection.anchor.paragraph_id)?;
            let coordinates = TextCoordinates::new(paragraph);
            let at = coordinates.utf16_to_utf8(
                Utf16Offset::new(selection.anchor.utf16))?.get();
            let mut offset = 0_usize;
            let mut runs = Vec::with_capacity(paragraph.runs().len());
            for run in paragraph.runs() {
                runs.push((offset, offset + run.text.len(), run.format.hyperlink.as_ref()));
                offset += run.text.len();
            }
            let found = runs.iter().position(|(start, end, existing)|
                *start <= at && at <= *end && start != end && existing.is_some());
            let Some(index) = found else {
                return Err(EngineError::new("missing_selection",
                    "Select text to add a hyperlink."));
            };
            let target = runs[index].2;
            let mut first = index;
            let mut last = index;
            while first > 0 && runs[first - 1].2 == target {
                first -= 1;
            }
            while last + 1 < runs.len() && runs[last + 1].2 == target {
                last += 1;
            }
            selection = BrowserSelection {
                anchor: snap_byte(paragraph, runs[first].0, false)?,
                focus: snap_byte(paragraph, runs[last].1, true)?,
            };
        }
        let mut transaction = self.session.begin_transaction();
        let range = edit_selection(&transaction, &selection)?;
        let patch = CharacterFormatPatch {
            hyperlink: url.map_or(PropertyChange::Clear, PropertyChange::Set),
            ..Default::default()
        };
        transaction.apply_character_format(&range, &patch)?;
        self.commit_from(transaction, Some(selection.clone()), Some(selection))?;
        self.typing = None;
        Ok(())
    }

    /// Toggle semantic list status on selected paragraphs, without editing text.
    pub(super) fn set_list(
        &mut self,
        list: Option<ListKind>,
        supplied: Option<BrowserSelection>,
    ) -> Result<(), EngineError> {
        let selected = self.current_selection(supplied)?;
        let mut transaction = self.session.begin_transaction();
        let selection = match selected {
            Some(selection) => selection,
            None => self.materialize(&mut transaction)?,
        };
        let range = edit_selection(&transaction, &selection)?;
        transaction.set_paragraph_list(&range, list)?;
        self.commit_from(transaction, Some(selection.clone()), Some(selection))?;
        self.typing = None;
        Ok(())
    }

    /// Enter inherits a list on a populated item, but an empty item exits it.
    pub(super) fn insert_list_paragraph(
        &mut self,
        supplied: Option<BrowserSelection>,
    ) -> Result<(), EngineError> {
        if let Some(selection) = self.current_selection(supplied.clone())?
            && selection.anchor == selection.focus
        {
            let (current, _, _) = paragraph(self.session.document(), &selection.anchor.paragraph_id)?;
            if current.list().is_some() && TextCoordinates::new(current).text().is_empty() {
                return self.set_list(None, Some(selection));
            }
        }
        self.replace_text("\n", supplied)
    }

    pub(super) fn format_paragraph(
        &mut self,
        alignment: Alignment,
        supplied: Option<BrowserSelection>,
    ) -> Result<(), EngineError> {
        let selected = self.current_selection(supplied)?;
        let mut transaction = self.session.begin_transaction();
        let selection = match selected {
            Some(selection) => selection,
            None => self.materialize(&mut transaction)?,
        };
        let range = edit_selection(&transaction, &selection)?;
        transaction.apply_paragraph_format(
            &range,
            &ParagraphFormatPatch {
                alignment: PropertyChange::Set(alignment),
                ..Default::default()
            },
        )?;
        self.commit_from(transaction, Some(selection.clone()), Some(selection))
    }

    pub(super) fn assign_style(
        &mut self,
        style_id: Option<String>,
        supplied: Option<BrowserSelection>,
    ) -> Result<(), EngineError> {
        let style_id = style_id.map(Id::new).transpose()?;
        let selected = self.current_selection(supplied)?;
        let mut transaction = self.session.begin_transaction();
        let selection = match selected {
            Some(selection) => selection,
            None => self.materialize(&mut transaction)?,
        };
        let range = edit_selection(&transaction, &selection)?;
        transaction.set_paragraph_style(&range, style_id.as_ref())?;
        self.commit_from(transaction, Some(selection.clone()), Some(selection))?;
        self.typing = None;
        Ok(())
    }

    pub(super) fn format_summary(&self, paragraphs: &[ParagraphView]) -> FormatSummary {
        let selected = self.selection.as_ref();
        let mut chosen = Vec::new();
        let mut empty_paragraphs = Vec::new();
        let mut alignments = Vec::new();
        let mut styles = Vec::new();
        let mut lists = Vec::new();
        if let Some(selection) = selected {
            let document = self.session.document();
            if let Ok((start, end)) = ordered(document, selection) {
                let first = paragraphs
                    .iter()
                    .position(|p| p.id == start.paragraph_id)
                    .unwrap_or(0);
                let last = paragraphs
                    .iter()
                    .position(|p| p.id == end.paragraph_id)
                    .unwrap_or(first);
                let collapsed = selection.anchor == selection.focus;
                for (index, display) in paragraphs.iter().enumerate().take(last + 1).skip(first) {
                    if !collapsed && index == last && end.utf16 == 0 && first != last {
                        continue;
                    }
                    alignments.push(display.format.alignment);
                    styles.push(display.style_id.clone());
                    lists.push(display.list);
                    let Ok((paragraph, _, _)) = paragraph(document, &display.id) else {
                        continue;
                    };
                    let (base, _) = view::inherited(document, paragraph);
                    if collapsed {
                        let mut effective = base;
                        if let Ok(direct) = self.typing_format(selection) {
                            view::overlay_character(&mut effective, &direct);
                        }
                        chosen.push(view::character_view(&effective));
                    } else {
                        let from = if index == first { start.utf16 } else { 0 };
                        let to = if index == last { end.utf16 } else { usize::MAX };
                        let mut offset = 0;
                        for run in &display.runs {
                            let next = offset + run.text.encode_utf16().count();
                            if next > offset && offset < to && next > from {
                                chosen.push(run.format.clone());
                            }
                            offset = next;
                        }
                        if display.runs.iter().all(|run| run.text.is_empty()) {
                            empty_paragraphs.push(view::character_view(&base));
                        }
                    }
                }
            }
        }
        // Character formatting applies to selected text. Blank paragraphs must
        // not make a uniformly formatted text range appear mixed; their own
        // defaults still describe a selection containing only blank paragraphs.
        if chosen.is_empty() {
            chosen = empty_paragraphs;
        }
        if chosen.is_empty() {
            chosen.push(view::character_view(
                self.typing.as_ref().unwrap_or(&CharacterFormat::default()),
            ));
        }
        fn common<T: PartialEq + Clone>(values: impl Iterator<Item = T>) -> Option<T> {
            let mut values = values;
            let first = values.next()?;
            values.all(|value| value == first).then_some(first)
        }
        let common_style = common(styles.iter().cloned());
        let style_mixed = !styles.is_empty() && common_style.is_none();
        let common_list = common(lists.iter().copied());
        let list_mixed = !lists.is_empty() && common_list.is_none();
        FormatSummary {
            bold: common(chosen.iter().map(|f| f.bold)),
            italic: common(chosen.iter().map(|f| f.italic)),
            underline: common(chosen.iter().map(|f| f.underline != "none")),
            font_family: common(chosen.iter().map(|f| f.font_family.clone())),
            font_size_pt: common(chosen.iter().map(|f| f.font_size_pt)),
            alignment: common(alignments.into_iter()).or(if paragraphs.is_empty() {
                Some(Alignment::Left)
            } else {
                None
            }),
            style_id: common_style.flatten(),
            style_mixed,
            list: common_list.flatten(),
            list_mixed,
        }
    }
}
