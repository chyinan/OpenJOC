# J4R5 — Source-matched silent-companion structural intervention

## Decision

`J4R5_STRUCTURAL_EFFECT_NOT_REPRODUCED_WITH_SOURCE_MATCHED_CONTROL`

J4R5 removes the source-lineage confound in J4R4's older static-to-dual
comparison. Four fresh-process Logic 12.3 exports hold the target source,
target track lineage, target position, project baseline, and producer settings
fixed. Conditions differ only by the absence or presence of one second
Object-routed track containing a six-second exact-digital-zero region.

The four stream-copied raw EC-3 carriers are byte-identical. Consequently all
same-condition and all twelve cross-condition target-997 coefficient vectors
are exact repeats: Base, ReconstructionBasis, Joint, RcLfe, and RB support
masks do not change. The earlier descriptive static-to-dual support expansion
is therefore not reproduced by the isolated, source-matched silent-shell
intervention and is downgraded as source/project-lineage confounded.

This is a narrow negative result for this controlled producer configuration.
It does not prove that every silent authored object is ignored, and it assigns
no semantics to ReconstructionBasis rows or reserved warp raw 3.

## J4R4 dependency and intervention contract

J4R4 found no companion-PCM effect above its broad compatible-N1 envelope,
but found a descriptive older-static versus dual-context RB support contrast.
That comparison used non-identical 997 Hz sources. J4R5 instead uses one
canonical target source in both conditions:

- WAV SHA-256 `611587d6369b6bbd58afdfe06b4d307a5eed9fd31eda5ef84abd56a341a51570`;
- PCM SHA-256 `502431d4b8d16c62e67a27f7ab9a4655c9e2c16e6183ae1b68a4fed213fb1287`;
- mono, 48 kHz, signed 24-bit, 288,000 samples, 6.0 seconds, continuous 997 Hz.

The one generated silent source is mono, 48 kHz, signed 24-bit, 288,000
samples, and exactly zero throughout. Its WAV SHA-256 is
`65644cc35c39276b826e858126fcb0023225ae58220cf5994071bf2894ecd490`
and its PCM SHA-256 is
`724f1dbcb897906cf72a211434de246daaa84a62e24baf6762f94d690edad6e0`.

Condition A contains the target Object track only. Condition B is cloned from
the same frozen target-only baseline and adds exactly one Object-routed silent
companion at Front Left. The target remains Front Right. Both have fixed
`Z=0`, `Size=0`, +0 dB, no plugins, no fades, and no mute/solo state. Old
unused media retained in the project package is not treated as an authored
track; the intervention is active track/region topology.

## Project lineage, human assist, and pre-export gates

The canonical admitted J4R1 project was never opened for modification. An
APFS clone was reduced to a frozen `BASE_TARGET_ONLY`, reopened and verified,
then cloned to A0/A1 and a B template. The B template received the one silent
track, passed save/close/reopen verification, and was cloned to B0/B1 while
Logic was absent. Logic was fully quit between runs; the known headless
post-window process was explicitly checked and terminated when necessary.

The only human mechanical action was entering the already specified
companion X automation value `-100`; no human scientific judgment was used.
Automation lanes remained the authoring source of truth and panner UI was
readback only.

Immediately before each DD+ export, bounded track bounces independently
proved the target PCM sample-identical to the canonical source. B0 and B1 also
proved 288,000 exact-zero companion samples. Every run used a distinct frozen
project clone, fresh Logic PID, unique nonce, verified Music/768 kbps/project
settings, and a single attested final export action.

## Four producer runs and exact repeatability

| Run | Condition | MP4 SHA-256 |
| --- | --- | --- |
| A0 | target only | `392fdeeec39a1346454f8effb4b9e751a42b83d8a8d49364d1a3e50aa7f44a82` |
| A1 | target only | `b576a07a35cf03a7fe3e90a56598bc9697b9ffe39a4b1ddb38f7413493eeda4e` |
| B0 | target plus silent companion | `8390c57efc0bf274e0a211b7b388c93d20ae25f2bf6e783c7a3b097f520023d1` |
| B1 | target plus silent companion | `106c28be254718481d1a81d83a96a1e01705bf25ada02fddb2db28652ce1182c` |

All four stream-copied raw EC-3 files are 771,072 bytes and share SHA-256
`94ca909823a9480881add1a9ef20b1c522f7078f8e47032b110ebeadffcfb19a`.
Thus both A0/A1 and B0/B1 pass exact producer repeatability, and the A/B
intervention produces no raw-carrier difference in this configuration. Exactly
four DD+ exports were made; no fifth export was attempted.

## Carrier structure and observed metadata

Each carrier has 251 access units of 3,072 bytes. Every access unit has payload
IDs `11,14,2,1`, a 536-bit payload-11 body, parser-observed object count 16,
element count 2 with IDs 1 and 2, and warp bits `[526,528)=3`. Payload-11 is
invariant within each carrier and identical across all four.

The metadata secondary classification is
`CARRIER_METADATA_UNCHANGED_DESPITE_AUTHORING_INTERVENTION`. This means only
that the compared parser-observed carrier structure did not change. The
observed count is not equated with authored object population, and no hidden
or post-warp vendor semantic is inferred. ETSI_STRICT still stops at
`ReservedWarpMode { raw: 3 }`.

## Same-HEAD numerical analysis

All four raw carriers were decoded at commit
`29b24f39e33a498d3c48a93f9558f201e15cf96d` with
`DOLBY_VENDOR_COMPAT`, trim-configuration count 1, CurrentDefault Base policy,
and reference-f64 precision. `SemanticBindingState::Unresolved` is unchanged.

The source-to-decoded mapping is `decoded_sample = source_sample + 1536`,
derived from target-source timing rather than an A/B RB difference. The three
predeclared 24,000-sample source windows are W1 `[60000,84000)`, W2
`[156000,180000)`, and W3 `[204000,228000)`.

Because each raw pair is byte-identical, same-condition projective residuals
are exactly zero. The structural envelopes remain the inherited lower floors:

| Partition | E_A | E_B | E_STRUCTURAL |
| --- | ---: | ---: | ---: |
| Base | `0` | `0` | `3.7834930931104663e-6` |
| ReconstructionBasis | `0` | `0` | `2.3853247222861374e-7` |
| Joint labeled Base+RB | `0` | `0` | `3.7834930931104663e-6` |

The RB support floor is `9.54129888914455e-7`. It is used only for RB support,
not as a Base or Joint decision threshold.

## Twelve cross-condition comparisons

All A0/A1 × B0/B1 × W1/W2/W3 comparisons are retained separately. For every
one:

- Base, RB, and Joint projective residual are exactly `0`;
- fitted relative magnitude is `1`, relative magnitude residual is `0`, and
  relative phase residual is numerical zero;
- complex coherence is numerical unity;
- RcLfe is exact zero on both sides and remains separate from Joint;
- the RB mask is unchanged at `row_000..row_008`;
- gained rows and lost rows are both empty.

No cross-condition RB residual exceeds `E_STRUCTURAL`; the strict all-twelve
effect rule therefore fails in the no-effect direction without averaging away
any run or window.

## Consequence for J4R4

J4R5 does not reproduce J4R4's older S0→S1 support expansion after matching the
target waveform and project lineage. The former
`OBJECT_POPULATION_OR_STRUCTURAL_CONTEXT_ESTABLISHES_RB_SUPPORT` label must not
be retained as a source-matched causal claim. Its large contrast is now most
plausibly attributed to the unresolved old source/project-lineage difference,
not to the isolated presence of the tested exact-zero authored shell.

What remains unknown is which older project or source distinction generated
that contrast, whether a different silent-shell construction could matter, and
whether effects smaller than the retained numerical floors exist. A null result
for this exact intervention is not a universal producer or codec rule.

## Claim boundary, storage, and final classification

ReconstructionBasis rows remain coordinate labels, not authored objects.
Authored-object-to-row identity, authored-object PCM inside RB, slot identity,
a complete context operator, hidden Dolby semantics, renderer semantics, and
audio-bound ObjectScene remain inadmissible. RcLfe remains separate.

Analysis used compact coefficient reports. Temporary reference-f64 captures
were provenance-frozen and then deleted; no large decoded tree is retained.
The four small producer carriers and bounded source-gate bounces are retained
as private experimental evidence; no private media is committed publicly.

Final classification:

`J4R5_STRUCTURAL_EFFECT_NOT_REPRODUCED_WITH_SOURCE_MATCHED_CONTROL`

The preferred next milestone, subject to separate authorization, is to revisit
the older S0 lineage difference using existing evidence before creating a new
semantic model or additional fixture.
