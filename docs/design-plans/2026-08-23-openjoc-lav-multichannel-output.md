# OpenJOC-LAV multichannel speaker output design

## Summary

OpenJOC-LAV adds an OpenJOC-only output policy backed by immutable layout contracts. Each contract binds a preset to its exact channel count, PCM order, FFmpeg layout and Windows speaker mask. Confirmed JOC buffers carry this semantic identity through processing and delivery, while ordinary E-AC-3 and passthrough remain on stock LAV paths. These contracts describe logical PCM channels: each listed layout has one logical LFE channel, regardless of how many physical subwoofers downstream hardware drives.

Delivery offers one exact float32/48 kHz `WAVEFORMATEXTENSIBLE` type and fails if downstream changes or rejects it; it never negotiates an alternative layout or format. A valid mask makes a layout representable, not supported. Support requires exact connection, matching `ConnectionMediaType`, graph execution and delivered samples in the named host/renderer environment.

## Definition of Done

OpenJOC-LAV can render confirmed JOC to explicitly selected, truthful logical PCM speaker layouts instead of only fixed Stereo. At least Stereo, 5.1 and 7.1 must pass exact DirectShow media-type negotiation and streaming tests; height layouts are supported only when the actual target host and renderer accept the exact semantic format.

Ordinary E-AC-3 and passthrough remain on their existing stock paths. The implementation does not infer layouts from carrier channel count, endpoint names, consumer speaker notation, physical subwoofer count or an ambiguous channel count. It does not add bass management, fabricate channel masks, silently fall back to another layout or use host-specific hacks.

## Acceptance Criteria

### openjoc-lav-multichannel-output.AC1: Output policy is explicit and stable

- **openjoc-lav-multichannel-output.AC1.1 Success:** A new filter defaults to Stereo and produces the same OpenJOC configuration and two-channel float output as the released path.
- **openjoc-lav-multichannel-output.AC1.2 Success:** Each admitted manual preset configures the public OpenJOC ABI with `OPENJOC_RENDER_SPEAKER` and the exact built-in preset name.
- **openjoc-lav-multichannel-output.AC1.3 Success:** Changing the policy recreates the OpenJOC stream decoder before subsequent frames are rendered.
- **openjoc-lav-multichannel-output.AC1.4 Failure:** Carrier channel count, endpoint/product display name, physical subwoofer count and filename never select or alter the render target.
- **openjoc-lav-multichannel-output.AC1.5 Failure:** Auto is not exposed or documented as supported without standards-based semantic preference evidence across stereo, 5.1 and one height-capable downstream.

### openjoc-lav-multichannel-output.AC2: Every candidate preserves canonical logical semantics

- **openjoc-lav-multichannel-output.AC2.1 Success:** Stereo, 5.1, 7.1, 5.1.2, 5.1.4, 7.1.2 and 7.1.4 map to the exact OpenJOC order, count and Windows mask recorded in the canonical layout table.
- **openjoc-lav-multichannel-output.AC2.2 Success:** PCM interleave order equals ascending set-bit WAVEFORMATEXTENSIBLE order, so admitted layouts require no silent reorder.
- **openjoc-lav-multichannel-output.AC2.3 Failure:** A layout without an exact canonical Windows mask is excluded; zero masks, reserved bits and count-only defaults are rejected.
- **openjoc-lav-multichannel-output.AC2.4 Failure:** A consumer `.2` subwoofer notation never creates a second logical LFE; physical subwoofer routing remains downstream.

### openjoc-lav-multichannel-output.AC3: DirectShow negotiation is exact and strict

- **openjoc-lav-multichannel-output.AC3.1 Success:** Every candidate media type is float32, 48 kHz, `WAVE_FORMAT_EXTENSIBLE`, with exact channels, valid bits, subformat, mask, block alignment, average byte rate and sample size.
- **openjoc-lav-multichannel-output.AC3.2 Success:** A layout is reported supported only after exact connection, exact `ConnectionMediaType`, Pause/Run and sample delivery in the named host/renderer environment.
- **openjoc-lav-multichannel-output.AC3.3 Failure:** Exact rejection returns a recorded failure and never falls back to int16, another 5.1 variant, 7.1, Stereo or the currently connected layout.
- **openjoc-lav-multichannel-output.AC3.4 Failure:** `QueryAccept`, `EnumMediaTypes`, a legal mask, endpoint properties or a channel count alone never produce a PASS claim.

### openjoc-lav-multichannel-output.AC4: Stock LAV behavior remains isolated

- **openjoc-lav-multichannel-output.AC4.1 Success:** Ordinary non-JOC E-AC-3 follows the existing decoder, postprocessor and delivery behavior under the same settings.
- **openjoc-lav-multichannel-output.AC4.2 Success:** Enabled E-AC-3 passthrough prevents OpenJOC decoder entry for every selected policy.
- **openjoc-lav-multichannel-output.AC4.3 Failure:** OpenJOC policy settings do not affect stock input media-type selection or generic fallback behavior.
- **openjoc-lav-multichannel-output.AC4.4 Failure:** Stock LAV mixing does not replace or duplicate OpenJOC speaker rendering.

### openjoc-lav-multichannel-output.AC5: Lifecycle and memory remain safe at maximum admitted size

- **openjoc-lav-multichannel-output.AC5.1 Success:** Initial playback, forward/backward seek, flush/new segment, EOS, stop/reopen, graph rebuild and media-type renegotiation retain the selected layout without stale state.
- **openjoc-lav-multichannel-output.AC5.2 Success:** Frame, queue, allocator and delivery byte counts use checked multiplication/addition before allocation or narrowing.
- **openjoc-lav-multichannel-output.AC5.3 Failure:** Oversized sample/channel counts fail before copy, append, allocator growth or sample delivery.
- **openjoc-lav-multichannel-output.AC5.4 Success:** Stereo, 5.1 and the maximum admitted layout complete the performance run without unexplained underruns or unbounded memory growth.

### openjoc-lav-multichannel-output.AC6: Settings and evidence are honest

- **openjoc-lav-multichannel-output.AC6.1 Success:** The existing property page exposes only Stereo and presets admitted by the shipped validation evidence, under the isolated OpenJOC registry namespace.
- **openjoc-lav-multichannel-output.AC6.2 Success:** Programmatic settings use an OpenJOC-specific interface without changing the stock `ILAVAudioSettings` ABI.
- **openjoc-lav-multichannel-output.AC6.3 Success:** The final matrix distinguishes `STREAM_PROVEN`, `UNSUPPORTED` and `UNVERIFIED` and records the exact failure stage/HRESULT where applicable.
- **openjoc-lav-multichannel-output.AC6.4 Failure:** Documentation never claims automatic physical-device adaptation or physical speaker playback without corresponding evidence.

## Glossary

- **Admitted preset/layout:** A candidate permitted in the shipped settings after its canonical contract and required validation evidence are established.
- **Canonical logical layout contract:** The immutable mapping between a preset name, logical channel count and order, FFmpeg channel layout and Windows speaker mask.
- **Carrier channel count:** The number of channels in the source transport. It does not identify the intended rendered speaker geometry.
- **Confirmed JOC:** Input positively identified as JOC and routed through the OpenJOC-specific decoding path.
- **Consumer `.2` subwoofer notation:** A consumer label advertising two subwoofers, such as 5.2. It does not establish a second logical LFE channel in the PCM format and is distinct from the height suffix in 5.1.2.
- **DirectShow exact negotiation:** Connecting with the requested media type unchanged, without converters or fallback, then confirming the connected type and successful streaming.
- **Logical LFE:** A semantic low-frequency-effects channel represented by an LFE bit in the media format. It is not a count of physical subwoofers.
- **Physical subwoofer routing:** Downstream hardware or renderer mapping of a logical LFE channel to one or more physical subwoofers.
- **`ConnectionMediaType`:** The media type actually established between connected DirectShow pins.
- **`QueryAccept` / `EnumMediaTypes`:** DirectShow capability probes that can indicate possible acceptance or enumerate candidates, but do not prove connection or streaming.
- **Representable:** Expressible with an exact, valid standard channel order and Windows speaker mask.
- **Renderer:** The downstream DirectShow component that consumes audio samples and sends them toward an audio endpoint.
- **Source-as-Output control condition:** The fixed PotPlayer graph configuration under which PotPlayer support evidence is collected.
- **Strict OpenJOC buffer:** Confirmed OpenJOC output marked so processing and delivery preserve its selected semantic layout and prohibit substitution or fallback.
- **`STREAM_PROVEN`:** Exact negotiation and sample delivery succeeded in the recorded host/renderer environment.
- **`UNSUPPORTED`:** Testing produced a measured rejection or changed the requested format.
- **`UNVERIFIED`:** The layout was not exercised sufficiently to establish support or rejection.
- **`WAVEFORMATEXTENSIBLE`:** The Windows audio format structure that carries PCM parameters plus a subformat and speaker-position mask.
- **Windows speaker mask:** A bitmask assigning each logical channel to a standard Windows speaker position; ascending set-bit order defines PCM interleave order.

## Architecture

Use a narrow strict lane for confirmed OpenJOC output. The stock LAV path stays unchanged.

An OpenJOC-specific output policy owns one of these values:

- `Stereo`
- `Preset(5.1)`
- `Preset(7.1)`
- `Preset(5.1.2)`
- `Preset(5.1.4)`
- `Preset(7.1.2)`
- `Preset(7.1.4)`

Auto is omitted because DirectShow exposes accepted media types, not one unambiguous preferred semantic layout. Equal channel counts can represent different geometries. Endpoint display names and physical speaker properties do not resolve that ambiguity.

The selected policy configures the OpenJOC C ABI. Stereo retains `OPENJOC_RENDER_STEREO`. A preset uses `OPENJOC_RENDER_SPEAKER` and the canonical built-in name. A policy change destroys and recreates the decoder; seek and flush continue using the existing reset path.

The LAV bridge maps the policy to one immutable contract containing preset name, channel count, OpenJOC order, FFmpeg custom/native channel layout and Windows speaker mask. The bridge does not reconstruct that contract from a returned frame count. Confirmed OpenJOC buffers carry a strict-output marker through postprocessing and queuing.

For a strict buffer, stock operations that can remix, conform, replace, suppress or substitute its layout are bypassed. Sample framing and safe format conversion that preserve the exact contract may remain. Delivery creates one float32/48 kHz media type and asks downstream to accept that exact type. Failure returns as failure; no generic LAV fallback runs for that buffer.

Production negotiation and test evidence are separate. Production uses the normal DirectShow delivery contract. The validation harness attempts an exact connection without converters, reads `ConnectionMediaType`, runs the graph and verifies sample delivery. PotPlayer support is recorded only from PotPlayer under the established Source-as-Output control condition.

## Logical layout contracts

| Preset | Count | PCM/WAVEFORMATEXTENSIBLE order | Mask |
|---|---:|---|---:|
| Stereo | 2 | FL FR | `0x00000003` |
| 5.1 | 6 | FL FR FC LFE Ls Rs | `0x0000060f` |
| 7.1 | 8 | FL FR FC LFE Lb Rb Ls Rs | `0x0000063f` |
| 5.1.2 | 8 | FL FR FC LFE Ls Rs TFL TFR | `0x0000560f` |
| 5.1.4 | 10 | FL FR FC LFE Ls Rs TFL TFR TBL TBR | `0x0002d60f` |
| 7.1.2 | 10 | FL FR FC LFE Lb Rb Ls Rs TFL TFR | `0x0000563f` |
| 7.1.4 | 12 | FL FR FC LFE Lb Rb Ls Rs TFL TFR TBL TBR | `0x0002d63f` |

Each contract has one logical LFE because its PCM/media format defines one LFE bit. Multiple downstream physical subwoofers are outside this contract. OpenJOC's separate 22.2 renderer model may retain multiple semantic LFE channels, but it is not a DirectShow preset unless a truthful media format exists and is accepted.

## Existing patterns

- `crates/openjoc-scene/src/speaker_layouts.rs` already owns canonical geometry, order and masks. LAV consumes these semantics rather than adding renderer logic.
- `crates/openjoc-capi/include/openjoc.h` already exposes built-in speaker presets through the public configuration ABI.
- `decoder/LAVAudio/OpenJocDecoder.cpp` already isolates dynamic C-ABI loading and decoder lifetime.
- `decoder/LAVAudio/LAVAudio.cpp` already separates confirmed JOC from ordinary E-AC-3 and checks passthrough before OpenJOC admission.
- `decoder/LAVAudio/AudioSettingsProp.cpp` already uses static combo tables for layout settings.
- `Software\\LAV\\Audio\\OpenJOC` already isolates OpenJOC settings from stock LAV.

The design diverges from generic LAV delivery only for strict OpenJOC buffers. Generic fallbacks remain valuable for ordinary codec output, but they contradict an explicitly selected semantic OpenJOC target.

## Implementation phases

<!-- START_PHASE_1 -->
### Phase 1: Canonical policy contract and tests

**Goal:** Define one testable preset table and output-policy contract without changing runtime output.

**Components:**
- LAV-side preset contract under `decoder/LAVAudio/`.
- Exact order/mask tests in the LAV smoke-test project.
- Existing OpenJOC canonical tests in `crates/openjoc-scene/tests/speaker_layouts.rs`.

**Dependencies:** Current OpenJOC core and audited LAV `openjoc-main` head.

**Done when:** Tests prove every candidate's name, count, order and mask and reject unknown/unmasked layouts. Covers `openjoc-lav-multichannel-output.AC1.4` and `openjoc-lav-multichannel-output.AC2.1` through `AC2.4`.
<!-- END_PHASE_1 -->

<!-- START_PHASE_2 -->
### Phase 2: Decoder configuration and semantic frame handoff

**Goal:** Render Stereo or the selected built-in speaker preset and preserve its semantic identity through `DecodeOpenJoc`.

**Components:**
- `decoder/LAVAudio/OpenJocDecoder.h/.cpp` decoder policy and recreation.
- `decoder/LAVAudio/LAVAudio.h/.cpp` policy state and exact FFmpeg channel layout construction.
- Decoder smoke tests for 2, 6, 8, 10 and 12 channels.

**Dependencies:** Phase 1.

**Done when:** Red/green tests prove ABI configuration, exact returned layout and safe policy changes. Covers `openjoc-lav-multichannel-output.AC1.1` through `AC1.3` and `openjoc-lav-multichannel-output.AC5.3`.
<!-- END_PHASE_2 -->

<!-- START_PHASE_3 -->
### Phase 3: Strict postprocessing and delivery

**Goal:** Preserve the selected semantic contract and reject exact negotiation failure without fallback.

**Components:**
- Strict marker and checked sizing in `decoder/LAVAudio/LAVAudio.h/.cpp`.
- Narrow OpenJOC bypass in `decoder/LAVAudio/PostProcessor.cpp` if tests prove it is required.
- Exact `WAVEFORMATEXTENSIBLE` construction and fake downstream pin tests.
- Checked append/allocation changes in the smallest shared utility scope required.

**Dependencies:** Phase 2.

**Done when:** Exact-media-type tests pass for all candidates; deliberate downstream rejection fails without mutation; stock buffers retain generic fallback behavior. Covers `openjoc-lav-multichannel-output.AC3.1`, `AC3.3` through `AC3.4`, `AC4.3` through `AC4.4` and `AC5.2` through `AC5.3`.
<!-- END_PHASE_3 -->

<!-- START_PHASE_4 -->
### Phase 4: Isolated settings surface

**Goal:** Persist and expose admitted OpenJOC output presets without changing the stock settings ABI.

**Components:**
- OpenJOC-specific settings interface under `include/`.
- Settings persistence in `decoder/LAVAudio/LAVAudio.h/.cpp`.
- Combo box resources and handling in `AudioSettingsProp.*`, `LAVAudio.rc` and `resource.h`.
- Settings round-trip and default tests.

**Dependencies:** Phase 3.

**Done when:** Stereo remains the default, values round-trip in the isolated namespace, and unsupported/unverified presets are not presented as shipped support. Covers `openjoc-lav-multichannel-output.AC1.1` and `openjoc-lav-multichannel-output.AC6.1` through `AC6.2`.
<!-- END_PHASE_4 -->

<!-- START_PHASE_5 -->
### Phase 5: Graph, lifecycle and stock-path regression harness

**Goal:** Produce repeatable automated evidence for exact negotiation, streaming, resets and isolation.

**Components:**
- Committed DirectShow sink/graph harness under the existing Windows smoke-test structure.
- Raw and MP4 JOC fixtures configured through the existing local fixture mechanism.
- Ordinary E-AC-3 and passthrough control cases.
- Seek, flush, EOS, reopen, allocator-boundary and performance cases.

**Dependencies:** Phases 3 and 4.

**Done when:** Exact connection and `ConnectionMediaType` match, the graph delivers samples, lifecycle cases pass, and stock controls remain unchanged. Covers `openjoc-lav-multichannel-output.AC3.2`, `openjoc-lav-multichannel-output.AC4.1` through `AC4.2` and `openjoc-lav-multichannel-output.AC5.1` through `AC5.4`.
<!-- END_PHASE_5 -->

<!-- START_PHASE_6 -->
### Phase 6: Real-host evidence and documentation

**Goal:** Decide the shipped support matrix from observed host/renderer behavior and document only proven claims.

**Components:**
- PotPlayer Source-as-Output runs for every candidate.
- Evidence records containing host, renderer, endpoint ID, requested/connected types and HRESULTs.
- Current OpenJOC-LAV documentation and claim-regression checks.

**Dependencies:** Phase 5.

**Done when:** Every row is `STREAM_PROVEN`, `UNSUPPORTED` or `UNVERIFIED`; at least 5.1 and 7.1 are proven for success; Auto and physical-hardware claims match evidence; docs list only admitted layouts. Covers `openjoc-lav-multichannel-output.AC3.2` and `openjoc-lav-multichannel-output.AC6.3` through `AC6.4`.
<!-- END_PHASE_6 -->

## Additional considerations

**Representable is not supported.** A legal standard mask starts a test; it does not finish one. A row becomes supported only in the exact named environment after streaming succeeds.

**Unsupported differs from unverified.** A measured rejection or format mutation is `UNSUPPORTED`. A layout that could not be exercised is `UNVERIFIED` and must not appear as supported.

**Validation access differs from shipped support.** Test builds may select every representable candidate through the OpenJOC-specific programmatic settings interface so PotPlayer and graph tests can exercise it. The final user-facing combo lists a row as supported only after the required evidence exists; unverified rows are not silently presented as production capability.

**No release automation.** Work stays on `codex/openjoc-lav-multichannel-output`. This task does not tag, publish, alter v0.11 assets or merge master.
