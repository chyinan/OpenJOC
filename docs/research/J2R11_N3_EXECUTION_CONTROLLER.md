# J2R11 — N3 Execution Controller and Corrected Dual-Window Matrix

Status: `N3_MATRIX_ADMITTED_PROCESS_LIFECYCLE_REQUIRES_REDESIGN`

J2R11 separates two questions that were previously conflated:

1. the topology of the future N3 producer matrix; and
2. the lifecycle boundary required to treat two producer runs as independent.

## Corrected topology

The reciprocal dual states used by the J1R9 evidence are two frozen authored-state windows in one dual-object swap project. They are not two independent dual baseline projects. The minimum future topology is therefore three baseline projects:

| Baseline | Runs | Frozen state measured |
| --- | --- | --- |
| `S_FL` | `S_FL_0`, `S_FL_1` | static Front Left control |
| `S_FR` | `S_FR_0`, `S_FR_1` | static Front Right control |
| `D_SWAP` | `D_SWAP_0` … `D_SWAP_3` | both `D_PRE` and `D_POST` windows in each output |

The planned matrix has four non-overlapping pairs: one pair for each static baseline and two pairs for the single `D_SWAP` project. `D_PRE` and `D_POST` are measurements inside a D_SWAP output; they are not producer IDs and must not be compared as if they were independent exports.

## Dry-run controller boundary

J2R11 rehearsed the producer-visible workflow only. Each rehearsal used a fresh disposable copy and a separately launched Logic Pro process, recorded the PID/start-time tuple, opened the project, reached **File → Export → Project as Spatial Audio File**, selected Dolby Digital Plus with Dolby Atmos / Music / 768 kbps / Project / ADM off, entered a unique absent destination, and cancelled before Save/render confirmation.

The controller rejects name-only process termination and rejects destination collisions. A future real-export authorization object must carry the session, stage, task, `producer_export=true`, corrected matrix hash, pair/run identity, destination, nonce, and authorization hash. J2R11 had no such authorization and therefore produced no media.

## Lifecycle limitation

The five completed cancel-only rehearsals were bounded by unique exact Logic PID/start-time tuples. Logic's ordinary graceful/Dock quit was not reliable; exact-PID `SIGTERM` was required after the project was closed. Consequently the corrected matrix is admitted for future planning, but the reusable real-export lifecycle still requires redesign and an explicit producer-export authorization before any render.

This document does not change OAMD warp handling, JOC reconstruction, `SemanticBindingState`, or authored-object semantics. No Logic, ADM, DD+, EC3, WAV, or other media is part of this milestone.
