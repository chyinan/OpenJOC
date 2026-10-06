# SPDX-FileCopyrightText: 2026 OpenJOC contributors
# SPDX-License-Identifier: Apache-2.0

from __future__ import annotations

import pathlib
import unittest


WORKSPACE = pathlib.Path(__file__).resolve().parents[2]
WORKFLOW = WORKSPACE / ".github" / "workflows" / "lav-release.yml"


class LavReleaseWorkflowTests(unittest.TestCase):
    def test_gain_documentation_identifies_required_release_asset(self) -> None:
        for relative, expected in (
            ("docs/integration/LAV_FILTERS_OPENJOC.md", "Available with the v0.19.0 LAV package"),
            ("docs/site/using/windows-lav-potplayer.md", "Available with the v0.19.0 LAV package"),
            ("docs/site/using/windows-lav-potplayer.zh.md", "适用于 v0.19.0 LAV 安装包"),
        ):
            text = (WORKSPACE / relative).read_text(encoding="utf-8")
            self.assertIn(expected, text)
            self.assertIn("v0.18.0", text)
            self.assertIn("openjoc-lav-0.19.0-windows-x64.zip", text)

    def test_native_gain_and_settings_gates_precede_publication(self) -> None:
        text = WORKFLOW.read_text(encoding="utf-8")
        validation = text.split("- name: Validate package and standalone smoke tests")[1].split("- name: Upload LAV asset")[0]
        for required in ("OpenJocOutputTests.exe", "OpenJocStrictOutputTests.exe", "OpenJocSettingsSmoke.exe", "--output-gain-only", "if ($LASTEXITCODE -ne 0)", "Copy-Item -Path (Join-Path $extract 'runtime\\*') -Destination $smokes"):
            self.assertIn(required, validation)
        self.assertNotIn("OpenJocDirectShowLifecycle.exe", validation)
        self.assertIn("ref: ${{ env.RELEASE_TAG }}", text)

    def test_workflow_builds_pinned_lav_and_uploads_the_matching_release_asset(self) -> None:
        text = WORKFLOW.read_text(encoding="utf-8")
        for expected in (
            "windows-2025",
            "workflow_dispatch:",
            "tags: ['v*']",
            "repository: chyinan/LAVFilters-OpenJOC",
            "01666bae613aeaf0568f7548bfe9ab77a09486e1",
            "scripts/package_lav_release.py",
            "openjoc-lav-$env:RELEASE_VERSION-windows-x64.zip",
            "gh release upload",
            "contents: write",
        ):
            self.assertIn(expected, text)
        # Both manual default and tag-trigger fallback must use this reviewed pin.
        self.assertEqual(text.count("01666bae613aeaf0568f7548bfe9ab77a09486e1"), 2)
        self.assertIn("cargo build -p openjoc-capi --release --locked", text)
        self.assertIn("--extra-libs=../thirdparty/64/lib/zlib.lib", text)
        self.assertIn("Join-Path $lav 'ffmpeg\\zlib.lib'", text)
        self.assertIn("ref: v1.3.1", text)
        self.assertIn("Build libbluray runtime dependency", text)
        self.assertIn("libbluray\\libbluray.vcxproj", text)
        self.assertIn("libbluray.dll", text)
        self.assertIn("mingw-w64-x86_64-gcc-libs", text)
        self.assertIn("setup-msys2\\msys2.cmd", text)
        self.assertIn("cygpath' '-w' '/mingw64/bin", text)
        self.assertIn(r'''gsub(/\\\\/, "/")''', text)
        self.assertIn("defined(Z_HAVE_UNISTD_H) && !defined(_WIN32)", text)
        self.assertIn('bash -c "sh ./build_ffmpeg_msvc.sh x64 release"', text)
        self.assertIn("Retain FFmpeg configure diagnostics", text)
        self.assertIn("lav/ffmpeg/ffbuild/config.log", text)
        self.assertIn("release_lav_msbuild.cmd", text)
        self.assertIn("release_lav_smokes.cmd", text)
        self.assertIn(r'''scripts\tests\LavSmokeNoopLifecycle.cpp''', text)
        self.assertIn("$complete = $true", text)
        self.assertIn("vcruntime140_threads.dll", text)
        self.assertIn("OpenJocDiagnosticTests.exe", text)
        self.assertIn("OpenJocPropertyPageSmoke.exe", text)
        self.assertIn("OpenJocPropertyPageSmoke.exe') $lavAudio", text)
        self.assertIn("JOC Stream property-page resource", text)
        self.assertIn("$attempt -le 150", text)
        self.assertIn("Start-Sleep -Seconds 10", text)


if __name__ == "__main__":
    unittest.main()
