# SPDX-FileCopyrightText: 2026 OpenJOC contributors
# SPDX-License-Identifier: Apache-2.0
# pattern: Functional Core

Set-StrictMode -Version 2.0

function Get-OpenJocRequiredRuntimeFiles {
    @(
        'LAVAudio.ax',
        'LAVAudio.ax.manifest',
        'LAVFilters.Dependencies.manifest',
        'openjoc_capi.dll',
        'avcodec-lav-63.dll',
        'avfilter-lav-12.dll',
        'avformat-lav-63.dll',
        'avutil-lav-61.dll',
        'swresample-lav-7.dll',
        'swscale-lav-10.dll',
        'libbluray.dll',
        'zlib1.dll',
        'libgcc_s_seh-1.dll',
        'libwinpthread-1.dll',
        'api-ms-win-crt-conio-l1-1-0.dll',
        'api-ms-win-crt-convert-l1-1-0.dll',
        'api-ms-win-crt-environment-l1-1-0.dll',
        'api-ms-win-crt-filesystem-l1-1-0.dll',
        'api-ms-win-crt-heap-l1-1-0.dll',
        'api-ms-win-crt-locale-l1-1-0.dll',
        'api-ms-win-crt-math-l1-1-0.dll',
        'api-ms-win-crt-multibyte-l1-1-0.dll',
        'api-ms-win-crt-private-l1-1-0.dll',
        'api-ms-win-crt-process-l1-1-0.dll',
        'api-ms-win-crt-runtime-l1-1-0.dll',
        'api-ms-win-crt-stdio-l1-1-0.dll',
        'api-ms-win-crt-string-l1-1-0.dll',
        'api-ms-win-crt-time-l1-1-0.dll',
        'api-ms-win-crt-utility-l1-1-0.dll',
        'ucrtbase.dll',
        'vcruntime140.dll',
        'vcruntime140_1.dll',
        'vcruntime140_threads.dll'
    )
}

function Get-OpenJocClassIds {
    [ordered]@{
        OpenJocAudio = '{27247580-C701-40CD-886D-E618FC8C9FFF}'
        StockLavAudio = '{E8E73B6B-4CB3-44A4-BE99-4F7BCB96E491}'
        AudioSettings = '{2D8F1801-A70D-48F4-B76B-7F5AE022AB54}'
        AudioMixing = '{C89FC33C-E60A-4C97-BEF4-ACC5762B6404}'
        AudioFormats = '{BD72668E-6BFF-4CD1-8480-D465708B336B}'
        AudioStatus = '{20ED4A03-6AFD-4FD9-980B-2F6143AA0892}'
    }
}

function ConvertTo-OpenJocCommandLineArgument {
    param([AllowEmptyString()][string]$Value)

    if ($null -eq $Value) { return '""' }
    $builder = New-Object System.Text.StringBuilder
    [void]$builder.Append('"')
    $slashes = 0
    foreach ($character in $Value.ToCharArray()) {
        if ($character -eq '\') {
            $slashes++
            continue
        }
        if ($character -eq '"') {
            [void]$builder.Append(('\' * (($slashes * 2) + 1)))
            [void]$builder.Append('"')
            $slashes = 0
            continue
        }
        if ($slashes -gt 0) {
            [void]$builder.Append(('\' * $slashes))
            $slashes = 0
        }
        [void]$builder.Append($character)
    }
    if ($slashes -gt 0) {
        [void]$builder.Append(('\' * ($slashes * 2)))
    }
    [void]$builder.Append('"')
    $builder.ToString()
}

function Test-OpenJocSamePath {
    param([string]$Left, [string]$Right)
    if ([string]::IsNullOrWhiteSpace($Left) -or [string]::IsNullOrWhiteSpace($Right)) {
        return $false
    }
    try {
        return [IO.Path]::GetFullPath($Left).TrimEnd('\') -ieq [IO.Path]::GetFullPath($Right).TrimEnd('\')
    } catch {
        return $false
    }
}

function Test-OpenJocSafeOwnedRoot {
    param([string]$Path)
    if ([string]::IsNullOrWhiteSpace($Path)) { return $false }
    try { $full = [IO.Path]::GetFullPath($Path).TrimEnd('\') } catch { return $false }
    if ($full.Length -lt 16) { return $false }
    if ([IO.Path]::GetPathRoot($full).TrimEnd('\') -ieq $full) { return $false }
    return $true
}

function Get-OpenJocUninstallDesiredSnapshot {
    param($CurrentSnapshot, $InstalledBaseline, [string]$OpenJocClassId)
    $currentMain = @($CurrentSnapshot | Where-Object { $_.ClassId -ieq $OpenJocClassId })
    $originalMain = @($InstalledBaseline | Where-Object { $_.ClassId -ieq $OpenJocClassId })
    if ($currentMain.Count -ne 1 -or $originalMain.Count -ne 1) {
        throw 'OpenJOC uninstall requires exactly one current and one original main-class snapshot.'
    }
    foreach ($item in @($CurrentSnapshot)) {
        if ($item.ClassId -ieq $OpenJocClassId) { $originalMain[0] } else { $item }
    }
}

Export-ModuleMember -Function @(
    'Get-OpenJocRequiredRuntimeFiles',
    'Get-OpenJocClassIds',
    'ConvertTo-OpenJocCommandLineArgument',
    'Test-OpenJocSamePath',
    'Test-OpenJocSafeOwnedRoot',
    'Get-OpenJocUninstallDesiredSnapshot'
)
