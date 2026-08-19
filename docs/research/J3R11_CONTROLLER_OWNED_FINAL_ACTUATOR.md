# J3R11 — Controller-Owned Final Export Actuator

Status: **bounded milestone complete with redesign classification**

Decision: `N3_FINAL_ACTION_AUTHORITY_REQUIRES_REDESIGN`

## Scope

J3R11 closes the J3R10 provenance incident and establishes the controller
boundary for a future producer action. It creates no Logic fixture, no ADM,
DD+, EC3, PCM, reference-f64, or other producer media. `SemanticBindingState`
remains `Unresolved`; warp raw 3 remains ETSI `ReservedWarpMode { raw: 3 }`.

## J3R10 incident closure

J3R10 remains classified as `J3R10_N3_BATCH_PARTIAL_RECOVERY_REQUIRED`.
Exactly one matching 400055-byte output appeared while the durable controller
still held `nonce_state=RESERVED` and `state=INVOCATION_ARMED`. It is therefore
not an N3 observation and is admitted only as a controller-provenance
incident. The output is quarantined and retained with a frozen SHA-256:

`758329837da26f2c1654e9129c478efa175173047b62b4724b94a323a474c141`

The run is terminalized as `PROVENANCE_INTEGRITY_FAILED_POST_OUTPUT`; its
nonce is `NONCE_RETIRED_COMPROMISED_UNCONSUMED`. The run, authorization, and
all five remaining J3R10 authorizations are non-reusable. The old queue is
retired.

Root cause is classified as `COMPUTER_USE_DIRECT_FINAL_ACTION_BYPASS`: a
real UI final action occurred outside the authoritative controller before
durable nonce consumption.

## Controller boundary

The private controller script implements a fail-closed
controller-owned state machine. Computer Use can only reach `READY_TO_ARM`.
The controller then:

1. persists `INVOCATION_ARMED` with file fsync, atomic rename, and parent
   directory fsync;
2. validates process tuple and semantic control identity;
3. durably transitions `RESERVED → CONSUMED`;
4. persists `EXPORT_INVOKE_PENDING`;
5. revalidates PID/start time, window/panel token, role, subrole, title, and
   enabled state;
6. exposes one ephemeral `CONTROLLER_FINAL_ACTUATOR_ENABLED` capability;
7. invokes only the semantic AXPress-equivalent callback;
8. records `FINAL_ACTUATOR_CALLED`, then output observation and completion.

Coordinates, Return/Enter, generic mouse clicks, default-button activation,
and Computer Use final Save/export actions are forbidden. A stale control,
wrong window, wrong process, panel recreation, duplicate invocation, or
output with an unconsumed nonce fails closed.

The controller self-test passes all bounded cases, including crash recovery,
stale control, wrong process, reserved-nonce output rejection, and duplicate
output rejection. The independent standard non-media Save-panel AXPress
oracle was attempted but **not admitted** because the expected private output
path was not observed. No producer media was generated. Accordingly the real
Logic cancel-only rehearsals were not rerun after this controller boundary;
prior no-media S_FL/S_FR/D_SWAP evidence remains diagnostic only.

## Replacement queue

Revision 2 is frozen but not executed. It contains six new runs and three
pairs:

- `N3R2_S_FL_0`, `N3R2_S_FL_1`
- `N3R2_S_FR_0`, `N3R2_S_FR_1`
- `N3R2_D_SWAP_0`, `N3R2_D_SWAP_1`

No J3R10 ID, authorization, or nonce is reused. The queue preserves the
bounded discovery-realm model and is not an authorization to export.

## Evidence boundary

The strongest result is controller/provenance handling, not producer
endpoint admission. J3R11 establishes no producer N3 endpoint, producer
envelope, C1/C2/C3 value, slot identity, object-row identity, authored-object
PCM, or renderer semantics.

Private evidence package:

the private controller-owned-actuator evidence run (path omitted from public
documentation)

The next blocker is a controller-owned standard non-media semantic AXPress
oracle, followed only by fresh cancel-only S_FL/S_FR/D_SWAP rehearsals. No
producer export should be attempted until both are admitted.
