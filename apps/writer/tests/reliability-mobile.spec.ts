import { expect, test, type Page } from '@playwright/test';
import { readyPages } from './step7-test-helpers';

// This exercises trusted touch input in a mobile-sized Chromium context. It is
// not evidence that a physical iOS software keyboard or selection handles open.
test.use({ viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true });

const editor = (page: Page) => page.getByRole('textbox', { name: 'Document content', exact: true });

async function start(page: Page) {
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Save', exact: true })).toBeEnabled();
  await readyPages(page);
}

async function tapPaper(page: Page) {
  const rect = await page.locator('svg[data-layout-page]').first().boundingBox();
  if (!rect) throw new Error('The actual Rust-rendered page is unavailable');
  await page.touchscreen.tap(rect.x + rect.width * 0.28, rect.y + rect.width * 0.15);
}

test('trusted document taps focus the stable input and accept typing after pagination rerenders', async ({ page }) => {
  await start(page);
  await page.getByRole('textbox', { name: 'Document title', exact: true }).focus();
  await tapPaper(page);
  await expect(editor(page)).toBeFocused();
  await editor(page).evaluate((element) => { (window as Window & { writingInput?: Element }).writingInput = element; });
  const text = Array.from({ length: 60 }, (_, index) => `Paragraph ${index} — Café e\u0301 👩🏽‍💻`).join('\n');
  await page.keyboard.insertText(text);
  await readyPages(page);
  expect(await page.locator('svg[data-layout-page]').count()).toBeGreaterThan(1);
  await expect(editor(page)).toBeFocused();
  expect(await editor(page).evaluate((element) => (window as Window & { writingInput?: Element }).writingInput === element)).toBe(true);
  await page.getByRole('textbox', { name: 'Document title', exact: true }).focus();
  await page.locator('.workspace').evaluate((element) => { element.scrollTop = 0; });
  await tapPaper(page);
  await expect(editor(page)).toBeFocused();
  await page.keyboard.insertText('Retapped ');
  await expect(editor(page)).toContainText('Retapped ');
});

test('a completed touch tap still focuses when an edit invalidates the painted page geometry', async ({ page }) => {
  await start(page);
  await editor(page).focus();
  await page.keyboard.insertText('Keep the latest Rust selection');
  await readyPages(page);
  await page.getByRole('textbox', { name: 'Document title', exact: true }).focus();
  await page.locator('svg[data-layout-page]').first().evaluate((svg) => {
    svg.addEventListener('pointerup', () => {
      const input = document.querySelector('[data-testid="editor"]');
      const title = document.querySelector('[aria-label="Document title"]');
      if (!(input instanceof HTMLElement) || !(title instanceof HTMLElement)) throw new Error('Missing editor or title');
      // Reproduce a tap concurrent with a document update before its next Rust
      // layout frame. The old SVG must not choose a stale document position.
      input.dispatchEvent(new InputEvent('beforeinput', { bubbles: true, cancelable: true,
        inputType: 'insertText', data: ' — updated' }));
      title.focus();
    }, { capture: true, once: true });
  });
  await tapPaper(page);
  await expect(editor(page)).toBeFocused();
  await expect(editor(page)).toHaveText('Keep the latest Rust selection — updated');
  await page.keyboard.insertText('!');
  await expect(editor(page)).toHaveText('Keep the latest Rust selection — updated!');
});

test('the input proxy and visible Rust caret stay within a reduced visual viewport', async ({ page }) => {
  await page.addInitScript(() => {
    const viewport = Object.assign(new EventTarget(), { offsetTop: 0, offsetLeft: 0, width: 390, height: 844, scale: 1 });
    (window as Window & { testingViewport?: typeof viewport }).testingViewport = viewport;
    Object.defineProperty(window, 'visualViewport', { configurable: true, get: () => viewport });
  });
  await start(page);
  await editor(page).focus();
  await page.keyboard.insertText(Array.from({ length: 35 }, (_, index) => `Viewport line ${index}`).join('\n'));
  await expect(editor(page)).toContainText('Viewport line 34');
  await readyPages(page);
  await page.evaluate(() => {
    const viewport = (window as Window & {
      testingViewport?: EventTarget & { offsetTop: number; offsetLeft: number; width: number; height: number };
    }).testingViewport;
    if (!viewport) throw new Error('Missing viewport fixture');
    Object.assign(viewport, { offsetTop: 80, offsetLeft: 5, width: 380, height: 380 });
    viewport.dispatchEvent(new Event('resize'));
    viewport.dispatchEvent(new Event('scroll'));
  });
  await expect.poll(async () => {
    const proxy = await page.locator('.paged-editor-input').boundingBox();
    return !!proxy && proxy.y >= 80 && proxy.y + proxy.height <= 460 && proxy.x >= 5 && proxy.x + proxy.width <= 385;
  }).toBe(true);
  await expect.poll(async () => {
    const caret = await page.locator('[data-editor-caret]').boundingBox();
    return !!caret && caret.y >= 80 && caret.y + caret.height <= 460;
  }).toBe(true);
  await expect(editor(page)).toBeFocused();
  const height = await page.locator('.workspace').evaluate((element) => element.getBoundingClientRect().height);
  // The keyboard can cover the whole editor in landscape or with open notices.
  // Its inset must not increase the flex item's own height on every resize.
  for (const reducedHeight of [200, 100]) {
    await page.evaluate((height) => {
      const viewport = (window as Window & { testingViewport?: EventTarget & { height: number; offsetTop: number } }).testingViewport!;
      Object.assign(viewport, { offsetTop: 0, height });
      viewport.dispatchEvent(new Event('resize'));
    }, reducedHeight);
    await page.waitForTimeout(150);
    const occludedHeight = await page.locator('.workspace').evaluate((element) => element.getBoundingClientRect().height);
    expect(Math.abs(occludedHeight - height)).toBeLessThan(1);
  }
  // Rotation/layout-viewport shrink must not inherit an inflated flex minimum
  // from the old keyboard padding either.
  await page.setViewportSize({ width: 390, height: 480 });
  await expect.poll(() => page.locator('.workspace').evaluate((element) =>
    element.getBoundingClientRect().bottom)).toBeLessThanOrEqual(480);
  await page.setViewportSize({ width: 390, height: 844 });
  await page.evaluate(() => {
    const viewport = (window as Window & { testingViewport?: EventTarget & { height: number; offsetTop: number } }).testingViewport!;
    Object.assign(viewport, { offsetTop: 0, height: 844 });
    viewport.dispatchEvent(new Event('resize'));
  });
  await expect.poll(() => page.locator('.workspace').evaluate((element) =>
    parseFloat((element as HTMLElement).style.getPropertyValue('--writer-keyboard-inset')))).toBe(0);
});

test('touch scroll and cancelled gestures preserve title focus and document selection', async ({ page }) => {
  await start(page);
  await editor(page).focus();
  await page.keyboard.insertText('Touch scroll must not become an edit or keyboard request.');
  await readyPages(page);
  const title = page.getByRole('textbox', { name: 'Document title', exact: true });
  await title.focus();
  const paper = page.locator('svg[data-layout-page]').first();
  const box = await paper.boundingBox();
  if (!box) throw new Error('Missing actual page');
  await paper.dispatchEvent('pointerdown', { pointerType: 'touch', pointerId: 24, button: 0,
    clientX: box.x + 50, clientY: box.y + 120 });
  await paper.dispatchEvent('pointermove', { pointerType: 'touch', pointerId: 24,
    clientX: box.x + 50, clientY: box.y + 180 });
  await paper.dispatchEvent('pointerup', { pointerType: 'touch', pointerId: 24,
    clientX: box.x + 50, clientY: box.y + 180 });
  await expect(title).toBeFocused();
  await paper.dispatchEvent('pointerdown', { pointerType: 'touch', pointerId: 25, button: 0,
    clientX: box.x + 50, clientY: box.y + 120 });
  await paper.dispatchEvent('pointercancel', { pointerType: 'touch', pointerId: 25 });
  await expect(title).toBeFocused();
  await expect(editor(page)).toHaveText('Touch scroll must not become an edit or keyboard request.');
});
