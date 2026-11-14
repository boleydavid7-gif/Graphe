import { readFile } from 'node:fs/promises';
import { test, expect } from '@playwright/test';
import { documentAction, focusEditor, readyPages } from './step7-test-helpers';

test('portable browser editing uses Rust pages, complete Unicode selections, formatting and undo', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Save', exact: true })).toBeEnabled();
  const root = await readyPages(page);
  const surface = root.locator('svg[data-layout-page]').first();
  const box = await surface.boundingBox();
  if (!box) throw new Error('The Rust page was not displayed');
  await page.mouse.click(box.x + box.width * 0.2, box.y + box.height * 0.15);
  const input = page.getByRole('textbox', { name: 'Document content', exact: true });
  await expect(input).toBeFocused();
  await page.keyboard.insertText('Cafe\u0301 👩🏽‍💻');
  await page.keyboard.press('Enter');
  await page.keyboard.insertText('العربية नमस्ते שלום');
  await expect(input.locator('p')).toHaveCount(2);
  await expect(input.locator('p').first()).toHaveText('Cafe\u0301 👩🏽‍💻');
  await expect(input.locator('p').last()).toHaveText('العربية नमस्ते שלום');
  await readyPages(page);
  await expect(root.locator('svg text')).toHaveCount(0);
  expect(await root.locator('svg use').count()).toBeGreaterThan(10);
  await page.keyboard.press('Control+Home');
  await page.keyboard.press('Shift+ArrowRight');
  await page.getByRole('button', { name: 'Bold', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Bold', exact: true })).toHaveAttribute('aria-pressed', 'true');
  await page.keyboard.press('Control+z');
  await expect(page.getByRole('button', { name: 'Bold', exact: true })).toHaveAttribute('aria-pressed', 'false');
  await page.keyboard.press('Control+Home');
  for (let count = 0; count < 4; count += 1) await page.keyboard.press('ArrowRight');
  await page.keyboard.press('Shift+ArrowRight');
  await page.keyboard.press('Shift+ArrowRight');
  await expect.poll(() => page.evaluate(() => window.getSelection()?.toString())).toBe(' 👩🏽‍💻');
  await expect(root.locator('[data-editor-selection="true"]').first()).toBeVisible();
  await page.keyboard.press('Backspace');
  await expect(input.locator('p').first()).toHaveText('Cafe\u0301');
  await page.keyboard.press('Control+z');
  await expect(input.locator('p').first()).toHaveText('Cafe\u0301 👩🏽‍💻');
});

test('portable browser native import, autosave, reload, reopen and export preserve identity and content', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Save', exact: true })).toBeEnabled();
  const original = await readFile(new URL('../../../tests/fixtures/layout/layout-v1.writer', import.meta.url));
  await page.getByLabel('Import Writer file', { exact: true }).setInputFiles({
    name: 'cross-browser.writer', mimeType: 'application/json', buffer: original,
  });
  await expect(page.getByRole('textbox', { name: 'Document title', exact: true })).toHaveValue('Typography reference');
  await readyPages(page);
  await page.getByRole('textbox', { name: 'Document title', exact: true }).fill('Cross-browser recovery');
  await focusEditor(page);
  await page.keyboard.press('Control+End');
  await page.keyboard.insertText(' — recovered e\u0301 👩🏽‍💻');
  await expect(page.locator('.save-state')).toHaveText('Saved on this device', { timeout: 10000 });
  await page.reload();
  await expect(page.getByRole('button', { name: 'Save', exact: true })).toBeEnabled();
  await page.getByRole('button', { name: 'Open', exact: true }).click();
  await page.getByRole('button', { name: /Cross-browser recovery/ }).click();
  const input = page.getByRole('textbox', { name: 'Document content', exact: true });
  await expect(input.locator('p').last()).toContainText(' — recovered e\u0301 👩🏽‍💻');
  const root = await readyPages(page);
  await expect(root).toHaveAttribute('data-layout-document-id', 'layout-reference-document');
  await expect(page.getByRole('combobox', { name: 'Paragraph style', exact: true }).getByRole('option', { name: 'Heading', exact: true })).toHaveCount(1);
  const pending = page.waitForEvent('download');
  await documentAction(page, 'Export');
  const download = await pending;
  expect(download.suggestedFilename()).toMatch(/\.writer$/);
  const path = await download.path();
  if (!path) throw new Error('The browser did not retain the exported native file');
  const file = JSON.parse(await readFile(path, 'utf8')) as {
    document: { id: string; styles: unknown[]; sections: { blocks: { content: { runs: { text: string }[] } }[] }[] };
  };
  expect(file.document.id).toBe('layout-reference-document');
  expect(file.document.styles).toHaveLength(2);
  expect(file.document.sections.at(-1)?.blocks.at(-1)?.content.runs.map((run) => run.text).join('')).toContain(' — recovered e\u0301 👩🏽‍💻');
});
