# SPDX-License-Identifier: Apache-2.0
param([Parameter(Mandatory=$true)][ValidateSet('Register','Unregister')][string]$Operation)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
function Invoke-LavRegistrationProcess {
    param([string]$Executable, [string]$ModulePath, [bool]$Unregister)
    # Start-Process joins ArgumentList items; quote the complete module path
    # explicitly so spaces/Unicode survive. Windows filenames cannot contain ".
    $ModulePath = [IO.Path]::GetFullPath($ModulePath)
    $arguments = if ($Unregister) { '/u /s "{0}"' -f $ModulePath } else { '/s "{0}"' -f $ModulePath }
    $process = Start-Process -FilePath $Executable -ArgumentList $arguments -Wait -PassThru -ErrorAction Stop
    if ($null -eq $process) { throw 'Registration process returned no result; registration state is unknown.' }
    try {
        if (-not $process.HasExited) { throw 'Registration process has not exited; registration state is unknown.' }
        if ($null -eq $process.PSObject.Properties['ExitCode']) { throw 'Registration process has no exit-code property; registration state is unknown.' }
        $code = $process.ExitCode
        if ($null -eq $code) { throw 'Registration process returned no exit code; registration state is unknown.' }
        if ($code -ne 0) { throw "regsvr32 failed for '$ModulePath' (exit $code). Earlier filters may already have changed; preserve both packages and review rollback instructions." }
    } finally {
        if ($process -is [System.IDisposable]) { $process.Dispose() }
    }
}
function Assert-LavPackageInventory {
    param([string]$Root)
    $runtime = Join-Path $Root 'runtime'
    $profile = Get-Content -LiteralPath (Join-Path $runtime 'OpenJocRuntimeProfile.json') -Raw | ConvertFrom-Json
    # Verify every inventory entry before loading any candidate native code.
    $seen = @{}
    foreach ($line in [IO.File]::ReadAllLines((Join-Path $root 'PACKAGE_SHA256SUMS.txt'))) {
        if ($line -notmatch '^([0-9A-Fa-f]{64})  (.+)$') { throw 'Invalid package hash manifest.' }
        $hash = $matches[1]; $name = $matches[2]
        if ([IO.Path]::IsPathRooted($name) -or $name.Contains(':') -or ($name -split '[/\\]') -contains '..') { throw 'Unsafe package manifest path.' }
        $path = [IO.Path]::GetFullPath((Join-Path $root $name))
        if (-not $path.StartsWith($root + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Path escapes package.' }
        if ($seen.ContainsKey($name)) { throw 'Duplicate manifest path.' }
        $seen[$name] = $true
        if ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -ne $hash) { throw "Package hash mismatch: $name" }
    }
    foreach ($file in Get-ChildItem -LiteralPath $root -File -Recurse) {
        $relative = $file.FullName.Substring($root.Length + 1).Replace('\','/')
        if ($relative -ne 'PACKAGE_SHA256SUMS.txt' -and -not $seen.ContainsKey($relative)) { throw "Unlisted package file: $relative" }
    }
    foreach ($name in $profile.required_runtime_files) {
        if (-not (Test-Path -LiteralPath (Join-Path $runtime $name) -PathType Leaf)) { throw "Missing runtime file: $name" }
    }
}
function Open-LavRegistrationBaseKey {
    param([Microsoft.Win32.RegistryHive]$Hive)
    return [Microsoft.Win32.RegistryKey]::OpenBaseKey($Hive, [Microsoft.Win32.RegistryView]::Registry64)
}
try {
    if (-not [Environment]::Is64BitProcess) { throw 'Use the supplied batch file with 64-bit Windows PowerShell.' }
    $principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw 'Close this window, then right-click the batch file and choose Run as administrator only if you approve registration changes.'
    }
    $root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
    $runtime = Join-Path $root 'runtime'
    Assert-LavPackageInventory -Root $root
    $filters = @('LAVAudio.ax', 'LAVVideo.ax', 'LAVSplitter.ax')
    $classes = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'registration-classes.json') -Raw | ConvertFrom-Json
    $hives = @([Microsoft.Win32.RegistryHive]::LocalMachine, [Microsoft.Win32.RegistryHive]::CurrentUser)
    # Check all paths before unregistration, then check each DLL again immediately
    # before its call. Do not delete registry values directly or restore blindly.
    function Assert-OwnedPaths([string[]]$Names) {
        foreach ($name in $Names) {
            $expected = [IO.Path]::GetFullPath((Join-Path $runtime $name))
            foreach ($clsid in $classes.$name) {
                foreach ($hive in $hives) {
                    $base = Open-LavRegistrationBaseKey -Hive $hive
                    try {
                        $key = $base.OpenSubKey("Software\Classes\CLSID\{$clsid}\InprocServer32")
                        if ($null -ne $key) {
                            try {
                                $current = [string]$key.GetValue('')
                                if ([string]::IsNullOrWhiteSpace($current) -or [IO.Path]::GetFullPath($current.Trim('"')) -ne $expected) {
                                    throw "Refusing to unregister: $hive CLSID {$clsid} points to '$current', not '$expected'."
                                }
                            } finally { $key.Dispose() }
                        }
                    } finally { $base.Dispose() }
                }
            }
        }
    }
    if ($Operation -eq 'Unregister') { Assert-OwnedPaths $filters }
    $regsvr32 = Join-Path ([Environment]::SystemDirectory) 'regsvr32.exe'
    foreach ($name in $filters) {
        if ($Operation -eq 'Unregister') { Assert-OwnedPaths @($name) }
        Invoke-LavRegistrationProcess -Executable $regsvr32 -ModulePath (Join-Path $runtime $name) -Unregister ($Operation -eq 'Unregister')
        Write-Host "$Operation succeeded: $name"
    }
    Write-Host 'No installed files were copied or deleted. Review active player filter paths before playback.'
    exit 0
} catch {
    Write-Error $_
    exit 1
}
