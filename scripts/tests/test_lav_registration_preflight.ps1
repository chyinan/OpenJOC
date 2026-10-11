# SPDX-License-Identifier: Apache-2.0
<#
No native code, registration, administrator rights, or real registry access.
Extract only the exact inventory and ownership functions from the shipped script.
The registry adapter is NOT evaluated: an in-memory replacement supplies keys.
Run under 64-bit Windows PowerShell 5.1, via test_lav_registration_preflight.py.
#>
param(
    [Parameter(Mandatory=$true)][string]$RegistrationScript,
    [Parameter(Mandatory=$true)][string]$ScratchRoot,
    [Parameter(Mandatory=$true)][string]$ReceiptPath
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$script:Results = New-Object 'System.Collections.Generic.List[object]'
$script:Registry = @{}
$script:Opened = New-Object 'System.Collections.Generic.List[string]'
$script:BasesDisposed = 0
$script:KeysDisposed = 0
$script:Failure = ''
$script:NativeCalls = 0
$script:TestRoot = $null
function Assert-True([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw "ASSERTION: $Message" }
}
function Assert-Equal($Actual, $Expected, [string]$Message) {
    if ($Actual -cne $Expected) { throw "ASSERTION: $Message; expected <$Expected>, got <$Actual>" }
}
function Assert-Throws([scriptblock]$Action, [string]$Pattern) {
    $caught = $null
    try { & $Action } catch { $caught = $_ }
    Assert-True ($null -ne $caught) 'Expected refusal'
    Assert-True ($caught.Exception.Message -match $Pattern) ("Unexpected refusal: " + $caught.Exception.Message)
}
function Invoke-Case([string]$Suite, [string]$CaseName, [scriptblock]$Body) {
    $status = 'passed'; $detail = $null
    try { & $Body } catch { $status = 'failed'; $detail = $_.ToString() }
    $script:Results.Add([pscustomobject]@{ suite=$Suite; name=$CaseName; status=$status; error=$detail })
}
function Start-Process {
    $script:NativeCalls++
    throw 'Native processes are forbidden in preflight tests'
}
function Open-LavRegistrationBaseKey {
    param([Microsoft.Win32.RegistryHive]$Hive)
    # No RegistryKey is constructed. The real function's method calls below
    # operate only on these PSCustomObjects, including all failure paths.
    $base = [pscustomobject]@{ HiveName=[string]$Hive }
    Add-Member -InputObject $base -MemberType ScriptMethod -Name OpenSubKey -Value {
        param([string]$Path)
        $id = $this.HiveName + '|' + $Path
        $script:Opened.Add($id)
        if ($script:Failure -eq 'OpenSubKey') { throw 'FIXTURE_OPEN_FAILED' }
        if (-not $script:Registry.ContainsKey($id)) { return $null }
        $key = [pscustomobject]@{ Value=$script:Registry[$id] }
        Add-Member -InputObject $key -MemberType ScriptMethod -Name GetValue -Value {
            param([string]$Name)
            Assert-Equal $Name '' 'Only the default InprocServer32 value may be read'
            if ($script:Failure -eq 'GetValue') { throw 'FIXTURE_VALUE_FAILED' }
            return $this.Value
        }
        Add-Member -InputObject $key -MemberType ScriptMethod -Name Dispose -Value { $script:KeysDisposed++ }
        return $key
    }
    Add-Member -InputObject $base -MemberType ScriptMethod -Name Dispose -Value { $script:BasesDisposed++ }
    return $base
}
function Write-FixtureText([string]$Path, [string]$Text) {
    [IO.File]::WriteAllText($Path, $Text, (New-Object Text.UTF8Encoding($false)))
}
function Write-FixtureManifest([string]$Root) {
    $lines = @(Get-ChildItem -LiteralPath $Root -File -Recurse | Sort-Object FullName | ForEach-Object {
        $relative = $_.FullName.Substring($Root.Length + 1).Replace('\','/')
        if ($relative -ne 'PACKAGE_SHA256SUMS.txt') {
            (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash + '  ' + $relative
        }
    })
    Write-FixtureText (Join-Path $Root 'PACKAGE_SHA256SUMS.txt') (($lines -join "`n") + "`n")
}
function New-PackageFixture([bool]$DifficultPath=$false) {
    $name = [Guid]::NewGuid().ToString('N')
    if ($DifficultPath) { $name += (' space ' + [char]0x97f3 + [char]0x9891 + ' [test] & package') }
    $root = Join-Path $script:TestRoot $name
    [void][IO.Directory]::CreateDirectory((Join-Path $root 'runtime'))
    [void][IO.Directory]::CreateDirectory((Join-Path $root 'scripts'))
    foreach ($name in @('LAVAudio.ax','LAVVideo.ax','LAVSplitter.ax','support.dll')) {
        Write-FixtureText (Join-Path $root ('runtime\' + $name)) ('Synthetic fixture only: ' + $name)
    }
    $profile = @{required_runtime_files=@('LAVAudio.ax','LAVVideo.ax','LAVSplitter.ax','support.dll')}
    Write-FixtureText (Join-Path $root 'runtime\OpenJocRuntimeProfile.json') ($profile | ConvertTo-Json)
    Copy-Item -LiteralPath $RegistrationScript -Destination (Join-Path $root 'scripts\manual-registration.ps1')
    Copy-Item -LiteralPath $script:ClassesPath -Destination (Join-Path $root 'scripts\registration-classes.json')
    Write-FixtureText (Join-Path $root 'README.md') 'Synthetic package; never load these files.'
    Write-FixtureManifest $root
    return $root
}
function Get-RegistryId([string]$Hive, [string]$ClassId) {
    return $Hive + '|Software\Classes\CLSID\{' + $ClassId + '}\InprocServer32'
}
function Reset-Registry([bool]$Present) {
    $script:Registry.Clear(); $script:Opened.Clear()
    $script:BasesDisposed=0; $script:KeysDisposed=0; $script:Failure=''
    if ($Present) {
        foreach ($name in $filters) {
            foreach ($clsid in $classes.$name) {
                foreach ($hive in $hives) {
                    $script:Registry[(Get-RegistryId ([string]$hive) $clsid)] = Join-Path $runtime $name
                }
            }
        }
    }
}
try {
    Assert-True ($PSVersionTable.PSVersion.Major -eq 5 -and $PSVersionTable.PSVersion.Minor -eq 1) 'Use Windows PowerShell 5.1'
    Assert-True ([Environment]::Is64BitProcess) 'Use 64-bit Windows PowerShell'
    Assert-True (Test-Path -LiteralPath $ScratchRoot -PathType Container) 'ScratchRoot must already exist'
    $script:TestRoot = Join-Path ([IO.Path]::GetFullPath($ScratchRoot)) ('lav-preflight-' + [Guid]::NewGuid().ToString('N'))
    [void][IO.Directory]::CreateDirectory($script:TestRoot)
    $RegistrationScript = (Resolve-Path -LiteralPath $RegistrationScript).ProviderPath
    $script:ClassesPath = Join-Path (Split-Path -Parent $RegistrationScript) 'registration-classes.json'
    $tokens=$null; $errors=$null
    $ast=[Management.Automation.Language.Parser]::ParseFile($RegistrationScript,[ref]$tokens,[ref]$errors)
    Assert-Equal (@($errors).Count) 0 'Production script must parse'
    foreach ($functionName in @('Assert-LavPackageInventory','Assert-OwnedPaths')) {
        $definitions=@($ast.FindAll({param($n) $n -is [Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -ceq $functionName},$true))
        Assert-Equal $definitions.Count 1 ('Expected one production function: ' + $functionName)
        $allowed = if ($functionName -eq 'Assert-LavPackageInventory') {
            @('Join-Path','Get-Content','ConvertFrom-Json','Get-FileHash','Get-ChildItem','Test-Path')
        } else { @('Join-Path','Open-LavRegistrationBaseKey') }
        foreach ($command in $definitions[0].FindAll({param($n) $n -is [Management.Automation.Language.CommandAst]},$true)) {
            Assert-True ($allowed -ccontains $command.GetCommandName()) ('Unexpected production command: ' + $command.Extent.Text)
        }
        # No source substitutions: evaluate precisely the shipped AST extent.
        . ([scriptblock]::Create($definitions[0].Extent.Text))
    }
    $classes=Get-Content -LiteralPath $script:ClassesPath -Raw | ConvertFrom-Json
    $filters=@('LAVAudio.ax','LAVVideo.ax','LAVSplitter.ax')
    $hives=@([Microsoft.Win32.RegistryHive]::LocalMachine,[Microsoft.Win32.RegistryHive]::CurrentUser)
    $runtime=Join-Path $script:TestRoot ('owned space ' + [char]0x97f3 + [char]0x9891 + ' [test] & package\runtime')
    Invoke-Case 'inventory' 'complete package accepted' {
        $root=New-PackageFixture; Assert-LavPackageInventory -Root $root
    }
    Invoke-Case 'inventory' 'spaces Chinese brackets and ampersand accepted' {
        $root=New-PackageFixture $true; Assert-LavPackageInventory -Root $root
    }
    Invoke-Case 'inventory' 'changed runtime hash refused' {
        $root=New-PackageFixture; Write-FixtureText (Join-Path $root 'runtime\LAVAudio.ax') 'Changed fixture'
        Assert-Throws { Assert-LavPackageInventory -Root $root } 'Package hash mismatch: runtime/LAVAudio.ax'
    }
    Invoke-Case 'inventory' 'missing listed runtime refused' {
        $root=New-PackageFixture; Remove-Item -LiteralPath (Join-Path $root 'runtime\LAVVideo.ax')
        Assert-Throws { Assert-LavPackageInventory -Root $root } 'LAVVideo.ax'
    }
    Invoke-Case 'inventory' 'extra ordinary file refused' {
        $root=New-PackageFixture; Write-FixtureText (Join-Path $root 'notes.txt') 'Extra file'
        Assert-Throws { Assert-LavPackageInventory -Root $root } 'Unlisted package file: notes.txt'
    }
    Invoke-Case 'inventory' 'missing manifest refused' {
        $root=New-PackageFixture; Remove-Item -LiteralPath (Join-Path $root 'PACKAGE_SHA256SUMS.txt')
        Assert-Throws { Assert-LavPackageInventory -Root $root } 'PACKAGE_SHA256SUMS.txt'
    }
    Invoke-Case 'inventory' 'malformed manifest line refused' {
        $root=New-PackageFixture; Write-FixtureText (Join-Path $root 'PACKAGE_SHA256SUMS.txt') 'Incomplete checksum line'
        Assert-Throws { Assert-LavPackageInventory -Root $root } 'Invalid package hash manifest'
    }
    Invoke-Case 'inventory' 'duplicate manifest entry refused' {
        $root=New-PackageFixture; $path=Join-Path $root 'PACKAGE_SHA256SUMS.txt'; $lines=[IO.File]::ReadAllLines($path)
        [IO.File]::AppendAllText($path,$lines[0]+"`n")
        Assert-Throws { Assert-LavPackageInventory -Root $root } 'Duplicate manifest path'
    }
    Invoke-Case 'inventory' 'required runtime omitted from inventory still refused' {
        $root=New-PackageFixture; Remove-Item -LiteralPath (Join-Path $root 'runtime\support.dll'); Write-FixtureManifest $root
        Assert-Throws { Assert-LavPackageInventory -Root $root } 'Missing runtime file: support.dll'
    }
    Invoke-Case 'inventory' 'missing runtime profile refused' {
        $root=New-PackageFixture; Remove-Item -LiteralPath (Join-Path $root 'runtime\OpenJocRuntimeProfile.json')
        Assert-Throws { Assert-LavPackageInventory -Root $root } 'OpenJocRuntimeProfile.json'
    }
    Invoke-Case 'inventory' 'malformed runtime profile refused' {
        $root=New-PackageFixture; Write-FixtureText (Join-Path $root 'runtime\OpenJocRuntimeProfile.json') '{'
        Assert-Throws { Assert-LavPackageInventory -Root $root } '.'
    }
    Invoke-Case 'inventory' 'empty manifest refused' {
        $root=New-PackageFixture; Write-FixtureText (Join-Path $root 'PACKAGE_SHA256SUMS.txt') ''
        Assert-Throws { Assert-LavPackageInventory -Root $root } 'Unlisted package file'
    }
    Invoke-Case 'inventory' 'changed ownership metadata hash refused' {
        $root=New-PackageFixture; Write-FixtureText (Join-Path $root 'scripts\registration-classes.json') '{}'
        Assert-Throws { Assert-LavPackageInventory -Root $root } 'Package hash mismatch: scripts/registration-classes.json'
    }
    Invoke-Case 'inventory' 'inventoried nested license accepted' {
        $root=New-PackageFixture; [void][IO.Directory]::CreateDirectory((Join-Path $root 'LICENSES\component'))
        Write-FixtureText (Join-Path $root 'LICENSES\component\COPYING') 'Synthetic notice'; Write-FixtureManifest $root
        Assert-LavPackageInventory -Root $root
    }
    Invoke-Case 'ownership' 'all fourteen classes checked in both hives' {
        Reset-Registry $true; Assert-OwnedPaths $filters
        Assert-Equal $script:Opened.Count 28 'Every class/hive pair must be read'
        Assert-Equal (@($script:Opened | Sort-Object -Unique).Count) 28 'No duplicated class/hive pair'
        Assert-Equal $script:BasesDisposed 28 'Base handles must be disposed'
        Assert-Equal $script:KeysDisposed 28 'Class handles must be disposed'
    }
    Invoke-Case 'ownership' 'all unregistered classes accepted' {
        Reset-Registry $false; Assert-OwnedPaths $filters
        Assert-Equal $script:Opened.Count 28 'Absent keys must not skip other classes'
        Assert-Equal $script:BasesDisposed 28 'Absent-key base handles must be disposed'
        Assert-Equal $script:KeysDisposed 0 'No absent key may be disposed'
    }
    foreach ($name in $filters) {
        foreach ($clsid in $classes.$name) {
            foreach ($hive in $hives) {
                Invoke-Case 'ownership' ("foreign $hive $name {$clsid} refused") {
                    Reset-Registry $true
                    $id=Get-RegistryId ([string]$hive) $clsid
                    $script:Registry[$id]=Join-Path $script:TestRoot ('previous-package\runtime\' + $name)
                    Assert-True ($filters -ccontains (Split-Path -Leaf $script:Registry[$id])) 'Foreign registration must keep the original filter basename'
                    Assert-Throws { Assert-OwnedPaths $filters } 'Refusing to unregister'
                    Assert-Equal $script:Opened[$script:Opened.Count - 1] $id 'Stop at the conflicting class'
                    Assert-Equal $script:BasesDisposed $script:Opened.Count 'Dispose bases after refusal'
                    Assert-Equal $script:KeysDisposed $script:Opened.Count 'Dispose keys after refusal'
                }
            }
        }
    }
    foreach ($format in @('quoted','case-insensitive','normalized')) {
        Invoke-Case 'ownership' ("same path $format accepted") {
            Reset-Registry $true
            foreach ($id in @($script:Registry.Keys)) {
                $value=$script:Registry[$id]
                if ($format -eq 'quoted') { $value='"'+$value+'"' }
                if ($format -eq 'case-insensitive') { $value=$value.ToUpperInvariant() }
                if ($format -eq 'normalized') { $value=Join-Path (Split-Path -Parent $value) ('unused\..\'+(Split-Path -Leaf $value)) }
                $script:Registry[$id]=$value
            }
            Assert-OwnedPaths $filters
            Assert-Equal $script:Opened.Count 28 'All same-path classes must pass'
        }
    }
    foreach ($value in @('', '   ')) {
        Invoke-Case 'ownership' ('empty or whitespace default value refused length '+$value.Length) {
            Reset-Registry $true
            $script:Registry[(Get-RegistryId 'LocalMachine' $classes.'LAVAudio.ax'[0])]=$value
            Assert-Throws { Assert-OwnedPaths $filters } 'Refusing to unregister'
            Assert-Equal $script:Opened.Count 1 'Refuse before checking later classes'
            Assert-Equal $script:BasesDisposed 1 'Dispose base on empty value'
            Assert-Equal $script:KeysDisposed 1 'Dispose key on empty value'
        }
    }
    foreach ($failureMode in @('OpenSubKey','GetValue')) {
        Invoke-Case 'ownership' ("registry read failure $failureMode propagates") {
            Reset-Registry $true; $script:Failure=$failureMode
            Assert-Throws { Assert-OwnedPaths $filters } 'FIXTURE_.*_FAILED'
            Assert-Equal $script:Opened.Count 1 'No later read after error'
            Assert-Equal $script:BasesDisposed 1 'Dispose base after read error'
            $expectedKeys=if ($failureMode -eq 'GetValue') {1} else {0}
            Assert-Equal $script:KeysDisposed $expectedKeys 'Dispose only opened key after read error'
        }
    }
    Assert-Equal $script:NativeCalls 0 'No native process may start'
} catch {
    $script:Results.Add([pscustomobject]@{suite='bootstrap';name='harness';status='failed';error=$_.ToString()})
} finally {
    if ($null -ne $script:TestRoot -and (Test-Path -LiteralPath $script:TestRoot)) {
        Remove-Item -LiteralPath $script:TestRoot -Recurse -Force
    }
}
$failed=@($script:Results | Where-Object status -eq 'failed').Count
$receipt=[ordered]@{
    status=$(if ($failed -eq 0) {'passed'} else {'failed'})
    total=$script:Results.Count; passed=($script:Results.Count - $failed); failed=$failed
    powershell_version=$PSVersionTable.PSVersion.ToString(); process_is_64bit=[Environment]::Is64BitProcess
    registration_script_sha256=(Get-FileHash -LiteralPath $RegistrationScript -Algorithm SHA256).Hash
    real_registry_access=$false; real_registration_attempted=$false; native_calls=$script:NativeCalls
    results=@($script:Results.ToArray())
}
Write-FixtureText $ReceiptPath ($receipt | ConvertTo-Json -Depth 8)
if ($failed -ne 0) { $script:Results | Where-Object status -eq 'failed' | Format-List | Out-String | Write-Output; exit 1 }
Write-Output ("Preflight: {0}/{0} passed; no real registry access or registration." -f $script:Results.Count)
