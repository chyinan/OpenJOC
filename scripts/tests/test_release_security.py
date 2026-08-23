from __future__ import annotations

import os
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path


SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))

from release_security import run_sanitized_command, scan_paths  # noqa: E402
from release_security_core import format_finding  # noqa: E402


class ReleaseSecurityShellTests(unittest.TestCase):
    def test_scans_file_without_disclosing_sensitive_value(self) -> None:
        sensitive_name = "_".join(("EXAMPLE", "API", "KEY"))
        synthetic_value = "synthetic-secret-value-123456789"
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "config.log"
            path.write_text(
                f"{sensitive_name}={synthetic_value}\n", encoding="utf-8"
            )

            findings = scan_paths((path,), private_path_markers=())

        self.assertEqual(len(findings), 1)
        rendered = format_finding(findings[0])
        self.assertIn(sensitive_name, rendered)
        self.assertNotIn(synthetic_value, rendered)

    def test_scans_zip_entries_without_disclosing_sensitive_value(self) -> None:
        sensitive_name = "_".join(("EXAMPLE", "CLIENT", "SECRET"))
        synthetic_value = "synthetic-secret-value-987654321"
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "candidate.zip"
            with zipfile.ZipFile(path, "w") as archive:
                archive.writestr(
                    "source/ffbuild/config.log",
                    f"{sensitive_name}={synthetic_value}\n",
                )

            findings = scan_paths((path,), private_path_markers=())

        self.assertEqual(len(findings), 1)
        rendered = format_finding(findings[0])
        self.assertIn("source/ffbuild/config.log", rendered)
        self.assertNotIn(synthetic_value, rendered)

    def test_invalid_zip_is_a_non_disclosing_gate_failure_not_an_exception(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "candidate.zip"
            path.write_bytes(b"PK\x03\x04truncated")

            findings = scan_paths((path,), private_path_markers=())

        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0].category, "scan_error")
        rendered = format_finding(findings[0])
        self.assertIn("invalid_or_unreadable_zip", rendered)
        self.assertNotIn("truncated", rendered)

    def test_scans_utf16_binary_strings_for_private_marker(self) -> None:
        private_marker = "C:" + "\\" + "Users" + "\\" + "release-user"
        private_path = private_marker + "\\workspace\\source.rs"
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "runtime.dll"
            path.write_bytes(("prefix\x00" + private_path).encode("utf-16-le"))

            findings = scan_paths((path,), private_path_markers=(private_marker,))

        self.assertEqual(len(findings), 1)
        rendered = format_finding(findings[0])
        self.assertIn("private_path_marker_1", rendered)
        self.assertNotIn("release-user", rendered)

    def test_sanitized_child_does_not_inherit_sensitive_variable(self) -> None:
        sensitive_name = "_".join(("EXAMPLE", "API", "TOKEN"))
        previous = os.environ.get(sensitive_name)
        os.environ[sensitive_name] = "synthetic-secret-value"
        try:
            completed = run_sanitized_command(
                [
                    sys.executable,
                    "-c",
                    "import os,sys;print('present' if os.getenv(sys.argv[1]) else 'absent')",
                    sensitive_name,
                ],
                cwd=Path.cwd(),
                allowed_names={"PATH", "SystemRoot"},
                overrides={},
                capture_output=True,
            )
        finally:
            if previous is None:
                os.environ.pop(sensitive_name, None)
            else:
                os.environ[sensitive_name] = previous

        self.assertEqual(completed.stdout.strip(), "absent")


if __name__ == "__main__":
    unittest.main()
