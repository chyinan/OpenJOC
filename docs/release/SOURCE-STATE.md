<!--
SPDX-FileCopyrightText: 2026 OpenJOC contributors
SPDX-License-Identifier: Apache-2.0
-->

# OpenJOC LAV 0.10.0 source state

- OpenJOC revision: `a4e5964eec42eb41b9e7ca0ffd82c03903bfe4be`
- LAV upstream base: `fefb6987994ed56e4525e8a125f5fbb53707bc52`
- FFmpeg recursive revision: `599d3a140460e1b57c234fe064db5185fb76ee5b`
- libbluray revision: `2df828e7dfef1d8c3fe7ebc2e8b764064a3f69f3`
- libudfread revision: `139a2194525f2745b98a98e4d8fa627d07440176`
- qsdecoder revision: `72e6b6a944460d3cbeffe13e78b88dd773a85602`

The source candidate includes all seven modified upstream LAV files, all seven
new OpenJOC LAV integration/test/smoke source files, the three provenance and
census resources, and a generated tracked-change patch. It also includes the
complete OpenJOC Cargo workspace, including `tools/import-etsi-tables`.

The public downstream repository is
[`chyinan/LAVFilters-OpenJOC`](https://github.com/chyinan/LAVFilters-OpenJOC)
with branch `openjoc-main`, commit
`b06ba2cbbd5c8806ca4423a8ff1527e4e2bd6a27`, and immutable tag
[`openjoc-0.10.0`](https://github.com/chyinan/LAVFilters-OpenJOC/releases/tag/openjoc-0.10.0).
The corresponding-source archive remains the authoritative full closure,
including source components not represented by the public LAV fork.
