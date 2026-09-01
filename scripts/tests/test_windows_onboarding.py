# SPDX-FileCopyrightText: 2026 OpenJOC contributors
# SPDX-License-Identifier: Apache-2.0

from __future__ import annotations

import hashlib
import json
import ctypes
import os
import pathlib
import shutil
import subprocess
import tempfile
import unittest


WORKSPACE = pathlib.Path(__file__).resolve().parents[2]
TEMPLATE = WORKSPACE / "packaging" / "windows-lav"
V010_RUNTIME_ENV = "OPENJOC_WINDOWS_ONBOARDING_TEST_RUNTIME"
_v010_runtime_value = os.environ.get(V010_RUNTIME_ENV)
V010_RUNTIME = pathlib.Path(_v010_runtime_value) if _v010_runtime_value else pathlib.Path()
REQUIRES_PRIVATE_RUNTIME = unittest.skipUnless(
    bool(_v010_runtime_value) and V010_RUNTIME.is_dir(),
    f"set {V010_RUNTIME_ENV} to a private qualified Windows LAV runtime",
)
ROOT_FILES = (
    "install.bat", "verify.bat", "uninstall.bat", "README.md", "POTPLAYER-QUICKSTART.md",
)
SCRIPT_FILES = (
    "install.ps1", "verify.ps1", "uninstall.ps1",
    "OpenJoc.Onboarding.Core.psm1", "OpenJoc.Onboarding.Shell.psm1",
)
EXPECTED_RUNTIME_FILES = {
    "LAVAudio.ax", "LAVAudio.ax.manifest", "LAVFilters.Dependencies.manifest",
    "openjoc_capi.dll", "avcodec-lav-63.dll", "avfilter-lav-12.dll",
    "avformat-lav-63.dll", "avutil-lav-61.dll", "swresample-lav-7.dll",
    "swscale-lav-10.dll", "libbluray.dll", "zlib1.dll", "libgcc_s_seh-1.dll",
    "libwinpthread-1.dll", "ucrtbase.dll", "vcruntime140.dll",
    "vcruntime140_1.dll", "vcruntime140_threads.dll",
    *{f"api-ms-win-crt-{name}-l1-1-0.dll" for name in (
        "conio", "convert", "environment", "filesystem", "heap", "locale",
        "math", "multibyte", "private", "process", "runtime", "stdio",
        "string", "time", "utility",
    )},
}


def powershell_51() -> pathlib.Path:
    return pathlib.Path(os.environ["WINDIR"]) / "System32" / "WindowsPowerShell" / "v1.0" / "powershell.exe"


class WindowsOnboardingTemplateTests(unittest.TestCase):
    def test_template_targets_current_v015_release(self) -> None:
        current_files = (
            TEMPLATE / "README.md",
            TEMPLATE / "scripts" / "install.ps1",
            TEMPLATE / "scripts" / "verify.ps1",
            TEMPLATE / "scripts" / "uninstall.ps1",
            TEMPLATE / "scripts" / "OpenJoc.Onboarding.Shell.psm1",
        )
        combined = "\n".join(path.read_text(encoding="utf-8") for path in current_files)
        self.assertIn("0.15.0", combined)
        self.assertNotIn("0.11.0", combined)

    def test_template_exposes_obvious_root_launchers_and_script_core(self) -> None:
        for relative in ROOT_FILES:
            self.assertTrue((TEMPLATE / relative).is_file(), relative)
        for relative in SCRIPT_FILES:
            self.assertTrue((TEMPLATE / "scripts" / relative).is_file(), relative)

    def test_bat_launchers_are_thin_file_boundaries(self) -> None:
        for operation in ("install", "verify", "uninstall"):
            text = (TEMPLATE / f"{operation}.bat").read_text()
            folded = text.casefold()
            self.assertIn("%~dp0", text)
            self.assertIn("powershell.exe", folded)
            self.assertIn("-executionpolicy bypass", folded)
            self.assertIn("-file", folded)
            self.assertIn("%*", text)
            self.assertNotIn("-command", folded)
            self.assertNotIn("enabledelayedexpansion", folded)
            self.assertNotIn("set-executionpolicy", folded)

    def test_common_scripts_parse_in_windows_powershell_and_pwsh(self) -> None:
        hosts = [powershell_51()]
        pwsh = shutil.which("pwsh.exe")
        if pwsh:
            hosts.append(pathlib.Path(pwsh))
        for host in hosts:
            for relative in SCRIPT_FILES:
                path = TEMPLATE / "scripts" / relative
                escaped_path = str(path).replace("'", "''")
                command = (
                    f"$p='{escaped_path}';$e=$null;$t=$null;"
                    "[void][System.Management.Automation.Language.Parser]::ParseFile($p,[ref]$t,[ref]$e);"
                    "if($e.Count){$e|ForEach-Object{Write-Error $_};exit 1}"
                )
                completed = subprocess.run([str(host), "-NoProfile", "-Command", command], capture_output=True, text=True)
                self.assertEqual(completed.returncode, 0, f"{host}: {relative}\n{completed.stderr}")

    def test_documented_exit_codes_cover_required_failure_classes(self) -> None:
        readme = (TEMPLATE / "README.md").read_text(encoding="utf-8")
        for code in ("0", "10", "20", "30", "40", "50"):
            self.assertRegex(readme, rf"(?m)^\| {code} \|")

    def test_core_defines_complete_runtime_inventory_and_exact_class_ids(self) -> None:
        core = TEMPLATE / "scripts" / "OpenJoc.Onboarding.Core.psm1"
        escaped = str(core).replace("'", "''")
        command = (
            f"Import-Module '{escaped}' -Force;"
            "$files=Get-OpenJocRequiredRuntimeFiles;$ids=Get-OpenJocClassIds;"
            "[pscustomobject]@{Files=$files;Ids=$ids}|ConvertTo-Json -Depth 4 -Compress"
        )
        completed = subprocess.run([str(powershell_51()), "-NoProfile", "-Command", command], check=True, capture_output=True, text=True)
        data = json.loads(completed.stdout)
        self.assertEqual(set(data["Files"]), EXPECTED_RUNTIME_FILES)
        self.assertEqual(data["Ids"]["OpenJocAudio"], "{27247580-C701-40CD-886D-E618FC8C9FFF}")
        self.assertEqual(data["Ids"]["StockLavAudio"], "{E8E73B6B-4CB3-44A4-BE99-4F7BCB96E491}")
        self.assertEqual(len(data["Ids"]), 6)

    def test_runtime_profile_overrides_legacy_inventory_for_new_packages(self) -> None:
        core = TEMPLATE / "scripts" / "OpenJoc.Onboarding.Core.psm1"
        with tempfile.TemporaryDirectory() as temporary:
            runtime = pathlib.Path(temporary) / "runtime"
            runtime.mkdir()
            profile = {
                "version": "0.15.0",
                "architecture": "x64",
                "required_runtime_files": [
                    "LAVAudio.ax",
                    "LAVAudio.ax.manifest",
                    "LAVFilters.Dependencies.manifest",
                    "openjoc_capi.dll",
                    "zlibwapi.dll",
                ],
            }
            (runtime / "OpenJocRuntimeProfile.json").write_text(
                json.dumps(profile), encoding="utf-8"
            )
            escaped_core = str(core).replace("'", "''")
            escaped_runtime = str(runtime).replace("'", "''")
            command = (
                f"Import-Module '{escaped_core}' -Force;"
                f"$p=Get-OpenJocRuntimeProfile '{escaped_runtime}';"
                f"$f=@(Get-OpenJocRequiredRuntimeFiles '{escaped_runtime}');"
                "$o=[pscustomobject]@{Version=$p.Version;Files=$f};"
                "$o|ConvertTo-Json -Compress"
            )
            completed = subprocess.run(
                [str(powershell_51()), "-NoProfile", "-Command", command],
                capture_output=True,
                text=True,
            )
            self.assertEqual(completed.returncode, 0, completed.stdout + completed.stderr)
            data = json.loads(completed.stdout)
            self.assertEqual(data["Version"], "0.15.0")
            self.assertEqual(data["Files"], profile["required_runtime_files"])

    def test_uninstall_snapshot_keeps_live_neighbors_and_restores_original_main(self) -> None:
        core = TEMPLATE / "scripts" / "OpenJoc.Onboarding.Core.psm1"
        environment = os.environ.copy()
        environment["OPENJOC_TEST_CORE"] = str(core)
        command = r"""
Import-Module $env:OPENJOC_TEST_CORE -Force
$main='{27247580-C701-40CD-886D-E618FC8C9FFF}'
$stock='{E8E73B6B-4CB3-44A4-BE99-4F7BCB96E491}'
$current=@(
  [pscustomobject]@{ClassId=$main;Existed=$false;File='clsid-0.reg';InprocPath=$null;SnapshotHash=$null},
  [pscustomobject]@{ClassId=$stock;Existed=$true;File='clsid-1.reg';InprocPath='C:\live-stock.ax';SnapshotHash='LIVE'}
)
$baseline=@(
  [pscustomobject]@{ClassId=$main;Existed=$true;File='clsid-0.reg';InprocPath='C:\OpenJOC\0.10.0\LAVAudio.ax';SnapshotHash='ORIGINAL'},
  [pscustomobject]@{ClassId=$stock;Existed=$true;File='clsid-1.reg';InprocPath='C:\old-stock.ax';SnapshotHash='OLD'}
)
$desired=@(Get-OpenJocUninstallDesiredSnapshot $current $baseline $main)
$desired | ConvertTo-Json -Compress
if($desired.Count -ne 2){exit 1}
if(($desired | Where-Object ClassId -eq $main).InprocPath -ne 'C:\OpenJOC\0.10.0\LAVAudio.ax'){exit 2}
if(($desired | Where-Object ClassId -eq $stock).InprocPath -ne 'C:\live-stock.ax'){exit 3}
"""
        completed = subprocess.run(
            [str(powershell_51()), "-NoProfile", "-Command", command],
            env=environment,
            capture_output=True,
            text=True,
        )
        self.assertEqual(completed.returncode, 0, completed.stdout + completed.stderr)

    def test_windows_command_line_quoting_roundtrips_difficult_arguments(self) -> None:
        core = TEMPLATE / "scripts" / "OpenJoc.Onboarding.Core.psm1"
        escaped_core = str(core).replace("'", "''")
        values = (
            "", "plain", "OpenJOC LAV (Test)", "OpenJOC 测试",
            "OpenJOC & LAV", "OpenJOC LAV's Test!", 'embedded"quote',
            "C:\\path with spaces\\",
        )
        shell32 = ctypes.windll.shell32
        kernel32 = ctypes.windll.kernel32
        shell32.CommandLineToArgvW.restype = ctypes.POINTER(ctypes.c_wchar_p)
        with tempfile.TemporaryDirectory() as temporary:
            for index, value in enumerate(values):
                literal = value.replace("'", "''")
                output_path = pathlib.Path(temporary) / f"quoted-{index}.txt"
                escaped_output = str(output_path).replace("'", "''")
                command = (
                    f"Import-Module '{escaped_core}' -Force;"
                    f"ConvertTo-OpenJocCommandLineArgument '{literal}' | "
                    f"Set-Content -LiteralPath '{escaped_output}' -Encoding Unicode"
                )
                subprocess.run([str(powershell_51()), "-NoProfile", "-Command", command], check=True, capture_output=True)
                quoted = output_path.read_text(encoding="utf-16").rstrip("\r\n")
                count = ctypes.c_int()
                argv = shell32.CommandLineToArgvW(f"dummy.exe {quoted}", ctypes.byref(count))
                self.assertTrue(argv, value)
                try:
                    self.assertEqual(count.value, 2, value)
                    self.assertEqual(argv[1], value)
                finally:
                    kernel32.LocalFree(argv)

    def test_scripts_do_not_use_forbidden_policy_shell_or_path_mutations(self) -> None:
        text = "\n".join((TEMPLATE / "scripts" / name).read_text() for name in SCRIPT_FILES).casefold()
        self.assertNotIn("invoke-expression", text)
        self.assertNotIn("set-executionpolicy", text)
        self.assertNotIn("environmentvariabletarget]::machine", text)
        self.assertNotIn("environmentvariabletarget]::user", text)
        self.assertNotIn("remove-item env:path", text)

    def test_recursive_removal_requires_an_openjoc_ownership_proof(self) -> None:
        shell = (TEMPLATE / "scripts" / "OpenJoc.Onboarding.Shell.psm1").read_text()
        self.assertIn("Test-OpenJocOwnedInstall", shell)
        self.assertIn(".openjoc-transient", shell)
        recursive_lines = [line.strip() for line in shell.splitlines() if "Remove-Item" in line and "-Recurse" in line]
        self.assertEqual(recursive_lines, ["Remove-Item -LiteralPath $Path -Recurse -Force"])

    def test_native_tools_are_captured_and_repair_backup_is_owned(self) -> None:
        shell = (TEMPLATE / "scripts" / "OpenJoc.Onboarding.Shell.psm1").read_text()
        self.assertIn("$info.RedirectStandardOutput = $true", shell)
        self.assertIn("$info.RedirectStandardError = $true", shell)
        self.assertIn("$existingMarker = Join-Path $Session.InstallRoot '.openjoc-transient'", shell)
        self.assertIn("Set-Content -LiteralPath $existingMarker", shell)
        self.assertIn("Remove-OpenJocOwnedDirectory $Session $backup -Transient", shell)

    def test_shared_lav_registration_baseline_is_persisted_and_verified(self) -> None:
        shell = (TEMPLATE / "scripts" / "OpenJoc.Onboarding.Shell.psm1").read_text()
        self.assertIn("RegistryBaseline = $baselineSnapshot", shell)
        self.assertIn("$baselineSnapshot = @($existingState.RegistryBaseline)", shell)
        self.assertIn("Shared LAV class isolation", shell)
        self.assertIn("SnapshotHash = $snapshotHash", shell)

    def test_registry_restore_and_verification_are_exact_for_absent_and_stock_keys(self) -> None:
        shell = (TEMPLATE / "scripts" / "OpenJoc.Onboarding.Shell.psm1").read_text()
        self.assertNotIn("Restore-OpenJocRegistrySnapshot $Session $sharedSnapshot $snapshotDirectory -ExistingOnly", shell)
        self.assertIn("Test-OpenJocRegistrySnapshotExact", shell)
        self.assertIn("Stock LAV registration", shell)
        self.assertNotIn("$deleteExit -notin @(0, 1)", shell)

    def test_registry_restore_preflights_all_snapshots_before_native_mutation(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            environment = os.environ.copy()
            environment.update({
                "OPENJOC_TEST_MODULE": str(TEMPLATE / "scripts" / "OpenJoc.Onboarding.Shell.psm1"),
                "OPENJOC_TEST_SNAPSHOT_DIR": temporary,
            })
            command = r"""
$m=Import-Module $env:OPENJOC_TEST_MODULE -Force -PassThru
& $m {
  $script:nativeCalled=$false
  function Invoke-OpenJocNative { $script:nativeCalled=$true; return 0 }
  $item=[pscustomobject]@{
    ClassId='{00000000-0000-0000-0000-000000000001}'
    Existed=$true
    File='clsid-0.reg'
    SnapshotHash=('0' * 64)
  }
  try {
    Restore-OpenJocRegistrySnapshot ([pscustomobject]@{}) @($item) $env:OPENJOC_TEST_SNAPSHOT_DIR
    exit 1
  } catch {
    if($script:nativeCalled){exit 2}
    $_.Exception.Message
  }
}
"""
            completed = subprocess.run(
                [str(powershell_51()), "-NoProfile", "-Command", command],
                env=environment,
                capture_output=True,
                text=True,
            )
            self.assertEqual(completed.returncode, 0, completed.stderr)
            self.assertIn("snapshot file is missing", completed.stdout)

    def test_registry_hashing_does_not_depend_on_powershell_module_autoload(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = pathlib.Path(temporary) / "snapshot.reg"
            path.write_bytes(b"OpenJOC registry snapshot\r\n")
            environment = os.environ.copy()
            environment.update({
                "OPENJOC_TEST_MODULE": str(TEMPLATE / "scripts" / "OpenJoc.Onboarding.Shell.psm1"),
                "OPENJOC_TEST_HASH_FILE": str(path),
                "PSModulePath": str(pathlib.Path(temporary) / "no modules here"),
            })
            command = r"""
$m=Import-Module $env:OPENJOC_TEST_MODULE -Force -PassThru
& $m { Get-OpenJocFileSha256 $env:OPENJOC_TEST_HASH_FILE }
"""
            completed = subprocess.run(
                [str(powershell_51()), "-NoProfile", "-Command", command],
                env=environment,
                capture_output=True,
                text=True,
            )
            self.assertEqual(completed.returncode, 0, completed.stderr)
            self.assertEqual(
                completed.stdout.strip(),
                "E7E9ED2555BEBBAC1AF63522782E49785D4219FC63720BF9233E848DB3B20F79",
            )

    def test_runtime_verification_checks_x64_pe_and_loadability(self) -> None:
        shell = (TEMPLATE / "scripts" / "OpenJoc.Onboarding.Shell.psm1").read_text()
        self.assertIn("Is64BitProcess", shell)
        self.assertIn("0x8664", shell)
        self.assertIn("LoadLibraryEx", shell)
        self.assertIn("Runtime loadability", shell)

    def test_uninstall_has_full_registry_rollback_before_owned_file_commit(self) -> None:
        shell = (TEMPLATE / "scripts" / "OpenJoc.Onboarding.Shell.psm1").read_text()
        self.assertIn("registry-for-rollback", shell)
        self.assertIn("Restore-OpenJocRegistrySnapshot $Session $rollbackSnapshot", shell)
        self.assertIn("$deletionStarted", shell)
        self.assertIn("Move-Item -LiteralPath $Session.InstallRoot -Destination $tombstone", shell)

    def test_install_rollback_never_deletes_an_existing_install_that_was_not_moved(self) -> None:
        shell = (TEMPLATE / "scripts" / "OpenJoc.Onboarding.Shell.psm1").read_text()
        self.assertIn("$existingInstallMoved = $false", shell)
        self.assertIn("$newInstallPlaced = $false", shell)
        self.assertIn("if ($newInstallPlaced -and (Test-Path -LiteralPath $Session.InstallRoot))", shell)
        install = shell[shell.index("function Invoke-OpenJocInstallTransaction"):shell.index("function Invoke-OpenJocUninstallTransaction")]
        marker_position = install.index("Set-Content -LiteralPath $existingMarker")
        move_position = install.index("Move-Item -LiteralPath $Session.InstallRoot -Destination $backup")
        self.assertLess(marker_position, move_position)
        placed_position = install.index("$newInstallPlaced = $true")
        relocated_snapshot_position = install.index("$rollbackDirectory = Join-Path $Session.InstallRoot", placed_position)
        remove_stage_marker_position = install.index("Remove-Item -LiteralPath (Join-Path $Session.InstallRoot '.openjoc-transient')", placed_position)
        self.assertLess(relocated_snapshot_position, remove_stage_marker_position)
        self.assertIn("if ($registrationAttempted -and $rollbackSnapshot)", install)

    def test_transient_directory_creation_refuses_existing_directory(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            existing = pathlib.Path(temporary) / "existing victim"
            existing.mkdir()
            victim = existing / "keep.txt"
            victim.write_text("keep", encoding="utf-8")
            module = TEMPLATE / "scripts" / "OpenJoc.Onboarding.Shell.psm1"
            escaped_module = str(module).replace("'", "''")
            escaped_existing = str(existing).replace("'", "''")
            command = (
                f"$m=Import-Module '{escaped_module}' -Force -PassThru;"
                f"try {{ & $m {{ New-OpenJocTransientDirectory '{escaped_existing}' }}; exit 1 }} "
                "catch { exit 0 }"
            )
            completed = subprocess.run([str(powershell_51()), "-NoProfile", "-Command", command], capture_output=True, text=True)
            self.assertEqual(completed.returncode, 0, completed.stderr)
            self.assertEqual(victim.read_text(encoding="utf-8"), "keep")

    def test_committed_install_removes_only_its_validated_rollback_snapshot(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            install = root / "owned install"
            rollback = install / "state" / "registry-for-rollback"
            rollback.mkdir(parents=True)
            token = "committed-cleanup-token"
            state = {
                "ProductId": "OpenJOC.LAV.Windows",
                "InstallRoot": str(install),
                "OwnershipToken": token,
            }
            (install / "openjoc-install.json").write_text(json.dumps(state), encoding="utf-8")
            (install / ".openjoc-ownership").write_text(token, encoding="ascii")
            (rollback / "clsid-0.reg").write_text("snapshot", encoding="utf-8")
            (rollback / "snapshot.json").write_text("[]", encoding="utf-8")
            environment = os.environ.copy()
            environment.update({
                "OPENJOC_TEST_MODULE": str(TEMPLATE / "scripts" / "OpenJoc.Onboarding.Shell.psm1"),
                "OPENJOC_TEST_INSTALL": str(install),
                "OPENJOC_TEST_ROLLBACK": str(rollback),
            })
            command = r"""
$m=Import-Module $env:OPENJOC_TEST_MODULE -Force -PassThru
$s=[pscustomobject]@{InstallRoot=$env:OPENJOC_TEST_INSTALL}
$items=@([pscustomobject]@{Existed=$true;File='clsid-0.reg'})
& $m { param($session,$snapshot,$directory) Remove-OpenJocCommittedRollbackSnapshot $session $snapshot $directory } $s $items $env:OPENJOC_TEST_ROLLBACK
if(Test-Path -LiteralPath $env:OPENJOC_TEST_ROLLBACK){exit 1}
"""
            completed = subprocess.run(
                [str(powershell_51()), "-NoProfile", "-Command", command],
                env=environment,
                capture_output=True,
                text=True,
            )
            self.assertEqual(completed.returncode, 0, completed.stdout + completed.stderr)

    def test_committed_rollback_cleanup_refuses_unexpected_content(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            install = root / "owned install"
            rollback = install / "state" / "registry-for-rollback"
            rollback.mkdir(parents=True)
            token = "refuse-unexpected-cleanup-token"
            state = {
                "ProductId": "OpenJOC.LAV.Windows",
                "InstallRoot": str(install),
                "OwnershipToken": token,
            }
            (install / "openjoc-install.json").write_text(json.dumps(state), encoding="utf-8")
            (install / ".openjoc-ownership").write_text(token, encoding="ascii")
            unexpected = rollback / "keep.txt"
            unexpected.write_text("do not delete", encoding="utf-8")
            environment = os.environ.copy()
            environment.update({
                "OPENJOC_TEST_MODULE": str(TEMPLATE / "scripts" / "OpenJoc.Onboarding.Shell.psm1"),
                "OPENJOC_TEST_INSTALL": str(install),
                "OPENJOC_TEST_ROLLBACK": str(rollback),
            })
            command = r"""
$m=Import-Module $env:OPENJOC_TEST_MODULE -Force -PassThru
$s=[pscustomobject]@{InstallRoot=$env:OPENJOC_TEST_INSTALL}
try {
  & $m { param($session,$directory) Remove-OpenJocCommittedRollbackSnapshot $session @() $directory } $s $env:OPENJOC_TEST_ROLLBACK
  exit 1
} catch { exit 0 }
"""
            completed = subprocess.run(
                [str(powershell_51()), "-NoProfile", "-Command", command],
                env=environment,
                capture_output=True,
                text=True,
            )
            self.assertEqual(completed.returncode, 0, completed.stdout + completed.stderr)
            self.assertEqual(unexpected.read_text(encoding="utf-8"), "do not delete")

    @REQUIRES_PRIVATE_RUNTIME
    def test_post_commit_log_failure_never_rolls_back_verified_install(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            package = root / "package"
            shutil.copytree(V010_RUNTIME, package / "runtime")
            install = root / "installed target"
            environment = os.environ.copy()
            environment.update({
                "OPENJOC_TEST_MODULE": str(TEMPLATE / "scripts" / "OpenJoc.Onboarding.Shell.psm1"),
                "OPENJOC_TEST_PACKAGE": str(package),
                "OPENJOC_TEST_INSTALL": str(install),
            })
            command = r"""
$m=Import-Module $env:OPENJOC_TEST_MODULE -Force -PassThru
& $m {
  function script:Save-OpenJocRegistrySnapshot {
    param($Session,$ClassIds,[string]$Directory,$IncludeClassIds)
    New-Item -ItemType Directory -Path $Directory -Force | Out-Null
    Set-Content -LiteralPath (Join-Path $Directory 'snapshot.json') -Value '[]' -Encoding UTF8
    $index=0
    foreach($classId in $ClassIds){
      [pscustomobject]@{ClassId=$classId;Existed=$false;File="clsid-$index.reg";InprocPath=$null;SnapshotHash=$null}
      $index++
    }
  }
  function script:Invoke-OpenJocNative { return 0 }
  function script:Restore-OpenJocRegistrySnapshot { }
  function script:Get-OpenJocVerification {
    [pscustomobject]@{Success=$true;Checks=@([pscustomobject]@{Name='mock';Passed=$true;Detail='verified'})}
  }
  function script:Write-OpenJocLog { throw 'simulated post-commit log failure' }
}
$s=[pscustomobject]@{PackageRoot=$env:OPENJOC_TEST_PACKAGE;InstallRoot=$env:OPENJOC_TEST_INSTALL;LogPath='unused';NonInteractive=$true}
$result=Invoke-OpenJocInstallTransaction $s
if($result.ExitCode -ne 0){exit 1}
if(-not (Test-Path -LiteralPath $env:OPENJOC_TEST_INSTALL)){exit 2}
if(Test-Path -LiteralPath (Join-Path $env:OPENJOC_TEST_INSTALL 'state\registry-for-rollback')){exit 3}
if($result.Warning -notlike '*post-commit*'){exit 4}
"""
            completed = subprocess.run(
                [str(powershell_51()), "-NoProfile", "-Command", command],
                env=environment,
                capture_output=True,
                text=True,
            )
            self.assertEqual(completed.returncode, 0, completed.stdout + completed.stderr)

    @REQUIRES_PRIVATE_RUNTIME
    def test_log_write_failure_cannot_block_precommit_rollback(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            package = root / "package"
            shutil.copytree(V010_RUNTIME, package / "runtime")
            install = root / "installed target"
            log_directory = root / "log path is a directory"
            log_directory.mkdir()
            environment = os.environ.copy()
            environment.update({
                "OPENJOC_TEST_MODULE": str(TEMPLATE / "scripts" / "OpenJoc.Onboarding.Shell.psm1"),
                "OPENJOC_TEST_PACKAGE": str(package),
                "OPENJOC_TEST_INSTALL": str(install),
                "OPENJOC_TEST_LOG": str(log_directory),
                "OPENJOC_TEST_UNREGISTERED": str(root / "unregistered.flag"),
                "OPENJOC_TEST_RESTORED": str(root / "restored.flag"),
            })
            command = r"""
$m=Import-Module $env:OPENJOC_TEST_MODULE -Force -PassThru
& $m {
  function script:Save-OpenJocRegistrySnapshot {
    param($Session,$ClassIds,[string]$Directory,$IncludeClassIds)
    New-Item -ItemType Directory -Path $Directory -Force | Out-Null
    Set-Content -LiteralPath (Join-Path $Directory 'snapshot.json') -Value '[]' -Encoding UTF8
    $index=0
    foreach($classId in $ClassIds){
      [pscustomobject]@{ClassId=$classId;Existed=$false;File="clsid-$index.reg";InprocPath=$null;SnapshotHash=$null}
      $index++
    }
  }
  function script:Invoke-OpenJocNative {
    param($Session,[string]$FilePath,$Arguments)
    Write-OpenJocLog $Session 'INFO' 'simulated native log write'
    if($Arguments -contains '/u'){
      Set-Content -LiteralPath $env:OPENJOC_TEST_UNREGISTERED -Value 'yes'
      return 0
    }
    return 7
  }
  function script:Get-OpenJocInprocPath { Join-Path $env:OPENJOC_TEST_INSTALL 'LAVAudio.ax' }
  function script:Restore-OpenJocRegistrySnapshot {
    Set-Content -LiteralPath $env:OPENJOC_TEST_RESTORED -Value 'yes'
  }
}
$s=[pscustomobject]@{PackageRoot=$env:OPENJOC_TEST_PACKAGE;InstallRoot=$env:OPENJOC_TEST_INSTALL;LogPath=$env:OPENJOC_TEST_LOG;NonInteractive=$true}
$result=Invoke-OpenJocInstallTransaction $s
if($result.ExitCode -ne 30){exit 1}
if(Test-Path -LiteralPath $env:OPENJOC_TEST_INSTALL){exit 2}
if(-not (Test-Path -LiteralPath $env:OPENJOC_TEST_UNREGISTERED)){exit 3}
if(-not (Test-Path -LiteralPath $env:OPENJOC_TEST_RESTORED)){exit 4}
"""
            completed = subprocess.run(
                [str(powershell_51()), "-NoProfile", "-Command", command],
                env=environment,
                capture_output=True,
                text=True,
            )
            self.assertEqual(completed.returncode, 0, completed.stdout + completed.stderr)

    def test_launcher_fallback_keeps_pre_ui_powershell_failure_visible(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            package = pathlib.Path(temporary) / "broken launcher package"
            shutil.copytree(TEMPLATE, package)
            missing_script = package / "scripts" / "install.ps1"
            missing_script.unlink()
            command = (
                f'{os.environ.get("COMSPEC", "cmd.exe")} /d /c '
                f'call "{package / "install.bat"}" -NonInteractive'
            )
            completed = subprocess.run(command, capture_output=True, text=True)
            output = completed.stdout + completed.stderr
            self.assertEqual(completed.returncode, 20)
            self.assertIn("PowerShell could not start", output)

    @REQUIRES_PRIVATE_RUNTIME
    def test_package_preflight_rejects_corrupted_critical_pe_file(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            package = pathlib.Path(temporary) / "OpenJOC corrupted package"
            runtime = package / "runtime"
            shutil.copytree(V010_RUNTIME, runtime)
            (runtime / "openjoc_capi.dll").write_bytes(b"not a PE file")
            module = TEMPLATE / "scripts" / "OpenJoc.Onboarding.Shell.psm1"
            escaped_module = str(module).replace("'", "''")
            escaped_package = str(package).replace("'", "''")
            command = (
                f"Import-Module '{escaped_module}' -Force;"
                f"$s=[pscustomobject]@{{PackageRoot='{escaped_package}'}};"
                "$r=Test-OpenJocPackage $s;$r|ConvertTo-Json -Compress;if($r.Success){exit 1}"
            )
            completed = subprocess.run([str(powershell_51()), "-NoProfile", "-Command", command], capture_output=True, text=True)
            self.assertEqual(completed.returncode, 0, completed.stderr)
            self.assertIn("invalid", completed.stdout.casefold())

    @REQUIRES_PRIVATE_RUNTIME
    def test_package_preflight_load_tests_non_root_runtime_dlls(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            package = pathlib.Path(temporary) / "OpenJOC unloadable non-root DLL"
            runtime = package / "runtime"
            shutil.copytree(V010_RUNTIME, runtime)
            target = runtime / "avfilter-lav-12.dll"
            target.write_bytes(target.read_bytes()[:512])
            module = TEMPLATE / "scripts" / "OpenJoc.Onboarding.Shell.psm1"
            escaped_module = str(module).replace("'", "''")
            escaped_package = str(package).replace("'", "''")
            command = (
                f"Import-Module '{escaped_module}' -Force;"
                f"$s=[pscustomobject]@{{PackageRoot='{escaped_package}'}};"
                "$r=Test-OpenJocPackage $s;$r|ConvertTo-Json -Compress;if($r.Success){exit 1}"
            )
            completed = subprocess.run([str(powershell_51()), "-NoProfile", "-Command", command], capture_output=True, text=True)
            self.assertEqual(completed.returncode, 0, completed.stderr)
            self.assertIn("loadability failed for avfilter-lav-12.dll", completed.stdout.casefold())

    @REQUIRES_PRIVATE_RUNTIME
    def test_registration_failure_rolls_back_new_install_with_stable_exit(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            package = root / "package"
            shutil.copytree(V010_RUNTIME, package / "runtime")
            install = root / "installed target"
            log = root / "transaction.log"
            environment = os.environ.copy()
            environment.update({
                "OPENJOC_TEST_MODULE": str(TEMPLATE / "scripts" / "OpenJoc.Onboarding.Shell.psm1"),
                "OPENJOC_TEST_PACKAGE": str(package),
                "OPENJOC_TEST_INSTALL": str(install),
                "OPENJOC_TEST_LOG": str(log),
            })
            command = r"""
$m=Import-Module $env:OPENJOC_TEST_MODULE -Force -PassThru
& $m {
  function Save-OpenJocRegistrySnapshot { @() }
  function Restore-OpenJocRegistrySnapshot { }
  function Invoke-OpenJocNative { return 7 }
  function Get-OpenJocInprocPath { return $null }
  $s=[pscustomobject]@{
    PackageRoot=$env:OPENJOC_TEST_PACKAGE
    InstallRoot=$env:OPENJOC_TEST_INSTALL
    LogPath=$env:OPENJOC_TEST_LOG
    NonInteractive=$true
  }
  $r=Invoke-OpenJocInstallTransaction $s
  $r | ConvertTo-Json -Compress
  if($r.ExitCode -ne 30 -or $r.Rollback -ne 'completed' -or
     (Test-Path -LiteralPath $env:OPENJOC_TEST_INSTALL)){exit 1}
}
"""
            completed = subprocess.run(
                [str(powershell_51()), "-NoProfile", "-Command", command],
                env=environment,
                capture_output=True,
                text=True,
            )
            self.assertEqual(completed.returncode, 0, completed.stderr)
            self.assertIn("regsvr32 exit 7", completed.stdout)
            self.assertFalse(install.exists())
            self.assertIn("rollback=completed", log.read_text(encoding="utf-8-sig"))

    def test_absent_install_does_not_claim_stock_or_shared_baseline_matches(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            environment = os.environ.copy()
            environment.update({
                "OPENJOC_TEST_MODULE": str(TEMPLATE / "scripts" / "OpenJoc.Onboarding.Shell.psm1"),
                "OPENJOC_TEST_PACKAGE": str(TEMPLATE),
                "OPENJOC_TEST_INSTALL": str(root / "absent install"),
                "OPENJOC_TEST_LOG": str(root / "verify.log"),
            })
            command = r"""
Import-Module $env:OPENJOC_TEST_MODULE -Force
$s=[pscustomobject]@{
  PackageRoot=$env:OPENJOC_TEST_PACKAGE
  InstallRoot=$env:OPENJOC_TEST_INSTALL
  LogPath=$env:OPENJOC_TEST_LOG
  NonInteractive=$true
}
$checks=(Get-OpenJocVerification $s).Checks |
  Where-Object { $_.Name -in @('Shared LAV class isolation','Stock LAV registration') }
$checks | ConvertTo-Json -Compress
if($checks.Passed -contains $true){exit 1}
if(($checks.Detail | Where-Object { $_ -notlike 'Cannot verify*' }).Count -ne 0){exit 2}
"""
            completed = subprocess.run(
                [str(powershell_51()), "-NoProfile", "-Command", command],
                env=environment,
                capture_output=True,
                text=True,
            )
            self.assertEqual(completed.returncode, 0, completed.stdout + completed.stderr)

    def test_verification_rejects_missing_original_registry_baseline_file(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            install = root / "owned install"
            baseline = install / "state" / "registry-before-install"
            baseline.mkdir(parents=True)
            token = "baseline-integrity-token"
            class_ids = (
                "{27247580-C701-40CD-886D-E618FC8C9FFF}",
                "{E8E73B6B-4CB3-44A4-BE99-4F7BCB96E491}",
                "{2D8F1801-A70D-48F4-B76B-7F5AE022AB54}",
                "{C89FC33C-E60A-4C97-BEF4-ACC5762B6404}",
                "{BD72668E-6BFF-4CD1-8480-D465708B336B}",
                "{20ED4A03-6AFD-4FD9-980B-2F6143AA0892}",
            )
            snapshots = [
                {
                    "ClassId": class_id,
                    "Existed": index == 0,
                    "File": f"clsid-{index}.reg",
                    "InprocPath": "C:\\OpenJOC\\0.10.0\\LAVAudio.ax" if index == 0 else None,
                    "SnapshotHash": "0" * 64 if index == 0 else None,
                }
                for index, class_id in enumerate(class_ids)
            ]
            state = {
                "ProductId": "OpenJOC.LAV.Windows",
                "InstallRoot": str(install),
                "OwnershipToken": token,
                "RegistryBaseline": snapshots,
            }
            (install / "openjoc-install.json").write_text(json.dumps(state), encoding="utf-8")
            (install / ".openjoc-ownership").write_text(token, encoding="ascii")
            environment = os.environ.copy()
            environment.update({
                "OPENJOC_TEST_MODULE": str(TEMPLATE / "scripts" / "OpenJoc.Onboarding.Shell.psm1"),
                "OPENJOC_TEST_PACKAGE": str(TEMPLATE),
                "OPENJOC_TEST_INSTALL": str(install),
                "OPENJOC_TEST_LOG": str(root / "verify.log"),
            })
            command = r"""
Import-Module $env:OPENJOC_TEST_MODULE -Force
$s=[pscustomobject]@{PackageRoot=$env:OPENJOC_TEST_PACKAGE;InstallRoot=$env:OPENJOC_TEST_INSTALL;LogPath=$env:OPENJOC_TEST_LOG;NonInteractive=$true}
$check=(Get-OpenJocVerification $s).Checks | Where-Object Name -eq 'Registry rollback baseline'
$check | ConvertTo-Json -Compress
if($check.Passed){exit 1}
if($check.Detail -notlike '*snapshot file is missing*'){exit 2}
"""
            completed = subprocess.run(
                [str(powershell_51()), "-NoProfile", "-Command", command],
                env=environment,
                capture_output=True,
                text=True,
            )
            self.assertEqual(completed.returncode, 0, completed.stdout + completed.stderr)

    def test_verification_keeps_valid_baseline_and_neighbor_results_independent(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            install = root / "owned install"
            baseline = install / "state" / "registry-before-install"
            baseline.mkdir(parents=True)
            token = "independent-verification-token"
            class_ids = (
                "{27247580-C701-40CD-886D-E618FC8C9FFF}",
                "{E8E73B6B-4CB3-44A4-BE99-4F7BCB96E491}",
                "{2D8F1801-A70D-48F4-B76B-7F5AE022AB54}",
                "{C89FC33C-E60A-4C97-BEF4-ACC5762B6404}",
                "{BD72668E-6BFF-4CD1-8480-D465708B336B}",
                "{20ED4A03-6AFD-4FD9-980B-2F6143AA0892}",
            )
            snapshots = [
                {
                    "ClassId": class_id,
                    "Existed": False,
                    "File": f"clsid-{index}.reg",
                    "InprocPath": None,
                    "SnapshotHash": None,
                }
                for index, class_id in enumerate(class_ids)
            ]
            state = {
                "ProductId": "OpenJOC.LAV.Windows",
                "InstallRoot": str(install),
                "OwnershipToken": token,
                "RegistryBaseline": snapshots,
            }
            (install / "openjoc-install.json").write_text(json.dumps(state), encoding="utf-8")
            (install / ".openjoc-ownership").write_text(token, encoding="ascii")
            environment = os.environ.copy()
            environment.update({
                "OPENJOC_TEST_MODULE": str(TEMPLATE / "scripts" / "OpenJoc.Onboarding.Shell.psm1"),
                "OPENJOC_TEST_PACKAGE": str(TEMPLATE),
                "OPENJOC_TEST_INSTALL": str(install),
                "OPENJOC_TEST_LOG": str(root / "verify.log"),
            })
            command = r"""
$module=Import-Module $env:OPENJOC_TEST_MODULE -Force -PassThru
& $module {
  function script:Test-OpenJocRegistrySnapshotExact {
    param($Session,$Items,[string]$Directory)
    if(@($Items).Count -eq 4){throw 'shared exact probe failed'}
    [pscustomobject]@{Success=$true;Detail='stock exact probe passed'}
  }
}
$s=[pscustomobject]@{PackageRoot=$env:OPENJOC_TEST_PACKAGE;InstallRoot=$env:OPENJOC_TEST_INSTALL;LogPath=$env:OPENJOC_TEST_LOG;NonInteractive=$true}
$checks=(Get-OpenJocVerification $s).Checks
$baseline=$checks | Where-Object Name -eq 'Registry rollback baseline'
$shared=$checks | Where-Object Name -eq 'Shared LAV class isolation'
$stock=$checks | Where-Object Name -eq 'Stock LAV registration'
if(-not $baseline.Passed){exit 1}
if($shared.Passed -or $shared.Detail -notlike '*shared exact probe failed*'){exit 2}
if(-not $stock.Passed -or $stock.Detail -ne 'stock exact probe passed'){exit 3}
"""
            completed = subprocess.run(
                [str(powershell_51()), "-NoProfile", "-Command", command],
                env=environment,
                capture_output=True,
                text=True,
            )
            self.assertEqual(completed.returncode, 0, completed.stdout + completed.stderr)

    def test_verification_rejects_self_referential_uninstall_baseline(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            install = root / "owned install"
            baseline = install / "state" / "registry-before-install"
            baseline.mkdir(parents=True)
            token = "self-referential-baseline-token"
            class_ids = (
                "{27247580-C701-40CD-886D-E618FC8C9FFF}",
                "{E8E73B6B-4CB3-44A4-BE99-4F7BCB96E491}",
                "{2D8F1801-A70D-48F4-B76B-7F5AE022AB54}",
                "{C89FC33C-E60A-4C97-BEF4-ACC5762B6404}",
                "{BD72668E-6BFF-4CD1-8480-D465708B336B}",
                "{20ED4A03-6AFD-4FD9-980B-2F6143AA0892}",
            )
            installed_ax = install / "LAVAudio.ax"
            snapshot_file = baseline / "clsid-0.reg"
            snapshot_file.write_text(
                "Windows Registry Editor Version 5.00\n"
                f'@="{installed_ax}"\n',
                encoding="utf-16",
            )
            snapshot_hash = hashlib.sha256(snapshot_file.read_bytes()).hexdigest().upper()
            snapshots = [
                {
                    "ClassId": class_id,
                    "Existed": index == 0,
                    "File": f"clsid-{index}.reg",
                    "InprocPath": str(installed_ax) if index == 0 else None,
                    "SnapshotHash": snapshot_hash if index == 0 else None,
                }
                for index, class_id in enumerate(class_ids)
            ]
            state = {
                "ProductId": "OpenJOC.LAV.Windows",
                "InstallRoot": str(install),
                "OwnershipToken": token,
                "RegistryBaseline": snapshots,
            }
            (install / "openjoc-install.json").write_text(json.dumps(state), encoding="utf-8")
            (install / ".openjoc-ownership").write_text(token, encoding="ascii")
            environment = os.environ.copy()
            environment.update({
                "OPENJOC_TEST_MODULE": str(TEMPLATE / "scripts" / "OpenJoc.Onboarding.Shell.psm1"),
                "OPENJOC_TEST_PACKAGE": str(TEMPLATE),
                "OPENJOC_TEST_INSTALL": str(install),
                "OPENJOC_TEST_LOG": str(root / "verify.log"),
            })
            command = r"""
Import-Module $env:OPENJOC_TEST_MODULE -Force
$s=[pscustomobject]@{PackageRoot=$env:OPENJOC_TEST_PACKAGE;InstallRoot=$env:OPENJOC_TEST_INSTALL;LogPath=$env:OPENJOC_TEST_LOG;NonInteractive=$true}
$check=(Get-OpenJocVerification $s).Checks | Where-Object Name -eq 'Registry rollback baseline'
if($check.Passed){exit 1}
if($check.Detail -notlike '*points to the current OpenJOC installation*'){exit 2}
"""
            completed = subprocess.run(
                [str(powershell_51()), "-NoProfile", "-Command", command],
                env=environment,
                capture_output=True,
                text=True,
            )
            self.assertEqual(completed.returncode, 0, completed.stdout + completed.stderr)

    def test_noninteractive_missing_package_is_actionable_from_difficult_paths(self) -> None:
        names = (
            "OpenJOC LAV spaces", "OpenJOC LAV (Test)", "OpenJOC 测试",
            "OpenJOC & LAV", "OpenJOC LAV's Test!",
        )
        with tempfile.TemporaryDirectory() as temporary:
            for name in names:
                package = pathlib.Path(temporary) / name
                shutil.copytree(TEMPLATE, package)
                (package / "runtime").mkdir()
                command = (
                    f'{os.environ.get("COMSPEC", "cmd.exe")} /d /c '
                    f'call "{package / "install.bat"}" -NonInteractive'
                )
                completed = subprocess.run(command, cwd=pathlib.Path(temporary), capture_output=True, text=True)
                output = completed.stdout + completed.stderr
                self.assertEqual(completed.returncode, 20, f"{name}: {output}")
                self.assertIn("INSTALLATION FAILED", output)
                self.assertIn("package", output.casefold())
                self.assertIn("Suggested action", output)


if __name__ == "__main__":
    unittest.main()
