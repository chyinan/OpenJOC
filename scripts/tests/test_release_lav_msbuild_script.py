# SPDX-FileCopyrightText: 2026 OpenJOC contributors
# SPDX-License-Identifier: Apache-2.0

# pattern: Functional Core

from __future__ import annotations

import os
import pathlib
import subprocess
import tempfile
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "release_lav_msbuild.cmd"


class ReleaseLavMsbuildScriptTests(unittest.TestCase):
    def test_declares_release_invariants(self) -> None:
        text = SCRIPT.read_text(encoding="utf-8")

        self.assertIn("SPDX-License-Identifier: GPL-2.0-or-later", text)
        self.assertIn("pattern: Imperative Shell", text)
        self.assertIn("LAVFilters.sln", text)
        self.assertIn("/t:LAVAudio:Rebuild", text)
        self.assertIn("/p:Configuration=Release", text)
        self.assertIn("/p:Platform=x64", text)
        self.assertIn("/p:RegisterOutput=false", text)
        self.assertIn("/p:EnableOpenJOC=true", text)
        self.assertIn("/p:EnableOpenJOCSideBySide=true", text)
        self.assertIn("/p:BuildProjectReferences=true", text)
        self.assertIn("Verbosity=minimal", text)
        self.assertNotIn("Verbosity=diagnostic", text)

    def test_full_solution_build_is_explicit_and_audio_is_the_default(self) -> None:
        text = SCRIPT.read_text(encoding="utf-8")
        self.assertIn('set "LAV_VARIANT=%~5"', text)
        self.assertIn('if "%LAV_VARIANT%"=="" set "LAV_VARIANT=audio"', text)
        self.assertIn('set "LAV_BUILD_TARGET=/t:LAVAudio:Rebuild"', text)
        full = text.split('if /i "%LAV_VARIANT%"=="full" (')[1].split("\n)")[0]
        self.assertIn('set "LAV_BUILD_TARGET=/t:Rebuild"', full)
        self.assertIn("/p:PlatformToolset=v145", full)
        self.assertIn("/p:WindowsTargetPlatformVersion=10.0", full)
        self.assertIn("%LAV_BUILD_TARGET% /m", text)
        self.assertIn("%LAV_TOOLSET_OPTIONS% ^", text)
        self.assertIn("Unsupported LAV variant", text)
        self.assertLess(text.index("Unsupported LAV variant"), text.index('call "%~1"'))
        self.assertNotIn("LAVVideo.vcxproj", text)
        self.assertNotIn("LAVSplitter.vcxproj", text)

    @unittest.skipUnless(os.name == "nt", "Windows cmd.exe is required")
    def test_rejects_missing_arguments(self) -> None:
        completed = subprocess.run(
            ["cmd.exe", "/d", "/c", str(SCRIPT)],
            check=False,
            capture_output=True,
            text=True,
        )

        self.assertEqual(completed.returncode, 64)
        self.assertIn("usage:", completed.stderr.lower())


@unittest.skipUnless(os.name == "nt", "Windows cmd.exe is required")
class ReleaseLavMsbuildDispatchTests(unittest.TestCase):
    """Exercise cmd argument routing without invoking Visual Studio or compiling."""

    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory(prefix="lav build dispatch ")
        self.addCleanup(self.temporary.cleanup)
        self.root = pathlib.Path(self.temporary.name)
        self.devcmd = self.root / "fake vs dev.cmd"
        self.devcmd.write_text(
            "@echo off\nexit /b %LAV_TEST_DEVCMD_EXIT%\n", encoding="ascii"
        )
        self.capture = self.root / "msbuild arguments.txt"
        (self.root / "msbuild.cmd").write_text(
            '@echo off\n> "%LAV_TEST_CAPTURE%" echo %*\nexit /b %LAV_TEST_MSBUILD_EXIT%\n',
            encoding="ascii",
        )
        self.lav = self.root / "lav sources"
        self.include = self.root / "capi include"
        self.log = self.root / "build log.txt"
        self.environment = dict(os.environ)
        self.environment.update(
            PATH=f"{self.root}{os.pathsep}{os.environ.get('PATH', '')}",
            LAV_TEST_CAPTURE=str(self.capture),
            LAV_TEST_DEVCMD_EXIT="0",
            LAV_TEST_MSBUILD_EXIT="0",
        )

    def run_script(self, variant: str | None = None) -> subprocess.CompletedProcess[str]:
        arguments = [str(SCRIPT), str(self.devcmd), str(self.lav), str(self.include), str(self.log)]
        if variant is not None:
            arguments.append(variant)
        # Match the release workflow's wrapper; avoids cmd.exe's special /c
        # handling of an initial quoted command when checkout paths have spaces.
        wrapper = self.root / "run-build.cmd"
        wrapper.write_text(
            "@echo off\ncall " + " ".join(f'"{value}"' for value in arguments)
            + "\nexit /b %errorlevel%\n",
            encoding="utf-8",
        )
        return subprocess.run(
            ["cmd.exe", "/d", "/c", wrapper.name],
            cwd=self.root,
            env=self.environment,
            check=False,
            capture_output=True,
            text=True,
        )

    def test_default_and_explicit_audio_only_rebuild_audio(self) -> None:
        for variant in (None, "audio"):
            with self.subTest(variant=variant):
                completed = self.run_script(variant)
                self.assertEqual(completed.returncode, 0, completed.stderr)
                arguments = self.capture.read_text()
                self.assertIn("/t:LAVAudio:Rebuild", arguments)
                self.assertNotIn("/t:Rebuild", arguments)
                self.assertNotIn("PlatformToolset", arguments)
                self.assertIn("/p:RegisterOutput=false", arguments)
                self.assertIn(f'"{self.lav}\\LAVFilters.sln"', arguments)
                self.assertIn(f'"/p:OpenJocIncludeDir={self.include}"', arguments)
                self.assertIn(f'"/flp:LogFile={self.log};Verbosity=minimal"', arguments)

    def test_full_rebuilds_complete_solution_with_verified_toolset(self) -> None:
        completed = self.run_script("full")
        self.assertEqual(completed.returncode, 0, completed.stderr)
        arguments = self.capture.read_text()
        self.assertIn("/t:Rebuild", arguments)
        self.assertNotIn("/t:LAVAudio", arguments)
        self.assertIn("/p:PlatformToolset=v145", arguments)
        self.assertIn("/p:WindowsTargetPlatformVersion=10.0", arguments)
        self.assertIn("/p:RegisterOutput=false", arguments)
        self.assertIn("/p:EnableOpenJOC=true", arguments)
        self.assertIn("/p:EnableOpenJOCSideBySide=true", arguments)

    def test_rejects_unknown_variant_before_running_build(self) -> None:
        completed = self.run_script("unknown")
        self.assertEqual(completed.returncode, 64)
        self.assertIn("Unsupported LAV variant", completed.stderr)
        self.assertFalse(self.capture.exists())

    def test_developer_environment_failure_prevents_msbuild(self) -> None:
        self.environment["LAV_TEST_DEVCMD_EXIT"] = "17"
        completed = self.run_script("full")
        self.assertEqual(completed.returncode, 17)
        self.assertFalse(self.capture.exists())

    def test_msbuild_failure_is_returned(self) -> None:
        self.environment["LAV_TEST_MSBUILD_EXIT"] = "23"
        completed = self.run_script("full")
        self.assertEqual(completed.returncode, 23)
        self.assertTrue(self.capture.exists())


if __name__ == "__main__":
    unittest.main()
