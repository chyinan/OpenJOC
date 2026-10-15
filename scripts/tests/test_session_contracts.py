from pathlib import Path
import hashlib
import json
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import verify_session_contracts as gate


class SessionContractTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.capture = self.root / "capture"
        self.capture.mkdir()
        for name, data in {"case.f32le": bytes(range(32)), "case.txt": b"push=Ok\nframe=exact\n"}.items():
            (self.capture / name).write_bytes(data)
        self.expected = {p.name: gate.file_record(p) for p in self.capture.iterdir()}

    def test_complete_files_match(self):
        self.assertEqual(gate.compare_capture(self.capture, self.expected), self.expected)

    def test_single_bit_pcm_and_event_changes_fail(self):
        original = {p.name: p.read_bytes() for p in self.capture.iterdir()}
        for name, data in original.items():
            with self.subTest(name=name):
                changed = bytearray(data)
                changed[-1] ^= 1
                (self.capture / name).write_bytes(changed)
                with self.assertRaisesRegex(gate.ContractError, "capture differs"):
                    gate.compare_capture(self.capture, self.expected)
                (self.capture / name).write_bytes(data)

    def test_length_difference_fails(self):
        with (self.capture / "case.f32le").open("ab") as stream:
            stream.write(b"\0")
        with self.assertRaisesRegex(gate.ContractError, "capture differs"):
            gate.compare_capture(self.capture, self.expected)

    def test_missing_capture_fails(self):
        (self.capture / "case.txt").unlink()
        with self.assertRaisesRegex(gate.ContractError, "missing=.*case.txt"):
            gate.compare_capture(self.capture, self.expected)

    def test_extra_capture_fails(self):
        (self.capture / "extra.txt").write_bytes(b"unexpected")
        with self.assertRaisesRegex(gate.ContractError, "extra=.*extra.txt"):
            gate.compare_capture(self.capture, self.expected)

    def test_directory_instead_of_file_fails(self):
        path = self.capture / "case.txt"
        path.unlink()
        path.mkdir()
        with self.assertRaisesRegex(gate.ContractError, "not a regular"):
            gate.compare_capture(self.capture, self.expected)

    def test_symlink_file_fails(self):
        path = self.capture / "case.txt"
        data = path.read_bytes()
        path.unlink()
        other = self.root / "other.txt"
        other.write_bytes(data)
        try:
            path.symlink_to(other)
        except OSError as error:
            self.skipTest(f"symlinks unavailable: {error}")
        with self.assertRaisesRegex(gate.ContractError, "not a regular"):
            gate.compare_capture(self.capture, self.expected)

    def test_integrated_negative_control_preserves_captures_and_expected(self):
        snapshot = json.dumps(self.expected, sort_keys=True)
        result = gate.check_mutation_sensitivity(self.capture, self.expected, self.root / "negative")
        self.assertEqual(result, ["case.f32le", "case.txt"])
        self.assertEqual(json.dumps(self.expected, sort_keys=True), snapshot)
        self.assertEqual(gate.compare_capture(self.capture, self.expected), self.expected)

    def test_negative_control_detects_broken_comparator(self):
        with mock.patch.object(gate, "compare_capture", return_value={}):
            with self.assertRaisesRegex(gate.ContractError, "escaped comparison"):
                gate.check_mutation_sensitivity(self.capture, self.expected, self.root / "negative")

    def test_persisted_expected_values_are_original_not_new_capture_values(self):
        manifest = gate.load_manifest(gate.ROOT / gate.MANIFEST)
        # Canonical digests of the three complete original archive files dictionaries.
        frozen = {
            "constructors": (16, "a1c37e67c7f65a53ddac51286ae2501777412621d39c1b11dd32de5e31634def"),
            "terminal": (28, "c4ee54ceeb3a79cc84bc2d1b589c8248f2f37713d4e3caf027045a3a8ccabf3d"),
            "lifecycle": (48, "c7615470a710a435ad1ac9e2010c0648b0352a563056b3e7cc60f639fa5088b1"),
        }
        for name, (cases, digest) in frozen.items():
            suite = manifest["suites"][name]
            self.assertEqual(suite["cases"], cases)
            self.assertEqual(len(suite["files"]), cases * 2)
            encoded = json.dumps(suite["files"], sort_keys=True, separators=(",", ":")).encode()
            self.assertEqual(hashlib.sha256(encoded).hexdigest(), digest)
        gate.verify_inputs(gate.ROOT, manifest)

    def test_release_lock_admission_is_version_only_and_preserves_frozen_hash(self):
        manifest = gate.load_manifest(gate.ROOT / gate.MANIFEST)
        expected = manifest["inputs"]["Cargo.lock"]
        original = (gate.ROOT / "Cargo.lock").read_bytes()
        path = self.root / "Cargo.lock"
        path.write_bytes(original)
        self.assertTrue(gate.release_lock_matches(path, expected))
        self.assertEqual(path.read_bytes(), original)
        cases = [
            (b'version = "0.20.0"', b'version = "0.21.0"'),
            (b'version = "0.20.0"', b'version = "0.19.0"'),
            (b'name = "openjoc-api"', b'name = "renamed-api"'),
            (b'name = "adler2"\nversion = "2.0.1"', b'name = "adler2"\nversion = "2.0.2"'),
            (b'checksum = "320119', b'checksum = "000119'),
            (b' "derive_arbitrary",', b' "adler2",'),
            (b' "derive_arbitrary",\n', b''),
            (b'name = "adler2"', b'name = "openjoc-api"'),
            (b'name = "openjoc-api"\nversion = "0.20.0"', b'name = "openjoc-api"\nversion = "0.20.0"\nsource = "git+https://example.invalid"'),
            (b' "openjoc-api",\n "openjoc-eac3",', b' "openjoc-eac3",\n "openjoc-api",'),
        ]
        for old, new in cases:
            with self.subTest(new=new):
                self.assertIn(old, original)
                path.write_bytes(original.replace(old, new, 1))
                self.assertFalse(gate.release_lock_matches(path, expected))
        path.write_bytes(original + b'\n')
        self.assertFalse(gate.release_lock_matches(path, expected))

    def _manifest(self):
        return {"schema_version": 1, "suites": {name: {"cases": 1, "files": self.expected.copy()} for name in gate.SUITES}}

    def _load(self, manifest):
        path = self.root / "manifest.json"
        path.write_text(json.dumps(manifest), encoding="utf-8")
        return gate.load_manifest(path)

    def test_empty_or_incomplete_manifest_fails(self):
        for suites in ({}, {"constructors": {"cases": 0, "files": {}}}):
            with self.assertRaises(gate.ContractError):
                self._load({"schema_version": 1, "suites": suites})
        manifest = self._manifest()
        manifest["suites"]["terminal"]["files"] = {}
        with self.assertRaises(gate.ContractError):
            self._load(manifest)

    def test_path_traversal_in_manifest_fails(self):
        manifest = self._manifest()
        files = manifest["suites"]["terminal"]["files"]
        files["../outside.txt"] = files.pop("case.txt")
        with self.assertRaisesRegex(gate.ContractError, "invalid capture filename"):
            self._load(manifest)

    def test_duplicate_json_keys_fail(self):
        path = self.root / "manifest.json"
        path.write_text('{"schema_version":1,"schema_version":1}', encoding="utf-8")
        with self.assertRaisesRegex(gate.ContractError, "duplicate manifest key"):
            gate.load_manifest(path)

    def test_missing_transcript_pair_fails(self):
        manifest = self._manifest()
        files = manifest["suites"]["terminal"]["files"]
        files["other.f32le"] = files.pop("case.txt")
        with self.assertRaisesRegex(gate.ContractError, "unpaired"):
            self._load(manifest)

    def test_input_hash_mismatch_fails(self):
        with self.assertRaisesRegex(gate.ContractError, "capture harness changed"):
            gate.verify_inputs(self.capture, {"inputs": {"case.txt": "0" * 64}})

    def test_output_is_create_only(self):
        marker = self.capture / "case.txt"
        before = marker.read_bytes()
        with mock.patch.object(gate, "run_checked") as run:
            self.assertEqual(gate.main(["--output-dir", str(self.capture)]), 1)
            run.assert_not_called()
        self.assertEqual(marker.read_bytes(), before)

    def test_missing_inputs_leave_failure_report_without_running_cargo(self):
        output = self.root / "output"
        with mock.patch.object(gate, "run_checked") as run:
            self.assertEqual(gate.main(["--root", str(self.root), "--output-dir", str(output)]), 1)
            run.assert_not_called()
        self.assertEqual(json.loads((output / "report.json").read_text())["result"], "FAIL")

    def test_successful_cargo_with_no_captures_fails_closed(self):
        output = self.root / "no-captures"
        with mock.patch.object(gate, "run_checked", return_value="mock toolchain"), mock.patch.object(gate, "source_record", return_value={}):
            self.assertEqual(gate.main(["--output-dir", str(output)]), 1)
        report = json.loads((output / "report.json").read_text())
        self.assertEqual(report["result"], "FAIL")
        self.assertIn("capture file set differs", report["error"])
        self.assertEqual(report["suites"]["constructors"]["result"], "FAIL")

    def test_toolchain_is_explicit_and_no_update_mode_exists(self):
        parser = gate.argument_parser()
        args = parser.parse_args(["--output-dir", "new-run"])
        self.assertEqual(args.toolchain, "1.99.0")
        self.assertFalse(any("--update" in action.option_strings for action in parser._actions))


if __name__ == "__main__":
    unittest.main()
