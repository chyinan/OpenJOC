# Reconstructed interoperable ADM BWF export

OpenJOC provides a standards-based reconstructed interchange export with a
bounded compressed-media streaming path:

```sh
openjoc export-adm INPUT.ec3 -o OUTPUT.wav
openjoc validate-adm OUTPUT.wav
```

## What this export means

For the explicitly admitted JOC profile, OpenJOC can associate decoded JOC
object audio with the decoded movement metadata carried by the same JOC
programme. The generated ADM Objects can therefore move. This reconstructs the
object scene carried by JOC; it does not recover the original Atmos authoring
master.

Generated names, numbers, and UIDs belong to the export. They are not the
original DAW/Logic track identity, authored Object numbering, ADM Object UIDs,
or source-stem PCM. JOC is lossy, so decoded movement can differ from source
automation. Unsupported profiles remain neutral in best-effort mode or fail
closed in strict mode. The detailed scope and report fields appear below.

## Structural correctness and renderer equivalence

OpenJOC reconstructs an interoperability-oriented ADM representation of the
decoded JOC object scene. It does not recover the original authored Dolby
Atmos master, and it does not guarantee perceptually identical localization to
a native JOC final renderer.

The reconstructed scene is validated at the decoded-data and ADM-structure
boundaries: object PCM, carrier-local object binding, coordinates, timing,
supported gain/state metadata, track identity, container structure, and ADM
relationships are checked within the documented profile. Those checks establish
structural and decoded-scene correctness; they do not establish native-renderer
perceptual equivalence.

A residual localization difference was observed in at least one real-world
validation programme after the reconstructed ADM passed the applicable
technical checks. That observation is material-specific and
non-generalizable. Native JOC playback remains the reference where
renderer-identical spatial localization is required. This is a known
limitation of reconstructed ADM interoperability, not a claim that the
decoded object scene or export structure is invalid.

`INPUT` may also be a captured OpenJOC scene directory or a complete
`ObjectScene` JSON document. For raw E-AC-3 or seekable ordinary ISO BMFF, the
CLI performs a lightweight sequential preflight, reopens the input once, and
writes each bounded decoder AU directly from the renderer-independent
reconstruction boundary into RIFF/RF64 ADM BWF. It does not create a captured scene or
full-duration diagnostic row WAVs first. Explicit JSON and scene-directory
inputs retain their existing diagnostic in-memory model.

## Production streaming and failure behavior

Compressed-media preflight establishes the sample rate, exact sample duration,
JOC/profile eligibility, stable ReconstructionBasis cardinality, Base LFE
presence, metadata counts, final track order, and checked `u64` PCM size. A
bounded first-AU PCM decode verifies the admitted Base topology; the PCM pass
then occurs exactly once after reopening the seekable input.

The streaming writer retains one decoded AU and one interleaving buffer. Its
PCM retention is proportional to track count times maximum AU samples, not to
programme duration. The 128 MiB diagnostic capture limit remains unchanged and
continues to apply to explicit capture products, not production ADM export.

Output is first written to a hidden sibling `partial` file. After exact sample
count finalization, the CLI validates ADM BWF by seeking across `data`, writes the
adjacent report to staging, and commits both paths with rollback-safe
replacement. Decode, range, validation, or I/O failure removes new staging and
does not publish a successful-looking output/report. Existing files are
preserved when an authorized replacement fails.

JOC reconstruction is kept in decoder-domain floating-point form until the
integer PCM boundary. The public JOC reconstruction equations are linear QMF
matrix sums and do not establish a `[-1, 1]` PCM invariant; therefore a legal
decoded object can contain floating-point headroom. The signed-24-bit ADM
writer checks every sample immediately before quantization and fails closed on
non-finite or out-of-range values. It never clips, saturates, applies a hidden
limiter, or normalizes individual objects/tracks. Successful reports include a
bounded `pcm_headroom_census` with whole-programme and per-signal statistics;
no programme-duration PCM copy is retained for that census.

Interactive stderr shows throttled analysis, export, finalization, and
validation progress. Redirected/non-TTY execution is quiet apart from the final
summary or error; `--no-progress` disables interactive updates explicitly.

The repository-owned external-tool smoke fixture can be generated without the
private Logic oracle:

```sh
cargo run -p openjoc-adm --example synthetic_interop_fixture -- candidate.wav
openjoc validate-adm candidate.wav
```

It contains one second across two neutral reconstruction Objects and a generated
5.1 transport bed. Only the bed LFE carries the synthetic Base LFE input; the
other five bed channels are explicit silence placeholders, plus an adjacent
semantic report.

An OpenJOC-generated ADM BWF file is a reconstructed representation of the
scene carried by an E-AC-3 JOC programme. It is not the ADM/BWF source master
that was used before encoding. OpenJOC does not and cannot recover information
discarded, quantized, merged, transformed, or never transmitted by the lossy
encoding process. Multiple different source ADM masters can produce identical
or observationally equivalent JOC data, so JOC → original ADM is not a unique
inverse.

## Reconstructed dynamic Objects

For the explicitly admitted decoded-JOC/OAMD profile, OpenJOC can associate
each decoded JOC object signal with the corresponding decoded OAMD dynamic
metadata. The export therefore emits generated ADM Objects with the decoded
position events at their sample-domain boundaries. A reconstructed Object may
move; it is not forced to a neutral position merely because its audio came
from JOC.

The generated Object represents a decoded JOC Object carried by the stream.
It is not a recovered authored Object. OpenJOC does not promise recovery of
the original DAW/Logic track identity, authored Object numbering, ADM Object
UID, Object name, source-stem PCM, unquantized automation,
programme/content hierarchy, authoring metadata, Dolby authoring provenance,
or a lossless JOC → ADM round trip.

### OAMD and ADM Cartesian coordinates

Decoded OAMD room positions and ADM Cartesian positions are different public
coordinate domains. In the admitted in-room profile, OAMD uses normalized room
coordinates: X is left-wall `0` to right-wall `1`, Y is front-wall `0` to
back-wall `1`, and Z is floor `-1` to ceiling `1`. ADM Cartesian uses a
centered normalized cube: X is positive to the right, Y is positive to the
front, and Z is positive upward. OpenJOC converts explicitly at the ADM
boundary:

```text
ADM X = 2 × OAMD X - 1
ADM Y = 1 - 2 × OAMD Y
ADM Z = OAMD Z
```

The bridge validates finite values and the supported normalized input/output
ranges. It rejects unsupported coordinates rather than silently clamping them.
This conversion does not alter decoded-object binding, the scene model, PCM,
or any renderer-domain processing. The public-spec reconciliation and control
truth table are recorded in
[`docs/research/joc-object-binding/oamd-to-adm-coordinate-reconciliation.md`](research/joc-object-binding/oamd-to-adm-coordinate-reconciliation.md).

## Are these the original Atmos Objects?

No. `OpenJOC Reconstructed JOC Object 04` is an OpenJOC-generated identity for
a decoded carrier-local object slot. It must not be read as proof that the
signal is authored Object 04 in the source Logic or ADM project. Encoding may
quantize metadata, reorganize object representation, change numbering, or
discard authoring information.

## Why reconstructed Objects may differ from the source master

When reconstructed Objects move, their trajectories are the spatial metadata
retained and decoded from the JOC programme. They are not guaranteed to be
numerically identical to the original DAW automation. JOC is a lossy delivery
representation, so the decoded scene can preserve meaningful movement while
still differing from the source master.

Therefore:

```text
reconstructed dynamic ADM != recovered original ADM master
```

Comparing an OpenJOC export with the original ADM is useful for evaluating
what survived encoding. Compare movement and audio with the understanding
that identities, numbering, and discarded authoring data are not guaranteed
to correspond directly.

## Why exported ADM objects may not move

If you open an exported ADM file in a DAW and some objects look still, this does
not mean OpenJOC failed to decode movement or that direct JOC rendering is
static. There are two export cases:

1. In the admitted profile, decoded JOC audio and decoded OAMD movement are
   bound by the clean ordinal contract, so generated dynamic Objects carry
   position blocks and may move.
2. Outside that profile, OpenJOC can recover audio and movement separately but
   cannot safely prove which signal belongs to which decoded metadata object.
   Best-effort output remains neutral/static and strict output rejects.

So “objects in exported ADM do not move” describes unsupported or unresolved
profiles, not the admitted dynamic path.

Direct JOC rendering is a different pipeline from ADM export:

`JOC decode → direct playback renderer`

while export is:

`JOC decode → reconstructed signals → reconstructed ADM export`

Even a moving generated Object does not prove that the original authored
Object was recovered. The decoded-object binding is carrier-local and does not
answer the stronger authored-source or renderer questions.

The generated 5.1 bed can also look like “conversion to 5.1,” but it is mainly a
minimum legal transport shape:

- if Base LFE is recovered, it goes to the LFE channel,
- the other five 5.1 bed channels are generated silence placeholders to complete a
  valid 5.1 DirectSpeakers structure.

These placeholder channels are there so downstream tools can accept a standard
container; they are not extra authored object content.

The same distinction applies to decoded Base full-band PCM. Base/downmix
channels are inputs to the JOC reconstruction domain, while an independent
Base contribution in the final delivery scene requires a separate decoder-
semantic proof. Decoded Base C energy, including vocal-correlated energy, does
not by itself authorize adding Base C to this bed: doing so could duplicate the
contribution already represented by decoded JOC Objects. The original authored
Bed is not a source of evidence for this decision, and no authored Center-bed
identity is recovered.

## Supported binding profiles

The exporter consumes `ObjectScene` and `ReconstructionBasis` directly. It does
not consume a 7.1.4/22.2 speaker render, FinalLinkedGain output, or HRTF
output. Those are renderer-domain results and are not reconstructed scene
signals.

The exact clean-room admission profiles are
`E_AC_3_JOC_OBSERVED_ORDINARY_PROFILE` and the exact observed compatibility
variant `E_AC_3_JOC_OBSERVED_ORDINARY_COMPAT_WARP3_PROFILE`, each with:

- 15 decoded JOC Objects and 15 reconstruction rows;
- no OAMD bed, one leading Base LFE, and no ISF;
- 15 dynamic OAMD Objects and 16 total OAMD entries.

The compatibility variant is admitted only when the known deviation family
and the opaque raw3 element shape both match the clean-room whitelist. ETSI
strict classification remains `ReservedWarpMode(3)`: OpenJOC preserves the
opaque raw3 payload, does not claim its full vendor meaning, and does not
apply a raw3-specific spatial transform. The observed decoded OAMD position
metadata is sufficient for the scoped bridge in this exact profile.

Within the same programme/discontinuity epoch, the typed mapping is:

```text
joc_ordinal = j
oamd_dynamic_ordinal = j
oamd_total_index = j + 1
```

`ResolvedWithinCarrier` means this decoded JOC audio ↔ decoded OAMD relation
is admitted. It is not an authored-object identity. The `+1` is a total-list
domain offset for the leading Base LFE, not an element-ID lookup or PCM
heuristic.

## Unsupported profiles

Bed-bearing, ISF-bearing, alternate-LFE, count/order-mismatched,
unknown-deviation compatibility, incomplete-Base-LFE, or otherwise
unvalidated profiles
remain unresolved for dynamic binding. Best-effort export retains generated
Objects at neutral/static positions and records `unsupported_binding_reason`;
strict export rejects. Unsupported dynamic properties such as inactive
transitions, gain, extent, divergence, channel lock, and zones are not
fabricated into ADM. For supported dynamic Objects, the Dolby profile
`jumpPosition` transport uses `interpolationLength=0` on the first block and
`250` samples on every subsequent block; this is a target ADM profile rule,
not a copy of the source OAMD `ramp_duration` value.

The Dolby Atmos master profile does not allow a mono DirectSpeakers/LFE bed.
When known Base LFE PCM is present, OpenJOC places it in the LFE position of the
minimum allowed 5.1 bed and generates silent L, R, C, Ls, and Rs placeholders.
Those five channels are named and reported as generated transport structure,
not recovered authored PCM. No authored name, stem, object identity, or
programme hierarchy is inferred.

## Standards

The implementation targets the public standards subset described by:

- ITU-R BS.2076-3 (02/2025), Audio Definition Model.
- ITU-R BS.2088-2 (11/2025), long-form WAVE metadata chunks and size semantics.
- Dolby Atmos Master ADM Profile v1.0, interoperability element/ID/bed/BWF
  constraints.
- EBU Tech 3285 Supplement 6 (2009), public `dbmd` envelope semantics.
- EBU Tech 3285 Supplement 7 (2018), `chna` chunk reference semantics.

Primary public references: [ITU-R BS.2076](https://www.itu.int/rec/R-REC-BS.2076/),
[ITU-R BS.2088-2](https://www.itu.int/dms_pubrec/itu-r/rec/bs/R-REC-BS.2088-2-202511-I!!PDF-E.pdf),
[Dolby Atmos Master ADM Profile](https://developer.dolby.com/globalassets/documentation/technology/dolby_atmos_master_adm_profile_v1.0.pdf),
[EBU Tech 3285 Supplement 6](https://tech.ebu.ch/publications/tech3285s6),
and [EBU Tech 3285 Supplement 7](https://tech.ebu.ch/publications/tech3285s7).

When the complete file and `data` sizes fit the 32-bit WAVE fields, the writer
uses `RIFF/WAVE` with exact 32-bit sizes and a 64-byte leading `JUNK` reserve.
When they do not fit, it uses `RF64/WAVE`, a mandatory first `ds64`, 32-bit
sentinels, and checked 64-bit RIFF/data/sample counts. Both forms use
`fmt `, `data`, uncompressed `axml`, `chna`, and a minimal public EBU
Supplement 6 `dbmd` envelope in that order after the size reserve. Reserved
Atmos-specific DBMD segment payloads are neither copied nor invented. Audio is
signed 24-bit little-endian PCM. The writer rejects
non-finite or out-of-range samples and does not normalize, limit, compress, or
apply loudness processing.

The recommended `.wav` extension is the user-facing BWF/ADM convention used by
common Atmos workflows. `.bw64` remains accepted as a legacy filename alias,
but it does not force a `BW64` signature; representable output remains `RIFF`
and oversized output becomes `RF64`.

## Supported ADM subset

The deterministic XML contains the minimum relationships needed for the
exported PCM tracks:

- one neutral `audioProgramme` and `audioContent`;
- generated `audioObject`, `audioPackFormat`, `audioChannelFormat`,
  `audioStreamFormat`, `audioTrackFormat`, and `audioTrackUID` elements;
- `Objects` channel formats for reconstruction signals;
- a legal room-centric 5.1 DirectSpeakers bed when Base LFE is present, with
  `RC_LFE` carrying recovered PCM and five reported silence placeholders;
- Dolby Atmos object IDs beginning at `AO_100B` and bed IDs in the bed range;
- one sample-derived `audioBlockFormat` per signal for neutral/unresolved output;
- for the admitted profile, one deterministic position block per OAMD event
  boundary, with exact sample-derived `rtime`/duration coverage and
  profile-compliant first/subsequent `jumpPosition` interpolation metadata;
- cartesian neutral position for signals whose spatial binding is unresolved;
- standard ADM ID syntax and child-element IDRef relationships;
- `chna` UIDs/track-format/pack-format references matching the XML and PCM
  track indices.

Generated identities are stable and neutral. Unresolved output uses
`OpenJOC Reconstructed Signal 01`; admitted decoded-object output uses
`OpenJOC Reconstructed JOC Object 01`. Names such as “Lead Vocal”, “Dialogue”,
or “Music” are never guessed.

## Mapping table

The same table is used by the writer and the reconstruction report.

| Semantic | Status | Current treatment |
|---|---|---|
| Reconstruction signal identity | `EXACT` | Local ReconstructionBasis row identity only. |
| Audio ↔ spatial metadata binding | `EXACT` within scope / `UNRESOLVED` otherwise | In the admitted profile, decoded JOC ordinal `j` maps to OAMD dynamic ordinal `j` and total index `j+1`; no authored identity is claimed. |
| Dynamic position/trajectory | `EXACT` within supported position scope / `NOT_REPRESENTABLE` otherwise | Admitted OAMD position events become deterministic ADM blocks with profile-compliant jump interpolation metadata; unsupported properties or profiles remain neutral/rejected by policy. |
| Bed/direct-speaker identity for reconstruction rows | `NOT_REPRESENTABLE` | Structural order is not promoted to authored identity. |
| Separately retained base LFE identity | `EXACT` | Exported in the LFE position of a generated 5.1 DirectSpeakers bed. |
| Dolby Atmos bed transport placeholders | `APPROXIMATED` | Five explicitly reported silent tracks complete the minimum allowed LFE-bearing bed. |
| Extent, channel lock, divergence, zones, JOC controls | `NOT_REPRESENTABLE` | Not silently invented in ADM fields. |
| Original hierarchy, names, UIDs, comments | `NOT_RECOVERABLE` | Neutral generated IDs are used only where required. |
| PCM sample timing and track order | `EXACT` | Derived from the scene sample domain; object-block interpolation follows the Dolby profile's first-block 0/subsequent-block 250-sample rule. |
| Float-to-24-bit storage | `APPROXIMATED` | Deterministic quantization, with no gain processing. |
| FinalLinkedGain, HRTF, speaker render | `NOT_APPLICABLE` | Export occurs before those renderer stages. |

For the admitted dynamic path, the property boundary is deliberately narrow:

| OAMD property | Status | Export treatment |
|---|---|---|
| X/Y/Z position | `ADMITTED` | Supported finite normalized OAMD room coordinates are converted once to normalized cartesian ADM position elements; unsupported ranges fail closed. |
| Active/inactive | `PARTIAL` | Active slots are retained; an inactive transition rejects dynamic export and follows policy. |
| Gain | `UNSUPPORTED` | Not copied into ADM dynamic blocks. |
| Timing / ramp | `APPROXIMATED` | Sample-domain block boundaries are retained; source OAMD ramp values are not copied, and Dolby profile jump interpolation is emitted as 0 samples for the first block and 250 samples thereafter. |
| Extent/size/spread | `UNSUPPORTED` | Not fabricated. |
| Divergence | `UNSUPPORTED` | Not fabricated. |
| Channel lock | `UNSUPPORTED` | Not fabricated. |
| Zones and opaque/additional data | `OPAQUE` | Retained by the decoder boundary where applicable, not interpreted by this ADM path. |

## Policy

Best-effort is the default:

```sh
openjoc export-adm INPUT.ec3 -o OUTPUT.wav --adm-policy best-effort
```

For the admitted profile it emits bound generated dynamic Objects and writes
every unsupported or omitted semantic to `OUTPUT.adm-report.json`. For other
profiles it preserves neutral best-effort output and records the binding reason.

Strict mode permits only the complete admitted dynamic path and rejects
unsupported or unresolved profiles:

```sh
openjoc export-adm INPUT.ec3 -o OUTPUT.wav --adm-policy strict
```

This is intentional. Strict mode must not turn an unproven row/object
correspondence or unsupported metadata property into a confident-looking ADM
file.

## Reconstruction report

The adjacent JSON report includes:

- OpenJOC and report schema versions;
- source format, selected RIFF/RF64 container, sample rate, duration, and PCM
  representation;
- reconstructed signal, metadata, dynamic, DirectSpeakers, and generated silent
  bed-placeholder counts;
- decoded binding state/profile, bound and unbound decoded-object counts, and
  whether dynamic metadata was exported;
- `dynamic_objects_with_bound_pcm` is the admitted decoded-object count only
  when the corresponding dynamic metadata path succeeds;
- the complete mapping table;
- generated signal identities;
- unrecoverable authoring information;
- approximations, omissions, warnings;
- the bounded `pcm_headroom_census`, including finite/non-finite counts,
  nominal-range violations, extrema, first violation, and per-signal peaks;
- `source_is_lossy_e_ac_3_joc: true`;
- `original_adm_master_recovered: false`;
- `lossless_round_trip: false`.
- `original_authored_identity_recovered: false` and
  `unsupported_binding_reason` when the scoped path is unavailable.
- `decoded_joc_object_binding_state`, `decoded_joc_binding_profile`,
  `decoded_joc_objects_bound`, `decoded_joc_objects_unbound`, and
  `dynamic_metadata_exported` are separate fields. A successful admitted
  dynamic export has a non-zero bound-object count; authored identity remains
  false.
- `dolby_authorship_metadata_state: "not-generated"`.

## FAQ

### Why do Objects move now when they were static before?

The admitted profile now proves the decoded JOC audio-to-OAMD relationship.
OpenJOC can attach each reconstructed decoded JOC Object signal to its
corresponding decoded movement metadata.

### Does that mean OpenJOC recovered my original ADM master?

No. The export reconstructs the object scene carried by JOC. It does not
recover the original authored project, hierarchy, names, UIDs, or automation.

### Is reconstructed Object #3 necessarily the same as Object #3 in my source project?

No. The numbering is generated from a carrier-local decoded ordinal. Encoding
may reorganize object representation and numbering.

### Why can its movement differ slightly from my source project?

JOC is lossy. The export uses the encoded and decoded metadata, which may have
been quantized or otherwise transformed.

### Can I compare an OpenJOC export with my original ADM?

Yes. That comparison can show which movement and audio information survived
JOC encoding. Do not assume that names, IDs, numbering, or discarded authoring
data line up directly.

### Does a moving ball prove bit-perfect original-object recovery?

No. It proves only that the admitted decoded JOC scene carries meaningful
decoded movement through the reconstructed export path.

`openjoc validate-adm` independently parses RIFF, RF64, and legacy BW64
containers. It checks container/file accounting, `ds64` and table semantics
when applicable, all chunk boundaries/padding, complete signed-24-bit PCM
`fmt` arithmetic, `chna` capacity/track indices/UIDs, well-formed EBUCore ADM
XML, Dolby programme/content requirements, continuous profile IDs, legal
bed/object ID ranges, allowed room-centric bed configurations, IDRef
relationships, channel/block types and timing, exact AXML↔CHNA links, and the
public EBU DBMD envelope/checksums. It seeks over PCM rather than loading the
complete file; `axml`, `chna`, and `dbmd` remain bounded allocations. This is a
structural validator, not vendor certification.

The JSON validation summary lists DBMD segment IDs and whether reserved Dolby
segments are present. The plain-text result says `STRUCTURE PASS`; it does not
claim Dolby authoring provenance or DEE acceptance.

## Determinism and limitations

XML ordering, IDs, names, time serialization, report ordering, chunk ordering,
and integer PCM conversion are deterministic. Synthetic fixtures should be
byte-identical across supported platforms.

R2 passes OpenJOC structural/profile validation and Logic Pro imports it as one
5.1 bed plus two Objects. DEE parses the ADM but rejects it with `Content was
not authored with Dolby tools.` OpenJOC deliberately does not generate the
reserved/private DBMD segments used for Dolby authoring provenance. Therefore
R2 is Logic-interoperable but is not directly DEE-ingestible. The maintainer
verified the authorized workflow: Logic imported R2, re-exported ADM BWF, and
DEE accepted the Logic-authored re-export. This does not make the byte-exact
OpenJOC output a direct DEE pass; direct DEE interoperability remains
unsupported and unclaimed.
