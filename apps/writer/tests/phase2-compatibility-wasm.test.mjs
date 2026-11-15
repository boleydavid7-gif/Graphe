import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import { URL } from 'node:url';
import { TextDecoder, TextEncoder } from 'node:util';
import initialize, { WriterEngine } from '../src/wasm/office_wasm.js';

await initialize({ module_or_path: await readFile(new URL('../src/wasm/office_wasm_bg.wasm', import.meta.url)) });
const encode = new TextEncoder();
const decode = new TextDecoder();
const expectedOffice = [
  'Project notes',
  'The next review will focus on dependable editing, accurate document layout, and reliable local saving. Writer keeps the document engine in Rust.',
  'Review priorities',
  'Confirm Unicode behavior — Café, e\u0301, 👩🏽‍💻 — and preserve formatting across native save and reopen cycles.',
  'Next steps',
  'Compare the saved document with the current revision, check keyboard navigation, and retain an exported backup. Audit edit e\u0301 👩🏽‍💻 العربية नमस्ते שלום.',
];
const expectedRtf = [
  'Compatibility notes',
  'Writer keeps logical Unicode e\u0301 and 😀 with bold, italic and underlined text.',
];
const cases = [
  { format: 'docx', path: '../../../tests/fixtures/docx/relationships/phase2-libreoffice-unicode.docx', expected: expectedOffice,
    warning: 'docx_formatting_changes', diagnostic: /Forced right-to-left/ },
  { format: 'odt', path: '../../../tests/fixtures/formats/libreoffice-audit-resaved.odt', expected: expectedOffice,
    warning: 'original_formatting_changes', diagnostic: /./ },
  { format: 'rtf', path: '../../../tests/fixtures/formats/rtf-known-formatting.rtf', expected: expectedRtf,
    warning: 'original_formatting_changes', diagnostic: /./ },
];

function instance(context, suffix = '') {
  const value = new WriterEngine(`phase2-${context.name}${suffix}`);
  context.after(() => value.free());
  return value;
}
function accept(json) {
  const response = JSON.parse(json);
  assert.equal(response.ok, true, JSON.stringify(response));
  return response.view;
}
const snapshot = (value) => JSON.parse(value.snapshot());
const paragraphs = (value) => snapshot(value).paragraphs.map((paragraph) => paragraph.runs.map((run) => run.text).join(''));
const native = (value) => JSON.parse(decode.decode(value.export_document()));
const command = (value, input) => accept(value.dispatch(JSON.stringify(input)));

for (const item of cases) {
  test(`compiled WASM ${item.format.toUpperCase()} normalization retains Unicode, notices, originals and complete edited round trips`, async (context) => {
    const source = await readFile(new URL(item.path, import.meta.url));
    const value = instance(context);
    const supplied = new Uint8Array(source);
    const imported = accept(value.import_format(supplied, item.format, `Original ${item.format}.${item.format}`));
    supplied.fill(0);
    assert.deepEqual(paragraphs(value), item.expected);
    assert.ok(imported.importWarnings.diagnostics.length > 0);
    assert.ok(imported.importWarnings.diagnostics.every((notice) => notice.kind === 'normalized_formatting'));
    assert.ok(imported.importWarnings.diagnostics.some((notice) => item.diagnostic.test(notice.message)));
    assert.deepEqual(value.export_original(), new Uint8Array(source));
    assert.deepEqual(Buffer.from(native(value).source.bytes_base64, 'base64'), source);

    const first = imported.paragraphs[0];
    const edited = `Edited ${item.format.toUpperCase()} e\u0301 👩🏽‍💻 العربية नमस्ते שלום`;
    command(value, { type: 'replace_text', text: edited, selection: {
      anchor: { paragraphId: first.id, utf16: 0 }, focus: { paragraphId: first.id, utf16: item.expected[0].length },
    } });
    command(value, { type: 'format_character', bold: true, italic: true, underline: true,
      fontFamily: 'Tinos', fontSizePt: 14, selection: {
        anchor: { paragraphId: first.id, utf16: 0 }, focus: { paragraphId: first.id, utf16: edited.length },
      } });
    command(value, { type: 'format_paragraph', alignment: 'right' });
    const expected = [edited, ...item.expected.slice(1)];
    assert.deepEqual(paragraphs(value), expected);
    const bytes = value.export_document();
    const current = snapshot(value);
    const reopened = instance(context, '-native');
    const restored = accept(reopened.open_document(bytes, imported.documentId));
    assert.equal(restored.documentId, imported.documentId);
    assert.deepEqual(restored.paragraphs, current.paragraphs);
    assert.deepEqual(restored.importWarnings, imported.importWarnings);
    assert.deepEqual(reopened.export_document(), bytes);
    assert.deepEqual(reopened.export_original(), new Uint8Array(source));

    const before = reopened.snapshot();
    const assessment = JSON.parse(reopened.export_assessment(item.format, restored.revision));
    assert.ok(assessment.warnings.some((warning) => warning.code === item.warning), JSON.stringify(assessment));
    assert.throws(() => reopened.export_format(item.format, false, restored.revision),
      (error) => JSON.parse(error).code === 'export_confirmation_required');
    const writerAssessment = JSON.parse(reopened.export_assessment('writer', restored.revision));
    assert.deepEqual(writerAssessment.warnings, []);
    const exported = reopened.export_format(item.format, true, restored.revision);
    assert.equal(reopened.snapshot(), before);
    assert.deepEqual(reopened.export_document(), bytes);
    assert.deepEqual(reopened.export_original(), new Uint8Array(source));

    const copy = instance(context, '-copy');
    const copied = accept(copy.import_format(exported, item.format, `Edited.${item.format}`));
    assert.deepEqual(paragraphs(copy), expected);
    const runs = copied.paragraphs[0].runs.filter((run) => run.text.length);
    assert.ok(runs.length > 0);
    assert.ok(runs.every((run) => run.format.bold && run.format.italic && run.format.underline === 'single'
      && run.format.fontFamily === 'Tinos' && run.format.fontSizePt === 14));
    assert.equal(copied.paragraphs[0].format.alignment, 'right');
    assert.deepEqual(copy.export_original(), exported);
  });
}

const unsafe = [
  { format: 'docx', path: '../../../tests/fixtures/docx/relationships/unsupported-active-document-protection.docx' },
  { format: 'odt', path: '../../../tests/fixtures/formats/unsupported-active-note.odt' },
  { format: 'rtf', bytes: encode.encode('{\\rtf1\\ansi\\rtlch Important unsupported directional text.}') },
];

for (const item of unsafe) {
  test(`compiled WASM ${item.format.toUpperCase()} still rejects unsafe content without replacing the current normalized document`, async (context) => {
    const value = instance(context);
    const safe = cases.find((candidate) => candidate.format === item.format);
    const source = await readFile(new URL(safe.path, import.meta.url));
    accept(value.import_format(source, item.format, `Safe.${item.format}`));
    command(value, { type: 'set_title', title: 'Retain this current revision and undo entry' });
    const before = value.snapshot();
    const bytes = value.export_document();
    const input = item.bytes ?? await readFile(new URL(item.path, import.meta.url));
    const rejected = JSON.parse(value.import_format(input, item.format, `Unsafe.${item.format}`));
    assert.equal(rejected.ok, false);
    assert.equal(rejected.error.code, 'unsupported_format');
    assert.ok(rejected.error.compatibility.diagnostics.length > 0);
    assert.equal(value.snapshot(), before);
    assert.deepEqual(value.export_document(), bytes);
    assert.deepEqual(value.export_original(), new Uint8Array(source));
    command(value, { type: 'undo' });
    assert.notEqual(snapshot(value).title, 'Retain this current revision and undo entry');
  });
}

test('compiled WASM RTF warning truncation cannot hide late meaningful formatting or an unsupported control', (context) => {
  const value = instance(context);
  const prefix = '{\\rtf1\\ansi' + '\\lang1033'.repeat(140);
  const source = encode.encode(prefix + '\\kerning1 Preserved Unicode e\\u769? text.}');
  const imported = accept(value.import_format(source, 'rtf', 'Bounded notices.rtf'));
  assert.equal(imported.importWarnings.diagnostics.length, 128);
  assert.ok(imported.importWarnings.omittedDiagnostics > 0);
  const assessment = JSON.parse(value.export_assessment('rtf', imported.revision));
  assert.ok(assessment.warnings.some((warning) => warning.code === 'original_formatting_changes'));
  const before = value.snapshot();
  const bytes = value.export_document();
  const rejected = JSON.parse(value.import_format(encode.encode(prefix + '\\kerning1 Visible{\\*\\future Important retained data}}'), 'rtf', 'Unsupported after notices.rtf'));
  assert.equal(rejected.ok, false);
  assert.equal(rejected.error.code, 'unsupported_format');
  assert.equal(value.snapshot(), before);
  assert.deepEqual(value.export_document(), bytes);
  assert.deepEqual(value.export_original(), source);
});
