# SPDX-FileCopyrightText: 2026 OpenJOC contributors
# SPDX-License-Identifier: Apache-2.0

from __future__ import annotations

import json
import hashlib
import pathlib
import subprocess
import sys
import tempfile
import unittest
import zipfile
import xml.etree.ElementTree as ET


WORKSPACE = pathlib.Path(__file__).resolve().parents[2]
TEMPLATE = WORKSPACE / "packaging" / "windows-lav"
PACKAGE_SCRIPT = WORKSPACE / "scripts" / "package_lav_release.py"
FFMPEG_DLLS = (
    "avcodec-lav-63.dll",
    "avfilter-lav-12.dll",
    "avformat-lav-63.dll",
    "avutil-lav-61.dll",
    "swresample-lav-7.dll",
    "swscale-lav-10.dll",
)
LAV_SUPPORT_DLLS = (
    "libbluray.dll",
    "libgcc_s_seh-1.dll",
    "libwinpthread-1.dll",
    "zlib1.dll",
)
CRT_DLLS = (
    *(f"api-ms-win-crt-{name}-l1-1-0.dll" for name in (
        "conio", "convert", "environment", "filesystem", "heap", "locale",
        "math", "multibyte", "private", "process", "runtime", "stdio",
        "string", "time", "utility",
    )),
    "ucrtbase.dll",
    "vcruntime140.dll",
    "vcruntime140_1.dll",
    "vcruntime140_threads.dll",
    "zlibwapi.dll",
)


class LavReleasePackageTests(unittest.TestCase):
    def test_package_contains_only_the_current_msvc_runtime_closure(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            lav = root / "lav"
            (lav / "bin_x64").mkdir(parents=True)
            for name in (*FFMPEG_DLLS, *LAV_SUPPORT_DLLS, "LAVAudio.ax"):
                (lav / "bin_x64" / name).write_bytes(name.encode("ascii"))
            manifest = lav / "decoder" / "LAVAudio" / "LAVAudio.manifest"
            manifest.parent.mkdir(parents=True)
            manifest.write_text("<assembly />\n", encoding="utf-8")
            capi = root / "openjoc_capi.dll"
            capi.write_bytes(b"openjoc-capi")
            dependencies = root / "dependencies"
            dependencies.mkdir()
            for name in CRT_DLLS:
                (dependencies / name).write_bytes(name.encode("ascii"))
            output = root / "out"
            completed = subprocess.run(
                [
                    sys.executable,
                    str(PACKAGE_SCRIPT),
                    "--release-version", "0.20.1",
                    "--lav-root", str(lav),
                    "--capi-dll", str(capi),
                    "--dependency-dir", str(dependencies),
                    "--onboarding-template", str(TEMPLATE),
                    "--output-dir", str(output),
                ],
                capture_output=True,
                text=True,
            )
            self.assertEqual(completed.returncode, 0, completed.stdout + completed.stderr)
            archive = output / "openjoc-lav-0.20.1-windows-x64.zip"
            self.assertTrue(archive.is_file())
            with zipfile.ZipFile(archive) as handle:
                names = set(handle.namelist())
                self.assertIn("runtime/OpenJocRuntimeProfile.json", names)
                self.assertIn("runtime/libbluray.dll", names)
                profile = json.loads(handle.read("runtime/OpenJocRuntimeProfile.json"))
                self.assertEqual(profile["version"], "0.20.1")
                self.assertEqual(profile["architecture"], "x64")
                self.assertEqual(
                    set(profile["required_runtime_files"]),
                    {"LAVAudio.ax", "LAVAudio.ax.manifest", "LAVFilters.Dependencies.manifest",
                     "openjoc_capi.dll", *LAV_SUPPORT_DLLS, *FFMPEG_DLLS, *CRT_DLLS},
                )
                self.assertIn("runtime/LAVAudio.ax.manifest", names)
                self.assertIn("runtime/LAVFilters.Dependencies.manifest", names)
                self.assertNotIn("variant", profile)
                self.assertNotIn("runtime/LAVVideo.ax", names)
                self.assertNotIn("runtime/LAVSplitter.ax", names)
                self.assertNotIn("runtime/IntelQuickSyncDecoder.dll", names)
                self.assertNotIn("register-all.bat", names)
                self.assertNotIn("unregister-all.bat", names)
                for path in TEMPLATE.rglob("*"):
                    if path.is_file():
                        self.assertEqual(
                            handle.read(path.relative_to(TEMPLATE).as_posix()).decode("utf-8"),
                            path.read_text(encoding="utf-8"),
                        )
                manifest = ET.fromstring(handle.read("runtime/LAVFilters.Dependencies.manifest"))
                self.assertEqual(
                    [entry.attrib["name"] for entry in manifest.findall("{urn:schemas-microsoft-com:asm.v1}file")],
                    list(FFMPEG_DLLS),
                )
                for line in handle.read("PACKAGE_SHA256SUMS.txt").decode().splitlines():
                    digest, name = line.split("  ", 1)
                    self.assertEqual(hashlib.sha256(handle.read(name)).hexdigest().upper(), digest)

            # Making the default explicit must not change any package byte.
            default_bytes = archive.read_bytes()
            explicit_output = root / "explicit-audio"
            command = list(completed.args)
            command[command.index("--output-dir") + 1] = str(explicit_output)
            command.extend(("--variant", "audio"))
            explicit = subprocess.run(command, capture_output=True, text=True)
            self.assertEqual(explicit.returncode, 0, explicit.stdout + explicit.stderr)
            self.assertEqual((explicit_output / archive.name).read_bytes(), default_bytes)


if __name__ == "__main__":
    unittest.main()
