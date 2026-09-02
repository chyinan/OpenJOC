<!--
SPDX-FileCopyrightText: 2026 OpenJOC contributors
SPDX-License-Identifier: Apache-2.0
-->

# Third-party notices

`LAVAudio.ax` derives from LAV Filters 0.83 at revision
`fefb6987994ed56e4525e8a125f5fbb53707bc52`. OpenJOC downstream changes are
identified per modified file and in the source-state patch. The exact input
census and DirectShow/DSUtilLite ancestry are supplied with the source archive.

The bundled FFmpeg DLLs derive from revision
`599d3a140460e1b57c234fe064db5185fb76ee5b` with GPL and version-3 components
enabled and nonfree components disabled. GPLv3 text is included. Static input
source archives are included for dav1d 1.5.3, GMP 6.3.0, Nettle 3.10.2,
GnuTLS 3.8.13, OpenCORE-AMR 0.1.6, Speex 1.2.1, and libxml2 2.11.5.

The DirectShow Base Classes originate in Microsoft sample code under MIT.
Two inherited CSS units originate in the MPC-HC GPLv3 source tree. Their
license and ancestry evidence are retained in the source archive.

`libbluray.dll` derives from libbluray `2df828e...` and libudfread
`139a219...`. `zlib1.dll` is MSYS2 zlib 1.3.2-2. `libgcc_s_seh-1.dll` is from
MSYS2 `mingw-w64-x86_64-gcc-libs` 16.2.0-3; the package's GPLv3, LGPL, and GCC
Runtime Library Exception texts are included, and its exact official
source-only package is attached to corresponding source. `libwinpthread-1.dll`
retains its MIT/BSD notices.

The Microsoft UCRT and VC runtime DLLs are app-local redistributables and are
covered by the included Microsoft redistributable notice and applicable
Microsoft terms.

OpenJOC embeds a derived CDF-1 SADIE II D1 KU100 resource. The exact packaged
file is `crates/openjoc-sofa/assets/sadie-ii-d1-48k-256tap.sofa`, generated
from the authorized upstream D1 SOFA (upstream SHA-256
`e6c72a84dd947b5ef75438ab96a9c2a32ed10f033472b9c4c11a49aff00a8a31`) by
`tools/generate-builtin-hrtf.py`; the generated SHA-256 is
`b9bcecd8a07e7eed4474a9b063c47672384339e83605bd245ff0adc098869fab`.
The embedded metadata retains the Apache License 2.0 notice. University of
York attribution and the transformation record remain in the corresponding
source.

This notice does not imply endorsement by any upstream project or vendor.
