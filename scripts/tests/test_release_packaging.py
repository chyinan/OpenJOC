# SPDX-FileCopyrightText: 2026 OpenJOC contributors
# SPDX-License-Identifier: Apache-2.0

# pattern: Imperative Shell

from __future__ import annotations

import os
import pathlib
import subprocess
import sys
import tempfile
import unittest


SCRIPTS = pathlib.Path(__file__).resolve().parents[1]
SCRIPT = SCRIPTS / "release_packaging.py"
sys.path.insert(0, str(SCRIPTS))

from release_packaging import (  # noqa: E402
    CANONICAL_RELEASE_VERSION,
    LAV_METADATA_FILES,
    LAV_MODIFIED_FILES,
    LAV_NEW_FILES,
    _copy_onboarding_source,
    _prepare_binary_base,
    _write_text,
    ensure_new_directory,
)


EXPECTED_LAV_NEW_FILES = {
    "decoder/LAVAudio/LAVOpenJocDiagnostics.h",
    "decoder/LAVAudio/AudioStatusCapacityTests.cpp",
    "decoder/LAVAudio/LAVAudioIdentitySmoke.cpp",
    "decoder/LAVAudio/LAVAudioResourceIdentitySmoke.cpp",
    "decoder/LAVAudio/OpenJocAdmission.cpp",
    "decoder/LAVAudio/OpenJocAdmission.h",
    "decoder/LAVAudio/OpenJocAdmissionTests.cpp",
    "decoder/LAVAudio/OpenJocDecoder.cpp",
    "decoder/LAVAudio/OpenJocDecoder.h",
    "decoder/LAVAudio/OpenJocDecoderSmoke.cpp",
    "decoder/LAVAudio/OpenJocDirectShowNegotiationSmoke.cpp",
    "decoder/LAVAudio/OpenJocOutput.cpp",
    "decoder/LAVAudio/OpenJocOutput.h",
    "decoder/LAVAudio/OpenJocOutputTests.cpp",
    "decoder/LAVAudio/OpenJocPolicyControl.cpp",
    "decoder/LAVAudio/OpenJocPropertyPageSmoke.cpp",
    "decoder/LAVAudio/OpenJocSettingsSmoke.cpp",
    "decoder/LAVAudio/OpenJocShippedLayouts.cpp",
    "decoder/LAVAudio/OpenJocShippedLayouts.h",
    "decoder/LAVAudio/OpenJocShippedLayoutsTests.cpp",
    "decoder/LAVAudio/OpenJocStrictNegotiation.cpp",
    "decoder/LAVAudio/OpenJocStrictNegotiation.h",
    "decoder/LAVAudio/OpenJocStrictOutput.cpp",
    "decoder/LAVAudio/OpenJocStrictOutput.h",
    "decoder/LAVAudio/OpenJocStrictOutputTests.cpp",
    "include/LAVOpenJocSettings.h",
}
EXPECTED_LAV_MODIFIED_FILES = {
    "common/DSUtilLite/growarray.h",
    "common/genversion.bat",
    "common/includes/common_defines.h",
    "decoder/LAVAudio/AudioSettingsProp.cpp",
    "decoder/LAVAudio/AudioSettingsProp.h",
    "decoder/LAVAudio/LAVAudio.cpp",
    "decoder/LAVAudio/LAVAudio.h",
    "decoder/LAVAudio/LAVAudio.rc",
    "decoder/LAVAudio/LAVAudio.vcxproj",
    "decoder/LAVAudio/LAVAudio.vcxproj.filters",
    "decoder/LAVAudio/PostProcessor.cpp",
    "decoder/LAVAudio/dllmain.cpp",
    "decoder/LAVAudio/resource.h",
    "include/LAVAudioSettings.h",
}


class ReleasePackagingTests(unittest.TestCase):
    def test_canonical_release_version_is_v012(self) -> None:
        self.assertEqual(CANONICAL_RELEASE_VERSION, "0.12.0")

    def test_corresponding_source_tracks_every_new_openjoc_lav_file(self) -> None:
        self.assertEqual(set(LAV_NEW_FILES), EXPECTED_LAV_NEW_FILES)

    def test_corresponding_source_tracks_every_modified_upstream_lav_file(self) -> None:
        self.assertEqual(set(LAV_MODIFIED_FILES), EXPECTED_LAV_MODIFIED_FILES)

    @unittest.skipUnless(
        os.environ.get("OPENJOC_LAV_SOURCE_ROOT"),
        "set OPENJOC_LAV_SOURCE_ROOT to audit the public LAV fork",
    )
    def test_corresponding_source_overlay_covers_the_exact_lav_diff(self) -> None:
        lav = pathlib.Path(os.environ["OPENJOC_LAV_SOURCE_ROOT"]).resolve()
        completed = subprocess.run(
            [
                "git",
                "diff",
                "--name-status",
                "--ignore-submodules=all",
                "fefb6987994ed56e4525e8a125f5fbb53707bc52",
            ],
            cwd=lav,
            check=True,
            capture_output=True,
            text=True,
        )
        changed = {
            path
            for line in completed.stdout.splitlines()
            for _, path in [line.split("\t", maxsplit=1)]
            if not path.startswith("docs/openjoc/")
        }
        handled = (
            set(LAV_METADATA_FILES) | set(LAV_MODIFIED_FILES) | set(LAV_NEW_FILES)
        )
        self.assertEqual(handled, changed)

    def test_release_workflow_uses_version_only_public_title(self) -> None:
        workflow = (SCRIPTS.parent / ".github" / "workflows" / "release.yml").read_text(
            encoding="utf-8"
        )
        self.assertIn('--title "OpenJOC ${version}"', workflow)
        self.assertNotIn("Windows DirectShow / LAV Filters Integration", workflow)

    def test_packager_has_stage_and_finalize_commands(self) -> None:
        completed = subprocess.run(
            [sys.executable, str(SCRIPT), "--help"],
            check=True,
            capture_output=True,
            text=True,
        )

        self.assertIn("stage", completed.stdout)
        self.assertIn("finalize-source", completed.stdout)
        self.assertIn("finalize-binary", completed.stdout)

    def test_stage_accepts_canonical_windows_onboarding_template(self) -> None:
        completed = subprocess.run(
            [sys.executable, str(SCRIPT), "stage", "--help"],
            check=True,
            capture_output=True,
            text=True,
        )

        self.assertIn("--onboarding-template", completed.stdout)
        self.assertIn("--release-version", completed.stdout)

    def test_new_directory_guard_refuses_existing_target(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            existing = pathlib.Path(temporary)
            with self.assertRaises(FileExistsError):
                ensure_new_directory(existing)

    def test_text_writer_normalizes_newlines_on_python_39(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = pathlib.Path(temporary) / "nested" / "evidence.txt"

            _write_text(path, "first\r\nsecond\r\n")

            self.assertEqual(path.read_bytes(), b"first\nsecond\n")

    def test_binary_base_overlay_removes_competing_legacy_root_scripts(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            binary_base = root / "base"
            template = root / "template"
            destination = root / "destination"
            binary_base.mkdir()
            template.mkdir()
            for name in ("install.ps1", "verify.ps1", "uninstall.ps1"):
                (binary_base / name).write_text("legacy", encoding="utf-8")
            (binary_base / "runtime").mkdir()
            (template / "install.bat").write_text("launcher", encoding="utf-8")
            (template / "scripts").mkdir()
            (template / "scripts" / "install.ps1").write_text("canonical", encoding="utf-8")

            _prepare_binary_base(binary_base, template, destination)

            for name in ("install.ps1", "verify.ps1", "uninstall.ps1"):
                self.assertFalse((destination / name).exists(), name)
            self.assertEqual((destination / "scripts" / "install.ps1").read_text(), "canonical")

    def test_corresponding_source_includes_canonical_onboarding_template(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            template = root / "template"
            source = root / "source" / "OpenJOC"
            (template / "scripts").mkdir(parents=True)
            (template / "install.bat").write_text("launcher", encoding="utf-8")
            (template / "scripts" / "install.ps1").write_text("installer", encoding="utf-8")

            _copy_onboarding_source(template, source)

            copied = source / "packaging" / "windows-lav"
            self.assertEqual((copied / "install.bat").read_text(), "launcher")
            self.assertEqual((copied / "scripts" / "install.ps1").read_text(), "installer")

    def test_packager_contains_no_v010_identity_in_generated_metadata(self) -> None:
        text = SCRIPT.read_text(encoding="utf-8")
        self.assertNotIn("OPENJOC_LAV_0_10", text)
        self.assertNotIn("openjoc-lav-0.10.0", text)
        self.assertNotIn("openjoc-0.10.0", text)


if __name__ == "__main__":
    unittest.main()
