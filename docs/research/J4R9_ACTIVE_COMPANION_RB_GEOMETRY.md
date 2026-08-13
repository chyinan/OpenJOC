# J4R9 — Active-companion ReconstructionBasis redistribution geometry

## Decision

`J4R9_ACTIVE_COMPANION_RB_REDISTRIBUTION_REQUIRES_WINDOW_DEPENDENT_TRANSFORM`

J4R8 admitted a source/project-matched causal effect: activating the otherwise
unchanged companion PCM changed the target-997 ReconstructionBasis (RB)
coordinates while preserving support. J4R9 now shows that the three frozen
windows cannot be predicted by one common residual direction or one common
row-wise transfer within the compatible RB null. The bounded result is a
window-dependent, support-preserving coordinate transform—not an object/row
identity or recovered vendor operator.

## Frozen inputs and same-HEAD policy

The J4R5 B0/B1 exact-zero-companion raw EC-3 repeat has SHA-256
`94ca909823a9480881add1a9ef20b1c522f7078f8e47032b110ebeadffcfb19a`;
the J4R8 C0/C1 active-companion repeat has SHA-256
`05c2ff7df2e9791101615893a3e35119cb791bae0d19003a248a97023f52eed3`.
Both pairs are byte-identical within condition. All four were decoded at
`c7712712fc3bd7572bb0b629f10ea2053b8c47e3` with one continuous 150-AU
prefix, `DOLBY_VENDOR_COMPAT`, trim count 1, `CurrentDefault`, reference-f64,
five Base full-band components, 15 RB coordinates, and separate RcLfe.

The frozen source windows are W1 `[60000,84000)`, W2 `[156000,180000)`, and
W3 `[204000,228000)` with the independently established +1,536-sample carrier
delay. Deterministic source fitting finds both tones at magnitude about 0.2 and
relative phase within `5.5e-12` rad in all windows; cross-tone nuisance is at
or below `3.42e-10`.

## Gauge-aligned residuals and row localization

For each window, `alpha = <B,C>/<B,B>` and `r = C - alpha B`. The target-997 RB
projective residuals are:

| Window | RB residual | Scale relative to W1 | Dominant row contribution |
|---|---:|---:|---:|
| W1 | `1.6350083054675358e-05` | `1.0` | row_000 `99.4900%` |
| W2 | `0.00030134562889656103` | `18.430831689897154` | row_000 `99.9999%` |
| W3 | `1.881715164302418e-05` | `1.150890278667016` | row_000 `99.9998%` |

All B and C masks remain `row_000..row_008`; no row is gained or lost. The
machine-readable record retains every row's before/after/residual complex
coefficient and residual-energy contribution.

## Direction, rank, and cross-prediction

Residual-direction coherence is `0.9974478483` for W1/W2,
`0.9974444987` for W1/W3, and `0.9999999491` for W2/W3. High coherence is
descriptive, not sufficient for admission at the frozen uncertainty.

The residual matrix singular values are `5.8705150254689294e-05`, `2.2631560105141893e-07`, and `1.1464627842183099e-09`.
Their ratios to the first are `1`, `0.003855123444358123`, and `1.9529168722751576e-05`.
Using the compatible RB null `2.3853247222861374e-07` as the predeclared
prediction boundary, leave-one-window-out common rank-1 prediction gives:

| Held-out | Error relative to C | Pass |
|---|---:|---|
| W1 | `1.1673793779560205e-06` | no |
| W2 | `9.251983205064116e-06` | no |
| W3 | `7.640769000687914e-09` | yes |

Common row-wise transfer also fails every held-out window: errors are
`0.00014212991691846575` (W1),
`0.0002878884644252763` (W2), and
`0.00013686538094517095` (W3). Near-zero rows are
handled explicitly and never divided blindly.

## Base and RcLfe controls

Base projective residuals are `1.5242341150598972e-06`,
`3.5104069567585366e-07`, and
`1.973539286453972e-07`; all remain below the Base
null `3.7834930931104663e-06`. RcLfe remains exact zero and is not folded into
the RB or labeled Joint geometry. This establishes preferential localization
to RB coordinates within tested sensitivity, not a physical transfer of
energy from Base to RB.

## Coding-tool and raw-bitstream differential

The frozen public-syntax field set contains 17 fields (block switching,
exponent strategy/reuse, bandwidth, BAP, dither, coupling, SPX, rematrix, AHT,
dynamic range, and mantissa grouping). Corresponding-block results are:

| Window | Differing blocks / total | Field-score | RB residual |
|---|---:|---:|---:|
| W1 | `570/570` | `0.1953560371517028` | `1.6350083054675358e-05` |
| W2 | `540/570` | `0.1867905056759546` | `0.00030134562889656103` |
| W3 | `540/570` | `0.18565531475748195` | `1.881715164302418e-05` |

The three-point Pearson association is `-0.4113177339769762`;
the permitted classification is `CODING_STATE_COVARIATION_UNDERDETERMINED`.
This inventory is diagnostic association only and supplies no causal coding-
tool attribution.

Every overlapping AU differs in raw EC-3. Changed-byte fractions are
`0.7338053385416666`, `0.7217371323529411`,
and `0.7465916053921569` for W1/W2/W3; changed-bit fractions
are `0.3744150797526042`, `0.36834118412990197`,
and `0.38190295649509803`. Raw Hamming distance is not treated
as semantic distance.

## Metadata constancy and model table

Across the frozen carriers, the observed payload-11 body remains 536 bits with
SHA-256 `f24fcee1e5af10619ca538e0b4adf6032d2ae4c201305c883d8341ad8947dec5`,
object count 16, element IDs 1 and 2, and warp raw 3. This is bounded observable
constancy; ETSI interpretation still stops at reserved warp raw 3.

| Model | Result |
|---|---|
| Global gauge only | rejected within tested carriers |
| Common row-wise transfer | rejected within tested carriers |
| Common rank-1 residual direction | rejected within tested carriers |
| Window-dependent support-preserving redistribution | supported within tested carriers |
| Coding-state-associated redistribution | association only |
| General signal-dependent context transform | not identifiable |

## Identifiable and non-identifiable quantities

The experiment identifies source phase, decoded complex coefficients,
gauge-aligned residual vectors, residual direction/rank diagnostics, compatible-
null prediction failures, and bounded public-syntax/raw differences. It does
not identify authored-object-to-row binding, slots, hidden Dolby state, an
exact vendor transform, a synthesis operator, renderer behavior, or a physical
sound field.

## Stop-loss, determinism, and classification

Two complete compact analysis passes were byte-identical. Temporary full
coefficient captures and coding inventories were deleted after compact hashes
were frozen; no new media, Logic action, ADM/DD+ export, or carrier generation
occurred. One further existing-carrier-only temporal/coding-state
characterization is permitted as the stop-loss next step; no new producer
matrix is justified.

Final classification:
`J4R9_ACTIVE_COMPANION_RB_REDISTRIBUTION_REQUIRES_WINDOW_DEPENDENT_TRANSFORM`.

`SemanticBindingState::Unresolved` remains unchanged.
