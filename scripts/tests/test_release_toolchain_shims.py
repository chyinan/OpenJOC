from __future__ import annotations

# pattern: Imperative Shell

import os
import subprocess
import unittest
from pathlib import Path


TOOLS = ("ar", "nm", "ranlib", "strip", "windres", "dlltool")


class ReleaseToolchainShimTests(unittest.TestCase):
    def test_all_cross_prefix_shims_are_licensed_and_delegate(self) -> None:
        scripts = Path(__file__).resolve().parents[1]
        shim_root = scripts / "msys2-cross-tools"
        bash_value = os.environ.get("OPENJOC_MSYS2_BASH")
        if not bash_value:
            raise unittest.SkipTest("OPENJOC_MSYS2_BASH is not configured")
        environment = {
            "MINGW_PREFIX": "/mingw64",
            "PATH": "/usr/bin:/mingw64/bin",
            "SystemRoot": os.environ["SystemRoot"],
        }
        for tool in TOOLS:
            with self.subTest(tool=tool):
                path = shim_root / f"x86_64-w64-mingw32-{tool}"
                text = path.read_text(encoding="utf-8")
                self.assertIn("SPDX-FileCopyrightText: 2026 OpenJOC contributors", text)
                self.assertIn("SPDX-License-Identifier: GPL-2.0-or-later", text)
                completed = subprocess.run(
                    [bash_value, "--noprofile", "--norc", str(path), "--version"],
                    check=True,
                    capture_output=True,
                    text=True,
                    env=environment,
                )
                self.assertIn("GNU", completed.stdout)


if __name__ == "__main__":
    unittest.main()
