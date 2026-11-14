import { readFile } from 'node:fs/promises';
import { test, expect, type Page } from '@playwright/test';
import { documentAction, focusEditor } from './step7-test-helpers';

const fixture = new URL('../../../tests/fixtures/layout/layout-v1.writer', import.meta.url);
const editor = (page: Page) => page.getByRole('textbox', { name: 'Document content', exact: true });
const preview = (page: Page) => page.getByRole('region', { name: 'Document page preview', exact: true });

async function start(page: Page) {
  await page.goto('/');
  await expect(editor(page)).toBeVisible();
  await expect(page.getByRole('button', { name: 'Save', exact: true })).toBeEnabled();
}

async function importReference(page: Page) {
  await page.getByLabel('Import Writer file', { exact: true }).setInputFiles({
    name: 'layout-v1.writer', mimeType: 'application/json', buffer: await readFile(fixture),
  });
  await expect(page.getByRole('textbox', { name: 'Document title' })).toHaveValue('Typography reference');
  await expect(editor(page)).toContainText('AV office');
}

async function showPreview(page: Page) {
  await documentAction(page, 'Page preview');
  await expect(preview(page)).toBeVisible();
  await expect(preview(page)).toHaveAttribute('aria-busy', 'false');
  // Preserved leading spaces have valid empty outlines and zero-sized <use>
  // boxes. Readiness must not assume the first glyph contains visible ink.
  await expect.poll(() => preview(page).locator('svg use').count()).toBeGreaterThan(0);
  return preview(page);
}

async function returnToEditing(page: Page) {
  await page.getByRole('button', { name: 'Return to editing', exact: true }).click();
  await expect(editor(page)).toBeVisible();
  await expect(editor(page)).toBeFocused();
}

/**
 * Portable vector visual regression: compare the actual rendered SVG paths and
 * transforms, rather than host text rasterization or an unpinned PNG baseline.
 * Element IDs are normalized to local outline indices; geometry is untouched.
 */
async function firstPageVector(page: Page) {
  const svg = preview(page).locator('[data-layout-page] svg').first();
  return svg.evaluate((element) => {
    const paths = [...(element.closest('[role="region"]')?.querySelectorAll('defs path') ?? [])];
    const outlineIndex = new Map(paths.map((path, index) => [path.id, index]));
    return {
      viewBox: element.getAttribute('viewBox'),
      outlines: paths.map((path) => path.getAttribute('d')),
      lines: [...element.querySelectorAll('[data-layout-line]')].map((line) => ({
        paragraphId: line.getAttribute('data-layout-paragraph-id'),
        lineIndex: line.getAttribute('data-layout-line'),
        startUtf16: line.getAttribute('data-layout-utf16-start'),
        endUtf16: line.getAttribute('data-layout-utf16-end'),
        glyphs: [...line.querySelectorAll('use')].map((glyph) => ({
          outline: outlineIndex.get((glyph.getAttribute('href') ?? '').slice(1)),
          transform: glyph.getAttribute('transform'),
          fill: glyph.getAttribute('fill'),
        })),
        decorations: [...line.querySelectorAll('rect')].map((decoration) => ({
          x: decoration.getAttribute('x'), y: decoration.getAttribute('y'),
          width: decoration.getAttribute('width'), height: decoration.getAttribute('height'),
          fill: decoration.getAttribute('fill'),
        })),
      })),
    };
  });
}

async function paragraphLineCount(page: Page, id: string | null) {
  return preview(page).locator('[data-layout-paragraph-id]').evaluateAll((elements, paragraphId) =>
    elements.filter((element) => element.getAttribute('data-layout-paragraph-id') === paragraphId).length, id);
}

test('page preview renders Rust glyph outlines and geometry, with a vector visual regression and font notices', async ({ page }) => {
  await start(page);
  await importReference(page);
  const region = await showPreview(page);
  await expect(region).toBeFocused();
  await expect(page.locator('.document-menu-panel button').filter({ hasText: /^Page preview$/ })).toHaveAttribute('aria-pressed', 'true');
  await expect(region).toHaveAttribute('data-layout-document-id', 'layout-reference-document');
  await expect(region).toHaveAttribute('data-layout-font-catalog-id', /\S+/);
  const pages = region.locator('[data-layout-page]');
  expect(await pages.count()).toBeGreaterThanOrEqual(3);
  await expect(pages.first()).toHaveAttribute('data-layout-width', String(300 * 64));
  await expect(pages.first()).toHaveAttribute('data-layout-height', String(225 * 64));
  expect(await region.locator('svg defs path').count()).toBeGreaterThan(10);
  expect(await region.locator('svg use').count()).toBeGreaterThan(20);
  await expect(region.locator('svg text')).toHaveCount(0);
  const referencesResolve = await region.locator('svg use').evaluateAll((glyphs) => glyphs.every((glyph) => {
    const reference = glyph.getAttribute('href');
    return reference?.startsWith('#') && glyph.ownerDocument.getElementById(reference.slice(1))?.tagName.toLowerCase() === 'path';
  }));
  expect(referencesResolve).toBe(true);
  const vector = await firstPageVector(page);
  expect(vector.lines.some((line) => line.startUtf16 !== null && line.endUtf16 !== null)).toBe(true);
  expect(JSON.stringify(vector, null, 2)).toMatchSnapshot('layout-vector-v1.json');
  await page.locator('details.document-menu summary').click();
  const notices = page.getByRole('link', { name: 'Font licenses', exact: true });
  await expect(notices).toBeVisible();
  const href = await notices.getAttribute('href');
  if (!href) throw new Error('Missing production font notice URL');
  const response = await page.request.get(new URL(href, page.url()).href);
  expect(response.ok()).toBe(true);
  expect(await response.text()).toBe(await readFile(new URL('../../../crates/office-layout/fonts/NOTICE.txt', import.meta.url), 'utf8'));
  await page.keyboard.press('Escape');
  await page.screenshot({ path: '/tmp/writer-step7-preview.png', fullPage: true });
});

test('page preview follows Rust editing, formatting, undo and redo revisions without replacing editing selection', async ({ page }) => {
  await start(page);
  await focusEditor(page);
  await page.keyboard.insertText('Cafe\u0301 👩🏽‍💻 العربية नमस्ते שלום');
  await page.keyboard.press('Control+a');
  await page.getByRole('button', { name: 'Bold', exact: true }).click();
  const region = await showPreview(page);
  const firstRevision = await region.getAttribute('data-layout-revision');
  const before = await firstPageVector(page);
  await returnToEditing(page);
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  await showPreview(page);
  await expect(preview(page)).not.toHaveAttribute('data-layout-revision', firstRevision ?? '');
  expect(await firstPageVector(page)).not.toEqual(before);
  await returnToEditing(page);
  await page.getByRole('button', { name: 'Redo', exact: true }).click();
  await showPreview(page);
  expect(await firstPageVector(page)).toEqual(before);
  await page.getByRole('button', { name: 'Return to editing', exact: true }).click();
  // The saved Rust selection still covers the entire text. Replacing it uses a
  // Rust transaction; preview's hidden DOM has no independent editing state.
  // Deliberately type immediately: deferred focus restoration used to lose this
  // first input. A wait for focus here would conceal that user-facing race.
  await page.keyboard.insertText('Replacement');
  await expect(editor(page)).toHaveText('Replacement');
  await showPreview(page);
  await expect(preview(page)).toContainText('Replacement');
});

test('native save, close, reload and reopen reproduce the same page preview', async ({ page }) => {
  await start(page);
  await importReference(page);
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(page.locator('.save-state')).toHaveText('Saved on this device');
  await showPreview(page);
  const before = await firstPageVector(page);
  const count = await preview(page).locator('[data-layout-page]').count();
  await returnToEditing(page);
  await documentAction(page, 'Close');
  await page.reload();
  await expect(editor(page)).toBeVisible();
  await page.getByRole('button', { name: 'Open', exact: true }).click();
  await page.getByRole('button', { name: /Typography reference/ }).click();
  await expect(editor(page)).toContainText('AV office');
  await showPreview(page);
  expect(await firstPageVector(page)).toEqual(before);
  await expect(preview(page).locator('[data-layout-page]')).toHaveCount(count);
});

test('preview revision refresh does not steal focus or lose title keystrokes', async ({ page }) => {
  await start(page);
  await importReference(page);
  await showPreview(page);
  const before = await firstPageVector(page);
  const previousRevision = await preview(page).getAttribute('data-layout-revision');
  const title = page.getByRole('textbox', { name: 'Document title', exact: true });
  await title.fill('');
  // Each real key causes a Rust metadata transaction and a new preview revision.
  // The delay lets the former revision-mount rAF focus theft reproduce; checking
  // only fill() would miss the lost second and subsequent keystrokes.
  await title.pressSequentially('Renamed layout', { delay: 80 });
  await expect(title).toHaveValue('Renamed layout');
  await expect(title).toBeFocused();
  await expect(preview(page)).toHaveAttribute('aria-busy', 'false');
  await expect(preview(page)).not.toHaveAttribute('data-layout-revision', previousRevision ?? '');
  expect(await firstPageVector(page)).toEqual(before);
});

test('Shift+Enter inserts a Rust soft break in the same paragraph and undo restores the original', async ({ page }) => {
  await start(page);
  await focusEditor(page);
  await page.keyboard.insertText('Before 👩🏽‍💻');
  const paragraph = editor(page).locator('p').first();
  const id = await paragraph.getAttribute('data-paragraph-id');
  await page.keyboard.press('Shift+Enter');
  await expect(editor(page).locator('p')).toHaveCount(1);
  await expect(paragraph).toHaveAttribute('data-paragraph-id', id ?? '');
  expect(await paragraph.textContent()).toBe('Before 👩🏽‍💻\n');
  await expect(paragraph.locator('br[data-trailing-soft-break="true"]')).toHaveCount(1);
  await page.keyboard.insertText('After e\u0301');
  expect(await paragraph.textContent()).toBe('Before 👩🏽‍💻\nAfter e\u0301');
  await expect(paragraph.locator('br[data-trailing-soft-break="true"]')).toHaveCount(0);
  await showPreview(page);
  await expect.poll(() => paragraphLineCount(page, id)).toBe(2);
  await page.getByRole('button', { name: 'Return to editing', exact: true }).click();
  await page.keyboard.press('Control+z');
  expect(await paragraph.textContent()).toBe('Before 👩🏽‍💻\n');
  await page.keyboard.press('Control+z');
  expect(await paragraph.textContent()).toBe('Before 👩🏽‍💻');
  await expect(editor(page).locator('p')).toHaveCount(1);
  await expect(paragraph).toHaveAttribute('data-paragraph-id', id ?? '');
  await showPreview(page);
  await expect.poll(() => paragraphLineCount(page, id)).toBe(1);
  await returnToEditing(page);
  await editor(page).evaluate((element) => element.dispatchEvent(new InputEvent('beforeinput', {
    bubbles: true, cancelable: true, inputType: 'insertLineBreak',
  })));
  expect(await paragraph.textContent()).toBe('Before 👩🏽‍💻\n');
  await expect(editor(page).locator('p')).toHaveCount(1);
  await page.keyboard.press('Control+z');
  expect(await paragraph.textContent()).toBe('Before 👩🏽‍💻');
});

test('DOCX geometry reaches the preview and fit-width presentation avoids narrow-window overflow', async ({ page }) => {
  await start(page);
  await page.getByLabel('Import Writer file', { exact: true }).setInputFiles({
    name: 'supported-formatting.docx',
    mimeType: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
    buffer: await readFile(new URL('../../../tests/fixtures/docx/supported-formatting.docx', import.meta.url)),
  });
  await expect(editor(page)).toContainText('日本語');
  const importedText = await editor(page).textContent();
  await documentAction(page, 'Page preview');
  await expect(preview(page).getByRole('alert')).toContainText(/tab/i);
  await expect(preview(page).locator('svg use')).toHaveCount(0);
  await returnToEditing(page);
  await expect(editor(page)).toHaveText(importedText ?? '');
  // Tab-stop policy is deliberately unsupported. Replace this one tab through
  // the normal browser-input/Rust-transaction path before testing geometry.
  await focusEditor(page);
  await editor(page).evaluate((element) => {
    const walker = element.ownerDocument.createTreeWalker(element, NodeFilter.SHOW_TEXT);
    let node = walker.nextNode();
    while (node) {
      const offset = node.textContent?.indexOf('\t') ?? -1;
      if (offset >= 0) {
        element.ownerDocument.getSelection()?.setBaseAndExtent(node, offset, node, offset + 1);
        element.ownerDocument.dispatchEvent(new Event('selectionchange'));
        return;
      }
      node = walker.nextNode();
    }
    throw new Error('The DOCX fixture must retain its original tab before editing');
  });
  await page.keyboard.insertText(' ');
  await expect.poll(() => editor(page).evaluate((element) => element.textContent?.includes('\t'))).toBe(false);
  await showPreview(page);
  await expect(preview(page).locator('[data-layout-page]').first()).toHaveAttribute('data-layout-width', String(612 * 64));
  const a4 = preview(page).locator(`[data-layout-width="${Math.round(11906 / 20 * 64)}"]`);
  await expect(a4.first()).toBeVisible();
  // CJK is intentionally absent from the bundled catalog. Missing glyphs must
  // remain visible as a limitation rather than silently using a host font.
  const notices = preview(page).locator('.layout-notices');
  await notices.locator('summary').click();
  await expect(notices.locator('[data-layout-diagnostic-kind="missing_glyph"]').first()).toBeVisible();
  await page.setViewportSize({ width: 420, height: 800 });
  await page.getByLabel('Preview zoom', { exact: true }).selectOption({ label: 'Fit width' });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await expect(page.getByRole('button', { name: 'Return to editing', exact: true })).toBeVisible();
});
