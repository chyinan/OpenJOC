# OpenJOC 0.9.0 — Interchange & Ecosystem

OpenJOC 0.9 adds standards-based interchange and a qualified ecosystem
distribution surface around the existing decoder, renderer, and player
integrations.

## Highlights

- Reconstructed ADM/BW64 export with deterministic XML, BW64 `ds64`, `axml`,
  `chna`, signed 24-bit PCM, structural validation, and a machine-readable
  reconstruction report.
- OpenJOC-enabled FFmpeg integration bundles for macOS arm64, Linux x86_64,
  and Windows x64.
- Feature-enabled GStreamer plugin packs for the tested GStreamer runtime
  baseline; the plugin pack does not claim compatibility with arbitrary
  GStreamer ABI versions.
- Developer SDK archives containing `openjoc.h`, C ABI 1.3 libraries,
  pkg-config/CMake metadata where supported, and a small C consumer example.
- `openjoc self-test` plus a project-owned synthetic JOC fixture for public
  classifier, decode, speaker, binaural, HRTF, and ADM health checks.
- Existing OpenJOC-enabled mpv bundles, native 22.2 speaker rendering, and
  built-in SADIE II generic binaural HRTF remain part of the release surface.

## ADM boundary

The ADM/BW64 exporter produces a reconstructed representation of the scene
recoverable from JOC. It does not recover the original authoring ADM master.
Information discarded or transformed by lossy encoding is unrecoverable.

The current renderer-independent OpenJOC scene contract records the
audio-to-spatial-metadata association as unresolved. Best-effort export emits
deterministic neutral reconstructed signals and records the unbound metadata;
strict export rejects the unresolved binding. The exporter never bakes
FinalLinkedGain, HRTF, or a 7.1.4/22.2 speaker render into ADM.

## Distribution and qualification

All archives record the exact OpenJOC commit, pinned FFmpeg integration
revision/patch hash where applicable, dependency and license inventories,
private-path scans, extraction checksums, and the tested runtime baseline.
The FFmpeg archives are custom OpenJOC builds, not official upstream FFmpeg
releases. GStreamer runtime installation remains a user responsibility.

## Known limitations

- Real Logic Pro, DaVinci Resolve, Dolby Atmos Renderer, Nuendo, and Pro Tools
  ADM import testing is deferred; 0.9 does not claim DAW round-trip or
  mastering-interchange certification.
- Original ADM names, hierarchy, UIDs, source PCM, and encoder-side decisions
  are not recoverable from JOC.
- The C ABI remains experimental during the 0.x release line.
- Physical Linux/Windows multichannel speaker hardware playback remains outside
  automated qualification.
- The macOS player package remains ad-hoc signed and not notarized unless the
  platform workflow reports otherwise.
