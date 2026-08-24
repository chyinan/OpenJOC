# SPDX-FileCopyrightText: 2026 OpenJOC contributors
# SPDX-License-Identifier: Apache-2.0

[CmdletBinding()]
param(
    [string]$RendererMoniker,
    [string]$EndpointId,
    [ValidateSet('DirectSound', 'WaveOut')]
    [string]$RendererFamily = 'DirectSound',
    [ValidateSet('Unclassified', 'VirtualWindowsDriver', 'PhysicalEndpoint')]
    [string]$EndpointKind = 'Unclassified',
    [switch]$InventoryOnly,
    [string]$OutputDirectory = (Join-Path (Get-Location) ('OpenJocEndpointQa-' + (Get-Date -Format 'yyyyMMdd-HHmmss')))
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2.0

function Get-SingleTsvValue {
    param([string[]]$Lines, [string]$Key)
    $prefix = $Key + "`t"
    $matches = @($Lines | Where-Object { $_.StartsWith($prefix, [StringComparison]::Ordinal) })
    if ($matches.Count -ne 1) {
        throw "Expected exactly one '$Key' row, observed $($matches.Count)."
    }
    return $matches[0].Substring($prefix.Length)
}

function Get-AttributeMap {
    param([string]$Line, [int]$StartIndex)
    $segments = $Line -split "`t"
    $result = @{}
    for ($index = $StartIndex; $index -lt $segments.Count; ++$index) {
        $separator = $segments[$index].IndexOf('=')
        if ($separator -gt 0) {
            $result[$segments[$index].Substring(0, $separator)] =
                $segments[$index].Substring($separator + 1)
        }
    }
    return $result
}

function Invoke-Harness {
    param([string[]]$Arguments)
    & $script:Harness @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "Harness failed with exit code ${LASTEXITCODE}: $($Arguments -join ' ')"
    }
}

if (-not $InventoryOnly -and ([string]::IsNullOrWhiteSpace($RendererMoniker) -or
        [string]::IsNullOrWhiteSpace($EndpointId))) {
    throw 'RendererMoniker and EndpointId are required unless -InventoryOnly is used.'
}

$packageRoot = $PSScriptRoot
$packageRuntime = Join-Path $packageRoot 'runtime'
$fixtureRoot = Join-Path $packageRoot 'fixtures'
$packageHashes = Join-Path $packageRoot 'PACKAGE_SHA256.tsv'
foreach ($required in @(
        $packageRuntime,
        $fixtureRoot,
        $packageHashes,
        (Join-Path $packageRuntime 'OpenJocDirectShowNegotiationSmoke.exe'),
        (Join-Path $fixtureRoot 'joc.lifecycle.ec3'),
        (Join-Path $fixtureRoot 'joc.lifecycle.mp4')
    )) {
    if (-not (Test-Path -LiteralPath $required)) {
        throw "Package input is missing: $required"
    }
}

foreach ($row in Get-Content -LiteralPath $packageHashes -Encoding UTF8) {
    if ([string]::IsNullOrWhiteSpace($row)) { continue }
    $fields = $row -split "`t", 2
    if ($fields.Count -ne 2 -or $fields[0] -notmatch '^[0-9A-Fa-f]{64}$') {
        throw 'PACKAGE_SHA256.tsv is malformed.'
    }
    $path = Join-Path $packageRoot $fields[1]
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Hashed package file is missing: $($fields[1])"
    }
    $actual = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash
    if ($actual -ne $fields[0]) {
        throw "Package hash mismatch: $($fields[1])"
    }
}

$output = [IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path -LiteralPath $output) {
    throw "OutputDirectory must not already exist: $output"
}
New-Item -ItemType Directory -Path $output | Out-Null
$reportRuntime = Join-Path $output 'report-runtime'
Copy-Item -LiteralPath $packageRuntime -Destination $reportRuntime -Recurse
$script:Harness = Join-Path $reportRuntime 'OpenJocDirectShowNegotiationSmoke.exe'
$manifest = Join-Path $reportRuntime 'OpenJocRuntimeIdentity.tsv'
Invoke-Harness @('--write-manifest', $reportRuntime, $manifest)
(Get-Item -LiteralPath $manifest).IsReadOnly = $true

$inventoryPath = Join-Path $output 'renderer-inventory.tsv'
Invoke-Harness @('--list-audio-renderers', $inventoryPath)
if ($InventoryOnly) {
    $inventoryReport = [ordered]@{
        schemaVersion = 1
        mode = 'INVENTORY_ONLY'
        generatedUtc = [DateTime]::UtcNow.ToString('o')
        rendererInventory = $inventoryPath
        rendererInventorySha256 = (Get-FileHash -LiteralPath $inventoryPath -Algorithm SHA256).Hash
        systemStateChanged = $false
    }
    $inventoryReport | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $output 'report.json') -Encoding UTF8
    Write-Host "Inventory complete: $output"
    exit 0
}

$capabilitiesPath = Join-Path $output 'endpoint-capabilities.tsv'
Invoke-Harness @('--inspect-audio-endpoint', $EndpointId, $capabilitiesPath)
$evidenceDirectory = Join-Path $output 'attempts'
New-Item -ItemType Directory -Path $evidenceDirectory | Out-Null

# Fixed policy enum values 0..6; no layout is inferred from the endpoint label.
$layouts = @(
    [pscustomobject]@{ Policy = 0; Layout = 'Stereo'; Channels = 2; Mask = '0x00000003'; Order = @('FL','FR') },
    [pscustomobject]@{ Policy = 1; Layout = '5.1'; Channels = 6; Mask = '0x0000060f'; Order = @('FL','FR','FC','LFE','Ls','Rs') },
    [pscustomobject]@{ Policy = 2; Layout = '7.1'; Channels = 8; Mask = '0x0000063f'; Order = @('FL','FR','FC','LFE','Lb','Rb','Ls','Rs') },
    [pscustomobject]@{ Policy = 3; Layout = '5.1.2'; Channels = 8; Mask = '0x0000560f'; Order = @('FL','FR','FC','LFE','Ls','Rs','TFL','TFR') },
    [pscustomobject]@{ Policy = 4; Layout = '5.1.4'; Channels = 10; Mask = '0x0002d60f'; Order = @('FL','FR','FC','LFE','Ls','Rs','TFL','TFR','TBL','TBR') },
    [pscustomobject]@{ Policy = 5; Layout = '7.1.2'; Channels = 10; Mask = '0x0000563f'; Order = @('FL','FR','FC','LFE','Lb','Rb','Ls','Rs','TFL','TFR') },
    [pscustomobject]@{ Policy = 6; Layout = '7.1.4'; Channels = 12; Mask = '0x0002d63f'; Order = @('FL','FR','FC','LFE','Lb','Rb','Ls','Rs','TFL','TFR','TBL','TBR') }
)

$attempts = @()
foreach ($layout in $layouts) {
    foreach ($container in @('raw', 'mp4')) {
        $fixtureName = if ($container -eq 'raw') { 'joc.lifecycle.ec3' } else { 'joc.lifecycle.mp4' }
        $fixture = Join-Path $fixtureRoot $fixtureName
        $evidencePath = Join-Path $evidenceDirectory ("policy-{0}-{1}.tsv" -f $layout.Policy, $container)
        Invoke-Harness @(
            '--native-renderer-probe', $reportRuntime, $manifest, $fixture,
            $RendererMoniker, [string]$layout.Policy, $evidencePath
        )
        $lines = @(Get-Content -LiteralPath $evidencePath -Encoding UTF8)
        $initialLine = @($lines | Where-Object { $_ -like "operation`t1`tinitial_stream`t*" })
        $preLine = @($lines | Where-Object { $_ -like "type_observation`tpre_stream`t*" })
        if ($initialLine.Count -ne 1 -or $preLine.Count -ne 1) {
            throw "Probe evidence is incomplete: $evidencePath"
        }
        $initial = Get-AttributeMap $initialLine[0] 3
        $pre = Get-AttributeMap $preLine[0] 2
        $result = Get-SingleTsvValue $lines 'result'
        $connectHresult = Get-SingleTsvValue $lines 'connect_direct_hr'
        $deliveryObserved =
            $connectHresult -eq '0x00000000' -and
            $pre['renderer_input_exact'] -eq '1' -and
            [uint64]$initial['classifier_bytes'] -gt 0 -and
            [uint64]$initial['stream_bytes'] -gt 0 -and
            [uint32]$initial['midstream_last_buffer_duration'] -gt 0 -and
            $initial['eos_complete'] -eq '1' -and
            $initial['graph_error_hr'] -eq '0x00000000'
        $attempts += [pscustomobject][ordered]@{
            layout = $layout.Layout
            policy = $layout.Policy
            container = $container
            channelCount = $layout.Channels
            channelMask = $layout.Mask
            semanticChannelOrder = $layout.Order
            sampleFormat = 'WAVEFORMATEXTENSIBLE/IEEE_FLOAT/32-bit/48000Hz'
            blockAlignment = $layout.Channels * 4
            averageBytesPerSecond = 48000 * $layout.Channels * 4
            rendererMoniker = Get-SingleTsvValue $lines 'renderer_moniker'
            result = $result
            proposalCount = [int](Get-SingleTsvValue $lines 'proposal_count')
            fallbackProposals = [int](Get-SingleTsvValue $lines 'fallback_proposals')
            connectDirectHresult = $connectHresult
            requestedMediaType = Get-SingleTsvValue $lines 'requested_type'
            acceptedRendererMediaType = if ($pre['renderer_input_exact'] -eq '1') { $pre['renderer_input_type'] } else { $null }
            sampleDelivery = [ordered]@{
                observed = $deliveryObserved
                classifierBytes = [uint64]$initial['classifier_bytes']
                streamBytes = [uint64]$initial['stream_bytes']
                rendererBufferDuration = [uint32]$initial['midstream_last_buffer_duration']
                eos = $initial['eos_complete'] -eq '1'
                graphErrorHresult = $initial['graph_error_hr']
            }
            rawEvidence = $evidencePath
            rawEvidenceSha256 = (Get-FileHash -LiteralPath $evidencePath -Algorithm SHA256).Hash
        }
    }
}

$allDelivered = @($attempts | Where-Object { -not $_.sampleDelivery.observed }).Count -eq 0
$verifiedEndpointResult = switch ($EndpointKind) {
    'VirtualWindowsDriver' { 'VIRTUAL_WINDOWS_ENDPOINT_VERIFIED' }
    'PhysicalEndpoint' { 'REAL_ENDPOINT_VERIFIED' }
    default { 'WINDOWS_ENDPOINT_SAMPLE_DELIVERY_VERIFIED' }
}
$report = [ordered]@{
    schemaVersion = 1
    generatedUtc = [DateTime]::UtcNow.ToString('o')
    rendererFamily = $RendererFamily
    endpointKind = $EndpointKind
    rendererMoniker = $RendererMoniker
    endpointId = $EndpointId
    automaticLayoutSelection = 'AUTO_NOT_RELIABLE'
    proposalPolicy = 'ONE_EXACT_WAVEFORMATEXTENSIBLE_PROPOSAL_NO_FALLBACK'
    endpointResult = if ($allDelivered) { $verifiedEndpointResult } else { 'ENDPOINT_REJECTED_OR_UNVERIFIED' }
    physicalMultichannelHardware = 'NOT_TESTED'
    rendererInventory = $inventoryPath
    rendererInventorySha256 = (Get-FileHash -LiteralPath $inventoryPath -Algorithm SHA256).Hash
    endpointCapabilities = $capabilitiesPath
    endpointCapabilitiesSha256 = (Get-FileHash -LiteralPath $capabilitiesPath -Algorithm SHA256).Hash
    attempts = $attempts
    systemStateChanged = $false
}
$report | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath (Join-Path $output 'report.json') -Encoding UTF8
Write-Host "Endpoint QA complete: $output"
