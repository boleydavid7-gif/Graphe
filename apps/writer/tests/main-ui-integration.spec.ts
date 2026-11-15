import { readFile } from 'node:fs/promises';
import { expect, test, type Locator, type Page } from '@playwright/test';
import { focusEditor, glyphPoint, readyPages } from './step7-test-helpers';

// These are real-browser checks of the combined production application. Narrow
// viewports and CDP touch do not establish physical iOS/Android keyboard behavior.
const text = 'Main integration — Café e\u0301 👩🏽‍💻';
const input = (page: Page) => page.getByRole('textbox', { name: 'Document content', exact: true });
const saveDialog = (page: Page) => page.getByRole('dialog', { name: 'Save As', exact: true });
const tabs = ['Write', 'Insert', 'Layout', 'Review'] as const;

async function activate(control: Locator, touch: boolean) {
  if (touch) await control.tap(); else await control.click();
}

async function start(page: Page) {
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Save', exact: true })).toBeEnabled();
  await readyPages(page);
}

async function indicatorMatchesActiveTab(page: Page) {
  const options = page.locator('.editor-tab-options');
  const indicator = options.locator('.editor-tab-indicator');
  await expect(page.locator('.editor-tab-indicator')).toHaveCount(1);
  await expect(indicator).toHaveCSS('opacity', '1');
  await expect.poll(async () => options.evaluate((element) => {
    const active = element.querySelector('button[aria-pressed="true"]');
    const line = element.querySelector('.editor-tab-indicator');
    if (!active || !line) throw new Error('The shared tab indicator or active command is missing');
    const container = element.getBoundingClientRect();
    const selected = active.getBoundingClientRect();
    const underline = line.getBoundingClientRect();
    return Math.max(Math.abs(underline.width - container.width / 4),
      Math.abs(underline.left - selected.left), Math.abs(underline.right - selected.right),
      Math.abs(underline.bottom - container.bottom));
  })).toBeLessThan(1);
}

async function openSaveAs(page: Page, touch: boolean) {
  const menu = page.locator('details.document-menu');
  await activate(menu.locator('summary'), touch);
  await activate(menu.getByRole('button', { name: 'Save As…', exact: true }), touch);
  await expect(saveDialog(page)).toBeVisible();
  await saveDialog(page).getByRole('combobox', { name: 'File format', exact: true }).selectOption('writer');
  await expect(saveDialog(page).getByRole('button', { name: 'Save copy', exact: true })).toBeEnabled();
}

async function compactCenteredDialog(page: Page) {
  const dialog = saveDialog(page);
  await expect(dialog).toHaveCSS('animation-name', 'none');
  await expect(dialog).toHaveCSS('translate', 'none');
  await expect(dialog).toHaveCSS('scale', 'none');
  await expect(dialog.getByRole('checkbox')).toHaveCount(0);
  await expect(dialog.getByRole('note', { name: 'Format warning', exact: true })).toHaveCount(0);
  await expect(dialog.getByRole('button', { name: 'Save copy', exact: true })).toBeInViewport();
  await expect(dialog.getByRole('button', { name: 'Cancel', exact: true })).toBeInViewport();
  await expect.poll(async () => dialog.evaluate((element) => {
    const rect = element.getBoundingClientRect();
    const viewport = window.visualViewport;
    const left = viewport?.offsetLeft ?? 0;
    const top = viewport?.offsetTop ?? 0;
    const width = viewport?.width ?? window.innerWidth;
    const height = viewport?.height ?? window.innerHeight;
    return Math.max(Math.abs(rect.top + rect.height / 2 - top - height / 2),
      Math.abs(rect.left + rect.width / 2 - left - width / 2),
      left - rect.left, rect.right - left - width, top - rect.top, rect.bottom - top - height);
  })).toBeLessThan(2);
  expect((await dialog.boundingBox())!.height).toBeLessThan(400);
}

for (const touch of [false, true]) {
  test.describe(touch ? 'latest main UI on mobile' : 'latest main UI on desktop', () => {
    test.use(touch
      ? { viewport: { width: 390, height: 844 }, hasTouch: true, isMobile: true }
      : { viewport: { width: 1280, height: 800 } });

    test('one shared indicator follows every tab, resizing and reduced motion', async ({ page }) => {
      const errors: string[] = [];
      page.on('pageerror', (error) => errors.push(error.message));
      await page.emulateMedia({ reducedMotion: 'no-preference' });
      await start(page);
      const navigation = page.getByRole('navigation', { name: 'Editor sections', exact: true });
      const paper = page.locator('.paged-editor-paper').first();
      const initialPaperTop = (await paper.boundingBox())!.y;
      for (const name of tabs) {
        const button = navigation.getByRole('button', { name, exact: true });
        await activate(button, touch);
        await expect(button).toHaveAttribute('aria-pressed', 'true');
        await expect(button).toBeInViewport();
        await indicatorMatchesActiveTab(page);
        await button.hover();
        await expect(button).toHaveCSS('background-color', 'rgba(0, 0, 0, 0)');
        if (touch) {
          await expect(page.locator('.formatting-bar')).toHaveCSS('height', '52px');
          expect(Math.abs((await paper.boundingBox())!.y - initialPaperTop)).toBeLessThan(1);
        }
      }
      await page.setViewportSize(touch ? { width: 430, height: 760 } : { width: 980, height: 720 });
      await indicatorMatchesActiveTab(page);
      await page.emulateMedia({ reducedMotion: 'reduce' });
      await expect(page.locator('.editor-tab-indicator')).toHaveCSS('transition-duration', '0s');
      for (const name of [...tabs].reverse()) {
        await activate(navigation.getByRole('button', { name, exact: true }), touch);
        await indicatorMatchesActiveTab(page);
      }
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
      await expect(input(page)).toHaveText('');
      expect(errors).toEqual([]);
    });

    test('Save copy stays unanimated, centered and reachable after viewport resizing', async ({ page }) => {
      const errors: string[] = [];
      page.on('pageerror', (error) => errors.push(error.message));
      // Motion must be enabled to catch generic entrance rules overriding the
      // copy dialog's deliberate keyboard-safe animation exclusion.
      await page.emulateMedia({ reducedMotion: 'no-preference' });
      await start(page);
      await focusEditor(page);
      await page.keyboard.insertText(text);
      await openSaveAs(page, touch);
      await compactCenteredDialog(page);
      // This is an actual browser viewport resize, not a physical keyboard test.
      await page.setViewportSize(touch ? { width: 390, height: 520 } : { width: 1000, height: 620 });
      await compactCenteredDialog(page);
      await activate(saveDialog(page).getByRole('button', { name: 'Cancel', exact: true }), touch);
      await expect(saveDialog(page)).toHaveCount(0);
      await expect(input(page)).toHaveText(text);
      await openSaveAs(page, touch);
      await compactCenteredDialog(page);
      const downloading = page.waitForEvent('download');
      await activate(saveDialog(page).getByRole('button', { name: 'Save copy', exact: true }), touch);
      const download = await downloading;
      expect(download.suggestedFilename()).toMatch(/\.writer$/);
      const path = await download.path();
      if (!path) throw new Error('The browser did not retain the native copy');
      expect((await readFile(path)).toString('utf8')).toContain(text);
      await expect(saveDialog(page)).toHaveCount(0);
      await expect(input(page)).toHaveText(text);
      expect(errors).toEqual([]);
    });

    if (touch) test('active selection handles retain their Rust-projected hit geometry', async ({ page, browserName }) => {
      test.skip(browserName !== 'chromium', 'Trusted multi-event touch input requires Chromium CDP.');
      await page.emulateMedia({ reducedMotion: 'no-preference' });
      await start(page);
      await focusEditor(page);
      await page.keyboard.insertText('alpha beta gamma');
      await page.getByRole('textbox', { name: 'Document title', exact: true }).focus();
      const point = await glyphPoint(page, 0, 2);
      const cdp = await page.context().newCDPSession(page);
      try {
        await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ ...point, id: 1 }] });
        const anchor = page.getByTestId('touch-selection-handle-anchor');
        await expect(anchor).toBeVisible(); // Wait for Writer's deliberate word hold.
        await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
        const before = await anchor.boundingBox();
        if (!before) throw new Error('The Rust anchor handle was not displayed');
        const endpoints = [await anchor.getAttribute('data-utf16'),
          await page.getByTestId('touch-selection-handle-focus').getAttribute('data-utf16')];
        await anchor.evaluate((element) => element.addEventListener('pointerdown', (event) => {
          element.setAttribute('data-test-pointer-id', String((event as PointerEvent).pointerId));
        }, { once: true }));
        await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{
          x: before.x + before.width / 2, y: before.y + before.height / 2, id: 2,
        }] });
        await expect.poll(() => anchor.evaluate((element) => {
          const pointerId = element.getAttribute('data-test-pointer-id');
          return pointerId !== null && element.hasPointerCapture(Number(pointerId));
        })).toBe(true);
        // Writer prevents the touch default to protect IME focus, so Chromium
        // need not set :active. Keep the real captured gesture and separately
        // force CSS pseudo states to verify generic motion/hover cannot leak in.
        await cdp.send('DOM.enable');
        await cdp.send('CSS.enable');
        const { root } = await cdp.send('DOM.getDocument');
        const { nodeId } = await cdp.send('DOM.querySelector', { nodeId: root.nodeId,
          selector: '[data-testid="touch-selection-handle-anchor"]' });
        await cdp.send('CSS.forcePseudoState', { nodeId, forcedPseudoClasses: ['active', 'hover'] });
        await expect(anchor).toHaveCSS('scale', 'none');
        await expect(anchor).toHaveCSS('transition-duration', '0s');
        await expect(anchor).toHaveCSS('background-color', 'rgba(0, 0, 0, 0)');
        const after = await anchor.boundingBox();
        if (!after) throw new Error('The active Rust anchor handle was not displayed');
        expect(Math.abs(after.width - before.width)).toBeLessThan(0.1);
        expect(Math.abs(after.height - before.height)).toBeLessThan(0.1);
        expect([await anchor.getAttribute('data-utf16'),
          await page.getByTestId('touch-selection-handle-focus').getAttribute('data-utf16')]).toEqual(endpoints);
      } finally {
        await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
        await cdp.detach();
      }
    });
  });
}
