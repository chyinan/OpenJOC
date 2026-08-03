# OpenJOC implementation report

## Evidence boundary

The repository currently supports a clean-room raw E-AC-3 path with aligned
base-channel PCM, JOC/OAMD extraction, a renderer-independent `ObjectScene`,
default-f32 object stems, and optional explicit reference-f64 object stems.
This report does not claim a complete real-world Atmos decoder or a
speaker/binaural renderer. The compatible base reference remains explicit
FFmpeg `pcm_f64le` PCM and is not a final render.

## Completed increment: input media and DEE containers

Implemented in `openjoc-container` and `openjoc-cli`:

- signature classification distinguishes raw E-AC-3 (`0b77`) from ISO BMFF
  (`ftyp`/recognized top-level box) before `index_syncframes`;
- FFprobe selects exactly one audio stream and requires codec `eac3`;
- FFmpeg is invoked only for `-c:a copy -f eac3` stream-copy demux;
- stdout is bounded, empty/oversized output is rejected, and OpenJOC indexes
  and groups the demuxed elementary stream independently;
- `inspect` and `decode` use the same boundary; raw `.ec3` remains direct;
- container, missing-track, multiple-track, unsupported-codec, malformed, and
  failed-demux diagnostics are structured and do not collapse to syncword
  errors;
- the generated compatible base-channel reference is now named
  `debug/compatible_base.wav`; it remains explicit `pcm_f64le` reference PCM,
  not a final render.

## Tests added

- `crates/openjoc-container/tests/input_media.rs`: raw/ISO/unknown signatures
  and probe-row parsing.
- `crates/openjoc-cli/tests/container.rs`: FFmpeg + MP4Box fixture demux,
  byte-equivalent stream-copy comparison, raw classification, inspect/decode
  routing, malformed container, AAC-only track, and multiple-track errors.

The MP4Box command is test-fixture tooling only; it is not an OpenJOC runtime
dependency. Runtime container behavior uses FFprobe and FFmpeg as external
black boxes and never uses their decoded PCM as object reconstruction.

## Current container audit refresh

The installed toolchain was rechecked on the current worktree: Poppler
26.02.0 (`pdftoppm`, `pdftotext`, and `pdfinfo`) and FFmpeg/FFprobe are
available on `PATH`. The supplied DEE file still has SHA-256
`67c10f65642f11713f8495026a37cf26fd1f901e9a343d2e3acf5ee879584896` and size
32,138,978 bytes. Running
`OPENJOC_DEE_FIXTURE=<that path> cargo test -p openjoc-cli --test container
user_supplied_dee_fixture_uses_container_boundary_when_enabled` passed; the
test independently demuxed and indexed all 7,773 access units. A release
`inspect` run reports `ISO BMFF (stream-copied E-AC-3)`, 7,773 frames, and
7,773 access units. The CLI's current diagnostic wording is “JOC extension
signaled ... EMDF profile absent”. That wording means the currently validated
frame-end auxiliary extractor did not locate a recognized OAMD/JOC EMDF pair;
it does not prove that the complete E-AC-3 stream contains no EMDF. No
separate metadata or recognized JOC box was found at the ISO BMFF track/box
level, while audio-block `skipfld` carriage remains unruled out because the
full internal traversal fails before that real-fixture lane is completely
validated. This refresh strengthens the container evidence only and does not
change the open real-vector status.

## User-supplied DEE fixture evidence

The legal fixture is intentionally not committed:

`D:\DRV SA PROJECT\Dolby ATMOS\Forever Friends ~Dolby ATMOS Test2~ .m4a`

- SHA-256: `67c10f65642f11713f8495026a37cf26fd1f901e9a343d2e3acf5ee879584896`
- size: 32,138,978 bytes
- FFprobe: one MJPEG video stream and one `eac3` audio stream (index 1), 48
  kHz, six channels, `5.1(side)`, 248.736 seconds
- independent stream-copy EC-3: 31,838,208 bytes, SHA-256
  `2e155599e319d7a6f1ef655684bd872aaae1cd5f73d82097c589a32c572df86a`
- OpenJOC `inspect`: 7,773 frames/access units, 1,536 samples each, container
  accepted; every frame carries `addbsi=[0x01,0x10]`, but every currently
  inspected normative E.1.2.5 frame-end `auxdatae` bit is zero. The current
  diagnostic wording is “JOC extension signaled; EMDF profile absent”; this
  means that the validated frame-end auxiliary profile extractor did not locate
  OAMD/JOC EMDF, not that the complete E-AC-3 stream contains no EMDF.
- ISO BMFF inspection found one `ec-3` audio track, no dependent substream,
  and no separate metadata/JOC box or recognized metadata box. Under TS
  103 420 §8.2 the complexity index in `addbsi` is not a substitute for the
  required EMDF payload. Audio-block `skipfld` carriage has not been ruled out
  on this fixture because the full internal audio-block traversal fails before
  that lane is completely validated; no claim is made that `skipfld` carriage
  is present.
- default FFmpeg base path: six-channel 48 kHz f64 PCM, 11,939,328 samples per
  channel (temporary WAV SHA-256
  `a065dc5d303b44e97943d3d8fa95e784559f157b2c220208112fd31b4a5997e2`)
- `--internal-base`: currently fails with `invalid E-AC-3 mantissa code 7 for
  bap 3`; the FFmpeg-versus-internal-base comparison is failed/not available,
  and internal-base fidelity is not verified

FFmpeg `astats` on that reference WAV reports the expected `5.1(side)` order
(FL, FR, FC, LFE, SL, SR), 11,939,328 samples/channel, and these peak/RMS
levels in dBFS:

```text
FL  peak -14.066079  RMS -29.027150
FR  peak -11.644446  RMS -27.419704
FC  peak  -3.850901  RMS -21.360071
LFE peak -33.119901  RMS -50.094647
SL  peak  -3.784534  RMS -20.646557
SR  peak  -1.605351  RMS -20.007338
```

The internal decoder emitted no PCM because it failed in the first access
unit, so delay, per-channel numerical error, and an internal peak/RMS vector
are explicitly `not available`; this is the required comparison failure
record, not a pass.

This is a useful container/diagnostic failure report, not a completion claim.
The nonzero JOC/OAMD acceptance lane remains open until a legal DEE fixture
with actual EMDF OAMD/JOC payloads is supplied or independently located from
authorized sources; the carrier extraction and internal base path must then be
validated against known ground truth.

## Implemented increment: wave output semantics

`openjoc-wave` now exposes an explicit `SampleFormat` abstraction for f32, f64,
s24, and s16. CLI object stems default to f32; `--reference-f64` selects the
lossless reference representation. Integer output requires an explicit
clipping policy (`Reject` or `Hard`) and an explicit dither policy (`None` or
seeded triangular one-LSB dither); no integer clipping or dither is implicit.
The compatible base-channel debug artifact remains an explicit FFmpeg
`pcm_f64le` reference and is named `debug/compatible_base.wav`.

Wave tests cover all four output formats, integer range handling, dither
reproducibility, and f64 compatibility. This increment does not change the
renderer-independent scene boundary or claim nonzero JOC/OAMD reconstruction.

## Implemented increment: renderer-independent trim retention

`ObjectScene` now retains each decoded OAMD trim snapshot in a timed
`trim_timeline`, including warp mode, global trim mode/configurations, centre,
surround, height, top/bottom and listener-Y controls, and per-object trim
disable flags. CLI scene artifacts write this data to
`metadata/trim_timeline.json`; no speaker or binaural rendering behavior is
implied. Scene validation checks trim timing, object cardinality, and finite
custom controls. Assembly and JSON roundtrip tests cover the retained state.

## Implemented increment: frame-local atomic staging

`SceneBuilder::append_frame` now stages the current frame's metadata and trim
updates before mutating the accumulated scene. It validates PCM finiteness,
timing bounds, trim cardinality, and numeric controls first, then appends PCM
and metadata without cloning prior object audio. `PayloadDecoder` likewise
uses the atomic builder in place; only its bounded JOC codec state is copied
for retry semantics. A later CLI file-sink increment is still required to
avoid retaining the complete input, base PCM, scene PCM, and debug frames.

## Implemented increment: borrowed frame sink

`PayloadDecoder::decode_frame_with` now lends each committed
`DecodedPayloadFrame` to a callback instead of requiring callers to retain an
owned frame result. The E-AC-3 aligned and `--internal-base` command paths use
the callback to write `debug/frame_NNN` artifacts immediately, so the CLI no
longer builds a complete `Vec<DecodedPayloadFrame>` before writing debug
output. The callback is invoked only after codec, OAMD, JOC state, and scene
staging commit; sink failures are reported explicitly and do not claim to
roll back committed decoder state.

This is a bounded-retention increment, not the complete streaming design. The
current CLI still loads the elementary stream and compatible base WAV into
memory, and `SceneBuilder` still retains the complete reconstructed scene PCM
for the renderer-independent `ObjectScene`. Metadata-only scene assembly and
streaming WAV sinks remain open and are tracked separately in the requirements
matrix.

## Verification commands

The current container and diagnostic checks were run with:

```text
cargo test -p openjoc-container
cargo test -p openjoc-cli --test container -- --nocapture
cargo test -p openjoc-cli
```

Recorded full-workspace quality gates for the borrowed frame-sink implementation
working tree represented by commit
`a2b96aa4475da3522794c82e189c5602046dcf3b` passed:

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo build --workspace --release --offline
git diff --check
```

This includes the workspace-wide all-feature test suite, strict clippy, the
offline release build, and a clean diff check. The gates were recorded before
the later container-audit and banner-only commits; they were not rerun for the
current HEAD or this documentation-only change.

## Revision and verification ledger

- Container implementation: `74faa5600387a34b073513cef62a987925d3d86d`.
- Wave output formats: `42e4af73da0fbd02574ab23e62a265f3a61089ea`.
- Trim retention: `2303f33a2b7e86656020992015a1f88ab5ec1e6f`.
- Frame-local scene staging: `241cb0334ec735dde7516613db0987b101b86f06`.
- Borrowed per-frame debug sink: `a2b96aa4475da3522794c82e189c5602046dcf3b`.
- Container evidence/status audit: documentation-only commit
  `cf9dcd4bbb31e13dd6f47c807aba15f6e0460c30`.
- Later status/documentation-only commits: container evidence/status audit
  `cf9dcd4bbb31e13dd6f47c807aba15f6e0460c30`; current HEAD before this
  documentation-only increment is banner-only commit
  `957bbd685506d664073a4f66433a0b0e7b2d8769`.

## Known limitations and next goals

All-carrier EMDF discovery, the real-vector acceptance lane,
FFmpeg-versus-internal-base fidelity report, metadata-only scene assembly, and
streaming PCM/file sinks remain open. The borrowed frame sink only removes the
all-frame debug vector; it is not the complete constant-memory streaming
design. Speaker and binaural renderers are later non-normative components and
are deliberately outside the current decoder increments.
