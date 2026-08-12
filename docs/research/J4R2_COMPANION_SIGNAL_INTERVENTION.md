# J4R2 — Source-verified companion-signal intervention

## Decision

`J4R2_COMPANION_SIGNAL_PROPAGATION_FAILED`

J4R2 produced one structurally valid Logic Pro Dolby Digital Plus Atmos
carrier from source-exact target and companion tracks. The companion's 2003 Hz
ON interval is plainly present in the decoded component representation, but
the two authored-silent OFF windows exceed the absolute observability floor
frozen before export. The predeclared companion propagation gate therefore
fails. No target 997 Hz ON/OFF effect is admitted, and this is not a no-effect
result.

## Dependency and source lineage

J4R1 admitted the source-to-ADM object-channel PCM chain and localized the
earlier J3R15 failure to regions no longer bound to the intended tracks. J4R2
started from a fresh disposable copy of that admitted project state. Both
canonical 6.0-second, mono, 48 kHz, 24-bit sources were frozen before import:

| Role | Signal | WAV SHA-256 | PCM SHA-256 |
| --- | --- | --- | --- |
| Target | continuous 997 Hz | `611587d6369b6bbd58afdfe06b4d307a5eed9fd31eda5ef84abd56a341a51570` | `502431d4b8d16c62e67a27f7ab9a4655c9e2c16e6183ae1b68a4fed213fb1287` |
| Companion | zero / 2003 Hz / zero | `879d73a0d71eeaa4138ddb3423964abe6f813b14a717360625ca1ff0833d78e6` | `7b8b22c96c0a78e1738ced9ed9504d2e132b097e710fd61bd7eab8a73bde13d2` |

The target remained Front Right (`X=+1`, `Y=+1`) and the companion Front Left
(`X=-1`, `Y=+1`), with fixed elevation and size. Both regions began at project
origin. Save/close/reopen persistence, routing, mute/solo state, unity track
gain, and source assignment were checked independently. No human assistance
or listening judgment was used.

Immediately before the DD+ export, bounded Logic track bounces recovered all
288,000 raw 24-bit samples of each canonical source exactly: delay zero, gain
one, positive polarity, correlation one, and maximum integer residual zero.
The companion bounce retained exact-zero `[0,96000)` and `[192000,288000)`
intervals around the 2003 Hz `[96000,192000)` interval.

## Producer and carrier provenance

All visible producer settings were frozen before the final action:

- Logic Pro 12.3 (build 6674);
- Dolby Digital Plus with Dolby Atmos;
- Music, 768 kbps;
- Project scope;
- simultaneous ADM-BWF export disabled.

A high-entropy output name and pre-invocation absence proof were bound to a
one-shot controller. Its nonce was consumed once, the final Save action was
invoked once, and exactly one matching MP4 appeared in the frozen discovery
realm. No replacement export was attempted.

| Artifact | SHA-256 | Bytes |
| --- | --- | ---: |
| MP4 | `2fbf231b9b1cf677b6d671dfb6dd8951873646abf43f42b1d92030ebf79daf1b` | 774,855 |
| stream-copied EC-3 | `389b0e17a1a5766da89b8ef39f154d1f7217a56d949f8828f4b1c8b0f88d186d` | 771,072 |

The MP4 declares 8.000 seconds. The raw stream contains 251 closed access
units of 3,072 bytes and 1,536 samples each: 385,536 samples, or 8.032 seconds.
The authored causal interval remains `[0,288000)`; the producer tail was
retained, and the analysis windows were not moved in response to target
results.

## Structural and metadata boundary

Raw EC-3 and MP4 paths agree for all 251 access units. Every AU carries payload
IDs `11/14/2/1`; JOC payload parsing succeeds; the 536-bit payload 11 has one
SHA-256 value throughout; and the observed OAMD structure remains at 16
objects and two top-level elements (`1,2`). This is empirical fixed-topology
evidence, not a complete post-reserved-bit semantic parse.

Warp `[526,528)` is raw `3` in 251/251 AUs. `ETSI_STRICT` continues to reject
it as `ReservedWarpMode { raw: 3 }`. No vendor warp rule or semantic mapping
was added.

## Alignment and companion propagation gate

Alignment was derived from the companion's 2003 Hz transition timing before
examining any target 997 Hz ON/OFF effect. The frozen mapping is:

`decoded_sample = source_sample + 1536`

The one-AU delay was applied once to the three predeclared source-domain
windows:

- OFF_A `[60000,84000)`;
- ON `[156000,180000)`;
- OFF_B `[204000,228000)`.

The simultaneous 997/2003 Hz complex least-squares estimator found the
following maximum 2003 Hz component magnitudes:

| Window | Base full-band | ReconstructionBasis |
| --- | ---: | ---: |
| OFF_A | `8.45169e-5` | `8.22339e-5` |
| ON | `0.1998917` | `0.1943761` |
| OFF_B | `8.44071e-5` | `8.13701e-5` |

The predeclared absolute observability floor was
`9.54129888914455e-7`, inherited conservatively from the frozen J3R14
coordinate-support threshold and not derived from this carrier. Although the
ON interval is orders of magnitude above the OFF windows, both OFF windows are
about 85 times above that absolute floor. The exact required
below/above/below pattern is therefore not established under the frozen rule.

This result does not show that the source failed to reach Logic—the immediate
pre-export track gate is source-exact. It shows that the tested decoded
component representation does not meet the predeclared absolute silent-window
criterion. Changing that criterion after observing the carrier would invalidate
the experiment.

## Reversibility and downstream analysis status

The protocol places companion propagation before target reversibility and the
target ON/OFF test. A diagnostic-only OFF_A↔OFF_B check found:

- Base projective residual approximately `3.16e-6`, inside the compatible
  Base N0/N1 envelope `3.7834930931104663e-6`;
- ReconstructionBasis residual approximately `9.02e-6`, outside both the RB
  N0/N1 envelope `2.3853247222861374e-7` and four-times support threshold
  `9.54129888914455e-7`;
- RcLfe remained zero.

Because the earlier companion gate already failed, these values are retained
only as diagnostics; they do not upgrade the primary decision. The symmetric
OFF reference and target 997 Hz ON/OFF Base/RB/joint/support inference are
`NOT_ADMITTED_AFTER_PROPAGATION_GATE_FAILURE`. No companion-signal-dependent
mixing or valid no-effect conclusion is drawn.

## Claim boundary

`SemanticBindingState::Unresolved` is unchanged. ReconstructionBasis rows are
coordinate labels, not authored objects or object PCM. No result establishes
object-to-row identity, slot causality, a complete synthesis operator,
renderer semantics, or an audio-bound `ObjectScene`. RcLfe remains separate.
The J3R13/J3R14 scoped context-dependence results remain admitted, while J3R15
remains inadmissible.

The highest-value next step is a separately authorized null-calibration
milestone for the companion-silence carrier domain, with predeclared
partition-compatible observability bounds. It must not retroactively alter the
J4R2 protocol or reuse J4R2 as a causal target result.
