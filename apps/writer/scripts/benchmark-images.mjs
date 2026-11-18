import assert from 'node:assert/strict';
import { readFile, writeFile } from 'node:fs/promises';
import { performance } from 'node:perf_hooks';
import { crc32 } from 'node:zlib';
import { Buffer } from 'node:buffer';
import initialize, { WriterEngine } from '../src/wasm/office_wasm.js';

// Run after npm run build:wasm: node scripts/benchmark-images.mjs [output.json].
// Measures the Rust/WASM bridge only; this excludes layout and browser rendering.
const arguments_ = process.argv.slice(2);
if (arguments_.length > 1) throw new Error('Usage: node scripts/benchmark-images.mjs [output.json]');
const wasmBytes = await readFile(new URL('../src/wasm/office_wasm_bg.wasm', import.meta.url));
const exports = await initialize({ module_or_path: wasmBytes });
const png = await readFile(new URL('../../../crates/office-wasm/tests/fixtures/images/sample.png', import.meta.url));
function paddedPng(totalBytes) {
  const payload = Buffer.alloc(totalBytes - png.length - 12, 120);
  Buffer.from('benchmark\0').copy(payload);
  const type = Buffer.from('tEXt');
  const length = Buffer.alloc(4); length.writeUInt32BE(payload.length);
  const checksum = Buffer.alloc(4); checksum.writeUInt32BE(crc32(Buffer.concat([type, payload])));
  const result = Buffer.concat([png.subarray(0, png.length - 12), length, type, payload, checksum, png.subarray(png.length - 12)]);
  assert.equal(result.length, totalBytes);
  return result;
}
function accepted(json) {
  const result = JSON.parse(json);
  assert.equal(result.ok, true, JSON.stringify(result));
  return result.view;
}
function dispatch(engine, command) { return accepted(engine.dispatch(JSON.stringify(command))); }
const specifications = [
  { name: 'no-images', assets: [] },
  { name: 'one-1-MiB-PNG', assets: [paddedPng(1024 * 1024)] },
  { name: 'near-5-MiB-native', assets: [paddedPng(1_950_720), paddedPng(1_950_720)] },
];
const cases = specifications.map((specification, index) => {
  const engine = new WriterEngine(`image-typing-benchmark-${index}`);
  let view = JSON.parse(engine.snapshot());
  for (const bytes of specification.assets) {
    view = accepted(engine.insert_image(bytes, view.revision));
    view = dispatch(engine, { type: 'move_image_cursor', direction: 'forward' });
  }
  const nativeBytes = engine.export_document().length;
  return { engine, view, name: specification.name, encodedAssetBytes: specification.assets.map((bytes) => bytes.length),
    nativeBytesBeforeTyping: nativeBytes, images: view.images.map((image) => ({
      widthPx: image.pixelWidth, heightPx: image.pixelHeight, widthPt: image.widthPt, heightPt: image.heightPt,
      canonicalPngBytes: engine.image_png(image.id, view.revision).length,
    })), dispatchMs: [], snapshotMs: [], snapshotBytes: 0 };
});
const warmupEditsPerCase = 20;
const measuredEditsPerCase = 100;
function edit(item, measure) {
  const command = JSON.stringify({ type: 'replace_text', text: 'x', expectedRevision: item.view.revision });
  let start = performance.now();
  const response = item.engine.dispatch(command);
  const dispatchMs = performance.now() - start;
  item.view = accepted(response);
  start = performance.now();
  const snapshot = item.engine.snapshot();
  const snapshotMs = performance.now() - start;
  item.snapshotBytes = Buffer.byteLength(snapshot);
  assert.equal(JSON.parse(snapshot).revision, item.view.revision);
  if (measure) { item.dispatchMs.push(dispatchMs); item.snapshotMs.push(snapshotMs); }
}
// Interleave cases and rotate their order to reduce drift from shared-host load.
for (let sample = 0; sample < warmupEditsPerCase + measuredEditsPerCase; sample++) {
  for (let offset = 0; offset < cases.length; offset++) {
    edit(cases[(sample + offset) % cases.length], sample >= warmupEditsPerCase);
  }
}
function statistics(values) {
  const sorted = [...values].sort((a, b) => a - b);
  const midpoint = sorted.length / 2;
  return { samples: sorted.length, medianMs: (sorted[midpoint - 1] + sorted[midpoint]) / 2,
    p95Ms: sorted[Math.ceil(sorted.length * 0.95) - 1], minMs: sorted[0], maxMs: sorted.at(-1) };
}
const result = { method: 'Actual compiled Rust/WASM WriterEngine. Each sample inserts one ASCII x via revision-guarded replace_text at Rust caret, then reads snapshot. Command construction and response parsing excluded from timing. Cases interleaved with rotating order; synchronous engine dispatch and snapshot timed independently.',
  warmupEditsPerCase, measuredEditsPerCase, node: process.version, wasmBytes: wasmBytes.length,
  host: 'Shared execution environment; concurrent Rust/frontend tests may affect CPU and tail latency.',
  cases: cases.map((item) => ({ name: item.name, encodedAssetBytes: item.encodedAssetBytes, images: item.images,
    nativeBytesBeforeTyping: item.nativeBytesBeforeTyping, nativeBytesAfterTyping: item.engine.export_document().length,
    snapshotBytesAfterTyping: item.snapshotBytes, dispatch: statistics(item.dispatchMs), snapshot: statistics(item.snapshotMs) })),
  wasmLinearMemoryBytesAfterAllCases: exports.memory?.buffer.byteLength,
};
for (const item of cases) {
  const paragraph = item.view.paragraphs.at(-1);
  assert.equal(paragraph.runs.map((run) => run.text).join(''), 'x'.repeat(warmupEditsPerCase + measuredEditsPerCase));
  item.engine.free();
}
const json = `${JSON.stringify(result, null, 2)}\n`;
if (arguments_[0]) await writeFile(arguments_[0], json);
else console.log(json.trimEnd());
