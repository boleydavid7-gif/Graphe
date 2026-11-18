import { readFile } from 'node:fs/promises';
import { expect, test, type Page } from '@playwright/test';
import { documentAction, focusEditor, readyPages } from './step7-test-helpers';

async function start(page: Page) {
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Save', exact: true })).toBeEnabled();
  await readyPages(page);
}

async function exportNative(page: Page) {
  await documentAction(page, 'Save As…');
  const dialog = page.getByRole('dialog', { name: 'Save As', exact: true });
  await dialog.getByRole('combobox', { name: 'File format', exact: true }).selectOption('writer');
  const pending = page.waitForEvent('download');
  await dialog.getByRole('button', { name: 'Save copy', exact: true }).click();
  const download = await pending;
  const path = await download.path();
  if (!path) throw new Error('No native file was downloaded');
  return JSON.parse(await readFile(path, 'utf8')) as {
    document: { sections: { properties: {
      page_size: { width: number; height: number; orientation: string };
      page_margins: { top: number; bottom: number };
    }; blocks: { kind: string; content: {
      format?: { page_break_before?: boolean };
      runs?: { text: string }[];
    } }[] }[] };
  };
}

test('Layout page setup persists real paper size, orientation and margins', async ({ page }) => {
  await start(page);
  await page.getByRole('button', { name: 'Layout', exact: true }).click();
  await page.getByRole('button', { name: 'Page setup…', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Page setup', exact: true });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole('combobox', { name: 'Paper size' })).toHaveValue('letter');
  await dialog.getByRole('combobox', { name: 'Paper size' }).selectOption('a4');
  await dialog.getByRole('combobox', { name: 'Page orientation' }).selectOption('landscape');
  await dialog.getByRole('spinbutton', { name: 'Top' }).fill('0.75');
  await dialog.getByRole('spinbutton', { name: 'Bottom' }).fill('0.5');
  await dialog.getByRole('button', { name: 'Apply page setup' }).click();
  await expect(dialog).toHaveCount(0);
  await readyPages(page);
  const native = await exportNative(page);
  const properties = native.document.sections[0].properties;
  expect(properties.page_size).toEqual({ width: 16838, height: 11906, orientation: 'landscape' });
  expect(properties.page_margins.top).toBe(1080);
  expect(properties.page_margins.bottom).toBe(720);
});

test('Insert Page break creates a separate paragraph; Layout can remove break', async ({ page }) => {
  await start(page);
  await focusEditor(page);
  await page.keyboard.insertText('Before ');
  await page.getByRole('button', { name: 'Insert', exact: true }).click();
  await page.getByRole('button', { name: 'Page break', exact: true }).click();
  await readyPages(page);
  let native = await exportNative(page);
  const paragraphs = native.document.sections[0].blocks.filter((block) => block.kind === 'paragraph');
  expect(paragraphs).toHaveLength(2);
  expect(paragraphs[0].content.runs?.map((run) => run.text).join('')).toBe('Before ');
  expect(paragraphs[1].content.format?.page_break_before).toBe(true);
  await page.getByRole('button', { name: 'Layout', exact: true }).click();
  await page.getByRole('button', { name: 'Remove page break', exact: true }).click();
  native = await exportNative(page);
  expect(native.document.sections[0].blocks[1].content.format?.page_break_before).toBe(false);
});
