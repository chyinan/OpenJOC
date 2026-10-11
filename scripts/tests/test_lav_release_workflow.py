# SPDX-FileCopyrightText: 2026 OpenJOC contributors
# SPDX-License-Identifier: Apache-2.0

from __future__ import annotations

import pathlib
import unittest


WORKSPACE = pathlib.Path(__file__).resolve().parents[2]
WORKFLOW = WORKSPACE / ".github" / "workflows" / "lav-release.yml"


class LavReleaseWorkflowTests(unittest.TestCase):
    def test_gain_documentation_identifies_required_release_asset(self) -> None:
        for relative, expected in (
            ("docs/integration/LAV_FILTERS_OPENJOC.md", "Available with the v0.19.0 LAV package"),
            ("docs/site/using/windows-lav-potplayer.md", "Available with the v0.19.0 LAV package"),
            ("docs/site/using/windows-lav-potplayer.zh.md", "适用于 v0.19.0 LAV 安装包"),
        ):
            text = (WORKSPACE / relative).read_text(encoding="utf-8")
            self.assertIn(expected, text)
            self.assertIn("v0.18.0", text)
            self.assertIn("openjoc-lav-0.19.0-windows-x64.zip", text)

    def test_native_gain_and_settings_gates_precede_publication(self) -> None:
        text = WORKFLOW.read_text(encoding="utf-8")
        validation = text.split("- name: Validate package and standalone smoke tests")[1].split("- name: Upload LAV asset")[0]
        for required in ("OpenJocOutputTests.exe", "OpenJocStrictOutputTests.exe", "OpenJocSettingsSmoke.exe", "--output-gain-only", "if ($LASTEXITCODE -ne 0)", "Copy-Item -Path (Join-Path $extract 'runtime\\*') -Destination $smokes"):
            self.assertIn(required, validation)
        self.assertNotIn("OpenJocDirectShowLifecycle.exe", validation)
        self.assertIn("ref: ${{ env.RELEASE_TAG }}", text)

    def test_audio_remains_the_dispatch_and_tag_default(self) -> None:
        text = WORKFLOW.read_text(encoding="utf-8")
        variant = text.split("      variant:\n")[1].split("      lav_ref:\n")[0]
        self.assertIn("type: choice", variant)
        self.assertIn("default: audio", variant)
        self.assertIn("options:\n          - audio\n          - full", variant)
        self.assertIn(
            "LAV_VARIANT: ${{ github.event_name == 'workflow_dispatch' && inputs.variant || 'audio' }}",
            text,
        )
        self.assertIn("if ($env:LAV_VARIANT -notin @('audio', 'full'))", text)
        self.assertNotIn("d13b7cac86c5750b5d4181569d98f44ae9a1607c", text)

    def test_full_build_verifies_toolchain_and_all_solution_outputs(self) -> None:
        text = WORKFLOW.read_text(encoding="utf-8")
        preflight = text.split("- name: Verify full-build v145 ATL and Windows SDK")[1].split("- name: Set up Rust")[0]
        for expected in (
            "if: env.LAV_VARIANT == 'full'",
            "Microsoft.VCToolsVersion.default.txt",
            "atlmfc\\include\\atlbase.h",
            "atlmfc\\lib\\x64\\atls.lib",
            "PlatformToolsets\\\\v145",
            "um\\Windows.h",
            "kernel32.lib",
            "cl /nologo /c /EHsc /std:c++17",
            "if ($LASTEXITCODE -ne 0",
            "Selected compiler SDK library missing",
        ):
            self.assertIn(expected, preflight)
        build = text.split("- name: Build selected LAV filters")[1].split("- name: Build LAV smoke tests")[0]
        self.assertIn('"$lav" "$include" "$log" "$env:LAV_VARIANT"', build)
        self.assertIn("$outputs = @('LAVAudio.ax')", build)
        self.assertIn("if ($env:LAV_VARIANT -eq 'full')", build)
        self.assertIn("@('LAVVideo.ax', 'LAVSplitter.ax', 'IntelQuickSyncDecoder.dll')", build)
        self.assertIn('throw "LAV output is missing: $name"', build)
        self.assertIn("libbluray\\bin_x64\\libbluray\\libbluray.dll", build)
        self.assertIn("-Destination (Join-Path $lav 'bin_x64\\libbluray.dll')", build)
        dependencies = text.split("- name: Collect MSVC runtime dependencies")[1].split("- name: Package LAV release asset")[0]
        for name in (
            "concrt140.dll", "msvcp140.dll", "msvcp140_1.dll", "msvcp140_2.dll",
            "msvcp140_atomic_wait.dll", "msvcp140_codecvt_ids.dll", "vccorlib140.dll",
        ):
            self.assertIn(name, dependencies)
        self.assertIn("foreach ($name in $requiredCrt)", dependencies)
        self.assertIn("Get-ChildItem -LiteralPath $crtDir -Filter '*.dll' | Copy-Item -Destination $dependencyDir -Force", dependencies)

    def test_full_package_has_a_distinct_name_and_matching_template(self) -> None:
        text = WORKFLOW.read_text(encoding="utf-8")
        self.assertIn("$assetPrefix = if ($env:LAV_VARIANT -eq 'full') { 'openjoc-lav-full' } else { 'openjoc-lav' }", text)
        self.assertIn('"asset_name=$assetPrefix-$($tag.Substring(1))-windows-x64" >> $env:GITHUB_OUTPUT', text)
        package = text.split("- name: Package LAV release asset")[1].split("- name: Validate package and standalone smoke tests")[0]
        self.assertIn("--variant $env:LAV_VARIANT", package)
        self.assertIn("$template = if ($env:LAV_VARIANT -eq 'full') { 'packaging\\windows-lav-full' } else { 'packaging\\windows-lav' }", package)
        self.assertIn("--onboarding-template (Join-Path $env:GITHUB_WORKSPACE $template)", package)
        archive = "$archive = Join-Path '${{ steps.package.outputs.package_dir }}' '${{ steps.identity.outputs.asset_name }}.zip'"
        self.assertEqual(text.count(archive), 2)
        self.assertIn("name: ${{ steps.identity.outputs.asset_name }}", text)
        # The full ZIP deliberately omits the one-click audio installer. Validate
        # its runtime profile using the checked-in read-only preflight instead.
        validation = text.split("- name: Validate package and standalone smoke tests")[1].split("- name: Upload LAV asset")[0]
        self.assertIn("$validationRoot = if ($env:LAV_VARIANT -eq 'full') { Join-Path $env:GITHUB_WORKSPACE 'packaging\\windows-lav' } else { $extract }", validation)
        self.assertIn("Import-Module (Join-Path $validationRoot 'scripts\\OpenJoc.Onboarding.Shell.psm1') -Force", validation)
        self.assertIn("$preflight = Test-OpenJocPackage $session", validation)
        self.assertIn("-PackageRoot $extract", validation)
        self.assertNotIn("finalize_lav_full_candidate", text)
        self.assertNotIn("download-artifact", text)

    def test_full_notices_use_the_exact_capi_release_sources(self) -> None:
        text = WORKFLOW.read_text(encoding="utf-8")
        sources = text.split("- name: Check out OpenJOC release sources")[1].split("- name: Check out pinned LAV sources")[0]
        self.assertIn("path: openjoc-release", sources)
        self.assertIn("ref: ${{ env.RELEASE_TAG }}", sources)
        package = text.split("- name: Package LAV release asset")[1].split("- name: Validate package and standalone smoke tests")[0]
        self.assertIn("$variantOptions = @()", package)
        options = package.split("\n          if ($env:LAV_VARIANT -eq 'full') {")[1].split("\n          }")[0]
        self.assertIn("$variantOptions = @('--core-source-root', (Join-Path $env:GITHUB_WORKSPACE 'openjoc-release'))", options)
        self.assertIn("--output-dir $output @variantOptions", package)
        self.assertIn("--capi-dll (Join-Path $env:GITHUB_WORKSPACE 'openjoc-release\\target\\release\\openjoc_capi.dll')", package)

    def test_workflow_builds_pinned_lav_and_uploads_the_matching_release_asset(self) -> None:
        text = WORKFLOW.read_text(encoding="utf-8")
        for expected in (
            "windows-2025",
            "workflow_dispatch:",
            "tags: ['v*']",
            "repository: chyinan/LAVFilters-OpenJOC",
            "a876fbc0ca2615fdaa240cf96ac13c82f28689a9",
            "scripts/package_lav_release.py",
            "${{ steps.identity.outputs.asset_name }}.zip",
            "gh release upload",
            "contents: write",
        ):
            self.assertIn(expected, text)
        # Both manual default and tag-trigger fallback must use this reviewed pin.
        self.assertEqual(text.count("a876fbc0ca2615fdaa240cf96ac13c82f28689a9"), 2)
        self.assertIn("cargo build -p openjoc-capi --release --locked", text)
        self.assertIn("--extra-libs=../thirdparty/64/lib/zlib.lib", text)
        self.assertIn("Join-Path $lav 'ffmpeg\\zlib.lib'", text)
        self.assertIn("ref: v1.3.1", text)
        self.assertIn("Build libbluray runtime dependency", text)
        self.assertIn("libbluray\\libbluray.vcxproj", text)
        self.assertIn("libbluray.dll", text)
        self.assertIn("mingw-w64-x86_64-gcc-libs", text)
        self.assertIn("setup-msys2\\msys2.cmd", text)
        self.assertIn("cygpath' '-w' '/mingw64/bin", text)
        preparation = text.split("- name: Prepare LAV FFmpeg inputs")[1].split("- name: Build LAV FFmpeg")[0]
        self.assertIn('python scripts/prepare_lav_ffmpeg_configure.py "$configure"', preparation)
        self.assertIn('if ($LASTEXITCODE -ne 0) { throw "LAV FFmpeg dependency preparation failed:', preparation)
        self.assertIn("defined(Z_HAVE_UNISTD_H) && !defined(_WIN32)", text)
        self.assertIn('bash -c "sh ./build_ffmpeg_msvc.sh x64 release"', text)
        self.assertIn("Retain FFmpeg configure diagnostics", text)
        self.assertIn("lav/ffmpeg/ffbuild/config.log", text)
        self.assertIn("release_lav_msbuild.cmd", text)
        self.assertIn("release_lav_smokes.cmd", text)
        self.assertIn(r'''scripts\tests\LavSmokeNoopLifecycle.cpp''', text)
        self.assertIn("$complete = $true", text)
        self.assertIn("vcruntime140_threads.dll", text)
        self.assertIn("OpenJocDiagnosticTests.exe", text)
        self.assertIn("OpenJocPropertyPageSmoke.exe", text)
        self.assertIn("OpenJocPropertyPageSmoke.exe') $lavAudio", text)
        self.assertIn("JOC Stream property-page resource", text)
        self.assertIn("$attempt -le 150", text)
        self.assertIn("Start-Sleep -Seconds 10", text)


if __name__ == "__main__":
    unittest.main()
