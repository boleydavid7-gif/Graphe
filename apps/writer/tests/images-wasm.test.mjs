import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';
import { readFile } from 'node:fs/promises';
import { URL } from 'node:url';
import { crc32 } from 'node:zlib';
import test from 'node:test';
import { WriterEngine } from '../src/wasm/office_wasm.js';
import { initializeWithFonts } from './font-catalog-helper.mjs';

await initializeWithFonts();
const png = new Uint8Array(await readFile(new URL('../../../crates/office-wasm/tests/fixtures/images/sample.png', import.meta.url)));
const jpeg = new Uint8Array(await readFile(new URL('../../../crates/office-wasm/tests/fixtures/images/sample.jpg', import.meta.url)));
let namespace = 0;
function engine(context) {
  const value = new WriterEngine(`image-abi-${++namespace}`);
  context.after(() => value.free());
  return value;
}
function view(value) { return JSON.parse(value.snapshot()); }
function result(json) {
  const response = JSON.parse(json);
  assert.equal(response.ok, true, JSON.stringify(response));
  return response.view;
}
function command(value, input) { return result(value.dispatch(JSON.stringify(input))); }
function insert(value, bytes) { return result(value.insert_image(bytes, view(value).revision)); }
function native(value) { return JSON.parse(Buffer.from(value.export_document()).toString('utf8')); }
function picture(file) {
  const block = file.document.sections.flatMap((section) => section.blocks).find((block) => block.kind === 'image');
  assert.ok(block, 'Native file contains a typed image block');
  return block.content;
}
function code(expected) {
  return (failure) => {
    assert.equal(JSON.parse(String(failure)).code, expected);
    return true;
  };
}

test('actual WASM inserts PNG/JPEG through binary ownership and exports original pixels without JSON snapshot payloads', (context) => {
  for (const bytes of [png, jpeg]) {
    const value = engine(context);
    const input = bytes.slice();
    const created = insert(value, input);
    input.fill(0);
    assert.equal(created.images.length, 1);
    assert.equal(created.images[0].pixelWidth, 160);
    assert.equal(created.images[0].pixelHeight, 80);
    assert.equal(created.images[0].widthPt, 120);
    assert.equal(created.images[0].heightPt, 60);
    assert.equal(created.selectedImageId, created.images[0].id);
    assert.deepEqual(created.bodyBlocks.map((block) => block.kind), ['paragraph', 'image', 'paragraph']);
    assert.equal(value.snapshot().includes('base64'), false);
    assert.equal(value.snapshot().includes('data:'), false);
    assert.deepEqual(Buffer.from(picture(native(value)).asset.bytes_base64, 'base64'), Buffer.from(bytes));
    const canonical = value.image_png(created.selectedImageId, created.revision);
    assert.deepEqual([...canonical.subarray(0, 8)], [137, 80, 78, 71, 13, 10, 26, 10]);
    const owned = canonical.slice();
    canonical.fill(0);
    assert.deepEqual(value.image_png(created.selectedImageId, created.revision), owned);
  }
});

test('actual WASM image property Apply is atomic, aspect-aware and exact across native reopen and undo/redo', (context) => {
  const value = engine(context);
  const created = insert(value, png);
  const before = value.export_document();
  const changed = command(value, { type: 'update_image', widthPt: 100, altText: 'Café e\u0301 👩🏽‍💻 العربية', alignment: 'center' });
  assert.equal(changed.images[0].widthPt, 100);
  assert.equal(changed.images[0].heightPt, 50);
  assert.equal(changed.images[0].alignment, 'center');
  assert.equal(changed.images[0].altText, 'Café e\u0301 👩🏽‍💻 العربية');
  const after = value.export_document();
  assert.notDeepEqual(after, before);
  const undone = command(value, { type: 'undo' });
  assert.equal(undone.selectedImageId, created.selectedImageId);
  assert.deepEqual(value.export_document(), before);
  command(value, { type: 'redo' });
  assert.deepEqual(value.export_document(), after);
  const reopened = result(value.open_document(after));
  assert.deepEqual(reopened.images, changed.images);
  assert.equal(reopened.selectedImageId, null);
  assert.deepEqual(value.export_document(), after);
});

test('actual WASM rejects stale/invalid images and hidden-caret edits without losing source, selection or history', (context) => {
  const value = engine(context);
  command(value, { type: 'replace_text', text: 'Keep ordinary text' });
  const original = value.export_document();
  const first = view(value);
  for (const bytes of [new Uint8Array(), new Uint8Array([137, 80, 78, 71, 13, 10, 26, 10]), new Uint8Array(2 * 1024 * 1024 + 1)]) {
    assert.equal(JSON.parse(value.insert_image(bytes, first.revision)).ok, false);
    assert.deepEqual(value.export_document(), original);
    assert.deepEqual(view(value), first);
  }
  const created = insert(value, jpeg);
  const before = value.export_document();
  for (const input of [
    { type: 'replace_text', text: 'must not go at the retained old caret' },
    { type: 'insert_paragraph' },
    { type: 'resize_image', widthPt: -2 },
    { type: 'resize_image', widthPt: 10_000 },
    { type: 'update_image', widthPt: 100, heightPt: 100, lockAspect: true, altText: 'must not apply' },
    { type: 'delete_image', expectedRevision: first.revision },
    { type: 'select_image', imageId: 'not-an-image' },
  ]) {
    const rejected = JSON.parse(value.dispatch(JSON.stringify(input)));
    assert.equal(rejected.ok, false, JSON.stringify(input));
    assert.deepEqual(value.export_document(), before);
    assert.deepEqual(view(value), created);
  }
  assert.throws(() => value.image_png(created.selectedImageId, first.revision), code('stale_revision'));
  assert.throws(() => value.selected_text(), code('image_clipboard_unsupported'));
  const deleted = command(value, { type: 'delete', direction: 'backward' });
  assert.equal(deleted.images.length, 0);
  assert.equal(deleted.paragraphs[0].runs.map((run) => run.text).join(''), 'Keep ordinary text');
  command(value, { type: 'undo' });
  assert.deepEqual(value.export_document(), before);
  assert.equal(view(value).selectedImageId, created.selectedImageId);
});

test('actual WASM image-rich native budget rejects the whole second insertion without weakening the 5 MiB limit', (context) => {
  const value = engine(context);
  // A valid, bounded ancillary text chunk increases exact original bytes while
  // leaving the already-small raster unchanged. Node's established CRC library
  // makes this a real decodable PNG rather than a spoofed byte-count fixture.
  const payload = Buffer.alloc(2 * 1024 * 1024 - 1024, 120);
  Buffer.from('comment\0').copy(payload);
  const type = Buffer.from('tEXt');
  const length = Buffer.alloc(4); length.writeUInt32BE(payload.length);
  const checksum = Buffer.alloc(4); checksum.writeUInt32BE(crc32(Buffer.concat([type, payload])));
  const larger = Buffer.concat([png.subarray(0, png.length - 12), length, type, payload, checksum, png.subarray(png.length - 12)]);
  const first = insert(value, larger);
  assert.equal(first.images[0].pixelWidth, 160);
  const moved = command(value, { type: 'move_image_cursor', direction: 'forward' });
  const before = value.export_document();
  const rejected = JSON.parse(value.insert_image(larger, moved.revision));
  assert.equal(rejected.ok, false);
  assert.equal(rejected.error.code, 'document_too_large');
  assert.deepEqual(value.export_document(), before);
  assert.deepEqual(view(value), moved);
});

test('actual WASM image geometry, explicit text navigation and text-selection barriers use the same Rust state', (context) => {
  const value = engine(context);
  const first = command(value, { type: 'replace_text', text: 'Before image' });
  const firstId = first.selection.focus.paragraphId;
  const created = insert(value, png);
  const layout = JSON.parse(value.layout_snapshot('{}', created.revision));
  const placements = layout.layout.pages.flatMap((page) => page.images ?? []);
  assert.equal(placements.length, 1);
  assert.equal(placements[0].imageId, created.selectedImageId);
  assert.equal(placements[0].rect.width, 120 * 64);
  assert.equal(placements[0].rect.height, 60 * 64);
  assert.equal(JSON.parse(value.selection_geometry('{}', created.revision)).geometry, null);
  const moved = command(value, { type: 'move_image_cursor', direction: 'forward' });
  assert.equal(moved.revision, created.revision);
  assert.equal(moved.selectedImageId, null);
  const lastId = moved.selection.focus.paragraphId;
  command(value, { type: 'replace_text', text: 'After image' });
  const before = value.export_document();
  const rejected = JSON.parse(value.dispatch(JSON.stringify({ type: 'replace_text', text: '', selection: {
    anchor: { paragraphId: firstId, utf16: 0 }, focus: { paragraphId: lastId, utf16: 11 },
  } })));
  assert.equal(rejected.ok, false);
  assert.deepEqual(value.export_document(), before);
  const selected = command(value, { type: 'select_image', imageId: created.selectedImageId });
  const pointer = { type: 'hit', pageNumber: layout.layout.pages[0].number,
    x: placements[0].rect.x + 10, y: placements[0].rect.y + 10 };
  result(value.dispatch(JSON.stringify({ type: 'select_image', imageId: selected.selectedImageId })));
  const action = JSON.parse(value.layout_action('{}', JSON.stringify(pointer), selected.revision));
  assert.equal(action.view.selectedImageId, created.selectedImageId, 'A text hit inside a picture never jumps to an old caret');
});

for (const format of ['docx', 'odt']) {
  test(`actual WASM ${format.toUpperCase()} image edit/export/reopen retains supported picture data and immutable originals`, (context) => {
    const value = engine(context);
    const created = insert(value, png);
    command(value, { type: 'update_image', widthPt: 180, altText: 'A local diagram 👩🏽‍💻', alignment: 'right' });
    command(value, { type: 'move_image_cursor', direction: 'forward' });
    command(value, { type: 'replace_text', text: 'Caption e\u0301 👩🏽‍💻' });
    const exported = value.export_format(format, true, view(value).revision);
    const opened = result(value.import_format(exported, format, `picture.${format}`));
    assert.equal(opened.images.length, 1);
    assert.equal(opened.images[0].altText, 'A local diagram 👩🏽‍💻');
    assert.equal(opened.images[0].widthPt, 180);
    assert.equal(opened.images[0].heightPt, 90);
    assert.equal(opened.images[0].alignment, 'right');
    assert.deepEqual(value.export_original(), exported);
    assert.deepEqual(Buffer.from(picture(native(value)).asset.bytes_base64, 'base64'), Buffer.from(png));
    command(value, { type: 'select_image', imageId: opened.images[0].id });
    command(value, { type: 'update_image', widthPt: 160, altText: 'Edited local diagram', alignment: 'left' });
    const saved = value.export_document();
    result(value.open_document(saved));
    assert.deepEqual(value.export_document(), saved);
    assert.deepEqual(value.export_original(), exported);
    const next = value.export_format(format, true, view(value).revision);
    const reimported = result(value.import_format(next, format, `edited.${format}`));
    assert.equal(reimported.images[0].altText, 'Edited local diagram');
    assert.equal(reimported.images[0].widthPt, 160);
    assert.equal(reimported.images[0].alignment, 'left');
    assert.notEqual(reimported.images[0].id, created.selectedImageId, 'External adapters allocate new document identities');
  });
}
