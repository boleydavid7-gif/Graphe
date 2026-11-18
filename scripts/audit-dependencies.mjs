#!/usr/bin/env node
/**
 * Graphe dependency-license inventory. No network requests or global installs.
 *
 * Usage:
 *   node scripts/audit-dependencies.mjs
 *   node scripts/audit-dependencies.mjs --static-only
 *
 * Requires Rust/Cargo with registry metadata available for complete runtime
 * analysis. --static-only produces lockfile-only evidence, NOT license clearance.
 * It never changes source or lockfiles.
 */
import { readFileSync, mkdirSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { resolve, join } from 'node:path';

const root = resolve(import.meta.dirname, '..');
const staticOnly = process.argv.includes('--static-only');
const outDir = resolve(root, 'artifacts/dependency-audit');
const read = (file) => readFileSync(resolve(root, file), 'utf8');
const quote = (value) => String(value ?? '').replace(/\|/g, '\\|').replace(/\r?\n/g, ' ');

/** Cargo.lock v4 uses simple package blocks; parse only stable lockfile fields. */
function parseCargoLock(raw) {
  return raw.split(/(?=^\[\[package\]\]$)/m)
    .filter((block) => block.startsWith('[[package]]'))
    .map((block) => {
      const field = (key) => block.match(new RegExp('^' + key + ' = "([^"]+)"', 'm'))?.[1] ?? null;
      return { name: field('name'), version: field('version'), checksum: field('checksum'),
        source: field('source'), identity: field('name') + '@' + field('version') };
    });
}
const crates = parseCargoLock(read('Cargo.lock'));
const npm = JSON.parse(read('apps/writer/package-lock.json'));
const js = Object.entries(npm.packages).filter(([path]) => path)
  .map(([path, item]) => ({ name: path.replace(/^node_modules\//, ''), version: item.version,
    license: item.license ?? null, dev: !!item.dev, optional: !!item.optional }));
const notices = [
  'crates/office-core/THIRD_PARTY_NOTICES.txt',
  'crates/office-layout/THIRD_PARTY_NOTICES.txt',
  'crates/office-formats/THIRD_PARTY_NOTICES.txt',
].map((path) => ({ path, content: read(path) }));
function hint(item) {
  const escapedName = item.name.replace(/[.*+?^$\{\}()|[\]\\]/g, '\\$&');
  const escapedVersion = item.version.replace(/[.*+?^$\{\}()|[\]\\]/g, '\\$&');
  const pattern = new RegExp('(?:^|\\s)' + escapedName + '\\s+' + escapedVersion + '(?:\\b|$)');
  return notices.filter((file) => pattern.test(file.content)).map((file) => file.path);
}
function cargoMetadata(triple) {
  const result = spawnSync('cargo',
    ['metadata', '--locked', '--format-version', '1', '--filter-platform', triple],
    { cwd: root, encoding: 'utf8', maxBuffer: 60 * 1024 * 1024, timeout: 180000 });
  if (result.error || result.status !== 0) {
    const detail = String(result.error?.message ?? result.stderr ?? 'Unknown cargo error');
    return { error: detail.slice(0, 1600), triple };
  }
  try { return { data: JSON.parse(result.stdout), triple }; }
  catch (error) { return { error: 'Invalid Cargo metadata: ' + String(error), triple }; }
}
/** Filter the Cargo resolve tree to normal dependency edges (not tests/build tools). */
function normalGraph(metadata) {
  const nodes = new Map((metadata.resolve?.nodes ?? []).map((node) => [node.id, node]));
  const visited = new Set(), queue = [];
  for (const id of metadata.workspace_members ?? []) {
    const pack = metadata.packages.find((entry) => entry.id === id);
    if (pack?.name?.startsWith('office-')) queue.push(id);
  }
  while (queue.length) {
    const id = queue.pop();
    if (!id || visited.has(id)) continue;
    visited.add(id);
    const node = nodes.get(id);
    for (const dep of node?.deps ?? []) {
      if (!dep.dep_kinds?.length || dep.dep_kinds.some((kind) => kind.kind === null))
        queue.push(dep.pkg);
    }
  }
  return new Set(metadata.packages.filter((pack) => visited.has(pack.id))
    .map((pack) => pack.name + '@' + pack.version));
}
const errors = [];
const targets = staticOnly ? [] : ['wasm32-unknown-unknown', process.platform === 'win32'
  ? 'x86_64-pc-windows-msvc' : process.platform === 'darwin'
  ? 'aarch64-apple-darwin' : 'x86_64-unknown-linux-gnu'].map(cargoMetadata);
const metadata = targets.filter((target) => target.data);
for (const target of targets) if (target.error)
  errors.push(target.triple + ': ' + target.error);
const runtime = Object.fromEntries(metadata.map((target) => [target.triple, normalGraph(target.data)]));
const metaPackages = new Map(metadata.flatMap((target) => target.data.packages
  .map((pack) => [pack.name + '@' + pack.version, pack])));
const ext = crates.filter((item) => !item.name.startsWith('office-'))
  .map((item) => {
    const pack = metaPackages.get(item.identity);
    return { ...item, license: pack?.license ?? null, licenseFile: pack?.license_file ?? null,
      repository: pack?.repository ?? null,
      targets: Object.keys(runtime).filter((triple) => runtime[triple].has(item.identity)),
      noticeMentions: hint(item),
      review: item.name === 'rclip-rtf' || item.name === 'rclip-core'
        ? 'UPSTREAM_LICENSE_FILE_EVIDENCE_REQUIRED'
        : pack?.license || pack?.license_file ? 'DECLARATION_ONLY_NOT_CLEARED'
        : 'MISSING_METADATA_OR_NOT_RESOLVED' };
  });
const standalone = ext.filter((item) => !item.license && !item.licenseFile);
const potential = ext.filter((item) => /(?:^|[\s(])(A?GPL|LGPL|SSPL|CDDL|EUPL|MPL|EPL)(?:[-\s)]|$)/i.test(item.license ?? ''));
const prod = js.filter((p) => !p.dev);
const summary = {
  source: 'Committed Cargo.lock and apps/writer/package-lock.json',
  metadataComplete: metadata.length === 2,
  rustPackageRecords: crates.length, externalRustPackageRecords: ext.length,
  npmPackageRecords: js.length, npmProductionRecords: prod.length,
  missingCargoLicenseMetadataRecords: standalone.length,
  potentiallyReciprocalRustLicenses: potential.length,
  rustWithoutExactVersionMentionsInThreeSubsetNotices: ext.filter((c) => !c.noticeMentions.length).length,
  manualRTFLicensingEvidence: ['rclip-rtf', 'rclip-core'],
  targetsAnalyzed: metadata.map((item) => item.triple),
};
const report = {
  generatedAt: new Date().toISOString(), status: 'EVIDENCE_ONLY_NOT_RELEASE_CLEARANCE',
  summary, errors, cargo: ext, npm: js,
};
const table = [
  '# Graphe dependency audit — generated evidence, NOT release clearance',
  '',
  'Generated: ' + report.generatedAt,
  '',
  'The underlying lockfile is authoritative for package identities, NOT for license terms.',
  'Cargo metadata supplies upstream declarations; license texts, distribution, provenance',
  'and final compiled contents still require human review.',
  '',
  '## Summary',
  '',
  ...Object.entries(summary).map(([key, value]) => '- **' + key + ':** ' + (Array.isArray(value) ? value.join(', ') : value)),
  '',
  '## Remaining evidence issues',
  '',
  '- Missing/resolution-dependent Cargo license metadata: ' + standalone.length,
  '- Potentially reciprocal Rust license declarations: ' + potential.length,
  '- `rclip-rtf` and `rclip-core` need exact upstream license-file evidence.',
  '- A match in an existing notice file is a *search hint*, not proof that notice obligations are met.',
  '- npm lockfile licenses are package declarations, not final produced-asset review.',
  ...errors.map((item) => '- Cargo metadata unavailable: ' + quote(item)),
  '',
  '| Cargo package | Declared license (metadata only) | Reachability | Subset notice reference | Review status |',
  '| --- | --- | --- | --- | --- |',
  ...ext.map((item) => '| `' + quote(item.identity) + '` | ' + quote(item.license ?? '(not verified)') +
    ' | ' + quote(item.targets.join(', ') || '(not established)') +
    ' | ' + quote(item.noticeMentions.join(', ') || '—') +
    ' | ' + quote(item.review) + ' |'),
  '',
  '## npm (lockfile declarations)',
  '',
  '| npm package | License | Phase |',
  '| --- | --- | --- |',
  ...js.map((item) => '| `' + quote(item.name) + '@' + quote(item.version) +
    '` | ' + quote(item.license ?? 'UNKNOWN') + ' | ' +
    (item.dev ? 'development' : 'production') + (item.optional ? ' (optional)' : '') + ' |'),
  '',
  '## Release gating',
  '',
  'This report is not a consolidated redistributable license notice, a legal opinion,',
  'or a verified SBOM of the final binaries. See `docs/OPEN_SOURCE_RELEASE.md`.',
].join('\n') + '\n';
mkdirSync(outDir, { recursive: true });
writeFileSync(join(outDir, 'dependency-audit.json'), JSON.stringify(report, null, 2) + '\n');
writeFileSync(join(outDir, 'dependency-audit.md'), table);
console.log(JSON.stringify({ output: outDir, ...summary, errors }, null, 2));
if (errors.length || !metadata.length) process.exitCode = 2;
