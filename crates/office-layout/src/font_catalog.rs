//! Verified immutable font bytes shared by all layout entry points.

use std::sync::{Arc, OnceLock};

use parley::fontique::Blob;
use sha2::{Digest, Sha256};
use skrifa::{
    FontRef, MetadataProvider,
    instance::{LocationRef, Size},
};

use crate::LayoutError;

/// One compile-pinned asset, in the catalog's original registration order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FontAssetDescriptor {
    /// Exact inert filename used as the stable font ID.
    pub font_id: &'static str,
    /// Exact encoded byte length accepted by Rust.
    pub byte_length: usize,
    /// Lowercase SHA-256 of the complete original font bytes.
    pub sha256: &'static str,
    /// Canonical CSS family for the semantic input projection.
    pub family: &'static str,
    /// CSS style, either `normal` or `italic`.
    pub style: &'static str,
    /// Minimum supported CSS weight, or the static face weight.
    pub weight_min: u16,
    /// Maximum supported CSS weight, or the static face weight.
    pub weight_max: u16,
}

include!(concat!(env!("OUT_DIR"), "/font_catalog.rs"));

#[derive(Clone)]
pub(crate) struct PinnedFont {
    pub(crate) id: &'static str,
    pub(crate) blob: Blob<u8>,
}

/// A complete catalog whose bytes match the compiled identities and hashes.
///
/// Its immutable buffers are shared across shaping engines. It can only be
/// constructed by successfully finishing a [`FontCatalogBuilder`].
#[derive(Clone)]
pub struct VerifiedFontCatalog {
    pub(crate) fonts: Vec<PinnedFont>,
}

/// Bounded, retryable staging for the complete pinned external font catalog.
///
/// Registration accepts any order but retains each verified ID at most once.
/// No font is made available to layout until the whole catalog is finished and
/// installed. Invalid input never reserves a slot or changes valid staged fonts.
pub struct FontCatalogBuilder {
    slots: Vec<Option<PinnedFont>>,
    registered: usize,
}

impl Default for FontCatalogBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl FontCatalogBuilder {
    /// Starts empty bounded staging without loading any font bytes.
    #[must_use]
    pub fn new() -> Self {
        Self {
            slots: vec![None; FONT_ASSETS.len()],
            registered: 0,
        }
    }

    /// Verifies and stages exactly one known font without publishing it.
    pub fn register(&mut self, font_id: &str, bytes: &[u8]) -> Result<(), LayoutError> {
        if font_id.len() > 128 {
            return Err(font_error(CATALOG_ID, "font ID exceeds 128 UTF-8 bytes"));
        }
        let index = FONT_ASSETS
            .iter()
            .position(|asset| asset.font_id == font_id)
            .ok_or_else(|| font_error(font_id, "font ID is outside the pinned catalog"))?;
        if self.slots[index].is_some() {
            return Err(font_error(font_id, "font ID is already registered"));
        }
        let asset = &FONT_ASSETS[index];
        verify_bytes(asset, bytes)?;
        self.slots[index] = Some(PinnedFont {
            id: asset.font_id,
            blob: Blob::new(Arc::new(bytes.to_vec())),
        });
        self.registered += 1;
        Ok(())
    }

    /// Number of completely verified staged font assets.
    #[must_use]
    pub const fn registered_count(&self) -> usize {
        self.registered
    }

    /// Whether this exact ID has already passed verification in this builder.
    #[must_use]
    pub fn is_registered(&self, font_id: &str) -> bool {
        FONT_ASSETS
            .iter()
            .position(|asset| asset.font_id == font_id)
            .is_some_and(|index| self.slots[index].is_some())
    }

    /// Produces the complete catalog in its original deterministic order.
    ///
    /// Missing fonts are an error. This does not consume successful staging, so
    /// an interrupted loader can register the remaining assets and try again.
    pub fn finish(&self) -> Result<VerifiedFontCatalog, LayoutError> {
        if self.registered != FONT_ASSETS.len() {
            let missing = FONT_ASSETS
                .iter()
                .zip(&self.slots)
                .filter(|(_, slot)| slot.is_none())
                .map(|(asset, _)| asset.font_id)
                .collect::<Vec<_>>()
                .join(", ");
            return Err(font_error(
                CATALOG_ID,
                format!("font catalog is incomplete; missing {missing}"),
            ));
        }
        Ok(VerifiedFontCatalog {
            fonts: self.slots.iter().flatten().cloned().collect(),
        })
    }
}

static EXTERNAL_CATALOG: OnceLock<VerifiedFontCatalog> = OnceLock::new();

/// Stable catalog identity; external transport does not change layout semantics.
#[must_use]
pub const fn font_catalog_id() -> &'static str {
    CATALOG_ID
}

/// SHA-256 of the complete compile-pinned source manifest JSON bytes.
///
/// This version key is suitable for asset/cache paths, not a replacement for
/// checking each font's length and hash in Rust.
#[must_use]
pub const fn font_catalog_manifest_sha256() -> &'static str {
    MANIFEST_SHA256
}

/// All required assets, with compiled lengths/hashes and original font order.
#[must_use]
pub const fn font_catalog_assets() -> &'static [FontAssetDescriptor] {
    FONT_ASSETS
}

/// Whether an entire external catalog has been atomically installed.
///
/// Native [`crate::LayoutEngine::new`] retains its independent bundled default;
/// on WebAssembly this readiness is required by every layout/PDF entry point.
#[must_use]
pub fn font_catalog_ready() -> bool {
    EXTERNAL_CATALOG.get().is_some()
}

/// Atomically publishes a fully verified external catalog for WebAssembly.
///
/// Installation is idempotent: a ready catalog is immutable and never reset or
/// replaced. Only the exact same compile-pinned catalog can be constructed.
/// Native targets expose this for transport tests but keep their bundled default.
pub fn install_font_catalog(catalog: VerifiedFontCatalog) -> Result<(), LayoutError> {
    let _ = EXTERNAL_CATALOG.set(catalog);
    Ok(())
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn default_catalog() -> Result<&'static VerifiedFontCatalog, LayoutError> {
    EXTERNAL_CATALOG
        .get()
        .ok_or(LayoutError::FontCatalogNotReady)
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn default_catalog() -> Result<&'static VerifiedFontCatalog, LayoutError> {
    static BUNDLED: OnceLock<Result<VerifiedFontCatalog, LayoutError>> = OnceLock::new();
    BUNDLED
        .get_or_init(bundled_catalog)
        .as_ref()
        .map_err(Clone::clone)
}

#[cfg(not(target_arch = "wasm32"))]
fn bundled_catalog() -> Result<VerifiedFontCatalog, LayoutError> {
    let mut fonts = Vec::with_capacity(FONT_ASSETS.len());
    for (asset, &bytes) in FONT_ASSETS.iter().zip(EMBEDDED_FONT_FILES) {
        verify_bytes(asset, bytes)?;
        fonts.push(PinnedFont {
            id: asset.font_id,
            blob: Blob::new(Arc::new(bytes)),
        });
    }
    Ok(VerifiedFontCatalog { fonts })
}

fn verify_bytes(asset: &FontAssetDescriptor, bytes: &[u8]) -> Result<(), LayoutError> {
    if bytes.len() != asset.byte_length {
        return Err(font_error(
            asset.font_id,
            format!(
                "expected {} bytes, received {}",
                asset.byte_length,
                bytes.len()
            ),
        ));
    }
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = Sha256::digest(bytes);
    if !digest.iter().enumerate().all(|(index, &value)| {
        asset.sha256.as_bytes()[index * 2] == HEX[usize::from(value >> 4)]
            && asset.sha256.as_bytes()[index * 2 + 1] == HEX[usize::from(value & 15)]
    }) {
        return Err(font_error(
            asset.font_id,
            "SHA-256 does not match the pinned manifest",
        ));
    }
    let font = FontRef::from_index(bytes, 0).map_err(|error| font_error(asset.font_id, error))?;
    let metrics = font.metrics(Size::unscaled(), LocationRef::default());
    if metrics.units_per_em == 0 || metrics.glyph_count == 0 {
        return Err(font_error(
            asset.font_id,
            "font has no usable metrics or glyphs",
        ));
    }
    Ok(())
}

fn font_error(font_id: &str, message: impl ToString) -> LayoutError {
    LayoutError::Font {
        font_id: font_id.into(),
        message: message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{FontAssetDescriptor, verify_bytes};

    #[test]
    fn a_matching_length_and_digest_still_require_a_valid_font_structure() {
        let asset = FontAssetDescriptor {
            font_id: "invalid.ttf",
            byte_length: 10,
            sha256: "af4d9eef04c124f5f634d62e071cf4d46c82f08181210b1d0368ccf351e17ebb",
            family: "Invalid",
            style: "normal",
            weight_min: 400,
            weight_max: 400,
        };
        let error = verify_bytes(&asset, b"not a font").unwrap_err();
        assert_eq!(error.code(), "layout_font_error");
        assert!(!error.to_string().contains("SHA-256"));
        assert!(!error.to_string().contains("expected 10 bytes"));
    }
}
