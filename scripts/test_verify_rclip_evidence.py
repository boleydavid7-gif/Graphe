"""Regression checks for the pinned evidence gate; stdlib only, no network."""
import importlib.util
import json
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location('rclip_evidence', Path(__file__).with_name('verify-rclip-evidence.py'))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class EvidenceTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix='graphe-evidence-test-')
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.evidence = self.root / 'docs/dependency-evidence/rclip-0.1.0'
        shutil.copytree(module.EVIDENCE, self.evidence)
        shutil.copyfile(module.ROOT / 'Cargo.lock', self.root / 'Cargo.lock')
        self.notice = self.root / 'crates/office-formats/THIRD_PARTY_NOTICES.txt'
        self.notice.parent.mkdir(parents=True)
        shutil.copyfile(module.ROOT / 'crates/office-formats/THIRD_PARTY_NOTICES.txt', self.notice)

    def verify(self):
        return module.verify(self.root, self.evidence)

    def test_retained_evidence_matches_current_dependencies(self):
        self.assertEqual(self.verify()['packages'], 2)

    def test_changed_dependency_needs_review(self):
        lock = self.root / 'Cargo.lock'
        lock.write_text(lock.read_text().replace(module.PINNED['rclip-core@0.1.0'][0], '0' * 64))
        with self.assertRaises(ValueError):
            self.verify()

    def test_corrupt_reference_is_rejected(self):
        with (self.evidence / 'Apache-2.0.txt').open('a') as file:
            file.write('changed terms')
        with self.assertRaises(ValueError):
            self.verify()

    def test_edited_upstream_snapshot_is_rejected(self):
        manifest = self.evidence / 'rclip-rtf/Cargo.toml'
        manifest.write_text(manifest.read_text().replace('MIT OR Apache-2.0', 'MIT'))
        with self.assertRaises(ValueError):
            self.verify()

    def test_missing_distributed_attribution_is_rejected(self):
        self.notice.write_text(self.notice.read_text().replace('Felix Schütt', 'missing author'))
        with self.assertRaises(ValueError):
            self.verify()

    def test_conflicting_source_identity_is_rejected(self):
        ledger = self.evidence / 'evidence.json'
        data = json.loads(ledger.read_text())
        data['packages'][0]['sourceComparisons'][0]['upstreamSha256'] = '0' * 64
        ledger.write_text(json.dumps(data))
        with self.assertRaises(ValueError):
            self.verify()


if __name__ == '__main__':
    unittest.main()
