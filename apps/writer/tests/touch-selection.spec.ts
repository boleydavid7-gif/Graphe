import { expect, test, type CDPSession, type Page } from '@playwright/test';
import { focusEditor, glyphPoint, readyPages } from './step7-test-helpers';

// Actual Chromium pointer input comes from CDP touch frames. This does not
// establish physical Android/iOS keyboard, OS clipboard, loupe or PWA behavior.
test.skip(({ browserName }) => browserName !== 'chromium',
  'These trusted multi-frame touch regressions require Chromium CDP.');
test.use({ viewport: { width: 390, height: 844 }, hasTouch: true, isMobile: true });
test.setTimeout(60_000);

type Point = { x: number; y: number };
type Endpoint = 'anchor' | 'focus';
const input = (page: Page) => page.getByRole('textbox', { name: 'Document content', exact: true });
const handle = (page: Page, endpoint: Endpoint) => page.getByTestId(`touch-selection-handle-${endpoint}`);
const actions = (page: Page) => page.getByRole('toolbar', { name: 'Text selection', exact: true });

async function start(page: Page, text = 'alpha beta gamma delta') {
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Save', exact: true })).toBeEnabled();
  await readyPages(page);
  await focusEditor(page);
  await page.keyboard.insertText(text);
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(page.locator('.save-state')).toHaveText('Saved on this device');
  await page.getByRole('textbox', { name: 'Document title', exact: true }).focus();
  await readyPages(page);
}

async function longPress(page: Page, client: CDPSession, point: Point) {
  await client.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ ...point, id: 1 }] });
  await page.waitForTimeout(700); // More than Writer's deliberate 550ms hold.
  // Wait for the actual browser timer/render before releasing on slow runners.
  await expect(handle(page, 'anchor')).toBeVisible();
  await client.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
  await expect(handle(page, 'anchor')).toBeVisible();
  await expect(handle(page, 'focus')).toBeVisible();
  await expect(actions(page)).toBeVisible();
}

async function endpoint(page: Page, which: Endpoint) {
  return handle(page, which).evaluate((element) => ({
    paragraphId: element.getAttribute('data-paragraph-id'),
    utf16: Number(element.getAttribute('data-utf16')),
    pageNumber: Number(element.getAttribute('data-page-number')),
  }));
}

async function beginDrag(page: Page, client: CDPSession, which: Endpoint) {
  const bounds = await handle(page, which).boundingBox();
  if (!bounds) throw new Error(`Missing displayed ${which} handle`);
  const point = { x: bounds.x + bounds.width / 2, y: bounds.y + bounds.height / 2 };
  await client.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ ...point, id: 2 }] });
  return point;
}

async function moveFinger(page: Page, client: CDPSession, from: Point, to: Point) {
  for (let step = 1; step <= 8; step += 1) {
    const progress = step / 8;
    await client.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: [{
      x: from.x + (to.x - from.x) * progress, y: from.y + (to.y - from.y) * progress, id: 2,
    }] });
    await page.waitForTimeout(25);
  }
}

async function drag(page: Page, client: CDPSession, which: Endpoint, source: Point, target: Point) {
  const grab = await beginDrag(page, client, which);
  // Preserve the grab's offset below the Rust caret rather than treating the
  // 44px button center as an independent text-layout coordinate.
  const destination = { x: grab.x + target.x - source.x, y: grab.y + target.y - source.y };
  await moveFinger(page, client, grab, destination);
  await client.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
}

async function pinchSelectedMiddleWord(page: Page, client: CDPSession) {
  await longPress(page, client, await glyphPoint(page, 0, 12)); // gamma, UTF-16 11..16.
  const center = await glyphPoint(page, 0, 14);
  await client.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [
    { x: center.x - 25, y: center.y, id: 1 }, { x: center.x + 25, y: center.y, id: 2 },
  ] });
  for (let step = 1; step <= 10; step += 1) {
    await client.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: [
      { x: center.x - 25 - step * 6, y: center.y, id: 1 },
      { x: center.x + 25 + step * 6, y: center.y, id: 2 },
    ] });
    await page.waitForTimeout(30);
  }
  await client.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
  await expect.poll(() => page.evaluate(() => window.visualViewport?.scale ?? 1)).toBeGreaterThan(1.3);
  // Finish the native zoom/viewport animation before starting a separate grip.
  // A changing viewport deliberately cancels an already-active app gesture.
  await page.evaluate(() => new Promise<void>((resolve) => {
    let previous = '';
    let stable = 0;
    const tick = () => {
      const viewport = window.visualViewport;
      const signature = JSON.stringify([viewport?.offsetLeft, viewport?.offsetTop, viewport?.scale]);
      stable = signature === previous ? stable + 1 : 0;
      previous = signature;
      if (stable >= 12) resolve(); else requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
  }));
}

async function viewportTouchPoint(page: Page, point: Point) {
  // CDP touch coordinates use CSS pixels relative to the *visual* viewport;
  // native PointerEvent.clientX/Y and DOM/SVG rectangles use layout coordinates.
  // Empirically verified after actual native zoom: subtract offsets, no scale.
  return page.evaluate(({ x, y }) => ({ x: x - (visualViewport?.offsetLeft ?? 0),
    y: y - (visualViewport?.offsetTop ?? 0) }), point);
}

async function records(page: Page) {
  return page.evaluate(async () => {
    const database = await new Promise<IDBDatabase>((resolve, reject) => {
      const request = indexedDB.open('writer-documents', 1);
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error);
    });
    try {
      return await new Promise<{ id: string; etag: string; bytes: number[] }[]>((resolve, reject) => {
        const transaction = database.transaction('documents', 'readonly');
        const request = transaction.objectStore('documents').getAll();
        let result: { id: string; etag: string; bytes: number[] }[] = [];
        request.onsuccess = () => {
          result = (request.result as { id: string; etag: string; bytes: Uint8Array }[])
            .map((record) => ({ id: record.id, etag: record.etag, bytes: [...record.bytes] }));
        };
        transaction.oncomplete = () => resolve(result);
        transaction.onabort = () => reject(transaction.error);
        transaction.onerror = () => reject(transaction.error);
      });
    } finally { database.close(); }
  });
}

async function startPages(page: Page) {
  await start(page);
  const text = 'Review steady source positions and dependable selection. '.repeat(70);
  const file = {
    format: 'office-writer', format_version: 2,
    document: {
      schema_version: 2, id: 'touch-pages-document', metadata: { title: 'Touch pages' }, styles: [],
      sections: [{ id: 'touch-pages-section', properties: {
        page_size: { width: 6000, height: 4000, orientation: null },
        page_margins: { top: 400, right: 400, bottom: 400, left: 400, header: 0, footer: 0, gutter: 0 },
      }, blocks: ['alpha beta gamma', text].map((value, index) => ({ kind: 'paragraph', content: {
        id: `touch-pages-${index}`, style: null, format: {}, runs: [{ text: value, format: {} }],
      } })) }],
    },
  };
  // A public synthetic input file, decoded by Rust through the real Open UI.
  const menu = page.locator('details.document-menu');
  await menu.locator('summary').click();
  await menu.getByRole('button', { name: 'Open document', exact: true }).click();
  const opening = page.getByRole('dialog', { name: 'Open document', exact: true });
  const pending = page.waitForEvent('filechooser');
  await opening.getByRole('button', { name: 'Open file…', exact: true }).click();
  await (await pending).setFiles({ name: 'Touch pages.writer', mimeType: 'application/octet-stream',
    buffer: Buffer.from(JSON.stringify(file)) });
  await expect(opening).toHaveCount(0);
  await readyPages(page);
  expect(await page.locator('svg[data-layout-page]').count()).toBeGreaterThan(3);
  await page.getByRole('textbox', { name: 'Document title', exact: true }).focus();
  await page.locator('.workspace').evaluate((element) => { element.scrollTop = 0; });
}

async function lastLinePoint(page: Page, paragraphIndex: number) {
  const paragraphId = await input(page).locator('p').nth(paragraphIndex).getAttribute('data-paragraph-id');
  return page.locator('g[data-layout-paragraph-id]').evaluateAll((elements, id) => {
    const line = elements.filter((element) => element.getAttribute('data-layout-paragraph-id') === id).at(-1);
    const svg = line?.closest('svg');
    if (!(line instanceof SVGGraphicsElement) || !(svg instanceof SVGSVGElement)) throw new Error('Missing final Rust line');
    const matrix = svg.getScreenCTM();
    if (!matrix) throw new Error('Missing Rust page-to-screen transform');
    const x = Number(line.getAttribute('data-layout-x')) + Number(line.getAttribute('data-layout-width')) + 256;
    const y = Number(line.getAttribute('data-layout-y')) + Number(line.getAttribute('data-layout-height')) / 2;
    const point = new DOMPoint(x, y).matrixTransform(matrix);
    return { x: point.x, y: point.y, utf16: Number(line.getAttribute('data-layout-utf16-end')),
      pageNumber: Number(svg.getAttribute('data-layout-page')) };
  }, paragraphId);
}

test('long press selects a combining-mark word, copies exact Rust text, and leaves saved bytes and history intact', async ({ page, context }) => {
  await context.grantPermissions(['clipboard-read', 'clipboard-write']);
  const text = 'Cafe\u0301 👩🏽‍💻 alpha beta';
  await start(page, text);
  const stored = await records(page);
  const root = await readyPages(page);
  const revision = await root.getAttribute('data-layout-revision');
  const client = await context.newCDPSession(page);
  try {
    await longPress(page, client, await glyphPoint(page, 0, 1));
    expect((await endpoint(page, 'anchor')).utf16).toBe(0);
    expect((await endpoint(page, 'focus')).utf16).toBe(5);
    await page.screenshot({ path: '../../docs/images/writer-touch-selection-mobile.png' });
    await actions(page).getByRole('button', { name: 'Copy', exact: true }).tap();
    await expect.poll(() => page.evaluate(() => navigator.clipboard.readText())).toBe('Cafe\u0301');
    await expect(root).toHaveAttribute('data-layout-revision', revision!);
    expect(await records(page)).toEqual(stored);
    await expect(page.locator('.save-state')).toHaveText('Saved on this device');
    await focusEditor(page);
    await page.keyboard.press('Control+z');
    await expect(input(page)).toHaveText('');
    await expect(page.getByRole('button', { name: 'Undo', exact: true })).toBeDisabled();
  } finally { await client.detach(); }
});

test('long press on an emoji keeps its complete UTF-16 range and formatting uses the selected Rust word', async ({ page, context }) => {
  await start(page, '👩🏽‍💻 alpha beta');
  const client = await context.newCDPSession(page);
  try {
    await longPress(page, client, await glyphPoint(page, 0, 0));
    expect((await endpoint(page, 'anchor')).utf16).toBe(0);
    expect((await endpoint(page, 'focus')).utf16).toBe(7);
    for (const name of ['Bold', 'Italic', 'Underline']) {
      const control = page.locator('.formatting-bar').getByRole('button', { name, exact: true });
      await control.tap();
      await expect(control).toHaveAttribute('aria-pressed', 'true');
    }
    const selected = input(page).locator('span').filter({ hasText: /^👩🏽‍💻$/ });
    await expect(selected).toHaveCSS('font-weight', '700');
    await expect(selected).toHaveCSS('font-style', 'italic');
    await expect(selected).toHaveCSS('text-decoration-line', 'underline');
    await expect(input(page)).toHaveText('👩🏽‍💻 alpha beta');
    await page.getByRole('button', { name: 'Undo', exact: true }).tap();
    await expect(page.locator('.formatting-bar').getByRole('button', { name: 'Underline', exact: true })).toHaveAttribute('aria-pressed', 'false');
    await page.getByRole('button', { name: 'Redo', exact: true }).tap();
    await expect(page.locator('.formatting-bar').getByRole('button', { name: 'Underline', exact: true })).toHaveAttribute('aria-pressed', 'true');
  } finally { await client.detach(); }
});

test('the word-hold timer keeps title focus until the trusted touch release', async ({ page, context }) => {
  await start(page);
  const title = page.getByRole('textbox', { name: 'Document title', exact: true });
  await expect(title).toBeFocused();
  const client = await context.newCDPSession(page);
  try {
    const point = await glyphPoint(page, 0, 1);
    await client.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ ...point, id: 1 }] });
    await page.waitForTimeout(700);
    await expect(handle(page, 'anchor')).toBeVisible();
    await expect(title).toBeFocused();
    await expect(input(page)).not.toBeFocused();
    await client.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
    await actions(page).getByRole('button', { name: 'Edit selected text', exact: true }).tap();
    await expect(input(page)).toBeFocused();
    expect((await endpoint(page, 'anchor')).utf16).toBe(0);
    expect((await endpoint(page, 'focus')).utf16).toBe(5);
  } finally { await client.detach(); }
});

test('both logical handles drag past each other without swapping endpoint identity', async ({ page, context }) => {
  await start(page);
  const root = await readyPages(page);
  const revision = await root.getAttribute('data-layout-revision');
  const client = await context.newCDPSession(page);
  try {
    await longPress(page, client, await glyphPoint(page, 0, 1));
    const focus = await endpoint(page, 'focus');
    await drag(page, client, 'anchor', await glyphPoint(page, 0, 0), await glyphPoint(page, 0, 11));
    await expect.poll(async () => (await endpoint(page, 'anchor')).utf16).toBe(11);
    expect(await endpoint(page, 'focus')).toEqual(focus);
    await drag(page, client, 'focus', await glyphPoint(page, 0, 5), await glyphPoint(page, 0, 17));
    await expect.poll(async () => (await endpoint(page, 'focus')).utf16).toBe(17);
    expect((await endpoint(page, 'anchor')).utf16).toBe(11);
    await expect(root).toHaveAttribute('data-layout-revision', revision!);
    await expect(input(page)).toHaveText('alpha beta gamma delta');
  } finally { await client.detach(); }
});

test('a captured handle survives scrolling and selects across wrapped lines and physical pages', async ({ page, context }) => {
  await startPages(page);
  const root = await readyPages(page);
  const revision = await root.getAttribute('data-layout-revision');
  const client = await context.newCDPSession(page);
  try {
    await longPress(page, client, await glyphPoint(page, 0, 1));
    const fixed = await endpoint(page, 'anchor');
    const source = await glyphPoint(page, 0, 5);
    const grab = await beginDrag(page, client, 'focus');
    await page.locator('.workspace').evaluate((element) => { element.scrollTop = element.scrollHeight; });
    await page.evaluate(() => new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve()))));
    expect(await page.locator('.workspace').evaluate((element) => element.scrollTop)).toBeGreaterThan(100);
    const target = await lastLinePoint(page, 1);
    await moveFinger(page, client, grab, { x: grab.x + target.x - source.x, y: grab.y + target.y - source.y });
    await client.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
    await expect.poll(async () => (await endpoint(page, 'focus')).utf16).toBe(target.utf16);
    expect((await endpoint(page, 'focus')).pageNumber).toBe(target.pageNumber);
    expect(await endpoint(page, 'anchor')).toEqual(fixed);
    const selectedPages = await page.locator('svg[data-layout-page]').evaluateAll((surfaces) =>
      surfaces.filter((surface) => surface.querySelector('[data-editor-selection="true"]')).length);
    expect(selectedPages).toBeGreaterThan(3);
    await expect(root).toHaveAttribute('data-layout-revision', revision!);
  } finally { await client.detach(); }
});

test('an external Rust selection change at the same revision cancels an in-progress handle drag', async ({ page, context }) => {
  await start(page);
  const root = await readyPages(page);
  const revision = await root.getAttribute('data-layout-revision');
  const client = await context.newCDPSession(page);
  try {
    await longPress(page, client, await glyphPoint(page, 0, 1));
    const grab = await beginDrag(page, client, 'focus');
    await expect(input(page)).toBeFocused();
    await page.keyboard.press('Control+a');
    await expect.poll(async () => (await endpoint(page, 'focus')).utf16).toBe(22);
    const anchor = await endpoint(page, 'anchor');
    const focus = await endpoint(page, 'focus');
    await moveFinger(page, client, grab, { x: grab.x + 60, y: grab.y });
    await client.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
    expect(await endpoint(page, 'anchor')).toEqual(anchor);
    expect(await endpoint(page, 'focus')).toEqual(focus);
    await expect(root).toHaveAttribute('data-layout-revision', revision!);
    await expect(input(page)).toHaveText('alpha beta gamma delta');
  } finally { await client.detach(); }
});

test('holding a dragged handle at the workspace edge autoscrolls and stops when released', async ({ page, context }) => {
  await startPages(page);
  const client = await context.newCDPSession(page);
  try {
    await longPress(page, client, await glyphPoint(page, 0, 1));
    const fixed = await endpoint(page, 'anchor');
    const grab = await beginDrag(page, client, 'focus');
    const viewport = await page.locator('.workspace').boundingBox();
    if (!viewport) throw new Error('Missing document workspace');
    await moveFinger(page, client, grab, { x: Math.min(grab.x, viewport.x + viewport.width - 10),
      y: Math.min(820, viewport.y + viewport.height - 6) });
    await expect.poll(() => page.locator('.workspace').evaluate((element) => element.scrollTop)).toBeGreaterThan(100);
    await page.waitForTimeout(250); // Finger remains still while the app scrolls.
    expect(await endpoint(page, 'anchor')).toEqual(fixed);
    expect((await endpoint(page, 'focus')).utf16).toBeGreaterThan(5);
    await client.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
    const stopped = await page.locator('.workspace').evaluate((element) => element.scrollTop);
    await page.waitForTimeout(200);
    expect(Math.abs(await page.locator('.workspace').evaluate((element) => element.scrollTop) - stopped)).toBeLessThanOrEqual(2);
  } finally { await client.detach(); }
});

test('native pan and a second finger cancel pending long presses instead of selecting during scrolling', async ({ page, context }) => {
  await startPages(page);
  const client = await context.newCDPSession(page);
  try {
    const point = await glyphPoint(page, 0, 1);
    await client.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ ...point, id: 1 }] });
    await client.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: [{ x: point.x, y: point.y + 80, id: 1 }] });
    await page.waitForTimeout(700);
    await client.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
    await expect(handle(page, 'anchor')).toHaveCount(0);
    await expect(actions(page)).toHaveCount(0);

    await page.locator('.workspace').evaluate((element) => { element.scrollTop = 0; });
    const next = await glyphPoint(page, 0, 1);
    await client.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ ...next, id: 1 }] });
    await client.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ ...next, id: 1 }, { x: next.x + 45, y: next.y, id: 2 }] });
    await page.waitForTimeout(700);
    await client.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
    await expect(handle(page, 'anchor')).toHaveCount(0);
    await expect(actions(page)).toHaveCount(0);
  } finally { await client.detach(); }
});

test('a delayed long press cannot select against a newer Rust revision', async ({ page, context }) => {
  await start(page);
  const root = await readyPages(page);
  const revision = await root.getAttribute('data-layout-revision');
  const client = await context.newCDPSession(page);
  try {
    const point = await glyphPoint(page, 0, 1);
    await client.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ ...point, id: 1 }] });
    await page.getByRole('textbox', { name: 'Document title', exact: true }).fill('Changed during the hold');
    await expect(root).not.toHaveAttribute('data-layout-revision', revision!);
    await page.waitForTimeout(700);
    await client.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
    await expect(handle(page, 'anchor')).toHaveCount(0);
    await expect(actions(page)).toHaveCount(0);
    await expect(input(page)).toHaveText('alpha beta gamma delta');
    await expect(page.getByRole('textbox', { name: 'Document title', exact: true })).toHaveValue('Changed during the hold');
  } finally { await client.detach(); }
});

test('native pinch preserves the selected Rust endpoints and page revision', async ({ page, context }) => {
  await start(page);
  const root = await readyPages(page);
  const revision = await root.getAttribute('data-layout-revision');
  const client = await context.newCDPSession(page);
  try {
    await longPress(page, client, await glyphPoint(page, 0, 1));
    const anchor = await endpoint(page, 'anchor');
    const focus = await endpoint(page, 'focus');
    await client.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ x: 170, y: 500, id: 1 }, { x: 220, y: 500, id: 2 }] });
    for (let step = 1; step <= 10; step += 1) {
      await client.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: [
        { x: 170 - step * 8, y: 500, id: 1 }, { x: 220 + step * 8, y: 500, id: 2 },
      ] });
      await page.waitForTimeout(30);
    }
    await client.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
    await expect.poll(() => page.evaluate(() => window.visualViewport?.scale ?? 1)).toBeGreaterThan(1.1);
    expect(await endpoint(page, 'anchor')).toEqual(anchor);
    expect(await endpoint(page, 'focus')).toEqual(focus);
    await expect(root).toHaveAttribute('data-layout-revision', revision!);
    await expect(input(page)).toHaveText('alpha beta gamma delta');
    // A completed pinch must not leave a touch-cancellation flag blocking a
    // subsequent mouse device. Click inside the paper's actual visible part.
    const point = await page.locator('svg[data-layout-page]').first().evaluate((element) => {
      const rect = element.getBoundingClientRect();
      const viewport = window.visualViewport;
      const left = viewport?.offsetLeft ?? 0;
      const top = viewport?.offsetTop ?? 0;
      const right = left + (viewport?.width ?? innerWidth);
      const bottom = top + (viewport?.height ?? innerHeight);
      return { x: (Math.max(rect.left, left) + Math.min(rect.right, right)) / 2 - left,
        y: (Math.max(rect.top, top) + Math.min(rect.bottom, bottom)) / 2 - top };
    });
    await page.mouse.click(point.x, point.y);
    await expect(input(page)).toBeFocused();
    await expect(handle(page, 'anchor')).toHaveCount(0);
  } finally { await client.detach(); }
});

test('a real post-pinch handle grab extends the selected Rust word without changing its anchor', async ({ page, context }) => {
  await start(page);
  const root = await readyPages(page);
  const revision = await root.getAttribute('data-layout-revision');
  const client = await context.newCDPSession(page);
  try {
    await pinchSelectedMiddleWord(page, client);
    expect((await endpoint(page, 'anchor')).utf16).toBe(11);
    expect((await endpoint(page, 'focus')).utf16).toBe(16);
    const fixed = await endpoint(page, 'anchor');
    await expect(handle(page, 'focus')).toBeVisible();
    // Playwright's boundingBox uses visual-viewport coordinates after pinch;
    // read the DOM rectangle to share the Rust glyphs' layout coordinate space.
    const box = await handle(page, 'focus').evaluate((element) => element.getBoundingClientRect().toJSON());
    const grab = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
    const source = await glyphPoint(page, 0, 16);
    const target = await glyphPoint(page, 0, 21);
    const destination = { x: grab.x + target.x - source.x, y: grab.y + target.y - source.y };
    await client.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ ...await viewportTouchPoint(page, grab), id: 3 }] });
    for (let step = 1; step <= 8; step += 1) {
      const point = { x: grab.x + (destination.x - grab.x) * step / 8,
        y: grab.y + (destination.y - grab.y) * step / 8 };
      await client.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: [{ ...await viewportTouchPoint(page, point), id: 3 }] });
      await page.waitForTimeout(25);
    }
    await client.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
    await expect.poll(async () => (await endpoint(page, 'focus')).utf16).toBe(21);
    expect(await endpoint(page, 'anchor')).toEqual(fixed);
    await expect(root).toHaveAttribute('data-layout-revision', revision!);
    await expect(input(page)).toHaveText('alpha beta gamma delta');
  } finally { await client.detach(); }
});

test('native horizontal panning remains available after pinch while preserving the selected range', async ({ page, context }) => {
  await start(page);
  const client = await context.newCDPSession(page);
  try {
    await pinchSelectedMiddleWord(page, client);
    const fixedAnchor = await endpoint(page, 'anchor');
    const fixedFocus = await endpoint(page, 'focus');
    const viewport = await page.evaluate(() => ({ left: visualViewport!.offsetLeft,
      width: visualViewport!.width, height: visualViewport!.height }));
    const point = { x: viewport.width * 0.75, y: viewport.height * 0.7 };
    await client.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ ...point, id: 3 }] });
    for (let step = 1; step <= 10; step += 1) {
      await client.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: [{ x: point.x - step * 7, y: point.y, id: 3 }] });
      await page.waitForTimeout(30);
    }
    await client.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
    await expect.poll(() => page.evaluate(() => visualViewport?.offsetLeft ?? 0)).toBeGreaterThan(viewport.left + 8);
    expect(await endpoint(page, 'anchor')).toEqual(fixedAnchor);
    expect(await endpoint(page, 'focus')).toEqual(fixedFocus);
    await expect(input(page)).toHaveText('alpha beta gamma delta');
  } finally { await client.detach(); }
});

test('keyboard viewport changes retain endpoints and composition prevents gesture selection', async ({ page, context }) => {
  await page.addInitScript(() => {
    const viewport = Object.assign(new EventTarget(), { offsetTop: 0, offsetLeft: 0, width: 390, height: 844, scale: 1 });
    Object.defineProperty(window, 'visualViewport', { configurable: true, value: viewport });
  });
  await start(page);
  const client = await context.newCDPSession(page);
  try {
    await longPress(page, client, await glyphPoint(page, 0, 1));
    const anchor = await endpoint(page, 'anchor');
    const focus = await endpoint(page, 'focus');
    await page.evaluate(() => {
      Object.assign(window.visualViewport!, { height: 400, offsetTop: 55 });
      window.visualViewport!.dispatchEvent(new Event('resize'));
      window.visualViewport!.dispatchEvent(new Event('scroll'));
    });
    expect(await endpoint(page, 'anchor')).toEqual(anchor);
    expect(await endpoint(page, 'focus')).toEqual(focus);
    await actions(page).getByRole('button', { name: 'Edit selected text', exact: true }).tap();
    await expect(input(page)).toBeFocused();
    await input(page).evaluate((element) => {
      element.dispatchEvent(new CompositionEvent('compositionstart', { bubbles: true, data: '' }));
      element.dispatchEvent(new CompositionEvent('compositionupdate', { bubbles: true, data: '未確定' }));
    });
    await expect(handle(page, 'anchor')).toHaveCount(0);
    await expect(actions(page)).toHaveCount(0);
    const point = await glyphPoint(page, 0, 7);
    await client.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ ...point, id: 1 }] });
    await page.waitForTimeout(700);
    await client.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
    await input(page).evaluate((element) => element.dispatchEvent(new CompositionEvent('compositionend', { bubbles: true, data: '' })));
    await expect(input(page)).toHaveText('alpha beta gamma delta');
    await page.evaluate(() => {
      Object.assign(window.visualViewport!, { height: 844, offsetTop: 0 });
      window.visualViewport!.dispatchEvent(new Event('resize'));
    });
    await readyPages(page);
    await expect(page.getByRole('button', { name: 'Save', exact: true })).toBeEnabled();
  } finally { await client.detach(); }
});

test('touch clipboard commands cut and paste selected Unicode through Rust undo history', async ({ page, context }) => {
  await context.grantPermissions(['clipboard-read', 'clipboard-write']);
  const text = 'Cafe\u0301 alpha beta';
  await start(page, text);
  const client = await context.newCDPSession(page);
  try {
    await longPress(page, client, await glyphPoint(page, 0, 1));
    await actions(page).getByRole('button', { name: 'Cut', exact: true }).tap();
    await expect(input(page)).toHaveText(' alpha beta');
    expect(await page.evaluate(() => navigator.clipboard.readText())).toBe('Cafe\u0301');
    await page.getByRole('button', { name: 'Undo', exact: true }).tap();
    await expect(input(page)).toHaveText(text);
    await page.evaluate(() => navigator.clipboard.writeText('Pasted 👩🏽‍💻'));
    await longPress(page, client, await glyphPoint(page, 0, 1));
    await actions(page).getByRole('button', { name: 'Paste', exact: true }).tap();
    await expect(input(page)).toHaveText('Pasted 👩🏽‍💻 alpha beta');
    await page.getByRole('button', { name: 'Undo', exact: true }).tap();
    await expect(input(page)).toHaveText(text);
  } finally { await client.detach(); }
});

test('clipboard denial preserves selected content, saved bytes and undo history', async ({ page, context }) => {
  await page.addInitScript(() => {
    Object.defineProperty(navigator, 'clipboard', { configurable: true, value: {
      writeText: async () => { throw new DOMException('Clipboard denied', 'NotAllowedError'); },
      readText: async () => { throw new DOMException('Clipboard denied', 'NotAllowedError'); },
    } });
  });
  await start(page);
  const stored = await records(page);
  const client = await context.newCDPSession(page);
  try {
    await longPress(page, client, await glyphPoint(page, 0, 1));
    const before = await endpoint(page, 'focus');
    await actions(page).getByRole('button', { name: 'Cut', exact: true }).tap();
    await expect(page.getByRole('alert')).toContainText(/clipboard|copy|denied/i);
    await expect(input(page)).toHaveText('alpha beta gamma delta');
    expect(await endpoint(page, 'focus')).toEqual(before);
    expect(await records(page)).toEqual(stored);
    await actions(page).getByRole('button', { name: 'Paste', exact: true }).tap();
    await expect(input(page)).toHaveText('alpha beta gamma delta');
    await focusEditor(page);
    await page.keyboard.press('Control+z');
    await expect(input(page)).toHaveText('');
  } finally { await client.detach(); }
});

for (const operation of ['Cut', 'Paste'] as const) {
  test(`a pending ${operation.toLowerCase()} cannot mutate a different selection at the same Rust revision`, async ({ page, context }) => {
    await page.addInitScript((pendingOperation) => {
      type PendingWindow = Window & { finishSelectionClipboard?: (value?: string) => void };
      const state = window as PendingWindow;
      Object.defineProperty(navigator, 'clipboard', { configurable: true, value: {
        writeText: () => pendingOperation === 'Cut'
          ? new Promise<void>((resolve) => { state.finishSelectionClipboard = () => resolve(); }) : Promise.resolve(),
        readText: () => pendingOperation === 'Paste'
          ? new Promise<string>((resolve) => { state.finishSelectionClipboard = (value) => resolve(value ?? 'Stale replacement'); }) : Promise.resolve(''),
      } });
    }, operation);
    await start(page);
    const root = await readyPages(page);
    const revision = await root.getAttribute('data-layout-revision');
    const client = await context.newCDPSession(page);
    try {
      await longPress(page, client, await glyphPoint(page, 0, 1));
      await actions(page).getByRole('button', { name: operation, exact: true }).tap();
      await expect.poll(() => page.evaluate(() => typeof (window as Window & { finishSelectionClipboard?: unknown }).finishSelectionClipboard)).toBe('function');
      await longPress(page, client, await glyphPoint(page, 0, 7));
      const anchor = await endpoint(page, 'anchor');
      const focus = await endpoint(page, 'focus');
      expect(anchor.utf16).toBe(6);
      expect(focus.utf16).toBe(10);
      await page.evaluate(() => (window as Window & { finishSelectionClipboard?: (value?: string) => void }).finishSelectionClipboard?.('Stale replacement'));
      await expect(actions(page).getByRole('button', { name: operation, exact: true })).toBeEnabled();
      await expect(input(page)).toHaveText('alpha beta gamma delta');
      expect(await endpoint(page, 'anchor')).toEqual(anchor);
      expect(await endpoint(page, 'focus')).toEqual(focus);
      await expect(root).toHaveAttribute('data-layout-revision', revision!);
    } finally { await client.detach(); }
  });
}
