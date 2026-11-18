#!/usr/bin/env node
/**
 * Graphe public-source PRECHECK, not license clearance or a complete secret scan.
 * Run from a local full checkout: node scripts/check-public-release.mjs
 * This checks known metadata and flags suspicious tracked files without printing secrets.
 * Review docs/OPEN_SOURCE_RELEASE.md and use full-history scanners separately.
 */
import { readFileSync, existsSync, statSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { resolve } from 'node:path';

const root = resolve(import.meta.dirname, '..');
const load = (path) => readFileSync(resolve(root, path), 'utf8');
const issues = [];
const warnings = [];
const assert = (test, detail) => { if (!test) issues.push(detail); };

const license = load('LICENSE');
assert(license.startsWith('Mozilla Public License Version 2.0'), 'Root LICENSE is not MPL-2.0 text.');
assert(license.includes('Exhibit A - Source Code Form License Notice'), 'Root LICENSE has no MPL Exhibit A.');
const cargo = load('Cargo.toml');
assert(/license\s*=\s*"MPL-2\.0"/.test(cargo), 'Rust workspace license metadata differs.');
const browser = JSON.parse(load('apps/writer/package.json'));
assert(browser.license === 'MPL-2.0', 'Browser package license metadata differs.');
for (const name of ['office-core','office-docx','office-layout','office-formats','office-wasm']) {
  assert(/license\.workspace\s*=\s*true/.test(load('crates/' + name + '/Cargo.toml')),
    name + ' crate does not inherit the intended license.');
}
const manifest = JSON.parse(load('crates/office-layout/fonts/manifest.json'));
const faces = manifest.entries.filter((entry) => entry.kind === 'font');
const noticeAssets = manifest.entries.filter((entry) => entry.kind === 'license');
const knownNotices = new Set(noticeAssets.map((entry) => entry.file));
for (const face of faces) assert(knownNotices.has(face.license_file),
  'Missing cataloged OFL notice: ' + face.license_file);
assert(faces.length === 42, 'Pinned font catalog changed; review licenses/hashes.');
assert(noticeAssets.length === 21, 'Font family license inventory changed.');
for (const doc of ['CONTRIBUTING.md','CODE_OF_CONDUCT.md','GOVERNANCE.md','SECURITY.md',
  'TRADEMARKS.md','NOTICE.md','docs/OPEN_SOURCE_RELEASE.md']) {
  assert(existsSync(resolve(root, doc)), 'Missing community/release file: ' + doc);
}

let tracked = [];
try {
  tracked = execFileSync('git', ['ls-files', '-z'], { cwd: root, encoding: 'utf8' }).split('\0').filter(Boolean);
} catch {
  warnings.push('Cannot list tracked files: this precheck requires a full local Git checkout.');
}
const possibleSecrets = [
  /-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----/,
  /\bgh[pousr]_[A-Za-z0-9]{32,}\b/,
  /\bgithub_pat_[A-Za-z0-9_]{40,}\b/,
  /\bAKIA[0-9A-Z]{16}\b/,
  /\bsk_(?:live|test)_[A-Za-z0-9]{16,}\b/,
];
const highRiskNames = /(^|\/)(?:\.env(?:\.|$)|\.npmrc$|\.pypirc$|id_rsa$|.*\.(?:pem|p12|pfx|key|sqlite|db|bak|log)$)/i;
const externalBinary = /\.(?:docx|odt|rtf|pdf|png|jpe?g|svg)$/i;
let binaryCount = 0;
let scanned = 0;
const suspiciousPaths = [];
for (const path of tracked) {
  if (highRiskNames.test(path)) suspiciousPaths.push(path);
  const absolute = resolve(root, path);
  if (!existsSync(absolute) || !statSync(absolute).isFile()) continue;
  if (externalBinary.test(path)) binaryCount += 1;
  if (statSync(absolute).size > 1_000_000 || /\.(?:ttf|woff2?|png|jpe?g|docx|odt|pdf|wasm|zip)$/i.test(path)) continue;
  const content = readFileSync(absolute, 'utf8');
  scanned += 1;
  if (possibleSecrets.some((pattern) => pattern.test(content)))
    suspiciousPaths.push(path);
}
if (suspiciousPaths.length) issues.push('Potential sensitive tracked paths (review, do not blindly delete): ' +
  [...new Set(suspiciousPaths)].join(', '));
warnings.push('Found ' + binaryCount + ' tracked documents/images requiring provenance and metadata review.');
warnings.push('This scanner does not examine Git history, screenshots, binaries or installed dependency license files.');
warnings.push('Proprietary work history must not be copied into a new public repo.');
warnings.push('Do not interpret a clean result as consent to publish.');
console.log(JSON.stringify({ status: issues.length ? 'REVIEW_REQUIRED' : 'METADATA_PRECHECK_ONLY',
  trackedFiles: tracked.length, scannedTextFiles: scanned, fontFaces: faces.length,
  fontLicenseFiles: noticeAssets.length, issues, warnings }, null, 2));
if (issues.length) process.exitCode = 1;
