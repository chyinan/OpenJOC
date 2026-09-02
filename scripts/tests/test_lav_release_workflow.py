# SPDX-FileCopyrightText: 2026 OpenJOC contributors
# SPDX-License-Identifier: Apache-2.0

from __future__ import annotations

import pathlib
import unittest


WORKSPACE = pathlib.Path(__file__).resolve().parents[2]
WORKFLOW = WORKSPACE / ".github" / "workflows" / "lav-release.yml"


class LavReleaseWorkflowTests(unittest.TestCase):
    def test_workflow_builds_pinned_lav_and_uploads_the_matching_release_asset(self) -> None:
        text = WORKFLOW.read_text(encoding="utf-8")
        for expected in (
            "windows-2025",
            "workflow_dispatch:",
            "tags: ['v*']",
            "repository: chyinan/LAVFilters-OpenJOC",
            "18c56fb777464bbc86b69cafb8114127988d0c6d",
            "scripts/package_lav_release.py",
            "openjoc-lav-$env:RELEASE_VERSION-windows-x64.zip",
            "gh release upload",
            "contents: write",
        ):
            self.assertIn(expected, text)
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


if __name__ == "__main__":
    unittest.main()
