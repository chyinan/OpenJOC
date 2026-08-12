# J3R7 — NSSavePanel directory challenge response

## Decision

`N3_VISUAL_GO_CONTROL_REQUIRES_REDESIGN`

J3R7 tested the proposed stronger directory witness method only to the point
where it first needs a safe, explicit **Go** activation.  The folder-location
sheet accepted an exact path, but did not expose a semantic Go control through
Accessibility.  The available screenshot was also insufficient to identify a
rendered Go control or derive current geometry.  A remembered or guessed
coordinate would violate the interaction contract, so no such click occurred.

## Why the witness challenge did not run

Two high-entropy empty directory witnesses were prepared under an isolated
private parent.  They cannot prove the active parent until the panel has
entered that parent through a verified action.  Since the Go control could not
be identified by either supported channel, the static A/B observation, dynamic
C challenge, wrong-parent check, and Logic rehearsals were all intentionally
skipped.  The sheet and save panel were cancelled without Return, Enter, Save,
or export.

## Preserved boundary

This establishes neither a producer destination nor a producer result.
`FINAL_ACTION_ALLOWED` remains false; no media was generated; and
`SemanticBindingState::Unresolved` is unchanged.  Further work must replace or
repair the save-panel control surface before attempting another challenge on
the same UI.
