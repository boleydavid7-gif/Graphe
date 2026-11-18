/** Publish the checked catalog once, at the same immutable URL used by WASM. */
import { Buffer } from 'node:buffer';
import { createHash } from 'node:crypto';
import { lstatSync, readFileSync, readdirSync } from 'node:fs';
import { join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const FONT_DIRECTORY = fileURLToPath(new URL('../../../crates/office-layout/fonts/', import.meta.url));
const CATALOG_ID = 'writer-fonts-v6';
const FONT_COUNT = 42;
const MAX_ASSET_BYTES = 25 * 1024 * 1024;
const TOKEN = '__WRITER_FONT_ASSET_BASE__';
const SAFE_NAME = /^[A-Za-z0-9][A-Za-z0-9._-]*$/;
const SHA256 = /^[a-f0-9]{64}$/;

function fail(message) {
  throw new Error(`Writer font assets: ${message}`);
}

function digest(bytes) {
  return createHash('sha256').update(bytes).digest('hex');
}

function catalog(directory) {
  const manifest = readFileSync(join(directory, 'manifest.json'));
  const checksums = readFileSync(join(directory, 'SHA256SUMS'));
  const entries = new Map();
  for (const line of checksums.toString('utf8').trimEnd().split('\n')) {
    const parsed = /^([a-f0-9]{64}) {2}([A-Za-z0-9][A-Za-z0-9._-]*)$/.exec(line);
    if (!parsed || entries.has(parsed[2])) fail('SHA256SUMS contains an invalid or duplicate entry.');
    entries.set(parsed[2], parsed[1]);
  }
  const hash = digest(manifest);
  if (entries.get('manifest.json') !== hash) fail('manifest.json does not match SHA256SUMS.');
  const model = JSON.parse(manifest.toString('utf8'));
  if (model.catalog_id !== CATALOG_ID || model.format_version !== 1 || !Array.isArray(model.entries)) {
    fail('Unsupported font catalog identity or version.');
  }
  const names = new Set(['manifest.json', 'NOTICE.txt']);
  const fonts = new Set();
  const licenses = new Set();
  for (const entry of model.entries) {
    if (!entry || typeof entry.file !== 'string' || !SAFE_NAME.test(entry.file) || names.has(entry.file)
      || !SHA256.test(entry.sha256) || !Number.isSafeInteger(entry.byte_length) || entry.byte_length <= 0
      || !['font', 'license'].includes(entry.kind)) fail('Invalid or duplicate manifest entry.');
    names.add(entry.file);
    if (entries.get(entry.file) !== entry.sha256) fail(`${entry.file} has inconsistent manifest/checksum hashes.`);
    if (entry.kind === 'font') {
      if (!entry.file.endsWith('.ttf')) fail('The current catalog must publish its original TTF faces.');
      fonts.add(entry.file);
    } else {
      licenses.add(entry.file);
    }
  }
  if (fonts.size !== FONT_COUNT || entries.size !== names.size
    || [...entries.keys()].some((name) => !names.has(name))) fail('Incomplete or unexpected checked catalog inventory.');
  for (const entry of model.entries) {
    if (entry.kind === 'font' && !licenses.has(entry.license_file)) fail(`${entry.file} has no checked license.`);
  }
  for (const entry of model.derived_files ?? []) {
    if (!names.has(entry.file) || entries.get(entry.file) !== entry.sha256) fail("Derived notice hash mismatch.");
    const bytes = readFileSync(join(directory, entry.file));
    if (bytes.length !== entry.byte_length || digest(bytes) !== entry.sha256) fail("Derived notice does not match SHA256SUMS.");
  }
  const assets = new Map();
  for (const name of names) {
    const bytes = name === 'manifest.json' ? manifest : readFileSync(join(directory, name));
    if (bytes.length > MAX_ASSET_BYTES) fail(`${name} exceeds the 25 MiB static asset limit.`);
    if (digest(bytes) !== entries.get(name)) fail(`${name} does not match SHA256SUMS.`);
    const entry = model.entries.find((candidate) => candidate.file === name);
    if (entry && bytes.length !== entry.byte_length) fail(`${name} does not match its declared byte length.`);
    assets.set(name, bytes);
  }
  assets.set('SHA256SUMS', checksums);
  return { hash, fonts, assets, baseUrl: `/fonts/${CATALOG_ID}/${hash}` };
}

function sizeGuard(name, bytes) {
  if (bytes > MAX_ASSET_BYTES) fail(`${name} exceeds the 25 MiB static asset limit (${bytes} bytes).`);
}

function validateDist(directory, publication) {
  const expected = new Set([...publication.assets.keys()].map((name) => `${publication.baseUrl.slice(1)}/${name}`));
  const published = new Map();
  function visit(current) {
    for (const entry of readdirSync(current, { withFileTypes: true })) {
      const path = join(current, entry.name);
      const name = relative(directory, path).replaceAll('\\', '/');
      const stat = lstatSync(path);
      if (stat.isSymbolicLink()) fail(`Unexpected symbolic link in production assets: ${name}.`);
      if (stat.isDirectory()) { visit(path); continue; }
      if (!stat.isFile()) fail(`Unexpected non-file production asset: ${name}.`);
      sizeGuard(name, stat.size);
      if (/\.(?:ttf|otf|woff2?)$/i.test(name) && !expected.has(name)) {
        fail(`Duplicate or unchecked font outside the canonical catalog: ${name}.`);
      }
      if (name.startsWith(`${publication.baseUrl.slice(1)}/`)) {
        if (!expected.delete(name)) fail(`Unexpected catalog asset: ${name}.`);
        const file = name.slice(publication.baseUrl.length);
        const bytes = readFileSync(path);
        if (!bytes.equals(publication.assets.get(file))) fail(`Published ${name} differs from the checked original.`);
        published.set(file, bytes);
      }
    }
  }
  visit(directory);
  if (expected.size) fail(`Missing published catalog assets: ${[...expected].join(', ')}.`);
  return published;
}

function middleware(publication, assets) {
  return (request, response, next) => {
    const pathname = (request.url ?? '').split('?')[0];
    if (!pathname.startsWith('/fonts/')) { next(); return; }
    const prefix = `${publication.baseUrl}/`;
    const name = pathname.startsWith(prefix) ? pathname.slice(prefix.length) : '';
    const bytes = assets.get(name);
    if (!bytes) { response.statusCode = 404; response.end('Font asset not found.'); return; }
    if (!['GET', 'HEAD'].includes(request.method ?? '')) {
      response.statusCode = 405;
      response.setHeader('Allow', 'GET, HEAD');
      response.end();
      return;
    }
    const etag = `"${digest(bytes)}"`;
    response.setHeader('ETag', etag);
    response.setHeader('Cache-Control', 'public, max-age=31536000, immutable');
    if (request.headers['if-none-match'] === etag) { response.statusCode = 304; response.end(); return; }
    response.setHeader('Content-Type', name.endsWith('.ttf') ? 'font/ttf'
      : name.endsWith('.json') ? 'application/json; charset=utf-8' : 'text/plain; charset=utf-8');
    response.setHeader('Content-Length', bytes.length);
    response.end(request.method === 'HEAD' ? undefined : bytes);
  };
}

function previousExternal(external, id, parent, resolved) {
  if (typeof external === 'function') return external(id, parent, resolved);
  return (Array.isArray(external) ? external : external ? [external] : [])
    .some((item) => typeof item === 'string' ? item === id : item.test(id));
}

/**
 * Validate and publish the source-controlled catalog without font conversion.
 * CSS tokens resolve before Vite's asset processing; canonical URLs are external
 * to that graph, so they do not produce another set of hashed font binaries.
 * @param {{ fontsDirectory?: string }} [options]
 * @returns {import('vite').Plugin}
 */
export function writerFontAssets(options = {}) {
  const directory = options.fontsDirectory ?? FONT_DIRECTORY;
  let publication = catalog(directory);
  let outputDirectory;
  let building = false;
  return {
    name: 'vite-plugin-writer-font-assets',
    enforce: 'pre',
    config(config) {
      const external = config.build?.rolldownOptions?.external;
      return {
        build: { rolldownOptions: { external: (id, parent, resolved) =>
          id.startsWith(`${publication.baseUrl}/`) || previousExternal(external, id, parent, resolved) } },
      };
    },
    configResolved(config) {
      building = config.command === 'build';
      outputDirectory = resolve(config.root, config.build.outDir);
    },
    buildStart() {
      const checked = catalog(directory);
      if (checked.hash !== publication.hash) fail('The manifest changed during initialization; restart the build.');
      publication = checked;
      if (building) {
        for (const [name, source] of publication.assets) {
          this.emitFile({ type: 'asset', fileName: `${publication.baseUrl.slice(1)}/${name}`, source });
        }
      }
    },
    transform(code, id) {
      if (!id.split('?')[0].endsWith('.css') || !code.includes(TOKEN)) return null;
      const transformed = code.replace(/url\((['"])__WRITER_FONT_ASSET_BASE__\/([^'"()]+)\1\)/g,
        (_match, quote, name) => {
          if (!publication.fonts.has(name)) fail(`CSS references an unchecked font: ${name}.`);
          return `url(${quote}${publication.baseUrl}/${name}${quote})`;
        });
      if (transformed.includes(TOKEN)) fail('Unresolved font URL token in CSS.');
      return { code: transformed, map: null };
    },
    configureServer(server) {
      server.middlewares.use(middleware(publication, publication.assets));
    },
    configurePreviewServer(server) {
      server.middlewares.use(middleware(publication, validateDist(outputDirectory, publication)));
    },
    generateBundle: {
      order: 'post',
      handler(_options, bundle) {
        for (const [name, output] of Object.entries(bundle)) {
          const source = output.type === 'asset' ? output.source : output.code;
          sizeGuard(name, typeof source === 'string' ? Buffer.byteLength(source) : source.byteLength);
        }
      },
    },
    writeBundle: {
      order: 'post',
      handler() { validateDist(outputDirectory, publication); },
    },
  };
}
