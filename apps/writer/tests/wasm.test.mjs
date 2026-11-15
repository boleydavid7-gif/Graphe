import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import { TextDecoder, TextEncoder } from 'node:util';
import { URL } from 'node:url';
import initialize, { WriterEngine } from '../src/wasm/office_wasm.js';

// Execute the real release WASM and generated target-web ABI in V8. This suite
// complements native Rust tests and production browser tests; it mocks neither.
const binary = await readFile(new URL('../src/wasm/office_wasm_bg.wasm', import.meta.url));
const exports = await initialize({ module_or_path: binary });
const encoder = new TextEncoder();
const decoder = new TextDecoder();

function engine(context) {
  const instance = new WriterEngine(context.name);
  context.after(() => instance.free());
  return instance;
}

function snapshot(instance) {
  return JSON.parse(instance.snapshot());
}

function dispatch(instance, command) {
  const response = JSON.parse(instance.dispatch(JSON.stringify(command)));
  assert.equal(response.ok, true, JSON.stringify(response));
  return response.view;
}

function reject(instance, command, code) {
  const before = instance.snapshot();
  const bytes = instance.export_document();
  const response = JSON.parse(instance.dispatch(JSON.stringify(command)));
  assert.equal(response.ok, false);
  assert.equal(response.error.code, code);
  assert.equal(typeof response.error.message, 'string');
  assert.equal(instance.snapshot(), before);
  assert.deepEqual(instance.export_document(), bytes);
}

function selection(paragraphId, anchor, focus = anchor) {
  return {
    anchor: { paragraphId, utf16: anchor },
    focus: { paragraphId, utf16: focus },
  };
}

function text(view) {
  return view.paragraphs.map((paragraph) => paragraph.runs.map((run) => run.text).join(''));
}

function bytes(value) {
  return encoder.encode(JSON.stringify(value));
}

function sameSupportedFields(actual, expected) {
  if (Array.isArray(expected)) {
    assert.equal(actual.length, expected.length);
    expected.forEach((item, index) => sameSupportedFields(actual[index], item));
  } else if (expected !== null && typeof expected === 'object') {
    for (const [key, value] of Object.entries(expected)) {
      assert.ok(Object.hasOwn(actual, key), `Missing supported field ${key}`);
      sameSupportedFields(actual[key], value);
    }
  } else assert.deepEqual(actual, expected);
}

test('real WASM initializes and owning handles create independent documents and free', (context) => {
  assert.ok(exports.memory instanceof globalThis.WebAssembly.Memory);
  const instance = engine(context);
  const initial = snapshot(instance);
  assert.equal(initial.title, '');
  assert.equal(initial.paragraphs.length, 1);
  assert.deepEqual(text(initial), ['']);
  assert.equal(initial.canUndo, false);
  assert.equal(initial.canRedo, false);
  assert.equal(typeof initial.revision, 'string');
  assert.equal(typeof initial.sessionId, 'string');
  const disposable = new WriterEngine('disposable-handle');
  disposable.free();
  // Freeing one handle must not invalidate the remaining live instance.
  assert.equal(dispatch(instance, { type: 'set_title', title: 'Still live' }).title, 'Still live');
  assert.throws(() => new WriterEngine(' '), (error) => JSON.parse(error).code === 'invalid_namespace');
});

test('WASM ABI rejects stale, unknown and invalid commands without changing document or history', (context) => {
  const instance = engine(context);
  const stale = snapshot(instance).revision;
  dispatch(instance, { type: 'replace_text', text: 'Validated content' });
  reject(instance, { type: 'replace_text', text: 'stale', expectedRevision: stale }, 'stale_revision');
  reject(instance, { type: 'new_document', extra: true }, 'invalid_command');
  reject(instance, { type: 'undo', extra: true }, 'invalid_command');
  reject(instance, { type: 'format_character', fontSizePt: 0 }, 'invalid_format');
  reject(instance, { type: 'replace_text', text: 'bad', selection: selection('missing', 0) }, 'invalid_selection');
  const result = JSON.parse(instance.dispatch('{broken'));
  assert.equal(result.ok, false);
  assert.equal(result.error.code, 'invalid_command');
  dispatch(instance, { type: 'undo' });
  assert.deepEqual(text(snapshot(instance)), ['']);
});

test('WASM selection ABI preserves Unicode and uses strict UTF-16 plus Rust grapheme affinity', (context) => {
  const instance = engine(context);
  const paragraphId = snapshot(instance).paragraphs[0].id;
  const value = 'Ae\u0301👩🏽‍💻🇺🇳Z';
  dispatch(instance, { type: 'replace_text', text: value });
  assert.equal(snapshot(instance).selection.focus.utf16, value.length);
  let view = dispatch(instance, { type: 'set_selection', selection: selection(paragraphId, 2), affinity: 'leading' });
  assert.equal(view.selection.focus.utf16, 1);
  view = dispatch(instance, { type: 'set_selection', selection: selection(paragraphId, 2), affinity: 'trailing' });
  assert.equal(view.selection.focus.utf16, 3);
  reject(instance, { type: 'set_selection', selection: selection(paragraphId, 4) }, 'invalid_selection');
  view = dispatch(instance, { type: 'set_selection', selection: selection(paragraphId, 10, 7), affinity: 'trailing' });
  assert.equal(view.selection.anchor.utf16, 10);
  assert.equal(view.selection.focus.utf16, 10);
  dispatch(instance, { type: 'set_selection', selection: selection(paragraphId, value.length - 1) });
  dispatch(instance, { type: 'delete', direction: 'backward' });
  assert.deepEqual(text(snapshot(instance)), ['Ae\u0301👩🏽‍💻Z']);
  dispatch(instance, { type: 'undo' });
  assert.deepEqual(text(snapshot(instance)), [value]);
  dispatch(instance, { type: 'set_selection', selection: selection(paragraphId, 10, 1) });
  const beforeRead = instance.snapshot();
  const beforeBytes = instance.export_document();
  assert.equal(instance.selected_text(), 'e\u0301👩🏽‍💻');
  assert.equal(instance.snapshot(), beforeRead);
  assert.deepEqual(instance.export_document(), beforeBytes);
  dispatch(instance, { type: 'set_selection', selection: selection(paragraphId, 3) });
  assert.equal(instance.selected_text(), '');
});

test('multiline insertion and paragraph joins use one Rust commit and exact undo/redo across the ABI', (context) => {
  const instance = engine(context);
  const original = instance.export_document();
  const view = dispatch(instance, { type: 'replace_text', text: 'first\r\nCafe\u0301\r👩🏽‍💻' });
  assert.deepEqual(text(view), ['first', 'Cafe\u0301', '👩🏽‍💻']);
  const committed = instance.export_document();
  dispatch(instance, { type: 'set_selection', selection: {
    anchor: { paragraphId: view.paragraphs[2].id, utf16: 7 },
    focus: { paragraphId: view.paragraphs[0].id, utf16: 2 },
  } });
  const beforeRead = instance.snapshot();
  assert.equal(instance.selected_text(), 'rst\nCafe\u0301\n👩🏽‍💻');
  assert.equal(instance.snapshot(), beforeRead);
  assert.deepEqual(instance.export_document(), committed);
  dispatch(instance, { type: 'undo' });
  assert.deepEqual(instance.export_document(), original);
  assert.equal(snapshot(instance).canUndo, false);
  dispatch(instance, { type: 'redo' });
  assert.deepEqual(instance.export_document(), committed);
  dispatch(instance, { type: 'set_selection', selection: selection(view.paragraphs[1].id, 0) });
  dispatch(instance, { type: 'delete', direction: 'backward' });
  assert.deepEqual(text(snapshot(instance)), ['firstCafe\u0301', '👩🏽‍💻']);
  dispatch(instance, { type: 'undo' });
  assert.deepEqual(instance.export_document(), committed);
});

test('mixed character and paragraph formatting cross the WASM boundary and undo exactly', (context) => {
  const instance = engine(context);
  const paragraphId = snapshot(instance).paragraphs[0].id;
  dispatch(instance, { type: 'replace_text', text: 'plain bold text' });
  const plain = instance.export_document();
  dispatch(instance, { type: 'set_selection', selection: selection(paragraphId, 6, 10) });
  let view = dispatch(instance, {
    type: 'format_character', bold: true, italic: true, underline: true, fontFamily: 'Georgia', fontSizePt: 14.5,
  });
  const formatted = view.paragraphs[0].runs.find((run) => run.text === 'bold');
  assert.deepEqual(formatted.format, {
    bold: true, italic: true, underline: 'single', fontFamily: 'Georgia', fontSizePt: 14.5, color: '#24211d',
  });
  view = dispatch(instance, { type: 'set_selection', selection: selection(paragraphId, 0, 15) });
  assert.equal(view.format.bold, null);
  assert.equal(view.format.fontFamily, null);
  view = dispatch(instance, { type: 'format_paragraph', alignment: 'center' });
  assert.equal(view.paragraphs[0].format.alignment, 'center');
  dispatch(instance, { type: 'undo' });
  dispatch(instance, { type: 'undo' });
  assert.deepEqual(instance.export_document(), plain);
});

test('native fixture round trips exact supported structures, styles, identifiers and byte ownership', async (context) => {
  const instance = engine(context);
  const fixture = await readFile(new URL('../../../tests/fixtures/native/writer-v1.writer', import.meta.url));
  const source = JSON.parse(decoder.decode(fixture));
  const opened = JSON.parse(instance.open_document(fixture, source.document.id));
  assert.equal(opened.ok, true);
  assert.equal(opened.view.title, source.document.metadata.title);
  const canonical = instance.export_document();
  assert.ok(canonical instanceof Uint8Array);
  sameSupportedFields(JSON.parse(decoder.decode(canonical)), {
    ...source, format_version: 3,
    document: { ...source.document, schema_version: 3 },
  });
  const other = new WriterEngine('fixture-reopened');
  context.after(() => other.free());
  assert.equal(JSON.parse(other.open_document(canonical, source.document.id)).ok, true);
  assert.deepEqual(other.export_document(), canonical);
  canonical.fill(0);
  assert.deepEqual(other.export_document(), instance.export_document());
  assert.equal(opened.view.paragraphs[0].runs[0].format.bold, true);
  assert.equal(opened.view.paragraphs[0].runs[0].format.fontFamily, 'Noto Sans');
  assert.equal(opened.view.canUndo, false);
});

test('corrupted, unsupported and identity-mismatched native bytes preserve the live WASM session and history', (context) => {
  const instance = engine(context);
  dispatch(instance, { type: 'replace_text', text: 'Keep unsaved work 👩🏽‍💻' });
  const original = instance.export_document();
  const before = instance.snapshot();
  const native = JSON.parse(decoder.decode(original));
  const futureFormat = globalThis.structuredClone(native);
  futureFormat.format_version = 999;
  const futureModel = globalThis.structuredClone(native);
  futureModel.document.schema_version = 999;
  const invalid = globalThis.structuredClone(native);
  invalid.document.sections[0].blocks[0].content.id = invalid.document.id;
  for (const input of [encoder.encode('{broken'), Uint8Array.of(0xff), bytes(futureFormat), bytes(futureModel), bytes(invalid)]) {
    const response = JSON.parse(instance.open_document(input));
    assert.equal(response.ok, false);
    assert.equal(response.error.code, 'invalid_file');
    assert.equal(typeof response.error.message, 'string');
    assert.equal(instance.snapshot(), before);
    assert.deepEqual(instance.export_document(), original);
  }
  const mismatch = JSON.parse(instance.open_document(original, 'different-record-id'));
  assert.equal(mismatch.ok, false);
  assert.equal(mismatch.error.code, 'document_mismatch');
  assert.equal(instance.snapshot(), before);
  dispatch(instance, { type: 'undo' });
  assert.deepEqual(text(snapshot(instance)), ['']);
});

test('oversized edits and files fail without publishing, while empty native files remain structurally exact', (context) => {
  const instance = engine(context);
  dispatch(instance, { type: 'replace_text', text: 'Preserved' });
  reject(instance, { type: 'replace_text', text: '\n'.repeat(100_000) }, 'document_too_large');
  reject(instance, { type: 'replace_text', text: 'x'.repeat(200_001) }, 'document_too_large');
  const before = instance.snapshot();
  const response = JSON.parse(instance.open_document(new Uint8Array(5 * 1024 * 1024 + 1)));
  assert.equal(response.ok, false);
  assert.equal(response.error.code, 'document_too_large');
  assert.equal(instance.snapshot(), before);
  const empty = { format: 'office-writer', format_version: 1, document: {
    schema_version: 1, id: 'empty-document', metadata: { title: '' }, sections: [], styles: [],
  } };
  const migratedEmpty = { ...empty, format_version: 3,
    document: { ...empty.document, schema_version: 3 } };
  assert.equal(JSON.parse(instance.open_document(bytes(empty))).ok, true);
  assert.deepEqual(snapshot(instance).paragraphs, []);
  assert.deepEqual(JSON.parse(decoder.decode(instance.export_document())), migratedEmpty);
  dispatch(instance, { type: 'replace_text', text: 'First edit' });
  assert.deepEqual(text(snapshot(instance)), ['First edit']);
  dispatch(instance, { type: 'undo' });
  assert.deepEqual(JSON.parse(decoder.decode(instance.export_document())), migratedEmpty);
});
