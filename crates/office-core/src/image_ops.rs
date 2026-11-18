//! Atomic edits to in-flow image blocks, sharing immutable asset caches.
use crate::{
    Block, Document, EditError, Id, Image, ImageAlignment, Paragraph, Section, Transaction, Twips,
};
fn invalid(reason: &'static str) -> EditError {
    EditError::InvalidImageOperation { reason }
}
fn body_block_id(block: &Block) -> &Id {
    match block {
        Block::Paragraph(paragraph) => paragraph.id(),
        Block::Table(table) => table.id(),
        Block::Image(image) => image.id(),
    }
}
fn section_with_blocks(section: &Section, blocks: Vec<Block>) -> Section {
    let current = Section::new(section.id().clone(), blocks);
    match section.properties() {
        Some(p) => current.with_properties(p.clone()),
        None => current,
    }
}
impl Transaction {
    /// Inserts an image after a body paragraph, preserving all existing blocks.
    pub fn insert_image_after(
        &mut self,
        paragraph_id: &Id,
        image: Image,
    ) -> Result<bool, EditError> {
        let before = self.candidate()?;
        let mut sections = before.sections().to_vec();
        for section in &mut sections {
            if let Some(index) = section
                .blocks()
                .iter()
                .position(|b| matches!(b,Block::Paragraph(p) if p.id()==paragraph_id))
            {
                let mut blocks = section.blocks().to_vec();
                blocks.insert(index + 1, Block::Image(image));
                *section = section_with_blocks(section, blocks);
                return self.accept_image_sections(before, sections);
            }
        }
        Err(invalid("image insertion requires a body paragraph"))
    }
    /// Inserts a body paragraph after an image, including an image-only document.
    pub fn insert_paragraph_after_image(
        &mut self,
        image_id: &Id,
        paragraph: Paragraph,
    ) -> Result<bool, EditError> {
        let before = self.candidate()?;
        let mut sections = before.sections().to_vec();
        for section in &mut sections {
            if let Some(index) = section
                .blocks()
                .iter()
                .position(|b| matches!(b,Block::Image(image) if image.id()==image_id))
            {
                let mut blocks = section.blocks().to_vec();
                blocks.insert(index + 1, Block::Paragraph(paragraph));
                *section = section_with_blocks(section, blocks);
                return self.accept_image_sections(before, sections);
            }
        }
        Err(invalid("image block was not found"))
    }
    /// Changes physical dimensions without replacing image bytes or identity.
    pub fn resize_image(
        &mut self,
        id: &Id,
        width: Twips,
        height: Twips,
    ) -> Result<bool, EditError> {
        self.update_image(id, |image| {
            Some(
                Image::new(image.id().clone(), image.asset().clone(), width, height)
                    .with_alt_text(image.alt_text())
                    .with_alignment(image.alignment()),
            )
        })
    }
    /// Changes UTF-8 alternative text as one undoable operation.
    pub fn set_image_alt_text(&mut self, id: &Id, text: String) -> Result<bool, EditError> {
        self.update_image(id, |image| Some(image.with_alt_text(text)))
    }
    /// Changes horizontal body placement as one undoable operation.
    pub fn set_image_alignment(
        &mut self,
        id: &Id,
        alignment: ImageAlignment,
    ) -> Result<bool, EditError> {
        self.update_image(id, |image| Some(image.with_alignment(alignment)))
    }
    /// Reorders a body image within its original section and changes horizontal
    /// alignment in one undoable transaction. A None target means section end.
    /// This is in-flow placement, not absolute positioning or text wrapping.
    pub fn move_image_before(
        &mut self,
        image_id: &Id,
        before_block_id: Option<&Id>,
        alignment: ImageAlignment,
    ) -> Result<bool, EditError> {
        let before = self.candidate()?;
        let mut sections = before.sections().to_vec();
        for section in &mut sections {
            let Some(index) = section
                .blocks()
                .iter()
                .position(|block| matches!(block, Block::Image(image) if image.id() == image_id))
            else {
                continue;
            };
            if let Some(target) = before_block_id
                && !section
                    .blocks()
                    .iter()
                    .any(|block| body_block_id(block) == target)
            {
                return Err(invalid(
                    "image position target must be a block in the same section",
                ));
            }
            let mut blocks = section.blocks().to_vec();
            let Block::Image(image) = blocks.remove(index) else {
                unreachable!()
            };
            let target_index = match before_block_id {
                Some(target) if target == image_id => index,
                Some(target) => blocks
                    .iter()
                    .position(|block| body_block_id(block) == target)
                    .ok_or_else(|| invalid("image position target is no longer available"))?,
                None => blocks.len(),
            };
            blocks.insert(target_index, Block::Image(image.with_alignment(alignment)));
            *section = section_with_blocks(section, blocks);
            return self.accept_image_sections(before, sections);
        }
        Err(invalid("image block was not found"))
    }

    /// Deletes an image block without deleting neighboring paragraphs.
    pub fn remove_image(&mut self, id: &Id) -> Result<bool, EditError> {
        self.update_image(id, |_| None)
    }
    fn update_image(
        &mut self,
        id: &Id,
        update: impl FnOnce(Image) -> Option<Image>,
    ) -> Result<bool, EditError> {
        let before = self.candidate()?;
        let mut sections = before.sections().to_vec();
        for section in &mut sections {
            if let Some(index) = section
                .blocks()
                .iter()
                .position(|b| matches!(b,Block::Image(image) if image.id()==id))
            {
                let mut blocks = section.blocks().to_vec();
                let Block::Image(image) = blocks.remove(index) else {
                    unreachable!()
                };
                if let Some(image) = update(image) {
                    blocks.insert(index, Block::Image(image));
                }
                *section = section_with_blocks(section, blocks);
                return self.accept_image_sections(before, sections);
            }
        }
        Err(invalid("image block was not found"))
    }
    fn accept_image_sections(
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
