use crate::{
    Block, CharacterFormat, CharacterFormatPatch, Document, EditError, Id, Paragraph,
    ParagraphFormat, ParagraphFormatPatch, PositionError, Section, Selection, TextCoordinates,
    TextRun, ValidationError,
};

struct Bounds {
    start: (usize, usize),
    end: (usize, usize),
    start_byte: usize,
    end_byte: usize,
    collapsed: bool,
}

impl Bounds {
    fn for_selection(document: &Document, selection: &Selection) -> Result<Self, EditError> {
        let range = selection.ordered_range(document)?;
        let (start, start_paragraph) = locate(document, range.start.paragraph_id())?;
        let (end, end_paragraph) = locate(document, range.end.paragraph_id())?;
        Ok(Self {
            start,
            end,
            start_byte: TextCoordinates::new(start_paragraph)
                .grapheme_to_utf8(range.start.offset())?
                .get(),
            end_byte: TextCoordinates::new(end_paragraph)
                .grapheme_to_utf8(range.end.offset())?
                .get(),
            collapsed: range.start == range.end,
        })
    }

    fn touches_paragraph(&self, index: (usize, usize)) -> bool {
        index >= self.start
            && index <= self.end
            && (self.collapsed || index != self.end || self.start == self.end || self.end_byte != 0)
    }
}

pub(crate) fn replace_text(
    document: &Document,
    selection: &Selection,
    text: &str,
    format: &CharacterFormat,
) -> Result<Document, EditError> {
    format.validate("insert_format")?;
    let bounds = Bounds::for_selection(document, selection)?;
    if bounds.start.0 != bounds.end.0 {
        return Err(EditError::CrossSectionRange);
    }
    if bounds.collapsed && text.is_empty() {
        return Ok(document.clone());
    }
    let section = &document.sections()[bounds.start.0];
    let Block::Paragraph(start) = &section.blocks()[bounds.start.1];
    let Block::Paragraph(end) = &section.blocks()[bounds.end.1];

    // Retyping identical supported content with identical direct properties is
    // a true no-op: retain the original formatting-run partition and empty runs.
    if bounds.start == bounds.end
        && replacement_is_identical(start, bounds.start_byte, bounds.end_byte, text, format)
    {
        return Ok(document.clone());
    }

    let mut runs = prefix_runs(start, bounds.start_byte);
    if !text.is_empty() {
        runs.push(TextRun {
            text: text.into(),
            format: format.clone(),
        });
    }
    runs.extend(suffix_runs(end, bounds.end_byte));
    let replacement = copy_paragraph(start, runs, start.format().clone(), start.style());
    let mut blocks = section.blocks().to_vec();
    blocks.splice(
        bounds.start.1..=bounds.end.1,
        [Block::Paragraph(replacement)],
    );
    let mut sections = document.sections().to_vec();
    sections[bounds.start.0] = copy_section(section, blocks);
    rebuild_document(document, sections)
}

pub(crate) fn insert_paragraph_break(
    document: &Document,
    selection: &Selection,
    new_id: &Id,
) -> Result<Document, EditError> {
    let bounds = Bounds::for_selection(document, selection)?;
    if bounds.start.0 != bounds.end.0 {
        return Err(EditError::CrossSectionRange);
    }

    // Check the original document, including paragraphs removed by the range.
    // Reusing their IDs would make a newly created paragraph impersonate an old
    // element even though the rebuilt document's IDs would remain unique.
    if document.id() == new_id
        || document.styles().iter().any(|style| &style.id == new_id)
        || document.sections().iter().any(|section| {
            section.id() == new_id
                || section.blocks().iter().any(|block| {
                    let Block::Paragraph(paragraph) = block;
                    paragraph.id() == new_id
                })
        })
    {
        return Err(ValidationError::DuplicateId {
            id: new_id.as_str().into(),
        }
        .into());
    }

    let section = &document.sections()[bounds.start.0];
    let Block::Paragraph(start) = &section.blocks()[bounds.start.1];
    let Block::Paragraph(end) = &section.blocks()[bounds.end.1];
    let prefix = copy_paragraph(
        start,
        prefix_runs(start, bounds.start_byte),
        start.format().clone(),
        start.style(),
    );
    let mut suffix = Paragraph::new(new_id.clone(), suffix_runs(end, bounds.end_byte))
        .with_format(start.format().clone());
    if let Some(style) = start.style() {
        suffix = suffix.with_style(style.clone());
    }

    // Cut the original paragraphs independently. Joining first could merge
    // neighboring graphemes and change the meaning of the requested boundary.
    let mut blocks = section.blocks().to_vec();
    blocks.splice(
        bounds.start.1..=bounds.end.1,
        [Block::Paragraph(prefix), Block::Paragraph(suffix)],
    );
    let mut sections = document.sections().to_vec();
    sections[bounds.start.0] = copy_section(section, blocks);
    rebuild_document(document, sections)
}

pub(crate) fn apply_character_format(
    document: &Document,
    selection: &Selection,
    patch: &CharacterFormatPatch,
) -> Result<Document, EditError> {
    patch.validate()?;
    let bounds = Bounds::for_selection(document, selection)?;
    if bounds.collapsed {
        return Ok(document.clone());
    }
    transform_paragraphs(document, |index, paragraph| {
        if index < bounds.start || index > bounds.end {
            return paragraph.clone();
        }
        let start = if index == bounds.start {
            bounds.start_byte
        } else {
            0
        };
        let end = if index == bounds.end {
            bounds.end_byte
        } else {
            paragraph.runs().iter().map(|run| run.text.len()).sum()
        };
        let runs = format_runs(paragraph, start, end, patch);
        copy_paragraph(
            paragraph,
            runs,
            paragraph.format().clone(),
            paragraph.style(),
        )
    })
}

pub(crate) fn apply_paragraph_format(
    document: &Document,
    selection: &Selection,
    patch: &ParagraphFormatPatch,
) -> Result<Document, EditError> {
    patch.validate()?;
    let bounds = Bounds::for_selection(document, selection)?;
    transform_paragraphs(document, |index, paragraph| {
        if bounds.touches_paragraph(index) {
            copy_paragraph(
                paragraph,
                paragraph.runs().to_vec(),
                patch.apply(paragraph.format()),
                paragraph.style(),
            )
        } else {
            paragraph.clone()
        }
    })
}

pub(crate) fn set_paragraph_style(
    document: &Document,
    selection: &Selection,
    style: Option<&Id>,
) -> Result<Document, EditError> {
    let bounds = Bounds::for_selection(document, selection)?;
    transform_paragraphs(document, |index, paragraph| {
        if bounds.touches_paragraph(index) {
            copy_paragraph(
                paragraph,
                paragraph.runs().to_vec(),
                paragraph.format().clone(),
                style,
            )
        } else {
            paragraph.clone()
        }
    })
}

fn locate<'a>(
    document: &'a Document,
    id: &Id,
) -> Result<((usize, usize), &'a Paragraph), PositionError> {
    for (section_index, section) in document.sections().iter().enumerate() {
        for (block_index, block) in section.blocks().iter().enumerate() {
            let Block::Paragraph(paragraph) = block;
            if paragraph.id() == id {
                return Ok(((section_index, block_index), paragraph));
            }
        }
    }
    Err(PositionError::ParagraphNotFound { id: id.clone() })
}

fn copy_paragraph(
    original: &Paragraph,
    runs: Vec<TextRun>,
    format: ParagraphFormat,
    style: Option<&Id>,
) -> Paragraph {
    let paragraph = Paragraph::new(original.id().clone(), runs).with_format(format);
    match style {
        Some(style) => paragraph.with_style(style.clone()),
        None => paragraph,
    }
}

fn copy_section(original: &Section, blocks: Vec<Block>) -> Section {
    let section = Section::new(original.id().clone(), blocks);
    match original.properties() {
        Some(properties) => section.with_properties(properties.clone()),
        None => section,
    }
}

fn rebuild_document(document: &Document, sections: Vec<Section>) -> Result<Document, EditError> {
    Ok(Document::from_parts(
        document.id().clone(),
        document.metadata().clone(),
        sections,
        document.styles().to_vec(),
    )?)
}

fn transform_paragraphs(
    document: &Document,
    mut transform: impl FnMut((usize, usize), &Paragraph) -> Paragraph,
) -> Result<Document, EditError> {
    let sections = document
        .sections()
        .iter()
        .enumerate()
        .map(|(section_index, section)| {
            let blocks = section
                .blocks()
                .iter()
                .enumerate()
                .map(|(block_index, block)| {
                    let Block::Paragraph(paragraph) = block;
                    Block::Paragraph(transform((section_index, block_index), paragraph))
                })
                .collect();
            copy_section(section, blocks)
        })
        .collect();
    rebuild_document(document, sections)
}

fn replacement_is_identical(
    paragraph: &Paragraph,
    start: usize,
    end: usize,
    text: &str,
    format: &CharacterFormat,
) -> bool {
    let coordinates = TextCoordinates::new(paragraph);
    if &coordinates.text()[start..end] != text {
        return false;
    }
    let mut offset = 0;
    paragraph.runs().iter().all(|run| {
        let run_start = offset;
        offset += run.text.len();
        offset <= start || run_start >= end || run.text.is_empty() || run.format == *format
    })
}

// Empty runs at a cut belong to its suffix. Prefix/suffix therefore partition
// them without duplication for insertion. Replacement removes empty runs at
// start <= location < end, retaining all empty runs outside that interval.
fn prefix_runs(paragraph: &Paragraph, cut: usize) -> Vec<TextRun> {
    let mut offset = 0;
    let mut result = Vec::new();
    for run in paragraph.runs() {
        let start = offset;
        offset += run.text.len();
        if start >= cut {
            continue;
        }
        let retained = (cut - start).min(run.text.len());
        result.push(TextRun {
            text: run.text[..retained].into(),
            format: run.format.clone(),
        });
    }
    result
}

fn suffix_runs(paragraph: &Paragraph, cut: usize) -> Vec<TextRun> {
    let mut offset = 0;
    let mut result = Vec::new();
    for run in paragraph.runs() {
        let start = offset;
        offset += run.text.len();
        if run.text.is_empty() {
            if start >= cut {
                result.push(run.clone());
            }
        } else if offset > cut {
            let local = cut.saturating_sub(start);
            result.push(TextRun {
                text: run.text[local..].into(),
                format: run.format.clone(),
            });
        }
    }
    result
}

fn format_runs(
    paragraph: &Paragraph,
    start: usize,
    end: usize,
    patch: &CharacterFormatPatch,
) -> Vec<TextRun> {
    let mut offset = 0;
    let mut result = Vec::new();
    for run in paragraph.runs() {
        let run_start = offset;
        offset += run.text.len();
        let selected_start = start.max(run_start);
        let selected_end = end.min(offset);
        let format = patch.apply(&run.format);
        if selected_start >= selected_end || format == run.format {
            result.push(run.clone());
            continue;
        }
        let local_start = selected_start - run_start;
        let local_end = selected_end - run_start;
        if local_start != 0 {
            result.push(TextRun {
                text: run.text[..local_start].into(),
                format: run.format.clone(),
            });
        }
        result.push(TextRun {
            text: run.text[local_start..local_end].into(),
            format,
        });
        if local_end != run.text.len() {
            result.push(TextRun {
                text: run.text[local_end..].into(),
                format: run.format.clone(),
            });
        }
    }
    result
}
