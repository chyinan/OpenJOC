<!--
SPDX-FileCopyrightText: 2026 OpenJOC contributors
SPDX-License-Identifier: Apache-2.0
-->

# OpenJOC LAV 0.10.0 distribution closure

Status: `PASS_PUBLIC_SOURCE_CANDIDATE`

The frozen local candidates completed the factual security, provenance,
license-notice, corresponding-source, and technical gates. The accepted
downstream source is now public at `chyinan/LAVFilters-OpenJOC`, branch
`openjoc-main`, tag `openjoc-0.10.0`.

The compiled `LAVAudio.ax` inputs are fully classified. The LAV/OpenJOC glue
is GPL-2.0-or-later, two inherited MPC-HC CSS units are GPL-3.0-only, and the
FFmpeg DLL build reports GPL version 3 or later. The effective combined binary
distribution classification is therefore GPL-3.0-only. No GPL-2.0-only input
and no known license incompatibility were found.

The corresponding-source candidate contains the exact LAV/FFmpeg state,
modified and newly created source, complete OpenJOC Cargo workspace, authentic
sanitized FFmpeg configuration evidence, dependency source archives, and the
exact MSYS2 GCC 16.2.0-3 source-only package for `libgcc_s_seh-1.dll`.

The main OpenJOC v0.10.0 release must attach the unchanged binary and
corresponding-source candidates. Repeat independent distribution review if
any binary, source, notice, or license input changes.
