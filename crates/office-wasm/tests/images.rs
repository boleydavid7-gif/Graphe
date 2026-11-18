//! Image operations cross the browser adapter through existing validated history.

use office_core::{
    Block, Document, Id, Image, ImageAsset, PageMargins, PageSize, Paragraph, Section,
    SectionProperties, TextRun, Twips, deserialize_writer_file, serialize_document,
};
use office_wasm::BrowserEngine;
use serde_json::{Value, json};

const PNG: &[u8] = include_bytes!("fixtures/images/sample.png");
const JPEG: &[u8] = include_bytes!("fixtures/images/sample.jpg");
fn id(value: &str) -> Id {
    Id::new(value).unwrap()
}
fn snapshot(engine: &BrowserEngine) -> Value {
    serde_json::from_str(&engine.snapshot()).unwrap()
}
fn command(engine: &mut BrowserEngine, value: Value) -> Value {
    let response: Value = serde_json::from_str(&engine.dispatch(&value.to_string())).unwrap();
    assert_eq!(response["ok"], true, "{response}");
    response["view"].clone()
}
fn insert(engine: &mut BrowserEngine, bytes: &[u8]) -> Value {
    let before = snapshot(engine);
    let response: Value =
        serde_json::from_str(&engine.insert_image(bytes, before["revision"].as_str())).unwrap();
    assert_eq!(response["ok"], true, "{response}");
    response["view"].clone()
}
fn point(paragraph: &str, offset: usize) -> Value {
    json!({"paragraphId":paragraph,"utf16":offset})
}

#[test]
fn binary_image_insertion_owns_validated_source_without_payloads_in_typing_snapshots() {
    for bytes in [PNG, JPEG] {
        let mut engine = BrowserEngine::new("image-insert").unwrap();
        let before = engine.export_document().unwrap();
        let view = insert(&mut engine, bytes);
        assert_eq!(view["images"].as_array().unwrap().len(), 1);
        assert_eq!(view["paragraphs"].as_array().unwrap().len(), 2);
        assert_eq!(view["images"][0]["widthPt"], 120.0);
        assert_eq!(view["images"][0]["heightPt"], 60.0);
        assert_eq!(view["images"][0]["pixelWidth"], 160);
        assert_eq!(view["images"][0]["pixelHeight"], 80);
        assert_eq!(view["selectedImageId"], view["images"][0]["id"]);
        assert_eq!(
            view["bodyBlocks"]
                .as_array()
                .unwrap()
                .iter()
                .map(|block| block["kind"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["paragraph", "image", "paragraph"]
        );
        assert!(!engine.snapshot().contains("base64"));
        assert!(!engine.snapshot().contains("data:"));
        let file = deserialize_writer_file(&engine.export_document().unwrap()).unwrap();
        let Block::Image(image) = &file.document().sections()[0].blocks()[1] else {
            panic!("missing image")
        };
        assert_eq!(image.asset().original_bytes(), bytes);
        let image_id = image.id().as_str();
        let png = engine
            .image_png(image_id, view["revision"].as_str())
            .unwrap();
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert_eq!(png, image.asset().render_png());
        command(&mut engine, json!({"type":"undo"}));
        assert_eq!(engine.export_document().unwrap(), before);
        assert_eq!(snapshot(&engine)["selectedImageId"], Value::Null);
        let redone = command(&mut engine, json!({"type":"redo"}));
        assert_eq!(redone["images"], view["images"]);
        assert_eq!(redone["selectedImageId"], view["selectedImageId"]);
    }
}

#[test]
fn combined_image_properties_are_one_exact_undo_step_and_persist_without_pixel_changes() {
    let mut engine = BrowserEngine::new("image-properties").unwrap();
    let inserted = insert(&mut engine, PNG);
    let image_id = inserted["selectedImageId"].as_str().unwrap().to_owned();
    let before = engine.export_document().unwrap();
    let png = engine.image_png(&image_id, None).unwrap();
    let changed = command(
        &mut engine,
        json!({"type":"update_image","widthPt":100,"altText":"Café e\u{301} 👩🏽‍💻 العربية","alignment":"right"}),
    );
    assert_eq!(changed["images"][0]["widthPt"], 100.0);
    assert_eq!(changed["images"][0]["heightPt"], 50.0);
    assert_eq!(changed["images"][0]["alignment"], "right");
    assert_eq!(changed["images"][0]["altText"], "Café é 👩🏽‍💻 العربية");
    assert_eq!(engine.image_png(&image_id, None).unwrap(), png);
    let after = engine.export_document().unwrap();
    let undone = command(&mut engine, json!({"type":"undo"}));
    assert_eq!(engine.export_document().unwrap(), before);
    assert_eq!(undone["selectedImageId"], image_id);
    command(&mut engine, json!({"type":"redo"}));
    assert_eq!(engine.export_document().unwrap(), after);
    let response: Value = serde_json::from_str(&engine.open_document(&after)).unwrap();
    assert_eq!(response["ok"], true);
    assert_eq!(response["view"]["images"], changed["images"]);
    assert_eq!(response["view"]["selectedImageId"], Value::Null);
    assert_eq!(engine.export_document().unwrap(), after);
    assert_eq!(engine.image_png(&image_id, None).unwrap(), png);
}

#[test]
fn selected_images_reject_old_caret_text_edits_and_restore_selection_through_history() {
    let mut engine = BrowserEngine::new("image-selection").unwrap();
    command(
        &mut engine,
        json!({"type":"replace_text","text":"Keep this text"}),
    );
    let before_image = engine.export_document().unwrap();
    let inserted = insert(&mut engine, PNG);
    let image_id = inserted["selectedImageId"].clone();
    let revision = inserted["revision"].clone();
    let native = engine.export_document().unwrap();
    assert_eq!(
        engine.selected_text().unwrap_err().code,
        "image_clipboard_unsupported"
    );
    for value in [
        json!({"type":"replace_text","text":"Wrong old caret"}),
        json!({"type":"insert_paragraph"}),
        json!({"type":"format_character","bold":true}),
        json!({"type":"insert_table","rows":1,"columns":1}),
    ] {
        let response: Value = serde_json::from_str(&engine.dispatch(&value.to_string())).unwrap();
        assert_eq!(response["ok"], false);
        assert_eq!(response["error"]["code"], "image_selected");
        assert_eq!(engine.export_document().unwrap(), native);
        assert_eq!(snapshot(&engine)["revision"], revision);
    }
    let deselected = command(&mut engine, json!({"type":"select_image","imageId":null}));
    assert_eq!(deselected["revision"], revision);
    let selected = command(
        &mut engine,
        json!({"type":"select_image","imageId":image_id}),
    );
    assert_eq!(selected["revision"], revision);
    let removed = command(&mut engine, json!({"type":"delete","direction":"backward"}));
    assert!(removed["images"].as_array().unwrap().is_empty());
    assert_eq!(
        removed["paragraphs"][0]["runs"][0]["text"],
        "Keep this text"
    );
    let restored = command(&mut engine, json!({"type":"undo"}));
    assert_eq!(restored["selectedImageId"], image_id);
    assert_eq!(engine.export_document().unwrap(), native);
    command(&mut engine, json!({"type":"undo"}));
    assert_eq!(engine.export_document().unwrap(), before_image);
}

#[test]
fn image_selection_hides_retained_table_context_until_text_is_selected_again() {
    let mut engine = BrowserEngine::new("image-table-context").unwrap();
    let inserted = insert(&mut engine, PNG);
    command(
        &mut engine,
        json!({"type":"move_image_cursor","direction":"forward"}),
    );
    let table = command(
        &mut engine,
        json!({"type":"insert_table","rows":1,"columns":1}),
    );
    assert!(table["table"].is_object());
    let selected = command(
        &mut engine,
        json!({"type":"select_image","imageId":inserted["selectedImageId"]}),
    );
    assert_eq!(selected["table"], Value::Null);
    assert_eq!(selected["selection"], table["selection"]);
    assert_eq!(selected["revision"], table["revision"]);
    let text = command(
        &mut engine,
        json!({"type":"set_selection","selection":table["selection"]}),
    );
    assert_eq!(text["selectedImageId"], Value::Null);
    assert_eq!(text["table"], table["table"]);
}

#[test]
fn rejected_files_sizes_and_revision_guards_preserve_content_and_exact_selection() {
    let mut engine = BrowserEngine::new("image-invalid").unwrap();
    let original = engine.export_document().unwrap();
    let view = snapshot(&engine);
    let oversized = vec![0; 2 * 1024 * 1024 + 1];
    for bytes in [
        b"not an image".as_slice(),
        b"\x89PNG\r\n\x1a\n".as_slice(),
        b"\xff\xd8bad".as_slice(),
        oversized.as_slice(),
    ] {
        let result: Value =
            serde_json::from_str(&engine.insert_image(bytes, view["revision"].as_str())).unwrap();
        assert_eq!(result["ok"], false);
        assert_eq!(engine.export_document().unwrap(), original);
        assert_eq!(snapshot(&engine), view);
    }
    let inserted = insert(&mut engine, PNG);
    let before = engine.export_document().unwrap();
    for value in [
        json!({"type":"resize_image","widthPt":0}),
        json!({"type":"resize_image","widthPt":99999}),
        json!({"type":"update_image","widthPt":100,"heightPt":100,"lockAspect":true,"altText":"must not apply"}),
        json!({"type":"update_image","widthPt":100,"altText":"x".repeat(8193)}),
        json!({"type":"select_image","imageId":"missing-image"}),
        json!({"type":"delete_image","expectedRevision":"stale"}),
    ] {
        let response: Value = serde_json::from_str(&engine.dispatch(&value.to_string())).unwrap();
        assert_eq!(response["ok"], false, "{value} {response}");
        assert_eq!(engine.export_document().unwrap(), before);
        assert_eq!(snapshot(&engine), inserted);
    }
    let stale: Value = serde_json::from_str(&engine.insert_image(PNG, Some("stale"))).unwrap();
    assert_eq!(stale["error"]["code"], "stale_revision");
    assert_eq!(
        engine
            .image_png(inserted["selectedImageId"].as_str().unwrap(), Some("stale"))
            .unwrap_err()
            .code,
        "stale_revision"
    );
}

#[test]
fn paragraph_text_replacement_cannot_flatten_or_remove_an_intervening_image() {
    let mut engine = BrowserEngine::new("image-boundary").unwrap();
    let first = command(&mut engine, json!({"type":"replace_text","text":"Before"}));
    let first_id = first["selection"]["focus"]["paragraphId"]
        .as_str()
        .unwrap()
        .to_owned();
    insert(&mut engine, PNG);
    let moved = command(
        &mut engine,
        json!({"type":"move_image_cursor","direction":"forward"}),
    );
    let last_id = moved["selection"]["focus"]["paragraphId"]
        .as_str()
        .unwrap()
        .to_owned();
    command(&mut engine, json!({"type":"replace_text","text":"After"}));
    let before = engine.export_document().unwrap();
    let response: Value=serde_json::from_str(&engine.dispatch(&json!({"type":"replace_text","text":"flattened","selection":{"anchor":point(&first_id,0),"focus":point(&last_id,5)}}).to_string())).unwrap();
    assert_eq!(response["ok"], false);
    assert_eq!(engine.export_document().unwrap(), before);
    let image_id = snapshot(&engine)["images"][0]["id"].clone();
    let selected = command(
        &mut engine,
        json!({"type":"select_image","imageId":image_id}),
    );
    let geometry: Value = serde_json::from_str(
        &engine
            .selection_geometry("{}", selected["revision"].as_str())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(geometry["geometry"], Value::Null);
}

#[test]
fn image_only_native_open_is_exact_and_explicit_text_escape_is_undoable_with_fresh_ids() {
    let image = Image::new(
        id("image-only:e4"),
        ImageAsset::from_bytes(PNG).unwrap(),
        Twips(2400),
        Twips(1200),
    );
    let document = Document::from_parts(
        id("image-only-doc"),
        Default::default(),
        vec![Section::new(id("image-section"), vec![Block::Image(image)])],
        vec![],
    )
    .unwrap();
    let bytes = serialize_document(&document).unwrap();
    let mut engine = BrowserEngine::new("image-only").unwrap();
    let opened: Value = serde_json::from_str(&engine.open_document(&bytes)).unwrap();
    assert_eq!(opened["ok"], true);
    assert_eq!(opened["view"]["selection"], Value::Null);
    assert!(opened["view"]["paragraphs"].as_array().unwrap().is_empty());
    assert_eq!(engine.export_document().unwrap(), bytes);
    command(
        &mut engine,
        json!({"type":"select_image","imageId":"image-only:e4"}),
    );
    let after = command(
        &mut engine,
        json!({"type":"move_image_cursor","direction":"forward"}),
    );
    assert_eq!(after["paragraphs"].as_array().unwrap().len(), 1);
    assert_ne!(after["paragraphs"][0]["id"], "image-only:e4");
    assert_eq!(after["selectedImageId"], Value::Null);
    let undone = command(&mut engine, json!({"type":"undo"}));
    assert_eq!(undone["selectedImageId"], "image-only:e4");
    assert_eq!(engine.export_document().unwrap(), bytes);
    command(&mut engine, json!({"type":"delete_image"}));
    assert!(snapshot(&engine)["images"].as_array().unwrap().is_empty());
}

#[test]
fn initial_size_and_explicit_resize_use_section_geometry_and_keep_aspect_ratio() {
    let document = Document::from_parts(
        id("small-page"),
        Default::default(),
        vec![
            Section::new(
                id("s"),
                vec![Block::Paragraph(Paragraph::new(
                    id("p"),
                    vec![TextRun::new("Before")],
                ))],
            )
            .with_properties(SectionProperties {
                page_size: Some(PageSize {
                    width: Twips(2000),
                    height: Twips(1200),
                    orientation: None,
                }),
                page_margins: Some(PageMargins {
                    top: Twips(200),
                    right: Twips(200),
                    bottom: Twips(200),
                    left: Twips(200),
                    header: Twips(0),
                    footer: Twips(0),
                    gutter: Twips(0),
                }),
            }),
        ],
        vec![],
    )
    .unwrap();
    let mut engine = BrowserEngine::new("image-sizing").unwrap();
    let result: Value =
        serde_json::from_str(&engine.open_document(&serialize_document(&document).unwrap()))
            .unwrap();
    assert_eq!(result["ok"], true);
    let fitted = insert(&mut engine, PNG);
    assert_eq!(fitted["images"][0]["widthPt"], 80.0);
    assert_eq!(fitted["images"][0]["heightPt"], 40.0);
    let changed = command(&mut engine, json!({"type":"resize_image","heightPt":30}));
    assert_eq!(changed["images"][0]["widthPt"], 60.0);
    assert_eq!(changed["images"][0]["heightPt"], 30.0);
    let natural = command(
        &mut engine,
        json!({"type":"resize_image","widthPt":55,"heightPt":35,"lockAspect":false}),
    );
    assert_eq!(natural["images"][0]["widthPt"], 55.0);
    assert_eq!(natural["images"][0]["heightPt"], 35.0);
}

#[test]
fn adding_images_to_an_imported_document_retains_its_immutable_original_across_undo_and_reopen() {
    let mut engine = BrowserEngine::new("image-original").unwrap();
    let source = "Original source é 👩🏽‍💻".as_bytes();
    let imported: Value =
        serde_json::from_str(&engine.import_format(source, "txt", "source.txt")).unwrap();
    assert_eq!(imported["ok"], true);
    insert(&mut engine, JPEG);
    let bytes = engine.export_document().unwrap();
    assert_eq!(engine.export_original().unwrap(), source);
    command(&mut engine, json!({"type":"undo"}));
    assert_eq!(engine.export_original().unwrap(), source);
    command(&mut engine, json!({"type":"redo"}));
    assert_eq!(engine.export_document().unwrap(), bytes);
    let opened: Value = serde_json::from_str(&engine.open_document(&bytes)).unwrap();
    assert_eq!(opened["ok"], true);
    assert_eq!(opened["view"]["images"].as_array().unwrap().len(), 1);
    assert_eq!(engine.export_original().unwrap(), source);
    assert_eq!(engine.export_document().unwrap(), bytes);
}
