from __future__ import annotations

import os
import subprocess
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

    def test_unicode_windows_homes_in_binary_strings(self) -> None:
        for encoding in ("utf-8", "utf-16-le"):
            for prefix in (b"\x00\x01", b"\x00\x01\x02"):
                for username in ("测试", "élève", "🧪", "tester测试", "e\u0301tude"):
                    marker = "C:" + "\\Users\\" + username
                    for markers in ((), (marker,)):
                        with self.subTest(
                            encoding=encoding,
                            offset=len(prefix),
                            username=username,
                            explicit=bool(markers),
                        ), tempfile.TemporaryDirectory() as temporary:
                            path = Path(temporary) / "runtime.dll"
                            path.write_bytes(
                                prefix + (marker + "\\workspace\\source.rs").encode(encoding)
                                + b"\x00\x00"
                            )
                            findings = scan_paths((path,), private_path_markers=markers)
                            self.assertTrue(findings)
                            self.assertTrue(
                                all(item.category == "private_path" for item in findings)
                            )
                            indicator = (
                                "private_path_marker_1" if markers else "windows_user_home"
                            )
                            self.assertTrue(any(item.indicator == indicator for item in findings))
                            self.assertNotIn(username, repr(findings))
                            self.assertNotIn(marker, "\n".join(map(format_finding, findings)))

    def test_unicode_explicit_markers_outside_windows_homes(self) -> None:
        markers = ("/opt/私有构建/🧪", "D:" + "\\开发\\élève")
        for encoding in ("utf-8", "utf-16-le"):
            for marker in markers:
                with self.subTest(
                    encoding=encoding, marker=marker
                ), tempfile.TemporaryDirectory() as temporary:
                    path = Path(temporary) / "runtime.bin"
                    path.write_bytes(b"\x00" + marker.encode(encoding) + b"\x00\x00")
                    findings = scan_paths((path,), private_path_markers=(marker,))
                    self.assertTrue(findings)
                    self.assertTrue(
                        all(item.indicator == "private_path_marker_1" for item in findings)
                    )
                    self.assertNotIn(marker, repr(findings))

    def test_unicode_binary_strings_in_zip_entries(self) -> None:
        marker = "C:" + "\\Users\\测试🧪"
        for encoding in ("utf-8", "utf-16-le"):
            with self.subTest(encoding=encoding), tempfile.TemporaryDirectory() as temporary:
                path = Path(temporary) / "candidate.zip"
                with zipfile.ZipFile(path, "w") as archive:
                    archive.writestr(
                        "bin/runtime.dll",
                        b"\x00" + (marker + "\\source.rs").encode(encoding) + b"\x00\x00",
                    )
                findings = scan_paths((path,), private_path_markers=(marker,))
                self.assertTrue(findings)
                self.assertTrue(all(item.subject.endswith("!bin/runtime.dll") for item in findings))
                self.assertNotIn(marker, repr(findings))

    def test_unicode_binary_scan_cli_fails_without_disclosing_values(self) -> None:
        marker = "C:" + "\\Users\\测试🧪"
        for encoding in ("utf-8", "utf-16-le"):
            for explicit in (False, True):
                with self.subTest(
                    encoding=encoding, explicit=explicit
                ), tempfile.TemporaryDirectory() as temporary:
                    path = Path(temporary) / "runtime.dll"
                    path.write_bytes(b"\x00" + marker.encode(encoding) + b"\x00\x00")
                    arguments = [
                        sys.executable, str(SCRIPTS / "release_security.py"), "scan"
                    ]
                    if explicit:
                        arguments += ["--private-path-marker", marker]
                    result = subprocess.run(
                        arguments + [str(path)], capture_output=True, text=True
                    )
                    self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
                    self.assertIn("private_path", result.stdout)
                    self.assertIn("release security scan failed", result.stdout)
                    self.assertNotIn(marker, result.stdout + result.stderr)
                    self.assertNotIn("测试", result.stdout + result.stderr)

    def test_utf16_unicode_assignment_values_are_not_disclosed(self) -> None:
        name = "_".join(("EXAMPLE", "API", "KEY"))
        value = "合成测试值🧪"
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "settings.bin"
            path.write_bytes(
                b"\x00\x00" + f"{name}={value}".encode("utf-16-le") + b"\x00\x00"
            )
            findings = scan_paths((path,), private_path_markers=())
        self.assertTrue(
            any(item.category == "sensitive_assignment" for item in findings)
        )
        self.assertNotIn(value, repr(findings))
        self.assertNotIn(value, "\n".join(map(format_finding, findings)))

    def test_ascii_assignments_after_bom_or_unicode_remain_detected(self) -> None:
        name = "_".join(("EXAMPLE", "API", "KEY"))
        value = "synthetic-value"
        cases = (
            b"\xff\xfe" + f"{name}={value}".encode("utf-16-le"),
            b"\x00" + f"é{name}={value}".encode("utf-8"),
            f"\x00测{name}={value}".encode("utf-16-le"),
        )
        for index, data in enumerate(cases):
            with self.subTest(case=index), tempfile.TemporaryDirectory() as temporary:
                path = Path(temporary) / "settings.bin"
                path.write_bytes(data + b"\x00\x00")
                findings = scan_paths((path,), private_path_markers=())
                self.assertTrue(
                    any(item.category == "sensitive_assignment" for item in findings)
                )
                self.assertNotIn(value, repr(findings))

    def test_benign_unicode_binary_strings_are_not_findings(self) -> None:
        text = "C:" + "\\Program Files\\测试🧪\\runtime.dll"
        for encoding in ("utf-8", "utf-16-le"):
            with self.subTest(encoding=encoding), tempfile.TemporaryDirectory() as temporary:
                path = Path(temporary) / "runtime.dll"
                path.write_bytes(b"\x00\x01" + text.encode(encoding) + b"\x00\x00")
                self.assertEqual(scan_paths((path,), private_path_markers=()), ())

    def test_invalid_encoding_does_not_join_separate_path_fragments(self) -> None:
        marker = "C:" + "\\Users\\测试"
        for encoding, separator in (("utf-8", b"\xff"), ("utf-16-le", b"\x00\xd8")):
            with self.subTest(encoding=encoding), tempfile.TemporaryDirectory() as temporary:
                path = Path(temporary) / "runtime.dll"
                path.write_bytes(
                    b"\x00\x00" + marker[:5].encode(encoding) + separator
                    + marker[5:].encode(encoding) + b"\x00\x00"
                )
                self.assertEqual(scan_paths((path,), private_path_markers=(marker,)), ())

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
