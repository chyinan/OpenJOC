# Native AArch64 ordered adjacent-output FIR

This experiment adds an AArch64 path for the current-input interior of the
static `BinauralRenderer` FIR. The path is a four-output safe-Rust unroll. On
the tested Apple Silicon build, LLVM lowers the independent output lanes to
NEON `fmul.2d`/`fadd.2d` instructions.

The x86 runtime AVX path is unchanged. Results from the existing ordered AVX
experiment are x86-only and are not ARM results.

## Arithmetic and state contract

Each output lane starts from its existing left/right accumulator. For every
lane, registered-source order and ascending tap order are unchanged, and the
operations remain separate: input × gain, gained input × tap, then accumulator
addition. There is no FMA, fast-math flag, horizontal reduction, coefficient
or gain precombination, HRTF change, output-backend change, or floating-point
mode change.

The scalar history prefix, scalar remainder, tail drain, reset, failure state,
and source-order validation remain in the existing implementation. The
candidate has no new unsafe boundary on AArch64.

## Measured host and method

Measured 2026-10-07 on one Mac mini (`Mac14,3`, Apple M2, 8 cores, 8 GB),
Darwin 25.6, native `arm64`; `arch -arm64` succeeded and the release probes
were Mach-O `arm64`, so these runs did not use Rosetta. The compiler was native
Rust 1.97.1 / LLVM 22.1.6. Rust 1.89.0 could not be downloaded on this host
because the rustup TLS handshake failed; no 1.89 performance claim is made.

The existing synthetic corpus generator and PCM gate were reused. The corpus
is synthetic and local; no recording, PCM output, or private input fingerprint
is part of this repository change. Timing used five serial AB/BA pairs for
each case. Child process `user + sys` and external wall time were recorded
separately; the probe's macOS `probe_process_cpu_ns` field is unavailable.

Median full-pipeline results, lower is better:

| Case | Baseline wall | Candidate wall | Pipeline wall delta | Child CPU delta |
| --- | ---: | ---: | ---: | ---: |
| D1 / 7.1.4 / 30 s | 5.319 s | 3.379 s | -37.236% | -36.283% |
| D1 / 7.1.4 / 60 s | 10.456 s | 6.600 s | -37.459% | -36.852% |
| D2 / 7.1.4 / 30 s | 5.348 s | 3.379 s | -37.430% | -36.456% |
| D2 / 7.1.4 / 60 s | 10.467 s | 6.604 s | -37.309% | -36.853% |
| Stereo / 2.0 / 30 s | 1.552 s | 1.544 s | -0.283% | -0.301% |
| Stereo / 2.0 / 60 s | 3.093 s | 3.072 s | -0.662% | -0.490% |
| Speaker / 7.1.4 / 30 s | 1.735 s | 1.743 s | +0.689% | +0.287% |
| Speaker / 7.1.4 / 60 s | 3.464 s | 3.459 s | -0.153% | -0.148% |

The public direct-FIR probe's nine alternating pairs improved from a median
3.698 ms/call to 1.708 ms/call (-53.721%). The speaker control is effectively
flat within this campaign and includes a small observed regression; it is not
used to support the binaural gain claim.

## Exactness and hotspot evidence

- The 86-test `openjoc-render` release unit suite passed, including the frozen
  old-loop comparison, unequal 255/256/257/2/1-tap filters, cancellation,
  signed zero, subnormal values, extreme finite values, overflow/error order,
  reset, drain, source permutation, and history/tail state checks.
- The current HEAD scalar build and candidate were compared with the existing
  `compare_runs` PCM comparator for D1, D2, stereo, and 7.1.4 speaker at both
  30 and 60 seconds. All eight full f32 PCM streams matched byte-for-byte with
  identical manifests, sample counts, channels, rates, tails, and lifecycle.
- The unchanged frozen oracle gate at `15aefe1b` passed all 24 lifecycle/public
  cases and the integrated one-bit mutation rejection check. The frozen oracle
  was not updated.
- A 5-second `sample` baseline profile placed 2,604 of 3,799 samples in the
  renderer hot loop. The candidate profile placed 1,241 samples in the
  AArch64 helper and 643 in its calling renderer body.
- Final helper disassembly contains separate scalar and packed multiply/add
  instructions, with no `fmla`, `fmls`, `faddp`, or `faddv` in the helper.

These results describe this Mac mini only. They do not establish performance
on all ARM systems, Android, or low-end phones, and the x86 AVX measurements
must not be generalized to ARM.
