import { readFile } from 'node:fs/promises';
import { test, expect, type Page } from '@playwright/test';
import { documentAction, focusEditor, glyphPoint, readyPages } from './step7-test-helpers';

const input = (page: Page) => page.getByRole('textbox', { name: 'Document content', exact: true });
const title = (page: Page) => page.getByRole('textbox', { name: 'Document title', exact: true });
const save = (page: Page) => page.getByRole('button', { name: 'Save', exact: true });

interface NativeRecord {
  id: string;
  etag: string;
  file: {
    document: { id: string; metadata: { title: string }; sections: { blocks: { content: {
      id: string; style: string | null; runs: { text: string; format: Record<string, unknown> }[];
      format: Record<string, unknown>;
    } }[] }[] };
    source?: { bytes_base64: string; filename: string };
  };
}

async function start(page: Page) {
  await page.goto('/');
  await expect(save(page)).toBeEnabled();
  await expect(input(page)).toHaveAttribute('contenteditable', 'true');
  await readyPages(page);
}

async function records(page: Page): Promise<NativeRecord[]> {
  const stored = await page.evaluate(async () => {
    if (!(await indexedDB.databases()).some((database) => database.name === 'writer-documents')) return [];
    const database = await new Promise<IDBDatabase>((resolve, reject) => {
      const request = indexedDB.open('writer-documents', 1);
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error);
    });
    try {
      return await new Promise<{ id: string; etag: string; bytes: string }[]>((resolve, reject) => {
        const transaction = database.transaction('documents', 'readonly');
        const request = transaction.objectStore('documents').getAll();
        let values: { id: string; etag: string; bytes: string }[] = [];
        request.onsuccess = () => { values = (request.result as { id: string; etag: string; bytes: Uint8Array }[])
          .map((record) => ({ id: record.id, etag: record.etag, bytes: new TextDecoder().decode(record.bytes) })); };
        transaction.oncomplete = () => resolve(values);
        transaction.onerror = () => reject(transaction.error);
        transaction.onabort = () => reject(transaction.error);
      });
    } finally { database.close(); }
  });
  return stored.map((record) => ({ id: record.id, etag: record.etag, file: JSON.parse(record.bytes) as NativeRecord['file'] }));
}

function text(record: NativeRecord) {
  return record.file.document.sections.flatMap((section) => section.blocks.map((block) => block.content.runs.map((run) => run.text).join(''))).join('\n');
}

async function importReference(page: Page) {
  await page.getByLabel('Import Writer file', { exact: true }).setInputFiles({
    name: 'layout-v1.writer', mimeType: 'application/json',
    buffer: await readFile(new URL('../../../tests/fixtures/layout/layout-v1.writer', import.meta.url)),
  });
  await expect(title(page)).toHaveValue('Typography reference');
  await readyPages(page);
}

async function clickPaper(page: Page) {
  const surface = page.getByTestId('paged-editor').locator('svg[data-layout-page]').first();
  const box = await surface.boundingBox();
  if (!box) throw new Error('The Rust page is not displayed');
  await page.mouse.click(box.x + box.width * 0.2, box.y + box.height * 0.15);
  await expect(input(page)).toBeFocused();
}

async function holdNextSave(page: Page) {
  await page.evaluate(() => {
    const state = window as Window & { writerSaveHeld?: boolean; releaseWriterSave?: boolean; writerSavedSnapshots?: string[] };
    state.writerSaveHeld = false;
    state.releaseWriterSave = false;
    state.writerSavedSnapshots = [];
    const original = IDBObjectStore.prototype.put;
    let held = false;
    IDBObjectStore.prototype.put = function (value: unknown, key?: IDBValidKey): IDBRequest<IDBValidKey> {
      const record = value as { id: string; bytes: Uint8Array };
      state.writerSavedSnapshots?.push(new TextDecoder().decode(record.bytes));
      const request = key === undefined ? original.call(this, value) : original.call(this, value, key);
      if (!held) {
        held = true;
        request.addEventListener('success', () => {
          const keepAlive = () => {
            const check = this.get(record.id);
            check.onsuccess = () => {
              state.writerSaveHeld = true;
              if (!state.releaseWriterSave) keepAlive();
            };
          };
          keepAlive();
        });
      }
      return request;
    };
  });
}

async function saveIsHeld(page: Page) {
  await expect.poll(() => page.evaluate(() => (window as Window & { writerSaveHeld?: boolean }).writerSaveHeld), { timeout: 10000 }).toBe(true);
}

async function releaseSave(page: Page) {
  await page.evaluate(() => { (window as Window & { releaseWriterSave?: boolean }).releaseWriterSave = true; });
}

test('working page input is Rust SVG and the desktop/mobile interface is an actual application', async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 720 });
  await start(page);
  expect(await records(page)).toHaveLength(0);
  await clickPaper(page);
  await page.keyboard.insertText('Writer is working 👩🏽‍💻');
  await expect(input(page)).toHaveText('Writer is working 👩🏽‍💻');
  await readyPages(page);
  expect(await page.getByTestId('paged-editor').locator('svg use').count()).toBeGreaterThan(5);
  await expect(page.getByTestId('paged-editor').locator('svg text')).toHaveCount(0);
  await page.keyboard.press('Control+z');
  await expect(input(page)).toHaveText('');
  await readyPages(page);
  await expect(page.getByTestId('paged-editor').locator('[data-editor-placeholder="true"]')).toHaveAttribute('aria-hidden', 'true');
  await expect(page.locator('.save-state')).toHaveText('Saved on this device', { timeout: 10000 });
  await page.screenshot({ path: '/tmp/writer-step7-first-desktop.png', fullPage: true });
  // The primary screenshots use an actual everyday native document, loaded by
  // Rust. The empty reference state above remains available for design review.
  await page.getByLabel('Import Writer file', { exact: true }).setInputFiles({
    name: 'project-notes.writer', mimeType: 'application/json',
    buffer: await readFile(new URL('../../../tests/fixtures/native/project-notes.writer', import.meta.url)),
  });
  await expect(title(page)).toHaveValue('Project notes');
  await readyPages(page);
  await expect(page.getByTestId('paged-editor').locator('[data-layout-diagnostic-kind="missing_glyph"]')).toHaveCount(0);
  await save(page).click();
  await expect(page.locator('.save-state')).toHaveText('Saved on this device');
  await page.screenshot({ path: '../../docs/images/writer-lamplight-desktop.png', fullPage: true });
  await page.setViewportSize({ width: 420, height: 844 });
  // This branch gives editing more mobile space while the input is focused.
  // Document actions must return when the user leaves the writing surface.
  await expect(input(page)).toBeFocused();
  await expect(page.locator('.mobile-document-actions')).toBeHidden();
  await title(page).click();
  await expect(page.getByRole('navigation', { name: 'Mobile document actions', exact: true })).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await page.screenshot({ path: '/tmp/writer-step7-first-mobile.png', fullPage: true });
  await page.screenshot({ path: '../../docs/images/writer-lamplight-mobile.png', fullPage: true });
});

test('page pointer selection and formatting use Rust content, fonts, geometry and history', async ({ page }) => {
  await start(page);
  await clickPaper(page);
  await page.keyboard.insertText('Select these words in Writer.');
  await readyPages(page);
  const beginning = await glyphPoint(page, 0, 0);
  const end = await glyphPoint(page, 0, 6);
  await page.mouse.move(beginning.x, beginning.y);
  await page.mouse.down();
  await page.mouse.move(end.x, end.y, { steps: 12 });
  await page.mouse.up();
  await expect.poll(() => page.evaluate(() => window.getSelection()?.toString())).toBe('Select');
  await expect(page.getByTestId('paged-editor').locator('[data-editor-selection="true"]').first()).toBeVisible();
  for (const name of ['Bold', 'Italic', 'Underline']) await page.getByRole('button', { name, exact: true }).click();
  await page.getByRole('combobox', { name: 'Font family', exact: true }).selectOption('Tinos');
  await page.getByRole('combobox', { name: 'Font size', exact: true }).selectOption('18');
  await page.getByRole('button', { name: 'Align center', exact: true }).click();
  await readyPages(page);
  await expect(page.getByTestId('paged-editor').locator('[data-layout-decoration="underline"]').first()).toBeVisible();
  await save(page).click();
  await expect(page.locator('.save-state')).toHaveText('Saved on this device');
  const file = (await records(page))[0];
  if (!file) throw new Error('Actual Writer save did not commit');
  const paragraph = file.file.document.sections[0]?.blocks[0]?.content;
  expect(paragraph?.runs.filter((run) => run.format.bold === true).map((run) => run.text).join('')).toBe('Select');
  expect(paragraph?.runs.find((run) => run.text === 'Select')?.format).toMatchObject({ bold: true, italic: true, underline: 'single', font_family: 'Tinos', font_size: 36 });
  expect(paragraph?.format.alignment).toBe('center');
  // A pointer Save preserves the previous writing focus. Do not hide the
  // first-key regression by refocusing or waiting for a deferred callback.
  await page.keyboard.press('Control+Home');
  await page.keyboard.press('Shift+ArrowRight');
  await page.keyboard.insertText('X');
  await expect(input(page)).toHaveText('Xelect these words in Writer.');
  await page.keyboard.press('Control+z');
  await expect(input(page)).toHaveText('Select these words in Writer.');
});

test('continuous pages, current-page status and zoom preserve the Rust layout on desktop and phone', async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 900 });
  await start(page);
  await importReference(page);
  const pages = page.getByTestId('paged-editor').locator('svg[data-layout-page]');
  await expect(pages).toHaveCount(4);
  await expect(page.locator('.page-status')).toHaveText(/Page \d of 4/);
  const originalViewBox = await pages.first().getAttribute('viewBox');
  await pages.last().scrollIntoViewIfNeeded();
  await expect(page.locator('.page-status')).toHaveText('Page 4 of 4');
  const width = (await pages.first().boundingBox())?.width;
  await page.getByRole('combobox', { name: 'Zoom', exact: true }).selectOption('150');
  await expect.poll(async () => (await pages.first().boundingBox())?.width ?? 0).toBeGreaterThan(width ?? 0);
  await expect(pages.first()).toHaveAttribute('viewBox', originalViewBox ?? '');
  await page.setViewportSize({ width: 420, height: 844 });
  await page.getByRole('combobox', { name: 'Zoom', exact: true }).selectOption('fit');
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await expect(pages).toHaveCount(4);
  await expect(page.locator('.page-status')).toHaveText(/Page \d of 4/);
});

test('Rust find, named styles and delayed title typing retain focus and source selections', async ({ page }) => {
  await start(page);
  await importReference(page);
  await title(page).fill('');
  await title(page).pressSequentially('A renamed reference', { delay: 70 });
  await expect(title(page)).toHaveValue('A renamed reference');
  await expect(title(page)).toBeFocused();
  await page.getByRole('button', { name: 'Find', exact: true }).click();
  const find = page.getByRole('textbox', { name: 'Find text', exact: true });
  await find.pressSequentially('mixed', { delay: 80 });
  await expect(find).toHaveValue('mixed');
  await expect(find).toBeFocused();
  await page.getByRole('button', { name: 'Next match', exact: true }).click();
  await readyPages(page);
  await expect(page.getByTestId('paged-editor').locator('[data-editor-selection="true"]').first()).toBeVisible();
  await page.getByRole('combobox', { name: 'Paragraph style', exact: true }).focus();
  await page.getByRole('combobox', { name: 'Paragraph style', exact: true }).selectOption({ label: 'Heading' });
  await expect(page.getByRole('combobox', { name: 'Paragraph style', exact: true })).toBeFocused();
  await save(page).click();
  await expect(page.locator('.save-state')).toHaveText('Saved on this device');
  const stored = (await records(page))[0];
  const formatted = stored?.file.document.sections[0]?.blocks.find((block) => block.content.id === 'layout-reference-unicode')?.content;
  expect(formatted?.style).toBe('layout-heading-style');
  expect(formatted?.runs.find((run) => run.text === 'mixed')?.format).toMatchObject({ italic: true, underline: 'double' });
});

test('autosave commits native bytes and exact DOCX originals, then reopens after a reload', async ({ page }) => {
  await start(page);
  const source = await readFile(new URL('../../../tests/fixtures/docx/supported-basic.docx', import.meta.url));
  await page.getByLabel('Import Writer file', { exact: true }).setInputFiles({
    name: 'autosave.docx', mimeType: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document', buffer: source,
  });
  await expect(input(page)).toContainText('DOCX hello');
  await focusEditor(page);
  await page.keyboard.press('Control+a');
  await page.keyboard.insertText('Autosaved e\u0301 👩🏽‍💻');
  await title(page).fill('Autosave recovery');
  await expect(page.locator('.save-state')).toHaveText('Saved on this device', { timeout: 10000 });
  const saved = (await records(page))[0];
  if (!saved) throw new Error('Autosave did not commit a record');
  expect(text(saved)).toBe('Autosaved e\u0301 👩🏽‍💻');
  expect(Buffer.from(saved.file.source?.bytes_base64 ?? '', 'base64')).toEqual(source);
  await page.reload();
  await expect(save(page)).toBeEnabled();
  await page.getByRole('button', { name: 'Open', exact: true }).click();
  await page.getByRole('button', { name: /Autosave recovery/ }).click();
  await expect(input(page)).toHaveText('Autosaved e\u0301 👩🏽‍💻');
  const pending = page.waitForEvent('download');
  await documentAction(page, 'Download original');
  const path = await (await pending).path();
  if (!path) throw new Error('No actual original download');
  expect(await readFile(path)).toEqual(source);
});

test('autosave quota failure pauses retries, preserves work and allows an explicit retry', async ({ page }) => {
  await start(page);
  await page.evaluate(() => {
    const current = IDBObjectStore.prototype.put;
    const state = window as Window & { failedAutosaveCalls?: number; restoreAutosavePut?: () => void };
    state.failedAutosaveCalls = 0;
    state.restoreAutosavePut = () => { IDBObjectStore.prototype.put = current; };
    IDBObjectStore.prototype.put = function (): IDBRequest<IDBValidKey> {
      state.failedAutosaveCalls = (state.failedAutosaveCalls ?? 0) + 1;
      throw new DOMException('Actual autosave quota regression', 'QuotaExceededError');
    };
  });
  await focusEditor(page);
  await page.keyboard.insertText('Preserved unsaved text');
  await expect(page.locator('.save-state')).toHaveText('Unsaved · autosave paused', { timeout: 10000 });
  await expect(page.getByRole('alert')).toContainText(/storage is full/i);
  await expect(input(page)).toHaveText('Preserved unsaved text');
  const attempts = await page.evaluate(() => (window as Window & { failedAutosaveCalls?: number }).failedAutosaveCalls);
  await page.keyboard.insertText(' and more');
  await expect(input(page)).toHaveText('Preserved unsaved text and more');
  await page.waitForTimeout(1200);
  expect(await page.evaluate(() => (window as Window & { failedAutosaveCalls?: number }).failedAutosaveCalls)).toBe(attempts);
  expect(await records(page)).toHaveLength(0);
  await page.evaluate(() => (window as Window & { restoreAutosavePut?: () => void }).restoreAutosavePut?.());
  await page.getByRole('button', { name: 'Retry save', exact: true }).click();
  await expect(page.locator('.save-state')).toHaveText('Saved on this device');
  expect(text((await records(page))[0] as NativeRecord)).toBe('Preserved unsaved text and more');
});

test('autosave conflicts preserve both tab versions and save a separate native copy', async ({ page, context }) => {
  await start(page);
  await title(page).fill('Autosave conflict');
  await focusEditor(page);
  await page.keyboard.insertText('Original');
  await expect(page.locator('.save-state')).toHaveText('Saved on this device', { timeout: 10000 });
  const second = await context.newPage();
  await start(second);
  await second.getByRole('button', { name: 'Open', exact: true }).click();
  await second.getByRole('button', { name: /Autosave conflict/ }).click();
  await expect(input(second)).toHaveText('Original');
  await focusEditor(page);
  await page.keyboard.press('Control+End');
  await page.keyboard.insertText(' from first tab');
  await expect(page.locator('.save-state')).toHaveText('Saved on this device', { timeout: 10000 });
  await focusEditor(second);
  await second.keyboard.press('Control+End');
  await second.keyboard.insertText(' from second tab');
  await expect(second.locator('.save-state')).toHaveText('Unsaved · autosave paused', { timeout: 10000 });
  await expect(second.getByRole('alert')).toContainText(/Another saved version/i);
  await expect(input(second)).toHaveText('Original from second tab');
  const before = await records(second);
  expect(before).toHaveLength(1);
  expect(text(before[0] as NativeRecord)).toBe('Original from first tab');
  await second.getByRole('button', { name: 'Save a separate copy', exact: true }).click();
  await expect(second.locator('.save-state')).toHaveText('Saved on this device');
  const after = await records(second);
  expect(after).toHaveLength(2);
  expect(new Set(after.map((record) => record.id)).size).toBe(2);
  expect(after.map(text).sort()).toEqual(['Original from first tab', 'Original from second tab'].sort());
});

test('manual Save shares the autosave lane and serializes the latest revision after the held transaction', async ({ page }) => {
  await start(page);
  await holdNextSave(page);
  await title(page).fill('Shared save lane');
  await focusEditor(page);
  await page.keyboard.insertText('Captured');
  await saveIsHeld(page);
  await expect(page.locator('.save-state')).toContainText('Saving');
  await page.keyboard.insertText(' plus newer');
  await save(page).click();
  await focusEditor(page);
  await page.keyboard.insertText(' and latest');
  await expect(input(page)).toHaveText('Captured plus newer and latest');
  await releaseSave(page);
  await expect(page.locator('.save-state')).toHaveText('Saved on this device', { timeout: 10000 });
  const saved = await records(page);
  expect(saved).toHaveLength(1);
  expect(text(saved[0] as NativeRecord)).toBe('Captured plus newer and latest');
  const snapshots = await page.evaluate(() => (window as Window & { writerSavedSnapshots?: string[] }).writerSavedSnapshots ?? []);
  expect(snapshots.length).toBeGreaterThanOrEqual(2);
  const first = JSON.parse(snapshots[0] ?? '{}') as NativeRecord['file'];
  expect(text({ id: '', etag: '', file: first })).toBe('Captured');
  const last = JSON.parse(snapshots.at(-1) ?? '{}') as NativeRecord['file'];
  expect(text({ id: '', etag: '', file: last })).toBe('Captured plus newer and latest');
  await expect(page.getByRole('alert')).toHaveCount(0);
});

test('an old autosave completion cannot replace or mark a new document saved after discard', async ({ page }) => {
  await start(page);
  await holdNextSave(page);
  await title(page).fill('Old in-flight document');
  await focusEditor(page);
  await page.keyboard.insertText('Old captured revision');
  await saveIsHeld(page);
  await page.getByRole('button', { name: 'New', exact: true }).click();
  const guard = page.getByRole('dialog', { name: 'Save your changes?', exact: true });
  await guard.getByRole('button', { name: 'Discard changes', exact: true }).click();
  await expect(input(page)).toHaveText('');
  await title(page).fill('New independent document');
  await focusEditor(page);
  await page.keyboard.insertText('New unsaved e\u0301 👩🏽‍💻');
  await expect(page.locator('.save-state')).toContainText('Unsaved');
  await releaseSave(page);
  await expect.poll(async () => (await records(page)).length, { timeout: 10000 }).toBe(2);
  await expect(page.locator('.save-state')).toHaveText('Saved on this device');
  const saved = await records(page);
  expect(new Set(saved.map((record) => record.id)).size).toBe(2);
  expect(saved.find((record) => record.file.document.metadata.title === 'Old in-flight document')).toBeDefined();
  expect(text(saved.find((record) => record.file.document.metadata.title === 'New independent document') as NativeRecord)).toBe('New unsaved e\u0301 👩🏽‍💻');
  await expect(title(page)).toHaveValue('New independent document');
  await expect(input(page)).toHaveText('New unsaved e\u0301 👩🏽‍💻');
});

test('continuous typing is saved within the maximum wait before the user stops typing', async ({ page }) => {
  await start(page);
  await focusEditor(page);
  const typing = input(page).pressSequentially('x'.repeat(70), { delay: 100 });
  await expect.poll(async () => (await records(page)).length, { timeout: 10000 }).toBe(1);
  const captured = (await records(page))[0];
  expect(text(captured as NativeRecord).length).toBeGreaterThan(0);
  expect(text(captured as NativeRecord).length).toBeLessThan(70);
  await typing;
  await expect(page.locator('.save-state')).toHaveText('Saved on this device', { timeout: 10000 });
  expect(text((await records(page))[0] as NativeRecord)).toBe('x'.repeat(70));
});

test('a structurally empty native file accepts a real page click and one reversible Rust edit', async ({ page }) => {
  await start(page);
  await page.getByLabel('Import Writer file', { exact: true }).setInputFiles({ name: 'empty.writer', mimeType: 'application/json',
    buffer: Buffer.from(JSON.stringify({ format: 'office-writer', format_version: 2, document: {
      schema_version: 2, id: 'step7-empty', metadata: { title: 'Empty native structure' }, sections: [], styles: [],
    } })),
  });
  await expect(title(page)).toHaveValue('Empty native structure');
  await readyPages(page);
  await clickPaper(page);
  await page.keyboard.insertText('First e\u0301 👩🏽‍💻');
  await expect(input(page)).toHaveText('First e\u0301 👩🏽‍💻');
  await readyPages(page);
  await expect(page.getByTestId('paged-editor').locator('svg use').first()).toBeVisible();
  await page.keyboard.press('Control+z');
  await expect(input(page)).toHaveText('');
  await save(page).click();
  await expect(page.locator('.save-state')).toHaveText('Saved on this device');
  const saved = (await records(page))[0];
  expect(saved?.file.document.id).toBe('step7-empty');
  expect(saved?.file.document.sections).toEqual([]);
  await expect(page.getByRole('alert')).toHaveCount(0);
});

test('missing-font text view is an explicit choice and keeps native content while page count is unknown', async ({ page }) => {
  await start(page);
  await focusEditor(page);
  await page.keyboard.insertText('日本語漢字');
  await readyPages(page);
  await expect(page.getByRole('button', { name: 'Use text view', exact: true })).toBeVisible();
  await expect(page.getByTestId('paged-editor').locator('svg[data-layout-page]')).toHaveCount(1);
  await page.getByRole('button', { name: 'Use text view', exact: true }).click();
  await expect(input(page)).toBeVisible();
  await expect(input(page)).toBeFocused();
  await expect(input(page)).toHaveText('日本語漢字');
  await expect(page.locator('.page-status')).toHaveText('Pages unavailable');
  await page.keyboard.press('Control+End');
  await page.keyboard.insertText(' — preserved');
  await save(page).click();
  await expect(page.locator('.save-state')).toHaveText('Saved on this device');
  expect(text((await records(page))[0] as NativeRecord)).toBe('日本語漢字 — preserved');
});

test('fixed zoom scrolls the Rust caret horizontally into a narrow editing viewport', async ({ page }) => {
  await page.setViewportSize({ width: 420, height: 844 });
  await start(page);
  await page.getByRole('combobox', { name: 'Zoom', exact: true }).selectOption('100');
  await focusEditor(page);
  await page.keyboard.insertText('W'.repeat(30));
  await readyPages(page);
  const workspace = page.getByRole('main', { name: 'Document workspace', exact: true });
  await expect.poll(() => workspace.evaluate((element) => element.scrollLeft)).toBeGreaterThan(0);
  await expect.poll(async () => {
    const viewport = await workspace.boundingBox();
    const caret = await page.getByTestId('paged-editor').locator('[data-editor-caret="true"]').boundingBox();
    return !!viewport && !!caret && caret.x >= viewport.x
      && caret.x + caret.width <= viewport.x + viewport.width;
  }).toBe(true);
  await expect(input(page)).toHaveText('W'.repeat(30));
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
});

test('Find reveals an offscreen Rust selection while keeping the query field focused', async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 720 });
  await start(page);
  await importReference(page);
  await page.getByRole('button', { name: 'Find', exact: true }).click();
  const query = page.getByRole('textbox', { name: 'Find text', exact: true });
  await query.fill('A4 section');
  await query.press('Enter');
  await expect(query).toBeFocused();
  await expect(page.locator('.page-status')).toHaveText('Page 4 of 4');
  await expect(page.getByTestId('paged-editor').locator('svg[data-layout-page="4"] [data-editor-selection="true"]').first()).toBeInViewport();
  await expect(query).toHaveValue('A4 section');
});

test('reading-view page status follows its own scroll position and Find returns to editing', async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 720 });
  await start(page);
  await importReference(page);
  await documentAction(page, 'Page preview');
  const reading = page.getByRole('region', { name: 'Document page preview', exact: true });
  await expect(reading).toHaveAttribute('aria-busy', 'false');
  await reading.locator('[data-layout-page="3"]').scrollIntoViewIfNeeded();
  await expect(page.locator('.page-status')).toHaveText('Page 3 of 4');
  await page.setViewportSize({ width: 1100, height: 760 });
  await expect(page.locator('.page-status')).toHaveText('Page 3 of 4');
  await page.keyboard.press('Control+f');
  // Type immediately: entering Find from reading view must focus its input in
  // the same visibility commit, without losing the first character.
  await page.keyboard.type('A4 section');
  await expect(reading).toHaveCount(0);
  const query = page.getByRole('textbox', { name: 'Find text', exact: true });
  await expect(query).toBeFocused();
  await expect(query).toHaveValue('A4 section');
  await readyPages(page);
});

test('glyph render reuse still refreshes paths, sizes, underlines, alignment and undo', async ({ page }) => {
  await start(page);
  await focusEditor(page);
  await page.keyboard.insertText('A');
  const glyph = async () => {
    const root = await readyPages(page);
    return root.locator('svg[data-layout-page] use').first().evaluate((element) => {
      const reference = element.getAttribute('href') ?? '';
      const outline = document.getElementById(reference.slice(1));
      return { outline: element.getAttribute('data-layout-outline-id'), path: outline?.getAttribute('d'),
        transform: element.getAttribute('transform') };
    });
  };
  const regular = await glyph();
  await page.keyboard.press('Control+a');
  await page.getByRole('button', { name: 'Bold', exact: true }).click();
  await expect.poll(async () => (await glyph()).path).not.toBe(regular.path);
  const bold = await glyph();
  expect(bold.outline).toBe(regular.outline);
  await page.keyboard.press('Control+z');
  await expect.poll(async () => (await glyph()).path).toBe(regular.path);
  await page.getByRole('combobox', { name: 'Font family', exact: true }).selectOption('Tinos');
  await expect.poll(async () => (await glyph()).path).not.toBe(regular.path);
  const serif = await glyph();
  await page.getByRole('combobox', { name: 'Font size', exact: true }).selectOption('24');
  await expect.poll(async () => (await glyph()).transform).not.toBe(serif.transform);
  const large = await glyph();
  await page.getByRole('button', { name: 'Underline', exact: true }).click();
  await expect(page.getByTestId('paged-editor').locator('[data-layout-decoration="underline"]').first()).toBeVisible();
  await page.getByRole('button', { name: 'Align right', exact: true }).click();
  await expect.poll(async () => (await glyph()).transform).not.toBe(large.transform);
  await page.keyboard.press('Control+z');
  await expect.poll(async () => (await glyph()).transform).toBe(large.transform);
  await page.keyboard.press('Control+z');
  await expect(page.getByTestId('paged-editor').locator('[data-layout-decoration="underline"]')).toHaveCount(0);
});
