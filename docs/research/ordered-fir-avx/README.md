# Ordered adjacent-output AVX direct FIR

This 2026-10-07 experiment accelerates the current-input interior of the static
`BinauralRenderer` FIR. Production commit:
`97a25a00151c2df6ae9e17ef9ecf8e140af3a0bb`.
Performance baseline: `291900ce33ea349c4b855655cdb282819e94eabc`.
Independent frozen PCM oracle: `15aefe1baa7b40f37950df252b6dbf6179894d6d`.
The earlier rejected gain-cache and QMF synthesis-row experiments are excluded.

## Results and measurement boundary

Seven alternating baseline/candidate pairs per mode used the existing
`pcm_regression_probe --timing-only` on one authorized, non-redistributable
152.704-second recording. Mode order was D1, D2, stereo, speaker. Both binaural
modes and the speaker control used layout 7.1.4; stereo used 2.0. The final source
and both binaries were unchanged throughout the campaign.

The primary metric is `summary_api_pipeline_wall_ns`: summed API
push/receive/drain **wall time**. It is not process CPU time, CPU cycles, complete
command elapsed time, or an endpoint playback measurement. Initialization,
warmup, PCM file serialization, hashing and final report formatting are outside
this metric. Returned PCM is consumed by the timing-only probe. No stage timing
or allocation instrumentation was enabled. The probe's separate process-CPU
field has a different scope and is not used for these percentages.

| Mode | Median paired wall-time change | All seven paired changes |
|---|---:|---:|
| Binaural D1 | **−24.37%** | −25.88% to −22.45% |
| Binaural D2 | **−22.88%** | −25.35% to −21.65% |
| Stereo control | −0.04% | −3.15% to +1.69% |
| Speaker control | **+2.29%** | −0.14% to +3.62% |

All 14 binaural pairs improved. Stereo is approximately flat in this sample.
Speaker was slower in six of seven pairs: this is an observed control regression,
not a zero-regression result. Its processing source was unchanged, and all PCM
and configuration checks passed; the cause of the timing shift was not
established. Do not subtract it from, hide it behind, or relabel it as the
binaural gain. These results support a useful binaural improvement on this host,
not a universal performance improvement across modes or machines.

The unchanged public direct-FIR probe uses 11 synthetic sources, 256 taps,
1,536-sample blocks, eight warmups and 128 calls per timed leg. Nine alternating
pairs gave a median **−62.37%** call wall-time change, with every pair improving
(−65.23% to −61.61%). Each leg checks complete f64 block-plus-tail bits before
timing and the final timed block afterward. These synthetic coefficients are
not described as built-in HRTFs.

### Short blocks and filters

Final longer-batch controls retained all nine alternating pairs per shape.

| Taps | Block samples | Median paired wall-time change |
|---:|---:|---:|
| 1 | 1 | +14.18% |
| 1 | 4 | −0.68% |
| 3 | 8 | −4.50% |
| 8 | 16 | −19.92% |
| 16 | 64 | −49.58% |
| 256 | 128 | −0.61% |
| 256 | 259 | +3.13% |
| 256 | 260 | −0.80% |
| 256 | 512 | −38.47% |

The 1-tap/1-sample absolute call medians were approximately 218 ns before and
241 ns after, over all 11 sources. The ratio of these independent medians need
not equal the median of paired ratios. This small absolute regression is
retained explicitly. Blocks shorter than four samples use the scalar body, and
the caller avoids a SIMD helper call when fewer than four interior outputs
remain. Other near-boundary shapes do not establish a useful gain.

## Arithmetic and safety contract

Each AVX lane represents one adjacent output sample, initialized from the
existing prior-source accumulator. For every lane, taps remain ascending and
operations remain separate: input × gain, gained input × tap, then accumulator
addition. The helper has no horizontal reduction, FMA, reassociation, precision
change, coefficient/gain precombination, HRTF change, or backend switch.

The scalar history prefix and remainder are unchanged. AVX detection includes
OS support; unsupported x86 CPUs and non-x86 targets retain scalar execution.
The helper requires AVX, not AVX2. Unaligned loads/stores are bounded by checked
slice lengths and `offset >= tap_count - 1`; no overread is allowed. No FTZ/DAZ
or other floating-point-mode setting is changed. Final release assembly contains
separate packed multiplies/adds and `vzeroupper`; the dispatch wrapper contains
no AVX instructions before its feature check.

Vector lanes may be computed ahead, but errors are still selected by registered
source, sample, then left before right. Numeric failure clears both public
outputs, preserves histories, and requires reset. Histories commit only after
all sources succeed. Source order, tail, reset, storage accounting and latency
are unchanged. A private const-generic selector lets tests force the scalar
body on an AVX host without a public or process-global override.

## Final-source validation

- Unchanged frozen gate: eight lifecycle/orientation cases plus full-recording
  D1/D2; complete PCM, output/configuration metadata, drain and lifecycle match
- Public 30/60-second corpus: all 16 stereo, speaker 2.0/7.1.4/22.2, D1/D2 and
  orientation D1/D2 cases pass the same strict gate
- Comparator self-tests and integrated one-bit mutation rejection pass
- Existing frozen old-render loop remains byte-for-byte unchanged; added f64
  bit/state tests cover vector boundaries, populated history, uneven chunks,
  multiple sources, signed zero, subnormals, extreme finite values, cancellation,
  prior-source accumulation, all four error lanes, earlier-right/later-left
  failure ordering, NaN/Inf input rejection, overflow, reset and drain
- Rust 1.98.1 focused release renderer tests and integrations pass; Rust 1.89.0
  release renderer tests pass **86/86**
- Final scoped workspace checks: **1,127 Rust tests passed, 16 ignored**;
  strict all-target clippy, default workspace release build, formatting,
  repository hygiene and diff checks pass
- Python suite: **224 total, 167 passed, 57 skipped** for platform-specific checks
- Rust 1.89.0 compilation passes for x86_64, aarch64-unknown-linux-gnu and
  wasm32-unknown-unknown
- All 28 timed mode-pairs match 12 required input/output/configuration identity
  fields; final source and benchmark binary identities were rechecked afterward

Native GStreamer and FFmpeg9 development libraries were absent locally. Their
native all-feature integration checks were excluded; their default-feature
checks remained covered. This is not a full workspace all-feature pass. No
system-package installation was retried. Existing native FFmpeg and player
packaging workflow filters already include `crates/openjoc-render/**`; no CI
filter change was necessary. Hosted CI for the final commit remains a separate
requirement, not a result claimed here.

Performance was measured only on a virtual AMD EPYC 9V74 x86_64 Linux host,
using official Rust 1.98.1 / LLVM 22.1.8, default release/default embedded-HRTF
features and no custom native/fast-math flags. ARM/wasm checks are compilation
checks. Native ARM performance, actual old-CPU dispatch execution, Windows/LAV/
PotPlayer and physical endpoint behavior were not measured.

## Evidence and reproduction

- [API wall-time pairs](api-wall-pairs.csv): all 28 final paired measurements
- [Micro wall-time runs](micro-wall-runs.csv): all explored integrated revisions,
  with the frozen production result marked `final`
- [Short-control wall-time runs](short-control-wall-runs.csv): every measured
  short-shape run, including regressions and the final longer batches
- [Standalone wall-time runs](standalone-wall-runs.csv): safe unrolling/AVX search;
  its standalone baseline did not reproduce the full renderer's existing
  left/right SSE2 packing and is not used for production gain claims
- [Provenance](provenance.json) and [complete commands](REPRODUCE.md)

The private recording, its filename/fingerprint, PCM and raw configuration
fingerprints are not published. CSVs use explicit numeric-field allowlists.
Private raw evidence remains local. Public sources reproduce the procedure and
public-corpus checks; reproducing the private timing numbers requires the same
authorized recording and environment, neither supplied by this report.
