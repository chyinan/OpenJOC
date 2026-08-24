# SPDX-FileCopyrightText: 2026 OpenJOC contributors
# SPDX-License-Identifier: Apache-2.0

# pattern: Functional Core

from __future__ import annotations

import pathlib
import subprocess
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "release_lav_smokes.cmd"
NOOP_LIFECYCLE_SOURCE = ROOT / "scripts" / "tests" / "LavSmokeNoopLifecycle.cpp"


class ReleaseLavSmokesScriptTests(unittest.TestCase):
    def test_declares_and_builds_all_release_smokes(self) -> None:
        text = SCRIPT.read_text(encoding="utf-8")

        self.assertIn("SPDX-License-Identifier: GPL-2.0-or-later", text)
        self.assertIn("pattern: Imperative Shell", text)
        self.assertIn("OpenJocAdmissionTests.cpp", text)
        self.assertIn("OpenJocDecoderSmoke.cpp", text)
        self.assertIn("LAVAudioIdentitySmoke.cpp", text)
        self.assertIn("OpenJocDirectShowLifecycle.exe", text)
        self.assertIn("OpenJocOutputTests.cpp", text)
        self.assertIn("OpenJocOutput.cpp", text)
        self.assertIn("OpenJocOutputTests.exe", text)
        self.assertIn("OpenJocStrictOutputTests.cpp", text)
        self.assertIn("OpenJocStrictOutput.cpp", text)
        self.assertIn("OpenJocStrictNegotiation.cpp", text)
        self.assertIn("OpenJocStrictOutputTests.exe", text)
        self.assertIn("LAV_ENABLE_OPENJOC", text)
        self.assertIn("LAV_OPENJOC_TESTING", text)
        self.assertGreaterEqual(text.count("OpenJocOutput.cpp"), 2)
        self.assertGreaterEqual(text.count("avutil-lav.lib"), 2)
        self.assertGreaterEqual(text.count('"/LIBPATH:%~2\\bin_x64\\lib"'), 2)
        self.assertGreaterEqual(text.count('"/I%~2\\common\\baseclasses"'), 2)
        self.assertGreaterEqual(text.count('"/I%~2\\common\\DSUtilLite"'), 2)
        self.assertGreaterEqual(text.count("strmiids.lib"), 3)
        self.assertGreaterEqual(text.count("call cl"), 6)

    def test_checked_in_noop_lifecycle_is_reproducible(self) -> None:
        text = NOOP_LIFECYCLE_SOURCE.read_text(encoding="utf-8")

        self.assertIn("SPDX-License-Identifier: GPL-2.0-or-later", text)
        self.assertIn("pattern: Imperative Shell", text)
        self.assertIn("int wmain()", text)

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
