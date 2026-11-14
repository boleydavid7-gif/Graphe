// Run on each native CI platform without frontend dependencies or WASM bindings.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { log } from 'node:console';
import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import process from 'node:process';
import { fileURLToPath, URL } from 'node:url';

const root = new URL('../../../', import.meta.url);
const fixtures = new URL('tests/fixtures/layout/', root);
const golden = JSON.parse(await readFile(new URL('native-parity.json', fixtures), 'utf8'));
assert.equal(golden.input, 'layout-v1.writer');
assert.deepEqual(golden.options, {});
const snapshot = execFileSync('cargo', ['run', '--offline', '--locked', '--quiet',
  '-p', 'office-layout', '--example', 'layout_snapshot', '--',
  fileURLToPath(new URL(golden.input, fixtures))], {
  cwd: fileURLToPath(root), encoding: 'utf8', maxBuffer: 32 * 1024 * 1024,
}).trim();
const layout = JSON.parse(snapshot);
const hash = createHash('sha256').update(snapshot).digest('hex');
assert.equal(hash, golden.sha256, 'Native layout must match the reviewed cross-target fingerprint');
log(JSON.stringify({ platform: process.platform, architecture: process.arch,
  fontCatalogId: layout.fontCatalogId, pages: layout.pages.length,
  outlines: layout.outlines.length, sha256: hash }));
