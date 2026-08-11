# J2R7 — Full-Complex Repeatability Calibration

## Scope and decision

J2R7 closes the sample/AU lineage question raised by J2R6 and applies a
predeclared repeatability hierarchy to the existing full-complex
`ReconstructionBasis` observations. It does not launch Logic, create media,
change the decoder, or assign a row to an authored object.

The narrow primary classification is:

```text
FULL_COMPLEX_DIFFERENCE_ABOVE_NUMERICAL_FLOOR_PRODUCER_NULL_UNAVAILABLE
```

This means the declared cross-carrier differences exceed the frozen numerical
and extraction floor, while a producer-level complex null envelope is not
available. It is not a causal context or slot claim.

## AU and sample lineage

The current phase-row recovery contains 129 complete AUs for the static
controls (198,144 samples at 48 kHz) and 126 complete AUs for the existing
dual-object carrier (193,536 samples). The older 128 and 125 values are
historical analysis spans in J1R10/J2R5, not measurements of a shorter raw
carrier. No AU was silently inserted, removed, shifted, padded, or trimmed.

All target tone windows are explicit sample intervals: 60,000–84,000 for the
pre window and 132,000–156,000 for the late window. Both lie within the common
first-125-AU interval (0–192,000 samples), so the C1/C2/C3 windows close
without relying on a terminal-AU assumption. The dual carrier's additional
complete AU is retained outside the inherited comparison span.

The target lineage is `PARTIAL_PROJECTIVE_ONLY`: source identity and sample
windows are controlled, but absolute phase/gain linkage across the historical
and current component captures is not independently closed. Accordingly, all
cross-carrier comparisons use one global complex gauge and never claim
absolute phase equivalence.

## Null hierarchy and calibration

The calibration manifest was generated before target results and contains no
target observation. The fixed envelope is the maximum admitted N0/N1
projective residual, with no multiplier or post-hoc threshold.

* N0: one same-input Center duplicate; all 15 phase-bearing row files were
  byte-identical.
* N1: 14 within-carrier pre/late steady-state comparisons across the
  ReconstructionBasis and base partitions.
* N2: no phase-bearing equivalent-path capture was available. Existing raw
  versus MP4/container equivalence is not silently promoted to a complex
  component null.
* N3: six independent same-condition raw-export null candidates are admitted
  at the carrier level (Front Left, X-negative half, X-positive half, Y-mid,
  J1R8, and J1R9). They have byte-identical raw EC-3 outputs, but no
  independent phase-row capture. Therefore `N3_complex_metric_count = 0` and
  no producer-level complex envelope is claimed.

The phase-bearing N0/N1 envelopes are approximately:

| Partition | Maximum projective residual |
| --- | ---: |
| Base full-band | 3.7834930931 × 10⁻⁶ |
| ReconstructionBasis | 2.3853247223 × 10⁻⁷ |
| Complete encoded-component descriptor | 3.7834930931 × 10⁻⁶ |

These are empirical maxima, not population confidence intervals.

## Target results

The fixed windows and calibration manifest were applied independently to:

* C1: static Front Left 997 Hz versus dual-object Front Left 997 Hz;
* C2: static Front Right 997 Hz versus dual-object Front Right 997 Hz;
* C3: reciprocal dual-object 997/2003 Hz comparisons.

C1 has ReconstructionBasis projective residual ≈0.00317481 and base residual
≈0.00348488. C2 has ReconstructionBasis residual ≈0.99999927 and base
residual ≈0.00354172. Both exceed the N0/N1 envelope in the declared windows;
the FR contrast is materially larger than FL. C3 residuals are approximately
0.003148–0.003234 for ReconstructionBasis and 0.003609–0.003702 for base.

The localization is descriptive:

* base full-band and ReconstructionBasis both vary beyond the N0/N1 floor;
* the complete descriptor also varies, so no compensating synthesis or
  physical-field equivalence is inferred;
* the base LFE is a zero-reference in these tone fits;
* RcLfe remains separate and was not mixed into ReconstructionBasis;
* a free 15! row permutation search was not performed. No common,
  predeclared permutation was admissible.

The FL/FR contrast is reported as an asymmetry of numerical residuals, not as
a slot effect. Frequency and context are reported separately; neither is
promoted to a universal law.

## Semantic and producer boundaries

`SemanticBindingState` remains `Unresolved`. These results do not admit
authored-object PCM, source-to-slot identity, row identity, audio-bound
`ObjectScene`, renderer output, or a rendered-field equivalence claim. The
ETSI strict result for OAMD `warp_mode = raw 3` remains
`ReservedWarpMode { raw: 3 }`; no vendor rule or opaque-continuation meaning
was added.

No Logic process was launched, no producer project was changed, no new Logic,
ADM, DD+, EC-3, WAV, or other media was generated, and the canonical corpus
was not modified. Detailed hashes and arrays remain in the private J2R7
evidence package.

The next milestone, if authorized, must be reviewer-selected. J2R7 does not
authorize a new fixture or a semantic-binding upgrade.
