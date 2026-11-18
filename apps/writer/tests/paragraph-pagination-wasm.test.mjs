import assert from 'node:assert/strict';
import test from 'node:test';
import { WriterEngine } from '../src/wasm/office_wasm.js';
import { initializeWithFonts } from './font-catalog-helper.mjs';
await initializeWithFonts();
function instance(context) {
  const result = new WriterEngine(context.name);
  context.after(() => result.free());
  return result;
}
function view(engine) { return JSON.parse(engine.snapshot()); }
function command(engine, input) {
  const result = JSON.parse(engine.dispatch(JSON.stringify(input)));
  assert.equal(result.ok, true, JSON.stringify(result));
  return result.view;
}
test('real WASM edits both pagination flags with revision guards and native persistence', (context) => {
  const engine = instance(context);
  const start = command(engine, { type: 'replace_text', text: 'Heading' });
  const updated = command(engine, { type: 'set_paragraph_pagination',
    keepTogether: true, keepWithNext: true, expectedRevision: start.revision });
  assert.equal(updated.paragraphs[0].format.keepTogether, true);
  assert.equal(updated.paragraphs[0].format.keepWithNext, true);
  const file = JSON.parse(new TextDecoder().decode(engine.export_document()));
  assert.equal(file.format_version, 7);
  assert.equal(file.document.schema_version, 7);
  const paragraph = file.document.sections[0].blocks[0].content;
  assert.equal(paragraph.format.keep_together, true);
  assert.equal(paragraph.format.keep_with_next, true);
  const before = engine.snapshot();
  const failed = JSON.parse(engine.dispatch(JSON.stringify({
    type: 'set_paragraph_pagination', keepTogether: false, expectedRevision: start.revision,
  })));
  assert.equal(failed.ok, false);
  assert.equal(engine.snapshot(), before);
  command(engine, { type: 'undo' });
  assert.equal(view(engine).paragraphs[0].format.keepTogether, false);
  command(engine, { type: 'redo' });
  assert.equal(view(engine).paragraphs[0].format.keepWithNext, true);
});
