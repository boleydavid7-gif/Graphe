use std::{collections::{BTreeMap, HashMap, HashSet}, sync::Arc};

use office_core::{
    Alignment, Block, CharacterFormat, Document, GraphemeOffset, LineSpacing, ListKind, PageMargins,
    PageSize, Paragraph, TextColor, TextCoordinates, Twips, Underline,
};
use parley::{
    AlignmentOptions, FontContext, FontFamily, FontStyle, FontWeight, LayoutContext,
    PositionedLayoutItem, StyleProperty,
    fontique::{Blob, Collection, CollectionOptions},
};
use skrifa::{
    FontRef, MetadataProvider,
    instance::{LocationRef, NormalizedCoord, Size},
    outline::{DrawSettings, OutlinePen},
};

use crate::{
    Caret, CaretAffinity, Decoration, DocumentLayout, GlyphOutline, LayoutDiagnostic, LayoutError,
    LayoutOptions, LineLayout, PageLayout, PositionedGlyph, Rect, TextRange, UNITS_PER_POINT,
    overlay_character, resolve_paragraph_style,
};

const MAX_TEXT_BYTES: usize = 200_000;
const MAX_GLYPHS: usize = 200_000;
const MAX_LINES: usize = 20_000;
const MAX_PAGES: usize = 512;
const MAX_OUTLINES: usize = 20_000;
const MAX_PATH_BYTES: usize = 16 * 1024 * 1024;
const MAX_LABEL_BYTES: usize = 1024;
const MAX_FAMILY_BYTES: usize = 256;
const MAX_TWIPS: i32 = 144_000;
const MAX_HALF_POINTS: u32 = 1024;
const CATALOG_ID: &str = "writer-fonts-v5";

struct PinnedFont {
    id: String,
    blob: Blob<u8>,
}

struct ShapeSink<'a> {
    output: &'a mut DocumentLayout,
    cache: &'a mut OutlineCache,
    glyph_count: &'a mut usize,
}

const FONT_FILES: &[(&str, &[u8])] = &[
    (
        "Arimo-Regular.ttf",
        include_bytes!("../fonts/Arimo-Regular.ttf"),
    ),
    (
        "Arimo-Italic.ttf",
        include_bytes!("../fonts/Arimo-Italic.ttf"),
    ),
    (
        "Tinos-Regular.ttf",
        include_bytes!("../fonts/Tinos-Regular.ttf"),
    ),
    (
        "Tinos-Italic.ttf",
        include_bytes!("../fonts/Tinos-Italic.ttf"),
    ),
    ("Tinos-Bold.ttf", include_bytes!("../fonts/Tinos-Bold.ttf")),
    (
        "Tinos-BoldItalic.ttf",
        include_bytes!("../fonts/Tinos-BoldItalic.ttf"),
    ),
    (
        "NotoSansArabic.ttf",
        include_bytes!("../fonts/NotoSansArabic.ttf"),
    ),
    (
        "NotoSansDevanagari.ttf",
        include_bytes!("../fonts/NotoSansDevanagari.ttf"),
    ),
    (
        "NotoSansHebrew.ttf",
        include_bytes!("../fonts/NotoSansHebrew.ttf"),
    ),
    ("NotoEmoji.ttf", include_bytes!("../fonts/NotoEmoji.ttf")),
    // Full original OFL faces. Static Caladea/Carlito have actual
    // bold/italic faces; Source Sans 3 and JetBrains Mono use wght axes.
    ("Caladea-Regular.ttf", include_bytes!("../fonts/Caladea-Regular.ttf")),
    ("Caladea-Bold.ttf", include_bytes!("../fonts/Caladea-Bold.ttf")),
    ("Caladea-Italic.ttf", include_bytes!("../fonts/Caladea-Italic.ttf")),
    ("Caladea-BoldItalic.ttf", include_bytes!("../fonts/Caladea-BoldItalic.ttf")),
    ("Carlito-Regular.ttf", include_bytes!("../fonts/Carlito-Regular.ttf")),
    ("Carlito-Bold.ttf", include_bytes!("../fonts/Carlito-Bold.ttf")),
    ("Carlito-Italic.ttf", include_bytes!("../fonts/Carlito-Italic.ttf")),
    ("Carlito-BoldItalic.ttf", include_bytes!("../fonts/Carlito-BoldItalic.ttf")),
    ("SourceSans3-Regular.ttf", include_bytes!("../fonts/SourceSans3-Regular.ttf")),
    ("SourceSans3-Italic.ttf", include_bytes!("../fonts/SourceSans3-Italic.ttf")),
    ("JetBrainsMono-Regular.ttf", include_bytes!("../fonts/JetBrainsMono-Regular.ttf")),
    ("JetBrainsMono-Italic.ttf", include_bytes!("../fonts/JetBrainsMono-Italic.ttf")),
    ("EBGaramond-Regular.ttf", include_bytes!("../fonts/EBGaramond-Regular.ttf")),
    ("EBGaramond-Italic.ttf", include_bytes!("../fonts/EBGaramond-Italic.ttf")),
    // Opt-in Open Font License additions, preserving the original fallback chain.
    ("Inter-Regular.ttf", include_bytes!("../fonts/Inter-Regular.ttf")),
    ("Inter-Italic.ttf", include_bytes!("../fonts/Inter-Italic.ttf")),
    ("LibreBaskerville-Regular.ttf", include_bytes!("../fonts/LibreBaskerville-Regular.ttf")),
    ("LibreBaskerville-Italic.ttf", include_bytes!("../fonts/LibreBaskerville-Italic.ttf")),
    ("PlayfairDisplay-Regular.ttf", include_bytes!("../fonts/PlayfairDisplay-Regular.ttf")),
    ("PlayfairDisplay-Italic.ttf", include_bytes!("../fonts/PlayfairDisplay-Italic.ttf")),
    ("SpaceGrotesk-Regular.ttf", include_bytes!("../fonts/SpaceGrotesk-Regular.ttf")),
    // v4: opt-in families with separate original upright/italic variable fonts.
    ("Lora-Regular.ttf", include_bytes!("../fonts/Lora-Regular.ttf")),
    ("Lora-Italic.ttf", include_bytes!("../fonts/Lora-Italic.ttf")),
    ("Montserrat-Regular.ttf", include_bytes!("../fonts/Montserrat-Regular.ttf")),
    ("Montserrat-Italic.ttf", include_bytes!("../fonts/Montserrat-Italic.ttf")),
    ("DMSans-Regular.ttf", include_bytes!("../fonts/DMSans-Regular.ttf")),
    ("DMSans-Italic.ttf", include_bytes!("../fonts/DMSans-Italic.ttf")),
    ("Bitter-Regular.ttf", include_bytes!("../fonts/Bitter-Regular.ttf")),
    ("Bitter-Italic.ttf", include_bytes!("../fonts/Bitter-Italic.ttf")),
    // v5: opt-in official OFL faces, preserving existing fallback order.
    ("WorkSans-Regular.ttf", include_bytes!("../fonts/WorkSans-Regular.ttf")),
    ("WorkSans-Italic.ttf", include_bytes!("../fonts/WorkSans-Italic.ttf")),
    ("CrimsonPro-Regular.ttf", include_bytes!("../fonts/CrimsonPro-Regular.ttf")),
    ("CrimsonPro-Italic.ttf", include_bytes!("../fonts/CrimsonPro-Italic.ttf")),
    ("Fraunces-Regular.ttf", include_bytes!("../fonts/Fraunces-Regular.ttf")),
    ("Fraunces-Italic.ttf", include_bytes!("../fonts/Fraunces-Italic.ttf")),
    ("RobotoMono-Regular.ttf", include_bytes!("../fonts/RobotoMono-Regular.ttf")),
    ("RobotoMono-Italic.ttf", include_bytes!("../fonts/RobotoMono-Italic.ttf")),
    ("Oswald-Regular.ttf", include_bytes!("../fonts/Oswald-Regular.ttf")),
];

/// A reusable native/WASM shaping context with an immutable pinned font catalog.
///
/// No host fonts or browser measurements are consulted. Requests have fixed
/// resource budgets; failures leave the source model and editing session intact.
pub struct LayoutEngine {
    fonts: Vec<PinnedFont>,
    external_families: HashMap<String, String>,
    external_ids: HashSet<String>,
    external_bytes: usize,
    font_context: FontContext,
    layout_context: LayoutContext<usize>,
}

impl LayoutEngine {
    /// Loads and validates the bundled catalog without filesystem or network I/O.
    pub fn new() -> Result<Self, LayoutError> {
        let mut font_context = FontContext {
            collection: Collection::new(CollectionOptions {
                system_fonts: false,
                shared: false,
            }),
            source_cache: Default::default(),
        };
        let mut fonts = Vec::new();
        for &(id, bytes) in FONT_FILES {
            let font = FontRef::from_index(bytes, 0).map_err(|error| font_error(id, error))?;
            if font
                .metrics(Size::unscaled(), LocationRef::default())
                .units_per_em
                == 0
            {
                return Err(font_error(id, "zero units per em"));
            }
            let blob = Blob::new(Arc::new(bytes));
            font_context.collection.register_fonts(blob.clone(), None);
            fonts.push(PinnedFont { id: id.into(), blob });
        }
        Ok(Self {
            fonts,
            external_families: HashMap::new(),
            external_ids: HashSet::new(),
            external_bytes: 0,
            font_context,
            layout_context: LayoutContext::new(),
        })
    }

    /// Register an owner-staged, session-only font for authoritative Rust layout.
    /// External fonts never overwrite the immutable bundled/substitution catalog.
    pub fn register_external_font(&mut self, id: &str, family: &str, bytes: &[u8]) -> Result<(), LayoutError> {
        const MAX_ONE: usize = 5 * 1024 * 1024;
        const MAX_TOTAL: usize = 24 * 1024 * 1024;
        let name = family.trim();
        if id.is_empty() || id.len() > 80 || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            || name.is_empty() || name != family || name.len() > 100
            || name.chars().any(char::is_control) || name.contains('<') || name.contains('>')
        {
            return Err(font_error(id, "invalid external font metadata"));
        }
        let folded = name.to_ascii_lowercase();
        if resolved_family(name) != "Arimo" || ["arimo", "arial", "helvetica", "sans-serif"].contains(&folded.as_str()) {
            return Err(font_error(id, "bundled font families and aliases cannot be overridden"));
        }
        if self.external_ids.contains(id) {
            return Err(font_error(id, "font already installed"));
        }
        if bytes.is_empty() || bytes.len() > MAX_ONE || self.external_ids.len() >= 24
            || self.external_bytes.saturating_add(bytes.len()) > MAX_TOTAL
            || !(bytes.starts_with(&[0, 1, 0, 0]) || bytes.starts_with(b"OTTO"))
        {
            return Err(font_error(id, "external font format, count or size limit exceeded"));
        }
        let font = FontRef::from_index(bytes, 0).map_err(|error| font_error(id, error))?;
        if font.metrics(Size::unscaled(), LocationRef::default()).units_per_em == 0 {
            return Err(font_error(id, "external font has zero units per em"));
        }
        let blob = Blob::new(Arc::new(bytes.to_vec()));
        let registered = self.font_context.collection.register_fonts(blob.clone(), None);
        if registered.is_empty() {
            return Err(font_error(id, "font was not accepted by the text shaper"));
        }
        self.fonts.push(PinnedFont { id: format!("RND:{id}:{name}"), blob });
        self.external_bytes += bytes.len();
        self.external_ids.insert(id.into());
        self.external_families.insert(folded, name.into());
        Ok(())
    }

    /// Shapes and paginates an immutable model using fixed-point output geometry.
    pub fn layout_document(
        &mut self,
        document: &Document,
        options: &LayoutOptions,
    ) -> Result<DocumentLayout, LayoutError> {
        preflight(document, options)?;
        let mut output = DocumentLayout {
            document_id: document.id().as_str().into(),
            // Every distinct external-font set produces a distinct layout
            // catalog identity, without changing the immutable v5 baseline.
            font_catalog_id: if self.external_ids.is_empty() { CATALOG_ID.into() } else {
                let mut ids: Vec<_> = self.external_ids.iter().cloned().collect();
                ids.sort();
                format!("{}+rnd:{}", CATALOG_ID, ids.join(","))
            },
            units_per_point: UNITS_PER_POINT,
            pages: Vec::new(),
            outlines: Vec::new(),
            diagnostics: Vec::new(),
        };
        let mut cache = OutlineCache::default();
        let mut glyph_count = 0;
        let mut line_count = 0;
        if document.sections().is_empty() {
            let (width, height, area) = geometry(options.page_size, options.page_margins)?;
            output.pages.push(PageLayout {
                number: 1,
                section_id: String::new(),
                width,
                height,
                content_rect: area,
                lines: Vec::new(),
            });
        }
        for section in document.sections() {
            let properties = section.properties();
            let size = properties
                .and_then(|p| p.page_size)
                .unwrap_or(options.page_size);
            let margins = properties
                .and_then(|p| p.page_margins)
                .unwrap_or(options.page_margins);
            let (page_width, page_height, area) = geometry(size, margins)?;
            push_page(
                &mut output.pages,
                section.id().as_str(),
                page_width,
                page_height,
                area,
            )?;
            let mut cursor = area.y;
            let mut previous_after = 0;
            let mut numbering = 0_usize;
            for block in section.blocks() {
                let Block::Paragraph(paragraph) = block;
                let marker = match paragraph.list() {
                    Some(ListKind::Bullet) => {
                        numbering = 0;
                        Some("•".to_owned())
                    }
                    Some(ListKind::Numbered) => {
                        numbering += 1;
                        Some(format!("{numbering}."))
                    }
                    None => {
                        numbering = 0;
                        None
                    }
                };
                let resolved = resolve_paragraph_style(document, paragraph);
                let before = twips(resolved.paragraph.spacing_before.unwrap_or(Twips(0)))?;
                let after = twips(
                    resolved
                        .paragraph
                        .spacing_after
                        .unwrap_or(options.spacing_after),
                )?;
                if resolved.paragraph.page_break_before == Some(true)
                    && output
                        .pages
                        .last()
                        .is_some_and(|page| !page.lines.is_empty())
                {
                    push_page(
                        &mut output.pages,
                        section.id().as_str(),
                        page_width,
                        page_height,
                        area,
                    )?;
                    cursor = area.y;
                    previous_after = 0;
                }
                let gap = previous_after.max(before);
                let lines = self.shape_paragraph(
                    document,
                    paragraph,
                    options,
                    area.width,
                    marker.as_deref(),
                    ShapeSink {
                        output: &mut output,
                        cache: &mut cache,
                        glyph_count: &mut glyph_count,
                    },
                )?;
                for (index, mut line) in lines.into_iter().enumerate() {
                    line_count += 1;
                    budget("lines", MAX_LINES, line_count)?;
                    if line.height > area.height {
                        return Err(LayoutError::Unsupported {
                            feature: "a line box is taller than the usable page area".into(),
                            paragraph_id: Some(paragraph.id().as_str().into()),
                        });
                    }
                    let line_gap = if index == 0 { gap } else { 0 };
                    if cursor + line_gap + line.height > area.y + area.height {
                        if line_gap > 0 {
                            diagnostic(
                                &mut output,
                                "page_spacing_suppressed",
                                paragraph,
                                "Paragraph spacing that does not fit before the first line is suppressed at the page boundary.".into(),
                            );
                        }
                        if output
                            .pages
                            .last()
                            .is_some_and(|page| !page.lines.is_empty())
                        {
                            push_page(
                                &mut output.pages,
                                section.id().as_str(),
                                page_width,
                                page_height,
                                area,
                            )?;
                        }
                        cursor = area.y;
                    } else {
                        cursor += line_gap;
                    }
                    translate_line(&mut line, area.x, cursor);
                    cursor += line.height;
                    // A page was just created above, so this is an internal invariant.
                    if let Some(page) = output.pages.last_mut() {
                        page.lines.push(line);
                    }
                }
                previous_after = after;
            }
        }
        Ok(output)
    }

    fn shape_paragraph(
        &mut self,
        document: &Document,
        paragraph: &Paragraph,
        options: &LayoutOptions,
        area_width: i64,
        marker: Option<&str>,
        sink: ShapeSink<'_>,
    ) -> Result<Vec<LineLayout>, LayoutError> {
        let ShapeSink {
            output,
            cache,
            glyph_count,
        } = sink;
        let coordinates = TextCoordinates::new(paragraph);
        let text = coordinates.text();
        let boundaries = BoundaryTable::new(&coordinates)?;
        if text.chars().any(|c| c == '\t') {
            return Err(LayoutError::Unsupported {
                feature: "tab stops are not yet implemented; source tabs are preserved".into(),
                paragraph_id: Some(paragraph.id().as_str().into()),
            });
        }
        if text
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\r' | '\n'))
        {
            return Err(LayoutError::Unsupported {
                feature: "a native control character has no layout policy".into(),
                paragraph_id: Some(paragraph.id().as_str().into()),
            });
        }
        let resolved = resolve_paragraph_style(document, paragraph);
        let indent = resolved.paragraph.indentation.clone().unwrap_or_default();
        // Marker space is part of Rust geometry and never enters document text.
        let list_indent = if marker.is_some() { quantize(38.)? } else { 0 };
        let left = twips(indent.left.unwrap_or(Twips(0)))? + list_indent;
        let right = twips(indent.right.unwrap_or(Twips(0)))?;
        let first = twips(indent.first_line.unwrap_or(Twips(0)))?;
        let width = area_width - left - right;
        if width <= 0 || width - first <= 0 {
            return Err(LayoutError::InvalidOptions {
                message: "paragraph indentation leaves no positive line width".into(),
            });
        }
        let mut formats = Vec::new();
        let mut ranges = Vec::new();
        let mut offset = 0;
        let mut base = resolved.character;
        if base.font_family.is_none() {
            base.font_family = Some(options.font_family.clone());
        }
        if base.font_size.is_none() {
            base.font_size = Some(options.font_size);
        }
        formats.push(base.clone());
        for run in paragraph.runs() {
            let mut format = base.clone();
            overlay_character(&mut format, &run.format);
            if format.hyperlink.is_some() {
                if format.underline.is_none() {
                    format.underline = Some(Underline::Single);
                }
                if format.color.is_none() {
                    format.color = Some(TextColor::Rgb { red: 23, green: 98, blue: 166 });
                }
            }
            formats.push(format);
            ranges.push((offset..offset + run.text.len(), formats.len() - 1));
            offset += run.text.len();
        }
        let families: Vec<String> = formats
            .iter()
            .map(|format| {
                let name = format.font_family.as_deref().unwrap_or("Arimo");
                self.external_families.get(&name.to_ascii_lowercase()).cloned()
                    .unwrap_or_else(|| resolved_family(name).into())
            })
            .collect();
        for (format, family) in formats.iter().zip(&families) {
            let requested = format.font_family.as_deref().unwrap_or("Arimo");
            if !requested.eq_ignore_ascii_case(family) {
                diagnostic(
                    output,
                    "font_substitution",
                    paragraph,
                    format!("Requested font {requested:?} uses pinned {family}."),
                );
            }
        }
        let mut builder =
            self.layout_context
                .ranged_builder(&mut self.font_context, text, 1., false);
        builder.push_default(StyleProperty::FontFamily(FontFamily::Source(
            font_list(families[0].as_str()).into(),
        )));
        builder.push_default(StyleProperty::FontSize(
            formats[0].font_size.unwrap_or(options.font_size).0 as f32 / 2.,
        ));
        builder.push_default(StyleProperty::FontWeight(FontWeight::new(
            if formats[0].bold == Some(true) {
                700.
            } else {
                400.
            },
        )));
        builder.push_default(StyleProperty::FontStyle(
            if formats[0].italic == Some(true) {
                FontStyle::Italic
            } else {
                FontStyle::Normal
            },
        ));
        builder.push_default(StyleProperty::Brush(0));
        for (range, index) in ranges {
            let format = &formats[index];
            builder.push(
                StyleProperty::FontFamily(FontFamily::Source(font_list(families[index].as_str()).into())),
                range.clone(),
            );
            builder.push(
                StyleProperty::FontSize(
                    format.font_size.unwrap_or(options.font_size).0 as f32 / 2.,
                ),
                range.clone(),
            );
            builder.push(
                StyleProperty::FontWeight(FontWeight::new(if format.bold == Some(true) {
                    700.
                } else {
                    400.
                })),
                range.clone(),
            );
            builder.push(
                StyleProperty::FontStyle(if format.italic == Some(true) {
                    FontStyle::Italic
                } else {
                    FontStyle::Normal
                }),
                range.clone(),
            );
            builder.push(StyleProperty::Brush(index), range);
        }
        let mut layout = builder.build(text);
        layout.set_alignment(
            match resolved.paragraph.alignment.unwrap_or(Alignment::Left) {
                Alignment::Left => parley::Alignment::Left,
                Alignment::Center => parley::Alignment::Center,
                Alignment::Right => parley::Alignment::Right,
                Alignment::Justify => parley::Alignment::Justify,
            },
            AlignmentOptions::default(),
        );
        let mut breaker = layout.break_lines();
        // A hanging first line can legitimately be wider than subsequent lines.
        // Parley's global maximum must cover every per-line width; it is not a
        // substitute for the individual line's actual available width.
        breaker
            .state_mut()
            .set_layout_max_advance(points(width.max(width - first)));
        let mut index = 0;
        loop {
            breaker
                .state_mut()
                .set_line_x(points(left + if index == 0 { first } else { 0 }));
            breaker
                .state_mut()
                .set_line_max_advance(points(width - if index == 0 { first } else { 0 }));
            if breaker.break_next().is_none() {
                break;
            }
            index += 1;
            budget("paragraph_lines", MAX_LINES, index)?;
        }
        breaker.finish();
        let mut result = Vec::new();
        for (line_index, line) in layout.lines().enumerate() {
            let source = line.text_range();
            let start = boundaries.at(source.start)?;
            let end = boundaries.at(source.end)?;
            let metrics = line.metrics();
            let natural = metrics.block_max_coord - metrics.block_min_coord;
            let spacing = resolved
                .paragraph
                .line_spacing
                .unwrap_or(options.line_spacing);
            let height = match spacing {
                LineSpacing::Multiple { value } => natural * value as f32 / 240.,
                LineSpacing::Exact { height } => height.0 as f32 / 20.,
                LineSpacing::AtLeast { height } => natural.max(height.0 as f32 / 20.),
            };
            if matches!(spacing, LineSpacing::Exact { .. }) && height < natural {
                diagnostic(
                    output,
                    "exact_line_ink_overflow",
                    paragraph,
                    "Exact line spacing is smaller than natural font metrics; centered glyph ink can extend beyond the line box without clipping.".into(),
                );
            }
            let height = quantize(height)?;
            if height <= 0 {
                return Err(LayoutError::Unsupported {
                    feature: "line spacing is below the fixed-point layout resolution".into(),
                    paragraph_id: Some(paragraph.id().as_str().into()),
                });
            }
            let baseline = quantize(metrics.baseline - metrics.block_min_coord)?
                + (height - quantize(natural)?) / 2;
            let baseline_delta = baseline - quantize(metrics.baseline)?;
            let actual_advance = (line.runs().map(|run| run.advance()).sum::<f32>()
                - metrics.hanging_advance)
                .max(0.);
            let mut shaped = LineLayout {
                paragraph_id: paragraph.id().as_str().into(),
                line_index: line_index as u32,
                text: text
                    .get(source.clone())
                    .ok_or_else(|| bad_mapping(paragraph))?
                    .into(),
                text_range: TextRange {
                    start_utf8: start.byte,
                    end_utf8: end.byte,
                    start_utf16: start.utf16,
                    end_utf16: end.utf16,
                    start_grapheme: start.grapheme,
                    end_grapheme: end.grapheme,
                },
                x: quantize(metrics.inline_min_coord + metrics.offset)?,
                y: 0,
                width: quantize(actual_advance)?,
                height,
                baseline,
                glyphs: Vec::new(),
                carets: Vec::new(),
                decorations: Vec::new(),
            };
            if actual_advance > points(width - if line_index == 0 { first } else { 0 }) + 0.02 {
                diagnostic(
                    output,
                    "line_overflow",
                    paragraph,
                    "An unbreakable span extends beyond its available line width.".into(),
                );
            }
            let mut caret_candidates = BTreeMap::new();
            let mut cluster_x = metrics.inline_min_coord + metrics.offset;
            for run in line.runs() {
                for cluster in run.visual_clusters() {
                    let range = cluster.text_range();
                    let advance = cluster.advance();
                    for (byte, affinity, x) in [
                        (
                            range.start,
                            CaretAffinity::Leading,
                            cluster_x + if cluster.is_rtl() { advance } else { 0. },
                        ),
                        (
                            range.end,
                            CaretAffinity::Trailing,
                            cluster_x + if cluster.is_rtl() { 0. } else { advance },
                        ),
                    ] {
                        if let Some(boundary) = boundaries.get(byte) {
                            caret_candidates.insert(
                                (byte, affinity),
                                Caret {
                                    utf8: byte,
                                    utf16: boundary.utf16,
                                    grapheme: boundary.grapheme,
                                    x: quantize(x)?,
                                    y: 0,
                                    height,
                                    affinity,
                                },
                            );
                        }
                    }
                    cluster_x += advance;
                }
            }
            for (key, caret) in &caret_candidates {
                if key.1 == CaretAffinity::Trailing
                    && caret_candidates
                        .get(&(key.0, CaretAffinity::Leading))
                        .is_some_and(|other| other.x == caret.x)
                {
                    continue;
                }
                shaped.carets.push(caret.clone());
            }
            if shaped.carets.is_empty() && source.is_empty() {
                shaped.carets.push(Caret {
                    utf8: start.byte,
                    utf16: start.utf16,
                    grapheme: start.grapheme,
                    x: shaped.x,
                    y: 0,
                    height,
                    affinity: CaretAffinity::Leading,
                });
            }
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                    continue;
                };
                let run = glyph_run.run();
                let font_data = run.font();
                let pinned = self
                    .fonts
                    .iter()
                    .find(|entry| entry.blob.id() == font_data.data.id())
                    .ok_or_else(|| font_error("unknown", "font outside pinned catalog"))?;
                let format = &formats[glyph_run.style().brush];
                let color = color(format);
                let synthesis = run.synthesis();
                if synthesis.embolden() {
                    diagnostic(output, "style_unavailable", paragraph, "A fallback face has no genuine requested bold weight; faux emboldening is not applied.".into());
                }
                if synthesis.skew().is_some() {
                    diagnostic(
                        output,
                        "synthetic_italic",
                        paragraph,
                        "A fallback italic is explicitly synthesized from an upright pinned face."
                            .into(),
                    );
                }
                for glyph in glyph_run.positioned_glyphs() {
                    *glyph_count += 1;
                    budget("glyphs", MAX_GLYPHS, *glyph_count)?;
                    if glyph.id == 0 {
                        diagnostic(output, "missing_glyph", paragraph, "No pinned font covers part of this text; its missing-glyph outline is shown.".into());
                    }
                    let normalized: Vec<i16> = run
                        .normalized_coords()
                        .iter()
                        .map(|coordinate| coordinate.to_bits())
                        .collect();
                    let outline_id =
                        cache.outline(output, pinned, font_data.index, glyph.id, &normalized)?;
                    let units = output.outlines[outline_id as usize].units_per_em;
                    let x = quantize(glyph.x)?;
                    let y = quantize(glyph.y)? + baseline_delta;
                    let size = quantize(run.font_size())?;
                    shaped.glyphs.push(PositionedGlyph {
                        outline_id,
                        x,
                        y,
                        font_size: size,
                        transform: glyph_transform(x, y, size, units, synthesis.skew()),
                        color: color.clone(),
                    });
                }
                if matches!(
                    format.underline,
                    Some(Underline::Single | Underline::Double)
                ) {
                    let font_metrics = run.font_metrics();
                    let thickness = quantize(font_metrics.underline_size)?.max(1);
                    // OpenType underline offsets use upward-positive font axes.
                    let y = quantize(glyph_run.baseline() - font_metrics.underline_offset)?
                        + baseline_delta;
                    let underline = Decoration {
                        x: quantize(glyph_run.offset())?,
                        y,
                        width: quantize(glyph_run.advance())?,
                        thickness,
                        color: color.clone(),
                    };
                    shaped.decorations.push(underline.clone());
                    if format.underline == Some(Underline::Double) {
                        shaped.decorations.push(Decoration {
                            y: y + thickness * 2,
                            ..underline
                        });
                    }
                }
            }
            result.push(shaped);
        }
        // Line geometry is fully owned before making the second pinned-font
        // layout for the decorative marker; native text offsets stay unchanged.
        if let Some(marker) = marker {
            if let Some(first_line) = result.first_mut() {
                let size = formats[0].font_size.unwrap_or(options.font_size).0 as f32 / 2.;
                self.shape_list_marker(
                    marker, first_line, size, output, cache, glyph_count, paragraph,
                )?;
            }
        }
        Ok(result)
    }

    /// Shapes decorative list markers into pinned-font SVG outlines without
    /// creating selectable text or shifting paragraph Unicode positions.
    fn shape_list_marker(
        &mut self,
        marker: &str,
        line: &mut LineLayout,
        font_size: f32,
        output: &mut DocumentLayout,
        cache: &mut OutlineCache,
        glyph_count: &mut usize,
        paragraph: &Paragraph,
    ) -> Result<(), LayoutError> {
        let mut builder = self.layout_context.ranged_builder(
            &mut self.font_context, marker, 1., false,
        );
        builder.push_default(StyleProperty::FontFamily(
            FontFamily::Source(font_list("Arimo").into()),
        ));
        builder.push_default(StyleProperty::FontSize(font_size));
        builder.push_default(StyleProperty::Brush(0));
        let mut layout = builder.build(marker);
        let mut breaker = layout.break_lines();
        breaker.state_mut().set_layout_max_advance(1000.);
        while breaker.break_next().is_some() {}
        breaker.finish();
        let Some(mark_line) = layout.lines().next() else {
            return Ok(());
        };
        let advance = mark_line.runs().map(|run| run.advance()).sum::<f32>();
        let start_x = line.x - quantize(7.)? - quantize(advance)?;
        if start_x < 0 {
            return Err(LayoutError::InvalidOptions {
                message: "list marker exceeds the available paragraph indent".into(),
            });
        }
        for item in mark_line.items() {
            let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                continue;
            };
            let run = glyph_run.run();
            let font_data = run.font();
            let pinned = self.fonts.iter()
                .find(|entry| entry.blob.id() == font_data.data.id())
                .ok_or_else(|| font_error("unknown", "list marker outside pinned catalog"))?;
            let synthesis = run.synthesis();
            let normalized: Vec<i16> = run.normalized_coords().iter()
                .map(|coordinate| coordinate.to_bits()).collect();
            let marker_baseline = quantize(glyph_run.baseline())?;
            for glyph in glyph_run.positioned_glyphs() {
                *glyph_count += 1;
                budget("glyphs", MAX_GLYPHS, *glyph_count)?;
                if glyph.id == 0 {
                    diagnostic(output, "missing_glyph", paragraph,
                        "The pinned font has no glyph for the list marker.".into());
                }
                let outline_id =
                    cache.outline(output, pinned, font_data.index, glyph.id, &normalized)?;
                let units = output.outlines[outline_id as usize].units_per_em;
                let x = start_x + quantize(glyph.x)?;
                let y = line.baseline - marker_baseline + quantize(glyph.y)?;
                let size = quantize(run.font_size())?;
                line.glyphs.push(PositionedGlyph {
                    outline_id,
                    x,
                    y,
                    font_size: size,
                    transform: glyph_transform(x, y, size, units, synthesis.skew()),
                    color: "#24211d".into(),
                });
            }
        }
        Ok(())
    }
}

/// Shapes and paginates once using the same pinned engine as native/WASM sessions.
pub fn layout_document(
    document: &Document,
    options: &LayoutOptions,
) -> Result<DocumentLayout, LayoutError> {
    LayoutEngine::new()?.layout_document(document, options)
}

fn preflight(document: &Document, options: &LayoutOptions) -> Result<(), LayoutError> {
    geometry(options.page_size, options.page_margins)?;
    validate_font(options.font_size.0, &options.font_family)?;
    validate_spacing(options.line_spacing)?;
    twips(options.spacing_after)?;
    if options.spacing_after.0 < 0 {
        return Err(invalid("negative default paragraph spacing"));
    }
    budget(
        "document_id_bytes",
        MAX_LABEL_BYTES,
        document.id().as_str().len(),
    )?;
    budget("sections", 2000, document.sections().len())?;
    budget("styles", 128, document.styles().len())?;
    let mut paragraphs = 0;
    let mut bytes = 0;
    let mut utf16 = 0;
    let mut runs = 0;
    for style in document.styles() {
        budget("style_id_bytes", MAX_LABEL_BYTES, style.id.as_str().len())?;
        if let Some(size) = style.character_format.font_size {
            validate_font(
                size.0,
                style
                    .character_format
                    .font_family
                    .as_deref()
                    .unwrap_or("Arimo"),
            )?;
        }
        if let Some(family) = &style.character_format.font_family {
            budget("font_family_bytes", MAX_FAMILY_BYTES, family.len())?;
        }
        let mut depth = 0;
        let mut next = style.based_on.as_ref();
        while let Some(id) = next {
            depth += 1;
            budget("style_depth", 32, depth)?;
            next = document
                .styles()
                .iter()
                .find(|style| style.id == *id)
                .and_then(|style| style.based_on.as_ref());
        }
    }
    for section in document.sections() {
        budget(
            "section_id_bytes",
            MAX_LABEL_BYTES,
            section.id().as_str().len(),
        )?;
        for block in section.blocks() {
            paragraphs += 1;
            budget("paragraphs", 2000, paragraphs)?;
            let Block::Paragraph(paragraph) = block;
            budget(
                "paragraph_id_bytes",
                MAX_LABEL_BYTES,
                paragraph.id().as_str().len(),
            )?;
            let format = resolve_paragraph_style(document, paragraph);
            validate_font(
                format.character.font_size.unwrap_or(options.font_size).0,
                format
                    .character
                    .font_family
                    .as_deref()
                    .unwrap_or(&options.font_family),
            )?;
            if let Some(spacing) = format.paragraph.line_spacing {
                validate_spacing(spacing)?;
            }
            for value in [
                format.paragraph.spacing_before,
                format.paragraph.spacing_after,
            ]
            .into_iter()
            .flatten()
            {
                twips(value)?;
            }
            if let Some(indent) = &format.paragraph.indentation {
                for value in [indent.left, indent.right, indent.first_line]
                    .into_iter()
                    .flatten()
                {
                    twips(value)?;
                }
            }
            for run in paragraph.runs() {
                if let Some(family) = &run.format.font_family {
                    budget("font_family_bytes", MAX_FAMILY_BYTES, family.len())?;
                }
                runs += 1;
                budget("runs", 20_000, runs)?;
                bytes += run.text.len();
                budget("text_bytes", MAX_TEXT_BYTES, bytes)?;
                utf16 += run.text.encode_utf16().count();
                budget("utf16_units", 200_000, utf16)?;
                let mut effective = format.character.clone();
                overlay_character(&mut effective, &run.format);
                validate_font(
                    effective.font_size.unwrap_or(options.font_size).0,
                    effective
                        .font_family
                        .as_deref()
                        .unwrap_or(&options.font_family),
                )?;
            }
        }
    }
    document
        .validate()
        .map_err(|error| LayoutError::InvalidDocument {
            message: error.to_string(),
        })
}

fn geometry(size: PageSize, margins: PageMargins) -> Result<(i64, i64, Rect), LayoutError> {
    if size.width.0 <= 0 || size.height.0 <= 0 {
        return Err(invalid("page dimensions must be positive"));
    }
    for value in [
        margins.top,
        margins.right,
        margins.bottom,
        margins.left,
        margins.header,
        margins.footer,
        margins.gutter,
    ] {
        if value.0 < 0 {
            return Err(invalid("page margins must be nonnegative"));
        }
        twips(value)?;
    }
    let width = twips(size.width)?;
    let height = twips(size.height)?;
    let x = twips(margins.left)? + twips(margins.gutter)?;
    let y = twips(margins.top)?;
    let area = Rect {
        x,
        y,
        width: width - x - twips(margins.right)?,
        height: height - y - twips(margins.bottom)?,
    };
    if area.width <= 0 || area.height <= 0 {
        return Err(invalid("margins leave no positive page body area"));
    }
    Ok((width, height, area))
}

fn validate_font(size: u32, family: &str) -> Result<(), LayoutError> {
    if size == 0 || family.trim().is_empty() {
        return Err(invalid("font size/family must be positive/nonblank"));
    }
    budget("font_half_points", MAX_HALF_POINTS as usize, size as usize)?;
    budget("font_family_bytes", MAX_FAMILY_BYTES, family.len())
}

fn validate_spacing(spacing: LineSpacing) -> Result<(), LayoutError> {
    match spacing {
        LineSpacing::Multiple { value } => {
            if value == 0 {
                return Err(invalid("line spacing must be positive"));
            }
            budget("line_multiple_240", 4800, value as usize)
        }
        LineSpacing::Exact { height } | LineSpacing::AtLeast { height } => {
            if height.0 <= 0 {
                return Err(invalid("line height must be positive"));
            }
            twips(height).map(|_| ())
        }
    }
}

fn resolved_family(requested: &str) -> &'static str {
    match requested.to_ascii_lowercase().as_str() {
        // These compatibility names are retained in the document; only glyph
        // selection uses their pinned, freely redistributable substitutes.
        "arimo" | "arial" | "helvetica" | "sans-serif" => "Arimo",
        "tinos" | "times" | "times new roman" | "serif" => "Tinos",
        "caladea" | "cambria" => "Caladea",
        "carlito" | "calibri" => "Carlito",
        "source sans 3" | "source sans pro" => "Source Sans 3",
        "jetbrains mono" => "JetBrains Mono",
        "eb garamond" => "EB Garamond",
        "inter" => "Inter",
        "libre baskerville" => "Libre Baskerville",
        "playfair display" => "Playfair Display",
        "space grotesk" => "Space Grotesk",
        "lora" => "Lora",
        "montserrat" => "Montserrat",
        "dm sans" => "DM Sans",
        "bitter" => "Bitter",
        "work sans" => "Work Sans",
        "crimson pro" => "Crimson Pro",
        "fraunces" => "Fraunces",
        "roboto mono" => "Roboto Mono",
        "oswald" => "Oswald",
        "noto sans arabic" | "noto naskh arabic" => "Noto Sans Arabic",
        "noto sans devanagari" => "Noto Sans Devanagari",
        "noto sans hebrew" => "Noto Sans Hebrew",
        "noto emoji" => "Noto Emoji",
        _ => "Arimo",
    }
}

fn font_list(primary: &str) -> String {
    format!(
        "{primary}, Arimo, Tinos, Noto Sans Arabic, Noto Sans Devanagari, Noto Sans Hebrew, Noto Emoji"
    )
}

fn color(format: &CharacterFormat) -> String {
    match format.color {
        Some(TextColor::Rgb { red, green, blue }) => format!("#{red:02x}{green:02x}{blue:02x}"),
        _ => "#24211d".into(),
    }
}

fn push_page(
    pages: &mut Vec<PageLayout>,
    section: &str,
    width: i64,
    height: i64,
    content_rect: Rect,
) -> Result<(), LayoutError> {
    budget("pages", MAX_PAGES, pages.len() + 1)?;
    pages.push(PageLayout {
        number: pages.len() as u32 + 1,
        section_id: section.into(),
        width,
        height,
        content_rect,
        lines: Vec::new(),
    });
    Ok(())
}

fn translate_line(line: &mut LineLayout, x: i64, y: i64) {
    line.x += x;
    line.y += y;
    line.baseline += y;
    for caret in &mut line.carets {
        caret.x += x;
        caret.y += y;
    }
    for decoration in &mut line.decorations {
        decoration.x += x;
        decoration.y += y;
    }
    // Prefixing an existing transform keeps the outline's scale/skew unchanged.
    for glyph in &mut line.glyphs {
        glyph.x += x;
        glyph.y += y;
        glyph.transform = format!("translate({x} {y}) {}", glyph.transform);
    }
}

fn glyph_transform(x: i64, y: i64, size: i64, units: u16, skew: Option<f32>) -> String {
    let scale = size as f64 / f64::from(units);
    let skew = skew
        .map(|angle| format!(" skewX({angle:.3})"))
        .unwrap_or_default();
    format!("translate({x} {y}) scale({scale:.9} -{scale:.9}){skew}")
}

fn twips(value: Twips) -> Result<i64, LayoutError> {
    let magnitude = i64::from(value.0).unsigned_abs();
    if magnitude > MAX_TWIPS as u64 {
        return Err(LayoutError::LimitExceeded {
            resource: "absolute_twips",
            limit: MAX_TWIPS as usize,
            actual: magnitude as usize,
        });
    }
    Ok((i64::from(value.0) * UNITS_PER_POINT + if value.0 >= 0 { 10 } else { -10 }) / 20)
}

fn points(value: i64) -> f32 {
    value as f32 / UNITS_PER_POINT as f32
}

fn quantize(value: f32) -> Result<i64, LayoutError> {
    if !value.is_finite() || value.abs() > 1_000_000_000. {
        return Err(invalid("nonfinite or excessive geometry"));
    }
    Ok(libm::round(f64::from(value) * UNITS_PER_POINT as f64) as i64)
}

fn invalid(message: &str) -> LayoutError {
    LayoutError::InvalidOptions {
        message: message.into(),
    }
}
fn font_error(font_id: &str, message: impl std::fmt::Display) -> LayoutError {
    LayoutError::Font {
        font_id: font_id.into(),
        message: message.to_string(),
    }
}
fn bad_mapping(paragraph: &Paragraph) -> LayoutError {
    LayoutError::Unsupported {
        feature: "a shaped line boundary differs from the core grapheme boundary".into(),
        paragraph_id: Some(paragraph.id().as_str().into()),
    }
}
fn budget(resource: &'static str, limit: usize, actual: usize) -> Result<(), LayoutError> {
    if actual > limit {
        Err(LayoutError::LimitExceeded {
            resource,
            limit,
            actual,
        })
    } else {
        Ok(())
    }
}

fn diagnostic(output: &mut DocumentLayout, kind: &str, paragraph: &Paragraph, message: String) {
    if output.diagnostics.iter().any(|entry| {
        entry.kind == kind
            && entry.paragraph_id.as_deref() == Some(paragraph.id().as_str())
            && entry.message == message
    }) {
        return;
    }
    if output.diagnostics.len() >= 127 {
        if output.diagnostics.len() == 127 {
            output.diagnostics.push(LayoutDiagnostic {
                kind: "diagnostics_truncated".into(),
                paragraph_id: None,
                message: "Additional layout fidelity diagnostics were omitted after the fixed report limit.".into(),
            });
        }
        return;
    }
    output.diagnostics.push(LayoutDiagnostic {
        kind: kind.into(),
        paragraph_id: Some(paragraph.id().as_str().into()),
        message,
    });
}

struct Boundary {
    byte: usize,
    utf16: usize,
    grapheme: usize,
}
struct BoundaryTable(Vec<Boundary>);
impl BoundaryTable {
    fn new(coordinates: &TextCoordinates) -> Result<Self, LayoutError> {
        let offsets: Vec<_> = (0..=coordinates.grapheme_len().get())
            .map(|index| {
                coordinates
                    .grapheme_to_utf8(GraphemeOffset::new(index))
                    .map(|value| value.get())
            })
            .collect::<Result<_, _>>()
            .map_err(|error| invalid(&error.to_string()))?;
        let mut boundaries = Vec::with_capacity(offsets.len());
        let mut next = 0;
        let mut utf16 = 0;
        for (byte, character) in coordinates.text().char_indices() {
            if offsets.get(next) == Some(&byte) {
                boundaries.push(Boundary {
                    byte,
                    utf16,
                    grapheme: next,
                });
                next += 1;
            }
            utf16 += character.len_utf16();
        }
        boundaries.push(Boundary {
            byte: coordinates.text().len(),
            utf16,
            grapheme: next,
        });
        Ok(Self(boundaries))
    }
    fn get(&self, byte: usize) -> Option<&Boundary> {
        self.0
            .binary_search_by_key(&byte, |entry| entry.byte)
            .ok()
            .map(|index| &self.0[index])
    }
    fn at(&self, byte: usize) -> Result<&Boundary, LayoutError> {
        self.get(byte)
            .ok_or_else(|| invalid("Parley line split is not a core grapheme boundary"))
    }
}

#[derive(Default)]
struct OutlineCache {
    indices: BTreeMap<String, u32>,
    total_path_bytes: usize,
}
impl OutlineCache {
    fn outline(
        &mut self,
        output: &mut DocumentLayout,
        pinned: &PinnedFont,
        face: u32,
        glyph: u32,
        coordinates: &[i16],
    ) -> Result<u32, LayoutError> {
        let key = format!("{}:{face}:{glyph}:{coordinates:?}", pinned.id);
        if let Some(index) = self.indices.get(&key) {
            return Ok(*index);
        }
        budget("outlines", MAX_OUTLINES, output.outlines.len() + 1)?;
        let font = FontRef::from_index(pinned.blob.as_ref(), face)
            .map_err(|error| font_error(&pinned.id, error))?;
        let location: Vec<_> = coordinates
            .iter()
            .map(|coordinate| NormalizedCoord::from_bits(*coordinate))
            .collect();
        let location = LocationRef::new(&location);
        let units = font.metrics(Size::unscaled(), location).units_per_em;
        let mut pen = SvgPen::default();
        if let Some(outline) = font.outline_glyphs().get(skrifa::GlyphId::new(glyph)) {
            outline
                .draw(DrawSettings::unhinted(Size::unscaled(), location), &mut pen)
                .map_err(|error| font_error(&pinned.id, error))?;
        }
        self.total_path_bytes += pen.0.len();
        budget("outline_path_bytes", MAX_PATH_BYTES, self.total_path_bytes)?;
        let id = output.outlines.len() as u32;
        output.outlines.push(GlyphOutline {
            id,
            font_id: pinned.id.clone(),
            glyph_id: glyph,
            units_per_em: units,
            path: pen.0,
        });
        self.indices.insert(key, id);
        Ok(id)
    }
}

#[derive(Default)]
struct SvgPen(String);
fn coordinate(value: f32) -> String {
    let rounded = libm::round(f64::from(value) * 1000.) / 1000.;
    let mut text = format!("{rounded:.3}");
    while text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    if text == "-0" { "0".into() } else { text }
}
impl OutlinePen for SvgPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0
            .push_str(&format!("M{} {}", coordinate(x), coordinate(y)));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0
            .push_str(&format!("L{} {}", coordinate(x), coordinate(y)));
    }
    fn quad_to(&mut self, x: f32, y: f32, x1: f32, y1: f32) {
        self.0.push_str(&format!(
            "Q{} {} {} {}",
            coordinate(x),
            coordinate(y),
            coordinate(x1),
            coordinate(y1)
        ));
    }
    fn curve_to(&mut self, x: f32, y: f32, x1: f32, y1: f32, x2: f32, y2: f32) {
        self.0.push_str(&format!(
            "C{} {} {} {} {} {}",
            coordinate(x),
            coordinate(y),
            coordinate(x1),
            coordinate(y1),
            coordinate(x2),
            coordinate(y2)
        ));
    }
    fn close(&mut self) {
        self.0.push('Z');
    }
}
