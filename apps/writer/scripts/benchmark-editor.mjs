/**
 * Manual, local performance probe against a previously built Writer preview.
 *
 *   npm run build
 *   npm run preview -- --port 4173 --strictPort
 *   node scripts/benchmark-editor.mjs --output /tmp/writer-editor.json
 *
 * No timing thresholds: results depend on the machine, browser and workload.
 * Fresh browser contexts contain only public synthetic documents. This tool
 * never changes the application, storage schema, document model or layout.
 */
/* global WebAssembly, window, document, InputEvent, MutationObserver,
          requestAnimationFrame, DataTransfer, ClipboardEvent */
import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';
import { createHash } from 'node:crypto';
import { existsSync } from 'node:fs';
import { readFile, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { performance } from 'node:perf_hooks';
import { URL } from 'node:url';
import { chromium } from '@playwright/test';
import initialize, { WriterEngine } from '../src/wasm/office_wasm.js';

const options = argumentsFor(process.argv.slice(2));
const sentence = 'This public performance sample verifies stable editing, Unicode e\u0301 👩🏽‍💻 and local save integrity. Rust owns layout and transactions. ';
const wasmBytes = await readFile(new URL('../src/wasm/office_wasm_bg.wasm', import.meta.url));
const report = {
  generatedAt: new Date().toISOString(),
  url: options.url,
  wasmSha256: createHash('sha256').update(wasmBytes).digest('hex'),
  paragraphs: options.paragraphs,
  samplesPerCase: options.samples,
  mode: options.mode,
  scope: 'Trusted beforeinput through committed Rust SVG revision and next requestAnimationFrame; not OS input-to-paint. WASM export timers exclude JavaScript marshaling/JSON parsing. Memory domains are separate and must not be summed as process RSS.',
  native: [],
  browser: [],
  completed: false,
};

try {
  await initialize({ module_or_path: wasmBytes });
  if (options.mode !== 'browser') {
    for (const count of options.paragraphs) report.native.push(measureNative(count));
    report.responseLimit = checkResponseLimit();
  }
  if (options.mode !== 'wasm') {
    const executablePath = process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH
      ?? (existsSync('/usr/bin/chromium') ? '/usr/bin/chromium' : undefined);
    const browser = await chromium.launch({ executablePath, headless: true,
      args: ['--enable-precise-memory-info'] });
    try {
      report.browserVersion = browser.version();
      for (const count of options.paragraphs) report.browser.push(await measureBrowser(browser, count));
    } finally { await browser.close(); }
  }
  report.completed = true;
} catch (error) {
  report.error = error instanceof Error ? error.stack : String(error);
  process.exitCode = 1;
} finally {
  // Refuse overwrites, including when a failed probe still has useful partial data.
  await writeFile(options.output, `${JSON.stringify(report, null, 2)}\n`, { flag: 'wx' });
  console.log(JSON.stringify({ completed: report.completed, output: options.output }));
}

function argumentsFor(args) {
  const result = { url: 'http://127.0.0.1:4173/', output: join(tmpdir(), `writer-editor-${Date.now()}.json`),
    paragraphs: [90, 225, 315], samples: 20, mode: 'all' };
  for (let at = 0; at < args.length; at += 2) {
    const [name, value] = [args[at], args[at + 1]];
    assert.ok(value, `Missing value for ${name}`);
    if (name === '--url') result.url = new URL(value).href;
    else if (name === '--output') result.output = value;
    else if (name === '--mode') {
      assert.ok(['all', 'wasm', 'browser'].includes(value));
      result.mode = value;
    } else if (name === '--samples') result.samples = Number(value);
    else if (name === '--paragraphs') result.paragraphs = value.split(',').map(Number);
    else throw new Error(`Unknown option: ${name}`);
  }
  assert.ok(Number.isSafeInteger(result.samples) && result.samples >= 1 && result.samples <= 100);
  assert.ok(result.paragraphs.length > 0 && result.paragraphs.every(count =>
    Number.isSafeInteger(count) && count >= 1 && count <= 540));
  return result;
}

function contentFor(count) {
  return Array.from({ length: count }, (_, index) => `${index + 1}. ${sentence}${sentence}`).join('\n');
}

function summarize(values) {
  const sorted = [...values].sort((left, right) => left - right);
  const quantile = value => sorted[Math.max(0, Math.ceil(sorted.length * value) - 1)];
  return { n: sorted.length, p50Ms: quantile(0.5), p95Ms: quantile(0.95), maxMs: sorted.at(-1) };
}

function accept(value) {
  const response = JSON.parse(value);
  assert.equal(response.ok, true, JSON.stringify(response));
  return response.view;
}

function measureNative(paragraphs) {
  const text = contentFor(paragraphs);
  const engine = new WriterEngine(`native-editor-benchmark-${paragraphs}`);
  try {
    let view = accept(engine.dispatch(JSON.stringify({ type: 'replace_text', text })));
    const initialAt = performance.now();
    let geometry = JSON.parse(engine.layout_snapshot('{}', view.revision));
    const initialLayoutMs = performance.now() - initialAt;
    const initialPages = geometry.layout.pages.length;
    const commit = [], layout = [], cached = [];
    for (let index = 0; index < options.samples; index += 1) {
      const paragraph = view.paragraphs.at(-1);
      const utf16 = paragraph.runs.map(run => run.text).join('').length;
      const point = { paragraphId: paragraph.id, utf16 };
      const at = performance.now();
      view = accept(engine.dispatch(JSON.stringify({ type: 'replace_text', text: 'x',
        selection: { anchor: point, focus: point } })));
      commit.push(performance.now() - at);
      const layoutAt = performance.now();
      geometry = JSON.parse(engine.layout_snapshot('{}', view.revision));
      layout.push(performance.now() - layoutAt);
      const cachedAt = performance.now();
      JSON.parse(engine.layout_snapshot('{}', view.revision));
      cached.push(performance.now() - cachedAt);
    }
    assert.equal(view.paragraphs.map(paragraph => paragraph.runs.map(run => run.text).join('')).join('\n'),
      text + 'x'.repeat(options.samples));
    const result = { paragraphs, utf8Bytes: Buffer.byteLength(text), utf16Units: text.length,
      initialPages, finalPages: geometry.layout.pages.length,
      glyphs: geometry.layout.pages.reduce((count, page) => count + page.lines.reduce((sum, line) => sum + line.glyphs.length, 0), 0),
      responseBytes: Buffer.byteLength(JSON.stringify(geometry)), initialLayoutMs,
      commitIncludingJson: summarize(commit), layoutIncludingJson: summarize(layout),
      cachedLayoutIncludingJson: summarize(cached) };
    console.log(`Native editor measurement: ${JSON.stringify(result)}`);
    return result;
  } finally { engine.free(); }
}

function checkResponseLimit() {
  const text = contentFor(540), engine = new WriterEngine('editor-response-limit-probe');
  try {
    const view = accept(engine.dispatch(JSON.stringify({ type: 'replace_text', text })));
    const snapshot = engine.snapshot(), bytes = engine.export_document();
    let failure;
    try { engine.layout_snapshot('{}', view.revision); }
    catch (error) { failure = JSON.parse(error); }
    assert.equal(failure?.code, 'layout_too_large', 'Review this probe if the documented layout response budget changes');
    assert.equal(engine.snapshot(), snapshot);
    assert.deepEqual(engine.export_document(), bytes);
    return { paragraphs: 540, utf8Bytes: Buffer.byteLength(text), utf16Units: text.length,
      error: failure, unchangedSnapshotAndNativeBytes: true };
  } finally { engine.free(); }
}

async function measureBrowser(browser, paragraphs) {
  const context = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
  try {
    const page = await context.newPage(), errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.addInitScript(() => {
      const state = window.__writerEditorBenchmark = { calls: [], samples: [], pending: [], memory: null };
      const measured = new Set(['writerengine_dispatch', 'writerengine_layout_snapshot',
        'writerengine_selection_geometry', 'writerengine_export_document']);
      function wrap(result) {
        if (!result?.instance?.exports) return result;
        const exports = {};
        for (const [name, original] of Object.entries(result.instance.exports)) {
          exports[name] = typeof original === 'function' && measured.has(name) ? (...args) => {
            const at = performance.now();
            try { return original(...args); }
            finally { state.calls.push({ name, at, duration: performance.now() - at }); }
          } : original;
          if (original instanceof WebAssembly.Memory) state.memory = original;
        }
        return { module: result.module, instance: { exports } };
      }
      const instantiate = WebAssembly.instantiate.bind(WebAssembly);
      WebAssembly.instantiate = async (...args) => wrap(await instantiate(...args));
      const streaming = WebAssembly.instantiateStreaming?.bind(WebAssembly);
      if (streaming) WebAssembly.instantiateStreaming = async (...args) => wrap(await streaming(...args));
    });
    const navigationAt = performance.now();
    await page.goto(options.url);
    const root = page.getByTestId('paged-editor');
    const input = page.getByRole('textbox', { name: 'Document content', exact: true });
    await page.getByRole('button', { name: 'Save', exact: true }).waitFor({ state: 'visible' });
    await ready(page);
    const startupMs = performance.now() - navigationAt;
    const coldCalls = await page.evaluate(() => window.__writerEditorBenchmark.calls);
    assert.ok(coldCalls.some(call => call.name === 'writerengine_layout_snapshot'), 'WASM instrumentation must observe the actual app');
    const cdp = await context.newCDPSession(page);
    await cdp.send('Performance.enable');
    const emptyMemory = await memory(page, cdp);
    await input.focus();
    const text = contentFor(paragraphs), seedAt = performance.now();
    await input.evaluate((element, value) => {
      const transfer = new DataTransfer(); transfer.setData('text/plain', value);
      element.dispatchEvent(new ClipboardEvent('paste', { clipboardData: transfer, bubbles: true, cancelable: true }));
    }, text);
    await page.waitForFunction(expected => document.querySelector('[data-testid="paged-editor"]')?.getAttribute('aria-busy') === 'false'
      && document.querySelectorAll('[data-testid="editor"] p').length === expected, paragraphs, { timeout: 120_000 });
    const initialPasteRenderMs = performance.now() - seedAt;
    const initialPages = await root.locator('svg[data-layout-page]').count();
    await page.keyboard.press('Control+End');
    await ready(page);
    await memory(page, cdp);
    await page.evaluate(() => {
      const state = window.__writerEditorBenchmark;
      state.calls = []; state.pending = []; state.samples = [];
      const input = document.querySelector('[data-testid="editor"]');
      const root = document.querySelector('[data-testid="paged-editor"]');
      let previous = root.getAttribute('data-layout-revision');
      input.addEventListener('beforeinput', event => {
        if (event instanceof InputEvent && event.inputType === 'insertText') {
          state.pending.push({ at: performance.now(), trusted: event.isTrusted, callStart: state.calls.length });
        }
      }, true);
      new MutationObserver(() => {
        const revision = root.getAttribute('data-layout-revision');
        if (root.getAttribute('aria-busy') !== 'false' || !revision || revision === previous || !state.pending.length) return;
        previous = revision;
        const pending = state.pending.shift(), at = performance.now();
        const calls = state.calls.slice(pending.callStart).filter(call => call.at >= pending.at && call.at <= at);
        requestAnimationFrame(() => state.samples.push({ revision, trusted: pending.trusted,
          geometryMs: at - pending.at, nextFrameMs: performance.now() - pending.at, calls }));
      }).observe(root, { attributes: true, attributeFilter: ['data-layout-revision', 'aria-busy'] });
    });
    for (let count = 1; count <= options.samples; count += 1) {
      await page.keyboard.type('x');
      await page.waitForFunction(expected => window.__writerEditorBenchmark.samples.length >= expected,
        count, { polling: 'raf', timeout: 120_000 });
    }
    const samples = await page.evaluate(() => window.__writerEditorBenchmark.samples);
    assert.equal(samples.length, options.samples);
    assert.ok(samples.every(sample => sample.trusted));
    assert.equal(new Set(samples.map(sample => sample.revision)).size, options.samples);
    assert.equal((await input.locator('p').allTextContents()).join('\n'), text + 'x'.repeat(options.samples));
    const loadedMemory = await memory(page, cdp);
    const nativeCalls = name => samples.map(sample => sample.calls.filter(call => call.name === name)
      .reduce((sum, call) => sum + call.duration, 0));
    const result = { paragraphs, utf16Units: text.length, utf8Bytes: Buffer.byteLength(text),
      initialPages, finalPages: await root.locator('svg[data-layout-page]').count(), startupMs,
      coldLayoutWasmMs: coldCalls.filter(call => call.name === 'writerengine_layout_snapshot').map(call => call.duration),
      initialPasteRenderMs, trustedKeys: options.samples,
      geometry: summarize(samples.map(sample => sample.geometryMs)),
      nextAnimationFrame: summarize(samples.map(sample => sample.nextFrameMs)),
      wasmDispatch: summarize(nativeCalls('writerengine_dispatch')),
      wasmLayout: summarize(nativeCalls('writerengine_layout_snapshot')),
      wasmSelection: summarize(nativeCalls('writerengine_selection_geometry')),
      emptyMemory, loadedMemory, svgUses: await root.locator('use').count(), pageErrors: errors, samples };
    assert.deepEqual(errors, []);
    console.log(`Browser editor measurement: ${JSON.stringify({ ...result, samples: undefined })}`);
    return result;
  } finally { await context.close(); }
}

async function ready(page) {
  await page.waitForFunction(() => document.querySelector('[data-testid="paged-editor"]')?.getAttribute('aria-busy') === 'false',
    null, { timeout: 120_000 });
}

async function memory(page, cdp) {
  await cdp.send('HeapProfiler.collectGarbage');
  const metrics = Object.fromEntries((await cdp.send('Performance.getMetrics')).metrics.map(item => [item.name, item.value]));
  const dom = await cdp.send('Memory.getDOMCounters'), heap = await cdp.send('Runtime.getHeapUsage');
  return { jsHeapUsedBytes: metrics.JSHeapUsedSize, jsHeapTotalBytes: metrics.JSHeapTotalSize,
    nodes: dom.nodes, documents: dom.documents, eventListeners: dom.jsEventListeners,
    embedderHeapUsedBytes: heap.embedderHeapUsedSize, backingStorageBytes: heap.backingStorageSize,
    wasmLinearMemoryBytes: await page.evaluate(() => window.__writerEditorBenchmark.memory?.buffer.byteLength ?? null) };
}
