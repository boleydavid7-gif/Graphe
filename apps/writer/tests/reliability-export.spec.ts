import { readFile } from 'node:fs/promises';
import { test, expect, type Download, type Page } from '@playwright/test';
import { documentAction, focusEditor, readyPages } from './step7-test-helpers';

const text = 'Reliable local copy — Café e\u0301 👩🏽‍💻';
const formats = ['writer', 'docx', 'odt', 'rtf', 'txt', 'pdf'] as const;
const media = { writer: 'application/json', docx: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
  odt: 'application/vnd.oasis.opendocument.text', rtf: 'application/rtf', txt: 'text/plain', pdf: 'application/pdf' };
async function bytes(download: Download) {
  const path = await download.path();
  if (!path) throw new Error('No downloaded file');
  return readFile(path);
}
async function begin(page: Page) {
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Save', exact: true })).toBeEnabled();
  await readyPages(page);
  await focusEditor(page);
  await page.keyboard.insertText(text);
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(page.locator('.save-state')).toHaveText('Saved on this device');
}
async function review(page: Page, format: typeof formats[number]) {
  await documentAction(page, 'Save As…');
  const dialog = page.getByRole('dialog', { name: 'Save As', exact: true });
  await dialog.getByRole('combobox', { name: 'File format', exact: true }).selectOption(format);
  await dialog.getByRole('textbox', { name: 'Filename', exact: true }).fill(`Reliable é 📝.${format}`);
  await expect(dialog.getByRole('checkbox')).toHaveCount(0);
  return dialog;
}

for (const touch of [false, true]) {
  test.describe(touch ? 'touch file workflows' : 'desktop file workflows', () => {
    test.use(touch ? { viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true } : {});
    for (const format of formats) {
      test(`${format} Save As and native retry links deliver exact files and preserve local saves`, async ({ page, browser }) => {
        await page.addInitScript(() => {
          const state = window as Window & { exportTypes?: Record<string, string>; releasedExports?: string[] };
          state.exportTypes = {};
          state.releasedExports = [];
          const create = URL.createObjectURL;
          URL.createObjectURL = function (blob) {
            const url = create.call(URL, blob);
            if (blob instanceof Blob) state.exportTypes![url] = blob.type;
            return url;
          };
          const revoke = URL.revokeObjectURL;
          URL.revokeObjectURL = function (url) {
            state.releasedExports!.push(url);
            revoke.call(URL, url);
          };
          const click = HTMLAnchorElement.prototype.click;
          HTMLAnchorElement.prototype.click = function () {
            if (this.download) (window as Window & { lastDeliveryConnected?: boolean }).lastDeliveryConnected = this.isConnected;
            return click.call(this);
          };
        });
        await begin(page);
        const dialog = await review(page, format);
        const pending = page.waitForEvent('download');
        const button = dialog.getByRole('button', { name: 'Save copy', exact: true });
        if (touch) await button.tap(); else await button.click();
        const file = await pending;
        expect(file.suggestedFilename()).toBe(`Reliable é 📝.${format}`);
        const original = await bytes(file);
        expect(original.length).toBeGreaterThan(0);
        expect(await page.evaluate(() => (window as Window & { lastDeliveryConnected?: boolean }).lastDeliveryConnected)).toBe(true);
        await expect(dialog).toHaveCount(0);
        const copy = page.getByRole('complementary', { name: 'Prepared export', exact: true });
        await expect(copy).toContainText('does not replace your local save');
        const retry = copy.getByRole('link', { name: 'Download file', exact: true });
        const url = await retry.getAttribute('href');
        expect(url).toMatch(/^blob:/);
        const contentType = await page.evaluate((url) => (window as Window & { exportTypes?: Record<string, string> }).exportTypes?.[url!], url);
        expect(contentType?.split(';')[0]).toBe(media[format]);
        await page.waitForTimeout(1200); // Reproduce the previous 1s URL revocation.
        const again = page.waitForEvent('download');
        if (touch) await retry.tap(); else await retry.click();
        expect(await bytes(await again)).toEqual(original);
        await expect(page.locator('.save-state')).toHaveText('Saved on this device');
        if (format === 'pdf') {
          expect(original.subarray(0, 5).toString()).toBe('%PDF-');
          expect(original.toString('latin1')).toContain('%%EOF');
          expect(original.toString('latin1')).toMatch(/\/Type\s*\/Page\b/);
        } else {
          const context = await browser.newContext({ acceptDownloads: true });
          const other = await context.newPage();
          try {
            await other.goto('/');
            await expect(other.getByRole('button', { name: 'Open', exact: true })).toBeEnabled();
            await other.getByRole('button', { name: 'Open', exact: true }).click();
            const chooser = other.waitForEvent('filechooser');
            await other.getByRole('button', { name: 'Open file…', exact: true }).click();
            await (await chooser).setFiles({ name: file.suggestedFilename(), mimeType: media[format], buffer: original });
            await expect(other.getByRole('dialog', { name: 'Open document', exact: true })).toHaveCount(0);
            await expect(other.getByRole('textbox', { name: 'Document content', exact: true })).toHaveText(text);
          } finally { await context.close(); }
        }
        await copy.getByRole('button', { name: 'Dismiss prepared export', exact: true }).click();
        const released = await page.evaluate((url) => (window as Window & { releasedExports?: string[] }).releasedExports?.includes(url!), url);
        expect(released).toBe(true);
      });
    }
  });
}

test('dialog shortcuts preserve the reviewed format and cannot stack modal workflows', async ({ page }) => {
  await begin(page);
  const dialog = await review(page, 'pdf');
  await page.keyboard.press('Control+Shift+s');
  await expect(dialog.getByRole('combobox', { name: 'File format', exact: true })).toHaveValue('pdf');
  await expect(dialog.getByRole('textbox', { name: 'Filename', exact: true })).toHaveValue('Reliable é 📝.pdf');
  await expect(dialog.getByRole('checkbox')).toHaveCount(0);
  await expect(dialog.getByRole('note', { name: 'Format warning', exact: true })).toContainText(/PDF|search|select/i);
  await page.keyboard.press('Control+o');
  await page.keyboard.press('Control+f');
  await page.keyboard.press('Control+s');
  await expect(page.locator('dialog[open]')).toHaveCount(1);
  await dialog.getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect(page.getByRole('textbox', { name: 'Document content', exact: true })).toHaveText(text);
});

test('a blocked automatic file handoff leaves a usable immutable retry without changing the document', async ({ page }) => {
  await begin(page);
  await page.evaluate(() => {
    HTMLAnchorElement.prototype.click = function () { throw new DOMException('Blocked automatic handoff', 'NotAllowedError'); };
  });
  const dialog = await review(page, 'txt');
  await dialog.getByRole('button', { name: 'Save copy', exact: true }).click();
  const copy = page.getByRole('complementary', { name: 'Prepared export', exact: true });
  await expect(copy).toContainText('could not start the automatic download');
  await expect(page.locator('.save-state')).toHaveText('Saved on this device');
  const pending = page.waitForEvent('download');
  await copy.getByRole('link', { name: 'Download file', exact: true }).click();
  expect((await bytes(await pending)).toString('utf8')).toBe(text);
});
