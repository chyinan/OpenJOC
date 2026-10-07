# Reproduce the ordered AVX FIR experiment

Use Linux with Rust 1.98.1 and 1.89.0 installed. The public corpus generator also
needs FFmpeg/ffprobe. Commands below normalize local paths and select the same
compiler explicitly. Run performance legs alone, without builds, tests or other
benchmarks. Retain every leg. Do not add native CPU or fast-math flags.

The final private campaign ran pair 1, completed correctness/quality checks, then
ran pairs 2–7. Source and binary identities remained unchanged. A continuous
seven-pair reproduction uses the same alternating order and workload.

## Worktrees, provenance and identical builds

Run from the candidate checkout containing production commit
`97a25a00151c2df6ae9e17ef9ecf8e140af3a0bb`:

```bash
set -euo pipefail
candidate="$PWD"
run="$(mktemp -d)"
export candidate run CARGO_BUILD_JOBS=1
unset RUSTFLAGS CARGO_ENCODED_RUSTFLAGS
before=291900ce33ea349c4b855655cdb282819e94eabc
oracle=15aefe1baa7b40f37950df252b6dbf6179894d6d
git worktree add --detach "$run/before" "$before"
git worktree add --detach "$run/oracle" "$oracle"
rustc +1.98.1 -Vv > "$run/compiler.txt"
cargo +1.98.1 -Vv >> "$run/compiler.txt"
cargo +1.98.1 fetch --locked --manifest-path "$candidate/Cargo.toml"
for side in before candidate; do
  repo="$candidate"; test "$side" != before || repo="$run/before"
  CARGO_TARGET_DIR="$run/target-$side" cargo +1.98.1 build \
    --release --locked --offline --manifest-path "$repo/Cargo.toml" \
    -p openjoc-api --example pcm_regression_probe \
    -p openjoc-render --example direct_fir_public_probe \
    > "$run/build-$side.log" 2>&1
done
sha256sum "$run"/target-{before,candidate}/release/examples/{pcm_regression_probe,direct_fir_public_probe} \
  > "$run/binaries-before.txt"
```

Both builds use API features `default,embedded-builtin-hrtf`. Compare the
`example-pcm_regression_probe.json` Cargo fingerprint records: features, target,
profile, rustflags, config and compile kind must match. Do not publish input or
configuration fingerprints from private probe output.

## Exact f64 public micro and nine alternating pairs

```bash
"$run/target-before/release/examples/direct_fir_public_probe" capture "$run/micro.bin"
for pair in {1..9}; do
  order="before candidate"; ((pair % 2)) || order="candidate before"
  for side in $order; do
    label=baseline; test "$side" != candidate || label=candidate
    "$run/target-$side/release/examples/direct_fir_public_probe" \
      compare "$label" "$run/micro.bin" > "$run/micro-$pair-$side.csv"
  done
done
```

This unchanged probe uses eight warmups and 128 measured calls, and checks f64
block/tail bits outside the timed window.

## Short controls

The final shape list is `(1,1), (1,4), (3,8), (8,16), (16,64), (256,128),
(256,259), (256,260), (256,512)` for `(taps, block samples)`. Use the same public
probe with only its shape/iteration constants and capture-label text changed.
Final iteration count is
`max(128, min(1048576, 30000000 // (11 * taps * block)))`.
Earlier short-batch exploration used a 65536 cap and remains in the CSV.

```bash
python3 - <<'PY'
import csv, os, pathlib, subprocess
candidate=pathlib.Path(os.environ['candidate']); run=pathlib.Path(os.environ['run'])
out=run/'short'; out.mkdir()
source=(candidate/'crates/openjoc-render/examples/direct_fir_public_probe.rs').read_text()
shapes=[(1,1),(1,4),(3,8),(8,16),(16,64),(256,128),(256,259),(256,260),(256,512)]
for taps,block in shapes:
    name=f't{taps}-b{block}'
    iterations=max(128,min(1048576,30000000//(11*taps*block)))
    text=source.replace('const TAPS_PER_SOURCE: usize = 256;',f'const TAPS_PER_SOURCE: usize = {taps};')
    text=text.replace('const BLOCK_LEN: usize = 1536;',f'const BLOCK_LEN: usize = {block};')
    text=text.replace('const ITERATIONS: usize = 128;',f'const ITERATIONS: usize = {iterations};')
    text=text.replace('|'.join(['256']*11),'|'.join([str(taps)]*11))
    src=out/(name+'.rs'); src.write_text(text)
    for side in ['before','candidate']:
        deps=run/f'target-{side}/release/deps'
        libs=list(deps.glob('libopenjoc_render-*.rlib')); assert len(libs)==1
        subprocess.run(['rustc','+1.98.1','--edition=2024','-C','opt-level=3',
            '--crate-name','short_control',str(src),'--extern','openjoc_render='+str(libs[0]),
            '-L','dependency='+str(deps),'-o',str(out/f'{name}-{side}')],check=True)
# All compilation finishes before timing starts.
for taps,block in shapes:
    name=f't{taps}-b{block}'; capture=out/(name+'.bin')
    subprocess.run([str(out/f'{name}-before'),'capture',str(capture)],check=True)
    for pair in range(1,10):
        order=['before','candidate'] if pair%2 else ['candidate','before']
        for side in order:
            label='baseline' if side=='before' else 'candidate'
            with (out/f'{name}-{pair}-{side}.csv').open('w') as stream:
                subprocess.run([str(out/f'{name}-{side}'),'compare',label,str(capture)],
                    stdout=stream,check=True)
PY
```

## Full-recording timing-only pairs

Set `AUTHORIZED_INPUT` to a recording you are allowed to process. The recording
used for this report is not distributed. Public generated corpus can substitute
for exercising the procedure, but is a different workload and cannot reproduce
these private-recording numbers.

```bash
: "${AUTHORIZED_INPUT:?Set a locally authorized input path}"
mkdir -p "$run/paired"
for pair in {1..7}; do
  for entry in 'binaural-d1,7.1.4' 'binaural-d2,7.1.4' 'stereo,2.0' 'speaker,7.1.4'; do
    IFS=, read -r mode layout <<< "$entry"
    order="before candidate"; ((pair % 2)) || order="candidate before"
    for side in $order; do
      "$run/target-$side/release/examples/pcm_regression_probe" \
        "$AUTHORIZED_INPUT" "$mode" "$layout" \
        "$run/paired/$mode-$pair-$side" --timing-only
    done
  done
done
sha256sum --check "$run/binaries-before.txt"
```

For every pair, require equality of these summary fields from the private
`.timing.tsv` files: `input_sha256`, `input_access_units`, `output_frames`,
`output_sample_count`, `tail_samples`, `channel_count`, `config_descriptor_hex`,
`config_fingerprint`, `latency_samples`, `mode`, `layout`, `timing_only`.
Compute each paired percentage as
`100 * (candidate.api_pipeline_wall_ns / before.api_pipeline_wall_ns - 1)`;
then take the median of the seven paired percentages. Do not replace it with a
ratio of independently sorted medians. Publish only allowlisted numeric timing
columns; keep private raw files, inputs and PCM local.

## Unchanged frozen PCM gate: 26 cases

Run this separately from timing-only measurements:

```bash
python3 scripts/generate_performance_corpus.py --repo "$candidate" \
  --target "$run/corpus-target" --output-dir "$run/corpus" --durations 30 60
args=()
for entry in 'stereo,2.0' 'speaker,2.0' 'speaker,7.1.4' 'speaker,22.2' \
  'binaural-d1,7.1.4' 'binaural-d2,7.1.4' 'orientation-d1,7.1.4' 'orientation-d2,7.1.4'; do
  args+=(--case "lifecycle-${entry//,/-},$candidate/crates/openjoc-wasm/testdata/joc.lifecycle.ec3,$entry")
  for seconds in 30 60; do
    args+=(--case "public-${seconds}s-${entry//,/-},$run/corpus/joc.lifecycle.${seconds}s.ec3,$entry")
  done
done
for mode in binaural-d1 binaural-d2; do
  args+=(--case "authorized-full-$mode,$AUTHORIZED_INPUT,$mode,7.1.4")
done
python3 scripts/verify_pcm_bitexact.py --toolchain 1.98.1 \
  --baseline-root "$run/oracle" --candidate-root "$candidate" \
  --baseline-target "$run/oracle-target" --candidate-target "$run/target-candidate" \
  --output-dir "$run/exact" --verbose-builds --keep --selftest-integrated "${args[@]}"
```

The recorded evaluation split this into 10 lifecycle/private cases and 16 public
cases, with identical options and source. The strict gate verifies the pinned
frozen baseline and public corpus identities; do not repin to bypass a mismatch.

## Final-source quality, MSRV and portability

```bash
export CARGO_TARGET_DIR="$run/target-candidate"
cargo +1.98.1 test --release --locked --offline -p openjoc-render -- --test-threads=1
cargo +1.98.1 clippy --locked --offline --workspace \
  --exclude gst-plugin-openjoc --exclude openjoc-ffmpeg --all-targets --all-features -- -D warnings
cargo +1.98.1 test --locked --offline --workspace \
  --exclude gst-plugin-openjoc --exclude openjoc-ffmpeg --all-features -- --test-threads=1
cargo +1.98.1 test --locked --offline -p gst-plugin-openjoc -p openjoc-ffmpeg -- --test-threads=1
cargo +1.98.1 build --locked --offline --workspace --release
cargo +1.98.1 fmt --all -- --check
python3 scripts/check_repository_hygiene.py
python3 -m unittest discover -s scripts/tests -v
git diff --check
rustup target add --toolchain 1.89.0 aarch64-unknown-linux-gnu wasm32-unknown-unknown
for target in x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu wasm32-unknown-unknown; do
  CARGO_TARGET_DIR="$run/msrv" cargo +1.89.0 check --locked --offline -p openjoc-render --target "$target"
done
CARGO_TARGET_DIR="$run/msrv" cargo +1.89.0 test --release --locked --offline \
  -p openjoc-render --lib -- --test-threads=1
objdump -Cd "$run/target-candidate/release/examples/direct_fir_public_probe" > "$run/assembly.txt"
```

Inspect the entry, feature-check dispatch and AVX helper separately. Require
separate packed gain/tap multiply and add operations, ascending tap recurrence,
no FMA/horizontal reduction, and no AVX instruction in the feature-check wrapper.
The optional native-feature exclusions above are environment limits, not a full
all-feature pass. Existing hosted native FFmpeg/player checks already trigger on
renderer changes; their eventual exact-commit results must be recorded separately.
