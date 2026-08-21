# Reconstructed interoperable ADM BWF export

OpenJOC 0.9 adds a standards-based reconstructed interchange export. The
OpenJOC 0.9.2 makes compressed-media export production-scale and streaming:

```sh
openjoc export-adm INPUT.ec3 -o OUTPUT.wav
openjoc validate-adm OUTPUT.wav
```

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

## 0.9 scope and boundary

The exporter consumes `ObjectScene` and `ReconstructionBasis` directly. It does
not consume a 7.1.4/22.2 speaker render, FinalLinkedGain output, or HRTF
output. Those are renderer-domain results and are not reconstructed scene
signals.

The current OpenJOC scene contract deliberately keeps the association between
reconstruction rows and OAMD metadata `Unresolved`. Therefore 0.9 exports each
reconstruction row as a deterministic neutral reconstructed signal and reports
the recovered metadata as unbound. A row index that happens to equal an OAMD
index is not evidence of object identity and is never serialized as an ADM
binding.

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
- Dolby Atmos Master ADM Profile v1.1, interoperability element/ID/bed/BWF
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
- one sample-derived `audioBlockFormat` per signal;
- cartesian neutral position for signals whose spatial binding is unresolved;
- standard ADM ID syntax and child-element IDRef relationships;
- `chna` UIDs/track-format/pack-format references matching the XML and PCM
  track indices.

Generated identities are stable and neutral, for example
`OpenJOC Reconstructed Signal 01`. Names such as “Lead Vocal”, “Dialogue”, or
“Music” are never guessed.

## Mapping table

The same table is used by the writer and the reconstruction report.

| Semantic | Status | 0.9 treatment |
|---|---|---|
| Reconstruction signal identity | `EXACT` | Local ReconstructionBasis row identity only. |
| Audio ↔ spatial metadata binding | `UNRESOLVED` | No row is bound to an OAMD object. |
| Dynamic position/trajectory | `NOT_REPRESENTABLE` | Recovered metadata remains outside the PCM track relationship. |
| Bed/direct-speaker identity for reconstruction rows | `NOT_REPRESENTABLE` | Structural order is not promoted to authored identity. |
| Separately retained base LFE identity | `EXACT` | Exported in the LFE position of a generated 5.1 DirectSpeakers bed. |
| Dolby Atmos bed transport placeholders | `APPROXIMATED` | Five explicitly reported silent tracks complete the minimum allowed LFE-bearing bed. |
| Extent, channel lock, divergence, zones, JOC controls | `NOT_REPRESENTABLE` | Not silently invented in ADM fields. |
| Original hierarchy, names, UIDs, comments | `NOT_RECOVERABLE` | Neutral generated IDs are used only where required. |
| PCM sample timing and track order | `EXACT` | Derived from the scene sample domain. |
| Float-to-24-bit storage | `APPROXIMATED` | Deterministic quantization, with no gain processing. |
| FinalLinkedGain, HRTF, speaker render | `NOT_APPLICABLE` | Export occurs before those renderer stages. |

## Policy

Best-effort is the default:

```sh
openjoc export-adm INPUT.ec3 -o OUTPUT.wav --adm-policy best-effort
```

It emits the recoverable reconstruction signals and writes every unresolved or
omitted semantic to `OUTPUT.adm-report.json`.

Strict mode rejects the current unresolved audio-to-spatial-metadata boundary:

```sh
openjoc export-adm INPUT.ec3 -o OUTPUT.wav --adm-policy strict
```

This is intentional. Strict mode must not turn an unproven row/object
correspondence into a confident-looking ADM file.

## Reconstruction report

The adjacent JSON report includes:

- OpenJOC and report schema versions;
- source format, selected RIFF/RF64 container, sample rate, duration, and PCM
  representation;
- reconstructed signal, metadata, dynamic, DirectSpeakers, and generated silent
  bed-placeholder counts;
- `dynamic_objects_with_bound_pcm: 0` while binding is unresolved;
- the complete mapping table;
- generated signal identities;
- unrecoverable authoring information;
- approximations, omissions, warnings;
- `source_is_lossy_e_ac_3_joc: true`;
- `original_adm_master_recovered: false`;
- `lossless_round_trip: false`.
- `dolby_authorship_metadata_state: "not-generated"`.

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
