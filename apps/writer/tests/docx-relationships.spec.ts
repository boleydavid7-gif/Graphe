import { readFile } from 'node:fs/promises';
import { test, expect, type Download, type Page } from '@playwright/test';
import { documentAction, focusEditor, readyPages } from './step7-test-helpers';

const directory = new URL('../../../tests/fixtures/docx/relationships/', import.meta.url);
const media = 'application/vnd.openxmlformats-officedocument.wordprocessingml.document';
const updated = 'Edited relationship document — Café e\u0301 👩🏽‍💻';

interface NativeFile {
  readonly document: {
    readonly id: string;
    readonly metadata: { readonly title: string };
    readonly sections: readonly {
      readonly blocks: readonly { readonly content: { readonly runs: readonly { readonly text: string }[] } }[];
    }[];
  };
  readonly source: { readonly format: string; readonly filename: string; readonly bytes_base64: string };
}

async function content(download: Download) {
  const path = await download.path();
  if (!path) throw new Error('The browser did not retain the downloaded document');
  return readFile(path);
}

async function openFile(page: Page, bytes: Buffer, filename: string) {
  await page.getByRole('button', { name: 'Open', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Open document', exact: true });
  const choosing = page.waitForEvent('filechooser');
  await dialog.getByRole('button', { name: 'Open file…', exact: true }).click();
  await (await choosing).setFiles({ name: filename, mimeType: media, buffer: bytes });
}

async function saveAs(page: Page, format: 'writer' | 'docx') {
  await documentAction(page, 'Save As…');
  const dialog = page.getByRole('dialog', { name: 'Save As', exact: true });
  await dialog.getByRole('combobox', { name: 'File format', exact: true }).selectOption(format);
  const acknowledgement = dialog.getByRole('checkbox', { name: 'I understand the format limitations', exact: true });
  await expect(acknowledgement).toBeVisible();
  await expect(dialog.getByRole('button', { name: 'Download copy', exact: true })).toBeDisabled();
  if (format === 'docx') await expect(dialog).toContainText('metadata and auxiliary package parts');
  await acknowledgement.check();
  const downloading = page.waitForEvent('download');
  await dialog.getByRole('button', { name: 'Download copy', exact: true }).click();
  const download = await downloading;
  expect(download.suggestedFilename()).toMatch(new RegExp(`\\.${format}$`, 'i'));
  await expect(dialog).toHaveCount(0);
  return content(download);
}

const input = (page: Page) => page.getByRole('textbox', { name: 'Document content', exact: true });

test('harmless DOCX auxiliaries open through the real UI, preserve originals locally and export current edited content', async ({ page }) => {
  const original = await readFile(new URL('supported-strict-authoring-metadata.docx', directory));
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Save', exact: true })).toBeEnabled();
  await openFile(page, original, '元の文書.docx');
  await expect(page.getByRole('dialog', { name: 'Open document', exact: true })).toHaveCount(0);
  await expect(page.getByRole('dialog', { name: 'DOCX preserved read-only', exact: true })).toHaveCount(0);
  await expect(page.locator('.save-state')).toHaveText('Saved on this device');
  await readyPages(page);
  const initial = JSON.parse((await saveAs(page, 'writer')).toString('utf8')) as NativeFile;
  expect(initial.source.format).toBe('docx');
  expect(initial.source.filename).toBe('元の文書.docx');
  expect(Buffer.from(initial.source.bytes_base64, 'base64')).toEqual(original);

  await focusEditor(page);
  await page.keyboard.press('Control+Home');
  await page.keyboard.press('Control+Shift+End');
  await page.keyboard.insertText(updated);
  await page.keyboard.press('Control+Home');
  await page.keyboard.press('Control+Shift+End');
  await page.getByRole('button', { name: 'Bold', exact: true }).click();
  await expect(input(page)).toHaveText(updated);
  await expect(page.getByRole('button', { name: 'Bold', exact: true })).toHaveAttribute('aria-pressed', 'true');
  await page.getByRole('textbox', { name: 'Document title', exact: true }).fill('Auxiliary relationship recovery');
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(page.locator('.save-state')).toHaveText('Saved on this device');
  const persisted = JSON.parse((await saveAs(page, 'writer')).toString('utf8')) as NativeFile;
  expect(persisted.document.id).toBe(initial.document.id);
  expect(persisted.document.sections.flatMap((section) => section.blocks)
    .flatMap((block) => block.content.runs).map((run) => run.text).join('')).toBe(updated);
  expect(Buffer.from(persisted.source.bytes_base64, 'base64')).toEqual(original);

  await page.reload();
  await expect(page.getByRole('button', { name: 'Save', exact: true })).toBeEnabled();
  await page.getByRole('button', { name: 'Open', exact: true }).click();
  await page.getByRole('button', { name: /Auxiliary relationship recovery/ }).click();
  await expect(input(page)).toHaveText(updated);
  await readyPages(page);
  expect(JSON.parse((await saveAs(page, 'writer')).toString('utf8'))).toEqual(persisted);
  const copy = await saveAs(page, 'docx');
  expect(copy).not.toEqual(original);
  await openFile(page, copy, 'edited-copy.docx');
  await expect(page.getByRole('dialog', { name: 'Open document', exact: true })).toHaveCount(0);
  await expect(input(page)).toHaveText(updated);
  await expect(page.locator('.save-state')).toHaveText('Saved on this device');
  const reopened = JSON.parse((await saveAs(page, 'writer')).toString('utf8')) as NativeFile;
  expect(Buffer.from(reopened.source.bytes_base64, 'base64')).toEqual(copy);
});

test('unsafe image and external relationships remain read-only with exact recovery and intact previous work', async ({ page }) => {
  const unexpected: string[] = [];
  await page.route('**/*', async (route) => {
    const url = new URL(route.request().url());
    if (['http:', 'https:'].includes(url.protocol) && !['127.0.0.1', 'localhost'].includes(url.hostname)) {
      unexpected.push(url.href);
      await route.abort('blockedbyclient');
    } else await route.continue();
  });
  const original = await readFile(new URL('supported-auxiliary-parts.docx', directory));
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Save', exact: true })).toBeEnabled();
  await openFile(page, original, 'supported.docx');
  await expect(page.getByRole('dialog', { name: 'Open document', exact: true })).toHaveCount(0);
  const initial = await input(page).textContent();
  await focusEditor(page);
  await page.keyboard.press('Control+End');
  await page.keyboard.insertText(' — retain these changes');
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(page.locator('.save-state')).toHaveText('Saved on this device');
  const before = await saveAs(page, 'writer');
  const current = await input(page).textContent();
  for (const name of ['unsupported-image.docx', 'unsupported-external-relationship.docx']) {
    const bytes = await readFile(new URL(name, directory));
    await openFile(page, bytes, name);
    const preserved = page.getByRole('dialog', { name: 'DOCX preserved read-only', exact: true });
    await expect(preserved).toBeVisible();
    await expect(preserved).toContainText('Your current document is unchanged');
    expect(await preserved.locator('.compatibility-issues li').count()).toBeGreaterThan(0);
    const downloading = page.waitForEvent('download');
    await preserved.getByRole('button', { name: 'Download original DOCX', exact: true }).click();
    expect(await content(await downloading)).toEqual(bytes);
    await preserved.getByRole('button', { name: 'Close', exact: true }).click();
    await expect(input(page)).toHaveText(current ?? '');
    expect(await saveAs(page, 'writer')).toEqual(before);
    await expect(page.locator('.save-state')).toHaveText('Saved on this device');
  }
  await focusEditor(page);
  await page.keyboard.press('Control+z');
  await expect(input(page)).toHaveText(initial ?? '');
  expect(unexpected).toEqual([]);
});
