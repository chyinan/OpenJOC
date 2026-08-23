# OpenJOC-LAV Multichannel Output Implementation Plan — Phase 2

**Goal:** Configure OpenJOC for Stereo or an explicit built-in speaker preset and preserve that immutable semantic identity through LAV frame handoff.

**Architecture:** `LAVOpenJocDecoder` owns the selected contract and destroys/recreates its stream decoder on policy changes. Returned frames are validated against OpenJOC layout metadata before copying, then LAV constructs the exact native FFmpeg mask from the contract rather than from channel count.

**Tech Stack:** C++17, OpenJOC dynamic C ABI, FFmpeg `AVChannelLayout`, MSVC/v143, command-line decoder smoke tests.

**Scope:** 6 phases from the original design; this file is phase 2 of 6.

**Codebase verified:** 2026-08-23 at OpenJOC `53d27ff5b8db379089ed5e2fde50bcea1632fbfb` plus design commit `04f64f7`, and LAV `b06ba2cbbd5c8806ca4423a8ff1527e4e2bd6a27`.

---

## Acceptance Criteria Coverage

### openjoc-lav-multichannel-output.AC1: Output policy is explicit and stable

- **openjoc-lav-multichannel-output.AC1.1 Success:** A new filter defaults to Stereo and produces the same OpenJOC configuration and two-channel float output as the released path.
- **openjoc-lav-multichannel-output.AC1.2 Success:** Each admitted manual preset configures the public OpenJOC ABI with `OPENJOC_RENDER_SPEAKER` and the exact built-in preset name.
- **openjoc-lav-multichannel-output.AC1.3 Success:** Changing the policy recreates the OpenJOC stream decoder before subsequent frames are rendered.

### openjoc-lav-multichannel-output.AC2: Every candidate preserves canonical logical semantics

- **openjoc-lav-multichannel-output.AC2.1 Success:** Stereo, 5.1, 7.1, 5.1.2, 5.1.4, 7.1.2 and 7.1.4 map to the exact OpenJOC order, count and Windows mask recorded in the canonical layout table.
- **openjoc-lav-multichannel-output.AC2.2 Success:** PCM interleave order equals ascending set-bit WAVEFORMATEXTENSIBLE order, so admitted layouts require no silent reorder.

### openjoc-lav-multichannel-output.AC5: Lifecycle and memory remain safe at maximum admitted size

- **openjoc-lav-multichannel-output.AC5.3 Failure:** Oversized sample/channel counts fail before copy, append, allocator growth or sample delivery.

---

<!-- START_SUBCOMPONENT_A (tasks 1-2) -->
<!-- START_TASK_1 -->
### Task 1: Add decoder policy, semantic validation, and policy recreation

**Verifies:** openjoc-lav-multichannel-output.AC1.1, openjoc-lav-multichannel-output.AC1.2, openjoc-lav-multichannel-output.AC1.3, openjoc-lav-multichannel-output.AC2.1, openjoc-lav-multichannel-output.AC5.3

**Files:**

- Modify: `D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\OpenJocDecoder.h` (`// pattern: Imperative Shell` stateful decoder boundary)
- Modify: `D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\OpenJocDecoder.cpp`
- Modify/Test: `D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\OpenJocDecoderSmoke.cpp` (integration)
- Modify/Test: `D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\OpenJocOutputTests.cpp` (unit)
- Modify: `D:\Program\OpenJOC\scripts\release_lav_smokes.cmd`
- Modify/Test: `D:\Program\OpenJOC\scripts\tests\test_release_lav_smokes_script.py` (unit)

**Implementation:**

1. Extend tests first to require `SetOutputPolicy`, `OutputContract`, and an immutable `LAVOpenJocFrame::output_contract`. Expected RED: `C2039` for the missing members.
2. Default a new decoder to the Stereo contract. For Stereo configure `OPENJOC_RENDER_STEREO` with no speaker preset; for every preset configure `OPENJOC_RENDER_SPEAKER` with the contract's exact ABI preset name.
3. On an actual policy change, destroy the stream decoder, discard pending frames, reset admission/classifier state and counters, and create the next stream decoder with the new contract. A same-policy assignment is a no-op. `Reset()` retains policy while clearing seek/flush stream state.
4. Dynamically load `openjoc_stream_decoder_get_channel_label`. Centralize both existing frame-copy paths in `ValidateAndCopyFrame`: require float output, 48 kHz, exact count, exact `layout_name`, exact label sequence after explicit semantic-to-FFmpeg mapping, checked element/byte multiplication, and valid source length before `vector::assign`; catch allocation failure.
5. Mark both the stateful decoder header and source with `// pattern: Imperative Shell`; keep validation/mapping in the Phase 1 functional core.

**Testing:**

- The real `openjoc_capi.dll` must produce 2/6/8/8/10/10/12 channels and exact layout metadata for all policies.
- Switch a live decoder from 5.1 to 7.1.4 after pending output exists; old frames must be unavailable, counters must reset, and all subsequent frames must carry only the new contract.
- Invalid policy values must return failure and preserve the old contract.
- Reject zero channels, count mismatch, label/layout mismatch, invalid data length, and overflow before copying.
- Replace the old baseline smoke assertion that classification must consume an entire real JOC file; assert classification/streaming behavior instead.

**Verification:**

Update `release_lav_smokes.cmd` so both `OpenJocOutputTests` and `OpenJocDecoderSmoke` compile `OpenJocOutput.cpp`, link `"/LIBPATH:D:\Program\LAVFilters-OpenJOC\bin_x64\lib" avutil-lav.lib`, and fail on any compile/link error. Build OpenJOC C API, run the complete script into a fresh `phase02-smokes` directory, and copy `openjoc_capi.dll` plus `D:\Program\LAVFilters-OpenJOC\bin_x64\avutil-lav-61.dll` beside those newly built executables (or prepend that exact `bin_x64` directory to `PATH`). Execute `phase02-smokes\OpenJocOutputTests.exe`, then execute `phase02-smokes\OpenJocDecoderSmoke.exe` with the verified ordinary E-AC-3 and raw JOC fixtures. Do not reuse any Phase 1 `.codex-tmp\OpenJocOutputTests.exe`. Expected: both exit 0, and the decoder smoke covers all seven policies plus the policy-switch matrix.

**Commit:** `feat(audio): configure OpenJOC decoder output policies`
<!-- END_TASK_1 -->

<!-- START_TASK_2 -->
### Task 2: Preserve the exact contract in FFmpeg/LAV frame handoff

**Verifies:** openjoc-lav-multichannel-output.AC1.1, openjoc-lav-multichannel-output.AC1.2, openjoc-lav-multichannel-output.AC2.1, openjoc-lav-multichannel-output.AC2.2, openjoc-lav-multichannel-output.AC5.3

**Files:**

- Modify: `D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\OpenJocOutput.h`
- Modify: `D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\OpenJocOutput.cpp`
- Modify/Test: `D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\OpenJocOutputTests.cpp` (unit)
- Modify: `D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\LAVAudio.h` (`// pattern: Imperative Shell`)
- Modify: `D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\LAVAudio.cpp` (`// pattern: Imperative Shell`)

**Implementation:**

1. Write RED tests for `BuildOpenJocAvChannelLayout(contract, out)`. Require native order, exact count/mask/index-to-channel order, and clean failure without a partially initialized layout.
2. Build the native layout only from the contract's FFmpeg mask. Validate with `av_channel_layout_check`; never use `av_channel_layout_default(channel_count)` or infer from the ABI preset spelling.
3. In `DecodeOpenJoc`, remove the `channel_count > 8` rejection and count-only default layout. Require the frame's contract pointer to equal the current decoder contract, require float32/48 kHz/exact count, and use the helper to construct `BufferDetails::layout`.
4. Preserve existing timestamps and checked size narrowing. Do not implement postprocessor bypass or delivery changes here; those are Phase 3.
5. Add `// pattern: Imperative Shell` to modified legacy runtime files where absent.

**Testing:**

- Compare all generated layouts against FFmpeg's parser for the stored FFmpeg standard name.
- Explicitly prove OpenJOC ABI `5.1` equals FFmpeg `5.1(side)` and does not equal FFmpeg `5.1`.
- Confirm 10- and 12-channel buffers now cross `DecodeOpenJoc` with exact masks.

**Verification:**

```powershell
$env:PATH = 'D:\Program\LAVFilters-OpenJOC\bin_x64;D:\Program\OpenJOC\target\release;' + $env:PATH
& 'D:\Program\OpenJOC\scripts\release_lav_smokes.cmd' `
  'C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat' `
  'D:\Program\LAVFilters-OpenJOC' `
  'D:\Program\OpenJOC\crates\openjoc-capi\include' `
  'D:\Program\OpenJOC\scripts\tests\LavSmokeNoopLifecycle.cpp' `
  'D:\Program\OpenJOC\.codex-tmp\phase02-smokes'
& 'D:\Program\OpenJOC\.codex-tmp\phase02-smokes\OpenJocOutputTests.exe'
& 'D:\Program\OpenJOC\.codex-tmp\phase02-smokes\OpenJocDecoderSmoke.exe' `
  'D:\Program\OpenJOC\.codex-tmp\phase02-fixtures\ordinary.eac3' `
  'D:\Program\OpenJOC\.codex-tmp\phase02-fixtures\joc.multi.ec3'
cargo test -p openjoc-scene --test speaker_layouts public_presets_have_the_admitted_names_and_backend_contracts -- --exact
& 'D:\Program\OpenJOC\scripts\release_lav_msbuild.cmd' `
  'C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat' `
  'D:\Program\LAVFilters-OpenJOC' `
  'D:\Program\OpenJOC\crates\openjoc-capi\include' `
  'D:\Program\OpenJOC\.codex-tmp\phase02-lav-msbuild.log'
```

Expected: unit tests, decoder smoke, canonical Rust test, and full x64 LAV build all pass.

**Commit:** `feat(audio): preserve OpenJOC layouts through LAV frame handoff`
<!-- END_TASK_2 -->
<!-- END_SUBCOMPONENT_A -->
