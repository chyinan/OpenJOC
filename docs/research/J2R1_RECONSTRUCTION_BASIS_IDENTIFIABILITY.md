# J2R1 — ReconstructionBasis Semantic Identifiability

**Date:** 2026-08-11  
**Scope:** existing controlled evidence only  
**Result:** `MODEL_CLASS_NARROWED_BUT_UNDERDETERMINED`

## Scope and non-goals

This milestone asks which mathematical properties of the relationship between
OAMD state/position, E-AC-3 base PCM, and diagnostic `ReconstructionBasis`
rows are identifiable from the already frozen corpus. It does not implement a
renderer, infer vendor syntax, generate media, or admit semantic object audio.

The production boundary remains:

```text
OAMD metadata/state -> metadata-only ObjectScene
JOC reconstruction -> ReconstructionBasis rows
SemanticBindingState = Unresolved
```

The rows are structural reconstruction-basis outputs. They are not authored
object stems, and a dominant row is not an authored object or verified object
PCM.

## Clean-room and provenance boundary

The analysis uses only public repository code/contracts and private,
user-owned controlled evidence already frozen by J1R9, J1R10, and J1R15. No
Dolby proprietary source, decompilation, leaked documentation, Cavern source,
new Logic interaction, new ADM export, new DD+ export, or new EC-3 media was
used. Private paths and media remain outside the repository; the public-safe
inventory records fixture labels, roles, and stable hashes only.

The complete response-level reviewer authorization was copied into the private
`LAST_REVIEWER_RESPONSE.md` before this work began. No follow-on milestone is
authorized by this report.

## Evidence inventory

The usable corpus is the nine-carrier J1R15 population:

| Evidence label | Controlled context | Stable input SHA-256 (from frozen inventory) | Reconstruction use |
| --- | --- | --- | --- |
| `CENTER_997` | single object, X=0, Y=+1, Z=0 | `a6dbfe2a8f28f36bddd863e17be219190f47a8e88c3cd587c78ad9b2a270b6b0` | 129 AUs, 15 rows |
| `FRONT_LEFT_997` | single object, X=-1, Y=+1, Z=0 | `948e774b998121457b91da006d92c4bc62f976bcc6515cbca52d0345efca6cc5` | 129 AUs, 15 rows |
| `FRONT_RIGHT_997` | single object, X=+1, Y=+1, Z=0 | `1d349872f81d2d477ff18360a4782700fb88a823473c51bc7f257f426f20ddb7` | 129 AUs, 15 rows |
| `REAR_CENTER_997` | single object, X=0, Y=-1, Z=0 | `ac3d198731e6251150e6a50b6447508194067ecea423ef6069e4851ba6b58026` | 129 AUs, 15 rows |
| `X_NEG_HALF_997` | single object, X=-0.5, Y=+1, Z=0 | `85f5e9a9ed1f4421d33fb03a7a243efb934147b4d1723a7909880961e590d409` | 129 AUs, 15 rows |
| `X_POS_HALF_997` | single object, X=+0.5, Y=+1, Z=0 | `0f71f7eaad8fee1136395822acc05ac5fe6e8f456cca5998e05a7600ae4557e5` | 129 AUs, 15 rows |
| `Y_MID_997` | single object, X=0, Y=0, Z=0 | `489a36d314ce1171c2b71ab918eaf4cdbee466284f278327809625f674f14975` | 129 AUs, 15 rows |
| `J1R8_Z_CAL_997` | existing Z trajectory, X=0, Y=+1 | `3ba31973beebe9d299c8bdd81b649a887da28ceea18f1a7f0d15570b79950dd4` | 128 AUs, 15 rows; Z windows are time-varying |
| `J1R9_DUAL_OBJECT_SWAP_997_2003` | two authenticated tones; FL/FR occupancy swap | `d35aee5421e965d2fa0eb80d4b6dd071ba719dcd12686a40bf8a87cacfdc452e` | 125 AUs, four matched windows, 15 rows |

The historical standalone Center/2003 control remains explicitly excluded
because it lacks the frozen qualification boundary. J1R15 established the
structural facts used here: 15 rows where present, exact 1536-sample AU
shape, no truncation/padding, finite values, repeated numerical output, and
separate base-carried `RcLfe`.

## Notation and measured observables

For a frozen analysis window, let

```text
y_r(t)          ReconstructionBasis row r
m_r(f)          magnitude-only least-squares projection of y_r onto tone f
v(f)            [m_0(f), ..., m_14(f)]
v_hat(f)        v(f) / ||v(f)||_2
```

The measured vector is magnitude-only. Phase, sample-aligned complex
coefficients, and a verified source-to-row transfer function are not
available. Comparisons therefore use the complete 15-component normalized
vector, cosine similarity, L2 distance, rank correlation, dominant-row index
as a descriptive categorical value, and row-wise ranges. These are derived
measurements, not semantic labels.

The J2R1 protocol was frozen before analysis. Canonical output is UTF-8 JSON
with sorted keys, two-space indentation, and a trailing newline. No numerical
threshold was tuned after seeing the result. A fixed-row-identity rejection
uses the exact categorical observation that the authenticated tone changes
row while the stable position/context row changes; no arbitrary amplitude
cutoff is required.

## Candidate model taxonomy

The following axes are kept independent:

1. fixed row identity versus distributed row participation;
2. position-only coefficients versus position-plus-slot/context coefficients;
3. frequency-independent/weakly frequency-dependent vectors versus materially
   frequency-dependent transforms;
4. frame-local/memoryless behavior versus history/state-conditioned behavior;
5. context-independent linear superposition versus context-conditioned or
   nonlinear behavior;
6. authored-object identity dependence versus OAMD slot/state dependence;
7. scalar row gains versus a general transform involving base PCM, basis PCM,
   time, or frequency.

The observable quantities are the row vectors, their stable shapes and hashes,
finite/deterministic status, and the OAMD context in which they were measured.
Normalized vectors, cosine/L2 values, dominant rows, and axis ranges are
derived under the declared spectral and alignment assumptions. They do not
identify an authored object.

## Tests and deterministic results

The private J2R1 harness recomputed measurements from the frozen J1R10 row
vectors and J1R15 structural reports, then ran twice from equivalent inputs.
All machine-readable outputs and the text summary were byte-identical. The
private run is identified as `20260811T083854Z_j2r1-identifiability_813f824`;
its aggregate evidence freeze is recorded outside the repository.

### Same-position cross-frequency comparison

Within the single J1R9 dual-object carrier, changing which authenticated tone
occupies a stable position preserves the complete normalized row distribution
to high numerical similarity:

| Stable position | Comparison | Cosine | L2 distance | Dominant row |
| --- | --- | ---: | ---: | --- |
| FL | 997 Hz → 2003 Hz | 0.9999955568 | 0.0029809980 | 0 → 0 |
| FR | 2003 Hz → 997 Hz | 0.9999960679 | 0.0028043272 | 3 → 3 |

This is evidence for a repeatable within-carrier position/context signature.
It is not proof of a universal frequency-independent transform.

### Single-versus-dual context comparison

At Front Right with the 997 Hz tone, the independently qualified single-object
control is row-0 dominant, while the J1R9 dual-object post-swap window is
row-3 dominant. Their complete normalized vectors have cosine `0.0010131503`
and L2 distance `1.4134969754`; the dominant-row sets are disjoint. This is a
direct same-position context contradiction to a globally fixed position-only
row identity. The corresponding FL comparison remains highly similar
(cosine `0.9999943719`, row 0 → row 0), so the result is not reducible to a
simple universal “all dual objects differ” rule.

### Existing X/Y/Z observations

The seven static 997 Hz controls across X=`-1,-0.5,0,+0.5,+1` and Y=`-1,0,+1`
are all row-0 dominant. Their non-row-0 normalized ranges stay near the
detector floor (largest static X range among rows 1–4 is approximately
`9.17e-8`; largest static Y range is approximately `8.49e-8`). This is a
descriptive observation, not a fitted X/Y basis law.

The existing J1R8 Z trajectory has a visible row-4 magnitude change between
the selected Z=0.5 and Z=1 windows (`1.1991e-9` to `0.00601939`). However,
the OAMD state is not constant across the selected post window. The response
is therefore compatible with a position response, an update/state response,
or their combination; it is not an isolated Z coefficient identification.

### Numerical and structural controls

J1R15 reports remain passing within their declared numerical scope:

- all nine usable carriers have 15 structural rows and exact AU-shaped sample
  counts (129, 128, or 125 AUs as applicable);
- repeated output is deterministic under the frozen representation;
- no NaN/Inf, unexplained amplitude growth, truncation, or padding was found;
- startup history is retained rather than silently trimmed;
- QMF state is carried across sequential AUs and reset only for explicit
  sequence/configuration discontinuities;
- `RcLfe` stays base-carried and separate from dynamic rows;
- diagnostic export remains `diagnostics/reconstruction_rows/row_NNN.wav`.

These controls establish numerical trust in the exposed basis rows, not their
semantic identity.

## Model decisions

| Model class | J2R1 status | Evidence boundary |
| --- | --- | --- |
| one authored object → one fixed row | **FALSIFIED** | J1R9 tone/occupancy exchange follows stable carrier position/context rather than authored tone identity |
| fixed OAMD slot → row | **SUPPORTED BUT UNDERDETERMINED** | static slot-0/row-0 and J1R9 FL slot-0/row-0, FR slot-3/row-3; no causal slot reassignment |
| globally fixed position-only basis | **SUPPORTED ONLY IN LIMITED CONTEXT** | strong J1R9 same-position cross-frequency similarity, contradicted by single FR versus dual FR |
| distributed position-plus-slot/context basis | **SURVIVES** | same position/frequency changes row distribution across contexts |
| history/state-conditioned transform | **NOT DISCRIMINATED** | state carry is structurally validated, but state, slot, and context causes are not isolated |
| context-independent linear superposition | **NOT OBSERVABLE** | no admissible phase-preserving mixed-source target for a residual test |
| general base/basis/time/frequency transform | **SURVIVES AND UNDERDETERMINED** | magnitude-only rows and base/basis decomposition leave equivalent parameterizations |

The previously rejected authored-object-to-row model remains rejected. No
surviving model is promoted to production semantics.

## Identifiable and non-identifiable parameters

Identifiable within the tested corpus and declared measurement method:

- row cardinality and per-row sample shape for the accepted carriers;
- finite/deterministic output status;
- the complete magnitude-vector signatures for each frozen window;
- the fact that J1R9 FL/FR signatures are stable under the tested tone swap;
- the fact that the single FR/997 and dual FR/997 signatures differ in row
  distribution;
- descriptive correlations between the observed rows and the recorded OAMD
  context.

Not identifiable:

- authored-object identity of any row;
- a causal OAMD slot-to-row mapping;
- a universal position-only coefficient function;
- a universal frequency-independent transform;
- phase/sign, absolute scale, or producer normalization;
- separation of base PCM from basis contribution as a unique parameterization;
- linear-superposition coefficients independent of context;
- whether the observed context difference is slot, population, state, or a
  more general carrier transform;
- a renderer speaker/binaural mapping.

Gauge freedoms and confounders include row permutation, global scale and
source amplitude, sign/phase ambiguity, base/basis decomposition, overlapping
source content, producer normalization, unmanipulated slot permutation, and
insufficient repeated-state observations.

## Evidence unavailable or unsuitable for fitting

The current corpus does not provide an independently controlled slot
reassignment at fixed position and fixed multi-object population; this is the
highest-value missing discriminator. It also lacks phase-preserving transfer
measurements, an independently decoded mixed-source target for linear
superposition residuals, and enough repeated states to separate history from
context. These tests are `NOT_OBSERVABLE`, not failed.

## Minimum distinguishing experiment (design only)

If a future milestone is authorized, the minimum experiment is one unchanged
two-object Logic project with a fixed 997 Hz target at Front Right and a fixed
2003 Hz companion at Front Left. Export two otherwise identical carriers that
differ only in the target's OAMD dynamic-slot assignment, with positions,
source WAVs, routing, programme range, duration, gain, bed, encoder profile,
sample rate, and timing held constant. The two slot assignments must be
verified in ADM and in the raw metadata; the two carriers must be deterministic
R0/R1 pairs.

Required outputs are the OAMD slot timelines, base PCM, complete 15-row
ReconstructionBasis PCM/vectors, AU timing, and the same magnitude-vector
measurement used here. Under a slot-causal model, the target's row signature
must follow the reassigned slot while the position and tone remain fixed. Under
a position/context model with slot-invariant semantics, it must remain tied to
the unchanged position/context. A smaller single-object experiment cannot
distinguish those hypotheses because it does not expose slot competition. The
experiment would require human Logic authoring under the frozen automation-lane
protocol; Codex cannot execute the producer step without human action. This
design is not authorized or executed by J2R1.

## Semantic-binding consequence

`SemanticBindingState::Unresolved` is unchanged. Authored-object PCM,
audio-bound `ObjectScene`, speaker-layout rendering, binaural rendering, and
ObjectScene row binding remain inadmissible. No production code, profile, JOC
mapping, or renderer behavior was changed. ETSI strict `warp_mode [526,528) =
raw 3` remains `ReservedWarpMode`; J2R1 did not inspect or assign meaning to
the opaque continuation.

## Final classification

`MODEL_CLASS_NARROWED_BUT_UNDERDETERMINED`

The corpus rules out fixed authored-object row identity and establishes several
repeatable numerical/context constraints. It does not identify the missing
OAMD-to-ReconstructionBasis transformation uniquely. Multiple slot,
position-plus-context, state-conditioned, and general transform models remain
observationally equivalent under the available evidence.

No new Logic fixture, ADM export, DD+ export, EC-3 file, or other media was
generated during J2R1. Do not begin J2R2 without a new reviewer authorization.
