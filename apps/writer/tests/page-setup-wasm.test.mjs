import { TextDecoder } from 'node:util';
import assert from 'node:assert/strict';
import test from 'node:test';
import { WriterEngine } from '../src/wasm/office_wasm.js';
import { initializeWithFonts } from './font-catalog-helper.mjs';

await initializeWithFonts();

function create(context) {
  const engine = new WriterEngine(context.name);
  context.after(() => engine.free());
  return engine;
}
function view(engine) { return JSON.parse(engine.snapshot()); }
function command(engine, value) {
  const response = JSON.parse(engine.dispatch(JSON.stringify(value)));
  assert.equal(response.ok, true, JSON.stringify(response));
  return response.view;
}
function rejected(engine, value) {
  const previous = engine.snapshot();
  const response = JSON.parse(engine.dispatch(JSON.stringify(value)));
  assert.equal(response.ok, false, JSON.stringify(response));
  assert.equal(engine.snapshot(), previous);
}

test('WASM page setup, revision guard and one-step undo', (context) => {
  const engine = create(context);
  const before = view(engine);
  const page = before.pageSetup;
  assert.equal(page.pageSize.width, 12240);
  const updated = command(engine, { type: 'set_page_setup', sectionId: page.sectionId,
    pageSize: { width: 16838, height: 11906, orientation: 'landscape' },
    pageMargins: { ...page.pageMargins, top: 720, bottom: 720 },
    expectedRevision: before.revision });
  assert.equal(updated.pageSetup.pageSize.width, 16838);
  assert.equal(updated.pageSetup.pageMargins.top, 720);
  rejected(engine, { type: 'set_page_setup', sectionId: page.sectionId,
    pageSize: { width: 16838, height: 11906, orientation: 'landscape' },
    pageMargins: page.pageMargins, expectedRevision: before.revision });
  command(engine, { type: 'undo' });
  assert.equal(view(engine).pageSetup.pageSize.width, 12240);
  command(engine, { type: 'redo' });
  assert.equal(view(engine).pageSetup.pageSize.width, 16838);
});

test('WASM manual page break splits text and persists exact geometry', (context) => {
  const engine = create(context);
  const first = command(engine, { type: 'replace_text', text: 'Before after' });
  const id = first.paragraphs[0].id;
  const point = { paragraphId: id, utf16: 6 };
  const broken = command(engine, { type: 'insert_page_break',
    selection: { anchor: point, focus: point }, expectedRevision: first.revision });
  assert.deepEqual(broken.paragraphs.map((paragraph) =>
    paragraph.runs.map((run) => run.text).join('')), ['Before', ' after']);
  assert.equal(broken.paragraphs[1].format.pageBreakBefore, true);
  const layout = JSON.parse(engine.layout_snapshot('{}', broken.revision));
  assert.ok(layout.layout.pages.length >= 2);
  command(engine, { type: 'undo' });
  assert.equal(view(engine).paragraphs.length, 1);
  assert.equal(view(engine).paragraphs[0].runs[0].text, 'Before after');
  command(engine, { type: 'redo' });
  assert.equal(view(engine).paragraphs[1].format.pageBreakBefore, true);
  const file = JSON.parse(new TextDecoder().decode(engine.export_document()));
  assert.equal(file.document.sections[0].blocks[1].content.format.page_break_before, true);
});
