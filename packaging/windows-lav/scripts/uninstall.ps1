# SPDX-FileCopyrightText: 2026 OpenJOC contributors
# SPDX-License-Identifier: Apache-2.0
# pattern: Imperative Shell

[CmdletBinding()]
param(
    [switch]$NonInteractive,
    [string]$InstallRoot,
    [switch]$ElevatedChild,
    [string]$LogPath,
    [string]$LauncherStatusPath
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2.0
Import-Module (Join-Path $PSScriptRoot 'OpenJoc.Onboarding.Shell.psm1') -Force
if ([string]::IsNullOrWhiteSpace($InstallRoot)) { $InstallRoot = Get-OpenJocDefaultInstallRoot }
$packageRoot = [IO.Path]::GetFullPath((Split-Path -Parent $PSScriptRoot))
$session = New-OpenJocSession -Operation 'uninstall' -PackageRoot $packageRoot -InstallRoot $InstallRoot -NonInteractive:$NonInteractive -LogPath $LogPath
if (-not [string]::IsNullOrWhiteSpace($LauncherStatusPath)) { Set-Content -LiteralPath $LauncherStatusPath -Value 'OpenJOC UI initialized' -Encoding ASCII }

try {
    Write-OpenJocHeader 'OpenJOC LAV Uninstaller 0.16.0'
    if (-not (Test-OpenJocUninstallRequiresElevation $session.InstallRoot)) {
        exit (Complete-OpenJocSession $session 0 'NOTHING TO REMOVE' 'OpenJOC LAV is not currently installed. Nothing needs to be removed.' $null 'Stock LAV and PotPlayer were not changed.' $null)
    }
    if (-not $NonInteractive -and -not $ElevatedChild) {
        $answer = Read-Host 'Remove OpenJOC LAV? Type Y and press Enter to continue'
        if ($answer -notmatch '^(?i:y|yes)$') {
            exit (Complete-OpenJocSession $session 10 'UNINSTALL CANCELLED' 'No files or registration were changed.' 'Confirm uninstall' 'Run uninstall.bat again when you want to remove OpenJOC LAV.' 'The confirmation was declined.')
        }
    }
    if (-not (Test-OpenJocAdministrator)) {
        if ($ElevatedChild) { throw 'Administrator permission was requested but not granted.' }
        Write-OpenJocStep $session 1 4 'Requesting administrator permission' 'WAITING'
        $arguments = @('-InstallRoot', $session.InstallRoot)
        if ($NonInteractive) { $arguments += '-NonInteractive' }
        $childExit = Invoke-OpenJocElevation $session $PSCommandPath $arguments
        if ($childExit -eq 10) {
            exit (Complete-OpenJocSession $session 10 'UNINSTALL CANCELLED' 'Administrator permission is required to unregister the DirectShow filter.' 'Request administrator permission' 'Run uninstall.bat again and select Yes at the Windows prompt.' 'The UAC prompt was cancelled.')
        }
        exit $childExit
    }
    Write-OpenJocStep $session 1 4 'Administrator permission' 'OK'
    Write-OpenJocStep $session 2 4 'Unregistering OpenJOC LAV' 'RUNNING'
    $result = Invoke-OpenJocUninstallTransaction $session
    if ($result.ExitCode -ne 0) {
        exit (Complete-OpenJocSession $session 50 'UNINSTALL FAILED' 'OpenJOC LAV could not be removed safely.' 'Unregister and remove OpenJOC-owned files' 'Close PotPlayer, then run uninstall.bat again. If it still fails, keep the log shown below.' $result.Detail)
    }
    Write-OpenJocStep $session 2 4 'Unregistering OpenJOC LAV' 'OK'
    Write-OpenJocStep $session 3 4 'Removing OpenJOC-owned files' 'OK'
    Write-OpenJocStep $session 4 4 'Confirming stock LAV isolation' 'OK'
    exit (Complete-OpenJocSession $session 0 'UNINSTALL SUCCESSFUL' $result.Detail $null 'Stock LAV, K-Lite, PotPlayer, and player settings were not removed.' $null)
} catch {
    exit (Complete-OpenJocSession $session 50 'UNINSTALL FAILED' 'An unexpected uninstall error occurred.' 'Remove OpenJOC LAV' 'Close PotPlayer, then run uninstall.bat again. If it still fails, keep the log shown below.' $_.Exception.Message)
}
