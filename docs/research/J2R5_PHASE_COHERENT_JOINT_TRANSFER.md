# J2R5 — Phase-Coherent Joint Base/ReconstructionBasis Transfer

**Classification:** `JOINT_TRANSFER_MODEL_NARROWED_BUT_UNDERDETERMINED`  
**Scope:** frozen J1R9/J1R10 component artifacts; no producer operation  
**SemanticBindingState:** `Unresolved`

## Scope and evidence boundary

J2R5 follows J2R4's negative slot-identifiability result. It does not fit a
slot effect and does not infer source-to-slot or row-to-object identity. It
asks the narrower numerical question: which properties of the decoded
base/ReconstructionBasis component transfer remain observable when phase,
time, base PCM, ReconstructionBasis rows, RcLfe, source frequency, and
single/dual context are considered together?

Only frozen evidence was read. No Logic process was launched, no project was
modified, and no new ADM, DD+, EC3, WAV, or other media was generated.

## Inventory and component partitions

The audit used the nine J1R15/J2R4 usable carrier groups, J1R9's reciprocal
997/2003 dual-object windows, the J1R10 row-signature inventory, and existing
base PCM artifacts. The canonical coordinate descriptor is:

1. base full-band: `FL`, `FR`, `FC`, `SL`, `SR`;
2. base LFE: `LFE` (kept separate);
3. ReconstructionBasis: `row_000` through `row_014` in decoder order;
4. RcLfe: a separate component when directly available.

The parent ReconstructionBasis artifacts contain deterministic **magnitude-
only** tone vectors. They do not contain per-row PCM or phase. J1R9 source PCM
hashes are retained, but an independently re-established source linkage for
every static carrier is not present. Therefore the complex joint vector is
only partially observable.

## Estimator and oracle

For existing base PCM, the private tool uses simultaneous sine/cosine least
squares at the declared 997 Hz and 2003 Hz frequencies, with a constant term,
at 48 kHz. The complex convention is `cosine - i*sine`. It reports the Gram
matrix condition-number estimate, residual RMS, coefficient, complex
coherence, and projective residual. No peak search, frequency retuning, or
post-hoc window selection is used.

An independent synthetic oracle recovered both tones to below `1e-9`, preserved
global complex scale/phase gauge invariance, detected a row-specific phase
change, handled zero vectors, and preserved the declared channel order. The
oracle validates the estimator; it does not manufacture missing phase from the
controlled corpus.

Windows were inherited before this comparison: static `1.25–1.75 s` and
`2.75–3.25 s`; J1R9 dual pre `1.25–1.75 s` and post `2.75–3.25 s`. No target
dependent threshold was selected. Continuous metrics are reported because a
binary equivalence threshold for the incomplete complex joint vector is not
supported by the frozen evidence.

## Matched strata and numerical results

Four base-complex comparisons were available:

| Comparison | Complex coherence | Projective residual |
| --- | ---: | ---: |
| dual FL, 997 ↔ 2003 | 0.9999934884 | 0.0036087600 |
| dual FR, 2003 ↔ 997 | 0.9999931464 | 0.0037023227 |
| single FL 997 ↔ dual FL 997 | 0.9999939278 | 0.0034848789 |
| single FR 997 ↔ dual FR 997 | 0.9999937288 | 0.0035415135 |

These are base-partition coordinates from compatible frozen artifacts. They
are not a rendered-field comparison and no equivalence claim is attached to
the numerical values without a preregistered threshold.

The ReconstructionBasis cross-frequency proxies reproduce the earlier
magnitude-only result (projective residuals approximately `0.00221` for FL and
`0.00280` for FR when treated as real nonnegative proxies). They cannot be
called complex coherence: the missing per-row phase is decisive. The complete
base-plus-basis complex descriptor is consequently unavailable.

For the three static base controls checked across the two fixed windows, the
base-only projective residuals were `5.25e-7` (Center), `3.88e-7` (Front Left),
and `3.93e-7` (Front Right), with coherence above `0.9999999999998`. This
supports a base-only `TIME_STABLE_WITHIN_TESTED_INTERVAL` observation. It does
not establish ReconstructionBasis phase stability.

## Base/basis, RcLfe, and superposition boundaries

The base partition is numerically measurable in the available artifacts; the
ReconstructionBasis phase partition is not. A complete base/basis covariance
or compensation statement is therefore **not observable**. The data support
only wording such as “base-partition coefficient differences co-occur with the
tested context comparison”; they do not prove physical compensation or final
sound-field equivalence.

RcLfe remains separate under the J1R15 architecture. No RcLfe samples are
folded into the 15 ReconstructionBasis rows. A component-level linear
superposition residual is `H_SUPERPOSITION_NOT_OBSERVABLE`: there is no
phase-preserving dual ReconstructionBasis PCM target and no complete matched
singleton/dual complex descriptor.

## What the corpus identifies

The corpus identifies deterministic base-channel complex coefficients for the
declared frozen windows, magnitude-only ReconstructionBasis vectors, and
scoped base-only frequency/context/time comparisons. It narrows the model
space: the earlier magnitude similarity is not sufficient evidence for a full
complex transfer invariant, while the base partition shows small but nonzero
projective residuals in every tested cross-context/frequency comparison.

It does not identify full complex ReconstructionBasis transfer, absolute
source-to-component phase for every carrier, base/basis physical compensation,
component-level superposition, authored-object identity, slot semantics, a
renderer, or universal frequency/context behavior. `warp_mode = 3` remains an
ETSI strict reserved value and is untouched.

## Final classification

`JOINT_TRANSFER_MODEL_NARROWED_BUT_UNDERDETERMINED`

Phase-aware base measurements supply new constraints, but the frozen
ReconstructionBasis evidence is magnitude-only. The remaining models cannot
be uniquely separated without new phase-preserving decoded component evidence;
this milestone is not authorized to create it. The machine-readable summary
is in [`J2R5_PHASE_COHERENT_JOINT_TRANSFER.json`](J2R5_PHASE_COHERENT_JOINT_TRANSFER.json).
