import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { after, test } from 'node:test';
import { fileURLToPath, URL } from 'node:url';
import { TextDecoder } from 'node:util';
import initialize, {
  WriterEngine, finalize_font_catalog, font_catalog_manifest, font_catalog_status, register_font,
} from '../src/wasm/office_wasm.js';

// This file intentionally starts cold. Other actual-WASM suites use the shared
// helper to initialize the complete catalog before their first layout request.
await initialize({ module_or_path: await readFile(new URL('../src/wasm/office_wasm_bg.wasm', import.meta.url)) });
const fontRoot = new URL('../../../crates/office-layout/fonts/', import.meta.url);
const manifest = JSON.parse(font_catalog_manifest());
const engine = new WriterEngine('font-lifecycle');
after(() => engine.free());
const edit = JSON.parse(engine.dispatch(JSON.stringify({ type: 'replace_text', text: 'AV office — e\u0301 👩🏽‍💻 العربية नमस्ते שלום' })));
assert.equal(edit.ok, true);
const before = engine.export_document();
const originalView = engine.snapshot();
const revision = JSON.parse(originalView).revision;

function status() { return JSON.parse(font_catalog_status()); }
function errorCode(expected) {
  return (error) => {
    const parsed = JSON.parse(String(error));
    assert.equal(parsed.code, expected);
    assert.equal(typeof parsed.message, 'string');
    assert.ok(parsed.message.length > 0);
    return true;
  };
}
function unchanged() {
  assert.deepEqual(engine.export_document(), before);
  assert.equal(engine.snapshot(), originalView);
}
function cannotRender() {
  assert.throws(() => engine.layout_snapshot('{}', revision), errorCode('font_catalog_not_ready'));
  assert.throws(() => engine.selection_geometry('{}', revision), errorCode('font_catalog_not_ready'));
  assert.throws(() => engine.layout_action('{}', '{"type":"select_all"}', revision), errorCode('font_catalog_not_ready'));
  assert.throws(() => engine.export_assessment('pdf', revision), errorCode('pdf_layout'));
  assert.throws(() => engine.export_format('pdf', true, revision), errorCode('pdf_layout'));
  unchanged();
}

test('actual WASM publishes the exact canonical 42-face manifest and blocks all typography before initialization', async () => {
  const raw = await readFile(new URL('manifest.json', fontRoot));
  const canonical = JSON.parse(raw);
  const expected = canonical.entries.filter((entry) => entry.kind === 'font');
  assert.equal(manifest.catalogId, canonical.catalog_id);
  assert.equal(manifest.manifestSha256, createHash('sha256').update(raw).digest('hex'));
  assert.equal(manifest.fonts.length, 42);
  assert.equal(new Set(manifest.fonts.map((font) => font.id)).size, 42);
  assert.equal(manifest.totalBytes, canonical.font_bytes);
  for (const font of manifest.fonts) {
    const entry = expected.find((entry) => entry.file === font.id);
    assert.ok(entry, `Unknown catalog face ${font.id}`);
    assert.equal(font.byteLength, entry.byte_length);
    assert.equal(font.sha256, entry.sha256);
    assert.equal(font.family, entry.family);
    assert.equal(font.style, entry.style);
    const weight = entry.axes?.find((axis) => axis.tag === 'wght');
    assert.equal(font.weightMin, weight?.minimum ?? entry.weight);
    assert.equal(font.weightMax, weight?.maximum ?? entry.weight);
  }
  assert.deepEqual(status(), { catalogId: manifest.catalogId, ready: false, registeredFonts: 0,
    totalFonts: 42, missingFontIds: manifest.fonts.map((font) => font.id) });
  cannotRender();
  assert.throws(() => finalize_font_catalog(), errorCode('layout_font_error'));
  assert.equal(status().registeredFonts, 0);
});

test('actual WASM rejects unknown, truncated, oversized and hash-corrupted fonts without reserving a slot', async () => {
  const first = manifest.fonts[0];
  const bytes = await readFile(new URL(first.id, fontRoot));
  const initial = status();
  assert.throws(() => register_font('../Arimo-Regular.ttf', bytes), errorCode('layout_font_error'));
  const corrupted = bytes.slice(); corrupted[corrupted.length - 1] ^= 1;
  for (const rejected of [new Uint8Array(), bytes.subarray(0, bytes.length - 1),
    new Uint8Array(bytes.length + 1), corrupted, new Uint8Array(bytes.length)]) {
    assert.throws(() => register_font(first.id, rejected), errorCode('layout_font_error'));
    assert.deepEqual(status(), initial);
    unchanged();
  }
});

test('actual WASM owns staged bytes, rejects duplicates and preserves partial progress after a premature finalize', async () => {
  const first = manifest.fonts[0];
  const owned = new Uint8Array(await readFile(new URL(first.id, fontRoot)));
  const partial = JSON.parse(register_font(first.id, owned));
  owned.fill(0);
  assert.equal(partial.ready, false);
  assert.equal(partial.registeredFonts, 1);
  assert.equal(partial.missingFontIds.includes(first.id), false);
  assert.throws(() => register_font(first.id, owned), errorCode('layout_font_error'));
  assert.throws(() => finalize_font_catalog(), errorCode('layout_font_error'));
  assert.deepEqual(status(), partial);
  cannotRender();
});

test('actual WASM accepts reversed registration order but exposes no partial layout until all faces are finalized', async () => {
  for (const font of manifest.fonts.slice(1).reverse()) {
    const next = JSON.parse(register_font(font.id, await readFile(new URL(font.id, fontRoot))));
    assert.equal(next.ready, false);
    unchanged();
  }
  assert.equal(status().registeredFonts, 42);
  assert.deepEqual(status().missingFontIds, []);
  cannotRender();
  const ready = JSON.parse(finalize_font_catalog());
  assert.deepEqual(ready, { catalogId: manifest.catalogId, ready: true, registeredFonts: 42,
    totalFonts: 42, missingFontIds: [] });
  unchanged();
});

test('actual WASM finalized fonts are immutable, reusable and shared by selection, layout and PDF export', async () => {
  const ready = status();
  assert.deepEqual(JSON.parse(finalize_font_catalog()), ready);
  const first = manifest.fonts[0];
  const bytes = await readFile(new URL(first.id, fontRoot));
  assert.throws(() => register_font(first.id, bytes),
    errorCode('font_catalog_finalized'));
  const layout = JSON.parse(engine.layout_snapshot('{}', revision)).layout;
  assert.equal(layout.fontCatalogId, manifest.catalogId);
  assert.ok(layout.outlines.length > 0);
  assert.ok(layout.outlines.some((outline) => outline.fontId === first.id), 'Staged caller-buffer mutation did not damage the first font');
  assert.ok(JSON.parse(engine.selection_geometry('{}', revision)).geometry);
  const pdf = engine.export_format('pdf', true, revision);
  assert.equal(new TextDecoder().decode(pdf.subarray(0, 5)), '%PDF-');
  unchanged();
});

test('actual WASM externally registered fonts retain complete native layout parity despite reversed delivery', async () => {
  const root = new URL('../../../tests/fixtures/layout/', import.meta.url);
  const expected = JSON.parse(await readFile(new URL('native-parity.json', root)));
  const value = new WriterEngine('font-native-parity');
  try {
    assert.equal(JSON.parse(value.open_document(await readFile(new URL(expected.input, root)))).ok, true);
    const current = JSON.parse(value.snapshot());
    const actual = JSON.parse(value.layout_snapshot(JSON.stringify(expected.options), current.revision)).layout;
    const native = execFileSync('cargo', ['run', '--offline', '--locked', '--quiet', '-p', 'office-layout',
      '--example', 'layout_snapshot', '--', fileURLToPath(new URL(expected.input, root))], {
      cwd: fileURLToPath(new URL('../../../', import.meta.url)), encoding: 'utf8', maxBuffer: 32 * 1024 * 1024,
    }).trim();
    assert.equal(JSON.stringify(actual), native);
  } finally { value.free(); }
});
