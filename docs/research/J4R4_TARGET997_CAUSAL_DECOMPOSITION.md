# J4R4 — Fixed-topology target-997 causal decomposition

## Decision

`J4R4_COMPANION_PCM_EFFECT_NOT_OBSERVED_WITH_FIXED_TOPOLOGY`

J4R4 reuses the source-verified J4R2 carrier and the J4R3 calibration to ask
whether turning the companion 2003 Hz source on changes the continuously active
target 997 Hz representation. Companion propagation, fixed metadata, frozen
alignment, target observability, and same-decoder-HEAD gates all pass. None of
the four preregistered ON-versus-OFF comparisons exceeds the compatible-N1
envelope in Base, ReconstructionBasis, or their labeled joint vector.

The primary result is therefore a bounded no-observed-effect result, not proof
of physical independence or a universal encoder rule. The broad J4R3
compatible-N1 envelope remains the sensitivity limit.

A secondary existing-corpus comparison finds the expanded ReconstructionBasis
support already present when the dual-object topology is present but the
companion source is silent. That contrast is classified `STRUCTURAL_CONTEXT_ONLY`
under the declared projective test. It is descriptive rather than a strict
shell-only causal result because the older static and J4R2 target waveforms are
not sample-identical.

## 1–5. Frozen admission, identities, alignment, and states

J4R3 admitted `COMPANION_2003_CARRIER_PROPAGATION_CONFIRMED` and
`OFF_STATE_REVERSIBILITY_SUPPORTED_UNDER_COMPATIBLE_N1` on the frozen J4R2
carrier. J4R4 did not refit those thresholds after reading target results:

| Partition | Frozen compatible-N1 envelope |
| --- | ---: |
| Base full-band | `0.009248177275882511` |
| ReconstructionBasis | `0.008424653820934484` |
| Joint labeled Base+RB | `0.008857503764548379` |

For provenance, the lower J3R14 N0/N3 floors are retained as Base
`3.7834930931104663e-6`, RB `2.3853247222861374e-7`, and Joint
`3.7834930931104663e-6`; the J3R13 exact-N3 floors are zero in all three
partitions. These lower floors are recorded but do not gate J4R4.

Both carriers were decoded at commit
`528bd4d3be0332a983681193cebca55264f455c0`, with the same reference-f64,
component, profile, trim, and Base policies. The frozen J4R2 raw EC-3 identity
is `389b0e17a1a5766da89b8ef39f154d1f7217a56d949f8828f4b1c8b0f88d186d`.
The secondary static Front-Right S0 carrier is a byte-identical two-export pair
with raw identity
`1d349872f81d2d477ff18360a4782700fb88a823473c51bc7f257f426f20ddb7`.

The alignment rule is fixed as:

`decoded_sample = source_sample + 1536`

Each analysis window contains 24,000 samples:

| State/window | Source samples | Decoded samples | Meaning |
| --- | ---: | ---: | --- |
| S1 / OFF_A | `[60000,84000)` | `[61536,85536)` | dual topology, companion silent |
| S1 / OFF_B | `[204000,228000)` | `[205536,229536)` | dual topology, companion silent |
| S2 / ON_PRIMARY | `[156000,180000)` | `[157536,181536)` | dual topology, companion active |
| S2 / ON_REPLICATION | `[108000,132000)` | `[109536,133536)` | independent active holdout |
| S0 / D_POST | `[132000,156000)` | `[133536,157536)` | older static Front-Right control |

## 6–8. Propagation, metadata, and estimator gates

The independently admitted companion calibration is reproduced without
changing its threshold. The 2003 Hz joint norms are `1.5179408833906213e-4`
for OFF_A, `1.5102053650661352e-4` for OFF_B,
`0.2788167423372097` for ON_PRIMARY, and `0.2788147626169152` for
ON_REPLICATION. Both OFF values remain below the frozen
`1.6745250526579124e-4` floor, while both ON values exceed it by roughly
1,665 times. `COMPANION_2003_CARRIER_PROPAGATION_CONFIRMED` therefore remains
the prerequisite propagation result.

The J4R2 carrier has one unique payload-11 body across 251 access units, with
constant observed parser-level object-count 16, element-count 2, and metadata-block-count
1. The parser count is not equated with authored project object population. Project controls retain fixed target `(X=+1,Y=+1,Z=0,Size=0)` and companion
`(X=-1,Y=+1,Z=0,Size=0)` positions. This is a structural constancy audit; it
does not infer slot identity or independently reparse position semantics in
every access unit.

The simultaneous 997/2003 Hz sine/cosine-plus-constant estimator has rank 5,
condition number about `1.414216`, and maximum normalized cross-frequency
basis inner product `1.4034848451579923e-15`. All coefficients are finite.
The target-997 joint norm is stable from `0.2784904953203073` to
`0.2784916316679788` across the four J4R2 windows. Estimator integrity and
target observability both pass. Every window contains 24,000 samples in the
fixed component order Base FL/FR/FC/SL/SR followed by RB `row_000..row_014`;
RcLfe is separate. All per-window regression residuals are bounded by their
nonzero component segment RMS, with maximum ratios `0.99818..0.99820`.

## 9–13. Four fixed-topology comparisons and primary classification

Every comparison uses an optimally fitted complex scalar before computing the
projective residual. The four comparisons are retained separately; they are
not averaged into a favorable result.

| Comparison | Base residual | RB residual | Joint residual |
| --- | ---: | ---: | ---: |
| OFF_A → ON_PRIMARY | `3.5066973400846875e-6` | `8.097186228788494e-4` | `5.644989466507735e-4` |
| OFF_B → ON_PRIMARY | `5.867853596763149e-6` | `8.039202384708338e-4` | `5.604660464713796e-4` |
| OFF_A → ON_REPLICATION | `3.0481417072663545e-6` | `1.5797967034459133e-5` | `1.1231446406359067e-5` |
| OFF_B → ON_REPLICATION | `3.900922970921702e-7` | `2.057986969047448e-5` | `1.435045895074733e-5` |

For each of these four rows, the machine-readable record preserves, separately
for Base, RB, and Joint, the projective residual, complex coherence, fitted
relative magnitude, and maximum relative-phase residual. It also records the
before/after RB masks, explicit gained/lost sets, and the RcLfe zero-reference
result with absolute complex delta `0`. No diagnostic is inferred from a
non-finite zero-vector projective metric.

All twelve values are below their frozen compatible-N1 envelopes. Base is
classified `NO_REPRODUCIBLE_BASE_CHANGE_ABOVE_COMPATIBLE_N1`; RB is classified
`NO_RB_SIGNAL_EFFECT_OBSERVED`; Joint also shows no reproduced change above
the compatible envelope.

The 997 Hz RB support mask is identical in OFF_A, OFF_B, ON_PRIMARY, and
ON_REPLICATION: `row_000..row_008`. Rows `row_009..row_014` are outside the
frozen support floor. No coordinate is absent in both OFF windows and present
in both ON windows; the intervention creates no admitted support. The public
JSON retains all 15 row magnitudes for every target window in fixed
`row_000..row_014` order, together with the shared, gained, and lost masks.

These results satisfy the preregistered primary class:

`J4R4_COMPANION_PCM_EFFECT_NOT_OBSERVED_WITH_FIXED_TOPOLOGY`

## 14–16. Static S0 contrast and three-state decomposition

S0 is the deterministic 129-AU static Front-Right carrier. Its frozen analysis
window is the J3R14 D_POST interval, recomputed under the same decoder HEAD and
analysis policy. S0 has 997 Hz RB support only at `row_000`; both silent S1
windows have support at `row_000..row_008`.

| Comparison | Base residual | RB residual | Joint residual |
| --- | ---: | ---: | ---: |
| S0 → S1/OFF_A | `0.004204709878243303` | `0.9999989915784783` | `0.8581700170729656` |
| S0 → S1/OFF_B | `0.004205167942600052` | `0.9999989969033863` | `0.8581687676491548` |

The Base residual stays within compatible N1, while both RB and Joint
contrasts exceed it. Rows `row_001..row_008` are gained in both silent S1
windows relative to S0.

The corresponding three-state description is:

- S0: static single-object context;
- S1: dual-object topology with companion PCM silent;
- S2: the same dual topology with companion PCM active.

Within the tested data, S0→S1 contains the robust RB support expansion while
S1→S2 contains no change above compatible N1. The secondary classification is
therefore `STRUCTURAL_CONTEXT_ONLY`.

This is not a strict isolated topology intervention. S0 uses a 193,536-sample,
variable-envelope 997 Hz source, whereas J4R2 uses a 288,000-sample,
constant-amplitude 997 Hz source. The waveforms are not sample-identical, so
source amplitude, envelope, duration, and any nonlinear interaction remain
confounds. Projective gauge removes a global complex scale; it does not remove
every source-lineage confound. The S0→S1 result is admitted only as a
descriptive existing-corpus projective association.

## 17–18. Base, RcLfe, and residual directions

Base changes remain below compatible N1 in all four causal comparisons and in
both S0→S1 comparisons. Base and RB are not assumed to form a physical
additive rendered field. RcLfe is zero in all windows, is excluded from the
Joint vector, and remains a separate base-carried path.

Gauge-aligned RB residual directions were retained as coordinate diagnostics.
The structural residual has raw above-floor support at `row_001..row_008` in
both S0→S1 contrasts and is dominated by `row_003`. Signal-activity residuals
have much smaller norms (`3.0672e-6` to `1.5721e-4`) and remain below compatible
N1. Their absolute complex coherence with either structural residual is only
`1.9462e-5` to `1.3976e-3`. Because the signal residuals fail the change gate,
this low directional coherence is descriptive only; it does not establish a
second transform or latent operator. The machine-readable record retains the
normalized vectors and per-contrast above-floor coordinate sets.

## 19–22. Consequence, ambiguity, claim boundary, and final classes

The narrow model consequence is:

`OBJECT_POPULATION_OR_STRUCTURAL_CONTEXT_ESTABLISHES_RB_SUPPORT`

Its status is a descriptive existing-corpus association, not strict causal
establishment. It narrows the earlier J3R14 observation: the expanded RB
support is already present in the tested dual topology while the companion is
silent, and activating its propagated PCM produces no additional target-997
change above the broad compatible-N1 envelope.

Remaining ambiguity includes the incompatible S0/J4R2 target-source lineage,
the broad approximately 0.8% post-intervention nuisance term that defines
compatible N1, whether smaller signal-dependent changes exist below that
sensitivity, and which structural feature of the dual context produces the
coordinate expansion. No universal vendor transform or additive synthesis
operator is inferred.

`SemanticBindingState::Unresolved` is unchanged. ReconstructionBasis rows are
coordinate labels, not authored objects. This work does not admit
authored-object-to-row identity, authored-object PCM inside JOC, a complete
reconstruction operator, renderer semantics, physical rendered-field
behavior, or an audio-bound `ObjectScene`. RcLfe remains separate. Warp
`[526,528)` remains raw 3 and ETSI strict continues to reject it as reserved;
no vendor semantic rule was added.

Final classifications:

- primary: `J4R4_COMPANION_PCM_EFFECT_NOT_OBSERVED_WITH_FIXED_TOPOLOGY`;
- RB: `NO_RB_SIGNAL_EFFECT_OBSERVED`;
- secondary: `STRUCTURAL_CONTEXT_ONLY`;
- model consequence: `OBJECT_POPULATION_OR_STRUCTURAL_CONTEXT_ESTABLISHES_RB_SUPPORT` (descriptive existing-corpus association only).

No Logic project was opened, no media or producer output was generated, and
no full PCM capture was retained. Two independently generated compact analysis
passes were byte-identical. Temporary captures totaled 144,914,072 logical
bytes across the original and audit recaptures; retained large-derived bytes
are zero.
