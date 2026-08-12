# J3R4 — Logic project-open controller

## Status

`N3_PROJECT_OPEN_CONTROL_NOT_ESTABLISHED`

J3R4 replaced the unreliable Logic **File → Open** accessibility path with the
standard macOS exact-document mechanism: `/usr/bin/open -a "Logic Pro"
<exact-disposable.logicx>`.  A fresh APFS clone of the S_FL baseline was
created under the private run, hashed, and opened by a fresh, exact Logic PID.
No canonical project, export UI, or media output was touched.

Before project identity could be verified, Logic showed an autosave/recovery
choice for that disposable project.  The durable controller classifies that as
`RECOVERY_DIALOG_PRESENT`; it rejects the run rather than choosing a version,
then terminates the exact PID.  Therefore neither window-title nor process-path
identity was treated as sufficient, and no S_FL/S_FR/D_SWAP/repeat rehearsal is
admitted.

This result validates the safe OS document-open request and recovery-dialog
interlock, but not a clean project-open binding.  The J3R3 replacement queue
is unchanged. `FINAL_ACTION_ALLOWED` stayed false; Return/Enter folder
navigation was never used; no media was generated; and
`SemanticBindingState::Unresolved` remains unchanged.
