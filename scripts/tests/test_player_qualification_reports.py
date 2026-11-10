from __future__ import annotations

import contextlib
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock
import zipfile


ROOT = Path(__file__).resolve().parents[2]
QUALIFIER = ROOT / "scripts/qualify-player-package.py"


def load_qualifier():
    spec = importlib.util.spec_from_file_location("qualification_report_tests", QUALIFIER)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class PlayerQualificationReportTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.archive = self.root / "incomplete-player.zip"
        # A regular, intentionally incomplete package exercises normal BLOCKED
        # reporting through the actual verifier, without executing a player.
        with zipfile.ZipFile(self.archive, "w") as package:
            package.writestr("player/README.txt", "Incomplete package for rejection coverage.\n")
        self.archive_before = self.archive.read_bytes()
        self.fixtures = self.root / "fixtures"
        self.fixtures.mkdir()

    def arguments(self, report, platform="linux-x86_64"):
        return [
            str(QUALIFIER), "--archive", str(self.archive), "--platform", platform,
            "--fixtures", str(self.fixtures), "--report", str(report),
        ]

    def assert_reports(self, report, summary, qualification):
        document = json.loads(report.read_text(encoding="utf-8"))
        self.assertEqual(document["schema"], "openjoc.player-qualification.v1")
        self.assertEqual(document["qualification"], qualification)
        self.assertIn("statuses", document)
        self.assertIn("evidence", document)
        text = summary.read_text(encoding="utf-8")
        self.assertIn(f"Qualification: {qualification}\n", text)
        self.assertEqual(self.archive.read_bytes(), self.archive_before)
        self.assertEqual(list(self.fixtures.iterdir()), [])
        return document

    def test_actual_cli_preserves_json_at_txt_report_path(self):
        report = self.root / "qualification.txt"
        result = subprocess.run(
            [sys.executable, *self.arguments(report)],
            capture_output=True, text=True, check=False,
        )
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertEqual(json.loads(result.stdout)["qualification"], "BLOCKED")
        document = self.assert_reports(report, self.root / "qualification.summary.txt", "BLOCKED")
        self.assertIn("missing or symlinked file", document["evidence"]["package_verifier"])
        self.assertEqual(document["statuses"]["PACKAGE"], "FAIL")

    def test_actual_cli_retains_json_and_text_convention(self):
        report = self.root / "qualification.json"
        result = subprocess.run(
            [sys.executable, *self.arguments(report)],
            capture_output=True, text=True, check=False,
        )
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assert_reports(report, self.root / "qualification.txt", "BLOCKED")

    def run_with_command_results(self, report, *, platform, verifier_ok=True,
                                 harness_ok=True, markers=True):
        qualifier = load_qualifier()
        verifier_output = "mpv.exe direct portable_config Lua menu autoload (no --config-dir): PASS\n"
        harness_output = "\n".join(qualifier.GAIN_HARNESS_MARKERS.values()) if markers else ""
        results = [(0 if verifier_ok else 1, verifier_output)]
        if verifier_ok:
            results.append((0 if harness_ok else 1, harness_output))
        # Only the native-command boundary and Windows runtime driver are
        # simulated. Parsing, extraction, statuses and report writes are real.
        with mock.patch.object(sys, "argv", self.arguments(report, platform)), \
             mock.patch.object(qualifier, "run", side_effect=results), \
             mock.patch.object(qualifier, "run_windows_settings_io_roundtrip", return_value=(True, "driver passed")), \
             mock.patch.object(qualifier.shutil, "which", return_value=None), \
             contextlib.redirect_stdout(io.StringIO()) as output:
            code = qualifier.main()
        return code, json.loads(output.getvalue())

    def test_success_reporting_uses_distinct_paths_on_all_platforms(self):
        for platform in ("linux-x86_64", "macos-arm64", "windows-x64"):
            for suffix in (".txt", ".TXT", ".Txt"):
                with self.subTest(platform=platform, suffix=suffix):
                    report = self.root / (platform + suffix)
                    summary = report.with_suffix(".summary.txt")
                    self.assertNotEqual(str(report).casefold(), str(summary).casefold())
                    code, output = self.run_with_command_results(report, platform=platform)
                    self.assertEqual(code, 0)
                    self.assertEqual(output["report"], str(report.resolve()))
                    self.assert_reports(report, summary, "QUALIFIED")

    def test_actual_cli_resolves_report_directory_alias(self):
        directory = self.root / "reports"
        directory.mkdir()
        alias = self.root / "reports-alias"
        try:
            alias.symlink_to(directory, target_is_directory=True)
        except OSError as error:
            self.skipTest(f"directory symlinks unavailable: {error}")
        report = alias / "qualification.txt"
        result = subprocess.run(
            [sys.executable, *self.arguments(report)],
            capture_output=True, text=True, check=False,
        )
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertEqual(json.loads(result.stdout)["report"], str(report.resolve()))
        self.assert_reports(report, alias / "qualification.summary.txt", "BLOCKED")

    def test_blocked_harness_still_writes_both_reports(self):
        for harness_ok, markers in ((False, True), (True, False)):
            with self.subTest(harness_ok=harness_ok, markers=markers):
                report = self.root / "blocked.txt"
                code, output = self.run_with_command_results(
                    report, platform="linux-x86_64", harness_ok=harness_ok, markers=markers,
                )
                self.assertEqual(code, 1)
                self.assertEqual(output["qualification"], "BLOCKED")
                self.assert_reports(report, report.with_suffix(".summary.txt"), "BLOCKED")

    def test_other_report_names_keep_existing_summary_names(self):
        for name in ("report.json", "report.data", "report", ".txt", "検証 report.json"):
            with self.subTest(name=name):
                report = self.root / name
                code, _ = self.run_with_command_results(report, platform="linux-x86_64")
                self.assertEqual(code, 0)
                self.assert_reports(report, report.with_suffix(".txt"), "QUALIFIED")


if __name__ == "__main__":
    unittest.main()
