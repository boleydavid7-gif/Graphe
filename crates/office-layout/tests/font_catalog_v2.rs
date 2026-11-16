//! Font catalog v2: native pinned glyph selection for actual bundled faces.
//! Tests must fail if a menu font silently draws with Arimo/Tinos.

use office_core::{Block, CharacterFormat, Document, DocumentMetadata, Id, Paragraph, Section, TextRun};
use office_layout::{DocumentLayout, LayoutOptions, layout_document};

fn sample(family: &str, bold: bool, italic: bool) -> DocumentLayout {
    let mut run = TextRun::new("Ag AV 123");
    run.format = CharacterFormat {
        font_family: Some(family.into()),
        bold: Some(bold),
        italic: Some(italic),
        ..Default::default()
    };
    let doc = Document::from_parts(
        Id::new("fonts-document").unwrap(),
        DocumentMetadata::default(),
        vec![Section::new(Id::new("fonts-section").unwrap(),
            vec![Block::Paragraph(Paragraph::new(Id::new("fonts-paragraph").unwrap(), vec![run]))],
        )], vec![],
    ).unwrap();
    layout_document(&doc, &LayoutOptions::default()).unwrap()
}

#[test]
fn all_new_families_are_shaped_from_their_actual_bundled_font_files() {
    for (family, source) in [
        ("Carlito", "Carlito"),
        ("Caladea", "Caladea"),
        ("Source Sans 3", "SourceSans3"),
        ("JetBrains Mono", "JetBrainsMono"),
        ("EB Garamond", "EBGaramond"),
    ] {
        let page = sample(family, false, false);
        assert!(!page.outlines.is_empty(), "{family}");
        assert!(page.outlines.iter().all(|outline| outline.font_id.starts_with(source)),
            "{family} incorrectly substituted: {:?}", page.outlines.iter()
            .map(|outline| outline.font_id.as_str()).collect::<Vec<_>>());
        assert!(!page.diagnostics.iter().any(|issue| issue.kind == "font_substitution"),
            "{family} triggered a surprising font substitution");
    }
}

#[test]
fn static_serif_and_sans_use_genuine_weight_and_italic_faces() {
    for family in ["Carlito", "Caladea"] {
        let bold = sample(family, true, false);
        assert!(bold.outlines.iter().all(|face| face.font_id.contains("Bold")),
            "{family} did not use the included bold face");
        let italic = sample(family, false, true);
        assert!(italic.outlines.iter().all(|face| face.font_id.contains("Italic")),
            "{family} did not use the included italic face");
    }
}

#[test]
fn variable_faces_support_italic_and_weight_selection() {
    for family in ["Source Sans 3", "JetBrains Mono", "EB Garamond"] {
        let normal = sample(family, false, false);
        let italic = sample(family, false, true);
        assert!(italic.outlines.iter().all(|face| face.font_id.contains("Italic")),
            "{family} did not load its actual italic variable face");
        let bold = sample(family, true, false);
        assert_ne!(normal.outlines[0].path, bold.outlines[0].path,
            "{family} did not apply the bold weight axis");
    }
}

#[test]
fn common_word_font_names_resolve_without_rewriting_document_family() {
    for (name, resolved) in [
        ("Calibri", "Carlito"),
        ("Cambria", "Caladea"),
        ("Source Sans Pro", "SourceSans3"),
    ] {
        let page = sample(name, false, false);
        assert!(page.outlines.iter().all(|outline| outline.font_id.starts_with(resolved)),
            "{name} should use pinned {resolved} while preserving source metadata");
        assert!(page.diagnostics.iter().any(|issue| issue.kind == "font_substitution"));
    }
}
