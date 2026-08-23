# SPDX-FileCopyrightText: 2026 OpenJOC contributors
# SPDX-License-Identifier: Apache-2.0

# pattern: Functional Core

from __future__ import annotations

import pathlib
import subprocess
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "release_lav_smokes.cmd"


class ReleaseLavSmokesScriptTests(unittest.TestCase):
    def test_declares_and_builds_all_release_smokes(self) -> None:
        text = SCRIPT.read_text(encoding="utf-8")

        self.assertIn("SPDX-License-Identifier: GPL-2.0-or-later", text)
        self.assertIn("pattern: Imperative Shell", text)
        self.assertIn("OpenJocAdmissionTests.cpp", text)
        self.assertIn("OpenJocDecoderSmoke.cpp", text)
        self.assertIn("LAVAudioIdentitySmoke.cpp", text)
        self.assertIn("OpenJocDirectShowLifecycle.exe", text)
        self.assertIn("LAV_ENABLE_OPENJOC", text)

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
