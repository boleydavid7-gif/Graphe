import { webcrypto } from 'node:crypto';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { loadFontCatalog, type FontCatalogManifest, type FontLoadProgress } from './font-loader';

const origin = 'https://graphe.test';
const digest = 'a'.repeat(64);
const cacheName = `graphe-fonts-v1-${digest}`;
const prefix = `${origin}/fonts/writer-fonts-v6/${digest}/`;
const bytes = Array.from({ length: 42 }, (_, i) => new TextEncoder().encode(`Pinned face bytes ${i}`));
let manifest: FontCatalogManifest;

class TestCache {
  entries = new Map<string, Response>();
  removed: string[] = [];
  match = vi.fn(async (url: string) => this.entries.get(url)?.clone());
  put = vi.fn(async (url: string, response: Response) => { this.entries.set(url, response.clone()); });
  delete = vi.fn(async (url: string) => { this.removed.push(url); return this.entries.delete(url); });
}

let cache: TestCache;
let fetcher: ReturnType<typeof vi.fn<typeof fetch>>;
let openCache: ReturnType<typeof vi.fn>;

function networkResponse(url: string, body: Uint8Array = fontBytes(url), headers?: HeadersInit): Response {
  const response = new Response(body.slice(), { status: 200, headers });
  Object.defineProperties(response, { type: { value: 'basic', configurable: true }, url: { value: url, configurable: true } });
  return response;
}

function fontBytes(url: string): Uint8Array<ArrayBuffer> {
  const index = Number(url.match(/Face-(\d+)\.ttf$/)?.[1]);
  const value = bytes[index];
  if (!value) throw new Error(`Unknown font URL ${url}`);
  return value;
}

function options() {
  return { registerFont: vi.fn(), finalizeRegistration: vi.fn(), onProgress: vi.fn<(value: FontLoadProgress) => void>() };
}

beforeEach(async () => {
  const fonts = await Promise.all(bytes.map(async (value, i) => ({
    id: `Face-${i}.ttf`, byteLength: value.byteLength,
    sha256: Array.from(new Uint8Array(await webcrypto.subtle.digest('SHA-256', value.buffer)),
      (byte) => byte.toString(16).padStart(2, '0')).join(''),
  })));
  manifest = { catalogId: 'writer-fonts-v6', manifestSha256: digest, fonts,
    totalBytes: bytes.reduce((sum, value) => sum + value.byteLength, 0) };
  cache = new TestCache();
  openCache = vi.fn(async () => cache);
  fetcher = vi.fn<typeof fetch>(async (input) => networkResponse(String(input)));
  vi.stubGlobal('crypto', webcrypto);
  vi.stubGlobal('location', { origin });
  vi.stubGlobal('caches', { open: openCache });
  vi.stubGlobal('fetch', fetcher);
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe('pinned font loader', () => {
  it('verifies all 42 faces before registration and marks complete only after Rust finalization', async () => {
    const callbacks = options();
    callbacks.registerFont.mockImplementation(() => {
      expect(fetcher).toHaveBeenCalledTimes(42);
      expect(cache.entries.has(prefix + '.complete')).toBe(false);
    });
    callbacks.finalizeRegistration.mockImplementation(() => {
      expect(callbacks.registerFont).toHaveBeenCalledTimes(42);
      expect(cache.entries.has(prefix + '.complete')).toBe(false);
    });
    await expect(loadFontCatalog(manifest, callbacks)).resolves.toEqual({ cacheAvailable: true, warnings: [] });
    expect(openCache).toHaveBeenCalledWith(cacheName);
    expect(cache.entries).toHaveLength(43);
    expect(callbacks.onProgress).toHaveBeenLastCalledWith({ phase: 'ready', loaded: 42, total: 42, cacheAvailable: true });
    expect(callbacks.registerFont.mock.calls[7]).toEqual(['Face-7.ttf', bytes[7]]);
    for (const [url, init] of fetcher.mock.calls) {
      expect(String(url)).toMatch(new RegExp(`^${prefix}Face-\\d+\\.ttf$`));
      expect(init).toMatchObject({ mode: 'same-origin', credentials: 'same-origin', redirect: 'error', cache: 'no-store' });
    }
  });

  it('limits network concurrency to four faces', async () => {
    let active = 0;
    let maximum = 0;
    fetcher.mockImplementation(async (url) => {
      active += 1; maximum = Math.max(maximum, active);
      await new Promise((resolve) => setTimeout(resolve, 1));
      active -= 1;
      return networkResponse(String(url));
    });
    await loadFontCatalog(manifest, options());
    expect(maximum).toBe(4);
  });

  it('loads offline from synthetic cached responses after rechecking every digest', async () => {
    await loadFontCatalog(manifest, options());
    fetcher.mockReset().mockRejectedValue(new TypeError('offline'));
    const callbacks = options();
    await expect(loadFontCatalog(manifest, callbacks)).resolves.toEqual({ cacheAvailable: true, warnings: [] });
    expect(fetcher).not.toHaveBeenCalled();
    expect(callbacks.registerFont).toHaveBeenCalledTimes(42);
    expect(cache.match).toHaveBeenCalledTimes(84);
    expect(cache.entries.get(prefix + 'Face-0.ttf')?.type).toBe('default');
  });

  it.each(['length', 'digest'] as const)('removes only a corrupt cached %s entry and refetches it', async (problem) => {
    await loadFontCatalog(manifest, options());
    const wrong = fontBytes(prefix + 'Face-7.ttf').slice();
    wrong[0] = 0;
    cache.entries.set(prefix + 'Face-7.ttf', new Response(problem === 'length' ? wrong.slice(1) : wrong));
    fetcher.mockClear();
    await loadFontCatalog(manifest, options());
    expect(fetcher).toHaveBeenCalledTimes(1);
    expect(fetcher.mock.calls[0]?.[0]).toBe(prefix + 'Face-7.ttf');
    expect(cache.removed).toEqual([prefix + 'Face-7.ttf']);
  });

  it('retains verified partial entries after failure and downloads only missing faces on retry', async () => {
    fetcher.mockImplementation(async (url) => {
      if (String(url).endsWith('Face-9.ttf')) return new Response('unavailable', { status: 503 });
      return networkResponse(String(url));
    });
    const failed = options();
    await expect(loadFontCatalog(manifest, failed)).rejects.toMatchObject({ code: 'network' });
    expect(failed.registerFont).not.toHaveBeenCalled();
    expect(failed.finalizeRegistration).not.toHaveBeenCalled();
    expect(cache.entries.has(prefix + '.complete')).toBe(false);
    const retained = new Set(cache.entries.keys());
    expect(retained.size).toBeGreaterThan(0);
    fetcher.mockReset().mockImplementation(async (url) => networkResponse(String(url)));
    await loadFontCatalog(manifest, options());
    expect(fetcher).toHaveBeenCalledTimes(42 - retained.size);
    for (const [url] of fetcher.mock.calls) expect(retained.has(String(url))).toBe(false);
  });

  it('never trusts an old completion marker when a face has been evicted', async () => {
    await loadFontCatalog(manifest, options());
    cache.entries.delete(prefix + 'Face-21.ttf');
    fetcher.mockClear();
    await loadFontCatalog(manifest, options());
    expect(fetcher).toHaveBeenCalledTimes(1);
    expect(fetcher.mock.calls[0]?.[0]).toBe(prefix + 'Face-21.ttf');
  });

  it('preserves other applications and the last working catalog when a new version fails', async () => {
    const previous = new TestCache();
    previous.entries.set('https://graphe.test/previous.complete', new Response('last working catalog'));
    const removeCache = vi.fn();
    vi.stubGlobal('caches', { open: openCache, delete: removeCache });
    fetcher.mockRejectedValue(new TypeError('offline'));
    await expect(loadFontCatalog(manifest, options())).rejects.toMatchObject({ code: 'network' });
    expect(removeCache).not.toHaveBeenCalled();
    expect(previous.entries.size).toBe(1);
    expect(await previous.entries.get('https://graphe.test/previous.complete')?.text()).toBe('last working catalog');
  });

  it('does not mark complete or ready when registration or Rust finalization fails', async () => {
    const callbacks = options();
    callbacks.finalizeRegistration.mockRejectedValueOnce(new Error('catalog incomplete'));
    await expect(loadFontCatalog(manifest, callbacks)).rejects.toMatchObject({ code: 'registration', message: 'catalog incomplete' });
    expect(cache.entries.has(prefix + '.complete')).toBe(false);
    expect(callbacks.onProgress.mock.calls.some(([value]) => value.phase === 'ready')).toBe(false);
    fetcher.mockClear();
    await loadFontCatalog(manifest, options());
    expect(fetcher).not.toHaveBeenCalled();
  });

  it('awaits cache put completion before publishing progress, registration or ready', async () => {
    let release: (() => void) | undefined;
    const gate = new Promise<void>((resolve) => { release = resolve; });
    const normalPut = cache.put.getMockImplementation()!;
    cache.put.mockImplementation(async (url, response) => {
      if (url.endsWith('Face-41.ttf')) await gate;
      await normalPut(url, response);
    });
    const callbacks = options();
    const pending = loadFontCatalog(manifest, callbacks);
    await vi.waitFor(() => expect(fetcher).toHaveBeenCalledTimes(42));
    expect(callbacks.registerFont).not.toHaveBeenCalled();
    expect(cache.entries.has(prefix + '.complete')).toBe(false);
    release?.();
    await pending;
    expect(callbacks.finalizeRegistration).toHaveBeenCalledOnce();
  });

  it.each(['unavailable', 'quota'] as const)('permits verified network loading with a warning when cache is %s', async (problem) => {
    if (problem === 'unavailable') openCache.mockRejectedValue(new DOMException('blocked', 'SecurityError'));
    else cache.put.mockRejectedValue(new DOMException('full', 'QuotaExceededError'));
    const callbacks = options();
    const result = await loadFontCatalog(manifest, callbacks);
    expect(result.cacheAvailable).toBe(false);
    expect(result.warnings).toEqual(['Font caching is unavailable. An internet connection may be needed next time.']);
    expect(callbacks.registerFont).toHaveBeenCalledTimes(42);
    expect(callbacks.onProgress).toHaveBeenLastCalledWith(expect.objectContaining({ phase: 'ready', cacheAvailable: false, warning: result.warnings[0] }));
  });

  it('accepts compressed transfer lengths while checking the decoded body length and digest', async () => {
    fetcher.mockImplementation(async (url) => networkResponse(String(url), fontBytes(String(url)), {
      'Content-Encoding': 'br', 'Content-Length': '5',
    }));
    await loadFontCatalog(manifest, options());
    const stored = cache.entries.get(prefix + 'Face-0.ttf');
    expect(stored?.headers.has('Content-Encoding')).toBe(false);
    expect(stored?.headers.get('Content-Length')).toBe(String(bytes[0]?.byteLength));
  });

  it('rejects excessive streamed bytes even without a Content-Length header', async () => {
    let cancelled = 0;
    fetcher.mockImplementation(async (url) => {
      const stream = new ReadableStream<Uint8Array>({
        start(controller) { controller.enqueue(new Uint8Array(fontBytes(String(url)).length + 1)); },
        cancel() { cancelled += 1; },
      });
      const response = new Response(stream);
      Object.defineProperties(response, { type: { value: 'basic' }, url: { value: String(url) } });
      return response;
    });
    const callbacks = options();
    await expect(loadFontCatalog(manifest, callbacks)).rejects.toMatchObject({ code: 'integrity' });
    expect(cancelled).toBeGreaterThan(0);
    expect(callbacks.registerFont).not.toHaveBeenCalled();
    expect(cache.entries.size).toBe(0);
  });

  it.each(['digest', 'redirect', 'url', 'opaque', 'status', 'html'] as const)('rejects unsafe %s network responses with no partial activation', async (problem) => {
    fetcher.mockImplementation(async (url) => {
      const bad = fontBytes(String(url)).slice(); bad[0] = 0;
      const response = networkResponse(String(url), problem === 'digest' ? bad
        : problem === 'html' ? new TextEncoder().encode('<html>Access denied</html>') : undefined);
      if (problem === 'redirect') Object.defineProperty(response, 'redirected', { value: true });
      if (problem === 'url') Object.defineProperty(response, 'url', { value: 'https://external.test/font.ttf' });
      if (problem === 'opaque') Object.defineProperty(response, 'type', { value: 'opaque' });
      if (problem === 'status') Object.defineProperty(response, 'status', { value: 206 });
      return response;
    });
    const callbacks = options();
    await expect(loadFontCatalog(manifest, callbacks)).rejects.toMatchObject({ code: ['digest', 'html'].includes(problem) ? 'integrity' : 'network' });
    expect(callbacks.registerFont).not.toHaveBeenCalled();
    expect(callbacks.finalizeRegistration).not.toHaveBeenCalled();
    expect(cache.entries.size).toBe(0);
    expect(fetcher.mock.calls.filter(([url]) => String(url).endsWith('Face-0.ttf'))).toHaveLength(2);
  });

  it('bounds retries and rejects stalled network requests rather than remaining pending', async () => {
    vi.useFakeTimers();
    fetcher.mockImplementation(() => new Promise<Response>(() => undefined));
    const callbacks = options();
    const pending = loadFontCatalog(manifest, { ...callbacks, timeoutMs: 10 });
    const rejection = expect(pending).rejects.toMatchObject({ code: 'timeout' });
    await vi.advanceTimersByTimeAsync(30);
    await rejection;
    expect(fetcher).toHaveBeenCalledTimes(8);
    expect(callbacks.registerFont).not.toHaveBeenCalled();
  });

  it('aborts and drains outstanding workers without late requests or progress', async () => {
    fetcher.mockImplementation(() => new Promise<Response>(() => undefined));
    const controller = new AbortController();
    const callbacks = options();
    const pending = loadFontCatalog(manifest, { ...callbacks, signal: controller.signal });
    const rejection = expect(pending).rejects.toMatchObject({ code: 'aborted' });
    await vi.waitFor(() => expect(fetcher).toHaveBeenCalledTimes(4));
    controller.abort();
    await rejection;
    const calls = callbacks.onProgress.mock.calls.length;
    await new Promise((resolve) => setTimeout(resolve, 5));
    expect(callbacks.onProgress).toHaveBeenCalledTimes(calls);
    expect(fetcher).toHaveBeenCalledTimes(4);
    expect(cache.entries.has(prefix + '.complete')).toBe(false);
  });

  it('rejects malformed manifests before opening caches or requesting URLs', async () => {
    for (const change of [
      { catalogId: '../../other' }, { manifestSha256: 'missing' }, { totalBytes: 1 },
      { fonts: manifest.fonts.slice(1) },
      { fonts: manifest.fonts.map((font, index) => index ? font : { ...font, id: '../outside.ttf' }) },
      { fonts: manifest.fonts.map((font, index) => index ? font : { ...font, byteLength: -1 }) },
      { fonts: manifest.fonts.map((font, index) => index ? font : { ...font, sha256: 'bad' }) },
      { fonts: manifest.fonts.map((font, index) => index ? font : manifest.fonts[1]!) },
    ]) {
      await expect(loadFontCatalog({ ...manifest, ...change }, options())).rejects.toMatchObject({ code: 'invalid_manifest' });
    }
    expect(openCache).not.toHaveBeenCalled();
    expect(fetcher).not.toHaveBeenCalled();
  });
});
