from __future__ import annotations

import importlib.util
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock


ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location(
    "player_package", ROOT / "scripts" / "player-package.py",
)
assert SPEC is not None and SPEC.loader is not None
player_package = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(player_package)


class PlayerPackageUnicodePathTests(unittest.TestCase):
    def test_objdump_receives_ascii_filename_and_unicode_working_directory(self) -> None:
        with tempfile.TemporaryDirectory(prefix="OpenJOC package — 日本語 ") as temporary_name:
            binary_dir = Path(temporary_name) / "bundle with spaces — 日本語" / "bin"
            binary_dir.mkdir(parents=True)
            executable = binary_dir / "mpv.exe"
            executable.touch()

            calls: list[tuple[list[str], dict[str, object]]] = []

            def fake_run(command: list[str], **kwargs: object) -> subprocess.CompletedProcess[str]:
                calls.append((command, kwargs))
                output = "pei-x86-64\n" if "-f" in command else "DLL Name: KERNEL32.dll\n"
                return subprocess.CompletedProcess(command, 0, output)

            with (
                mock.patch.object(player_package.shutil, "which", return_value="C:/msys64/bin/objdump.exe"),
                mock.patch.object(player_package.subprocess, "run", side_effect=fake_run),
            ):
                format_output = player_package.objdump_output(executable, "-f")
                imports = player_package.pe_imports(executable)

            self.assertIn("pei-x86-64", format_output)
            self.assertEqual(imports, ["KERNEL32.dll"])
            self.assertEqual(len(calls), 2)
            for command, kwargs in calls:
                self.assertEqual(command[-1], "mpv.exe")
                self.assertFalse(any("日本語" in argument or " " in argument for argument in command))
                self.assertEqual(kwargs["cwd"], binary_dir)
            self.assertEqual(calls[0][0][1:], ["-f", "mpv.exe"])
            self.assertEqual(calls[1][0][1:], ["-p", "mpv.exe"])

    def test_objdump_uses_byte_identical_copy_for_unicode_pe_basename(self) -> None:
        with tempfile.TemporaryDirectory(prefix="OpenJOC package — 日本語 ") as temporary_name:
            binary_dir = Path(temporary_name) / "bundle with spaces — 日本語" / "bin"
            binary_dir.mkdir(parents=True)
            executable = binary_dir / "日本語.dll"
            executable.write_bytes(b"byte-identical PE fixture")
            inspected: list[tuple[list[str], Path, bytes]] = []

            def fake_run(command: list[str], *, cwd: Path) -> str:
                copy_path = cwd / command[-1]
                inspected.append((command, cwd, copy_path.read_bytes()))
                return "pei-x86-64"

            with (
                mock.patch.object(player_package.shutil, "which", return_value="C:/msys64/bin/objdump.exe"),
                mock.patch.object(player_package, "run", side_effect=fake_run),
            ):
                output = player_package.objdump_output(executable, "-f")

            self.assertEqual(output, "pei-x86-64")
            self.assertEqual(len(inspected), 1)
            command, cwd, copied_bytes = inspected[0]
            self.assertEqual(command[-1], "inspection.bin")
            self.assertTrue(all(argument.isascii() for argument in command))
            self.assertEqual(cwd.parent, binary_dir)
            self.assertTrue(cwd.name.startswith(".openjoc-objdump-"))
            self.assertEqual(copied_bytes, executable.read_bytes())
            self.assertFalse((cwd / command[-1]).exists(), "temporary inspection copy should be removed")
            self.assertTrue(executable.is_file(), "static inspection must not move or alter the actual package file")


if __name__ == "__main__":
    unittest.main()
