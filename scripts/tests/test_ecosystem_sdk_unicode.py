"""Decode pkg-config path bytes only after shell unquoting, without a shell."""
from __future__ import annotations

import contextlib
import importlib.util
import io
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

SPEC = importlib.util.spec_from_file_location(
    "ecosystem_sdk_unicode", Path(__file__).resolve().parents[1] / "verify-ecosystem-package.py"
)
assert SPEC is not None and SPEC.loader is not None
VERIFY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VERIFY)

# pkgconf 1.8.1 prefixes each non-ASCII UTF-8 byte with a backslash.
UNICODE_BYTES = [
    (b"\\\xc3\\\xbc", "ü"),
    (b"\\\xe9\\\x9f\\\xb3\\\xe9\\\xa2\\\x91", "音频"),
    (b"\\\xf0\\\x9f\\\x8e\\\xa7", "🎧"),
]


class PkgConfigByteParsingTests(unittest.TestCase):
    def test_byte_escaped_unicode_is_reassembled_before_decoding(self):
        for escaped, decoded in UNICODE_BYTES:
            with self.subTest(path=decoded):
                output = b"-I/SDK\\ " + escaped + b"/include -lopenjoc_capi\n"
                self.assertEqual(VERIFY.parse_pkg_config_flags(output), [f"-I/SDK {decoded}/include", "-lopenjoc_capi"])

    def test_unescaped_utf8_and_quoted_fragments_remain_unicode(self):
        self.assertEqual(
            VERIFY.parse_pkg_config_flags('-I"/SDK ü/include" -L\'/音频 🎧/lib\' -DNAME="two words"'.encode("utf-8")),
            ["-I/SDK ü/include", "-L/音频 🎧/lib", "-DNAME=two words"],
        )

    def test_msys_drive_paths_and_literal_backslashes_keep_their_bytes(self):
        self.assertEqual(
            VERIFY.parse_pkg_config_flags(b"-IC:/SDK\\ " + UNICODE_BYTES[0][0] + b"/include " + rb"-DROOT=C:\\SDK\\lib -DNAME=\$HOME"),
            ["-IC:/SDK ü/include", r"-DROOT=C:\SDK\lib", "-DNAME=$HOME"],
        )

    def test_lexical_bridge_preserves_all_nonascii_bytes(self):
        # Verify bytes before host decoding, so this test also runs on Windows.
        for value in range(128, 256):
            raw = b"-I/path/" + bytes([value])
            with self.subTest(byte=value), mock.patch.object(VERIFY.os, "fsdecode", return_value="decoded") as decode:
                self.assertEqual(VERIFY.parse_pkg_config_flags(b"-I/path/\\" + bytes([value])), ["decoded"])
                decode.assert_called_once_with(raw)

    @unittest.skipUnless(os.name == "posix", "POSIX byte filenames require surrogateescape")
    def test_non_utf8_posix_filename_bytes_are_not_replaced_or_dropped(self):
        flags = VERIFY.parse_pkg_config_flags(b"-I/path/\\\xff -L/path/\\\xa0")
        self.assertEqual([os.fsencode(flag) for flag in flags], [b"-I/path/\xff", b"-L/path/\xa0"])

    def test_malformed_shell_quoting_fails_closed(self):
        for output in [b"-I'not closed", b'-I"not closed', b"-I/dangling\\"]:
            with self.subTest(output=output), self.assertRaises(ValueError):
                VERIFY.parse_pkg_config_flags(output)


class SdkBinaryCaptureTests(unittest.TestCase):
    def run_boundary(self, payload: bytes, *, malformed: bool = False, platform: str = "linux-x86_64"):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "sdk"
            (root / "examples").mkdir(parents=True)
            (root / "examples/c_api_example.c").write_text("int main(void) { return 0; }\n")
            tool = Path(temporary) / "tools/cc"
            tool.parent.mkdir()
            completed = subprocess.CompletedProcess([], 0, stdout="")
            with mock.patch.object(VERIFY.shutil, "which", side_effect=lambda name: str(tool.parent / name)), mock.patch.object(VERIFY.subprocess, "check_output", return_value=payload) as query, mock.patch.object(VERIFY.subprocess, "run", return_value=completed) as run, mock.patch.object(VERIFY, "run_consumer") as consumer, contextlib.redirect_stdout(io.StringIO()):
                if malformed:
                    with self.assertRaises(ValueError):
                        VERIFY.verify_sdk_consumers(root, platform)
                    self.assertEqual(run.call_count, 1)
                    self.assertEqual(consumer.call_count, 1)
                else:
                    VERIFY.verify_sdk_consumers(root, platform)
                    self.assertEqual(run.call_args_list[1].args[0][2:-2], ["-I/SDK ü/include", "-lopenjoc_capi"])
                    self.assertEqual(consumer.call_count, 3)
                query.assert_called_once()
                self.assertFalse(query.call_args.kwargs.get("text", False))
                self.assertNotIn("encoding", query.call_args.kwargs)
                self.assertNotIn("errors", query.call_args.kwargs)

    def test_sdk_captures_bytes_before_building_unicode_argv(self):
        for platform in ("linux-x86_64", "macos-arm64", "windows-x64"):
            with self.subTest(platform=platform):
                self.run_boundary(b"-I/SDK\\ " + UNICODE_BYTES[0][0] + b"/include -lopenjoc_capi", platform=platform)

    def test_malformed_output_never_starts_pkg_config_consumer(self):
        for payload in [b"-I'not closed", b'-I"not closed', b"-I/dangling\\"]:
            with self.subTest(payload=payload):
                self.run_boundary(payload, malformed=True)


@unittest.skipUnless(sys.platform.startswith("linux") and shutil.which("cc") and shutil.which("pkg-config"), "requires Linux C compiler and pkg-config")
class NativeUnicodeSdkTests(unittest.TestCase):
    def test_unicode_paths_compile_and_run_direct_and_pkg_config_consumers(self):
        for name in ["SDK ü (release)", "SDK 音频", "SDK 🎧"]:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary) / name
                for relative in ("include", "lib/pkgconfig", "examples"):
                    (root / relative).mkdir(parents=True)
                (root / "include/openjoc.h").write_text("int unicode_path_probe(void);\n")
                (root / "examples/c_api_example.c").write_text(
                    '#include "openjoc.h"\nint main(void) { return unicode_path_probe() == 73 ? 0 : 1; }\n'
                )
                source = Path(temporary) / "library.c"
                source.write_text("int unicode_path_probe(void) { return 73; }\n")
                subprocess.run(
                    [shutil.which("cc"), "-shared", "-fPIC", str(source), "-o", str(root / "lib/libopenjoc_capi.so")],
                    check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                )
                # A C link/path fixture only, not the OpenJOC ABI or decoder.
                (root / "lib/pkgconfig/openjoc.pc").write_text(
                    "prefix=${pcfiledir}/../..\nincludedir=${prefix}/include\nlibdir=${prefix}/lib\n"
                    "Name: openjoc\nDescription: SDK Unicode path fixture\nVersion: 0.0.0\n"
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
