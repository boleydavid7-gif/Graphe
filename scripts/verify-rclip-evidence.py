#!/usr/bin/env python3
"""Verify the pinned rclip evidence dossier; does not grant release clearance."""
import hashlib
import json
import re
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
EVIDENCE = ROOT / 'docs/dependency-evidence/rclip-0.1.0'
PINNED = {
    'rclip-core@0.1.0': (
        '6414ce752a0d18ee3d000b6c76fccbc30e90b93c2a8c1e7fdd855ab9b7ea0af6',
        '2be8cabe96655f98bec3d5dc9781f1713cd46246',
    ),
    'rclip-rtf@0.1.0': (
        'e64929d6244eea54b6ae4276d5724ae8833c269b8156dd63d48b4f13d39a78ed',
        '371c7203faeb3a5f13604a574a7a2e37bba48226',
    ),
}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def verify(root=ROOT, evidence=EVIDENCE):
    ledger = json.loads((evidence / 'evidence.json').read_text())
    lock = tomllib.loads((root / 'Cargo.lock').read_text())['package']
    require({p['identity'] for p in ledger['packages']} == set(PINNED), 'Unexpected evidence packages')
    reference = (evidence / 'Apache-2.0.txt').read_bytes()
    digest = 'cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30'
    require(sha(reference) == digest == ledger['referenceLicense']['sha256'], 'Reference license changed')
    notice = (root / 'crates/office-formats/THIRD_PARTY_NOTICES.txt').read_text()
    marker = 'Applies to: pdf-writer 0.15.0, ryu 1.0.23. Also applies to rclip-rtf'
    block = notice[notice.index(marker):].split('\n\n', 1)[1]
    end = 'END OF TERMS AND CONDITIONS'
    terms = lambda text: re.sub(r'\s+', ' ', text[:text.index(end) + len(end)]).strip()
    require(terms(block) == terms(reference.decode()), 'Distributed Apache terms differ from official reference')
    for record in ledger['packages']:
        identity = record['identity']
        name, version = identity.split('@')
        for snapshot in record['snapshots']:
            require(sha((evidence / snapshot['file']).read_bytes()) == snapshot['sha256'], 'Snapshot changed: ' + snapshot['file'])
        checksum, commit = PINNED[identity]
        package = next(p for p in lock if p['name'] == name and p['version'] == version)
        require(package['checksum'] == checksum == record['archiveSha256'], 'Package checksum changed: ' + identity)
        require(record['sourceCommit'] == commit, 'Source commit changed: ' + identity)
        for snapshot in record['snapshots']:
            path = (evidence / snapshot['file']).resolve()
            require(path.is_relative_to(evidence.resolve()), 'Evidence path escapes dossier')
            require(sha(path.read_bytes()) == snapshot['sha256'], 'Snapshot changed: ' + snapshot['file'])
        manifest = tomllib.loads((evidence / name / 'Cargo.toml').read_text())['package']
        original = tomllib.loads((evidence / name / 'Cargo.toml.orig').read_text())['package']
        workspace = tomllib.loads((evidence / name / 'workspace-Cargo.toml').read_text())['workspace']['package']
        vcs = json.loads((evidence / name / '.cargo_vcs_info.json').read_text())
        require(manifest['name'] == name and manifest['version'] == version, 'Manifest identity mismatch')
        require(manifest['license'] == workspace['license'] == record['declaredLicense'] == 'MIT OR Apache-2.0', 'License declaration mismatch')
        require(original['license']['workspace'] is True, 'License inheritance mismatch')
        require(vcs['git']['sha1'] == commit and vcs['path_in_vcs'] == record['sourcePath'], 'VCS evidence mismatch')
        author = 'Felix Schütt <felix.schuett@maps4print.com>'
        require(manifest['authors'] == workspace['authors'] == record['authors'] == [author], 'Author metadata mismatch')
        entry = notice.split(name + ' ' + version + '\n', 1)[1].split('\n\n', 1)[0]
        require('Selected license: Apache-2.0' in entry and checksum in entry and author in entry and commit in entry, 'Distributed attribution mismatch')
        require(all(p['identical'] and p['archiveSha256'] == p['upstreamSha256'] for p in record['sourceComparisons']), 'Source comparison mismatch')
    return {'packages': len(PINNED), 'referenceLicense': 'Apache-2.0', 'status': 'PINNED_EVIDENCE_VERIFIED_NOT_LEGAL_CLEARANCE'}


if __name__ == '__main__':
    print(json.dumps(verify(), indent=2))
