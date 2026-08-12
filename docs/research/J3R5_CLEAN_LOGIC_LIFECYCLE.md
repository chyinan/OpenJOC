# J3R5 — Clean Logic lifecycle and project-open admission

## Decision

`N3_REAL_UI_CONTROLLER_REQUIRES_REDESIGN`

J3R5 establishes a clean **application lifecycle** and exact disposable
**project-open** path, but does not admit an export controller for producer
work.  The remaining gap is narrow: the real UI controller has not yet
verified deterministic selection of the intended directory-scoped output
destination.  The export final action remains unavailable.

## Observed lifecycle

Logic Pro was cleanly quit through its normal application Quit command and
restarted to the project chooser without a project-recovery dialog.  The same
clean relaunch result held after four cancel-only rehearsals.  An unavailable
audio-interface warning was observed independently; it was dismissed and is
not a project-recovery result.

The project-recovery choice appeared for two S_FL disposable copies.  In each
case the last-saved choice was selected; no autosaved version was restored.
S_FR and D_SWAP did not show that choice.  The evidence therefore supports
`PROJECT_SPECIFIC_RECOVERY_STATE`, not a claim that the app-wide lifecycle is
unclean or that any particular termination mechanism is causal.

## Exact project binding and rehearsal scope

The standard macOS exact-document open mechanism opened four newly-created
disposable copies: S_FL, S_FR, D_SWAP, and a repeated S_FL.  Each displayed its
own disposable project title and document URL.  No canonical project was
opened.  The S_FL/S_FR projects retained their opposing front positions, and
D_SWAP retained the two-object topology expected for that control.

Each rehearsal reached Spatial Audio export and was configured as Dolby
Digital Plus with Dolby Atmos, Music at 768 kbps, Project range, with the
additional ADM master option disabled.  Each dialog was cancelled.  Save was
never enabled by the controller, no media was generated, and no producer
export occurred.

The controller's durable mock transition suite passes wrong-path rejection,
output reservation rejection, restart recovery, and single-consumption
interlocks.  It does not yet establish deterministic real-UI destination
selection, so `FINAL_ACTION_ALLOWED` remains false.

## Unchanged boundaries

`SemanticBindingState::Unresolved` remains unchanged.  This milestone adds no
warp interpretation, vendor semantic rule, authored-object PCM claim,
audio-bound ObjectScene, renderer work, or new media.
