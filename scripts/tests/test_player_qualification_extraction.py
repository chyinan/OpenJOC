from __future__ import annotations

import importlib.util
import io
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest
import zipfile


ROOT = Path(__file__).resolve().parents[2]
QUALIFIER_PATH = ROOT / "scripts" / "qualify-player-package.py"
SPEC = importlib.util.spec_from_file_location("player_qualification_extraction", QUALIFIER_PATH)
QUALIFIER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(QUALIFIER)


def add_file(archive: tarfile.TarFile, name: str, data: bytes, mode: int = 0o644) -> None:
    member = tarfile.TarInfo(name)
    member.size = len(data)
    member.mode = mode
    archive.addfile(member, io.BytesIO(data))


def add_link(archive: tarfile.TarFile, name: str, target: str, kind: bytes) -> None:
    member = tarfile.TarInfo(name)
    member.type = kind
    member.linkname = target
    archive.addfile(member)


class PlayerQualificationExtractionTests(unittest.TestCase):
    def test_external_links_reject_before_modifying_sentinel(self) -> None:
        for kind in (tarfile.SYMTYPE, tarfile.LNKTYPE):
            for absolute in (False, True):
                with self.subTest(kind=kind, absolute=absolute), tempfile.TemporaryDirectory() as name:
                    base = Path(name)
                    destination = base / "extracted"
                    destination.mkdir()
                    sentinel = base / "sentinel.txt"
                    sentinel.write_bytes(b"synthetic original\n")
                    archive_path = base / "candidate.tar.gz"
                    with tarfile.open(archive_path, "w:gz") as archive:
                        add_file(archive, "package/README.txt", b"synthetic package\n")
                        if kind == tarfile.SYMTYPE:
                            target = str(base) if absolute else "../.."
                            add_link(archive, "package/link", target, kind)
                            add_file(archive, "package/link/sentinel.txt", b"synthetic replacement\n")
                        else:
                            target = str(sentinel) if absolute else "../sentinel.txt"
                            add_link(archive, "package/sentinel.txt", target, kind)
                            add_file(archive, "package/sentinel.txt", b"synthetic replacement\n")
                    with self.assertRaises(tarfile.FilterError):
                        QUALIFIER.safe_extract(archive_path, destination)
                    self.assertEqual(sentinel.read_bytes(), b"synthetic original\n")

    def test_link_chain_cannot_escape_destination(self) -> None:
        with tempfile.TemporaryDirectory() as name:
            base = Path(name)
            destination = base / "extracted"
            destination.mkdir()
            sentinel = base / "sentinel.txt"
            sentinel.write_bytes(b"synthetic original\n")
            archive_path = base / "candidate.tar.gz"
            with tarfile.open(archive_path, "w:gz") as archive:
                add_file(archive, "package/README.txt", b"synthetic package\n")
                add_link(archive, "package/first", "..", tarfile.SYMTYPE)
                add_link(archive, "package/first/second", "..", tarfile.SYMTYPE)
                add_file(archive, "package/first/second/sentinel.txt", b"synthetic replacement\n")
            # The first, in-destination symlink needs creation privileges on Windows.
            if os.name == "nt":
                self.skipTest("symlink-chain creation requires Windows symlink privileges")
            with self.assertRaises(tarfile.FilterError):
                QUALIFIER.safe_extract(archive_path, destination)
            self.assertEqual(sentinel.read_bytes(), b"synthetic original\n")

    def test_special_files_are_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as name:
            base = Path(name)
            destination = base / "extracted"
            destination.mkdir()
            archive_path = base / "candidate.tar.gz"
            with tarfile.open(archive_path, "w:gz") as archive:
                member = tarfile.TarInfo("package/pipe")
                member.type = tarfile.FIFOTYPE
                archive.addfile(member)
            with self.assertRaises(tarfile.SpecialFileError):
                QUALIFIER.safe_extract(archive_path, destination)
            self.assertFalse((destination / "package/pipe").exists())

    def test_normal_tar_and_zip_payloads_keep_exact_bytes(self) -> None:
        payloads = {"bin/mpv": b"#!/bin/sh\nexit 0\n", "docs/日本語.txt": b"synthetic\x00payload\n"}
        for suffix in (".tar.gz", ".zip"):
            with self.subTest(suffix=suffix), tempfile.TemporaryDirectory() as name:
                base = Path(name)
                destination = base / "extracted with spaces 日本語"
                destination.mkdir()
                archive_path = base / ("candidate" + suffix)
                if suffix == ".zip":
                    with zipfile.ZipFile(archive_path, "w") as archive:
                        for relative, data in payloads.items():
                            archive.writestr("package with spaces/" + relative, data)
                else:
                    with tarfile.open(archive_path, "w:gz") as archive:
                        for relative, data in payloads.items():
                            add_file(archive, "package with spaces/" + relative, data, 0o755 if relative.startswith("bin/") else 0o644)
                root = QUALIFIER.safe_extract(archive_path, destination)
                self.assertEqual(root, destination / "package with spaces")
                self.assertEqual({p.relative_to(root).as_posix(): p.read_bytes() for p in root.rglob("*") if p.is_file()}, payloads)
                if suffix == ".tar.gz" and os.name != "nt":
                    self.assertEqual((root / "bin/mpv").stat().st_mode & 0o777, 0o755)

    @unittest.skipIf(os.name == "nt", "internal symlink creation requires Windows symlink privileges")
    def test_internal_symlinks_and_hardlinks_remain_supported(self) -> None:
        with tempfile.TemporaryDirectory() as name:
            base = Path(name)
            destination = base / "extracted"
            destination.mkdir()
            archive_path = base / "candidate.tar.gz"
            with tarfile.open(archive_path, "w:gz") as archive:
                add_file(archive, "package/lib/real.dylib", b"synthetic library\n")
                add_link(archive, "package/lib/alias.dylib", "real.dylib", tarfile.SYMTYPE)
                add_link(archive, "package/lib/hard.dylib", "package/lib/real.dylib", tarfile.LNKTYPE)
            root = QUALIFIER.safe_extract(archive_path, destination)
            self.assertTrue((root / "lib/alias.dylib").is_symlink())
            for filename in ("real.dylib", "alias.dylib", "hard.dylib"):
                self.assertEqual((root / "lib" / filename).read_bytes(), b"synthetic library\n")

    def test_unsafe_member_names_still_reject_before_extraction(self) -> None:
        for suffix in (".tar.gz", ".zip"):
            for member_name in ("../outside.txt", "/absolute.txt", "package/../outside.txt"):
                with self.subTest(suffix=suffix, name=member_name), tempfile.TemporaryDirectory() as name:
                    base = Path(name)
                    destination = base / "extracted"
                    destination.mkdir()
                    archive_path = base / ("candidate" + suffix)
                    if suffix == ".zip":
                        with zipfile.ZipFile(archive_path, "w") as archive:
                            archive.writestr(member_name, b"synthetic\n")
                    else:
                        with tarfile.open(archive_path, "w:gz") as archive:
                            add_file(archive, member_name, b"synthetic\n")
                    with self.assertRaisesRegex(SystemExit, "unsafe path"):
                        QUALIFIER.safe_extract(archive_path, destination)
                    self.assertEqual(list(destination.iterdir()), [])

    def test_cli_rejects_external_link_before_runtime_or_report(self) -> None:
        with tempfile.TemporaryDirectory() as name:
            base = Path(name)
            fixtures = base / "fixtures"
            fixtures.mkdir()
            sentinel = base / "sentinel.txt"
            sentinel.write_bytes(b"synthetic original\n")
            archive_path = base / "candidate.tar.gz"
            report = base / "report.json"
            with tarfile.open(archive_path, "w:gz") as archive:
                add_link(archive, "package/link", str(base), tarfile.SYMTYPE)
                add_file(archive, "package/link/sentinel.txt", b"synthetic replacement\n")
            result = subprocess.run(
                [sys.executable, str(QUALIFIER_PATH), "--archive", str(archive_path),
                 "--platform", "linux-x86_64", "--fixtures", str(fixtures), "--report", str(report)],
                capture_output=True, text=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertRegex(result.stderr, r"tarfile\.(?:AbsoluteLinkError|LinkOutsideDestinationError)")
            self.assertEqual(sentinel.read_bytes(), b"synthetic original\n")
            self.assertFalse(report.exists())
            self.assertNotIn("package verification:", result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
