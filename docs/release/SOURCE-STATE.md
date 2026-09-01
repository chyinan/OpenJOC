<!--
SPDX-FileCopyrightText: 2026 OpenJOC contributors
SPDX-License-Identifier: Apache-2.0
-->

# OpenJOC LAV 0.15.0 source state

- OpenJOC revision: exact remediation branch HEAD is recorded in `REPRODUCIBILITY-MANIFEST.txt`
- LAV upstream base: `fefb6987994ed56e4525e8a125f5fbb53707bc52`
- FFmpeg recursive revision: `599d3a140460e1b57c234fe064db5185fb76ee5b`
- libbluray revision: `2df828e7dfef1d8c3fe7ebc2e8b764064a3f69f3`
- libudfread revision: `139a2194525f2745b98a98e4d8fa627d07440176`
- qsdecoder revision: `72e6b6a944460d3cbeffe13e78b88dd773a85602`

The source candidate includes all 14 modified upstream LAV files, all 30 new
OpenJOC LAV integration/contract/test/smoke source files, the three provenance
and census resources, and a generated tracked-change patch. It also includes
the complete OpenJOC Cargo workspace, including `tools/import-etsi-tables`.

The public downstream repository is
[`chyinan/LAVFilters-OpenJOC`](https://github.com/chyinan/LAVFilters-OpenJOC)
with branch `openjoc-main`, downstream commit
`dce9fb9f281fd16a4221b46d975bfd90c2e809f8`, and upstream base
`fefb6987994ed56e4525e8a125f5fbb53707bc52`.
The corresponding-source archive remains the authoritative full closure,
including source components not represented by the public LAV fork.
