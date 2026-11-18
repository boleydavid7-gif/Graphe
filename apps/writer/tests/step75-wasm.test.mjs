import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { fileURLToPath, URL } from 'node:url';
import { TextDecoder, TextEncoder } from 'node:util';
import { WriterEngine } from '../src/wasm/office_wasm.js';
import { initializeWithFonts } from './font-catalog-helper.mjs';
import { execCargo } from './native-cargo.mjs';

await initializeWithFonts();
const encode = new TextEncoder();
const decode = new TextDecoder();
const initialText = 'Writer format — Café e\u0301 👩🏽‍💻\nSecond paragraph العربية नमस्ते שלום';
const editedText = 'Edited Café e\u0301 👩🏽‍💻';

function instance(context, suffix = '') {
  const value = new WriterEngine(`${context.name}${suffix}`);
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
function paragraphs(value) { return view(value).paragraphs.map((paragraph) => paragraph.runs.map((run) => run.text).join('')); }
function native(value) { return JSON.parse(decode.decode(value.export_document())); }

function seed(value) {
  command(value, { type: 'replace_text', text: initialText });
  command(value, { type: 'set_title', title: 'Local format reference' });
  const id = view(value).paragraphs[0].id;
  command(value, { type: 'format_character', bold: true, selection: {
    anchor: { paragraphId: id, utf16: 0 }, focus: { paragraphId: id, utf16: 6 },
  } });
}

function assessment(value, format) {
  const current = view(value);
  const result = JSON.parse(value.export_assessment(format, current.revision));
  assert.equal(result.sessionId, current.sessionId);
  assert.equal(result.documentId, current.documentId);
  assert.equal(result.revision, current.revision);
  assert.equal(result.format, format);
  assert.ok(Array.isArray(result.warnings));
  return result;
}

function exportCopy(value, format) {
  const before = value.export_document();
  const result = assessment(value, format);
  if (result.warnings.length) {
    assert.throws(() => value.export_format(format, false, result.revision),
      (error) => JSON.parse(error).code === 'export_confirmation_required');
  }
  const bytes = value.export_format(format, result.warnings.length > 0, result.revision);
  assert.ok(bytes instanceof Uint8Array && bytes.length > 0);
  assert.deepEqual(value.export_document(), before, 'Assessing/exporting a copy cannot change current native content');
  return bytes;
}

for (const format of ['docx', 'odt', 'rtf', 'txt']) {
  test(`actual WASM ${format.toUpperCase()} import, Rust editing, export and reopen preserve supported content`, (context) => {
    const source = instance(context, '-source');
    seed(source);
    const original = exportCopy(source, format);
    const current = instance(context, '-edit');
    const imported = accept(current.import_format(original, format, `原本-${format}.${format}`));
    assert.deepEqual(paragraphs(current), initialText.split('\n'));
    assert.equal(imported.originalSource.format, format);
    assert.deepEqual(current.export_original(), original);

    const first = imported.paragraphs[0];
    const firstText = first.runs.map((run) => run.text).join('');
    command(current, { type: 'replace_text', text: editedText, selection: {
      anchor: { paragraphId: first.id, utf16: 0 }, focus: { paragraphId: first.id, utf16: firstText.length },
    } });
    command(current, { type: 'format_character', bold: true, italic: true, underline: true,
      fontFamily: 'Tinos', fontSizePt: 14, selection: {
        anchor: { paragraphId: first.id, utf16: 0 }, focus: { paragraphId: first.id, utf16: editedText.length },
      } });
    command(current, { type: 'format_paragraph', alignment: 'right' });
    assert.deepEqual(current.export_original(), original);
    const expected = [editedText, initialText.split('\n')[1]];
    assert.deepEqual(paragraphs(current), expected);

    const fullFile = current.export_document();
    const reopened = instance(context, '-native-reopen');
    const reopenedView = accept(reopened.open_document(fullFile, view(current).documentId));
    assert.equal(reopenedView.documentId, view(current).documentId);
    assert.deepEqual(reopened.export_document(), fullFile);
    assert.deepEqual(reopened.export_original(), original);
    assert.equal(native(reopened).source.format, format);

    const result = exportCopy(reopened, format);
    const final = instance(context, '-external-reopen');
    accept(final.import_format(result, format, `edited.${format}`));
    assert.deepEqual(paragraphs(final), expected);
    if (format !== 'txt') {
      const runs = view(final).paragraphs[0].runs.filter((run) => run.text.length > 0);
      assert.ok(runs.length > 0);
      assert.ok(runs.every((run) => run.format.bold && run.format.italic && run.format.underline === 'single'));
      assert.ok(runs.every((run) => run.format.fontFamily === 'Tinos' && run.format.fontSizePt === 14));
      assert.equal(view(final).paragraphs[0].format.alignment, 'right');
    } else {
      assert.ok(assessment(reopened, 'txt').warnings.length > 0, 'Text export must disclose loss of rich formatting');
      assert.ok(view(final).paragraphs[0].runs.every((run) => !run.format.bold && !run.format.italic));
    }
  });
}

test('actual WASM native Save As preserves identifiers, styles, metadata, source and exact file content', async (context) => {
  const value = instance(context);
  const fixture = await readFile(new URL('../../../tests/fixtures/native/project-notes.writer', import.meta.url));
  accept(value.import_format(fixture, 'writer', 'notes.writer'));
  const before = value.export_document();
  const copied = exportCopy(value, 'writer');
  assert.deepEqual(copied, before);
  const other = instance(context, '-reopened');
  accept(other.import_format(copied, 'writer', 'renamed.writer'));
  assert.deepEqual(native(other), native(value));
  assert.ok(native(other).document.styles.length > 0);
});

test('actual WASM ordinary supported copies have no technical warning or loss confirmation gate', (context) => {
  const value = instance(context);
  command(value, { type: 'replace_text', text: 'Ordinary Café e\u0301 👩🏽‍💻' });
  const before = value.export_document();
  const snapshot = value.snapshot();
  for (const format of ['writer', 'docx', 'odt', 'rtf', 'txt']) {
    const result = assessment(value, format);
    assert.deepEqual(result.warnings, [], `${format} ordinary copy must not warn about internal identifiers`);
    assert.ok(value.export_format(format, false, result.revision).byteLength > 0);
    assert.equal(value.snapshot(), snapshot);
    assert.deepEqual(value.export_document(), before);
  }
});

test('actual WASM text BOM decoding preserves Unicode and original bytes without guessing legacy encodings', (context) => {
  const value = instance(context);
  const source = 'Café e\u0301 👩🏽‍💻\r\nSecond';
  const utf8 = new Uint8Array([0xef, 0xbb, 0xbf, ...encode.encode(source)]);
  const imported = accept(value.import_format(utf8, 'txt', 'unicode.txt'));
  assert.deepEqual(paragraphs(value), ['Café e\u0301 👩🏽‍💻', 'Second']);
  assert.equal(imported.originalSource.format, 'txt');
  assert.deepEqual(value.export_original(), utf8);
  const nativeBefore = value.export_document();
  const saved = instance(context, '-reopened');
  accept(saved.open_document(nativeBefore));
  assert.deepEqual(saved.export_original(), utf8);
  assert.deepEqual(paragraphs(saved), paragraphs(value));

  for (const [label, bytes] of [
    ['little', new Uint8Array([0xff, 0xfe, ...Buffer.from(source, 'utf16le')])],
    ['big', new Uint8Array([0xfe, 0xff, ...Buffer.from(source, 'utf16le').swap16()])],
  ]) {
    const encoded = instance(context, `-utf16-${label}`);
    accept(encoded.import_format(bytes, 'txt', `${label}.txt`));
    assert.deepEqual(paragraphs(encoded), ['Café e\u0301 👩🏽‍💻', 'Second']);
    assert.deepEqual(encoded.export_original(), bytes);
  }

  const semanticBom = instance(context, '-semantic-bom');
  const semanticText = '\ufeffKeep semantic FEFF e\u0301 👩🏽‍💻';
  command(semanticBom, { type: 'replace_text', text: semanticText });
  const copy = exportCopy(semanticBom, 'txt');
  const reopenedBom = instance(context, '-semantic-bom-reopen');
  accept(reopenedBom.import_format(copy, 'txt', 'semantic.txt'));
  assert.deepEqual(paragraphs(reopenedBom), [semanticText], 'A real first FEFF must survive the transport BOM convention');
});

test('actual WASM malformed, unsupported and export-only imports cannot replace live work or its undo history', (context) => {
  const value = instance(context);
  seed(value);
  const before = value.snapshot();
  const bytes = value.export_document();
  const inputs = [
    [new Uint8Array([0xff, 0xfe, 0x00]), 'txt', 'broken.txt'],
    [encode.encode('{\\rtf1\\ansi unclosed'), 'rtf', 'broken.rtf'],
    [encode.encode('not a ZIP'), 'odt', 'broken.odt'],
    [encode.encode('%PDF-1.7\n'), 'pdf', 'export-only.pdf'],
    [encode.encode('unknown'), 'future', 'future.bin'],
  ];
  for (const [input, format, filename] of inputs) {
    const result = JSON.parse(value.import_format(input, format, filename));
    assert.equal(result.ok, false, `Reject ${format}`);
    if (format === 'pdf') assert.equal(result.error.code, 'export_only_format');
    if (format === 'future') assert.equal(result.error.code, 'unknown_format');
    assert.equal(value.snapshot(), before);
    assert.deepEqual(value.export_document(), bytes);
  }
  const unsupported = encode.encode('{\\rtf1\\ansi\\trowd\\cellx1000 Unsupported table\\cell\\row}');
  const result = JSON.parse(value.import_format(unsupported, 'rtf', 'table.rtf'));
  assert.equal(result.ok, false);
  assert.equal(result.error.code, 'unsupported_format');
  assert.ok(result.error.compatibility.diagnostics.length > 0);
  assert.equal(value.snapshot(), before);
  assert.deepEqual(value.export_document(), bytes);
  command(value, { type: 'undo' });
  assert.ok(view(value).canRedo, 'Failed imports must not consume or replace current undo history');
});

test('actual WASM export assessment is revision-bound and requires explicit loss acknowledgement', (context) => {
  const value = instance(context);
  seed(value);
  const report = assessment(value, 'txt');
  assert.ok(report.warnings.length > 0);
  command(value, { type: 'set_title', title: 'Newer in-memory work' });
  const before = value.snapshot();
  const bytes = value.export_document();
  assert.throws(() => value.export_assessment('txt', report.revision), (error) => JSON.parse(error).code === 'stale_revision');
  assert.throws(() => value.export_format('txt', true, report.revision), (error) => JSON.parse(error).code === 'stale_revision');
  const latest = assessment(value, 'txt');
  assert.throws(() => value.export_format('txt', false, latest.revision),
    (error) => JSON.parse(error).code === 'export_confirmation_required');
  assert.equal(value.snapshot(), before);
  assert.deepEqual(value.export_document(), bytes);
  assert.equal(decode.decode(value.export_format('txt', true, latest.revision)), initialText);
});

test('actual WASM PDF export uses existing Rust pages, matches native bytes and remains explicitly export-only', async (context) => {
  const value = instance(context);
  seed(value);
  command(value, { type: 'replace_text', text: '\n'.repeat(70) });
  const current = view(value);
  const pages = JSON.parse(value.layout_snapshot('{}', current.revision)).layout.pages.length;
  assert.ok(pages > 1);
  const report = assessment(value, 'pdf');
  assert.ok(report.warnings.length > 0, 'Outlined PDF must disclose its text/accessibility limitations');
  const bytes = exportCopy(value, 'pdf');
  const source = decode.decode(bytes);
  assert.match(source, /^%PDF-/);
  assert.match(source, /%%EOF\s*$/);
  assert.equal([...source.matchAll(/\/Type\s*\/Page\b/g)].length, pages);
  const before = value.export_document();
  const result = JSON.parse(value.import_format(bytes, 'pdf', 'print.pdf'));
  assert.equal(result.ok, false);
  assert.equal(result.error.code, 'export_only_format');
  assert.deepEqual(value.export_document(), before);

  const temporary = await mkdtemp(join(tmpdir(), 'writer-native-wasm-pdf-'));
  try {
    const fixture = fileURLToPath(new URL('../../../tests/fixtures/layout/layout-v1.writer', import.meta.url));
    const output = join(temporary, 'native.pdf');
    execCargo( ['run', '--offline', '--locked', '--quiet', '-p', 'office-formats',
      '--example', 'export_pdf', '--', fixture, output], {
      cwd: fileURLToPath(new URL('../../../', import.meta.url)), stdio: 'pipe',
    });
    const identical = instance(context, '-native-pdf-reference');
    accept(identical.open_document(await readFile(fixture)));
    assert.deepEqual(exportCopy(identical, 'pdf'), new Uint8Array(await readFile(output)),
      'The full four-page PDF serialization must match native Rust byte-for-byte');
  } finally { await rm(temporary, { recursive: true, force: true }); }
});
