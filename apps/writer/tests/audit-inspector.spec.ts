import { readFile } from 'node:fs/promises';
import { expect, test, type Page } from '@playwright/test';
import { documentAction, focusEditor, readyPages } from './step7-test-helpers';

interface NativeFile {
  document: {
    metadata: { title: string };
    sections: { blocks: { content: {
      style: string | null;
      format: { alignment: string | null };
      runs: { text: string; format: { bold: boolean | null; italic: boolean | null; underline: string | null; font_size: number | null; font_family: string | null } }[];
    } }[] }[];
  };
}

async function nativeCopy(page: Page): Promise<NativeFile> {
  await documentAction(page, 'Save As…');
  const dialog = page.getByRole('dialog', { name: 'Save As', exact: true });
  await dialog.getByRole('combobox', { name: 'File format', exact: true }).selectOption('writer');
  const pending = page.waitForEvent('download');
  await dialog.getByRole('button', { name: 'Save copy', exact: true }).click();
  const path = await (await pending).path();
  if (!path) throw new Error('Missing native inspection download');
  return JSON.parse((await readFile(path)).toString('utf8')) as NativeFile;
}

test('Details inspector controls update Rust formatting, history, preview and saved native content', async ({ page }) => {
  const runtimeErrors: string[] = [];
  page.on('pageerror', (error) => runtimeErrors.push(error.message));
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Save', exact: true })).toBeEnabled();
  await readyPages(page);
  await page.getByRole('button', { name: 'Open', exact: true }).click();
  const open = page.getByRole('dialog', { name: 'Open document', exact: true });
  const chooser = page.waitForEvent('filechooser');
  await open.getByRole('button', { name: 'Open file…', exact: true }).click();
  await (await chooser).setFiles({
    name: 'project-notes.writer', mimeType: 'application/octet-stream',
    buffer: await readFile(new URL('../../../tests/fixtures/native/project-notes.writer', import.meta.url)),
  });
  await expect(open).toHaveCount(0);
  await readyPages(page);
  const inspector = page.getByRole('complementary', { name: 'Formatting inspector', exact: true });
  await expect(inspector).toHaveCount(0);
  await page.getByRole('button', { name: 'Details', exact: true }).click();
  await expect(inspector).toBeVisible();
  await expect(page.getByRole('button', { name: 'Hide details', exact: true })).toHaveAttribute('aria-expanded', 'true');
  await focusEditor(page);
  await page.keyboard.press('Control+Home');
  await page.keyboard.press('Shift+End');
  await inspector.getByRole('combobox', { name: 'Paragraph style', exact: true }).selectOption('"project-notes-body-style"');
  await expect(page.locator('#paragraph-style')).toHaveValue('"project-notes-body-style"');
  await inspector.getByRole('combobox', { name: 'Font size', exact: true }).selectOption('18');
  await expect(page.locator('#font-size')).toHaveValue('18');
  await page.getByRole('combobox', { name: 'Font family', exact: true }).selectOption('Tinos');
  for (const property of ['bold', 'italic', 'underline']) {
    const button = inspector.getByRole('button', { name: property, exact: true });
    await expect(button).toHaveAttribute('aria-pressed', 'false');
    await button.click();
    await expect(button).toHaveAttribute('aria-pressed', 'true');
    await expect(page.locator('.formatting-bar').getByRole('button', {
      name: property.charAt(0).toUpperCase() + property.slice(1), exact: true,
    })).toHaveAttribute('aria-pressed', 'true');
  }
  for (const alignment of ['left', 'center', 'right']) {
    await inspector.getByRole('button', { name: `Align ${alignment}`, exact: true }).click();
    await expect(page.locator('.formatting-bar').getByRole('button', { name: `Align ${alignment}`, exact: true })).toHaveAttribute('aria-pressed', 'true');
  }
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  await expect(inspector.getByRole('button', { name: 'Align center', exact: true })).toHaveAttribute('aria-pressed', 'true');
  await page.getByRole('button', { name: 'Redo', exact: true }).click();
  await expect(inspector.getByRole('button', { name: 'Align right', exact: true })).toHaveAttribute('aria-pressed', 'true');
  const paged = await readyPages(page);
  const revision = await paged.getAttribute('data-layout-revision');
  const documentId = await paged.getAttribute('data-layout-document-id');
  await inspector.getByRole('button', { name: 'Page preview', exact: true }).click();
  const preview = page.getByRole('region', { name: 'Document page preview', exact: true });
  await expect(preview).toHaveAttribute('aria-busy', 'false');
  await expect(preview).toHaveAttribute('data-layout-revision', revision!);
  await expect(preview).toHaveAttribute('data-layout-document-id', documentId!);
  await expect(preview.locator('svg use').first()).toBeAttached();
  await expect(inspector.getByRole('button', { name: 'Page preview', exact: true })).toHaveAttribute('aria-pressed', 'true');
  await preview.getByRole('button', { name: 'Return to editing', exact: true }).click();
  await expect(preview).toHaveCount(0);
  await readyPages(page);
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(page.locator('.save-state')).toHaveText('Saved on this device');
  const native = await nativeCopy(page);
  const first = native.document.sections[0]!.blocks[0]!.content;
  expect(first.style).toBe('project-notes-body-style');
  expect(first.format.alignment).toBe('right');
  expect(first.runs.map((run) => run.text).join('')).toBe('Project notes');
  for (const run of first.runs.filter((run) => run.text)) {
    expect(run.format).toMatchObject({ bold: true, italic: true, underline: 'single', font_size: 36, font_family: 'Tinos' });
  }
  await inspector.getByRole('button', { name: 'Close formatting inspector', exact: true }).click();
  await expect(inspector).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Details', exact: true })).toHaveAttribute('aria-expanded', 'false');
  await expect(page.getByRole('button', { name: 'Details', exact: true })).toBeFocused();
  await page.reload();
  await expect(page.getByRole('button', { name: 'Open', exact: true })).toBeEnabled();
  await page.getByRole('button', { name: 'Open', exact: true }).click();
  await page.getByRole('button', { name: /Project notes/ }).click();
  await readyPages(page);
  expect(await nativeCopy(page)).toEqual(native);
  expect(runtimeErrors).toEqual([]);
});

test('Formatting inspector is intentionally hidden on narrow layouts while the editor remains reachable', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Save', exact: true })).toBeEnabled();
  await readyPages(page);
  await page.getByRole('button', { name: 'Details', exact: true }).click();
  const inspector = page.getByRole('complementary', { name: 'Formatting inspector', exact: true });
  await expect(inspector).toBeVisible();
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(page.locator('#writer-inspector')).toBeHidden();
  await expect(page.locator('.inspector-toggle')).toBeHidden();
  await focusEditor(page);
  await page.keyboard.insertText('Mobile editing remains available 👩🏽‍💻');
  await expect(page.getByRole('textbox', { name: 'Document content', exact: true })).toHaveText('Mobile editing remains available 👩🏽‍💻');
  await readyPages(page);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.setViewportSize({ width: 1280, height: 720 });
  await expect(inspector).toBeVisible();
  await inspector.getByRole('button', { name: 'Close formatting inspector', exact: true }).click();
  await expect(inspector).toHaveCount(0);
});

test.describe('touch inspector input', () => {
  test.use({ hasTouch: true, viewport: { width: 1024, height: 768 } });

  test('tablet inspector formatting preserves the Rust range without forcing the software input to refocus', async ({ page }) => {
    await page.goto('/');
    await expect(page.getByRole('button', { name: 'Save', exact: true })).toBeEnabled();
    await readyPages(page);
    const input = page.getByRole('textbox', { name: 'Document content', exact: true });
    await focusEditor(page);
    const text = 'Tablet range Café e\u0301 👩🏽‍💻';
    await page.keyboard.insertText(text);
    await page.keyboard.press('Control+a');
    await page.getByRole('button', { name: 'Details', exact: true }).tap();
    const inspector = page.getByRole('complementary', { name: 'Formatting inspector', exact: true });
    await expect(inspector).toBeVisible();
    for (const property of ['bold', 'italic', 'underline']) {
      await focusEditor(page);
      const control = inspector.getByRole('button', { name: property, exact: true });
      // Controlled Safari-like blur plus a trusted touch activation. This
      // checks focus policy, not a physical software keyboard's appearance.
      await control.evaluate((element) => element.addEventListener('pointerdown', () => {
        if (document.activeElement instanceof HTMLElement) document.activeElement.blur();
      }, { capture: true, once: true }));
      await control.tap();
      await expect(control).toHaveAttribute('aria-pressed', 'true');
      await expect(input).not.toBeFocused();
      await expect(input).toHaveText(text);
    }
    const spans = input.locator('span').filter({ hasText: text });
    await expect(spans).toHaveCSS('font-weight', '700');
    await expect(spans).toHaveCSS('font-style', 'italic');
    await expect(spans).toHaveCSS('text-decoration-line', 'underline');
    // A following hardware-keyboard activation keeps the desktop return-focus
    // behavior instead of inheriting the preceding touch gesture's policy.
    const bold = inspector.getByRole('button', { name: 'bold', exact: true });
    await bold.focus();
    await page.keyboard.press('Space');
    await expect(bold).toHaveAttribute('aria-pressed', 'false');
    await expect(input).toBeFocused();
  });
});
