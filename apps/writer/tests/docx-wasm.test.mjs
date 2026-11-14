import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import { URL } from 'node:url';
import { TextDecoder, TextEncoder } from 'node:util';
import initialize, { WriterEngine } from '../src/wasm/office_wasm.js';

await initialize({ module_or_path: await readFile(new URL('../src/wasm/office_wasm_bg.wasm', import.meta.url)) });
const fixtureRoot = new URL('../../../tests/fixtures/docx/', import.meta.url);
const basic = await readFile(new URL('supported-basic.docx', fixtureRoot));
const formatted = await readFile(new URL('supported-formatting.docx', fixtureRoot));
const unsupported = await readFile(new URL('writer-source-preservation.docx', fixtureRoot));
const carriageReturn = await readFile(new URL('unsupported-carriage-return.docx', fixtureRoot));
const partialSpacing = await readFile(new URL('unsupported-partial-line-spacing.docx', fixtureRoot));
const decoder = new TextDecoder();

function engine(context) {
  const instance = new WriterEngine(context.name);
  context.after(() => instance.free());
  return instance;
}

function snapshot(instance) { return JSON.parse(instance.snapshot()); }
function native(instance) { return JSON.parse(decoder.decode(instance.export_document())); }
function accept(response) {
  const result = JSON.parse(response);
  assert.equal(result.ok, true, JSON.stringify(result));
  return result.view;
}
function command(instance, value) { return accept(instance.dispatch(JSON.stringify(value))); }
function text(view) {
  return view.paragraphs.map((paragraph) => paragraph.runs.map((run) => run.text).join(''));
}

test('actual WASM imports formatting and page geometry and preserves original ownership across reopen', (context) => {
  const instance = engine(context);
  const input = new Uint8Array(formatted);
  const view = accept(instance.import_docx(input, '原本.docx'));
  input.fill(0);
  assert.equal(view.title, 'Supported Writer DOCX – café');
  assert.equal(view.originalSource.filename, '原本.docx');
  assert.equal(view.originalSource.byteLength, formatted.length);
  assert.deepEqual(instance.export_original(), new Uint8Array(formatted));
  const file = native(instance);
  assert.equal(file.format_version, 2);
  assert.equal(file.document.schema_version, 2);
  assert.equal(file.document.sections.length, 2);
  assert.equal(file.document.sections[0].properties.page_size.width, 12240);
  assert.equal(file.document.sections[1].properties.page_size.width, 11906);
  assert.equal(file.document.sections[1].blocks[0].content.format.page_break_before, true);
  assert.deepEqual(Buffer.from(file.source.bytes_base64, 'base64'), formatted);
  const bytes = instance.export_document();
  const reopened = new WriterEngine(`${context.name}-reopened`);
  context.after(() => reopened.free());
  accept(reopened.open_document(bytes, view.documentId));
  assert.deepEqual(reopened.export_document(), bytes);
  assert.deepEqual(reopened.export_original(), new Uint8Array(formatted));
  const copy = reopened.export_original();
  copy.fill(0);
  assert.deepEqual(reopened.export_original(), new Uint8Array(formatted));
});

test('actual WASM rejects unsupported/corrupted DOCX without replacing content, history or original', (context) => {
  const instance = engine(context);
  accept(instance.import_docx(basic, 'supported.docx'));
  command(instance, { type: 'set_title', title: 'Unsaved work' });
  const before = instance.snapshot();
  const bytes = instance.export_document();
  for (const [input, code] of [[unsupported, 'unsupported_docx'], [carriageReturn, 'unsupported_docx'], [partialSpacing, 'unsupported_docx'], [new Uint8Array([1, 2, 3]), 'invalid_docx']]) {
    const result = JSON.parse(instance.import_docx(input, 'next.docx'));
    assert.equal(result.ok, false);
    assert.equal(result.error.code, code);
    if (code === 'unsupported_docx') assert.ok(result.error.compatibility.diagnostics.length > 0);
    assert.equal(instance.snapshot(), before);
    assert.deepEqual(instance.export_document(), bytes);
    assert.deepEqual(instance.export_original(), new Uint8Array(basic));
  }
  command(instance, { type: 'undo' });
  assert.equal(snapshot(instance).title, '');
});

test('actual WASM Word export contains edited content while native files retain the disclosed original', (context) => {
  const instance = engine(context);
  const view = accept(instance.import_docx(basic, 'supported.docx'));
  const paragraph = view.paragraphs[0];
  const length = paragraph.runs.map((run) => run.text).join('').length;
  command(instance, { type: 'replace_text', text: 'Current text 👩🏽‍💻', selection: {
    anchor: { paragraphId: paragraph.id, utf16: 0 }, focus: { paragraphId: paragraph.id, utf16: length },
  } });
  const word = instance.export_docx();
  assert.deepEqual(instance.export_docx(), word);
  const converted = new WriterEngine(`${context.name}-converted`);
  context.after(() => converted.free());
  accept(converted.import_docx(word, 'edited.docx'));
  assert.deepEqual(text(snapshot(converted)), ['Current text 👩🏽‍💻']);
  assert.ok(!text(snapshot(converted)).join('').includes('DOCX hello'));
  assert.deepEqual(instance.export_original(), new Uint8Array(basic));
  assert.deepEqual(Buffer.from(native(instance).source.bytes_base64, 'base64'), basic);
  command(instance, { type: 'undo' });
  assert.deepEqual(text(snapshot(instance)), ['DOCX hello — café 👩🏽‍💻']);
  command(instance, { type: 'redo' });
  assert.deepEqual(text(snapshot(instance)), ['Current text 👩🏽‍💻']);
});

test('actual WASM preserves sources on fork, clears them on New, and rejects corrupt retained sources atomically', (context) => {
  const instance = engine(context);
  const original = accept(instance.import_docx(basic, 'supported.docx'));
  const fork = command(instance, { type: 'fork_document' });
  assert.notEqual(fork.documentId, original.documentId);
  assert.deepEqual(instance.export_original(), new Uint8Array(basic));
  const current = instance.export_document();
  const corrupt = native(instance);
  corrupt.source.bytes_base64 = Buffer.from('broken ZIP').toString('base64');
  const result = JSON.parse(instance.open_document(new TextEncoder().encode(JSON.stringify(corrupt))));
  assert.equal(result.ok, false);
  assert.deepEqual(instance.export_document(), current);
  const upper = native(instance);
  upper.source.media_type = upper.source.media_type.toUpperCase();
  accept(instance.open_document(new TextEncoder().encode(JSON.stringify(upper))));
  assert.deepEqual(instance.export_original(), new Uint8Array(basic));
  command(instance, { type: 'new_document' });
  assert.equal(snapshot(instance).originalSource, null);
  assert.equal(native(instance).source, undefined);
  assert.throws(() => instance.export_original(), (error) => JSON.parse(error).code === 'no_original');
});
