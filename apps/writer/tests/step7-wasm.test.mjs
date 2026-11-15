import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import { URL } from 'node:url';
import { TextDecoder, TextEncoder } from 'node:util';
import initialize, { WriterEngine } from '../src/wasm/office_wasm.js';

await initialize({ module_or_path: await readFile(new URL('../src/wasm/office_wasm_bg.wasm', import.meta.url)) });
const encoder = new TextEncoder();
const decoder = new TextDecoder();

function engine(context) {
  const value = new WriterEngine(context.name);
  context.after(() => value.free());
  return value;
}
function view(instance) { return JSON.parse(instance.snapshot()); }
function dispatch(instance, command) {
  const result = JSON.parse(instance.dispatch(JSON.stringify(command)));
  assert.equal(result.ok, true, JSON.stringify(result));
  return result.view;
}
function layout(instance, options = {}) {
  return JSON.parse(instance.layout_snapshot(JSON.stringify(options), view(instance).revision));
}
function action(instance, value, options = {}) {
  return JSON.parse(instance.layout_action(JSON.stringify(options), JSON.stringify(value), view(instance).revision));
}
function geometry(instance, options = {}) {
  return JSON.parse(instance.selection_geometry(JSON.stringify(options), view(instance).revision));
}
function documentFile(text) {
  return { format: 'office-writer', format_version: 2, document: {
    schema_version: 2, id: 'step7-reference', metadata: { title: 'Interaction reference' }, styles: [],
    sections: [{ id: 'step7-section', properties: null, blocks: [{ kind: 'paragraph', content: {
      id: 'step7-paragraph', style: null, format: {}, runs: [{ text, format: {} }],
    } }] }],
  } };
}
function open(instance, file) {
  const response = JSON.parse(instance.open_document(encoder.encode(JSON.stringify(file))));
  assert.equal(response.ok, true, JSON.stringify(response));
  return response.view;
}

test('actual WASM hit testing maps drawn core boundaries and changes only the Rust selection', (context) => {
  const instance = engine(context);
  const initial = dispatch(instance, { type: 'replace_text', text: 'Ae\u0301👩🏽‍💻Z' });
  const bytes = instance.export_document();
  const line = layout(instance).layout.pages[0].lines[0];
  const caret = line.carets.find((candidate) => candidate.utf16 === 3 && candidate.affinity === 'leading');
  assert.ok(caret);
  const hit = action(instance, { type: 'hit', pageNumber: 1, x: caret.x, y: caret.y + Math.floor(caret.height / 2) });
  assert.equal(hit.view.selection.focus.utf16, 3);
  assert.equal(hit.geometry.focus.position.utf8, 4);
  assert.equal(hit.geometry.focus.position.grapheme, 2);
  assert.equal(hit.view.revision, initial.revision);
  assert.equal(hit.view.canUndo, initial.canUndo);
  assert.equal(hit.view.canRedo, initial.canRedo);
  assert.deepEqual(geometry(instance).geometry, hit.geometry);
  assert.deepEqual(instance.export_document(), bytes);
});

test('actual WASM visual keyboard selection uses complete graphemes and has no separate undo history', (context) => {
  const instance = engine(context);
  dispatch(instance, { type: 'replace_text', text: 'e\u0301👩🏽‍💻Z' });
  const before = instance.export_document();
  action(instance, { type: 'move', direction: 'document_start' });
  const first = action(instance, { type: 'move', direction: 'right', extend: true });
  assert.equal(first.view.selection.focus.utf16, 2);
  assert.equal(instance.selected_text(), 'e\u0301');
  const second = action(instance, { type: 'move', direction: 'right', extend: true });
  assert.equal(second.geometry.focus.position.utf16, 9);
  assert.equal(second.geometry.focus.position.grapheme, 2);
  assert.equal(instance.selected_text(), 'e\u0301👩🏽‍💻');
  assert.ok(second.geometry.rectangles.every((rectangle) => rectangle.rect.width > 0 && rectangle.rect.height > 0));
  assert.deepEqual(instance.export_document(), before);
  dispatch(instance, { type: 'undo' });
  assert.equal(view(instance).canUndo, false);
  assert.equal(view(instance).paragraphs[0].runs.map((run) => run.text).join(''), '');
});

test('actual WASM maps mixed RTL, number prefixes and wrapped page selections to stable UTF-16 ranges', (context) => {
  const instance = engine(context);
  const text = '123 العربية English שלום '.repeat(35);
  const file = documentFile(text);
  const margins = { top: 200, right: 200, bottom: 200, left: 200, header: 0, footer: 0, gutter: 0 };
  file.document.sections[0].properties = { page_size: { width: 3600, height: 2800 }, page_margins: margins };
  open(instance, file);
  const before = instance.export_document();
  const snapshot = layout(instance).layout;
  assert.ok(snapshot.pages.length > 2);
  let tested = 0;
  for (const page of snapshot.pages) {
    for (const line of page.lines) {
      const positions = new Map();
      for (const caret of line.carets) {
        const at = positions.get(caret.x) ?? new Set();
        at.add(caret.utf16);
        positions.set(caret.x, at);
      }
      const target = line.carets.find((caret) => positions.get(caret.x).size === 1
        && caret.utf16 > line.textRange.startUtf16 && caret.utf16 < line.textRange.endUtf16);
      if (!target) continue;
      const hit = action(instance, { type: 'hit', pageNumber: page.number,
        x: target.x, y: target.y + Math.floor(target.height / 2) });
      assert.equal(hit.geometry.focus.position.utf16, target.utf16);
      assert.equal(hit.geometry.focus.position.utf8, target.utf8);
      assert.equal(hit.geometry.focus.pageNumber, page.number);
      tested += 1;
    }
  }
  assert.ok(tested > 10, 'Probe actual visual boundaries across multiple wrapped mixed-direction pages');
  const all = action(instance, { type: 'select_all' });
  assert.equal(instance.selected_text(), text);
  assert.ok(new Set(all.geometry.rectangles.map((rectangle) => rectangle.pageNumber)).size > 2);
  for (const rectangle of all.geometry.rectangles) {
    const page = snapshot.pages.find((candidate) => candidate.number === rectangle.pageNumber);
    assert.equal(rectangle.paragraphId, 'step7-paragraph');
    assert.ok(rectangle.rect.x >= 0 && rectangle.rect.y >= page.contentRect.y);
    assert.ok(rectangle.rect.width > 0 && rectangle.rect.height > 0);
    assert.ok(rectangle.rect.x + rectangle.rect.width <= page.width);
    assert.ok(rectangle.rect.y + rectangle.rect.height <= page.contentRect.y + page.contentRect.height);
  }
  action(instance, { type: 'move', direction: 'document_start' });
  const lineCount = snapshot.pages.reduce((count, page) => count + page.lines.length, 0);
  let previous = geometry(instance).geometry.focus;
  let transitions = 0;
  for (let count = 0; count <= lineCount; count += 1) {
    const next = action(instance, { type: 'move', direction: 'down' });
    const focus = next.geometry.focus;
    if (focus.pageNumber === previous.pageNumber && focus.lineIndex === previous.lineIndex) break;
    assert.ok(focus.pageNumber > previous.pageNumber || focus.lineIndex > previous.lineIndex);
    assert.equal(typeof next.preferredX, 'number');
    previous = focus;
    transitions += 1;
  }
  assert.ok(transitions >= lineCount - 1);
  assert.deepEqual(instance.export_document(), before);
});

test('actual WASM rejects stale and invalid visual actions without changing selection, preferred column or source', (context) => {
  const instance = engine(context);
  const stale = view(instance).revision;
  dispatch(instance, { type: 'replace_text', text: 'Preserved e\u0301 👩🏽‍💻\nSecond' });
  action(instance, { type: 'move', direction: 'up' });
  const before = instance.snapshot();
  const bytes = instance.export_document();
  const selected = geometry(instance);
  assert.throws(() => instance.layout_action('{}', '{"type":"select_all"}', stale), (error) => JSON.parse(error).code === 'stale_layout');
  for (const value of ['{broken', '[]', '{"type":"move","direction":"future"}',
    '{"type":"hit","pageNumber":0,"x":0,"y":0}',
    '{"type":"hit","pageNumber":1,"x":64000001,"y":0}',
    '{"type":"select_all","future":true}']) {
    assert.throws(() => instance.layout_action('{}', value, view(instance).revision), (error) => {
      const code = JSON.parse(error).code;
      return code === 'invalid_layout_action' || code === 'invalid_layout_position';
    });
    assert.equal(instance.snapshot(), before);
    assert.deepEqual(instance.export_document(), bytes);
    assert.deepEqual(geometry(instance), selected);
  }
});

test('actual WASM soft-break navigation uses downstream carets and End before the break', (context) => {
  const instance = engine(context);
  dispatch(instance, { type: 'replace_text', text: 'one' });
  dispatch(instance, { type: 'insert_line_break' });
  dispatch(instance, { type: 'replace_text', text: 'two' });
  action(instance, { type: 'move', direction: 'document_start' });
  const end = action(instance, { type: 'move', direction: 'end' });
  assert.equal(end.geometry.focus.position.utf16, 3);
  assert.equal(end.geometry.focus.lineIndex, 0);
  const next = action(instance, { type: 'move', direction: 'right' });
  assert.equal(next.geometry.focus.position.utf16, 4);
  assert.equal(next.geometry.focus.position.affinity, 'leading');
  assert.equal(next.geometry.focus.lineIndex, 1);
  assert.equal(next.geometry.focus.position.paragraphId, end.geometry.focus.position.paragraphId);
});

test('actual WASM Replace All is one undoable command with stale revision protection', (context) => {
  const instance = engine(context);
  const original = dispatch(instance, { type: 'replace_text', text: 'red red\nred' });
  const replaced = dispatch(instance, { type: 'replace_all', query: 'red', text: 'blue',
    expectedRevision: original.revision });
  assert.deepEqual(replaced.paragraphs.map((p) => p.runs.map((r) => r.text).join('')), ['blue blue', 'blue']);
  const bytes = instance.export_document();
  const stale = JSON.parse(instance.dispatch(JSON.stringify({ type: 'replace_all',
    query: 'blue', text: 'green', expectedRevision: original.revision })));
  assert.equal(stale.ok, false);
  assert.equal(stale.error.code, 'stale_revision');
  assert.deepEqual(instance.export_document(), bytes);
  const undone = dispatch(instance, { type: 'undo' });
  assert.deepEqual(undone.paragraphs.map((p) => p.runs.map((r) => r.text).join('')), ['red red', 'red']);
  assert.equal(undone.canUndo, true);
});

test('actual WASM literal find preserves content and expands partial Unicode hits to complete core graphemes', (context) => {
  const instance = engine(context);
  dispatch(instance, { type: 'replace_text', text: 'Writer writer Ae\u0301\u0301 👩🏽‍💻\nwriter' });
  const before = instance.snapshot();
  const bytes = instance.export_document();
  const lower = JSON.parse(instance.find_text('writer'));
  assert.equal(lower.matches.length, 2);
  assert.equal(JSON.parse(instance.find_text('Writer')).matches.length, 1);
  assert.equal(JSON.parse(instance.find_text('')).matches.length, 0);
  assert.equal(JSON.parse(instance.find_text('é')).matches.length, 0);
  const mark = JSON.parse(instance.find_text('\u0301'));
  assert.equal(mark.matches.length, 1, 'Overlapping scalar hits in the same grapheme are deduplicated');
  const emoji = JSON.parse(instance.find_text('💻'));
  assert.equal(emoji.matches.length, 1);
  assert.equal(instance.snapshot(), before);
  assert.deepEqual(instance.export_document(), bytes);
  dispatch(instance, { type: 'set_selection', selection: emoji.matches[0].selection });
  assert.equal(instance.selected_text(), '👩🏽‍💻');
  dispatch(instance, { type: 'set_selection', selection: mark.matches[0].selection });
  assert.equal(instance.selected_text(), 'e\u0301\u0301');
  const selected = instance.snapshot();
  assert.throws(() => instance.find_text('x'.repeat(1025)), (error) => JSON.parse(error).code === 'invalid_find');
  assert.equal(instance.snapshot(), selected);
  dispatch(instance, { type: 'replace_text', text: 'x '.repeat(1100), selection: null });
  const bounded = JSON.parse(instance.find_text('x'));
  assert.equal(bounded.matches.length, 1000);
  assert.equal(bounded.truncated, true);
});

test('actual WASM named styles apply atomically, expose mixed selections and preserve direct overrides', async (context) => {
  const instance = engine(context);
  const bytes = await readFile(new URL('../../../tests/fixtures/layout/layout-v1.writer', import.meta.url));
  const opened = JSON.parse(instance.open_document(bytes));
  assert.equal(opened.ok, true);
  assert.ok(opened.view.styles.some((style) => style.id === 'layout-heading-style' && style.name === 'Heading'));
  const first = opened.view.paragraphs[0];
  const second = opened.view.paragraphs[1];
  const selection = { anchor: { paragraphId: first.id, utf16: 0 }, focus: { paragraphId: second.id, utf16: 5 } };
  let current = dispatch(instance, { type: 'set_selection', selection });
  assert.equal(current.format.styleMixed, true);
  const before = instance.export_document();
  current = dispatch(instance, { type: 'assign_style', styleId: 'layout-heading-style' });
  assert.equal(current.paragraphs[0].styleId, 'layout-heading-style');
  assert.equal(current.paragraphs[1].styleId, 'layout-heading-style');
  assert.equal(current.format.styleMixed, false);
  assert.equal(current.format.styleId, 'layout-heading-style');
  assert.ok(current.paragraphs[1].runs.some((run) => run.format.italic && run.format.underline === 'double'));
  const styled = instance.export_document();
  const invalid = JSON.parse(instance.dispatch('{"type":"assign_style","styleId":"missing-style"}'));
  assert.equal(invalid.ok, false);
  assert.deepEqual(instance.export_document(), styled);
  dispatch(instance, { type: 'undo' });
  assert.deepEqual(instance.export_document(), before);
  dispatch(instance, { type: 'redo' });
  assert.deepEqual(instance.export_document(), styled);
  current = dispatch(instance, { type: 'assign_style', styleId: null });
  assert.equal(current.paragraphs[0].styleId, null);
  assert.equal(current.paragraphs[1].styleId, null);
});

test('actual WASM ordinary typing does not consume a new run per key or merge unrelated imported runs', (context) => {
  const instance = engine(context);
  let current = view(instance);
  for (let count = 0; count < 1000; count += 1) {
    current = dispatch(instance, { type: 'replace_text', text: 'x', expectedRevision: current.revision });
  }
  const typed = instance.export_document();
  const file = JSON.parse(decoder.decode(typed));
  const runs = file.document.sections[0].blocks[0].content.runs;
  assert.equal(runs.length, 1, 'Normal collapsed typing must not exhaust the browser run budget');
  assert.equal(runs[0].text, 'x'.repeat(1000));
  dispatch(instance, { type: 'undo' });
  assert.equal(view(instance).paragraphs[0].runs.map((run) => run.text).join(''), 'x'.repeat(999));
  dispatch(instance, { type: 'redo' });
  assert.deepEqual(instance.export_document(), typed);

  const imported = new WriterEngine(`${context.name}-imported`);
  context.after(() => imported.free());
  const original = documentFile('');
  original.document.sections[0].blocks[0].content.runs = [
    { text: 'left', format: {} }, { text: 'right', format: {} },
  ];
  open(imported, original);
  const before = imported.export_document();
  dispatch(imported, { type: 'set_selection', selection: {
    anchor: { paragraphId: 'step7-paragraph', utf16: 9 }, focus: { paragraphId: 'step7-paragraph', utf16: 9 },
  } });
  dispatch(imported, { type: 'replace_text', text: 'X' });
  const edited = JSON.parse(decoder.decode(imported.export_document()));
  assert.deepEqual(edited.document.sections[0].blocks[0].content.runs.map((run) => run.text), ['left', 'rightX']);
  dispatch(imported, { type: 'undo' });
  assert.deepEqual(imported.export_document(), before, 'Undo restores the exact imported run structure');
});
