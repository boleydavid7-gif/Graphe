import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import { fileURLToPath, URL } from 'node:url';
import { TextEncoder } from 'node:util';
import { WriterEngine } from '../src/wasm/office_wasm.js';
import { initializeWithFonts } from './font-catalog-helper.mjs';
import { execCargo } from './native-cargo.mjs';

// Exercise the release engine and generated target-web ABI directly. The layout
// and font catalog in these tests are Rust implementations, not JS substitutes.
await initializeWithFonts();
const encoder = new TextEncoder();

function engine(context) {
  const instance = new WriterEngine(context.name);
  context.after(() => instance.free());
  return instance;
}

function snapshot(instance) { return JSON.parse(instance.snapshot()); }
function layout(instance, options = {}) {
  return JSON.parse(instance.layout_snapshot(JSON.stringify(options), snapshot(instance).revision));
}
function dispatch(instance, command) {
  const result = JSON.parse(instance.dispatch(JSON.stringify(command)));
  assert.equal(result.ok, true, JSON.stringify(result));
  return result.view;
}
function open(instance, value) {
  const bytes = encoder.encode(JSON.stringify(value));
  const response = JSON.parse(instance.open_document(bytes));
  assert.equal(response.ok, true, JSON.stringify(response));
  return response.view;
}

function writerFile(paragraphs, properties = null) {
  return {
    format: 'office-writer', format_version: 2,
    document: {
      schema_version: 2, id: 'layout-fixture', metadata: { title: 'Typography fixture' }, styles: [],
      sections: [{ id: 'layout-section', properties, blocks: paragraphs.map((paragraph, index) => ({
        kind: 'paragraph', content: { id: `layout-paragraph-${index}`, style: null, format: {},
          runs: [{ text: paragraph, format: {} }] },
      })) }],
    },
  };
}

function lines(result) { return result.layout.pages.flatMap((page) => page.lines); }
function unique(values) { return [...new Set(values)].sort((a, b) => a - b); }
function measured(instance, value, format = {}) {
  const file = writerFile([value]);
  file.document.sections[0].blocks[0].content.runs[0].format = format;
  open(instance, file);
  return lines(layout(instance))[0];
}

test('pinned font bytes and redistributed license notices match the catalog provenance manifest', async () => {
  const root = new URL('../../../crates/office-layout/fonts/', import.meta.url);
  const manifest = JSON.parse(await readFile(new URL('manifest.json', root), 'utf8'));
  assert.equal(manifest.format_version, 1);
  assert.equal(manifest.catalog_id, 'writer-fonts-v6');
  const entries = new Map(manifest.entries.map((entry) => [entry.file, entry]));
  let fontBytes = 0;
  for (const entry of [...manifest.entries, ...manifest.derived_files]) {
    assert.match(entry.file, /^[A-Za-z0-9._-]+$/);
    const bytes = await readFile(new URL(entry.file, root));
    assert.equal(bytes.length, entry.byte_length, entry.file);
    assert.equal(createHash('sha256').update(bytes).digest('hex'), entry.sha256, entry.file);
    if (entry.kind === 'font') {
      fontBytes += bytes.length;
      assert.equal(entries.get(entry.license_file)?.kind, 'license', entry.file);
      assert.match(entry.commit, /^[0-9a-f]{40}$/);
      assert.ok(entry.url.includes(entry.commit), 'Download URLs must use immutable upstream commits');
    }
  }
  assert.equal(fontBytes, manifest.font_bytes);
  const notices = await readFile(new URL('NOTICE.txt', root), 'utf8');
  for (const entry of manifest.entries.filter((entry) => entry.kind === 'license')) {
    assert.equal(entry.spdx, 'OFL-1.1');
    // Original license and aggregate notice bytes are independently pinned
    // above; compare complete legal text allowing their documented CRLF/LF
    // delivery differences, without rewriting any upstream asset.
    const license = await readFile(new URL(entry.file, root), 'utf8');
    assert.ok(notices.replace(/\r\n/g, '\n').includes(license.replace(/\r\n/g, '\n')), entry.file);
  }
});

test('expanded pinned font families render their actual bundled outlines', (context) => {
  const instance = engine(context);
  for (const [family, filePrefix] of [
    ['Carlito', 'Carlito-'],
    ['Caladea', 'Caladea-'],
    ['Source Sans 3', 'SourceSans3-'],
    ['EB Garamond', 'EBGaramond-'],
    ['JetBrains Mono', 'JetBrainsMono-'],
    ['Inter', 'Inter-'],
    ['Libre Baskerville', 'LibreBaskerville-'],
    ['Playfair Display', 'PlayfairDisplay-'],
    ['Space Grotesk', 'SpaceGrotesk-'],
  ]) {
    const file = writerFile(['The quick brown fox jumps over the lazy dog.']);
    file.document.sections[0].blocks[0].content.runs[0].format = {
      font_family: family, bold: false, italic: false,
    };
    open(instance, file);
    const output = layout(instance).layout;
    assert.ok(output.pages[0].lines.length, family);
    assert.ok(output.outlines.some((outline) => outline.fontId.startsWith(filePrefix)),
      `${family} was not shaped with its pinned family`);
    assert.ok(!output.diagnostics.some((item) => item.kind === 'font_substitution'),
      `${family} silently fell back to another family`);
  }
});

test('compiled WASM layout initializes, is deterministic, and does not edit the live session', (context) => {
  const instance = engine(context);
  dispatch(instance, { type: 'replace_text', text: 'AV office — Cafe\u0301 👩🏽‍💻 العربية नमस्ते שלום' });
  const before = instance.snapshot();
  const bytes = instance.export_document();
  const result = layout(instance);
  const view = snapshot(instance);
  assert.equal(result.sessionId, view.sessionId);
  assert.equal(result.revision, view.revision);
  assert.equal(result.documentId, view.documentId);
  assert.ok(!result.layout.diagnostics.some((diagnostic) => diagnostic.kind === 'missing_glyph'));
  assert.deepEqual(layout(instance), result);
  assert.equal(instance.snapshot(), before);
  assert.deepEqual(instance.export_document(), bytes);
  dispatch(instance, { type: 'undo' });
  assert.equal(snapshot(instance).paragraphs[0].runs.map((run) => run.text).join(''), '');
  dispatch(instance, { type: 'redo' });
  assert.deepEqual(layout(instance).layout, result.layout);
});

test('compiled WASM rejects stale and malformed layout requests without altering content or history', (context) => {
  const instance = engine(context);
  const stale = snapshot(instance).revision;
  dispatch(instance, { type: 'replace_text', text: 'Keep this unsaved work 👩🏽‍💻' });
  const before = instance.snapshot();
  const bytes = instance.export_document();
  assert.throws(() => instance.layout_snapshot('{}', stale), (error) => JSON.parse(error).code === 'stale_layout');
  for (const options of ['{broken', '{"unknownOption":true}', 'null']) {
    assert.throws(() => instance.layout_snapshot(options, snapshot(instance).revision), (error) => {
      const parsed = JSON.parse(error);
      return typeof parsed.code === 'string' && typeof parsed.message === 'string';
    });
    assert.equal(instance.snapshot(), before);
    assert.deepEqual(instance.export_document(), bytes);
  }
  assert.equal(layout(instance).revision, snapshot(instance).revision);
  dispatch(instance, { type: 'undo' });
  assert.equal(snapshot(instance).paragraphs[0].runs.map((run) => run.text).join(''), '');
});

test('compiled WASM reports unsupported tab layout without repairing text, source, selection or history', (context) => {
  const instance = engine(context);
  open(instance, writerFile(['Before\tAfter']));
  dispatch(instance, { type: 'set_title', title: 'Keep this history entry' });
  const before = instance.snapshot();
  const bytes = instance.export_document();
  assert.throws(() => instance.layout_snapshot('{}', snapshot(instance).revision), (error) => {
    const parsed = JSON.parse(error);
    return parsed.code === 'unsupported_layout' && /tab/i.test(parsed.message);
  });
  assert.equal(instance.snapshot(), before);
  assert.deepEqual(instance.export_document(), bytes);
  dispatch(instance, { type: 'undo' });
  assert.equal(snapshot(instance).title, 'Typography fixture');
});

test('compiled WASM save and reopen preserve the exact Rust layout independently of session identity', (context) => {
  const instance = engine(context);
  dispatch(instance, { type: 'replace_text', text: 'First paragraph 👩🏽‍💻\nSecond paragraph e\u0301 العربية नमस्ते שלום' });
  const before = layout(instance);
  const bytes = instance.export_document();
  const reopened = new WriterEngine(`${context.name}-reopened`);
  context.after(() => reopened.free());
  const response = JSON.parse(reopened.open_document(bytes, snapshot(instance).documentId));
  assert.equal(response.ok, true, JSON.stringify(response));
  const after = layout(reopened);
  assert.notEqual(after.sessionId, before.sessionId);
  assert.deepEqual(after.layout, before.layout);
  assert.deepEqual(reopened.export_document(), bytes);
});

test('compiled WASM maps core grapheme boundaries to UTF-8, UTF-16, and positioned glyph outlines', (context) => {
  const instance = engine(context);
  const value = 'Ae\u0301👩🏽‍💻🇺🇳Z';
  open(instance, writerFile([value]));
  const result = layout(instance);
  assert.equal(result.layout.unitsPerPoint, 64);
  assert.equal(result.layout.documentId, 'layout-fixture');
  assert.equal(typeof result.layout.fontCatalogId, 'string');
  const paragraphLines = lines(result);
  assert.equal(paragraphLines.length, 1);
  const carets = paragraphLines.flatMap((line) => line.carets);
  assert.deepEqual(unique(carets.map((caret) => caret.utf8)), [0, 1, 4, 19, 27, 28]);
  assert.deepEqual(unique(carets.map((caret) => caret.utf16)), [0, 1, 3, 10, 14, 15]);
  assert.deepEqual(unique(carets.map((caret) => caret.grapheme)), [0, 1, 2, 3, 4, 5]);
  assert.deepEqual(paragraphLines[0].textRange, {
    startUtf8: 0, endUtf8: 28, startUtf16: 0, endUtf16: 15, startGrapheme: 0, endGrapheme: 5,
  });
  const outlines = new Map(result.layout.outlines.map((outline) => [outline.id, outline]));
  assert.ok([...outlines.values()].some((outline) => outline.path.includes('M')));
  for (const line of paragraphLines) {
    assert.equal(line.paragraphId, 'layout-paragraph-0');
    for (const key of ['x', 'y', 'width', 'height', 'baseline']) assert.ok(Number.isSafeInteger(line[key]), key);
    for (const glyph of line.glyphs) {
      assert.ok(outlines.has(glyph.outlineId));
      assert.ok(Number.isSafeInteger(glyph.x) && Number.isSafeInteger(glyph.y));
      assert.ok(Number.isSafeInteger(glyph.fontSize) && glyph.fontSize > 0);
    }
  }
});

test('compiled WASM lays out an empty paragraph and A4/custom defaults without mutating the native model', (context) => {
  const instance = engine(context);
  const before = instance.export_document();
  const initial = layout(instance);
  assert.equal(initial.layout.pages.length, 1);
  assert.equal(initial.layout.pages[0].width, 612 * 64);
  assert.equal(initial.layout.pages[0].height, 792 * 64);
  assert.equal(lines(initial).length, 1);
  assert.equal(lines(initial)[0].glyphs.length, 0);
  assert.deepEqual(unique(lines(initial)[0].carets.map((caret) => caret.utf16)), [0]);
  const a4 = layout(instance, { pageSize: { width: 11906, height: 16838, orientation: 'portrait' } });
  assert.equal(a4.layout.pages[0].width, Math.round(11906 / 20 * 64));
  assert.equal(a4.layout.pages[0].height, Math.round(16838 / 20 * 64));
  assert.deepEqual(instance.export_document(), before);
  for (const options of [
    { pageSize: { width: 0, height: 16838 } },
    { fontSize: 0 },
    { pageMargins: { top: 20000, right: 1440, bottom: 1440, left: 1440, header: 0, footer: 0, gutter: 0 } },
  ]) {
    assert.throws(() => instance.layout_snapshot(JSON.stringify(options), snapshot(instance).revision), (error) => {
      const parsed = JSON.parse(error);
      return typeof parsed.code === 'string' && typeof parsed.message === 'string';
    });
    assert.deepEqual(instance.export_document(), before);
  }
  const empty = writerFile([]);
  empty.document.sections = [];
  open(instance, empty);
  const emptyBytes = instance.export_document();
  const blank = layout(instance);
  assert.equal(blank.layout.pages.length, 1);
  assert.deepEqual(blank.layout.pages[0].lines, []);
  assert.deepEqual(blank.layout.outlines, []);
  assert.deepEqual(instance.export_document(), emptyBytes, 'A physical blank page must not synthesize model content');
});

test('compiled WASM measures kerning, font size, mixed formatting, and complex scripts', (context) => {
  const instance = engine(context);
  const pair = measured(instance, 'AV').width;
  const separate = measured(instance, 'A').width + measured(instance, 'V').width;
  assert.ok(pair < separate, `AV pair advance ${pair} should be smaller than separate ${separate}`);
  const small = measured(instance, 'Measurement', { font_size: 24 }).width;
  const large = measured(instance, 'Measurement', { font_size: 48 }).width;
  assert.ok(Math.abs(large - small * 2) <= 2, 'Unhinted shaping should scale to the specified point size');
  const mixed = writerFile(['']);
  mixed.document.sections[0].blocks[0].content.runs = [
    { text: 'Regular ', format: { font_size: 24 } },
    { text: 'Bold italic', format: { bold: true, italic: true, font_size: 36, underline: 'double', color: { kind: 'rgb', red: 17, green: 34, blue: 51 } } },
    { text: ' العربية नमस्ते שלום 👩🏽‍💻', format: {} },
  ];
  open(instance, mixed);
  const result = layout(instance);
  const glyphs = lines(result).flatMap((line) => line.glyphs);
  assert.ok(new Set(glyphs.map((glyph) => glyph.fontSize)).size >= 2);
  assert.ok(lines(result).some((line) => line.decorations.length >= 2));
  assert.ok(new Set(result.layout.outlines.map((outline) => outline.fontId)).size >= 5, 'Latin, Arabic, Devanagari, Hebrew and emoji use pinned fonts');
  const isolated = measured(instance, 'م');
  const isolatedId = layout(instance).layout.outlines.find((outline) => outline.id === isolated.glyphs[0].outlineId).glyphId;
  const joined = measured(instance, 'مم');
  const joiningOutlines = new Map(layout(instance).layout.outlines.map((outline) => [outline.id, outline.glyphId]));
  assert.ok(joined.glyphs.every((glyph) => joiningOutlines.get(glyph.outlineId) !== isolatedId),
    'Arabic joining must select contextual forms rather than isolated glyphs');
  assert.equal(measured(instance, 'क्ष').glyphs.length, 1, 'The pinned Devanagari font supplies a conjunct ligature');
  assert.equal(measured(instance, '👩🏽‍💻').glyphs.length, 1, 'The pinned emoji font supplies a multi-codepoint ZWJ ligature');
});

test('compiled WASM resolves pinned families and keeps weight/style variation outlines distinct', (context) => {
  const instance = engine(context);
  const arimo = measured(instance, 'Measured family width', { font_family: 'Arimo' }).width;
  const tinos = measured(instance, 'Measured family width', { font_family: 'Tinos' }).width;
  assert.notEqual(arimo, tinos);
  const file = writerFile(['']);
  file.document.sections[0].blocks[0].content.runs = [
    { text: 'A', format: { font_family: 'Arimo', bold: false, italic: false } },
    { text: 'A', format: { font_family: 'Arimo', bold: true, italic: false } },
    { text: 'A', format: { font_family: 'Arimo', bold: false, italic: true } },
    { text: 'A', format: { font_family: 'Arimo', bold: true, italic: true } },
  ];
  open(instance, file);
  const result = layout(instance);
  const glyphs = lines(result).flatMap((line) => line.glyphs);
  assert.equal(glyphs.length, 4);
  const outlines = new Map(result.layout.outlines.map((outline) => [outline.id, outline]));
  assert.equal(new Set(glyphs.map((glyph) => outlines.get(glyph.outlineId).path)).size, 4,
    'Outline reuse must include the variation location as well as font and glyph');
});

test('compiled WASM paginates narrow pages and applies section geometry and explicit page breaks', (context) => {
  const instance = engine(context);
  const margins = { top: 400, right: 400, bottom: 400, left: 400, header: 0, footer: 0, gutter: 0 };
  const file = writerFile(['word '.repeat(350), 'Explicit next page'], {
    page_size: { width: 6000, height: 4000, orientation: 'landscape' }, page_margins: margins,
  });
  file.document.sections[0].blocks[1].content.format = { page_break_before: true };
  open(instance, file);
  const result = layout(instance);
  assert.ok(result.layout.pages.length >= 3);
  for (const page of result.layout.pages) {
    assert.equal(page.width, 300 * 64);
    assert.equal(page.height, 200 * 64);
    assert.deepEqual(page.contentRect, { x: 20 * 64, y: 20 * 64, width: 260 * 64, height: 160 * 64 });
    for (const line of page.lines) {
      assert.ok(line.y >= page.contentRect.y);
      assert.ok(line.y + line.height <= page.contentRect.y + page.contentRect.height);
    }
  }
  const lastPage = result.layout.pages.at(-1);
  assert.equal(lastPage.lines[0].paragraphId, 'layout-paragraph-1');
  assert.ok(!lastPage.lines.some((line) => line.paragraphId === 'layout-paragraph-0'));
  assert.equal(lines(result).filter((line) => line.paragraphId === 'layout-paragraph-0').map((line) => line.text).join(''), 'word '.repeat(350));
});

test('compiled WASM applies paragraph indentation, exact line spacing, alignment and soft breaks', (context) => {
  const instance = engine(context);
  const value = 'Measured wrapping words '.repeat(18);
  const file = writerFile([value, value, 'Centered\nsoft break', 'Right aligned', value]);
  const blocks = file.document.sections[0].blocks;
  blocks[0].content.format = {
    indentation: { left: 720, right: 720, first_line: 360 },
    line_spacing: { kind: 'exact', height: 360 }, spacing_after: 240,
  };
  blocks[1].content.format = {
    indentation: { left: 720, right: 720, first_line: -360 },
    line_spacing: { kind: 'at_least', height: 480 }, spacing_before: 160,
  };
  blocks[2].content.format = { alignment: 'center' };
  blocks[3].content.format = { alignment: 'right' };
  blocks[4].content.format = { alignment: 'justify' };
  open(instance, file);
  const result = layout(instance);
  const all = lines(result);
  const first = all.filter((line) => line.paragraphId === 'layout-paragraph-0');
  const hanging = all.filter((line) => line.paragraphId === 'layout-paragraph-1');
  assert.ok(first.length > 1 && hanging.length > 1);
  assert.equal(first[0].x, 126 * 64);
  assert.equal(first[1].x, 108 * 64);
  assert.equal(hanging[0].x, 90 * 64);
  assert.equal(hanging[1].x, 108 * 64);
  assert.ok(first.every((line) => line.height === 18 * 64));
  assert.ok(hanging.every((line) => line.height >= 24 * 64));
  const centered = all.filter((line) => line.paragraphId === 'layout-paragraph-2');
  assert.equal(centered.length, 2);
  assert.equal(centered.map((line) => line.text).join('').replaceAll('\n', ''), 'Centeredsoft break');
  const body = result.layout.pages[0].contentRect;
  for (const line of centered) assert.ok(Math.abs(line.x - (body.x + (body.width - line.width) / 2)) <= 1);
  const right = all.find((line) => line.paragraphId === 'layout-paragraph-3');
  assert.ok(Math.abs(right.x + right.width - (body.x + body.width)) <= 1);
  const justified = all.filter((line) => line.paragraphId === 'layout-paragraph-4');
  assert.ok(justified.length > 1);
  assert.ok(justified.slice(0, -1).every((line) => Math.abs(line.width - body.width) <= 1));
  assert.ok(justified.at(-1).width < body.width, 'The final line should not be stretched');
});

test('compiled WASM honors DOCX-imported Letter/A4 geometry, formatting and explicit breaks', async (context) => {
  const instance = engine(context);
  const bytes = await readFile(new URL('../../../tests/fixtures/docx/supported-formatting.docx', import.meta.url));
  const response = JSON.parse(instance.import_docx(bytes, 'layout.docx'));
  assert.equal(response.ok, true, JSON.stringify(response));
  const imported = instance.snapshot();
  const importedBytes = instance.export_document();
  assert.throws(() => instance.layout_snapshot('{}', response.view.revision), (error) => JSON.parse(error).code === 'unsupported_layout');
  assert.equal(instance.snapshot(), imported);
  assert.deepEqual(instance.export_document(), importedBytes);
  assert.deepEqual(instance.export_original(), new Uint8Array(bytes));
  for (const paragraph of response.view.paragraphs) {
    const value = paragraph.runs.map((run) => run.text).join('');
    const at = value.indexOf('\t');
    if (at >= 0) dispatch(instance, { type: 'replace_text', text: ' ', selection: {
      anchor: { paragraphId: paragraph.id, utf16: at }, focus: { paragraphId: paragraph.id, utf16: at + 1 },
    } });
  }
  const before = instance.export_document();
  const result = layout(instance);
  assert.ok(result.layout.pages.length >= 2);
  assert.equal(result.layout.pages[0].width, 612 * 64);
  assert.equal(result.layout.pages[0].height, 792 * 64);
  assert.ok(result.layout.pages.some((page) => page.width === Math.round(11906 / 20 * 64) && page.height === Math.round(16838 / 20 * 64)));
  assert.ok(lines(result).some((line) => line.decorations.length > 0));
  assert.ok(result.layout.diagnostics.some((diagnostic) => diagnostic.kind === 'missing_glyph'));
  assert.deepEqual(instance.export_document(), before);
  assert.deepEqual(instance.export_original(), new Uint8Array(bytes));
});

test('compiled WASM layout matches the complete native snapshot golden', async (context) => {
  const instance = engine(context);
  const fixtureRoot = new URL('../../../tests/fixtures/layout/', import.meta.url);
  const expected = JSON.parse(await readFile(new URL('native-parity.json', fixtureRoot), 'utf8'));
  const input = await readFile(new URL(expected.input, fixtureRoot));
  const response = JSON.parse(instance.open_document(input));
  assert.equal(response.ok, true, JSON.stringify(response));
  const result = layout(instance, expected.options);
  const compact = JSON.stringify(result.layout);
  const native = execCargo( ['run', '--offline', '--locked', '--quiet', '-p', 'office-layout',
    '--example', 'layout_snapshot', '--', fileURLToPath(new URL(expected.input, fixtureRoot))], {
    cwd: fileURLToPath(new URL('../../../', import.meta.url)), encoding: 'utf8', maxBuffer: 32 * 1024 * 1024,
  }).trim();
  assert.equal(compact, native, 'Native Rust and release WASM must produce identical complete geometry and outlines');
  // Catalog v5 adds optional faces. The old reference layout still uses
  // only v1 fonts, so normalize the version marker alone before checking the
  // reviewed full-geometry fingerprint. This must not hide changed glyphs,
  // line breaks, coordinates, pages, or outlines.
  const oldMarker = '"fontCatalogId":"writer-fonts-v6"';
  assert.equal(compact.split(oldMarker).length, 2);
  const legacyGeometry = compact.replace(oldMarker, '"fontCatalogId":"writer-fonts-v1"');
  assert.equal(createHash('sha256').update(legacyGeometry).digest('hex'), expected.sha256);
});
