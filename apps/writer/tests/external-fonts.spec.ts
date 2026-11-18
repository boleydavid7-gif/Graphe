import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { readFile } from 'node:fs/promises';
import { expect, test, type Page } from '@playwright/test';
import { documentAction, focusEditor, readyPages } from './step7-test-helpers';

// These tests use production Rust/WASM, actual HTTP requests and real browser
// CacheStorage. Blocking font requests does not simulate an offline app shell.
interface PinnedFont { readonly file: string; readonly kind: string; readonly byte_length: number; readonly sha256: string }
const fontRoot = new URL('../../../crates/office-layout/fonts/', import.meta.url);
const manifestBytes = readFileSync(new URL('manifest.json', fontRoot));
const manifest = JSON.parse(manifestBytes.toString('utf8')) as {
  readonly catalog_id: string; readonly entries: readonly PinnedFont[];
};
const fonts = manifest.entries.filter((entry) => entry.kind === 'font');
const manifestDigest = createHash('sha256').update(manifestBytes).digest('hex');
const prefix = `/fonts/${manifest.catalog_id}/${manifestDigest}/`;
const cacheName = `graphe-fonts-v1-${manifestDigest}`;
const firstFont = fonts[0];
if (!firstFont || fonts.length !== 42) throw new Error('The pinned test font catalog must contain exactly 42 faces');
const testFont = firstFont;
const editor = (page: Page) => page.getByRole('textbox', { name: 'Document content', exact: true });
const isFont = (url: URL) => /\.(?:ttf|otf)$/.test(url.pathname);
const fontId = (url: string) => new URL(url).pathname.slice(prefix.length);
const unicode = 'Café e\u0301 👩🏽‍💻 العربية नमस्ते שלום';

async function start(page: Page) {
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Save', exact: true })).toBeEnabled({ timeout: 30_000 });
  await readyPages(page);
}

async function cacheState(page: Page) {
  return page.evaluate(async ({ name, path }) => {
    const cache = await caches.open(name);
    const keys = await cache.keys();
    const complete = await cache.match(new URL(path + '.complete', location.origin).href);
    return { files: keys.map((request) => new URL(request.url).pathname).filter((key) => /\.(?:ttf|otf)$/.test(key)),
      complete: complete ? await complete.json() as { catalogId: string; manifestSha256: string; fontCount: number } : null };
  }, { name: cacheName, path: prefix });
}

async function showTypefaceSample(page: Page) {
  await page.locator('#font-family').selectOption('Lora');
  await page.locator('.inspector-toggle').click();
  await expect(page.getByLabel('Typeface sample', { exact: true })).toContainText('Aa Bb Cc');
  const loaded = await page.evaluate(async () => {
    const faces = await document.fonts.load('16px "Lora"');
    return faces.some((face) => face.status === 'loaded');
  });
  expect(loaded).toBe(true);
}

async function saveLocal(page: Page) {
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(page.locator('.save-state')).toHaveText('Saved on this device', { timeout: 10_000 });
}

async function downloadCopy(page: Page, format: 'writer' | 'pdf') {
  await documentAction(page, 'Save As…');
  const dialog = page.getByRole('dialog', { name: 'Save As', exact: true });
  await dialog.getByRole('combobox', { name: 'File format', exact: true }).selectOption(format);
  const save = dialog.getByRole('button', { name: 'Save copy', exact: true });
  await expect(save).toBeEnabled();
  const pending = page.waitForEvent('download');
  await save.click();
  const download = await pending;
  expect(download.suggestedFilename()).toMatch(new RegExp(`\\.${format}$`, 'i'));
  const path = await download.path();
  if (!path) throw new Error(`The ${format} copy was not retained by the browser`);
  return readFile(path);
}

/** Compare supplied Rust glyph paths/positions, never browser text measurement. */
async function vectors(page: Page) {
  const root = await readyPages(page);
  return root.locator('svg[data-layout-page]').evaluateAll((pages) => pages.map((svg) => ({
    viewBox: svg.getAttribute('viewBox'),
    glyphs: [...svg.querySelectorAll('use')].map((glyph) => ({
      outline: document.getElementById((glyph.getAttribute('href') ?? '').slice(1))?.getAttribute('d'),
      transform: glyph.getAttribute('transform'), fill: glyph.getAttribute('fill'),
    })),
    lines: [...svg.querySelectorAll('[data-layout-line]')].map((line) => ({
      paragraph: line.getAttribute('data-layout-paragraph-id'), start: line.getAttribute('data-layout-utf16-start'),
      end: line.getAttribute('data-layout-utf16-end'),
    })),
  })));
}

test('cold startup fetches all 42 pinned fonts once, completes the verified cache and reuses bytes for previews', async ({ page }) => {
  const requests: string[] = [];
  const errors: string[] = [];
  page.on('request', (request) => { if (isFont(new URL(request.url()))) requests.push(request.url()); });
  page.on('pageerror', (error) => errors.push(error.message));
  await start(page);
  expect(requests).toHaveLength(42);
  expect(new Set(requests).size).toBe(42);
  expect(requests.every((url) => new URL(url).pathname.startsWith(prefix))).toBe(true);
  expect(requests.map(fontId).sort()).toEqual(fonts.map((font) => font.file).sort());
  const cached = await cacheState(page);
  expect(cached.files.sort()).toEqual(fonts.map((font) => prefix + font.file).sort());
  expect(cached.complete).toEqual({ catalogId: manifest.catalog_id, manifestSha256: manifestDigest, fontCount: 42 });
  await showTypefaceSample(page);
  expect(requests).toHaveLength(42);
  expect(errors).toEqual([]);
});

test('a warmed font cache starts and previews without font network access while the app shell remains online', async ({ page }) => {
  await start(page);
  const attempts: string[] = [];
  await page.route(isFont, async (route) => {
    attempts.push(route.request().url());
    await route.abort('internetdisconnected');
  });
  await page.reload();
  await expect(page.getByRole('button', { name: 'Save', exact: true })).toBeEnabled({ timeout: 30_000 });
  await readyPages(page);
  await showTypefaceSample(page);
  await focusEditor(page);
  await page.keyboard.insertText(`Verified cached fonts ${unicode}`);
  await expect(editor(page)).toContainText(`Verified cached fonts ${unicode}`);
  expect(attempts).toEqual([]);
});

for (const corruption of ['length', 'digest'] as const) {
  test(`a cached font with a wrong ${corruption} is revalidated and repaired from its pinned URL`, async ({ page }) => {
    await start(page);
    await page.evaluate(async ({ name, path, kind }) => {
      const cache = await caches.open(name);
      const url = new URL(path, location.origin).href;
      const response = await cache.match(url);
      if (!response) throw new Error('Missing cached font for the integrity regression');
      const bytes = kind === 'length' ? new Uint8Array([1, 2, 3]) : new Uint8Array(await response.arrayBuffer());
      if (kind === 'digest') bytes[0] = (bytes[0] ?? 0) ^ 1;
      await cache.put(url, new Response(bytes, { headers: { 'Content-Type': 'font/ttf' } }));
    }, { name: cacheName, path: prefix + testFont.file, kind: corruption });
    const requests: string[] = [];
    page.on('request', (request) => { if (isFont(new URL(request.url()))) requests.push(request.url()); });
    await page.reload();
    await expect(page.getByRole('button', { name: 'Save', exact: true })).toBeEnabled({ timeout: 30_000 });
    await readyPages(page);
    expect(requests.map(fontId)).toEqual([testFont.file]);
    const repairedDigest = await page.evaluate(async ({ name, path }) => {
      const response = await (await caches.open(name)).match(new URL(path, location.origin).href);
      if (!response) throw new Error('The repaired cache entry is missing');
      const digest = await crypto.subtle.digest('SHA-256', await response.arrayBuffer());
      return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, '0')).join('');
    }, { name: cacheName, path: prefix + testFont.file });
    expect(repairedDigest).toBe(testFont.sha256);
  });
}

test('an incomplete catalog cannot enable editing, and in-place retry reuses valid cached fonts without deleting older versions', async ({ page }) => {
  test.setTimeout(90_000);
  let releaseFirst: () => void = () => undefined;
  const held = new Promise<void>((resolve) => { releaseFirst = resolve; });
  let fail = true;
  let attempts = 0;
  const requests: string[] = [];
  page.on('request', (request) => { if (isFont(new URL(request.url()))) requests.push(request.url()); });
  await page.route((url) => url.pathname === prefix + testFont.file, async (route) => {
    attempts += 1;
    if (attempts === 1) await held;
    if (fail) await route.abort('failed');
    else await route.continue();
  });
  try {
    await page.goto('/', { waitUntil: 'domcontentloaded' });
    await expect(page.getByText(/Loading verified fonts.*41\s*\/\s*42/i)).toBeVisible({ timeout: 30_000 });
    await expect(page.getByRole('button', { name: 'Save', exact: true })).toBeDisabled();
    await expect(editor(page)).toHaveCount(0);
    const partial = await cacheState(page);
    expect(partial.files).toHaveLength(41);
    expect(partial.complete).toBeNull();
    await page.evaluate(async () => {
      await (await caches.open('graphe-fonts-v1-older-catalog-regression')).put(
        new URL('/fonts/older-catalog/sentinel', location.origin).href, new Response('Keep the previous catalog'));
    });
    releaseFirst();
    await expect(page.getByRole('alert')).toContainText('Graphe could not start');
    expect(attempts).toBe(2);
    expect((await cacheState(page)).complete).toBeNull();
    const beforeRetry = requests.length;
    let navigations = 0;
    page.on('framenavigated', (frame) => { if (frame === page.mainFrame()) navigations += 1; });
    fail = false;
    await page.getByRole('button', { name: 'Try again', exact: true }).click();
    await expect(page.getByRole('button', { name: 'Save', exact: true })).toBeEnabled({ timeout: 30_000 });
    await readyPages(page);
    expect(requests.slice(beforeRetry).map(fontId)).toEqual([testFont.file]);
    expect(navigations).toBe(0);
    expect((await cacheState(page)).complete?.fontCount).toBe(42);
    const older = await page.evaluate(async () => {
      const response = await (await caches.open('graphe-fonts-v1-older-catalog-regression')).match(
        new URL('/fonts/older-catalog/sentinel', location.origin).href);
      return response?.text();
    });
    expect(older).toBe('Keep the previous catalog');
  } finally { releaseFirst(); }
});

for (const badResponse of ['wrong-digest', 'wrong-length', 'html-login'] as const) {
  test(`a ${badResponse} font response cannot register or enable an editable partial catalog`, async ({ page }) => {
    const original = await readFile(new URL(testFont.file, fontRoot));
    const body = badResponse === 'wrong-digest' ? Buffer.from(original)
      : badResponse === 'wrong-length' ? original.subarray(0, original.length - 1)
      : Buffer.from('<html><body>Sign in to access this resource</body></html>');
    if (badResponse === 'wrong-digest') body[0] = (body[0] ?? 0) ^ 1;
    let attempts = 0;
    await page.route((url) => url.pathname === prefix + testFont.file, async (route) => {
      attempts += 1;
      await route.fulfill({ status: 200, body,
        contentType: badResponse === 'html-login' ? 'text/html' : 'font/ttf' });
    });
    await page.goto('/');
    await expect(page.getByRole('alert')).toContainText('Graphe could not start', { timeout: 30_000 });
    await expect(page.getByRole('alert')).toContainText(/font.*(length|integrity|incomplete)/i);
    await expect(page.getByRole('button', { name: 'Save', exact: true })).toBeDisabled();
    await expect(editor(page)).toHaveCount(0);
    expect(attempts).toBe(2);
    const cached = await cacheState(page);
    expect(cached.files).not.toContain(prefix + testFont.file);
    expect(cached.complete).toBeNull();
  });
}

test('a font redirect is rejected without following an external location', async ({ page }) => {
  let external = 0;
  await page.route('https://untrusted-font-source.invalid/**', async (route) => {
    external += 1;
    await route.abort();
  });
  await page.route((url) => url.pathname === prefix + testFont.file, async (route) => {
    await route.fulfill({ status: 302, headers: { Location: 'https://untrusted-font-source.invalid/font.ttf' } });
  });
  await page.goto('/');
  await expect(page.getByRole('alert')).toContainText('Graphe could not start', { timeout: 30_000 });
  await expect(page.getByRole('button', { name: 'Save', exact: true })).toBeDisabled();
  await expect(editor(page)).toHaveCount(0);
  expect(external).toBe(0);
  expect((await cacheState(page)).complete).toBeNull();
});

for (const failure of ['cache-denied', 'cache-quota'] as const) {
  test(`${failure} keeps verified online editing and committed document saving available with an offline warning`, async ({ page }) => {
    await page.addInitScript((mode) => {
      if (mode === 'cache-denied') CacheStorage.prototype.open = () =>
        Promise.reject(new DOMException('Cache access denied by test', 'SecurityError'));
      else Cache.prototype.put = () => Promise.reject(new DOMException('Cache quota exhausted by test', 'QuotaExceededError'));
    }, failure);
    await start(page);
    await expect(page.getByText('Font caching is unavailable. An internet connection may be needed next time.', { exact: true })).toBeVisible();
    await focusEditor(page);
    await page.keyboard.insertText(`Keep this document when font caching fails ${unicode}`);
    await saveLocal(page);
    await expect(editor(page)).toContainText(`Keep this document when font caching fails ${unicode}`);
    expect((await downloadCopy(page, 'writer')).toString('utf8')).toContain('Keep this document when font caching fails');
  });
}

test('cached font startup preserves native document identity, Unicode page geometry and deterministic PDF output after reopen', async ({ page }) => {
  test.setTimeout(90_000);
  await start(page);
  const title = 'External fonts save and reopen regression';
  await page.getByRole('textbox', { name: 'Document title', exact: true }).fill(title);
  await focusEditor(page);
  const text = Array.from({ length: 35 }, (_, index) => `Paragraph ${index + 1}: ${unicode} — pinned metrics and outlines.`).join('\n');
  await page.keyboard.insertText(text);
  await saveLocal(page);
  const before = await vectors(page);
  expect(before.length).toBeGreaterThan(1);
  const native = await downloadCopy(page, 'writer');
  const pdf = await downloadCopy(page, 'pdf');
  expect(pdf.subarray(0, 5).toString()).toBe('%PDF-');
  const attempts: string[] = [];
  await page.route(isFont, async (route) => { attempts.push(route.request().url()); await route.abort('internetdisconnected'); });
  await page.reload();
  await expect(page.getByRole('button', { name: 'Save', exact: true })).toBeEnabled({ timeout: 30_000 });
  await documentAction(page, 'Open document');
  const library = page.getByRole('dialog', { name: 'Open document', exact: true });
  await library.getByRole('button', { name: new RegExp(title) }).click();
  await expect(library).toHaveCount(0);
  await expect(editor(page)).toContainText('Paragraph 35');
  expect(await vectors(page)).toEqual(before);
  expect(await downloadCopy(page, 'writer')).toEqual(native);
  expect(await downloadCopy(page, 'pdf')).toEqual(pdf);
  expect(attempts).toEqual([]);
});
