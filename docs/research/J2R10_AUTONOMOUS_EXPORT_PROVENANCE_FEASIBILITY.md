# J2R10 — Autonomous Export-Only Logic Workflow & Durable N3 Provenance

J2R10 tested the producer-control portion of the future N3 repeatability
experiment without generating any audio or encoded media. It used disposable
copies of the four existing private project conditions and cancelled before
the final export action.

## Result

`EXPORT_ONLY_WORKFLOW_ADMITTED_PROVENANCE_REQUIRES_REDESIGN`

Logic Pro 12.3 was launched, each disposable project was opened, and the
space-audio export dialog was reached for all four target condition IDs. The
dialog was switched to **Dolby Digital Plus with Dolby Atmos**, with **Music /
768 kbps** selected. Each rehearsal entered a unique destination that was
absent before the dialog and cancelled before rendering. A static repeat and a
dual repeat followed the same path. No ADM BWF, DD+, EC-3, MP4, WAV, or other
audio/encoded output was created.

The workflow is therefore demonstrated at the UI/dialog boundary, but it is
not yet admitted as a complete independent-producer provenance controller:

* Logic can close its project window while the process remains resident. In
  this environment the Dock quit action was not reliably addressable through
  the Computer Use accessibility surface, so exact-PID termination was needed
  as a fail-closed recovery step.
* The two dual target IDs are projective-only views of the existing J1R9 swap
  project, not full-run static baselines. They are not promoted to stronger
  condition identity.

The future minimum remains preregistered as four conditions × two independent
producer runs (eight outputs), but those exports were not authorized or
performed in J2R10. The durable run-record schema, unique destination rules,
run-ID namespaces, append-only hash chaining, and completion state machine were
validated with non-media simulations only.

## Scope boundary

`SemanticBindingState` remains `Unresolved`. No JOC, ObjectScene, authored-object
PCM, renderer, warp-3 interpretation, vendor rule, or production decoder
behavior changed. Private evidence and disposable projects are outside the
repository.

The next blocker is a stronger, independently observable Logic process-lifecycle
and baseline-provenance controller before any real producer export can be
authorized.
