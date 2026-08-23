from __future__ import annotations

# pattern: Imperative Shell

import hashlib
import os
import subprocess
import unittest
from pathlib import Path


SOURCE_NAME = "mingw-w64-gcc-16.2.0-3.src.tar.zst"
BINARY_NAME = "mingw-w64-x86_64-gcc-libs-16.2.0-3-any.pkg.tar.zst"
SOURCE_SHA256 = "eb3479a8b0b23810fbbbc25ef76879e867e88d09960a40145d73f5505fda4da0"
BINARY_SHA256 = "f8e25ea67bb796e7f65550f0dca9fce4cdde8aaa3dadafe4d13c6a8233c8de26"
RUNTIME_SHA256 = "b37c1770c8ca092700875845b34918803ee6311573eba1c32ff4b1166e4a0e1e"


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


class GccRuntimeSourceTests(unittest.TestCase):
    def test_exact_official_source_and_binary_package_metadata(self) -> None:
        root_value = os.environ.get("OPENJOC_GCC_EVIDENCE_ROOT")
        if not root_value:
            raise unittest.SkipTest("OPENJOC_GCC_EVIDENCE_ROOT is not configured")
        root = Path(root_value).resolve()
        source = root / SOURCE_NAME
        binary = root / BINARY_NAME
        self.assertEqual(sha256(source), SOURCE_SHA256)
        self.assertEqual(sha256(binary), BINARY_SHA256)

        tar = Path(os.environ.get("OPENJOC_TAR", r"C:\Windows\System32\tar.exe"))
        source_info = subprocess.run(
            [str(tar), "-xOf", str(source), "mingw-w64-gcc/.SRCINFO"],
            check=True,
            capture_output=True,
            text=True,
            encoding="utf-8",
        ).stdout
        self.assertIn("pkgver = 16.2.0", source_info)
        self.assertIn("pkgrel = 3", source_info)
        self.assertIn("source = https://ftp.gnu.org/gnu/gcc/gcc-16.2.0/", source_info)
        self.assertIn(
            "sha256sums = e6738e29597f733270731aa90600f37ffdc045079dfc27ec7e8192cc81085c3e",
            source_info,
        )

        package_info = subprocess.run(
            [str(tar), "-xOf", str(binary), ".PKGINFO"],
            check=True,
            capture_output=True,
            text=True,
            encoding="utf-8",
        ).stdout
        self.assertIn("pkgname = mingw-w64-x86_64-gcc-libs", package_info)
        self.assertIn("pkgbase = mingw-w64-gcc", package_info)
        self.assertIn("pkgver = 16.2.0-3", package_info)
        self.assertIn("GCC-exception-3.1", package_info)

    def test_binary_package_runtime_matches_release_runtime(self) -> None:
        root_value = os.environ.get("OPENJOC_GCC_EVIDENCE_ROOT")
        runtime_value = os.environ.get("OPENJOC_RELEASE_LIBGCC")
        if not root_value or not runtime_value:
            raise unittest.SkipTest("GCC evidence and release runtime paths are required")
        packaged_runtime = (
            Path(root_value)
            / "binary-package-extract"
            / "mingw64"
            / "bin"
            / "libgcc_s_seh-1.dll"
        )
        release_runtime = Path(runtime_value)
        self.assertEqual(sha256(packaged_runtime), RUNTIME_SHA256)
        self.assertEqual(sha256(release_runtime), RUNTIME_SHA256)

    def test_release_provenance_document_records_hashes_and_licenses(self) -> None:
        document = Path(__file__).resolve().parents[2] / "docs" / "release" / "GCC_RUNTIME_SOURCE.md"
        text = document.read_text(encoding="utf-8")
        for required in (
            SOURCE_NAME,
            SOURCE_SHA256.upper(),
            BINARY_NAME,
            BINARY_SHA256.upper(),
            RUNTIME_SHA256.upper(),
            "COPYING3",
            "COPYING.LIB",
            "COPYING.RUNTIME",
            "GCC Runtime Library Exception 3.1",
        ):
            self.assertIn(required, text)


if __name__ == "__main__":
    unittest.main()
