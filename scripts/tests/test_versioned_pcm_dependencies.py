"""Fail-closed tests against the actual frozen oracle's Cargo metadata."""
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import verify_pcm_bitexact as gate

ROOT = Path(__file__).resolve().parents[2]


class VersionedDependenciesTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.baseline = Path(self.temporary.name) / 'baseline'
        self.candidate = Path(self.temporary.name) / 'candidate'
        paths = subprocess.check_output(
            ['git', 'ls-tree', '-r', '--name-only', gate.BASELINE_REVISION], cwd=ROOT, text=True,
        ).splitlines()
        for relative in paths:
            if relative != 'Cargo.lock' and not relative.endswith('Cargo.toml'):
                continue
            content = subprocess.check_output(['git', 'show', f'{gate.BASELINE_REVISION}:{relative}'], cwd=ROOT)
            for root in (self.baseline, self.candidate):
                path = root / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(content)
        for package in ('openjoc-api', 'openjoc-eac3'):
            path = self.candidate / f'crates/{package}/Cargo.toml'
            path.write_bytes(gate._expected_feature_manifest(path.read_bytes(), package))

    def bump(self):
        # Test fixture mutation only. Production normalization is identity/path-aware.
        for path in [self.candidate / 'Cargo.lock', *self.candidate.rglob('Cargo.toml')]:
            path.write_text(path.read_text().replace('"0.18.0"', '"0.19.0"'))

    def mutate(self, relative, old, new):
        path = self.candidate / relative
        text = path.read_text()
        self.assertIn(old, text)
        path.write_text(text.replace(old, new, 1))

    def test_current_and_version_only_candidate_pass_without_rewriting_oracle(self):
        before = {p: p.read_bytes() for p in self.baseline.rglob('*') if p.is_file()}
        gate.validate_versioned_dependencies(self.baseline, self.candidate)
        self.bump()
        # The old byte-equality guard rejects this legitimate release bump.
        self.assertEqual(gate.hash_file(self.baseline / "Cargo.lock"), gate.BASELINE_LOCK_SHA256)
        self.assertNotEqual(gate.hash_file(self.baseline / "Cargo.lock"), gate.hash_file(self.candidate / "Cargo.lock"))
        gate.validate_versioned_dependencies(self.baseline, self.candidate)
        self.assertEqual(before, {p: p.read_bytes() for p in before})
        self.assertIn('version = "0.19.0"', (self.candidate / 'Cargo.toml').read_text())

    def test_overlay_uses_frozen_manifests_and_rejects_extra_changes(self):
        committed = {
            f"crates/{package}/Cargo.toml": (self.baseline / f"crates/{package}/Cargo.toml").read_bytes()
            for package in ("openjoc-api", "openjoc-eac3")
        }
        def git_show(command, **kwargs):
            return committed[command[-1].removeprefix("HEAD:")]
        with mock.patch.object(gate, "git_head", return_value=gate.BASELINE_REVISION), mock.patch.object(
            gate.subprocess, "check_output", side_effect=git_show,
        ):
            gate.install_baseline_feature_overlay(self.baseline)
            gate.install_baseline_feature_overlay(self.baseline)
            for relative, content in committed.items():
                package = Path(relative).parent.name
                self.assertEqual((self.baseline / relative).read_bytes(), gate._expected_feature_manifest(content, package))
            path = self.baseline / "crates/openjoc-api/Cargo.toml"
            path.write_bytes(path.read_bytes().replace(b'"0.18.0"', b'"0.19.0"'))
            with self.assertRaisesRegex(gate.GateError, "unexpected baseline manifest"):
                gate.install_baseline_feature_overlay(self.baseline)

    def test_manifest_mutations_fail(self):
        self.bump()
        cases = [
            ('Cargo.toml', 'rust-version = "1.89"', 'rust-version = "1.90"'),
            ('Cargo.toml', 'version = "0.19.0"', 'version = "0.20.0"'),
            ('crates/openjoc-api/Cargo.toml', 'version = "0.19.0"', 'version = "0.18.0"'),
            ('crates/openjoc-api/Cargo.toml', 'path = "../openjoc-eac3"', 'path = "../openjoc-emdf"'),
            ('crates/openjoc-api/Cargo.toml', 'allocation-profile = []', 'allocation-profile = ["embedded-builtin-hrtf"]'),
            ('crates/openjoc-api/Cargo.toml', 'path = "../openjoc-eac3"', 'path = "../openjoc-eac3", registry = "other"'),
            ('tools/import-etsi-tables/Cargo.toml', 'sha2 = "0.10"', 'sha2 = "0.11"'),
            ('crates/openjoc-api/Cargo.toml', 'name = "openjoc-api"', 'name = "different-api"'),
            ('crates/openjoc-api/Cargo.toml', 'openjoc-eac3 = { version = "0.19.0", path = "../openjoc-eac3" }', ''),
        ]
        for relative, old, new in cases:
            with self.subTest(relative=relative, new=new):
                path = self.candidate / relative
                original = path.read_bytes()
                self.mutate(relative, old, new)
                with self.assertRaises(gate.GateError):
                    gate.validate_versioned_dependencies(self.baseline, self.candidate)
                path.write_bytes(original)

    def test_lock_mutations_fail(self):
        self.bump()
        path = self.candidate / 'Cargo.lock'
        original = path.read_bytes()
        cases = [
            ('name = "adler2"\nversion = "2.0.1"', 'name = "adler2"\nversion = "2.0.2"'),
            ('checksum = "320119', 'checksum = "000119'),
            ('registry+https://github.com/rust-lang/crates.io-index', 'registry+https://example.invalid/index'),
            (' "derive_arbitrary",', ' "adler2",'),
            ('name = "openjoc-api"\nversion = "0.19.0"', 'name = "openjoc-api"\nversion = "0.18.0"'),
            ('name = "openjoc-api"\nversion = "0.19.0"', 'name = "openjoc-api"\nversion = "0.19.0"\nsource = "git+https://example.invalid"'),
            ('name = "adler2"', 'name = "another-external-package"'),
        ]
        for old, new in cases:
            with self.subTest(new=new):
                self.mutate('Cargo.lock', old, new)
                with self.assertRaises(gate.GateError):
                    gate.validate_versioned_dependencies(self.baseline, self.candidate)
                path.write_bytes(original)
        path.write_bytes(original + b'\n[[package]]\nname = "new-package"\nversion = "1.0.0"\n')
        with self.assertRaises(gate.GateError):
            gate.validate_versioned_dependencies(self.baseline, self.candidate)


if __name__ == '__main__':
    unittest.main()
