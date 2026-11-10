"""Preserve executable path intent at the preset oracle command boundary."""
from __future__ import annotations

import contextlib
import importlib.util
import io
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import unittest
from unittest import mock


SPEC = importlib.util.spec_from_file_location(
    "preset_oracle", Path(__file__).resolve().parents[1] / "verify-arbitrary-layout-preset-oracle.py"
)
assert SPEC is not None and SPEC.loader is not None
oracle = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(oracle)


class PresetOracleBinaryPathTests(unittest.TestCase):
    def run_oracle(self, old_binary: str, new_binary: str) -> list[str]:
        invoked = []
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            fixture = root / "fixture.ec3"
            fixture.write_bytes(b"synthetic command-boundary input")
            argv = [
                "oracle", "--old-binary", old_binary, "--new-binary", new_binary,
                "--fixture", str(fixture), "--output-dir", str(root / "output"),
            ]

            def run_command(command, **kwargs):
                self.assertTrue(kwargs["check"])
                if command[0] == "ffmpeg":
                    return subprocess.CompletedProcess(command, 0, stdout=struct.pack("<ff", 0.25, -0.25))
                invoked.append(command[0])
                self.assertEqual(command[1:3], ["render-joc", str(fixture)])
                output = Path(command[command.index("--output") + 1])
                output.write_bytes(b"synthetic container; FFmpeg boundary is mocked")
                return subprocess.CompletedProcess(command, 0, stdout="frames: 1\n")

            with mock.patch.object(sys, "argv", argv), mock.patch.object(oracle, "PRESETS", ["2.0"]), mock.patch.object(oracle.subprocess, "run", side_effect=run_command), contextlib.redirect_stdout(io.StringIO()):
                oracle.main()
        return invoked

    def test_explicit_local_old_binary_keeps_directory_prefix(self):
        self.assertEqual(
            self.run_oracle("./openjoc-old", "openjoc-new"),
            ["./openjoc-old", "openjoc-new"],
        )

    def test_explicit_local_new_binary_keeps_directory_prefix(self):
        self.assertEqual(
            self.run_oracle("openjoc-old", "./openjoc-new"),
            ["openjoc-old", "./openjoc-new"],
        )

    def test_supported_binary_spelling_is_preserved(self):
        forms = [
            "openjoc", "./openjoc", "./build ü/openjoc old", "../build/openjoc",
            "build/openjoc", str(Path(sys.executable).resolve()),
        ]
        if sys.platform == "win32":
            forms.extend([r".\openjoc.exe", r".\build ü\openjoc old.exe"])
        for binary in forms:
            with self.subTest(binary=binary):
                self.assertEqual(self.run_oracle(binary, binary), [binary, binary])


if __name__ == "__main__":
    unittest.main()
