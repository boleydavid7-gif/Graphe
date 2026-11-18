import { TextDecoder } from 'node:util';
import assert from 'node:assert/strict';
import test from 'node:test';
import { WriterEngine } from '../src/wasm/office_wasm.js';
import { initializeWithFonts } from './font-catalog-helper.mjs';

await initializeWithFonts();
function instance(context) {
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
function reject(engine, value, code) {
  const before = engine.snapshot();
  const native = engine.export_document();
  const response = JSON.parse(engine.dispatch(JSON.stringify(value)));
  assert.equal(response.ok, false, JSON.stringify(response));
  assert.equal(response.error.code, code);
  assert.equal(engine.snapshot(), before);
  assert.deepEqual(engine.export_document(), native);
}

test('compiled WASM section break and independent geometry survive undo and export', (context) => {
  const engine = instance(context);
  const first = command(engine, { type: 'replace_text', text: 'First Second' });
  const originalId = first.paragraphs[0].id;
  const at = { paragraphId: originalId, utf16: 5 };
  const broken = command(engine, { type: 'insert_section_break',
    selection: { anchor: at, focus: at }, expectedRevision: first.revision });
  assert.equal(broken.pageSetup.sectionIndex, 1);
  assert.equal(broken.pageSetup.sectionCount, 2);
  assert.deepEqual(broken.paragraphs.map((p) => p.runs.map((r) => r.text).join('')), ['First', ' Second']);
  const secondId = broken.pageSetup.sectionId;
  const altered = command(engine, { type: 'set_page_setup', sectionId: secondId,
    pageSize: { width: 20160, height: 12240, orientation: 'landscape' },
    pageMargins: { ...broken.pageSetup.pageMargins, top: 720 },
    expectedRevision: broken.revision });
  assert.equal(altered.pageSetup.pageSize.width, 20160);
  const file = JSON.parse(new TextDecoder().decode(engine.export_document()));
  assert.equal(file.document.sections.length, 2);
  assert.equal(file.document.sections[1].properties.page_size.width, 20160);
  const layout = JSON.parse(engine.layout_snapshot('{}', altered.revision));
  assert.ok(layout.layout.pages.length >= 2);
  reject(engine, { type: 'remove_section_break', sectionId: secondId,
    expectedRevision: altered.revision }, 'section_layout_confirmation_required');
  const joined = command(engine, { type: 'remove_section_break', sectionId: secondId,
    confirmLayoutChange: true, expectedRevision: altered.revision });
  assert.equal(joined.pageSetup.sectionCount, 1);
  assert.equal(joined.paragraphs.map((p) => p.runs.map((r) => r.text).join('')).join(''), 'First Second');
  command(engine, { type: 'undo' });
  assert.equal(view(engine).pageSetup.sectionCount, 2);
  assert.equal(view(engine).pageSetup.pageSize.width, 20160);
});

test('compiled WASM refuses invalid ranges without mutating the document', (context) => {
  const engine = instance(context);
  const start = command(engine, { type: 'replace_text', text: 'A B' });
  const paragraphId = start.paragraphs[0].id;
  reject(engine, { type: 'insert_section_break',
    selection: { anchor: { paragraphId, utf16: 0 }, focus: { paragraphId, utf16: 2 } },
    expectedRevision: start.revision }, 'invalid_section_break');
  reject(engine, { type: 'insert_section_break',
    selection: { anchor: { paragraphId, utf16: 0 }, focus: { paragraphId, utf16: 0 } } },
  'invalid_section_break');
});
