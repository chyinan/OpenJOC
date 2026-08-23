<!--
SPDX-FileCopyrightText: 2026 OpenJOC contributors
SPDX-License-Identifier: Apache-2.0
-->

# Third-party component matrix

| Component | Exact version/revision | Binary form | Corresponding source | Classification | Status |
|---|---|---|---|---|---|
| LAV Audio Decoder + OpenJOC glue | downstream `b06ba2c...` on LAV 0.83 / upstream base `fefb698...` + recorded changes | `LAVAudio.ax` | Full snapshot, patch, new files | Effective GPL-3.0-only combined work | COMPLETE |
| FFmpeg LAV runtime | `599d3a1...` | Six DLLs | Full snapshot + sanitized config evidence | GPL-3.0-or-later | COMPLETE |
| DirectShow Base Classes | Microsoft sample `d59e5f1...`, LAV lineage | Linked in AX | Full source + per-file census | MIT; seven units also GPL-2.0-or-later | COMPLETE |
| MPC-HC CSS units | MPC-HC `dcbf6bf...`, LAV import `bd86f1c...` | Linked in AX | Full source + ancestry evidence | GPL-3.0-only | COMPLETE |
| libbluray/libudfread | `2df828e...` / `139a219...` | DLL / embedded | Recursive snapshots | LGPL-2.1-or-later family | COMPLETE |
| dav1d | 1.5.3 | Static FFmpeg input | Exact source archive | BSD-2-Clause | COMPLETE |
| GMP | 6.3.0 | Static FFmpeg input | Exact source archive | LGPL-3.0-or-later / GPL-2.0-or-later | COMPLETE |
| Nettle | 3.10.2 | Static FFmpeg input | Exact source archive | LGPL-3.0-or-later with file notices | COMPLETE |
| GnuTLS | 3.8.13 | Static FFmpeg input | Exact source archive | LGPL-2.1-or-later with bundled notices | COMPLETE |
| OpenCORE-AMR | 0.1.6 | Static FFmpeg input | Exact source archive | Apache-2.0 | COMPLETE |
| Speex | 1.2.1 | Static FFmpeg input | Exact source archive | BSD-style | COMPLETE |
| libxml2 | 2.11.5 | Static FFmpeg input | Exact source archive | MIT | COMPLETE |
| zlib | MSYS2 1.3.2-2 / source 1.3.2 | DLL | Exact source archive | Zlib | COMPLETE |
| libgcc runtime | MSYS2 GCC 16.2.0-3 | DLL | Exact official source-only package | GPL-3.0-or-later WITH GCC-exception-3.1 | COMPLETE |
| libwinpthread | MSYS2 `14.0.0.r283.ga7cb47123-1` | DLL | LAV/MSYS2 source material | MIT AND BSD-3-Clause-Clear | COMPLETE |
| Microsoft UCRT/VC runtime | UCRT 10.0.19041.685; VC 14.50.35719.0 | App-local DLLs | Microsoft redist terms apply | Microsoft redistributable | COMPLETE |
| SADIE II D1 KU100 resource | recorded OpenJOC asset hash/provenance | Embedded in C ABI DLL | Exact resource + provenance | Upstream attribution terms | COMPLETE |

Windows system/API-set DLLs are operating-system dependencies and are not
counted as redistributed project components.
