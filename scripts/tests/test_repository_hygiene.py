import unittest

from scripts.repository_hygiene_core import (
    documentation_consistency_errors,
    markdown_link_errors,
    root_artifact_errors,
)


class RootArtifactTests(unittest.TestCase):
    def test_rejects_development_reports_only_at_root(self) -> None:
        errors = root_artifact_errors(
            {
                "README.md",
                "OPENJOC_RENDER_HANDOFF.md",
                "OPENJOC_WINDOWS_BASELINE.md",
                "PROGRESS-docs.md",
                "FINAL_AUDIT.md",
                "docs/archive/OPENJOC_RENDER_HANDOFF.md",
            }
        )

        self.assertEqual(
            errors,
            [
                "root development artifact is tracked: FINAL_AUDIT.md",
                "root development artifact is tracked: OPENJOC_RENDER_HANDOFF.md",
                "root development artifact is tracked: OPENJOC_WINDOWS_BASELINE.md",
                "root development artifact is tracked: PROGRESS-docs.md",
            ],
        )


class MarkdownLinkTests(unittest.TestCase):
    def test_accepts_relative_files_images_directories_and_anchors(self) -> None:
        documents = {
            "README.md": (
                '# Home\n[Docs](docs/)\n<img src="docs/header.png" alt="Header">\n'
            ),
            "docs/README.md": "# Documentation Index\n[Limits](LIMITS.md#known-limits)\n",
            "docs/LIMITS.md": "# Known limits\n",
        }
        tracked = set(documents) | {"docs/header.png"}

        self.assertEqual(markdown_link_errors(documents, tracked), [])

    def test_reports_missing_files_and_anchors(self) -> None:
        documents = {
            "README.md": "[Missing](docs/NOPE.md)\n[Bad anchor](docs/OK.md#not-there)\n",
            "docs/OK.md": "# Present\n",
        }

        errors = markdown_link_errors(documents, set(documents))

        self.assertEqual(
            errors,
            [
                "README.md: missing local link target: docs/NOPE.md",
                "README.md: missing Markdown anchor #not-there in docs/OK.md",
            ],
        )

    def test_reports_missing_html_targets_and_unclosed_fences(self) -> None:
        documents = {
            "README.md": '<a href="docs/NOPE.md">Missing</a>\n```text\n',
        }

        errors = markdown_link_errors(documents, set(documents))

        self.assertEqual(
            errors,
            [
                "README.md: unclosed Markdown fence",
                "README.md: missing local link target: docs/NOPE.md",
            ],
        )


class DocumentationConsistencyTests(unittest.TestCase):
    def fixture_files(self) -> dict[str, str]:
        return {
            "Cargo.toml": '[workspace.package]\nversion = "0.12.0"\nrust-version = "1.85"\n',
            "README.md": (
                "# OpenJOC\n[Latest](https://github.com/chyinan/OpenJOC/releases/latest)\n"
                "install.bat --layout-file C ABI 64 output channels\n"
            ),
            "CHANGELOG.md": "# Changelog\n## [0.12.0]\n",
            "crates/openjoc-capi/include/openjoc.h": (
                "#define OPENJOC_ABI_VERSION_MAJOR 1u\n"
                "#define OPENJOC_ABI_VERSION_MINOR 4u\n"
            ),
            "crates/openjoc-scene/src/speaker_layouts.rs": (
                "pub const MAX_CUSTOM_SPEAKERS: usize = 64;\n"
            ),
            "packaging/player/PLAYER_PACKAGE_MANIFEST.json": (
                '{"openjoc":{"version":"0.12.0","c_abi":{"major":1,"minor":4}}}'
            ),
            "docs/C_API.md": "The ABI is `1.4-experimental`.\n",
            "docs/CAPABILITIES.md": (
                "Versioned C ABI 1.4; custom geometry up to 64 output channels.\n"
                "openjoc render-joc FILE [--layout LAYOUT | --layout-file LAYOUT.json]\n"
            ),
            "docs/CUSTOM_SPEAKER_LAYOUTS.md": "admits up to 64 output channels\n",
            "docs/KNOWN_LIMITATIONS.md": (
                "DirectShow fixed policies: Stereo, 5.1, 7.1, 5.1.2, 5.1.4, "
                "7.1.2, and 7.1.4. AUTO_NOT_RELIABLE. No Bass Management. "
                "Physical multichannel hardware is not verified.\n"
            ),
            "docs/README.md": "# OpenJOC documentation\n",
            "docs/integration/LAV_FILTERS_OPENJOC.md": (
                "DirectShow fixed policies: Stereo, 5.1, 7.1, 5.1.2, 5.1.4, "
                "7.1.2, and 7.1.4. AUTO_NOT_RELIABLE. No Bass Management. "
                "Physical multichannel hardware is not verified.\n"
            ),
        }

    def test_accepts_consistent_current_contracts(self) -> None:
        self.assertEqual(documentation_consistency_errors(self.fixture_files()), [])

    def test_rejects_missing_directshow_layout_or_evidence_boundaries(self) -> None:
        files = self.fixture_files()
        files["docs/KNOWN_LIMITATIONS.md"] = "DirectShow supports Stereo and 5.1.\n"

        errors = documentation_consistency_errors(files)

        self.assertIn(
            "docs/KNOWN_LIMITATIONS.md does not preserve the fixed DirectShow layout contract",
            errors,
        )
        self.assertIn(
            "docs/KNOWN_LIMITATIONS.md does not preserve AUTO_NOT_RELIABLE",
            errors,
        )
        self.assertIn(
            "docs/KNOWN_LIMITATIONS.md does not preserve the physical hardware boundary",
            errors,
        )


    def test_rejects_readme_release_version_pins(self) -> None:
        files = self.fixture_files()
        files["README.md"] += (
            "Release 0.12 requires Rust 1.85 and C ABI 1.4.\n"
            "[Pinned](https://github.com/chyinan/OpenJOC/releases/download/v0.11.0/a.zip)\n"
        )

        errors = documentation_consistency_errors(files)

        self.assertIn("README.md pins a product release version", errors)
        self.assertIn("README.md pins a Rust toolchain version", errors)
        self.assertIn("README.md pins a C ABI version", errors)
        self.assertIn("README.md links to a version-pinned release", errors)

    def test_rejects_stale_current_framing_and_missing_layout_file_synopsis(self) -> None:
        files = self.fixture_files()
        files["docs/CAPABILITIES.md"] = (
            "Versioned C ABI 1.4; custom geometry up to 64 output channels.\n"
        )
        files["docs/PUBLIC_SMOKE_FIXTURE.md"] = "current 0.9 development commit\n"
        files["docs/JOC_SPATIAL_BRIDGE.md"] = "The 0.9.1 `render-joc` command\n"
        files["docs/ADM_EXPORT.md"] = "| Semantic | Status | 0.9 treatment |\n"

        errors = documentation_consistency_errors(files)

        self.assertIn(
            "docs/CAPABILITIES.md omits --layout-file from the canonical CLI synopsis",
            errors,
        )
        for phrase in (
            "current 0.9 development commit",
            "The 0.9.1 `render-joc` command",
            "| Semantic | Status | 0.9 treatment |",
        ):
            self.assertIn(f"current documentation retains stale claim: {phrase}", errors)

    def test_reports_mismatched_abi_and_retired_future_document(self) -> None:
        files = self.fixture_files()
        files["docs/C_API.md"] = "The ABI is `1.3-experimental`.\n"
        files["docs/FUTURE_PLAYER_ADAPTERS.md"] = "DirectShow COM filter still required\n"

        errors = documentation_consistency_errors(files)

        self.assertIn("docs/C_API.md does not document current C ABI 1.4", errors)
        self.assertIn(
            "retired stale document still exists: docs/FUTURE_PLAYER_ADAPTERS.md",
            errors,
        )


if __name__ == "__main__":
    unittest.main()
