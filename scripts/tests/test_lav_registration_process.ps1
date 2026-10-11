# SPDX-License-Identifier: Apache-2.0
<#
Windows PowerShell 5.1 regression suite. No administrator rights are needed.
The registration script itself is NEVER invoked or dot-sourced. Only the exact
helper, filters assignment, ownership preflight, and registration foreach AST
extents are evaluated. All sequencing tests mock Start-Process and ownership.
Native tests only run the separately compiled harmless GUI fixture under
RUNNER_TEMP; no AX file exists and regsvr32 is never launched.

Example (after compiling lav_registration_gui_probe.cpp with existing MSVC):
powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File `
  .\scripts\tests\test_lav_registration_process.ps1 `
  -ScriptPath .\packaging\windows-lav-full\scripts\manual-registration.ps1 `
  -GuiProbe "$env:RUNNER_TEMP\lav_registration_gui_probe.exe" `
  -ResultPath "$env:RUNNER_TEMP\lav-registration-receipt.json" -ScratchRoot "$env:RUNNER_TEMP"
#>
[CmdletBinding()]
param(
    [Alias('ScriptPath')][string]$RegistrationScript = (Join-Path $PSScriptRoot '..\..\packaging\windows-lav-full\scripts\manual-registration.ps1'),
    [Alias('GuiProbe')][Parameter(Mandatory=$true)][string]$ProbeExecutable,
    [Alias('ResultPath')][Parameter(Mandatory=$true)][string]$ReceiptPath,
    [string]$ScratchRoot = $env:RUNNER_TEMP
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$script:Results = New-Object 'System.Collections.Generic.List[object]'
$script:MockCalls = New-Object 'System.Collections.Generic.List[object]'
$script:Events = New-Object 'System.Collections.Generic.List[string]'
$script:SuccessMessages = New-Object 'System.Collections.Generic.List[string]'
$script:NativeObservations = New-Object 'System.Collections.Generic.List[object]'
$script:MockBehaviors = @()
$script:RejectOwner = ''
$script:RejectPreflight = $false
$script:NativeStarted = $false
$script:TestRoot = $null
$script:ScriptHash = $null
$script:ProbeHash = $null
$script:HelperHash = $null
$script:BootstrapError = $null
$savedLastExitCode = Get-Variable -Name LASTEXITCODE -Scope Global -ErrorAction SilentlyContinue
$savedEnvironment = @{}
foreach ($key in @('LAV_PROBE_EXPECTED_MODULE','LAV_PROBE_EXPECTED_OPERATION','LAV_PROBE_MARKER_PATH','LAV_PROBE_EXIT_CODE')) {
    $savedEnvironment[$key] = [Environment]::GetEnvironmentVariable($key, 'Process')
}

function Assert-True([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw "ASSERTION: $Message" }
}
function Assert-Equal($Actual, $Expected, [string]$Message) {
    if ($Actual -cne $Expected) { throw "ASSERTION: $Message; expected <$Expected>, got <$Actual>." }
}
function Assert-Throws([scriptblock]$Action, [string]$Pattern) {
    $caught = $null
    try { & $Action | Out-Null } catch { $caught = $_ }
    Assert-True ($null -ne $caught) 'Expected an exception, but the operation succeeded'
    Assert-True ($caught.Exception.Message -match $Pattern) ("Wrong exception: " + $caught.Exception.Message)
}
function Invoke-Case([string]$Suite, [string]$Name, [scriptblock]$Body) {
    $timer = [Diagnostics.Stopwatch]::StartNew()
    $status = 'passed'; $detail = $null
    try { & $Body | Out-Null } catch { $status = 'failed'; $detail = $_.ToString() }
    $timer.Stop()
    $script:Results.Add([pscustomobject]@{
        suite=$Suite; name=$Name; status=$status; duration_ms=$timer.ElapsedMilliseconds; error=$detail
    })
}
function Get-TextSha256([string]$Text) {
    $sha = [Security.Cryptography.SHA256]::Create()
    try { return ([BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($Text)))).Replace('-','').ToLowerInvariant() }
    finally { $sha.Dispose() }
}
function Reset-Mocks([string[]]$Behaviors) {
    $script:MockCalls.Clear(); $script:Events.Clear(); $script:SuccessMessages.Clear()
    $script:MockBehaviors = @($Behaviors)
    $script:RejectOwner = ''; $script:RejectPreflight = $false
}
function Assert-MockCall([int]$Index, [string]$Module, [bool]$Unregister) {
    $call = $script:MockCalls[$Index]
    $expected = if ($Unregister) { '/u /s "{0}"' -f $Module } else { '/s "{0}"' -f $Module }
    Assert-Equal $call.file_path $script:FakeExecutable 'Unexpected executable'
    Assert-True $call.argument_is_string 'ArgumentList must be one string, not split items'
    Assert-Equal $call.arguments $expected 'The complete module path must be double-quoted exactly once'
    Assert-True $call.wait 'Missing -Wait'
    Assert-True $call.pass_thru 'Missing -PassThru'
    Assert-Equal $call.error_action 'Stop' 'Missing -ErrorAction Stop'
}

# Deliberately shadows only the cmdlet's unqualified name used by the extracted
# helper. An unexpected extra call fails closed instead of running anything.
function Start-Process {
    [CmdletBinding()]
    param([string]$FilePath, [object]$ArgumentList, [switch]$Wait, [switch]$PassThru)
    $index = $script:MockCalls.Count
    $errorActionText = if ($PSBoundParameters.ContainsKey('ErrorAction')) { [string]$PSBoundParameters['ErrorAction'] } else { '' }
    $script:MockCalls.Add([pscustomobject]@{
        file_path=$FilePath; arguments=$ArgumentList; argument_is_string=($ArgumentList -is [string]);
        wait=[bool]$Wait; pass_thru=[bool]$PassThru; error_action=$errorActionText
    })
    $script:Events.Add("Start:$index")
    if ($index -ge $script:MockBehaviors.Count) { throw 'MOCK_UNEXPECTED_PROCESS_CALL' }
    switch ($script:MockBehaviors[$index]) {
        'Zero'       { return [pscustomobject]@{ HasExited=$true; ExitCode=0 } }
        'Nonzero'    { return [pscustomobject]@{ HasExited=$true; ExitCode=7 } }
        'Negative'   { return [pscustomobject]@{ HasExited=$true; ExitCode=-1 } }
        'Null'       { return $null }
        'Missing'    { return [pscustomobject]@{ HasExited=$true } }
        'NullCode'   { return [pscustomobject]@{ HasExited=$true; ExitCode=$null } }
        'Getter'     {
            $result = [pscustomobject]@{ HasExited=$true }
            Add-Member -InputObject $result -MemberType ScriptProperty -Name ExitCode -Value { throw 'MOCK_EXIT_GETTER_FAILURE' }
            return $result
        }
        'NotExited'  { return [pscustomobject]@{ HasExited=$false; ExitCode=0 } }
        'Launch'     { throw 'MOCK_LAUNCH_FAILURE' }
        default      { throw 'MOCK_UNKNOWN_BEHAVIOR' }
    }
}
function Assert-OwnedPaths([string[]]$Names) {
    $script:Events.Add('Own:' + ($Names -join ','))
    if (($script:RejectPreflight -and $Names.Count -gt 1) -or
        ($Names.Count -eq 1 -and $Names[0] -ceq $script:RejectOwner)) {
        throw 'MOCK_OWNERSHIP_REJECTED'
    }
}
function Write-Host {
    param([object]$Object)
    $script:SuccessMessages.Add([string]$Object)
}

function Assert-Marker([string]$Path, [int]$Code, [bool]$Unregister) {
    Assert-True (Test-Path -LiteralPath $Path -PathType Leaf) 'Helper returned before GUI completion marker existed'
    $marker = Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
    Assert-Equal $marker.completed $true 'GUI fixture did not complete'
    Assert-Equal $marker.argv_match $true 'GUI argv did not exactly match the expected arguments'
    Assert-Equal $marker.exit_code $Code 'Wrong GUI marker exit code'
    $expectedArgc = if ($Unregister) { 4 } else { 3 }
    Assert-Equal $marker.argc $expectedArgc 'Unexpected GUI argument count'
}
function Initialize-NativeCase([string]$Operation, [int]$Code) {
    $marker = Join-Path $script:TestRoot (([Guid]::NewGuid().ToString('N')) + '.json')
    $module = Join-Path $script:TestRoot ('missing audio ' + [char]0x97f3 + [char]0x9891 + ' & module.ax')
    Assert-True (-not (Test-Path -LiteralPath $module)) 'The fake AX module must not exist'
    [Environment]::SetEnvironmentVariable('LAV_PROBE_EXPECTED_MODULE', $module, 'Process')
    [Environment]::SetEnvironmentVariable('LAV_PROBE_EXPECTED_OPERATION', $Operation, 'Process')
    [Environment]::SetEnvironmentVariable('LAV_PROBE_MARKER_PATH', $marker, 'Process')
    [Environment]::SetEnvironmentVariable('LAV_PROBE_EXIT_CODE', [string]$Code, 'Process')
    return [pscustomobject]@{ marker=$marker; module=$module }
}

try {
    Assert-True ([Environment]::OSVersion.Platform -eq [PlatformID]::Win32NT) 'This suite requires Windows'
    Assert-True ($PSVersionTable.PSVersion.Major -eq 5 -and $PSVersionTable.PSVersion.Minor -eq 1) 'Use Windows PowerShell 5.1, not PowerShell Core'
    Assert-True ([Environment]::Is64BitProcess) 'Use 64-bit Windows PowerShell'
    Assert-True (-not [string]::IsNullOrWhiteSpace($env:RUNNER_TEMP)) 'RUNNER_TEMP is required for all native fixture writes'
    Assert-True (-not [string]::IsNullOrWhiteSpace($ScratchRoot)) 'ScratchRoot is required'
    $runnerRoot = [IO.Path]::GetFullPath($env:RUNNER_TEMP).TrimEnd([char]'\')
    $ScratchRoot = [IO.Path]::GetFullPath($ScratchRoot).TrimEnd([char]'\')
    Assert-True ($ScratchRoot.Equals($runnerRoot, [StringComparison]::OrdinalIgnoreCase) -or $ScratchRoot.StartsWith($runnerRoot + '\', [StringComparison]::OrdinalIgnoreCase)) 'ScratchRoot must be inside RUNNER_TEMP'
    if (-not (Test-Path -LiteralPath $ScratchRoot -PathType Container)) { New-Item -ItemType Directory -Path $ScratchRoot -Force | Out-Null }
    $RegistrationScript = (Resolve-Path -LiteralPath $RegistrationScript).ProviderPath
    $script:ScriptHash = (Get-FileHash -LiteralPath $RegistrationScript -Algorithm SHA256).Hash
    $tokens = $null; $parseErrors = $null
    $ast = [Management.Automation.Language.Parser]::ParseFile($RegistrationScript, [ref]$tokens, [ref]$parseErrors)
    Assert-Equal (@($parseErrors).Count) 0 'Registration script has parse errors'
    $helpers = @($ast.FindAll({ param($node)
        $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -ceq 'Invoke-LavRegistrationProcess'
    }, $true))
    Assert-Equal $helpers.Count 1 'Expected exactly one registration helper'
    $loops = @($ast.FindAll({ param($node)
        $node -is [Management.Automation.Language.ForEachStatementAst] -and
        $node.Variable.VariablePath.UserPath -ceq 'name' -and $node.Condition.Extent.Text.Trim() -ceq '$filters'
    }, $true))
    Assert-Equal $loops.Count 1 'Expected exactly one foreach ($name in $filters) registration loop'
    $filterAssignments = @($ast.FindAll({ param($node)
        $node -is [Management.Automation.Language.AssignmentStatementAst] -and $node.Left.Extent.Text -ceq '$filters'
    }, $true))
    Assert-Equal $filterAssignments.Count 1 'Expected exactly one filters assignment'
    $preflights = @($ast.FindAll({ param($node)
        $node -is [Management.Automation.Language.IfStatementAst] -and
        $node.Extent.Text -match '^if\s*\(\$Operation\s+-eq\s+''Unregister''\)\s*\{\s*Assert-OwnedPaths\s+\$filters\s*\}$'
    }, $true))
    Assert-Equal $preflights.Count 1 'Missing all-filter ownership preflight'
    Assert-True ($preflights[0].Extent.EndOffset -lt $loops[0].Extent.StartOffset) 'Ownership preflight must precede registration loop'
    # Fail closed if extracted executable commands drift beyond the reviewed
    # helper/loop contract; the outer registration body is never evaluated.
    foreach ($command in $helpers[0].FindAll({ param($node) $node -is [Management.Automation.Language.CommandAst] }, $true)) {
        Assert-Equal ($command.GetCommandName()) 'Start-Process' 'Unexpected helper command'
    }
    foreach ($command in $loops[0].FindAll({ param($node) $node -is [Management.Automation.Language.CommandAst] }, $true)) {
        Assert-True (@('Assert-OwnedPaths','Invoke-LavRegistrationProcess','Join-Path','Write-Host') -ccontains $command.GetCommandName()) 'Unexpected loop command'
    }
    $script:HelperHash = Get-TextSha256 $helpers[0].Extent.Text
    . ([scriptblock]::Create($helpers[0].Extent.Text))
    . ([scriptblock]::Create($filterAssignments[0].Extent.Text))
    $loopBlock = [scriptblock]::Create($loops[0].Extent.Text)
    $preflightBlock = [scriptblock]::Create($preflights[0].Extent.Text)
    Assert-Equal ($filters -join ',') 'LAVAudio.ax,LAVVideo.ax,LAVSplitter.ax' 'Wrong production filter sequence'
    $script:FakeExecutable = Join-Path $ScratchRoot 'NEVER-RUN mock registration process.exe'
    Assert-True (-not (Test-Path -LiteralPath $script:FakeExecutable)) 'Mock executable path must not exist'
    $runtime = Join-Path $ScratchRoot ('candidate ' + [char]0x03a9 + ' ' + [char]0x97f3 + [char]0x9891 + ' & filters')
    $regsvr32 = $script:FakeExecutable
    $singleModule = Join-Path $runtime 'LAVAudio.ax'
    $failures = [ordered]@{
        Nonzero='exit 7'; Negative='exit -1'; Null='no result'; Missing='exit-code property';
        NullCode='no exit code'; Getter='MOCK_EXIT_GETTER_FAILURE|no exit code'; NotExited='has not exited'; Launch='MOCK_LAUNCH_FAILURE'
    }

    foreach ($operationName in @('Register','Unregister')) {
        $Operation = $operationName
        $isUnregister = $Operation -eq 'Unregister'
        Invoke-Case 'helper-mock' "$Operation zero uses exact quoted argument and process flags" {
            Reset-Mocks @('Zero')
            Invoke-LavRegistrationProcess -Executable $script:FakeExecutable -ModulePath $singleModule -Unregister $isUnregister
            Assert-Equal $script:MockCalls.Count 1 'Unexpected process count'
            Assert-MockCall 0 $singleModule $isUnregister
        }
        Invoke-Case 'helper-mock' "$Operation normalizes module to a full path before quoting" {
            Reset-Mocks @('Zero')
            $unnormalizedModule = Join-Path $runtime 'nested\..\LAVAudio.ax'
            Invoke-LavRegistrationProcess -Executable $script:FakeExecutable -ModulePath $unnormalizedModule -Unregister $isUnregister
            Assert-Equal $script:MockCalls.Count 1 'Unexpected process count'
            Assert-MockCall 0 ([IO.Path]::GetFullPath($unnormalizedModule)) $isUnregister
        }
        foreach ($behavior in $failures.Keys) {
            Invoke-Case 'helper-mock' "$Operation rejects $behavior" {
                Reset-Mocks @($behavior)
                Assert-Throws { Invoke-LavRegistrationProcess -Executable $script:FakeExecutable -ModulePath $singleModule -Unregister $isUnregister } $failures[$behavior]
                Assert-Equal $script:MockCalls.Count 1 'Unexpected process count'
                Assert-MockCall 0 $singleModule $isUnregister
            }
        }
        Invoke-Case 'sequence-mock' "$Operation executes Audio Video Splitter in production order" {
            Reset-Mocks @('Zero','Zero','Zero')
            & $preflightBlock
            & $loopBlock
            Assert-Equal $script:MockCalls.Count 3 'Expected all three filters'
            $expectedEvents = @()
            if ($isUnregister) { $expectedEvents += 'Own:LAVAudio.ax,LAVVideo.ax,LAVSplitter.ax' }
            for ($i=0; $i -lt 3; $i++) {
                if ($isUnregister) { $expectedEvents += ('Own:' + $filters[$i]) }
                $expectedEvents += "Start:$i"
                Assert-MockCall $i (Join-Path $runtime $filters[$i]) $isUnregister
                Assert-Equal $script:SuccessMessages[$i] ("$Operation succeeded: " + $filters[$i]) 'Incorrect success report'
            }
            Assert-Equal ($script:Events -join '|') ($expectedEvents -join '|') 'Wrong ownership/process ordering'
            Assert-Equal $script:SuccessMessages.Count 3 'Unexpected success report count'
        }
        foreach ($behavior in $failures.Keys) {
            for ($failureIndex=0; $failureIndex -lt 3; $failureIndex++) {
                Invoke-Case 'sequence-mock' "$Operation stops on $behavior at $($filters[$failureIndex])" {
                    $plan = @('Zero','Zero','Zero'); $plan[$failureIndex] = $behavior
                    Reset-Mocks $plan
                    Assert-Throws { & $preflightBlock; & $loopBlock } $failures[$behavior]
                    Assert-Equal $script:MockCalls.Count ($failureIndex + 1) 'Subsequent registration ran after failure'
                    Assert-Equal $script:SuccessMessages.Count $failureIndex 'A failed or subsequent filter was reported successful'
                    $expectedEvents = @()
                    if ($isUnregister) { $expectedEvents += 'Own:LAVAudio.ax,LAVVideo.ax,LAVSplitter.ax' }
                    for ($i=0; $i -le $failureIndex; $i++) {
                        if ($isUnregister) { $expectedEvents += ('Own:' + $filters[$i]) }
                        $expectedEvents += "Start:$i"
                        Assert-MockCall $i (Join-Path $runtime $filters[$i]) $isUnregister
                    }
                    Assert-Equal ($script:Events -join '|') ($expectedEvents -join '|') 'Failure violated immediate ownership/process ordering'
                }
            }
        }
    }
    foreach ($lastExitState in @('unset','stale-zero','stale-nonzero')) {
        foreach ($behavior in @('Zero','Nonzero')) {
            Invoke-Case 'last-exit-code' "$lastExitState cannot override process $behavior" {
                Remove-Variable -Name LASTEXITCODE -Scope Global -ErrorAction SilentlyContinue
                if ($lastExitState -eq 'stale-zero') { Set-Variable -Name LASTEXITCODE -Scope Global -Value 0 }
                if ($lastExitState -eq 'stale-nonzero') { Set-Variable -Name LASTEXITCODE -Scope Global -Value 91 }
                Reset-Mocks @($behavior)
                $action = { Invoke-LavRegistrationProcess -Executable $script:FakeExecutable -ModulePath $singleModule -Unregister $false }
                if ($behavior -eq 'Zero') { & $action } else { Assert-Throws $action 'exit 7' }
                Assert-Equal $script:MockCalls.Count 1 'Unexpected process count'
                Assert-MockCall 0 $singleModule $false
            }
        }
    }
    $Operation = 'Unregister'
    Invoke-Case 'ownership-mock' 'All-filter preflight rejection blocks every unregister' {
        Reset-Mocks @('Zero','Zero','Zero'); $script:RejectPreflight = $true
        Assert-Throws { & $preflightBlock; & $loopBlock } 'MOCK_OWNERSHIP_REJECTED'
        Assert-Equal $script:MockCalls.Count 0 'Unregistration ran after preflight rejected ownership'
        Assert-Equal $script:SuccessMessages.Count 0 'Unexpected success report'
        Assert-Equal ($script:Events -join '|') 'Own:LAVAudio.ax,LAVVideo.ax,LAVSplitter.ax' 'Unexpected activity after preflight rejection'
    }
    for ($failureIndex=0; $failureIndex -lt 3; $failureIndex++) {
        Invoke-Case 'ownership-mock' "Immediate ownership rejection blocks $($filters[$failureIndex]) and subsequent unregisters" {
            Reset-Mocks @('Zero','Zero','Zero'); $script:RejectOwner = $filters[$failureIndex]
            Assert-Throws { & $preflightBlock; & $loopBlock } 'MOCK_OWNERSHIP_REJECTED'
            Assert-Equal $script:MockCalls.Count $failureIndex 'Rejected or subsequent module was unregistered'
            Assert-Equal $script:SuccessMessages.Count $failureIndex 'Rejected module was reported successful'
            $expectedEvents = @('Own:LAVAudio.ax,LAVVideo.ax,LAVSplitter.ax')
            for ($i=0; $i -le $failureIndex; $i++) {
                $expectedEvents += ('Own:' + $filters[$i])
                if ($i -lt $failureIndex) {
                    $expectedEvents += "Start:$i"
                    Assert-MockCall $i (Join-Path $runtime $filters[$i]) $true
                }
            }
            Assert-Equal ($script:Events -join '|') ($expectedEvents -join '|') 'Ownership guard is not immediately before every unregister'
        }
    }

    # Remove our mock before calling the EXACT production helper with a real
    # harmless GUI child. Fail if another function/alias intercepts Start-Process.
    Remove-Item -LiteralPath Function:\Start-Process
    Assert-Equal (Get-Command Start-Process).CommandType ([Management.Automation.CommandTypes]::Cmdlet) 'Start-Process is still mocked or aliased'
    Assert-Equal (Get-Command Start-Process).ModuleName 'Microsoft.PowerShell.Management' 'Unexpected Start-Process implementation'
    $ProbeExecutable = (Resolve-Path -LiteralPath $ProbeExecutable).ProviderPath
    Assert-Equal ([IO.Path]::GetFileName($ProbeExecutable)) 'lav_registration_gui_probe.exe' 'Only the dedicated GUI fixture may be executed'
    $script:ProbeHash = (Get-FileHash -LiteralPath $ProbeExecutable -Algorithm SHA256).Hash.ToLowerInvariant()
    $bytes = [IO.File]::ReadAllBytes($ProbeExecutable)
    Assert-True ($bytes.Length -gt 128 -and $bytes[0] -eq 0x4d -and $bytes[1] -eq 0x5a) 'Fixture is not a PE executable'
    $peOffset = [BitConverter]::ToInt32($bytes, 0x3c)
    Assert-True ($peOffset -ge 0 -and ($peOffset + 94) -lt $bytes.Length) 'Invalid fixture PE header bounds'
    Assert-Equal ([BitConverter]::ToUInt32($bytes, $peOffset)) 0x4550 'Invalid fixture PE signature'
    Assert-Equal ([BitConverter]::ToUInt16($bytes, ($peOffset + 24 + 68))) 2 'Fixture must use WINDOWS GUI subsystem, not CONSOLE'
    $script:TestRoot = Join-Path $ScratchRoot ('LAV GUI ' + [char]0x03a9 + ' ' + [char]0x97f3 + [char]0x9891 + ' & space ' + [Guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $script:TestRoot | Out-Null
    $nativeExe = Join-Path $script:TestRoot 'lav_registration_gui_probe.exe'
    Copy-Item -LiteralPath $ProbeExecutable -Destination $nativeExe
    Assert-Equal ((Get-FileHash -LiteralPath $nativeExe -Algorithm SHA256).Hash.ToLowerInvariant()) $script:ProbeHash 'Copied probe hash mismatch'
    $script:NativeStarted = $true
    foreach ($code in @(0,7)) {
        Invoke-Case 'native-gui-control' "Real Start-Process waits and exposes GUI ExitCode $code" {
            $fixture = Initialize-NativeCase 'Register' $code
            $timer = [Diagnostics.Stopwatch]::StartNew()
            $process = Start-Process -FilePath $nativeExe -ArgumentList ('/s "{0}"' -f $fixture.module) -Wait -PassThru -ErrorAction Stop
            try {
                $timer.Stop()
                Assert-True ($null -ne $process) 'Real Start-Process returned null'
                Assert-True $process.HasExited 'GUI child has not exited'
                Assert-Equal $process.ExitCode $code 'Real process ExitCode differs'
                Assert-True ($timer.ElapsedMilliseconds -ge 600) 'Start-Process returned before the GUI delay'
                Assert-Marker $fixture.marker $code $false
                $script:NativeObservations.Add([pscustomobject]@{ type='control'; operation='Register'; expected_exit_code=$code; observed_exit_code=$process.ExitCode; elapsed_ms=$timer.ElapsedMilliseconds; marker=$fixture.marker })
            } finally { if ($null -ne $process) { $process.Dispose() } }
        }
    }
    foreach ($operationName in @('Register','Unregister')) {
        foreach ($code in @(0,7)) {
            Invoke-Case 'native-gui-helper' "Exact helper $operationName waits for GUI exit $code and correct argv" {
                $fixture = Initialize-NativeCase $operationName $code
                $timer = [Diagnostics.Stopwatch]::StartNew()
                $action = { Invoke-LavRegistrationProcess -Executable $nativeExe -ModulePath $fixture.module -Unregister ($operationName -eq 'Unregister') }
                if ($code -eq 0) { & $action } else { Assert-Throws $action 'exit 7' }
                $timer.Stop()
                Assert-True ($timer.ElapsedMilliseconds -ge 600) 'Helper returned before the GUI delay'
                Assert-Marker $fixture.marker $code ($operationName -eq 'Unregister')
                Assert-True (-not (Test-Path -LiteralPath $fixture.module)) 'A native test created a module'
                $script:NativeObservations.Add([pscustomobject]@{ type='exact-helper'; operation=$operationName; expected_exit_code=$code; outcome=$(if ($code -eq 0) { 'returned-success' } else { 'threw-exit-7' }); elapsed_ms=$timer.ElapsedMilliseconds; marker=$fixture.marker })
            }
        }
    }
} catch {
    $script:BootstrapError = $_.ToString()
} finally {
    foreach ($key in $savedEnvironment.Keys) { [Environment]::SetEnvironmentVariable($key, $savedEnvironment[$key], 'Process') }
    if ($null -eq $savedLastExitCode) { Remove-Variable -Name LASTEXITCODE -Scope Global -ErrorAction SilentlyContinue }
    else { Set-Variable -Name LASTEXITCODE -Scope Global -Value $savedLastExitCode.Value }
}

$failed = @($script:Results | Where-Object { $_.status -eq 'failed' }).Count
$passed = @($script:Results | Where-Object { $_.status -eq 'passed' }).Count
$okay = $null -eq $script:BootstrapError -and $failed -eq 0 -and $script:NativeStarted -and $script:NativeObservations.Count -eq 6
$suites = @($script:Results | Group-Object suite | ForEach-Object {
    [pscustomobject]@{ name=$_.Name; passed=@($_.Group | Where-Object { $_.status -eq 'passed' }).Count; failed=@($_.Group | Where-Object { $_.status -eq 'failed' }).Count }
})
$nativeCases = @($script:Results | Where-Object { $_.suite -like 'native-gui-*' })
$nativePassed = @($nativeCases | Where-Object { $_.status -eq 'passed' }).Count
$receipt = [ordered]@{
    schema_version=1; generated_utc=[DateTime]::UtcNow.ToString('o'); status=$(if ($okay) { 'passed' } else { 'failed' });
    registration_script=$RegistrationScript; script_sha256=$script:ScriptHash; registration_script_sha256=$script:ScriptHash; exact_helper_sha256=$script:HelperHash;
    probe_source_sha256=$(if (Test-Path -LiteralPath (Join-Path $PSScriptRoot 'lav_registration_gui_probe.cpp')) { (Get-FileHash -LiteralPath (Join-Path $PSScriptRoot 'lav_registration_gui_probe.cpp') -Algorithm SHA256).Hash.ToLowerInvariant() } else { $null });
    probe_executable_sha256=$script:ProbeHash; powershell_version=$PSVersionTable.PSVersion.ToString();
    powershell_edition=$PSVersionTable.PSEdition; operating_system=[Environment]::OSVersion.VersionString; process_is_64bit=[Environment]::Is64BitProcess;
    native_tests_started=$script:NativeStarted; real_registration_attempted=$false; bootstrap_error=$script:BootstrapError;
    total=$script:Results.Count; passed=$passed; failed=$failed; suites=$suites; cases=@($script:Results.ToArray());
    native_gui=[ordered]@{ status=$(if ($nativePassed -eq 6 -and $script:NativeObservations.Count -eq 6) { 'passed' } else { 'failed' }); total=$nativeCases.Count; passed=$nativePassed; control_cases=2; helper_cases=4 };
    native_observations=@($script:NativeObservations.ToArray()); fixture_directory=$script:TestRoot
}
$json = $receipt | ConvertTo-Json -Depth 8
$receiptFullPath = [IO.Path]::GetFullPath($ReceiptPath)
$receiptDirectory = [IO.Path]::GetDirectoryName($receiptFullPath)
if (-not (Test-Path -LiteralPath $receiptDirectory -PathType Container)) { New-Item -ItemType Directory -Path $receiptDirectory -Force | Out-Null }
[IO.File]::WriteAllText($receiptFullPath, $json, (New-Object Text.UTF8Encoding($false)))
Write-Output $json
if (-not $okay) { exit 1 }
exit 0
