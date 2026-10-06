# Bit-exact static direct-FIR local accumulators

This experiment changes only output accumulator storage in `BinauralRenderer::render_block`: initialize local left/right f64 values from the existing mixed output, perform the same ascending-tap left/right multiply/add recurrence, and store before the same finite checks. Input/history selection, coefficients, gains, source order, arithmetic precision, backend, history, tail and error/reset behavior are unchanged.

## Baselines and result

**Performance before:** merged-QMF `1647fc4640c5fe58fce1b5087034ff2208c0391f`. **Sound oracle:** frozen `15aefe1baa7b40f37950df252b6dbf6179894d6d`. They have different purposes; timings from the older sound oracle are not used to attribute this optimization's benefit.

Rust 1.98.1 / LLVM 22.1.8, x86_64 Linux, default release/default API features, no custom Rust flags, virtual AMD EPYC 9V74. Five alternating pairs per case used the existing timing-only API probe; init/warmup and capture/hash/file I/O are outside the measured window. These are synthetic Linux API measurements, not Windows/LAV/PotPlayer, physical endpoint or i5-8250U results.

| D1/D2 virtual 7.1.4 | Before median CPU seconds | Candidate median | All paired CPU reductions |
|---|---:|---:|---:|
| D1, 30.016s | 16.171 | 6.899 | 53.82–59.77% |
| D1, 60s | 32.662 | 13.769 | 56.91–58.65% |
| D2, 30.016s | 16.360 | 7.123 | 55.55–59.53% |
| D2, 60s | 32.316 | 13.660 | 56.90–58.84% |

All 20 primary pairs improved. Same-binary D1/D2 controls first established paired noise ranges of −5.00%…+1.43% and −1.08%…+3.34%. Stereo and speaker control fluctuations remain within their independently measured noise envelope; no benefit is claimed for them. Ordinary-core probe binaries are identical.

The final public-renderer micro uses 11 synthetic sources, 256 taps each, 1,536 samples and 128 calls per leg. Five alternating pairs gave median **14.965→4.034 ms/call**, with all pairs 72.16–75.08% lower and non-overlapping ranges. Every run compares 3,582 captured f64 values (block plus full tail) and 3,072 post-timing values through `to_bits()`. These synthetic coefficients are not described as builtin HRTFs.

## Attribution and exactness

Three merged-baseline gprofng 2.44 profiles put 847/973 samples (87.05%) inside the static tap loop, or 847/852 of `render_block`'s exclusive samples. Per-tap accumulation/store PCs dominated; source selection accounted for only 18 samples. The candidate keeps accumulators in registers. Emitted mulpd/addpd combines independent ears, with no horizontal tap reduction or FMA.

Sampling requested 100ms and achieved 99.79–99.99% sampled/actual process-CPU coverage for decoder runs. The collector still warned `Collection interval timer period was changed (100000 -> 0); profile data may be unreliable`. These are broad sample-based hotspot shares, not precise instruction costs. Inclusive shares overlap. Profiling was separate from plain timing.

Validation:
- Rust 1.98.1 frozen gate: 18 cases, including full 30/60s D1/D2/Stereo/speaker PCM, ordinary EAC3 f32/f64, and short 9.1.6/22.2/orientation/pull extensions
- Rust 1.89.0 frozen gate: 14 cases, plus 82 renderer tests
- Custom SOFA: the same included asymmetric 48k HDF5 fixture in both builds, virtual 5.1/EqualPowerDualMono; all 11,526,152 PCM bytes and complete AU/frame/PTS/rate/channel/drain metadata match frozen 15aefe1. A one-bit corruption is rejected
- Exact original-loop tests cover f64 outputs and private history/tail/lifecycle state, unequal 255/256/257/2/1-tap filters, non-dyadic/cancellation inputs, signed-zero/non-unit gains, caller block permutation, empty/arbitrary chunks, reset, and overflow/error precedence
- Full Rust 1.98.1 workspace: 1,105 passed, 0 failed, 16 pre-existing/manual ignores; Python: 151 passed, 56 expected skips

Extended layouts/custom SOFA/orientation are API regressions, not claims of LAV downstream layout support. The earlier QMF Windows package pair is not evidence for this FIR change.

## Raw data and provenance

`end-to-end.csv`, `controls.csv` and `same-binary-noise.csv` retain raw paired CPU/wall/RTF observations, binary hashes, workload descriptors and config fingerprints. `public-micro.csv` has all final local pairs. `provenance.json` holds compiler/config/source/binary identities and full config descriptors; `profile-summary.json` holds profile counts/coverage and hot PCs. Summarize the raw data with the existing helper:

```sh
python3 scripts/performance_profile_repro.py docs/research/direct-fir-local-accumulators/end-to-end.csv --metric summary_probe_process_cpu_ns --metric summary_api_pipeline_wall_ns
```

An initial public micro attempt accidentally built candidate source for both labels and is excluded. A later cached/incomplete retry was not accepted; one intermediate retry log was overwritten and is not represented as retained evidence. Accepted runs use explicit canonical cwd/manifest paths, new empty targets, source/example/binary hashes, complete compiler-root logs and assembly checks. Six guard tests in local validation rejected wrong-root, Fresh-only, incomplete, wrong-target and mutated command paths. Full build logs/guard fixtures/disassembly are local validation records; the compact committed dataset contains the raw timings, profile counts and source/binary identities rather than those logs. An isolated kernel analogue is exploratory only and is not used for the production performance claim. Initial short noise controls were repeated after an overlapping disassembly command; only the clean repeats are in the committed noise CSV.

## Reproduce on Linux

Run from a checkout of this PR. Install Rust 1.98.1 and 1.89.0 plus FFmpeg/ffprobe. Use **new empty target directories** and retain verbose compiler logs. A different target alone does not establish source identity. Both sides must use the same compiler/features/flags; do not add native CPU or fast-math flags.

```sh
set -euo pipefail
candidate="$PWD"
run="$(mktemp -d)"
git worktree add --detach "$run/perf-before" 1647fc4640c5fe58fce1b5087034ff2208c0391f
git worktree add --detach "$run/sound-before" 15aefe1baa7b40f37950df252b6dbf6179894d6d
mkdir -p "$run/perf-before/crates/openjoc-render/examples" "$run/sound-before/crates/openjoc-api/examples"
cp crates/openjoc-render/examples/direct_fir_public_probe.rs "$run/perf-before/crates/openjoc-render/examples/"
cp crates/openjoc-api/examples/pcm_regression_probe.rs "$run/sound-before/crates/openjoc-api/examples/"
cp crates/openjoc-api/Cargo.toml "$run/sound-before/crates/openjoc-api/Cargo.toml"
export CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0
rustc +1.98.1 -Vv
cargo +1.98.1 fetch --locked
python3 scripts/generate_performance_corpus.py --repo "$candidate" --target "$run/corpus-target" --output-dir "$run/corpus" --durations 30 60
# Keep the published input hashes. A hash mismatch is a different workload, not permission to repin it.
for side in before candidate; do
  repo="$candidate"; test "$side" != before || repo="$run/perf-before"
  (cd "$repo" && CARGO_TARGET_DIR="$run/micro-$side" cargo +1.98.1 build --manifest-path "$repo/Cargo.toml" --release --locked --offline --verbose -p openjoc-render --example direct_fir_public_probe) > "$run/build-$side.log" 2>&1
done
# Inspect each log's source root and hash both binaries before measuring.
"$run/micro-before/release/examples/direct_fir_public_probe" capture "$run/local-oracle.bin"
for pair in 1 2 3 4 5; do
  order="before candidate"; test $((pair % 2)) != 0 || order="candidate before"
  for side in $order; do
    label=baseline; test "$side" != candidate || label=candidate
    "$run/micro-$side/release/examples/direct_fir_public_probe" compare "$label" "$run/local-oracle.bin" > "$run/micro-$pair-$side.csv"
  done
done
# Unchanged frozen sound gate; do not substitute perf-before here.
python3 scripts/verify_pcm_bitexact.py --toolchain 1.98.1 --baseline-root "$run/sound-before" --candidate-root "$candidate" --baseline-target "$run/sound-target" --candidate-target "$run/candidate-target" --output-dir "$run/sound-gate" --selftest-integrated \
  --case "d1-30,$run/corpus/joc.lifecycle.30s.ec3,binaural-d1,7.1.4" \
  --case "d1-60,$run/corpus/joc.lifecycle.60s.ec3,binaural-d1,7.1.4" \
  --case "d2-30,$run/corpus/joc.lifecycle.30s.ec3,binaural-d2,7.1.4" \
  --case "d2-60,$run/corpus/joc.lifecycle.60s.ec3,binaural-d2,7.1.4"
# Build the distinct performance-before API executable.
(cd "$run/perf-before" && CARGO_TARGET_DIR="$run/perf-target" cargo +1.98.1 build --manifest-path "$run/perf-before/Cargo.toml" --release --locked --offline --verbose -p openjoc-api --example pcm_regression_probe) > "$run/build-perf-api.log" 2>&1
for mode in binaural-d1 binaural-d2; do for duration in 30 60; do
  for pair in 1 2 3 4 5; do
    order="before candidate"; test $((pair % 2)) != 0 || order="candidate before"
    for side in $order; do
      bin="$run/perf-target/release/examples/pcm_regression_probe"; test "$side" != candidate || bin="$run/candidate-target/release/examples/pcm_regression_probe"
      "$bin" "$run/corpus/joc.lifecycle.${duration}s.ec3" "$mode" 7.1.4 "$run/$mode-$duration-$pair-$side" --timing-only
    done
  done
done; done
```

Each `.timing.tsv` contains complete raw timing/config/shape fields; compare the same input hash, config descriptor, latency, frames, samples, channels and tail on every pair. Repeat Stereo/speaker controls the same way. Full PCM capture and profiling must remain separate from timing-only runs. For the MSRV repeat, select `--toolchain 1.89.0` and new target directories.

### Custom-SOFA exact extension

Use a **separate** frozen worktree so the ordinary frozen gate's untracked-file allowlist remains intact. This capture is not a performance comparison.

```sh
git worktree add --detach "$run/custom-before" 15aefe1baa7b40f37950df252b6dbf6179894d6d
mkdir -p "$run/custom-before/crates/openjoc-api/examples"
cp crates/openjoc-api/examples/custom_sofa_bitexact_probe.rs "$run/custom-before/crates/openjoc-api/examples/"
cp crates/openjoc-api/Cargo.toml "$run/custom-before/crates/openjoc-api/Cargo.toml"
for side in before candidate; do
  repo="$candidate"; test "$side" != before || repo="$run/custom-before"
  (cd "$repo" && CARGO_TARGET_DIR="$run/custom-$side-target" cargo +1.98.1 build --manifest-path "$repo/Cargo.toml" --release --locked --offline --verbose -p openjoc-api --example custom_sofa_bitexact_probe) > "$run/custom-build-$side.log" 2>&1
  "$run/custom-$side-target/release/examples/custom_sofa_bitexact_probe" capture "$run/corpus/joc.lifecycle.30s.ec3" "$candidate/docs/research/direct-fir-local-accumulators/asymmetric-custom-sofa.h5" "$run/custom-$side"
done
python3 - "$run" <<'PY'
import sys
from pathlib import Path
sys.path.insert(0, "scripts")
from verify_pcm_bitexact import compare_runs
p = Path(sys.argv[1])
compare_runs(p / "custom-before.manifest.tsv", p / "custom-before.pcm32le", p / "custom-candidate.manifest.tsv", p / "custom-candidate.pcm32le")
print("custom-SOFA full PCM and metadata match")
PY
```

### Windows native repeat

Use the same revisions/harness overlays and **fresh** directories, Rust 1.98.1 MSVC x64, and explicit working directories/manifests. Build/run each side in PowerShell as below (substitute the verified checkout paths). Never treat the other side's cached target as a before build.

```powershell
$before = (Resolve-Path C:\src\OpenJOC-perf-before).Path
$candidate = (Resolve-Path C:\src\OpenJOC-candidate).Path
$run = Join-Path $env:TEMP ("OpenJOC-FIR-" + [guid]::NewGuid())
New-Item -ItemType Directory $run | Out-Null
$env:CARGO_BUILD_JOBS = "1"; $env:CARGO_INCREMENTAL = "0"
rustc +1.98.1 -Vv
foreach ($side in @("before", "candidate")) {
  $repo = if ($side -eq "before") { $before } else { $candidate }
  $env:CARGO_TARGET_DIR = Join-Path $run $side
  Push-Location $repo
  cargo +1.98.1 build --manifest-path "$repo\Cargo.toml" --release --locked --verbose -p openjoc-render --example direct_fir_public_probe
  if ($LASTEXITCODE -ne 0) { throw "Build failed: $side" }
  Pop-Location
}
$beforeExe = "$run\before\release\examples\direct_fir_public_probe.exe"
$candidateExe = "$run\candidate\release\examples\direct_fir_public_probe.exe"
Get-FileHash $beforeExe, $candidateExe -Algorithm SHA256
& $beforeExe capture "$run\oracle.bin"
1..5 | ForEach-Object {
  if ($_ % 2) { & $beforeExe compare baseline "$run\oracle.bin"; & $candidateExe compare candidate "$run\oracle.bin" }
  else { & $candidateExe compare candidate "$run\oracle.bin"; & $beforeExe compare baseline "$run\oracle.bin" }
}
```

Run the same Python frozen-PCM gate on Windows with platform-local paths and new targets; never compare Linux bytes to Windows bytes as a substitute for same-platform pairing. The existing probe's Linux process-CPU clock is unavailable on Windows, so use recorded pipeline wall/RTF there. This document supplies a reproducible route, not a claim that native Windows or real devices have already been measured for this candidate.
