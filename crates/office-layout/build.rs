//! Compiles trusted catalog metadata and target-gated native font references.

use std::{collections::BTreeMap, env, fmt::Write, fs, path::PathBuf};

use serde_json::Value;
use sha2::{Digest, Sha256};

fn main() {
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("Cargo supplies crate root"));
    let manifest_path = root.join("fonts/manifest.json");
    let order_path = root.join("fonts/registration-order.txt");
    println!("cargo:rerun-if-changed={}", manifest_path.display());
    println!("cargo:rerun-if-changed={}", order_path.display());
    let bytes = fs::read(&manifest_path).expect("read pinned font manifest");
    let manifest: Value = serde_json::from_slice(&bytes).expect("parse pinned font manifest");
    assert_eq!(manifest["format_version"], 1);
    let catalog_id = manifest["catalog_id"].as_str().expect("catalog ID");
    assert_eq!(catalog_id, "writer-fonts-v6");
    let mut entries = BTreeMap::new();
    for entry in manifest["entries"].as_array().expect("manifest entries") {
        if entry["kind"] != "font" {
            continue;
        }
        let id = entry["file"].as_str().expect("font filename");
        assert!(entries.insert(id, entry).is_none(), "duplicate font ID");
    }
    let order = fs::read_to_string(order_path).expect("read explicit font registration order");
    let ids: Vec<_> = order.lines().filter(|line| !line.is_empty()).collect();
    assert_eq!(
        ids.len(),
        entries.len(),
        "font order must cover the catalog"
    );
    let mut manifest_hash = String::with_capacity(64);
    for value in Sha256::digest(&bytes) {
        write!(manifest_hash, "{value:02x}").expect("write digest to string");
    }
    let mut generated = format!(
        "const CATALOG_ID: &str = {catalog_id:?};\nconst MANIFEST_SHA256: &str = {:?};\nconst FONT_ASSETS: &[FontAssetDescriptor] = &[\n",
        manifest_hash,
    );
    let mut embedded = String::from(
        "#[cfg(not(target_arch = \"wasm32\"))]\nconst EMBEDDED_FONT_FILES: &[&[u8]] = &[\n",
    );
    let mut total = 0_u64;
    for id in ids {
        assert!(
            id.ends_with(".ttf")
                && id
                    .bytes()
                    .all(|value| value.is_ascii_alphanumeric() || b"-_.".contains(&value)),
            "font IDs must be inert local filenames"
        );
        let entry = entries
            .remove(id)
            .expect("ordered font exists exactly once");
        assert_eq!(entry["face_index"], 0, "single-face pinned font required");
        let length = entry["byte_length"].as_u64().expect("font byte length");
        assert!((1..=3 * 1024 * 1024).contains(&length));
        total += length;
        let hash = entry["sha256"].as_str().expect("font SHA256");
        assert!(
            hash.len() == 64
                && hash
                    .bytes()
                    .all(|value| value.is_ascii_hexdigit() && !value.is_ascii_uppercase())
        );
        let family = entry["family"].as_str().expect("font family");
        assert!(!family.is_empty());
        let style = entry["style"].as_str().expect("font style");
        assert!(matches!(style, "normal" | "italic"));
        let weight = entry["weight"].as_u64().expect("font weight");
        let axis = entry["axes"]
            .as_array()
            .expect("font axes")
            .iter()
            .find(|axis| axis["tag"] == "wght");
        let minimum = axis.map_or(weight, |axis| {
            axis["minimum"].as_u64().expect("minimum weight")
        });
        let maximum = axis.map_or(weight, |axis| {
            axis["maximum"].as_u64().expect("maximum weight")
        });
        assert!(minimum > 0 && minimum <= maximum && maximum <= 1000);
        generated.push_str(&format!(
            "FontAssetDescriptor {{ font_id: {id:?}, byte_length: {length}, sha256: {hash:?}, family: {family:?}, style: {style:?}, weight_min: {minimum}, weight_max: {maximum} }},\n"
        ));
        embedded.push_str(&format!(
            "include_bytes!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/fonts/{id}\")),\n"
        ));
    }
    assert!(entries.is_empty());
    assert!(total <= 32 * 1024 * 1024, "catalog staging budget exceeded");
    generated.push_str("];// descriptor order preserves the original registration order\n");
    generated.push_str(&embedded);
    generated.push_str("];\n");
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo supplies output directory"));
    fs::write(output.join("font_catalog.rs"), generated).expect("write compiled font catalog");
}
