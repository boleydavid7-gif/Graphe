// Development measurement only. Requires a built Writer preview and Chromium.
// Uses isolated temporary databases; never opens the user's document library.
import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';
import { createHash, randomUUID } from 'node:crypto';
import { existsSync } from 'node:fs';
import { readFile, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { clearTimeout, setTimeout } from 'node:timers';
import { chromium } from '@playwright/test';
import ts from 'typescript';

const options = { url: 'http://127.0.0.1:4173/', output: join(tmpdir(), `writer-library-benchmark-${Date.now()}-${randomUUID()}.json`), trials: 5, cases: [] };
for (let i = 2; i < process.argv.length; i += 1) {
  const flag = process.argv[i];
  const value = process.argv[++i];
  assert.ok(typeof value === 'string' && value.length > 0, `Missing value for ${flag}`);
  if (flag === '--url') options.url = value;
  else if (flag === '--output') options.output = value;
  else if (flag === '--trials') options.trials = Number(value);
  else if (flag === '--case') options.cases.push(value.split(':').map(Number));
  else throw new Error(`Unknown option ${flag}; use --url, --output, --trials or --case COUNT:ORIGINAL_BYTES`);
}
assert.ok(Number.isInteger(options.trials) && options.trials >= 2 && options.trials <= 20);
if (!options.cases.length) options.cases = [[10, 6144], [100, 6144], [1000, 6144], [10, 786432], [100, 786432], [200, 786432]];
for (const [count, originalBytes] of options.cases) {
  assert.ok(Number.isInteger(count) && count > 0 && count <= 2000);
  assert.ok(Number.isInteger(originalBytes) && originalBytes > 0);
  assert.ok(count * (Math.ceil(originalBytes / 3) * 4 + 8192) <= 256 * 1024 * 1024, 'Synthetic library capped at 256 MiB');
}

const source = await readFile(new URL('../src/storage.ts', import.meta.url), 'utf8');
const fixture = JSON.parse(await readFile(new URL('../../../tests/fixtures/native/project-notes.writer', import.meta.url), 'utf8'));
const start = source.indexOf('  async list(): Promise<DocumentLibrary>');
const end = source.indexOf('  async load(', start);
assert.ok(start >= 0 && end > start, 'Review benchmark when the storage API changes');

// Audited pre-optimization reference. It deliberately retains getAll and the
// owned byte copy from decode(), with the same validation/sort/error contract.
const legacyList = String.raw`  async list(): Promise<DocumentLibrary> {
    const database = await this.database();
    return new Promise((resolve, reject) => {
      let transaction: IDBTransaction;
      let result: DocumentLibrary = { documents: [], problems: [] };
      let reason: StorageError | undefined;
      try {
        transaction = database.transaction(STORE, 'readonly');
        const request = transaction.objectStore(STORE).getAll();
        request.onsuccess = () => {
          try {
            const documents: StoredDocumentSummary[] = [];
            const problems: LibraryProblem[] = [];
            for (const value of request.result as unknown[]) {
              try { documents.push(summary(decode(value))); }
              catch (cause) {
                if (!(cause instanceof StorageError) || cause.code !== 'corrupt') throw cause;
                const id = isRecord(value) && typeof value.id === 'string' && value.id.length > 0 ? value.id : undefined;
                problems.push({ message: cause.message, ...(id ? { id } : {}) });
              }
            }
            documents.sort((left, right) => right.updatedAt - left.updatedAt || left.id.localeCompare(right.id));
            result = { documents, problems };
          } catch (cause) { reason = failure(cause); transaction.abort(); }
        };
      } catch (cause) { reject(failure(cause)); return; }
      transaction.oncomplete = () => resolve(result);
      transaction.onabort = () => reject(reason ?? failure(transaction.error));
      transaction.onerror = (event) => { reason ??= failure((event.target as IDBRequest).error ?? transaction.error); };
    });
  }
`;
const sources = { baseline: source.slice(0, start) + legacyList + source.slice(end), actual: source };
const emitted = Object.fromEntries(Object.entries(sources).map(([key, value]) => [key, ts.transpileModule(value, {
  compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext },
}).outputText]));
const actualMethod = source.slice(start, end).includes('.openCursor(') ? 'openCursor' : 'getAll';
const median = (values) => {
  const sorted = [...values].sort((left, right) => left - right);
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2;
};

// Validate representative source-bearing native files through actual WASM,
// rather than populating the library with arbitrary byte buffers.
const { default: initialize, WriterEngine } = await import('../src/wasm/office_wasm.js');
await initialize({ module_or_path: await readFile(new URL('../src/wasm/office_wasm_bg.wasm', import.meta.url)) });
for (const originalBytes of new Set(options.cases.map((entry) => entry[1]))) {
  const file = globalThis.structuredClone(fixture);
  file.source = { format: 'txt', media_type: 'text/plain', filename: 'retained-original.txt', bytes_base64: Buffer.alloc(originalBytes, 120).toString('base64') };
  const engine = new WriterEngine('library-benchmark-validation');
  try {
    const opened = JSON.parse(engine.open_document(new globalThis.TextEncoder().encode(JSON.stringify(file))));
    assert.equal(opened.ok, true, JSON.stringify(opened));
    assert.equal(engine.export_original().length, originalBytes);
  } finally { engine.free(); }
}

const executablePath = process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH ?? (existsSync('/usr/bin/chromium') ? '/usr/bin/chromium' : undefined);
const browser = await chromium.launch({ headless: true, executablePath, args: ['--enable-precise-memory-info', '--js-flags=--expose-gc'] });
const result = {
  timestamp: new Date().toISOString(), browser: browser.version(), url: options.url,
  storageSourceSha256: createHash('sha256').update(source).digest('hex'), actualMethod,
  trials: options.trials, cases: [], correctness: null,
  memoryScope: 'CDP backingStorageSize at native IDB success, with GC immediately before pausing. Not process RSS or absolute peak. Cursor/native cache and event lifetimes affect retained values. Timing trials do not contain debugger pauses.',
};
let context;
try {
  context = await browser.newContext();
  await context.route('**/__writer-library-benchmark-*.mjs', async (route) => {
    const key = new URL(route.request().url()).pathname.match(/__writer-library-benchmark-(\w+)\.mjs$/)?.[1];
    await route.fulfill({ status: Object.hasOwn(emitted, key) ? 200 : 404, contentType: 'text/javascript', body: emitted[key] ?? 'throw Error("Unknown benchmark module")' });
  });
  const page = await context.newPage();
  await page.goto(options.url);
  await page.waitForFunction(() => globalThis.document.querySelector('[data-testid="paged-editor"]')?.getAttribute('aria-busy') === 'false');
  const cdp = await context.newCDPSession(page);
  await cdp.send('HeapProfiler.enable');
  await cdp.send('Debugger.enable');
  await page.evaluate(async () => {
    globalThis.libraryBenchmark = { modules: {}, instances: {}, native: {}, observerCount: 0 };
    const state = globalThis.libraryBenchmark;
    for (const key of ['baseline', 'actual']) state.modules[key] = await import(`/__writer-library-benchmark-${key}.mjs`);
    state.seed = async (name, count, originalBytes, fixture) => {
      const database = await new Promise((resolve, reject) => {
        const request = globalThis.indexedDB.open(name, 1);
        request.onupgradeneeded = () => request.result.createObjectStore('documents', { keyPath: 'id' });
        request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error);
      });
      const base64 = globalThis.btoa('x'.repeat(originalBytes));
      let bytesTotal = 0;
      try {
        for (let start = 0; start < count; start += 16) await new Promise((resolve, reject) => {
          const transaction = database.transaction('documents', 'readwrite');
          for (let index = start; index < Math.min(count, start + 16); index += 1) {
            const file = globalThis.structuredClone(fixture);
            file.document.id = `benchmark-${String(index).padStart(5, '0')}`;
            file.document.metadata.title = `Library document ${index} — Café`;
            file.source = { format: 'txt', media_type: 'text/plain', filename: 'retained-original.txt', bytes_base64: base64 };
            const bytes = new globalThis.TextEncoder().encode(JSON.stringify(file)); bytesTotal += bytes.byteLength;
            transaction.objectStore('documents').put({ id: file.document.id, title: file.document.metadata.title, bytes, etag: `version-${index}`, updatedAt: 1000000 + index });
          }
          transaction.oncomplete = resolve; transaction.onabort = () => reject(transaction.error);
        });
      } finally { database.close(); }
      return bytesTotal;
    };
    state.list = async (key, name) => {
      const storage = state.instances[key] ??= new state.modules[key].WriterStorage(name);
      const start = globalThis.performance.now(); const value = await storage.list();
      return { ms: globalThis.performance.now() - start, ...value };
    };
    state.pauseAt = (method, target) => {
      state.native[method] ??= globalThis.IDBObjectStore.prototype[method];
      state.observerCount = 0;
      globalThis.IDBObjectStore.prototype[method] = function (...args) {
        const request = state.native[method].apply(this, args);
        if (this.name === 'documents') request.addEventListener('success', () => {
          const value = request.result;
          if (method === 'openCursor' && !value) return;
          if (method === 'openCursor') void value.value;
          state.observerCount += 1;
          if (state.observerCount === target) {
            if (typeof globalThis.gc !== 'function') throw new Error('Exposed GC unavailable');
            globalThis.gc();
            // eslint-disable-next-line no-debugger -- Controlled CDP memory sample, outside timing trials.
            debugger;
          }
        });
        return request;
      };
    };
    state.removeHooks = () => { for (const [key, method] of Object.entries(state.native)) globalThis.IDBObjectStore.prototype[key] = method; };
    state.close = () => { state.removeHooks(); for (const storage of Object.values(state.instances)) storage.close(); state.instances = {}; };
  });

  for (const [count, originalBytes] of options.cases) {
    const name = `writer-benchmark-${randomUUID()}`;
    try {
      const bytesTotal = await page.evaluate((args) => globalThis.libraryBenchmark.seed(...args), [name, count, originalBytes, fixture]);
      const expected = await page.evaluate((args) => globalThis.libraryBenchmark.list(...args), ['baseline', name]);
      assert.equal(expected.documents.length, count); assert.deepEqual(expected.problems, []);
      const samples = { baseline: [], actual: [] };
      await page.evaluate((args) => globalThis.libraryBenchmark.list(...args), ['actual', name]);
      for (let trial = 0; trial < options.trials; trial += 1) for (const key of trial % 2 ? ['actual', 'baseline'] : ['baseline', 'actual']) {
        await cdp.send('HeapProfiler.collectGarbage');
        const value = await page.evaluate((args) => globalThis.libraryBenchmark.list(...args), [key, name]);
        assert.deepEqual(value.documents, expected.documents); assert.deepEqual(value.problems, expected.problems);
        assert.ok(value.documents.every((entry) => !Object.hasOwn(entry, 'bytes')));
        samples[key].push(value.ms);
      }
      const row = { count, originalBytes, bytesTotal, variants: {} };
      for (const key of ['baseline', 'actual']) {
        await cdp.send('HeapProfiler.collectGarbage');
        const before = await cdp.send('Runtime.getHeapUsage');
        const method = key === 'baseline' ? 'getAll' : actualMethod;
        await page.evaluate((args) => globalThis.libraryBenchmark.pauseAt(...args), [method, method === 'getAll' ? 1 : count]);
        const paused = new Promise((resolve) => cdp.once('Debugger.paused', resolve));
        const pending = page.evaluate(async (args) => (await globalThis.libraryBenchmark.list(...args)).documents.length, [key, name]);
        let deadline;
        try {
          await Promise.race([paused, new Promise((_, reject) => {
            deadline = setTimeout(() => reject(new Error('IndexedDB memory sample did not pause')), 15000);
          })]);
        } finally { clearTimeout(deadline); }
        const retained = await cdp.send('Runtime.getHeapUsage');
        await cdp.send('Debugger.resume'); assert.equal(await pending, count);
        await page.evaluate(() => globalThis.libraryBenchmark.removeHooks());
        const trialsMs = samples[key];
        row.variants[key] = { trialsMs, medianMs: median(trialsMs), before, retained, retainedBackingDelta: retained.backingStorageSize - before.backingStorageSize };
      }
      result.cases.push(row);
      console.log(JSON.stringify(row));
    } finally {
      await page.evaluate(() => globalThis.libraryBenchmark.close());
      await page.evaluate((name) => new Promise((resolve, reject) => {
        const request = globalThis.indexedDB.deleteDatabase(name);
        request.onsuccess = resolve; request.onerror = () => reject(request.error); request.onblocked = () => reject(new Error('Benchmark database still open'));
      }), name);
    }
  }
  const corruptionName = `writer-benchmark-corruption-${randomUUID()}`;
  try {
    const before = await page.evaluate(async ({ name, fixture }) => {
      const request = globalThis.indexedDB.open(name, 1);
      request.onupgradeneeded = () => request.result.createObjectStore('documents', { keyPath: 'id' });
      const database = await new Promise((resolve, reject) => { request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error); });
      try {
        await new Promise((resolve, reject) => {
          const transaction = database.transaction('documents', 'readwrite');
          const store = transaction.objectStore('documents');
          const valid = globalThis.structuredClone(fixture);
          valid.document.id = 'healthy';
          for (const row of [
            { id: 'healthy', title: valid.document.metadata.title, bytes: new globalThis.TextEncoder().encode(JSON.stringify(valid)), etag: 't', updatedAt: 1 },
            { id: 'broken-bytes', title: 'Empty bytes', bytes: new Uint8Array(), etag: 't', updatedAt: 1 },
            { id: 'broken-date', title: 'Bad date', bytes: new Uint8Array([2]), etag: 't', updatedAt: 8_640_000_000_000_001 },
            { id: 91, title: 'Nonstring identity', bytes: new Uint8Array([3]), etag: 't', updatedAt: 1 },
          ]) store.put(row);
          transaction.oncomplete = resolve; transaction.onabort = () => reject(transaction.error);
        });
        return await new Promise((resolve, reject) => {
          const transaction = database.transaction('documents', 'readonly'); const request = transaction.objectStore('documents').getAll();
          transaction.oncomplete = () => resolve(JSON.stringify(request.result)); transaction.onabort = () => reject(transaction.error);
        });
      } finally { database.close(); }
    }, { name: corruptionName, fixture });
    const baseline = await page.evaluate((args) => globalThis.libraryBenchmark.list(...args), ['baseline', corruptionName]);
    const actual = await page.evaluate((args) => globalThis.libraryBenchmark.list(...args), ['actual', corruptionName]);
    assert.deepEqual(actual.documents, baseline.documents); assert.deepEqual(actual.problems, baseline.problems);
    assert.equal(actual.documents.length, 1); assert.equal(actual.problems.length, 3);
    const after = await page.evaluate(async (name) => {
      const database = await new Promise((resolve, reject) => { const request = globalThis.indexedDB.open(name, 1); request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error); });
      try {
        return await new Promise((resolve, reject) => {
          const transaction = database.transaction('documents', 'readonly'); const request = transaction.objectStore('documents').getAll();
          transaction.oncomplete = () => resolve(JSON.stringify(request.result)); transaction.onabort = () => reject(transaction.error);
        });
      } finally { database.close(); }
    }, corruptionName);
    assert.equal(after, before);
  } finally {
    await page.evaluate(() => globalThis.libraryBenchmark.close());
    await page.evaluate((name) => new Promise((resolve, reject) => {
      const request = globalThis.indexedDB.deleteDatabase(name); request.onsuccess = resolve; request.onerror = () => reject(request.error);
    }), corruptionName);
  }
  result.correctness = { timedMetadataAndOrderingEqual: true, noByteSummaries: true, corruptionReportsEqual: true, damagedRecordsUnchanged: true };
  await writeFile(options.output, `${JSON.stringify(result, null, 2)}\n`, { flag: 'wx' });
} finally {
  await context?.close();
  await browser.close();
}
console.log(`Measurement report: ${options.output}`);
