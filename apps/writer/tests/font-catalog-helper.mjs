// Initialize actual release WASM with the same canonical pinned files as native
// Rust. No browser font metrics, host discovery or mocked typography are used.
import { readFile } from 'node:fs/promises';
import { URL } from 'node:url';
import initialize, {
  finalize_font_catalog, font_catalog_manifest, font_catalog_status, register_font,
} from '../src/wasm/office_wasm.js';

const fontRoot = new URL('../../../crates/office-layout/fonts/', import.meta.url);
let initialization;

export function initializeWithFonts() {
  initialization ??= initializeCatalog().catch((error) => {
    initialization = undefined;
    throw error;
  });
  return initialization;
}

async function initializeCatalog() {
  const exports = await initialize({
    module_or_path: await readFile(new URL('../src/wasm/office_wasm_bg.wasm', import.meta.url)),
  });
  const manifest = JSON.parse(font_catalog_manifest());
  const status = JSON.parse(font_catalog_status());
  const missing = new Set(status.missingFontIds);
  if (!status.ready) {
    for (const font of manifest.fonts) {
      if (missing.has(font.id)) register_font(font.id, await readFile(new URL(font.id, fontRoot)));
    }
    const ready = JSON.parse(finalize_font_catalog());
    if (!ready.ready || ready.missingFontIds.length !== 0) throw new Error('Pinned Rust font catalog is incomplete.');
  }
  return exports;
}
