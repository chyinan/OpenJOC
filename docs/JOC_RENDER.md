# JOC speaker rendering

OpenJOC 0.6.0 exposes one experimental JOC-to-speaker workflow with an
explicit selectable speaker preset:

```sh
openjoc render-joc INPUT.ec3 \
  --layout 7.1.4 \
  --output openjoc-render.wav
```

Seekable ordinary MP4/M4A input is accepted at the same boundary as the other
E-AC-3 commands:

```sh
openjoc render-joc INPUT.m4a \
  --layout 7.1.4 \
  --output openjoc-render.wav
```

The admitted height and wide-channel presets use the same command surface:

```sh
openjoc render-joc INPUT.m4a --layout 5.1.4 --output output-5.1.4.wav
openjoc render-joc INPUT.m4a --layout 7.1.2 --output output-7.1.2.wav
openjoc render-joc INPUT.m4a --layout 7.1.6 --output output-7.1.6.caf
openjoc render-joc INPUT.m4a --layout 9.1 --output output-9.1.caf
openjoc render-joc INPUT.m4a --layout 9.1.2 --output output-9.1.2.caf
openjoc render-joc INPUT.m4a --layout 9.1.4 --output output-9.1.4.caf
openjoc render-joc INPUT.m4a --layout 9.1.6 --output output-9.1.6.caf
```

Supported presets are `2.0`, `5.1`, `5.1.2`, `5.1.4`, `7.1`, `7.1.2`, `7.1.4`,
`7.1.6`, `9.1`, `9.1.2`, `9.1.4`, and `9.1.6`. The `--layout`
argument is required; there is no implicit output-layout default. `5.1` is the
regression anchor for the original 0.4.0 integration. The renderer remains
experimental and does not claim Dolby or Reference Player equivalence.

The semantic speaker output contract is explicit and stable. The following
channel order is used for PCM planes and CAF descriptions; the LFE index is
zero-based and LFE is not a geometric projection anchor.

| Preset | Channel sequence | Count | LFE index | WAVEFORMATEXTENSIBLE mask |
| --- | --- | ---: | ---: | ---: |
| `2.0` | `FL, FR` | 2 | none | `0x00000003` |
| `5.1` | `FL, FR, FC, LFE, Ls, Rs` | 6 | 3 | `0x0000060f` |
| `5.1.2` | `FL, FR, FC, LFE, Ls, Rs, TFL, TFR` | 8 | 3 | `0x0000560f` |
| `5.1.4` | `FL, FR, FC, LFE, Ls, Rs, TFL, TFR, TBL, TBR` | 10 | 3 | `0x0002d60f` |
| `7.1` | `FL, FR, FC, LFE, Lb, Rb, Ls, Rs` | 8 | 3 | `0x0000063f` |
| `7.1.2` | `FL, FR, FC, LFE, Lb, Rb, Ls, Rs, TFL, TFR` | 10 | 3 | `0x0000563f` |
| `7.1.4` | `FL, FR, FC, LFE, Lb, Rb, Ls, Rs, TFL, TFR, TBL, TBR` | 12 | 3 | `0x0002d63f` |
| `7.1.6` | `FL, FR, FC, LFE, Lb, Rb, Ls, Rs, Ltf, Rtf, Ltm, Rtm, Ltr, Rtr` | 14 | 3 | `none; CAF only` |
| `9.1` | `FL, FR, FC, LFE, Lb, Rb, Ls, Rs, Lw, Rw` | 10 | 3 | `none; CAF only` |
| `9.1.2` | `FL, FR, FC, LFE, Lb, Rb, Ls, Rs, Lw, Rw, Ltm, Rtm` | 12 | 3 | `none; CAF only` |
| `9.1.4` | `FL, FR, FC, LFE, Lb, Rb, Ls, Rs, Lw, Rw, Ltf, Rtf, Ltr, Rtr` | 14 | 3 | `none; CAF only` |
| `9.1.6` | `FL, FR, FC, LFE, Lb, Rb, Ls, Rs, Lw, Rw, Ltf, Rtf, Ltm, Rtm, Ltr, Rtr` | 16 | 3 | `none; CAF only` |

The seven layouts with an exact standard mask use WAVEFORMATEXTENSIBLE with the
standard speaker mask and IEEE float32 samples by default (`--reference-f64`
selects float64).
`7.1.6` is not exactly representable by standard WAVEFORMATEXTENSIBLE because
`Ltm`/`Rtm` have no equivalent predefined mask bits. The 9.1 family is likewise
not exactly representable because `Lw`/`Rw` and, where present, `Ltm`/`Rtm`
have no exact standard mask bits. A `.wav` request fails closed with no
substituted identities or fake mask. Use `.caf` for semantic multichannel
output. The public library exposes the canonical order and optional mask
through `openjoc_scene::SpeakerLayoutPreset`.

Output container selection is independent of the renderer’s semantic channel
layout. A `.wav` destination uses WAVEFORMATEXTENSIBLE and fails closed when a
semantic identity has no exact standard speaker-mask bit. A `.caf` destination
uses Core Audio Format with an ordered `chan` chunk of semantic channel
descriptions, preserving richer identities such as public left/right wide
labels and coordinate-described top-middle channels. The currently public
presets include `7.1.6` and the 9.1 family; `Lw`/`Rw` use semantic CAF labels
and `Ltm`/`Rtm` use distinct coordinate descriptions. Those layouts can also
be selected as binaural virtual fields when the supplied SOFA dataset provides
exact or safely interpolatable coverage.
Binaural output may also use `.caf`; its PCM remains stereo and its SOFA/DSP
path is unchanged.

The command performs container extraction, E-AC-3 Base/LFE decoding, JOC and
OAMD validation/decoding, persistent `JocSpatialBridge` accumulation, and
incremental container writing. It does not materialize a duration-sized
`ObjectScene` or reconstruction-basis capture.

## Reconstruction timeline alignment

The QMF identity path declares one canonical round-trip latency:
`openjoc_qmf::QMF_ROUNDTRIP_LATENCY_SAMPLES = 577` samples. The JOC decoder
keeps its causal ReconstructionBasis output in a bounded
`ReconstructionOutputTimeline` and hands the bridge only common logical
Base/ReconstructionBasis intervals. Logical sample ranges and frame indices
are preserved; Base and ReconstructionBasis are not independently retimed at
the bridge. The final decoder QMF state is flushed at end of stream, so the
last pending Base intervals are emitted with their ReconstructionBasis tail.
The same path validates sample rate, contiguous sample ranges, frame order,
coordinate counts, topology/reset epochs, finite PCM, and LFE length before
bridge projection. Startup pre-roll and EOF tail validity are carried in the
typed timeline metadata. Discontinuities reset the bounded timeline and
bridge-control state together.

For a Windows PowerShell render comparison, use the same input, layout, and
control/profile options for all three commands:

```powershell
openjoc.exe render-joc INPUT.m4a --layout 7.1.4 --validation-profile auto --diagnostic-contribution full --output Full-7.1.4.wav
openjoc.exe render-joc INPUT.m4a --layout 7.1.4 --validation-profile auto --diagnostic-contribution base-only --output Base-7.1.4.wav
openjoc.exe render-joc INPUT.m4a --layout 7.1.4 --validation-profile auto --diagnostic-contribution reconstruction-only --output Reconstruction-7.1.4.wav
```

These are contribution-isolation diagnostics, not separate decoder modes.
They share the same aligned timeline, bridge-control scheduling, LFE policy,
WAV sample count, and channel order.

The preset is data, not a separate JOC algorithm. It supplies public channel
order and clean normalized layer/row/anchor geometry to the generic full-XYZ
`SpatialLayout` projection. X runs left-to-right, Y runs front-to-rear, and
signed Z places points at/below the bed or toward the upper layer. X is solved
within each row, Y blends adjacent rows, and admitted bed/top layers compose
through signed-Z equal-power weights. These are explicit OpenJOC preset
coordinates, not authored object positions and not a vendor renderer geometry
claim.

The same generic engine is internally validated with clean executable fixtures
for 2.0, 3.1, 5.1, 5.1.2, 5.1.4, 7.1, 7.1.2, 7.1.4, and 7.1.6. The twelve
public presets share one data-driven full-XYZ projector; their layout
differences are topology data. LFE remains independently owned and is excluded
from geometric projection. Additional topology families do not become public
product presets without their separate output contracts. Storage capability
does not imply arbitrary-layout or 22.2 semantics.

The public library layer is broader than this CLI preset list. Callers can
construct a validated `openjoc_scene::SpatialLayout` with arbitrary enabled
channels, LFE designation, knot axes, node vectors, and route vectors, then
pass it to the public `JocSpatialBridge::render_coordinates` API. The CLI
does not introduce a custom-layout file format; its stable user-facing names
are convenience presets over that generic layout engine.

Dynamic Region/Zone metadata is honored for ordinary point sources in the
admitted subset. Region selection derives a constrained speaker topology
before the existing point projector, so the default/no-region state is
unchanged and points outside selected support clamp to selected topology
endpoints instead of being muted. Ordinary Dynamic Extent metadata is also
honored for the eleven admitted 5.1/7.1/9.1-family layouts: XYZ metadata is
reduced to one isotropic scalar, zero extent preserves the point target, and
extent target changes use the existing Q32 scheduler. Only the six named
horizontal states and independent Top-Bottom include/exclude behavior on
validated one- or two-plane layouts are admitted. On admitted layouts,
non-default Region and ordinary nonzero Extent compose by selecting the
effective Region topology before point and Extent target generation; the
existing crossover, normalization, and Q32 scheduling remain in charge.
Dynamic point ChannelLock runs after ordinary point projection and takes
precedence over ordinary Extent target generation. Region selects the effective
topology before point and ChannelLock evaluation; ChannelLock selects the
current maximum active non-LFE output, resolves that output through the current
topology anchor map, and locks only when the full XYZ squared distance is
strictly below `0.04`. A passing evaluation emits an exclusive one-hot target
and a local effective-position snap. Extent state remains retained while its
target branch is bypassed, and all target changes use the existing Q32
scheduler. Non-point and unadmitted cases remain fail-closed.

## Automatic bridge control and optional override

The ordinary path assembles bridge control from decoded Base/RB codec
coordinates, parsed OAMD metadata, metadata timing, and the clean
codec-coordinate ordering contract. It does not infer authored-object
identity, use PCM statistics, or repair ordinals with a row/object guess.

`CONTROL.json` is optional. Supply `--topology bridge-control.json` only when a
complete explicit override, synthetic fixture, or expert debug control is
desired. The explicit source is used instead of automatic state; the two
sources are never implicitly merged.

The sidecar schema is `openjoc.joc-render-control.v1`. Its topology records
must be in the bridge's explicit codec-coordinate order: decoded Base full-band
channels first, followed by the decoded ReconstructionBasis rows. The record
count must match on every access unit. `updates` is optional and contains
frame-indexed `SpatialCoordinateUpdate` arrays; omitted fields inherit the
persistent bridge state.

The sidecar schema retains fields for source classes outside the ordinary
dynamic contract. Fixed records may use validated neutral
`fixed/<family>/<member>` identities with exact supplied route rows; Named
records may use neutral `named/<0..15>` identities across the eleven admitted
public layouts. Supplied Named direct rows are copied unchanged, and missing
rows in the authorized fallback families derive semantic target vectors from
the current layout. The eleven explicit LFE-target cells, malformed identities,
zero-survivor fallback families, friendly names, and unsupported combinations
fail explicitly rather than falling through to dynamic geometry. Automatic
control never fabricates a nearest-speaker route.

A minimal 5-channel Base plus one ReconstructionBasis row control file is:

```json
{
  "schema": "openjoc.joc-render-control.v1",
  "topology": {
    "explicit_groups": [],
    "fixed_layout": [],
    "dynamic_records": [
      {"descriptor":{"source_class":"explicit_channel","identity":"FL","coordinates":[0.5,0.5,0.0],"spread":null,"paired":null,"raw3":null},"scalar":1.0,"active":true},
      {"descriptor":{"source_class":"explicit_channel","identity":"FR","coordinates":[0.5,0.5,0.0],"spread":null,"paired":null,"raw3":null},"scalar":1.0,"active":true},
      {"descriptor":{"source_class":"explicit_channel","identity":"FC","coordinates":[0.5,0.5,0.0],"spread":null,"paired":null,"raw3":null},"scalar":1.0,"active":true},
      {"descriptor":{"source_class":"explicit_channel","identity":"Ls","coordinates":[0.5,0.5,0.0],"spread":null,"paired":null,"raw3":null},"scalar":1.0,"active":true},
      {"descriptor":{"source_class":"explicit_channel","identity":"Rs","coordinates":[0.5,0.5,0.0],"spread":null,"paired":null,"raw3":null},"scalar":1.0,"active":true},
      {"descriptor":{"source_class":"explicit_channel","identity":"FC","coordinates":[0.5,0.5,0.0],"spread":null,"paired":null,"raw3":null},"scalar":1.0,"active":true}
    ]
  },
  "updates": []
}
```

The optional sidecar must be authored for the input stream's decoded
coordinate count; the example is not a universal JOC mapping. Unsupported or
withheld bridge semantics fail explicitly.

## Expert contribution-isolation diagnostic

`--diagnostic-contribution` is an experimental, expert-only fidelity
diagnostic. It is not a normal rendering feature. Its three values are:

- `full` (default): render Base full-band coordinates plus
  ReconstructionBasis coordinates, and copy Base-carried LFE through the
  current LFE path. Omitting the option is identical to selecting `full`.
- `base-only`: preserve every Base full-band coordinate and LFE, but replace
  every ReconstructionBasis PCM plane with exact zero PCM.
- `reconstruction-only`: preserve every ReconstructionBasis PCM plane, replace
  every Base full-band PCM plane with exact zero PCM, and emit zero LFE.

For example, create the three speaker renders from the same decoded stream and
layout:

```sh
openjoc render-joc INPUT.m4a --layout 7.1.4 \
  --diagnostic-contribution full --output MyMix-7.1.4-full.wav
openjoc render-joc INPUT.m4a --layout 7.1.4 \
  --diagnostic-contribution base-only --output MyMix-7.1.4-base-only.wav
openjoc render-joc INPUT.m4a --layout 7.1.4 \
  --diagnostic-contribution reconstruction-only --output MyMix-7.1.4-rb-only.wav
```

The masking occurs only where the canonical codec-basis partition is still
explicit: Base coordinates first, then ReconstructionBasis rows. A masked
coordinate remains present with zero PCM. Its canonical ordinal, topology,
descriptor, metadata inheritance, binding record, projection target, and Q32
scheduler are unchanged. All modes retain the same access-unit timing, bridge
control, layout, WAV writer, channel order, and sample timeline. For the
current linear, non-postprocessed speaker path, the expected numerical check
is `FULL ≈ BASE_ONLY + RECONSTRUCTION_ONLY`, with LFE owned by `BASE_ONLY`.

ReconstructionBasis rows are diagnostic reconstruction coordinates, not
authored-object stems. Reconstruction-only audio may therefore sound
residual-like or unusual without implying a decoder defect. Its purpose is to
expose spectral or temporal artifacts, contribution magnitude, and the effect
of adding the reconstruction coordinates to Base.

Use this listening decision tree without treating subjective listening as
proof:

- Clean `BASE_ONLY` plus degraded `FULL` means the defect requires
  ReconstructionBasis participation; bad ReconstructionBasis PCM and bad
  ReconstructionBasis weighting remain distinct possibilities.
- Degraded `BASE_ONLY` means projection/mixing of the Base coordinates is
  sufficient to produce the defect, making current renderer semantics the
  primary suspect.
- Severe aliasing, discontinuities, or unstable metallic artifacts in
  `RECONSTRUCTION_ONLY`, beyond an expected residual-like character, make JOC
  reconstruction a stronger suspect but do not prove it is faulty.
- Clean `BASE_ONLY`, internally plausible `RECONSTRUCTION_ONLY`, and degraded
  `FULL` make interference, gain combination, or projection weighting the
  strongest suspect.

## Output contract

Every exposed preset has a deterministic semantic PCM order. The orders are:

```text
5.1:   FL, FR, FC, LFE, Ls, Rs
5.1.2: FL, FR, FC, LFE, Ls, Rs, TFL, TFR
7.1:   FL, FR, FC, LFE, Lb, Rb, Ls, Rs
7.1.2: FL, FR, FC, LFE, Lb, Rb, Ls, Rs, TFL, TFR
7.1.4: FL, FR, FC, LFE, Lb, Rb, Ls, Rs, TFL, TFR, TBL, TBR
7.1.6: FL, FR, FC, LFE, Lb, Rb, Ls, Rs, Ltf, Rtf, Ltm, Rtm, Ltr, Rtr
9.1:   FL, FR, FC, LFE, Lb, Rb, Ls, Rs, Lw, Rw
9.1.2: FL, FR, FC, LFE, Lb, Rb, Ls, Rs, Lw, Rw, Ltm, Rtm
9.1.4: FL, FR, FC, LFE, Lb, Rb, Ls, Rs, Lw, Rw, Ltf, Rtf, Ltr, Rtr
9.1.6: FL, FR, FC, LFE, Lb, Rb, Ls, Rs, Lw, Rw, Ltf, Rtf, Ltm, Rtm, Ltr, Rtr
```

The order is deterministic and is not the internal E-AC-3 order. For the
original `5.1` path it remains:

```text
0 FL, 1 FR, 2 FC, 3 LFE, 4 Ls, 5 Rs
```

The default sample format is IEEE float32. `--reference-f64` selects IEEE
float64. The `.wav` and `.caf` backends preserve the same PCM channel order.
The RcLfe/Base LFE plane is copied only to `LFE`; it is not sent through
ordinary spatial projection and is not double-added. The active bridge planes
are ordered by the selected preset's explicit channel identities before the
public WAV interleave.

Multichannel speaker WAV output uses WAVEFORMATEXTENSIBLE only when every
semantic channel has an exact standard speaker bit. Its channel mask has
exactly one standard speaker bit per interleaved channel, and the sample planes
are emitted in ascending mask-bit order. In particular, 7.1 and 7.1.4 keep the
back pair (`Lb`, `Rb`) before the side pair (`Ls`, `Rs`). 7.1.6 and the 9.1
family have no WAV mask; their semantic planes are serialized by CAF in the
order above, with distinct Wide labels and, where present, `Ltm`/`Rtm`
descriptions. Stereo binaural output and diagnostic WAVs retain their existing
basic WAV behavior.

The command prints the feature, experimental maturity, unresolved semantic
binding, requested and selected layout, channel count, LFE index, requested
and selected profile, compatibility deviations, sample rate, sample count,
and output channel order. `AUTO` evaluates
`ETSI_STRICT` first and selects `OBSERVED_VENDOR_COMPAT` only when the existing
whitelist admits all deviations. Explicit `ETSI_STRICT` never falls back.

`raw3` remains preserved and excluded from projection arithmetic. The stable
feature name is `JocSpatialBridge`; `SemanticBindingState` remains
`Unresolved`. This workflow makes no official vendor-equivalence, bit-exact,
or fidelity claim.

## Progress and performance diagnostics

When `render-joc` is attached to an interactive terminal, it writes a bounded,
throttled progress line to stderr. The line includes rendered audio, total
audio when the input can be indexed, elapsed time, and an estimated realtime
factor/ETA. Progress is disabled when stderr is not a TTY and can be disabled
explicitly with `--no-progress`. It uses no ANSI cursor or color assumptions;
stdout remains reserved for the final diagnostic summary. Progress is not
part of the render math and its measured write overhead is included in a
performance report.

For a successful render, `--performance-report FILE.json` writes a new JSON
file using schema `openjoc.joc-render-performance.v1`. If the WAV or report
already exists, an interactive terminal prompts once with `[y/N]`; Enter, `n`,
`no`, or EOF declines. `--overwrite` skips that prompt. Non-interactive runs
refuse existing outputs unless `--overwrite` is present. Authorized
replacements remain transactional, so a failed render preserves the previous
final files. The report contains the OpenJOC version, selected layout
and validation profile, sample rate, access-unit/sample/frame counts, audio
duration, wall duration, realtime factor, output byte count, build mode, and
timings for container loading, profile/index validation, E-AC-3 decode, JOC
reconstruction, bridge control assembly, spatial bridge render, optional
binaural render, and output conversion/container writing. It also records p50/p95/
p99/maximum core-frame timing and progress overhead. The report contains no
input or output paths, so it can be shared without exposing private fixture
locations.

The version-1 report also includes the additive object
`joc_reconstruction_stages_ms`. Its fields are `payload_parsing`,
`coefficient_decode`, `dequantization`, `qmf_analysis`, `interpolation`,
`matrix_reconstruction`, `qmf_synthesis`, `output_assembly`, and
`buffer_initialization`. These scopes are enabled only when a performance
report is requested and are diagnostic measurements; they are not a new codec
or rendering mode. Existing readers that ignore unknown JSON members remain
compatible.

The same schema now includes `eac3_decode_stages_ms`,
`eac3_decode_workload`, `eac3_decode_frame_ms`, and the bounded 16-entry
`eac3_slowest_frames` list. The measured core boundaries cover syncframe/header
parsing, block syntax and exponents, bit allocation, mantissa unpacking and
dequantization, coupling/rematrix/SPX, inverse transform, window/overlap-add,
PCM assembly, allocation/copy work, decoder-state commit, and residual
`other`. Workload counters identify syncframes, blocks, channel/LFE blocks,
long/short transforms, AHT elements, and coupling/SPX blocks without retaining
payload bytes, filenames, or paths. Slow-frame records contain only the AU
index, duration, and those aggregate stage counters/timings.

The top-level `eac3_decode` timer covers only stateful base-audio decode. The
`joc_reconstruction` timer now stops when `PayloadDecoder::decode_frame`
returns, before render-sink dispatch. Bridge, renderer, WAV, and progress work
therefore remain outside both codec timers; `core_frame_processing_ms` remains
the intentional end-to-end per-AU latency distribution. A deterministic unit
test locks this timing-scope boundary.

The focused E-AC-3 release harness excludes JOC reconstruction, spatial
rendering, progress, and file output:

```sh
OPENJOC_EAC3_BENCH_AUS=200 OPENJOC_EAC3_BENCH_RUNS=7 \
  cargo test -p openjoc-eac3 --release --test syncframe \
  eac3_core_release_benchmark -- --ignored --nocapture --test-threads=1
```

Add `OPENJOC_EAC3_BENCH_STAGE_TIMING=1` to emit the core sub-stage totals. The
bounded public-syntax I0/D0 input exercises two stateful six-block decode and
TDAC paths, but it is synthetic and is not real-media qualification. On an
Apple M2 Mac mini (8 cores, 8 GB), macOS 26.6, Rust/Cargo 1.94.0 release mode,
the seven-run 200-AU median changed as follows:

| Metric | Before | After |
|---|---:|---:|
| ms/AU | 1.547745 | 0.303612 |
| p50 | 1.376500 ms | 0.219292 ms |
| p95 | 1.956375 ms | 0.298417 ms |
| p99 | 2.128291 ms | 0.398208 ms |
| maximum | 2.285750 ms | 0.484750 ms |
| PCM checksum | `a0b0a5a0…64d6df29` | `a0b0a5a0…64d6df29` |

This is a 5.10x harness wall-time improvement. Timed core stage total fell
from 292.348 ms to 47.805 ms per 200 AUs, and inverse-transform time fell from
279.806 ms to 36.897 ms. The retained scalar implementation initializes the
finite long/short transform rotations once, reuses them in the unchanged f64
equations and accumulation order, and uses fixed arrays for transform-local
intermediates. No SIMD, multithreading, unsafe code, reduced precision, or
transform approximation is used. Native sampling before the change was
dominated by `inverse_long` and `__sincos_stret`; afterward no trigonometric
function remained in the steady-state profile and the scalar inverse transform
remained the largest residual function. Xcode Instruments allocation tracing
was unavailable on the Command-Line-Tools-only host; source/call-stack
inspection found residual short-lived vectors primarily in audio-block
syntax/exponent/AHT construction, while measured allocation/copy time was
1.5% of the optimized timed core total.

No real E-AC-3/JOC media is present on this development host, so the E-AC-3
performance classification is
`OPENJOC_EAC3_CORE_SUBSTANTIALLY_IMPROVED_REAL_MEDIA_RETEST_REQUIRED`.

The reconstruction-only release harness is separate from the speaker/WAV
harness and does not need a real media file:

```sh
cargo run -p openjoc-joc --release --example reconstruction_benchmark -- 1024 qmf
cargo run -p openjoc-joc --release --example reconstruction_benchmark -- 1024 pcm
```

`qmf` measures parsed JOC reconstruction against prebuilt QMF input; `pcm`
also includes QMF analysis. Both use a deterministic 15-object, 5-channel,
24-timeslot synthetic frame and report AU wall time, p50/p95/p99/maximum, and
stage totals. They are repeatable engineering harnesses, not real-media
qualification fixtures.

The local Apple-silicon release measurements that motivated the retained QMF
optimization were:

| Harness | Before | After | Dominant post-fix stage |
|---|---:|---:|---|
| 128-AU `qmf` | 20.49 ms/AU | 3.57 ms/AU | QMF synthesis |
| 1024-AU `qmf` | 21.68 ms/AU | 2.65 ms/AU | QMF synthesis |

The pre-fix native sampling profile was dominated by repeated
`__sincos_stret` calls from QMF synthesis. The retained fix constructs the
invariant f64 prototype and analysis/synthesis phase tables once, then reuses
them in the same scalar f64 equations. A post-fix 1024-AU run reported about
2.17 s in QMF synthesis, 0.14 s in matrix reconstruction, and 0.26 s in
interpolation; its checksum matched the pre-fix run exactly. No real DEE file
was available locally, so the supplied Windows evidence remains the
qualification baseline: 9,496 AUs, 303.872 s of audio, 671.052 s of JOC
reconstruction, approximately 70.7 ms/AU, and 0.388x overall realtime.

The checked-in release harness is a synthetic speaker/WAV measurement only:

```sh
cargo test -p openjoc-cli --release \
  joc_render::tests::performance_harness_speaker_and_wav \
  -- --ignored --nocapture
```

It runs 128 frames of 1,536 samples at 48 kHz, with a warmup and five measured
iterations by default. Set `OPENJOC_PERF_REPETITIONS=N` for a longer run. A
local Apple-silicon release-build run with 100 measured iterations produced:

| Layout | Sink | Median seconds | Realtime factor |
|---|---:|---:|---:|
| 5.1 | discard | 0.009262 | 442.229x |
| 5.1 | WAV | 0.018394 | 222.681x |
| 7.1.4 | discard | 0.015055 | 272.061x |
| 7.1.4 | WAV | 0.028092 | 145.808x |

These figures exclude real E-AC-3 decode, OAMD parsing, and JOC
reconstruction, so they do not qualify real-media realtime performance. The
local sampling profile found the scalar `JocSpatialBridge::render_coordinates`
loop dominant, followed by per-frame block construction and WAV encoding. A
reusable WAV interleave/encoding scratch path and a bounded stack-backed
coordinate view were retained after measurement; a speculative scheduler
gain-buffer change was rejected because it regressed the release harness.

The current qualification classification is:

```text
OPENJOC_JOC_RENDER_PROFILED_REAL_MEDIA_RETEST_REQUIRED
```

No real DEE E-AC-3/JOC media is present in this public checkout. On Windows,
run the following exact placeholder command against the private file and
retain its JSON report for qualification:

```powershell
openjoc.exe render-joc "D:\path\to\DEE-file.mp4" `
  --layout 7.1.4 `
  --output "D:\path\to\DEE-render.wav" `
  --performance-report "D:\path\to\DEE-performance.json" `
  --overwrite
```

That retest must use a release build, record the machine/OS/toolchain, and
evaluate the report's realtime factor plus the core-frame p99 against the
project's stated realtime budget. Synthetic or public-fixture results alone
must not be reported as a real DEE qualification.

## Stereo speaker output and E-AC-3 decoder policy

`2.0` is speaker stereo, not binaural. Its physical output is always `FL, FR`:

```sh
openjoc render-joc INPUT.m4a \
  --layout 2.0 --downmix auto -o stereo.wav
openjoc render-joc INPUT.m4a \
  --layout 2.0 --downmix loro -o stereo-loro.wav
openjoc render-joc INPUT.m4a \
  --layout 2.0 --downmix ltrt -o stereo-ltrt.wav
```

`--downmix auto` follows the encoded `dmixmod` metadata; `loro` and `ltrt`
select the corresponding public stereo matrices. Optional encoded LFE mix
metadata may fold LFE into both channels; otherwise LFE is excluded. Base
back/height configurations without an admitted reduction remain fail closed,
while generic reconstructed/object projection to 2.0 remains available. No
playback crossover or bass-management DSP is added.

E-AC-3 dynamic-range control is metadata-driven, not a generic compressor:

```sh
openjoc render-joc INPUT.m4a --layout 7.1.4 --drc disabled -o output.wav
openjoc render-joc INPUT.m4a --layout 7.1.4 --drc line -o output.wav
openjoc render-joc INPUT.m4a --layout 7.1.4 --drc rf -o output.wav
openjoc render-joc INPUT.m4a --layout 7.1.4 --drc custom \
  --drc-boost 75 --drc-cut 50 -o output.wav
```

`--drc` accepts `disabled`, `line`, `rf`, or `custom`; custom boost and cut
values are percentages in `0..=100`. With no override, existing decoder
behavior is preserved, including full Line-mode `dynrng` where applicable.
`dialnorm` remains available as decoded metadata but is not applied as
calibrated playback-level normalization.

## SOFA-backed binaural rendering

The same real-JOC command can virtualize one supported speaker preset to stereo
through a caller-supplied admitted SOFA file:

```sh
openjoc render-joc INPUT.m4a \
  --binaural \
  --sofa listener.sofa \
  -o binaural.wav
```

The ordinary binaural command does not require a physical `--layout`. Its
product default virtual speaker field is `7.1.4`, and the output is always
two-channel stereo (`Left Ear`, `Right Ear`). To choose another internal field:

```sh
openjoc render-joc INPUT.m4a \
  --binaural --sofa listener.sofa --virtual-layout 9.1.6 \
  -o binaural-9.1.6.wav
```

This is speaker-virtualized binaural rendering:

```text
real JOC -> JocSpatialBridge -> selected virtual speaker layout
          -> exact or safely interpolated HRIR per non-LFE speaker -> Left/Right ears
```

It is not a direct moving-object HRIR renderer and makes no vendor or
reference-product headphone-rendering claim. `--virtual-layout` names the
internal virtual speaker field; it does not select physical output channels.
The legacy combination `--layout 7.1.4 --binaural-sofa HRTF.sofa` remains
accepted as a virtual-layout alias. `--layout` without binaural remains the
physical speaker output selector. `--layout` and `--virtual-layout` together
are rejected as ambiguous. `--layout 2.0` is never routed through SOFA.

All public virtual presets from `5.1` through `9.1.6` are eligible when the
selected SOFA provides exact or safely interpolatable coverage. Missing or
sparse directions fail closed; OpenJOC never aliases a direction to a nearest
speaker or silently omits a virtual channel.

The SOFA path is local and user-supplied; 0.6.0 does not bundle a generic HRTF
dataset. It is parsed only within the existing
strict `SimpleFreeFieldHRIR`/NetCDF classic CDF-1 scope. Listener basis is
explicitly shared with the renderer contract: local `+X` is right, `+Y` is
front, and `+Z` is up. The virtual directions cover front, side/rear, wide,
and front/middle/rear height positions. Exact lookup is preferred. Non-exact
directions use a bounded spherical-local segment/triangle interpolation with
common ear weights and delay-aligned HRIR shapes; all required directions are
prepared before output starts. The decoded JOC sample rate must equal the SOFA
HRIR rate;
OpenJOC does not silently resample either stream.

The admitted presets contain LFE. The simple CLI form defaults to
`--lfe-policy exclude`; an explicit renderer-level policy can be selected:

- `--lfe-policy exclude` omits the virtual LFE channel.
- `--lfe-policy equal-power-dual-mono` adds the virtual LFE to both ears at
  equal-power gain.

Neither policy is a JOC semantic interpretation or vendor bass-management
claim. The binaural backend defaults to `direct`, the existing causal FIR
reference. `--backend partitioned --partition-size 256` selects the existing
fixed uniform partitioned backend; the partition size must be a power of two.
Both backends preserve streaming state, emit the complete causal HRIR tail,
and write stereo Left-then-Right PCM/WAV without a temporary multichannel
speaker WAV. The default format is IEEE float32; `--reference-f64` selects
IEEE float64.

Diagnostics identify `JocSpatialBridge` as enabled, maturity as experimental,
`SemanticBindingState` as `Unresolved`, the selected validation profile, output
mode, virtual layout, SOFA filename, backend, HRIR coverage, LFE policy, and
tail contract. Private SOFA paths are not embedded in public metadata.

The automatic assembly currently supports explicit-channel beds, dynamic
point-like records, the admitted Region/Extent/ChannelLock records, and
explicitly supplied Fixed/Named route rows. `raw3` remains opaque with no
assigned semantic name. `AUTO` behavior is unchanged: strict validation is
selected first and the existing compatibility policy is used only where its
current whitelist admits it. Friendly Named names, linked limiting, delay,
bass management, and Region combinations outside the admitted subset remain
incomplete or withheld. Named fallback does not change the binaural policy.

`2.0` is an admitted ordinary speaker pair with channel order `FL, FR`, no LFE
output index, and the exact stereo WAVEFORMATEXTENSIBLE mask. Reconstructed
objects use the same generic full-XYZ point/topology projector as the other
layouts. The channel-based Base contribution is separately reduced with the
selected public E-AC-3 policy:

- `--downmix auto` follows `dmixmod`: `01` selects Lt/Rt, `10` selects Lo/Ro,
  and absent/reserved/not-indicated metadata defaults deterministically to
  Lo/Ro.
- `--downmix loro` uses the public Lo/Ro center/surround coefficients.
- `--downmix ltrt` uses the public matrix-surround polarity and coefficients.
- Optional `lfemixlevcode` metadata controls LFE fold-down; without it, LFE is
  excluded. No crossover, subwoofer redirect, or other bass-management DSP is
  performed.

The admitted 2.0 Base matrix is constrained to L/R/C/Ls/Rs and mono surround
(`Cs`) locations. Base back/height channels are rejected rather than reduced
with an invented coefficient. `--downmix` is a 2.0 speaker policy and cannot
be combined with SOFA binaural output. The 9.1 family remains exposed for CAF
speaker output using the authorized clean-room Wide-row geometry; its binaural
path is dataset-dependent on exact or safely interpolatable Wide HRIR coverage.
The generic bridge API can still accept caller-defined layouts at library

## Professional preset feasibility audit

The following audit covers the broader professional names without treating a
name alone as evidence of a clean geometry definition.

| Preset | Classification | CLI status | Reason / boundary |
|---|---|---|---|
| `2.0` | `SUPPORTED_WITH_BASE_CHANNEL_CONSTRAINT` | Exposed | Generic two-speaker object projection plus public Lo/Ro/Lt/Rt Base reduction; optional metadata-gated LFE fold-down; Base back/height locations fail closed. |
| `5.1` | `SUPPORTED_EXISTING_GEOMETRY` | Exposed | Uses the generic full-XYZ projector with explicit front/side bed rows and the original public order. |
| `5.1.2` | `SUPPORTED_EXISTING_GEOMETRY` | Exposed | Uses the generic full-XYZ projector with one upper row and public order. |
| `5.1.4` | `SUPPORTED_EXISTING_GEOMETRY` | Exposed | Uses the generic full-XYZ projector with two upper rows and explicit public order/mask. |
| `7.1` | `SUPPORTED_EXISTING_GEOMETRY` | Exposed | Uses the generic full-XYZ projector with explicit front/side/rear bed rows and public order. |
| `7.1.2` | `SUPPORTED_EXISTING_GEOMETRY` | Exposed | Uses the generic full-XYZ projector with one upper row and explicit public order/mask. |
| `7.1.4` | `SUPPORTED_EXISTING_GEOMETRY` | Exposed | Uses the generic full-XYZ projector with explicit bed/top rows and public order. |
| `7.1.6` | `SUPPORTED_SEMANTIC_CAF_ONLY` | Exposed | Speaker output retains the existing three-row geometry and CAF-only identity boundary; binaural virtual output is ready when Top Middle directions are exact or safely interpolatable. |
| `9.1` | `SUPPORTED_SEMANTIC_CAF_ONLY` | Exposed | Speaker output retains the authorized Q15 Wide bed and CAF-only identity boundary; binaural virtual output is ready when Wide directions are exact or safely interpolatable. |
| `9.1.2` | `SUPPORTED_SEMANTIC_CAF_ONLY` | Exposed | Reuses the 9.1 bed and one existing upper row; binaural virtual output is dataset-dependent on Wide and upper-direction coverage. |
| `9.1.4` | `SUPPORTED_SEMANTIC_CAF_ONLY` | Exposed | Reuses the 9.1 bed and existing two-row upper topology; binaural virtual output is dataset-dependent on Wide and upper-direction coverage. |
| `9.1.6` | `SUPPORTED_SEMANTIC_CAF_ONLY` | Exposed | Reuses the 9.1 bed and existing three-row upper topology; binaural virtual output is dataset-dependent on Wide, Top Middle, and upper-direction coverage. |
| `22.2` | `BLOCKED_BY_CLEAN_GEOMETRY_DEFINITION` | Not exposed | The generic engine can represent a 24-channel 3D layout, but no clean/public 22.2 speaker geometry is admitted in this repository. |

The 22.2 result is not a renderer-domain limitation. If a clean speaker
definition is later admitted, adding its channels, geometry, LFE designation,
and public order is expected to be `DATA_ONLY`; no JOC bridge mathematics or
source-class behavior needs to change. The tests exercise a synthetic
24-channel layout to separate renderer capacity from professional-layout
provenance.

## Large-channel output audit

The renderer and in-memory `RenderedBlock` are N-channel: channel count is
derived from the validated layout and each channel is accumulated in its own
`Vec<f64>`. The ordinary WAV writer accepts the same dynamic channel count;
the current implementation is RIFF-only, so data sizes beyond the 32-bit RIFF
limit are rejected and no RF64 writer is provided. WAV output also carries no
speaker output carries standard WAVEFORMATEXTENSIBLE speaker-mask metadata;
generic diagnostic WAV output may remain basic WAV where no speaker identity is
required. `SemanticBindingState` remains `Unresolved`, and no vendor-fidelity
claim is made.

This distinguishes renderer support from full container description and from
third-party DAW interoperability. No 22.2 interoperability claim is made.
