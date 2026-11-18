//! Binary font registration is runtime initialization, never document content.

use std::cell::RefCell;

use office_layout::{
    FontCatalogBuilder, font_catalog_assets, font_catalog_id, font_catalog_manifest_sha256,
    font_catalog_ready, install_font_catalog,
};
use serde::Serialize;

use crate::EngineError;

thread_local! {
    // A failed request never consumes existing validated registrations. Once
    // installed, the global catalog owns shared immutable bytes and this staging
    // allocation is discarded; no application reset/replacement API exists.
    static STAGED: RefCell<Option<FontCatalogBuilder>> = const { RefCell::new(None) };
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Asset<'a> {
    id: &'a str,
    byte_length: usize,
    sha256: &'a str,
    family: &'a str,
    style: &'a str,
    weight_min: u16,
    weight_max: u16,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Manifest<'a> {
    catalog_id: &'a str,
    manifest_sha256: &'a str,
    fonts: Vec<Asset<'a>>,
    total_bytes: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Status<'a> {
    catalog_id: &'a str,
    ready: bool,
    registered_fonts: usize,
    total_fonts: usize,
    missing_font_ids: Vec<&'a str>,
}

pub(super) fn manifest() -> String {
    let assets = font_catalog_assets();
    serde_json::to_string(&Manifest {
        catalog_id: font_catalog_id(),
        manifest_sha256: font_catalog_manifest_sha256(),
        fonts: assets
            .iter()
            .map(|asset| Asset {
                id: asset.font_id,
                byte_length: asset.byte_length,
                sha256: asset.sha256,
                family: asset.family,
                style: asset.style,
                weight_min: asset.weight_min,
                weight_max: asset.weight_max,
            })
            .collect(),
        total_bytes: assets.iter().map(|asset| asset.byte_length).sum(),
    })
    .expect("pinned manifest contains serializable static values")
}

pub(super) fn status() -> String {
    let ready = font_catalog_ready();
    let assets = font_catalog_assets();
    let missing_font_ids = if ready {
        Vec::new()
    } else {
        STAGED.with_borrow(|staged| {
            assets
                .iter()
                .filter(|asset| {
                    !staged
                        .as_ref()
                        .is_some_and(|builder| builder.is_registered(asset.font_id))
                })
                .map(|asset| asset.font_id)
                .collect()
        })
    };
    serde_json::to_string(&Status {
        catalog_id: font_catalog_id(),
        ready,
        registered_fonts: assets.len() - missing_font_ids.len(),
        total_fonts: assets.len(),
        missing_font_ids,
    })
    .expect("catalog status contains serializable static values")
}

pub(super) fn register(font_id: &str, bytes: &[u8]) -> Result<String, EngineError> {
    if font_catalog_ready() {
        return Err(EngineError::new(
            "font_catalog_finalized",
            "The verified font catalog is immutable and already ready.",
        ));
    }
    STAGED.with_borrow_mut(|staged| {
        staged
            .get_or_insert_with(FontCatalogBuilder::new)
            .register(font_id, bytes)
            .map_err(font_error)
    })?;
    Ok(status())
}

pub(super) fn finalize() -> Result<String, EngineError> {
    if font_catalog_ready() {
        return Ok(status());
    }
    STAGED.with_borrow_mut(|staged| -> Result<(), EngineError> {
        let catalog = staged
            .get_or_insert_with(FontCatalogBuilder::new)
            .finish()
            .map_err(font_error)?;
        install_font_catalog(catalog).map_err(font_error)?;
        *staged = None;
        Ok(())
    })?;
    Ok(status())
}

fn font_error(error: office_layout::LayoutError) -> EngineError {
    EngineError::new(error.code(), error.to_string())
}
