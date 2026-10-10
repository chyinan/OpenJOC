# SPDX-FileCopyrightText: 2026 OpenJOC contributors
# SPDX-License-Identifier: Apache-2.0

from __future__ import annotations

import importlib.util
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "prepare_lav_ffmpeg_configure.py"
FIXTURES = Path(__file__).parent / "fixtures" / "ffmpeg-msvc"
SPEC = importlib.util.spec_from_file_location("prepare_lav_ffmpeg_configure", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
PREPARE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PREPARE)


def fixture(name: str, newline: bytes = b"\n") -> bytes:
    # Git checkout settings must not decide which line-ending case is tested.
    return (FIXTURES / name).read_bytes().replace(b"\r\n", b"\n").replace(b"\n", newline)


class PrepareLavFfmpegConfigureTests(unittest.TestCase):
    def test_legacy_patch_changes_only_the_backslash_expression(self) -> None:
        for newline in (b"\n", b"\r\n"):
            with self.subTest(newline=newline):
                original = fixture("legacy.configure", newline)
                actual, mode = PREPARE.prepare_configure(original)
                expected = original.replace(br'gsub(/\\/, "/")', br'gsub(/\\\\/, "/")')
                self.assertNotEqual(actual, original)
                self.assertEqual(actual, expected)
                self.assertEqual(mode, "legacy-patched")
                self.assertEqual(PREPARE.prepare_configure(actual), (actual, "legacy-already-patched"))

    def test_native_excerpt_is_byte_exact_and_repeatable(self) -> None:
        for newline in (b"\n", b"\r\n"):
            with self.subTest(newline=newline):
                original = fixture("native.configure", newline)
                self.assertEqual(PREPARE.prepare_configure(original), (original, "native-show-includes"))
                self.assertEqual(PREPARE.prepare_configure(original), (original, "native-show-includes"))

    def rejected_inputs(self) -> dict[str, bytes]:
        native = fixture("native.configure")
        legacy = fixture("legacy.configure")
        return {
            "empty": b"",
            "unknown": b"#!/bin/sh\n# new upstream dependency generator\n",
            "loose native tokens": b"_depflags='-showIncludes'\nCC_DEPFLAGS=$CC_DEPFLAGS\n",
            "commented native flags": native.replace(b"        _depflags=", b"        # _depflags="),
            "wrong compiler": native.replace(b"_type=msvc", b"_type=gcc"),
            "missing native flags": native.replace(b"_depflags='-showIncludes'", b"_depflags='-MMD'"),
            "missing native propagation": native.replace(PREPARE.NATIVE_EVAL, b"# removed propagation"),
            "missing native export": native.replace(PREPARE.NATIVE_EXPORT, b"CC_DEPFLAGS="),
            "duplicate MSVC blocks": native + native,
            "overridden native flags": native.replace(PREPARE.NATIVE_FLAGS, PREPARE.NATIVE_FLAGS + b"\n        _depflags='-unknown'"),
            "overridden native propagation": native + b'\neval "${1}_DEPFLAGS=unknown"\n',
            "overridden native export": native + b"\nCC_DEPFLAGS=unknown\n",
            "overridden legacy command": legacy.replace(b'        _cflags_speed=', b"        _DEPCMD='unknown-generator'\n        _cflags_speed="),
            "overridden legacy flags": legacy.replace(b'        _cflags_speed=', b"        _DEPFLAGS='-unknown'\n        _cflags_speed="),
            "overridden patched legacy command": legacy.replace(PREPARE.LEGACY_COMMAND, PREPARE.PATCHED_COMMAND + b"\n        _DEPCMD='unknown-generator'"),
            "changed WSL command": legacy.replace(b"wslpath -u", b"unknown-path -u"),
            "duplicate native flags": native.replace(PREPARE.NATIVE_FLAGS, PREPARE.NATIVE_FLAGS + b"\n        " + PREPARE.NATIVE_FLAGS),
            "native flags outside MSVC": native.replace(b"        _depflags=", b"    elif other_compiler; then\n        _depflags="),
            "legacy and native mixed": legacy.replace(b'        _cflags_speed=', b"        _depflags='-showIncludes'\n        _cflags_speed=") + b"\n" + PREPARE.NATIVE_EVAL + b"\n" + PREPARE.NATIVE_EXPORT + b"\n",
            "changed legacy command": legacy.replace(b'print "$@:", $$0', b'print $$0'),
            "changed legacy flags": legacy.replace(b" -showIncludes -Zs", b" -showIncludes"),
            "legacy token only": b'# gsub(/\\\\/, "/")\n',
            "duplicate legacy command": legacy.replace(PREPARE.LEGACY_COMMAND, PREPARE.LEGACY_COMMAND + b"\n            " + PREPARE.LEGACY_COMMAND),
        }

    def test_unknown_or_ambiguous_inputs_fail_closed(self) -> None:
        for name, original in self.rejected_inputs().items():
            with self.subTest(name=name):
                with self.assertRaises(ValueError):
                    PREPARE.prepare_configure(original)

    def run_helper(self, path: Path) -> subprocess.CompletedProcess[str]:
        return subprocess.run([sys.executable, str(SCRIPT), str(path)], capture_output=True, text=True)

    def test_cli_rejection_never_mutates_input(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "configure"
            for name, original in self.rejected_inputs().items():
                with self.subTest(name=name):
                    path.write_bytes(original)
                    os.utime(path, ns=(1_000_000_000, 1_000_000_000))
                    before = path.stat().st_mtime_ns
                    completed = self.run_helper(path)
                    self.assertNotEqual(completed.returncode, 0)
                    self.assertIn("FFmpeg dependency preparation failed:", completed.stderr)
                    self.assertEqual(path.read_bytes(), original)
                    self.assertEqual(path.stat().st_mtime_ns, before)

    def test_cli_patch_and_repeat_do_not_rewrite_noop_inputs(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ffmpeg inputs ") as directory:
            path = Path(directory) / "configure"
            for name in ("legacy.configure", "native.configure"):
                for newline in (b"\n", b"\r\n"):
                    with self.subTest(name=name, newline=newline):
                        original = fixture(name, newline)
                        path.write_bytes(original)
                        expected, mode = PREPARE.prepare_configure(original)
                        completed = self.run_helper(path)
                        self.assertEqual(completed.returncode, 0, completed.stderr)
                        self.assertIn(mode, completed.stdout)
                        self.assertEqual(path.read_bytes(), expected)
                        os.utime(path, ns=(1_000_000_000, 1_000_000_000))
                        before = path.stat().st_mtime_ns
                        completed = self.run_helper(path)
                        self.assertEqual(completed.returncode, 0, completed.stderr)
                        self.assertEqual(path.read_bytes(), expected)
                        self.assertEqual(path.stat().st_mtime_ns, before)

    def test_cli_missing_file_fails_without_creating_it(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "missing-configure"
            completed = self.run_helper(path)
            self.assertNotEqual(completed.returncode, 0)
            self.assertIn("FFmpeg dependency preparation failed:", completed.stderr)
            self.assertFalse(path.exists())


if __name__ == "__main__":
    unittest.main()
