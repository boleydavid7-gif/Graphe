/** Pinned Rust catalog metadata; this is not a browser font-selection model. */
export interface FontCatalogManifest {
  catalogId: string;
  manifestSha256: string;
  fonts: readonly { id: string; byteLength: number; sha256: string }[];
  totalBytes: number;
}

export interface FontLoadProgress {
  phase: 'loading' | 'registering' | 'ready';
  loaded: number;
  total: number;
  cacheAvailable: boolean;
  warning?: string;
}

export interface FontLoadResult {
  cacheAvailable: boolean;
  warnings: readonly string[];
}

interface FontLoadOptions {
  registerFont: (id: string, bytes: Uint8Array) => void | Promise<void>;
  /** Rust must validate the complete catalog before any ready state/marker. */
  finalizeRegistration: () => void | Promise<void>;
  onProgress?: (progress: FontLoadProgress) => void;
  signal?: AbortSignal;
  /** Bounded override for tests or a shorter application request deadline. */
  timeoutMs?: number;
}

type FontMetadata = FontCatalogManifest['fonts'][number];
type FontFailureCode = 'invalid_manifest' | 'integrity' | 'network' | 'timeout' | 'aborted' | 'registration';

export class FontLoadError extends Error {
  constructor(readonly code: FontFailureCode, message: string, readonly fontId?: string) {
    super(message);
    this.name = 'FontLoadError';
  }
}

const FONT_COUNT = 42;
const CONCURRENCY = 4;
const REQUEST_TIMEOUT = 30_000;
const CACHE_WARNING = 'Font caching is unavailable. An internet connection may be needed next time.';
const SHA256 = /^[a-f0-9]{64}$/;

function validate(manifest: FontCatalogManifest): void {
  if (!/^writer-fonts-v\d{1,4}$/.test(manifest.catalogId)
      || !SHA256.test(manifest.manifestSha256) || manifest.fonts.length !== FONT_COUNT) {
    throw new FontLoadError('invalid_manifest', 'The pinned font catalog metadata is invalid.');
  }
  const names = new Set<string>();
  let total = 0;
  for (const font of manifest.fonts) {
    if (!/^[a-zA-Z0-9][a-zA-Z0-9._-]{0,127}\.(?:ttf|otf)$/.test(font.id)
        || names.has(font.id) || !SHA256.test(font.sha256)
        || !Number.isSafeInteger(font.byteLength) || font.byteLength <= 0
        || font.byteLength > 8 * 1024 * 1024) {
      throw new FontLoadError('invalid_manifest', 'The pinned font catalog contains an invalid face.');
    }
    names.add(font.id);
    total += font.byteLength;
  }
  if (!Number.isSafeInteger(manifest.totalBytes) || total !== manifest.totalBytes
      || total > 64 * 1024 * 1024) {
    throw new FontLoadError('invalid_manifest', 'The pinned font catalog byte budget is invalid.');
  }
}

function aborted(signal: AbortSignal): FontLoadError {
  return signal.reason instanceof FontLoadError ? signal.reason
    : new FontLoadError('aborted', 'Loading fonts was cancelled.');
}

function cancellable<T>(promise: Promise<T>, signal: AbortSignal): Promise<T> {
  if (signal.aborted) return Promise.reject(aborted(signal));
  return new Promise<T>((resolve, reject) => {
    const cancel = () => reject(aborted(signal));
    signal.addEventListener('abort', cancel, { once: true });
    promise.then(resolve, reject).finally(() => signal.removeEventListener('abort', cancel));
  });
}

async function timed<T>(operation: (signal: AbortSignal) => Promise<T>, timeoutMs: number,
  parent: AbortSignal): Promise<T> {
  if (parent.aborted) throw aborted(parent);
  const controller = new AbortController();
  const cancel = () => controller.abort(aborted(parent));
  parent.addEventListener('abort', cancel, { once: true });
  const timer = setTimeout(() => controller.abort(new FontLoadError('timeout', 'Loading fonts timed out. Please retry.')), timeoutMs);
  try { return await cancellable(operation(controller.signal), controller.signal); }
  finally {
    clearTimeout(timer);
    parent.removeEventListener('abort', cancel);
  }
}

async function readVerified(response: Response, font: FontMetadata,
  signal: AbortSignal): Promise<Uint8Array<ArrayBuffer>> {
  const length = response.headers.get('Content-Length');
  // Fetch exposes decoded bytes, but HTTP Content-Length can describe the gzip/
  // Brotli transfer. Only identity responses can use that header as an exact
  // decoded-length precheck. The streamed body and SHA are always checked.
  const encoding = response.headers.get('Content-Encoding')?.trim().toLowerCase();
  if ((length !== null && (!/^\d+$/.test(length)
      || ((!encoding || encoding === 'identity') && Number(length) !== font.byteLength))) || !response.body) {
    void response.body?.cancel().catch(() => undefined);
    throw new FontLoadError('integrity', `The downloaded font ${font.id} has the wrong length.`, font.id);
  }
  const bytes = new Uint8Array(font.byteLength);
  const reader = response.body.getReader();
  const cancel = () => { void reader.cancel().catch(() => undefined); };
  signal.addEventListener('abort', cancel, { once: true });
  let offset = 0;
  let emptyChunks = 0;
  try {
    for (;;) {
      const { value, done } = await cancellable(reader.read(), signal);
      if (done) break;
      if (!value.byteLength && ++emptyChunks > 16) {
        throw new FontLoadError('network', `The download of ${font.id} made no progress.`, font.id);
      }
      if (value.byteLength > bytes.length - offset) {
        throw new FontLoadError('integrity', `The downloaded font ${font.id} exceeds its pinned size.`, font.id);
      }
      bytes.set(value, offset);
      offset += value.byteLength;
    }
    if (offset !== font.byteLength) {
      throw new FontLoadError('integrity', `The downloaded font ${font.id} is incomplete.`, font.id);
    }
    const digest = new Uint8Array(await cancellable(crypto.subtle.digest('SHA-256', bytes.buffer), signal));
    const actual = Array.from(digest, (byte) => byte.toString(16).padStart(2, '0')).join('');
    if (actual !== font.sha256) {
      throw new FontLoadError('integrity', `The downloaded font ${font.id} failed its integrity check.`, font.id);
    }
    return bytes;
  } finally {
    signal.removeEventListener('abort', cancel);
    cancel();
    try { reader.releaseLock(); } catch { /* Cancellation can still have a pending read. */ }
  }
}

async function download(url: string, font: FontMetadata, timeoutMs: number,
  signal: AbortSignal): Promise<Uint8Array<ArrayBuffer>> {
  let last: unknown;
  for (let attempt = 0; attempt < 2; attempt += 1) {
    if (signal.aborted) throw aborted(signal);
    try {
      return await timed(async (requestSignal) => {
        const response = await cancellable(fetch(url, {
          mode: 'same-origin', credentials: 'same-origin', redirect: 'error',
          cache: 'no-store', signal: requestSignal,
        }), requestSignal);
        if (response.status !== 200 || response.type !== 'basic' || response.redirected || response.url !== url) {
          void response.body?.cancel().catch(() => undefined);
          throw new FontLoadError('network', `The font ${font.id} could not be downloaded safely.`, font.id);
        }
        return readVerified(response, font, requestSignal);
      }, timeoutMs, signal);
    } catch (error) {
      if (signal.aborted) throw aborted(signal);
      last = error;
    }
  }
  if (last instanceof FontLoadError) throw last;
  throw new FontLoadError('network', `The font ${font.id} could not be loaded. Check your connection and retry.`, font.id);
}

/**
 * Fetch verified bytes only, with four requests at most. CacheStorage is optional;
 * it is not an offline app shell. Each read is rehashed, even after completion.
 * Partial catalogs never activate Rust, and older application caches are untouched.
 */
export async function loadFontCatalog(manifest: FontCatalogManifest,
  options: FontLoadOptions): Promise<FontLoadResult> {
  validate(manifest);
  const timeoutMs = options.timeoutMs ?? REQUEST_TIMEOUT;
  if (!Number.isFinite(timeoutMs) || timeoutMs < 1 || timeoutMs > REQUEST_TIMEOUT) {
    throw new FontLoadError('invalid_manifest', 'The font request timeout is invalid.');
  }
  const controller = new AbortController();
  const signal = controller.signal;
  const cancel = () => controller.abort(options.signal ? aborted(options.signal) : undefined);
  if (options.signal?.aborted) cancel();
  else options.signal?.addEventListener('abort', cancel, { once: true });
  let cache: Cache | undefined;
  const warnings: string[] = [];
  let loaded = 0;
  const progress = (phase: FontLoadProgress['phase']) => options.onProgress?.({
    phase, loaded, total: manifest.fonts.length, cacheAvailable: cache !== undefined,
    ...(warnings.length ? { warning: warnings[0] } : {}),
  });
  const cacheFailed = () => {
    cache = undefined;
    if (!warnings.length) warnings.push(CACHE_WARNING);
    progress('loading');
  };
  const prefix = new URL(`/fonts/${manifest.catalogId}/${manifest.manifestSha256}/`, location.origin).href;
  const result = new Array<Uint8Array<ArrayBuffer>>(manifest.fonts.length);
  try {
    try {
      cache = await timed(() => caches.open(`graphe-fonts-v1-${manifest.manifestSha256}`), timeoutMs, signal);
    } catch (error) {
      if (signal.aborted) throw error;
      cacheFailed();
    }
    progress('loading');
    let next = 0;
    const worker = async () => {
      for (;;) {
        if (signal.aborted) throw aborted(signal);
        const index = next++;
        const font = manifest.fonts[index];
        if (!font) return;
        const url = prefix + font.id;
        let bytes: Uint8Array<ArrayBuffer> | undefined;
        const activeCache = cache;
        if (activeCache) {
          try {
            const response = await timed(() => activeCache.match(url), timeoutMs, signal);
            if (response) {
              try {
                if (response.status !== 200 || !['basic', 'default'].includes(response.type) || response.redirected
                    || (response.url !== '' && response.url !== url)) {
                  throw new FontLoadError('integrity', `The cached font ${font.id} is invalid.`, font.id);
                }
                bytes = await timed((readSignal) => readVerified(response, font, readSignal), timeoutMs, signal);
              } catch (error) {
                if (signal.aborted) throw error;
                await timed(() => activeCache.delete(url), timeoutMs, signal);
              }
            }
          } catch (error) {
            if (signal.aborted) throw error;
            cacheFailed();
          }
        }
        if (!bytes) {
          bytes = await download(url, font, timeoutMs, signal);
          const writable = cache;
          if (writable) {
            try {
              await timed(() => writable.put(url, new Response(bytes, { status: 200,
                headers: { 'Content-Type': 'font/ttf', 'Content-Length': String(font.byteLength) },
              })), timeoutMs, signal);
            } catch (error) {
              if (signal.aborted) throw error;
              cacheFailed();
            }
          }
        }
        result[index] = bytes;
        loaded += 1;
        progress('loading');
      }
    };
    const workers = Array.from({ length: CONCURRENCY }, worker);
    try { await Promise.all(workers); }
    catch (error) {
      controller.abort(error);
      await Promise.allSettled(workers);
      throw error;
    }
    progress('registering');
    try {
      for (const [index, font] of manifest.fonts.entries()) {
        const bytes = result[index];
        if (!bytes) throw new Error('Incomplete verified catalog');
        await timed(async () => options.registerFont(font.id, bytes), timeoutMs, signal);
      }
      await timed(async () => options.finalizeRegistration(), timeoutMs, signal);
    } catch (error) {
      if (signal.aborted) throw error;
      throw new FontLoadError('registration', error instanceof Error ? error.message : 'The verified font catalog could not be registered.');
    }
    const completedCache = cache;
    if (completedCache) {
      try {
        await timed(() => completedCache.put(prefix + '.complete', new Response(JSON.stringify({
          catalogId: manifest.catalogId, manifestSha256: manifest.manifestSha256, fontCount: manifest.fonts.length,
        }), { headers: { 'Content-Type': 'application/json' } })), timeoutMs, signal);
      } catch (error) {
        if (signal.aborted) throw error;
        cacheFailed();
      }
    }
    progress('ready');
    return { cacheAvailable: cache !== undefined, warnings };
  } finally {
    options.signal?.removeEventListener('abort', cancel);
  }
}
