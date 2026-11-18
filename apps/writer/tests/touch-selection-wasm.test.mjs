import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import { URL } from 'node:url';
import { TextEncoder } from 'node:util';
import { WriterEngine } from '../src/wasm/office_wasm.js';
import { initializeWithFonts } from './font-catalog-helper.mjs';

// Compiled Rust ABI: fixtures and expected endpoints are test data, never a
// JavaScript word breaker, grapheme segmenter, document or layout engine.
await initializeWithFonts();

function engine(context) {
  const instance = new WriterEngine(context.name);
  context.after(() => instance.free());
  return instance;
}
const snapshot = (instance) => JSON.parse(instance.snapshot());
function dispatch(instance, command) {
  const response = JSON.parse(instance.dispatch(JSON.stringify(command)));
  assert.equal(response.ok, true, JSON.stringify(response));
  return response.view;
}
function layout(instance) {
  return JSON.parse(instance.layout_snapshot('{}', snapshot(instance).revision)).layout;
}
function action(instance, command, revision = snapshot(instance).revision) {
  return JSON.parse(instance.layout_action('{}', JSON.stringify(command), revision));
}
function fixture(paragraphs, smallPages = false) {
  return new TextEncoder().encode(JSON.stringify({
    format: 'office-writer', format_version: 2,
    document: {
      schema_version: 2, id: 'touch-audit-document', metadata: { title: 'Touch selection' }, styles: [],
      sections: [{ id: 'touch-section', properties: smallPages ? {
        page_size: { width: 4800, height: 3600, orientation: null },
        page_margins: { top: 240, right: 240, bottom: 240, left: 240, header: 0, footer: 0, gutter: 0 },
      } : null, blocks: paragraphs.map((text, index) => ({ kind: 'paragraph', content: {
        id: `touch-paragraph-${index}`, style: null, format: {}, runs: [{ text, format: {} }],
      } })) }],
    },
  }));
}
function open(instance, bytes) {
  const response = JSON.parse(instance.open_document(bytes));
  assert.equal(response.ok, true, JSON.stringify(response));
  return response.view;
}
function caretPoint(measured, paragraphId, utf16) {
  for (const page of measured.pages) for (const line of page.lines) {
    if (line.paragraphId !== paragraphId) continue;
    const caret = line.carets.find((candidate) => candidate.utf16 === utf16);
    if (caret) return { pageNumber: page.number, x: caret.x, y: caret.y + Math.floor(caret.height / 2) };
  }
  throw new Error(`No Rust caret at ${paragraphId}:${utf16}`);
}
function wordPoint(measured, paragraphId, start, end) {
  const first = caretPoint(measured, paragraphId, start);
  const last = caretPoint(measured, paragraphId, end);
  assert.equal(first.pageNumber, last.pageNumber, 'Test words must fit on one line');
  assert.equal(first.y, last.y, 'Test words must fit on one line');
  return { pageNumber: first.pageNumber, x: Math.round((first.x + last.x) / 2), y: first.y };
}

test('compiled WASM selects Rust Unicode words and complete emoji without content, revision or history changes', (context) => {
  const instance = engine(context);
  const text = 'Cafe\u0301 👩🏽‍💻 नमस्ते العربية שלום';
  dispatch(instance, { type: 'replace_text', text });
  const before = snapshot(instance);
  const bytes = instance.export_document();
  const measured = layout(instance);
  const paragraphId = before.paragraphs[0].id;
  for (const word of ['Cafe\u0301', '👩🏽‍💻', 'नमस्ते', 'العربية', 'שלום']) {
    const start = text.indexOf(word);
    const selected = action(instance, { type: 'select_word', ...wordPoint(measured, paragraphId, start, start + word.length) });
    assert.equal(instance.selected_text(), word);
    assert.deepEqual(selected.view.selection, {
      anchor: { paragraphId, utf16: start }, focus: { paragraphId, utf16: start + word.length },
    });
    assert.equal(selected.view.revision, before.revision);
    assert.equal(selected.view.canUndo, before.canUndo);
    assert.equal(selected.view.canRedo, before.canRedo);
    assert.deepEqual(instance.export_document(), bytes);
  }
  dispatch(instance, { type: 'undo' });
  assert.equal(snapshot(instance).paragraphs[0].runs.map((run) => run.text).join(''), '');
  assert.equal(snapshot(instance).canUndo, false, 'Selection must not add undo entries');
});

test('compiled WASM independently drags logical endpoints across pages and past each other', (context) => {
  const instance = engine(context);
  const long = 'Review dependable selection and stable source positions. '.repeat(55);
  open(instance, fixture(['alpha beta gamma', long], true));
  const before = snapshot(instance);
  const bytes = instance.export_document();
  const measured = layout(instance);
  assert.ok(measured.pages.length > 3);
  const firstId = before.paragraphs[0].id;
  const lastId = before.paragraphs[1].id;
  const selected = action(instance, { type: 'select_word', ...wordPoint(measured, firstId, 0, 5) });
  assert.equal(instance.selected_text(), 'alpha');
  const fixed = selected.view.selection.anchor;
  const fixedGeometry = selected.geometry.anchor;
  const dragged = action(instance, { type: 'drag_endpoint', endpoint: 'focus', ...caretPoint(measured, lastId, long.length) });
  assert.deepEqual(dragged.view.selection.anchor, fixed);
  assert.deepEqual(dragged.geometry.anchor, fixedGeometry);
  assert.deepEqual(dragged.view.selection.focus, { paragraphId: lastId, utf16: long.length });
  assert.ok(new Set(dragged.geometry.rectangles.map((rectangle) => rectangle.pageNumber)).size > 3);

  // Cross the moving anchor past the original focus. Handle identity remains
  // logical anchor/focus; screen left/right ordering must not silently swap it.
  action(instance, { type: 'select_word', ...wordPoint(measured, firstId, 0, 5) });
  const reversed = action(instance, { type: 'drag_endpoint', endpoint: 'anchor', ...caretPoint(measured, firstId, 10) });
  assert.deepEqual(reversed.view.selection.anchor, { paragraphId: firstId, utf16: 10 });
  assert.deepEqual(reversed.view.selection.focus, { paragraphId: firstId, utf16: 5 });
  assert.equal(instance.selected_text(), ' beta');
  const crossedFocus = action(instance, { type: 'drag_endpoint', endpoint: 'focus', ...caretPoint(measured, firstId, 14) });
  assert.deepEqual(crossedFocus.view.selection.anchor, reversed.view.selection.anchor);
  assert.deepEqual(crossedFocus.view.selection.focus, { paragraphId: firstId, utf16: 14 });
  assert.equal(snapshot(instance).revision, before.revision);
  assert.deepEqual(instance.export_document(), bytes);
});

test('compiled WASM rejects stale, malformed and invalid touch actions atomically', (context) => {
  const instance = engine(context);
  dispatch(instance, { type: 'replace_text', text: 'Keep this selection e\u0301 👩🏽‍💻' });
  const measured = layout(instance);
  const paragraphId = snapshot(instance).paragraphs[0].id;
  const point = wordPoint(measured, paragraphId, 0, 4);
  action(instance, { type: 'select_word', ...point });
  const stale = snapshot(instance).revision;
  dispatch(instance, { type: 'set_title', title: 'A newer revision' });
  const before = instance.snapshot();
  const bytes = instance.export_document();
  assert.throws(() => action(instance, { type: 'select_word', ...point }, stale),
    (error) => JSON.parse(error).code === 'stale_layout');
  for (const command of [
    { type: 'select_word', ...point, future: true },
    { type: 'select_word', ...point, pageNumber: 0 },
    { type: 'drag_endpoint', ...point, endpoint: 'left' },
    { type: 'drag_endpoint', ...point, endpoint: 'focus', pageNumber: 99999 },
    { type: 'drag_endpoint', ...point, endpoint: 'anchor', x: 64_000_001 },
  ]) {
    assert.throws(() => action(instance, command), (error) => {
      const structured = JSON.parse(error);
      return typeof structured.code === 'string' && typeof structured.message === 'string';
    });
    assert.equal(instance.snapshot(), before);
    assert.deepEqual(instance.export_document(), bytes);
  }
});

test('compiled WASM touch selection preserves exact imported originals and source-bearing native files', async (context) => {
  const instance = engine(context);
  const original = await readFile(new URL('../../../tests/fixtures/docx/relationships/supported-auxiliary-parts.docx', import.meta.url));
  const response = JSON.parse(instance.import_docx(original, 'Original.docx'));
  assert.equal(response.ok, true, JSON.stringify(response));
  const before = snapshot(instance);
  const native = instance.export_document();
  const measured = layout(instance);
  const first = measured.pages.flatMap((page) => page.lines).find((line) => line.carets.length > 1);
  assert.ok(first);
  const caret = first.carets[0];
  const page = measured.pages.find((candidate) => candidate.lines.includes(first));
  action(instance, { type: 'select_word', pageNumber: page.number, x: caret.x + 1, y: caret.y });
  action(instance, { type: 'drag_endpoint', endpoint: 'focus', ...caretPoint(measured, first.paragraphId, first.textRange.endUtf16) });
  assert.deepEqual(instance.export_document(), native);
  assert.deepEqual(instance.export_original(), new Uint8Array(original));
  assert.equal(snapshot(instance).revision, before.revision);
  assert.equal(snapshot(instance).canUndo, before.canUndo);
});

test('compiled WASM empty paragraphs remain valid while absent selections reject endpoint dragging', (context) => {
  const instance = engine(context);
  open(instance, fixture(['']));
  const before = instance.export_document();
  const measured = layout(instance);
  const paragraphId = snapshot(instance).paragraphs[0].id;
  const point = caretPoint(measured, paragraphId, 0);
  const selected = action(instance, { type: 'select_word', ...point });
  assert.deepEqual(selected.view.selection.anchor, selected.view.selection.focus);
  assert.equal(instance.selected_text(), '');
  dispatch(instance, { type: 'set_selection', selection: null });
  const absent = instance.snapshot();
  assert.throws(() => action(instance, { type: 'drag_endpoint', endpoint: 'focus', ...point }),
    (error) => JSON.parse(error).code === 'invalid_selection');
  assert.equal(instance.snapshot(), absent);
  assert.deepEqual(instance.export_document(), before);
});
