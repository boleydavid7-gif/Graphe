#!/usr/bin/env python3
"""Read checksum-verified Cargo archives; collect evidence, never legal clearance.
No extraction, fetching, dependency changes, or invented license texts.
Run after audit-dependencies.mjs with the same CARGO_HOME.
"""
import hashlib
import io
import json
import os
from pathlib import Path
import re
import sys
import tarfile
import tomllib

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / 'artifacts/dependency-audit'
CARGO = Path(os.environ.get('CARGO_HOME', Path.home() / '.cargo'))
lock = tomllib.loads((ROOT / 'Cargo.lock').read_text())
inventory = json.loads((OUT / 'dependency-audit.json').read_text())
runtime = {p['identity'] for p in inventory['cargo'] if 'wasm32-unknown-unknown' in p['targets']}
records = []
notices = ['DRAFT EVIDENCE — NOT A CLEARED DISTRIBUTION NOTICE\n']
for package in lock['package']:
    if not package.get('source'):
        continue
    name, version = package['name'], package['version']
    identity = f'{name}@{version}'
    archives = list((CARGO / 'registry/cache').glob(f'*/{name}-{version}.crate'))
    record = {'identity': identity, 'wasmRuntimeGraph': identity in runtime,
              'expectedArchiveSha256': package['checksum'], 'files': [], 'status': 'ARCHIVE_UNAVAILABLE'}
    records.append(record)
    if not archives:
        continue
    data = archives[0].read_bytes()
    actual = hashlib.sha256(data).hexdigest()
    record['actualArchiveSha256'] = actual
    if actual != package['checksum']:
        record['status'] = 'ARCHIVE_CHECKSUM_MISMATCH'
        continue
    record['status'] = 'TEXT_EVIDENCE_ONLY_NOT_CLEARED'
    with tarfile.open(fileobj=io.BytesIO(data), mode='r:gz') as archive:
        for member in archive.getmembers():
            if not member.isfile():
                continue
            relative = '/'.join(member.name.split('/')[1:])
            basename = relative.rsplit('/', 1)[-1]
            wanted = bool(re.match(r'^(LICENSE|LICENCE|COPYING|COPYRIGHT|NOTICE|AUTHORS)(?:[._-].*)?$', basename, re.I))
            metadata = relative in ['Cargo.toml', 'Cargo.toml.orig', '.cargo_vcs_info.json']
            if not (wanted or metadata):
                continue
            if member.size > 8 * 1024 * 1024:
                record['status'] = 'OVERSIZED_EVIDENCE_FILE'
                continue
            content = archive.extractfile(member).read()
            record['files'].append({'path': relative, 'bytes': len(content),
                                    'sha256': hashlib.sha256(content).hexdigest(), 'licenseOrNotice': wanted})
            if relative == 'Cargo.toml':
                manifest = tomllib.loads(content.decode())['package']
                record['declaredLicense'] = manifest.get('license')
                record['declaredLicenseFile'] = manifest.get('license-file')
                record['authors'] = manifest.get('authors', [])
                record['repository'] = manifest.get('repository')
            if relative == '.cargo_vcs_info.json':
                record['vcs'] = json.loads(content)
            if wanted and identity in runtime:
                notices.append(f'\n===== {identity}: {relative} [sha256:{hashlib.sha256(content).hexdigest()}] =====\n')
                notices.append(content.decode('utf-8', errors='strict'))
        texts = [f for f in record['files'] if f['licenseOrNotice']]
        record['licenseTextFiles'] = len(texts)
        if not texts:
            record['status'] = 'NO_STANDALONE_LICENSE_TEXT_IN_ARCHIVE'
            if identity in runtime:
                notices.append(f'\n===== {identity}: NO STANDALONE LICENSE TEXT — REVIEW REQUIRED =====\n')
summary = {'externalRecords': len(records), 'wasmRuntimeGraphRecords': len(runtime),
           'archiveChecksumsVerified': sum(r.get('actualArchiveSha256') == r['expectedArchiveSha256'] for r in records),
           'statusCounts': {s: sum(r['status'] == s for r in records) for s in sorted({r['status'] for r in records})},
           'scope': 'Archive authenticity and text-file presence/hashes only; SPDX compatibility, copyright and actual optimized binary contents remain review items.'}
OUT.mkdir(parents=True, exist_ok=True)
(OUT / 'rust-license-text-evidence.json').write_text(json.dumps({'summary': summary, 'packages': records}, indent=2) + '\n')
(OUT / 'rust-wasm-notices-DRAFT.txt').write_text(''.join(notices))
print(json.dumps(summary, indent=2))
print('Unresolved:', [(r['identity'], r['status']) for r in records if r['status'] != 'TEXT_EVIDENCE_ONLY_NOT_CLEARED'])

# Installed production npm license evidence, after a fresh npm ci. No network.
npm_lock = json.loads((ROOT / 'apps/writer/package-lock.json').read_text())
npm_evidence = []
npm_notices = []
for name, package in npm_lock['packages'].items():
    if not name or package.get('dev'):
        continue
    directory = ROOT / 'apps/writer' / name
    files = []
    if directory.is_dir():
        for path in sorted(directory.iterdir()):
            if path.is_file() and re.match(r'^(LICENSE|LICENCE|COPYING|NOTICE)(?:[._-].*)?$', path.name, re.I):
                data = path.read_bytes()
                files.append({'file': path.name, 'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()})
                npm_notices.append(f"\n===== {name}@{package['version']}: {path.name} =====\n" + data.decode())
    npm_evidence.append({'package': name, 'version': package['version'], 'declaredLicense': package.get('license'),
                         'lockIntegrity': package.get('integrity'), 'licenseFiles': files,
                         'status': 'INSTALLED_TEXT_EVIDENCE_ONLY' if files else 'INSTALLED_LICENSE_TEXT_UNAVAILABLE'})
(OUT / 'npm-production-text-evidence.json').write_text(json.dumps(npm_evidence, indent=2) + '\n')
(OUT / 'npm-production-notices-DRAFT.txt').write_text(''.join(npm_notices))

# Missing texts are unresolved evidence, not a conclusion about permission.
# Corrupt archives or oversized evidence must never produce a successful check.
if any(r['status'] in {'ARCHIVE_CHECKSUM_MISMATCH', 'OVERSIZED_EVIDENCE_FILE'} for r in records):
    sys.exit(2)
