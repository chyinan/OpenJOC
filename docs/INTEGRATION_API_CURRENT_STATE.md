# OpenJOC integration API: current state audit

This document records the OpenJOC 0.7 integration boundary at the release
source state. The release commit is the authoritative source revision.

## Existing pipeline

The repository already has a bounded, sequential raw E-AC-3 input path:

1. `openjoc-container::RawEac3AccessUnitReader` frames complete syncframes and
   groups an independent I0 plus optional dependent D0 into one access unit.
2. `openjoc-eac3::JocAccessUnitPcmDecoder` owns E-AC-3 inheritance, transform
   overlap, DRC/base policy, channel topology, LFE separation, and channel-major
   Base PCM. Its state is committed only after the complete AU succeeds.
3. `openjoc-eac3` extracts and validates one JOC/OAMD EMDF carrier per AU.
4. `openjoc-scene::PayloadDecoder` parses OAMD/JOC, owns
   `JocDecoderState`, QMF analysis/reconstruction, metadata inheritance, and
   the streaming scene summary. `PayloadDecoder::finish_streaming_with_reconstruction_tail`
   explicitly returns the 577-sample reconstruction tail.
5. `openjoc-joc::ReconstructionOutputTimeline` aligns Base and raw
   ReconstructionBasis PCM. It retains only fixed-latency state and does not
   conceal QMF latency by changing logical sample ranges.
6. `openjoc-scene::JocSpatialBridge` and `BridgeControlAssembler` consume the
   aligned Base/RB frame and current OAMD metadata to render semantic speaker
   layouts. The 2.0 path keeps Base stereo downmix separate from RB spatial
   projection.
7. The CLI owns file/container loading, progress, WAV/CAF writers, topology
   sidecars, and diagnostic reporting. These are not appropriate library
   responsibilities.

## Ownership and boundaries

| Concern | Existing owner | Integration decision |
| --- | --- | --- |
| Input | CLI/container reader | Public session accepts one borrowed complete JOC AU; no file ownership |
| AU boundary | `RawEac3AccessUnitReader`, `group_access_units` | One packet = I0 + optional D0, no arbitrary byte fragmentation promise |
| Base PCM | `JocAccessUnitPcmDecoder` | Owned transiently for one AU, then copied only into bounded timeline state |
| JOC/OAMD state | `PayloadDecoder` / `JocDecoderState` | Session-owned, serially accessed |
| QMF/Base-RB delay | `ReconstructionOutputTimeline` | 577 samples for binaural; speaker output reports 609 samples including the admitted 32-sample final linked speaker stage; logical PTS is not shifted |
| Speaker output | `JocSpatialBridge` + canonical `SpeakerLayoutPreset` | Session-owned automatic-control renderer |
| Stereo | Existing E-AC-3 downmix metadata and policy equations | Shared policy mapping in the high-level API |
| SOFA | `openjoc-sofa::parse_simple_free_field_hrir` | Memory-buffer configuration supported; no path retained |
| Output | CLI WAV/CAF writers | API returns owned interleaved `f32` PCM frames |
| Progress/diagnostics | CLI terminal and JSON reports | API returns statuses/errors; no unconditional printing |
| Flush/drain | Existing decoder/timeline tail APIs | `drain`, `flush`, and `reset` are explicit session operations |

## Important limitations found in the audit

- The internal public renderer is still located in the CLI crate and contains
  file-output orchestration. The new `openjoc-api` crate therefore implements
  the headless automatic-control path over the same lower-level decoder,
  timeline, bridge, and SOFA primitives. The legacy CLI path remains available
  for topology sidecars, detailed profiling, and WAV/CAF output.
- The first public packet contract intentionally does not accept arbitrary
  byte fragmentation or a packet containing multiple AUs. A framework adapter
  should use its demuxer to hand OpenJOC one complete AU at a time.
- The first API exposes direct SOFA convolution. Partitioned binaural
  scheduling remains a CLI/renderer option until a later ABI extension.
- Preroll is carried as an explicit packet flag and is decoded to prime state;
  a later player adapter may add a discard-output policy without changing the
  packet boundary.

These constraints are explicit rather than hidden behind filesystem or CLI
assumptions.
