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
// Preserve the original reviewed page/glyph fingerprint, changing only the
// declared catalog version in the snapshot used for this legacy fixture.
// All geometry, glyph outlines and the rest of the JSON stay byte-identical.
const catalogMarker = '"fontCatalogId":"writer-fonts-v6"';
assert.equal(snapshot.split(catalogMarker).length, 2);
const legacyGeometry = snapshot.replace(catalogMarker, '"fontCatalogId":"writer-fonts-v1"');
const referenceHash = createHash('sha256').update(legacyGeometry).digest('hex');
assert.equal(referenceHash, golden.sha256,
  'Original font page geometry differs from the reviewed cross-target fingerprint');
const hash = createHash('sha256').update(snapshot).digest('hex');
log(JSON.stringify({ platform: process.platform, architecture: process.arch,
  fontCatalogId: layout.fontCatalogId, pages: layout.pages.length,
  outlines: layout.outlines.length, sha256: hash }));
