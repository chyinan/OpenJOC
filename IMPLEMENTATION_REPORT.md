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
available on `PATH`. The supplied external DEE fixture labelled
`forever_friends` still has SHA-256
`67c10f65642f11713f8495026a37cf26fd1f901e9a343d2e3acf5ee879584896` and size
32,138,978 bytes. Running
`OPENJOC_DEE_FIXTURE=<that path> cargo test -p openjoc-cli --test container
user_supplied_dee_fixture_uses_container_boundary_when_enabled` passed; the
test independently demuxed and indexed all 7,773 access units. A release
`inspect` run reports `ISO BMFF (stream-copied E-AC-3)`, 7,773 frames, and
7,773 access units. The CLI's current diagnostic wording is “JOC extension
signaled ... EMDF profile absent”. That wording means the currently validated
frame-end auxiliary extractor did not locate a recognized OAMD/JOC EMDF pair;
it does not establish EMDF absence from every legal E-AC-3 carrier. No
separate metadata or recognized JOC box was found at the ISO BMFF track/box
level, while audio-block `skipfld` carriage remains unruled out because the
full internal traversal fails before that real-fixture lane is completely
validated. This refresh strengthens the container evidence only and does not
change the open real-vector status.

## Implemented increment: external multi-fixture carrier census

`openjoc census [MANIFEST] -o DIR` (or
`OPENJOC_REAL_FIXTURE_MANIFEST=<path>`) accepts a local JSON manifest of
user-supplied raw EC3/M4A/MP4 fixtures. The manifest is gitignored by default;
labels, optional SHA-256 values, and notes are checked before any demux. The
command emits deterministic `census.json` and `census.txt` reports without
copying programme bytes into the repository. Missing manifests/files, duplicate
labels, hash mismatches, unsupported inputs, probe failures, and bounded demux
failures are structured errors.

Each fixture report records source and stream-copy hashes and sizes, selected
track metadata, syncframe/access-unit/sample counts, substream topology,
`addbsi` and complexity distributions, frame-end `auxdatae`/EMDF attempts,
payload IDs 11/14, skip-field bytes reached, unresolved blocks, malformed
carrier cases, and the first bounded failure. Carrier states distinguish
“extension signaled but no EMDF in validated carriers” from “carrier
unresolved”; the latter is not an EMDF-absence claim. The text report starts
with a cross-fixture comparison table.

The current opt-in external corpus contains four non-committed DEE outputs:

| label | source SHA-256 | bytes | frames/access units | addbsi complexity | frame-end auxdatae | payload 11/14 | state |
| --- | --- | ---: | ---: | ---: | ---: | --- | --- |
| `forever_friends` | `67c10f65642f11713f8495026a37cf26fd1f901e9a343d2e3acf5ee879584896` | 32,138,978 | 7,773/7,773 | 7,773 × 16 | 0/7,773 | false/false | carrier unresolved |
| `hitchcock` | `0075ade8f801e38a4f98637d9d9a8099771ea1edd0bb66bd829aa2c0faa3e425` | 29,370,578 | 7,146/7,146 | 7,146 × 16 | 0/7,146 | false/false | carrier unresolved |
| `grand_escape` | `b7a320d2ff14a27e64b9e0262f2092b31145bc217100a2f987d174fef0ef2956` | 44,175,378 | 10,599/10,599 | 10,599 × 16 | 0/10,599 | false/false | carrier unresolved |
| `brainrot` | `2808eecb80353141135000ab499815219a86770e5b02e912dc971dd01e86afd7` | 16,283,910 | 3,910/3,910 | 3,910 × 16 | 0/3,910 | false/false | carrier unresolved |

All four are ISO BMFF files with one 48 kHz six-channel `eac3` stream and
1,536 samples per access unit. No payload IDs 11 or 14 were located in the
currently bounded frame-end carrier. The parse-only walker reaches each first
audio-block prefix but leaves later blocks unresolved; it does not scan
mantissa bytes for EMDF. The first complete internal-base failures are:

```text
forever_friends: bap 3, raw 7, channel 0, frame bit 2828
hitchcock:       bap 3, raw 7, channel 0, frame bit 2084
grand_escape:    bap 5, raw 15, channel 1, frame bit 1726
brainrot:        bap 3, raw 7, channel 1, frame bit 1774
```

The diagnostics identify reproducible syntax failures; they do not establish
nonzero JOC/OAMD reconstruction or internal-base fidelity.

## User-supplied DEE fixture evidence

The external fixture is intentionally not committed (stable label:
`forever_friends`).

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
  OAMD/JOC EMDF; this is not evidence about unvalidated legal carrier paths.
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

With the local ignored manifest selected, the opt-in corpus test also passed:

```text
OPENJOC_REAL_FIXTURE_MANIFEST=<local manifest> cargo test -p openjoc-cli --all-features fixture_census -- --nocapture
OPENJOC_REAL_FIXTURE_MANIFEST=<local manifest> cargo run -p openjoc-cli --release --offline -- --no-banner census <local manifest> -o <local report directory>
```

The full-workspace quality gates were rerun at code HEAD
`b9cab25a5df0e8ab3b3344dd2cbad71f7c120017` (before this documentation-only
commit) and passed:

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo build --workspace --release --offline
git diff --check
```

This includes the workspace-wide all-feature test suite, strict clippy, the
offline release build, and a clean diff check. The later commit from this
increment changes documentation only; no production source or test file is
changed by that commit.

## Revision and verification ledger

- Container implementation: `74faa5600387a34b073513cef62a987925d3d86d`.
- Wave output formats: `42e4af73da0fbd02574ab23e62a265f3a61089ea`.
- Trim retention: `2303f33a2b7e86656020992015a1f88ab5ec1e6f`.
- Frame-local scene staging: `241cb0334ec735dde7516613db0987b101b86f06`.
- Borrowed per-frame debug sink: `a2b96aa4475da3522794c82e189c5602046dcf3b`.
- Bounded E-AC-3 carrier inspection and first-failure diagnostics:
  `d38c00c81740db15062256d1c5651c40a295f279`.
- External multi-fixture census harness and CLI report integration:
  `7f43db05d6876314d8d5ec5415840605c3204d54`.
- Opt-in manifest-gated external census test:
  `b9cab25a5df0e8ab3b3344dd2cbad71f7c120017`.
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
