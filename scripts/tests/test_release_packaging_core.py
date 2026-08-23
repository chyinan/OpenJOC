# SPDX-FileCopyrightText: 2026 OpenJOC contributors
# SPDX-License-Identifier: Apache-2.0

# pattern: Functional Core

from __future__ import annotations

import pathlib
import sys
import tempfile
import unittest
import zipfile


SCRIPTS = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))

from release_packaging_core import (  # noqa: E402
    archive_files,
    classify_dependency,
    deterministic_zip,
    render_sha256_manifest,
)


class ReleasePackagingCoreTests(unittest.TestCase):
    def test_archive_files_are_sorted_and_exclude_release_forbidden_content(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            (root / "z.txt").write_text("z", encoding="utf-8")
            (root / "a").mkdir()
            (root / "a" / "b.txt").write_text("b", encoding="utf-8")
            (root / ".git").mkdir()
            (root / ".git" / "objects").write_text("forbidden", encoding="utf-8")
            (root / "build.obj").write_bytes(b"forbidden")
            (root / "symbols.pdb").write_bytes(b"forbidden")

            self.assertEqual(
                [path.as_posix() for path in archive_files(root)],
                ["a/b.txt", "z.txt"],
            )

    def test_ffmpeg_text_reference_outputs_are_source_not_private_media(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            reference = root / "LAVFilters-OpenJOC" / "ffmpeg" / "tests" / "ref" / "lavf"
            reference.mkdir(parents=True)
            (reference / "peak.wav").write_text("reference values", encoding="utf-8")
            (root / "private.wav").write_bytes(b"private media")

            files = [path.as_posix() for path in archive_files(root)]

            self.assertIn(
                "LAVFilters-OpenJOC/ffmpeg/tests/ref/lavf/peak.wav", files
            )
            self.assertNotIn("private.wav", files)

    def test_deterministic_zip_has_fixed_timestamps_and_sorted_entries(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary) / "root"
            root.mkdir()
            (root / "b.txt").write_text("b", encoding="utf-8")
            (root / "a.txt").write_text("a", encoding="utf-8")
            first = pathlib.Path(temporary) / "first.zip"
            second = pathlib.Path(temporary) / "second.zip"

            deterministic_zip(root, first)
            deterministic_zip(root, second)

            self.assertEqual(first.read_bytes(), second.read_bytes())
            with zipfile.ZipFile(first) as archive:
                self.assertEqual(archive.namelist(), ["a.txt", "b.txt"])
                self.assertTrue(
                    all(info.date_time == (2026, 8, 22, 0, 0, 0) for info in archive.infolist())
                )

    def test_hash_manifest_excludes_itself_and_uses_forward_slashes(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            (root / "runtime").mkdir()
            (root / "runtime" / "x.dll").write_bytes(b"x")
            (root / "SHA256SUMS.txt").write_text("old", encoding="utf-8")

            manifest = render_sha256_manifest(root, excluded={"SHA256SUMS.txt"})

            self.assertIn("  runtime/x.dll", manifest)
            self.assertNotIn("SHA256SUMS.txt", manifest)

    def test_dependency_classification_requires_local_or_known_os_dll(self) -> None:
        payload = {"local.dll"}

        self.assertEqual(classify_dependency("LOCAL.DLL", payload), "LOCAL")
        self.assertEqual(classify_dependency("kernel32.dll", payload), "OS")
        self.assertEqual(
            classify_dependency("api-ms-win-core-synch-l1-2-0.dll", payload),
            "OS",
        )
        self.assertEqual(classify_dependency("missing.dll", payload), "MISSING")


if __name__ == "__main__":
    unittest.main()
