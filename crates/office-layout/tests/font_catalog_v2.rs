//! Pinned font catalog: native glyph selection for actual bundled faces.
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
        ("Inter", "Inter"),
        ("Libre Baskerville", "LibreBaskerville"),
        ("Playfair Display", "PlayfairDisplay"),
        ("Space Grotesk", "SpaceGrotesk"),
        ("Lora", "Lora"),
        ("Montserrat", "Montserrat"),
        ("DM Sans", "DMSans"),
        ("Bitter", "Bitter"),
        ("Work Sans", "WorkSans"),
        ("Crimson Pro", "CrimsonPro"),
        ("Fraunces", "Fraunces"),
        ("Roboto Mono", "RobotoMono"),
        ("Oswald", "Oswald"),
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
    for family in [
        "Source Sans 3", "JetBrains Mono", "EB Garamond",
        "Inter", "Libre Baskerville", "Playfair Display",
        "Lora", "Montserrat", "DM Sans", "Bitter",
        "Work Sans", "Crimson Pro", "Fraunces", "Roboto Mono",
    ] {
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
        ("Times New Roman", "Tinos"),
        ("Arial", "Arimo"),
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

#[test]
fn oswald_uses_its_own_real_variable_weight_font() {
    let regular = sample("Oswald", false, false);
    let bold = sample("Oswald", true, false);
    assert!(regular.outlines.iter().all(|face| face.font_id.starts_with("Oswald")));
    assert!(bold.outlines.iter().all(|face| face.font_id.starts_with("Oswald")));
    assert_ne!(regular.outlines[0].path, bold.outlines[0].path);
}

#[test]
fn owner_uploaded_fonts_cannot_override_bundled_catalog_or_exceed_limits() {
    let mut engine = office_layout::LayoutEngine::new().unwrap();
    let bytes = include_bytes!("../fonts/Arimo-Regular.ttf");
    assert!(engine.register_external_font("my-id", "Arial", bytes).is_err());
    assert!(engine.register_external_font("my-id", "Arimo", bytes).is_err());
    assert!(engine.register_external_font("bad/id", "Custom", bytes).is_err());
    assert!(engine.register_external_font("ok", "Custom", &[1, 2, 3]).is_err());
    // A valid but mislabeled TTF must not be silently installed as a
    // different family (it would otherwise fall back at layout time).
    assert!(engine.register_external_font("mismatch", "Custom", bytes).is_err());
    assert!(engine.register_external_font("ok", "Custom", &vec![0; 5 * 1024 * 1024 + 1]).is_err());
}
