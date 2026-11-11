"""Filesystem closure tests; Apple command output is supplied by a test double."""
from __future__ import annotations

import importlib.util
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch


SCRIPT = Path(__file__).resolve().parents[1] / "player-package.py"
SPEC = importlib.util.spec_from_file_location("player_macos_alias_test", SCRIPT)
PLAYER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PLAYER)
SYSTEM = "/usr/lib/libSystem.B.dylib"


class MacOSDependencyAliasTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="openjoc dylib alias ")
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name)
        self.prefix = self.base / "builder prefix"
        self.prefix.mkdir()
        self.bundle = self.base / "portable bundle"
        (self.bundle / "bin").mkdir(parents=True)
        self.executable = self.bundle / "bin/mpv"
        self.executable.write_bytes(b"fixture:mpv")
        self.destination = self.bundle / "lib"
        self.edges = {}
        self.rpaths = {self.executable.read_bytes(): [str(self.prefix)]}
        self.commands = []

    def library(self, name, identity, dependencies=()):
        path = self.prefix / name
        path.write_bytes(("fixture:" + name).encode())
        self.edges[path.read_bytes()] = [identity, *dependencies, SYSTEM]
        self.rpaths[path.read_bytes()] = [str(self.prefix)]
        return path

    def alias(self, alias, target):
        path = self.prefix / alias
        try:
            path.symlink_to(target.name)
        except OSError as error:
            self.skipTest(f"symlinks unavailable: {error}")
        return path

    def apple_command(self, command, **kwargs):
        self.commands.append(command)
        owner = Path(command[-1])
        if command[:2] == ["otool", "-L"]:
            edges = self.edges[owner.read_bytes()]
            return str(owner) + ":\n" + "\n".join(
                f"\t{edge} (compatibility version 1.0.0, current version 1.0.0)"
                for edge in edges
            )
        if command[:2] == ["otool", "-l"]:
            return "\n".join(
                f"Load command {index}\n          cmd LC_RPATH\n      cmdsize 48\n         path {value} (offset 12)"
                for index, value in enumerate(self.rpaths.get(owner.read_bytes(), []))
            )
        if command[0] == "install_name_tool":
            return ""
        self.fail(f"unexpected external command: {command}")

    def collect(self, executable_edges):
        self.edges[self.executable.read_bytes()] = [*executable_edges, SYSTEM]
        with patch.object(PLAYER, "run", side_effect=self.apple_command):
            records, external = PLAYER.collect_macos(
                self.executable, self.destination, [self.prefix]
            )
        self.assertEqual(external, [SYSTEM])
        return records

    def assert_closed_after_prefix_removed(self):
        shutil.rmtree(self.prefix)
        changed = []
        for command in self.commands:
            if command[0] != "install_name_tool":
                continue
            for index, value in enumerate(command):
                if value == "-change":
                    target = command[index + 2]
                    self.assertTrue(target.startswith("@rpath/"))
                    name = target.removeprefix("@rpath/")
                    self.assertTrue((self.destination / name).is_file(),
                                    f"rewritten dependency has no bundled file: {target}")
                    changed.append(target)
        self.assertTrue(changed)

    def test_rpath_symlink_keeps_referenced_alias_in_bundle(self):
        actual = self.library("libsample.1.2.dylib", "@rpath/libsample.1.dylib")
        self.alias("libsample.1.dylib", actual)
        records = self.collect(["@rpath/libsample.1.dylib"])
        self.assert_closed_after_prefix_removed()
        self.assertEqual([record["path"] for record in records], ["lib/libsample.1.dylib"])
        self.assertEqual((self.destination / "libsample.1.dylib").read_bytes(),
                         b"fixture:libsample.1.2.dylib")

    def test_loader_relative_transitive_alias_stays_in_closure(self):
        leaf = self.library("libleaf.2.4.dylib", "@rpath/libleaf.2.dylib")
        self.alias("libleaf.2.dylib", leaf)
        middle = self.library("libmiddle.1.dylib", "@rpath/libmiddle.1.dylib",
                              ["@loader_path/libleaf.2.dylib"])
        records = self.collect([str(middle)])
        self.assert_closed_after_prefix_removed()
        self.assertEqual([record["path"] for record in records],
                         ["lib/libleaf.2.dylib", "lib/libmiddle.1.dylib"])

    def test_two_aliases_of_one_payload_preserve_both_edges(self):
        actual = self.library("libsample.1.2.dylib", "@rpath/libsample.1.dylib")
        self.alias("libsample.1.dylib", actual)
        self.alias("libsample.dylib", actual)
        records = self.collect(["@rpath/libsample.1.dylib", "@rpath/libsample.dylib"])
        self.assert_closed_after_prefix_removed()
        self.assertEqual([record["path"] for record in records],
                         ["lib/libsample.1.dylib", "lib/libsample.dylib"])

    def test_absolute_alias_behavior_is_preserved(self):
        actual = self.library("libsample.1.2.dylib", "@rpath/libsample.1.dylib")
        alias = self.alias("libsample.1.dylib", actual)
        records = self.collect([str(alias)])
        self.assertIn("lib/libsample.1.dylib", [record["path"] for record in records])
        self.assert_closed_after_prefix_removed()

    def test_non_symlink_rpath_library_remains_closed(self):
        self.library("libsample.1.dylib", "@rpath/libsample.1.dylib")
        records = self.collect(["@rpath/libsample.1.dylib"])
        self.assertEqual([record["path"] for record in records], ["lib/libsample.1.dylib"])
        self.assert_closed_after_prefix_removed()

    def test_missing_dependency_is_still_rejected(self):
        with self.assertRaisesRegex(RuntimeError, "unresolved Mach-O dependency"):
            self.collect(["@rpath/libmissing.1.dylib"])


@unittest.skipUnless(
    sys.platform == "darwin" and all(shutil.which(tool) for tool in
                                   ("cc", "otool", "install_name_tool", "codesign")),
    "requires native macOS compiler, loader, and signing tools",
)
class NativeMacOSDependencyAliasTests(unittest.TestCase):
    def test_relocated_alias_runs_without_original_prefix(self):
        with tempfile.TemporaryDirectory(prefix="openjoc native dylib alias ") as temporary:
            base = Path(temporary)
            prefix = base / "builder prefix"
            prefix.mkdir()
            library = prefix / "libsample.1.2.dylib"
            (base / "library.c").write_text("int sample(void) { return 73; }\n")
            (base / "main.c").write_text("int sample(void); int main(void) { return sample(); }\n")
            subprocess.run([
                "cc", "-dynamiclib", str(base / "library.c"),
                "-Wl,-headerpad_max_install_names", "-install_name", "@rpath/libsample.1.dylib",
                "-o", str(library),
            ], check=True)
            (prefix / "libsample.1.dylib").symlink_to(library.name)
            (prefix / "libsample.dylib").symlink_to(library.name)
            executable = prefix / "mpv"
            subprocess.run([
                "cc", str(base / "main.c"), f"-L{prefix}", "-lsample",
                "-Wl,-headerpad_max_install_names", "-Wl,-rpath," + str(prefix),
                "-o", str(executable),
            ], check=True)
            environment = {key: value for key, value in os.environ.items()
                           if not key.startswith("DYLD_")}
            self.assertEqual(subprocess.run([str(executable)], env=environment).returncode, 73)
            bundle = base / "portable bundle"
            (bundle / "bin").mkdir(parents=True)
            shutil.copy2(executable, bundle / "bin/mpv")
            records, _ = PLAYER.collect_macos(bundle / "bin/mpv", bundle / "lib", [prefix])
            self.assertEqual([record["path"] for record in records], ["lib/libsample.1.dylib"])
            PLAYER.sign_macos_runtime(bundle)
            shutil.rmtree(prefix)
            completed = subprocess.run([str(bundle / "bin/mpv")], env=environment,
                                       capture_output=True, text=True)
            self.assertEqual(completed.returncode, 73, completed.stderr)


if __name__ == "__main__":
    unittest.main()
