# Decoded JOC object ↔ OAMD semantic binding closure

Status: **closed without admission**.  This report records the evidence and the
exact reason that reconstructed dynamic ADM is not enabled.

## Question

Can an OpenJOC decoded reconstruction row be paired with the authored OAMD
object that owns its position, extent, gain, and time-varying updates, without
PCM fingerprinting or a proprietary implementation dependency?

The answer is no for the current public contract.  The codec and repository
preserve several useful ordered domains, but the required cross-domain identity
edge is not specified and is not observable through the documented DRP surface.

## Decision

* `PUBLIC_STANDARD_MAPPING`: `PARTIAL`.
* `JOC_OUTPUT_SEMANTIC_CLASS`: `DECODED_OBJECT_ESSENCE` (the decoder output is
  object audio, not an anonymous speaker coordinate); the repository also
  preserves a reconstruction-row domain.
* `JOC_OUTPUT_INDEX_PRESERVATION`: `PROVEN` for parser, state, reconstruction,
  synthesis, timeline, scene, and ADM track construction.  Present/absent
  entries remain in their declared slots; this is not proof of authored identity.
* `OAMD_OBJECT_INDEX_SEMANTICS`: `PARTIAL`.  OAMD ordering and inactive-entry
  retention are explicit locally, including the bed/ISF/dynamic expansion.
* `JOC_TO_OAMD_BINDING`: `OPAQUE_VENDOR_BLOCKED` / still ambiguous.
* `BINDING_RULE`: `NONE`.
* `BINDING_REQUIRES_PCM_FINGERPRINTING`: `NO` (fingerprinting is not accepted as
  a semantic proof).
* `BINDING_REQUIRES_PROPRIETARY_IMPLEMENTATION`: `YES` for the missing edge if
  one tries to reproduce DRP behavior.
* `BINDING_TIMELINE_ALIGNMENT`: `PARTIAL`: frame/sample timing is aligned, but
  the row-to-object identity is not.
* `DYNAMIC_RECONSTRUCTED_ADM`: `NOT_ADMITTED`.

The smallest exact blocker is:

> `JOC_OUTPUT_INDEX_TO_OAMD_INDEX_UNSPECIFIED`: no normative identifier,
> ordinal equation, permutation, LFE skip/offset rule, or mismatch/reset policy
> connects `QoutJOC[obj]` to an OAMD object entry.

## Evidence tiers

### Tier A — public normative material

ETSI TS 103 420 v1.2.1 and TS 102 366 v1.4.1 were checked against the repository
copies and rendered pages.  ITU-R BS.2076-3 was also used for the ADM semantic
context.  The clauses establish that a JOC decoder produces reconstructed object
essences together with timestamped corresponding object properties, and that
OAMD objects have a local order (bed, ISF, dynamic).  They do not equate the two
lists or define a cross-payload identity operation.  JOC side-information
presence, QMF/matrix reconstruction, timing offsets, and LFE bypass do not add
that missing identity rule.  The detailed clause ledger is in
[`normative-mapping.json`](normative-mapping.json).

### Tier B — source/index audit

The repository has a strong structural invariant: declared JOC object slots are
carried by parser → decoder state → reconstruction → synthesis → timeline →
scene → ADM track order.  OAMD entries likewise retain their local indices;
programme layout expands bed/LFE/dynamic anchors and counts dynamic slots.  The
layout relation is deliberately classified structural, not semantic, because
`ObjectScene.semantic_binding` remains `Unresolved` and the bridge has no
verified row-to-object association.  See [`index-domain-map.json`](index-domain-map.json).

### Tier C — historical and corpus evidence

The existing J2R1/J2R4 reports contain useful falsification and slot census
work, but none observes an authored trajectory continuing on a known decoded
row through reordering, absent entries, or a reset.  They therefore do not
contradict the unresolved edge.  Four user-owned commercial carriers were
decoded with `diagnose-oamd --all-access-units`; each first access unit had 16
OAMD entries (15 dynamic plus one LFE) and 15 JOC output objects.  The OAMD trim
warp mode was reserved/opaque, so the runs are accepted-with-deviation and not
renderer-fidelity evidence.  Brainrot was run twice and produced the same
132,967,916-byte JSON SHA-256.  Census hashes and limitations are in
[`corpus-census.json`](corpus-census.json) and the reclassification is in
[`historical-evidence-reclassification.json`](historical-evidence-reclassification.json).

### Tier D — bounded clean-room vendor oracle

An isolated analyst was allowed to inspect only the user-provided DRP context
and return a sanitized behavioral specification.  The exact Ghidra installation
`D:\\Software\\ghidra_12.1.2_PUBLIC` exists and its related historical project
material was checked read-only; it yielded no independently expressible binding
rule.  DRP 4.2.0.16846 was
queried through ordinary documented interfaces on a controlled dual-object
position-swap carrier.  Its metadata-directory output exposed only
program/frame-level information; WAV output was speaker-feed audio and channel
mapping, not per-object PCM.  Two runs were byte-identical, but no documented
output exposed object ordinals, OAMD slot IDs, per-coordinate descriptors, or a
binding sidecar.  No hypothesis was falsified.  This is recorded as
`CLEAN_ROOM_VENDOR_ORACLE` only; no proprietary code, addresses, decompiled
tables, or reverse-engineering artifacts entered the repository.

## Why implementation stops

The current ADM writer intentionally rejects strict export when semantic binding
is unresolved.  Replacing that guard with an ordinal assumption would create
silently mislabelled dynamic objects: object count equality (15 versus 15),
local list order, and preserved PCM rows do not prove authored identity.  A
neutral static ADM export would not be a reconstructed dynamic ADM and is not
claimed as one.

The new tests are characterization tests for the safe invariants only:

* three-object JOC payloads retain a present/absent/present middle slot;
* state, reset, reconstruction, synthesis/timeline, and OAMD inactive entries
  retain their declared ordinals.

They intentionally do not assert a semantic JOC↔OAMD mapping.

## Reopening criteria

Dynamic ADM may be admitted only after a public specification or a sanitized,
independently reproducible vendor behavioral contract explicitly identifies the
per-object association, including LFE handling, count changes, absent entries,
splice/reset behavior, and timeline ownership.  Until then the correct behavior
is fail-closed and to keep `SemanticBindingState::Unresolved`.

## Provenance and reproducibility

Normative source URLs, versions, hashes, and clause conclusions are in
[`normative-mapping.json`](normative-mapping.json).  Repository paths and exact
code classifications are in [`index-domain-map.json`](index-domain-map.json).
Private media were not copied into this repository.  No release tag, master
branch, LAVFilters-OpenJOC code, public API, or C ABI was changed.
