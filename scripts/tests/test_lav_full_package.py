# SPDX-FileCopyrightText: 2026 OpenJOC contributors
# SPDX-License-Identifier: Apache-2.0

"""Synthetic packaging checks only: no native loads or filter registration."""

from __future__ import annotations

import hashlib
import json
import pathlib
import shutil
import subprocess
import sys
import tempfile
import unittest
import xml.etree.ElementTree as ET
import zipfile


WORKSPACE = pathlib.Path(__file__).resolve().parents[2]
SCRIPTS = WORKSPACE / "scripts"
sys.path.insert(0, str(SCRIPTS))
import package_lav_release as packager

TEMPLATE = WORKSPACE / "packaging" / "windows-lav-full"
FULL_EXTRA_CRT = {
    "concrt140.dll", "msvcp140.dll", "msvcp140_1.dll", "msvcp140_2.dll",
    "msvcp140_atomic_wait.dll", "msvcp140_codecvt_ids.dll", "vccorlib140.dll",
}
FILTERS = {"LAVAudio.ax", "LAVVideo.ax", "LAVSplitter.ax"}
MANIFEST_SOURCES = {
    "LAVAudio.ax.manifest": "decoder/LAVAudio/LAVAudio.manifest",
    "LAVVideo.ax.manifest": "decoder/LAVVideo/LAVVideo.manifest",
    "LAVSplitter.ax.manifest": "demuxer/LAVSplitter/LAVSplitter.manifest",
}


class LavFullPackageTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        # Match the packager's resolved paths (Windows TEMP may use 8.3 names).
        self.root = pathlib.Path(self.temporary.name).resolve()
        self.lav = self.root / "lav"
        (self.lav / "bin_x64").mkdir(parents=True)
        for name in (*packager.FFMPEG_DLLS, *packager.LAV_SUPPORT_DLLS,
                     *FILTERS, "IntelQuickSyncDecoder.dll"):
            (self.lav / "bin_x64" / name).write_bytes(name.encode("ascii"))
        for name, relative in MANIFEST_SOURCES.items():
            manifest = self.lav / relative
            manifest.parent.mkdir(parents=True)
            manifest.write_text(f'<assembly name="{name}" />\n', encoding="utf-8")
        for relative in packager.FULL_LAV_LICENSE_FILES:
            notice = self.lav / relative
            notice.parent.mkdir(parents=True, exist_ok=True)
            notice.write_text(f"Synthetic license fixture: {relative}\n", encoding="utf-8")
        self.capi = self.root / "openjoc_capi.dll"
        self.capi.write_bytes(b"synthetic-openjoc-capi")
        self.core = self.root / "capi-source"
        self.core.mkdir()
        for name in packager.FULL_CORE_LICENSE_FILES:
            (self.core / name).write_bytes(f"Synthetic actual CAPI source: {name}\n".encode())
        self.dependencies = self.root / "dependencies"
        self.dependencies.mkdir()
        for name in {*packager.CRT_DLLS, *FULL_EXTRA_CRT}:
            (self.dependencies / name).write_bytes(name.encode("ascii"))
        self.output = self.root / "out"

    def package(
        self, *, variant: str = "full", template: pathlib.Path = TEMPLATE,
        include_core_source: bool = True,
    ) -> subprocess.CompletedProcess:
        return subprocess.run(
            [sys.executable, str(SCRIPTS / "package_lav_release.py"),
             "--release-version", "0.20.0", "--variant", variant,
             "--lav-root", str(self.lav), "--capi-dll", str(self.capi),
             "--dependency-dir", str(self.dependencies),
             "--onboarding-template", str(template), "--output-dir", str(self.output),
             *(["--core-source-root", str(self.core)] if include_core_source else [])],
            capture_output=True, text=True,
        )

    def assert_failed_without_archive(self, result: subprocess.CompletedProcess, reason: str) -> None:
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(reason, result.stderr)
        self.assertEqual(list(self.output.glob("*.zip")), [])

    def test_full_inventory_manifests_and_hashes_match_the_complete_payload(self) -> None:
        # Unselected files must not expand the verified runtime closure.
        (self.dependencies / "unrelated.dll").write_bytes(b"not part of runtime")
        result = self.package()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        archive = self.output / "openjoc-lav-full-0.20.0-windows-x64.zip"
        with zipfile.ZipFile(archive) as handle:
            names = set(handle.namelist())
            profile = json.loads(handle.read("runtime/OpenJocRuntimeProfile.json"))
            expected = {
                *FILTERS, *MANIFEST_SOURCES, "IntelQuickSyncDecoder.dll", "openjoc_capi.dll",
                *packager.FFMPEG_DLLS, *packager.LAV_SUPPORT_DLLS, *packager.CRT_DLLS,
                *FULL_EXTRA_CRT, "LAVFilters.Dependencies.manifest",
            }
            self.assertEqual(profile["version"], "0.20.0")
            self.assertEqual(profile["architecture"], "x64")
            self.assertEqual(profile["variant"], "full")
            self.assertEqual(set(profile["required_runtime_files"]), expected)
            self.assertEqual(len(profile["required_runtime_files"]), len(expected))
            self.assertEqual({n.removeprefix("runtime/") for n in names if n.startswith("runtime/")},
                             expected | {"OpenJocRuntimeProfile.json"})
            binaries = {n for n in names if n.endswith((".dll", ".ax"))}
            self.assertEqual(len(binaries), 42)
            self.assertTrue(all(pathlib.PurePosixPath(n).parent.as_posix() == "runtime" for n in binaries))
            manifest = ET.fromstring(handle.read("runtime/LAVFilters.Dependencies.manifest"))
            manifest_names = [entry.attrib["name"] for entry in manifest.findall("{urn:schemas-microsoft-com:asm.v1}file")]
            self.assertEqual(manifest_names, sorted(n for n in expected if n.endswith(".dll")))
            for name, relative in MANIFEST_SOURCES.items():
                self.assertEqual(handle.read(f"runtime/{name}"), (self.lav / relative).read_bytes())
            for path in TEMPLATE.rglob("*"):
                if path.is_file():
                    self.assertEqual(handle.read(path.relative_to(TEMPLATE).as_posix()), path.read_bytes())
            self.assertNotIn("install.bat", names)
            self.assertNotIn("uninstall.bat", names)
            self.assertIn("register-all.bat", names)
            self.assertIn("unregister-all.bat", names)
            self.assertIn("SOURCE_AND_LICENSES.md", names)
            for relative in packager.FULL_LAV_LICENSE_FILES:
                self.assertEqual(handle.read(f"LICENSES/lav/{relative}"), (self.lav / relative).read_bytes())
            for relative in packager.FULL_CORE_LICENSE_FILES:
                copied = handle.read(f"LICENSES/openjoc/{relative}")
                self.assertEqual(copied, (self.core / relative).read_bytes())
                self.assertNotEqual(copied, (WORKSPACE / relative).read_bytes())
            hashed = set()
            for line in handle.read("PACKAGE_SHA256SUMS.txt").decode().splitlines():
                digest, name = line.split("  ", 1)
                self.assertNotIn(name, hashed)
                hashed.add(name)
                self.assertEqual(hashlib.sha256(handle.read(name)).hexdigest().upper(), digest)
            self.assertEqual(hashed, names - {"PACKAGE_SHA256SUMS.txt"})

    def test_missing_full_filter_quicksync_manifest_or_crt_fails_closed(self) -> None:
        paths = [
            *(self.lav / "bin_x64" / name for name in ("LAVVideo.ax", "LAVSplitter.ax", "IntelQuickSyncDecoder.dll")),
            *(self.lav / MANIFEST_SOURCES[name] for name in ("LAVVideo.ax.manifest", "LAVSplitter.ax.manifest")),
            *(self.dependencies / name for name in sorted(FULL_EXTRA_CRT)),
        ]
        for path in paths:
            with self.subTest(missing=path.name):
                original = path.read_bytes()
                path.unlink()
                try:
                    self.assert_failed_without_archive(self.package(), str(path))
                finally:
                    path.write_bytes(original)

    def test_full_rejects_audio_installer_template(self) -> None:
        self.assert_failed_without_archive(
            self.package(template=WORKSPACE / "packaging" / "windows-lav"),
            "required full package template input is missing",
        )

    def test_audio_rejects_full_registration_template(self) -> None:
        self.assert_failed_without_archive(self.package(variant="audio"), "existing audio onboarding template")

    def test_missing_required_full_source_license_fails_closed(self) -> None:
        for relative in packager.FULL_LAV_LICENSE_FILES:
            with self.subTest(missing=relative):
                path = self.lav / relative
                original = path.read_bytes()
                path.unlink()
                try:
                    self.assert_failed_without_archive(self.package(), str(path))
                finally:
                    path.write_bytes(original)

    def test_core_source_root_is_required_for_full_but_not_audio(self) -> None:
        self.assert_failed_without_archive(
            self.package(include_core_source=False), "--core-source-root is required for full package"
        )
        result = self.package(variant="audio", template=WORKSPACE / "packaging" / "windows-lav",
                              include_core_source=False)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_missing_core_source_directory_fails_closed(self) -> None:
        self.core = self.root / "missing-capi-source"
        self.assert_failed_without_archive(self.package(), "core source root is missing")

    def test_missing_actual_core_source_license_fails_closed(self) -> None:
        for relative in packager.FULL_CORE_LICENSE_FILES:
            with self.subTest(missing=relative):
                path = self.core / relative
                original = path.read_bytes()
                path.unlink()
                try:
                    self.assert_failed_without_archive(self.package(), str(path))
                finally:
                    path.write_bytes(original)

    def test_additional_nested_lav_licenses_are_preserved(self) -> None:
        notice = self.lav / "libbluray" / "contrib" / "libudfread" / "COPYING"
        notice.parent.mkdir(parents=True)
        notice.write_bytes(b"Synthetic nested copyright notice\n")
        result = self.package()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        with zipfile.ZipFile(self.output / "openjoc-lav-full-0.20.0-windows-x64.zip") as handle:
            self.assertEqual(handle.read("LICENSES/lav/libbluray/contrib/libudfread/COPYING"), notice.read_bytes())

    def test_full_rejects_template_with_audio_installer_added(self) -> None:
        template = self.root / "mixed-template"
        shutil.copytree(TEMPLATE, template)
        (template / "install.bat").write_text("@echo off\n", encoding="utf-8")
        self.assert_failed_without_archive(self.package(template=template), "separate manual-registration template")

    def test_audio_and_full_outputs_can_coexist_without_replacement(self) -> None:
        result = self.package(variant="audio", template=WORKSPACE / "packaging" / "windows-lav")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        audio = self.output / "openjoc-lav-0.20.0-windows-x64.zip"
        audio_bytes = audio.read_bytes()
        result = self.package()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(audio.read_bytes(), audio_bytes)
        full = self.output / "openjoc-lav-full-0.20.0-windows-x64.zip"
        full_bytes = full.read_bytes()
        result = self.package()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("refusing to replace existing package", result.stderr)
        self.assertEqual(full.read_bytes(), full_bytes)
        self.assertEqual(audio.read_bytes(), audio_bytes)

    def test_variant_must_be_explicitly_supported(self) -> None:
        self.assert_failed_without_archive(self.package(variant="all"), "invalid choice")


class LavFullRegistrationTemplateTests(unittest.TestCase):
    def test_refactored_registration_source_is_pinned(self) -> None:
        script = TEMPLATE / "scripts" / "manual-registration.ps1"
        # The extraction-equivalence test separately proves the original regfix
        # statements and process helper are unchanged. Pin this complete source
        # too; normalize only checkout-dependent line endings.
        digest = hashlib.sha256(script.read_bytes().replace(b"\r\n", b"\n")).hexdigest()
        self.assertEqual(digest, "7f4b04991801833c3f403873ed134c5c13dac9563a95251001e610834520a6cd")

    def test_launchers_keep_explicit_confirmation_and_x64_host(self) -> None:
        for name, operation in (("register-all", "Register"), ("unregister-all", "Unregister")):
            with self.subTest(launcher=name):
                text = (TEMPLATE / f"{name}.bat").read_text(encoding="utf-8")
                self.assertIn('choice /C YN /N /M "Explicitly', text)
                self.assertIn("if errorlevel 2 exit /b 10", text)
                self.assertIn("Sysnative", text)
                self.assertIn("%SystemRoot%\\System32\\WindowsPowerShell", text)
                self.assertIn('"%~dp0scripts\\manual-registration.ps1"', text)
                self.assertIn(f"-Operation {operation}", text)
                self.assertIn('set "RESULT=%ERRORLEVEL%"', text)
                self.assertIn("exit /b %RESULT%", text)

    def test_registry_guard_covers_filter_source_and_property_page_classes(self) -> None:
        classes = json.loads((TEMPLATE / "scripts" / "registration-classes.json").read_text())
        self.assertEqual(set(classes), FILTERS)
        self.assertEqual({name: len(ids) for name, ids in classes.items()},
                         {"LAVAudio.ax": 7, "LAVVideo.ax": 3, "LAVSplitter.ax": 4})
        self.assertIn("27247580-C701-40CD-886D-E618FC8C9FFF", classes["LAVAudio.ax"])
        self.assertIn("EE30215D-164F-4A92-A4EB-9D4C13390F9F", classes["LAVVideo.ax"])
        self.assertIn("171252A0-8820-4AFE-9DF8-5C92B2D66B04", classes["LAVSplitter.ax"])
        self.assertIn("B98D13E7-55DB-4385-A33D-09FD1BA26338", classes["LAVSplitter.ax"])


if __name__ == "__main__":
    unittest.main()
