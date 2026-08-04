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

## Implemented increment: multi-fixture real-DEE carrier census and first-failure diagnosis

The local command `openjoc census [MANIFEST] -o DIR`, or the equivalent
`OPENJOC_REAL_FIXTURE_MANIFEST` environment variable, processes multiple
user-supplied raw EC3/M4A/MP4 descriptors without committing programme bytes.
It verifies labels and optional SHA-256 values, uses the completed input-media
boundary, and writes deterministic machine-readable and human-readable
reports. Reports explicitly separate validated carrier paths from unresolved
paths and include a comparison table, payload-ID distributions, skip-field
reachability, and structured first-failure fields with bit offsets. The
grouped-mantissa correction now lets the parse-only walker reach all six blocks
on each current fixture; no malformed mantissa or unresolved block remains.

The current external corpus is recorded by stable label, hash, and size in
`PROVENANCE.md` and `IMPLEMENTATION_REPORT.md`:

| label | bytes | source SHA-256 | frames/access units | skip observed/examined/unresolved | state |
| --- | ---: | --- | ---: | ---: | --- |
| `forever_friends` | 32,138,978 | `67c10f65642f11713f8495026a37cf26fd1f901e9a343d2e3acf5ee879584896` | 7,773/7,773 | 7,773/46,638/0 | `emdf_profile_incomplete` |
| `hitchcock` | 29,370,578 | `0075ade8f801e38a4f98637d9d9a8099771ea1edd0bb66bd829aa2c0faa3e425` | 7,146/7,146 | 7,146/42,876/0 | `emdf_profile_incomplete` |
| `grand_escape` | 44,175,378 | `b7a320d2ff14a27e64b9e0262f2092b31145bc217100a2f987d174fef0ef2956` | 10,599/10,599 | 10,599/63,594/0 | `emdf_profile_incomplete` |
| `brainrot` | 16,283,910 | `2808eecb80353141135000ab499815219a86770e5b02e912dc971dd01e86afd7` | 3,910/3,910 | 3,910/23,460/0 | `emdf_profile_incomplete` |

All four currently show `addbsi` complexity 16 and zero frame-end
`auxdatae`. The parse-only boundary reaches every six-block prefix and passes
each exact declared skip-field range to Annex H classification as a bounded
diagnostic candidate. Every fixture has one candidate per access unit that
parses with payload IDs 11, 14, 2, and 1, but ID 11 fails the Table 56
configuration requirement (`codecdatae=0`, `payload_frame_aligned=0`). TS 102
366 calls `skipfld` dummy bytes and TS 103 420 does not expressly designate the
field as a JOC carrier, so this is not a normative carriage conclusion. The
state is therefore `emdf_profile_incomplete`: it is not a legal nonzero
JOC/OAMD acceptance vector, and it is not a claim about unvalidated carrier
locations or nonzero reconstruction.

### Current bounded carrier rule

The current normative audit is limited to TS 102 366 V1.4.1 p.44 (`skiple`,
the 9-bit `skipl` count, and dummy `skipfld` bytes), p.117 (`skipflde`), and
p.124 (the exact order and `skipl × 8` data range), TS 103 420 V1.2.1 pp.68-69
(Tables 55-56, payload IDs 11/14, `addbsi`, and placement), and TS 102 366
Annex H pp.204-209 (EMDF synchronization and container bounds). The walker
retains the frame-relative and elementary-stream absolute bit offsets and
declared length. Annex H parsing starts only at bit zero of the exact extracted
range: no sliding syncword search, cross-field concatenation, implicit padding,
or multiple-container interpretation is used. A non-sync range is non-EMDF;
sync-start bounded syntax failure is a malformed candidate; a complete
container with undeclared trailing bytes is a trailing-data candidate.

TS 102 366 calls `skipfld` dummy data, while TS 103 420 does not expressly state
that it carries JOC EMDF. Accordingly, the skip-field path is an implemented
diagnostic candidate classification, not proof of normative carriage. A
complete profile must remain within one candidate container, satisfy all Table
55/56 restrictions, use same-frame `addbsi`, and obey last-dependent placement;
IDs 11 and 14 are never merged across carriers.

## Implemented increment: normative grouped mantissa traversal

TS 102 366 V1.4.1 clause 6.3.5 requires bap 1/2/4 packed groups to survive
exponent-set boundaries and interleaved BAP values. OpenJOC now carries that
state across conventional channel, coupling, and LFE syntax within each audio
block, while resetting it at the block boundary. The complete decoder and the
parse-only carrier walker use the same bounded state; no legal code domain was
expanded and no arbitrary byte scan was added. A focused regression covers a
group split across separate exponent-set calls with an interleaved bap=3 code.

This correction moves all four current fixtures from `carrier_unresolved` to
complete six-block traversal: every skip field is reached, malformed mantissa
count is zero, and unresolved block count is zero. Commit
`d900ef13c3c3977d6f0cd861d00293d002f00006` then feeds
each exact skip-field range to the bounded Annex H classifier and records the
incomplete profile state. Legal nonzero JOC/OAMD acceptance, resolved
skip-field carriage semantics, complete legal-carrier coverage, and
internal-base fidelity remain open.

## Controlled Logic vector result

The first controlled production attempt is now complete through the strict
profile gate. A private Logic Pro 12.3 project uses deterministic 48 kHz PCM24
sources, a stereo bed, one moving 997 Hz object, 30 explicit automation events,
and no creative processing. The accepted ADM export is exactly four seconds;
its object channel is sample-identical to the known source and its ADM metadata
contains 197 position blocks. Private sources, project media, ADM, DD+ output,
manifests, reports, and screenshots remain outside the repository.

The final DD+ Atmos export yields 126 E-AC-3 access units. OpenJOC reaches all
six audio-block prefixes in every access unit and parses one exact bounded
`skipfld` Annex H candidate with IDs 11/14/2/1. The new census configuration
inventory proves that IDs 11 and 14 both carry `codecdatae=0`, while ID 11 is
also not frame aligned. Strict TS 103 420 Table 56 validation therefore rejects
all 126 profiles. The validator is unchanged; OAMD/JOC parsing,
reconstruction, continuity, and internal-base comparison are deliberately not
entered after this blocker. Two release census runs are byte-identical.

This result changes the next evidence need: another authorized encoder/version
or an authoritative carriage/profile clarification is required. A vendor
divergence is observed; commercial intent is not established and must not be
assumed.

## Explicit open goals after the current increment

- Establish a user-supplied legal DEE real-vector lane without committing
  copyrighted programme bytes. It must prove nonzero JOC side information,
  nonzero reconstructed PCM, dynamic OAMD, multiple access units, state reuse,
  a moving object, and known stems or ADM-BWF ground truth.
- The currently supplied DEE M4A corpus is a container/diagnostic fixture set:
  every fixture signals `addbsi` complexity 16, every frame-end `auxdatae` bit
  is zero, and each reached skip-field range parses as a bounded EMDF candidate
  with IDs 11, 14, 2, and 1. TS 102 366 calls these bytes dummy data and TS
  103 420 does not expressly assign them as a JOC carrier. The ID-11 Table 56
  configuration is invalid, so no complete profile enters OAMD/JOC extraction.
  The CLI's literal “EMDF profile absent” wording is compatibility text bounded
  to profile validation; census output names the carrier kind and
  incomplete-profile error. No separate metadata/JOC track or recognized box
  was found. This is not evidence about unvalidated legal carrier paths and is
  not real-vector acceptance.
- Complete all-carrier EMDF coverage beyond the two currently examined bounded
  paths, resolve whether `skipfld` is an authorized JOC carrier, establish legal
  nonzero JOC/OAMD acceptance, and verify nonzero reconstruction remain open
  goals.
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
