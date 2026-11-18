//! Fixed-layout PDF output from the existing Rust glyph/geometry projection.
//!
//! Glyphs are reusable vector forms, not a new shaping or pagination system.
//! This first profile deliberately does not promise searchable text, semantic
//! tagging, PDF/UA or PDF/A; the format facade reports those limits before save.

use std::collections::BTreeMap;

use office_core::Document;
use office_layout::{DocumentLayout, LayoutOptions, layout_document};
use pdf_writer::{Content, Name, Pdf, Rect, Ref, Settings, TextStr};
use svgtypes::{PathParser, PathSegment, Transform};

use crate::{FormatDiagnostic, FormatError};

const MAX_PDF_BYTES: usize = 16 * 1024 * 1024;
const MAX_COORDINATE: f64 = 1_000_000_000.;

pub(crate) fn assess_document(document: &Document) -> Result<Vec<FormatDiagnostic>, FormatError> {
    let layout = layout(document)?;
    Ok(layout
        .diagnostics
        .into_iter()
        .map(|diagnostic| {
            FormatDiagnostic::new(
                format!("pdf_layout_{}", diagnostic.kind),
                "layout",
                diagnostic.paragraph_id.unwrap_or_default(),
                diagnostic.message,
            )
        })
        .collect())
}

pub(crate) fn export_document(document: &Document) -> Result<Vec<u8>, FormatError> {
    let layout = layout(document)?;
    write_layout(&layout, &document.metadata().title)
}

fn layout(document: &Document) -> Result<DocumentLayout, FormatError> {
    document.validate()?;
    layout_document(document, &LayoutOptions::default())
        .map_err(|error| FormatError::new("pdf_layout", error.to_string()))
}

fn write_layout(layout: &DocumentLayout, title: &str) -> Result<Vec<u8>, FormatError> {
    check_size(title.len().saturating_mul(4))?;
    if layout.units_per_point != office_layout::UNITS_PER_POINT || layout.pages.is_empty() {
        return Err(invalid("Invalid PDF layout units or page collection."));
    }
    let settings = Settings { pretty: false };
    let mut pdf = Pdf::with_settings(settings);
    let catalog = Ref::new(1);
    let page_tree = Ref::new(2);
    let info = Ref::new(3);
    pdf.catalog(catalog).pages(page_tree);
    pdf.document_info(info)
        .title(TextStr(title))
        .creator(TextStr("Writer"))
        .producer(TextStr("Office Engine Writer"));

    // References are allocated once in deterministic outline/page order; no
    // clock, random document identifier or original-source bytes enter the PDF.
    let page_start = 4 + layout.outlines.len();
    let content_start = page_start + layout.pages.len();
    let page_refs = (0..layout.pages.len())
        .map(|index| reference(page_start + index))
        .collect::<Result<Vec<_>, _>>()?;
    pdf.pages(page_tree)
        .kids(page_refs.iter().copied())
        .count(i32::try_from(page_refs.len()).map_err(|_| size_error())?);

    let mut forms = BTreeMap::new();
    for (index, outline) in layout.outlines.iter().enumerate() {
        let form = reference(4 + index)?;
        let name = format!("g{}", outline.id);
        if forms.insert(outline.id, (form, name)).is_some() {
            return Err(invalid("Duplicate PDF glyph outline identifier."));
        }
        let (path, bounds) = outline_content(&outline.path)?;
        check_size(pdf.len().saturating_add(path.len()))?;
        pdf.form_xobject(form, &path).bbox(bounds).resources();
    }

    let units = layout.units_per_point as f64;
    for (index, page) in layout.pages.iter().enumerate() {
        if page.width <= 0 || page.height <= 0 {
            return Err(invalid("PDF page dimensions must be positive."));
        }
        let width = number(page.width as f64 / units)?;
        let height = number(page.height as f64 / units)?;
        let mut content = Content::with_settings(settings);
        // The shared layout is top-left/downward in fixed units. This single
        // page matrix converts it into PDF bottom-left/upward points. Supplied
        // glyph transforms already include their font-y flip and synthesis.
        content.transform([1. / units as f32, 0., 0., -1. / units as f32, 0., height]);
        // Borders use the same authoritative Rust cell rectangles as the editor.
        for cell in &page.table_cells {
            if cell.rect.width <= 0 || cell.rect.height <= 0 {
                return Err(invalid("Invalid PDF table cell geometry."));
            }
            content.set_stroke_rgb(128. / 255., 128. / 255., 128. / 255.);
            content.set_line_width((units / 2.) as f32);
            content.rect(
                number(cell.rect.x as f64)?,
                number(cell.rect.y as f64)?,
                number(cell.rect.width as f64)?,
                number(cell.rect.height as f64)?,
            );
            content.stroke();
            check_size(pdf.len().saturating_add(content.len()))?;
        }
        let mut used_forms = BTreeMap::new();
        for line in &page.lines {
            for glyph in &line.glyphs {
                let (form, name) = forms
                    .get(&glyph.outline_id)
                    .ok_or_else(|| invalid("PDF glyph refers to an absent outline."))?;
                let transform: Transform = glyph
                    .transform
                    .parse()
                    .map_err(|_| invalid("Invalid Rust glyph transform for PDF output."))?;
                let matrix = [
                    number(transform.a)?,
                    number(transform.b)?,
                    number(transform.c)?,
                    number(transform.d)?,
                    number(transform.e)?,
                    number(transform.f)?,
                ];
                let [red, green, blue] = color(&glyph.color)?;
                content.save_state();
                content.set_fill_rgb(red, green, blue);
                content.transform(matrix);
                content.x_object(Name(name.as_bytes()));
                content.restore_state();
                used_forms.insert(glyph.outline_id, (*form, name));
                check_size(pdf.len().saturating_add(content.len()))?;
            }
            for decoration in &line.decorations {
                let [red, green, blue] = color(&decoration.color)?;
                if decoration.width < 0 || decoration.thickness < 0 {
                    return Err(invalid("Invalid PDF decoration geometry."));
                }
                content.set_fill_rgb(red, green, blue);
                content.rect(
                    number(decoration.x as f64)?,
                    number(decoration.y as f64)?,
                    number(decoration.width as f64)?,
                    number(decoration.thickness as f64)?,
                );
                content.fill_nonzero();
                check_size(pdf.len().saturating_add(content.len()))?;
            }
        }
        let content_ref = reference(content_start + index)?;
        {
            let mut pdf_page = pdf.page(page_refs[index]);
            pdf_page
                .parent(page_tree)
                .media_box(Rect::new(0., 0., width, height))
                .contents(content_ref);
            let mut resources = pdf_page.resources();
            let mut x_objects = resources.x_objects();
            for (form, name) in used_forms.values() {
                x_objects.pair(Name(name.as_bytes()), *form);
            }
        }
        pdf.stream(content_ref, &content.finish());
        check_size(pdf.len())?;
    }
    let bytes = pdf.finish();
    check_size(bytes.len())?;
    Ok(bytes)
}

fn reference(index: usize) -> Result<Ref, FormatError> {
    let index = i32::try_from(index).map_err(|_| size_error())?;
    if index <= 0 {
        return Err(size_error());
    }
    Ok(Ref::new(index))
}

fn number(value: f64) -> Result<f32, FormatError> {
    if !value.is_finite() || value.abs() > MAX_COORDINATE {
        return Err(invalid("Nonfinite or excessive PDF geometry."));
    }
    Ok(value as f32)
}

fn color(value: &str) -> Result<[f32; 3], FormatError> {
    if value.len() != 7 || !value.starts_with('#') {
        return Err(invalid("Invalid PDF layout color."));
    }
    let mut result = [0.; 3];
    for (index, target) in result.iter_mut().enumerate() {
        let pair = value
            .get(1 + index * 2..3 + index * 2)
            .ok_or_else(|| invalid("Invalid PDF layout color."))?;
        *target = f32::from(
            u8::from_str_radix(pair, 16).map_err(|_| invalid("Invalid PDF layout color."))?,
        ) / 255.;
    }
    Ok(result)
}

fn outline_content(path: &str) -> Result<(Vec<u8>, Rect), FormatError> {
    let mut content = Content::with_settings(Settings { pretty: false });
    let mut current = (0., 0.);
    let mut first = (0., 0.);
    let mut bounds = Bounds::default();
    for segment in PathParser::from(path) {
        let segment = segment.map_err(|_| invalid("Invalid Rust glyph outline for PDF output."))?;
        match segment {
            PathSegment::MoveTo { abs: true, x, y } => {
                content.move_to(number(x)?, number(y)?);
                bounds.add(x, y)?;
                current = (x, y);
                first = current;
            }
            PathSegment::LineTo { abs: true, x, y } => {
                content.line_to(number(x)?, number(y)?);
                bounds.add(x, y)?;
                current = (x, y);
            }
            PathSegment::Quadratic {
                abs: true,
                x1,
                y1,
                x,
                y,
            } => {
                // PDF has cubic curves. Degree elevation exactly preserves the
                // quadratic: C1=P0+2/3(Q-P0), C2=P2+2/3(Q-P2).
                content.cubic_to(
                    number(current.0 + (x1 - current.0) * 2. / 3.)?,
                    number(current.1 + (y1 - current.1) * 2. / 3.)?,
                    number(x + (x1 - x) * 2. / 3.)?,
                    number(y + (y1 - y) * 2. / 3.)?,
                    number(x)?,
                    number(y)?,
                );
                bounds.add(x1, y1)?;
                bounds.add(x, y)?;
                current = (x, y);
            }
            PathSegment::CurveTo {
                abs: true,
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => {
                content.cubic_to(
                    number(x1)?,
                    number(y1)?,
                    number(x2)?,
                    number(y2)?,
                    number(x)?,
                    number(y)?,
                );
                bounds.add(x1, y1)?;
                bounds.add(x2, y2)?;
                bounds.add(x, y)?;
                current = (x, y);
            }
            PathSegment::ClosePath { .. } => {
                content.close_path();
                current = first;
            }
            _ => return Err(invalid("Unsupported command in Rust PDF glyph outline.")),
        }
        check_size(content.len())?;
    }
    if !path.is_empty() {
        content.fill_nonzero();
    }
    // Every Bézier point lies in its control-point convex hull. This slightly
    // conservative form box therefore prevents clipping without measuring or
    // laying out text again. Empty whitespace glyphs draw an empty form.
    Ok((content.finish().to_vec(), bounds.rect()?))
}

#[derive(Default)]
struct Bounds(Option<(f64, f64, f64, f64)>);

impl Bounds {
    fn add(&mut self, x: f64, y: f64) -> Result<(), FormatError> {
        number(x)?;
        number(y)?;
        self.0 = Some(match self.0 {
            None => (x, y, x, y),
            Some((left, bottom, right, top)) => {
                (left.min(x), bottom.min(y), right.max(x), top.max(y))
            }
        });
        Ok(())
    }

    fn rect(&self) -> Result<Rect, FormatError> {
        let (left, bottom, right, top) = self.0.unwrap_or((0., 0., 1., 1.));
        // One font-unit padding avoids clipping from f32 conversion at the box
        // boundary; it never changes glyph placement or page geometry.
        Ok(Rect::new(
            number(left - 1.)?,
            number(bottom - 1.)?,
            number(right + 1.)?,
            number(top + 1.)?,
        ))
    }
}

fn check_size(size: usize) -> Result<(), FormatError> {
    if size > MAX_PDF_BYTES {
        Err(size_error())
    } else {
        Ok(())
    }
}

fn size_error() -> FormatError {
    FormatError::new(
        "pdf_limit",
        "PDF output exceeds the supported 16 MiB resource budget.",
    )
}

fn invalid(message: &str) -> FormatError {
    FormatError::new("invalid_pdf_layout", message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quadratic_degree_elevation_and_conservative_form_box_are_explicit() {
        let (bytes, bounds) = outline_content("M0 0Q3 6 6 0Z").unwrap();
        let commands = String::from_utf8(bytes).unwrap();
        assert!(commands.contains("2 4 4 4 6 0 c"), "{commands}");
        assert_eq!(
            (bounds.x1, bounds.y1, bounds.x2, bounds.y2),
            (-1., -1., 7., 7.)
        );
    }

    #[test]
    fn unexpected_outline_commands_and_nonfinite_geometry_fail_explicitly() {
        for path in ["M0 0a2 2 0 0 0 1 1", "M0 0l1 1", "M1e999 0", "not a path"] {
            assert!(outline_content(path).is_err(), "{path}");
        }
        assert!(number(f64::NAN).is_err());
        assert!(number(f64::INFINITY).is_err());
        assert!(check_size(MAX_PDF_BYTES + 1).is_err());
        assert!(color("#😀12").is_err());
        assert_eq!(color("#FF0080").unwrap(), [1., 0., 128. / 255.]);
    }
}
