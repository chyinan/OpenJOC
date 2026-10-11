# Source and licenses for the optional full bundle

This bundle combines differently licensed components. No single blanket license
covers its binaries. `LICENSES/lav/` preserves the license/copyright notices
from the LAV source tree used for this build, including LAV, FFmpeg, libbluray,
Quick Sync, and nested component notices. The packager requires the primary
notices to be present and does not download replacement license text.
`LICENSES/openjoc/` contains OpenJOC's root license and third-party notices from
the actual source checkout used to build the CAPI, supplied explicitly through
`--core-source-root`.

Source projects:

- OpenJOC core: https://github.com/chyinan/OpenJOC
- LAV integration: https://github.com/chyinan/LAVFilters-OpenJOC
- LAV FFmpeg: https://gitea.1f0.de/LAV/FFmpeg
- zlib: https://github.com/madler/zlib

These links identify source repositories, not immutable build provenance.
Use the exact revisions and build configuration recorded by this release's
workflow; recursively check out the LAV submodules selected by that build.
`libbluray` and `qsdecoder` are separate LAV submodules. Do not substitute source
or binaries from an older CI candidate based only on the package version.

The LAV/FFmpeg GPL and corresponding-source requirements and upstream notices
remain applicable. Microsoft CRT/UCRT redistributables and MinGW support DLLs
are collected runtime dependencies; this package does not claim to have rebuilt
them. A copied license inventory is not a legal review or a complete
corresponding-source distribution.
