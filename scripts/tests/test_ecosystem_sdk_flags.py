"""Preserve pkg-config shell quoting at the SDK compiler command boundary."""
from __future__ import annotations

import contextlib
import importlib.util
import io
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

SPEC = importlib.util.spec_from_file_location(
    "ecosystem_sdk_flags", Path(__file__).resolve().parents[1] / "verify-ecosystem-package.py"
)
assert SPEC is not None and SPEC.loader is not None
VERIFY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VERIFY)


class SdkPkgConfigFlagsTests(unittest.TestCase):
    def check_flags(self, output: str, expected: list[str], platform: str = "linux-x86_64") -> None:
        with tempfile.TemporaryDirectory(prefix="openjoc sdk flags ") as temporary:
            root = Path(temporary) / "SDK bundle"
            (root / "examples").mkdir(parents=True)
            (root / "examples/c_api_example.c").write_text("int main(void) { return 0; }\n")
            compiler = Path(temporary) / "tools/cc"
            compiler.parent.mkdir()
            commands = []

            def which(name):
                return str(compiler.parent / name) if name in {"cc", "pkg-config", "cmake"} else None

            def run(command, **kwargs):
                commands.append(command)
                return subprocess.CompletedProcess(command, 0, stdout="")

            with mock.patch.object(VERIFY.shutil, "which", side_effect=which), mock.patch.object(VERIFY.subprocess, "run", side_effect=run), mock.patch.object(VERIFY.subprocess, "check_output", return_value=output) as query, mock.patch.object(VERIFY, "run_consumer") as consumer, contextlib.redirect_stdout(io.StringIO()):
                VERIFY.verify_sdk_consumers(root, platform)
            query.assert_called_once()
            self.assertEqual(query.call_args.args[0][1:], ["--cflags", "--libs", "openjoc"])
            self.assertEqual(query.call_args.kwargs["env"]["PKG_CONFIG_PATH"], str(root / "lib/pkgconfig"))
            self.assertEqual(commands[1][2:-2], expected)
            self.assertEqual(consumer.call_count, 3)
            self.assertEqual([call.args[3] for call in consumer.call_args_list], ["direct compiler", "pkg-config", "CMake CONFIG"])
            self.assertTrue(all(isinstance(command, list) for command in commands))

    def test_escaped_spaces_are_single_compiler_arguments(self):
        for platform in ("linux-x86_64", "macos-arm64", "windows-x64"):
            with self.subTest(platform=platform):
                self.check_flags(
                    r"-I/SDK\ bundle/include -L/SDK\ bundle/lib -lopenjoc_capi" + "\n",
                    ["-I/SDK bundle/include", "-L/SDK bundle/lib", "-lopenjoc_capi"], platform,
                )

    def test_quoted_paths_and_macro_values_preserve_argument_boundaries(self):
        self.check_flags(
            '-I"/SDK ü/include" -L\'/SDK ü/lib\' -DNAME="two words" -lopenjoc_capi\n',
            ["-I/SDK ü/include", "-L/SDK ü/lib", "-DNAME=two words", "-lopenjoc_capi"],
        )

    def test_msys_style_drive_paths_use_pkg_config_shell_escaping(self):
        self.check_flags(
            r"-IC:/SDK\ bundle/include -LC:/SDK\ bundle/lib -lopenjoc_capi",
            ["-IC:/SDK bundle/include", "-LC:/SDK bundle/lib", "-lopenjoc_capi"], "windows-x64",
        )

    def test_ordinary_flags_and_order_are_unchanged(self):
        self.check_flags(
            "-I/sdk/include\t-L/sdk/lib -lopenjoc_capi -lm -Wl,--as-needed\n",
            ["-I/sdk/include", "-L/sdk/lib", "-lopenjoc_capi", "-lm", "-Wl,--as-needed"],
        )

    def test_shell_metacharacters_remain_literal_arguments(self):
        self.check_flags(
            r"-I/SDK\ \(release\)/include -DNAME=\$HOME -lopenjoc_capi",
            ["-I/SDK (release)/include", "-DNAME=$HOME", "-lopenjoc_capi"],
        )


@unittest.skipUnless(sys.platform.startswith("linux") and shutil.which("cc") and shutil.which("pkg-config"), "requires Linux C compiler and pkg-config")
class NativePkgConfigPathTests(unittest.TestCase):
    def test_space_containing_sdk_root_compiles_and_runs_both_consumers(self):
        with tempfile.TemporaryDirectory(prefix="openjoc sdk native ") as temporary:
            root = Path(temporary) / "SDK bundle (release)"
            for relative in ("include", "lib/pkgconfig", "examples"):
                (root / relative).mkdir(parents=True)
            (root / "include/openjoc.h").write_text("int path_probe(void);\n")
            (root / "examples/c_api_example.c").write_text(
                '#include "openjoc.h"\nint main(void) { return path_probe() == 73 ? 0 : 1; }\n'
            )
            source = Path(temporary) / "library.c"
            source.write_text("int path_probe(void) { return 73; }\n")
            subprocess.run(
                [shutil.which("cc"), "-shared", "-fPIC", str(source), "-o", str(root / "lib/libopenjoc_capi.so")],
                check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            )
            # This is only a tiny C link fixture, not the OpenJOC ABI or decoder.
            (root / "lib/pkgconfig/openjoc.pc").write_text(
                "prefix=${pcfiledir}/../..\nincludedir=${prefix}/include\nlibdir=${prefix}/lib\n"
                "Name: openjoc\nDescription: SDK path regression fixture\nVersion: 0.0.0\n"
                "Cflags: -I${includedir}\nLibs: -L${libdir} -lopenjoc_capi\n"
            )
            original_which = shutil.which
            output = io.StringIO()
            with mock.patch.object(VERIFY.shutil, "which", side_effect=lambda name: None if name == "cmake" else original_which(name)), contextlib.redirect_stdout(output):
                with self.assertRaisesRegex(SystemExit, "SDK qualification requires CMake"):
                    VERIFY.verify_sdk_consumers(root, "linux-x86_64")
            self.assertEqual(output.getvalue(), "SDK direct compiler consumer: PASS\nSDK pkg-config consumer: PASS\n")


if __name__ == "__main__":
    unittest.main()
