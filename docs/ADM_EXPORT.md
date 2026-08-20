# Reconstructed ADM/BW64 export

OpenJOC 0.9 adds a standards-based reconstructed interchange export:

```sh
openjoc export-adm INPUT.ec3 -o OUTPUT.bw64
openjoc validate-adm OUTPUT.bw64
```

`INPUT` may also be a captured OpenJOC scene directory or a complete
`ObjectScene` JSON document. For an E-AC-3 input, the CLI performs a bounded
temporary decode and exports from the renderer-independent reconstruction
boundary.

An OpenJOC-generated ADM/BW64 file is a reconstructed representation of the
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

Known base LFE PCM is exported separately as a DirectSpeakers `LFE1` track. No
authored name, stem, object identity, or programme hierarchy is inferred.

## Standards

The implementation targets the public standards subset described by:

- ITU-R BS.2076-3 (02/2025), Audio Definition Model.
- ITU-R BS.2088-2 (11/2025), BW64 long-form file format.
- EBU Tech 3285 Supplement 7 (2018), `chna` chunk reference semantics.

Primary public references: [ITU-R BS.2076](https://www.itu.int/rec/R-REC-BS.2076/),
[ITU-R BS.2088-2](https://www.itu.int/dms_pubrec/itu-r/rec/bs/R-REC-BS.2088-2-202511-I!!PDF-E.pdf),
and [EBU Tech 3285 Supplement 7](https://tech.ebu.ch/publications/tech3285s7).

The generated file uses `BW64`, mandatory first `ds64`, a conventional `fmt `
chunk, `data`, uncompressed `axml`, and `chna`. Audio is signed 24-bit
little-endian PCM. The writer rejects non-finite or out-of-range samples and
does not normalize, limit, compress, or apply loudness processing.

## Supported ADM subset

The deterministic XML contains the minimum relationships needed for the
exported PCM tracks:

- one neutral `audioProgramme` and `audioContent`;
- generated `audioObject`, `audioPackFormat`, `audioChannelFormat`, and
  `audioTrackUID` elements;
- `Objects` channel formats for reconstruction signals;
- `DirectSpeakers` / `LFE1` for the separately retained base LFE;
- one sample-derived `audioBlockFormat` per signal;
- cartesian neutral position for signals whose spatial binding is unresolved;
- `chna` references matching track order and channel count.

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
| Separately retained base LFE identity | `EXACT` | Exported as DirectSpeakers `LFE1`. |
| Extent, channel lock, divergence, zones, JOC controls | `NOT_REPRESENTABLE` | Not silently invented in ADM fields. |
| Original hierarchy, names, UIDs, comments | `NOT_RECOVERABLE` | Neutral generated IDs are used only where required. |
| PCM sample timing and track order | `EXACT` | Derived from the scene sample domain. |
| Float-to-24-bit storage | `APPROXIMATED` | Deterministic quantization, with no gain processing. |
| FinalLinkedGain, HRTF, speaker render | `NOT_APPLICABLE` | Export occurs before those renderer stages. |

## Policy

Best-effort is the default:

```sh
openjoc export-adm INPUT.ec3 -o OUTPUT.bw64 --adm-policy best-effort
```

It emits the recoverable reconstruction signals and writes every unresolved or
omitted semantic to `OUTPUT.adm-report.json`.

Strict mode rejects the current unresolved audio-to-spatial-metadata boundary:

```sh
openjoc export-adm INPUT.ec3 -o OUTPUT.bw64 --adm-policy strict
```

This is intentional. Strict mode must not turn an unproven row/object
correspondence into a confident-looking ADM file.

## Reconstruction report

The adjacent JSON report includes:

- OpenJOC and report schema versions;
- source format, sample rate, duration, and PCM representation;
- reconstructed signal, metadata, dynamic, and DirectSpeakers counts;
- `dynamic_objects_with_bound_pcm: 0` while binding is unresolved;
- the complete mapping table;
- generated signal identities;
- unrecoverable authoring information;
- approximations, omissions, warnings;
- `source_is_lossy_e_ac_3_joc: true`;
- `original_adm_master_recovered: false`;
- `lossless_round_trip: false`.

`openjoc validate-adm` checks the BW64 header, first `ds64`, required chunks,
PCM format, `chna` sizes and track order, UID uniqueness, and ADM XML markers.
It is an internal structural validator for this supported subset, not a claim
of certification by a DAW, renderer, or vendor.

## Determinism and limitations

XML ordering, IDs, names, time serialization, report ordering, chunk ordering,
and integer PCM conversion are deterministic. Synthetic fixtures should be
byte-identical across supported platforms.

Real Logic Pro, DaVinci Resolve, Dolby Atmos Renderer, Nuendo, and Pro Tools
import testing is intentionally deferred and is not a 0.9 release blocker.
The 0.9 claim is standards-based reconstructed export with internal structural
validation, not DAW round-trip compatibility or mastering interchange
certification.
