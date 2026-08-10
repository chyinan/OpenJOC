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

For the controlled Logic corpus, the current evidence is narrower than this
architectural target: the normative OAMD prefix and two spatial code fields
are now independently identified, but complete OAMD/timeline semantics and
end-to-end JOC acceptance remain open. The section `J1R7A` below is the
current real-vector boundary; older dated sections preserve their historical
state rather than claiming that boundary was already closed.

## Current interoperability boundary: two explicit JOC profiles

The parser, validator, and decoder are separate layers. Parsing retains the
complete original EMDF container and payload configuration. Validation then
selects one of two explicit profiles:

- `ETSI_STRICT`: published TS 103 420 Table 55/56 requirements remain
  normative; `codecdatae=1`, frame alignment, placement, and all conditional
  controls are enforced. A violation is a failure with evidence.
- `DOLBY_VENDOR_COMPAT`: accepts only the stable Logic/Dolby signaling pattern
  observed in controlled production vectors. It preserves every original bit
  and reports each deviation as `accepted_with_deviation`; it is not a claim
  of ETSI compliance.

The decoder consumes a validated representation and contains no hidden
compatibility normalization. `inspect` reports both profiles. `decode` uses
`--validation-profile etsi-strict|dolby-vendor-compat`, defaulting to strict;
the caller-defined OAMD cardinality can be supplied explicitly with
`--trim-config-count N`.
External fixture manifests can declare expected profile results, so the
controlled Logic vector and future Dolby Encoding Engine vectors remain
regression-gated without committing private media.

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
profile gate and the explicit vendor-compatible validation gate. A private
Logic Pro 12.3 project uses deterministic 48 kHz PCM24
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
all 126 profiles with seven recorded deviations per access unit. The explicit
vendor profile accepts the same exact pattern with those deviations preserved.
The raw stream reaches the OAMD decoder boundary under vendor compatibility.
With explicit trim-count candidates, the first downstream failure is the
reserved OAMD warp mode 3; no count or compatibility fallback is guessed.
Object reconstruction, continuity, and internal-base comparison remain open.
Two release census runs are byte-identical.

This result changes the next evidence need: another authorized encoder/version
or an authoritative carriage/profile clarification is required. A vendor
divergence is observed; commercial intent is not established and must not be
assumed.

## Bit-exact OAMD entry evidence (current forensic boundary)

The private Logic raw EC-3 and its MP4 were traced across all 126 access units
with `openjoc diagnose-oamd --all-access-units --trim-config-count 1`. The
reports preserve original bytes and name each offset's coordinate system. MP4
packet `pos,size` mapping closes against the demuxed stream, yielding sample
indices 0..125 and exact original-file bit positions. Every AU closes one
bounded EMDF container with payload IDs 11, 14, 2, and 1; payload 11 remains
536 bits and its 9-bit configuration is repeated rather than inherited.

The OAMD payload is 536 bits with `object_count=16`, two top-level elements
(ID 1 then ID 2), and the trim element at OAMD bits `[525,533)`. After the
element's discard bit, the warp field is OAMD bits `[526,528)` and its raw
value is 3 in every observation. The configured normative parser returns
`ReservedWarpMode{code: 3}` for every tested explicit trim count; no value is
remapped and no offset is injected. The first payload-11 body transition is
visible at AU 15, while the OAMD entry geometry and warp value stay fixed.

The trace initially placed the warp field at the element body start; a focused
synthetic fixture caught and corrected that diagnostic-coordinate mistake. The
regression now proves that raw reserved value 3 is retained while strict OAMD
validation still fails. This is conclusion C: one Logic export is not enough
to introduce a Dolby compatibility syntax rule. OAMD object-scene decoding,
object PCM, ADM waveform comparison, and internal-base fidelity remain blocked
at this exact boundary.

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
  configuration is invalid under ETSI_STRICT; vendor compatibility may accept
  only an explicitly observed pattern with deviations. No complete strict
  profile enters OAMD/JOC extraction.
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

## Round-2 decision boundary (2026-08-05)

The controlled Logic vector now has a reproducible AU timing and differential
package.  AU 15 (`0.480 s`) is the first payload-11 change; the warp field is
raw `3` at OAMD `[526,528)` in every AU, independently confirmed by a small
bit oracle and direct byte masking.  The ADM object timeline has 197 Cartesian
blocks, including an update at `0.480 s` and a position boundary at `0.500 s`.
These are alignment observations only.

Hypotheses 0/1/2 are retained only in the diagnostic report.  All three have
the same bounded top-level closure and none reaches a unique object/update/
position interpretation, so OpenJOC does not select one.  `ETSI_STRICT`
continues to reject `ReservedWarpMode{code: 3}`; `DOLBY_VENDOR_COMPAT` has not
been extended for warp semantics.  OAMD timeline generation, JOC
reconstruction, nonzero object PCM, and ADM fidelity remain unverified.

Before any completion claim, run the full workspace formatting, strict clippy,
all-feature test, and release-build commands and record their results in
`IMPLEMENTATION_REPORT.md`. A passing synthetic/inactive-OAMD test proves only
the plumbing and zero-stem behavior; it is not evidence of nonzero real JOC
reconstruction.

## Current controlled-corpus decision (2026-08-05)

The private Logic corpus now includes A static-centre, B requested single-jump,
C requested linear-ramp, D existing mixed motion, E no dynamic object, and F
two objects. All six exports have 126 AUs at 48 kHz/1,536 samples. A/E have
one unique payload-11 body; B/C/D/F have 63. B/C are not canonical single-
variable vectors: their ADM and payload reports prove that D's mixed
automation was retained in the copy. They remain useful evidence of the UI
copy limitation, not proof of jump/ramp semantics.

All six vectors report warp raw `3` in 126/126 AUs, including static A and
no-dynamic-object E. The independent bit oracle, formal entry trace,
diagnostic trace, and direct byte mask agree on OAMD-relative `[526,528)`.
The three semantic hypotheses 0/1/2 each close only bounded syntax and remain
non-unique. This is therefore the unresolved-unknown outcome: no parser offset
bug is demonstrated, but no vendor meaning is proven either. Strict behavior
remains `ReservedWarpMode { raw: 3 }`; no vendor warp compatibility profile
extension, remap, magic offset, or hidden trim behavior is allowed.

The ADM BWF remains an external oracle. Object names, block counts, Cartesian
coordinates, jump/interpolation attributes, gains, and times are preserved in
the private timeline report. Because the normative OAMD object grammar is not
entered, coordinate conversion, timeline equivalence, object PCM, JOC
reconstruction, and fidelity are explicitly open.

The next actionable blocker is to obtain a genuinely canonical single-jump
and linear-ramp Logic export (or an additional authorized encoder/version),
then repeat the same independent-oracle and ADM comparison without weakening
ETSI validation. The CLI forensic writer now refuses existing report targets
unless explicit `--force` is supplied, preserving evidence history.

## Round-4 vendor opaque boundary (2026-08-05)

This increment does not decide the meaning of `warp=3`. `ETSI_STRICT` remains
unchanged. An explicit `DOLBY_VENDOR_COMPAT` path can now retain a complete
declared element-2 trim body opaquely, with the raw value, exact ranges, hash,
formal first error, and deviation code preserved. It is not trim decoding,
warp remapping, a default trim, or a complete OAMD acceptance.

On private A/E/D raw and MP4 carriers, element 1 formally parses across all
126 AUs (16 objects, 15 dynamic, one block and 16 updates per AU) and payload
14/JOC formally parses 126/126 (five channels, 15 output objects, full mode,
900 codewords/AU, nonzero symbols). The next concrete blocker is the explicit
cross-chain object-count mismatch: `JOC declares 15 objects but OAMD declares
16`. No real object PCM, complete OAMD timeline, ObjectScene, or ADM fidelity
claim is made. The private non-overwriting batch is
`OpenJOC-Private/reports/runs/2026-08-05T0358Z_vendor-opaque-2f5de17`.
The follow-up B/C/F raw+MP4 regression repeats 126/126 opaque acceptances,
raw warp `3:126`, normalized carrier equality, and 126/126 formal payload-14
parses; B/C remain non-canonical automation copies.

## Round-3 differential refresh (2026-08-05)

This refresh branch is `codex/logic-warp-differential-corpus`, starting from
`952b052d61e23e5b7c7d96d37b41a01f090424b7`. Computer Use reopened Logic Pro
12.3 and the private canonical-B copy, but the unsaved editing experiment was
discarded before exit. Consequently no new B/C export is treated as canonical;
the earlier B/C copies remain non-canonical and are not proof of single-jump
or linear-ramp semantics.

The reproducible private refresh is
`OpenJOC-Private/reports/runs/2026-08-05T1042Z_logic-warp-evidence_952b052`.
It reruns raw and MP4 diagnostics and independent ADM extraction for A-F
without overwriting prior forensic or census output. All 12 carrier reports
have 126 AUs at exactly 0.032 s/AU. A/E have one payload-11 body; B/C/D/F
have 63. The first changed payload body remains AU 14 -> 15 (AU 15 = 0.480 s),
and raw/MP4 normalized traces are equal.

The four-way boundary evidence remains exact and unchanged: formal trace,
diagnostic trace, independent cursor-free oracle, and direct byte masking all
report `[526,528)`, `11`, and integer `3`; all bounded element/payload ranges
close. Hypotheses 0/1/2 remain diagnostic-only and non-unique. ADM is used
only as an external timing/position oracle; the object grammar is not entered.
Strict behavior remains `ReservedWarpMode { raw: 3 }`, and no vendor warp
compatibility rule is permitted until a canonical independent vector or
authoritative specification evidence makes one interpretation unique.

## Current programme-cardinality boundary (2026-08-05)

The former generic `JOC count == OAMD count` check was too broad. The current
orchestration keeps these relationships distinct:

```text
addbsi complexity  == total OAMD programme entries
JOC output rows     == OAMD dynamic slots
ObjectScene entries == total OAMD programme entries
```

`ProgrammeLayout` is derived from the parsed OAMD anchors. Across private
Logic A-F, the evidence is `OAMD[0] = Speaker(RcLfe)` followed by 15 dynamic
anchors. The explicit binding is `RcLfe -> BaseLfe(channel 0)` and dynamic
slot `i -> JOC row i` for `i=0..14`. This is not a generic N-versus-N-minus-1
allowance: ordinary beds, ISF, multiple/misordered LFE, duplicate rows,
missing LFE PCM, unequal frame lengths, and cardinality mismatches fail with
typed errors. The JOC core continues to see exactly 15 rows.

The compatible-base path separates six-channel E-AC-3 input into five QMF
channels (`FL,FR,FC,SL,SR`) and a retained LFE PCM source. The LFE is attached
only at the scene boundary, never copied from a JOC row and never fabricated.
The scene manifest now has 16 entries: one `lfe` entry followed by 15
`dynamic` entries. A/E/D/F each decode 126 AUs / 193,536 samples and produce
15 dynamic PCM outputs plus one base-carried LFE output. The tested LFE source
is present but silent; its zero peak/RMS are measured evidence.

OAMD activity is determined only by the parsed `active` field. The observed
Logic frames flag all 15 dynamic slots active, including E even though E's ADM
contains zero dynamic object channels. This is recorded as the distinction
between fixed codec slot capacity and ADM content; PCM nonzero status is not
used to rewrite activity. F's `OBJ_997HZ` and `OBJ_2003HZ` are therefore only
partial ADM/frequency oracles, not proven row identities.

The private reproducibility package is
`OpenJOC-Private/reports/runs/2026-08-05T044606Z_object-cardinality_a4f88af_r3`.
Its layout, binding, LFE, row-metric, PCM, scene, ADM-partial, and
strict/vendor reports are duplicated under `reports3` and
`reports_repeat3`; every JSON/TXT pair is byte-identical. Media and reports
remain private and are not committed.

This advances only the vendor-compatible scene boundary. `ETSI_STRICT` still
returns `ReservedWarpMode { raw: 3 }`; trim remains opaque; no complete OAMD
timeline, JOC semantic fidelity, ADM position equivalence, speaker render, or
internal-base comparison has been completed.

## Current base-fidelity evidence boundary (2026-08-05)

The private run
`OpenJOC-Private/reports/runs/2026-08-05T053438Z_internal-base-fidelity_dcfb56c`
now compares FFmpeg compatible-base and OpenJOC `--internal-base` on the same
raw EC-3 for A/E/D/F. Both paths retain explicit reference-f64 full-base,
five-channel JOC-input, and separate LFE artifacts. The mapping is
`FL,FR,FC,LFE,SL,SR` -> `FL,FR,FC,SL,SR` + `LFE`; the JOC matrix never receives
the LFE. FFmpeg 8.1.2 uses `0:a:0`, explicit `pan`, `pcm_f64le`, no resampling,
and its default presentation DRC/dialnorm policy is recorded. OpenJOC's TDAC
state and deterministic dither policy are recorded independently.

The numerical evidence is not a fidelity pass: all vectors first differ at
zero delay in AU 0/block 0 (FL/FR/FC/SL/SR samples 9/9/7/12/5), with selected
delay 0; raw SNR is approximately 84.5--90.6 dB for front/centre and
38.8--51.3 dB for side channels. Both LFE streams are exactly silent. The
same payload/state is compared through object rows and deterministic 440/659/
997/2003 Hz matrices; F's ADM object names do not yield a unique row identity,
and E remains the no-dynamic-object codec-capacity control.

This increment therefore distinguishes: real carrier accepted; real EMDF and
payload 11/14 located; OAMD entered; vendor-compatible scene activated; base
PCM numerically compared; but warp semantics, complete OAMD timeline, ADM
position/trim fidelity, JOC semantic fidelity, speaker rendering, and a
fidelity acceptance result remain open. `ETSI_STRICT` still returns
`ReservedWarpMode { raw: 3 }`; no remap or vendor warp interpretation was
added. `reports1` and `reports2` are byte-identical and all media remain
private.

## Base policy and first numeric boundary (2026-08-05)

The next controlled increment is private run
`2026-08-05T070007Z_base-root-cause_792d937`. It audits local FFmpeg 8.1.2
decoder options and runs the single-variable R0--R6 policy matrix. No option
is adopted as a normative decoder rule. The internal path now has an explicit
`InternalBasePolicy` boundary: `CurrentDefault` preserves existing CLI output,
and `CodecCore` disables only optional `dynrng/dynrng2` presentation gain.
The default is not silently changed and strict `warp=3` behavior is untouched.

The matrix shows no universal DRC/noise/target-level explanation. The first
sample-level differences stay in AU 0/block 0, but the dominant side-channel
residual is a repeatable TDAC-state event at sample 1536. Resetting overlap
state only before frame/AU 1 in a private diagnostic probe removes that event;
ETSI TS 102 366 V1.4.1's overlap/add rule requires the previous block, so this
is evidence of an unresolved encoder/decoder boundary or priming convention,
not permission to add a hidden reset. No production remap, gain, FFmpeg
algorithm, or vendor warp rule was added. The next open target is to obtain a
normative or independently controlled explanation of that first-frame state
boundary before changing TDAC semantics.

## TDAC boundary increment (2026-08-05)

This increment adds no AU reset. The opt-in TDAC trace follows ETSI TS 102 366
V1.4.1 clauses 5.2.11/6.9.4 and records `carry_in`, current windowed head,
output sum, output, and carry-out together with pre-window IMDCT and window
components. A synthetic 12-block versus 6+6 framed invariant passes exactly.

The private A/E/D/F evidence package
`OpenJOC-Private/reports/runs/2026-08-05T_tdac-boundary-corrected_054d3d4`
(repeated in `..._repeat`) covers all 125 AU boundaries. The trace uses
E-AC-3 syntax order `L,C,R,Ls,Rs`; the reference order is
`FL,FR,FC,LFE,SL,SR`, mapped as `[L,R,C,Ls,Rs]` after LFE removal. Every full-band codec channel satisfies
`AU n block5 carry_out == AU n+1 block0 carry_in`; state staging and rollback
are therefore verified. At the first boundary the Ls/Rs normal residual is
about `7.57e-3/7.35e-3` RMS, while the zero-carry probe is about
`1.26e-7/1.25e-7`; the inferred black-box carry is not correlated with the
stored tail. This localizes the remaining difference to an upstream block-5
tail or external FFmpeg frame-boundary policy, not lost state or a channel
vector permutation.

The result is an evidence-boundary increment, not a decoder-fidelity claim:
strict `warp=3` rejection, vendor opaque trim retention, complete OAMD timeline,
ADM/render fidelity, and accepted internal-base/JOC fidelity remain open. The
next target is an independently controlled/normative explanation of the
side-channel block-5 tail before any production TDAC semantic change.

## Independent TDAC oracle and pre-roll boundary (2026-08-05)

The authoritative private evidence is split between
`2026-08-05T_tdac-oracle-preroll_b18ea4d_r4` (independent oracle, real block5
replay, Logic virtual crop, joint decision) and
`2026-08-05T_tdac-oracle-preroll_b18ea4d_r5` (P0/P1/P2/P4 base-only vectors).
The oracle is independent of OpenJOC production TDAC, FFmpeg, and real-vector
parser/state/cosine helpers. It passes 53 synthetic comparisons and an exact
12-block versus 6+6 partition invariant. Real AU0 block5/AU1 block0 replay
supports production carry tail/head arithmetic to `5.12e-17`/`2.00e-15`
maximum absolute error.

P0/P1/P2/P4 preserve the same active content and add 0/1/2/4 silent AUs.
FFmpeg raw and MP4 PCM are sample-identical, but none reproduces the Logic
approximately `7e-3` first Ls/Rs boundary event. Excluding the first two
Logic AUs lowers diagnostic error without changing production output. The
result is unresolved evidence, not a priming fix: no TDAC reset, gain, remap,
or special case was added. `ETSI_STRICT` and raw warp `3` behavior remain
unchanged; no vendor warp interpretation was added.

Current completion boundary: real carrier/EMDF/payload and vendor scene
entry are established, and TDAC arithmetic is independently supported. A
complete OAMD timeline, JOC semantic reconstruction, accepted fidelity,
ADM position/render comparison, and non-zero object-PCM fidelity are not
claimed. The next experiment must isolate AU0/block5 Ls/Rs coefficient
provenance (coupling/SPX/rematrix/dither or encoder boundary) without
altering continuous TDAC semantics.

## AU0/block5 provenance round (2026-08-05)

The current private evidence package is
`OpenJOC-Private/reports/runs/2026-08-05T125009Z_logic-first-block-provenance_77116e9`.
Apple `afconvert` is available as a real macOS comparator. Its declared
`L,C,R,Ls,Rs,LFE` layout is mapped explicitly to
`FL,FR,FC,LFE,SL,SR`; Apple, FFmpeg MP4/raw, and OpenJOC outputs are kept in
separate coordinate/timing reports. This is a comparator boundary, not a
normative oracle.

The Logic pre-roll corpus consists of four fresh project copies exported at
the four-second selection range: LE0/LE1/LE2/LE4 add 0/1/2/4 silent AUs to
the source. All have 126 AUs and exact raw-EC3/MP4 payload-11 equality. Every
payload-11 body is unchanged across its 126 AUs and every warp observation is
raw `3`. The first project-scope export was 256 seconds/8001 AUs and is
excluded from the accepted corpus; it is not used as evidence.

Four-way coefficient probes show no target-block coupling, SPX, rematrix, or
AHT activity. BAP-zero bins are classified as dither/noise only under the
transmitted dither flag. Exponent/BAP state at frame 0 differs from later
blocks, so exact later-block matches are not universal; relaxed comparisons
that omit exponent strategy are diagnostic only. Carry lifecycle remains
continuous, and the independent mathematical TDAC oracle still reproduces
the production head/tail arithmetic.

Three diagnostic warp hypotheses (0/1/2) all close the bounded element but
are explicitly non-unique and do not reach normative object-element decode.
Therefore no hypothesis is selected and no alias for raw 3 is implemented.
`ETSI_STRICT` continues to fail with `ReservedWarpMode { raw: 3 }`;
`DOLBY_VENDOR_COMPAT` preserves raw 3 and reports opaque unresolved trim.

At that 2026-08-05 round, the first remaining blocker was AU0/block5 Ls/Rs pre-IMDCT coefficient
provenance and internal-base fidelity. The diagnostic tail inverse is
ill-conditioned (about `8.42e6`) and cannot identify a unique internal tool.
Complete OAMD timeline, JOC semantic reconstruction, object PCM fidelity,
ADM position comparison, and accepted fidelity remain open. No TDAC reset,
gain, remap, AU special case, or vendor warp rule was added in this round.

## Exact target-AU history experiment (2026-08-05)

The private run `2026-08-05T_exact-au-history_e73ef3f_r7` uses the existing LE0
raw EC-3 bytes. OpenJOC's indexed AU parser establishes 126 AUs and exact
3,072-byte target AU0/AU1 ranges. H0/H1/H2/H4/HP prepend exact AU0 or AU0+AU1
copies without re-encoding; target bytes and hashes remain identical in every
target occurrence. MP4 stream-copy remux succeeds for all five histories and
MP4-to-EC3 roundtrip is byte-identical. These are diagnostic byte-history
corpora, not normative programme vectors.

OpenJOC replay shows stable parsed headers, exponent/BAP state, exposed
pre-IMDCT coefficients, and AU0/block5 Ls/Rs tails for identical target bytes.
H1/H2/H4/HP target AU0 first diverges at block-0 TDAC `carry_in` and final PCM;
target AU1 remains stable. Snapshot clone/replay is deterministic. An opt-in
trace records raw mantissa/grouped/dither/dequantized/pre-IMDCT stages from
the same production cursor; component transplant remains explicitly not
performed because production state components are not public.

FFmpeg changes black-box target output across histories, especially the AU0
side channels. Apple `afconvert` accepts every remuxed history and remains
stable at target AU0/AU1. Therefore the result is narrowed to a comparator
history/priming boundary versus Logic AU0/block5 upstream provenance; no
codec-core or TDAC fix is justified. Strict `warp=3` rejection, vendor opaque
trim behavior, OAMD/JOC profile behavior, and all fidelity boundaries remain
unchanged.

## Decoder comparison contract (2026-08-06)

The evaluation-only contract separates cold start `[0,1536)`, warm-up
`[1536,3072)`, and decoder-specific steady state. On the exact H0/H1/H2/H4/HP
corpus, OpenJOC reaches observed history convergence at source AU1, Apple is
stable from AU0, and FFmpeg has no PCM convergence suffix through AU8. PTS is
unavailable; Apple also has 288 fewer trailing samples. The absolute sample
1536 event is therefore a warm-up comparator disagreement, not a demonstrated
TDAC defect. Complete decoder-state hashes, universal priming semantics,
OAMD timeline, and JOC semantic fidelity remain open. No production trimming
or codec change was introduced.

## Steady-state coding-tool differential (2026-08-06)

Evidence package: private `2026-08-06T_steady-state-tool-differential_b62168f`
with byte-identical repeat `_r2`. The comparison uses fixed windows S1 AU2–15,
S2 AU32–63, and S3 AU80–110. OpenJOC/FFmpeg AU mapping is high confidence;
Apple mapping is medium confidence with 288 trailing samples absent and PTS
unavailable. Block alignment to an external decoder is not yet demonstrated.

The measured OpenJOC–FFmpeg median per-channel block RMS residual is about
`0.98e-6`; Apple differs from both by about `1e-5` on the same diagnostic grid.
No independent per-AU/per-block tool strata exist, so coupling, SPX, dither,
rematrix, AHT, and exponent strategy cannot be assigned causally. LFE silence
is excluded. JOC output remains a 15-row evaluation-only propagation report;
semantic object identity, complete OAMD timeline, JOC semantic reconstruction,
nonzero object PCM fidelity, and ADM fidelity remain open. Next blocker:
parser-emitted full tool inventory and a trusted external block anchor.

## Block-anchor and parser tool inventory (2026-08-06)

The parser-emitted inventory foundation is implemented without changing
decoder semantics. `diagnose-tools` reports explicit/reused provenance,
semantic channels, block-switch, exponent state, BAP histograms, dither,
coupling, SPX, rematrix, AHT, dynamic-range and grouping state. A/E/D/F each
produce 126 AU × 6 blocks with full-band and LFE records (4536 rows/vector),
and repeat packages `_r5`/`_r6` are byte-identical for core JSON.

The private anchor source is deterministic 48 kHz 5.1 with 16 AU and six
distinct 256-sample markers per AU. An independent detector recovers 480/480
source blocks at high confidence. The subsequent G9 Logic-encoded
G_Block_Anchor_5_1 carrier and OpenJOC/FFmpeg/Apple comparison were generated:
the four required paths each recover 461/480 blocks, with 19 margin-only
near-neighbor ambiguities and no score or jitter failures. External mapping
and anchored tool effects therefore remain unavailable; the narrow blocker is
generalizable correlation-broadening evidence, not a demonstrated DSP or
TDAC defect. OAMD/JOC boundaries and strict `warp=3` behavior remain
unchanged.

## J1R7A — Spec-anchored normative OAMD position-field closure (2026-08-09)

This milestone is docs-only. The private run
`20260809T180109Z_j1r7a-spec-anchored-oamd_b6eb1de` froze two identical
analyses in `j1r7a_spec_cursor_evidence_freeze.json` (SHA-256
`572209bcb35cf2b37a512df1c9523b1a8762a2672445f96e57ad48a09257ba4f`). No
production code, test, CLI, fixture, Logic project, ADM, carrier, manifest, or
forensic output was changed.

The cursor begins at payload-11 bit 0 and follows only ETSI TS 103 420
syntax. Seven frozen sources × 129 AUs = 903 observations all reach the same
bounded normative prefix `[0,526)`. Within that prefix:

- `pos3D_X` is the exact six-bit payload-relative field `[52,58)`.
- `pos3D_Y` is the exact six-bit payload-relative field `[58,64)`.
- Controlled ADM-qualified X values `-1,-.5,0,+.5,+1` align with raw codes
  `0,16,31,46,62`; Y values `+1,0,-1` align with `0,31,62`.
- The historical `[58,63)` value is only the first five bits of the full Y
  field and is not a revised production field.

The first unresolved syntax is trim `warp_mode [526,528)`, raw `11` = `3`.
ETSI Table 32 marks `0b1X` reserved. `ETSI_STRICT` therefore still rejects
`ReservedWarpMode { raw: 3 }`; `DOLBY_VENDOR_COMPAT` is unchanged and no warp
alias is added. J1R6D H0/H1/H2 are non-discriminating diagnostic labels over
the same cursor, not semantic support.

The completed boundary is limited to normative prefix/field identity and
controlled numeric alignment. Complete trim/timeline/previous-state semantics,
post-warp vendor continuation, authored-object ↔ OAMD-slot identity,
OAMD ↔ JOC binding, object PCM, ObjectScene/render fidelity, ADM/render
comparison, and end-to-end acceptance remain open. The active next research
line is **J1R7B — Reserved Warp-3 Empirical Boundary Characterization**;
this milestone only records it and does not execute it.

## J1R8 — Controlled 3D position calibration closure (2026-08-10)

The private J1R8 run
`20260810T032631Z_j1r8-z-elevation-calibration_c90779b` is frozen by
aggregate SHA-256
`faeaf08c88f2aa8d241262de6edf6ab60e35ccdd959fa91239f6640f94779c8a`.
Exactly one Center-derived Logic fixture was used. The automation parameter
`对象位置提升` was independently verified after save/reopen at `0, 50, 100,
0`; ADM independently showed Z baseline → ~0.5 → 1.0 → baseline while X/Y
remained fixed at approximately `0/+1`. The ETSI normative Z fields are the
one-bit sign `[64,65)` and four-bit magnitude `[65,69)` from the J1R7A cursor
ledger. The observed magnitude sequence was
`0,3,6,7,13,14,15,10,3,1,0`, establishing controlled numeric alignment and
return-to-baseline evidence without claiming a formula.

The source PCM remained sample-identical to Center and the unchanged project
produced byte-identical stream-copied raw EC3 for R0/R1. `warp [526,528)`
remained raw `3` for all 129 AUs and therefore remains
`ETSI_STRICT -> ReservedWarpMode { raw: 3 }`; no vendor compatibility rule
was added. The empirical suffix `[528,536)` remained all zero, with no
padding or other semantics assigned. This closes controlled 3D X/Y/Z
position calibration for the tested evidence scope, not complete OAMD or
Atmos reconstruction.

The Size branch is frozen: Object Size authoring persistence and ADM
propagation are established, but tested DD+ Size-state semantics,
deactivation as an intra-stream payload-11 transition, direct size-index
response, and Size-related warp/suffix response remain unresolved/not
observed. Complete OAMD timeline/state semantics, reserved-warp meaning,
OAMD↔JOC binding, verified object PCM, ObjectScene/render fidelity, and
end-to-end acceptance remain open. No second Z fixture is required by this
calibration; any future fixture should be selected only for a precise
remaining control.

## J1R9 — Dual-object JOC-row identity boundary (2026-08-10)

The private evidence package
`20260810T104057Z_j1r9-dual-object-multitone-identity_6492301` is frozen by
aggregate SHA-256
`d9611198677caf2f0d6c56aacc4b2fe70843f8fc7a9489546b9658e697045863`.
It is one deterministic four-second FL/FR swap: independently authenticated
997 Hz and 2003 Hz source identities exchange positions in ADM, while R0/R1
stream-copied raw EC3 is byte-identical.

The valid conclusion is deliberately narrow. In stable pre/post windows,
OAMD Element 1 slot 0 remains Front-Left and slot 3 Front-Right, while paired
high-energy pre-render JOC rows exchange audio identity: row 0 becomes 2003
Hz at FL and row 3 becomes 997 Hz at FR. Therefore
`ONE_ROW_PER_AUTHORED_OBJECT_MODEL_REJECTED` for this controlled swap; the
evidence gives scoped support to spatially anchored row structure instead.

The boundary remains strict: full authored-object/OAMD-slot binding is not
established, raw `warp=3` remains reserved and opaque, the post-warp suffix is
uninterpreted, and there is no ObjectScene, renderer, or final object
PCM/fidelity claim. No production behaviour changed. The next task is to
specify a falsifiable spatial-basis binding model from the qualified corpus
before any ObjectScene admission.
