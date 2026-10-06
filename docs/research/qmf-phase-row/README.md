# Released-path CPU investigation

## Scope and status

Measurements are a Linux x86_64 analogue on virtualized AMD EPYC 9V74, frozen source 15aefe1baa7b40f37950df252b6dbf6179894d6d, Rust 1.98.1 / LLVM 22.1.8, default release profile and default API features. They are not Windows, LAV, PotPlayer, physical-device or i5-8250U measurements. Inputs are the existing generated, varied 30/60-second synthetic corpus, not real programme media. All four regenerated input hashes match the established corpus exactly.

The published v0.18.0 source is e09d7b579fd765e211146d1b39138ae92d9e79e6, not the newer frozen gate source. Its QMF, EAC3 transform and static FIR kernels are unchanged at the frozen baseline, while API/FFmpeg wrappers have changed. See [RELEASED_PATHS.md](RELEASED_PATHS.md) for exact source links and release-build evidence.

Only one production candidate is under test: the QMF analysis phase-row slice/zip. Micro exactness, all required full-PCM checks and all thirty primary paired end-to-end timing comparisons passed. Final focused, full-workspace, MSRV and Python validation passed. The QMF change is proposed as a separate draft PR stacked on [the strict gate PR #17](https://github.com/chyinan/OpenJOC/pull/17), which remains a dependency until merged. Local validation below is separate from the new PR’s hosted CI and from pending Windows/device validation; see the PR for current CI links. No merge or auto-merge is requested.

## Released route

- PotPlayer must explicitly prefer the side-by-side OpenJOC LAV filter
- Shipping LAV default is Stereo with calibrated dialnorm; ordinary non-JOC/passthrough bypasses this OpenJOC path
- Binaural is opt-in: D1 with virtual7.1.4 by default, LFE excluded
- The released LAV/API binaural route uses direct FIR. rustfft is compiled as a dependency, and the released CLI exposes explicit partitioned-FFT selection, but LAV/API does not select it
- Released LAV builds used Rust 1.98.1 on x86_64-pc-windows-msvc; the earlier1.89 regression lane is retained separately

## Timing definitions

- Process CPU is Linux CLOCK_PROCESS_CPUTIME_ID, sampled around the measured processing loop and drain after construction/warm-up/reset. It includes small harness-loop overhead; it is not CPU utilization percent or hardware cycle count
- Pipeline wall time sums push/receive/drop call windows plus drain API windows. Plain timing does not hash or write PCM inside those windows; profiling, allocation counts and full PCM capture are separate runs
- RTF is pipeline wall seconds divided by input programme samples /48,000. The inputs are30.016s and60.000s, not exactly30s for the shorter file
- ns_per_scalar_sample divides pipeline nanoseconds by output_sample_count × channel_count, including delayed/drain output. output_sample_count is per-channel time positions, not already multiplied by channels. This denominator changes across layouts; use RTF or total CPU time for cross-layout comparisons
- Microseconds/analyze refers to one64-input-sample QMF call producing64 complex subbands. It is not a per-scalar-output sample metric

## Baseline self-repeat first

Before profiling or product edits, five sequential alternating pairs ran the same product under the same compiler/features. The independently built API binaries are byte-identical: SHA256 8d3fb53e62e1e64971d17325a0ad2e1b554fac8d427eb9ca67e7d5c5b7414b83.

Baseline-side five-repeat RTF (wall time / input duration, lower is faster):

| 30.016s path | Median | Min–max | MAD | Full range / median |
|---|---:|---:|---:|---:|
| Stereo 2.0 |0.123518|0.120459–0.126962|0.001782|5.26%|
| Speaker 7.1.4 |0.140206|0.137229–0.145087|0.000815|5.60%|
| D1 binaural 7.1.4 |0.593013|0.587565–0.613622|0.005448|4.39%|

The other five repeats were the same byte-identical binary, not a changed product. Paired candidate-minus-baseline deltas ranged −4.09% to +9.68% in Stereo, −1.42% to +4.32% in speaker7.1.4, and −0.47% to +2.22% in D1 binaural. Allocation counts are not evidence of CPU benefit. Raw CSV, compiler provenance, logs and binary hashes are retained in the reproducibility bundle; compact tables are adjacent to this report.

## Function attribution

Linux perf was installed from official Debian packages, but record smoke tests captured zero samples with mmap/sample-id warnings. No kernel security setting was changed. GNU gprofng 2.44 was available. Requests for1ms/10ms sampling were under-delivered here; a native2s busy-loop check at100ms yielded19samples/1.9sampledCPU seconds against1.998target userCPU seconds. Therefore these are coarse100ms profiles.

Every profile retains the collector warning `Collection interval timer period was changed (100000 -> 0); profile data may be unreliable`. The high sampled/actual CPU coverage and repeated Stereo result support only broad hotspot selection; they do not erase the warning or justify fine-grained percentages as precise population estimates. Profiling was separate from all plain timing and exact capture runs.

| 60s path | Samples | Sampled CPU | Target user+sys CPU | Largest exclusive functions |
|---|---:|---:|---:|---|
| Stereo 2.0 |73|7.3s|7.364s|QMF analyze72.60%; inverse_long13.70%; spatial render_coordinates4.11%; QMF synthesize2.74%|
| Stereo repeat |74|7.4s|See raw statistics|QMF analyze74.32%; inverse_long9.46%; QMF synthesize4.05%|
| Speaker 7.1.4 |83|8.3s|8.350s|QMF analyze60.24%; inverse_long16.87%; spatial render_coordinates8.43% exclusive/9.64% inclusive|
| D1 binaural 7.1.4 |356|35.6s|35.642s|BinauralRenderer::render_block78.93%; QMF analyze15.45%; inverse_long2.53%; QMF synthesize1.40%|

Percentages are exclusive sample shares unless marked inclusive. Inclusive caller shares overlap and must not be summed. Raw headers, target resource statistics, named functions and annotated hot PCs are retained under profiles/. The broad earlier decode bucket is now attributed primarily to QMF analysis on these synthetic speaker paths. Direct FIR dominates binaural and remains a separate, unimplemented next candidate.

## One QMF candidate

At crates/openjoc-qmf/src/lib.rs:114–123, validate the128-entry phase row once per subband and zip it with folded samples. Keep the exact outer subband order, ascending128-term reduction, Complex64::new products and AddAssign, phase construction, history and folding. No FFT, arithmetic precision, reassociation or FMA change.

Baseline assembly performs phase bounds validation and reloads/stores the accumulator on every term. Its hottest Stereo PCs are the addpd, stack store and loop increment. Candidate release assembly moves bounds validation outside the inner loop, retains the accumulator in registers and unrolls two sequential terms with separate mulpd/addpd; the accumulation recurrence remains ordered.

Micro oracle: every 44,288 real/imag f64 component across 346 blocks matches byte-for-byte (354,304bytes, SHA25672363cfd5c17c2c849903f99b870f32ccdec1e0e4ae98df6af00bca4ce245e5d), including nonsilent multiblock signals, signed/positive zero, impulses, two resets and complete history flushes.

Five alternating warmed65,536-call micro pairs: baseline median23.511µs/analyze, candidate6.208µs; every pair71.65–74.81% lower elapsed. Baseline total-time range1514.026–1615.078ms and candidate393.626–437.164ms do not overlap. Generation, OnceLock initialization, capture and hashing were outside timing. A later independent five-pair micro run on finalized probe bytes again improved all pairs,64.12–77.74%, with medians23.312µs →6.062µs. Both runs and all raw pairs are retained, including the noisier outliers. Finalized captures match the original full f64 oracle byte-for-byte. End-to-end evidence follows below.

## End-to-end result

Thirty alternating primary comparisons used less measured process CPU with the candidate. Full PCM matched for all three modes at both durations before timing. Timing-only captures are separate and validate shape; they are not substituted for PCM equality.

| Path | Baseline median CPU s | Candidate median CPU s | Paired CPU reduction range |
|---|---:|---:|---:|
|stereo30|3.7316|1.6841|51.9–55.0%|
|stereo60|7.2809|3.3444|53.7–55.5%|
|speaker714-30|3.9912|2.0535|46.3–50.7%|
|speaker714-60|7.7879|4.0395|46.8–50.4%|
|d1-714-30|17.9740|15.9270|9.7–13.2%|
|d1-714-60|36.3589|32.2626|8.0–13.0%|

The same improvement repeats at 30 and 60 seconds. Absolute reductions differ because QMF dominates Stereo/speaker processing but direct FIR dominates binaural. These figures apply only to this synthetic Linux API workload, not all media or Windows/PotPlayer hardware.

## Exactness coverage and validation

- Full 30/60s PCM: Stereo 2.0, speaker7.1.4, D1 binaural 7.1.4, speaker22.2, D2 binaural22.2 and D1 orientation 7.1.4 all pass
- Extra short22.2/D2/orientation comparisons pass
- Ordinary EAC3 core60s passes its full f32 and f64 planes; baseline/candidate core binaries are byte-identical
- One-bit integrated PCM mutation is rejected
- Initial combined run reached nine passing comparisons, then rejected my erroneous ordinary-core layout argument `core`; corrected invocation with `5.1` passed. This was a driver configuration error, not a DSP mismatch; both logs are retained
- Final delivery is based on PR17 head7f42a6886a5d3a20be6937eef8f087b3759b4805, preserving its Windows fixture fixes. Production QMF source equals the measured source. The final rebuilt API probe is byte-identical to the measured candidate: SHA256 5794eb77e7117973561ebfa392fb4c4e872138a22b155b26f0eb53653038c426
- Rust 1.98.1 full workspace release tests:1,103 passed,0 failed,16 explicitly ignored across116 groups
- Rust 1.98.1 focused QMF/JOC/API all-feature tests:80 passed,0 failed,4 ignored manual diagnostics
- Rust 1.89.0 MSRV QMF tests:7 passed,0 failed
- Python suite:205 total,149 passed,56 expected skips; initial invocation from the parent directory had a scripts-import error, corrected by running from the repository root
- Strict focused Clippy, full-workspace rustfmt and git diff --check pass
- Native Windows/DirectShow/PotPlayer, physical endpoints, user i5-8250U, full workspace all-features requiring external SDKs and real programme media remain untested
