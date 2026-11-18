//! Deterministic ODF 1.3 output from the existing supported model.

use super::{
    MEDIA_TYPE,
    properties::{character_xml, paragraph_xml, points, xml_escape},
};
use crate::FormatError;
use office_core::{
    Block, CharacterFormat, Document, HalfPoints, Image, ImageAlignment, LineSpacing,
    PageOrientation, Paragraph, ParagraphFormat, TextColor, Twips, Underline,
};
use std::{
    collections::BTreeMap,
    io::{Cursor, Write},
};
use zip::{CompressionMethod, DateTime, System, ZipWriter, write::SimpleFileOptions};

const DECLARATIONS: &str = "xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" xmlns:style=\"urn:oasis:names:tc:opendocument:xmlns:style:1.0\" xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" xmlns:fo=\"urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0\" xmlns:svg=\"urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0\" xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\" xmlns:draw=\"urn:oasis:names:tc:opendocument:xmlns:drawing:1.0\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" office:version=\"1.3\"";

fn error(error: impl std::fmt::Display) -> FormatError {
    FormatError::new("odt_export", error.to_string())
}

pub(super) fn export_document(document: &Document) -> Result<Vec<u8>, FormatError> {
    document.validate()?;
    // Every generated paragraph/run style must fit the same strict importer
    // profile. Refuse large conversions before publishing an unreopenable copy.
    let paragraphs = document.paragraphs().count();
    let images = document
        .sections()
        .iter()
        .flat_map(|section| section.blocks())
        .filter(|block| matches!(block, Block::Image(_)))
        .count();
    let styles = document
        .styles()
        .len()
        .saturating_add(paragraphs)
        .saturating_add(images)
        .saturating_add(document.paragraphs().map(|p| p.runs().len()).sum::<usize>());
    if paragraphs > 4096 || styles > 2048 {
        return Err(FormatError::new(
            "format_limit",
            "ODT export exceeds the supported paragraph/style limit. Keep the native Graphe file.",
        ));
    }
    if document
        .sections()
        .iter()
        .any(|section| section.blocks().is_empty())
    {
        return Err(FormatError::new(
            "unsupported_odt_export",
            "An empty section has no paragraph to carry its ODT page geometry and page boundary. Save a native Writer copy.",
        ));
    }
    let named: BTreeMap<_, _> = document
        .styles()
        .iter()
        .enumerate()
        .map(|(i, s)| (s.id.as_str(), format!("N{i}")))
        .collect();
    let mut styles = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><office:document-styles {DECLARATIONS}><office:styles>"
    );
    let defaults = CharacterFormat {
        bold: Some(false),
        italic: Some(false),
        underline: Some(Underline::None),
        font_family: Some("Arimo".into()),
        font_size: Some(HalfPoints(24)),
        color: Some(TextColor::Auto {}),
        hyperlink: None,
    };
    let paragraph_defaults = ParagraphFormat {
        line_spacing: Some(LineSpacing::Multiple { value: 360 }),
        spacing_after: Some(Twips(160)),
        ..ParagraphFormat::default()
    };
    styles.push_str(&format!(
        "<style:default-style style:family=\"paragraph\">{}{}</style:default-style>",
        paragraph_xml(&paragraph_defaults),
        character_xml(&defaults)?
    ));
    for style in document.styles() {
        let parent = style
            .based_on
            .as_ref()
            .map(|id| format!(" style:parent-style-name=\"{}\"", named[id.as_str()]))
            .unwrap_or_default();
        styles.push_str(&format!("<style:style style:name=\"{}\" style:display-name=\"{}\" style:family=\"paragraph\"{parent}>{}{}</style:style>",named[style.id.as_str()],xml_escape(&style.name)?,paragraph_xml(&style.paragraph_format),character_xml(&style.character_format)?));
    }
    styles.push_str("</office:styles><office:automatic-styles>");
    let defaults = office_layout::LayoutOptions::default();
    for (index, section) in document.sections().iter().enumerate() {
        let properties = section.properties();
        let size = properties
            .and_then(|p| p.page_size)
            .unwrap_or(defaults.page_size);
        let margins = properties
            .and_then(|p| p.page_margins)
            .unwrap_or(defaults.page_margins);
        // ODF has no equivalent of the native model's inert header/footer offset
        // and binding-margin distinctions without header/footer/section features.
        // Reject nonzero binding margins instead of changing the body geometry.
        if margins.gutter.0 != 0 {
            return Err(FormatError::new(
                "unsupported_odt_export",
                "ODT export does not represent a binding gutter.",
            ));
        }
        let orientation = size
            .orientation
            .map(|o| {
                format!(
                    " style:print-orientation=\"{}\"",
                    match o {
                        PageOrientation::Portrait => "portrait",
                        PageOrientation::Landscape => "landscape",
                    }
                )
            })
            .unwrap_or_default();
        styles.push_str(&format!("<style:page-layout style:name=\"L{index}\"><style:page-layout-properties fo:page-width=\"{}\" fo:page-height=\"{}\" fo:margin-top=\"{}\" fo:margin-right=\"{}\" fo:margin-bottom=\"{}\" fo:margin-left=\"{}\"{orientation}/></style:page-layout>",points(size.width),points(size.height),points(margins.top),points(margins.right),points(margins.bottom),points(margins.left)));
    }
    styles.push_str("</office:automatic-styles><office:master-styles>");
    for (index, _) in document.sections().iter().enumerate() {
        styles.push_str(&format!(
            "<style:master-page style:name=\"M{index}\" style:page-layout-name=\"L{index}\"/>"
        ));
    }
    styles.push_str("</office:master-styles></office:document-styles>");
    let mut content = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><office:document-content {DECLARATIONS}><office:automatic-styles>"
    );
    for (s, section) in document.sections().iter().enumerate() {
        for (p, paragraph) in section.paragraphs().enumerate() {
            let parent = paragraph
                .style()
                .map(|id| format!(" style:parent-style-name=\"{}\"", named[id.as_str()]))
                .unwrap_or_default();
            let master = if p == 0 && matches!(section.blocks().first(), Some(Block::Paragraph(_)))
            {
                format!(" style:master-page-name=\"M{s}\"")
            } else {
                String::new()
            };
            let mut format = paragraph.format().clone();
            if s > 0 && p == 0 && matches!(section.blocks().first(), Some(Block::Paragraph(_))) {
                format.page_break_before = Some(true);
            }
            content.push_str(&format!("<style:style style:name=\"P{s}_{p}\" style:family=\"paragraph\"{parent}{master}>{}</style:style>",paragraph_xml(&format)));
            for (r, run) in paragraph.runs().iter().enumerate() {
                content.push_str(&format!("<style:style style:name=\"T{s}_{p}_{r}\" style:family=\"text\">{}</style:style>",character_xml(&run.format)?));
            }
        }
    }
    content.push_str("<style:style style:name=\"GridCell\" style:family=\"table-cell\"><style:table-cell-properties fo:border=\"0.5pt solid #808080\" fo:padding=\"4pt\"/></style:style>");
    let mut image_parts = Vec::new();
    for (s, section) in document.sections().iter().enumerate() {
        for (b, block) in section.blocks().iter().enumerate() {
            if let Block::Image(image) = block {
                let master = if b == 0 {
                    format!(" style:master-page-name=\"M{s}\"")
                } else {
                    String::new()
                };
                let page_break = if s > 0 && b == 0 {
                    " fo:break-before=\"page\""
                } else {
                    ""
                };
                let alignment = match image.alignment() {
                    ImageAlignment::Left => "left",
                    ImageAlignment::Center => "center",
                    ImageAlignment::Right => "right",
                };
                content.push_str(&format!("<style:style style:name=\"I{s}_{b}\" style:family=\"paragraph\"{master}><style:paragraph-properties fo:text-align=\"{alignment}\" fo:margin-top=\"0pt\" fo:margin-bottom=\"0pt\"{page_break}/></style:style>"));
                let part = image_part(image, s, b);
                if part.2.len() > 8 * 1024 * 1024 {
                    return Err(FormatError::new(
                        "format_limit",
                        "The normalized image exceeds the ODT resource limit. Keep the native Graphe document.",
                    ));
                }
                image_parts.push(part);
            }
            if let Block::Table(table) = block {
                let master = if b == 0 {
                    format!(" style:master-page-name=\"M{s}\"")
                } else {
                    String::new()
                };
                let properties = if s > 0 && b == 0 {
                    "<style:table-properties fo:break-before=\"page\"/>"
                } else {
                    ""
                };
                content.push_str(&format!("<style:style style:name=\"B{s}_{b}\" style:family=\"table\"{master}>{properties}</style:style>"));
                if let Some(widths) = table.column_widths() {
                    for (c, width) in widths.iter().enumerate() {
                        content.push_str(&format!("<style:style style:name=\"C{s}_{b}_{c}\" style:family=\"table-column\"><style:table-column-properties style:column-width=\"{}\"/></style:style>",points(*width)));
                    }
                }
            }
        }
    }
    // A derived orientation/color-safe PNG can be larger than its original
    // JPEG. Check the exported resource budget before allocating a ZIP, so a
    // successful copy always fits the same bounded image importer.
    let mut image_bytes = 0_usize;
    for (_, _, bytes) in &image_parts {
        image_bytes = image_bytes.saturating_add(bytes.len());
        if bytes.len() > office_core::MAX_IMAGE_BYTES
            || image_bytes > office_core::MAX_DOCUMENT_IMAGE_BYTES
        {
            return Err(FormatError::new(
                "format_limit",
                "Normalized ODT pictures exceed the supported image byte budget. Keep the native Graphe file.",
            ));
        }
    }
    content.push_str("</office:automatic-styles><office:body><office:text>");
    if document.sections().is_empty() {
        content.push_str("<text:p/>");
    }
    for (s, section) in document.sections().iter().enumerate() {
        content.push_str(&format!("<text:section text:name=\"WriterSection{s}\">"));
        let mut p = 0;
        for (b, block) in section.blocks().iter().enumerate() {
            match block {
                Block::Paragraph(paragraph) => {
                    write_paragraph(&mut content, paragraph, s, p)?;
                    p += 1;
                }
                Block::Image(image) => {
                    let (path, _, _) = image_part(image, s, b);
                    content.push_str(&format!("<text:p text:style-name=\"I{s}_{b}\"><draw:frame draw:name=\"Image{s}_{b}\" text:anchor-type=\"as-char\" svg:width=\"{}\" svg:height=\"{}\"><draw:image xlink:href=\"{path}\" xlink:type=\"simple\" xlink:show=\"embed\" xlink:actuate=\"onLoad\"/><svg:desc>{}</svg:desc></draw:frame></text:p>", points(image.width()), points(image.height()), xml_escape(image.alt_text())?));
                }
                Block::Table(table) => {
                    content.push_str(&format!(
                        "<table:table table:name=\"Table{s}_{b}\" table:style-name=\"B{s}_{b}\">"
                    ));
                    for c in 0..table.rows()[0].cells().len() {
                        if table.column_widths().is_some() {
                            content.push_str(&format!(
                                "<table:table-column table:style-name=\"C{s}_{b}_{c}\"/>"
                            ));
                        } else {
                            content.push_str("<table:table-column/>");
                        }
                    }
                    for row in table.rows() {
                        content.push_str("<table:table-row>");
                        for cell in row.cells() {
                            content.push_str("<table:table-cell table:style-name=\"GridCell\" office:value-type=\"string\">");
                            for paragraph in cell.paragraphs() {
                                write_paragraph(&mut content, paragraph, s, p)?;
                                p += 1;
                            }
                            content.push_str("</table:table-cell>");
                        }
                        content.push_str("</table:table-row>");
                    }
                    content.push_str("</table:table>");
                }
            }
        }
        content.push_str("</text:section>");
    }
    content.push_str("</office:text></office:body></office:document-content>");
    let meta = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><office:document-meta {DECLARATIONS} xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><office:meta><dc:title>{}</dc:title></office:meta></office:document-meta>",
        xml_escape(&document.metadata().title)?
    );
    let mut manifest = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><manifest:manifest xmlns:manifest=\"urn:oasis:names:tc:opendocument:xmlns:manifest:1.0\" manifest:version=\"1.3\"><manifest:file-entry manifest:full-path=\"/\" manifest:media-type=\"{MEDIA_TYPE}\" manifest:version=\"1.3\"/><manifest:file-entry manifest:full-path=\"content.xml\" manifest:media-type=\"text/xml\"/><manifest:file-entry manifest:full-path=\"styles.xml\" manifest:media-type=\"text/xml\"/><manifest:file-entry manifest:full-path=\"meta.xml\" manifest:media-type=\"text/xml\"/></manifest:manifest>"
    );
    // Inventory only resources generated from complete validated model images.
    // Original packages and unused auxiliary parts are never grafted here.
    manifest.truncate(manifest.len() - "</manifest:manifest>".len());
    for (path, media, _) in &image_parts {
        manifest.push_str(&format!(
            "<manifest:file-entry manifest:full-path=\"{path}\" manifest:media-type=\"{media}\"/>"
        ));
    }
    manifest.push_str("</manifest:manifest>");
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default()
        .system(System::Unix)
        .last_modified_time(DateTime::from_date_and_time(2020, 1, 1, 0, 0, 0).map_err(error)?)
        .unix_permissions(0o644);
    writer
        .start_file(
            "mimetype",
            options.compression_method(CompressionMethod::Stored),
        )
        .map_err(error)?;
    writer.write_all(MEDIA_TYPE.as_bytes()).map_err(error)?;
    for (name, text) in [
        ("META-INF/manifest.xml", manifest),
        ("content.xml", content),
        ("styles.xml", styles),
        ("meta.xml", meta),
    ] {
        if text.len() > 2 * 1024 * 1024 {
            return Err(FormatError::new(
                "format_limit",
                "ODT XML export exceeds the supported part limit.",
            ));
        }
        writer
            .start_file(
                name,
                options
                    .compression_method(CompressionMethod::Deflated)
                    .compression_level(Some(6)),
            )
            .map_err(error)?;
        writer.write_all(text.as_bytes()).map_err(error)?;
    }
    for (name, _, data) in image_parts {
        writer
            .start_file(
                name,
                options
                    .compression_method(CompressionMethod::Deflated)
                    .compression_level(Some(6)),
            )
            .map_err(error)?;
        writer.write_all(data).map_err(error)?;
    }
    let bytes = writer.finish().map_err(error)?.into_inner();
    super::package::read(&bytes)?;
    Ok(bytes)
}

fn image_part(image: &Image, section: usize, block: usize) -> (String, &'static str, &[u8]) {
    let asset = image.asset();
    let normalize = asset.has_orientation_transform() || asset.has_color_approximation();
    let (extension, media, bytes) = if normalize {
        ("png", "image/png", asset.render_png())
    } else if asset.media_type() == "image/jpeg" {
        ("jpg", "image/jpeg", asset.original_bytes())
    } else {
        ("png", "image/png", asset.original_bytes())
    };
    (
        format!("Pictures/image{section}_{block}.{extension}"),
        media,
        bytes,
    )
}

fn write_paragraph(
    content: &mut String,
    paragraph: &Paragraph,
    s: usize,
    p: usize,
) -> Result<(), FormatError> {
    content.push_str(&format!("<text:p text:style-name=\"P{s}_{p}\">"));
    for (r, run) in paragraph.runs().iter().enumerate() {
        content.push_str(&format!("<text:span text:style-name=\"T{s}_{p}_{r}\">"));
        let mut plain = String::new();
        for character in run.text.chars() {
            if matches!(character, ' ' | '\n' | '\t' | '\r') {
                content.push_str(&xml_escape(&plain)?);
                plain.clear();
                match character {
                    ' ' => content.push_str("<text:s/>"),
                    '\n' => content.push_str("<text:line-break/>"),
                    '\t' => content.push_str("<text:tab/>"),
                    _ => {
                        return Err(FormatError::new(
                            "unsupported_odt_export",
                            "Carriage-return characters have no exact supported ODT text representation.",
                        ));
                    }
                }
            } else {
                plain.push(character);
            }
        }
        content.push_str(&xml_escape(&plain)?);
        content.push_str("</text:span>");
    }
    content.push_str("</text:p>");
    Ok(())
}
