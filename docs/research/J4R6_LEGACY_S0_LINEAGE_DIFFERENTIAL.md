# J4R6 — Legacy S0 lineage differential

## Decision

`J4R6_LEGACY_S0_DIFFERENCE_NARROWED_TO_MULTIPLE_BOUNDED_AXES`

J4R6 reproduced the legacy/new phenotype at one decoder HEAD, but the frozen
corpus does not contain a bridge that independently varies source PCM while
holding the project lineage fixed, or vice versa. The unexplained boundary is
therefore narrowed to a small, explicit set rather than one causal axis.

## J4R5 consequence

J4R5 established that adding one exact-zero Object-routed companion shell to
the source-matched target-only project changed none of the four raw EC-3
outputs. `A0 = A1 = B0 = B1` byte-for-byte. Exact-zero companion presence is
therefore eliminated for that tested configuration. The older S0-to-S1
association remains source/project-lineage confounded.

## Endpoints and common window

Legacy `S0_STATIC_FR` is the independently repeated static Front Right carrier
with raw SHA-256 `1d349872f81d2d477ff18360a4782700fb88a823473c51bc7f257f426f20ddb7`.
New `A_TARGET_ONLY` is J4R5 A0/A1, raw SHA-256
`94ca909823a9480881add1a9ef20b1c522f7078f8e47032b110ebeadffcfb19a`.

The common source-domain steady interval `[60000,84000)` was frozen before
coefficient inspection. It is the earlier frozen legacy steady window and is
active in both recovered sources. With the independently retained one-AU
decoder delay, the analyzed interval is `[61536,85536)`.

## Same-HEAD phenotype reproduction

Both carriers were decoded at commit `29b24f39e33a498d3c48a93f9558f201e15cf96d`
with `DOLBY_VENDOR_COMPAT`, one trim configuration, `CurrentDefault`, and
reference-f64 precision. Two compact analyses were byte-identical.

| Partition | Projective residual | Coherence |
|---|---:|---:|
| Base | 0.0042046975543231615 | 0.9999911602201677 |
| ReconstructionBasis | 0.9999989917408474 | 0.0014200412981607976 |
| Joint labeled descriptor | 0.8611192789020674 | 0.5084029774727759 |

S0 supports only `row_000`; A supports `row_000..row_008`. Rows
`row_001..row_008` are gained. RcLfe remains zero and separate. This reproduces
the phenotype to be explained; it does not choose its cause.

## Source PCM differential

Both sources are mono 48 kHz 24-bit PCM at 997 Hz and begin at sample zero,
but they are not the same waveform.

| Field | Legacy S0 | New A |
|---|---:|---:|
| WAV SHA-256 | `8664d74090fa5072c88eb0a231f80a232cbfb31a65caab418acba6067839fc96` | `611587d6369b6bbd58afdfe06b4d307a5eed9fd31eda5ef84abd56a341a51570` |
| PCM SHA-256 | `37ef436c0898d8badef429ecb5f48e716ff6cfe3dd6fc86f58d8a85bcef4c536` | `502431d4b8d16c62e67a27f7ab9a4655c9e2c16e6183ae1b68a4fed213fb1287` |
| Samples | 193,536 | 288,000 |
| Duration | 4.032 s | 6.0 s |
| Peak | 0.1080000401 | 0.2000000477 |
| RMS | 0.0595947631 | 0.1414213566 |
| Whole-file 997 fit | 0.0776187056 | 0.2000000005 |
| DC | 0.0000063310 | 0 |

The legacy signal has a lower, varying envelope. A is a constant-amplitude
0.2 sine. Neither source has a leading or trailing exact-zero sample run;
fades and normalization are not independently recoverable beyond the samples
and frozen authoring records.

## Track, object, and project lineage

Position `(X=+1,Y=+1,Z=0,Size=0)`, Object routing, 0 dB gain, mute/solo-off,
48 kHz, and absence of observed plug-ins match. Track and region identities do
not: S0 uses `J1_OBJ_TAG` / `J1_B_CENTER_997_object_1`, while A uses
`OBJ_997HZ` / `TARGET_997`.

S0 descends from the older `J1_B_CENTER_997` positional fixture family. A
descends from the admitted J4R1 ADM-propagation project and a target-only
clone. Save/reopen evidence exists for both. The exact legacy Logic build and
all serialized internal state are not frozen at J4R5 granularity.

Producer format, Music profile, 768 kbps, 48 kHz, and ADM-off settings match.
The export/range result does not: S0 produces a 4.096 s MP4 and 129-AU
(4.128 s) raw stream; A produces an 8.000 s MP4 and 251-AU (8.032 s) raw
stream around a six-second source. Source duration, project end/export range,
and producer tail are therefore not separable from the existing evidence.

## Encoded structural differential

Both carriers use 3,072 bytes per AU, payload IDs `11,14,2,1`, parser-observed
object count 16, element count 2 with IDs 1/2, 15 reconstruction rows, and raw
warp value 3 throughout. S0 has three payload-11 body states; A has one.
Carrier length and payload-state behavior differ. These are raw observations,
not vendor semantics; warp raw 3 remains ETSI Reserved.

## Within-family stability

The admitted S_FR pair is raw-EC3 byte-identical and retains `row_000`. J4R5
A0/A1 are raw-EC3 byte-identical and retain `row_000..row_008`. Both family
phenotypes are stable under their independently evidenced repeats.

## Frozen corpus and bridge search

Candidate membership was frozen before bridge conclusions: legacy S0, J4R5 A,
J4R2 fixed topology, and the source-exact J3R15 ADM record (provenance only).
The legacy static-position atlas also supplies a position control.

- J4R5 B eliminates the exact-zero shell axis because it is bitstream-identical
  to A.
- J4R2 fixed topology retains the A source/project family and
  `row_000..row_008` with the companion OFF or ON; it does not separate source
  from project lineage.
- Legacy static FL and FR retain the legacy support phenotype, so FR position
  alone does not explain A.
- No frozen carrier combines the legacy project with A PCM, or the A project
  with legacy PCM.

Natural bridges therefore narrow but do not causalize the remaining axes.

## Final difference vector

| Axis | Status |
|---|---|
| SOURCE_PCM | UNMATCHED |
| SOURCE_DURATION | UNMATCHED |
| SOURCE_ENVELOPE | UNMATCHED |
| PROJECT_LINEAGE | UNMATCHED |
| TARGET_TRACK_IDENTITY | UNMATCHED |
| AUTHORING_OBJECT_POPULATION | MATCHED within recovered active topology |
| EXPORT_RANGE | UNMATCHED |
| PRODUCER_SETTINGS | MATCHED |
| ENCODED_METADATA | UNMATCHED |
| OTHER_RECOVERABLE_DIFFERENCE | UNKNOWN |

## Minimum next discriminator

The smallest useful J4R7 is to clone the qualified legacy S0 project and
replace only its target region source with canonical `TARGET_997.wav`, while
retaining the legacy track identity, FR automation, region duration, project
end/export range, and producer settings. Use the corresponding 4.032-second
source prefix, require an exact pre-export bounce, and export two repeats.
This tests the source-PCM axis before rebuilding a project-shell matrix.

## Claim and storage boundary

`SemanticBindingState::Unresolved` is unchanged. Reconstruction rows remain
coordinate labels, not authored objects. No Logic launch, media creation,
producer export, renderer, or vendor rule occurred. Temporary same-HEAD
reference-f64 captures were hashed into compact evidence and deleted; no large
decoded tree was retained.
