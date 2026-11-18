//! Bounded as-character raster frames projected into standalone image blocks.

use super::*;
use office_core::{Alignment, Image, ImageAlignment, ImageAsset, ImageError, Twips};

pub(super) const DRAW: &str = "urn:oasis:names:tc:opendocument:xmlns:drawing:1.0";
const XLINK: &str = "http://www.w3.org/1999/xlink";

fn unsupported(error: &ImageError) -> bool {
    matches!(
        error,
        ImageError::UnsupportedFormat
            | ImageError::UnsupportedAnimation
            | ImageError::UnsupportedMetadata { .. }
    )
}

fn image_error(error: ImageError) -> FormatError {
    let code = if matches!(error, ImageError::LimitExceeded { .. }) {
        "format_limit"
    } else {
        "invalid_image"
    };
    FormatError::new(code, error.to_string())
}

pub(super) fn preflight(
    content: &XmlDocument<'_>,
    parts: &BTreeMap<String, Vec<u8>>,
    media: &BTreeMap<String, String>,
) -> Result<(), FormatError> {
    let mut count = 0_usize;
    let mut bytes_total = 0_usize;
    let mut pixels = 0_u64;
    for node in content
        .descendants()
        .filter(|node| node.has_tag_name((DRAW, "image")))
    {
        let Some(href) = node.attribute((XLINK, "href")) else {
            continue;
        };
        let path = href.strip_prefix("./").unwrap_or(href);
        if !media
            .get(path)
            .is_some_and(|kind| matches!(kind.as_str(), "image/png" | "image/jpeg"))
        {
            continue;
        }
        let Some(bytes) = parts.get(path) else {
            continue;
        };
        let header = match ImageAsset::inspect_header(bytes) {
            Ok(header) => header,
            Err(error) if unsupported(&error) => continue,
            Err(error) => return Err(image_error(error)),
        };
        count += 1;
        bytes_total = bytes_total.saturating_add(bytes.len());
        pixels = pixels.saturating_add(u64::from(header.width_px) * u64::from(header.height_px));
        if count > office_core::MAX_DOCUMENT_IMAGES
            || bytes_total > office_core::MAX_DOCUMENT_IMAGE_BYTES
            || pixels > office_core::MAX_DOCUMENT_IMAGE_PIXELS
        {
            return Err(FormatError::new(
                "format_limit",
                "ODT pictures exceed the aggregate count, byte or decoded-pixel budget.",
            ));
        }
    }
    Ok(())
}

impl TextParser<'_> {
    pub(super) fn body_paragraph(
        &mut self,
        node: Node<'_, '_>,
    ) -> Result<(Option<Block>, Option<SectionProperties>), FormatError> {
        if node
            .descendants()
            .any(|node| node.has_tag_name((DRAW, "frame")))
        {
            return self.image_paragraph(node);
        }
        let (paragraph, geometry) = self.paragraph(node)?;
        Ok((Some(Block::Paragraph(paragraph)), geometry))
    }

    fn image_unsupported(&mut self, path: &str, reason: &str) {
        diagnostic(
            self.report,
            "unsupported_image",
            "content.xml",
            path,
            reason,
        );
    }

    fn image_paragraph(
        &mut self,
        node: Node<'_, '_>,
    ) -> Result<(Option<Block>, Option<SectionProperties>), FormatError> {
        attributes(node, "content.xml", &[(TEXT, "style-name")], self.report);
        let children = node.children().filter(Node::is_element).collect::<Vec<_>>();
        if !node.has_tag_name((TEXT, "p"))
            || children.len() != 1
            || !children[0].has_tag_name((DRAW, "frame"))
            || node
                .children()
                .filter(Node::is_text)
                .any(|child| child.text().is_some_and(|text| !text.trim().is_empty()))
        {
            self.image_unsupported("text:p", "Mixed text/pictures, multiple pictures and wrapped image content are retained read-only.");
            return Ok((None, None));
        }
        let frame = children[0];
        attributes(
            frame,
            "content.xml",
            &[
                (DRAW, "name"),
                (TEXT, "anchor-type"),
                (SVG, "width"),
                (SVG, "height"),
                (DRAW, "z-index"),
            ],
            self.report,
        );
        container_text(frame)?;
        if frame.attribute((TEXT, "anchor-type")) != Some("as-char") {
            self.image_unsupported(
                "draw:frame/@text:anchor-type",
                "Floating and text-wrapped pictures remain preserved read-only.",
            );
            return Ok((None, None));
        }
        if frame
            .attribute((DRAW, "z-index"))
            .is_some_and(|value| value != "0")
        {
            self.image_unsupported(
                "draw:frame/@draw:z-index",
                "Layered pictures remain preserved read-only.",
            );
        }
        let width = frame
            .attribute((SVG, "width"))
            .ok_or_else(|| package::invalid("image width is missing"))
            .and_then(properties::length)?;
        let height = frame
            .attribute((SVG, "height"))
            .ok_or_else(|| package::invalid("image height is missing"))
            .and_then(properties::length)?;
        if width.0 <= 0 || height.0 <= 0 {
            return Err(package::invalid("image dimensions must be positive"));
        }
        let mut picture = None;
        let mut description = None;
        let mut title = None;
        for child in frame.children().filter(Node::is_element) {
            if child.has_tag_name((DRAW, "image")) {
                if picture.replace(child).is_some() {
                    return Err(package::invalid(
                        "image frame contains duplicate image resources",
                    ));
                }
            } else if child.has_tag_name((SVG, "desc")) || child.has_tag_name((SVG, "title")) {
                attributes(child, "content.xml", &[], self.report);
                if child.children().any(|node| node.is_element()) {
                    self.image_unsupported(
                        "svg:desc",
                        "Structured picture descriptions remain preserved read-only.",
                    );
                }
                let value = child
                    .children()
                    .filter(Node::is_text)
                    .filter_map(|n| n.text())
                    .collect::<String>();
                let destination = if child.has_tag_name((SVG, "desc")) {
                    &mut description
                } else {
                    &mut title
                };
                if destination.replace(value).is_some() {
                    return Err(package::invalid("duplicate picture description"));
                }
            } else {
                self.image_unsupported(
                    child.tag_name().name(),
                    "Extended drawing content remains preserved read-only.",
                );
            }
        }
        let Some(picture) = picture else {
            self.image_unsupported(
                "draw:frame",
                "Only complete embedded PNG/JPEG pictures fit the editable image profile.",
            );
            return Ok((None, None));
        };
        attributes(
            picture,
            "content.xml",
            &[
                (XLINK, "href"),
                (XLINK, "type"),
                (XLINK, "show"),
                (XLINK, "actuate"),
                (DRAW, "mime-type"),
            ],
            self.report,
        );
        container_text(picture)?;
        if picture.children().any(|node| node.is_element())
            || picture
                .attribute((XLINK, "type"))
                .is_some_and(|v| v != "simple")
            || picture
                .attribute((XLINK, "show"))
                .is_some_and(|v| v != "embed")
            || picture
                .attribute((XLINK, "actuate"))
                .is_some_and(|v| v != "onLoad")
        {
            self.image_unsupported(
                "draw:image",
                "Extended or deferred picture resources remain preserved read-only.",
            );
        }
        let href = picture
            .attribute((XLINK, "href"))
            .ok_or_else(|| package::invalid("image relationship is missing"))?;
        let path = href.strip_prefix("./").unwrap_or(href);
        if path.is_empty()
            || path.starts_with('/')
            || path.contains([':', '\\', '#', '?', '%'])
            || path
                .split('/')
                .any(|segment| segment.is_empty() || segment == "." || segment == "..")
        {
            self.image_unsupported("draw:image/@xlink:href", "External or noncanonical picture references remain preserved read-only; no resource is fetched.");
            return Ok((None, None));
        }
        let media = self
            .image_media
            .get(path)
            .ok_or_else(|| package::invalid("image is absent from the manifest"))?;
        if !matches!(media.as_str(), "image/png" | "image/jpeg") {
            self.image_unsupported(
                "draw:image",
                "This raster format is unsupported; the original remains preserved read-only.",
            );
            return Ok((None, None));
        }
        if picture
            .attribute((DRAW, "mime-type"))
            .is_some_and(|value| value != media)
        {
            return Err(package::invalid(
                "image MIME declaration differs from the manifest",
            ));
        }
        let bytes = self
            .parts
            .get(path)
            .ok_or_else(|| package::invalid("embedded picture part is missing"))?;
        let header = match ImageAsset::inspect_header(bytes) {
            Ok(header) => header,
            Err(error) if unsupported(&error) => {
                self.image_unsupported(path, &error.to_string());
                return Ok((None, None));
            }
            Err(error) => return Err(image_error(error)),
        };
        self.image_count += 1;
        self.image_bytes = self.image_bytes.saturating_add(bytes.len());
        self.image_pixels = self
            .image_pixels
            .saturating_add(u64::from(header.width_px) * u64::from(header.height_px));
        if self.image_count > office_core::MAX_DOCUMENT_IMAGES
            || self.image_bytes > office_core::MAX_DOCUMENT_IMAGE_BYTES
            || self.image_pixels > office_core::MAX_DOCUMENT_IMAGE_PIXELS
        {
            return Err(FormatError::new(
                "format_limit",
                "ODT pictures exceed the aggregate count, byte or decoded-pixel budget.",
            ));
        }
        let asset = if let Some(asset) = self.image_cache.get(path) {
            asset.clone()
        } else {
            let asset = match ImageAsset::from_bytes(bytes) {
                Ok(asset) => asset,
                Err(error) if unsupported(&error) => {
                    self.image_unsupported(path, &error.to_string());
                    return Ok((None, None));
                }
                Err(error) => return Err(image_error(error)),
            };
            self.image_cache.insert(path.to_owned(), asset.clone());
            asset
        };
        if asset.media_type() != media {
            return Err(package::invalid(
                "image signature differs from the declared media type",
            ));
        }
        if asset.has_color_approximation() {
            self.notices.record("content.xml", path,
                "Picture color profiles or high-precision colors are approximated in the editable preview; exact original image bytes remain in the native document.", true);
        }
        if asset.has_orientation_transform() {
            self.notices.record("content.xml", path, "Picture orientation metadata is applied to the editable preview. External copies use a normalized raster; exact original image bytes remain in the native document.", true);
        }
        let mut paragraph = self.catalog.default_paragraph.clone();
        let mut master = None;
        if let Some(name) = node.attribute((TEXT, "style-name")) {
            for style in self.catalog.chain("paragraph", name)? {
                properties::overlay_paragraph(&mut paragraph, &style.paragraph);
                if let Some(value) = &style.master {
                    master = Some(value.clone());
                }
            }
        }
        let alignment = match paragraph.alignment.unwrap_or(Alignment::Left) {
            Alignment::Left => ImageAlignment::Left,
            Alignment::Center => ImageAlignment::Center,
            Alignment::Right => ImageAlignment::Right,
            Alignment::Justify => {
                self.notices.record(
                    "content.xml",
                    "image paragraph alignment",
                    "Justified picture alignment is normalized to left alignment.",
                    true,
                );
                ImageAlignment::Left
            }
        };
        let indented = paragraph.indentation.as_ref().is_some_and(|value| {
            [value.left, value.right, value.first_line]
                .into_iter()
                .flatten()
                .any(|v| v.0 != 0)
        });
        if paragraph.spacing_before.is_some_and(|v| v != Twips(0))
            || paragraph.spacing_after.is_some_and(|v| v != Twips(0))
            || indented
            || matches!(
                paragraph.line_spacing,
                Some(LineSpacing::Exact { .. } | LineSpacing::AtLeast { .. })
            )
        {
            self.notices.record("content.xml", "image paragraph spacing", "Picture paragraph spacing and indentation are normalized to the in-flow image profile; original formatting remains preserved.", true);
        }
        if paragraph.page_break_before == Some(true) && master.is_none() {
            self.image_unsupported(
                "image paragraph page break",
                "A picture page break without a section boundary remains preserved read-only.",
            );
        }
        let alt = match (title, description) {
            (Some(title), Some(description)) if !title.is_empty() && title != description => {
                self.notices.record(
                    "content.xml",
                    "picture description",
                    "Picture titles and descriptions are combined into one image description.",
                    true,
                );
                format!("{title} — {description}")
            }
            (_, Some(description)) => description,
            (Some(title), _) => title,
            _ => String::new(),
        };
        self.image_parts.insert(path.to_owned());
        let image = Image::new(
            Id::new(format!(
                "{}:odt:image:{}",
                self.document_id.as_str(),
                self.image_count
            ))?,
            asset,
            width,
            height,
        )
        .with_alignment(alignment)
        .with_alt_text(alt);
        let geometry = master
            .map(|name| self.catalog.geometry(Some(&name)))
            .transpose()?
            .flatten();
        Ok((Some(Block::Image(image)), geometry))
    }
}
