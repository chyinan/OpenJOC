"""Exercise the real session gate with synthetic command-generated captures."""
from pathlib import Path
import contextlib
import hashlib
import io
import json
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import verify_session_contracts as gate


class SessionContractMainTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.data = {"case.f32le": bytes(range(32)), "case.txt": b"event=exact\n"}
        self.expected = {
            name: {"bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}
            for name, data in self.data.items()
        }
        manifest = {
            "schema_version": 1,
            "inputs": {},
            "suites": {
                name: {"cases": 1, "files": self.expected}
                for name in gate.SUITES
            },
        }
        manifest_path = self.root / gate.MANIFEST
        manifest_path.parent.mkdir(parents=True)
        manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
        (self.root / "Cargo.toml").write_text("[workspace]\n", encoding="utf-8")
        (self.root / "Cargo.lock").write_text("version = 4\n", encoding="utf-8")
        (self.root / "scripts/verify_session_contracts.py").write_bytes(Path(gate.__file__).read_bytes())
        self.source_before = gate.source_record(self.root)
        self.invoked = []

    def run_gate(self, change=lambda directory: None):
        def run_checked(command, root, env, log):
            # Only external commands are substituted, including version probes.
            # Manifest/input/source validation and all gate logic execute normally.
            if command[0] == "cargo" and "test" in command:
                locations = [env[variable] for variable, _ in gate.SUITES.values() if variable in env]
                self.assertEqual(len(locations), 1)
                directory = Path(locations[0])
                self.invoked.append(directory.name)
                for filename, data in self.data.items():
                    (directory / filename).write_bytes(data)
                change(directory)
            return "synthetic external command output"

        output = self.root / "output"
        with mock.patch.object(gate, "run_checked", side_effect=run_checked), contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
            status = gate.main(["--root", str(self.root), "--output-dir", str(output)])
        self.assertEqual(gate.source_record(self.root), self.source_before)
        return status, json.loads((output / "report.json").read_text(encoding="utf-8"))

    def assert_rejected(self, change, error):
        status, report = self.run_gate(change)
        self.assertEqual(status, 1)
        self.assertEqual(report["result"], "FAIL")
        self.assertIn(error, report["error"])

    def test_exact_captures_pass_all_suites(self):
        status, report = self.run_gate()
        self.assertEqual(status, 0)
        self.assertEqual(report["result"], "PASS")
        self.assertEqual(self.invoked, list(gate.SUITES))
        for suite in report["suites"].values():
            self.assertEqual(suite["files"], self.expected)
            self.assertEqual(len(suite["single_bit_negative_controls"]), 2)

    def test_extra_directory_is_rejected(self):
        self.assert_rejected(lambda directory: (directory / "unexpected").mkdir(), "extra=['unexpected']")

    def test_extra_nested_pcm_is_rejected(self):
        def change(directory):
            (directory / "unexpected").mkdir()
            (directory / "unexpected/extra.f32le").write_bytes(b"unvalidated")
        self.assert_rejected(change, "extra=['unexpected']")

    def test_dangling_symlink_is_rejected(self):
        def change(directory):
            try:
                (directory / "unexpected").symlink_to(directory / "absent")
            except OSError as error:
                self.skipTest(f"symlinks unavailable: {error}")
        self.assert_rejected(change, "extra=['unexpected']")

    def test_symlinked_capture_directory_is_rejected(self):
        def change(directory):
            target = directory.with_name(directory.name + "-real")
            directory.rename(target)
            try:
                directory.symlink_to(target, target_is_directory=True)
            except OSError as error:
                self.skipTest(f"symlinks unavailable: {error}")
        self.assert_rejected(change, "capture directory missing or symlinked")

    def test_missing_capture_is_rejected(self):
        self.assert_rejected(lambda directory: (directory / "case.txt").unlink(), "missing=['case.txt']")

    def test_extra_regular_file_is_rejected(self):
        self.assert_rejected(lambda directory: (directory / "extra.txt").write_bytes(b"extra"), "extra=['extra.txt']")

    def test_expected_directory_is_rejected(self):
        def change(directory):
            (directory / "case.txt").unlink()
            (directory / "case.txt").mkdir()
        self.assert_rejected(change, "not a regular non-symlink file")

    def test_expected_symlink_is_rejected(self):
        def change(directory):
            (directory / "case.txt").unlink()
            target = self.root / "outside.txt"
            target.write_bytes(self.data["case.txt"])
            try:
                (directory / "case.txt").symlink_to(target)
            except OSError as error:
                self.skipTest(f"symlinks unavailable: {error}")
        self.assert_rejected(change, "not a regular non-symlink file")

    def check_mismatch_collected(self, filename, mutate):
        def change(directory):
            path = directory / filename
            path.write_bytes(mutate(path.read_bytes()))
        status, report = self.run_gate(change)
        self.assertEqual(status, 1)
        self.assertEqual(report["result"], "FAIL")
        self.assertIn("capture differs:", report["error"])
        self.assertEqual(self.invoked, list(gate.SUITES))
        for name, suite in report["suites"].items():
            self.assertEqual(suite["result"], "FAIL")
            self.assertNotEqual(suite["files"][filename], self.expected[filename])
            self.assertNotIn("single_bit_negative_controls", suite)
            self.assertIn(name + ": 1 files differ", report["error"])

    @staticmethod
    def midpoint_bit(data):
        result = bytearray(data)
        result[len(result) // 2] ^= 1
        return bytes(result)

    def test_midpoint_pcm_bit_mismatch_collects_all_suites(self):
        self.check_mismatch_collected("case.f32le", self.midpoint_bit)

    def test_midpoint_event_bit_mismatch_collects_all_suites(self):
        self.check_mismatch_collected("case.txt", self.midpoint_bit)

    def test_other_pcm_bit_mismatch_collects_all_suites(self):
        self.check_mismatch_collected("case.f32le", lambda data: bytes([data[0] ^ 1]) + data[1:])

    def test_truncated_pcm_collects_all_suites(self):
        self.check_mismatch_collected("case.f32le", lambda data: data[:-1])

    def test_empty_pcm_collects_all_suites(self):
        self.check_mismatch_collected("case.f32le", lambda data: b"")

    def test_appended_event_collects_all_suites(self):
        self.check_mismatch_collected("case.txt", lambda data: data + b"extra")

    def test_broken_comparator_still_fails_closed(self):
        with mock.patch.object(gate, "compare_capture", return_value={}):
            self.assert_rejected(lambda directory: None, "single-bit mutation escaped comparison")


if __name__ == "__main__":
    unittest.main()
