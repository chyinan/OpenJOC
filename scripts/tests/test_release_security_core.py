from __future__ import annotations

import sys
import unittest
from pathlib import Path


SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))

from release_security_core import (  # noqa: E402
    build_release_environment,
    format_finding,
    scan_text,
)


class ReleaseEnvironmentTests(unittest.TestCase):
    def test_keeps_only_allowlisted_environment_variables(self) -> None:
        sensitive_name = "_".join(("EXAMPLE", "API", "KEY"))
        source = {
            "PATH": r"C:\Windows\System32",
            "SystemRoot": r"C:\Windows",
            sensitive_name: "synthetic-secret-value",
            "UNRELATED_APPLICATION_SETTING": "not-for-a-release-build",
        }

        result = build_release_environment(
            source,
            allowed_names={"PATH", "SystemRoot"},
            overrides={"OPENJOC_RELEASE_BUILD": "1"},
        )

        self.assertEqual(
            result,
            {
                "OPENJOC_RELEASE_BUILD": "1",
                "PATH": r"C:\Windows\System32",
                "SystemRoot": r"C:\Windows",
            },
        )

    def test_rejects_sensitive_override_names(self) -> None:
        sensitive_name = "_".join(("EXAMPLE", "CLIENT", "SECRET"))

        with self.assertRaisesRegex(ValueError, "sensitive environment variable"):
            build_release_environment(
                {},
                allowed_names=set(),
                overrides={sensitive_name: "synthetic-secret-value"},
            )


class ReleaseTextScanTests(unittest.TestCase):
    def test_source_code_token_variables_are_not_secret_assignments(self) -> None:
        findings = scan_text(
            "token = parser_next_token(context);\nsession = create_session();\n",
            subject="upstream/source.c",
            private_path_markers=(),
        )

        self.assertEqual(findings, ())

    def test_generic_unix_documentation_home_is_not_a_private_build_path(self) -> None:
        findings = scan_text(
            "Example: copy the sample to /home/user/project",
            subject="upstream/docs.md",
            private_path_markers=(),
        )

        self.assertEqual(findings, ())

    def test_reports_sensitive_assignment_without_returning_its_value(self) -> None:
        sensitive_name = "_".join(("EXAMPLE", "API", "KEY"))
        synthetic_value = "synthetic-secret-value-123456789"

        findings = scan_text(
            f"ordinary=true\n{sensitive_name}={synthetic_value}\n",
            subject="ffbuild/config.log",
            private_path_markers=(),
        )

        self.assertEqual(len(findings), 1)
        rendered = format_finding(findings[0])
        self.assertIn("sensitive_assignment", rendered)
        self.assertIn(sensitive_name, rendered)
        self.assertNotIn(synthetic_value, rendered)
        self.assertNotIn(synthetic_value, repr(findings[0]))

    def test_reports_known_sensitive_name_even_without_assignment(self) -> None:
        known_name = "_".join(("FIGMA", "API", "KEY"))

        findings = scan_text(
            f"export {known_name}\n",
            subject="generated.log",
            private_path_markers=(),
        )

        self.assertEqual([finding.category for finding in findings], ["known_name"])
        self.assertEqual(findings[0].indicator, known_name)

    def test_reports_private_path_without_copying_the_full_path(self) -> None:
        private_marker = "C:" + "\\" + "Users" + "\\" + "release-user"
        private_path = private_marker + "\\workspace\\source.rs"

        findings = scan_text(
            f"compiler_input={private_path}\n",
            subject="binary.strings",
            private_path_markers=(private_marker,),
        )

        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0].category, "private_path")
        self.assertEqual(findings[0].indicator, "private_path_marker_1")
        self.assertNotIn(private_path, format_finding(findings[0]))

    def test_ignores_empty_secret_like_assignment(self) -> None:
        sensitive_name = "_".join(("EXAMPLE", "TOKEN"))

        findings = scan_text(
            f"{sensitive_name}=\n",
            subject="generated.log",
            private_path_markers=(),
        )

        self.assertEqual(findings, ())

    def test_reports_credential_pattern_without_returning_token(self) -> None:
        synthetic_token = "gh" + "p_" + ("a" * 36)

        findings = scan_text(
            f"diagnostic={synthetic_token}\n",
            subject="generated.log",
            private_path_markers=(),
        )

        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0].category, "credential_pattern")
        self.assertNotIn(synthetic_token, format_finding(findings[0]))
        self.assertNotIn(synthetic_token, repr(findings[0]))

    def test_reports_windows_user_home_without_returning_username(self) -> None:
        private_path = "C:" + "\\" + "Users" + "\\" + "example-user" + "\\src"

        findings = scan_text(
            f"source={private_path}\n",
            subject="binary.strings",
            private_path_markers=(),
        )

        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0].category, "private_path")
        self.assertEqual(findings[0].indicator, "windows_user_home")
        self.assertNotIn("example-user", format_finding(findings[0]))


if __name__ == "__main__":
    unittest.main()
