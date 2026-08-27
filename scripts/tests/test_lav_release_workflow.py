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
            "d80b2802d05577045426881716138791c18f7b3a",
            "scripts/package_lav_release.py",
            "openjoc-lav-$env:RELEASE_VERSION-windows-x64.zip",
            "gh release upload",
            "contents: write",
        ):
            self.assertIn(expected, text)
        self.assertIn("cargo build -p openjoc-capi --release --locked", text)
        self.assertIn("--extra-libs=../thirdparty/64/lib/zlib.lib", text)
        self.assertIn('bash -c "sh ./build_ffmpeg_msvc.sh x64 release"', text)
        self.assertIn("release_lav_msbuild.cmd", text)
        self.assertIn("release_lav_smokes.cmd", text)


if __name__ == "__main__":
    unittest.main()
