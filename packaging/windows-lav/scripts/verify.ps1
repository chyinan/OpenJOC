# SPDX-FileCopyrightText: 2026 OpenJOC contributors
# SPDX-License-Identifier: Apache-2.0
# pattern: Imperative Shell

[CmdletBinding()]
param(
    [switch]$NonInteractive,
    [string]$InstallRoot,
    [string]$LogPath,
    [string]$LauncherStatusPath
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2.0
Import-Module (Join-Path $PSScriptRoot 'OpenJoc.Onboarding.Shell.psm1') -Force
if ([string]::IsNullOrWhiteSpace($InstallRoot)) { $InstallRoot = Get-OpenJocDefaultInstallRoot }
$packageRoot = [IO.Path]::GetFullPath((Split-Path -Parent $PSScriptRoot))
$session = New-OpenJocSession -Operation 'verify' -PackageRoot $packageRoot -InstallRoot $InstallRoot -NonInteractive:$NonInteractive -LogPath $LogPath
if (-not [string]::IsNullOrWhiteSpace($LauncherStatusPath)) { Set-Content -LiteralPath $LauncherStatusPath -Value 'OpenJOC UI initialized' -Encoding ASCII }

try {
    Write-OpenJocHeader 'OpenJOC LAV Verification 0.15.0'
    $verification = Get-OpenJocVerification $session
    Write-OpenJocVerification $session $verification
    if ($verification.Success) {
        exit (Complete-OpenJocSession $session 0 'VERIFICATION PASSED' 'OpenJOC LAV is installed correctly.' $null 'Open PotPlayer and follow POTPLAYER-QUICKSTART.md.' $null)
    }
    exit (Complete-OpenJocSession $session 40 'VERIFICATION FAILED' 'OpenJOC LAV is not installed correctly.' 'Verify installed files and DirectShow registration' 'Run install.bat again. If it still fails, keep the log shown below.' 'One or more verification checks failed; see the FAIL rows above.')
} catch {
    exit (Complete-OpenJocSession $session 40 'VERIFICATION FAILED' 'OpenJOC LAV could not be verified.' 'Verify OpenJOC LAV' 'Run install.bat again. If it still fails, keep the log shown below.' $_.Exception.Message)
}
