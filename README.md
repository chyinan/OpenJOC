<p align="center">
  <img src="docs/assets/openjoc-header.png" alt="OpenJOC — open-source Dolby JOC decoder" width="100%">
</p>

# OpenJOC

OpenJOC is an independent, clean-room, research-grade E-AC-3 JOC metadata
and reconstruction-basis decoder. It implements behavior from public ETSI
specifications and controlled, permitted evidence; it does not copy Dolby
private implementations.

OpenJOC is an independent clean-room implementation, but it is not a
public-document-only implementation. Most behavior is derived directly from
public normative specifications and public technical sources. Where those
sources are insufficient to establish interoperability behavior, the project
may use a separated traditional clean-room process: authorized contaminated
analysis is sanitized into a behavioral specification and then implemented
independently. Proprietary implementation code, decompiler output, assembly,
private symbols or addresses, proprietary structure layouts, and copied
implementation expressions are not implementation inputs; see
[the provenance policy](docs/PROVENANCE.md).

OpenJOC is not affiliated with, endorsed by, or sponsored by Dolby
Laboratories. Dolby, Dolby Atmos, and related marks are trademarks of their
respective owners.

The immutable v0.2.0 release contract is deliberately narrow:

- `scene.json` is metadata-only;
- `diagnostics/reconstruction_rows/row_NNN.wav` contains diagnostic
  `ReconstructionBasis` rows, not verified authored-object PCM;
- `SemanticBindingState` remains `Unresolved`;
- `ETSI_STRICT` was the historical no-profile default and is never silently
  downgraded;
- `OBSERVED_VENDOR_COMPAT` is explicit, partial, and preserves opaque observed
  continuation without assigning vendor semantics.

OpenJOC 0.8.0 is the current release line: **Cross-Platform Player
Integration**. It extends the embeddable decode/render engine with native
22.2 speaker rendering, built-in zero-configuration binaural HRTF, GStreamer
integration, FFmpeg-facing integrations, and reproducible OpenJOC-enabled mpv
bundles for qualified macOS arm64, Linux x86_64, and Windows x64 surfaces.
Ordinary rendering assembles bridge control from decoded JOC/OAMD state;
`--topology` remains an optional complete override/test input.

One renderer. Same spatial semantics across platforms. OpenJOC implements the
spatial rendering DSP directly; frameworks and operating systems provide
packet/timestamp transport, PCM transport, and normal device I/O, but do not
define the OpenJOC spatial rendering result. OpenJOC is not a zero-dependency
distribution: integrations use their host framework and runtime dependencies.

The selectable `5.1`, `5.1.2`, `5.1.4`, `7.1`, `7.1.2`, `7.1.4`, `7.1.6`,
`9.1`, `9.1.2`, `9.1.4`, `9.1.6`, and native `22.2` workflows are documented in
[JOC speaker rendering](docs/JOC_RENDER.md). The same workflow can virtualize
all public virtual binaural layouts through the built-in SADIE II generic HRIR
or a user-supplied strict SOFA HRIR bank when directions are exact or safely
interpolatable. The default virtual layout is `7.1.4`.
The underlying public `SpatialLayout` plus `JocSpatialBridge` API remains a
generic N-channel library interface for caller-defined layouts; the CLI names
are convenience presets, not the renderer's fundamental maximum.

OpenJOC 0.3.0 was the local release candidate. It added an explicit
spatial-rendering foundation for caller-supplied mono sources: validated 2D and
3D speaker layouts, sample-accurate trajectories, direct-FIR and uniform
partitioned binaural rendering, and a strict supported SOFA import path. These
renderer workflows are independent of unresolved JOC authored-object binding.

User-facing `decode` and `decode-payload` commands default to observable `AUTO`
profile selection. `AUTO` tries `ETSI_STRICT` first and can select only the
existing whitelisted `OBSERVED_VENDOR_COMPAT` policy when every blocking
deviation is admitted. Explicit `ETSI_STRICT` never falls back.

The opt-in `JocSpatialBridge` provides the codec-coordinate spatial projection
function. Its current maturity is experimental, its semantic binding remains
unresolved, and its official runtime validation oracle is not independently
confirmed. These states are documented separately from the stable function
name.

Read the canonical documentation:

- [Capabilities](docs/CAPABILITIES.md) — current 0.8.0 capability status.
- [JOC speaker rendering](docs/JOC_RENDER.md) — the 0.8.0 real-input workflow.
- [Known limitations](docs/KNOWN_LIMITATIONS.md) — what remains out of scope.
- [Architecture](docs/ARCHITECTURE.md) — production data flow and boundaries.
- [Spatial portability](docs/SPATIAL_PORTABILITY.md) — 22.2 geometry, built-in HRTF, and platform-independence policy.
- [Third-party data notices](THIRD_PARTY_NOTICES.md) — SADIE II license, attribution, and citation.
- [Library API](docs/LIBRARY_API.md) — headless Rust packet/session contract.
- [C API](docs/C_API.md) — versioned C/C++ ABI and ownership rules.
- [GStreamer integration](docs/integration/GSTREAMER.md) — experimental
  automatic selection for admitted E-AC-3 JOC while ordinary E-AC-3 stays on
  the normal decoder path; applications can select binaural or native
  multichannel speaker rendering while OpenJOC retains the spatial DSP.
- [FFmpeg-facing external bridge](docs/integration/FFMPEG.md) — experimental
  libavformat/AVFrame integration for applications embedding FFmpeg. It does
  not modify stock FFmpeg binaries or register an out-of-tree decoder plugin.
- [Native FFmpeg libavcodec wrapper](docs/integration/FFMPEG_NATIVE.md) — an
  experimental `libopenjoc` named decoder for custom FFmpeg source builds;
  stock FFmpeg binaries remain unchanged and ordinary E-AC-3 stays on `eac3`.
- [mpv player integration](docs/integration/MPV.md) — OpenJOC can be used as
  the JOC decoder in custom mpv builds linked against the `libopenjoc`-enabled
  FFmpeg integration; ordinary E-AC-3 remains on `eac3`.
- [Player packaging](docs/integration/PLAYER_PACKAGING.md) — reproducible,
  auditable OpenJOC Player Bundle tooling for macOS arm64, Linux x86_64, and
  Windows x64; this is not an official mpv or FFmpeg distribution.
- [Future player adapters](docs/FUTURE_PLAYER_ADAPTERS.md) — next integration assessment.
- [Roadmap](docs/ROADMAP.md) — future priorities only.

## Build from source

OpenJOC requires Rust 1.85 or newer. The application dependency graph is
recorded in `Cargo.lock`.

```sh
cargo build -p openjoc-cli --release --locked
./target/release/openjoc --help
```

An offline build is supported when all locked registry dependencies are
already present in the selected Cargo cache:

```sh
cargo build -p openjoc-cli --release --locked --offline
```

This is not a claim that a brand-new machine can build without first obtaining
the Rust toolchain and dependencies. The repository declares a minimum Rust
version but does not pin one exact compiler release.

## Embed the decode/render engine

OpenJOC 0.8.0 provides `OpenJocSession` and `OpenJocConfig` for headless Rust
integration. A push supplies one complete E-AC-3 JOC access unit; receive returns
owned interleaved `f32` PCM with sample-domain timestamps and semantic channel
labels. Sessions support push/receive, drain, flush, reset/discontinuity, and
report the public output delay (609 samples for speaker output, 577 for
binaural). The API does not parse CLI arguments, open files, or use global
decoder state.

The experimental C ABI is distributed with the platform archives as
`include/openjoc.h` plus static/shared libraries. It uses opaque handles,
numeric statuses, `struct_size` forward compatibility, instance-owned errors,
and panic containment. ABI 1.3 adds a decode-free classifier and retains the
framework-neutral bounded compressed-stream handle used by native media
adapters while preserving the complete-AU decoder API. The C surface is
ABI 1.3-experimental; compatibility may
evolve during OpenJOC 0.x; framework adapters are documented separately.

OpenJOC also provides an experimental FFmpeg-facing libavformat/AVFrame bridge
for applications embedding FFmpeg. It uses FFmpeg for demux and packet
transport, while the same `OpenJocSession` owns JOC decode and spatial
rendering. Build it explicitly with `-p openjoc-ffmpeg --features ffmpeg`; see
the focused integration document for the FFmpeg 9 development dependencies
and proof executable.

OpenJOC additionally provides an experimental native libavcodec wrapper for
custom FFmpeg builds configured with `--enable-version3 --enable-libopenjoc`.
It registers the explicit named decoder `libopenjoc`; it does not alter stock
FFmpeg installations or replace the ordinary `eac3` decoder. The focused
native integration document contains the reproducible patch and build flow.

## Install into a prefix

From a clean source checkout or source archive:

```sh
cargo install --path crates/openjoc-cli --locked --root /path/to/prefix
/path/to/prefix/bin/openjoc --help
```

Binary distribution is handled by the human-created GitHub Release workflow
after a stable version tag. The historical [OpenJOC 0.2.0 release](https://github.com/chyinan/OpenJOC/releases/tag/v0.2.0)
contains the prior published assets; this source tree does not advertise a
Homebrew formula or crates.io installation. The source installation path
remains the workspace source tree.

## Basic CLI

```sh
openjoc inspect input.ec3
openjoc decode input.ec3 -o output/ --internal-base
openjoc decode input.mp4 -o output/ --internal-base --streaming
openjoc decode input.ec3 -o output/ --internal-base --validation-profile etsi-strict
# Calibrated speaker render (Default dialnorm is implicit)
openjoc render-joc input.m4a \
  --layout 7.1.4 -o output.wav
# Convenient offline file level (static post-render sample peak)
openjoc render-joc input.m4a \
  --layout 7.1.4 --normalize-peak -0.1 -o output-loud.wav
# Binaural with the same optional offline file-level step
openjoc render-joc input.m4a \
  --binaural --sofa listener.sofa --normalize-peak -0.1 -o headphones.wav
# Semantic CAF output for the 9.1.6 speaker layout
openjoc render-joc input.m4a \
  --layout 9.1.6 -o render-9.1.6.caf
openjoc render-joc input.m4a --binaural --sofa listener.sofa \
  --virtual-layout 9.1.6 -o binaural-9.1.6.wav
# Optional complete explicit override/test input:
openjoc render-joc input.m4a --topology bridge-control.json --layout 7.1.4 --output render.wav
openjoc diagnose-tools input.ec3 --vector-id ID --json tools.json
```

For normal decoding and playback, OpenJOC's default calibrated dialnorm policy
is recommended; basic examples intentionally omit `--dialnorm default` because
it is already the engine default. For an offline file that should be
conveniently loud, add `--normalize-peak -0.1` (or another chosen dBFS target).
This does not change decoder dynamics.

`2.0` is speaker stereo and is separate from binaural output. Select the
standards-based stereo policy with `--downmix auto`, `--downmix loro`, or
`--downmix ltrt`.

## Advanced decoder/output policies

`--drc disabled|line|rf|custom` controls encoded E-AC-3 dynamic-range metadata;
custom mode accepts `--drc-boost` and `--drc-cut` percentages from `0` through
`100`. DRC changes program dynamics; it is not a generic compressor or volume
normalization.

`--dialnorm default` uses calibrated default behavior and is recommended for
normal playback/decoding. `--dialnorm digital` explicitly selects encoded
digital program-level calibration. `--dialnorm analog` uses unity dialnorm
gain; it is an advanced compatibility/diagnostic policy. On hot material,
unity dialnorm can present a substantially hotter level to downstream
headroom processing, so it may make FinalLinkedGain engage more heavily. Do
not choose Analog merely because it is louder; it is not a higher-quality,
uncompressed, lossless, raw, or mastering mode.

`--normalize-peak TARGET_DBFS` is an optional file-export transform. It performs
one canonical render while spooling bounded, renderer-native PCM and measuring
the sample peak, then sequentially applies one common scalar after
FinalLinkedGain (and after binaural convolution when selected) before WAV/CAF
conversion. It is disabled by default, supports both boost and attenuation, and
is sample-peak based and post-render. It is not dialnorm, DRC, a limiter, a
compressor, LUFS normalization, or true-peak normalization; an inter-sample
true peak may exceed the sample-peak target.

Recommended offline signal flow:

```text
DRC -> calibrated dialnorm -> JOC rendering -> FinalLinkedGain
    -> optional static sample-peak normalization -> file
```

Analog instead uses unity dialnorm before JOC rendering and FinalLinkedGain.
Post-render normalization is applied after FinalLinkedGain, so it preserves the
already-rendered dynamic shape with one static linked scalar. Analog changes the
level entering FinalLinkedGain and may therefore change its gain-reduction
behavior. FinalLinkedGain is internal renderer headroom behavior, not a user
mastering control.
For 2.0 JOC rendering, Base channels use the selected stereo downmix while
reconstructed JOC objects use generic spatial projection to physical `FL`/`FR`.

`render-joc` selects the output container from the destination extension:
`.wav` uses WAVEFORMATEXTENSIBLE where the semantic layout is exactly
representable, and `.caf` uses Core Audio Format channel-layout metadata. The
9.1 family is semantic-CAF-only: its Wide identities are not exactly
representable by standard WAVEFORMATEXTENSIBLE and WAV requests fail closed.
The renderer’s semantic channel order is independent of that container choice.

`decode` and `decode-payload` use `AUTO` when no profile is supplied: they
evaluate `ETSI_STRICT` first and select `OBSERVED_VENDOR_COMPAT` only when the
existing compatibility validator admits the complete deviation set. An
explicit `--validation-profile etsi-strict` never falls back. The selected
profile and reason are written to the bounded validation diagnostics.

Interactive `render-joc` progress is written to stderr and is automatically
disabled for non-TTY output; use `--no-progress` to opt out. Add
`--performance-report FILE.json` to capture versioned stage timings and
realtime diagnostics. Use `--overwrite` for authorized replacement of existing
render outputs in scripts or other non-interactive runs. See [Experimental JOC speaker rendering](docs/JOC_RENDER.md)
for the report schema, synthetic harness, and real-media qualification
boundary.

Raw EC3 parsing and internal-base decoding run in-process. Some seekable
MP4/M4A and compatible-base paths use `ffprobe` and/or `ffmpeg`; see the
[capability matrix](docs/CAPABILITIES.md) for the exact boundary.

## Assemble the 0.8.0 Apple-Silicon release bundle

On an Apple-silicon macOS host with Python 3.12+, Rust, and the locked Cargo
dependencies already cached, a clean committed tree can assemble the release
bundle locally before publication:

```sh
python3 scripts/build-local-release.py --output /path/to/empty/output
cd /path/to/empty/output
shasum -a 256 -c openjoc-0.8.0-aarch64-apple-darwin.SHA256SUMS
tar -xzf openjoc-0.8.0-aarch64-apple-darwin.tar.gz
cd openjoc-0.8.0-aarch64-apple-darwin
./verify.sh
```

The bundle includes the canonical `docs/` tree, uses `git archive HEAD`, builds
with the locked dependency set, includes `openjoc.h` and the macOS C ABI
static/shared libraries, and refuses tracked worktree/index changes. It is
ad-hoc signed, not Developer-ID signed, and not notarized. The script derives
the artifact version from the workspace package metadata.

## Quickstart: OpenJOC-enabled mpv

Download the qualified `openjoc-mpv-0.8.0-<platform>` bundle, extract it, and
run the included launcher:

```sh
bin/openjoc-mpv path/to/media
bin/openjoc-mpv --profile=openjoc-headphones path/to/joc-media
```

Ordinary E-AC-3 stays on the normal `eac3` decoder. Confirmed JOC is routed
automatically to `libopenjoc`. `openjoc-headphones` is binaural output from a
virtual 7.1.4 field through the built-in generic SADIE II HRTF; the
`openjoc-stereo`, `openjoc-51`, `openjoc-714`, `openjoc-916`, and `openjoc-222`
profiles are physical 2.0, 5.1, 7.1.4, 9.1.6, and 22.2 rendering. Binaural
and physical 2.0 both produce two-channel PCM, but they are different renders.
Explicit `--audio-spdif=eac3` requests compressed passthrough and bypasses
OpenJOC software rendering.

## CI and tagged releases

Pull requests and pushes to `master` run the public GitHub Actions CI matrix.
It checks the documented Rust 1.85 MSRV, Linux quality gates, and
platform-neutral builds/tests on Windows x64 and macOS arm64. CI results are
build/test evidence; a CI result alone does not admit a published binary
release for a platform.

Only a human-created stable tag can start release automation (the historical
`v0.1.0` tag is preserved). The workflow requires the tag to match the Cargo
package version exactly, then
builds and verifies macOS arm64, Windows x86_64, and GNU/Linux x86_64 release
archives. Each archive carries the CLI, public docs, `openjoc.h`, and the
platform C ABI static/shared libraries (including the Windows import library).
Per-platform manifests remain internal workflow artifacts. The aggregation job
recomputes archive hashes and publishes the three CLI/library archives, the
three OpenJOC Player Bundles, and one unified `SHA256SUMS` file. The workflow
never creates or pushes tags, and
refuses to overwrite an existing GitHub Release. Artifact attestation is not
currently enabled; aggregate SHA-256, per-platform manifest checks, and the
macOS bundle's `verify.sh` remain release verification surfaces.

## Platform scope

The 0.8.0 release workflow targets Apple-silicon macOS
(`aarch64-apple-darwin`), Windows x86_64 (`x86_64-pc-windows-msvc`), and
GNU/Linux x86_64 (`x86_64-unknown-linux-gnu`). The OpenJOC Player Bundle
workflow additionally qualifies macOS arm64, Linux x86_64, and Windows x64
extract-and-run packages. Multichannel PCM generation and transport are
qualified in CI; physical speaker-system playback has not been separately
validated on Linux/Windows hardware. The macOS bundle is ad-hoc signed and is
not Developer-ID signed or notarized.

## Contributing and provenance

Before changing codec behavior, read [CONTRIBUTING.md](CONTRIBUTING.md) and the
release-facing capability and limitation documents. The project treats public
normative sources and behavioral clean-room specifications as separate claim
classes; neither is a claim of Dolby endorsement, certification, or
bit-identical Reference Player output.

## License

Licensed under [Apache-2.0](LICENSE). The source archive includes the complete
license text, and Cargo package metadata carries the SPDX license identifier.
