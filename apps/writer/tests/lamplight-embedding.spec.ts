import { readFile } from 'node:fs/promises';
import { expect, test, type Frame, type Page } from '@playwright/test';

// The trusted host is fulfilled locally. Its iframe loads the actual production
// Writer preview, engine and IndexedDB; this is not a live ProairetOS/device test.
const host = 'https://proairetos.com/__writer_merge_regression';
// Both HTTPS origins are intercepted locally; no live requests are sent.
const writer = 'https://writer.proairetos.com';
const tokens = {
  wall: '#211d19', page: '#302b25', card: '#383129', inset: '#443a30',
  raised: '#504333', mica: '#352d26', ink: '#f3eee5', ink2: '#c9bfae',
  lamp: '#edc669', onLamp: '#282117',
};
const text = 'Embedded Café e\u0301 👩🏽‍💻';
const content = (frame: Frame) => frame.getByRole('textbox', { name: 'Document content', exact: true });

async function openHost(page: Page): Promise<Frame> {
  // Serve the unchanged production preview under an intercepted HTTPS origin.
  // HTTPS host -> HTTP loopback iframes are blocked by modern local-network
  // protections; keep those protections and model actual HTTPS embedding.
  await page.route(`${writer}/**`, async (route) => {
    const url = new URL(route.request().url());
    const response = await route.fetch({ url: `http://127.0.0.1:4173${url.pathname}${url.search}` });
    await route.fulfill({ response });
  });
  await page.route(host, (route) => route.fulfill({ contentType: 'text/html', body: `<!doctype html>
    <html><head><meta name="viewport" content="width=device-width, initial-scale=1">
    <style>html,body{margin:0;height:100%;overflow:hidden}iframe{display:block;border:0;width:100%;height:100%}</style></head>
    <body><script>window.writerReady=[];addEventListener('message',event=>{
      if(event.data?.type==='writer:lamplight-ready')window.writerReady.push({origin:event.origin,data:event.data});
    });</script><iframe title="Writer integration test" referrerpolicy="origin" src="${writer}/"></iframe></body></html>` }));
  await page.goto(host);
  await expect.poll(() => page.frames().filter((frame) => frame.url() === `${writer}/`).length).toBe(1);
  const frame = page.frames().find((candidate) => candidate.url() === `${writer}/`)!;
  await expect(frame.getByRole('button', { name: 'Save', exact: true })).toBeEnabled();
  await expect(frame.getByTestId('paged-editor')).toHaveAttribute('aria-busy', 'false');
  await expect(frame.locator('svg[data-layout-page]').first()).toBeVisible();
  return frame;
}

async function theme(page: Page, mode = 'dark', palette: Record<string, string> = tokens) {
  await page.evaluate(({ mode, palette, target }) => {
    document.querySelector('iframe')!.contentWindow!.postMessage({
      type: 'proairetos:lamplight', version: 1, app: 'writer', mode, tokens: palette,
    }, target);
  }, { mode, palette, target: writer });
}

async function menuAction(frame: Frame, name: string) {
  const menu = frame.locator('details.document-menu');
  if (!await menu.evaluate((element) => element.hasAttribute('open'))) await menu.locator('summary').tap();
  await menu.getByRole('button', { name, exact: true }).tap();
}

test('standalone Writer keeps its identity and fallback palette', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Save', exact: true })).toBeEnabled();
  await expect(page.locator('.top-bar .identity')).toBeVisible();
  expect(await page.locator('html').getAttribute('data-writer-appearance')).toBeNull();
});

test('trusted parent theme updates chrome without changing paper or Rust page geometry', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(error.message));
  const frame = await openHost(page);
  await expect.poll(() => page.evaluate(() => (window as Window & { writerReady?: unknown[] }).writerReady?.length)).toBe(1);
  const paper = frame.locator('.paged-editor-paper').first();
  const before = await paper.evaluate((element) => ({ background: getComputedStyle(element).backgroundColor,
    width: element.querySelector('svg')!.getAttribute('data-layout-width'), height: element.querySelector('svg')!.getAttribute('data-layout-height') }));
  // Synthetic negative events exercise both guards; the positive message below
  // crosses the actual parent/iframe boundary with the browser's real origin.
  await frame.evaluate((palette) => {
    const data = { type: 'proairetos:lamplight', version: 1, app: 'writer', mode: 'dark', tokens: palette };
    window.dispatchEvent(new MessageEvent('message', { source: window.parent, origin: 'https://untrusted.invalid', data }));
    window.dispatchEvent(new MessageEvent('message', { source: window, origin: 'https://proairetos.com', data }));
  }, tokens);
  expect(await frame.locator('html').getAttribute('data-writer-appearance')).toBeNull();
  await theme(page, 'dark', { ...tokens, lamp: 'invalid' });
  expect(await frame.locator('html').getAttribute('data-writer-appearance')).toBeNull();
  await theme(page);
  await expect(frame.locator('html')).toHaveAttribute('data-writer-appearance', 'dark');
  await expect(frame.locator('.top-bar .identity')).toBeHidden();
  await expect(frame.locator('.top-bar')).toHaveCSS('background-color', 'rgb(56, 49, 41)');
  await theme(page, 'light', { ...tokens, card: '#eee8dc' });
  await expect(frame.locator('html')).toHaveAttribute('data-writer-appearance', 'light');
  await expect(frame.locator('.top-bar')).toHaveCSS('background-color', 'rgb(238, 232, 220)');
  expect(await paper.evaluate((element) => ({ background: getComputedStyle(element).backgroundColor,
    width: element.querySelector('svg')!.getAttribute('data-layout-width'), height: element.querySelector('svg')!.getAttribute('data-layout-height') }))).toEqual(before);
  expect(errors).toEqual([]);
});

test.describe('trusted embedded mobile Writer', () => {
  test.use({ viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true });

  test('trusted paper taps, focus chrome, saved reopening and native Save copy remain functional', async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', (error) => errors.push(error.message));
    let frame = await openHost(page);
    await theme(page);
    await expect(frame.locator('html')).toHaveAttribute('data-writer-appearance', 'dark');
    const title = frame.getByRole('textbox', { name: 'Document title', exact: true });
    await expect(title).toHaveCSS('font-size', '16px');
    await title.fill('Embedded merge regression');
    await frame.locator('svg[data-layout-page]').first().tap({ position: { x: 90, y: 90 } });
    await expect(content(frame)).toBeFocused();
    await expect(frame.locator('.status-bar')).toBeHidden();
    await expect(frame.locator('.mobile-document-actions')).toBeHidden();
    await page.keyboard.insertText(text);
    await expect(content(frame)).toHaveText(text);
    await expect(frame.getByTestId('paged-editor')).toHaveAttribute('aria-busy', 'false');
    await title.tap();
    await expect(frame.locator('.status-bar')).toBeVisible();
    await expect(frame.locator('.mobile-document-actions')).toBeVisible();
    await frame.getByRole('button', { name: 'Save', exact: true }).tap();
    await expect.poll(async () => ({
      status: await frame.locator('.mobile-save-state').textContent(),
      alerts: await frame.getByRole('alert').allTextContents(),
    })).toEqual({ status: 'Saved on this device', alerts: [] });
    await menuAction(frame, 'Save As…');
    const dialog = frame.getByRole('dialog', { name: 'Save As', exact: true });
    await dialog.getByRole('combobox', { name: 'File format', exact: true }).selectOption('writer');
    const copy = dialog.getByRole('button', { name: 'Save copy', exact: true });
    await expect(copy).toBeInViewport();
    const downloading = page.waitForEvent('download');
    await copy.tap();
    const download = await downloading;
    expect(download.suggestedFilename()).toMatch(/\.writer$/);
    const path = await download.path();
    expect(path).not.toBeNull();
    const bytes = await readFile(path!);
    expect(bytes.toString('utf8')).toContain('Embedded merge regression');
    await expect(dialog).toHaveCount(0);
    await expect(content(frame)).toHaveText(text);
    await page.reload();
    frame = page.frames().find((candidate) => candidate.url() === `${writer}/`)!;
    await expect(frame.getByRole('button', { name: 'Save', exact: true })).toBeEnabled();
    await theme(page);
    await menuAction(frame, 'Open document');
    const library = frame.getByRole('dialog', { name: 'Open document', exact: true });
    await library.getByRole('button', { name: /Embedded merge regression/ }).tap();
    await expect(library).toHaveCount(0);
    await expect(content(frame)).toHaveText(text);
    expect(errors).toEqual([]);
  });

  test('long-press selection and handle hover keep Rust endpoints and transparent touch targets', async ({ page, browserName }) => {
    test.skip(browserName !== 'chromium', 'Trusted multi-event touch injection uses Chromium CDP.');
    const frame = await openHost(page);
    await theme(page);
    await frame.locator('svg[data-layout-page]').first().tap({ position: { x: 90, y: 90 } });
    await page.keyboard.insertText(text);
    await expect(frame.getByTestId('paged-editor')).toHaveAttribute('aria-busy', 'false');
    const glyph = await frame.locator('svg[data-layout-page] g[data-layout-paragraph-id] use').nth(2).boundingBox();
    if (!glyph) throw new Error('The Rust glyph is not displayed');
    const cdp = await page.context().newCDPSession(page);
    await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [
      { x: glyph.x + glyph.width / 2, y: glyph.y + glyph.height / 2, radiusX: 2, radiusY: 2 },
    ] });
    await page.waitForTimeout(650);
    await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
    const anchor = frame.getByTestId('touch-selection-handle-anchor');
    const focus = frame.getByTestId('touch-selection-handle-focus');
    await expect(anchor).toBeVisible();
    await expect(focus).toBeVisible();
    const endpoints = [await anchor.getAttribute('data-utf16'), await focus.getAttribute('data-utf16')];
    expect(endpoints).toEqual(['0', '8']);
    await anchor.hover();
    await expect(anchor).toHaveCSS('background-color', 'rgba(0, 0, 0, 0)');
    expect([await anchor.getAttribute('data-utf16'), await focus.getAttribute('data-utf16')]).toEqual(endpoints);
    await frame.getByRole('button', { name: 'Bold', exact: true }).tap();
    await expect(frame.getByRole('button', { name: 'Bold', exact: true })).toHaveAttribute('aria-pressed', 'true');
    await expect(content(frame)).toHaveText(text);
    expect([await anchor.getAttribute('data-utf16'), await focus.getAttribute('data-utf16')]).toEqual(endpoints);
    await frame.getByRole('button', { name: 'Undo', exact: true }).tap();
    await expect(frame.getByRole('button', { name: 'Bold', exact: true })).toHaveAttribute('aria-pressed', 'false');
    await expect(content(frame)).toHaveText(text);
    await cdp.detach();
  });
});
