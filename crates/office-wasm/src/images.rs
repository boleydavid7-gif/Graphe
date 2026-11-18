//! Image selection and edits remain authoritative in the existing Rust session.

use office_core::{
    Block, Id, Image, ImageAlignment, ImageAsset, Paragraph, Section, TextCoordinates, Twips,
};
use office_layout::LayoutOptions;

use crate::{
    BrowserEngine, EngineError, transaction_document,
    wire::{BrowserSelection, Direction, ImageView, Point},
};

/// Keep encoded input bounded independently of the full native document limit.
const MAX_BROWSER_IMAGE_BYTES: usize = 2 * 1024 * 1024;

pub(super) struct ImageUpdate {
    pub width_pt: Option<f64>,
    pub height_pt: Option<f64>,
    pub lock_aspect: bool,
    pub alt_text: Option<String>,
    pub alignment: Option<ImageAlignment>,
}

fn image_error(message: impl Into<String>) -> EngineError {
    EngineError::new("invalid_image", message)
}

fn caret(id: &Id, utf16: usize) -> BrowserSelection {
    let point = Point {
        paragraph_id: id.as_str().into(),
        utf16,
    };
    BrowserSelection {
        anchor: point.clone(),
        focus: point,
    }
}

fn section_with_blocks(section: &Section, blocks: Vec<Block>) -> Section {
    let current = Section::new(section.id().clone(), blocks);
    match section.properties() {
        Some(properties) => current.with_properties(properties.clone()),
        None => current,
    }
}

fn body_bounds(section: &Section) -> Result<(i64, i64), EngineError> {
    let defaults = LayoutOptions::default();
    let properties = section.properties();
    let size = properties
        .and_then(|value| value.page_size)
        .unwrap_or(defaults.page_size);
    let margins = properties
        .and_then(|value| value.page_margins)
        .unwrap_or(defaults.page_margins);
    let width = i64::from(size.width.0)
        - i64::from(margins.left.0)
        - i64::from(margins.right.0)
        - i64::from(margins.gutter.0);
    let height = i64::from(size.height.0) - i64::from(margins.top.0) - i64::from(margins.bottom.0);
    if width <= 0 || height <= 0 {
        return Err(image_error(
            "Page margins must leave positive space for the image.",
        ));
    }
    Ok((width, height))
}

fn fitted_size(asset: &ImageAsset, bounds: (i64, i64)) -> Result<(Twips, Twips), EngineError> {
    // 96 display pixels per inch means 15 integer twips per pixel. Fit is a
    // documented insertion policy; later explicit resize values are not clamped.
    let mut width = i64::from(asset.width_px()) * 15;
    let mut height = i64::from(asset.height_px()) * 15;
    if width > bounds.0 {
        height = height * bounds.0 / width;
        width = bounds.0;
    }
    if height > bounds.1 {
        width = width * bounds.1 / height;
        height = bounds.1;
    }
    if width <= 0 || height <= 0 {
        return Err(image_error(
            "The image aspect ratio cannot fit this page in the supported units.",
        ));
    }
    Ok((
        Twips(
            i32::try_from(width)
                .map_err(|_| image_error("Image width exceeds supported units."))?,
        ),
        Twips(
            i32::try_from(height)
                .map_err(|_| image_error("Image height exceeds supported units."))?,
        ),
    ))
}

fn dimension(value: f64) -> Result<i64, EngineError> {
    if !value.is_finite() || value <= 0.0 || value * 20.0 > f64::from(i32::MAX) {
        return Err(EngineError::new(
            "invalid_image_size",
            "Image dimensions must be finite, positive point values.",
        ));
    }
    let twips = (value * 20.0).round() as i64;
    if twips <= 0 {
        return Err(EngineError::new(
            "invalid_image_size",
            "Image dimensions must be at least one twip.",
        ));
    }
    Ok(twips)
}

impl BrowserEngine {
    pub(super) fn image(&self, id: &Id) -> Option<&Image> {
        self.session
            .document()
            .sections()
            .iter()
            .flat_map(|section| section.blocks())
            .find_map(|block| match block {
                Block::Image(image) if image.id() == id => Some(image),
                _ => None,
            })
    }

    fn selected_image_value(&self) -> Result<Image, EngineError> {
        let id = self
            .selected_image
            .as_ref()
            .ok_or_else(|| EngineError::new("missing_image_selection", "Select an image first."))?;
        self.image(id).cloned().ok_or_else(|| {
            EngineError::new("image_not_found", "The selected image no longer exists.")
        })
    }

    pub(super) fn image_views(&self) -> Vec<ImageView> {
        self.session
            .document()
            .sections()
            .iter()
            .flat_map(|section| {
                section.blocks().iter().filter_map(|block| match block {
                    Block::Image(image) => Some(ImageView {
                        id: image.id().as_str().into(),
                        section_id: section.id().as_str().into(),
                        width_pt: f64::from(image.width().0) / 20.0,
                        height_pt: f64::from(image.height().0) / 20.0,
                        pixel_width: image.asset().width_px(),
                        pixel_height: image.asset().height_px(),
                        color_approximation: image.asset().has_color_approximation(),
                        orientation_applied: image.asset().has_orientation_transform(),
                        alt_text: image.alt_text().into(),
                        alignment: image.alignment(),
                    }),
                    _ => None,
                })
            })
            .collect()
    }

    fn image_revision(&self, expected_revision: Option<&str>) -> Result<(), EngineError> {
        if expected_revision.is_some_and(|value| value != self.revision()) {
            return Err(EngineError::new(
                "stale_revision",
                "The image operation belongs to an earlier document revision.",
            ));
        }
        Ok(())
    }

    /// Inserts a local PNG or JPEG after a collapsed body-text caret.
    ///
    /// Rust validates encoded bytes, calculates initial physical fit and commits
    /// the image plus an optional trailing paragraph as one atomic history step.
    /// A rejected file, stale revision or exhausted resource budget changes no
    /// content, selection, history, or retained original document bytes.
    pub fn insert_image(&mut self, bytes: &[u8], expected_revision: Option<&str>) -> String {
        let result = self.insert_raster_image(bytes, expected_revision);
        self.response(result)
    }

    fn insert_raster_image(
        &mut self,
        bytes: &[u8],
        expected_revision: Option<&str>,
    ) -> Result<(), EngineError> {
        self.image_revision(expected_revision)?;
        if bytes.len() > MAX_BROWSER_IMAGE_BYTES {
            return Err(EngineError::new(
                "image_too_large",
                "Choose a PNG or JPEG of at most 2 MiB. The document is unchanged.",
            ));
        }
        if self.selected_image.is_some() {
            return Err(image_error(
                "Place a text caret before inserting another image.",
            ));
        }
        let selected = self
            .current_selection(None)?
            .ok_or_else(|| image_error("Choose a body paragraph before inserting an image."))?;
        if selected.anchor != selected.focus {
            return Err(image_error(
                "Place a collapsed text caret before inserting an image.",
            ));
        }
        let paragraph_id = Id::new(&selected.focus.paragraph_id)?;
        let section = self.session.document().sections().iter().find(|section| section.blocks().iter()
            .any(|block| matches!(block, Block::Paragraph(paragraph) if paragraph.id() == &paragraph_id)))
            .ok_or_else(|| image_error("Images inside table cells are not supported yet. Choose a body paragraph."))?;
        let bounds = body_bounds(section)?;
        let asset =
            ImageAsset::from_bytes(bytes).map_err(|error| image_error(error.to_string()))?;
        let (width, height) = fitted_size(&asset, bounds)?;
        let image_id = self.allocate_id()?;
        let trailing_id = self.allocate_id()?;
        let image = Image::new(image_id.clone(), asset, width, height);
        let mut transaction = self.session.begin_transaction();
        transaction.insert_image_after(&paragraph_id, image)?;
        for section in &mut transaction.draft_mut().sections {
            if let Some(index) = section
                .blocks()
                .iter()
                .position(|block| matches!(block, Block::Image(image) if image.id() == &image_id))
            {
                if !matches!(section.blocks().get(index + 1), Some(Block::Paragraph(_))) {
                    let mut blocks = section.blocks().to_vec();
                    blocks.insert(
                        index + 1,
                        Block::Paragraph(Paragraph::new(trailing_id, vec![])),
                    );
                    *section = section_with_blocks(section, blocks);
                }
                break;
            }
        }
        self.commit_with_image(
            transaction,
            Some(selected.clone()),
            Some(selected),
            None,
            Some(image_id),
        )?;
        self.typing = None;
        Ok(())
    }

    /// Reads immutable canonical PNG bytes for presentation at a checked revision.
    ///
    /// This deliberately avoids embedding binary/base64 payloads in each typing
    /// snapshot. Callers may cache a presentation URL by session and stable image
    /// ID, releasing it on source replacement, image removal or session disposal.
    pub fn image_png(
        &self,
        image_id: &str,
        expected_revision: Option<&str>,
    ) -> Result<Vec<u8>, EngineError> {
        self.image_revision(expected_revision)?;
        let id = Id::new(image_id)?;
        self.image(&id)
            .map(|image| image.asset().render_png().to_vec())
            .ok_or_else(|| {
                EngineError::new("image_not_found", "The requested image does not exist.")
            })
    }

    pub(super) fn select_image(&mut self, image_id: Option<String>) -> Result<(), EngineError> {
        let next = image_id.map(Id::new).transpose()?;
        if next.as_ref().is_some_and(|id| self.image(id).is_none()) {
            return Err(EngineError::new(
                "image_not_found",
                "The requested image does not exist.",
            ));
        }
        self.selected_image = next;
        self.typing = None;
        self.layout_state.reset_visual();
        Ok(())
    }

    /// One atomic alignment and body-block ordering update.
    pub(super) fn move_selected_image(
        &mut self,
        before_block_id: Option<String>,
        alignment: ImageAlignment,
    ) -> Result<(), EngineError> {
        let image = self.selected_image_value()?;
        let before = before_block_id.map(Id::new).transpose()?;
        let mut transaction = self.session.begin_transaction();
        transaction.move_image_before(image.id(), before.as_ref(), alignment)?;
        self.commit(transaction, self.selection.clone())
    }

    pub(super) fn resize_selected_image(
        &mut self,
        width_pt: Option<f64>,
        height_pt: Option<f64>,
        lock_aspect: bool,
    ) -> Result<(), EngineError> {
        if width_pt.is_none() && height_pt.is_none() {
            return Err(EngineError::new(
                "invalid_image_size",
                "Specify an image width or height.",
            ));
        }
        self.update_selected_image(ImageUpdate {
            width_pt,
            height_pt,
            lock_aspect,
            alt_text: None,
            alignment: None,
        })
    }

    pub(super) fn update_selected_image(&mut self, update: ImageUpdate) -> Result<(), EngineError> {
        let image = self.selected_image_value()?;
        let ImageUpdate {
            width_pt,
            height_pt,
            lock_aspect,
            alt_text,
            alignment,
        } = update;
        let resize = width_pt.is_some() || height_pt.is_some();
        if width_pt.is_none() && height_pt.is_none() && alt_text.is_none() && alignment.is_none() {
            return Err(EngineError::new(
                "invalid_image_update",
                "Specify image properties to change.",
            ));
        }
        let old_width = i64::from(image.width().0);
        let old_height = i64::from(image.height().0);
        let mut width = width_pt.map(dimension).transpose()?;
        let mut height = height_pt.map(dimension).transpose()?;
        if lock_aspect {
            match (width, height) {
                (Some(w), None) => height = Some((old_height * w + old_width / 2) / old_width),
                (None, Some(h)) => width = Some((old_width * h + old_height / 2) / old_height),
                (Some(w), Some(h))
                    if (w * old_height - h * old_width).abs() > old_width.max(old_height) =>
                {
                    return Err(EngineError::new(
                        "invalid_image_size",
                        "Change one dimension while aspect ratio is locked.",
                    ));
                }
                _ => {}
            }
        }
        let width = width.unwrap_or(old_width);
        let height = height.unwrap_or(old_height);
        if resize {
            let section = self
                .session
                .document()
                .sections()
                .iter()
                .find(|section| {
                    section.blocks().iter()
                .any(|block| matches!(block, Block::Image(current) if current.id() == image.id()))
                })
                .expect("selected image belongs to section");
            let bounds = body_bounds(section)?;
            if width <= 0 || height <= 0 || width > bounds.0 || height > bounds.1 {
                return Err(EngineError::new(
                    "invalid_image_size",
                    "The image must fit within the page's usable width and height.",
                ));
            }
        }
        let width = Twips(
            i32::try_from(width)
                .map_err(|_| image_error("Image width exceeds supported units."))?,
        );
        let height = Twips(
            i32::try_from(height)
                .map_err(|_| image_error("Image height exceeds supported units."))?,
        );
        let mut transaction = self.session.begin_transaction();
        if resize {
            transaction.resize_image(image.id(), width, height)?;
        }
        if let Some(alt_text) = alt_text {
            transaction.set_image_alt_text(image.id(), alt_text)?;
        }
        if let Some(alignment) = alignment {
            transaction.set_image_alignment(image.id(), alignment)?;
        }
        self.commit(transaction, self.selection.clone())
    }

    pub(super) fn set_image_alt_text(&mut self, alt_text: String) -> Result<(), EngineError> {
        self.update_selected_image(ImageUpdate {
            width_pt: None,
            height_pt: None,
            lock_aspect: true,
            alt_text: Some(alt_text),
            alignment: None,
        })
    }

    pub(super) fn set_image_alignment(
        &mut self,
        alignment: ImageAlignment,
    ) -> Result<(), EngineError> {
        self.update_selected_image(ImageUpdate {
            width_pt: None,
            height_pt: None,
            lock_aspect: true,
            alt_text: None,
            alignment: Some(alignment),
        })
    }

    pub(super) fn delete_image(&mut self) -> Result<(), EngineError> {
        let image = self.selected_image_value()?;
        let mut transaction = self.session.begin_transaction();
        transaction.remove_image(image.id())?;
        let candidate = transaction_document(&transaction)?;
        let after = self
            .selection
            .as_ref()
            .and_then(|selection| {
                candidate
                    .paragraphs()
                    .any(|paragraph| paragraph.id().as_str() == selection.focus.paragraph_id)
                    .then(|| selection.clone())
            })
            .or_else(|| crate::first_cursor(&candidate));
        self.commit_with_image(
            transaction,
            self.selection.clone(),
            after,
            self.selected_image.clone(),
            None,
        )?;
        self.typing = None;
        Ok(())
    }

    pub(super) fn move_image_cursor(&mut self, direction: Direction) -> Result<(), EngineError> {
        let image = self.selected_image_value()?;
        let (section_index, index) = self.session.document().sections().iter().enumerate().find_map(|(section_index, section)|
            section.blocks().iter().position(|block| matches!(block, Block::Image(current) if current.id() == image.id())).map(|index| (section_index, index)))
            .expect("selected image belongs to document");
        let section = &self.session.document().sections()[section_index];
        let neighbor_index = match direction {
            Direction::Backward => index.checked_sub(1),
            Direction::Forward => Some(index + 1),
        };
        if let Some(Block::Paragraph(paragraph)) =
            neighbor_index.and_then(|index| section.blocks().get(index))
        {
            let end = if matches!(direction, Direction::Backward) {
                TextCoordinates::new(paragraph).utf16_len().get()
            } else {
                0
            };
            self.selection = Some(caret(paragraph.id(), end));
            self.selected_image = None;
            self.typing = None;
            self.layout_state.reset_visual();
            return Ok(());
        }
        // An image-only import must not trap the editor. Enter/arrow is an
        // explicit request to create a text flow when no adjacent body one exists.
        let paragraph_id = self.allocate_id()?;
        let mut transaction = self.session.begin_transaction();
        if matches!(direction, Direction::Forward) {
            transaction.insert_paragraph_after_image(
                image.id(),
                Paragraph::new(paragraph_id.clone(), vec![]),
            )?;
        } else {
            let section = &mut transaction.draft_mut().sections[section_index];
            let mut blocks = section.blocks().to_vec();
            blocks.insert(
                index,
                Block::Paragraph(Paragraph::new(paragraph_id.clone(), vec![])),
            );
            *section = section_with_blocks(section, blocks);
        }
        self.commit_with_image(
            transaction,
            self.selection.clone(),
            Some(caret(&paragraph_id, 0)),
            self.selected_image.clone(),
            None,
        )?;
        self.typing = None;
        Ok(())
    }
}
