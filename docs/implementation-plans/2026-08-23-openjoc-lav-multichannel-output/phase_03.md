# OpenJOC-LAV Multichannel Output Implementation Plan — Phase 3

**Goal:** Preserve a strict OpenJOC semantic contract through postprocessing and deliver exactly one float32/48 kHz WAVEFORMATEXTENSIBLE type, failing safely on any rejection or mutation.

**Architecture:** Strict buffers carry the immutable Phase 1 contract pointer. A functional strict-output core builds and compares every field and format byte of the exact media type and performs checked arithmetic. Executable delivery and queue transaction seams make ordering, ownership and failure atomicity testable; the LAV imperative shell calls those seams, bypasses layout-changing postprocessing and uses a no-fallback delivery branch while stock buffers retain their existing behavior. Strict equality never uses `CMediaType::operator==`, which omits flags and sample size.

**Tech Stack:** C++17, DirectShow baseclasses, WAVEFORMATEXTENSIBLE, MSVC/v143, fake pin/sample/allocator unit integration tests.

**Scope:** 6 phases from the original design; this file is phase 3 of 6.

**Codebase verified:** 2026-08-23 at OpenJOC `53d27ff5b8db379089ed5e2fde50bcea1632fbfb` plus design commit `04f64f7`, and LAV `b06ba2cbbd5c8806ca4423a8ff1527e4e2bd6a27`.

---

## Acceptance Criteria Coverage

### openjoc-lav-multichannel-output.AC3: DirectShow negotiation is exact and strict

- **openjoc-lav-multichannel-output.AC3.1 Success:** Every candidate media type is float32, 48 kHz, `WAVE_FORMAT_EXTENSIBLE`, with exact channels, valid bits, subformat, mask, block alignment, average byte rate and sample size.
- **openjoc-lav-multichannel-output.AC3.3 Failure:** Exact rejection returns a recorded failure and never falls back to int16, another 5.1 variant, 7.1, Stereo or the currently connected layout.
- **openjoc-lav-multichannel-output.AC3.4 Failure:** `QueryAccept`, `EnumMediaTypes`, a legal mask, endpoint properties or a channel count alone never produce a PASS claim.

### openjoc-lav-multichannel-output.AC4: Stock LAV behavior remains isolated

- **openjoc-lav-multichannel-output.AC4.3 Failure:** OpenJOC policy settings do not affect stock input media-type selection or generic fallback behavior.
- **openjoc-lav-multichannel-output.AC4.4 Failure:** Stock LAV mixing does not replace or duplicate OpenJOC speaker rendering.

### openjoc-lav-multichannel-output.AC5: Lifecycle and memory remain safe at maximum admitted size

- **openjoc-lav-multichannel-output.AC5.2 Success:** Frame, queue, allocator and delivery byte counts use checked multiplication/addition before allocation or narrowing.
- **openjoc-lav-multichannel-output.AC5.3 Failure:** Oversized sample/channel counts fail before copy, append, allocator growth or sample delivery.

---

<!-- START_SUBCOMPONENT_A (tasks 1-3) -->
<!-- START_TASK_1 -->
### Task 1: Build exact strict media types and checked arithmetic

**Verifies:** openjoc-lav-multichannel-output.AC3.1, openjoc-lav-multichannel-output.AC3.4, openjoc-lav-multichannel-output.AC5.2, openjoc-lav-multichannel-output.AC5.3

**Files:**

- Create: `D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\OpenJocStrictOutput.h` (`// pattern: Functional Core`)
- Create: `D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\OpenJocStrictOutput.cpp`
- Create: `D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\OpenJocStrictNegotiation.h` (`// pattern: Imperative Shell` executable delivery/queue seams)
- Create: `D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\OpenJocStrictNegotiation.cpp`
- Create/Test: `D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\OpenJocStrictOutputTests.cpp` (unit/integration)
- Modify: `D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\LAVAudio.vcxproj`
- Modify: `D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\LAVAudio.vcxproj.filters`
- Modify: `D:\Program\OpenJOC\scripts\release_lav_smokes.cmd`
- Modify/Test: `D:\Program\OpenJOC\scripts\tests\test_release_lav_smokes_script.py` (unit)

**Implementation:**

1. Write tests that reference a missing strict builder and checked-size helpers. Expected RED: missing header/symbol compilation failure.
2. Add a `// pattern: Functional Core` header/source module that consumes only a validated contract. For every policy, including semantic ABI layout `2.0` / UI label `Stereo`, build float32/48 kHz `WAVE_FORMAT_EXTENSIBLE` with `cbSize=22`, 32 container/valid bits, IEEE-float subformat, exact count/mask, and checked `nBlockAlign`, `nAvgBytesPerSec`, and sample size. Compare complete `AM_MEDIA_TYPE` identity: major/subtype/formattype, fixed/temporal flags, sample size, null `pUnk`, format length and all format bytes.
3. Add checked helpers for sample-count addition, `sample_count * block_align`, DWORD/LONG narrowing, and allocator 3/2 growth. Do not alter stock `CreateMediaType`; its non-extensible Stereo behavior remains stock-only.
4. Add the files to the LAV project and command-line smoke build.

**Testing:**

- Compare every field and the complete format bytes for all seven candidates; explicitly assert strict Stereo is extensible.
- Reject zero mask, mask/count mismatch, wrong format/rate, invalid contract, overflow, and narrowing failure.
- A fake `QueryAccept` success may be described only as exact proposal acceptance, never as `STREAM_PROVEN`.

**Verification:** Run the strict-output test, script-structure test, the complete `release_lav_smokes.cmd`, and OpenJOC-enabled x64 build. Expected: all pass.

**Commit:** `feat(audio): build exact OpenJOC PCM media types`
<!-- END_TASK_1 -->

<!-- START_TASK_2 -->
### Task 2: Carry strict identity through postprocessing and queueing safely

**Verifies:** openjoc-lav-multichannel-output.AC4.4, openjoc-lav-multichannel-output.AC5.2, openjoc-lav-multichannel-output.AC5.3

**Files:**

- Modify: `D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\LAVAudio.h` (`// pattern: Imperative Shell`)
- Modify: `D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\LAVAudio.cpp` (`// pattern: Imperative Shell`)
- Modify: `D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\PostProcessor.cpp` (`// pattern: Imperative Shell`)
- Modify: `D:\Program\LAVFilters-OpenJOC\common\DSUtilLite\growarray.h` (`// pattern: Functional Core` checked container primitive)
- Extend/Test: `D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\OpenJocStrictOutputTests.cpp` (unit/integration)

**Implementation:**

1. RED tests must prove 7.1 and 5.1.2 never coalesce despite both having eight channels, strict and stock buffers never coalesce, mixer/layout options cannot change a strict buffer, and append/count overflow returns failure.
2. Add a non-owning `const LAVOpenJocOutputContract *` to `BufferDetails`; Phase 1 static storage supplies its lifetime. Set it only in `DecodeOpenJoc` after the full Phase 2 contract checks.
3. At the top of `PostProcess`, when the pointer is present, validate float32/48 kHz/native exact mask/non-planar/exact byte count and return immediately. Bypass mixer/resampler, standard-layout conformity, side/back replacement, current-layout retention, mono/6.1 expansion, volume-stat path, and sample-format fallback.
4. Make the contract pointer part of `QueueOutput` compatibility. Route flush/metadata/buffer/commit ordering through the executable queue transaction seam. A different pointer or strict/null transition must flush first; propagate flush failure. Use checked sample addition, propagate append failure without partially committing timestamps or counts, and clear the marker in `FlushOutput`.
5. In `GrowableArray`, check DWORD addition and byte multiplication, propagate `SetSize`/allocation HRESULT, preserve the old buffer on `realloc` failure, and stop returning unconditional `S_OK` from `Append`/`AppendZero`.
6. Mark every modified runtime-bearing file deterministically: the stateful LAV headers/sources are `// pattern: Imperative Shell`, while `growarray.h` and strict checked arithmetic are `// pattern: Functional Core`.

**Testing:**

- Same-contract merge succeeds; different-contract and strict/stock transitions flush exactly once and never mix bytes.
- Fake queue callbacks prove flush and append failures do not commit result counts/timestamps or consume the incoming buffer.
- All LAV mixing/conformity settings preserve the exact strict contract.
- Deterministic overflow and allocation-failure tests leave prior buffer contents and counts unchanged.
- Stock buffers still take the existing postprocessor path.

**Verification:** Run strict-output tests, GrowableArray behavior tests, the complete `release_lav_smokes.cmd`, OpenJOC x64 build, and a stock-control build. Expected: all pass.

**Commit:** `fix(audio): preserve strict OpenJOC buffer identity safely`
<!-- END_TASK_2 -->

<!-- START_TASK_3 -->
### Task 3: Deliver once, exactly, with no fallback or allocator substitution

**Verifies:** openjoc-lav-multichannel-output.AC3.3, openjoc-lav-multichannel-output.AC3.4, openjoc-lav-multichannel-output.AC4.3, openjoc-lav-multichannel-output.AC5.2, openjoc-lav-multichannel-output.AC5.3

**Files:**

- Modify: `D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\LAVAudio.cpp` (`ReconnectOutput`, strict acquisition/completion, `Deliver`, EOS/resync propagation)
- Extend: `D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\OpenJocStrictNegotiation.h/.cpp` (production callback orchestration)
- Extend/Test: `D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\OpenJocStrictOutputTests.cpp` (integration)

**Implementation:**

1. RED fake downstream accepts int16, 5.1-back, 7.1, Stereo/current layout but rejects the exact strict type. Require one exact proposal, no sample, and no output-pin type mutation. Add RED cases for a sample-attached substitute type and undersized sample capacity.
2. Add a strict branch before the generic fallback block. Validate the contract and checked byte count, then call the same executable orchestration seam used by fake downstream tests: exact `QueryAccept` -> checked reconnect -> sample acquisition/validation -> media-type commit -> delivery. Issue at most one exact proposal, convert `S_FALSE` to `VFW_E_TYPE_NOT_ACCEPTED`, preserve a downstream failure HRESULT, release acquired resources on every exit, and never enter the stock retry chain.
3. Check allocator growth arithmetic and actual capacity. After `GetDeliveryBuffer`, reject a sample-attached media type unless it exactly matches complete format bytes; reject `GetSize() < requiredBytes`. Only after every check succeeds may the code set media type/actual length or copy bytes.
4. Leave generic stock `GetDeliveryBuffer` type adoption and fallback order unchanged.

**Testing:**

- Trap test proves no float-to-int16, side-to-back, >8-to-7.1, Stereo, or current-layout fallback.
- Attached-type mismatch and undersized allocator fail before `SetActualDataLength` and `memcpy`.
- Exact/no-attached type with sufficient capacity succeeds.
- Failed acquisition that nevertheless returns an attached type or sample releases both resources.
- Strict failures produced while draining EOS/resync propagate even when delivery already cleared the queue marker; stock failure-ignoring behavior remains unchanged.
- Ordinary stock buffer control can still take the existing generic fallback.

**Verification:** Run strict tests, the complete `release_lav_smokes.cmd`, all prior smokes, OpenJOC-enabled build, stock-control build, then rebuild OpenJOC target as the phase-ending artifact.

**Commit:** `fix(audio): reject OpenJOC negotiation and allocator mismatches`
<!-- END_TASK_3 -->
<!-- END_SUBCOMPONENT_A -->
