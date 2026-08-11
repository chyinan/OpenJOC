# J2R2 — OAMD Slot-Reassignment Causal Experiment

**Date:** 2026-08-11  
**Scope:** protocol preregistration and autonomous-feasibility admission only  
**Classification:** `AUTONOMOUS_PRODUCER_FEASIBILITY_NOT_ESTABLISHED`

## Reviewer question and J2R1 dependency

J2R1 established `MODEL_CLASS_NARROWED_BUT_UNDERDETERMINED`: fixed
authored-object-to-fixed-row identity is falsified, but slot, position, context,
history, and general base/basis transform explanations remain confounded.
J2R2 asks whether the proposed two-object slot-reassignment experiment can
actually distinguish those explanations. It does not generate a carrier and it
does not admit semantic binding.

`SemanticBindingState::Unresolved` remains the production state.

## Clean-room and no-media boundary

This preregistration uses the existing public repository contracts and
private, user-owned J1R9/J1R11/J1R15 evidence. It does not use proprietary
Dolby source, decompilation, leaked material, undocumented Logic project
internals, or another decoder implementation as a semantic oracle.

J2R2 generated no Logic fixture, ADM-BWF, DD+, EC-3, WAV, M4A, or other media.
No producer export or human action was performed. The canonical controlled
corpus was not modified.

## Terminology

The protocol keeps these concepts distinct:

| Term | Meaning in this protocol |
| --- | --- |
| authored source | controlled 997 Hz or 2003 Hz waveform |
| authored track | Logic track carrying that source region |
| producer object | producer-side object/track representation |
| OAMD dynamic slot | decoded dynamic metadata slot index |
| ReconstructionBasis row | structural JOC reconstruction row index |
| position/state | decoded OAMD/ADM spatial state and its timing |
| base PCM | base-carried decoded PCM component |
| basis contribution | ReconstructionBasis contribution; never an authored-object stem |

GUI track order is not assumed to equal OAMD slot order. Producer object
identity is not assumed to equal authored-object semantic identity. A frequency
component in a row is not called the PCM of the authored object.

## Proposed conditions

The conceptual A/B contrast is:

```text
Condition A
  target:    fixed 997 Hz source at Front Right
  companion: fixed 2003 Hz source at Front Left
  assignment: producer/OAMD assignment A

Condition B
  same sources, positions, object population and timing
  assignment: producer/OAMD assignment B
```

The intended independent variable is the target/companion producer/OAMD slot
assignment. Source waveform, amplitude, phase, start sample, object count and
active state, positions, gains, timing, programme range, duration, sample rate,
pre-roll/history, bed, routing, plug-ins, channel strips, project boundaries,
encoder profile, bitrate, and export settings are controlled variables.

The candidate producer operation is a documented object-assignment control or
an otherwise observable operation that changes the decoded OAMD slot while
preserving those controls. A simple vertical track-order reorder is explicitly
not admitted as the intervention: the frozen J1R11 attempt did not preserve a
changed order after save/reopen and did not establish a changed OAMD slot.

## Existing feasibility evidence

The frozen evidence gives three relevant facts:

1. J1R9 successfully used human-assisted automation-lane authoring for
   positions; the lane, not Object Panner, is the authoring source of truth.
2. The position SOP records that Computer Use cannot reliably drag Logic
   automation values. Human value entry is therefore required whenever a new
   position value is authored.
3. J1R11 attempted the smallest non-position operation—swapping the two object
   tracks—while preserving source, automation, routing, and names. After
   save/reopen, the track order was unchanged and the declared slot-permutation
   gate failed. The operation therefore did not prove a changed OAMD slot.

These are not Logic persistence failures. They are evidence that the proposed
slot intervention is not yet an independently observable producer control.
No non-rendering dry run was needed: the prior frozen failure is directly
relevant and J2R2 forbids new Logic interaction or human action.

## Causal hypotheses

### H_POS

Within the fixed two-object context, the target vector is determined by target
position/state and the otherwise fixed scene context, not by the slot carrying
the target. After admitted gauge alignment, A and B remain equivalent.

### H_SLOT

The target vector depends materially on OAMD slot/state assignment. After all
metadata and base controls pass, the target vector changes beyond the frozen
material-change threshold when the target moves between independently verified
slots.

### H_ROW_PERMUTATION

The row-indexed results differ only by one shared, independently constrained row
permutation applied consistently to target and companion. This is reported
separately from a changed coefficient transform.

### H_BASE_REALLOCATION

The producer changes the base/basis decomposition. A basis-only difference is
then insufficient evidence for slot-dependent coefficients.

### H_CONTEXT_OR_REOPTIMIZATION

Another encoder or context variable changed. This is inconclusive for H_SLOT.

### H_HISTORY

Startup, pre-roll, preceding-state, or time-dependent history differs.

### H_NULL_INVALID

The intended slot intervention is not observable or was not successfully
applied.

The protocol must preserve these alternatives; it must not force a binary
H_POS-versus-H_SLOT choice.

## Required observations and estimators

If a future producer control is first admitted, both conditions must provide:

- raw per-row amplitude/energy and a complete normalized 15-row magnitude
  vector for both 997 Hz and 2003 Hz;
- coherent least-squares sinusoidal coefficients, including phase only when a
  declared reference and sample alignment make it identifiable;
- total ReconstructionBasis energy and corresponding base PCM frequency
  components, including relevant `RcLfe` measurements;
- exact OAMD slot/state/position sequence, frame/AU alignment, decoder state,
  profile, and carrier/output hashes;
- time-window consistency after identical pre-roll and startup handling.

The future analysis interval must be frozen before media generation. It must
retain all rows, use L2 normalization after extraction, retain zero/near-zero
rows as measurements, and avoid arbitrary FFT-bin selection. Phase comparison
is currently `NOT_OBSERVABLE` from the magnitude-only J1R10 outputs.

## Threshold preregistration

Thresholds are fixed from existing repeatability and precision evidence, not
from a future A/B result:

| Gate | Preregistered rule | Evidence source |
| --- | --- | --- |
| same-condition repeat | raw carrier and canonical f64 row artifacts byte-identical | J1R9/J1R10/J1R15 deterministic repeats |
| unavoidable f32 comparison | max absolute sample difference ≤ `1e-6` | J1R15 f32/reference-f64 comparison |
| normalized-vector equivalence | L2 distance ≤ `1e-6` after admitted alignment | J1R10 vector representation and J1R15 repeatability |
| material vector change | L2 distance > `1e-3`; interval between is `INCONCLUSIVE` | fixed before future output, informed by J1R9/J1R10 distances |
| base invariance | exact channel-wise equality where supported; otherwise max absolute difference ≤ `1e-6` and no material frequency-energy redistribution | J1R15 precision and base/basis separation |
| metadata equivalence | exact object count, source identity, position/state/timing except intended slot index | J1R9/J1R11 ADM/OAMD gates |

No arbitrary row permutation search or post-hoc threshold tuning is allowed.

## Base/basis confounder

`ReconstructionBasis` is not the entire encoded sound field. A future A/B
comparison must first compare base PCM and base-plus-basis energy. If base
allocation changes beyond the repeatability gate, classify
`H_BASE_REALLOCATION` or `H_CONTEXT_OR_REOPTIMIZATION`; do not call a basis-only
difference H_SLOT. If the base/basis joint observable cannot be measured,
causal slot attribution remains unresolved.

## Row-permutation handling

The only admissible permutation is one independently constrained by a common
structural observation and applied consistently to target and companion. A
free search over 15! permutations would overfit row labels and is prohibited.
If no independent permutation calibration exists, report row-label ambiguity
and do not select H_SLOT.

## Independent slot-verification gate

Before any future carrier is analyzed, all of the following must agree:

1. a public-safe authoring manifest describing the intended operation;
2. source-frequency identity and unchanged authored sound field;
3. decoded OAMD slot/state/position records showing exactly one assignment
   change;
4. deterministic hashes and unchanged metadata outside that assignment.

Belief that GUI track order controls OAMD order is not evidence. Failure of
this gate is `H_NULL_INVALID`.

## Producer automation feasibility

The existing workflow requires Logic Pro and OpenJOC tooling. Position
authoring is human-assisted through the automation lane; Object Panner is
readback only. The prior J1R11 track-order operation did not persist, and no
documented producer API or observable control has been shown to set a decoded
OAMD slot independently. Because the intended intervention cannot presently
be applied and verified by Codex without human action, autonomous producer
feasibility is **not established**.

J2R2 therefore does not authorize a disposable dry run, export, or request for
human intervention. A future workflow would need deterministic project-copy
creation, exact condition naming, save/reopen verification, failure detection,
and decoded OAMD verification before export could be considered.

## Minimality review

If a real slot-only control becomes available, two unique conditions are the
smallest A/B contrast: one target and one companion, with the same sources and
positions and only their assignment exchanged. Same-condition repeatability
must be an acceptance gate, not a third causal condition. Additional carriers,
third slots, silent objects, extra frequencies, or longer durations are not
justified until the intervention itself is proven. More carriers cannot repair
an unverified independent variable.

The future candidate count is therefore **two**, but it is not admitted for
execution by J2R2.

## Final classification

`AUTONOMOUS_PRODUCER_FEASIBILITY_NOT_ESTABLISHED`

This is a feasibility boundary, not evidence for H_SLOT or H_POS. The
slot-reassignment experiment remains scientifically valuable, but its proposed
Logic-side intervention has not been demonstrated to change only OAMD slot
assignment, and Codex cannot currently execute and verify it autonomously.

No new media was generated. `SemanticBindingState::Unresolved` remains. No
warp/vendor interpretation, authored-object PCM, audio-bound ObjectScene,
renderer, or production semantic rule was added. J2R3 is not authorized.
