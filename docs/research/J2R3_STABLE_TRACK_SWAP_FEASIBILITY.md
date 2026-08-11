# J2R3 — Stable-Track Content/Automation Swap Feasibility

**Scope:** non-rendering producer-intervention feasibility only  
**Classification:** `AUTONOMOUS_PRODUCER_FEASIBILITY_NOT_ESTABLISHED`  
**Media generated:** none  
**SemanticBindingState:** `Unresolved`

## Relation to J2R1 and J2R2

J2R1 narrowed the ReconstructionBasis model while leaving slot, position,
history, context, and base/basis explanations underdetermined. J2R2
preregistered a two-condition slot-reassignment comparison and showed that the
previous Logic track-order operation was not an independently verified slot
intervention. J2R3 evaluates a safer producer operation: keep two existing
object-track shells fixed and exchange the authored regions and their complete
object automation on disposable copies. It does not export, bounce, decode, or
admit semantic binding.

## Why track reordering is rejected

The frozen J1R11 post-freeze evidence shows that track order changed and
survived save/reopen, but the raw carrier remained byte-identical to the
baseline and all 15 stable OAMD trajectories were unchanged. Track display
order is therefore not an OAMD-slot oracle and is not used by this protocol.

## Proposed stable-track intervention

Let `T_A` and `T_B` be two fixed producer object-track shells. Condition A puts
the 997 Hz/Front-Right authored pair on `T_A` and the 2003 Hz/Front-Left pair
on `T_B`. Condition B exchanges those complete authored pairs while leaving
the shells fixed. A valid intervention must exchange source region identity,
timing, offsets, gain/fades/mute/loop state, and every authored object
automation lane (position, gain, size/spread/divergence, and active state where
observable). Routing, channel strips, plugins, sends, output assignment,
project Atmos settings, bed, sample rate, timing, and unrelated tracks must
remain unchanged.

This project-level exchange would establish only authored-sound-field
equivalence. It would not prove an OAMD slot reassignment. A future,
separately authorized export must parse both carriers and verify reciprocal
slot assignment, equal position/state trajectories, and unchanged metadata
outside the intended assignment. `PROJECT-LEVEL SWAP SUCCESS IS NOT OAMD
SLOT-REASSIGNMENT PROOF`.

## Feasibility audit result

No reusable, private, safe script was found that can perform this exact Logic
operation without relying on undocumented project internals. The existing
workflow is human-assisted for automation-lane authoring; Object Panner is
readback only. During this audit, two attempts to obtain the visible Logic Pro
state (`com.apple.logic10`) timed out. Consequently Codex could not safely
identify the two track shells, verify their complete automation inventories,
create a disposable copy through the producer UI, or detect save/reopen
completion. No click, drag, swap, save, reopen, bounce, or export was attempted
after that observation.

Because the intervention could not be completed and independently verified,
the two-run dry-run requirement is `NOT_RUN`, not a pass or a media result.
The narrow classification is therefore:

`AUTONOMOUS_PRODUCER_FEASIBILITY_NOT_ESTABLISHED`

This is not evidence that the stable-track design is scientifically invalid;
it is a producer-control and observability boundary. No human action is
requested in this milestone, and no new Logic fixture or media is authorized.

## State snapshot contract

Any future admitted dry-run must record public-safe normalized snapshots for
project state, each track shell, authored regions, all relevant automation,
protected unrelated state, and fields that are unobservable. The comparison
must distinguish fields expected to stay identical, fields expected to exchange,
volatile UI fields ignored, and fields not observed. Logic package bytes must
not be interpreted as proprietary syntax and need not be byte-identical.

## Save/reopen and repeatability gates

For each fresh disposable copy the required sequence is baseline snapshot →
swap → immediate snapshot → save → close → reopen → reopened snapshot. Two
independent copies must produce the same normalized transformation and
classification without human interaction or unresolved dialogs. These gates
were not run because the producer UI could not be observed reliably.

## Automation and safety boundary

The candidate mechanism would use verified semantic UI selection (or an
existing trusted interface), never an unverified screen coordinate. Required
permissions, project-open detection, track selection, action completion, save
completion, dialog detection, and cleanup must be recorded before admission.
The canonical project and private producer assets remain untouched.

## Current claim boundary

This milestone does not establish OAMD slot dependence, track-to-slot identity,
row-to-object identity, authored-object PCM, an audio-bound ObjectScene, a
renderer, universal frequency independence, or any warp raw-3/vendor meaning.
`SemanticBindingState::Unresolved` remains unchanged.

## Final classification

`AUTONOMOUS_PRODUCER_FEASIBILITY_NOT_ESTABLISHED`

The next useful step is a reviewer-authorized producer-control milestone only
after a reliable, non-human Logic automation channel is available. If one is
found, the smallest admitted experiment remains two disposable conditions plus
same-condition repeatability; no media should be generated until the project
state gates pass.
