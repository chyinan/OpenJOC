from __future__ import annotations

import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
import tempfile
import textwrap
import unittest


ROOT = Path(__file__).resolve().parents[2]
SH = shutil.which("sh")


@unittest.skipUnless(os.name == "posix" and SH, "requires a Unix-like shell")
class FfmpegVerifierCargoTargetTests(unittest.TestCase):
    """Execute the real shell script with explicit Cargo/pkg-config test doubles.

    The emitted decoder only records argv and its identity. These are command
    routing regressions, not native FFmpeg, OpenJOC, or PCM qualification.
    """

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name)
        self.root = self.base / "repository with spaces"
        (self.root / "scripts").mkdir(parents=True)
        shutil.copyfile(ROOT / "scripts/verify-ffmpeg.sh", self.root / "scripts/verify-ffmpeg.sh")
        self.tools = self.base / "test tools"
        self.tools.mkdir()
        self.log = self.base / "calls.jsonl"
        self.fixture = self.root / "audio fixture ü.ec3"
        self.fixture.write_bytes(b"Synthetic routing input, never decoded.\n")
        self.decoder = self.base / "record_decoder.py"
        self.decoder.write_text(textwrap.dedent("""\
            import json, os, sys
            with open(os.environ['TEST_CALL_LOG'], 'a', encoding='utf-8') as stream:
                stream.write(json.dumps({'program': 'decoder', 'identity': sys.argv[1],
                                         'args': sys.argv[2:]}) + '\\n')
            raise SystemExit(int(os.environ.get('TEST_DECODER_EXIT', '0')))
            """), encoding="utf-8")
        cargo = self.base / "record_cargo.py"
        cargo.write_text(textwrap.dedent("""\
            import json, os, pathlib, shlex, subprocess, sys
            args = sys.argv[1:]
            with open(os.environ['TEST_CALL_LOG'], 'a', encoding='utf-8') as stream:
                stream.write(json.dumps({'program': 'cargo', 'args': args}) + '\\n')
            if os.environ.get('TEST_FAIL_CARGO') == args[0]:
                raise SystemExit(23)
            destination = pathlib.Path(os.environ.get('CARGO_TARGET_DIR', 'target'))
            if os.environ.get('CARGO_BUILD_TARGET'):
                destination /= os.environ['CARGO_BUILD_TARGET']
            executable = destination / 'release' / 'openjoc-avdecode'
            if args[0] == 'build':
                executable.parent.mkdir(parents=True, exist_ok=True)
                executable.write_text('#!/bin/sh\\nexec ' + shlex.quote(sys.executable)
                                      + ' ' + shlex.quote(os.environ['TEST_DECODER'])
                                      + ' current "$@"\\n', encoding='utf-8')
                executable.chmod(0o755)
            elif args[0] == 'run':
                raise SystemExit(subprocess.run([str(executable.resolve()),
                                                 *args[args.index('--') + 1:]]).returncode)
            """), encoding="utf-8")
        self.make_executable(self.tools / "cargo", f"exec {shlex.quote(sys.executable)} {shlex.quote(str(cargo))} \"$@\"")
        self.make_executable(self.tools / "pkg-config", 'exit "${TEST_PKG_CONFIG_EXIT:-0}"')
        self.environment = os.environ.copy()
        for name in ("CARGO_TARGET_DIR", "CARGO_BUILD_TARGET", "TEST_FAIL_CARGO", "TEST_DECODER_EXIT", "TEST_PKG_CONFIG_EXIT"):
            self.environment.pop(name, None)
        self.environment.update({"PATH": str(self.tools) + os.pathsep + self.environment.get("PATH", ""),
                                 "TEST_CALL_LOG": str(self.log), "TEST_DECODER": str(self.decoder)})

    def make_executable(self, path: Path, command: str):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text("#!/bin/sh\n" + command + "\n", encoding="utf-8")
        path.chmod(0o755)

    def run_verifier(self, *, fixture: bool = True):
        command = [SH, str(self.root / "scripts/verify-ffmpeg.sh")]
        if fixture:
            command.append(str(self.fixture))
        completed = subprocess.run(command, cwd=self.root, env=self.environment,
                                   text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        calls = [json.loads(line) for line in self.log.read_text(encoding="utf-8").splitlines()] if self.log.exists() else []
        return completed, calls

    def assert_current_decoder(self, calls):
        decoders = [call for call in calls if call["program"] == "decoder"]
        self.assertEqual(decoders, [{"program": "decoder", "identity": "current",
                                    "args": [str(self.fixture), "--binaural", "--null", "--checksum"]}])

    def test_default_build_runs_current_decoder_with_unchanged_arguments(self):
        completed, calls = self.run_verifier()
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assert_current_decoder(calls)
        self.assertEqual(calls[:2], [
            {"program": "cargo", "args": ["build", "-p", "openjoc-ffmpeg", "--release", "--features", "ffmpeg", "--locked"]},
            {"program": "cargo", "args": ["test", "-p", "openjoc-ffmpeg", "--features", "ffmpeg", "--locked"]},
        ])

    def test_relative_target_directory_with_spaces_is_honored(self):
        self.environment["CARGO_TARGET_DIR"] = "custom build directory"
        completed, calls = self.run_verifier()
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assert_current_decoder(calls)

    def test_absolute_unicode_target_directory_is_honored(self):
        self.environment["CARGO_TARGET_DIR"] = str(self.base / "shared builds ü 中文")
        completed, calls = self.run_verifier()
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assert_current_decoder(calls)

    def test_explicit_build_target_uses_cargo_layout(self):
        self.environment["CARGO_BUILD_TARGET"] = "x86_64-unknown-linux-gnu"
        completed, calls = self.run_verifier()
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assert_current_decoder(calls)

    def test_stale_default_binary_is_not_selected_for_custom_build(self):
        self.environment["CARGO_TARGET_DIR"] = "current build"
        self.make_executable(self.root / "target/release/openjoc-avdecode",
                             f"exec {shlex.quote(sys.executable)} {shlex.quote(str(self.decoder))} stale \"$@\"")
        completed, calls = self.run_verifier()
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assert_current_decoder(calls)

    def test_fixture_run_preserves_release_features_lock_and_binary_selection(self):
        completed, calls = self.run_verifier()
        self.assertEqual(completed.returncode, 0, completed.stderr)
        runs = [call for call in calls if call["program"] == "cargo" and call["args"][0] == "run"]
        self.assertEqual(runs, [{"program": "cargo", "args": [
            "run", "-p", "openjoc-ffmpeg", "--release", "--features", "ffmpeg", "--locked",
            "--bin", "openjoc-avdecode", "--", str(self.fixture), "--binaural", "--null", "--checksum"]}])

    def test_no_fixture_only_builds_and_tests(self):
        completed, calls = self.run_verifier(fixture=False)
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertEqual([call["args"][0] for call in calls], ["build", "test"])

    def test_decoder_failure_propagates(self):
        self.environment["TEST_DECODER_EXIT"] = "17"
        completed, calls = self.run_verifier()
        self.assertEqual(completed.returncode, 17, completed.stderr)
        self.assert_current_decoder(calls)

    def test_prerequisite_failure_stops_before_decoder(self):
        for failed_stage in ("pkg-config", "build", "test", "run"):
            with self.subTest(stage=failed_stage):
                self.log.unlink(missing_ok=True)
                self.environment.pop("TEST_PKG_CONFIG_EXIT", None)
                self.environment.pop("TEST_FAIL_CARGO", None)
                if failed_stage == "pkg-config":
                    self.environment["TEST_PKG_CONFIG_EXIT"] = "23"
                else:
                    self.environment["TEST_FAIL_CARGO"] = failed_stage
                completed, calls = self.run_verifier()
                self.assertEqual(completed.returncode, 23, completed.stderr)
                self.assertFalse(any(call["program"] == "decoder" for call in calls))
                expected = {"pkg-config": [], "build": ["build"], "test": ["build", "test"],
                            "run": ["build", "test", "run"]}[failed_stage]
                self.assertEqual([call["args"][0] for call in calls], expected)


if __name__ == "__main__":
    unittest.main()
