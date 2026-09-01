from __future__ import annotations

import json
import os
import subprocess
import unittest
from pathlib import Path
from xml.etree import ElementTree


NEW_LAV_FILES = (
    "decoder/LAVAudio/LAVOpenJocDiagnostics.h",
    "decoder/LAVAudio/AudioStatusCapacityTests.cpp",
    "decoder/LAVAudio/LAVAudioIdentitySmoke.cpp",
    "decoder/LAVAudio/LAVAudioResourceIdentitySmoke.cpp",
    "decoder/LAVAudio/OpenJocAdmission.cpp",
    "decoder/LAVAudio/OpenJocAdmission.h",
    "decoder/LAVAudio/OpenJocAdmissionTests.cpp",
    "decoder/LAVAudio/OpenJocCandidate.cpp",
    "decoder/LAVAudio/OpenJocCandidate.h",
    "decoder/LAVAudio/OpenJocDecoder.cpp",
    "decoder/LAVAudio/OpenJocDecoder.h",
    "decoder/LAVAudio/OpenJocDecoderSmoke.cpp",
    "decoder/LAVAudio/OpenJocDialnorm.h",
    "decoder/LAVAudio/OpenJocDialnormPolicyTests.cpp",
    "decoder/LAVAudio/OpenJocDirectShowNegotiationSmoke.cpp",
    "decoder/LAVAudio/OpenJocOutput.cpp",
    "decoder/LAVAudio/OpenJocOutput.h",
    "decoder/LAVAudio/OpenJocOutputTests.cpp",
    "decoder/LAVAudio/OpenJocPolicyControl.cpp",
    "decoder/LAVAudio/OpenJocPropertyPageSmoke.cpp",
    "decoder/LAVAudio/OpenJocSettingsSmoke.cpp",
    "decoder/LAVAudio/OpenJocShippedLayouts.cpp",
    "decoder/LAVAudio/OpenJocShippedLayouts.h",
    "decoder/LAVAudio/OpenJocShippedLayoutsTests.cpp",
    "decoder/LAVAudio/OpenJocStrictNegotiation.cpp",
    "decoder/LAVAudio/OpenJocStrictNegotiation.h",
    "decoder/LAVAudio/OpenJocStrictOutput.cpp",
    "decoder/LAVAudio/OpenJocStrictOutput.h",
    "decoder/LAVAudio/OpenJocStrictOutputTests.cpp",
    "include/LAVOpenJocSettings.h",
)
MODIFIED_UPSTREAM_FILES = (
    "common/DSUtilLite/growarray.h",
    "common/genversion.bat",
    "common/includes/common_defines.h",
    "decoder/LAVAudio/AudioSettingsProp.cpp",
    "decoder/LAVAudio/AudioSettingsProp.h",
    "decoder/LAVAudio/LAVAudio.cpp",
    "decoder/LAVAudio/LAVAudio.h",
    "decoder/LAVAudio/LAVAudio.rc",
    "decoder/LAVAudio/LAVAudio.vcxproj",
    "decoder/LAVAudio/LAVAudio.vcxproj.filters",
    "decoder/LAVAudio/PostProcessor.cpp",
    "decoder/LAVAudio/dllmain.cpp",
    "decoder/LAVAudio/resource.h",
    "include/LAVAudioSettings.h",
)
MODIFIED_UPSTREAM_GPL_NOTICE_FILES = tuple(
    relative
    for relative in MODIFIED_UPSTREAM_FILES
    if relative
    not in {
        "common/genversion.bat",
        "decoder/LAVAudio/LAVAudio.rc",
        "decoder/LAVAudio/LAVAudio.vcxproj",
        "decoder/LAVAudio/LAVAudio.vcxproj.filters",
        "decoder/LAVAudio/resource.h",
    }
)
V012_MODIFIED_UPSTREAM_FILES = {
    "common/DSUtilLite/growarray.h",
    "common/genversion.bat",
    "decoder/LAVAudio/AudioSettingsProp.h",
    "decoder/LAVAudio/resource.h",
}
V015_MODIFIED_UPSTREAM_FILES = {
    "decoder/LAVAudio/LAVAudio.rc",
    "decoder/LAVAudio/PostProcessor.cpp",
}


def lav_root() -> Path:
    configured = os.environ.get("OPENJOC_LAV_SOURCE_ROOT")
    if not configured:
        raise unittest.SkipTest("OPENJOC_LAV_SOURCE_ROOT is not configured")
    return Path(configured).resolve()


class LavReleaseNoticeTests(unittest.TestCase):
    def test_genversion_handles_hyphenated_downstream_tags(self) -> None:
        root = lav_root()
        script_path = root / "common" / "genversion.bat"
        script = script_path.read_text(encoding="utf-8")
        self.assertIn("git describe --tags --abbrev^=0 HEAD", script)
        self.assertIn("git rev-list --count", script)
        self.assertNotIn("delims=-", script)

        generated = root / "common" / "includes" / "version_rev.h"
        previous = generated.read_bytes() if generated.exists() else None
        try:
            completed = subprocess.run(
                ["cmd.exe", "/d", "/c", str(script_path)],
                cwd=root,
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(completed.returncode, 0, completed.stderr)
            self.assertEqual(completed.stderr.strip(), "")
            tag = subprocess.check_output(
                ["git", "describe", "--tags", "--abbrev=0", "HEAD"],
                cwd=root,
                text=True,
            ).strip()
            expected = subprocess.check_output(
                ["git", "rev-list", "--count", f"{tag}..HEAD"],
                cwd=root,
                text=True,
            ).strip()
            self.assertEqual(
                generated.read_text(encoding="ascii").strip(),
                f"#define LAV_VERSION_BUILD {expected}",
            )
        finally:
            if previous is None:
                generated.unlink(missing_ok=True)
            else:
                generated.write_bytes(previous)

    def test_new_files_have_openjoc_copyright_and_gpl2_or_later_spdx(self) -> None:
        root = lav_root()
        for relative in NEW_LAV_FILES:
            with self.subTest(path=relative):
                text = (root / relative).read_text(encoding="utf-8")
                header = "\n".join(text.splitlines()[:12])
                self.assertIn("SPDX-FileCopyrightText: 2026 OpenJOC contributors", header)
                self.assertIn("SPDX-License-Identifier: GPL-2.0-or-later", header)

    def test_modified_upstream_files_have_release_and_date_notice(self) -> None:
        root = lav_root()
        for relative in MODIFIED_UPSTREAM_FILES:
            with self.subTest(path=relative):
                text = (root / relative).read_text(encoding="utf-8")
                header = "\n".join(text.splitlines()[:45])
                self.assertIn("OpenJOC downstream modification", header)
                if relative in V015_MODIFIED_UPSTREAM_FILES:
                    release = "openjoc-0.15.0"
                    date = "2026-09-01"
                elif relative in V012_MODIFIED_UPSTREAM_FILES:
                    release = "openjoc-0.12.0"
                    date = "2026-08-25"
                else:
                    release = "openjoc-0.10.0"
                    date = "2026-08-22"
                self.assertIn(release, header)
                self.assertIn(date, header)

    def test_modified_source_files_retain_upstream_gpl_notice(self) -> None:
        root = lav_root()
        for relative in MODIFIED_UPSTREAM_GPL_NOTICE_FILES:
            with self.subTest(path=relative):
                text = (root / relative).read_text(encoding="utf-8")
                self.assertIn("either version 2 of the License", text)
                self.assertIn("(at your option) any later version", text)

    def test_provenance_documents_and_machine_readable_census_are_complete(self) -> None:
        root = lav_root()
        docs = root / "docs" / "openjoc"
        provenance = docs / "DIRECTSHOW_BASECLASSES_PROVENANCE.md"
        narrative_census = docs / "LAV_SOURCE_LICENSE_CENSUS.md"
        machine_census = docs / "LAV_SOURCE_LICENSE_CENSUS.json"
        for path in (provenance, narrative_census, machine_census):
            with self.subTest(path=path.name):
                self.assertTrue(path.is_file())

        for path in (provenance, narrative_census):
            with self.subTest(path=path.name):
                header = "\n".join(path.read_text(encoding="utf-8").splitlines()[:8])
                self.assertIn(
                    "SPDX-FileCopyrightText: 2026 OpenJOC contributors", header
                )
                self.assertIn("SPDX-License-Identifier: GPL-2.0-or-later", header)

        data = json.loads(machine_census.read_text(encoding="utf-8"))
        self.assertEqual(data["schema_version"], 1)
        self.assertEqual(data["copyright"], "2026 OpenJOC contributors")
        self.assertEqual(data["license"], "GPL-2.0-or-later")
        self.assertEqual(
            data["lav_upstream_revision"],
            "fefb6987994ed56e4525e8a125f5fbb53707bc52",
        )

        expected_units: set[str] = set()
        namespace = {"msbuild": "http://schemas.microsoft.com/developer/msbuild/2003"}
        projects = (
            "decoder/LAVAudio/LAVAudio.vcxproj",
            "common/DSUtilLite/DSUtilLite.vcxproj",
            "common/baseclasses/baseclasses.vcxproj",
        )
        for relative_project in projects:
            project_path = root / relative_project
            tree = ElementTree.parse(project_path)
            project_dir = Path(relative_project).parent
            for node in tree.findall(".//msbuild:ClCompile[@Include]", namespace):
                include = node.attrib["Include"].replace("\\", "/")
                expected_units.add((project_dir / include).as_posix())

        records = data["compiled_inputs"]
        actual_units = {record["source_path"] for record in records}
        self.assertEqual(actual_units, expected_units)
        self.assertEqual(len(records), 65)
        required_fields = {
            "source_path",
            "origin",
            "upstream_revision",
            "original_license_evidence",
            "lav_modification_state",
            "openjoc_modification_state",
            "resulting_classification",
            "confidence",
        }
        for record in records:
            with self.subTest(path=record["source_path"]):
                self.assertEqual(set(record), required_fields)
                self.assertNotIn(
                    record["resulting_classification"],
                    {"UNRESOLVED", "UNKNOWN", "NOASSERTION"},
                )

        by_path = {record["source_path"]: record for record in records}
        self.assertEqual(
            by_path["common/DSUtilLite/DSMResourceBag.cpp"][
                "resulting_classification"
            ],
            "GPL-2.0-or-later",
        )
        for relative in (
            "common/DSUtilLite/DeCSS/CSSauth.cpp",
            "common/DSUtilLite/DeCSS/CSSscramble.cpp",
        ):
            self.assertEqual(
                by_path[relative]["resulting_classification"],
                "GPL-3.0-only",
            )

        modified = {record["file"]: record for record in data["modified_upstream_files"]}
        self.assertEqual(set(modified), set(MODIFIED_UPSTREAM_FILES))
        self.assertTrue(all(item["modification_notice_present"] for item in modified.values()))


if __name__ == "__main__":
    unittest.main()
