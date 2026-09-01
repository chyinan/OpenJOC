# SPDX-FileCopyrightText: 2026 OpenJOC contributors
# SPDX-License-Identifier: Apache-2.0
# pattern: Imperative Shell

Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'

Import-Module (Join-Path $PSScriptRoot 'OpenJoc.Onboarding.Core.psm1') -Force

if (-not ('OpenJocNativeLibrary' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class OpenJocNativeLibrary {
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    public static extern IntPtr LoadLibraryEx(string fileName, IntPtr file, uint flags);
    [DllImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static extern bool FreeLibrary(IntPtr module);
}
'@
}

$script:ProductId = 'OpenJOC.LAV.Windows'
$script:Version = '0.15.0'
$script:OpenJocClsid = '{27247580-C701-40CD-886D-E618FC8C9FFF}'
$script:StockClsid = '{E8E73B6B-4CB3-44A4-BE99-4F7BCB96E491}'
$script:PackageRuntimeRoot = Join-Path (Split-Path -Parent $PSScriptRoot) 'runtime'
$script:PackageRuntimeProfile = Get-OpenJocRuntimeProfile $script:PackageRuntimeRoot
if ($null -ne $script:PackageRuntimeProfile) {
    $script:Version = $script:PackageRuntimeProfile.Version
}

function Get-OpenJocDefaultInstallRoot {
    $programFiles = [Environment]::GetFolderPath([Environment+SpecialFolder]::ProgramFiles)
    Join-Path $programFiles 'OpenJOC\LAV\0.15.0'
}

function New-OpenJocSession {
    param(
        [Parameter(Mandatory = $true)][string]$Operation,
        [Parameter(Mandatory = $true)][string]$PackageRoot,
        [Parameter(Mandatory = $true)][string]$InstallRoot,
        [switch]$NonInteractive,
        [string]$LogPath
    )
    $initialLoggingFailure = $null
    if ([string]::IsNullOrWhiteSpace($LogPath)) {
        $logRoot = Join-Path $env:LOCALAPPDATA 'OpenJOC\Logs'
        try { New-Item -ItemType Directory -Path $logRoot -Force | Out-Null }
        catch { $initialLoggingFailure = $_.Exception.Message }
        $stamp = Get-Date -Format 'yyyyMMdd-HHmmss-fff'
        $LogPath = Join-Path $logRoot ("OpenJOC-LAV-{0}-{1}.log" -f $Operation.ToLowerInvariant(), $stamp)
    }
    $session = [pscustomobject]@{
        Operation = $Operation
        PackageRoot = [IO.Path]::GetFullPath($PackageRoot)
        InstallRoot = [IO.Path]::GetFullPath($InstallRoot)
        NonInteractive = [bool]$NonInteractive
        LogPath = [IO.Path]::GetFullPath($LogPath)
        LoggingFailure = $initialLoggingFailure
    }
    Write-OpenJocLog $session 'INFO' ("start version={0} powershell={1} windows={2} architecture={3}" -f $script:Version, $PSVersionTable.PSVersion, [Environment]::OSVersion.VersionString, $env:PROCESSOR_ARCHITECTURE)
    return $session
}

function Write-OpenJocLog {
    param($Session, [string]$Level, [string]$Message)
    $line = "{0} [{1}] {2}" -f (Get-Date -Format 'yyyy-MM-ddTHH:mm:ss.fffK'), $Level, ($Message -replace '[\r\n]+', ' ')
    try {
        Add-Content -LiteralPath $Session.LogPath -Value $line -Encoding UTF8
    } catch {
        if ($Session.PSObject.Properties.Name -contains 'LoggingFailure') {
            if ([string]::IsNullOrWhiteSpace($Session.LoggingFailure)) { $Session.LoggingFailure = $_.Exception.Message }
        }
    }
}

function Write-OpenJocHeader {
    param([string]$Title)
    Write-Host ''
    Write-Host '============================================================'
    Write-Host ("             {0}" -f $Title)
    Write-Host '============================================================'
    Write-Host ''
}

function Write-OpenJocStep {
    param($Session, [int]$Number, [int]$Total, [string]$Text, [string]$State)
    Write-Host ("[{0}/{1}] {2}... {3}" -f $Number, $Total, $Text, $State)
    Write-OpenJocLog $Session 'STEP' ("{0}/{1} {2}: {3}" -f $Number, $Total, $Text, $State)
}

function Complete-OpenJocSession {
    param(
        $Session,
        [int]$ExitCode,
        [string]$Heading,
        [string]$Message,
        [string]$Step,
        [string]$SuggestedAction,
        [string]$TechnicalDetail
    )
    Write-Host ''
    Write-Host '------------------------------------------------------------'
    Write-Host $Heading
    Write-Host $Message
    Write-Host '------------------------------------------------------------'
    if (-not [string]::IsNullOrWhiteSpace($Step)) { Write-Host ("Step: {0}" -f $Step) }
    if (-not [string]::IsNullOrWhiteSpace($SuggestedAction)) { Write-Host ("Suggested action: {0}" -f $SuggestedAction) }
    if (-not [string]::IsNullOrWhiteSpace($TechnicalDetail)) { Write-Host ("Technical detail: {0}" -f $TechnicalDetail) }
    Write-OpenJocLog $Session ($(if ($ExitCode -eq 0) { 'INFO' } else { 'ERROR' })) ("finish exit={0} heading={1} detail={2}" -f $ExitCode, $Heading, $TechnicalDetail)
    if ($ExitCode -ne 0) {
        if (($Session.PSObject.Properties.Name -contains 'LoggingFailure') -and -not [string]::IsNullOrWhiteSpace($Session.LoggingFailure)) {
            Write-Host 'Log: unavailable (the diagnostic log could not be written).'
        } else {
            Write-Host ("Log: {0}" -f $Session.LogPath)
        }
    }
    if (-not $Session.NonInteractive) {
        Write-Host ''
        [void](Read-Host 'Press Enter to close')
    }
    return $ExitCode
}

function Test-OpenJocAdministrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($identity)
    $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Invoke-OpenJocNative {
    param($Session, [string]$FilePath, [string[]]$Arguments)
    $quoted = @($Arguments | ForEach-Object { ConvertTo-OpenJocCommandLineArgument $_ })
    Write-OpenJocLog $Session 'INFO' ("native file={0} argument_count={1}" -f $FilePath, $Arguments.Count)
    $info = New-Object System.Diagnostics.ProcessStartInfo
    $info.FileName = $FilePath
    $info.Arguments = [string]::Join(' ', $quoted)
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    $process = [Diagnostics.Process]::Start($info)
    $standardOutput = $process.StandardOutput.ReadToEnd()
    $standardError = $process.StandardError.ReadToEnd()
    $process.WaitForExit()
    if (-not [string]::IsNullOrWhiteSpace($standardOutput)) { Write-OpenJocLog $Session 'NATIVE' $standardOutput.Trim() }
    if (-not [string]::IsNullOrWhiteSpace($standardError)) { Write-OpenJocLog $Session 'NATIVE_ERROR' $standardError.Trim() }
    Write-OpenJocLog $Session 'INFO' ("native exit={0} file={1}" -f $process.ExitCode, $FilePath)
    return $process.ExitCode
}

function Invoke-OpenJocElevation {
    param($Session, [string]$ScriptPath, [string[]]$Arguments)
    $hostPath = Join-Path $PSHOME 'powershell.exe'
    if (-not (Test-Path -LiteralPath $hostPath)) { $hostPath = (Get-Process -Id $PID).Path }
    $allArguments = @('-NoLogo', '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $ScriptPath) + $Arguments + @('-ElevatedChild', '-LogPath', $Session.LogPath)
    $quoted = @($allArguments | ForEach-Object { ConvertTo-OpenJocCommandLineArgument $_ })
    $info = New-Object System.Diagnostics.ProcessStartInfo
    $info.FileName = $hostPath
    $info.Arguments = [string]::Join(' ', $quoted)
    $info.UseShellExecute = $true
    $info.Verb = 'runas'
    try {
        Write-OpenJocLog $Session 'INFO' 'requesting administrator permission'
        $process = [Diagnostics.Process]::Start($info)
        $process.WaitForExit()
        Write-OpenJocLog $Session 'INFO' ("elevated child exit={0}" -f $process.ExitCode)
        return $process.ExitCode
    } catch [ComponentModel.Win32Exception] {
        if ($_.Exception.NativeErrorCode -eq 1223) {
            Write-OpenJocLog $Session 'WARN' 'UAC cancelled by user'
            return 10
        }
        throw
    }
}

function Test-OpenJocPackage {
    param($Session)
    if (-not [Environment]::Is64BitOperatingSystem -or -not [Environment]::Is64BitProcess) {
        return [pscustomobject]@{ Success = $false; Detail = 'This package requires 64-bit Windows and a 64-bit PowerShell host.' }
    }
    $runtime = Join-Path $Session.PackageRoot 'runtime'
    Test-OpenJocRuntimePayload $runtime
}

function Test-OpenJocPeX64 {
    param([string]$Path)
    $stream = $null
    $reader = $null
    try {
        $stream = [IO.File]::Open($Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
        $reader = New-Object IO.BinaryReader($stream)
        if ($stream.Length -lt 64 -or $reader.ReadUInt16() -ne 0x5A4D) { return $false }
        $stream.Position = 0x3C
        $peOffset = $reader.ReadInt32()
        if ($peOffset -lt 64 -or ($peOffset + 24) -gt $stream.Length) { return $false }
        $stream.Position = $peOffset
        if ($reader.ReadUInt32() -ne 0x00004550) { return $false }
        return $reader.ReadUInt16() -eq 0x8664
    } catch {
        return $false
    } finally {
        if ($reader) { $reader.Dispose() } elseif ($stream) { $stream.Dispose() }
    }
}

function Test-OpenJocRuntimePayload {
    param([string]$RuntimeRoot)
    $missing = @()
    $requiredFiles = @(Get-OpenJocRequiredRuntimeFiles $RuntimeRoot)
    foreach ($name in $requiredFiles) {
        if (-not (Test-Path -LiteralPath (Join-Path $RuntimeRoot $name) -PathType Leaf)) { $missing += $name }
    }
    if ($missing.Count -gt 0) {
        return [pscustomobject]@{ Success = $false; Detail = ("Package runtime is incomplete. Missing: {0}" -f ($missing -join ', ')) }
    }
    $invalid = @()
    foreach ($name in $requiredFiles) {
        if ([IO.Path]::GetExtension($name) -notin @('.dll', '.ax')) { continue }
        if (-not (Test-OpenJocPeX64 (Join-Path $RuntimeRoot $name))) { $invalid += $name }
    }
    if ($invalid.Count -gt 0) {
        return [pscustomobject]@{ Success = $false; Detail = ("Package contains an invalid, corrupted, or non-x64 runtime file: {0}" -f ($invalid -join ', ')) }
    }
    foreach ($name in $requiredFiles) {
        if ([IO.Path]::GetExtension($name) -notin @('.dll', '.ax')) { continue }
        $path = Join-Path $RuntimeRoot $name
        $module = [OpenJocNativeLibrary]::LoadLibraryEx($path, [IntPtr]::Zero, 0x00001100)
        if ($module -eq [IntPtr]::Zero) {
            $errorCode = [Runtime.InteropServices.Marshal]::GetLastWin32Error()
            return [pscustomobject]@{ Success = $false; Detail = "Runtime loadability failed for $name (Win32 error $errorCode)." }
        }
        [void][OpenJocNativeLibrary]::FreeLibrary($module)
    }
    [pscustomobject]@{ Success = $true; Detail = 'All required x64 runtime files and transitive dependencies are loadable.' }
}

function Get-OpenJocRegistryKeyName {
    param([string]$ClassId)
    "HKLM\Software\Classes\CLSID\$ClassId"
}

function Get-OpenJocRegistryProviderPath {
    param([string]$ClassId)
    "Registry::HKEY_LOCAL_MACHINE\SOFTWARE\Classes\CLSID\$ClassId"
}

function Get-OpenJocInprocPath {
    param([string]$ClassId)
    $path = "Registry::HKEY_LOCAL_MACHINE\SOFTWARE\Classes\CLSID\$ClassId\InprocServer32"
    try { return (Get-ItemProperty -LiteralPath $path -Name '(default)' -ErrorAction Stop).'(default)' } catch { return $null }
}

function Get-OpenJocFileSha256 {
    param([string]$Path)
    $stream = $null
    $algorithm = $null
    try {
        $stream = [IO.File]::Open($Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
        $algorithm = [Security.Cryptography.SHA256]::Create()
        $bytes = $algorithm.ComputeHash($stream)
        return [BitConverter]::ToString($bytes).Replace('-', '')
    } finally {
        if ($algorithm) { $algorithm.Dispose() }
        if ($stream) { $stream.Dispose() }
    }
}

function Test-OpenJocUninstallRequiresElevation {
    param([string]$InstallRoot)
    $expectedAx = Join-Path $InstallRoot 'LAVAudio.ax'
    (Test-Path -LiteralPath $InstallRoot) -or (Test-OpenJocSamePath (Get-OpenJocInprocPath $script:OpenJocClsid) $expectedAx)
}

function Save-OpenJocRegistrySnapshot {
    param($Session, [string[]]$ClassIds, [string]$Directory, [string]$ExcludeOwnedAx)
    New-Item -ItemType Directory -Path $Directory -Force | Out-Null
    $items = @()
    $reg = Join-Path $env:WINDIR 'System32\reg.exe'
    $index = 0
    foreach ($classId in $ClassIds) {
        $current = Get-OpenJocInprocPath $classId
        $include = -not ($ExcludeOwnedAx -and (Test-OpenJocSamePath $current $ExcludeOwnedAx))
        $exists = Test-Path -LiteralPath (Get-OpenJocRegistryProviderPath $classId)
        $file = "clsid-{0}.reg" -f $index
        $snapshotHash = $null
        if ($exists -and $include) {
            $exit = Invoke-OpenJocNative $Session $reg @('export', (Get-OpenJocRegistryKeyName $classId), (Join-Path $Directory $file), '/y')
            if ($exit -ne 0) { throw "failed to snapshot registry class $classId (reg.exe exit $exit)" }
            $snapshotHash = Get-OpenJocFileSha256 (Join-Path $Directory $file)
        }
        $items += [pscustomobject]@{
            ClassId = $classId
            Existed = [bool]($exists -and $include)
            File = $file
            InprocPath = $(if ($include) { $current } else { $null })
            SnapshotHash = $snapshotHash
        }
        $index++
    }
    $items | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $Directory 'snapshot.json') -Encoding UTF8
    return $items
}

function Assert-OpenJocRegistrySnapshotFiles {
    param($Items, [string]$Directory, [string[]]$ExpectedClassIds)
    $snapshotItems = @($Items)
    if ($ExpectedClassIds) {
        if ($snapshotItems.Count -ne $ExpectedClassIds.Count) {
            throw 'registry snapshot does not contain the expected number of classes'
        }
        foreach ($classId in $ExpectedClassIds) {
            if (@($snapshotItems | Where-Object { $_.ClassId -ieq $classId }).Count -ne 1) {
                throw "registry snapshot does not contain exactly one entry for class $classId"
            }
        }
    }
    foreach ($item in $snapshotItems) {
        if ($item.File -notmatch '^clsid-[0-9]+\.reg$') {
            throw "registry snapshot contains an unsafe file name for class $($item.ClassId)"
        }
        if (-not $item.Existed) { continue }
        $snapshotFile = Join-Path $Directory $item.File
        if (-not (Test-Path -LiteralPath $snapshotFile -PathType Leaf)) {
            throw "refusing registry restore because snapshot file is missing: $($item.File)"
        }
        $actualHash = Get-OpenJocFileSha256 $snapshotFile
        if ([string]::IsNullOrWhiteSpace($item.SnapshotHash) -or $actualHash -ne $item.SnapshotHash) {
            throw "refusing registry restore because snapshot integrity failed: $($item.File)"
        }
    }
}

function Assert-OpenJocRollbackBaseline {
    param($Items, [string]$Directory, [string[]]$ExpectedClassIds, [string]$InstallRoot)
    Assert-OpenJocRegistrySnapshotFiles $Items $Directory $ExpectedClassIds
    $main = @($Items | Where-Object { $_.ClassId -ieq $script:OpenJocClsid })
    if ($main.Count -ne 1) {
        throw 'registry rollback baseline does not contain exactly one OpenJOC main-class entry'
    }
    if ($main[0].Existed) {
        if ([string]::IsNullOrWhiteSpace($main[0].InprocPath)) {
            throw 'registry rollback baseline is missing the original OpenJOC main-class path'
        }
        $installedAx = Join-Path $InstallRoot 'LAVAudio.ax'
        if (Test-OpenJocSamePath $main[0].InprocPath $installedAx) {
            throw 'registry rollback baseline points to the current OpenJOC installation and would restore a path that uninstall removes'
        }
    }
}

function Copy-OpenJocRegistrySnapshot {
    param($Items, [string]$SourceDirectory, [string]$DestinationDirectory, [string[]]$ExpectedClassIds)
    Assert-OpenJocRegistrySnapshotFiles $Items $SourceDirectory $ExpectedClassIds
    if (Test-Path -LiteralPath $DestinationDirectory) { throw "refusing existing snapshot destination: $DestinationDirectory" }
    New-Item -ItemType Directory -Path $DestinationDirectory | Out-Null
    foreach ($item in @($Items)) {
        if ($item.Existed) {
            Copy-Item -LiteralPath (Join-Path $SourceDirectory $item.File) -Destination (Join-Path $DestinationDirectory $item.File)
        }
    }
    @($Items) | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $DestinationDirectory 'snapshot.json') -Encoding UTF8
}

function Restore-OpenJocRegistrySnapshot {
    param($Session, $Items, [string]$Directory)
    $reg = Join-Path $env:WINDIR 'System32\reg.exe'
    Assert-OpenJocRegistrySnapshotFiles $Items $Directory $null
    foreach ($item in $Items) {
        $providerPath = Get-OpenJocRegistryProviderPath $item.ClassId
        if (Test-Path -LiteralPath $providerPath) {
            $deleteExit = Invoke-OpenJocNative $Session $reg @('delete', (Get-OpenJocRegistryKeyName $item.ClassId), '/f')
            if ($deleteExit -ne 0) { throw "failed to clear registry class $($item.ClassId) (reg.exe exit $deleteExit)" }
            if (Test-Path -LiteralPath $providerPath) { throw "registry class still exists after deletion: $($item.ClassId)" }
        }
        if ($item.Existed) {
            $importExit = Invoke-OpenJocNative $Session $reg @('import', (Join-Path $Directory $item.File))
            if ($importExit -ne 0) { throw "failed to restore registry class $($item.ClassId) (reg.exe exit $importExit)" }
        }
    }
    $exact = Test-OpenJocRegistrySnapshotExact $Session $Items $Directory
    if (-not $exact.Success) { throw $exact.Detail }
}

function Test-OpenJocRegistrySnapshotExact {
    param($Session, $Items, [string]$Directory)
    $reg = Join-Path $env:WINDIR 'System32\reg.exe'
    foreach ($item in @($Items)) {
        $providerPath = Get-OpenJocRegistryProviderPath $item.ClassId
        if (-not $item.Existed) {
            if (Test-Path -LiteralPath $providerPath) {
                return [pscustomobject]@{ Success = $false; Detail = "Registry class $($item.ClassId) should be absent but exists." }
            }
            continue
        }
        if (-not (Test-Path -LiteralPath $providerPath)) {
            return [pscustomobject]@{ Success = $false; Detail = "Registry class $($item.ClassId) was removed or is unreadable." }
        }
        $verificationFile = Join-Path ([IO.Path]::GetTempPath()) ("OpenJOC-registry-verify-{0}.reg" -f [Guid]::NewGuid().ToString('N'))
        try {
            $exit = Invoke-OpenJocNative $Session $reg @('export', (Get-OpenJocRegistryKeyName $item.ClassId), $verificationFile, '/y')
            if ($exit -ne 0) { return [pscustomobject]@{ Success = $false; Detail = "Could not read registry class $($item.ClassId) (reg.exe exit $exit)." } }
            $actualHash = Get-OpenJocFileSha256 $verificationFile
            if ($actualHash -ne $item.SnapshotHash) {
                return [pscustomobject]@{ Success = $false; Detail = "Registry class $($item.ClassId) differs from its saved baseline." }
            }
        } finally {
            if (Test-Path -LiteralPath $verificationFile) { Remove-Item -LiteralPath $verificationFile -Force }
        }
    }
    [pscustomobject]@{ Success = $true; Detail = 'Registry snapshot matches exactly.' }
}

function Test-OpenJocOwnedInstall {
    param([string]$InstallRoot)
    if (-not (Test-OpenJocSafeOwnedRoot $InstallRoot)) { return $false }
    try {
        $rootItem = Get-Item -LiteralPath $InstallRoot -Force -ErrorAction Stop
        if (($rootItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { return $false }
    } catch { return $false }
    $statePath = Join-Path $InstallRoot 'openjoc-install.json'
    $ownershipPath = Join-Path $InstallRoot '.openjoc-ownership'
    if (-not (Test-Path -LiteralPath $statePath -PathType Leaf) -or -not (Test-Path -LiteralPath $ownershipPath -PathType Leaf)) { return $false }
    try { $state = Get-Content -Raw -LiteralPath $statePath | ConvertFrom-Json } catch { return $false }
    try { $ownershipToken = (Get-Content -Raw -LiteralPath $ownershipPath).Trim() } catch { return $false }
    return (
        $state.ProductId -eq $script:ProductId -and
        (Test-OpenJocSamePath $state.InstallRoot $InstallRoot) -and
        -not [string]::IsNullOrWhiteSpace($state.OwnershipToken) -and
        $state.OwnershipToken -eq $ownershipToken
    )
}

function Remove-OpenJocOwnedDirectory {
    param($Session, [string]$Path, [switch]$Transient)
    if (-not (Test-Path -LiteralPath $Path)) { return }
    if (-not (Test-OpenJocSafeOwnedRoot $Path)) { throw "refusing unsafe removal path: $Path" }
    $pathItem = Get-Item -LiteralPath $Path -Force
    if (($pathItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "refusing to recursively remove a reparse point: $Path" }
    $owned = if ($Transient) {
        $marker = Join-Path $Path '.openjoc-transient'
        (Test-Path -LiteralPath $marker -PathType Leaf) -and ((Get-Content -Raw -LiteralPath $marker).Trim() -eq $script:ProductId)
    } else { Test-OpenJocOwnedInstall $Path }
    if (-not $owned) { throw "refusing to remove a directory not proven to be OpenJOC-owned: $Path" }
    Write-OpenJocLog $Session 'INFO' ("remove owned directory={0}" -f $Path)
    Remove-Item -LiteralPath $Path -Recurse -Force
}

function Remove-OpenJocCommittedRollbackSnapshot {
    param($Session, $Items, [string]$Directory)
    if (-not (Test-OpenJocOwnedInstall $Session.InstallRoot)) {
        throw 'refusing rollback-snapshot cleanup because installation ownership is invalid'
    }
    $expectedDirectory = Join-Path $Session.InstallRoot 'state\registry-for-rollback'
    if (-not (Test-OpenJocSamePath $Directory $expectedDirectory)) {
        throw "refusing unexpected rollback-snapshot cleanup path: $Directory"
    }
    if (-not (Test-Path -LiteralPath $Directory)) { return }
    $directoryItem = Get-Item -LiteralPath $Directory -Force
    if (($directoryItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "refusing rollback-snapshot reparse point: $Directory"
    }
    $allowedNames = @('snapshot.json')
    foreach ($item in @($Items)) {
        if ($item.File -notmatch '^clsid-[0-9]+\.reg$') {
            throw "rollback snapshot contains an unsafe file name: $($item.File)"
        }
        if ($item.Existed) { $allowedNames += $item.File }
    }
    foreach ($child in Get-ChildItem -LiteralPath $Directory -Force) {
        if ($child.PSIsContainer -or $allowedNames -notcontains $child.Name) {
            throw "refusing rollback-snapshot cleanup because it contains an unexpected item: $($child.Name)"
        }
    }
    foreach ($name in $allowedNames | Select-Object -Unique) {
        $path = Join-Path $Directory $name
        if (Test-Path -LiteralPath $path -PathType Leaf) { Remove-Item -LiteralPath $path -Force }
    }
    if (@(Get-ChildItem -LiteralPath $Directory -Force).Count -ne 0) {
        throw 'refusing to remove a non-empty rollback-snapshot directory'
    }
    Remove-Item -LiteralPath $Directory -Force
}

function New-OpenJocTransientDirectory {
    param([string]$Path)
    if (Test-Path -LiteralPath $Path) { throw "refusing to claim an existing transient directory: $Path" }
    New-Item -ItemType Directory -Path $Path | Out-Null
    $created = Get-Item -LiteralPath $Path -Force
    if (($created.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "refusing transient reparse point: $Path" }
    Set-Content -LiteralPath (Join-Path $Path '.openjoc-transient') -Value $script:ProductId -Encoding ASCII
}

function Get-OpenJocVerification {
    param($Session)
    $checks = @()
    $missing = @()
    foreach ($name in Get-OpenJocRequiredRuntimeFiles $Session.InstallRoot) {
        if (-not (Test-Path -LiteralPath (Join-Path $Session.InstallRoot $name) -PathType Leaf)) { $missing += $name }
    }
    $checks += [pscustomobject]@{ Name = 'Installed files'; Passed = ($missing.Count -eq 0); Detail = $(if ($missing.Count) { 'Missing: ' + ($missing -join ', ') } else { 'All required runtime files are present.' }) }
    $runtimePayload = Test-OpenJocRuntimePayload $Session.InstallRoot
    $checks += [pscustomobject]@{ Name = 'Runtime loadability'; Passed = $runtimePayload.Success; Detail = $runtimePayload.Detail }
    $owned = Test-OpenJocOwnedInstall $Session.InstallRoot
    $checks += [pscustomobject]@{ Name = 'OpenJOC installation state'; Passed = $owned; Detail = $(if ($owned) { 'Ownership state is valid.' } else { 'The ownership state is missing or invalid.' }) }
    $baselinePass = $false
    $baselineDetail = 'Cannot verify rollback readiness because the OpenJOC installation state is absent or invalid.'
    $sharedPass = $false
    $sharedDetail = 'Cannot verify shared LAV isolation because the OpenJOC installation state is absent or invalid.'
    $stockPass = $false
    $stockDetail = 'Cannot verify stock LAV preservation because the OpenJOC installation state is absent or invalid.'
    if ($owned) {
        try {
            $state = Get-Content -Raw -LiteralPath (Join-Path $Session.InstallRoot 'openjoc-install.json') | ConvertFrom-Json
            $classIds = Get-OpenJocClassIds
            $allIds = @($classIds.Values)
            $baselineDirectory = Join-Path $Session.InstallRoot 'state\registry-before-install'
            $completeBaseline = @($state.RegistryBaseline)
            Assert-OpenJocRollbackBaseline $completeBaseline $baselineDirectory $allIds $Session.InstallRoot
            $baselinePass = $true
            $baselineDetail = 'The complete pre-install registry baseline is present and hash-valid.'
        } catch {
            $baselinePass = $false
            $baselineDetail = "The saved pre-install registry baseline is not rollback-ready: $($_.Exception.Message)"
            $sharedDetail = "The saved shared-class baseline could not be verified because the complete rollback baseline is invalid: $($_.Exception.Message)"
            $stockDetail = "The saved stock LAV baseline could not be verified because the complete rollback baseline is invalid: $($_.Exception.Message)"
        }
        if ($baselinePass) {
            try {
                $sharedIds = @($classIds.AudioSettings, $classIds.AudioMixing, $classIds.AudioFormats, $classIds.AudioStatus)
                $sharedBaseline = @($state.RegistryBaseline | Where-Object { $sharedIds -contains $_.ClassId })
                $sharedExact = Test-OpenJocRegistrySnapshotExact $Session $sharedBaseline $baselineDirectory
                $sharedPass = $sharedExact.Success
                $sharedDetail = $sharedExact.Detail
            } catch {
                $sharedPass = $false
                $sharedDetail = "The saved shared-class baseline could not be verified: $($_.Exception.Message)"
            }
            try {
                $stockBaseline = @($state.RegistryBaseline | Where-Object { $_.ClassId -eq $classIds.StockLavAudio })
                $stockExact = Test-OpenJocRegistrySnapshotExact $Session $stockBaseline $baselineDirectory
                $stockPass = $stockExact.Success
                $stockDetail = $stockExact.Detail
            } catch {
                $stockPass = $false
                $stockDetail = "The saved stock LAV baseline could not be verified: $($_.Exception.Message)"
            }
        }
    }
    $checks += [pscustomobject]@{ Name = 'Registry rollback baseline'; Passed = $baselinePass; Detail = $baselineDetail }
    $checks += [pscustomobject]@{ Name = 'Shared LAV class isolation'; Passed = $sharedPass; Detail = $sharedDetail }
    $checks += [pscustomobject]@{ Name = 'Stock LAV registration'; Passed = $stockPass; Detail = $stockDetail }
    $expectedAx = Join-Path $Session.InstallRoot 'LAVAudio.ax'
    $registered = Get-OpenJocInprocPath $script:OpenJocClsid
    $registrationPass = Test-OpenJocSamePath $registered $expectedAx
    $checks += [pscustomobject]@{ Name = 'DirectShow registration'; Passed = $registrationPass; Detail = $(if ($registrationPass) { 'The OpenJOC CLSID resolves to the installed filter.' } else { "Expected '$expectedAx'; found '$registered'." }) }
    $capiPass = Test-Path -LiteralPath (Join-Path $Session.InstallRoot 'openjoc_capi.dll') -PathType Leaf
    $checks += [pscustomobject]@{ Name = 'OpenJOC C API runtime'; Passed = $capiPass; Detail = $(if ($capiPass) { 'openjoc_capi.dll is present.' } else { 'openjoc_capi.dll is missing.' }) }
    $packageAx = Join-Path (Join-Path $Session.PackageRoot 'runtime') 'LAVAudio.ax'
    $stablePass = -not (Test-OpenJocSamePath $registered $packageAx)
    $checks += [pscustomobject]@{ Name = 'Stable installed path'; Passed = $stablePass; Detail = $(if ($stablePass) { 'Registration does not point into the extracted ZIP.' } else { 'Registration points into the extracted package.' }) }
    [pscustomobject]@{ Success = -not ($checks.Passed -contains $false); Checks = $checks }
}

function Write-OpenJocVerification {
    param($Session, $Verification)
    foreach ($check in $Verification.Checks) {
        $state = if ($check.Passed) { 'PASS' } else { 'FAIL' }
        Write-Host ("{0,-38} {1}" -f ($check.Name + '...'), $state)
        Write-OpenJocLog $Session $state ("{0}: {1}" -f $check.Name, $check.Detail)
        if (-not $check.Passed) { Write-Host ("  {0}" -f $check.Detail) }
    }
}

function Invoke-OpenJocInstallTransaction {
    param($Session)
    $classIds = Get-OpenJocClassIds
    $allIds = @($classIds.Values)
    $sharedIds = @($classIds.AudioSettings, $classIds.AudioMixing, $classIds.AudioFormats, $classIds.AudioStatus)
    $parent = Split-Path -Parent $Session.InstallRoot
    $transactionId = [Guid]::NewGuid().ToString('N')
    $stage = "$($Session.InstallRoot).openjoc-stage-$transactionId"
    $backup = "$($Session.InstallRoot).openjoc-backup-$transactionId"
    $rollbackSnapshot = $null
    $rollbackDirectory = $null
    $baselineSnapshot = $null
    $baselineDirectory = $null
    $registrationAttempted = $false
    $existingInstallMoved = $false
    $newInstallPlaced = $false
    $existingMarkerAdded = $false
    $committed = $false
    $verification = $null
    try {
        New-Item -ItemType Directory -Path $parent -Force | Out-Null
        New-OpenJocTransientDirectory $stage
        Get-ChildItem -LiteralPath (Join-Path $Session.PackageRoot 'runtime') -File | Copy-Item -Destination $stage -Force
        $rollbackDirectory = Join-Path $stage 'state\registry-for-rollback'
        $rollbackSnapshot = Save-OpenJocRegistrySnapshot $Session $allIds $rollbackDirectory $null
        $baselineDirectory = Join-Path $stage 'state\registry-before-install'
        if (Test-Path -LiteralPath $Session.InstallRoot) {
            if (-not (Test-OpenJocOwnedInstall $Session.InstallRoot)) { throw 'The destination exists but is not a verified OpenJOC installation.' }
            $existingState = Get-Content -Raw -LiteralPath (Join-Path $Session.InstallRoot 'openjoc-install.json') | ConvertFrom-Json
            $baselineSnapshot = @($existingState.RegistryBaseline)
            $existingBaselineDirectory = Join-Path $Session.InstallRoot 'state\registry-before-install'
            Assert-OpenJocRollbackBaseline $baselineSnapshot $existingBaselineDirectory $allIds $Session.InstallRoot
            Copy-OpenJocRegistrySnapshot $baselineSnapshot $existingBaselineDirectory $baselineDirectory $allIds
        } else {
            $baselineSnapshot = Save-OpenJocRegistrySnapshot $Session $allIds $baselineDirectory $null
        }
        $ownershipToken = [Guid]::NewGuid().ToString('N')
        $state = [ordered]@{
            ProductId = $script:ProductId
            Product = 'OpenJOC LAV 0.15.0'
            Version = $script:Version
            Architecture = 'x64'
            InstallRoot = $Session.InstallRoot
            ClassId = $script:OpenJocClsid
            OwnershipToken = $ownershipToken
            RegistryBaseline = $baselineSnapshot
        }
        $state | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $stage 'openjoc-install.json') -Encoding UTF8
        Set-Content -LiteralPath (Join-Path $stage '.openjoc-ownership') -Value $ownershipToken -Encoding ASCII

        if (Test-Path -LiteralPath $Session.InstallRoot) {
            $existingMarker = Join-Path $Session.InstallRoot '.openjoc-transient'
            Set-Content -LiteralPath $existingMarker -Value $script:ProductId -Encoding ASCII
            $existingMarkerAdded = $true
            try {
                Move-Item -LiteralPath $Session.InstallRoot -Destination $backup
                $existingInstallMoved = $true
            } catch {
                if (Test-Path -LiteralPath $existingMarker) { Remove-Item -LiteralPath $existingMarker -Force }
                $existingMarkerAdded = $false
                throw
            }
        }
        Move-Item -LiteralPath $stage -Destination $Session.InstallRoot
        $newInstallPlaced = $true
        $rollbackDirectory = Join-Path $Session.InstallRoot 'state\registry-for-rollback'
        $baselineDirectory = Join-Path $Session.InstallRoot 'state\registry-before-install'
        Remove-Item -LiteralPath (Join-Path $Session.InstallRoot '.openjoc-transient') -Force
        $registrationAttempted = $true
        $regsvr32 = Join-Path $env:WINDIR 'System32\regsvr32.exe'
        $registrationExit = Invoke-OpenJocNative $Session $regsvr32 @('/s', (Join-Path $Session.InstallRoot 'LAVAudio.ax'))
        if ($registrationExit -ne 0) { throw "Windows refused to register the DirectShow filter (regsvr32 exit $registrationExit)." }

        $sharedSnapshot = @($rollbackSnapshot | Where-Object { $sharedIds -contains $_.ClassId })
        Restore-OpenJocRegistrySnapshot $Session $sharedSnapshot $rollbackDirectory
        $verification = Get-OpenJocVerification $Session
        if (-not $verification.Success) { throw 'Post-install verification failed.' }
        $committed = $true
        $cleanupWarnings = @()
        try {
            Remove-OpenJocCommittedRollbackSnapshot $Session $rollbackSnapshot $rollbackDirectory
        } catch {
            $cleanupWarnings += "The verified installation succeeded, but its committed rollback snapshot could not be removed: $($_.Exception.Message)"
        }
        try { Write-OpenJocLog $Session 'INFO' 'removed committed transaction rollback snapshot' }
        catch { $cleanupWarnings += "Post-commit logging failed: $($_.Exception.Message)" }
        if (Test-Path -LiteralPath $backup) {
            try { Remove-OpenJocOwnedDirectory $Session $backup -Transient }
            catch {
                $cleanupWarnings += "The verified installation succeeded, but the previous-version backup could not be removed: $($_.Exception.Message)"
            }
        }
        $cleanupWarning = $cleanupWarnings -join ' '
        if (-not [string]::IsNullOrWhiteSpace($cleanupWarning)) {
            try { Write-OpenJocLog $Session 'WARN' $cleanupWarning }
            catch { $cleanupWarning = "$cleanupWarning Post-commit warning logging failed: $($_.Exception.Message)" }
        }
        return [pscustomobject]@{ ExitCode = 0; Verification = $verification; Detail = 'Installed and verified.'; Rollback = 'not required'; Warning = $cleanupWarning }
    } catch {
        $detail = $_.Exception.Message
        if ($committed) {
            $postCommitWarning = "The installation was verified and committed, but post-commit housekeeping failed: $detail"
            try { Write-OpenJocLog $Session 'WARN' $postCommitWarning } catch { }
            return [pscustomobject]@{ ExitCode = 0; Verification = $verification; Detail = 'Installed and verified.'; Rollback = 'not permitted after commit'; Warning = $postCommitWarning }
        }
        Write-OpenJocLog $Session 'ERROR' ("install transaction failed: {0}" -f $detail)
        $rollbackErrors = @()
        try {
            if ($registrationAttempted -and (Test-OpenJocSamePath (Get-OpenJocInprocPath $script:OpenJocClsid) (Join-Path $Session.InstallRoot 'LAVAudio.ax'))) {
                $regsvr32 = Join-Path $env:WINDIR 'System32\regsvr32.exe'
                [void](Invoke-OpenJocNative $Session $regsvr32 @('/u', '/s', (Join-Path $Session.InstallRoot 'LAVAudio.ax')))
            }
            if ($registrationAttempted -and $rollbackSnapshot) { Restore-OpenJocRegistrySnapshot $Session $rollbackSnapshot $rollbackDirectory }
        } catch { $rollbackErrors += $_.Exception.Message }
        try { if ($newInstallPlaced -and (Test-Path -LiteralPath $Session.InstallRoot)) { Remove-OpenJocOwnedDirectory $Session $Session.InstallRoot } } catch { $rollbackErrors += $_.Exception.Message }
        try { if (Test-Path -LiteralPath $stage) { Remove-OpenJocOwnedDirectory $Session $stage -Transient } } catch { $rollbackErrors += $_.Exception.Message }
        try {
            if ($existingInstallMoved -and (Test-Path -LiteralPath $backup)) {
                Remove-Item -LiteralPath (Join-Path $backup '.openjoc-transient') -Force
                Move-Item -LiteralPath $backup -Destination $Session.InstallRoot
            }
        } catch { $rollbackErrors += $_.Exception.Message }
        try {
            if (-not $existingInstallMoved -and $existingMarkerAdded) {
                $existingMarker = Join-Path $Session.InstallRoot '.openjoc-transient'
                if (Test-Path -LiteralPath $existingMarker) { Remove-Item -LiteralPath $existingMarker -Force }
            }
        } catch { $rollbackErrors += $_.Exception.Message }
        $rollback = if ($rollbackErrors.Count -eq 0) { 'completed' } else { 'FAILED: ' + ($rollbackErrors -join '; ') }
        Write-OpenJocLog $Session $(if ($rollbackErrors.Count) { 'ERROR' } else { 'INFO' }) ("rollback={0}" -f $rollback)
        return [pscustomobject]@{ ExitCode = $(if ($rollbackErrors.Count) { 50 } elseif ($detail -like '*verification*') { 40 } else { 30 }); Verification = $null; Detail = $detail; Rollback = $rollback }
    }
}

function Invoke-OpenJocUninstallTransaction {
    param($Session)
    $expectedAx = Join-Path $Session.InstallRoot 'LAVAudio.ax'
    $registered = Get-OpenJocInprocPath $script:OpenJocClsid
    if (-not (Test-Path -LiteralPath $Session.InstallRoot) -and -not (Test-OpenJocSamePath $registered $expectedAx)) {
        return [pscustomobject]@{ ExitCode = 0; AlreadyAbsent = $true; Detail = 'OpenJOC LAV is not currently installed.' }
    }
    if (-not (Test-OpenJocOwnedInstall $Session.InstallRoot)) {
        return [pscustomobject]@{ ExitCode = 50; AlreadyAbsent = $false; Detail = 'The install directory is not proven to be OpenJOC-owned, so nothing was deleted.' }
    }
    $classIds = Get-OpenJocClassIds
    $allIds = @($classIds.Values)
    try {
        $installedState = Get-Content -Raw -LiteralPath (Join-Path $Session.InstallRoot 'openjoc-install.json') | ConvertFrom-Json
        $installedBaseline = @($installedState.RegistryBaseline)
        $installedBaselineDirectory = Join-Path $Session.InstallRoot 'state\registry-before-install'
        Assert-OpenJocRollbackBaseline $installedBaseline $installedBaselineDirectory $allIds $Session.InstallRoot
    } catch {
        return [pscustomobject]@{ ExitCode = 50; AlreadyAbsent = $false; Detail = "The original pre-install registry baseline is invalid, so nothing was changed: $($_.Exception.Message)" }
    }
    $transactionId = [Guid]::NewGuid().ToString('N')
    $temp = Join-Path ([IO.Path]::GetTempPath()) ("OpenJOC-uninstall-$transactionId")
    $tombstone = "$($Session.InstallRoot).openjoc-uninstall-$transactionId"
    $rollbackSnapshot = $null
    $rollbackDirectory = $null
    $registrationChanged = $false
    $rootMoved = $false
    $deletionStarted = $false
    try {
        New-OpenJocTransientDirectory $temp
        $rollbackDirectory = Join-Path $temp 'registry-for-rollback'
        $rollbackSnapshot = Save-OpenJocRegistrySnapshot $Session $allIds $rollbackDirectory $null
        $desiredDirectory = Join-Path $temp 'registry-after-uninstall'
        $currentDesiredSnapshot = Save-OpenJocRegistrySnapshot $Session $allIds $desiredDirectory $expectedAx
        $desiredSnapshot = @(Get-OpenJocUninstallDesiredSnapshot $currentDesiredSnapshot $installedBaseline $script:OpenJocClsid)
        $currentMain = @($currentDesiredSnapshot | Where-Object { $_.ClassId -ieq $script:OpenJocClsid })[0]
        $desiredMain = @($desiredSnapshot | Where-Object { $_.ClassId -ieq $script:OpenJocClsid })[0]
        if ($desiredMain.Existed) {
            Copy-Item -LiteralPath (Join-Path $installedBaselineDirectory $desiredMain.File) -Destination (Join-Path $desiredDirectory $currentMain.File)
        }
        $desiredMain.File = $currentMain.File
        $desiredSnapshot | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $desiredDirectory 'snapshot.json') -Encoding UTF8
        Assert-OpenJocRegistrySnapshotFiles $desiredSnapshot $desiredDirectory $allIds
        if (Test-OpenJocSamePath $registered $expectedAx) {
            $regsvr32 = Join-Path $env:WINDIR 'System32\regsvr32.exe'
            $registrationChanged = $true
            $exit = Invoke-OpenJocNative $Session $regsvr32 @('/u', '/s', $expectedAx)
            if ($exit -ne 0) { throw "Windows refused to unregister the DirectShow filter (regsvr32 exit $exit)." }
            Restore-OpenJocRegistrySnapshot $Session $desiredSnapshot $desiredDirectory
        }
        $registryVerification = Test-OpenJocRegistrySnapshotExact $Session $desiredSnapshot $desiredDirectory
        if (-not $registryVerification.Success) { throw $registryVerification.Detail }

        if (Test-Path -LiteralPath $tombstone) { throw "refusing existing uninstall tombstone: $tombstone" }
        Set-Content -LiteralPath (Join-Path $Session.InstallRoot '.openjoc-transient') -Value $script:ProductId -Encoding ASCII
        Move-Item -LiteralPath $Session.InstallRoot -Destination $tombstone
        $rootMoved = $true
        if (Test-Path -LiteralPath $Session.InstallRoot) { throw 'The install directory still exists after the removal commit.' }
        $deletionStarted = $true
        Remove-OpenJocOwnedDirectory $Session $tombstone -Transient
        $rootMoved = $false
        Remove-OpenJocOwnedDirectory $Session $temp -Transient
        return [pscustomobject]@{ ExitCode = 0; AlreadyAbsent = $false; Detail = 'OpenJOC-owned registration and files were removed. Stock LAV and shared registrations match their saved pre-uninstall state.' }
    } catch {
        $detail = $_.Exception.Message
        Write-OpenJocLog $Session 'ERROR' ("uninstall failed: {0}" -f $detail)
        $rollbackErrors = @()
        if (-not $deletionStarted) {
            try {
                if ($rootMoved -and (Test-Path -LiteralPath $tombstone)) {
                    Move-Item -LiteralPath $tombstone -Destination $Session.InstallRoot
                    $rootMoved = $false
                }
                $transientMarker = Join-Path $Session.InstallRoot '.openjoc-transient'
                if (Test-Path -LiteralPath $transientMarker) { Remove-Item -LiteralPath $transientMarker -Force }
            } catch { $rollbackErrors += $_.Exception.Message }
            try {
                if ($registrationChanged -and $rollbackSnapshot) {
                    Restore-OpenJocRegistrySnapshot $Session $rollbackSnapshot $rollbackDirectory
                }
            } catch { $rollbackErrors += $_.Exception.Message }
        } else {
            $rollbackErrors += 'File deletion had already started; registry remains safely uninstalled but file cleanup may be incomplete.'
        }
        try {
            if (Test-Path -LiteralPath $temp) { Remove-OpenJocOwnedDirectory $Session $temp -Transient }
        } catch { $rollbackErrors += $_.Exception.Message }
        $rollbackDetail = if ($rollbackErrors.Count -eq 0) { 'The previous installed state was restored.' } else { 'Rollback detail: ' + ($rollbackErrors -join '; ') }
        Write-OpenJocLog $Session $(if ($rollbackErrors.Count) { 'ERROR' } else { 'INFO' }) $rollbackDetail
        return [pscustomobject]@{ ExitCode = 50; AlreadyAbsent = $false; Detail = ("{0} {1}" -f $detail, $rollbackDetail) }
    }
}

Export-ModuleMember -Function @(
    'Get-OpenJocDefaultInstallRoot', 'New-OpenJocSession', 'Write-OpenJocHeader',
    'Write-OpenJocStep', 'Complete-OpenJocSession', 'Test-OpenJocAdministrator',
    'Invoke-OpenJocElevation', 'Test-OpenJocPackage', 'Get-OpenJocVerification',
    'Write-OpenJocVerification', 'Invoke-OpenJocInstallTransaction',
    'Invoke-OpenJocUninstallTransaction', 'Test-OpenJocUninstallRequiresElevation'
)
