# Capabilities

This is the canonical current capability status. Real-media acceptance confirms
that Logic imports OpenJOC reconstructed ADM and that a Logic-authored re-export
is accepted by Dolby Encoding Engine. Direct ingestion of the byte-exact
OpenJOC-authored file is not claimed. Historical release state belongs to the
[CHANGELOG](../CHANGELOG.md) and [archive](archive/README.md); dated engineering
evidence belongs to the research and provenance records.

## Status vocabulary

The explicit `render-scene` workflow is admitted for static caller-bound mono
sources, direct or uniform-partitioned binaural convolution, and the strict
J5R8 SimpleFreeFieldHRIR/CDF-1 SOFA subset. It is not a JOC or authored-object
renderer.

- `ADMITTED` — supported within the stated contract.
- `ADMITTED_WITH_SCOPE` — supported only within an explicit bounded scope.
- `DIAGNOSTIC_ONLY` — emitted for analysis, not a semantic product claim.
- `PARTIAL` — an explicit subset is supported; the full capability is not.
- `UNRESOLVED` — evidence is insufficient for an implementation claim.
- `NOT_ADMITTED` — deliberately outside the current contract.
- `EXPECTED_STRICT_REJECTION` — rejection is the correct result for the input.

## Current matrix

| Area | Capability | Status | Evidence boundary | Important scope |
|---|---|---|---|---|
| Input | Raw E-AC-3 parsing and bounded streaming | `ADMITTED` | Controlled carriers and public syntax | Full real-stream codec fidelity remains scoped |
| Input | Seekable ordinary MP4/M4A with one E-AC-3 track | `ADMITTED_WITH_SCOPE` | Container and sample-cursor regressions | Uses `ffprobe`/`ffmpeg`; non-seekable and fragmented MP4 are not admitted |
| Base E-AC-3 | Ordinary base decode and channel/LFE labels | `ADMITTED_WITH_SCOPE` | Public syntax, topology, TDAC and state tests | Not a speaker renderer; cross-decoder fidelity remains incomplete |
| Coding tools | Coupling, SPX, AHT, rematrix | `ADMITTED_WITH_SCOPE` | Normative/public-syntax numerical and state harnesses | Some real-producer activation and full PCM fidelity remain open |
| Substreams | One I0 plus optional D0 assembly | `ADMITTED_WITH_SCOPE` | Chanmap, atomic assembly and reset tests | Multiple dependents are not admitted |
| OAMD | Normative metadata prefix and metadata-only timeline | `ADMITTED_WITH_SCOPE` | Normative parser and controlled state tests | Complete vendor trim continuation is unavailable |
| OAMD | `ETSI_STRICT` profile | `ADMITTED` | Published ETSI validation rules | Observed raw `warp=3` is `ReservedWarpMode` and is rejected |
| OAMD | `OBSERVED_VENDOR_COMPAT` profile | `PARTIAL` | Explicit observed-signaling acceptance and deviation evidence | Continuation is retained opaquely; no vendor semantic interpretation |
| Scene | Metadata-only `ObjectScene` | `ADMITTED` | Schema, timeline, assembly and atomicity tests | `ResolvedWithinCarrier` is reserved for the exact decoded-JOC/OAMD profile below; authored identity remains unresolved |
| Reconstruction | `ReconstructionBasis` rows | `ADMITTED_WITH_SCOPE` | Public JOC output-object contract, deterministic numerical/continuity tests, and sanitized scene vectors | Rows are decoded JOC output-object PCM within the admitted carrier profile; they are never authored-object PCM or source stems |
| Binding | Decoded JOC PCM ↔ OAMD dynamic metadata | `ADMITTED_WITH_SCOPE` | Sanitized clean-room contract, typed ordinal gate, positive/negative vectors, actual-count scene assembly tests | Exactly 15 JOC objects + no bed + one leading Base LFE + no ISF + 15 dynamic + 16 total; no heuristic or authored identity claim |
| Binding | Reconstructed dynamic ADM Objects from decoded JOC Objects | `ADMITTED_WITH_SCOPE` | Bounded metadata preflight, typed binding profile, deterministic position-block export, strict/best-effort policy tests, independent ADM validation | Generated Objects carry decoded OAMD movement within the exact ordinary or exact observed raw3-compatible profile; structural/decoded-scene correctness does not guarantee native JOC perceptual localization equivalence; unsupported properties/profiles remain neutral or reject |
| Binding | Unvalidated JOC binding profiles | `UNRESOLVED` | Explicit admission rejection and report reason | Bed, ISF, alternate-LFE, count/order-mismatched, unknown compatibility deviations, incomplete-Base-LFE, and other unvalidated profiles are not dynamically bound |
| Components | Typed decoded-component manifest | `ADMITTED` | `diagnostics/components.json` separates Base, Base LFE, indexed RB coordinates, RcLfe boundary, and the carrier-local binding state | PCM-free layout; no authored-object identity |
| JOC bridge | Codec-domain streaming reconstruction input and readiness gate | `ADMITTED_WITH_SCOPE` | `JocSpatialFrameBridge`, absolute `SampleRange`, finite/dimension checks, synthetic linearity/partition tests, readiness census | `T(t)` remains unresolved; no authored-object semantic binding |
| JOC bridge | Opt-in codec-coordinate spatial projection and accumulation | `ADMITTED_WITH_SCOPE` | `JocSpatialBridge`, topology binding, spatial projection, Q32 gain scheduling, linear accumulation, raw3 preservation, and partition tests | Experimental maturity; bridge/operator state remains unresolved and is separate from scoped decoded-object ADM binding; official runtime oracle not independently confirmed |
| JOC rendering | Real supported E-AC-3 JOC to preset speaker WAV/CAF workflow | `ADMITTED_WITH_SCOPE` | `render-joc` decoder/bridge/output integration tests, automatic bridge-control assembly tests, 2.0 topology and Lo/Ro/Lt/Rt numeric tests, preset geometry, topology/count/LFE/order/mask/semantic-CAF checks, synthetic arbitrary-layout and 24-channel bridge tests | Experimental 2.0 plus 5.1, 5.1.2, 5.1.4, 7.1, 7.1.2, 7.1.4, 7.1.6, 9.1, 9.1.2, 9.1.4, 9.1.6, and 22.2 paths; all use one generic full-XYZ/N-layer data-driven projector and separate LFE ownership; 7.1.6 and the 9.1 family are semantic CAF-only, while 22.2 writes explicit unmasked 24-channel WAV or richer CAF metadata; `--topology` is optional and remains a complete explicit override/test input; generic library layouts remain supported; no authored-object binding or vendor-fidelity claim |
| JOC rendering | Real supported E-AC-3 JOC to stereo generic/user-SOFA binaural WAV | `ADMITTED_WITH_SCOPE` | Virtual-speaker integration tests, bundled SADIE II resource round-trip/coverage, exact HRIR identity, delay-aligned spherical interpolation, azimuth-wrap and sparse-data tests, sample-rate preflight, direct-reference equivalence, partitioned equivalence, LFE policy, reset, and tail tests | Uses the default virtual field `7.1.4` unless `--virtual-layout` is supplied; `--binaural` uses the offline bundled SADIE II D1 resource and `--sofa` overrides it; selected HRTF data must provide exact or safely interpolatable directions for every non-LFE virtual speaker; CLI defaults LFE to `exclude`, with explicit `equal-power-dual-mono` available; output is always two-channel OpenJOC speaker virtualization, not a vendor/direct-object binaural claim |
| JOC layout engine | Canonical preset and arbitrary user-defined geometry up to 64 output channels | `ADMITTED_WITH_SCOPE` | Public `SpeakerLayout`/`SpatialLayout` plus `JocSpatialBridge`, versioned custom JSON, irregular 3/4/7/11/13/17/31-channel geometry, validation rejects, arbitrary-order and 24-channel tests | Preset names remain the ordinary CLI path; `--layout-file` is advanced; custom WAV is intentionally unmasked and CAF carries coordinates; downstream host/device geometry remains separate |
| Integration | Headless Rust `OpenJocSession` / `OpenJocConfig` | `ADMITTED_WITH_SCOPE` | Session lifecycle, complete-AU packet validation, owned interleaved `f32` PCM, timestamps, reset/flush/drain, multi-instance, and latency tests | One serial caller per session; arbitrary byte fragmentation, multi-AU pushes, file I/O, and CLI parsing are outside the packet API |
| Integration | Versioned C ABI 1.4 | `ADMITTED_WITH_SCOPE` | Public `openjoc.h`, preserved complete-AU decoder, in-memory custom speaker geometry descriptor, decode-free bounded classifier, bounded packet-stream handle, fragmentation/multi-AU tests, lazy positive JOC admission, semantic layout/fingerprint access, numeric statuses, `struct_size` fallback, C11/C++ compilation, instance-owned errors, and panic containment | Experimental ABI; compatibility may evolve during OpenJOC 0.x |
| Integration | Windows DirectShow / LAV Filters OpenJOC Audio Decoder | `ADMITTED_WITH_SCOPE` | Public `LAVFilters-OpenJOC` fork/tag; strict raw/MP4 DirectShow capture proves exact media types and sample delivery for Stereo, 5.1, 7.1, 5.1.2, 5.1.4, 7.1.2, and 7.1.4; endpoint probes preserve VB-Audio WaveOut success, VB-Audio DirectSound rejection, and Realtek DirectSound success; positive JOC admission, ordinary E-AC-3 isolation, passthrough precedence, seek/EOS/reopen/policy switching, side-by-side install, uninstall, and stock LAV rollback | One exact 48 kHz `WAVEFORMATEXTENSIBLE` IEEE-float proposal per explicit policy, with no fallback; `AUTO_NOT_RELIABLE`; Stereo default; no endpoint-name inference, Bass Management, or physical-subwoofer routing; physical multichannel hardware remains unavailable/unverified |
| Integration | Native FFmpeg `libopenjoc` source wrapper | `ADMITTED_WITH_SCOPE` | Reproducible FFmpeg 9.0.1/master patch, pkg-config detection, receive-frame callback, explicit named selection, stock-E-AC-3 safety, raw/MP4/Matroska private-media tests, seek/flush/drain/multi-instance tests, and full-program binaural/7.1.4/22.2 parity | Requires a patched custom FFmpeg build with dynamic OpenJOC; the project provides source patches/builds but does not claim upstream FFmpeg support |
| Integration | mpv player patchset and OpenJOC Player Bundle | `ADMITTED_WITH_SCOPE` | Clean mpv 0.41.0/master patch application, positive bounded JOC classifier, ordinary-E-AC-3 isolation, explicit decoder override, binaural transport, physical 7.1.4/9.1.6/22.2 channel-map paths, passthrough separation, and native macOS/Linux/Windows package qualification | Project-provided custom mpv/FFmpeg builds; bundles are not official upstream distributions; Linux/Windows physical speaker hardware remains outside the qualification boundary |
| Interchange | Reconstructed RIFF/RF64 ADM BWF export | `ADMITTED_WITH_SCOPE` | Production bounded-memory compressed-media preflight/direct writer, conditional decoded-JOC dynamic metadata path, independent seek-based RIFF/RF64/ds64/Atmos-profile XML/CHNA/public-DBMD validator, legal room-centric 5.1 LFE transport bed, transactional cleanup, signed 24-bit PCM, mapping report, scale high-watermarks, and strict unsupported-profile rejection | Exact admitted profile exports generated dynamic Objects with position blocks; unsupported profiles retain neutral best-effort Objects or reject strict; original ADM identity/master and direct DEE ingest remain unclaimed |
| Ecosystem | OpenJOC SDK, custom FFmpeg bundle, and feature-enabled GStreamer plugin pack | `ADMITTED_WITH_SCOPE` | Extracted package manifests/checksums, license/private-path scans, C consumer build/run, FFmpeg/ffprobe smoke, `gst-inspect-1.0`, and target runtime baselines | Packages are project-provided; FFmpeg is not upstream, GStreamer runtime ABI must match the recorded baseline, and C ABI 1.4 remains experimental |
| Diagnostics | Public synthetic JOC fixture and `openjoc self-test` | `ADMITTED_WITH_SCOPE` | Project-owned fixture generation, positive classifier, decode, 7.1.4 speaker, built-in HRTF/binaural, and ADM health report | Fixture is generated on demand; absent optional fixture checks report `NOT_APPLICABLE`, not silent success |
| Output policy | Dialnorm Default/Digital/Analog and optional sample-peak normalization | `ADMITTED_WITH_SCOPE` | Numeric dialnorm mapping, program-level ownership, final linked speaker headroom, normalization constant-gain equivalence, WAV/CAF output tests | Default is recommended calibrated decoder behavior; Analog is advanced unity-gain compatibility policy; normalization is offline sample-peak only, not LUFS or true-peak |
| Semantics | Original authored-object identity recovery | `NOT_ADMITTED` | Clean-room contract explicitly separates carrier-local decoded binding from authored identity | Generated names, numbering, UIDs, hierarchy, authoring metadata, and source-stem identity are not recovered |
| Semantics | Original ADM master recovery | `NOT_ADMITTED` | Lossy JOC input and reconstructed export report | `original_adm_master_recovered=false`; the original master is not recoverable from this representation |
| Semantics | Verified authored PCM or authored-object renderer fidelity | `NOT_ADMITTED` | Carrier-local binding does not identify source stems or reproduce a proprietary renderer | `SemanticBindingState::ResolvedWithinCarrier` never upgrades to authored identity or renderer parity |
| Rendering | Explicit-scene stereo and general 2D speaker renderer | `ADMITTED_WITH_SCOPE` | `openjoc-render` independent stereo/VBAP oracle, layout, trajectory, continuity and block-partition tests | Caller-supplied mono sources only; arbitrary validated horizontal layouts, adjacent-pair panning, and absolute-sample position/gain trajectories; no JOC bridge, HRTF, room model, or Dolby renderer-fidelity claim |
| Rendering | Explicit-scene 3D speaker topology, VBAP triplet renderer, and sample-accurate trajectories | `ADMITTED_WITH_SCOPE` | `openjoc-render` checked 3×3 public-math and independent great-circle oracle, tetrahedron/octahedron/partial/ambiguity, continuity, and partition tests | Caller supplies speaker order and triplets explicitly; shortest great-circle segments and linear gain only; no automatic triangulation, Delaunay/hull inference, distance, Doppler, listener orientation, LFE, HRTF, JOC bridge, or authored-object identity |
| Rendering | Static explicit-source binaural direct-FIR renderer | `ADMITTED_WITH_SCOPE` | `openjoc-render` exact-direction HRIR/provider validation, independent full-convolution oracle, ear-order, history, tail, reset, failure-atomicity, and input/tail partition tests | Caller supplies finite equal-length HRIR taps and exact static directions; fixed listener orientation, direct causal f64 FIR reference path; SOFA resolution/interpolation remains at the `openjoc-sofa` boundary; no moving source, room, distance, HRTF database, or JOC bridge |
| Rendering | Static explicit-source uniform partitioned binaural convolution | `ADMITTED_WITH_SCOPE` | `openjoc-render` fixed-FFT backend, Direct FIR equivalence, multiple partition sizes/sources, partial-input, exact-tail and lifecycle regressions | Caller selects one fixed power-of-two `P`; FFT size is `2P`, input is exact `P`-sample partitions plus one final partial, scheduling latency is explicitly `P` samples; no adaptive selection, nonuniform partitions, SOFA, interpolation, moving sources, or JOC bridge |
| Rendering | Strict `SimpleFreeFieldHRIR` SOFA ingestion and bounded HRIR interpolation | `ADMITTED_WITH_SCOPE` | `openjoc-sofa` synthetic CDF-1 fixture, coordinate/ear/delay/malformed-file tests, exact identity, spherical segment/triangle interpolation, delay/ITD, azimuth wrap, finite-result and sparse-coverage tests, direct and partitioned construction integration | Local read-only NetCDF classic CDF-1 subset; SOFA convention versions 1.0–1.2, exactly two receivers, spherical degree/degree/metre sources, integer sample delays; no HDF5/NetCDF-4, resampling, downloads, writing, or universal-coverage claim; interpolation fails closed outside the measured local spherical domain |
| Rendering | Authored-object-bound `ObjectScene` or renderer fidelity | `NOT_ADMITTED` | Scoped decoded-object binding does not identify authored sources or reproduce a proprietary renderer | No authored-object identity, binaural parity, or Dolby renderer-fidelity claim |
| Release | OpenJOC 0.13.0 platform assets with existing LAV integration | `ADMITTED_WITH_SCOPE` | GitHub Actions source/version checks, native platform quality gates, bundle verification, C ABI artifact checks, aggregate checksum verification, decoded-JOC dynamic ADM regression coverage, and the existing layered DirectShow/LAV transport and endpoint evidence | Workflow targets macOS arm64, Windows x86_64, and GNU/Linux x86_64; the Windows DirectShow/LAV subset remains limited to the seven explicit fixed policies from v0.12.0; physical multichannel hardware and automatic semantic negotiation are not claimed; direct DEE ingest of byte-exact OpenJOC ADM remains unsupported/unclaimed; reconstructed ADM native-renderer perceptual equivalence is not guaranteed |

The matrix deliberately separates production status from evidence class. A
numerically valid reconstruction row is not an authored object, and a real
carrier accepted by the vendor profile is not proof that ETSI strict semantics
are wrong.

## User-visible entry points

```text
openjoc inspect FILE
openjoc decode FILE -o DIR [--internal-base] [--streaming]
openjoc decode-payload --downmix FILE --joc FILE --oamd FILE -o DIR
openjoc diagnose-tools FILE --vector-id ID --json OUTPUT
openjoc census [MANIFEST] -o DIR
openjoc diagnose-oamd FILE [OPTIONS]
openjoc render-joc FILE [--topology TOPOLOGY.json] [--layout LAYOUT | --layout-file LAYOUT.json | --binaural [--sofa HRTF.sofa] [--virtual-layout LAYOUT]] --output OUTPUT.wav|OUTPUT.caf [--downmix auto|loro|ltrt] [--lfe-policy exclude|equal-power-dual-mono]
openjoc --version
```

The CLI emits structured failures, never silently downgrades the selected
validation profile, and names diagnostic outputs as reconstruction rows rather
than authored-object stems. Decode output directories are create-once
destinations, and stable machine-readable manifests identify their schema with
`openjoc.*.v1` markers.
