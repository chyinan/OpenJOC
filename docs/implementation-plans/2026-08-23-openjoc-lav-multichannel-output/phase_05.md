# OpenJOC-LAV Multichannel Output Implementation Plan — Phase 5

**Goal:** Produce repeatable native DirectShow evidence for exact connection, streaming, lifecycle, stock-path isolation, allocator boundaries, and performance.

**Architecture:** A standalone x64 harness privately activates the exact branch-built LAV Splitter and LAV Audio modules by resolving each module's `DllGetClassObject` and calling its `IClassFactory`; it never obtains either branch filter through registered `CoCreateInstance`. A strict capture sink proves full media-type equality, state transitions, samples, bytes, channel fingerprints, and no-fallback behavior; a renderer-moniker mode performs the same exact connection against a named real DirectShow renderer without interpreting its friendly name. The ordinary-E-AC-3 control is an independently built pristine start-HEAD tree, not the modified branch with OpenJOC disabled.

**Tech Stack:** C++17, DirectShow baseclasses, LAV Splitter Source, MSVC/v143, Python script tests, generated public EC-3/MP4 fixtures.

**Scope:** 6 phases from the original design; this file is phase 5 of 6.

**Codebase verified:** 2026-08-23 at OpenJOC `53d27ff5b8db379089ed5e2fde50bcea1632fbfb` plus design commit `04f64f7`, and LAV `b06ba2cbbd5c8806ca4423a8ff1527e4e2bd6a27`.

---

## Acceptance Criteria Coverage

### openjoc-lav-multichannel-output.AC3: DirectShow negotiation is exact and strict

- **openjoc-lav-multichannel-output.AC3.2 Success:** A layout is reported supported only after exact connection, exact `ConnectionMediaType`, Pause/Run and sample delivery in the named host/renderer environment.

### openjoc-lav-multichannel-output.AC4: Stock LAV behavior remains isolated

- **openjoc-lav-multichannel-output.AC4.1 Success:** Ordinary non-JOC E-AC-3 follows the existing decoder, postprocessor and delivery behavior under the same settings.
- **openjoc-lav-multichannel-output.AC4.2 Success:** Enabled E-AC-3 passthrough prevents OpenJOC decoder entry for every selected policy.

### openjoc-lav-multichannel-output.AC5: Lifecycle and memory remain safe at maximum admitted size

- **openjoc-lav-multichannel-output.AC5.1 Success:** Initial playback, forward/backward seek, flush/new segment, EOS, stop/reopen, graph rebuild and media-type renegotiation retain the selected layout without stale state.
- **openjoc-lav-multichannel-output.AC5.2 Success:** Frame, queue, allocator and delivery byte counts use checked multiplication/addition before allocation or narrowing.
- **openjoc-lav-multichannel-output.AC5.3 Failure:** Oversized sample/channel counts fail before copy, append, allocator growth or sample delivery.
- **openjoc-lav-multichannel-output.AC5.4 Success:** Stereo, 5.1 and the maximum admitted layout complete the performance run without unexplained underruns or unbounded memory growth.

---

<!-- START_SUBCOMPONENT_A (tasks 1-4) -->
<!-- START_TASK_1 -->
### Task 1: Commit the fixture, build, and harness entrypoint

**Verifies:** None — infrastructure prerequisite for the behavioral graph tests.

**Files:**

- Create: `D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\OpenJocDirectShowNegotiationSmoke.cpp`
- Create: `D:\Program\OpenJOC\scripts\test_lav_directshow_negotiation.cmd`
- Create/Test: `D:\Program\OpenJOC\scripts\tests\test_lav_directshow_negotiation_script.py` (unit)
- Modify/Test: `D:\Program\OpenJOC\crates\openjoc-ffmpeg\src\lib.rs` (test-only public-syntax fingerprint fixture exporter)
- Modify: `D:\Program\OpenJOC\scripts\generate-player-fixtures.sh`
- Modify/Test: `D:\Program\OpenJOC\scripts\tests\test_lav_release_notices.py` (unit)

**Implementation:**

1. RED Python tests require the new script/source, exact seven-argument usage, missing-argument exit 64, separate target/pristine binary paths, and generation of `joc.fingerprint.ec3` plus `joc.fingerprint.mp4`; expected failure is missing files/commands.
2. Preserve current single-AU fixtures and hashes. Extend the test-only exporter in `crates/openjoc-ffmpeg/src/lib.rs` to create a bounded multi-frame public-syntax probe with distinct five-bed mantissa codes and an asymmetric object-position sweep. Before wrapping the complete stream into seekable MP4, decode it through every representable policy and assert that each output channel's time-series fingerprint is stable and pairwise distinct. Fixture generation itself fails if any policy cannot distinguish every channel; there is no later `FIXTURE_INADEQUATE` escape hatch.
3. Define script arguments exactly as `VSDEVCMD TARGET_LAV_ROOT PRISTINE_LAV_ROOT OPENJOC_INCLUDE OPENJOC_CAPI FIXTURE_DIR OUTPUT_DIR`. Require `PRISTINE_LAV_ROOT` to identify the frozen LAV start commit `b06ba2cbbd5c8806ca4423a8ff1527e4e2bd6a27` (or the previously verified equivalent tree hash) before building it. Build target/pristine LAV Splitter and LAV Audio Release artifacts in disjoint directories, build baseclasses, and compile the standalone harness; link `strmbase.lib`, `strmiids.lib`, `ole32.lib`, `uuid.lib`, and `winmm.lib`.
4. Implement `PrivateComModule`: call `LoadLibraryEx` on an absolute staged `.ax`, canonicalize and record `GetModuleFileName(HMODULE)`, compute its SHA-256, resolve `DllGetClassObject`, obtain the normal published CLSID's `IClassFactory`, and call `CreateInstance`. Creating a normal CLSID from that private factory is the intended activation path; branch LAV filters must never use registered `CoCreateInstance`. Renderer-moniker binding remains normal system activation.
5. Stage the exact branch-built `LAVAudio.ax`, `LAVSplitter.ax`, every FFmpeg/private-assembly DLL, `libbluray.dll`, and supplied `openjoc_capi.dll` in an isolated runtime directory, plus a disjoint pristine-control runtime. Before evidence is accepted, enumerate the native harness process modules and require exactly one loaded module for each relevant staged basename; canonical absolute path and SHA-256 must match the staged manifest for both filters, `openjoc_capi.dll`, every loaded `*-lav-*.dll`, and `libbluray.dll`. A JSON hash or intended search path without this runtime assertion is insufficient. Do not use registered system copies.
6. Add a `--self-test` for private activation, complete media-type comparison, fingerprint uniqueness, and evidence-state transitions. Mark the stateful harness declarations/implementation `// pattern: Imperative Shell` and pure comparison/state/fingerprint helpers `// pattern: Functional Core`.

**Verification:** Missing arguments return 64, Python script tests pass, fingerprint generation succeeds for all seven policies, harness `--self-test` exits 0, both target and frozen-pristine LAV Audio/Splitter x64 Release builds succeed, and the complete `release_lav_smokes.cmd` runs after adding the harness-related command-line tests.

**Commit:** `test: add DirectShow negotiation harness entrypoint`
<!-- END_TASK_1 -->

<!-- START_TASK_2 -->
### Task 2: Prove exact controlled-sink streaming and no fallback

**Verifies:** openjoc-lav-multichannel-output.AC3.2, openjoc-lav-multichannel-output.AC5.2, openjoc-lav-multichannel-output.AC5.3

**Files:**

- Modify/Test: `D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\OpenJocDirectShowNegotiationSmoke.cpp` (e2e)

**Implementation:**

1. RED cases prove a legal mask alone, `QueryAccept` alone, or Pause/Run with zero samples cannot become PASS. Add a rejection trap that accepts all known fallbacks but rejects the exact target.
2. Privately create the real `CLAVSplitterSource` (published CLSID `{B98D13E7-55DB-4385-A33D-09FD1BA26338}`) and LAV Audio filter from the explicitly staged target modules through their own `DllGetClassObject` factories, assert both activated paths/hashes, load raw and MP4 through `IFileSourceFilter::Load`, connect its E-AC-3 pin to LAV Audio, set policy through `ILAVOpenJocSettings`, then call `ConnectDirect(decoderOut, sinkIn, &exactTarget)`.
3. Implement `StrictCaptureSink` recording `ReceiveConnection`, every `QueryAccept` proposal, both pin connection types, sample-attached types, allocator properties, samples/bytes, timestamps, flush/new-segment, and EOS.
4. A controlled-sink positive case requires requested type, output `ConnectionMediaType`, input `ConnectionMediaType`, and post-stream types to match GUIDs, flags, sample size, `cbFormat`, and every format byte; Pause/GetState(Paused), Run/GetState(Running), samples/bytes > 0, EOS, and no graph error must all succeed.
5. Compare capture bytes with a direct `LAVOpenJocDecoder` oracle. Revalidate that the generated probe's per-channel time-series hashes are pairwise distinct for the current policy, then require every captured channel fingerprint and interleaved byte sequence to equal the oracle. A non-distinguishing fixture makes fixture generation/test setup fail and cannot yield a pass or an ambiguous evidence row.
6. The rejection trap must first bootstrap a connected graph with a sink-accepted non-target PCM type. Only after the connection exists, select/feed a strict OpenJOC policy so `ReconnectOutput` proposes the one exact dynamic type. The sink rejects that exact type while being prepared to accept int16, 5.1-back, 7.1, Stereo, and the current bootstrap type. Require exactly one dynamic proposal, no second type, no delivered sample, unchanged preexisting `ConnectionMediaType` on both pins, and the exact recorded failure stage/HRESULT. Do not start this negative case with `ConnectDirect(..., &exactTarget)`, because that would merely fail initial connection and could not observe fallback behavior.
7. Add renderer-moniker binding (`MkParseDisplayName`/`BindToObject`) for Phase 6; never derive semantics from its friendly name.

**Testing:** Run raw `joc.fingerprint.ec3` and `joc.fingerprint.mp4` for all seven policies against strict capture. All exact controlled-sink cases pass; deliberate post-bootstrap dynamic rejection fails exactly once without fallback. Label the environment as controlled sink, not real-renderer support.

**Commit:** `test: prove exact OpenJOC DirectShow streaming`
<!-- END_TASK_2 -->

<!-- START_TASK_3 -->
### Task 3: Cover stock E-AC-3, passthrough, and graph lifecycle

**Verifies:** openjoc-lav-multichannel-output.AC4.1, openjoc-lav-multichannel-output.AC4.2, openjoc-lav-multichannel-output.AC5.1

**Files:**

- Modify/Test: `D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\OpenJocDirectShowNegotiationSmoke.cpp` (e2e)
- Modify: `D:\Program\OpenJOC\scripts\test_lav_directshow_negotiation.cmd`

**Implementation:**

1. Add RED matrix cases for ordinary E-AC-3 under every policy, E-AC-3 passthrough under every policy, and layout retention across forward/back seek, flush/new segment, EOS, stop/reopen, graph rebuild, and policy/media-type renegotiation.
2. Use the separately built, privately activated pristine start-HEAD control from Task 1. Do not substitute the modified branch with `EnableOpenJOC=false`, because shared delivery/postprocessor/container changes would remain. Ordinary E-AC-3 target runs must match the pristine filter's output type/PCM/sample/EOS behavior and remain on stock admission/postprocess/delivery paths; evidence records target and pristine module paths/hashes independently.
3. Enable `Bitstream_EAC3` for every policy and prove OpenJOC stream-input bytes remain zero while bitstream media type/bytes match the independently built pristine control.
4. Run raw `joc.multi.ec3` and MP4 `joc.multi.mp4`: seek to 25% and 75%, observe BeginFlush/EndFlush/NewSegment, complete EOS, Stop→seek zero→Run, destroy/rebuild graph, and re-read exact `ConnectionMediaType` at each boundary.
5. Use test-only volatile registry overrides for filter recreation and restore them with RAII. A policy change must recreate the decoder and renegotiate the exact new type.

**Verification:** All seven lifecycle rows pass for raw and MP4; ordinary and passthrough results match the frozen pristine control; private module-path/hash assertions and registry restoration are verified. Run the complete `release_lav_smokes.cmd`. Because target and pristine build roots are disjoint, the final target artifact is never overwritten.

**Commit:** `test: cover OpenJOC lifecycle and stock isolation`
<!-- END_TASK_3 -->

<!-- START_TASK_4 -->
### Task 4: Exercise allocator boundaries and performance high-water marks

**Verifies:** openjoc-lav-multichannel-output.AC5.2, openjoc-lav-multichannel-output.AC5.3, openjoc-lav-multichannel-output.AC5.4

**Files:**

- Modify/Test: `D:\Program\LAVFilters-OpenJOC\decoder\LAVAudio\OpenJocDirectShowNegotiationSmoke.cpp` (e2e/performance)

**Implementation:**

1. RED: an allocator returning `requiredBytes-1` must fail before copy; exact capacity must succeed; queue/sample overflow inputs must fail deterministically.
2. Reuse Phase 3 checked helpers. Record requested/actual allocator capacity, sample `GetSize`, actual length, checked byte calculations, and allocator high-water mark for maximum validation candidate 7.1.4.
3. Warm up, then run at least 128 `joc.multi.ec3` graph cycles for Stereo, 5.1, and 7.1.4. Record elapsed time, samples, bytes, timestamp continuity, EOS, allocator high-water, and working set.
4. Pass only when repeated sample/byte counts match, no delivery/EOS/timestamp error occurs, and post-warm-up working set does not grow linearly. This is a performance result, not an endpoint-support inference.

**Verification:** Boundary cases and all three performance rows pass; logs contain numeric high-water data and no support claim based on names or masks. Run the complete `release_lav_smokes.cmd` after the final harness additions.

**Commit:** `test: stress OpenJOC allocator and graph performance`
<!-- END_TASK_4 -->
<!-- END_SUBCOMPONENT_A -->
