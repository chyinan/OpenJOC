# J2R8 — Existing N3 Producer-Repeatability Complex Recovery

Status: completed on 2026-08-11. This is a bounded research result, not a
semantic-binding or renderer result.

## Decision

`EXISTING_N3_COMPLEX_ENVELOPE_ESTABLISHED_SCOPED_CONTEXT_DEPENDENCE_ADMITTED`

The six N3 contrasts frozen by J2R7 were decoded twice through the existing
phase-preserving path. Raw R0/R1 carriers, base PCM, and all 15
ReconstructionBasis rows were byte-identical for every pair. The resulting
full-complex producer repeatability envelope is therefore at the floating
regression floor (maximum projective residual `4.97e-16`; the underlying
f64le artifacts are byte-identical).

This makes the already-frozen C1 and C2 target differences exceed the fixed
combined N0–N3 envelope in the tested windows. The conclusion is scoped to
these carriers and tested contexts; it does not identify the causal context
variable.

## Frozen candidates

The exact six-candidate list was inherited from J2R7 without additions,
substitutions, or threshold changes. Canonical list hash:

`dd24dd726e6535800a0d9ac9f6fb77f9b198dc9c868c811575024102fe28f09c`

| Candidate | Producer relationship | Same-condition lineage | Relevance |
|---|---|---|---|
| `N3_FRONT_LEFT_R0_R1` | independent producer exports | projective-only | static-compatible |
| `N3_X_NEG_HALF_R0_R1` | independent producer exports | projective-only | static-compatible |
| `N3_X_POS_HALF_R0_R1` | independent producer exports | projective-only | static-compatible |
| `N3_Y_MID_R0_R1` | independent producer exports | projective-only | static-compatible |
| `N3_J1R8_R0_R1` | independent producer exports | projective-only | workflow-compatible, scene-different |
| `N3_J1R9_R0_R1` | independent producer exports | projective-only | dual-compatible |

Counts are static `4`, dual `1`, workflow-compatible/scene-different `1`.
No candidate was admitted from a container alias or stream-copy-only
relationship.

## Recovery and numerical evidence

- Every candidate produced 15 structural ReconstructionBasis rows.
- Static and Z controls recovered 129 AUs / 198,144 samples per row; the
  dual control recovered 126 AUs / 193,536 samples per row.
- No row was missing, padded, truncated, or silently permuted.
- All recovered samples were finite; zero rows were retained as valid
  structural rows.
- Base full-band, ReconstructionBasis, and complete-descriptor R0/R1 bytes
  were identical for all six candidates.
- The base-carried LFE/RcLfe path remained separate and was not folded into
  ReconstructionBasis semantics.

The producer envelope is an empirical maximum over the six admitted pairs,
not a population confidence bound. The base-LFE partition remains
`NOT_AVAILABLE` when its fitted reference is zero; it was not silently
promoted into a semantic result.

## C2 sanity audit

The extreme frozen C2 ReconstructionBasis residual (`0.9999992742845898`)
passed the bounded sanity gate:

- static Front Right and dual 997 Hz vectors are both observable above the
  fixed `1e-15` floor (norms `0.05575847064445246` and
  `0.2443995333918825`);
- 15 rows are present in both vectors and the sample shift is zero;
- simultaneous 997/2003 Hz regression is well-conditioned (maximum
  condition estimate `2.0033531737621555`);
- support overlap is explicit: rows `0..3` overlap and dual-only rows are
  `4..8`;
- the largest residual localizes to structural row 3, not to a discarded
  near-zero reference, alignment error, or row-permutation search.

The result is therefore recorded as an observable component difference in the
tested static-versus-dual context. It is not an authored-object or slot
identity claim.

## Frozen target reapplication

J2R7 target definitions, windows, component ordering, complex gauge rule and
calibration formula were unchanged.

- **C1 static FL vs dual FL:** base, ReconstructionBasis and complete
  descriptor remain above the combined fixed envelope.
- **C2 static FR vs dual FR:** base, ReconstructionBasis and complete
  descriptor remain above the combined fixed envelope; the sanity gate
  passes.
- **C3 reciprocal dual-frequency contrasts:** the observed differences remain
  above the fixed envelope, but a producer-level frequency classification is
  unavailable because only one dual-compatible N3 contrast is frozen. No
  universal frequency claim is made.

## Semantic boundary

`SemanticBindingState::Unresolved` is unchanged. ReconstructionBasis rows are
still structural reconstruction components, not authored objects, OAMD slots,
or object stems. Authored-object PCM and audio-bound ObjectScene remain
inadmissible. RcLfe remains separate. `warp_mode [526,528) = raw 3` remains
`ReservedWarpMode { raw: 3 }`; no vendor rule or raw-3 interpretation was
added.

No Logic project was opened, no producer export was run, and no new media or
fixture was created.

Private deterministic evidence is frozen under the corresponding J2R8 run;
its aggregate evidence-freeze hash is
`d25e8063993181d5f2d91684a8df163698d63735d253cd741dbd2c90a34e84e1`.
