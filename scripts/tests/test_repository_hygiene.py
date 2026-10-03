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

    def test_accepts_explicit_heading_anchor_ids(self) -> None:
        documents = {
            "README.md": "[SOFA scope](docs/SOFA.md#sofa-scope)\n",
            "docs/SOFA.md": "## SOFA scope {#sofa-scope}\n",
        }

        self.assertEqual(markdown_link_errors(documents, set(documents)), [])

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
                "The fixed DirectShow output contract covers exactly Stereo, Binaural, "
                "5.1, 7.1, 5.1.2, 5.1.4, "
                "7.1.2, and 7.1.4. AUTO_NOT_RELIABLE. No Bass Management. "
                "Automatic downstream layout discovery is AUTO_NOT_RELIABLE. "
                "Physical multichannel hardware is not verified.\n"
            ),
            "docs/README.md": "# OpenJOC documentation\n",
            "docs/integration/LAV_FILTERS_OPENJOC.md": (
                "The public DirectShow subset contains exactly eight explicit fixed "
                "48 kHz IEEE-float PCM policies:\n"
                "- Stereo (Speakers)\n- Binaural (Headphones)\n- 5.1\n- 7.1\n"
                "- 5.1.2\n- 5.1.4\n- 7.1.2\n- 7.1.4\n"
                "Each policy makes one exact semantic proposal. AUTO_NOT_RELIABLE. "
                "No Bass Management. Physical multichannel hardware is not verified.\n"
            ),
            "docs/site/using/windows-lav-potplayer.md": (
                "| Policy | Channels | Mask |\n| --- | ---: | --- |\n"
                "| Stereo (Speakers) | 2 | `0x00000003` |\n"
                "| Binaural (Headphones) | 2 | `0x00000003` |\n"
                "| 5.1 | 6 | `0x0000060f` |\n"
                "| 7.1 | 8 | `0x0000063f` |\n"
                "| 5.1.2 | 8 | `0x0000560f` |\n"
                "| 5.1.4 | 10 | `0x0002d60f` |\n"
                "| 7.1.2 | 10 | `0x0000563f` |\n"
                "| 7.1.4 | 12 | `0x0002d63f` |\n"
                "The built-in selector also offers SADIE II D2 / KEMAR.\n"
            ),
            "docs/site/using/windows-lav-potplayer.zh.md": (
                "| 策略 | 声道数 | Mask |\n| --- | ---: | --- |\n"
                "| Stereo (Speakers) | 2 | `0x00000003` |\n"
                "| Binaural (Headphones) | 2 | `0x00000003` |\n"
                "| 5.1 | 6 | `0x0000060f` |\n"
                "| 7.1 | 8 | `0x0000063f` |\n"
                "| 5.1.2 | 8 | `0x0000560f` |\n"
                "| 5.1.4 | 10 | `0x0002d60f` |\n"
                "| 7.1.2 | 10 | `0x0000563f` |\n"
                "| 7.1.4 | 12 | `0x0002d63f` |\n"
                "内置选择器还提供 SADIE II D2 / KEMAR。\n"
            ),
            "docs/site/project/capabilities.md": (
                "| Integration | Windows DirectShow / LAV Filters OpenJOC Audio Decoder | "
                "`ADMITTED_WITH_SCOPE` | Public fork; strict raw/MP4 DirectShow capture proves "
                "exact media types and sample delivery for Stereo, Binaural, 5.1, 7.1, "
                "5.1.2, 5.1.4, 7.1.2, and 7.1.4; endpoint probes preserve results | Scope |\n"
                "CLI uses --binaural-hrtf sadie-ii-d2-kemar.\n"
            ),
            "docs/site/project/capabilities.zh.md": (
                "| 集成 | Windows DirectShow / LAV Filters OpenJOC Audio Decoder | "
                "`ADMITTED_WITH_SCOPE` | 公开分支；严格的原始/MP4 DirectShow 捕获，证明 "
                "Stereo、Binaural、5.1、7.1、5.1.2、5.1.4、7.1.2 和 7.1.4 "
                "的精确媒体类型与采样传递；端点探测保留结果 | 范围 |\n"
                "CLI --binaural-hrtf sadie-ii-d2-kemar.\n"
            ),
            "docs/site/compatibility/known-limitations.md": (
                "Its fixed 48 kHz IEEE-float PCM policies are Stereo, Binaural "
                "(Headphones), 5.1, 7.1, 5.1.2, 5.1.4, 7.1.2, and 7.1.4. "
                "Each makes one exact semantic proposal.\n"
            ),
            "docs/site/compatibility/known-limitations.zh.md": (
                "它固定提供 48 kHz IEEE-float PCM 输出方案：Stereo、Binaural "
                "(Headphones)、5.1、7.1、5.1.2、5.1.4、7.1.2 和 7.1.4。"
                "每种方案只提出一种明确的 WAVEFORMATEXTENSIBLE 格式。\n"
            ),
            "docs/site/reference/cli-reference.md": (
                "Verified against OpenJOC v0.18.0; SHA256SUMS; bin/openjoc --help. "
                "--binaural-hrtf selects sadie-ii-d1-ku100 or sadie-ii-d2-kemar.\n"
            ),
            "docs/site/reference/cli-reference.zh.md": (
                "根据 OpenJOC v0.18.0 核对；SHA256SUMS；bin/openjoc --help。"
                "--binaural-hrtf 可选择 sadie-ii-d1-ku100 或 sadie-ii-d2-kemar。\n"
            ),
            "docs/site/using/binaural-sofa.md": (
                "Pass a built-in ID with --binaural-hrtf: sadie-ii-d1-ku100 or sadie-ii-d2-kemar.\n"
            ),
            "docs/site/using/binaural-sofa.zh.md": (
                "使用 --binaural-hrtf 选择内置 ID：sadie-ii-d1-ku100 或 sadie-ii-d2-kemar。\n"
            ),
        }

    def test_accepts_consistent_current_contracts(self) -> None:
        self.assertEqual(documentation_consistency_errors(self.fixture_files()), [])

    def test_rejects_lav_policy_count_that_disagrees_with_canonical_table(self) -> None:
        files = self.fixture_files()
        files["README.md"] += "The guide documents the seven fixed PCM policies.\n"
        files["docs/site/using/windows-lav-potplayer.md"] = (
            "The Windows adapter exposes exactly eight fixed PCM policies.\n"
            "| Policy | Channels | Mask |\n| --- | --- | --- |\n"
            + "".join(f"| {name} | 2 | `0x00000003` |\n" for name in (
                "Stereo", "Binaural (Headphones)", "5.1", "7.1", "5.1.2", "5.1.4", "7.1.2", "7.1.4"
            ))
            + "The built-in selector also offers SADIE II D2 / KEMAR.\n"
        )
        self.assertIn("README.md fixed PCM policy count disagrees with the canonical LAV table",
                      documentation_consistency_errors(files))
        files["README.md"] = files["README.md"].replace("seven fixed", "eight fixed")
        self.assertEqual(documentation_consistency_errors(files), [])

    def test_rejects_missing_binaural_from_current_public_policy_lists(self) -> None:
        for path in (
            "docs/KNOWN_LIMITATIONS.md",
            "docs/site/project/capabilities.md",
            "docs/site/project/capabilities.zh.md",
            "docs/site/compatibility/known-limitations.md",
            "docs/site/compatibility/known-limitations.zh.md",
            "docs/integration/LAV_FILTERS_OPENJOC.md",
        ):
            with self.subTest(path=path):
                files = self.fixture_files()
                # Preserve the list length to prove the check compares names, not counts.
                files[path] = files[path].replace("Binaural", "Stereo", 1)

                errors = documentation_consistency_errors(files)

                self.assertTrue(
                    any(
                        error.startswith(
                            f"{path} fixed DirectShow policy names disagree with the current contract"
                        )
                        and "missing: Binaural" in error
                        for error in errors
                    ),
                    errors,
                )

    def test_rejects_missing_binaural_from_canonical_lav_policy_tables(self) -> None:
        for path in (
            "docs/site/using/windows-lav-potplayer.md",
            "docs/site/using/windows-lav-potplayer.zh.md",
        ):
            with self.subTest(path=path):
                files = self.fixture_files()
                files[path] = files[path].replace(
                    "| Binaural (Headphones) |", "| Stereo (Speakers) |", 1
                )

                errors = documentation_consistency_errors(files)

                self.assertTrue(
                    any(
                        error.startswith(f"{path} fixed DirectShow policy names")
                        and "missing: Binaural" in error
                        for error in errors
                    ),
                    errors,
                )

    def test_rejects_missing_d2_cli_or_lav_selector_descriptions(self) -> None:
        selector_docs = (
            "docs/site/project/capabilities.md",
            "docs/site/project/capabilities.zh.md",
            "docs/site/reference/cli-reference.md",
            "docs/site/reference/cli-reference.zh.md",
            "docs/site/using/binaural-sofa.md",
            "docs/site/using/binaural-sofa.zh.md",
        )
        for path in selector_docs:
            with self.subTest(path=path):
                files = self.fixture_files()
                files[path] = files[path].replace("sadie-ii-d2-kemar", "")

                errors = documentation_consistency_errors(files)

                self.assertIn(
                    f"{path} omits the D2/KEMAR CLI HRTF selector",
                    errors,
                )

        for path in (
            "docs/site/using/windows-lav-potplayer.md",
            "docs/site/using/windows-lav-potplayer.zh.md",
        ):
            with self.subTest(path=path):
                files = self.fixture_files()
                files[path] = files[path].replace("SADIE II D2 / KEMAR", "")

                errors = documentation_consistency_errors(files)

                self.assertIn(
                    f"{path} omits the D2/KEMAR LAV HRTF selector",
                    errors,
                )

    def test_rejects_stale_cli_reference_help_audit(self) -> None:
        for path in (
            "docs/site/reference/cli-reference.md",
            "docs/site/reference/cli-reference.zh.md",
        ):
            with self.subTest(path=path):
                files = self.fixture_files()
                files[path] = files[path].replace("v0.18.0", "v0.17.0")

                errors = documentation_consistency_errors(files)

                self.assertIn(
                    f"{path} does not record the current v0.18.0 CLI help audit",
                    errors,
                )

    def test_ignores_historical_seven_policy_planning_and_evidence(self) -> None:
        files = self.fixture_files()
        seven_policy_record = (
            "At that time, the seven policies were Stereo, 5.1, 7.1, 5.1.2, "
            "5.1.4, 7.1.2, and 7.1.4.\n"
        )
        files[
            "planning/implementation-plans/2026-08-23-openjoc-lav-multichannel-output/phase_01.md"
        ] = seven_policy_record
        files[
            "docs/integration/evidence/windows-lav-multichannel-2026-08-25/OPENJOC_LAV_MULTICHANNEL_OUTPUT_RESULT.txt"
        ] = seven_policy_record

        self.assertEqual(documentation_consistency_errors(files), [])

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
