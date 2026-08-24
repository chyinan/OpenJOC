# SPDX-FileCopyrightText: 2026 OpenJOC contributors
# SPDX-License-Identifier: Apache-2.0

# pattern: Functional Core

from __future__ import annotations

import pathlib
import subprocess
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "test_lav_directshow_negotiation.cmd"
FIXTURE_SCRIPT = ROOT / "scripts" / "generate-player-fixtures.sh"
RUST_BRIDGE = ROOT / "crates" / "openjoc-ffmpeg" / "src" / "lib.rs"
LAV_ROOT = pathlib.Path(r"D:\Program\LAVFilters-OpenJOC")
HARNESS = LAV_ROOT / "decoder" / "LAVAudio" / "OpenJocDirectShowNegotiationSmoke.cpp"


class LavDirectShowNegotiationScriptTests(unittest.TestCase):
    def test_declares_exact_seven_argument_contract_and_frozen_pristine_identity(self) -> None:
        text = SCRIPT.read_text(encoding="utf-8")

        self.assertIn("SPDX-License-Identifier: GPL-2.0-or-later", text)
        self.assertIn("pattern: Imperative Shell", text)
        self.assertIn(
            "VSDEVCMD TARGET_LAV_ROOT PRISTINE_LAV_ROOT OPENJOC_INCLUDE "
            "OPENJOC_CAPI FIXTURE_DIR OUTPUT_DIR",
            text,
        )
        self.assertIn("if not \"%~8\"==\"\"", text)
        self.assertIn("if \"%~7\"==\"\"", text)
        self.assertIn("exit /b 64", text)
        self.assertIn("b06ba2cbbd5c8806ca4423a8ff1527e4e2bd6a27", text)
        self.assertIn("b39333900119799887bd84f21510d2179906826b", text)
        self.assertIn("rev-parse HEAD", text)
        self.assertIn("rev-parse HEAD:", text)
        self.assertIn("OPENJOC_PRISTINE_ARCHIVE_PROVENANCE.txt", text)
        self.assertIn(
            "5C24633B1DC5DD18AA07529AD73CDBCE9BB10F55AA3E39AA17027AB85C114B0E",
            text,
        )
        self.assertIn(
            "77824565B23684D5FE3DA7EA7A5081D58C89AF11DD7B01DB769A2765EE1F7C7A",
            text,
        )
        self.assertIn(
            "CDBD55F80C06F3C7E44C261DB47ECFBAC2B0A2EB5BC4C2696D00397F6E941D12",
            text,
        )
        self.assertIn(
            "420A3962D283B23D10BA486E7A3AF2FC57C46C1E22116FF5AF6DF935651A6B89",
            text,
        )
        self.assertIn('for /f "usebackq tokens=1,2,*"', text)
        self.assertNotIn("findstr", text.lower())
        self.assertIn("where.exe git.exe", text)
        self.assertIn('"%PROVENANCE_GIT%" -C', text)
        self.assertIn("diff --quiet --ignore-submodules=dirty --", text)
        self.assertIn(
            "diff --cached --quiet --ignore-submodules=dirty HEAD --", text
        )
        self.assertIn("ls-files --others --exclude-standard", text)
        self.assertIn("OPENJOC_PRISTINE_ARCHIVE_PROVENANCE.txt", text)
        self.assertIn("PRISTINE_UNTRACKED_COUNT", text)
        self.assertIn("NOGIT_DIR", text)
        self.assertIn('set "PATH=%NOGIT_DIR%;%PATH%"', text)

    def test_keeps_target_and_pristine_build_and_runtime_paths_disjoint(self) -> None:
        text = SCRIPT.read_text(encoding="utf-8")

        self.assertIn("TARGET_BUILD_DIR", text)
        self.assertIn("PRISTINE_BUILD_DIR", text)
        self.assertIn("TARGET_RUNTIME_DIR", text)
        self.assertIn("PRISTINE_RUNTIME_DIR", text)
        self.assertIn("target-runtime", text)
        self.assertIn("pristine-runtime", text)
        self.assertGreaterEqual(text.count("LAVFilters.Dependencies.manifest"), 2)
        self.assertNotIn("EnableOpenJOC=false", text)
        self.assertIn(
            "for %%T in (baseclasses DSUtilLite libbluray Demuxers LAVAudio LAVSplitter)",
            text,
        )
        self.assertNotIn(" /m ", text)
        self.assertIn("/p:BuildProjectReferences=false", text)
        self.assertIn("/p:CL_MPCount=1", text)
        self.assertIn("/p:UseMultiToolTask=true", text)
        self.assertIn("/p:MultiProcMaxCount=1", text)
        self.assertIn("MultiProcessorCompilation", text)
        self.assertIn("ForceImportBeforeCppTargets", text)
        self.assertIn("OpenJocEvidenceIntermediateRoot", text)
        self.assertIn(
            "$(OpenJocEvidenceIntermediateRoot)\\$(MSBuildProjectName)\\",
            text,
        )
        self.assertNotIn("/p:IntDir=", text)
        self.assertNotIn("obj\\%%T/", text)
        self.assertIn('call :build_lane "%TARGET_LAV_ROOT%" "%TARGET_BUILD_DIR%" true', text)
        self.assertIn(
            'call :build_lane "%PRISTINE_LAV_ROOT%" "%PRISTINE_BUILD_DIR%" false',
            text,
        )
        self.assertIn("/p:EnableOpenJOC=true", text)
        self.assertIn("/p:EnableOpenJOCSideBySide=%~3", text)
        self.assertIn("/p:OpenJocIncludeDir=", text)
        self.assertGreaterEqual(text.count('copy /y "%OPENJOC_CAPI%"'), 2)
        self.assertIn('"/p:OutDir=%~2/"', text)
        self.assertIn('"/p:OpenJocEvidenceIntermediateRoot=%~2\\obj"', text)
        self.assertIn('if exist "%OUTPUT_DIR%"', text)
        self.assertIn("refusing to reuse output directory", text)
        self.assertNotIn("if errorlevel 1 exit /b %errorlevel%", text)

    def test_compiles_and_runs_private_activation_self_test(self) -> None:
        text = SCRIPT.read_text(encoding="utf-8")

        self.assertIn("OpenJocDirectShowNegotiationSmoke.cpp", text)
        self.assertIn("OpenJocDirectShowNegotiationSmoke.exe", text)
        self.assertIn("strmbase.lib", text)
        self.assertIn("strmiids.lib", text)
        self.assertIn("ole32.lib", text)
        self.assertIn("uuid.lib", text)
        self.assertIn("winmm.lib", text)
        self.assertIn("bcrypt.lib", text)
        self.assertIn("--write-manifest", text)
        self.assertIn("--self-test", text)
        self.assertGreaterEqual(text.count("--self-test"), 2)
        self.assertGreaterEqual(text.count("OpenJocRuntimeIdentity.tsv"), 2)
        self.assertIn("attrib +R", text)

    def test_fixture_generation_exports_fingerprint_raw_and_mp4(self) -> None:
        fixture_text = FIXTURE_SCRIPT.read_text(encoding="utf-8")
        rust_text = RUST_BRIDGE.read_text(encoding="utf-8")

        self.assertIn("OPENJOC_FINGERPRINT_JOC_PATH", fixture_text)
        self.assertIn("joc.fingerprint.ec3", fixture_text)
        self.assertIn("joc.fingerprint.mp4", fixture_text)
        self.assertIn("joc.multi.mp4", fixture_text)
        self.assertIn("distinct bed excitation paths", fixture_text)
        self.assertNotIn("distinct bed mantissas", fixture_text)
        self.assertIn("export_synthetic_joc_fingerprint_fixture_when_requested", fixture_text)
        self.assertIn("OPENJOC_FINGERPRINT_JOC_PATH", rust_text)
        self.assertIn("export_synthetic_joc_fingerprint_fixture_when_requested", rust_text)
        self.assertIn("assert_fingerprint_fixture_distinguishes_every_policy", rust_text)

    def test_harness_declares_private_module_and_no_support_claim_self_test(self) -> None:
        text = HARNESS.read_text(encoding="utf-8")

        self.assertIn("pattern: Imperative Shell", text)
        self.assertIn("pattern: Functional Core", text)
        self.assertIn("class PrivateComModule", text)
        self.assertIn("LoadLibraryExW", text)
        self.assertIn("GetModuleFileNameW", text)
        self.assertIn("GetFinalPathNameByHandleW", text)
        self.assertIn("DllGetClassObject", text)
        self.assertIn("IClassFactory", text)
        self.assertIn("CreateInstance", text)
        self.assertIn("BCryptHashData", text)
        self.assertIn("K32EnumProcessModules", text)
        self.assertIn("kTargetLavAudio", text)
        self.assertIn("kPristineLavAudio", text)
        self.assertIn("kLavSplitterSource", text)
        self.assertIn("OpenJocRuntimeIdentity.tsv", text)
        self.assertIn("LAVSplitter.ax", text)
        self.assertIn("openjoc_capi.dll", text)
        self.assertIn("libbluray.dll", text)
        self.assertIn("LAVFilters.Dependencies.manifest", text)
        self.assertIn("left.pUnk != right.pUnk", text)
        self.assertIn("UNVERIFIED", text)
        self.assertNotIn("STREAM_PROVEN", text)
        self.assertNotIn("physical_subwoofer_count", text)
        self.assertNotIn("SetDllDirectory", text)

    def test_rejects_missing_arguments(self) -> None:
        completed = subprocess.run(
            ["cmd.exe", "/d", "/c", str(SCRIPT)],
            check=False,
            capture_output=True,
            text=True,
        )

        self.assertEqual(completed.returncode, 64)
        self.assertIn("usage:", completed.stderr.lower())


if __name__ == "__main__":
    unittest.main()
