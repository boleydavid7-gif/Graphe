// Development measurement of the actual release WASM; no engine or font mocks.
// Run after npm run build: node scripts/measure-layout.mjs
import { readFile } from 'node:fs/promises';
import { performance } from 'node:perf_hooks';
import { TextEncoder } from 'node:util';
import { URL } from 'node:url';
import initialize, { WriterEngine } from '../src/wasm/office_wasm.js';

const binary = await readFile(new URL('../src/wasm/office_wasm_bg.wasm', import.meta.url));
const initialization = performance.now();
await initialize({ module_or_path: binary });
const initializationMs = performance.now() - initialization;
const engine = new WriterEngine('layout-performance');
const encoder = new TextEncoder();
const results = [];
try {
  for (const length of [1000, 10_000, 50_000, 100_000, 200_000]) {
    const file = {
      format: 'office-writer', format_version: 2,
      document: {
        schema_version: 2, id: 'measurement-document', metadata: { title: 'Layout measurement' }, styles: [],
        sections: [{ id: 'measurement-section', properties: null, blocks: [{
          kind: 'paragraph', content: {
            id: 'measurement-paragraph', style: null, format: {},
            runs: [{ text: 'a '.repeat(length / 2), format: {} }],
          },
        }] }],
      },
    };
    const opened = JSON.parse(engine.open_document(encoder.encode(JSON.stringify(file))));
    if (!opened.ok) throw new Error(JSON.stringify(opened.error));
    const before = engine.snapshot();
    const revision = opened.view.revision;
    const started = performance.now();
    try {
      const encoded = engine.layout_snapshot('{}', revision);
      const layoutMs = performance.now() - started;
      const cachedStart = performance.now();
      engine.layout_snapshot('{}', revision);
      const cachedMs = performance.now() - cachedStart;
      const result = JSON.parse(encoded);
      results.push({ length, layoutMs, cachedMs, bytes: encoder.encode(encoded).length,
        pages: result.layout.pages.length, sourceUnchanged: engine.snapshot() === before });
    } catch (failure) {
      results.push({ length, elapsedMs: performance.now() - started,
        error: JSON.parse(String(failure)).code, sourceUnchanged: engine.snapshot() === before });
    }
  }
} finally { engine.free(); }
console.log(JSON.stringify({ runtime: process.version, initializationMs,
  measured: 'Release WASM layout, JSON serialization and ABI copy; excludes JSON parsing and browser paint', results }, null, 2));
