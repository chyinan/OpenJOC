# J2R9 — N3 Export-Execution Provenance Closure

J2R9 audits the six frozen J2R8 N3 R0/R1 pairs without creating media or
changing decoder behavior. It separates two questions that were previously
collapsed:

1. Are the elementary payloads byte-identical?
2. Is there durable evidence that each endpoint was produced by a distinct
   producer/export execution?

## Result

`N3_EXPORT_INDEPENDENCE_NOT_PROVEN_J2R8_DOWNGRADED_TO_NUMERICAL_FLOOR`

All six pairs have byte-identical elementary E-AC-3 payloads. Their MP4
containers differ. However, the frozen evidence contains no producer/export
run identifiers, invocation records, independent completion records, or
destination/overwrite provenance sufficient to prove two independent producer
executions for any pair. Distinct filenames, paths, inodes, and timestamps are
supporting artifact-separation evidence only.

Accordingly, the corrected producer envelope contains zero provenance-admitted
N3 pairs. The public classification is therefore:

`FULL_COMPLEX_DIFFERENCE_ABOVE_NUMERICAL_FLOOR_PRODUCER_NULL_UNAVAILABLE`

The J2R8 numerical observations remain valid as deterministic analysis of the
frozen endpoint artifacts. The producer-level context-dependence admission is
not retained because its producer null was not proven.

## Frozen scope

The six candidate IDs and J2R7 calibration manifest are unchanged. The frozen
J2R8 target IDs, windows, and metrics are preserved without recomputation.
Static/dual coverage is recorded explicitly: four static-compatible candidates
and one dual-compatible candidate were in the prior set, but none is admitted
after the execution-provenance gate. The requirement for at least two dual N3
contrasts remains closed, not relaxed.

No Logic fixture, ADM, DD+, EC-3, or other media was created. No production
parser, renderer, semantic binding, or warp behavior changed. `SemanticBindingState`
remains `Unresolved`, and `warp_mode = raw 3` remains ETSI-reserved.

Private deterministic evidence is held outside the repository. It records the
two-axis classification, payload hashes, provenance gaps, corrected envelope,
and frozen target reapplication.
