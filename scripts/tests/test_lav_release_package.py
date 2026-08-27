# SPDX-FileCopyrightText: 2026 OpenJOC contributors
# SPDX-License-Identifier: Apache-2.0

from __future__ import annotations

import json
import pathlib
import subprocess
import sys
import tempfile
import unittest
import zipfile


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
            for name in (*FFMPEG_DLLS, "LAVAudio.ax"):
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
                    "--release-version", "0.13.0",
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
            archive = output / "openjoc-lav-0.13.0-windows-x64.zip"
            self.assertTrue(archive.is_file())
            with zipfile.ZipFile(archive) as handle:
                names = set(handle.namelist())
                self.assertIn("runtime/OpenJocRuntimeProfile.json", names)
                self.assertNotIn("runtime/libbluray.dll", names)
                profile = json.loads(handle.read("runtime/OpenJocRuntimeProfile.json"))
                self.assertEqual(profile["version"], "0.13.0")
                self.assertEqual(profile["architecture"], "x64")
                self.assertEqual(
                    set(profile["required_runtime_files"]),
                    {"LAVAudio.ax", "LAVAudio.ax.manifest", "LAVFilters.Dependencies.manifest",
                     "openjoc_capi.dll", *FFMPEG_DLLS, *CRT_DLLS},
                )
                self.assertIn("runtime/LAVAudio.ax.manifest", names)
                self.assertIn("runtime/LAVFilters.Dependencies.manifest", names)


if __name__ == "__main__":
    unittest.main()
