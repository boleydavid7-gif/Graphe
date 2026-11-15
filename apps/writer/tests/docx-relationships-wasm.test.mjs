import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import { URL } from 'node:url';
import { TextDecoder, TextEncoder } from 'node:util';
import initialize, { WriterEngine } from '../src/wasm/office_wasm.js';

// Execute the compiled release engine; no JavaScript document or import mock.
await initialize({ module_or_path: await readFile(new URL('../src/wasm/office_wasm_bg.wasm', import.meta.url)) });
const root = new URL('../../../tests/fixtures/docx/relationships/', import.meta.url);
const safe = await readFile(new URL('supported-auxiliary-parts.docx', root));
const encode = new TextEncoder();
const decode = new TextDecoder();
const edited = 'Edited Café e\u0301 👩🏽‍💻 — العربية नमस्ते שלום';
const unsafeNames = [
  'unsupported-image.docx', 'unsupported-header.docx', 'unsupported-footer.docx',
  'unsupported-hyperlink.docx', 'unsupported-altchunk.docx', 'unsupported-external-relationship.docx',
  'misplaced-root-font-table.docx',
  'misplaced-main-app-properties.docx', 'unknown-theme-relationship.docx',
  'unsupported-font-embedding.docx', 'unsupported-auxiliary-external-chain.docx',
  'unsupported-main-image-same-thumbnail.docx',
  'unsupported-malformed-default-session-id.docx',
  'unsupported-active-document-protection.docx', 'unsupported-active-track-revisions.docx',
  'unsupported-normalization-flood.docx',
];
const unsafe = await Promise.all(unsafeNames.map(async (name) => [name, await readFile(new URL(name, root))]));
const relationshipCases = new Set([
  'unsupported-image.docx', 'unsupported-header.docx', 'unsupported-footer.docx',
  'unsupported-hyperlink.docx', 'unsupported-altchunk.docx', 'unsupported-external-relationship.docx',
  'misplaced-root-font-table.docx', 'misplaced-main-app-properties.docx', 'unknown-theme-relationship.docx',
  'unsupported-auxiliary-external-chain.docx', 'unsupported-main-image-same-thumbnail.docx',
]);
let next = 0;

function instance(context) {
  const value = new WriterEngine(`docx-relationships-${++next}`);
  context.after(() => value.free());
  return value;
}
function accept(json) {
  const response = JSON.parse(json);
  assert.equal(response.ok, true, JSON.stringify(response));
  return response.view;
}
function view(value) { return JSON.parse(value.snapshot()); }
function command(value, input) { return accept(value.dispatch(JSON.stringify(input))); }
function text(paragraph) { return paragraph.runs.map((run) => run.text).join(''); }
function native(value) { return JSON.parse(decode.decode(value.export_document())); }
function importFile(value, bytes, generic = true) {
  return generic ? value.import_format(bytes, 'docx', '元の文書.docx') : value.import_docx(bytes, '元の文書.docx');
}

test('actual WASM accepts harmless auxiliary parts through both DOCX APIs and retains exact ownership', async (context) => {
  for (const name of ['supported-auxiliary-parts.docx', 'supported-authoring-metadata.docx',
    'supported-strict-auxiliary-parts.docx', 'supported-strict-authoring-metadata.docx']) {
    const source = await readFile(new URL(name, root));
    for (const generic of [false, true]) {
      const value = instance(context);
      const input = new Uint8Array(source);
      const opened = accept(importFile(value, input, generic));
      input.fill(0);
      assert.equal(opened.originalSource.filename, '元の文書.docx');
      assert.equal(opened.originalSource.format, 'docx');
      assert.equal(opened.originalSource.byteLength, source.length);
      assert.deepEqual(value.export_original(), new Uint8Array(source));
      assert.deepEqual(Buffer.from(native(value).source.bytes_base64, 'base64'), source);
      const bytes = value.export_document();
      const reopened = instance(context);
      const restored = accept(reopened.open_document(bytes, opened.documentId));
      assert.equal(restored.documentId, opened.documentId);
      assert.deepEqual(restored.paragraphs, opened.paragraphs);
      assert.deepEqual(reopened.export_document(), bytes);
      assert.deepEqual(reopened.export_original(), new Uint8Array(source));
    }
  }
});

test('actual WASM edits and formats auxiliary-bearing DOCX while current-content copies remain distinct from the original', (context) => {
  const value = instance(context);
  const opened = accept(importFile(value, safe));
  const first = opened.paragraphs[0];
  const originalText = text(first);
  command(value, { type: 'replace_text', text: edited, selection: {
    anchor: { paragraphId: first.id, utf16: 0 },
    focus: { paragraphId: first.id, utf16: originalText.length },
  } });
  command(value, { type: 'format_character', bold: true, italic: true, underline: true,
    fontFamily: 'Tinos', fontSizePt: 14, selection: {
      anchor: { paragraphId: first.id, utf16: 0 }, focus: { paragraphId: first.id, utf16: edited.length },
    } });
  command(value, { type: 'undo' });
  command(value, { type: 'undo' });
  assert.equal(text(view(value).paragraphs[0]), originalText);
  command(value, { type: 'redo' });
  command(value, { type: 'redo' });
  const current = view(value);
  const report = JSON.parse(value.export_assessment('docx', current.revision));
  assert.equal(report.sessionId, current.sessionId);
  assert.equal(report.documentId, current.documentId);
  assert.equal(report.revision, current.revision);
  assert.ok(report.warnings.some((warning) => warning.code === 'docx_original_parts'));
  assert.throws(() => value.export_format('docx', false, current.revision),
    (error) => JSON.parse(error).code === 'export_confirmation_required');
  const exported = value.export_format('docx', true, current.revision);
  assert.notDeepEqual(exported, new Uint8Array(safe));
  const copy = instance(context);
  const copyView = accept(importFile(copy, exported));
  assert.equal(text(copyView.paragraphs[0]), edited);
  assert.ok(copyView.paragraphs[0].runs.filter((run) => run.text.length)
    .every((run) => run.format.bold && run.format.italic && run.format.underline === 'single'
      && run.format.fontFamily === 'Tinos' && run.format.fontSizePt === 14));
  assert.deepEqual(value.export_original(), new Uint8Array(safe));
  assert.deepEqual(Buffer.from(native(value).source.bytes_base64, 'base64'), safe);
  assert.deepEqual(copy.export_original(), exported);
});

test('actual WASM keeps unsafe content, protection and unknown relationships read-only without fetching external targets', (context) => {
  let fetches = 0;
  context.mock.method(globalThis, 'fetch', () => {
    fetches += 1;
    throw new Error('Document relationship targets must never be fetched');
  });
  for (const generic of [false, true]) {
    const value = instance(context);
    const opened = accept(importFile(value, safe, generic));
    command(value, { type: 'set_title', title: 'Keep this unsaved version' });
    const before = value.snapshot();
    const bytes = value.export_document();
    for (const [name, input] of unsafe) {
      const response = JSON.parse(importFile(value, input, generic));
      assert.equal(response.ok, false, `${name}: ${JSON.stringify(response)}`);
      assert.equal(response.error.code, generic ? 'unsupported_format' : 'unsupported_docx', name);
      assert.ok(response.error.compatibility.diagnostics.length > 0, name);
      if (relationshipCases.has(name)) assert.ok(response.error.compatibility.diagnostics
        .some((diagnostic) => diagnostic.kind === 'unsupported_relationship'), name);
      assert.equal(value.snapshot(), before, name);
      assert.deepEqual(value.export_document(), bytes, name);
      assert.deepEqual(value.export_original(), new Uint8Array(safe), name);
    }
    command(value, { type: 'undo' });
    assert.equal(view(value).title, opened.title);
  }
  assert.equal(fetches, 0);
});

test('actual WASM revalidates unsupported original relationships inside native Writer files atomically', (context) => {
  const value = instance(context);
  accept(importFile(value, safe));
  command(value, { type: 'set_title', title: 'Keep original and history' });
  const before = value.snapshot();
  const bytes = value.export_document();
  for (const [name, source] of unsafe) {
    const forged = native(value);
    forged.source.filename = name;
    forged.source.bytes_base64 = source.toString('base64');
    const response = JSON.parse(value.open_document(encode.encode(JSON.stringify(forged))));
    assert.equal(response.ok, false, `${name}: ${JSON.stringify(response)}`);
    assert.equal(response.error.code, 'unsupported_docx', name);
    assert.equal(value.snapshot(), before, name);
    assert.deepEqual(value.export_document(), bytes, name);
    assert.deepEqual(value.export_original(), new Uint8Array(safe), name);
  }
});

test('actual WASM rejects spoofed or duplicate auxiliary declarations without changing the active document', async (context) => {
  const value = instance(context);
  accept(importFile(value, safe));
  const before = value.snapshot();
  const nativeBefore = value.export_document();
  for (const name of ['mismatched-auxiliary-mime.docx', 'mismatched-auxiliary-root.docx', 'duplicate-settings-relationship.docx']) {
    const input = await readFile(new URL(name, root));
    const response = JSON.parse(importFile(value, input));
    assert.equal(response.ok, false, `${name}: ${JSON.stringify(response)}`);
    assert.ok(['invalid_docx', 'unsupported_format'].includes(response.error.code), name);
    assert.equal(value.snapshot(), before);
    assert.deepEqual(value.export_document(), nativeBefore);
    assert.deepEqual(value.export_original(), new Uint8Array(safe));
  }
});

test('actual WASM preserves package thumbnails without allowing the same image as document content', async (context) => {
  const source = await readFile(new URL('supported-root-thumbnail.docx', root));
  const unsupported = await readFile(new URL('unsupported-main-image-same-thumbnail.docx', root));
  for (const generic of [false, true]) {
    const value = instance(context);
    const opened = accept(importFile(value, source, generic));
    assert.deepEqual(value.export_original(), new Uint8Array(source));
    const bytes = value.export_document();
    const reopened = instance(context);
    accept(reopened.open_document(bytes, opened.documentId));
    assert.deepEqual(reopened.export_document(), bytes);
    const before = reopened.snapshot();
    const response = JSON.parse(importFile(reopened, unsupported, generic));
    assert.equal(response.ok, false, JSON.stringify(response));
    assert.equal(response.error.code, generic ? 'unsupported_format' : 'unsupported_docx');
    assert.equal(reopened.snapshot(), before);
    assert.deepEqual(reopened.export_original(), new Uint8Array(source));
  }
});

test('actual WASM exposes successful normalization notices for public Word and LibreOffice sources and preserves edited native content', async (context) => {
  for (const name of ['normalizable-wordweb-profile.docx', 'microsoft-sdk-hello-world.docx',
    'microsoft-sdk-plain.docx', 'libreoffice-basic.docx']) {
    const source = await readFile(new URL(name, root));
    for (const generic of [false, true]) {
      const value = instance(context);
      const opened = accept(importFile(value, source, generic));
      assert.ok(opened.importWarnings.diagnostics.length > 0, name);
      assert.ok(opened.importWarnings.diagnostics.every((notice) => notice.kind === 'normalized_formatting'), name);
      assert.deepEqual(value.export_original(), new Uint8Array(source));
      const first = opened.paragraphs[0];
      const currentText = 'Edited normalized document — e\u0301 👩🏽‍💻';
      command(value, { type: 'replace_text', text: currentText, selection: {
        anchor: { paragraphId: first.id, utf16: 0 }, focus: { paragraphId: first.id, utf16: text(first).length },
      } });
      assert.deepEqual(view(value).importWarnings, opened.importWarnings);
      const bytes = value.export_document();
      const file = native(value);
      assert.equal(file.importWarnings, undefined);
      assert.equal(file.document.importWarnings, undefined);
      const reopened = instance(context);
      const restored = accept(reopened.open_document(bytes, opened.documentId));
      assert.equal(text(restored.paragraphs[0]), currentText);
      assert.deepEqual(restored.paragraphs, view(value).paragraphs);
      assert.deepEqual(restored.importWarnings, opened.importWarnings);
      assert.deepEqual(reopened.export_document(), bytes);
      assert.deepEqual(reopened.export_original(), new Uint8Array(source));
    }
  }
});

test('actual WASM normalization notices survive edits and forks and cannot make protected or revision content editable', async (context) => {
  const source = await readFile(new URL('normalizable-wordweb-profile.docx', root));
  const value = instance(context);
  const opened = accept(importFile(value, source));
  assert.ok(opened.importWarnings.diagnostics.length > 0);
  for (const input of [{ type: 'set_title', title: 'Normalized work' }, { type: 'undo' }, { type: 'redo' }, { type: 'fork_document' }]) {
    assert.deepEqual(command(value, input).importWarnings, opened.importWarnings);
  }
  const before = value.snapshot();
  const bytes = value.export_document();
  for (const name of ['unsupported-active-document-protection.docx', 'unsupported-active-track-revisions.docx', 'unsupported-normalization-flood.docx']) {
    const input = await readFile(new URL(name, root));
    const blocked = JSON.parse(importFile(value, input));
    assert.equal(blocked.ok, false, JSON.stringify(blocked));
    assert.ok(blocked.error.compatibility.diagnostics.length > 0, name);
    assert.equal(value.snapshot(), before);
    assert.deepEqual(value.export_document(), bytes);
    assert.deepEqual(value.export_original(), new Uint8Array(source));
  }
  const plain = accept(value.import_format(encode.encode('New plain text'), 'txt', 'new.txt'));
  assert.deepEqual(plain.importWarnings, { diagnostics: [], omittedDiagnostics: 0 });
  assert.deepEqual(command(value, { type: 'new_document' }).importWarnings, { diagnostics: [], omittedDiagnostics: 0 });
});

test('actual WASM formatting summaries use selected text across blank paragraphs', (context) => {
  const value = instance(context);
  const inserted = command(value, { type: 'replace_text', text: 'first\n\nlast' });
  const [first, blank, last] = inserted.paragraphs;
  const selection = { anchor: { paragraphId: first.id, utf16: 0 }, focus: { paragraphId: last.id, utf16: 4 } };
  const formatted = command(value, { type: 'format_character', bold: true, italic: true,
    fontFamily: 'Tinos', fontSizePt: 14, selection });
  assert.equal(formatted.format.bold, true);
  assert.equal(formatted.format.italic, true);
  assert.equal(formatted.format.fontFamily, 'Tinos');
  assert.equal(formatted.format.fontSizePt, 14);
  const blankOnly = command(value, { type: 'set_selection', selection: {
    anchor: { paragraphId: first.id, utf16: 5 }, focus: { paragraphId: last.id, utf16: 0 },
  } });
  assert.equal(blankOnly.format.bold, false);
  assert.equal(blankOnly.format.fontFamily, 'Arimo');
  const collapsed = command(value, { type: 'set_selection', selection: {
    anchor: { paragraphId: blank.id, utf16: 0 }, focus: { paragraphId: blank.id, utf16: 0 },
  } });
  assert.equal(collapsed.format.bold, false);
  assert.deepEqual(collapsed.paragraphs.map(text), ['first', '', 'last']);
});
