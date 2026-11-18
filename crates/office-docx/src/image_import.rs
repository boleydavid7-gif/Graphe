//! Strict dedicated-paragraph DrawingML pictures; unsupported effects stay read-only.

use std::collections::{BTreeMap, BTreeSet};

use office_core::{
    Alignment, CharacterFormat, Image, ImageAlignment, ImageAsset, ImageError, LineSpacing,
    MAX_DOCUMENT_IMAGE_BYTES, MAX_DOCUMENT_IMAGE_PIXELS, MAX_DOCUMENT_IMAGES,
    MAX_IMAGE_ALT_TEXT_BYTES, ParagraphFormat, Twips,
};
use roxmltree::Node;

use super::{
    ImportContext, WORD_STRICT, bookmark_marker, check_whitespace, check_word_attributes,
    generated_id, unsupported_element,
};
use crate::{
    CompatibilityIssueKind as Issue, CompatibilityReport, ConversionError,
    conversion::invalid,
    package::Package,
    properties::{parse_paragraph, parse_run_character},
};

pub(super) struct ImagePart {
    pub(super) name: String,
    pub(super) content_type: String,
    pub(super) relationship_description: String,
    pub(super) relationship_part: String,
}

#[derive(Default)]
pub(super) struct ImageState {
    pub(super) used: BTreeSet<String>,
    assets: BTreeMap<String, ImageAsset>,
    doc_ids: BTreeSet<u32>,
    count: usize,
    bytes: usize,
    pixels: u64,
}

struct Names {
    wp: &'static str,
    drawing: &'static str,
    picture: &'static str,
    relationship: &'static str,
}

impl Names {
    fn new(word: &str) -> Self {
        if word == WORD_STRICT {
            Self {
                wp: "http://purl.oclc.org/ooxml/drawingml/wordprocessingDrawing",
                drawing: "http://purl.oclc.org/ooxml/drawingml/main",
                picture: "http://purl.oclc.org/ooxml/drawingml/picture",
                relationship: "http://purl.oclc.org/ooxml/officeDocument/relationships",
            }
        } else {
            Self {
                wp: "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing",
                drawing: "http://schemas.openxmlformats.org/drawingml/2006/main",
                picture: "http://schemas.openxmlformats.org/drawingml/2006/picture",
                relationship: "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
            }
        }
    }
}

pub(super) fn contains_drawing(node: Node<'_, '_>, word: &str) -> bool {
    node.descendants()
        .any(|child| child.has_tag_name((word, "drawing")))
}

pub(super) struct PictureInput<'a> {
    pub(super) package: &'a Package,
    pub(super) parts: &'a BTreeMap<String, ImagePart>,
    pub(super) state: &'a mut ImageState,
}

pub(super) fn parse(
    node: Node<'_, '_>,
    context: ImportContext<'_>,
    input: PictureInput<'_>,
    report: &mut CompatibilityReport,
) -> Result<Option<Image>, ConversionError> {
    let part = context.part;
    let names = Names::new(context.namespace);
    check_word_attributes(node, context.namespace, &[], part, report);
    let mut properties = None;
    let mut drawing = None;
    let mut run_seen = false;
    for child in node.children() {
        if !child.is_element() {
            check_whitespace(child, part)?;
        } else if child.has_tag_name((context.namespace, "pPr")) {
            if properties.is_some() || run_seen {
                return Err(invalid(
                    part,
                    "duplicate or misplaced image paragraph properties",
                ));
            }
            properties = Some(parse_paragraph(child, context.namespace, part, report)?);
        } else if child.has_tag_name((context.namespace, "r")) {
            run_seen = true;
            check_word_attributes(child, context.namespace, &[], part, report);
            let mut rpr = false;
            let mut content_seen = false;
            for content in child.children() {
                if !content.is_element() {
                    check_whitespace(content, part)?;
                } else if content.has_tag_name((context.namespace, "rPr")) {
                    if rpr || content_seen {
                        return Err(invalid(
                            part,
                            "duplicate or misplaced picture run properties",
                        ));
                    }
                    rpr = true;
                    let (format, style) =
                        parse_run_character(content, context.namespace, part, report)?;
                    if style
                        .as_ref()
                        .is_some_and(|style| !context.styles.character_styles.contains_key(style))
                    {
                        if report.is_supported() {
                            return Err(invalid(
                                part,
                                "image run references an unavailable character style",
                            ));
                        }
                        report.push(Issue::UnsupportedValue,part,"image/runStyle","The image character style cannot be resolved without unsupported content.");
                    }
                    if format != CharacterFormat::default() || style.is_some() {
                        report.normalize(part,"image/runFormat","Picture-carrier text formatting is normalized to the standalone in-flow image box. Exact original formatting remains recoverable in the source.");
                    }
                } else if content.has_tag_name((context.namespace, "drawing")) {
                    content_seen = true;
                    if drawing.replace(content).is_some() {
                        unsupported_element(
                            content,
                            part,
                            report,
                            "Only one picture in a dedicated paragraph is supported.",
                        );
                    }
                } else if content.has_tag_name((context.namespace, "t"))
                    && content.text().unwrap_or("").is_empty()
                {
                    content_seen = true;
                    attrs(
                        content,
                        &[(Some(super::XML_NAMESPACE), "space")],
                        part,
                        report,
                    );
                    children(content, &[], part, report)?;
                } else {
                    content_seen = true;
                    unsupported_element(
                        content,
                        part,
                        report,
                        "Text mixed with a picture, fields and other run content remain preserved read-only.",
                    );
                }
            }
        } else if !bookmark_marker(child, context.namespace) {
            unsupported_element(
                child,
                part,
                report,
                "A body picture must occupy a dedicated ordinary paragraph.",
            );
        }
    }
    let Some(drawing) = drawing else {
        report.push(
            Issue::UnsupportedElement,
            part,
            "image/paragraph",
            "This picture is nested in unsupported paragraph content.",
        );
        return Ok(None);
    };
    attrs(drawing, &[], part, report);
    children(drawing, &[(names.wp, "inline")], part, report)?;
    let Some(inline) = optional(drawing, names.wp, "inline") else {
        report.push(Issue::UnsupportedElement, part, "image/drawing", "Only an explicit inline raster picture is editable; floating or empty drawing content stays preserved.");
        return Ok(None);
    };
    attrs(
        inline,
        &[
            (None, "distT"),
            (None, "distB"),
            (None, "distL"),
            (None, "distR"),
        ],
        part,
        report,
    );
    for field in ["distT", "distB", "distL", "distR"] {
        if inline.attribute(field).is_some_and(|value| value != "0") {
            unsigned(inline.attribute(field).unwrap(), part)?;
            report.normalize(part, "image/inlineDistance", "Inline-picture distances were normalized to the supported in-flow image box. The original is retained.");
        }
    }
    children(
        inline,
        &[
            (names.wp, "extent"),
            (names.wp, "effectExtent"),
            (names.wp, "docPr"),
            (names.wp, "cNvGraphicFramePr"),
            (names.drawing, "graphic"),
        ],
        part,
        report,
    )?;
    let extent = required(inline, names.wp, "extent", part)?;
    attrs(extent, &[(None, "cx"), (None, "cy")], part, report);
    children(extent, &[], part, report)?;
    let cx = required_unsigned(extent, "cx", part)?;
    let cy = required_unsigned(extent, "cy", part)?;
    let Some(width) = emu_twips(cx, part, report)? else {
        return Ok(None);
    };
    let Some(height) = emu_twips(cy, part, report)? else {
        return Ok(None);
    };
    if let Some(effect) = optional(inline, names.wp, "effectExtent") {
        attrs(
            effect,
            &[(None, "l"), (None, "t"), (None, "r"), (None, "b")],
            part,
            report,
        );
        children(effect, &[], part, report)?;
        for field in ["l", "t", "r", "b"] {
            if required_unsigned(effect, field, part)? != 0 {
                report.normalize(part,"image/effectExtent","Image effect margins were normalized to the in-flow image box; unsupported effects themselves remain guarded.");
            }
        }
    }
    let docpr = required(inline, names.wp, "docPr", part)?;
    let doc_id = required_unsigned(docpr, "id", part)?;
    let doc_id = u32::try_from(doc_id)
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| invalid(part, "picture docPr id must be a positive 32-bit integer"))?;
    if !input.state.doc_ids.insert(doc_id) {
        return Err(invalid(part, "duplicate picture docPr identifier"));
    }
    let mut alt = nonvisual(docpr, part, report)?;
    if let Some(frame) = optional(inline, names.wp, "cNvGraphicFramePr") {
        attrs(frame, &[], part, report);
        children(frame, &[(names.drawing, "graphicFrameLocks")], part, report)?;
        if let Some(locks) = optional(frame, names.drawing, "graphicFrameLocks") {
            locks_node(locks, part, report)?;
        }
    }
    let graphic = required(inline, names.drawing, "graphic", part)?;
    attrs(graphic, &[], part, report);
    children(graphic, &[(names.drawing, "graphicData")], part, report)?;
    let data = required(graphic, names.drawing, "graphicData", part)?;
    attrs(data, &[(None, "uri")], part, report);
    if data.attribute("uri") != Some(names.picture) {
        unsupported_element(
            data,
            part,
            report,
            "Only raster-picture graphicData is editable.",
        );
        return Ok(None);
    }
    children(data, &[(names.picture, "pic")], part, report)?;
    let pic = required(data, names.picture, "pic", part)?;
    attrs(pic, &[], part, report);
    children(
        pic,
        &[
            (names.picture, "nvPicPr"),
            (names.picture, "blipFill"),
            (names.picture, "spPr"),
        ],
        part,
        report,
    )?;
    let nonvisual_node = required(pic, names.picture, "nvPicPr", part)?;
    attrs(nonvisual_node, &[], part, report);
    children(
        nonvisual_node,
        &[(names.picture, "cNvPr"), (names.picture, "cNvPicPr")],
        part,
        report,
    )?;
    let picture_pr = required(nonvisual_node, names.picture, "cNvPr", part)?;
    u32::try_from(required_unsigned(picture_pr, "id", part)?)
        .map_err(|_| invalid(part, "picture identifier exceeds the OOXML range"))?;
    let other_alt = nonvisual(picture_pr, part, report)?;
    if alt.is_empty() {
        alt = other_alt;
    } else if !other_alt.is_empty() && alt != other_alt {
        report.normalize(part,"image/alternativeText","Different picture descriptions were consolidated into the document-level alternative text. Both remain in the original.");
    }
    let picture_locks = required(nonvisual_node, names.picture, "cNvPicPr", part)?;
    attrs(
        picture_locks,
        &[(None, "preferRelativeResize")],
        part,
        report,
    );
    if let Some(value) = picture_locks.attribute("preferRelativeResize") {
        boolean(value, part)?;
    }
    children(picture_locks, &[(names.drawing, "picLocks")], part, report)?;
    if let Some(locks) = optional(picture_locks, names.drawing, "picLocks") {
        locks_node(locks, part, report)?;
    }
    let fill = required(pic, names.picture, "blipFill", part)?;
    attrs(fill, &[(None, "dpi"), (None, "rotWithShape")], part, report);
    if let Some(value) = fill.attribute("dpi") {
        unsigned(value, part)?;
    }
    if let Some(value) = fill.attribute("rotWithShape") {
        boolean(value, part)?;
    }
    children(
        fill,
        &[
            (names.drawing, "blip"),
            (names.drawing, "srcRect"),
            (names.drawing, "stretch"),
        ],
        part,
        report,
    )?;
    if let Some(crop) = optional(fill, names.drawing, "srcRect") {
        zero_rect(crop, &["l", "t", "r", "b"], part, report)?;
    }
    let blip = required(fill, names.drawing, "blip", part)?;
    attrs(
        blip,
        &[(Some(names.relationship), "embed"), (None, "cstate")],
        part,
        report,
    );
    if blip
        .attribute("cstate")
        .is_some_and(|value| !matches!(value, "none" | "email" | "screen" | "print" | "hqprint"))
    {
        report.push(
            Issue::UnsupportedValue,
            part,
            "image/cstate",
            "Unsupported image compression state.",
        );
    }
    children(
        blip,
        &[(names.drawing, "extLst"), (names.drawing, "alphaModFix")],
        part,
        report,
    )?;
    if let Some(alpha) = optional(blip, names.drawing, "alphaModFix") {
        attrs(alpha, &[(None, "amt")], part, report);
        children(alpha, &[], part, report)?;
        if alpha
            .attribute("amt")
            .is_some_and(|value| value != "100000")
        {
            report.push(
                Issue::UnsupportedValue,
                part,
                "image/alphaModFix",
                "Image opacity effects cannot be represented safely.",
            );
        }
    }
    if let Some(ext) = optional(blip, names.drawing, "extLst") {
        dpi_extension(ext, &names, part, report)?;
    }
    let Some(stretch) = optional(fill, names.drawing, "stretch") else {
        report.push(
            Issue::UnsupportedElement,
            part,
            "image/fill",
            "Tiled or implicit image fill cannot be represented safely.",
        );
        return Ok(None);
    };
    attrs(stretch, &[], part, report);
    children(stretch, &[(names.drawing, "fillRect")], part, report)?;
    zero_rect(
        required(stretch, names.drawing, "fillRect", part)?,
        &["l", "t", "r", "b"],
        part,
        report,
    )?;
    shape(
        required(pic, names.picture, "spPr", part)?,
        &names,
        cx,
        cy,
        part,
        report,
    )?;
    let Some(relation) = blip.attribute((names.relationship, "embed")) else {
        report.push(
            Issue::UnsupportedRelationship,
            part,
            "image/embed",
            "Picture has no supported embedded image relationship.",
        );
        return Ok(None);
    };
    let Some(media) = input.parts.get(relation) else {
        report.push(
            Issue::UnsupportedRelationship,
            part,
            "image/embed",
            "Picture relationship is external, missing or has an unsupported image content type.",
        );
        return Ok(None);
    };
    input.state.count = input.state.count.saturating_add(1);
    let bytes = &input.package.parts[&media.name];
    input.state.bytes = input.state.bytes.saturating_add(bytes.len());
    if input.state.count > MAX_DOCUMENT_IMAGES || input.state.bytes > MAX_DOCUMENT_IMAGE_BYTES {
        return Err(invalid(
            part,
            "picture count or aggregate bytes exceed the supported document budget",
        ));
    }
    let header = match ImageAsset::inspect_header(bytes) {
        Ok(header) => header,
        Err(
            error @ (ImageError::UnsupportedAnimation | ImageError::UnsupportedMetadata { .. }),
        ) => {
            report.push(
                Issue::UnsupportedValue,
                &media.name,
                "image/raster",
                format!("This raster profile remains preserved read-only: {error}"),
            );
            return Ok(None);
        }
        Err(error) => {
            return Err(invalid(
                &media.name,
                format!("invalid image header: {error}"),
            ));
        }
    };
    input.state.pixels = input
        .state
        .pixels
        .saturating_add(u64::from(header.width_px) * u64::from(header.height_px));
    if input.state.pixels > MAX_DOCUMENT_IMAGE_PIXELS {
        return Err(invalid(
            part,
            "aggregate image pixels exceed the supported document budget",
        ));
    }
    let asset = if let Some(asset) = input.state.assets.get(&media.name) {
        asset.clone()
    } else {
        let asset = match ImageAsset::from_bytes(bytes) {
            Ok(asset) => asset,
            Err(
                error @ (ImageError::UnsupportedAnimation | ImageError::UnsupportedMetadata { .. }),
            ) => {
                report.push(
                    Issue::UnsupportedValue,
                    &media.name,
                    "image/raster",
                    format!("This raster profile remains preserved read-only: {error}"),
                );
                return Ok(None);
            }
            Err(error) => {
                return Err(invalid(
                    &media.name,
                    format!("invalid or unsupported raster image: {error}"),
                ));
            }
        };
        if asset.media_type() != media.content_type {
            return Err(invalid(
                &media.name,
                "image bytes disagree with the declared image content type",
            ));
        }
        input.state.assets.insert(media.name.clone(), asset.clone());
        asset
    };
    if asset.has_orientation_transform() {
        report.normalize(part,"image/orientation","The image is displayed using its EXIF orientation; external copies normalize those pixels. Exact original image bytes remain in the native file and source.");
    }
    if asset.has_color_approximation() {
        report.normalize(part,"image/color","Image color metadata requires an approximation in this profile. The original bytes are retained; preview/PDF and normalized copies use the same Rust pixels.");
    }
    if alt.len() > MAX_IMAGE_ALT_TEXT_BYTES {
        report.push(Issue::UnsupportedValue,part,"image/alternativeText","Picture alternative text exceeds the supported native limit; the complete source remains preserved.");
        return Ok(None);
    }
    input.state.used.insert(relation.into());
    let alignment = carrier_alignment(properties, context, part, report)?;
    Ok(Some(
        Image::new(
            generated_id(context.document_id, "image", input.state.count - 1)?,
            asset,
            width,
            height,
        )
        .with_alt_text(alt)
        .with_alignment(alignment),
    ))
}

fn carrier_alignment(
    properties: Option<crate::properties::ParsedParagraphProperties>,
    context: ImportContext<'_>,
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<ImageAlignment, ConversionError> {
    let (direct, style) = if let Some(value) = properties {
        if value.section.is_some() || value.numbering_id.is_some() {
            report.push(
                Issue::UnsupportedElement,
                part,
                "image/paragraph",
                "Image paragraphs carrying sections or list markers require a future model.",
            );
        }
        (value.format, value.style_id)
    } else {
        (ParagraphFormat::default(), None)
    };
    let style_id = match style {
        Some(value) => match context.styles.identifiers.get(&value) {
            Some(id) => Some(id.clone()),
            None if !report.is_supported() => {
                report.push(Issue::UnsupportedValue, part, "image/style", "The image paragraph style cannot be resolved without unsupported source content.");
                return Ok(ImageAlignment::Left);
            }
            None => return Err(invalid(part, "image paragraph references a missing style")),
        },
        None => context.styles.default_style.clone(),
    };
    let mut ancestry = Vec::new();
    let mut current = style_id.as_ref();
    let mut seen = BTreeSet::new();
    while let Some(id) = current {
        if !seen.insert(id.as_str()) {
            return Err(invalid(part, "image paragraph style cycle"));
        }
        let style = context
            .styles
            .definitions
            .iter()
            .find(|style| &style.id == id)
            .ok_or_else(|| invalid(part, "image paragraph style is missing"))?;
        ancestry.push(&style.paragraph_format);
        current = style.based_on.as_ref();
    }
    let mut effective = ParagraphFormat::default();
    for value in ancestry.into_iter().rev().chain(std::iter::once(&direct)) {
        effective.alignment = value.alignment.or(effective.alignment);
        effective.spacing_before = value.spacing_before.or(effective.spacing_before);
        effective.spacing_after = value.spacing_after.or(effective.spacing_after);
        effective.line_spacing = value.line_spacing.or(effective.line_spacing);
        if let Some(value) = &value.indentation {
            let inherited = effective.indentation.get_or_insert_default();
            inherited.left = value.left.or(inherited.left);
            inherited.right = value.right.or(inherited.right);
            inherited.first_line = value.first_line.or(inherited.first_line);
        }
        effective.page_break_before = value.page_break_before.or(effective.page_break_before);
    }
    if effective.page_break_before == Some(true)
        || matches!(effective.line_spacing, Some(LineSpacing::Exact { .. }))
    {
        report.push(
            Issue::UnsupportedValue,
            part,
            "image/paragraph",
            "Image paragraph page breaks or exact-height clipping cannot be normalized safely.",
        );
    }
    if effective.spacing_before.is_some_and(|v| v.0 != 0)
        || effective.spacing_after.is_some_and(|v| v.0 != 0)
        || effective.line_spacing.is_some()
        || effective.indentation.as_ref().is_some_and(|value| {
            [value.left, value.right, value.first_line]
                .iter()
                .any(|v| v.is_some_and(|v| v.0 != 0))
        })
    {
        report.normalize(part,"image/paragraphFormat","Picture-carrier spacing and indentation were normalized to the supported in-flow box. Exact source formatting remains in the original.");
    }
    Ok(match effective.alignment {
        Some(Alignment::Center) => ImageAlignment::Center,
        Some(Alignment::Right) => ImageAlignment::Right,
        Some(Alignment::Justify) => {
            report.normalize(
                part,
                "image/alignment",
                "Justified picture alignment was normalized to left alignment.",
            );
            ImageAlignment::Left
        }
        _ => ImageAlignment::Left,
    })
}

fn shape(
    node: Node<'_, '_>,
    names: &Names,
    cx: u64,
    cy: u64,
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<(), ConversionError> {
    attrs(node, &[(None, "bwMode")], part, report);
    if node.attribute("bwMode").is_some_and(|v| v != "auto") {
        report.push(
            Issue::UnsupportedValue,
            part,
            "image/bwMode",
            "Image color effects remain read-only.",
        );
    }
    children(
        node,
        &[
            (names.drawing, "xfrm"),
            (names.drawing, "prstGeom"),
            (names.drawing, "noFill"),
        ],
        part,
        report,
    )?;
    if let Some(fill) = optional(node, names.drawing, "noFill") {
        attrs(fill, &[], part, report);
        children(fill, &[], part, report)?;
    }
    if let Some(transform) = optional(node, names.drawing, "xfrm") {
        attrs(
            transform,
            &[(None, "rot"), (None, "flipH"), (None, "flipV")],
            part,
            report,
        );
        if transform.attribute("rot").is_some_and(|v| v != "0")
            || transform
                .attribute("flipH")
                .is_some_and(|v| !matches!(v, "0" | "false"))
            || transform
                .attribute("flipV")
                .is_some_and(|v| !matches!(v, "0" | "false"))
        {
            report.push(
                Issue::UnsupportedValue,
                part,
                "image/transform",
                "DrawingML rotation and flips remain read-only.",
            );
        }
        children(
            transform,
            &[(names.drawing, "off"), (names.drawing, "ext")],
            part,
            report,
        )?;
        zero_rect(
            required(transform, names.drawing, "off", part)?,
            &["x", "y"],
            part,
            report,
        )?;
        let extent = required(transform, names.drawing, "ext", part)?;
        attrs(extent, &[(None, "cx"), (None, "cy")], part, report);
        children(extent, &[], part, report)?;
        if required_unsigned(extent, "cx", part)? != cx
            || required_unsigned(extent, "cy", part)? != cy
        {
            report.push(
                Issue::UnsupportedValue,
                part,
                "image/transform",
                "Inconsistent inline and picture extents remain read-only.",
            );
        }
    }
    if let Some(geometry) = optional(node, names.drawing, "prstGeom") {
        attrs(geometry, &[(None, "prst")], part, report);
        if geometry.attribute("prst") != Some("rect") {
            report.push(
                Issue::UnsupportedValue,
                part,
                "image/shape",
                "Only a full rectangular picture is supported.",
            );
        }
        children(geometry, &[(names.drawing, "avLst")], part, report)?;
        if let Some(adjust) = optional(geometry, names.drawing, "avLst") {
            attrs(adjust, &[], part, report);
            children(adjust, &[], part, report)?;
        }
    }
    Ok(())
}

fn nonvisual(
    node: Node<'_, '_>,
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<String, ConversionError> {
    attrs(
        node,
        &[
            (None, "id"),
            (None, "name"),
            (None, "descr"),
            (None, "title"),
            (None, "hidden"),
        ],
        part,
        report,
    );
    children(node, &[], part, report)?;
    if node
        .attribute("hidden")
        .is_some_and(|v| !matches!(v, "0" | "false"))
    {
        report.push(
            Issue::UnsupportedValue,
            part,
            "image/hidden",
            "Hidden images cannot become visible editable content.",
        );
    }
    let descr = node.attribute("descr").unwrap_or("");
    let title = node.attribute("title").unwrap_or("");
    if !descr.is_empty() && !title.is_empty() && descr != title {
        report.normalize(part,"image/title","The picture title is retained only in the source; its alternative description is preserved in the model.");
    }
    Ok(if descr.is_empty() { title } else { descr }.into())
}

fn locks_node(
    node: Node<'_, '_>,
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<(), ConversionError> {
    // Picture arrowhead locks cannot change raster pixels or geometry. Word
    // and LibreOffice emit these even for rectangular raster pictures.
    attrs(
        node,
        &[(None, "noChangeAspect"), (None, "noChangeArrowheads")],
        part,
        report,
    );
    children(node, &[], part, report)?;
    if let Some(value) = node.attribute("noChangeAspect") {
        boolean(value, part)?;
    }
    if let Some(value) = node.attribute("noChangeArrowheads") {
        boolean(value, part)?;
    }
    Ok(())
}

fn dpi_extension(
    node: Node<'_, '_>,
    names: &Names,
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<(), ConversionError> {
    attrs(node, &[], part, report);
    // The only recognized extension is a static DPI preference, irrelevant to
    // explicit EMU extents. SVG alternatives and all unknown extensions block.
    for ext in node.children() {
        if !ext.is_element() {
            check_whitespace(ext, part)?;
            continue;
        }
        if !ext.has_tag_name((names.drawing, "ext"))
            || ext.attribute("uri") != Some("{28A0092B-C50C-407E-A947-70E740481C1C}")
        {
            unsupported_element(
                ext,
                part,
                report,
                "Unsupported image extension remains preserved read-only.",
            );
            continue;
        }
        attrs(ext, &[(None, "uri")], part, report);
        const A14: &str = "http://schemas.microsoft.com/office/drawing/2010/main";
        children(ext, &[(A14, "useLocalDpi")], part, report)?;
        let dpi = required(ext, A14, "useLocalDpi", part)?;
        attrs(dpi, &[(None, "val")], part, report);
        children(dpi, &[], part, report)?;
        boolean(
            dpi.attribute("val")
                .ok_or_else(|| invalid(part, "useLocalDpi requires val"))?,
            part,
        )?;
    }
    Ok(())
}

fn zero_rect(
    node: Node<'_, '_>,
    fields: &[&str],
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<(), ConversionError> {
    let allowed: Vec<_> = fields.iter().map(|&field| (None, field)).collect();
    attrs(node, &allowed, part, report);
    children(node, &[], part, report)?;
    for field in fields {
        if node.attribute(*field).is_some_and(|value| value != "0") {
            report.push(
                Issue::UnsupportedValue,
                part,
                "image/cropOrOffset",
                "Picture cropping, fill adjustments and offsets remain read-only.",
            );
        }
    }
    Ok(())
}

fn attrs(
    node: Node<'_, '_>,
    allowed: &[(Option<&str>, &str)],
    part: &str,
    report: &mut CompatibilityReport,
) {
    for attr in node.attributes() {
        if !allowed.contains(&(attr.namespace(), attr.name())) {
            report.push(
                Issue::UnsupportedAttribute,
                part,
                format!("image/{}/{}", node.tag_name().name(), attr.name()),
                "Unsupported image attribute remains in the original.",
            );
        }
    }
}
fn children(
    node: Node<'_, '_>,
    allowed: &[(&str, &str)],
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<(), ConversionError> {
    let mut seen = BTreeSet::new();
    for child in node.children() {
        if !child.is_element() {
            check_whitespace(child, part)?;
        } else if allowed.contains(&(
            child.tag_name().namespace().unwrap_or(""),
            child.tag_name().name(),
        )) {
            if !seen.insert((child.tag_name().namespace(), child.tag_name().name())) {
                return Err(invalid(part, "duplicate picture structure"));
            }
        } else {
            unsupported_element(
                child,
                part,
                report,
                "Unsupported picture content remains read-only.",
            );
        }
    }
    Ok(())
}
fn optional<'a, 'input>(
    node: Node<'a, 'input>,
    namespace: &str,
    name: &str,
) -> Option<Node<'a, 'input>> {
    node.children()
        .find(|child| child.has_tag_name((namespace, name)))
}
fn required<'a, 'input>(
    node: Node<'a, 'input>,
    namespace: &str,
    name: &str,
    part: &str,
) -> Result<Node<'a, 'input>, ConversionError> {
    optional(node, namespace, name)
        .ok_or_else(|| invalid(part, format!("picture is missing {name}")))
}
fn unsigned(value: &str, part: &str) -> Result<u64, ConversionError> {
    if value.is_empty() || !value.bytes().all(|c| c.is_ascii_digit()) {
        return Err(invalid(part, "picture integer is malformed"));
    }
    value
        .parse()
        .map_err(|_| invalid(part, "picture integer is out of range"))
}
fn required_unsigned(node: Node<'_, '_>, name: &str, part: &str) -> Result<u64, ConversionError> {
    unsigned(
        node.attribute(name)
            .ok_or_else(|| invalid(part, format!("picture is missing {name}")))?,
        part,
    )
}
fn boolean(value: &str, part: &str) -> Result<(), ConversionError> {
    if matches!(value, "0" | "1" | "true" | "false") {
        Ok(())
    } else {
        Err(invalid(part, "picture boolean is malformed"))
    }
}
fn emu_twips(
    value: u64,
    part: &str,
    report: &mut CompatibilityReport,
) -> Result<Option<Twips>, ConversionError> {
    if value == 0 {
        return Err(invalid(part, "picture extents must be positive"));
    }
    let rounded = value / 635 + u64::from(value % 635 >= 318);
    let Some(twips) = i32::try_from(rounded).ok().filter(|v| *v > 0) else {
        report.push(
            Issue::UnsupportedValue,
            part,
            "image/extent",
            "Picture extent is outside the positive native twip range.",
        );
        return Ok(None);
    };
    if !value.is_multiple_of(635) {
        report.normalize(part,"image/extent","Picture dimensions were rounded to the nearest native twip (1/20 point); exact source geometry remains in the original.");
    }
    Ok(Some(Twips(twips)))
}
