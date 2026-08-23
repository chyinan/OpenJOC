# SPDX-FileCopyrightText: 2026 OpenJOC contributors
# SPDX-License-Identifier: Apache-2.0

# pattern: Functional Core

from __future__ import annotations

import pathlib
import subprocess
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
        self.assertIn("/p:EnableOpenJOC=true", text)
        self.assertIn("/p:EnableOpenJOCSideBySide=true", text)
        self.assertIn("/p:BuildProjectReferences=true", text)
        self.assertIn("Verbosity=minimal", text)
        self.assertNotIn("Verbosity=diagnostic", text)

    def test_rejects_missing_arguments(self) -> None:
        completed = subprocess.run(
            ["cmd.exe", "/d", "/c", str(SCRIPT)],
            check=False,
            capture_output=True,
            text=True,
        )

        self.assertEqual(completed.returncode, 64)
        self.assertIn("usage:", completed.stderr.lower())


if __name__ == "__main__":
    unittest.main()
