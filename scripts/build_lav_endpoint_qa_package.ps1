# SPDX-FileCopyrightText: 2026 OpenJOC contributors
# SPDX-License-Identifier: Apache-2.0

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$RuntimeDirectory,
    [Parameter(Mandatory = $true)]
    [string]$FixtureDirectory,
    [Parameter(Mandatory = $true)]
    [string]$OutputZip
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2.0

$runtime = (Resolve-Path -LiteralPath $RuntimeDirectory).Path
$fixtures = (Resolve-Path -LiteralPath $FixtureDirectory).Path
$output = [IO.Path]::GetFullPath($OutputZip)
if ([IO.Path]::GetExtension($output) -ne '.zip') {
    throw 'OutputZip must end in .zip.'
}
if (Test-Path -LiteralPath $output) {
    throw "OutputZip must not already exist: $output"
}
if (-not (Test-Path -LiteralPath (Split-Path -Parent $output) -PathType Container)) {
    throw 'OutputZip parent directory does not exist.'
}

$requiredRuntime = @(
    'OpenJocDirectShowNegotiationSmoke.exe',
    'OpenJocDirectShowNegotiationSmoke.exe.manifest',
    'LAVAudio.ax',
    'LAVSplitter.ax',
    'openjoc_capi.dll',
    'libbluray.dll'
)
$requiredFixtures = @('joc.lifecycle.ec3', 'joc.lifecycle.mp4')
foreach ($name in $requiredRuntime) {
    if (-not (Test-Path -LiteralPath (Join-Path $runtime $name) -PathType Leaf)) {
        throw "Runtime file is missing: $name"
    }
}
foreach ($name in $requiredFixtures) {
    if (-not (Test-Path -LiteralPath (Join-Path $fixtures $name) -PathType Leaf)) {
        throw "Fixture is missing: $name"
    }
}

$tempBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$staging = Join-Path $tempBase ('OpenJocEndpointQa-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $staging | Out-Null
try {
    $stagedRuntime = Join-Path $staging 'runtime'
    $stagedFixtures = Join-Path $staging 'fixtures'
    New-Item -ItemType Directory -Path $stagedRuntime, $stagedFixtures | Out-Null
    Get-ChildItem -LiteralPath $runtime -File |
        Where-Object { $_.Name -ne 'OpenJocRuntimeIdentity.tsv' } |
        Copy-Item -Destination $stagedRuntime
    foreach ($name in $requiredFixtures) {
        Copy-Item -LiteralPath (Join-Path $fixtures $name) -Destination $stagedFixtures
    }
    $qaSource = Join-Path $PSScriptRoot 'windows_multichannel_qa'
    Copy-Item -LiteralPath (Join-Path $qaSource 'Run-OpenJocEndpointQa.ps1') -Destination $staging
    Copy-Item -LiteralPath (Join-Path $qaSource 'Run-OpenJocEndpointQa.cmd') -Destination $staging
    Copy-Item -LiteralPath (Join-Path $qaSource 'README.md') -Destination $staging

    $hashRows = foreach ($file in Get-ChildItem -LiteralPath $staging -File -Recurse | Sort-Object FullName) {
        $relative = $file.FullName.Substring($staging.Length + 1)
        "{0}`t{1}" -f (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash, $relative
    }
    $hashRows | Set-Content -LiteralPath (Join-Path $staging 'PACKAGE_SHA256.tsv') -Encoding UTF8
    Compress-Archive -Path (Join-Path $staging '*') -DestinationPath $output -CompressionLevel Optimal
}
finally {
    $resolvedStaging = [IO.Path]::GetFullPath($staging)
    if ($resolvedStaging.StartsWith($tempBase, [StringComparison]::OrdinalIgnoreCase) -and
        (Split-Path -Leaf $resolvedStaging).StartsWith('OpenJocEndpointQa-', [StringComparison]::Ordinal)) {
        Remove-Item -LiteralPath $resolvedStaging -Recurse -Force
    }
}
Write-Host "QA package created: $output"
