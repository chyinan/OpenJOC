# J2R4 — Existing-Corpus OAMD Slot Natural-Experiment Census

**Classification:** `EXISTING_CORPUS_SLOT_EFFECT_NOT_IDENTIFIABLE`  
**Scope:** existing frozen ReconstructionBasis/OAMD analysis windows only  
**New Logic fixtures/media:** none  
**SemanticBindingState:** `Unresolved`

## Question and boundary

J2R4 asks whether the already frozen corpus contains an admissible natural
experiment that separates observed reconstruction-row/slot structure from
position, source frequency, context, history, or base/basis effects. It does
not assign authored-object identity to a row, and it does not interpret
`warp_mode = 3`. No producer application was launched and no new media was
created.

The census uses nine usable J1R10 carrier groups and thirteen predeclared
stable analysis windows. The observed dominant row/slot labels are retained as
structural observations. Because no independently verified within-scene slot
intervention exists, every source-to-slot association is recorded as
`AMBIGUOUS` rather than as authored-object evidence.

## Census result

| Measure | Result |
| --- | ---: |
| usable carrier groups | 9 |
| analysis windows | 13 |
| observed slot 0 windows | 11 |
| observed slot 3 windows | 2 |
| direct source-to-slot admissions | 0 |
| ambiguous source-to-slot observations | 13 |
| exact independently admitted slot contrasts | 0 |
| pairwise window comparisons | 78 |
| descriptive design matrix | 13 observations × 13 columns |
| exact rank / nullity | 12 / 1 |

The pairwise search found no exact same-authored-scene, same-position,
same-context, independently verified slot contrast. The useful near controls
remain confounded: same-position/source comparisons cross context/history;
same-position/context comparisons change source frequency; and the static
Front Right versus dual-object Front Right comparison changes both context and
observed slot. These observations narrow the alternatives but cannot identify
a causal slot effect.

## What is and is not separated

The corpus does provide descriptive controls for several factors:

- single-object 997 Hz position controls, including half-step X and Y-mid;
- Z-calibration dwell windows with X/Y held fixed;
- same-position cross-frequency comparisons inside the J1R9 dual-object
  context, with stable observed slots (Front Left slot 0 and Front Right slot
  3);
- a static-versus-dual Front Right contrast in which the observed slot changes.

The last contrast is not a slot intervention. Position, object population,
carrier context, and history are not held fixed at the level required for
causal admission. The J1R11 track-order result is also not a slot oracle: the
track-order change persisted while the raw carrier and OAMD trajectories stayed
byte/trajectory-identical.

An exact incidence audit using treatment-coded position, source frequency,
context, and observed slot factors has rank 12 with one null dimension. This is
an explicit confounding diagnostic, not a fitted semantic model. The missing
contrast is a pair of otherwise identical two-object scenes with reciprocal,
independently verified OAMD slot assignments and unchanged positions,
timing, source fields, and context. J2R4 is not authorized to create it.

## Decision

`EXISTING_CORPUS_SLOT_EFFECT_NOT_IDENTIFIABLE`

The strongest supported statement is that the existing corpus contains stable
structural row/slot patterns and several informative controls, but no
admissible natural experiment that identifies authored-object-to-slot binding
or a causal slot effect. `SemanticBindingState` remains `Unresolved`;
authored-object PCM and an audio-bound `ObjectScene` remain inadmissible. No
JOC, renderer, vendor rule, warp interpretation, or production parser change
was performed.

The single highest-value future observation would be the reciprocal,
independently OAMD-verified two-object slot intervention described above. It is
only a recommendation for a separately authorized milestone, not a result of
this census.

The machine-readable evidence is in
[`J2R4_EXISTING_CORPUS_SLOT_CENSUS.json`](J2R4_EXISTING_CORPUS_SLOT_CENSUS.json).
