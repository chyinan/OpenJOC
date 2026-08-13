# J4R8 — Source/project-matched active-companion intervention

## Decision

`J4R8_ACTIVE_COMPANION_PCM_CAUSES_TARGET_RB_CHANGE_ADMITTED`

Within the tested source/project-matched Logic 12.3 configuration, changing
only the otherwise unchanged companion Object's PCM from exact zero to a
continuous 2003 Hz signal reproducibly changes the target 997 Hz
ReconstructionBasis coordinate representation. The secondary classification
is `ACTIVE_COMPANION_SUPPORT_PRESERVING_REDISTRIBUTION`: the representation
changes, but the supported coordinate set remains `row_000..row_008`.

## Dependencies and stop-loss boundary

J4R7 closed the legacy-project source-swap investigation with residual
project/container dependence. J4R8 does not reopen legacy S0, amplitude,
duration, or container archaeology. Its B control is the admitted J4R5
target-plus-silent-companion lineage. The historical J3R13/J3R14 corpus
results remain valid descriptions of that corpus, but J4R8 supplies the first
source/project-matched evidence here for a companion-PCM activity effect.

## Source, project, and producer lineage

The target remains the frozen mono 48 kHz, 24-bit, 288,000-sample 997 Hz
source (`611587d6369b6bbd58afdfe06b4d307a5eed9fd31eda5ef84abd56a341a51570`
as a WAV; PCM payload `502431d4b8d16c62e67a27f7ab9a4655c9e2c16e6183ae1b68a4fed213fb1287`).
The new companion is a deterministic mono 48 kHz, 24-bit, 288,000-sample
continuous 2003 Hz source at nominal amplitude 0.2:

- WAV SHA-256: `9f47a90273299e689b0f799090adeeeafe4b18100edf1dd29cab46b974f3c4d5`
- PCM SHA-256: `35db8857cf3209d81d1431bfdf3444247f41e60b783ff891a08e109a7d266872`
- fitted amplitude: `0.19999999942673055`

The C template was cloned from the J4R5 B template. The companion track,
Object routing, Front-Left position, gain, and six-second origin were retained;
only its region PCM changed from exact zero to the continuous source. The
target remained Front Right. Save, close, full Logic quit, reopen, and timeline
readback passed. No scientific judgment was delegated to the human.

Immediate pre-DD+ mono bounces from C0 and C1 recovered both 288,000-sample
source payloads exactly at origin; each deterministic 6,912-sample bounce tail
was exact zero. Two and only two C DD+ exports were made with Music/768 kbps
project settings. Their MP4 containers differ, while their stream-copied raw
EC-3 files are byte-identical at SHA-256
`05c2ff7df2e9791101615893a3e35119cb791bae0d19003a248a97023f52eed3`.
The J4R5 B0/B1 raw EC-3 repeat is likewise byte-identical.

## Carrier and metadata gates

Each B/C carrier contains 251 access units of 3,072 bytes (385,536 decoded
samples at 48 kHz), valid JOC payloads, 15 ReconstructionBasis coordinates,
five full-band Base components, and separate RcLfe. All decoded arrays are
finite and length-compatible.

Across all four carriers and all 251 access units, the bounded observed
metadata signature is identical:

- payload IDs `11,14,2,1`;
- payload-11 length 536 bits and body SHA-256
  `f24fcee1e5af10619ca538e0b4adf6032d2ae4c201305c883d8341ad8947dec5`;
- observed object count 16;
- two elements with IDs 1 and 2;
- addbsi complexity index 16;
- warp bits raw 3.

This is an observed carrier-structure constancy gate, not a claim about hidden
vendor state. ETSI interpretation stops at `ReservedWarpMode { raw: 3 }`.

## Same-HEAD analysis and alignment

B0, B1, C0, and C1 were decoded at commit
`878105679e853f3465812c2a4ac69adf856b3906` with one continuous stateful
prefix, `DOLBY_VENDOR_COMPAT`, trim count 1, `CurrentDefault`, reference-f64,
and component order Base `FL,FR,FC,SL,SR`, RB `row_000..row_014`, with RcLfe
separate.

The source-to-decoded delay was independently verified as 1,536 samples from
the companion-activity onset: in every B/C repeat pairing, decoded Base is
exactly equal over `[0,1536)` and first differs at sample 1536. This timing gate
does not use the target-997 RB comparison. The frozen source-domain windows
are W1 `[60000,84000)`, W2 `[156000,180000)`, and W3
`[204000,228000)`; decoded windows add 1,536 samples.

## Propagation and tight activity envelopes

The simultaneous complex least-squares estimator fits 997 and 2003 Hz. C's
2003-Hz labeled Joint norms are `0.27881789756347214`,
`0.2788167422898916`, and `0.2788180520343959` in W1/W2/W3, each far above
the predeclared propagation floor `0.00016745250526579124`. B retains the
compatible silent-source nuisance floor.

The pre-frozen activity envelope is
`max(compatible numerical floor, B repeat, C repeat)`, with no multiplier and
no imported broad J4R3 N1. It is:

| Partition | `E_ACTIVITY` |
|---|---:|
| Base full-band | `3.7834930931104663e-6` |
| RB | `2.3853247222861374e-7` |
| labeled Base+RB Joint | `3.7834930931104663e-6` |

The support floor `9.54129888914455e-7` is used only for RB support.

## Twelve B-to-C comparisons

Because each same-condition raw pair is byte-identical, the four repeat
pairings reproduce the following value in each window:

| Window | Base residual | RB residual | Joint residual | RB support |
|---|---:|---:|---:|---|
| W1 | `1.5242341150598972e-6` | `1.6350083054675358e-5` | `1.145091292184873e-5` | `row_000..row_008` unchanged |
| W2 | `3.5104069567585366e-7` | `3.0134562889656103e-4` | `2.1008193444960242e-4` | `row_000..row_008` unchanged |
| W3 | `1.973539286453972e-7` | `1.881715164302418e-5` | `1.311942214170177e-5` | `row_000..row_008` unchanged |

All 12 Base residuals remain below the Base envelope. All 12 RB and all 12
Joint residuals exceed their corresponding tight envelopes. Complex
coherence, magnitude, phase, and sign are retained in the machine-readable
record; no failed repeat or window is averaged away. No RB support is gained
or lost. RcLfe is an unchanged separate zero-reference component.

## Consequence and claim boundary

The observed effect is a support-preserving change in ReconstructionBasis
coordinates, not Base full-band PCM. It justifies no authored-object-to-row
identity, companion-to-row identity, OAMD slot meaning, cross-object DSP
formula, complete transform, renderer behavior, or audio-bound ObjectScene.
`SemanticBindingState::Unresolved` remains unchanged.

Temporary same-HEAD decoded arrays were retained only through compact evidence
freezing and then removed. No full RB WAV tree or persistent reference-f64
archive was created. The next bounded question, subject to separate reviewer
authorization, is to characterize the observed transform using only these
existing B/C carriers; no immediate amplitude matrix is justified.

## Final classification

`J4R8_ACTIVE_COMPANION_PCM_CAUSES_TARGET_RB_CHANGE_ADMITTED`

