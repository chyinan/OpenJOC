# J2R12 — One-Shot Real-Export Authorization Interlock

Status: `N3_REAL_EXPORT_CONTROLLER_ADMITTED`

This milestone admits the safety controller for a future reviewer-authorized producer run. It does not authorize, perform, or imply any real DD+ export.

## Immutable queue

The J2R11 matrix is frozen as exactly eight producer-run IDs and four non-overlapping pairs:

- `N3_S_FL_PAIR`: `S_FL_PRODUCER_0`, `S_FL_PRODUCER_1`
- `N3_S_FR_PAIR`: `S_FR_PRODUCER_0`, `S_FR_PRODUCER_1`
- `N3_D_SWAP_PAIR_A`: `D_SWAP_PRODUCER_0`, `D_SWAP_PRODUCER_1`
- `N3_D_SWAP_PAIR_B`: `D_SWAP_PRODUCER_2`, `D_SWAP_PRODUCER_3`

Every D_SWAP output owns both frozen authored-state windows, `D_PRE` and `D_POST`. Those windows are measurements inside one output, never producer-run IDs or independent pairs. The queue is bound to the admitted corrected matrix hash and rejects missing, extra, duplicate, reordered, or cross-pair runs.

## Fail-closed authorization

A real execution requires a durable one-shot authorization object bound to the session, authorized stage/task, reviewer response, matrix and queue hashes, baseline/condition/run/pair, destination, Logic process lifecycle, and a unique nonce. `producer_export=true` is mandatory; an ordinary `--execute` flag is insufficient. The validator checks durable state, queue membership, nonce and destination preconditions, process identity, project identity, export dialog/settings, and final confirmation in a fixed order, stopping before confirmation on any failure.

The nonce registry is one-use and durable. Replays, stale runs, destination aliases, path traversal, and recovery states are rejected. A failure after invocation becomes `RECOVERY_REQUIRED`; the run ID is never silently reused.

## UI and lifecycle boundary

The real Logic UI backend is capped at `READY_TO_CONFIRM` in J2R12. The transition to `CONFIRMATION_AUTHORIZED` is unreachable because this stage has no real-export authorization. The controller cancels, verifies no output, closes the disposable project, and terminates only the exact captured Logic PID. Three fresh-process rehearsals (S_FR closure, D_SWAP pair-B, and an additional S_FL run) reached the same DD+ Atmos / Music / 768 kbps / Project / ADM-off dialog and cancelled before render without media or canonical-project changes.

A mock backend exercised authorized success, nonce consumption, and failure/recovery states using in-memory/non-media sentinels. This proves state-machine behavior only; it is not producer evidence.

`SemanticBindingState` remains `Unresolved`. Warp raw 3 remains `ReservedWarpMode { raw: 3 }`. No JOC, ObjectScene, authored-object PCM, or renderer claim was changed.
