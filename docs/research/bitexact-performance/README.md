# Bit-exact performance baseline and gate

## Final outcome

The requested CPU-benefit bar was not met. The proposed gate/report branch
contains the reusable exactness harness, CI coverage, and evidence only; it
contains no product performance changes. Candidate A and Candidate D remain on
the separate local experiment branch `perf/experimental-bitexact-candidates`.
Both preserve exact PCM in their tested scope and reduce measured allocations,
but neither establishes a repeatable CPU improvement. **No actual OpenJOC
real-time CPU improvement has been established, and neither product change is
recommended for merge.**

| Experiment | Exact PCM result | CPU gain | Final decision |
|---|---|---|---|
| Candidate A, owned-frame helper, six API paths | Pass in the tested 30/60-second matrix | Unproven; mixed medians and overlapping ranges | Excluded |
| Candidate D, optional partitioned FFT, P=1/64/128/256 | Pass bit-for-bit; P=7/257 errors unchanged | Unproven; slower median and faster in 2/5 pairs | Excluded |

## First-milestone status

This milestone adds a paired benchmark and PCM oracle only. It does not contain
a production optimization. The candidate and frozen baseline product trees are
both at `15aefe1baa7b40f37950df252b6dbf6179894d6d`; the candidate's only product
source change is test-only synthetic-fixture support in `openjoc-ffmpeg`.
The baseline worktree is checked before every paired run: its HEAD and lockfile
are pinned, tracked differences are restricted to the exact allocator-feature
manifest overlays, and untracked files are restricted to byte-identical probe
examples. Baseline/candidate worktrees, Cargo target directories, and report
directories must be canonical and non-overlapping. Both revisions are built
separately in release mode using the same Rust 1.89.0 toolchain, flags, feature
set, and input. There is no `--skip-build` path.

The baseline SHA-256 for `Cargo.lock` is
`40f4a91c662652bef309f2bf57d01cfa811c5d004ebf9d194bf9e688d4ab45e7`.
Current harness SHA-256 values after the final documentation-only probe comment
clarification:

- API probe: `cad13d95eeb1450c835fa28bed1761bf431e6eae64e130ab63ca4cca56a121b2`
- ordinary-core probe: `60b2eff158f269c30ca16db96b6a6578dcfe0c850ad5c435716110e0696630b4`
- paired runner: `57de566e5f629deb2a53781e32c371db7bdca35fb0c3c05cbac9c71fc074cd3e`
- corpus generator: `4f90ad1fe1c76d395a3764797739a94ba8f992326c959fdf873a83bc857e88b0`

The 60-second matrix immediately preceded that comment-only clarification; the
current source was then rebuilt and exact-checked on the 128-AU API fixture and
the 30.016-second ordinary-core case. No executable probe logic changed between
those runs.

The exact oracle compares the complete PCM stream byte-for-byte, including
every `f32::to_bits` value, signed zero, frame/sample counts, output channel
order/labels, sample rate, timestamps, configuration descriptors, delayed
output, and drain tails. Ordinary E-AC-3 core also produces a full-precision
interleaved `f64` plane; the gate requires and compares that stream as well as
the public `f32` conversion. Missing or inconsistent `f64` streams fail
closed. No tolerance, fast-math, altered precision, or relaxed comparison is
used.

Negative-sensitivity checks include an exact control followed by one-bit
mutations to signed zero, ordinary samples, and drain-tail samples; descriptor
changes (sample/channel count, sample rate, timestamp, and configuration);
truncation; and appended data. The paired run also mutates a disposable output
copy and requires a specifically reported PCM-bit mismatch. Standalone tests
cover missing core-`f64` output, the baseline tree allowlist/overlap checks, and
retention of API captures that do not produce an `f64` file.

## Inputs and exact-gate evidence

The checked-in lifecycle fixture is `crates/openjoc-wasm/testdata/joc.lifecycle.ec3`
(SHA-256 `b860509a1613134931e1e39b9d2b6d4d31687b1a6586d8e2bcf5fe99e7da14f8`,
128 AUs). Long inputs are generated once by `scripts/generate_performance_corpus.py`
with FFmpeg 7.1.5 and reused byte-identically by both builds:

| Input | SHA-256 | AUs/syncframes | Programme duration | Construction/coverage |
|---|---|---:|---:|---|
| `joc.lifecycle.30s.ec3` | `a44fc36470d07f98c68053c9015e3cb169a21af927b1b3b93bd5712a42234671` | 938 | 30.016 s | Validating JOC lifecycle sequence, varied metadata/excitation on every AU |
| `joc.lifecycle.60s.ec3` | `8202e69a5cd13c9142165315f7640d2d7c3cf554b6de0bba8b2c5c40cc499cf2` | 1,875 | 60.000 s | Same sequence; includes decoder-valid 10-bit counter transition 1023→1 |
| `ordinary.multitone.30s.eac3` | `a6a22c1e5735bc5674734dc1ab40db735f69aff9fe202439b54a9aeaaf9d3fd9` | 938 | 30.016 s | Independent-only, six distinct tones with per-channel amplitude modulation |
| `ordinary.multitone.60s.eac3` | `0b98eb8c92ad542cbda37c402e420e74f8a9ddd25e72cd440788be110ce4a136` | 1,875 | 60.000 s | Same continuous multi-tone construction |

The JOC generator advances sequence values 1…1023→1 explicitly; it does not
assume a natural 1023→0 wrap. The synthetic long inputs are useful continuous
state/excitation probes, not representative real programme material. They are
not checked into the repository.

Latest-harness exact gates, all built from separate baseline/candidate target
directories and compared before per-case PCM files were discarded:

- 128-AU checked-in fixture, speaker 2.0: **PASS**; 129 output frames,
  196,640 samples including the delayed/drain tail, 2 channels, and
  1,573,120 compared PCM bytes. Disposable one-bit mutation was rejected.
- 30.016-second JOC D1 binaural 7.1.4 and ordinary E-AC-3 core: **PASS**.
  Binaural output was 939 frames × stereo, 1,441,023 samples including its
  255-sample tail. Core output was 938 syncframes, 1,440,768 samples, six
  channels, and 34,578,432 public `f32` PCM bytes; its full `f64` stream also
  passed exact comparison.
- 60-second paired matrix: **PASS** for speaker 2.0, 7.1.4, and 22.2; D1
  binaural 7.1.4; D2 binaural 22.2; D1 orientation 7.1.4; and ordinary
  E-AC-3 core. Speaker cases emitted 1,876 frames and 2,880,032 samples
  (including a 1,568-sample delayed/drain tail); binaural/orientation cases
  emitted 2,880,255 stereo samples including the 255-sample tail; core emitted
  1,875 syncframes, 2,880,000 samples, and six channels. The core `f64` plane
  passed exact comparison.

The CI-ready `pcm-bitexact` job runs the original short-input gate across five
configurations: speaker 2.0/7.1.4/22.2, D1 binaural 7.1.4, and D1 orientation
7.1.4. It also runs the optional partitioned-backend exact matrix above. The
workflow has been added locally but has not run on hosted CI yet. Neither step
claims coverage of the generated 30/60-second API/core cases.

## Timing and hotspot observations

Plain release timing is deliberately separate from instrumented stages and
allocation runs. `--timing-only` does not capture/hash/write PCM in the AU
loop: it black-boxes and drops each returned frame inside the measured call,
preallocates the timing vector, and writes observations after processing.
Baseline/candidate timing runs also require equal input/config/layout, latency,
AU/frame/sample/tail/channel summaries. The exact PCM capture run remains the
separate acceptance oracle. Constructor, warmup, reset, first-output,
steady-call percentiles, and drain timing are reported separately. For
orientation runs, render-pipeline time excludes control preparation/apply and
retired-kernel drop; the harness reports orientation-control time and an
inclusive RTF as separate measures.

Here RTF is measured wall time divided by input audio duration; lower is
faster, 1.0 is real-time throughput, and values above 1.0 mean that the measured
render took longer than its audio duration. Candidate A's D2 binaural 22.2
case is above 1.0 on this synthetic Linux host, so it was slower than real time
in that tested case. This is not a result for real programme audio, LAV,
Windows, or an i5-8250U and must not be extrapolated to those systems.

Latest plain timing-only results on the generated 30.016-second inputs used
three sequential, alternating-order baseline/candidate pairs per case:

| Case | Baseline median RTF (range) | Candidate median RTF (range) | Reading |
|---|---:|---:|---|
| JOC speaker 2.0 | 0.12107 (0.11958–0.12121) | 0.12475 (0.12153–0.12544) | No speedup conclusion; run/host spread is material |
| Ordinary E-AC-3 core | 0.02630 (0.02618–0.02964) | 0.02731 (0.02691–0.02922) | No speedup conclusion; run/host spread is material |

Baseline and candidate contain the same product implementation, so these
measurements calibrate the harness and expose noise; they are not a product
comparison or a performance claim.

Instrumented stage snapshots locate plausible work, but their timings must not
be used as speedup evidence. In a 128-AU speaker 2.0 run, decode accounted for
about 96% of measured API stage time. In a 128-AU D2-binaural 22.2 run, binaural
render accounted for about 87% of stage time. In a 32-syncframe ordinary-core
run, synthesis accounted for about 68% of API pipeline time. A separate
counting-allocator build observed roughly 1,756 allocations and 1.87 MB of
requested bytes per 2.0 AU; 1,946/2.18 MB for 7.1.4; 2,144/2.57 MB for 22.2;
and 2,004/0.679 MB for ordinary core. These are cumulative allocator request
counts, not live memory, retained bytes, copied bytes, peak RSS, or elapsed-time
gains.

A profiler sample could not be collected because Linux `perf` is not installed
in this execution image. The container reports Linux 6.18.44 x86_64 but does
not expose CPU topology (`lscpu` cannot read `/sys/devices/system/cpu/possible`).
No CPU-model, laptop, Windows, or Intel 8250U performance claim is made.

## Reproduction

From the candidate worktree, create the detached oracle and apply the identical
probe/allocator-feature overlays. The baseline tree validator permits only
these exact harness overlays:

```sh
BASELINE=/tmp/openjoc-perf-baseline
git worktree add --detach "$BASELINE" 15aefe1baa7b40f37950df252b6dbf6179894d6d
mkdir -p "$BASELINE/crates/openjoc-api/examples" "$BASELINE/crates/openjoc-eac3/examples"
cp crates/openjoc-api/Cargo.toml "$BASELINE/crates/openjoc-api/"
cp crates/openjoc-api/examples/pcm_regression_probe.rs "$BASELINE/crates/openjoc-api/examples/"
cp crates/openjoc-eac3/Cargo.toml "$BASELINE/crates/openjoc-eac3/"
cp crates/openjoc-eac3/examples/eac3_pcm_regression_probe.rs "$BASELINE/crates/openjoc-eac3/examples/"

python3 scripts/verify_pcm_bitexact.py --self-test
python3 -m unittest scripts/tests/test_pcm_bitexact_gate.py -v

python3 scripts/verify_pcm_bitexact.py \
  --baseline-root "$BASELINE" \
  --candidate-root "$PWD" \
  --baseline-target /tmp/openjoc-target-baseline \
  --candidate-target /tmp/openjoc-target-candidate \
  --output-dir /tmp/openjoc-pcm-gate \
  --selftest-integrated \
  --case lifecycle-2.0,crates/openjoc-wasm/testdata/joc.lifecycle.ec3,speaker,2.0
```

The optional partitioned-backend same-implementation oracle uses a generated
sidecar once and shares its exact bytes between the two builds. The runner
copies only its allowlisted probe example into the detached baseline and checks
supported/unsupported partition behavior, silence/reset/replay, source order,
tail chunking, descriptors, and every f64 output bit:

```sh
python3 scripts/verify_partitioned_fft_scratch.py \
  --baseline-root "$BASELINE" \
  --candidate-root "$PWD" \
  --baseline-target /tmp/openjoc-partitioned-target-baseline \
  --candidate-target /tmp/openjoc-partitioned-target-candidate \
  --output-dir /tmp/openjoc-partitioned-gate \
  --timing-pairs 5
```

Use `RUNS=../openjoc-perf-run` for the owned report directory. Use
`--timing-only --repeats 3` for plain paired timing, or `--stage` and
`--allocations` in separate instrumented runs. The runner always rebuilds both
revisions, so compiler, flags, build profile, input, and configuration are
provenance logged. Set `--online` only when locked Cargo dependencies are not
already available; no dependency or lockfile was changed for this milestone.

Raw logs and full `measurements.csv` files are produced under an owned scratch
directory outside the worktree. From this repository root, the standard local
location is `../openjoc-perf-run`; set `RUNS=../openjoc-perf-run` when
reproducing the commands above. Exact PCM is stream-compared and deleted
casewise to bound disk use. Compact raw measurements and provenance for the
first product candidate are checked in as
[candidate-a-timing60s.csv](candidate-a-timing60s.csv),
[candidate-a-allocations.csv](candidate-a-allocations.csv),
[candidate-a-exact-gates.csv](candidate-a-exact-gates.csv), and
[candidate-a-provenance.json](candidate-a-provenance.json); no PCM stream or
machine-private absolute path is tracked.

## Candidate A: owned-frame clone removal

Candidate A is preserved as experiment commit
`d3df6bbc7672eec06ccd4652f33806679200e10e` on the local experiment branch. In
`openjoc-api/src/lib.rs`, a borrowed payload callback previously deep-cloned
`DecodedPayloadFrame` into an `Option`; the call now uses the existing
`decode_frame_with_profile_and_binding_profile_to_frame` helper and the same
`?` error conversion. No arithmetic, source order, profile selection, commit,
reset/retry behavior, rendering, or public PCM logic changed. The baseline,
harness, candidate, feature, input, and compiler provenance is recorded in the
JSON file above.

Candidate A passes the full 30.016-second five-case and 60-second seven-case
exact matrices documented earlier, including the ordinary-core f64 plane,
plus the integrated one-bit sensitivity mutation. Focused test runs passed for
`openjoc-api`, `openjoc-scene`, `openjoc-capi`, `openjoc-ffmpeg`, and
`openjoc-wasm`; API timestamp reset/retry and scene payload commit/profile
tests remained green.

The separate 128-AU counting-allocator run shows exactly **64 fewer
allocations** and **100,795 fewer requested bytes per AU** in each of six API
paths; reallocations were unchanged. Representative cases are:

| API path | PCM exact | CPU gain | Allocations/AU baseline → A | Requested bytes/AU baseline → A |
|---|---|---|---:|---:|
| Speaker 2.0 | Pass | Unproven | 1,756.19 → 1,692.19 | 1,868,457 → 1,767,662 |
| Speaker 7.1.4 | Pass | Unproven | 1,946.24 → 1,882.24 | 2,184,401 → 2,083,606 |
| Speaker 22.2 | Pass | Unproven | 2,143.73 → 2,079.73 | 2,565,886 → 2,465,091 |
| D1 binaural 7.1.4 | Pass | Unproven | 1,938.30 → 1,874.30 | 2,147,621 → 2,046,826 |
| D2 binaural 22.2 | Pass | Unproven | 2,123.88 → 2,059.88 | 2,455,606 → 2,354,811 |
| D1 orientation 7.1.4 | Pass | Unproven | 1,938.30 → 1,874.30 | 2,147,621 → 2,046,826 |

Plain timing-only used five serial alternating pairs per 60-second case. Median
RTF and full observed ranges are recorded in `candidate-a-timing60s.csv`:

| Case | Baseline median (range) | Candidate A median (range) |
|---|---:|---:|
| JOC 2.0 | 0.12549 (0.12436–0.12655) | 0.12438 (0.11873–0.12604) |
| JOC 7.1.4 | 0.13947 (0.13650–0.14130) | 0.13931 (0.13492–0.14394) |
| JOC 22.2 | 0.15233 (0.15024–0.15505) | 0.15561 (0.15409–0.15858) |
| D1 binaural 7.1.4 | 0.59813 (0.58786–0.60911) | 0.61310 (0.60188–0.61504) |
| D2 binaural 22.2 | 1.10094 (1.08196–1.10749) | 1.09829 (1.07071–1.10595) |
| D1 orientation 7.1.4, render-only | 0.33972 (0.33655–0.34149) | 0.33748 (0.32883–0.34228) |
| Ordinary E-AC-3 core | 0.02769 (0.02674–0.02796) | 0.02784 (0.02667–0.02830) |

The medians are mixed and the observed ranges overlap. **No CPU speedup has
been demonstrated.** Candidate A is excluded from the proposed final branch;
its allocation reductions are not a substitute for CPU performance benefit.

The initial stage profile for 128-AU speaker 2.0 placed about 96% of API stage
time in decode; a D2-binaural 22.2 profile placed about 87% in the binaural
stage bucket. That bucket includes binaural rendering and frame materialization,
so those percentages are not attribution to E-AC-3 decoding or pure FIR. The
active API static binaural engine is the direct-FIR `BinauralRenderer`; the
optional partitioned rustFFT backend is not called on this path. Isolated
static/dynamic render probes found no evidence that per-sample source lookup
was the dominant remaining CPU cost. The optional partitioned path did reveal
repeated FFT scratch allocations; Candidate D evaluates that separate backend
below, without changing default direct-FIR or LAV behavior.

## Candidate D: optional partitioned FFT scratch reuse

Candidate D is preserved as experiment commit
`1d5d5d7ebbe65306bc0c28c7617f66d1ae87bffa` on the same local experiment
branch. It adds one renderer-owned scratch buffer sized to the greater of the
forward/inverse FFT scratch requirements and switches the two render-time
`Fft::process` calls to `process_with_scratch`. FFT plans, source/partition/ear
order, coefficients, complex multiplication, inverse scaling, tail state, and
default direct-FIR routing are unchanged. This applies only to the explicit
optional partitioned backend, not the default LAV/API direct-FIR path.

The same-backend exact gate used one input sidecar generated once and shared by
both builds (SHA-256
`a3290ac6a8d011056df398eb07cefe33b0f3617ecd9007a444f8ca85acee874f`). It used
two nonsilent sources with different HRIR lengths (300 and 533 taps), a silence
interval after nonzero state, another nonzero signal, and a reset/replay of the
first signal. The gate exercised source-block permutations, partial final
input, varied tail-drain chunks, and zero-input behavior after reset. Full
interleaved `f64` PCM matched byte-for-byte for supported partition sizes 1,
64, 128, and 256 (FFT sizes 2, 128, 256, and 512), 70,400 bytes per size. Both
revisions rejected unsupported sizes 7 and 257 with `InvalidPartitionSize`.
The replay output remained bit-identical and silent input produced zero PCM.
An integrated one-bit PCM mutation failed specifically as a PCM-bit mismatch.
Per-size output hashes and rejected sizes are in the compact CSVs below.

A separate counting-allocator run measured 33 allocations and 270,336
requested bytes per 256-sample block on baseline versus zero allocations and
zero requested bytes for D. Allocation probes were separate from plain timing.
Five alternating pairs each timed 1,024 warmed `render_partition` calls for
11 sources × 256 taps × 256 input samples (512-point FFT). The baseline median
was 61,635 ns/block and D's median was 66,221 ns/block; D was faster in only
2/5 pairs, with noisy overlapping runs. **This does not demonstrate CPU
benefit, so Candidate D is also excluded from the proposed final branch.** The
benchmark uses synthetic source/HRIR data, measures only the optional
partitioned render call, and is not programme-audio or end-to-end LAV evidence.

The raw paired timing table makes the mixed direction explicit. Every paired
measurement returned the same output digest, while the overall CPU-gain result
remains **unproven**:

| Pair | Baseline ns/block | Candidate D ns/block | Exact output | CPU gain |
|---:|---:|---:|---|---|
| 1 | 63,685.831 | 52,315.594 | Pass | Unproven |
| 2 | 58,207.208 | 59,980.075 | Pass | Unproven |
| 3 | 61,634.511 | 66,220.872 | Pass | Unproven |
| 4 | 59,678.897 | 72,608.786 | Pass | Unproven |
| 5 | 75,996.224 | 67,915.740 | Pass | Unproven |

Compact run-level evidence is tracked as
[Candidate D timings](candidate-d-timings.csv),
[allocation counts](candidate-d-allocations.csv),
[same-backend exact matrix](candidate-d-exact-gates.csv),
[unsupported sizes](candidate-d-unsupported-partitions.csv), and
[provenance](candidate-d-provenance.json). No raw PCM or absolute machine
paths are committed.

## Bounded follow-up

No further product candidates were evaluated in this bounded pass. Both
measured allocation reductions are recorded separately from the inconclusive
or adverse CPU timings; allocation counts alone do not satisfy the requested
CPU-benefit threshold. The final gate/report branch retains the repeatable
oracle and CI job without either experimental product edit.

## Remaining attribution priorities

These are source-backed profiling targets from the initial plan, not accepted
optimizations or claims that they dominate runtime:

1. Split the API 2.0 `decode` bucket before attributing it. That timed bucket
   covers E-AC-3 decoding, metadata, JOC payload parsing/reconstruction, and
   owned-frame construction; its roughly 96% share is not a measurement of
   E-AC-3 alone. If reopened, profile those sub-stages using the same release
   input, then evaluate the source-backed branch-local scene temporary clones
   (B) or initialized binding reuse fast path (C) only if their costs appear.
2. Split the D2 binaural bucket's renderer work from frame materialization.
   The default API/LAV path is static direct FIR through `BinauralRenderer`;
   the optional partitioned FFT experiment does not apply to it. The isolated
   dynamic-vs-static probe did not show the per-sample source lookup as the
   primary cost. Measure stable direct-FIR kernels and ownership separately
   before considering a buffer-lifetime change.
3. Recheck ordinary-core synthesis on a sustained valid input and separate
   transform, plane materialization, and bridge/ABI work. The earlier 68%
   synthesis share came from one 32-syncframe instrumented sample and is only
   directional; there is no sampling-profiler trace.
4. Lower-priority code hypotheses in the plan are dynamic source-index
   resolution (E) and region descriptor/layout clones (F); JOC/EMDF/E-AC-3
   storage-copy opportunities (G) carry more arithmetic/order/state risk and
   should remain deferred without profiler evidence. Do not remove state
   rollback clones or alter observable diagnostics as a shortcut.

### Read-only LAV/ABI inspection boundary

The initial plan inspected the LAV fork read-only at pinned source revision
[`8f32aad51aea24602f2175316a241568be5fc4a1`](https://github.com/chyinan/LAVFilters-OpenJOC/tree/8f32aad51aea24602f2175316a241568be5fc4a1/decoder/LAVAudio).
It found candidate ownership
boundaries: the Rust decoder copies borrowed PCM into an owned vector;
`LAVAudio.cpp` allocates/appends `BufferDetails`, conditionally swaps or appends
queued output, and copies into the final DirectShow sample. Other paths include
label/layout validation and recursive-mutex-protected inspection. These are
alternative paths, not one cumulative copy chain. The C ABI returns retained
borrowed PCM and adds no PCM copy; API and frame buffers have lifetime contracts
that prevent casually recycling them. Native AVFrame paths also copy into
owned buffers. No copy is assumed redundant without a preserved ownership,
lifetime, size-check, queue, retry, and flush contract. The strict postprocessor
lane validates and optionally gathers statistics before returning; no extra
resampling cost was attributed to it.

This was static source inspection only. No Windows build, native LAV/DirectShow
runtime, PotPlayer run, physical endpoint delivery, or i5-8250U test was
performed. No user-hardware, Windows, or playback-stability claim follows from
these source references.

## Final validation on the gate/report branch

The final local branch is based on `69acffd` and contains only harness, CI, and
report changes; its tracked product-source diff is empty.
Validation on Linux x86_64 with Rust 1.89.0:

- `cargo +1.89.0 fmt --check` — pass.
- `CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 cargo +1.89.0 test --workspace --release --locked -- --test-threads=2 --quiet` — 1,102 passed, 0 failed, 16 explicitly ignored across 116 test binaries/doc-test groups.
- `CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 cargo +1.89.0 clippy --workspace --release --locked --all-targets -- -D unknown_lints -D warnings` — pass. Rust 1.89 emits the repository's pre-existing unknown-clippy-lint warnings for `clippy::manual_is_multiple_of` and `clippy::chunks_exact_to_as_chunks`.
- `python3 -m unittest discover -s scripts/tests -v` — 197 total, 141 passed and 56 skipped because optional Windows/GCC/LAV/public-vector environments are absent; repository hygiene and comparator sensitivity tests are included.
- `git diff --check` and CI YAML parsing with PyYAML — pass.
- Final branch paired API 2.0 short-fixture gate — pass, 196,640 samples / 1,573,120 PCM bytes; integrated bit mutation rejected.
- Final branch optional partitioned runner — pass for exactness at P=1/64/128/256 and preserved `InvalidPartitionSize` at P=7/257. This is a same-product CI-harness simulation on the gate/report branch; Candidate D's actual changed-vs-baseline evidence is separately recorded above.

The workflow change is local and has not run on hosted CI. Native Windows SDK,
LAV/DirectShow, PotPlayer, long-duration device playback, and the requested
8250U target remain untested.

An initial debug-mode workspace test attempt stopped with `ENOSPC` while
creating a test-target cache and reported no failed test assertion. After
removing only this task's newly generated reproducible debug cache, the full
workspace release-mode run above completed successfully.

Current explicit gaps: no real programme corpus; the generated 30/60 inputs
are 48 kHz only; 60-second orientation D2 and bounded-pull modes are not in the
current gate matrix; the original API/core CI corpus uses only the short fixture; no
sampling-profiler trace is available. The local CI workflow change has not
been run on hosted CI. These gaps are not inferred away by the synthetic
results.
