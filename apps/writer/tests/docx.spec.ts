import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { inflateRawSync } from 'node:zlib';
import { test, expect, type BrowserContext, type Download, type Page } from '@playwright/test';
import { documentAction, focusEditor } from './step7-test-helpers';

interface NativeFile {
  format_version: number;
  document: {
    schema_version: number;
    id: string;
    metadata: { title: string };
    sections: {
      properties: { page_size?: { width: number; height: number } } | null;
      blocks: { content: { runs: { text: string; format: Record<string, unknown> }[]; format: Record<string, unknown> } }[];
    }[];
  };
  source?: { format: string; filename: string; bytes_base64: string };
}

const editor = (page: Page) => page.getByRole('textbox', { name: 'Document content', exact: true });
const save = (page: Page) => page.getByRole('button', { name: 'Save', exact: true });
const basicText = 'DOCX hello — café 👩🏽‍💻';
const fixtureUrl = (name: string) => new URL(`../../../tests/fixtures/docx/${name}`, import.meta.url);

async function start(page: Page) {
  await page.goto('/');
  await expect(editor(page)).toBeVisible();
  await expect(save(page)).toBeEnabled();
}

async function importDocx(page: Page, name = 'supported-basic.docx') {
  await page.getByLabel('Import Writer file', { exact: true }).setInputFiles({
    name,
    mimeType: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
    buffer: await readFile(fixtureUrl(name)),
  });
}

async function downloaded(download: Download): Promise<Buffer> {
  const path = await download.path();
  if (!path) throw new Error('Chromium did not retain the downloaded file');
  return readFile(path);
}

async function original(page: Page): Promise<Buffer> {
  const pending = page.waitForEvent('download');
  await documentAction(page, 'Download original');
  return downloaded(await pending);
}

async function exportNative(page: Page): Promise<Buffer> {
  await documentAction(page, 'Export');
  const dialog = page.getByRole('dialog', { name: 'Export Writer file', exact: true });
  await expect(dialog).toBeVisible();
  await expect(dialog).toContainText(/original/i);
  await expect(dialog).toContainText(/deleted|removed/i);
  const pending = page.waitForEvent('download');
  await dialog.getByRole('button', { name: 'Download Writer file', exact: true }).click();
  const download = await pending;
  expect(download.suggestedFilename()).toMatch(/\.writer$/);
  return downloaded(download);
}

async function storedFiles(page: Page): Promise<NativeFile[]> {
  const records = await page.evaluate(async () => {
    if (!(await indexedDB.databases()).some((database) => database.name === 'writer-documents')) return [];
    const database = await new Promise<IDBDatabase>((resolve, reject) => {
      const request = indexedDB.open('writer-documents', 1);
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error);
    });
    try {
      return await new Promise<string[]>((resolve, reject) => {
        const transaction = database.transaction('documents', 'readonly');
        const request = transaction.objectStore('documents').getAll();
        let results: string[] = [];
        request.onsuccess = () => {
          results = (request.result as { bytes: Uint8Array }[]).map((record) => new TextDecoder().decode(record.bytes));
        };
        transaction.oncomplete = () => resolve(results);
        transaction.onerror = () => reject(transaction.error);
        transaction.onabort = () => reject(transaction.error);
      });
    } finally { database.close(); }
  });
  return records.map((record) => JSON.parse(record) as NativeFile);
}

/** Test-only ZIP inventory, for reading packages emitted by the actual Rust exporter. */
function zipPart(bytes: Buffer, name: string): string {
  const end = bytes.lastIndexOf(Buffer.from([0x50, 0x4b, 0x05, 0x06]));
  if (end < 0) throw new Error('Missing ZIP end record');
  let cursor = bytes.readUInt32LE(end + 16);
  for (let index = 0; index < bytes.readUInt16LE(end + 10); index += 1) {
    if (bytes.readUInt32LE(cursor) !== 0x02014b50) throw new Error('Invalid central entry');
    const method = bytes.readUInt16LE(cursor + 10);
    const size = bytes.readUInt32LE(cursor + 20);
    const nameLength = bytes.readUInt16LE(cursor + 28);
    const extraLength = bytes.readUInt16LE(cursor + 30);
    const commentLength = bytes.readUInt16LE(cursor + 32);
    const path = bytes.subarray(cursor + 46, cursor + 46 + nameLength).toString('utf8');
    if (path === name) {
      const local = bytes.readUInt32LE(cursor + 42);
      const data = local + 30 + bytes.readUInt16LE(local + 26) + bytes.readUInt16LE(local + 28);
      const payload = bytes.subarray(data, data + size);
      if (method !== 0 && method !== 8) throw new Error('Unexpected exporter compression');
      return (method === 8 ? inflateRawSync(payload) : payload).toString('utf8');
    }
    cursor += 46 + nameLength + extraLength + commentLength;
  }
  throw new Error(`Missing exported ZIP part ${name}`);
}

test('DOCX original is saved before editing and survives native save, reopen, export and fresh import', async ({ page, context }) => {
  await start(page);
  const source = await readFile(fixtureUrl('supported-basic.docx'));
  await importDocx(page);
  await expect(editor(page)).toHaveText(basicText);
  await expect(page.locator('.save-state')).toHaveText('Saved on this device');
  const initial = await storedFiles(page);
  expect(initial).toHaveLength(1);
  expect(initial[0]?.source?.filename).toBe('supported-basic.docx');
  expect(Buffer.from(initial[0]?.source?.bytes_base64 ?? '', 'base64')).toEqual(source);

  const edited = 'Only the edited version 👋';
  await focusEditor(page);
  await page.keyboard.press('Control+a');
  await page.keyboard.insertText(edited);
  await page.getByRole('textbox', { name: 'Document title' }).fill('DOCX preservation');
  await save(page).click();
  await expect(page.locator('.save-state')).toHaveText('Saved on this device');
  await documentAction(page, 'Close');
  await page.getByRole('button', { name: 'Open', exact: true }).click();
  await page.getByRole('button', { name: /DOCX preservation/ }).click();
  await expect(editor(page)).toHaveText(edited);
  expect(await original(page)).toEqual(source);

  const pendingWord = page.waitForEvent('download');
  await documentAction(page, 'Export Word');
  const wordDownload = await pendingWord;
  expect(wordDownload.suggestedFilename()).toMatch(/\.docx$/);
  const xml = zipPart(await downloaded(wordDownload), 'word/document.xml');
  expect(xml).toContain(edited);
  expect(xml).not.toContain('DOCX hello');
  expect(await original(page)).toEqual(source);

  const bytes = await exportNative(page);
  const native = JSON.parse(bytes.toString('utf8')) as NativeFile;
  expect(native.format_version).toBe(2);
  expect(native.document.schema_version).toBe(2);
  expect(Buffer.from(native.source?.bytes_base64 ?? '', 'base64')).toEqual(source);
  await page.reload();
  await expect(editor(page)).toBeVisible();
  await page.getByRole('button', { name: 'Open', exact: true }).click();
  await page.getByRole('button', { name: /DOCX preservation/ }).click();
  await expect(editor(page)).toHaveText(edited);
  expect(await original(page)).toEqual(source);

  const fresh = await context.browser()?.newContext({ baseURL: new URL(page.url()).origin, acceptDownloads: true });
  if (!fresh) throw new Error('Missing browser for isolated native import');
  try {
    const other = await fresh.newPage();
    await start(other);
    await other.getByLabel('Import Writer file', { exact: true }).setInputFiles({ name: 'preserved.writer', mimeType: 'application/json', buffer: bytes });
    await expect(editor(other)).toHaveText(edited);
    expect(await original(other)).toEqual(source);
  } finally { await fresh.close(); }
});

test('DOCX mixed formatting and section geometry remain present in the saved native file', async ({ page }) => {
  await start(page);
  await importDocx(page, 'supported-formatting.docx');
  await expect(page.getByRole('textbox', { name: 'Document title' })).toHaveValue('Supported Writer DOCX – café');
  await expect(editor(page).locator('p')).toHaveCount(3);
  const file = JSON.parse((await exportNative(page)).toString('utf8')) as NativeFile;
  expect(file.document.sections).toHaveLength(2);
  expect(file.document.sections[0]?.properties?.page_size).toMatchObject({ width: 12240, height: 15840 });
  expect(file.document.sections[1]?.properties?.page_size).toMatchObject({ width: 11906, height: 16838 });
  const paragraph = file.document.sections[0]?.blocks[0]?.content;
  expect(paragraph?.format).toMatchObject({ alignment: 'center', line_spacing: { kind: 'exact', height: 360 } });
  expect(paragraph?.runs[0]?.format).toMatchObject({ bold: true, underline: 'double', font_family: 'Arimo', font_size: 28 });
  expect(paragraph?.runs[1]?.format.bold).toBe(false);
  expect(paragraph?.runs[1]?.text).toContain('日本語 & <XML> \nSecond line\ttabbed');
  expect(await original(page)).toEqual(await readFile(fixtureUrl('supported-formatting.docx')));
});

test('unsupported DOCX stays read-only and its exact original is downloadable without replacing unsaved work', async ({ page }) => {
  await start(page);
  await focusEditor(page);
  await page.keyboard.insertText('Keep this unsaved work');
  await importDocx(page, 'writer-source-preservation.docx');
  await page.getByRole('dialog', { name: 'Save your changes?', exact: true }).getByRole('button', { name: 'Discard changes', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'DOCX preserved read-only', exact: true });
  await expect(dialog).toBeVisible();
  await expect(dialog).toContainText(/unsupported/i);
  await expect(page.locator('.editor-surface')).toHaveText('Keep this unsaved work');
  await expect(page.locator('.save-state')).toContainText('Unsaved');
  const pending = page.waitForEvent('download');
  await dialog.getByRole('button', { name: 'Download original DOCX', exact: true }).click();
  expect(await downloaded(await pending)).toEqual(await readFile(fixtureUrl('writer-source-preservation.docx')));
  await dialog.getByRole('button', { name: 'Close', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Undo', exact: true })).toBeEnabled();
  expect(await storedFiles(page)).toHaveLength(0);
});

test('a DOCX preservation save failure keeps the prior live content, history and dirty state', async ({ page }) => {
  await start(page);
  await focusEditor(page);
  await page.keyboard.insertText('Prior live work');
  await page.evaluate(() => {
    IDBObjectStore.prototype.put = function (): IDBRequest<IDBValidKey> {
      throw new DOMException('DOCX preservation quota regression', 'QuotaExceededError');
    };
  });
  await importDocx(page);
  await page.getByRole('dialog', { name: 'Save your changes?', exact: true }).getByRole('button', { name: 'Discard changes', exact: true }).click();
  await expect(page.getByRole('alert')).toContainText('storage is full');
  await expect(editor(page)).toHaveText('Prior live work');
  await expect(page.locator('.save-state')).toContainText('Unsaved');
  await expect(page.getByRole('button', { name: 'Undo', exact: true })).toBeEnabled();
  await expect(page.getByRole('button', { name: 'Download original', exact: true })).toHaveCount(0);
  expect(await storedFiles(page)).toHaveLength(0);
});

test('DOCX editing is activated only after the original-preservation transaction commits', async ({ page }) => {
  await start(page);
  await page.getByRole('textbox', { name: 'Document title' }).fill('Prior unsaved document');
  await focusEditor(page);
  await page.keyboard.insertText('Keep the prior session until commit');
  await page.evaluate(() => {
    const state = window as Window & { docxPreservationHeld?: boolean; releaseDocxPreservation?: boolean };
    state.docxPreservationHeld = false;
    state.releaseDocxPreservation = false;
    const originalPut = IDBObjectStore.prototype.put;
    IDBObjectStore.prototype.put = function (value: unknown, key?: IDBValidKey): IDBRequest<IDBValidKey> {
      IDBObjectStore.prototype.put = originalPut;
      const request = key === undefined ? originalPut.call(this, value) : originalPut.call(this, value, key);
      const record = value as { id: string };
      request.addEventListener('success', () => {
        const keepAlive = () => {
          const check = this.get(record.id);
          check.onsuccess = () => {
            state.docxPreservationHeld = true;
            if (!state.releaseDocxPreservation) keepAlive();
          };
        };
        keepAlive();
      });
      return request;
    };
  });
  await importDocx(page);
  await page.getByRole('dialog', { name: 'Save your changes?', exact: true }).getByRole('button', { name: 'Discard changes', exact: true }).click();
  await expect.poll(() => page.evaluate(() => (window as Window & { docxPreservationHeld?: boolean }).docxPreservationHeld)).toBe(true);
  try {
    await expect(editor(page)).toHaveText('Keep the prior session until commit');
    await expect(editor(page)).toHaveAttribute('contenteditable', 'false');
    await expect(page.getByRole('textbox', { name: 'Document title' })).toHaveValue('Prior unsaved document');
    await expect(page.getByRole('textbox', { name: 'Document title' })).toBeDisabled();
    await expect(page.locator('.save-state')).toContainText('Unsaved');
    await expect(page.getByRole('button', { name: 'Download original', exact: true })).toHaveCount(0);
  } finally {
    await page.evaluate(() => { (window as Window & { releaseDocxPreservation?: boolean }).releaseDocxPreservation = true; });
  }
  await expect(editor(page)).toHaveText(basicText);
  await expect(page.locator('.save-state')).toHaveText('Saved on this device');
  const files = await storedFiles(page);
  expect(files).toHaveLength(1);
  expect(Buffer.from(files[0]?.source?.bytes_base64 ?? '', 'base64')).toEqual(await readFile(fixtureUrl('supported-basic.docx')));
});

test('importing the same DOCX twice creates distinct saved documents without overwriting either original', async ({ page }) => {
  await start(page);
  await importDocx(page);
  await expect(page.locator('.save-state')).toHaveText('Saved on this device');
  const first = await storedFiles(page);
  expect(first).toHaveLength(1);
  await importDocx(page);
  await expect.poll(async () => (await storedFiles(page)).length).toBe(2);
  const both = await storedFiles(page);
  expect(both).toHaveLength(2);
  expect(new Set(both.map((file) => file.document.id)).size).toBe(2);
  expect(both.map((file) => file.document.id)).toContain(first[0]?.document.id);
  const source = await readFile(fixtureUrl('supported-basic.docx'));
  for (const file of both) expect(Buffer.from(file.source?.bytes_base64 ?? '', 'base64')).toEqual(source);
});

test('a native file with an unsupported retained source cannot replace the live document or masquerade as raw DOCX', async ({ page }) => {
  await start(page);
  await importDocx(page);
  await expect(editor(page)).toHaveText(basicText);
  const native = JSON.parse((await exportNative(page)).toString('utf8')) as NativeFile;
  if (!native.source) throw new Error('Supported DOCX native export did not retain its source');
  native.source.filename = 'writer-source-preservation.docx';
  native.source.bytes_base64 = (await readFile(fixtureUrl('writer-source-preservation.docx'))).toString('base64');
  await page.getByLabel('Import Writer file', { exact: true }).setInputFiles({
    name: 'unsupported-source.writer', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify(native)),
  });
  await expect(page.getByRole('alert')).toContainText('unsupported');
  await expect(page.getByRole('dialog', { name: 'DOCX preserved read-only', exact: true })).toHaveCount(0);
  await expect(editor(page)).toHaveText(basicText);
  expect(await original(page)).toEqual(await readFile(fixtureUrl('supported-basic.docx')));
});

test('a retained DOCX survives a complete Chromium process restart', async ({ playwright }, testInfo) => {
  const profile = await mkdtemp(join(tmpdir(), 'writer-docx-restart-'));
  let context: BrowserContext | undefined;
  const launch = () => playwright.chromium.launchPersistentContext(profile, {
    ...testInfo.project.use.launchOptions,
    baseURL: testInfo.project.use.baseURL ?? 'http://127.0.0.1:4173',
    headless: true,
    acceptDownloads: true,
  });
  try {
    context = await launch();
    const page = await context.newPage();
    await start(page);
    await importDocx(page);
    await expect(editor(page)).toHaveText(basicText);
    await page.getByRole('textbox', { name: 'Document title' }).fill('Retained DOCX restart');
    await save(page).click();
    await expect(page.locator('.save-state')).toHaveText('Saved on this device');
    await context.close();
    context = undefined;
    context = await launch();
    const reopened = await context.newPage();
    await start(reopened);
    await reopened.getByRole('button', { name: 'Open', exact: true }).click();
    await reopened.getByRole('button', { name: /Retained DOCX restart/ }).click();
    await expect(editor(reopened)).toHaveText(basicText);
    expect(await original(reopened)).toEqual(await readFile(fixtureUrl('supported-basic.docx')));
  } finally {
    await context?.close();
    await rm(profile, { recursive: true, force: true });
  }
});


test.describe('Common font compatibility choices', () => {
  const choices = [
    ['Times New Roman', 'Tinos'],
    ['Arial', 'Arimo'],
    ['Calibri', 'Carlito'],
    ['Cambria', 'Caladea'],
  ] as const;

  for (const [family, substitute] of choices) {
    test(`${family} identifies its substitute but saves the requested font name`, async ({ page }) => {
      await start(page);
      const fontPicker = page.locator('#font-family');
      const common = fontPicker.locator('optgroup[label="Common fonts (licensed substitutes)"]');
      await expect(common.locator(`option[value="${family}"]`))
        .toHaveText(`${family} (uses ${substitute})`);

      await focusEditor(page);
      await page.keyboard.insertText('Font compatibility sample');
      await page.keyboard.press('Control+Home');
      await page.keyboard.press('Shift+End');
      await fontPicker.selectOption(family);
      await expect(fontPicker).toHaveValue(family);

      await page.getByRole('button', { name: 'Details', exact: true }).click();
      const preview = page.locator('.font-family-preview');
      await expect(preview).toContainText(`Uses ${substitute} — substitute, not the original font`);
      await expect(preview).toHaveAttribute('style', new RegExp(`font-family:.*${substitute}`));

      await documentAction(page, 'Save As…');
      const saveAs = page.getByRole('dialog', { name: 'Save As', exact: true });
      await saveAs.getByRole('combobox', { name: 'File format', exact: true }).selectOption('writer');
      const nativePending = page.waitForEvent('download');
      await saveAs.getByRole('button', { name: 'Save copy', exact: true }).click();
      const native = JSON.parse((await downloaded(await nativePending)).toString('utf8')) as NativeFile;
      const runs = native.document.sections.flatMap((section) =>
        section.blocks.flatMap((block) => block.content.runs));
      expect(runs.some((run) => run.text.includes('Font compatibility sample') &&
        run.format.font_family === family)).toBeTruthy();

      const wordPending = page.waitForEvent('download');
      await documentAction(page, 'Export Word');
      const wordXml = zipPart(await downloaded(await wordPending), 'word/document.xml');
      expect(wordXml).toContain(`w:ascii="${family}"`);
      expect(wordXml).not.toContain(`w:ascii="${substitute}"`);
    });
  }
});
