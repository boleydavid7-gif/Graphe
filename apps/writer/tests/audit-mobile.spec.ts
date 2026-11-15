import { expect, test, type Page } from '@playwright/test';
import { readyPages } from './step7-test-helpers';

// Real Chromium with trusted touch taps and keyboard events. Viewport/clipboard/
// autocorrect payloads below are controlled browser-event simulations, not a
// physical Android/iOS keyboard, selection handle, clipboard or PWA test.
test.use({ viewport: { width: 390, height: 844 }, hasTouch: true });

const input = (page: Page) => page.getByRole('textbox', { name: 'Document content', exact: true });

async function start(page: Page) {
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Save', exact: true })).toBeEnabled();
  await readyPages(page);
}

async function tapPaper(page: Page) {
  const box = await page.locator('svg[data-layout-page]').first().boundingBox();
  if (!box) throw new Error('Missing Rust-rendered document page');
  await page.touchscreen.tap(box.x + box.width * 0.3, box.y + box.width * 0.14);
  await expect(input(page)).toBeFocused();
}

test('touch formatting retains the Rust Unicode range without reopening input, and undo/redo retain text', async ({ page }) => {
  await start(page);
  await tapPaper(page);
  const text = 'Cafe\u0301 👩🏽‍💻 — العربية';
  await page.keyboard.insertText(text);
  await page.keyboard.press('Control+Home');
  for (let index = 0; index < 4; index += 1) await page.keyboard.press('Shift+ArrowRight');
  await expect.poll(() => page.evaluate(() => window.getSelection()?.toString())).toBe('Cafe\u0301');
  for (const name of ['Bold', 'Italic', 'Underline']) {
    const control = page.getByRole('button', { name, exact: true });
    // Model Safari's focus behavior explicitly: unlike desktop Chromium, it
    // may blur input before dispatching the button's compatibility click.
    await control.evaluate((element) => element.addEventListener('pointerdown', () => {
      if (document.activeElement instanceof HTMLElement) document.activeElement.blur();
    }, { capture: true, once: true }));
    await control.tap();
    await expect(control).toHaveAttribute('aria-pressed', 'true');
    // A toolbar touch may blur the native input. It must not force focus back
    // and reopen the keyboard; the actual selection stays in the Rust engine.
    await expect(input(page)).not.toBeFocused();
    await expect(input(page)).toHaveText(text);
  }
  const formatted = input(page).locator('span').filter({ hasText: /^Cafe\u0301$/ });
  await expect(formatted).toHaveCSS('font-weight', '700');
  await expect(formatted).toHaveCSS('font-style', 'italic');
  await expect(formatted).toHaveCSS('text-decoration-line', 'underline');
  await page.getByRole('button', { name: 'Undo', exact: true }).tap();
  await expect(page.getByRole('button', { name: 'Underline', exact: true })).toHaveAttribute('aria-pressed', 'false');
  await page.getByRole('button', { name: 'Redo', exact: true }).tap();
  await expect(page.getByRole('button', { name: 'Underline', exact: true })).toHaveAttribute('aria-pressed', 'true');
  await tapPaper(page);
  await page.keyboard.press('Control+End');
  await page.keyboard.insertText('!');
  await expect(input(page)).toHaveText(`${text}!`);
});

for (const cancelable of [true, false]) test(`${cancelable ? 'cancellable' : 'uncancellable'} autocorrect target ranges replace the intended word rather than the current caret`, async ({ page }) => {
  await start(page);
  await tapPaper(page);
  await page.keyboard.insertText('teh Cafe\u0301 👩🏽‍💻');
  const cancelled = await input(page).evaluate((element, cancelable) => {
    const text = element.querySelector('span')?.firstChild;
    if (!text) throw new Error('Missing controlled text projection');
    const target = new StaticRange({ startContainer: text, startOffset: 0, endContainer: text, endOffset: 3 });
    const event = new InputEvent('beforeinput', { bubbles: true, cancelable,
      inputType: 'insertReplacementText', data: 'the' });
    Object.defineProperty(event, 'getTargetRanges', { value: () => [target] });
    const cancelled = !element.dispatchEvent(event);
    if (!cancelable) {
      // Emulate the browser mutating its transient projection before input;
      // the Rust replacement still uses the captured pre-edit target range.
      text.textContent = 'the Cafe\u0301 👩🏽‍💻';
      element.dispatchEvent(new InputEvent('input', { bubbles: true,
        inputType: 'insertReplacementText', data: 'the' }));
    }
    return cancelled;
  }, cancelable);
  expect(cancelled).toBe(cancelable);
  await expect(input(page)).toHaveText('the Cafe\u0301 👩🏽‍💻');
  await page.keyboard.press('Control+z');
  await expect(input(page)).toHaveText('teh Cafe\u0301 👩🏽‍💻');
  await page.keyboard.press('Control+y');
  await expect(input(page)).toHaveText('the Cafe\u0301 👩🏽‍💻');
});

test('keyboard viewport changes, toolbar blur and screen rotation preserve input and clear obsolete inset', async ({ page }) => {
  await page.addInitScript(() => {
    const viewport = Object.assign(new EventTarget(), { offsetTop: 0, offsetLeft: 0, width: 390, height: 844, scale: 1 });
    Object.defineProperty(window, 'visualViewport', { configurable: true, value: viewport });
  });
  await start(page);
  await tapPaper(page);
  await page.keyboard.insertText('Viewport e\u0301 👩🏽‍💻');
  await page.evaluate(() => {
    Object.assign(window.visualViewport!, { height: 400, offsetTop: 35 });
    window.visualViewport!.dispatchEvent(new Event('resize'));
    window.visualViewport!.dispatchEvent(new Event('scroll'));
  });
  const inset = () => page.locator('.workspace').evaluate((element) =>
    parseFloat((element as HTMLElement).style.getPropertyValue('--writer-keyboard-inset')));
  await expect.poll(inset).toBeGreaterThan(0);
  const title = page.getByRole('textbox', { name: 'Document title', exact: true });
  await title.focus();
  await expect.poll(inset).toBe(0);
  // A queued viewport event must not restore editor focus after the user has
  // deliberately moved to document controls.
  await page.evaluate(() => window.visualViewport!.dispatchEvent(new Event('resize')));
  await expect(title).toBeFocused();
  await page.setViewportSize({ width: 844, height: 390 });
  await page.evaluate(() => {
    Object.assign(window.visualViewport!, { width: 844, height: 390, offsetTop: 0 });
    window.visualViewport!.dispatchEvent(new Event('resize'));
  });
  await expect.poll(inset).toBe(0);
  await page.setViewportSize({ width: 390, height: 844 });
  await page.evaluate(() => {
    Object.assign(window.visualViewport!, { width: 390, height: 844 });
    window.visualViewport!.dispatchEvent(new Event('resize'));
  });
  await tapPaper(page);
  await page.keyboard.press('Control+End');
  await page.keyboard.insertText(' — rotated');
  await expect(input(page)).toHaveText('Viewport e\u0301 👩🏽‍💻 — rotated');
  await expect.poll(inset).toBe(0);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
});

test('controlled native clipboard events cut and paste Unicode paragraphs through Rust history', async ({ page }) => {
  await start(page);
  await tapPaper(page);
  const text = 'Cafe\u0301 👩🏽‍💻\nالعربية नमस्ते';
  await page.keyboard.insertText(text);
  await page.keyboard.press('Control+a');
  const copied = await input(page).evaluate((element) => {
    const clipboardData = new DataTransfer();
    element.dispatchEvent(new ClipboardEvent('copy', { bubbles: true, cancelable: true, clipboardData }));
    return clipboardData.getData('text/plain');
  });
  expect(copied).toBe(text);
  const cut = await input(page).evaluate((element) => {
    const clipboardData = new DataTransfer();
    element.dispatchEvent(new ClipboardEvent('cut', { bubbles: true, cancelable: true, clipboardData }));
    return clipboardData.getData('text/plain');
  });
  expect(cut).toBe(text);
  await expect(input(page)).toHaveText('');
  await page.keyboard.press('Control+z');
  await expect(input(page).locator('p')).toHaveCount(2);
  await expect(input(page).locator('p').first()).toHaveText('Cafe\u0301 👩🏽‍💻');
  await page.keyboard.press('Control+End');
  await input(page).evaluate((element) => {
    const clipboardData = new DataTransfer();
    clipboardData.setData('text/plain', '\nPasted e\u0301 👩🏽‍💻');
    element.dispatchEvent(new ClipboardEvent('paste', { bubbles: true, cancelable: true, clipboardData }));
  });
  await expect(input(page).locator('p')).toHaveCount(3);
  await expect(input(page).locator('p').last()).toHaveText('Pasted e\u0301 👩🏽‍💻');
  await page.keyboard.press('Control+z');
  await expect(input(page).locator('p')).toHaveCount(2);
});

test.describe('browser-native touch zoom', () => {
  test.skip(({ browserName }) => browserName !== 'chromium',
    'Chromium CDP provides the controlled multitouch input for this regression.');
  test.use({ isMobile: true });

  test('two-finger page zoom remains available without changing Rust text or layout', async ({ page, context }) => {
    await start(page);
    await tapPaper(page);
    await page.keyboard.insertText('Pinch zoom — Cafe\u0301 👩🏽‍💻');
    await readyPages(page);
    const paper = page.locator('svg[data-layout-page]').first();
    const originalViewBox = await paper.getAttribute('viewBox');
    const originalRevision = await page.getByTestId('paged-editor').getAttribute('data-layout-revision');
    await page.getByRole('textbox', { name: 'Document title', exact: true }).focus();
    const box = await paper.boundingBox();
    if (!box) throw new Error('Missing actual Rust page');
    const y = Math.max(box.y + 40, Math.min(500, box.y + box.height - 40));
    const client = await context.newCDPSession(page);
    try {
      await client.send('Input.dispatchTouchEvent', { type: 'touchStart',
        touchPoints: [{ x: 170, y, id: 1 }, { x: 220, y, id: 2 }] });
      for (let step = 1; step <= 10; step += 1) {
        await client.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: [
          { x: 170 - step * 8, y, id: 1 }, { x: 220 + step * 8, y, id: 2 },
        ] });
        // Distinct native touch frames let the browser process its own gesture.
        await page.waitForTimeout(30);
      }
      await client.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
      await expect.poll(() => page.evaluate(() => window.visualViewport?.scale ?? 1)).toBeGreaterThan(1.1);
      await expect(paper).toHaveAttribute('viewBox', originalViewBox ?? '');
      await expect(page.getByTestId('paged-editor')).toHaveAttribute('data-layout-revision', originalRevision ?? '');
      await expect(input(page)).toHaveText('Pinch zoom — Cafe\u0301 👩🏽‍💻');
    } finally {
      await client.detach();
    }
  });
});
