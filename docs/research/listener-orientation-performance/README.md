# Listener orientation preparation performance — 2026-10-05

This is dated benchmark evidence, not an end-to-end playback acceptance report.
Baseline: PR #12 head `668681879de4281615cb68ff467afc5d91864af4`.
Current behavior and limitations remain owned by
[the canonical limitations page](../../site/compatibility/known-limitations.md).

## Scope and method

- Host: x86_64 Linux, AMD EPYC 9V74 virtual CPU, 9 visible CPUs; optimized Rust
  1.89.0, Cargo release defaults, no target-cpu/native flags, no CPU affinity.
- Both built-in resources at this head: SADIE II D1 KU100 and D2 KEMAR.
- Layouts: 2.0, 5.1, 7.1.4, 9.1.6. **22.2 is not measured:** constructor
  rejects it with `no binaural direction for BL` on both revisions. This is an
  existing orientation-layout limitation, not an optimization regression.
- 2,000 calls per layout/resource/trajectory, after 30 warmup calls. All 64,000
  final calls succeeded. `identity` is static; `rapid` jumps 137 degrees about
  a non-axis-aligned unit axis; `nearby` repeats four adjacent poses; `smooth`
  repeats the original PR's 120-pose yaw30/pitch15/roll10 sinusoidal trajectory.
- Wall-time quantiles are empirical sorted samples at n/2 and floor(0.95n),
  with the maximum observed sample reported separately. They include host
  scheduling jitter. No percentile is a hard deadline or confidence bound.
- CPU is mean Linux thread CPU time per prepare call. Allocations count alloc
  and realloc calls and requested bytes, not live/peak RSS. Results exclude
  bank/index initialization, input pose generation and returned-update destruction.
  Both versions use the same counting allocator; diagnostic stage timing is
  **disabled** for the acceptance table. CPU clock and allocator overhead are
  not subtracted. Error count is explicit; rejected calls are never silently dropped.

## Measurement-led changes

1. Initial profiling isolated transform, exact lookup, full-bank candidate
   ranking, geometry, tap interpolation/widening and update validation/construction.
   D1 9.1.6 rapid p50: ranking 1.567 ms, exact 0.660 ms, geometry 0.618 ms,
   taps 0.035 ms; transform/update were below 0.001 ms each.
2. Build a bank-bound immutable direction tree during preparer construction.
   Its conservative bounding boxes prune only candidates whose maximum dot
   product cannot beat the retained heap. Dot-descending/index-ascending ordering,
   acos and all 128 final candidates remain identical. Additional resident index
   capacity is **160,648 bytes per bank** (about 157 KiB); preparer clones share it.
   Exact queries retain the original tolerance and lowest matching record index.
3. Ranking-only optimization was insufficient: D2 9.1.6 smooth profile still
   had geometry p95 4.058 ms and total p95 5.037 ms. The implementation therefore
   also reuses already-computed candidate angles for segments and first/third
   triangle terms across second vertices. The latter uses a bounded 3 KiB stack
   array for large tiers, not an allocated matrix. Negative/nonfinite weights
   reject early using the same tests and tolerances. Arithmetic producing an
   accepted weight is unchanged; triangle/segment and expansion order stay intact.
4. No tap truncation, precision downgrade, coverage fallback, render-path work,
   mutable shared query cache or per-sample allocation was introduced. The
   240-sample transition implementation is untouched.

## Before / after results

Wall times are **p50 / p95 / worst in milliseconds**. CPU is mean milliseconds
per call. Allocation columns are means per call and are **identical before and
after for every row**. All error counts are zero. D1/D2 abbreviate the resources above.

| HRTF | Layout | Trajectory | Before wall ms | After wall ms | CPU before → after ms | Allocations | Requested bytes |
|---|---|---|---|---|---|---:|---:|
| D1 | 2.0 | identity | 0.046 / 0.064 / 0.826 | 0.003 / 0.003 / 0.302 | 0.053 → 0.004 | 9.00 | 8410.00 |
| D1 | 2.0 | rapid | 0.360 / 0.883 / 2.744 | 0.079 / 0.519 / 1.994 | 0.455 → 0.157 | 20.50 | 25514.09 |
| D1 | 2.0 | nearby | 0.262 / 0.409 / 3.309 | 0.043 / 0.056 / 0.257 | 0.292 → 0.045 | 21.00 | 25656.00 |
| D1 | 2.0 | smooth | 0.350 / 1.995 / 2.487 | 0.101 / 1.348 / 1.752 | 0.546 → 0.247 | 21.71 | 25548.37 |
| D1 | 5.1 | identity | 0.093 / 0.113 / 0.462 | 0.005 / 0.005 / 0.027 | 0.097 → 0.005 | 21.00 | 21025.00 |
| D1 | 5.1 | rapid | 0.884 / 1.664 / 3.704 | 0.199 / 0.726 / 2.400 | 1.020 → 0.298 | 49.01 | 63641.90 |
| D1 | 5.1 | nearby | 0.736 / 0.824 / 10.533 | 0.110 / 0.124 / 0.270 | 0.757 → 0.112 | 51.00 | 64140.00 |
| D1 | 5.1 | smooth | 1.169 / 2.640 / 12.633 | 0.417 / 1.557 / 1.849 | 1.248 → 0.501 | 53.53 | 63888.16 |
| D1 | 7.1.4 | identity | 0.324 / 0.367 / 0.695 | 0.019 / 0.019 / 0.074 | 0.335 → 0.020 | 45.00 | 46255.00 |
| D1 | 7.1.4 | rapid | 2.045 / 3.201 / 6.639 | 0.519 / 1.429 / 4.024 | 2.205 → 0.663 | 106.87 | 140060.67 |
| D1 | 7.1.4 | nearby | 1.790 / 2.008 / 7.843 | 0.233 / 0.259 / 0.408 | 1.842 → 0.239 | 108.00 | 140526.00 |
| D1 | 7.1.4 | smooth | 2.328 / 5.283 / 9.537 | 0.738 / 2.951 / 3.233 | 2.572 → 0.864 | 110.92 | 140103.58 |
| D1 | 9.1.6 | identity | 0.474 / 0.571 / 2.242 | 0.027 / 0.027 / 0.066 | 0.487 → 0.027 | 61.00 | 63075.00 |
| D1 | 9.1.6 | rapid | 2.902 / 4.278 / 7.284 | 0.820 / 1.887 / 4.149 | 3.078 → 0.961 | 146.01 | 191116.91 |
| D1 | 9.1.6 | nearby | 2.407 / 2.673 / 5.128 | 0.312 / 0.359 / 0.790 | 2.455 → 0.324 | 146.00 | 191450.00 |
| D1 | 9.1.6 | smooth | 2.960 / 6.356 / 9.595 | 0.917 / 3.500 / 5.343 | 3.232 → 1.123 | 149.56 | 190904.41 |
| D2 | 2.0 | identity | 0.046 / 0.071 / 0.660 | 0.003 / 0.003 / 0.020 | 0.051 → 0.003 | 9.00 | 8410.00 |
| D2 | 2.0 | rapid | 0.364 / 0.866 / 2.212 | 0.075 / 0.460 / 1.510 | 0.448 → 0.145 | 20.50 | 25514.09 |
| D2 | 2.0 | nearby | 0.261 / 0.345 / 1.353 | 0.043 / 0.055 / 0.292 | 0.280 → 0.045 | 21.00 | 25656.00 |
| D2 | 2.0 | smooth | 0.349 / 1.991 / 2.789 | 0.100 / 1.341 / 3.061 | 0.544 → 0.247 | 21.71 | 25548.37 |
| D2 | 5.1 | identity | 0.093 / 0.112 / 1.162 | 0.005 / 0.005 / 0.029 | 0.098 → 0.005 | 21.00 | 21025.00 |
| D2 | 5.1 | rapid | 0.896 / 1.820 / 5.887 | 0.200 / 0.717 / 2.225 | 1.043 → 0.296 | 49.01 | 63641.90 |
| D2 | 5.1 | nearby | 0.738 / 0.873 / 3.502 | 0.111 / 0.126 / 0.272 | 0.759 → 0.114 | 51.00 | 64140.00 |
| D2 | 5.1 | smooth | 1.264 / 2.674 / 3.911 | 0.417 / 1.569 / 3.841 | 1.280 → 0.509 | 53.53 | 63888.16 |
| D2 | 7.1.4 | identity | 0.323 / 0.373 / 1.659 | 0.019 / 0.019 / 0.086 | 0.336 → 0.020 | 45.00 | 46255.00 |
| D2 | 7.1.4 | rapid | 2.057 / 3.222 / 9.916 | 0.529 / 1.429 / 4.082 | 2.222 → 0.670 | 106.87 | 140060.67 |
| D2 | 7.1.4 | nearby | 1.783 / 1.914 / 4.849 | 0.234 / 0.266 / 1.383 | 1.805 → 0.243 | 108.00 | 140526.00 |
| D2 | 7.1.4 | smooth | 2.309 / 5.249 / 11.185 | 0.738 / 2.966 / 5.162 | 2.478 → 0.876 | 110.92 | 140103.58 |
| D2 | 9.1.6 | identity | 0.474 / 0.522 / 1.745 | 0.027 / 0.027 / 0.179 | 0.479 → 0.028 | 61.00 | 63075.00 |
| D2 | 9.1.6 | rapid | 2.985 / 4.433 / 7.582 | 0.832 / 1.904 / 4.323 | 3.155 → 0.973 | 146.01 | 191116.91 |
| D2 | 9.1.6 | nearby | 2.433 / 2.727 / 11.998 | 0.312 / 0.364 / 0.667 | 2.498 → 0.324 | 146.00 | 191450.00 |
| D2 | 9.1.6 | smooth | 2.976 / 6.351 / 11.712 | 0.911 / 3.478 / 3.892 | 3.247 → 1.117 | 149.56 | 190904.41 |

The original smooth trajectory improves from **5.249–6.356 ms p95** to
**2.951–3.500 ms p95** for D1/D2, 7.1.4/9.1.6. Thus the **4 ms preparation p95
criterion passes on this host and corpus**. Worst samples still exceed 4 ms in
some cases. Device sensor-to-sound latency, real-time scheduling, low-power
hardware, sustained playback and listening remain unvalidated. This does not
promote the feature to general low-latency acceptance.

## Reproduction and raw data

```sh
cargo +1.89.0 run --locked --release -p openjoc-api --example orientation_profile -- 2000
cargo +1.89.0 run --locked --release -p openjoc-api --features orientation-profile --example orientation_profile -- 1000
cargo +1.89.0 test --release -p openjoc-sofa -p openjoc-api -p openjoc-render
```

To reproduce the baseline, check out the baseline commit in a separate worktree,
copy only `crates/openjoc-api/examples/orientation_profile.rs` from this revision,
and run the first command there. Baseline compilation may warn about the unused
`orientation-profile` cfg, which is absent on that revision; do not enable it.
Run versions sequentially without concurrent compilation or other benchmarks.
An optional final argument selects `identity`, `rapid`, `nearby` or `smooth`.

- [Before](before.csv) / [after](after.csv): 2,000-call uninstrumented measurements.
- [Initial stage profile](before-stages.csv): 1,000 calls, identity/rapid/nearby.
- [Index-only smooth profile](index-only-smooth-stages.csv): 240 calls, showing
  why stopping after ranking optimization would miss the original target.
- [Final stage profile](after-stages.csv): 1,000 calls for all four trajectories.

Stage rows aggregate that stage across all sources in a pose. Stage quantiles
cannot be added to reconstruct the total quantile. Allocation and CPU columns
on stage rows describe the **whole prepare call**, not per-stage allocation/CPU.
The exact stage includes direction normalization; geometry includes projection
and segment/triangle search. The tap stage includes complete kernel validation
and widening/interpolation. Timing instrumentation is opt-in and absent from
normal builds. Custom SOFA stage timing and device/WASM performance are outside
this built-in-resource benchmark.

## Regression coverage

- Indexed versus linear ranking: candidate IDs, dot bits and angle bits;
  complete resolved HRIRs, metadata and tap bits, plus invalid-input errors.
- Both banks, axial/exact/alias directions, 720 deterministic sphere directions,
  rapid changes and 40 repeated/adjacent directions.
- Expanded comparison with the independent full-rescan geometry reference:
  coverage errors, selected neighbors, interpolation weights and complete taps.
- Existing static identity equivalence and dynamic transition tests remain required.
