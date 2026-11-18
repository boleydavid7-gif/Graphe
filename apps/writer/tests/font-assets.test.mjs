import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';
import { createHash } from 'node:crypto';
import { cpSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, truncateSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { fileURLToPath, URL } from 'node:url';
import { build, createLogger, createServer, preview } from 'vite';
import { writerFontAssets } from '../scripts/font-assets.mjs';

const source = fileURLToPath(new URL('../../../crates/office-layout/fonts/', import.meta.url));
const cssPath = fileURLToPath(new URL('../src/styles.css', import.meta.url));
const manifest = readFileSync(join(source, 'manifest.json'));
const hash = createHash('sha256').update(manifest).digest('hex');
const baseUrl = `/fonts/writer-fonts-v6/${hash}`;
const maxBytes = 25 * 1024 * 1024;

function directory(context, label) {
  const path = mkdtempSync(join(tmpdir(), `writer-fonts-${label}-`));
  context.after(() => rmSync(path, { recursive: true, force: true }));
  return path;
}

function fixture(context) {
  const root = directory(context, 'vite');
  mkdirSync(join(root, 'src'));
  writeFileSync(join(root, 'index.html'), '<!doctype html><html><head></head><body><script type="module" src="/src/main.js"></script></body></html>');
  writeFileSync(join(root, 'src/main.js'), 'import "./styles.css"; document.body.dataset.ready = "true";');
  writeFileSync(join(root, 'src/styles.css'), readFileSync(cssPath));
  return root;
}

function files(root, prefix = '') {
  return readdirSync(join(root, prefix), { withFileTypes: true }).flatMap((entry) => {
    const path = prefix ? `${prefix}/${entry.name}` : entry.name;
    return entry.isDirectory() ? files(root, path) : [path];
  });
}

function serverOrigin(server) {
  const address = server.httpServer.address();
  assert.ok(address && typeof address !== 'string');
  return `http://127.0.0.1:${address.port}`;
}

test('the checked 42-face catalog and CSS share one immutable publication URL', () => {
  const plugin = writerFontAssets();
  const css = readFileSync(cssPath, 'utf8');
  assert.equal((css.match(/@font-face\s*\{/g) ?? []).length, 42);
  assert.equal((css.match(/__WRITER_FONT_ASSET_BASE__\//g) ?? []).length, 42);
  assert.ok(!css.includes('../../../crates/office-layout/fonts/'));
  const transformed = plugin.transform(css, cssPath).code;
  assert.equal((transformed.match(new RegExp(`${baseUrl}/`, 'g')) ?? []).length, 42);
  assert.ok(!transformed.includes('__WRITER_FONT_ASSET_BASE__'));
  assert.throws(() => plugin.transform("@font-face{src:url('__WRITER_FONT_ASSET_BASE__/unverified.ttf')}", cssPath), /unchecked font/);
});

test('manifest, font and notice mutations fail before publication', (context) => {
  for (const filename of ['manifest.json', 'Arimo-Regular.ttf', 'NOTICE.txt']) {
    const copy = directory(context, 'tampered');
    cpSync(source, copy, { recursive: true });
    const original = readFileSync(join(copy, filename));
    writeFileSync(join(copy, filename), Buffer.concat([original, Buffer.from(' ')]));
    assert.throws(() => writerFontAssets({ fontsDirectory: copy }), /does not match SHA256SUMS/);
  }
});

test('duplicate or noncanonical checksum inventories are rejected', (context) => {
  const copy = directory(context, 'checksums');
  cpSync(source, copy, { recursive: true });
  const original = readFileSync(join(copy, 'SHA256SUMS'), 'utf8');
  for (const suffix of [original.split('\n')[0], `${'0'.repeat(64)}  ../escape.ttf`]) {
    writeFileSync(join(copy, 'SHA256SUMS'), `${original}${suffix}\n`);
    assert.throws(() => writerFontAssets({ fontsDirectory: copy }), /invalid or duplicate entry/);
  }
});

test('all emitted assets and chunks enforce the 25 MiB ceiling', () => {
  const plugin = writerFontAssets();
  const asset = { type: 'asset', source: new Uint8Array(maxBytes + 1) };
  assert.throws(() => plugin.generateBundle.handler({}, { 'oversize.wasm': asset }), /oversize.wasm exceeds the 25 MiB/);
  plugin.generateBundle.handler({}, { 'within-budget.wasm': { type: 'asset', source: new Uint8Array(maxBytes) } });
  assert.throws(() => plugin.generateBundle.handler({}, { 'oversize.js': { type: 'chunk', code: 'é'.repeat(maxBytes / 2 + 1) } }), /oversize.js exceeds the 25 MiB/);
});

test('real Vite builds publish original bytes once and preview serves the resulting catalog', async (context) => {
  const root = fixture(context);
  const plugin = writerFontAssets();
  const warnings = [];
  const logger = createLogger('silent');
  logger.warn = (message) => warnings.push(message);
  logger.warnOnce = (message) => warnings.push(message);
  await build({ configFile: false, root, publicDir: false, plugins: [plugin], customLogger: logger,
    build: { outDir: 'dist', emptyOutDir: true } });
  assert.deepEqual(warnings, []);
  const dist = join(root, 'dist');
  const names = files(dist);
  const fontNames = names.filter((name) => /\.(ttf|otf|woff2?)$/.test(name));
  assert.equal(fontNames.length, 42);
  assert.ok(fontNames.every((name) => name.startsWith(`${baseUrl.slice(1)}/`)));
  assert.equal(fontNames.filter((name) => name.startsWith('assets/')).length, 0);
  const publicationFiles = names.filter((name) => name.startsWith(`${baseUrl.slice(1)}/`));
  assert.equal(publicationFiles.length, 66);
  for (const name of publicationFiles) {
    assert.deepEqual(readFileSync(join(dist, name)), readFileSync(join(source, name.split('/').at(-1))), name);
  }
  const emittedCss = names.filter((name) => name.endsWith('.css')).map((name) => readFileSync(join(dist, name), 'utf8')).join('\n');
  assert.equal((emittedCss.match(new RegExp(`${baseUrl}/`, 'g')) ?? []).length, 42);
  assert.ok(!emittedCss.includes('__WRITER_FONT_ASSET_BASE__'));

  const server = await preview({ configFile: false, root, publicDir: false, plugins: [writerFontAssets()],
    logLevel: 'silent', build: { outDir: 'dist' }, preview: { host: '127.0.0.1', port: 0 } });
  context.after(() => server.close());
  const origin = serverOrigin(server);
  for (const name of ['Arimo-Regular.ttf', 'manifest.json', 'SHA256SUMS', 'NOTICE.txt', 'Arimo.OFL.txt']) {
    const response = await globalThis.fetch(`${origin}${baseUrl}/${name}`);
    assert.equal(response.status, 200);
    assert.deepEqual(Buffer.from(await response.arrayBuffer()), readFileSync(join(source, name)));
  }
  assert.equal((await globalThis.fetch(`${origin}${baseUrl}/missing.ttf`)).status, 404);
  assert.equal((await globalThis.fetch(`${origin}/fonts/writer-fonts-v6/${'0'.repeat(64)}/Arimo-Regular.ttf`)).status, 404);

  writeFileSync(join(dist, 'assets/redundant.ttf'), 'a duplicate font must not ship');
  assert.throws(() => plugin.writeBundle.handler(), /Duplicate or unchecked font/);
  rmSync(join(dist, 'assets/redundant.ttf'));
  writeFileSync(join(dist, 'oversize.bin'), '');
  truncateSync(join(dist, 'oversize.bin'), maxBytes + 1);
  assert.throws(() => plugin.writeBundle.handler(), /oversize.bin exceeds the 25 MiB/);
});

test('real Vite development serves the identical font URLs, checked bytes and clear misses', async (context) => {
  const root = fixture(context);
  const server = await createServer({ configFile: false, root, publicDir: false, plugins: [writerFontAssets()],
    logLevel: 'silent', server: { host: '127.0.0.1', port: 0 } });
  context.after(() => server.close());
  await server.listen();
  const origin = serverOrigin(server);
  const css = await (await globalThis.fetch(`${origin}/src/styles.css`)).text();
  assert.ok(css.includes(`${baseUrl}/Arimo-Regular.ttf`));
  assert.ok(!css.includes('__WRITER_FONT_ASSET_BASE__'));
  const response = await globalThis.fetch(`${origin}${baseUrl}/Arimo-Regular.ttf`);
  assert.equal(response.status, 200);
  assert.equal(response.headers.get('content-type'), 'font/ttf');
  assert.deepEqual(Buffer.from(await response.arrayBuffer()), readFileSync(join(source, 'Arimo-Regular.ttf')));
  const etag = response.headers.get('etag');
  assert.ok(etag);
  assert.equal((await globalThis.fetch(`${origin}${baseUrl}/Arimo-Regular.ttf`, { headers: { 'If-None-Match': etag } })).status, 304);
  const head = await globalThis.fetch(`${origin}${baseUrl}/Arimo-Regular.ttf`, { method: 'HEAD' });
  assert.equal(head.status, 200);
  assert.equal((await head.arrayBuffer()).byteLength, 0);
  assert.equal((await globalThis.fetch(`${origin}${baseUrl}/Arimo-Regular.ttf`, { method: 'POST' })).status, 405);
  assert.equal((await globalThis.fetch(`${origin}${baseUrl}/missing.ttf`)).status, 404);
  assert.equal((await globalThis.fetch(`${origin}/fonts/invalid/manifest.json`)).status, 404);
});
