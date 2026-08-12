# J3R6 — NSSavePanel destination binding

## Decision

`N3_NSSAVEPANEL_NAVIGATION_REQUIRES_REDESIGN`

J3R5 established a clean Logic lifecycle and exact disposable-project opening.
J3R6 tested the remaining destination-binding mechanism independently with a
standard macOS save panel before returning to Logic.  It did not safely admit
that mechanism, so no Logic destination rehearsal or producer export ran.

## Permanent interaction boundary

Return and Enter are never folder-navigation, folder-confirmation, or save
actions in the Logic save panel.  J3R6 used the documented folder-location
sheet and Accessibility actions only.  After an exact approved path was typed,
the Accessibility tree provided no separately addressable semantic **Go** or
**Open** control.  Its exposed secondary Open action did not navigate.  A
single-click attempt on a displayed candidate then selected the other candidate
directory instead.  That is exactly the ambiguity the destination contract is
intended to reject.

The standard oracle consequently could not provide the required two independent
signals for the active parent directory.  It was cancelled without saving.

## Destination contract retained

The fail-closed contract distinguishes the panel leaf from the producer's final
leaf:

- typed panel leaf: `<authorized_stem>.mp4`;
- expected producer final leaf: `<authorized_stem>_ec3.mp4`.

It requires an approved real parent directory, independent UI/path readback,
exact leaf readback, no existing expected output or media, and readiness
revocation if the parent changes.  It rejects unsafe leaf syntax, symlink
components, readback disagreement, media-bearing parents, and existing final
outputs.  `FINAL_ACTION_ALLOWED` remains false even after a hypothetical
destination proof.

## Scope preserved

S_FL, S_FR, D_SWAP, and the repeat destination rehearsals were intentionally
not run after the independent oracle failed.  No save/export action, Logic
project change, media generation, queue execution, or semantic decoder change
occurred.  `SemanticBindingState::Unresolved` remains unchanged.

The next work must redesign the real save-panel adapter so it can explicitly
activate a folder-location confirmation and read back the actual active parent
through two independent signals.  It must not substitute Return/Enter or an
ambiguous candidate-row click.
