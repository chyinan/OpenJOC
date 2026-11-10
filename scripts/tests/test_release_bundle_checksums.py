from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
REQUIRES_SHELL = os.name != "nt" and shutil.which("sh") and shutil.which("shasum")


@unittest.skipUnless(REQUIRES_SHELL, "requires POSIX shell and shasum")
class ReleaseBundleChecksumTests(unittest.TestCase):
    """Run the real verifier; only host identity and the CLI are test doubles."""

    def setUp(self) -> None:
        temporary = tempfile.TemporaryDirectory(prefix="OpenJOC checksum ")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name) / "bundle with spaces"
        self.root.mkdir()
        self.calls = Path(temporary.name) / "cli-calls.txt"
        for name in (
            "LICENSE", "README.md", "docs/CAPABILITIES.md", "docs/KNOWN_LIMITATIONS.md",
            "docs/README.md", "docs/extra guide.md", "include/openjoc.h",
            "lib/libopenjoc_capi.a", "lib/libopenjoc_capi.dylib",
        ):
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(f"synthetic payload: {name}\n", encoding="utf-8")
        executable = self.root / "bin/openjoc"
        executable.parent.mkdir()
        executable.write_text(
            '#!/bin/sh\nprintf "%s\\n" "$*" >> "$OPENJOC_TEST_CLI_CALLS"\n'
            'printf "OpenJOC 0.20.1\\n"\n', encoding="utf-8"
        )
        executable.chmod(0o755)
        shutil.copyfile(ROOT / "scripts/verify-release-bundle.sh", self.root / "verify.sh")
        self.records = [
            {"path": path.relative_to(self.root).as_posix(),
             "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
             "size": path.stat().st_size}
            for path in sorted(self.root.rglob("*")) if path.is_file()
        ]
        manifest = {
            "schema": "openjoc.bundle-manifest.v1", "version": "0.20.1",
            "target": "aarch64-apple-darwin", "notarized": False,
            "signing": {"developer_identity_signed": False, "linker_adhoc_signed": True},
            "capability_contract": "docs/CAPABILITIES.md",
            "known_limitations": "docs/KNOWN_LIMITATIONS.md", "files": self.records,
        }
        (self.root / "RELEASE_MANIFEST.json").write_text(
            json.dumps(manifest, indent=2) + "\n", encoding="utf-8"
        )
        self.write_checksums(self.records)
        tools = Path(temporary.name) / "tools"
        tools.mkdir()
        uname = tools / "uname"
        uname.write_text(
            '#!/bin/sh\ncase "$1" in -s) echo Darwin;; -m) echo arm64;; *) exit 2;; esac\n',
            encoding="utf-8",
        )
        uname.chmod(0o755)
        self.environment = {
            **os.environ, "PATH": str(tools) + os.pathsep + os.environ.get("PATH", ""),
            "OPENJOC_TEST_CLI_CALLS": str(self.calls),
        }

    def write_checksums(self, records: list[dict[str, object]], marker: str = " ") -> None:
        (self.root / "SHA256SUMS").write_text(
            "".join(f'{item["sha256"]} {marker}{item["path"]}\n' for item in records),
            encoding="utf-8",
        )

    def run_verifier(self) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            ["sh", str(self.root / "verify.sh")], cwd=self.root.parent,
            env=self.environment, text=True, capture_output=True, check=False,
        )

    def assert_rejected_before_cli(self) -> None:
        result = self.run_verifier()
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertFalse(self.calls.exists(), "unverified payload must not execute")

    def test_complete_checksums_pass_in_any_order_with_both_markers(self) -> None:
        for marker in (" ", "*"):
            with self.subTest(marker=marker):
                self.write_checksums(list(reversed(self.records)), marker)
                result = self.run_verifier()
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertIn("verification: PASS", result.stdout)
        self.assertEqual(len(self.calls.read_text().splitlines()), 16)

    def test_missing_checksum_for_each_payload_is_rejected(self) -> None:
        for omitted in self.records:
            with self.subTest(path=omitted["path"]):
                self.write_checksums([item for item in self.records if item is not omitted])
                if self.calls.exists():
                    self.calls.unlink()
                self.assert_rejected_before_cli()

    def test_omitted_library_checksum_cannot_hide_changed_bytes(self) -> None:
        omitted = "lib/libopenjoc_capi.dylib"
        self.write_checksums([item for item in self.records if item["path"] != omitted])
        (self.root / omitted).write_bytes(b"changed library not covered by SHA256SUMS\n")
        self.assert_rejected_before_cli()

    def test_duplicate_checksum_is_rejected(self) -> None:
        self.write_checksums(self.records + [self.records[0]])
        self.assert_rejected_before_cli()

    def test_changed_covered_payload_is_still_rejected(self) -> None:
        (self.root / "lib/libopenjoc_capi.a").write_bytes(b"changed bytes\n")
        self.assert_rejected_before_cli()

    def test_unexpected_or_missing_file_is_still_rejected(self) -> None:
        extra = self.root / "unexpected.bin"
        extra.write_bytes(b"extra")
        self.assert_rejected_before_cli()
        extra.unlink()
        (self.root / "include/openjoc.h").unlink()
        self.assert_rejected_before_cli()

    def test_empty_or_malformed_checksum_list_is_rejected(self) -> None:
        for content in ("", "not a checksum\n", "# ignored comment\n", "0" * 64 + " README.md\n"):
            with self.subTest(content=content):
                (self.root / "SHA256SUMS").write_text(content, encoding="utf-8")
                self.assert_rejected_before_cli()


if __name__ == "__main__":
    unittest.main()
