import { readFile } from 'node:fs/promises';
import { expect, test, type Page } from '@playwright/test';
import { documentAction, focusEditor, readyPages } from './step7-test-helpers';

type NativeFile = { document: { sections: Array<{
  id: string;
  properties: { page_size?: { width: number; height: number; orientation: string } } | null;
  blocks: Array<{ kind: string; content: {
    id?: string;
    runs?: Array<{ text: string }>;
  } }>;
}> } };

async function start(page: Page) {
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Save', exact: true })).toBeEnabled();
  await readyPages(page);
}

async function exportNative(page: Page): Promise<NativeFile> {
  await documentAction(page, 'Save As…');
  const dialog = page.getByRole('dialog', { name: 'Save As', exact: true });
  await dialog.getByRole('combobox', { name: 'File format', exact: true }).selectOption('writer');
  const waiting = page.waitForEvent('download');
  await dialog.getByRole('button', { name: 'Save copy', exact: true }).click();
  const file = await waiting;
  const path = await file.path();
  if (!path) throw new Error('The native file was not downloaded');
  return JSON.parse(await readFile(path, 'utf8')) as NativeFile;
}

test('section breaks retain contents, enable independent landscape layout, and require confirmation to join', async ({ page }) => {
  await start(page);
  await focusEditor(page);
  await page.keyboard.insertText('Before');
  await page.getByRole('button', { name: 'Insert', exact: true }).click();
  const sectionBreak = page.getByRole('button', { name: 'Section break', exact: true });
  await expect(sectionBreak).toBeEnabled();
  await sectionBreak.click();
  await page.keyboard.insertText('After');
  await readyPages(page);
  await page.getByRole('button', { name: 'Layout', exact: true }).click();
  await expect(page.locator('.section-position')).toContainText('Section 2 of 2');
  await page.getByRole('button', { name: 'Page setup…', exact: true }).click();
  const setup = page.getByRole('dialog', { name: 'Page setup', exact: true });
  await expect(setup).toContainText('Section 2 of 2');
  await setup.getByRole('combobox', { name: 'Page orientation' }).selectOption('landscape');
  await setup.getByRole('button', { name: 'Apply page setup' }).click();
  await expect(setup).toHaveCount(0);
  await readyPages(page);

  const before = await exportNative(page);
  expect(before.document.sections).toHaveLength(2);
  const first = before.document.sections[0];
  const second = before.document.sections[1];
  if (!first || !second) throw new Error('The native file must contain two sections');
  expect(first.blocks.filter((block) => block.kind === 'paragraph')
    .flatMap((block) => block.content.runs?.map((run) => run.text) ?? []).join('')).toBe('Before');
  expect(second.blocks.filter((block) => block.kind === 'paragraph')
    .flatMap((block) => block.content.runs?.map((run) => run.text) ?? []).join('')).toBe('After');
  expect(second.properties?.page_size?.orientation).toBe('landscape');
  expect(second.properties?.page_size?.width).toBe(15840);
  expect(first.properties?.page_size?.width).toBeUndefined();

  await page.getByRole('button', { name: 'Layout', exact: true }).click();
  await page.getByRole('button', { name: 'Remove section break…', exact: true }).click();
  const confirm = page.getByRole('dialog', { name: 'Remove section break', exact: true });
  await expect(confirm).toContainText('preceding section');
  await confirm.getByRole('button', { name: 'Remove break', exact: true }).click();
  await expect(confirm).toHaveCount(0);
  await readyPages(page);
  const after = await exportNative(page);
  expect(after.document.sections).toHaveLength(1);
  const blocks = after.document.sections[0]?.blocks ?? [];
  expect(blocks.filter((block) => block.kind === 'paragraph')
    .flatMap((block) => block.content.runs?.map((run) => run.text) ?? []).join('')).toBe('BeforeAfter');
});
