# OpenJOC active goal and evidence boundary

OpenJOC is an independent clean-room implementation based on ETSI TS 103 420
V1.2.1, ETSI TS 102 366 V1.4.1, the official ETSI companion tables, and
public mathematical/audio-DSP literature. Cavern, other JOC decoder source,
decompiled Dolby binaries, and proprietary implementations are forbidden
sources.

The previous broad goal was too early. The evidence-backed completed boundary
is currently:

```text
raw E-AC-3 elementary stream
  + aligned base-channel PCM
  + independently parsed JOC/OAMD/EMDF
  -> renderer-independent ObjectScene
  + default-f32 object stems
  + optional explicit reference-f64 object stems
```

This is not yet a complete real-world Atmos decoder or speaker/binaural
renderer. In particular, the retained `debug/compatible_base.wav` is explicit
FFmpeg `pcm_f64le` compatible-base reference PCM, not a final render.

## Completed increment: input media and DEE containers

1. Audit status claims and keep `REQUIREMENTS_MATRIX.md`, `PROVENANCE.md`, and
   `IMPLEMENTATION_REPORT.md` aligned with executable evidence.
2. Classify input by file signature before codec parsing: raw EC3 versus ISO
   BMFF/M4A/MP4 versus unsupported input.
3. For one supported ISO BMFF E-AC-3 audio track, use FFmpeg/FFprobe only as
   external black-box container tools. Stream copy is required; no audio
   re-encoding is allowed. OpenJOC independently validates and parses the
   resulting E-AC-3, EMDF, JOC, and OAMD bytes.
4. Make `inspect` and `decode` share this boundary, preserve raw `.ec3`
   behavior, bound demux output, and return structured container-aware errors.
5. Cover raw/container detection, demux equivalence, missing/multiple/
   unsupported tracks, malformed containers, inspect, and decode integration.
6. Verify with `cargo fmt`, strict clippy, all-feature tests, and a release
   build. Commit this increment as a resumable change.

## Implemented increment: explicit wave output semantics

1. Keep reconstructed scene PCM in f64 internally and expose a checked wave
   sink supporting f32, explicit reference-f64, s24, and s16.
2. Make default CLI object output f32 and require `--reference-f64` for the
   reference representation.
3. Define integer clipping and dither as explicit policies, with tests for
   rejection, hard clipping, and deterministic seeded dither.
4. Keep the compatible base-channel debug WAV explicitly named and f64; it is
   not a speaker or binaural render.

## Implemented increment: renderer-independent trim retention

1. Preserve decoded trim snapshots, including warp/global/custom controls,
   balances, and per-object disable flags, without choosing a render algorithm.
2. Export trim state as a separate timed scene artifact and validate its
   object cardinality, timing, and finite numeric controls.

## Implemented increment: frame-local atomic scene staging

1. Stage per-frame object metadata, trim snapshots, and PCM validation before
   commit; do not clone previously accumulated object audio.
2. Preserve retry atomicity for both `SceneBuilder` and `PayloadDecoder` while
   retaining only bounded JOC state copies.

## Implemented increment: borrowed frame sinks

1. Add `PayloadDecoder::decode_frame_with`, which lends one committed
   `DecodedPayloadFrame` to a callback without transferring ownership of an
   accumulated frame list.
2. Route aligned and internal E-AC-3 CLI debug export through that callback so
   debug structures are written and dropped frame by frame.
3. Keep the remaining input, base-WAV, and accumulated-scene PCM retention
   explicitly open; this increment is not a claim of complete streaming scene
   assembly.

## Explicit open goals after the current increment

- Establish a user-supplied legal DEE real-vector lane without committing
  copyrighted programme bytes. It must prove nonzero JOC side information,
  nonzero reconstructed PCM, dynamic OAMD, multiple access units, state reuse,
  a moving object, and known stems or ADM-BWF ground truth.
- The currently supplied DEE M4A is a container/diagnostic fixture only: its
  `addbsi` complexity index is present, but every currently inspected
  frame-end `auxdatae` bit is zero and the validated frame-end extractor did
  not locate OAMD/JOC EMDF. The CLI's literal “EMDF profile absent” wording is
  bounded to those validated carrier paths. No separate metadata/JOC track or
  recognized box was found, but audio-block `skipfld` carriage has not been
  ruled out because full internal audio-block traversal fails before that lane
  is completely validated. Do not infer that `skipfld` is present, and do not
  count this as the real vector until all required payload and PCM evidence is
  available.
- Complete all-carrier EMDF integration, including bounded validation of
  audio-block `skipfld` carriage, remains an open goal independent of the
  completed frame-end carrier path.
- Compare FFmpeg base-channel PCM with `--internal-base` on that legal vector,
  recording channel order/count, delay, peak, RMS, and numerical error. The
  internal base decoder is not verified until this succeeds.
- Preserve trim state in `ObjectScene` and `metadata/trim_timeline.json` without
  imposing speaker or binaural rendering behavior. (Implemented; streaming
  staging remains open.)
- Replace accumulated-scene PCM cloning and whole-input/debug retention with
  frame-local atomic staging and streaming sinks. (Frame-local staging and the
  borrowed debug-frame sink are implemented; streaming input/base/object PCM
  sinks and the CLI retention audit remain open.)
- Keep codec and rendering boundaries separate. Later speaker rendering targets
  stereo, 5.1, 5.1.2, 7.1.4, and 9.1.6. Later binaural rendering targets
  selectable public SOFA HRTFs. Neither is a Dolby reference or normative
  standard HRTF.

## Required verification loop

Before any completion claim, run the full workspace formatting, strict clippy,
all-feature test, and release-build commands and record their results in
`IMPLEMENTATION_REPORT.md`. A passing synthetic/inactive-OAMD test proves only
the plumbing and zero-stem behavior; it is not evidence of nonzero real JOC
reconstruction.
