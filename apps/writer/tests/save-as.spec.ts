import { readFile } from 'node:fs/promises';
import { expect, test, type Download, type Locator, type Page } from '@playwright/test';
import { focusEditor, readyPages } from './step7-test-helpers';

const plain = 'Ordinary writing — Café e\u0301 👩🏽‍💻';
const editableFormats = ['writer', 'docx', 'odt', 'rtf', 'txt'] as const;
type Format = typeof editableFormats[number];
const input = (page: Page) => page.getByRole('textbox', { name: 'Document content', exact: true });
const dialog = (page: Page) => page.getByRole('dialog', { name: 'Save As', exact: true });
const warning = (page: Page) => dialog(page).getByRole('note', { name: 'Format warning', exact: true });

async function activate(control: Locator, touch: boolean) {
  if (touch) await control.tap(); else await control.click();
}

async function start(page: Page) {
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Save', exact: true })).toBeEnabled();
  await readyPages(page);
}

async function review(page: Page, format: Format, touch: boolean) {
  const menu = page.locator('details.document-menu');
  if (!await menu.evaluate((element) => element.hasAttribute('open'))) await activate(menu.locator('summary'), touch);
  await activate(menu.getByRole('button', { name: 'Save As…', exact: true }), touch);
  await dialog(page).getByRole('combobox', { name: 'File format', exact: true }).selectOption(format);
  await expect(dialog(page).getByRole('checkbox')).toHaveCount(0);
  await expect(dialog(page).getByRole('button', { name: 'Save copy', exact: true })).toBeEnabled();
}

async function downloadBytes(download: Download) {
  const path = await download.path();
  if (!path) throw new Error('The browser did not retain the copied file');
  return readFile(path);
}

async function saveCopy(page: Page, touch: boolean) {
  const pending = page.waitForEvent('download');
  await activate(dialog(page).getByRole('button', { name: 'Save copy', exact: true }), touch);
  const result = await pending;
  await expect(dialog(page)).toHaveCount(0);
  return downloadBytes(result);
}

async function nativeCopy(page: Page, touch: boolean) {
  await review(page, 'writer', touch);
  return saveCopy(page, touch);
}

async function localRecords(page: Page) {
  return page.evaluate(async () => {
    const database = await new Promise<IDBDatabase>((resolve, reject) => {
      const opening = indexedDB.open('writer-documents', 1);
      opening.onsuccess = () => resolve(opening.result);
      opening.onerror = () => reject(opening.error);
    });
    try {
      return await new Promise<{ id: string; etag: string; bytes: number[] }[]>((resolve, reject) => {
        const transaction = database.transaction('documents', 'readonly');
        const request = transaction.objectStore('documents').getAll();
        let records: { id: string; etag: string; bytes: number[] }[] = [];
        request.onsuccess = () => {
          records = (request.result as { id: string; etag: string; bytes: Uint8Array }[])
            .map((record) => ({ id: record.id, etag: record.etag, bytes: [...record.bytes] }));
        };
        transaction.oncomplete = () => resolve(records);
        transaction.onerror = () => reject(transaction.error);
        transaction.onabort = () => reject(transaction.error);
      });
    } finally { database.close(); }
  });
}

for (const touch of [false, true]) {
  test.describe(touch ? 'compact Save As on touch' : 'compact Save As on desktop', () => {
    if (touch) test.use({ viewport: { width: 390, height: 844 }, hasTouch: true });

    test('ordinary supported copies need only filename, format and one Save copy', async ({ page }) => {
      const errors: string[] = [];
      page.on('pageerror', (error) => errors.push(error.message));
      await start(page);
      await focusEditor(page);
      await page.keyboard.insertText(plain);
      await activate(page.getByRole('button', { name: 'Save', exact: true }), touch);
      await expect(page.locator('.save-state')).toHaveText('Saved on this device');
      const stored = await localRecords(page);
      const before = await nativeCopy(page, touch);
      for (const format of editableFormats) {
        await review(page, format, touch);
        await expect(warning(page)).toHaveCount(0);
        await expect(dialog(page).locator('ul')).toHaveCount(0);
        await expect(dialog(page)).not.toContainText(/native Writer identifiers|acknowledge|auxiliary package parts/i);
        const filename = await dialog(page).getByRole('textbox', { name: 'Filename', exact: true }).boundingBox();
        const formatBox = await dialog(page).getByRole('combobox', { name: 'File format', exact: true }).boundingBox();
        const bounds = await dialog(page).boundingBox();
        if (!filename || !formatBox || !bounds) throw new Error('The compact Save As fields were not displayed');
        expect(filename.y).toBeLessThan(formatBox.y);
        expect(bounds.x).toBeGreaterThanOrEqual(0);
        expect(bounds.y).toBeGreaterThanOrEqual(0);
        expect(bounds.x + bounds.width).toBeLessThanOrEqual(page.viewportSize()!.width);
        expect(bounds.y + bounds.height).toBeLessThanOrEqual(page.viewportSize()!.height);
        expect(bounds.height).toBeLessThan(400);
        // Decorative entrance transforms must not override modal centering;
        // the same conflict can push Save copy below an overlay keyboard.
        expect(Math.abs(bounds.y + bounds.height / 2 - page.viewportSize()!.height / 2)).toBeLessThan(2);
        if (touch) {
          expect(await dialog(page).getByRole('textbox', { name: 'Filename', exact: true })
            .evaluate((element) => parseFloat(getComputedStyle(element).fontSize))).toBeGreaterThanOrEqual(16);
        }
        if (format === 'writer') await page.screenshot({ path: `../../docs/images/writer-save-as-${touch ? 'mobile' : 'desktop'}.png` });
        expect((await saveCopy(page, touch)).length).toBeGreaterThan(0);
        await expect(input(page)).toHaveText(plain);
        await expect(page.locator('.save-state')).toHaveText('Saved on this device');
      }
      expect(await nativeCopy(page, touch)).toEqual(before);
      expect(await localRecords(page)).toEqual(stored);
      expect(errors).toEqual([]);
    });

    test('meaningful text-format loss has one brief warning and never changes unsaved native work', async ({ page }) => {
      // Keep the explicit-save/copy distinction observable on slow runners;
      // only autosave timers wait, while Rust and actual IndexedDB stay active.
      await page.addInitScript(() => {
        const schedule = window.setTimeout.bind(window);
        window.setTimeout = ((handler: TimerHandler, delay?: number, ...args: unknown[]) => schedule(handler,
          delay === 750 || delay === 5000 ? 60_000 : delay, ...args)) as typeof window.setTimeout;
      });
      await start(page);
      await focusEditor(page);
      await page.keyboard.insertText(plain);
      await activate(page.getByRole('button', { name: 'Save', exact: true }), touch);
      await expect(page.locator('.save-state')).toHaveText('Saved on this device');
      const stored = await localRecords(page);
      await focusEditor(page);
      await page.keyboard.press('Control+Home');
      await page.keyboard.press('Control+Shift+End');
      await activate(page.getByRole('button', { name: 'Bold', exact: true }), touch);
      const before = await nativeCopy(page, touch);
      await review(page, 'txt', touch);
      await expect(warning(page)).toContainText(/format|bold|text/i);
      expect((await warning(page).innerText()).length).toBeLessThan(300);
      await expect(warning(page).locator('ul')).toHaveCount(0);
      await dialog(page).getByRole('combobox', { name: 'File format', exact: true }).selectOption('rtf');
      await expect(warning(page)).toHaveCount(0);
      await dialog(page).getByRole('combobox', { name: 'File format', exact: true }).selectOption('txt');
      await expect(warning(page)).toBeVisible();
      await expect(dialog(page).getByRole('checkbox')).toHaveCount(0);
      expect((await saveCopy(page, touch)).toString('utf8')).toBe(plain);
      await expect(page.locator('.save-state')).toContainText('Unsaved');
      expect(await nativeCopy(page, touch)).toEqual(before);
      expect(await localRecords(page)).toEqual(stored);
      await focusEditor(page);
      await page.keyboard.press('Control+z');
      await expect(page.getByRole('button', { name: 'Bold', exact: true })).toHaveAttribute('aria-pressed', 'false');
      await expect(input(page)).toHaveText(plain);
    });

    test('a native source-bearing copy uses a compact preservation hint and retains its exact original', async ({ page }) => {
      await start(page);
      const source = await readFile(new URL('../../../tests/fixtures/docx/relationships/supported-auxiliary-parts.docx', import.meta.url));
      const menu = page.locator('details.document-menu');
      await activate(menu.locator('summary'), touch);
      await activate(menu.getByRole('button', { name: 'Open document', exact: true }), touch);
      const opening = page.getByRole('dialog', { name: 'Open document', exact: true });
      const choosing = page.waitForEvent('filechooser');
      await activate(opening.getByRole('button', { name: 'Open file…', exact: true }), touch);
      await (await choosing).setFiles({ name: 'Original source.docx', buffer: source,
        mimeType: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document' });
      await expect(opening).toHaveCount(0);
      const stored = await localRecords(page);
      await review(page, 'writer', touch);
      await expect(warning(page)).toHaveCount(0);
      await expect(dialog(page)).toContainText(/original/i);
      const first = await saveCopy(page, touch);
      const file = JSON.parse(first.toString('utf8')) as { source: { bytes_base64: string } };
      expect(Buffer.from(file.source.bytes_base64, 'base64')).toEqual(source);
      expect(await nativeCopy(page, touch)).toEqual(first);
      expect(await localRecords(page)).toEqual(stored);
    });

    test('filename Enter submits one copy above an overlay keyboard and restores the next editor keystroke', async ({ page }) => {
      await page.addInitScript(() => {
        const viewport = Object.assign(new EventTarget(), { offsetTop: 0, offsetLeft: 0,
          width: window.innerWidth, height: window.innerHeight, scale: 1 });
        (window as Window & { saveCopyViewport?: typeof viewport }).saveCopyViewport = viewport;
        Object.defineProperty(window, 'visualViewport', { configurable: true, get: () => viewport });
      });
      await start(page);
      await focusEditor(page);
      await page.keyboard.insertText(plain);
      await activate(page.getByRole('button', { name: 'Save', exact: true }), touch);
      await expect(page.locator('.save-state')).toHaveText('Saved on this device');
      const stored = await localRecords(page);
      await focusEditor(page);
      await page.keyboard.press('Control+End');
      await page.keyboard.press('Control+Shift+s');
      await dialog(page).getByRole('textbox', { name: 'Filename', exact: true }).fill('Keyboard copy.writer');
      await page.evaluate(() => {
        const viewport = (window as Window & { saveCopyViewport?: EventTarget & { offsetTop: number; height: number } }).saveCopyViewport!;
        Object.assign(viewport, { offsetTop: 80, height: 380 });
        viewport.dispatchEvent(new Event('resize'));
        viewport.dispatchEvent(new Event('scroll'));
      });
      await expect.poll(async () => {
        const box = await dialog(page).boundingBox();
        const button = await dialog(page).getByRole('button', { name: 'Save copy', exact: true }).boundingBox();
        return !!box && !!button && box.y >= 80 && box.y + box.height <= 460
          && button.y >= 80 && button.y + button.height <= 460;
      }).toBe(true);
      const pending = page.waitForEvent('download');
      await page.keyboard.press('Enter');
      const downloaded = await pending;
      expect(downloaded.suggestedFilename()).toBe('Keyboard copy.writer');
      const copied = await downloadBytes(downloaded);
      await expect(dialog(page)).toHaveCount(0);
      await expect(input(page)).toBeFocused();
      expect(await localRecords(page)).toEqual(stored);
      await page.keyboard.insertText('!');
      await expect(input(page)).toHaveText(`${plain}!`);
      const file = JSON.parse(copied.toString('utf8')) as { document: { metadata: { title: string } } };
      expect(file.document.metadata.title).toBe('');
    });
  });
}
