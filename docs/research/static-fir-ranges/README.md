# Static direct-FIR tap-range split

This change removes the per-tap choice between current-block input and stored history. For each output offset, it visits current-input taps first and history taps second, both in the original ascending order. It preserves source order, local f64 left/right accumulators, current input × gain followed by input × tap, finite checks, history updates and drain behavior. No FMA, horizontal tap reduction, precision/HRTF/gain change, FFT or alternate backend is introduced.

## Public synthetic measurements

Performance baseline: master `30de70366bfb6824ba420a2e160309ea27569726`. The reused baseline API executable was built from `5d88140045a65dc1940b923b83d399a984bce482`, whose complete Git tree is identical (`dc9c06dd6351955326f219135e140d9ae49c9ec3`). Measured candidate source is now production/test commit `c609785ed17b5cd9fa0cecf5e5376ed04cf32c61`, on that master. Its committed renderer source hash exactly matches the measured patch identified in `provenance.json`. The acoustic oracle remains **`15aefe1baa7b40f37950df252b6dbf6179894d6d`**; its timing is not used to attribute this change's benefit.

Rust 1.98.1 / LLVM 22.1.8, default release/default API features, x86_64 Linux on a virtual AMD EPYC 9V74. No custom RUSTFLAGS, native-CPU flags or fast-math. Five alternating pairs per primary case (AB, BA, AB, BA, AB), using the existing `pcm_regression_probe --timing-only`. Benchmarks run serially, without concurrent local builds/tests. Shared-host frequency/load is uncontrolled. These are synthetic API processing measurements, not Windows/LAV/PotPlayer, physical-device latency or low-power CPU results.

| Workload | Before median API CPU | Candidate median API CPU | Every paired CPU reduction |
|---|---:|---:|---:|
| D1, 30.016 s | 7.568 s | 5.920 s | 18.24–24.54% |
| D1, 60 s | 14.595 s | 11.707 s | 16.97–27.52% |
| D2, 30.016 s | 7.211 s | 5.725 s | 16.02–23.82% |
| D2, 60 s | 14.188 s | 11.667 s | 16.34–20.29% |

All 20 primary pairs improved. The separately retained early D1 30.016 s diagnostic has two pairs improving 18.08% and 21.07%; it is not pooled into the primary dataset. All timing runs check equal input/config/output-shape descriptors and stable executable identities. Timing-only is not a PCM equality gate.

API timing excludes initialization, eight-AU warmup and output audit/hash/report serialization. Timed calls consume and drop the real output frames. `summary_probe_process_cpu_ns` measures CPU across the processing loop; `summary_api_pipeline_wall_ns` sums timed push/receive/drain calls; `external_process_wall_ns` includes all process overhead. These metrics are distinct and retained separately in CSV. Per-call percentiles describe AU service cost, not device playback latency.

### Micro and control caveats

The unchanged public-renderer micro uses 11 synthetic sources × 256 taps, 1,536 samples, eight warmups and 128 timed calls. Every leg compares 3,582 captured block-plus-tail f64 values and 3,072 post-timing values bitwise. Five pairs give median **4.185 → 3.798 ms/call (9.23% lower)**. Individual reductions are **−5.80%, +14.85%, +10.64%, +8.81%, +11.44%**. The first pair regressed and remains included; neither universal per-pair improvement nor nonoverlapping ranges is claimed.

Three-pair controls: stereo median CPU 1.806 → 1.813 s (+0.39%); speaker 7.1.4 2.145 → 2.161 s (+0.73%). Stereo paired changes range from 1.16% faster to 5.45% slower; speaker is 0.73–5.15% slower in all three pairs. These noisy controls do not support a control-path gain and are insufficient to prove zero regression. No control-specific code changed, but compiler/layout or shared-host effects cannot be distinguished here.

## Arithmetic and exactness

Source review and generated `render_block` assembly preserve one accumulator with independent left/right SIMD lanes: current range uses scalar input/gain multiplication then ear-pair multiplication/addition, followed by history range multiplication/addition into the same accumulator. Taps stay ascending. No FMA or horizontal regrouping appears. Per-tap current/history selection disappears; bounds checks remain. This does not attribute all observed speedup to branch cost alone.

The renderer tests include boundary tap counts 1/2/3/15/16/17 and populated-history transitions, alongside existing unequal long filters, signed zero, non-unit gains, source order, arbitrary chunks, tails/reset and error precedence checks. The public micro's complete bit checks passed on all ten legs. A separately supplied local input also passed full PCM byte and complete-descriptor comparison against current and frozen references; its artifacts are intentionally not included and are not a public reproducibility claim. Broader frozen-oracle and workspace validation should be reviewed with the accompanying change's validation record; this benchmark report alone does not assert those stages passed.

## Raw data and identity

- `end-to-end.csv`: all 20 primary public synthetic pairs
- `early-diagnostic.csv`: the two early diagnostic pairs, separate from primary results
- `controls.csv`: all six stereo/speaker control pairs, including slowdowns
- `public-micro.csv`: all ten micro legs, including the regressing first pair
- `provenance.json`: exact source/patch/probe/executable hashes, machine/toolchain/config and data counts

The measured renderer source SHA-256 is `5270d8e4cae5611893e88a9d9f4f9839179f13e087d8076c9314a866ebddee44`; the patch on master is `872601faf3acb7f00695542e4267db4d8ea958c170a0a9843ce88f2f1ab54aeb`. Candidate API executable SHA-256 is `22db6c7c0477467dfdfae47ef670533bedb0488b4f4087bc8a072d56b6032462`. The production/test commit `c609785ed17b5cd9fa0cecf5e5376ed04cf32c61` has that exact renderer source hash; documentation is separate. This maps the measured executable to source without requiring a documentation-only rebuild. Retained verbose build logs verify actual canonical compiler source roots, rather than relying only on target directory names. Both micro binaries were built through workspace manifests; no standalone alternative manifest was used.

Summarize the supplied CSV using the unchanged helper:

```sh
python3 scripts/performance_profile_repro.py docs/research/static-fir-ranges/end-to-end.csv \
  --metric summary_probe_process_cpu_ns --metric summary_api_pipeline_wall_ns
```

## Reproduce

Use this change's checkout as candidate, a fresh baseline worktree and new empty targets. Verify the candidate source hash above before building. Different absolute build paths can produce different executable hashes; retain actual hashes and canonical source/compiler logs, not only labels. Use the same toolchain, profiles, features and flags for both sides.

```sh
set -euo pipefail
candidate="$PWD"
run="$(mktemp -d)"
git worktree add --detach "$run/before" 30de70366bfb6824ba420a2e160309ea27569726
export CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0
unset RUSTFLAGS CARGO_ENCODED_RUSTFLAGS
rustc +1.98.1 -Vv
cargo +1.98.1 -Vv
cargo +1.98.1 fetch --locked
python3 scripts/generate_performance_corpus.py --repo "$candidate" \
  --target "$run/corpus-target" --output-dir "$run/corpus" --durations 30 60
# Verify generated corpus hashes against scripts/verify_pcm_bitexact.py; do not repin.
for side in before candidate; do
  repo="$candidate"; test "$side" != before || repo="$run/before"
  (cd "$repo" && CARGO_TARGET_DIR="$run/$side-target" cargo +1.98.1 build \
    --manifest-path "$repo/Cargo.toml" --release --locked --offline --verbose \
    -p openjoc-api --example pcm_regression_probe \
    -p openjoc-render --example direct_fir_public_probe) > "$run/build-$side.log" 2>&1
  sha256sum "$run/$side-target/release/examples/pcm_regression_probe" \
    "$run/$side-target/release/examples/direct_fir_public_probe"
done
before="$run/before-target/release/examples"
after="$run/candidate-target/release/examples"
"$before/direct_fir_public_probe" capture "$run/micro-oracle.bin"
for pair in 1 2 3 4 5; do
  order="before candidate"; test $((pair % 2)) != 0 || order="candidate before"
  for side in $order; do
    bin="$before"; label=baseline
    test "$side" != candidate || { bin="$after"; label=candidate; }
    "$bin/direct_fir_public_probe" compare "$label" "$run/micro-oracle.bin" \
      > "$run/micro-$pair-$side.csv"
  done
done
for mode in binaural-d1 binaural-d2; do
  for duration in 30 60; do
    for pair in 1 2 3 4 5; do
      order="before candidate"; test $((pair % 2)) != 0 || order="candidate before"
      for side in $order; do
        bin="$before"; test "$side" != candidate || bin="$after"
        "$bin/pcm_regression_probe" "$run/corpus/joc.lifecycle.${duration}s.ec3" \
          "$mode" 7.1.4 "$run/$mode-$duration-$pair-$side" --timing-only
      done
    done
  done
done
# Repeat the same alternating loop for stereo/2.0 and speaker/7.1.4,
# using the 30s corpus and three pairs to match the retained controls.
```

Each timing TSV retains raw AU costs plus complete summary/config/shape fields. Compare equal input hash, descriptor, latency, frame/sample/channel/tail counts before interpreting timing. Keep profiling, capture and byte comparisons outside timing runs. The existing `verify_pcm_bitexact.py` gate can build/check the frozen oracle without replacing it:

```sh
git worktree add --detach "$run/sound-before" 15aefe1baa7b40f37950df252b6dbf6179894d6d
mkdir -p "$run/sound-before/crates/openjoc-api/examples"
cp crates/openjoc-api/examples/pcm_regression_probe.rs "$run/sound-before/crates/openjoc-api/examples/"
cp crates/openjoc-api/Cargo.toml "$run/sound-before/crates/openjoc-api/Cargo.toml"
python3 scripts/verify_pcm_bitexact.py --toolchain 1.98.1 \
  --baseline-root "$run/sound-before" --candidate-root "$candidate" \
  --baseline-target "$run/sound-target" --candidate-target "$run/gate-candidate-target" \
  --output-dir "$run/sound-gate" --selftest-integrated \
  --case "d1-30,$run/corpus/joc.lifecycle.30s.ec3,binaural-d1,7.1.4" \
  --case "d1-60,$run/corpus/joc.lifecycle.60s.ec3,binaural-d1,7.1.4" \
  --case "d2-30,$run/corpus/joc.lifecycle.30s.ec3,binaural-d2,7.1.4" \
  --case "d2-60,$run/corpus/joc.lifecycle.60s.ec3,binaural-d2,7.1.4"
```

Full build logs/assembly and private additional-input validation remain local. Only public synthetic measurements are in this directory.
