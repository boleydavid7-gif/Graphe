import assert from 'node:assert/strict';
import { readFileSync, readdirSync, existsSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath, URL } from 'node:url';
import test from 'node:test';
import { loadConfigFromFile } from 'vite';

const root = fileURLToPath(new URL('../', import.meta.url));

test('Writer Vite build retains the standalone Worker root mount path', async () => {
  const config = await loadConfigFromFile({ command: 'build', mode: 'production' }, join(root, 'vite.config.ts'));
  assert.equal(config?.config.base ?? '/', '/');
});

test('built entry references root assets for the standalone Writer Worker', { skip: !existsSync(join(root, 'dist', 'index.html')) && 'Run npm run build first' }, () => {
  const html = readFileSync(join(root, 'dist', 'index.html'), 'utf8');
  const urls = [...html.matchAll(/(?:src|href)="([^"]+)"/g)].map((match) => match[1]);
  const assets = urls.filter((url) => /\.(?:js|css)(?:\?|$)/.test(url));
  assert.ok(assets.length > 0, 'Production HTML must reference bundled JS/CSS');
  for (const asset of assets) assert.ok(asset.startsWith('/assets/'), `Unexpected standalone asset path: ${asset}`);
  const files = readdirSync(join(root, 'dist', 'assets'));
  assert.ok(files.some((name) => name.endsWith('.wasm')), 'Production build must include the Rust WebAssembly asset');
});
