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
$module = Join-Path $PSScriptRoot 'OpenJoc.Onboarding.Shell.psm1'
Import-Module $module -Force
if ([string]::IsNullOrWhiteSpace($InstallRoot)) { $InstallRoot = Get-OpenJocDefaultInstallRoot }
$packageRoot = [IO.Path]::GetFullPath((Split-Path -Parent $PSScriptRoot))
$session = New-OpenJocSession -Operation 'install' -PackageRoot $packageRoot -InstallRoot $InstallRoot -NonInteractive:$NonInteractive -LogPath $LogPath
if (-not [string]::IsNullOrWhiteSpace($LauncherStatusPath)) { Set-Content -LiteralPath $LauncherStatusPath -Value 'OpenJOC UI initialized' -Encoding ASCII }

try {
    Write-OpenJocHeader 'OpenJOC LAV Installer 0.16.0'
    Write-OpenJocStep $session 1 6 'Checking Windows architecture' 'RUNNING'
    $preflight = Test-OpenJocPackage $session
    if (-not $preflight.Success) {
        Write-OpenJocStep $session 1 6 'Checking package files' 'FAILED'
        exit (Complete-OpenJocSession $session 20 'INSTALLATION FAILED' 'The extracted package is incomplete or unsupported.' 'Check package files' 'Extract a fresh copy of the complete v0.16.0 ZIP, then run install.bat again.' $preflight.Detail)
    }
    Write-OpenJocStep $session 1 6 'Checking Windows architecture and package files' 'OK'

    if (-not (Test-OpenJocAdministrator)) {
        if ($ElevatedChild) { throw 'Administrator permission was requested but not granted.' }
        Write-OpenJocStep $session 2 6 'Requesting administrator permission' 'WAITING'
        $arguments = @('-InstallRoot', $session.InstallRoot)
        if ($NonInteractive) { $arguments += '-NonInteractive' }
        $childExit = Invoke-OpenJocElevation $session $PSCommandPath $arguments
        if ($childExit -eq 10) {
            exit (Complete-OpenJocSession $session 10 'INSTALLATION CANCELLED' 'Administrator permission is required to register the DirectShow filter.' 'Request administrator permission' 'Run install.bat again and select Yes at the Windows prompt.' 'The UAC prompt was cancelled.')
        }
        exit $childExit
    }

    Write-OpenJocStep $session 2 6 'Administrator permission' 'OK'
    Write-OpenJocStep $session 3 6 'Preparing the installation directory' 'RUNNING'
    $result = Invoke-OpenJocInstallTransaction $session
    if ($result.ExitCode -ne 0) {
        exit (Complete-OpenJocSession $session $result.ExitCode 'INSTALLATION FAILED' 'OpenJOC LAV could not be installed safely.' 'Install, register, and verify OpenJOC LAV' 'Close PotPlayer, then run install.bat again. If it still fails, keep the log shown below.' ("{0} Rollback: {1}." -f $result.Detail, $result.Rollback))
    }
    Write-OpenJocStep $session 3 6 'Preparing the installation directory' 'OK'
    Write-OpenJocStep $session 4 6 'Registering OpenJOC LAV Audio Decoder' 'OK'
    Write-OpenJocStep $session 5 6 'Checking runtime dependencies' 'OK'
    Write-OpenJocStep $session 6 6 'Verifying DirectShow registration and stock isolation' 'OK'
    Write-OpenJocVerification $session $result.Verification
    if (-not [string]::IsNullOrWhiteSpace($result.Warning)) { Write-Warning $result.Warning }
    exit (Complete-OpenJocSession $session 0 'INSTALLATION SUCCESSFUL' 'OpenJOC LAV Audio Decoder is installed correctly.' $null 'Next step: open PotPlayer and follow POTPLAYER-QUICKSTART.md.' $null)
} catch {
    exit (Complete-OpenJocSession $session 30 'INSTALLATION FAILED' 'An unexpected installation error occurred.' 'Install OpenJOC LAV' 'Run install.bat again. If it still fails, keep the log shown below.' $_.Exception.Message)
}
