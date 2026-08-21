# OpenJOC 0.9.2 — Streaming ADM Interoperability Hotfix

OpenJOC 0.9.2 makes reconstructed ADM BWF export production-scale and confirms
an open authoring workflow with Logic Pro.

## Production streaming ADM export

- Compressed E-AC-3 JOC input is preflighted with bounded memory and decoded
  once into the ADM writer.
- Full-duration PCM is not retained; memory scales with one bounded access unit
  and track count rather than programme duration.
- Signed 24-bit PCM is written directly with checked sample, frame, and size
  accounting. Out-of-range PCM is rejected without clipping or normalization.
- Output and the adjacent semantic report are staged and committed
  transactionally, with rollback on decode, validation, range, or I/O failure.

## RIFF/RF64 ADM BWF interoperability

- Representable files use `RIFF/WAVE`; oversized files use `RF64/WAVE` with a
  validated `ds64` chunk. `BW64` is no longer the emitted default.
- Deterministic `fmt `, `data`, `axml`, `chna`, and public-envelope `dbmd`
  chunks follow the supported Dolby Atmos ADM profile subset.
- The independent streaming validator seeks across PCM and validates container
  accounting, PCM arithmetic, ADM IDs/IDRefs, legal bed/object ranges,
  room-centric bed layouts, CHNA relationships, and public DBMD structure.
- `validate-adm` reports `STRUCTURE PASS`; it is not Dolby certification.

## Real-media acceptance

The maintainer verified this production workflow:

```text
E-AC-3 JOC
  → OpenJOC reconstructed ADM BWF
  → Logic Pro import
  → Logic Pro ADM BWF re-export
  → Dolby Encoding Engine
```

Logic Pro imports the OpenJOC file as a multichannel bed plus Objects. Logic's
ADM re-export is accepted by Dolby Encoding Engine.

Direct ingestion of the byte-exact OpenJOC-authored reconstructed ADM file by
Dolby Encoding Engine is not claimed. OpenJOC does not generate or forge
Dolby-tool authoring provenance metadata.

## Semantic boundary

This release does not reconstruct the original Atmos master and is not a
lossless inverse of E-AC-3 JOC. Reports continue to state:

```text
original_adm_master_recovered = false
lossless_round_trip = false
semantic_binding_state = unresolved
dynamic_objects_with_bound_pcm = 0
dolby_authorship_metadata_state = "not-generated"
```

OpenJOC 0.9.0 and 0.9.1 remain immutable.
