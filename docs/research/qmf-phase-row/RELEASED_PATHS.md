# Published OpenJOC playback route map

Verified 2026-10-05 via GitHub release metadata, exact-tag sources and successful release job logs. Source references below identify release builds and playback routing.

## Release identity and build

- Latest published stable release: [v0.18.0](https://github.com/chyinan/OpenJOC/releases/tag/v0.18.0), published 2026-09-24T18:28:46Z
- Annotated tag object 1c7603edf8996160706cd9d997406a0021c4f186 resolves to commit **e09d7b579fd765e211146d1b39138ae92d9e79e6**
- [Main release run](https://github.com/chyinan/OpenJOC/actions/runs/36038226842) and [LAV release run/job](https://github.com/chyinan/OpenJOC/actions/runs/36038225956/job/107763569793) both succeeded
- LAV asset openjoc-lav-0.18.0-windows-x64.zip: release API digest sha256:83eed27044e8fb8b283e9016fe4a5f280b075f794cf817eb8fa1a8718e495a00
- LAV workflow checks out OpenJOC release tag separately from build tooling and pins **chyinan/LAVFilters-OpenJOC 8f32aad51aea24602f2175316a241568be5fc4a1**; actual successful push run logs confirm both SHAs
- [LAV workflow](https://github.com/chyinan/OpenJOC/blob/e09d7b579fd765e211146d1b39138ae92d9e79e6/.github/workflows/lav-release.yml#L25-L83) builds CAPI with cargo build -p openjoc-capi --release --locked --jobs 1
- Actual logged compiler: **rustc 1.98.1 (48a229cea 2026-09-01)**; commit 48a229ceaefd4985c50990b14116b6d856af0985; host x86_64-pc-windows-msvc; LLVM 22.1.8
- No root Cargo profile overrides, tracked .cargo/config, toolchain pin, target-cpu or RUSTFLAGS override in the LAV build workflow. Declared Rust release build therefore uses Cargo defaults: opt-level=3, debug=false, codegen-units=16, incremental=false, lto=false, panic=unwind; lto=false permits thin local LTO and is not equivalent to lto=off. [Cargo profile reference](https://doc.rust-lang.org/cargo/reference/profiles.html#release). Unlogged runner-global Cargo config not independently excluded; capture cargo -vv and environment when reproducing
- [C++ build script](https://github.com/chyinan/OpenJOC/blob/e09d7b579fd765e211146d1b39138ae92d9e79e6/scripts/release_lav_msbuild.cmd#L13-L24): Release|x64, EnableOpenJOC=true, EnableOpenJOCSideBySide=true, builds LAVAudio. LAV vcxproj enables WholeProgramOptimization in release
- Pinned external LAV FFmpeg gitlink **599d3a140460e1b57c234fe064db5185fb76ee5b**; submodule URL https://gitea.1f0.de/LAV/FFmpeg.git. Workflow runs sh ./build_ffmpeg_msvc.sh x64 release. This is not the standalone OpenJOC FFmpeg package source/build
- [Root Cargo](https://github.com/chyinan/OpenJOC/blob/e09d7b579fd765e211146d1b39138ae92d9e79e6/Cargo.toml); API default feature embedded-builtin-hrtf. LAV CAPI does not use all-features

## Windows playback and policy routing

User must explicitly prefer the side-by-side LAV Audio Decoder (OpenJOC) in PotPlayer. Installing alone does not select it. [Released guide](https://github.com/chyinan/OpenJOC/blob/e09d7b579fd765e211146d1b39138ae92d9e79e6/docs/site/using/windows-lav-potplayer.md#L18-L56)

Route:
PotPlayer DirectShow graph → LAVAudio::ProcessBuffer → candidate check → positive JOC classification/admission → LAVOpenJocDecoder → dynamically loaded openjoc_capi.dll stream API → FfmpegDecoder compressed-stream bridge → lazy OpenJocSession → speaker renderer → optional direct binaural FIR → f64-to-f32 interleave → FFmpeg channel reorder → LAV exact-format delivery → downstream renderer/device.

- Candidate requires available OpenJOC, no bitstream context, no SPDIF subtype, AC3/EAC3 codec; ordinary non-JOC stays on stock LAV/FFmpeg. Passthrough bypasses OpenJOC. [Candidate](https://github.com/chyinan/LAVFilters-OpenJOC/blob/8f32aad51aea24602f2175316a241568be5fc4a1/decoder/LAVAudio/OpenJocCandidate.cpp#L10-L16)
- Shipping default is **Stereo**, calibrated dialnorm; not binaural. [Defaults](https://github.com/chyinan/LAVFilters-OpenJOC/blob/8f32aad51aea24602f2175316a241568be5fc4a1/decoder/LAVAudio/LAVAudio.cpp#L384-L389)
- Eight fixed 48 kHz float PCM output policies: Stereo, Binaural, 5.1, 7.1, 5.1.2, 5.1.4, 7.1.2, 7.1.4. 22.2 is a standalone/API research case, not a released LAV downstream policy
- [CreateDecoderForContract](https://github.com/chyinan/LAVFilters-OpenJOC/blob/8f32aad51aea24602f2175316a241568be5fc4a1/decoder/LAVAudio/OpenJocDecoder.cpp#L270-L353): Stereo → OPENJOC_RENDER_STEREO; Binaural → OPENJOC_RENDER_BINAURAL, virtual7.1.4 default (9.1.6 explicit experimental), D1 default/D2/custom SOFA, LFE excluded; other policies → OPENJOC_RENDER_SPEAKER with preset
- [CAPI stream constructor](https://github.com/chyinan/OpenJOC/blob/e09d7b579fd765e211146d1b39138ae92d9e79e6/crates/openjoc-capi/src/lib.rs#L1264-L1282) creates FfmpegDecoder; [bridge pump](https://github.com/chyinan/OpenJOC/blob/e09d7b579fd765e211146d1b39138ae92d9e79e6/crates/openjoc-ffmpeg/src/lib.rs#L803-L912) positively admits AU, constructs OpenJocSession and pushes packet
- Speaker renderer runs even for binaural, generating virtual-speaker PCM before HRTF; final linked gain and common-profile stereo shortcut are disabled for binaural at session construction. [Session constructor](https://github.com/chyinan/OpenJOC/blob/e09d7b579fd765e211146d1b39138ae92d9e79e6/crates/openjoc-api/src/lib.rs#L793-L832)
- Explicit Stereo can take admitted common-profile compatibility PCM downmix, rather than physical7.1.4 render followed by OS downmix; compare equivalent policy end to end

## Is optional FFT in published products?

Yes, it is compiled as library code/dependency, and explicitly reachable in the CLI. No, it is not the selected released LAV/OpenJocSession binaural backend.

- render crate unconditionally depends on rustfft6.4.1 and exports PartitionedBinauralRenderer; successful LAV build log shows Compiling rustfft v6.4.1. Whether final CAPI DLL retains unused machine code was not inspected
- [Released API BinauralState](https://github.com/chyinan/OpenJOC/blob/e09d7b579fd765e211146d1b39138ae92d9e79e6/crates/openjoc-api/src/lib.rs#L2048-L2114) stores/constructs BinauralRenderer; config descriptor explicitly says binaural_backend=direct. No backend/partition selector in CAPI/LAV policy
- [CLI parser](https://github.com/chyinan/OpenJOC/blob/e09d7b579fd765e211146d1b39138ae92d9e79e6/crates/openjoc-cli/src/main.rs#L1375-L1456) defaults Direct; --backend partitioned uses default P256, --partition-size N also selects partitioned
- [CLI constructor](https://github.com/chyinan/OpenJOC/blob/e09d7b579fd765e211146d1b39138ae92d9e79e6/crates/openjoc-cli/src/joc_render.rs#L2367-L2380) branches to direct or partitioned renderer. Thus optional-FFT microbench findings cannot establish common LAV playback speed

## Function-level profile boundaries

Use wall-clock/CPU sampling separately from stage instrumentation, exact capture and allocator builds. Constructor/preflight, first output, steady AUs, reset/discontinuity and drain must be distinct.

1. Adapter/chunk overhead: LAVOpenJocDecoder::Process and its Impl::FeedDecoder / CollectFrames / FinishDecoder, CAPI send_chunk, FfmpegDecoder::send_packet_inner/pump/reorder_frame. Measure full host CPU separately from API CPU
2. API complete-AU boundary: [OpenJocSession::push_packet](https://github.com/chyinan/OpenJOC/blob/e09d7b579fd765e211146d1b39138ae92d9e79e6/crates/openjoc-api/src/lib.rs#L886-L1065). Its broad decode stage includes core PCM, metadata parsing, JOC reconstruction and clones; cannot identify a DSP hotspot alone
3. Core PCM: JocAccessUnitPcmDecoder::decode_pcm_planes_with_policy → decode_audio_frame_pcm_with_policy_override_and_timing → AudioPcmSynthesizer::synthesize_internal → inverse_transform → inverse_long/inverse_short → inverse_complex, plus overlap_add. [Transform](https://github.com/chyinan/OpenJOC/blob/e09d7b579fd765e211146d1b39138ae92d9e79e6/crates/openjoc-eac3/src/transform.rs#L109-L163), [inner direct complex sum](https://github.com/chyinan/OpenJOC/blob/e09d7b579fd765e211146d1b39138ae92d9e79e6/crates/openjoc-eac3/src/transform.rs#L296-L306)
4. JOC: PayloadDecoder::decode_frame_with_profile_and_binding_profile → JocDecoder::decode_pcm_frame → ReferenceQmf64F64::analyze; decode_frame_inner → interpolate_matrix/reconstruct_objects → ReferenceQmf64F64::synthesize. [JOC analysis](https://github.com/chyinan/OpenJOC/blob/e09d7b579fd765e211146d1b39138ae92d9e79e6/crates/openjoc-joc/src/decoder.rs#L304-L350), [QMF direct f64](https://github.com/chyinan/OpenJOC/blob/e09d7b579fd765e211146d1b39138ae92d9e79e6/crates/openjoc-qmf/src/lib.rs#L95-L154)
5. Speaker: SpeakerRenderer::render_frame_aligned → timeline and delayed pending frame → render_aligned_block → calibration/BridgeControlAssembler::assemble_frame → JocSpatialBridge::render_coordinates → stereo compatibility downmix if policy/profile calls for it → composition/final linked gain. [Speaker stage](https://github.com/chyinan/OpenJOC/blob/e09d7b579fd765e211146d1b39138ae92d9e79e6/crates/openjoc-api/src/lib.rs#L1553-L1810)
6. Static binaural: emit_rendered → BinauralState::render → [BinauralRenderer::render_block](https://github.com/chyinan/OpenJOC/blob/e09d7b579fd765e211146d1b39138ae92d9e79e6/crates/openjoc-render/src/lib.rs#L1233-L1325). Separate validation, source lookup, inner direct FIR, history update, tail drain. Inner kernel is registered-source → sample offset → ascending tap; f64 left += then right +=, with current sample × gain or gain-scaled history
7. Output: OpenJocSession::to_pcm_frame f64→f32 and interleave; bridge channel reorder; LAV validation/copy; downstream delivery

## Release versus frozen gate source

Frozen gate baseline is 15aefe1baa7b40f37950df252b6dbf6179894d6d, newer than release.

Verified same released source blobs for EAC3 audio_block.rs/transform.rs/access_unit.rs, QMF lib.rs, JOC decoder.rs and scene payload_decoder.rs. Static BinauralRenderer implementation is unchanged; render lib only adds dynamic modules/exports/errors. JocSpatialBridge adds route_vectors accessor, with render_coordinates unchanged.

API and FFmpeg wrappers substantially differ due to post-release listener-orientation paths, extra state and custom-SOFA resampling. Full frozen-baseline API timings are not actual v0.18 timings, even when kernels match. Orientation is a separate post-release case. Built-in static D1/D2 is the appropriate released-route analogue.
