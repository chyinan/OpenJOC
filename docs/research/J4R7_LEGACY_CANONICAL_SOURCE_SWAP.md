# J4R7 — Legacy canonical-source swap

## Decision

`J4R7_LEGACY_PROJECT_CONTAINER_STILL_DOMINANT_AFTER_SOURCE_SWAP`

Replacing the legacy static-FR source region with an exact 193,536-sample prefix of the admitted canonical 997 Hz source did not change its ReconstructionBasis support phenotype. The old carrier and the new source-swap carrier both support only `row_000`; the admitted new-lineage target-only carrier supports `row_000..row_008` in the same frozen analysis window.

This closes the legacy-S0 blocker as residual project/container-lineage dependence. It does not identify the responsible hidden producer state, and no further S0 archaeology is recommended.

## Controlled intervention

The disposable project retained the legacy track, Object routing, +0 dB gain, static Front Right automation, region timing, programme range, and DD+ Music/768 kbps settings. Only the active source region changed. The replacement source is the bit-exact prefix `[0,193536)` of the canonical target PCM; the saved project contained exactly that one object-track region after close/reopen.

Both pre-DD+ track bounces reproduced the source prefix exactly and contained only the expected 3,072-sample zero extension to the frozen 4.096-second project range. Two and only two DD+ exports were made from fresh Logic processes. Their MP4 containers differed, while stream-copied raw EC3 was byte-identical (`4a2f2eebe7e6b32d709a2984d854f5605e523ba86ed4d301b1cddfde59de2dfd`).

## Same-head numerical result

All three carriers were decoded at the same OpenJOC HEAD, policy, precision, and source-domain window `[60000,84000)`, mapped to decoded samples `[61536,85536)`.

| Carrier | RB support |
|---|---|
| old legacy S0 | `row_000` |
| legacy project + canonical source | `row_000` |
| admitted new-lineage A | `row_000..row_008` |

The legacy-canonical-to-A projective residual is approximately `0.999999` for RB and `0.858170` for the labeled Base+RB descriptor. The old-to-source-swapped legacy residual is approximately `5.61e-8` for RB. Thus the source swap leaves the legacy RB representation effectively unchanged while the project lineage remains distinct from A.

## Metadata and claim boundary

The new carrier retains the established structural envelope: 129 AUs, 3,072 bytes/AU, payload IDs 11/14/2/1, parser-observed object count 16, element count 2, and warp raw 3 in all AUs. Its payload-11 cadence has three observed values, as in the legacy family. These are empirical structural observations, not hidden-state semantics.

`SemanticBindingState::Unresolved` is unchanged. RB rows remain reconstruction coordinates, not authored objects. Warp raw 3 remains ETSI Reserved. No vendor rule, renderer claim, object/row identity, or audio-bound ObjectScene is admitted.

## Closure

The legacy S0 comparison chain is closed with a residual container/project-lineage limitation. Subsequent work should return to the admitted new-lineage A/B causal chain rather than add another legacy S0 perturbation.

