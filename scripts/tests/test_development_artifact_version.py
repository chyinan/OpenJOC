# SPDX-FileCopyrightText: 2026 OpenJOC contributors
# SPDX-License-Identifier: Apache-2.0

from __future__ import annotations

import io
import json
import pathlib
import subprocess
import sys
import tarfile
import tempfile
import textwrap
import unittest
from unittest import mock


WORKSPACE = pathlib.Path(__file__).resolve().parents[2]
WORKFLOW = WORKSPACE / ".github" / "workflows" / "manual-development-artifacts.yml"
CARGO_METADATA = [
    "cargo", "metadata", "--format-version", "1", "--locked", "--no-deps",
]


def linux_audit_source() -> str:
    """Run the shipped heredoc, not a test-side copy of the audit logic."""
    workflow = WORKFLOW.read_text(encoding="utf-8")
    step = workflow.split("      - name: Audit the Linux artifact inventory\n", 1)[1]
    step = step.split("\n      - name:", 1)[0]
    script = step.split("<<'PY'\n", 1)[1].split("\n          PY", 1)[0]
    return textwrap.dedent(script)


class DevelopmentArtifactVersionTests(unittest.TestCase):
    def run_audit(self, version, metadata, names=None, metadata_error=None):
        if names is None:
            names = ["openjoc/bin/openjoc", "openjoc/README.md"]
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            archive = root / "development.tar.gz"
            manifest = root / "development.manifest.json"
            manifest.write_text(json.dumps({"version": version}), encoding="utf-8")
            with tarfile.open(archive, "w:gz") as handle:
                for name in names:
                    member = tarfile.TarInfo(name)
                    member.size = 4
                    handle.addfile(member, io.BytesIO(b"test"))
            # Mock only Cargo's process boundary, retaining real JSON and tar reads.
            with mock.patch.object(
                subprocess, "check_output",
                return_value=json.dumps(metadata), side_effect=metadata_error,
            ) as cargo, mock.patch.object(
                sys, "argv", ["-", str(archive), str(manifest)],
            ):
                exec(compile(linux_audit_source(), str(WORKFLOW), "exec"), {})
                cargo.assert_called_once_with(CARGO_METADATA, text=True)

    @staticmethod
    def metadata(version):
        return {"packages": [
            {"name": "other-package", "version": "9.9.9"},
            {"name": "openjoc-cli", "version": version},
        ]}

    def test_matching_source_versions_are_accepted(self):
        for version in ("0.17.0", "0.20.1", "1.2.3", "1.3.0-dev.1"):
            with self.subTest(version=version):
                self.run_audit(version, self.metadata(version))

    def test_manifest_version_must_match_cli_not_another_package(self):
        for version in ("0.17.0", "0.20.0", "9.9.9"):
            with self.subTest(version=version), self.assertRaises(AssertionError):
                self.run_audit(version, self.metadata("0.20.1"))

    def test_missing_cli_is_rejected(self):
        with self.assertRaises(StopIteration):
            self.run_audit("0.17.0", {"packages": [
                {"name": "other-package", "version": "0.17.0"},
            ]})

    def test_failed_cargo_metadata_is_rejected(self):
        failure = subprocess.CalledProcessError(1, CARGO_METADATA)
        with self.assertRaises(subprocess.CalledProcessError):
            self.run_audit("0.17.0", {}, metadata_error=failure)

    def test_missing_executable_is_rejected(self):
        with self.assertRaises(AssertionError):
            self.run_audit("0.20.1", self.metadata("0.20.1"), ["openjoc/README.md"])

    def test_target_and_debug_entries_are_rejected(self):
        for name in (
            "openjoc/target/release/library", "openjoc/debug/library",
            r"openjoc\\target\\release\\library", r"openjoc\\debug\\library",
        ):
            with self.subTest(name=name), self.assertRaises(AssertionError):
                self.run_audit("0.20.1", self.metadata("0.20.1"), [
                    "openjoc/bin/openjoc", name,
                ])


if __name__ == "__main__":
    unittest.main()
