# J2R6 — ReconstructionBasis Phase-Evidence Recovery

## Scope and decision

J2R6 re-audited the existing decoder path and recovered phase-bearing
`ReconstructionBasis` rows from the current `DecodedJocFrame` output. The
result is:

```text
FULL_COMPLEX_JOINT_TRANSFER_MODEL_NARROWED_BUT_UNDERDETERMINED
```

This is a numerical and representation result. It is not authored-object
identification, renderer validation, or a semantic interpretation of the
reserved warp value.

## What was recovered

The current decoder synthesizes `DecodedJocFrame.reconstruction_qmf` with the
reference f64 QMF path and emits signed f64 PCM in
`DecodedJocFrame.reconstruction_basis.rows`. The phase is therefore present in
the current row path; it was absent only from earlier J1R10 magnitude-summary
artifacts. A private, read-only harness captured the rows without changing the
production decoder.

Nine existing carrier groups were recoverable, each with 15 structural rows:

* Center, Front Left, Front Right, Rear Center, X-negative half,
  X-positive half, and Y-mid 997 Hz controls;
* the existing J1R8 Z calibration carrier; and
* the existing J1R9 dual-object 997/2003 Hz carrier.

No Logic project, ADM, DD+, EC-3, or other media was created in this stage.

## Structural and phase evidence

The recovered static controls contain 129 access units (198,144 samples per
row) and preserve one streaming decoder state across the carrier. The current
J1R8 recovery contains 129 access units while its older J1R15 inventory listed
128. The current J1R9 recovery contains 126 access units while its frozen J1R5
analysis window covered 125. No samples were silently trimmed or padded; the
dual comparisons use the explicitly inherited first-125-AU window.

All recovered samples are finite and row lengths are uniform. A duplicated
Center capture at the same revision, input, and policy is byte-identical for
all 15 rows. Inactive zero rows are retained as structural basis rows and are
not treated as failures. RcLfe remains on its separate base-carried path.

Independent sine/cosine fits preserve signed phase. In the fixed windows used
by the preceding J2R5 protocol, complex cross-frequency coherence is
approximately 0.999995 for both dual-object transfer comparisons, with
projective residuals approximately 0.00315 (FL) and 0.00323 (FR). The static
FL versus dual-FL 997 Hz control has coherence approximately 0.999995 and
residual approximately 0.00317. Static FR versus dual-FR 997 Hz is not
projectively equivalent (coherence approximately 0.00120; residual
approximately 1.0). This contrast narrows the set of compatible joint-transfer
models, but the available controls do not isolate frequency, context, and
position causes sufficiently to select one model.

Temporal projective fits within the static controls are highly stable (roughly
10^-7 residual in the declared windows). These are descriptive numerical
regressions, not a semantic row-to-object mapping and not a claim that a row
is an authored object.

## Boundaries that remain closed

`SemanticBindingState` remains `Unresolved`. ReconstructionBasis rows remain
metadata/reconstruction-basis outputs; authored-object PCM, audio-bound
`ObjectScene`, and renderer semantics remain inadmissible. The row order is
the decoder's structural order, not an object identity contract.

The OAMD `warp_mode` value remains the observed raw value 3 and is still
`ReservedWarpMode { raw: 3 }` under `ETSI_STRICT`. J2R6 did not inspect or
interpret vendor trim continuation, post-warp suffix bits, or add a vendor
rule.

## Reproducibility and next blocker

The private recovery harness and phase-aware analysis were run twice with
byte-identical core reports. The evidence package records the exact carrier
lineage, AU alignment limitation, row hashes, phase fits, and semantic
boundaries. Public documentation intentionally contains no private paths or
media hashes.

The next non-fixture blocker is either admissible semantic-binding evidence or
a reviewer-authorized, predeclared complex-transfer acceptance threshold. Row
energy, phase coherence, or a context contrast alone must not be promoted to
authored-object identity. No new fixture is justified or created by J2R6.
