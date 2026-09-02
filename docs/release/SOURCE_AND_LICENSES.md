<!--
SPDX-FileCopyrightText: 2026 OpenJOC contributors
SPDX-License-Identifier: Apache-2.0
-->

# OpenJOC LAV 0.16.0 source and license boundaries

OpenJOC core and `openjoc_capi.dll` remain Apache-2.0. The downstream LAV
integration code is separately marked GPL-2.0-or-later and does not copy the
OpenJOC Apache header into the LAV license boundary.

The public source is the downstream fork
[`chyinan/LAVFilters-OpenJOC`](https://github.com/chyinan/LAVFilters-OpenJOC),
branch `openjoc-main`, downstream revision
`e12452ead8551cd58f70ce8dc34453eb44ee6a1b`,
based on LAV Filters 0.83. The corresponding-source ZIP remains attached to
the OpenJOC release because it contains the full recursive closure, including
components outside the public LAV fork.

`LAVAudio.ax` is based on LAV Filters 0.83 revision
`fefb6987994ed56e4525e8a125f5fbb53707bc52`. Its exact 65 compiled units are
listed in `LAV_SOURCE_LICENSE_CENSUS.json`: 16 GPL-2.0-or-later LAVAudio units,
15 GPL-2.0-or-later and two GPL-3.0-only DSUtilLite units, 24 MIT DirectShow
Base Classes units, and seven MIT plus GPL-2.0-or-later lineage-modified Base
Classes units. The combined distribution classification is GPL-3.0-only.

The six FFmpeg DLLs come from revision
`599d3a140460e1b57c234fe064db5185fb76ee5b`. The retained configuration has
`CONFIG_GPL=1`, `CONFIG_VERSION3=1`, `CONFIG_NONFREE=0`, and records `GPL
version 3 or later`.

The matching source archive contains:

- the complete OpenJOC Cargo workspace at
  the exact release branch HEAD recorded in `REPRODUCIBILITY-MANIFEST.txt`, including
  `tools/import-etsi-tables`, `Cargo.lock`, build scripts, generated table
  inputs/outputs, and the embedded SADIE resource provenance;
- the exact LAV source state, recursive FFmpeg/libbluray/libudfread/qsdecoder
  snapshots, modification patch, provenance documents, and sanitized FFmpeg
  configuration evidence;
- exact source archives for static FFmpeg inputs, zlib 1.3.1 used to build
  `zlibwapi.dll`, and the matching MSYS2 zlib 1.3.2 source package plus its
  upstream zlib 1.3.2 archive; and
- `mingw-w64-gcc-16.2.0-3.src.tar.zst`, the exact official MSYS2 source-only
  package corresponding to the redistributed GCC runtime DLL.

The binary archive contains no private test media, proprietary Dolby material,
PotPlayer binaries, K-Lite binaries, build objects, PDBs, or Git object
databases. Product names identify compatibility and provenance only; they do
not imply endorsement.
