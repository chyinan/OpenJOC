# SPDX-FileCopyrightText: 2026 OpenJOC contributors
# SPDX-License-Identifier: Apache-2.0

"""Keep the PR regression gate lightweight and separate from real registration."""

from __future__ import annotations

import pathlib
import re
import unittest


WORKSPACE = pathlib.Path(__file__).resolve().parents[2]
WORKFLOW = WORKSPACE / ".github/workflows/lav-packaging-tests.yml"
HARNESS = WORKSPACE / "scripts/tests/test_lav_registration_process.ps1"
PROBE = WORKSPACE / "scripts/tests/lav_registration_gui_probe.cpp"


class LavPackagingWorkflowTests(unittest.TestCase):
    def test_path_filtered_gate_runs_for_prs_and_supported_push_branches(self) -> None:
        text = WORKFLOW.read_text(encoding="utf-8")
        triggers = text.split("\npermissions:\n", 1)[0]
        self.assertIn("\n  pull_request:\n", triggers)
        self.assertIn("branches: ['master', 'feat/lav-full-package-a876fbc']", triggers)
        pr_paths = triggers.split("  pull_request:\n", 1)[1].split("  push:\n", 1)[0]
        push_paths = triggers.split("  push:\n", 1)[1].split("    paths:\n", 1)[1]
        self.assertEqual(pr_paths.split("    paths:\n", 1)[1].strip(), push_paths.strip())
        for path in (
            ".github/workflows/lav-packaging-tests.yml",
            ".github/workflows/lav-release.yml",
            "packaging/windows-lav/**",
            "packaging/windows-lav-full/**",
            "scripts/package_lav_release.py",
            "scripts/release_packaging_core.py",
            "scripts/prepare_lav_ffmpeg_configure.py",
            "scripts/release_lav_msbuild.cmd",
            "scripts/tests/test_lav_registration_process.ps1",
            "scripts/tests/test_lav_registration_preflight.ps1",
            "scripts/tests/test_lav_registration_preflight.py",
            "scripts/tests/lav_registration_gui_probe.cpp",
        ):
            with self.subTest(path=path):
                self.assertIn(f"- '{path}'", pr_paths)
        self.assertNotIn("workflow_dispatch:", triggers)
        self.assertNotIn("pull_request_target:", triggers)
        self.assertNotIn("tags:", triggers)

    def test_focused_python_suites_are_present_and_trigger_their_own_gate(self) -> None:
        text = WORKFLOW.read_text(encoding="utf-8")
        tests = text.split("- name: Test package and workflow contracts", 1)[1].split("- name: Build only", 1)[0]
        modules = re.findall(r"scripts\.tests\.(test_[a-z_]+)", tests)
        self.assertEqual(set(modules), {
            "test_lav_full_package",
            "test_lav_registration_preflight",
            "test_lav_release_package",
            "test_lav_release_workflow",
            "test_lav_packaging_workflow",
            "test_release_packaging_core",
            "test_prepare_lav_ffmpeg_configure",
            "test_release_lav_msbuild_script",
        })
        self.assertEqual(len(modules), len(set(modules)))
        for module in modules:
            with self.subTest(module=module):
                self.assertTrue((WORKSPACE / "scripts/tests" / f"{module}.py").is_file())
                self.assertEqual(text.count(f"- 'scripts/tests/{module}.py'"), 2)
        self.assertIn('throw "LAV packaging regression failed: $LASTEXITCODE"', tests)

    def test_job_cannot_build_or_publish_a_full_release(self) -> None:
        text = WORKFLOW.read_text(encoding="utf-8")
        self.assertIn("permissions:\n  contents: read\n", text)
        self.assertIn("persist-credentials: false", text)
        self.assertIn("fetch-depth: 1", text)
        self.assertIn("runs-on: windows-2025", text)
        self.assertIn("timeout-minutes: 10", text)
        self.assertIn("cancel-in-progress: true", text)
        for forbidden in (
            "contents: write", "actions: write", "actions: read", "secrets.",
            "actions/download-artifact", "gh release", "cargo ", "rustup ",
            "repack_lav_registration_fix", "build_ffmpeg", "msbuild ",
            "regsvr32.exe", "install.bat", "uninstall.bat", "Start-Process",
        ):
            with self.subTest(forbidden=forbidden):
                self.assertNotIn(forbidden, text)
        self.assertIn('cd /d "$env:RUNNER_TEMP"', text)
        self.assertIn("/W4 /WX /EHsc /std:c++17 /DUNICODE /D_UNICODE", text)
        self.assertIn("/SUBSYSTEM:WINDOWS shell32.lib kernel32.lib", text)
        self.assertIn(r"scripts\tests\lav_registration_gui_probe.cpp", text)

    def test_ps51_harness_receipt_covers_exact_shipped_helper(self) -> None:
        text = WORKFLOW.read_text(encoding="utf-8")
        test_step = text.split("- name: Test exact helper", 1)[1].split("- name: Retain", 1)[0]
        for required in (
            r"System32\WindowsPowerShell\v1.0\powershell.exe",
            r"packaging\windows-lav-full\scripts\manual-registration.ps1",
            r"-File (Join-Path $env:GITHUB_WORKSPACE 'scripts\tests\test_lav_registration_process.ps1')",
            "-ScriptPath $registration -GuiProbe $probe -ResultPath $receiptPath",
            "-ScratchRoot (Join-Path $env:RUNNER_TEMP 'lav-registration-tests')",
            "$receipt.total -ne 86", "$receipt.passed -ne 86", "$receipt.failed -ne 0",
            "$receipt.powershell_version -notlike '5.1.*'",
            "$receipt.powershell_edition -ne 'Desktop'", "$receipt.process_is_64bit -ne $true",
            "$receipt.real_registration_attempted -ne $false", "$receipt.native_tests_started -ne $true",
            "$receipt.native_gui.total -ne 6", "$receipt.native_gui.passed -ne 6",
            "$receipt.registration_script_sha256", "$receipt.probe_source_sha256", "$receipt.probe_executable_sha256",
            'throw "Windows PowerShell regression failed: $LASTEXITCODE"',
        ):
            with self.subTest(required=required):
                self.assertIn(required, test_step)
        self.assertNotRegex(test_step, r"(?m)^\s*(?:&|\.)\s+\$registration\b")
        self.assertIn("if: always()", text.split("- name: Retain", 1)[1])
        self.assertIn("path: ${{ runner.temp }}/lav-registration-regression.json", text)

    def test_harness_keeps_registration_mocked_and_native_fixture_confined(self) -> None:
        text = HARNESS.read_text(encoding="utf-8")
        for required in (
            "function Start-Process {", "MOCK_UNEXPECTED_PROCESS_CALL",
            r"(Join-Path $PSScriptRoot '..\..\packaging\windows-lav-full\scripts\manual-registration.ps1')",
            "[Management.Automation.Language.Parser]::ParseFile($RegistrationScript",
            "'Unexpected helper command'", "'Unexpected loop command'",
            "'ScratchRoot must be inside RUNNER_TEMP'",
            "'Only the dedicated GUI fixture may be executed'",
            "'Fixture must use WINDOWS GUI subsystem, not CONSOLE'",
            "'The fake AX module must not exist'",
            "'All-filter preflight rejection blocks every unregister'",
            "'unset','stale-zero','stale-nonzero'",
            "real_registration_attempted=$false",
            "$script:NativeObservations.Count -eq 6",
        ):
            with self.subTest(required=required):
                self.assertIn(required, text)
        self.assertIn("probe_source_sha256=", text)
        self.assertNotRegex(text, r"(?m)^\s*(?:&|\.)\s+\$RegistrationScript\b")
        self.assertNotIn("regsvr32.exe", text)
        self.assertNotIn("lav_full_candidate_manual_registration.ps1", text)
        self.assertIn("'native-gui-control'", text)
        self.assertIn("'native-gui-helper'", text)

    def test_native_fixture_does_not_load_modules_or_touch_registration(self) -> None:
        text = PROBE.read_text(encoding="utf-8")
        for required in (
            "int WINAPI wWinMain", "CommandLineToArgvW", "IsInsideRunnerTemp",
            'ReadEnvironment(L"RUNNER_TEMP", runnerTemp)', "Sleep(700)",
            "CREATE_NEW", 'return exitText == L"7" ? 7 : 0;',
        ):
            with self.subTest(required=required):
                self.assertIn(required, text)
        for forbidden in ("LoadLibrary", "CreateProcess", "ShellExecute", "RegOpen", "RegSet", "DllRegisterServer"):
            with self.subTest(forbidden=forbidden):
                self.assertNotIn(forbidden, text)


if __name__ == "__main__":
    unittest.main()
