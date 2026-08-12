# J3R15 — Fixed-target companion OFF→ON→OFF intervention

J3R15 attempted the single within-carrier experiment proposed by J3R14: keep
one continuous 997 Hz target, two object tracks, their positions, routing and
topology fixed, while changing only the companion source from digital silence
to a 2003 Hz sine and back to digital silence. The predeclared target position
was Front Right and the companion position was Front Left.

## Frozen sources and producer setup

The two mono, 48 kHz, 24-bit source files were frozen before Logic import:

- target: 288,000 samples of continuous 997 Hz,
  SHA-256 `89862d6afbd2e8d213ca37ea8824f374f4dfdb3a2e6f5a10ee337d7f59aa483e`;
- companion: 288,000 samples containing digital silence in `[0,96000)`, a
  2003 Hz sine in `[96000,192000)`, and digital silence in
  `[192000,288000)`, SHA-256
  `b4082d4d4c6c1f842a53eb3c88cecaa99b030679cb4c25b10580ab6995ae702c`.

Decoded PCM in the Logic project bundle is sample-identical to both frozen
sources. Human assistance was limited to two deterministic automation-lane
value entries. OpenJOC independently verified save/reopen persistence and
Object Panner readback: target `(X=+1,Y=+1)` and companion
`(X=-1,Y=+1)`, both with elevation and size zero. No human interpretation or
audio-quality judgment was used.

An initial ADM preflight retained position ramps from the parent shell and was
rejected at the authoring gate. After the fixed values were persisted, a new
ADM export established constant positions and fixed object topology. Exactly
one DD+ Atmos Music/768 kbps/48 kHz carrier was then exported. Its MP4 SHA-256
is `17c3b005b4fc8c1ff88a394d4f5b003204e04b9841a4ef675ba35ab12d518aa1`;
the stream-copied EC-3 SHA-256 is
`9a24c22adf39ee6906ca8d4625456f0ff41cd746f7e166adfe1fb0c4d269d55d`.
No second producer repeat was authorized or performed.

The shell's export boundary remained 8.0 seconds even though the frozen source
intervention occupies 0–6 seconds. The raw carrier contains 251 access units
(8.032 seconds at 1,536 samples per AU). All three predeclared windows remain
inside the frozen source interval:

| window | samples | seconds |
| --- | ---: | ---: |
| OFF_A | `[60000,84000)` | `[1.25,1.75)` |
| ON | `[156000,180000)` | `[3.25,3.75)` |
| OFF_B | `[204000,228000)` | `[4.25,4.75)` |

## Structural and metadata gate

The raw carrier is recognized as E-AC-3 JOC. It supplies 15 diagnostic
ReconstructionBasis rows; RcLfe remains separate and all frozen windows are
covered. A bounded 180-AU decode is byte-identical to the corresponding prefix
of the full internal-base decode, so no alternate window alignment or
per-window reset is introduced.

Across 251/251 access units, payload 11 is one invariant 536-bit body with
`object_count=16` and two top-level elements. This is empirical metadata
constancy only. `warp_mode [526,528)` is raw 3 in every AU and remains ETSI
`ReservedWarpMode`; no post-warp or vendor semantics are inferred.

The corrected ADM identifies `OBJ_997HZ` at Front Right and `OBJ_2003HZ` at
Front Left for the export. However, the CHNA-mapped PCM channels for those two
objects (tracks 11 and 12) are both bit-exact digital zero for all 384,000 ADM
samples. ADM therefore qualifies the fixed positions and topology, but not the
intended object-audio lineage.

## Signal intervention gate

The already validated simultaneous 997/2003 Hz complex estimator was applied
without moving the predeclared windows. The 997 Hz target remains observable
in all three windows. The required companion pattern is not observable:

| window | 2003 Hz Base+RB joint norm |
| --- | ---: |
| OFF_A | `1.5755161961256847e-4` |
| ON | `1.5398908190998142e-4` |
| OFF_B | `1.56965910887735e-4` |

The two OFF windows are not below the frozen observability floor, and ON does
not introduce a distinct 2003 Hz component. Consequently, the source file's
OFF→ON→OFF content was not propagated into the qualified producer output. The
experiment fails before any causal ReconstructionBasis comparison is
admissible.

For forensic completeness only, OFF_A versus OFF_B at 997 Hz gives projective
residuals `7.66133329392753e-4` for Base,
`5.243030138727352e-4` for ReconstructionBasis, and
`1.2778919065390873e-3` for the labeled joint descriptor. These also exceed
the earlier compatible numerical envelopes, but the upstream lineage failure
already determines the classification. OFF_A-versus-ON diagnostics are not an
ON/OFF causal result. Rows 000–008 exceed the frozen support floor in all three
windows, with no gained or lost rows; RcLfe is zero.

## Decision and boundaries

The required primary classification is:

`J3R15_EXPERIMENT_EXECUTION_OR_LINEAGE_INADMISSIBLE`

This is not evidence that companion PCM has no effect. It means the intended
companion intervention itself did not survive the producer-to-export lineage,
so neither signal-dependent mixing nor a no-signal-effect result can be
admitted. J3R14's general context-conditioned mixing class remains
underdetermined and receives no causal refinement.

`SemanticBindingState` remains `Unresolved`. No authored-object-to-row
identity, authored-object PCM, slot causality, complete reconstruction
operator, audio-bound ObjectScene, renderer behavior, or vendor warp semantics
is admitted. This is the final stage of its autonomous session; no Stage 16
exists.
