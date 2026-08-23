# SPDX-FileCopyrightText: 2026 OpenJOC contributors
# SPDX-License-Identifier: Apache-2.0

# pattern: Imperative Shell

from __future__ import annotations

import pathlib
import subprocess
import sys
import tempfile
import unittest


SCRIPTS = pathlib.Path(__file__).resolve().parents[1]
SCRIPT = SCRIPTS / "release_packaging.py"
sys.path.insert(0, str(SCRIPTS))

from release_packaging import (  # noqa: E402
    _copy_onboarding_source,
    _prepare_binary_base,
    _write_text,
    ensure_new_directory,
)


class ReleasePackagingTests(unittest.TestCase):
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
