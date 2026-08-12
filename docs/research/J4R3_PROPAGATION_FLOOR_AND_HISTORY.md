# J4R3 — Metric-compatible propagation floor and OFF-state history

## Decision

`J4R3_COMPANION_PROPAGATION_CONFIRMED_AND_OFF_REVERSIBILITY_SUPPORTED`

J4R3 independently confirms propagation of the controlled companion 2003 Hz
signal in the already-frozen J4R2 carrier. The source is exactly zero in every
OFF calibration and holdout window, estimator cross-frequency leakage is more
than eleven orders of magnitude below the observed carrier floor, both OFF
holdouts fit a preregistered metric-compatible nuisance bound, and the ON joint
coefficient is about 1,832 times the largest OFF observation.

This result does not retroactively change
`J4R2_COMPANION_SIGNAL_PROPAGATION_FAILED`. J4R2 correctly failed its different,
predeclared absolute threshold. J4R3 is a new calibration using the same frozen
carrier. The target 997 Hz causal ON/OFF question remains untested.

The frozen compatible-N1 rule also supports OFF-state reversibility: every
A-to-B projective residual is below 110% of the larger within-A/within-B
residual in its partition. This is not an unconditional stationarity claim.
The two pre-intervention OFF windows agree closely, whereas the two
post-intervention OFF windows differ by about 0.8% under the same projective
metric. That broad within-B term sets the compatible envelope and remains an
important declared limitation. No recovery trajectory or causal history model
is admitted.

## Frozen inputs and analysis scope

No Logic project was opened and no media or producer output was generated.
The analysis reused the frozen J4R2 raw EC-3 carrier
`389b0e17a1a5766da89b8ef39f154d1f7217a56d949f8828f4b1c8b0f88d186d`
and the previously admitted source files. The frozen mapping was applied once:

`decoded_sample = source_sample + 1536`

The four 24,000-sample source-domain OFF windows were fixed before examining
the ON result:

| Role | Name | Source samples | Decoded samples |
| --- | --- | ---: | ---: |
| calibration | A1 | `[24000,48000)` | `[25536,49536)` |
| holdout | A2 | `[60000,84000)` | `[61536,85536)` |
| holdout | B1 | `[204000,228000)` | `[205536,229536)` |
| calibration | B2 | `[240000,264000)` | `[241536,265536)` |

The ON observation uses source samples `[156000,180000)` and decoded samples
`[157536,181536)`. ReconstructionBasis row labels in this report are coordinate
labels only; they do not identify authored objects.

The complete reviewer authorization was persisted before preregistration, and
all windows, alignment, floor rules, and history rules were frozen before any
ON-dependent evaluation. An early private working-directory split preceded
durable stage activation; exact protocol hashes and a byte-identical
post-activation replay reconciled it. All final evidence comes from the
activated authoritative directory.

## Estimator integrity and source-domain null

The simultaneous real-valued sine/cosine regression contains 997 Hz, 2003 Hz,
and a constant term. Over each 24,000-sample window the frequency difference
spans exactly 503 cycles. The complete design matrix has condition number
`1.4142157257161052`; its Gram matrix has condition number
`2.0000061188627316`, and the largest normalized inner product between a
997-Hz basis column and a 2003-Hz basis column is
`5.735412145213555e-16`. Synthetic single-tone recovery limits 997-to-2003
cross-frequency leakage to `1.136254379351928e-16`, far below the observed
OFF carrier coefficient.

The canonical companion source is bit-exact zero throughout both OFF regions.
All four OFF analysis windows therefore recover an exact zero 2003 Hz source
coefficient. The source ON window recovers magnitude
`0.20000000053627348`, consistent with the frozen 0.2-amplitude source. The
observed OFF carrier floor is not estimator leakage and is not present in the
source.

## Component-localized 2003 Hz census

The complete/joint metric is the Euclidean norm over five labeled Base
full-band channels plus 15 ReconstructionBasis rows. Base LFE and RcLfe remain
separate; RcLfe is zero in every window.

| Window | Base norm | RB norm | Joint norm | Dominant Base | Dominant RB |
| --- | ---: | ---: | ---: | --- | --- |
| A1 | `1.1151428e-4` | `1.0108631e-4` | `1.5051204e-4` | FC `8.4348009e-5` | row_001 `8.1371045e-5` |
| A2 | `1.1238711e-4` | `1.0203226e-4` | `1.5179409e-4` | FC `8.4516949e-5` | row_001 `8.2233856e-5` |
| B1 | `1.1225276e-4` | `1.0102732e-4` | `1.5102054e-4` | FC `8.4407099e-5` | row_001 `8.1370065e-5` |
| B2 | `1.1319794e-4` | `1.0178440e-4` | `1.5222955e-4` | FC `8.4448360e-5` | row_001 `8.2220649e-5` |
| ON | `1.9989172e-1` | `1.9437612e-1` | `2.7881674e-1` | FL `1.9989169e-1` | row_000 `1.9437611e-1` |

The OFF floor is distributed across Base and ReconstructionBasis coordinates;
it is not a single-row event. The ON contrast is localized chiefly to Base FL
and `row_000`, but those labels do not establish an authored-object binding.

## Metric-compatible floor and propagation contrast

The preregistered upper floor is 110% of the larger calibration joint norm:

`1.10 × max(A1, B2) = 1.6745250526579124e-4`

Both holdouts pass independently:

- A2: `1.5179408833906213e-4`;
- B1: `1.5102053650661352e-4`.

The ON joint norm is `0.2788167423372097`. Relative to the maximum observed OFF
norm (`1.522295502416284e-4`), the ON/OFF ratio is
`1831.5546613299068` and the absolute contrast is
`0.2786645127869681`. The threshold uses OFF data only; it was not derived
from the ON magnitude.

The independently frozen symmetric controls at 1987, 1995, 2003, 2011, and
2019 Hz show the same approximately `1.5e-4` joint OFF floor around the target
frequency, while only 2003 Hz rises to `0.2788` in the ON window. An independent
black-box compatible-base decode of the same carrier reproduces the Base
2003 Hz nuisance at approximately `8.45e-5`. Together with the exact source
null and estimator oracle, this supports the bounded origin classification
`DETERMINISTIC_CODEC_SPECTRAL_FLOOR`. The external decode is numerical
corroboration only, not a normative or semantic oracle.

The independently calibrated result is therefore
`COMPANION_2003_CARRIER_PROPAGATION_CONFIRMED`.

## Target-997 OFF-state history

Target-997 projective residuals were measured separately from companion
propagation:

| Partition | within A | within B | 110% compatible envelope |
| --- | ---: | ---: | ---: |
| Base full-band | `2.0465309e-5` | `8.4074339e-3` | `9.2481773e-3` |
| ReconstructionBasis | `9.0040916e-6` | `7.6587762e-3` | `8.4246538e-3` |
| Joint labeled Base+RB | `1.5990202e-5` | `8.0522761e-3` | `8.8575038e-3` |

Under the frozen protocol, within-A and within-B are finite and deterministic,
and the compatible envelope contains every A-to-B comparison. The exact
predeclared classifier therefore returns
`OFF_STATE_REVERSIBILITY_SUPPORTED_UNDER_COMPATIBLE_N1`. In particular, the
largest cross residual is `8.4172919e-3` for Base, `7.6653554e-3` for
ReconstructionBasis, and `8.0575809e-3` jointly, all below the corresponding
envelopes in the table.

This bounded support should not be mistaken for a stronger absolute
stationarity result. The within-B residual is roughly three orders of
magnitude above the within-A residual in each partition (about 504 times
larger jointly), and the frozen envelope is correspondingly broad. The
calibration supports compatible-N1 reversibility while exposing that
within-B variability; it does not explain its cause.

Accordingly:

- OFF-A stationarity is established within the pre-intervention pair;
- OFF-B stationarity satisfies the frozen compatible-N1 rule, with its broad
  envelope explicitly declared;
- cross-OFF reversibility is supported under compatible N1;
- the bounded post-OFF recovery profile is not run because no history/state
  shift is admitted under that rule;
- target 997 Hz ON/OFF causal classification remains prohibited.

## Claim boundary and next step

`SemanticBindingState::Unresolved` is unchanged. No result identifies an
authored object with a ReconstructionBasis row, admits authored-object PCM
inside JOC, assigns slot identity, completes a synthesis operator, establishes
renderer behavior, or enables an audio-bound `ObjectScene`. RcLfe remains
separate. Warp `[526,528)` remains raw 3 and ETSI strict continues to treat it
as reserved; no vendor semantic rule was added.

The protocol's next step, subject to separate authorization, is the previously
blocked target-997 ON/OFF causal analysis on this same frozen carrier using the
admitted metric-compatible floors. Its interpretation must retain the broad
within-B compatible-N1 limitation. No new producer fixture is justified by
J4R3 alone.

The bounded 189-AU analysis generated 85,022,223 logical bytes of temporary
decode/capture data. Those data were deleted after the two byte-identical
compact analysis passes; retained large-derived bytes are zero.
