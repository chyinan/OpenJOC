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

    def validate_without_rewriting_metadata(self):
        def snapshot():
            return {
                path: path.read_bytes()
                for root in (self.baseline, self.candidate)
                for path in root.rglob('*') if path.is_file()
            }
        before = snapshot()
        try:
            gate.validate_versioned_dependencies(self.baseline, self.candidate)
        finally:
            self.assertEqual(before, snapshot())

    def add_native_scene_dependency(self):
        version = gate.tomllib.loads((self.candidate / 'Cargo.toml').read_text())['workspace']['package']['version']
        relative = 'crates/gst-plugin-openjoc/Cargo.toml'
        marker = f'openjoc-joc = {{ version = "{version}", path = "../openjoc-joc", optional = true }}'
        self.mutate(relative, marker, marker + '\n' +
                    f'openjoc-scene = {{ version = "{version}", path = "../openjoc-scene", optional = true }}')
        self.mutate(relative, '    "dep:openjoc-joc",', '    "dep:openjoc-joc",\n    "dep:openjoc-scene",')
        path = self.candidate / 'Cargo.lock'
        before, marker, after = path.read_text().partition('name = "gst-plugin-openjoc"')
        self.assertTrue(marker)
        package, end, rest = after.partition('[[package]]')
        self.assertEqual(package.count(' "openjoc-joc",'), 1)
        package = package.replace(' "openjoc-joc",', ' "openjoc-joc",\n "openjoc-scene",', 1)
        path.write_text(before + marker + package + end + rest)

    def test_native_scene_exact_variant_at_both_supported_workspace_versions(self):
        self.add_native_scene_dependency()
        self.validate_without_rewriting_metadata()
        self.bump()
        self.validate_without_rewriting_metadata()
        self.mutate('crates/openjoc-scene/Cargo.toml', 'serde_json = "1"',
                    'serde_json = { version = "1", features = ["float_roundtrip"] }')
        self.validate_without_rewriting_metadata()

    def test_native_scene_manifest_variant_is_exact(self):
        self.bump()
        self.add_native_scene_dependency()
        relative = 'crates/gst-plugin-openjoc/Cargo.toml'
        path = self.candidate / relative
        original = path.read_bytes()
        dependency = 'openjoc-scene = { version = "0.19.0", path = "../openjoc-scene", optional = true }'
        cases = [
            (dependency, dependency.replace('0.19.0', '0.18.0')),
            (dependency, dependency.replace('../openjoc-scene', '../openjoc-joc')),
            (dependency, dependency.replace(', optional = true', '')),
            (dependency, dependency.replace('optional = true', 'optional = false')),
            (dependency, dependency.replace('optional = true', 'optional = 1')),
            (dependency, dependency.replace('optional = true', 'optional = 1.0')),
            (dependency, dependency.replace(' }', ', features = [] }')),
            (dependency, dependency.replace(' }', ', default-features = false }')),
            (dependency, dependency.replace(' }', ', registry = "other" }')),
            (dependency, dependency.replace(' }', ', git = "https://example.invalid/scene" }')),
            (dependency, dependency.replace(' }', ', package = "openjoc-scene" }')),
            (dependency, dependency.replace('openjoc-scene =', 'scene-alias =').replace(' }', ', package = "openjoc-scene" }')),
            (dependency, ''),
            ('    "dep:openjoc-scene",\n', ''),
            ('    "dep:openjoc-scene",', '    "dep:openjoc-scene",\n    "dep:openjoc-scene",'),
            ('    "dep:openjoc-scene",', '    "openjoc-scene",'),
            ('    "dep:openjoc-joc",\n    "dep:openjoc-scene",', '    "dep:openjoc-scene",\n    "dep:openjoc-joc",'),
            ('default = []', 'default = ["dep:openjoc-scene"]'),
            (dependency, '[dev-dependencies]\n' + dependency),
            (dependency, '[build-dependencies]\n' + dependency),
            (dependency, '[target.\'cfg(unix)\'.dependencies]\n' + dependency),
        ]
        for old, new in cases:
            with self.subTest(new=new):
                self.mutate(relative, old, new)
                with self.assertRaises(gate.GateError):
                    self.validate_without_rewriting_metadata()
                path.write_bytes(original)
        # Activation in defaults cannot replace activation in gstreamer.
        self.mutate(relative, '    "dep:openjoc-scene",\n', '')
        self.mutate(relative, 'default = []', 'default = ["dep:openjoc-scene"]')
        with self.assertRaises(gate.GateError):
            self.validate_without_rewriting_metadata()

    def test_native_scene_manifest_and_lock_additions_are_coupled(self):
        self.bump()
        manifest = self.candidate / 'crates/gst-plugin-openjoc/Cargo.toml'
        lock = self.candidate / 'Cargo.lock'
        original_manifest, original_lock = manifest.read_bytes(), lock.read_bytes()
        self.add_native_scene_dependency()
        allowed_manifest, allowed_lock = manifest.read_bytes(), lock.read_bytes()
        for manifest_bytes, lock_bytes in (
            (allowed_manifest, original_lock),
            (original_manifest, allowed_lock),
        ):
            with self.subTest(manifest_added=manifest_bytes == allowed_manifest):
                manifest.write_bytes(manifest_bytes)
                lock.write_bytes(lock_bytes)
                with self.assertRaises(gate.GateError):
                    self.validate_without_rewriting_metadata()

    def test_native_scene_lock_variant_rejects_edge_drift(self):
        self.bump()
        self.add_native_scene_dependency()
        path = self.candidate / 'Cargo.lock'
        original = path.read_text()
        before, marker, after = original.partition('name = "gst-plugin-openjoc"')
        package, end, rest = after.partition('[[package]]')
        edge = ' "openjoc-scene",\n'
        self.assertEqual(package.count(edge), 1)
        for replacement in ('', edge + edge, ' "openjoc-joc",\n',
                            ' "openjoc-scene 0.18.0",\n', ' "openjoc-scene 0.19.0",\n',
                            ' "openjoc-scene 0.20.0",\n', ' "scene-alias",\n'):
            with self.subTest(replacement=replacement):
                path.write_text(before + marker + package.replace(edge, replacement, 1) + end + rest)
                with self.assertRaises(gate.GateError):
                    self.validate_without_rewriting_metadata()
        path.write_text(before + marker + package.replace(' "openjoc-joc",\n' + edge, edge + ' "openjoc-joc",\n', 1) + end + rest)
        with self.assertRaises(gate.GateError):
            self.validate_without_rewriting_metadata()
        # Moving the only new edge to another workspace package is not equivalent.
        wrong = before + marker + package.replace(edge, '', 1) + end + rest
        left, joc_marker, right = wrong.partition('name = "openjoc-joc"\n')
        joc, separator, remaining = right.partition('[[package]]')
        self.assertTrue(joc_marker)
        self.assertNotIn(edge, joc)
        self.assertEqual(joc.count('dependencies = [\n'), 1)
        joc = joc.replace('dependencies = [\n', 'dependencies = [\n' + edge, 1)
        wrong = left + joc_marker + joc + separator + remaining
        gate.tomllib.loads(wrong)  # This negative control must remain valid TOML.
        path.write_text(wrong)
        with self.assertRaises(gate.GateError):
            self.validate_without_rewriting_metadata()

    def test_native_scene_allowance_preserves_all_unrelated_metadata(self):
        self.bump()
        self.add_native_scene_dependency()
        cases = [
            ('crates/gst-plugin-openjoc/Cargo.toml', 'publish = false', 'publish = true'),
            ('crates/gst-plugin-openjoc/Cargo.toml', 'publish = false', 'publish = 0'),
            ('crates/gst-plugin-openjoc/Cargo.toml', 'publish = false', 'publish = 0.0'),
            ('crates/gst-plugin-openjoc/Cargo.toml', 'version.workspace = true', 'version.workspace = 1'),
            ('crates/gst-plugin-openjoc/Cargo.toml', 'glib = { version = "0.21", optional = true }', 'glib = { version = "0.21", optional = 1 }'),
            ('crates/gst-plugin-openjoc/Cargo.toml', 'features = ["v1_20"]', 'features = []'),
            ('crates/openjoc-api/Cargo.toml', 'name = "openjoc-api"', 'name = "different-api"'),
            ('crates/openjoc-api/Cargo.toml', 'version.workspace = true', 'version.workspace = 1'),
            ('Cargo.toml', 'rust-version = "1.89"', 'rust-version = "1.90"'),
            ('Cargo.toml', 'priority = -1', 'priority = -1.0'),
            ('Cargo.lock', 'version = 4', 'version = 4.0'),
            ('Cargo.lock', 'name = "adler2"\nversion = "2.0.1"', 'name = "adler2"\nversion = "2.0.2"'),
            ('Cargo.lock', 'checksum = "320119', 'checksum = "000119'),
            ('Cargo.lock', 'registry+https://github.com/rust-lang/crates.io-index', 'registry+https://example.invalid/index'),
            ('Cargo.lock', ' "derive_arbitrary",', ' "adler2",'),
            ('Cargo.lock', 'name = "openjoc-api"\nversion = "0.19.0"', 'name = "openjoc-api"\nversion = "0.18.0"'),
        ]
        for relative, old, new in cases:
            with self.subTest(relative=relative, new=new):
                path = self.candidate / relative
                original = path.read_bytes()
                self.mutate(relative, old, new)
                with self.assertRaises(gate.GateError):
                    self.validate_without_rewriting_metadata()
                path.write_bytes(original)
        path = self.candidate / 'Cargo.lock'
        path.write_text(path.read_text() + '\n[[package]]\nname = "new-package"\nversion = "1.0.0"\n')
        with self.assertRaises(gate.GateError):
            self.validate_without_rewriting_metadata()

    def test_current_repository_metadata_is_accepted_without_rewrites(self):
        for path in self.candidate.rglob('*'):
            if path.is_file():
                path.write_bytes((ROOT / path.relative_to(self.candidate)).read_bytes())
        self.validate_without_rewriting_metadata()

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

    def test_scene_float_roundtrip_is_the_only_parser_allowance(self):
        self.bump()
        relative = "crates/openjoc-scene/Cargo.toml"
        path = self.candidate / relative
        original = path.read_text()
        marker = 'serde_json = "1"'
        self.assertIn(marker, original)
        before = {p: p.read_bytes() for p in self.baseline.rglob('*') if p.is_file()}
        allowed = 'serde_json = { version = "1", features = ["float_roundtrip"] }'
        path.write_text(original.replace(marker, allowed, 1))
        gate.validate_versioned_dependencies(self.baseline, self.candidate)
        self.assertEqual(before, {p: p.read_bytes() for p in before})
        for dependency in (
            '{ version = "2", features = ["float_roundtrip"] }',
            '{ version = "1", features = ["float_roundtrip", "arbitrary_precision"] }',
            '{ version = "1", features = ["arbitrary_precision"] }',
            '{ version = "1", features = ["float_roundtrip"], default-features = false }',
            '{ version = "1", features = ["float_roundtrip"], registry = "other" }',
            '{ version = "1", features = ["float_roundtrip"], path = "../serde_json" }',
        ):
            with self.subTest(dependency=dependency):
                path.write_text(original.replace(marker, f"serde_json = {dependency}", 1))
                with self.assertRaises(gate.GateError):
                    gate.validate_versioned_dependencies(self.baseline, self.candidate)
        path.write_text(original.replace(marker, allowed, 1))
        # The parser allowance must not hide an unrelated change in this manifest.
        path.write_text(path.read_text().replace('features = ["derive"]', 'features = []'))
        with self.assertRaises(gate.GateError):
            gate.validate_versioned_dependencies(self.baseline, self.candidate)
        path.write_text(original)
        # Nor may another package acquire the same parser feature.
        other = self.candidate / "crates/openjoc-adm/Cargo.toml"
        self.assertIn(marker, other.read_text())
        other.write_text(other.read_text().replace(marker, allowed, 1))
        with self.assertRaises(gate.GateError):
            gate.validate_versioned_dependencies(self.baseline, self.candidate)

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
