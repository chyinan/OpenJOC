# J3R3 — Durable producer-execution provenance controller

## Status

`N3_DURABLE_PROVENANCE_CONTROLLER_REQUIRES_REDESIGN`

J3R2's six retained carriers are `EXISTING_NON_N3_STRUCTURAL_CARRIERS`.
They remain structurally valid but each has `n3_null_admission = false`, because
its authorization stayed `RESERVED_UNCONSUMED` and no durable final-invocation,
completion, or complete PID/start-time tuple was recorded.  Their observed
same-condition relationship remains only: container different; elementary
payload bytes identical.

## Root cause and repair

The J3R2 UI action bypassed the state-recording path: immutable authorization
objects were written, but nonce consumption, final action, output observation,
completion, and process exit were not atomically checkpointed.  A post-hoc
audit cannot invent them.

The private controller now provides the single future final-action authority.
It persists `PLANNED → AUTHORIZATION_VALIDATED → NONCE_RESERVED →
PROCESS_BOUND → PROJECT_VERIFIED → DESTINATION_VERIFIED →
EXPORT_SETTINGS_VERIFIED → READY_TO_ARM → INVOCATION_ARMED →
NONCE_CONSUMED → EXPORT_INVOKED → OUTPUT_OBSERVED → OUTPUT_STABLE →
EXPORT_COMPLETED → PROCESS_EXITED → RUN_VERIFIED`.  Writes use file fsync,
atomic rename, then parent-directory fsync.  Failure states are fail-closed;
in particular an output with a reserved nonce is
`PROVENANCE_INTEGRITY_FAILED`, quarantined, and never an N3 endpoint.

`FINAL_ACTION_ALLOWED` is unavailable until an armed record already binds the
authorization, queue/run/pair, PID/start-time/instance, project, destination,
settings, and reserved nonce.  Nonce consumption and invocation state are
durably written before a future UI action can be exposed.  Restart after arming
or consumption enters recovery-required, never automatic retry.

Mock tests cover authorized success, invalid authorization, no-arm final
action, output-before-invocation, output-with-reserved-nonce, restart after
arming, and PID reuse.  The requested three cancel-only Logic rehearsals did
not begin: the macOS path form exposed no executable semantic Go/Open action;
the SOP prohibits the forbidden Return/Enter navigation fallback.  No project
was opened and no media was generated.

## Replacement queue

The next six IDs are pre-registered only: `N3R_S_FL_0`, `N3R_S_FL_1`,
`N3R_S_FR_0`, `N3R_S_FR_1`, `N3R_D_SWAP_0`, and `N3R_D_SWAP_1`; pairs are
`N3R_S_FL_PAIR`, `N3R_S_FR_PAIR`, and `N3R_D_SWAP_PAIR`.  It supports a future
C1/C2 design, not C3.  It creates no Stage-4 authorization and no media.

`SemanticBindingState::Unresolved`, decoder behavior, and warp handling remain
unchanged.
