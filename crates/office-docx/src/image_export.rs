//! Deterministic DrawingML picture parts for the bounded body-image profile.

use office_core::{Block, Document, Image, ImageAlignment};

use super::{ConversionError, OFFICE_REL, Xml, unsupported};

const WP: &str = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing";
const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const PIC: &str = "http://schemas.openxmlformats.org/drawingml/2006/picture";

pub(super) struct ImagePart<'a> {
    pub(super) image: &'a Image,
    pub(super) name: String,
    pub(super) relationship: String,
    pub(super) content_type: &'static str,
    pub(super) bytes: &'a [u8],
    ordinal: usize,
}

pub(super) fn parts(document: &Document) -> Vec<ImagePart<'_>> {
    document
        .sections()
        .iter()
        .flat_map(|section| section.blocks())
        .filter_map(|block| {
            if let Block::Image(image) = block {
                Some(image)
            } else {
                None
            }
        })
        .enumerate()
        .map(|(index, image)| {
            let ordinal = index + 1;
            let asset = image.asset();
            // Word consumers do not agree on EXIF orientation or source color
            // profiles. For these explicit approximations use the same normalized
            // pixels as Rust preview/PDF; native/source files retain exact originals.
            let normalized = asset.has_orientation_transform() || asset.has_color_approximation();
            let content_type = if normalized {
                "image/png"
            } else {
                asset.media_type()
            };
            let extension = if content_type == "image/png" {
                "png"
            } else {
                "jpg"
            };
            ImagePart {
                image,
                name: format!("word/media/image{ordinal}.{extension}"),
                relationship: format!("rIdImage{ordinal}"),
                content_type,
                bytes: if normalized {
                    asset.render_png()
                } else {
                    asset.original_bytes()
                },
                ordinal,
            }
        })
        .collect()
}

pub(super) fn write(xml: &mut Xml, value: &ImagePart<'_>) -> Result<(), ConversionError> {
    let image = value.image;
    let cx = i64::from(image.width().0)
        .checked_mul(635)
        .ok_or_else(|| unsupported("image extent exceeds the supported EMU range"))?
        .to_string();
    let cy = i64::from(image.height().0)
        .checked_mul(635)
        .ok_or_else(|| unsupported("image extent exceeds the supported EMU range"))?
        .to_string();
    let ordinal = value.ordinal.to_string();
    let name = format!("Picture {}", value.ordinal);
    let alignment = match image.alignment() {
        ImageAlignment::Left => "left",
        ImageAlignment::Center => "center",
        ImageAlignment::Right => "right",
    };
    xml.start("w:p", &[])?;
    xml.start("w:pPr", &[])?;
    xml.empty("w:spacing", &[("w:before", "0"), ("w:after", "0")])?;
    xml.empty("w:jc", &[("w:val", alignment)])?;
    xml.end("w:pPr")?;
    xml.start("w:r", &[])?;
    xml.start("w:drawing", &[])?;
    xml.start(
        "wp:inline",
        &[
            ("xmlns:wp", WP),
            ("distT", "0"),
            ("distB", "0"),
            ("distL", "0"),
            ("distR", "0"),
        ],
    )?;
    xml.empty("wp:extent", &[("cx", &cx), ("cy", &cy)])?;
    xml.empty(
        "wp:docPr",
        &[
            ("id", &ordinal),
            ("name", &name),
            ("descr", image.alt_text()),
        ],
    )?;
    xml.start("wp:cNvGraphicFramePr", &[])?;
    xml.empty(
        "a:graphicFrameLocks",
        &[("xmlns:a", A), ("noChangeAspect", "1")],
    )?;
    xml.end("wp:cNvGraphicFramePr")?;
    xml.start("a:graphic", &[("xmlns:a", A)])?;
    xml.start("a:graphicData", &[("uri", PIC)])?;
    xml.start("pic:pic", &[("xmlns:pic", PIC)])?;
    xml.start("pic:nvPicPr", &[])?;
    xml.empty("pic:cNvPr", &[("id", "0"), ("name", &name)])?;
    xml.empty("pic:cNvPicPr", &[])?;
    xml.end("pic:nvPicPr")?;
    xml.start("pic:blipFill", &[])?;
    xml.empty(
        "a:blip",
        &[("xmlns:r", OFFICE_REL), ("r:embed", &value.relationship)],
    )?;
    xml.start("a:stretch", &[])?;
    xml.empty("a:fillRect", &[])?;
    xml.end("a:stretch")?;
    xml.end("pic:blipFill")?;
    xml.start("pic:spPr", &[])?;
    xml.start("a:xfrm", &[])?;
    xml.empty("a:off", &[("x", "0"), ("y", "0")])?;
    xml.empty("a:ext", &[("cx", &cx), ("cy", &cy)])?;
    xml.end("a:xfrm")?;
    xml.start("a:prstGeom", &[("prst", "rect")])?;
    xml.empty("a:avLst", &[])?;
    xml.end("a:prstGeom")?;
    xml.end("pic:spPr")?;
    xml.end("pic:pic")?;
    xml.end("a:graphicData")?;
    xml.end("a:graphic")?;
    xml.end("wp:inline")?;
    xml.end("w:drawing")?;
    xml.end("w:r")?;
    xml.end("w:p")
}
