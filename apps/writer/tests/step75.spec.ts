import { readFile } from 'node:fs/promises';
import { test, expect, type Download, type Page } from '@playwright/test';
import { documentAction, focusEditor, readyPages } from './step7-test-helpers';

type EditableFormat = 'docx' | 'odt' | 'rtf' | 'txt';
type FileFormat = EditableFormat | 'writer' | 'pdf';
const initial = 'Format source — Café e\u0301 👩🏽‍💻';
const edited = 'Edited local document — e\u0301 👩🏽‍💻';
const second = 'Second paragraph العربية नमस्ते שלום';
const media: Record<FileFormat, string> = {
  writer: 'application/json',
  docx: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
  odt: 'application/vnd.oasis.opendocument.text', rtf: 'application/rtf',
  txt: 'text/plain', pdf: 'application/pdf',
};

interface NativeFile {
  readonly format: string;
  readonly format_version: number;
  readonly document: {
    readonly id: string;
    readonly metadata: { readonly title: string };
    readonly styles: readonly unknown[];
    readonly sections: readonly {
      readonly blocks: readonly {
        readonly content: {
          readonly id: string;
          readonly runs: readonly { readonly text: string; readonly format: { readonly bold?: boolean; readonly italic?: boolean } }[];
        };
      }[];
    }[];
  };
  readonly source?: { readonly format: string; readonly filename: string; readonly bytes_base64: string };
}

const input = (page: Page) => page.getByRole('textbox', { name: 'Document content', exact: true });

// Serde writes omitted optional defaults as explicit null. Preserve every
// actual value and container/run distinction while comparing the sparse input.
function withoutUnspecified(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(withoutUnspecified);
  if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value)
    .filter(([, item]) => item !== null).map(([key, item]) => [key, withoutUnspecified(item)]));
  return value;
}

async function start(page: Page) {
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Save', exact: true })).toBeEnabled();
  await readyPages(page);
}

async function downloaded(download: Download) {
  const path = await download.path();
  if (!path) throw new Error('The browser did not retain the downloaded file');
  return readFile(path);
}

/** Use the compact Save As dialog and current Rust assessment. */
async function saveAs(page: Page, format: FileFormat, filename?: string) {
  await documentAction(page, 'Save As…');
  const dialog = page.getByRole('dialog', { name: 'Save As', exact: true });
  await expect(dialog).toBeVisible();
  await dialog.getByRole('combobox', { name: 'File format', exact: true }).selectOption(format);
  if (filename) await dialog.getByRole('textbox', { name: 'Filename', exact: true }).fill(filename);
  await expect(dialog.getByRole('checkbox')).toHaveCount(0);
  await expect(dialog.getByRole('button', { name: 'Save copy', exact: true })).toBeEnabled();
  const pending = page.waitForEvent('download');
  await dialog.getByRole('button', { name: 'Save copy', exact: true }).click();
  const download = await pending;
  expect(download.suggestedFilename()).toMatch(new RegExp(`\\.${format}$`, 'i'));
  await expect(dialog).toHaveCount(0);
  return { bytes: await downloaded(download), filename: download.suggestedFilename() };
}

/** Exercise Open -> Open file, rather than importing through a hidden engine hook. */
async function openFile(page: Page, format: FileFormat, bytes: Buffer, filename: string) {
  await page.getByRole('button', { name: 'Open', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Open document', exact: true });
  await expect(dialog).toBeVisible();
  const chosen = page.waitForEvent('filechooser');
  await dialog.getByRole('button', { name: 'Open file…', exact: true }).click();
  await (await chosen).setFiles({ name: filename, mimeType: media[format], buffer: bytes });
}

async function waitSaved(page: Page) {
  await expect(page.locator('.save-state')).toHaveText('Saved on this device', { timeout: 10000 });
}

async function readNative(page: Page): Promise<NativeFile> {
  const file = await saveAs(page, 'writer');
  return JSON.parse(file.bytes.toString('utf8')) as NativeFile;
}

for (const format of ['docx', 'odt', 'rtf', 'txt'] as const) {
  test(`${format.toUpperCase()} opens through Open file, edits in Rust, saves locally, exports and reopens`, async ({ page, browser }) => {
    await start(page);
    await focusEditor(page);
    await page.keyboard.insertText(initial);
    await page.keyboard.press('Enter');
    await page.keyboard.insertText(second);
    await page.getByRole('textbox', { name: 'Document title', exact: true }).fill(`${format.toUpperCase()} source`);
    await waitSaved(page);
    const original = await saveAs(page, format, `source-${format}.${format}`);

    // A fresh browser-local library proves import does not depend on an
    // already-open JavaScript projection or the producer's IndexedDB record.
    const importedContext = await browser.newContext({ acceptDownloads: true });
    const other = await importedContext.newPage();
    try {
      await start(other);
      await openFile(other, format, original.bytes, original.filename);
      await expect(other.getByRole('dialog', { name: 'Open document', exact: true })).toHaveCount(0);
      await expect(input(other).locator('p')).toHaveCount(2);
      await expect(input(other).locator('p').first()).toHaveText(initial);
      await expect(input(other).locator('p').last()).toHaveText(second);
      await waitSaved(other);
      await readyPages(other);

      await focusEditor(other);
      await other.keyboard.press('Control+Home');
      await other.keyboard.press('Control+Shift+End');
      await other.keyboard.insertText(edited);
      await other.keyboard.press('Enter');
      await other.keyboard.insertText(second);
      await other.keyboard.press('Control+Home');
      await other.keyboard.press('Control+Shift+End');
      await other.getByRole('button', { name: 'Bold', exact: true }).click();
      await expect(other.getByRole('button', { name: 'Bold', exact: true })).toHaveAttribute('aria-pressed', 'true');
      await other.getByRole('button', { name: 'Italic', exact: true }).click();
      const title = `${format.toUpperCase()} saved recovery`;
      await other.getByRole('textbox', { name: 'Document title', exact: true }).fill(title);
      await other.getByRole('button', { name: 'Save', exact: true }).click();
      await waitSaved(other);

      const before = await readNative(other);
      expect(before.source?.format).toBe(format);
      expect(before.source?.filename).toBe(original.filename);
      expect(Buffer.from(before.source?.bytes_base64 ?? '', 'base64')).toEqual(original.bytes);
      expect(before.document.metadata.title).toBe(title);
      expect(before.document.sections.flatMap((section) => section.blocks).flatMap((block) => block.content.runs)
        .filter((run) => run.text.length).every((run) => run.format.bold && run.format.italic)).toBe(true);

      await other.reload();
      await expect(other.getByRole('button', { name: 'Save', exact: true })).toBeEnabled();
      await other.getByRole('button', { name: 'Open', exact: true }).click();
      await other.getByRole('button', { name: new RegExp(title) }).click();
      await expect(input(other).locator('p').first()).toHaveText(edited);
      await expect(input(other).locator('p').last()).toHaveText(second);
      await waitSaved(other);
      const persisted = await readNative(other);
      expect(persisted).toEqual(before);

      const exported = await saveAs(other, format, `edited-${format}.${format}`);
      await expect(other.locator('.save-state')).toHaveText('Saved on this device');
      await openFile(other, format, exported.bytes, exported.filename);
      await expect(other.getByRole('dialog', { name: 'Open document', exact: true })).toHaveCount(0);
      await expect(input(other).locator('p').first()).toHaveText(edited);
      await expect(input(other).locator('p').last()).toHaveText(second);
      await waitSaved(other);
      const final = await readNative(other);
      const finalRuns = final.document.sections.flatMap((section) => section.blocks)
        .flatMap((block) => block.content.runs).filter((run) => run.text.length);
      expect(finalRuns.length).toBeGreaterThan(0);
      if (format === 'txt') expect(finalRuns.every((run) => !run.format.bold && !run.format.italic)).toBe(true);
      else expect(finalRuns.every((run) => run.format.bold && run.format.italic)).toBe(true);
      expect(Buffer.from(final.source?.bytes_base64 ?? '', 'base64')).toEqual(exported.bytes);
    } finally { await importedContext.close(); }
  });
}

test('native Save As retains identifiers, existing styles and metadata when opened from a file', async ({ page }) => {
  await start(page);
  const original = await readFile(new URL('../../../tests/fixtures/native/project-notes.writer', import.meta.url));
  await openFile(page, 'writer', original, 'native-source.writer');
  await expect(page.getByRole('textbox', { name: 'Document title', exact: true })).toHaveValue('Project notes');
  const file = await saveAs(page, 'writer', 'native-copy.writer');
  const expected = JSON.parse(original.toString('utf8')) as NativeFile;
  const actual = JSON.parse(file.bytes.toString('utf8')) as NativeFile;
  expect(withoutUnspecified(actual)).toEqual(withoutUnspecified({ ...expected, format_version: 7,
    document: { ...expected.document, schema_version: 7 } }));
  expect(actual.document.styles.length).toBeGreaterThan(0);
  await expect(page.locator('.save-state')).toHaveText('Not saved yet');
});

test('a brief meaningful-loss warning follows the format and one Save copy preserves rich local content', async ({ page }) => {
  await start(page);
  await focusEditor(page);
  await page.keyboard.insertText('Keep this rich text — e\u0301 👩🏽‍💻');
  await page.keyboard.press('Control+Home');
  await page.keyboard.press('Control+Shift+End');
  await page.getByRole('button', { name: 'Bold', exact: true }).click();
  await page.getByRole('textbox', { name: 'Document title', exact: true }).fill('File format review');
  await waitSaved(page);
  const before = await readNative(page);
  await documentAction(page, 'Save As…');
  const dialog = page.getByRole('dialog', { name: 'Save As', exact: true });
  await dialog.getByRole('combobox', { name: 'File format', exact: true }).selectOption('txt');
  const warning = dialog.getByRole('note', { name: 'Format warning', exact: true });
  await expect(warning).toContainText(/format|bold|text/i);
  await expect(dialog.getByRole('checkbox')).toHaveCount(0);
  await expect(dialog.getByRole('button', { name: 'Save copy', exact: true })).toBeEnabled();
  await page.screenshot({ path: '../../docs/images/writer-file-formats.png' });
  await dialog.getByRole('combobox', { name: 'File format', exact: true }).selectOption('rtf');
  await expect(warning).toHaveCount(0);
  await expect(dialog.getByRole('checkbox')).toHaveCount(0);
  await expect(dialog.getByRole('button', { name: 'Save copy', exact: true })).toBeEnabled();
  await dialog.getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect(page.locator('[data-testid="editor"]:focus, summary[aria-label="Document options"]:focus')).toHaveCount(1);
  expect(await readNative(page)).toEqual(before);
  await expect(page.locator('.save-state')).toHaveText('Saved on this device');

  // Establish keyboard invocation, then do not repair focus after closing the
  // modal. Escape, Cancel and Download must permit the next real key to edit.
  await focusEditor(page);
  await page.keyboard.press('Control+End');
  await page.keyboard.press('Control+Shift+s');
  await expect(dialog).toBeVisible();
  await page.keyboard.press('Escape');
  await page.keyboard.type('!');
  await expect(input(page)).toHaveText('Keep this rich text — e\u0301 👩🏽‍💻!');
  await expect(input(page)).toBeFocused();

  await page.keyboard.press('Control+Shift+s');
  await expect(dialog).toBeVisible();
  await dialog.getByRole('button', { name: 'Cancel', exact: true }).click();
  await page.keyboard.type('?');
  await expect(input(page)).toHaveText('Keep this rich text — e\u0301 👩🏽‍💻!?');
  await expect(input(page)).toBeFocused();

  await page.keyboard.press('Control+Shift+s');
  await dialog.getByRole('combobox', { name: 'File format', exact: true }).selectOption('txt');
  await expect(warning).toBeVisible();
  await expect(dialog.getByRole('checkbox')).toHaveCount(0);
  const copy = page.waitForEvent('download');
  await dialog.getByRole('button', { name: 'Save copy', exact: true }).click();
  await page.keyboard.type('+');
  await copy;
  await expect(input(page)).toHaveText('Keep this rich text — e\u0301 👩🏽‍💻!?+');
  await expect(input(page)).toBeFocused();
});

test('unsupported RTF retains exact original and cannot replace the active local document', async ({ page }) => {
  await start(page);
  await focusEditor(page);
  await page.keyboard.insertText('Keep current document e\u0301 👩🏽‍💻');
  await waitSaved(page);
  const before = await readNative(page);
  const original = Buffer.from('{\\rtf1\\ansi\\trowd\\cellx1000 Unsupported table\\cell\\row}', 'ascii');
  await openFile(page, 'rtf', original, 'unsupported-table.rtf');
  const dialog = page.getByRole('dialog', { name: 'RTF preserved read-only', exact: true });
  await expect(dialog).toBeVisible();
  await expect(dialog).toContainText('current document is unchanged');
  const pending = page.waitForEvent('download');
  await dialog.getByRole('button', { name: 'Download original RTF', exact: true }).click();
  expect(await downloaded(await pending)).toEqual(original);
  await dialog.getByRole('button', { name: 'Close', exact: true }).click();
  expect(await readNative(page)).toEqual(before);
});

test('Open file reports corrupted RTF and export-only PDF inside the dialog without losing current work', async ({ page }) => {
  await start(page);
  await focusEditor(page);
  await page.keyboard.insertText('Keep saved native content');
  await waitSaved(page);
  const before = await readNative(page);
  for (const [format, bytes, filename] of [
    ['rtf', Buffer.from('{\\rtf1 unclosed', 'ascii'), 'bad.rtf'],
    ['pdf', Buffer.from('%PDF-1.7\n', 'ascii'), 'print-only.pdf'],
  ] as const) {
    await openFile(page, format, bytes, filename);
    const dialog = page.getByRole('dialog', { name: 'Open document', exact: true });
    await expect(dialog.getByRole('alert')).toBeVisible();
    if (format === 'pdf') await expect(dialog.getByRole('alert')).toContainText('PDF is export only');
    await dialog.getByRole('button', { name: 'Cancel', exact: true }).click();
    expect(await readNative(page)).toEqual(before);
  }
});

test('PDF Save As exports multiple Rust pages without changing local saves or offering PDF import', async ({ page }) => {
  await start(page);
  await focusEditor(page);
  const lines = Array.from({ length: 80 }, (_, index) => `Page layout line ${index + 1} — e\u0301 👩🏽‍💻`).join('\n');
  const editor = input(page);
  await editor.evaluate((element, text) => {
    const clipboard = new DataTransfer();
    clipboard.setData('text/plain', text);
    element.dispatchEvent(new ClipboardEvent('paste', { clipboardData: clipboard, bubbles: true, cancelable: true }));
  }, lines);
  const root = await readyPages(page);
  const pages = await root.locator('svg[data-layout-page]').count();
  expect(pages).toBeGreaterThan(1);
  await waitSaved(page);
  const before = await readNative(page);
  const pdf = await saveAs(page, 'pdf', 'local-pages.pdf');
  expect(pdf.bytes.subarray(0, 5).toString('ascii')).toBe('%PDF-');
  expect(pdf.bytes.toString('latin1')).toMatch(/%%EOF\s*$/);
  expect([...pdf.bytes.toString('latin1').matchAll(/\/Type\s*\/Page\b/g)]).toHaveLength(pages);
  expect(await readNative(page)).toEqual(before);
  expect(await page.getByLabel('Import Writer file', { exact: true }).getAttribute('accept')).not.toContain('.pdf');
});

test('Save As keeps supplementary Unicode filename characters intact at its filename bound', async ({ page }) => {
  await start(page);
  await focusEditor(page);
  await page.keyboard.insertText('Unicode filename');
  await waitSaved(page);
  const filename = `${'a'.repeat(99)}👩.txt`;
  const copy = await saveAs(page, 'txt', filename);
  expect(copy.filename).toBe(filename);
  expect(copy.filename).not.toContain('\ufffd');
});

test('production assets retain the additional format-library license and attribution texts', async ({ page }) => {
  await start(page);
  await documentAction(page, 'About Graphe');
  const about = page.getByRole('dialog', { name: 'About Graphe', exact: true });
  await expect(about).toBeVisible();
  const href = await about.getByRole('link', { name: 'Third-party format library notices', exact: true }).getAttribute('href');
  expect(href).toMatch(/\/assets\/.+\.txt$/);
  if (!href) throw new Error('Missing bundled third-party format notices');
  const response = await page.request.get(href);
  expect(response.ok()).toBe(true);
  const text = await response.text();
  for (const label of ['pdf-writer 0.15.0', 'svgtypes 0.16.2', 'rclip-rtf 0.1.0', 'rclip-core 0.1.0',
    'arrayvec 0.7.8', 'autocfg 1.5.1', 'euclid 0.22.14', 'kurbo 0.13.1',
    'num-traits 0.2.19', 'polycool 0.4.0', 'ryu 1.0.23', 'siphasher 1.0.4']) {
    expect(text).toContain(label);
  }
  expect(text).toContain('Apache License');
  expect(text).toContain('TERMS AND CONDITIONS');
  expect(text).toContain('Permission is hereby granted');
});
