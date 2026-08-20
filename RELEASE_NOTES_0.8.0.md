# OpenJOC 0.8.0 — Cross-Platform Player Integration

OpenJOC 0.8.0 extends the shared OpenJOC renderer from an embeddable library
surface into a qualified cross-platform player-capable stack. One renderer
provides the same spatial semantics across the CLI, GStreamer, FFmpeg
integrations, and the OpenJOC-enabled mpv bundles.

## Highlights

- Native 22.2 speaker rendering: ITU-R BS.2051-3 Sound System H, 24 PCM
  channels, 22 spatial speakers, and LFE1/LFE2.
- Built-in generic SADIE II D1 KU100 HRTF at 48 kHz and 256 taps, available
  offline through `--binaural`; custom compatible SOFA input remains supported.
- Native Rust GStreamer integration with positive JOC autoplugging while
  ordinary E-AC-3 remains on the normal decoder path.
- External FFmpeg packet/frame bridge and a named native `libopenjoc`
  libavcodec wrapper for custom FFmpeg source builds.
- Pinned mpv integration and reproducible `openjoc-mpv` bundles for macOS
  arm64, Linux x86_64, and Windows x64.

## Player packages

Download the package for the qualified platform, extract it, and run
`bin/openjoc-mpv`. Ordinary E-AC-3 uses `eac3`; confirmed JOC selects
`libopenjoc` automatically. The packaged profiles are:

- `openjoc-headphones`: binaural virtual 7.1.4, two ear channels.
- `openjoc-stereo`: physical 2.0.
- `openjoc-51`: physical 5.1.
- `openjoc-714`: physical 7.1.4.
- `openjoc-916`: physical 9.1.6.
- `openjoc-222`: physical 22.2.

Binaural and physical 2.0 both produce two-channel PCM, but they are different
renders. Explicit `--audio-spdif=eac3` requests compressed passthrough and
bypasses OpenJOC software rendering.

## Integration and licensing notes

The FFmpeg and mpv integrations are project-provided custom patches/builds;
upstream FFmpeg and mpv do not officially ship OpenJOC. OpenJOC remains
Apache-2.0, while bundled mpv, FFmpeg, GStreamer, SADIE II, and runtime
components retain their component-specific notices and licenses. The exact
package inventories are in `BUILD_INFO`, `DEPENDENCIES`, and
`THIRD_PARTY_NOTICES`.

The experimental C ABI remains version 1.3; the package version does not change
the ABI major/minor.

## Qualification scope

macOS arm64, Linux x86_64, and Windows x64 player software paths are qualified
in native CI, including decoder selection, binaural, 2.0/5.1/7.1.4/9.1.6/22.2,
EOS, dependency closure, licenses, and private-path scans. Multichannel PCM
generation and transport are qualified in CI; physical speaker-system playback
has not been separately validated on Linux/Windows hardware. The macOS bundle
is ad-hoc signed, not Developer-ID signed, and not notarized. Linux runtime
compatibility is bounded by the Ubuntu 24.04/glibc baseline recorded in
`BUILD_INFO`.

## Upgrade notes

Users of 0.7.0 are encouraged to upgrade for the new player integrations,
native 22.2, built-in binaural resource, cross-platform packages, and
accumulated correctness and packaging hardening. Existing packet/input,
semantic-binding, and custom-SOFA limitations remain documented in
`docs/KNOWN_LIMITATIONS.md`.

This release body is a publication draft. Tagging and GitHub Release creation
remain explicit user-authorized actions and are not performed during release
hardening.
