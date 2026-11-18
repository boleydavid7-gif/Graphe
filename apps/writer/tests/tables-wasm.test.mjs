import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import { URL } from 'node:url';
import initialize, { WriterEngine } from '../src/wasm/office_wasm.js';

await initialize({ module_or_path: await readFile(new URL('../src/wasm/office_wasm_bg.wasm', import.meta.url)) });

function engine(context) {
  const value = new WriterEngine(context.name);
  context.after(() => value.free());
  return value;
}
function snapshot(value) { return JSON.parse(value.snapshot()); }
function command(value, input) {
  const result = JSON.parse(value.dispatch(JSON.stringify(input)));
  assert.equal(result.ok, true, JSON.stringify(result));
  return result.view;
}
function caret(paragraphId, utf16 = 0) {
  return { anchor: { paragraphId, utf16 }, focus: { paragraphId, utf16 } };
}

test('actual WASM tables edit Unicode, format, undo, serialize and reopen with stable identities', (context) => {
  const value = engine(context);
  const created = command(value, { type: 'insert_table', rows: 2, columns: 2 });
  const first = created.selection.focus.paragraphId;
  const tableId = created.table.tableId;
  command(value, { type: 'replace_text', text: 'e\u0301👩🏽‍💻' });
  command(value, { type: 'format_character', bold: true,
    selection: { anchor: { paragraphId: first, utf16: 0 }, focus: { paragraphId: first, utf16: 9 } } });
  const moved = command(value, { type: 'move_table_cell', direction: 'forward' });
  assert.equal(moved.table.column, 1);
  const typed = command(value, { type: 'replace_text', text: 'Second' });
  const bytes = value.export_document();
  assert.equal(command(value, { type: 'undo' }).paragraphs[2].runs.length, 0);
  command(value, { type: 'redo' });
  assert.deepEqual(value.export_document(), bytes);
  const restored = JSON.parse(value.open_document(bytes));
  assert.equal(restored.ok, true, JSON.stringify(restored));
  assert.equal(restored.view.documentId, typed.documentId);
  assert.equal(restored.view.paragraphs[1].id, first);
  assert.equal(restored.view.paragraphs[1].tableCell.tableId, tableId);
  assert.equal(restored.view.paragraphs[1].runs[0].format.bold, true);
  assert.deepEqual(value.export_document(), bytes);
});

test('actual WASM row and column commands are atomic, stale guarded and undoable', (context) => {
  const value = engine(context);
  const created = command(value, { type: 'insert_table', rows: 2, columns: 2 });
  const before = value.export_document();
  const expanded = command(value, { type: 'table_action', action: 'insert_row_below' });
  assert.equal(expanded.table.rowCount, 3);
  assert.equal(expanded.table.row, 1);
  command(value, { type: 'undo' });
  assert.deepEqual(value.export_document(), before);
  command(value, { type: 'redo' });
  assert.equal(command(value, { type: 'table_action', action: 'insert_column_after' }).table.columnCount, 3);
  const snapshotBefore = snapshot(value);
  const invalid = JSON.parse(value.dispatch(JSON.stringify({ type: 'table_action', action: 'delete_table',
    expectedRevision: created.revision })));
  assert.equal(invalid.error.code, 'stale_revision');
  assert.deepEqual(snapshot(value), snapshotBefore);
  assert.equal(command(value, { type: 'table_action', action: 'delete_column' }).table.columnCount, 2);
  assert.equal(command(value, { type: 'table_action', action: 'delete_row' }).table.rowCount, 2);
});

test('actual WASM rejects cross-cell edits and surrogate offsets without corrupting tables', (context) => {
  const value = engine(context);
  const created = command(value, { type: 'insert_table', rows: 1, columns: 2 });
  const first = created.selection.focus.paragraphId;
  const second = created.paragraphs[2].id;
  command(value, { type: 'replace_text', text: '😀one' });
  command(value, { type: 'set_selection', selection: caret(first, 0) });
  const before = value.export_document();
  command(value, { type: 'delete', direction: 'backward' });
  assert.deepEqual(value.export_document(), before);
  for (const input of [
    { type: 'replace_text', text: 'bad', selection: { anchor: { paragraphId: first, utf16: 0 }, focus: { paragraphId: second, utf16: 0 } } },
    { type: 'set_selection', selection: caret(first, 1) },
    { type: 'insert_table', rows: 21, columns: 1 },
    { type: 'insert_table', rows: 1, columns: 1 },
    { type: 'table_action', action: 'delete_row' },
  ]) {
    const rejected = JSON.parse(value.dispatch(JSON.stringify(input)));
    assert.equal(rejected.ok, false, JSON.stringify(input));
    assert.deepEqual(value.export_document(), before);
  }
});

test('actual WASM cell paragraphs, keyboard traversal and native layout share core positions', (context) => {
  const value = engine(context);
  command(value, { type: 'insert_table', rows: 1, columns: 2 });
  const typed = command(value, { type: 'replace_text', text: 'First\nSecond' });
  assert.equal(typed.paragraphs.filter((p) => p.tableCell?.column === 0).length, 2);
  const bytes = value.export_document();
  const layout = JSON.parse(value.layout_snapshot('{}', typed.revision));
  assert.ok(layout.layout.pages[0].lines.some((line) => line.paragraphId === typed.selection.focus.paragraphId));
  const next = command(value, { type: 'move_table_cell', direction: 'forward' });
  assert.equal(next.table.column, 1);
  assert.equal(next.revision, typed.revision);
  const exited = command(value, { type: 'move_table_cell', direction: 'forward' });
  assert.equal(exited.table, null);
  assert.equal(exited.revision, typed.revision);
  assert.deepEqual(value.export_document(), bytes);
});
