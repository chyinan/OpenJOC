# Reproduce the QMF phase-row experiment

This guide covers three separate activities: the frozen-oracle PCM check, paired
release timing, and the focused QMF microprobe. They answer different
questions. Run the exact PCM check first; timing and sampling are diagnostic
and never replace it.

The oracle is the frozen commit
`15aefe1baa7b40f37950df252b6dbf6179894d6d`, with pinned `Cargo.lock`
SHA-256
`40f4a91c662652bef309f2bf57d01cfa811c5d004ebf9d194bf9e688d4ab45e7`.
The paired runner accepts `--toolchain 1.98.1` for release profiling, while
its default remains Rust 1.89.0. The synthetic corpus generator remains pinned
to Rust 1.89.0. No numbers from this procedure establish PotPlayer behavior,
Windows device playback, or performance on a particular CPU.

## 1. Set up the worktrees and corpus

From the candidate repository root, set the frozen oracle and owned scratch
paths. Keep the exact-oracle baseline and the QMF microprobe baseline separate:

```sh
set -euo pipefail
REPO="$PWD"
PARENT="$(dirname "$REPO")"
BASELINE="$PARENT/openjoc-frozen-baseline"
MICROBASE="$PARENT/openjoc-qmf-micro-baseline"
RUNS="$PARENT/openjoc-qmf-reproduction"
CORPUS="$RUNS/corpus"
mkdir -p "$RUNS" "$CORPUS"
```

Prefer the supplied corpus bundle so Windows and Linux use identical bytes.
Its `corpus.json` manifest records generator provenance and SHA-256 for all
four inputs. The primary JOC inputs are:

- `joc.lifecycle.30s.ec3`: SHA-256
  `a44fc36470d07f98c68053c9015e3cb169a21af927b1b3b93bd5712a42234671`
- `joc.lifecycle.60s.ec3`: SHA-256
  `8202e69a5cd13c9142165315f7640d2d7c3cf554b6de0bba8b2c5c40cc499cf2`

Copy the bundle's `corpus.json` and all input files into `$CORPUS`, then verify
the hashes against the manifest before running the gate. The companion
ordinary-E-AC-3 fixtures and their hashes are also recorded there. If you must
regenerate instead, use the existing generator contract:

```sh
python3 scripts/generate_performance_corpus.py \
  --repo "$REPO" --target "$RUNS/corpus-target" \
  --output-dir "$CORPUS" --durations 30 60
```

The generator uses Rust 1.89.0 and requires FFmpeg/ffprobe 7.1.5 for the
recorded E-AC-3 bytes. It writes `corpus.json`; compare generated hashes to the
pinned values before proceeding. Do not silently substitute encoder output
from a different FFmpeg release.

Create the detached oracle and apply only the API/core probe overlays allowed
by `verify_pcm_bitexact.py`:

```sh
git worktree add --detach "$BASELINE" 15aefe1baa7b40f37950df252b6dbf6179894d6d
mkdir -p "$BASELINE/crates/openjoc-api/examples" \
         "$BASELINE/crates/openjoc-eac3/examples"
cp "$REPO/crates/openjoc-api/Cargo.toml" "$BASELINE/crates/openjoc-api/"
cp "$REPO/crates/openjoc-api/examples/pcm_regression_probe.rs" \
   "$BASELINE/crates/openjoc-api/examples/"
cp "$REPO/crates/openjoc-eac3/Cargo.toml" "$BASELINE/crates/openjoc-eac3/"
cp "$REPO/crates/openjoc-eac3/examples/eac3_pcm_regression_probe.rs" \
   "$BASELINE/crates/openjoc-eac3/examples/"
```

The runner checks oracle HEAD, lockfile SHA, exact feature-manifest overlays,
and identical probe bytes. Do not copy the QMF microprobe example into this
`BASELINE`: it is outside the strict baseline allowlist. The next section uses
a separate `$MICROBASE` worktree for that purpose.

## 2. Run the primary 30/60-second exact matrix

Run all six primary API cases through the bit-exact PCM gate before any timing
comparison. This builds both revisions in separate release target directories
with Rust 1.98.1 and compares full PCM output, descriptors, delayed output, and
tail handling:

```sh
python3 scripts/verify_pcm_bitexact.py \
  --toolchain 1.98.1 --verbose-builds \
  --baseline-root "$BASELINE" --candidate-root "$REPO" \
  --baseline-target "$RUNS/target-baseline-exact" \
  --candidate-target "$RUNS/target-candidate-exact" \
  --output-dir "$RUNS/exact" --selftest-integrated \
  --case joc30-stereo,"$CORPUS/joc.lifecycle.30s.ec3",stereo,2.0 \
  --case joc30-d1-binaural,"$CORPUS/joc.lifecycle.30s.ec3",binaural-d1,7.1.4 \
  --case joc30-speaker,"$CORPUS/joc.lifecycle.30s.ec3",speaker,7.1.4 \
  --case joc60-stereo,"$CORPUS/joc.lifecycle.60s.ec3",stereo,2.0 \
  --case joc60-d1-binaural,"$CORPUS/joc.lifecycle.60s.ec3",binaural-d1,7.1.4 \
  --case joc60-speaker,"$CORPUS/joc.lifecycle.60s.ec3",speaker,7.1.4
```

Do not continue to timing if any exact case fails. Keep this directory's logs,
`toolchain.txt`, measurement output, and binary SHA-256 values with the run
record.

## 3. Run five paired timing repeats separately

Only after the exact matrix passes, make a separate plain-allocator timing run.
It rebuilds both revisions, uses five alternating baseline/candidate pairs per
case, and checks paired output shape. `--timing-only` does not capture PCM, so
this is not an exactness check:

```sh
python3 scripts/verify_pcm_bitexact.py \
  --toolchain 1.98.1 --verbose-builds --timing-only --repeats 5 \
  --baseline-root "$BASELINE" --candidate-root "$REPO" \
  --baseline-target "$RUNS/target-baseline-timing" \
  --candidate-target "$RUNS/target-candidate-timing" \
  --output-dir "$RUNS/timing" \
  --case joc30-stereo,"$CORPUS/joc.lifecycle.30s.ec3",stereo,2.0 \
  --case joc30-d1-binaural,"$CORPUS/joc.lifecycle.30s.ec3",binaural-d1,7.1.4 \
  --case joc30-speaker,"$CORPUS/joc.lifecycle.30s.ec3",speaker,7.1.4 \
  --case joc60-stereo,"$CORPUS/joc.lifecycle.60s.ec3",stereo,2.0 \
  --case joc60-d1-binaural,"$CORPUS/joc.lifecycle.60s.ec3",binaural-d1,7.1.4 \
  --case joc60-speaker,"$CORPUS/joc.lifecycle.60s.ec3",speaker,7.1.4

python3 scripts/performance_profile_repro.py \
  "$RUNS/timing/measurements.csv" \
  --metric summary_api_pipeline_wall_ns
```

The helper filters invalid inputs and reports each side's median, range, and
median absolute deviation (MAD), plus paired relative deltas. Deltas are
descriptive; they do not establish speedup. It reports probe-binary identity
only when the measurement CSV contains SHA-256 fields. Preserve compiler
version, flags, inputs, and both binary hashes alongside the CSV.

## 4. Collect coarse CPU-clock function samples

On Linux, use GNU gprofng as a separate diagnostic run against the built
baseline probe. Capture one experiment per configuration; the following shows
explicit LAV Stereo 2.0. Repeat with `binaural-d1 7.1.4` and `speaker 7.1.4`.
The profile's `.er` experiment and probe timing file are separate from the
exact and timing-run outputs:

```sh
mkdir -p "$RUNS/gprofng"
BIN="$RUNS/target-baseline-exact/release/examples/pcm_regression_probe"

gprofng collect app -p 100 -o "$RUNS/gprofng/stereo-2.0.er" \
  "$BIN" "$CORPUS/joc.lifecycle.30s.ec3" stereo 2.0 \
  "$RUNS/gprofng/stereo-2.0" --timing-only
```

A 100 ms sampling interval is coarse. In the available Linux run it produced
only 19 samples over about 1.9 seconds of a 2.011-second user-CPU interval, so
individual function counts have limited coverage and are not statistically
precise. Record actual sample count, warnings, binary SHA-256, compiler flags,
and mode/layout for each `.er` file. Do not use profiled wall time as an
unprofiled performance result. To inspect the current candidate rather than
the frozen oracle, repeat the same three cases with the candidate executable
under distinct candidate-labeled `.er` paths. This Linux sampling recipe does
not demonstrate Windows, PotPlayer, or device playback behavior.

## 5. Run the QMF-only microprobe in its own diagnostic worktree

The QMF probe is a focused primitive test, not a replacement for the primary
API exact gate. Its source is a new QMF example, which the strict oracle
validator does not allow in `$BASELINE`. Use a distinct detached worktree at
the same frozen commit, and copy only the probe example into that diagnostic
worktree:

```sh
git worktree add --detach "$MICROBASE" 15aefe1baa7b40f37950df252b6dbf6179894d6d
mkdir -p "$MICROBASE/crates/openjoc-qmf/examples"
cp "$REPO/crates/openjoc-qmf/examples/qmf_analyze_probe.rs" \
   "$MICROBASE/crates/openjoc-qmf/examples/"

CARGO_TARGET_DIR="$RUNS/target-qmf-micro-baseline" \
  cargo +1.98.1 build --release --locked --offline \
    -p openjoc-qmf --example qmf_analyze_probe --manifest-path "$MICROBASE/Cargo.toml"
CARGO_TARGET_DIR="$RUNS/target-qmf-micro-candidate" \
  cargo +1.98.1 build --release --locked --offline \
    -p openjoc-qmf --example qmf_analyze_probe --manifest-path "$REPO/Cargo.toml"

BASE_PROBE="$RUNS/target-qmf-micro-baseline/release/examples/qmf_analyze_probe"
CANDIDATE_PROBE="$RUNS/target-qmf-micro-candidate/release/examples/qmf_analyze_probe"
mkdir -p "$RUNS/qmf-capture"
"$BASE_PROBE" capture "$RUNS/qmf-capture/baseline.raw"
"$CANDIDATE_PROBE" capture "$RUNS/qmf-capture/candidate.raw"
cmp "$RUNS/qmf-capture/baseline.raw" "$RUNS/qmf-capture/candidate.raw"
sha256sum "$RUNS/qmf-capture/baseline.raw" "$RUNS/qmf-capture/candidate.raw"
```

A passing byte comparison covers the probe's full capture: 346 analysis blocks,
all 64 complex f64 subbands per block, reset sequences, signed/positive-zero
flushes, a multiblock deterministic signal, and impulses at selected phase
positions. Keep both captures and hashes with the run record.

Then measure the fixed microprobe with five alternating pairs. This order
reduces simple first/second-run bias; it does not prove a performance gain:

```sh
set -o pipefail
for pair in 1 2 3 4 5; do
  if (( pair % 2 )); then order=(baseline candidate); else order=(candidate baseline); fi
  for side in "${order[@]}"; do
    if [[ "$side" == baseline ]]; then exe="$BASE_PROBE"; else exe="$CANDIDATE_PROBE"; fi
    printf 'pair=%s side=%s\n' "$pair" "$side" | tee -a "$RUNS/qmf-capture/bench.log"
    "$exe" bench 65536 2>&1 | tee -a "$RUNS/qmf-capture/bench.log"
  done
done
```

Preserve the raw log and report the paired values and spread. These calls isolate
QMF analysis over deterministic in-memory blocks; they do not model E-AC-3
decoding, full JOC rendering, a real programme, LAV, or player overhead.

## 6. Windows and portability notes

`verify_pcm_bitexact.py` resolves release example executables with `.exe` on
Windows automatically. For the QMF microprobe, direct PowerShell invocation
uses explicit `.exe` names. The `corpus.json` manifest in the supplied corpus
bundle should be used to verify the copied bytes rather than re-encoding them
on the Windows machine. A Windows API exact/timing reproduction is still not
PotPlayer or output-device validation and is not evidence for an Intel
i5-8250U. Do not make Windows or device-performance claims from Linux/gprofng
or from synthetic corpus results.

For Windows, set the bundle path to the extracted directory containing
`corpus.json` and the media files, then use the same exact/timing runner. The
commands below use the bundled bytes; `Get-FileHash` verifies the two primary
JOC inputs before work begins. `@Cases` expands into the six named API cases.

```powershell
$Repo = (Get-Location).Path
$Parent = Split-Path $Repo -Parent
$Baseline = Join-Path $Parent "openjoc-frozen-baseline"
$Microbase = Join-Path $Parent "openjoc-qmf-micro-baseline"
$Runs = Join-Path $Parent "openjoc-qmf-reproduction"
$CorpusBundle = Join-Path $Parent "openjoc-profile-corpus" # extracted bundle directory
$Corpus = Join-Path $Runs "corpus"
New-Item -ItemType Directory -Force $Runs, $Corpus | Out-Null
Copy-Item (Join-Path $CorpusBundle "corpus.json") $Corpus
Copy-Item (Join-Path $CorpusBundle "joc.lifecycle.30s.ec3") $Corpus
Copy-Item (Join-Path $CorpusBundle "joc.lifecycle.60s.ec3") $Corpus
Copy-Item (Join-Path $CorpusBundle "ordinary.multitone.30s.eac3") $Corpus
Copy-Item (Join-Path $CorpusBundle "ordinary.multitone.60s.eac3") $Corpus
$Joc30 = Join-Path $Corpus "joc.lifecycle.30s.ec3"
$Joc60 = Join-Path $Corpus "joc.lifecycle.60s.ec3"
if ((Get-FileHash $Joc30 -Algorithm SHA256).Hash.ToLower() -ne `
  "a44fc36470d07f98c68053c9015e3cb169a21af927b1b3b93bd5712a42234671") { throw "30s JOC corpus hash mismatch" }
if ((Get-FileHash $Joc60 -Algorithm SHA256).Hash.ToLower() -ne `
  "8202e69a5cd13c9142165315f7640d2d7c3cf554b6de0bba8b2c5c40cc499cf2") { throw "60s JOC corpus hash mismatch" }

git worktree add --detach $Baseline 15aefe1baa7b40f37950df252b6dbf6179894d6d
New-Item -ItemType Directory -Force `
  (Join-Path $Baseline "crates/openjoc-api/examples"), `
  (Join-Path $Baseline "crates/openjoc-eac3/examples") | Out-Null
Copy-Item (Join-Path $Repo "crates/openjoc-api/Cargo.toml") (Join-Path $Baseline "crates/openjoc-api/Cargo.toml")
Copy-Item (Join-Path $Repo "crates/openjoc-api/examples/pcm_regression_probe.rs") (Join-Path $Baseline "crates/openjoc-api/examples/pcm_regression_probe.rs")
Copy-Item (Join-Path $Repo "crates/openjoc-eac3/Cargo.toml") (Join-Path $Baseline "crates/openjoc-eac3/Cargo.toml")
Copy-Item (Join-Path $Repo "crates/openjoc-eac3/examples/eac3_pcm_regression_probe.rs") (Join-Path $Baseline "crates/openjoc-eac3/examples/eac3_pcm_regression_probe.rs")
$Cases = @(
  "--case", "joc30-stereo,$Joc30,stereo,2.0",
  "--case", "joc30-d1-binaural,$Joc30,binaural-d1,7.1.4",
  "--case", "joc30-speaker,$Joc30,speaker,7.1.4",
  "--case", "joc60-stereo,$Joc60,stereo,2.0",
  "--case", "joc60-d1-binaural,$Joc60,binaural-d1,7.1.4",
  "--case", "joc60-speaker,$Joc60,speaker,7.1.4"
)
python scripts/verify_pcm_bitexact.py --toolchain 1.98.1 --verbose-builds `
  --baseline-root $Baseline --candidate-root $Repo `
  --baseline-target (Join-Path $Runs "target-baseline-exact") `
  --candidate-target (Join-Path $Runs "target-candidate-exact") `
  --output-dir (Join-Path $Runs "exact") --selftest-integrated @Cases
if ($LASTEXITCODE -ne 0) { throw "exact PCM gate failed" }

# After the exact gate passes, run timing separately. These rows are not PCM checks.
python scripts/verify_pcm_bitexact.py --toolchain 1.98.1 --verbose-builds `
  --baseline-root $Baseline --candidate-root $Repo `
  --baseline-target (Join-Path $Runs "target-baseline-timing") `
  --candidate-target (Join-Path $Runs "target-candidate-timing") `
  --output-dir (Join-Path $Runs "timing") --timing-only --repeats 5 @Cases
if ($LASTEXITCODE -ne 0) { throw "paired timing run failed" }
```

Cargo's Windows release examples end in `.exe`; the paired runner resolves this
automatically. For direct QMF microprobe invocations, use explicit
`qmf_analyze_probe.exe` paths, and never copy that probe into `$Baseline`:

```powershell
git worktree add --detach $Microbase 15aefe1baa7b40f37950df252b6dbf6179894d6d
New-Item -ItemType Directory -Force (Join-Path $Microbase "crates/openjoc-qmf/examples") | Out-Null
Copy-Item (Join-Path $Repo "crates/openjoc-qmf/examples/qmf_analyze_probe.rs") `
  (Join-Path $Microbase "crates/openjoc-qmf/examples/qmf_analyze_probe.rs")
$env:CARGO_TARGET_DIR = Join-Path $Runs "target-qmf-micro-baseline"
cargo +1.98.1 build --release --locked --offline -p openjoc-qmf `
  --example qmf_analyze_probe --manifest-path (Join-Path $Microbase "Cargo.toml")
$BaseProbe = Join-Path $env:CARGO_TARGET_DIR "release/examples/qmf_analyze_probe.exe"
$env:CARGO_TARGET_DIR = Join-Path $Runs "target-qmf-micro-candidate"
cargo +1.98.1 build --release --locked --offline -p openjoc-qmf `
  --example qmf_analyze_probe --manifest-path (Join-Path $Repo "Cargo.toml")
$CandidateProbe = Join-Path $env:CARGO_TARGET_DIR "release/examples/qmf_analyze_probe.exe"
$CaptureDir = Join-Path $Runs "qmf-capture"
New-Item -ItemType Directory -Force $CaptureDir | Out-Null
& $BaseProbe capture (Join-Path $CaptureDir "baseline.raw")
& $CandidateProbe capture (Join-Path $CaptureDir "candidate.raw")
& "$env:WINDIR\System32\fc.exe" /b `
  (Join-Path $CaptureDir "baseline.raw") (Join-Path $CaptureDir "candidate.raw")
if ($LASTEXITCODE -ne 0) { throw "QMF full-capture bytes differ" }
Get-FileHash (Join-Path $CaptureDir "baseline.raw"), (Join-Path $CaptureDir "candidate.raw") -Algorithm SHA256

# Alternating baseline-first / candidate-first order, five pairs.
$BenchLog = Join-Path $CaptureDir "bench.log"
for ($pair = 1; $pair -le 5; $pair++) {
  if ($pair % 2) { $Order = @("baseline", "candidate") }
  else { $Order = @("candidate", "baseline") }
  foreach ($side in $Order) {
    if ($side -eq "baseline") { $Probe = $BaseProbe } else { $Probe = $CandidateProbe }
    "pair=$pair side=$side" | Tee-Object -FilePath $BenchLog -Append
    & $Probe bench 65536 2>&1 | Tee-Object -FilePath $BenchLog -Append
    if ($LASTEXITCODE -ne 0) { throw "QMF benchmark probe failed" }
  }
}
```

To regenerate on Windows rather than reuse the bundle, the same generator
contract applies: Rust 1.89.0, FFmpeg/ffprobe 7.1.5, `--durations 30 60`, and
hash validation against `corpus.json`. Keep those generator builds separate
from the Rust 1.98.1 paired profiling builds. For direct microprobe invocation,
Cargo's release examples are at `target\release\examples\qmf_analyze_probe.exe`;
use separate `CARGO_TARGET_DIR` values for the baseline and candidate builds.
