# J4R1 — ADM object-channel PCM propagation

## Decision

`J4R1_ADM_OBJECT_PCM_PROPAGATION_ADMITTED`

J4R1 establishes a bounded authoring-representation contract from deterministic
source PCM, through active Logic track regions and object routing, to
AXML/CHNA-linked ADM object-channel PCM. Both object channels in the new
diagnostic ADM reproduce their intended six-second sources exactly at sample
origin. This is an engineering and experiment-lineage result only; it does not
establish any JOC reconstruction-row or authored-object identity.

## Dependency on J3R15

J3R15 remains `J3R15_EXPERIMENT_EXECUTION_OR_LINEAGE_INADMISSIBLE`. Its first,
position-contaminated ADM carried source-exact object PCM, whereas its later
position-correct ADM carried exact zeros on the same AXML/CHNA-linked object
channels. Logic backup metadata brackets a transition from both J3R15 source
files being active project audio to `AudioFiles=[]` with those files listed as
unused, and the saved project view contained no audio regions. The narrowest
supported cause is `REGION_NOT_BOUND_TO_EXPECTED_TRACK`; the exact GUI action
that removed or unbound the regions is unresolved.

The failed J3R15 DD+ carrier is not retroactively repaired or reclassified.

## Deterministic source oracle

The two sources were generated and frozen before Logic import. Both are mono,
48 kHz, 24-bit, 288,000-sample (6.0-second) WAV files with conservative peak
amplitude 0.2.

| Role | Signal | WAV SHA-256 | PCM SHA-256 |
| --- | --- | --- | --- |
| Target | continuous 997 Hz | `611587d6369b6bbd58afdfe06b4d307a5eed9fd31eda5ef84abd56a341a51570` | `502431d4b8d16c62e67a27f7ab9a4655c9e2c16e6183ae1b68a4fed213fb1287` |
| Companion | zero / 2003 Hz / zero | `879d73a0d71eeaa4138ddb3423964abe6f813b14a717360625ca1ff0833d78e6` | `7b8b22c96c0a78e1738ced9ed9504d2e132b097e710fd61bd7eab8a73bde13d2` |

The companion is exact digital zero over samples `[0,96000)`, a 2003 Hz sine
over `[96000,192000)`, and exact digital zero over `[192000,288000)`.

## Logic project and pre-export PCM gate

The disposable Logic Pro 12.3 (build 6674) Atmos project is identified as
`J4R1_ADM_PCM_PROPAGATION`. Its saved metadata lists the two new source files as
active audio rather than unused media. The target and companion regions begin
at project sample zero on the intended object tracks. The target remained at
Front Right (`X=+1`, `Y=+1`) and the companion at Front Left (`X=-1`, `Y=+1`),
with elevation and size zero in the authoring UI. Region placement, routing,
static position, and save/close/reopen persistence were independently checked.
No human assistance or listening judgment was used.

Before ADM export, bounded mono track bounces recovered the sources exactly:

- target bounce WAV SHA-256
  `650cbe599456c8e6fd8a5e2fcb0f455ef482417efa078b272f31d3ee92a35b20`;
  its raw PCM is identical to the target oracle;
- companion bounce WAV SHA-256
  `2eb811cfd37de8e9742fcd6cc96806e41f355123a57ef5794e1f33b82bb852d8`;
  its raw PCM is identical to the companion oracle.

This gate proves that the intended nonzero source audio was present before ADM
export; waveform display alone was not used as evidence.

## ADM structure and linkage

Exactly one diagnostic ADM-BWF was exported:

- SHA-256: `67dc46927edd241f48ce6cc2ac4ad2a293d73451c79548c7eb8aa24df49977d3`;
- size: 13,850,456 bytes;
- PCM: 12 channels, 48 kHz, 24-bit, 384,000 samples (8.0 seconds).

The producer command was Logic's project-as-ADM-BWF export. The exact
per-dialog option inventory and configured programme-range value were not
captured independently and are recorded as `NOT_CAPTURED`; the resulting file
format, duration, source span, and zero tail are measured facts rather than
reconstructed UI settings.

The declared six-second source interval begins at sample zero. Samples
`[288000,384000)` are an explicit two-second producer tail and both object
channels are exact zero there. The output was not silently cropped.

Direct AXML/CHNA reference validation identifies:

- `OBJ_997HZ`: `AO_100B → ATU_0000000B → channel 11`, with parsed AXML
  position `X=+1`, `Y=+1`;
- `OBJ_2003HZ`: `AO_100C → ATU_0000000C → channel 12`, with parsed AXML
  position `X=-1`, `Y=+1`.

Channel number alone was not treated as identity; the object-to-track mapping
is derived through the ADM references.

## Object-channel PCM and source correspondence

| Object channel | OFF_A (0–2 s) | ON (2–4 s) | OFF_B (4–6 s) |
| --- | --- | --- | --- |
| channel 11 / `OBJ_997HZ` | 997 amplitude `0.20000000053627362` | `0.20000000053627287` | `0.20000000053627218` |
| channel 12 / `OBJ_2003HZ` | exact digital zero | 2003 amplitude `0.20000000053627373` | exact digital zero |

For both source/object pairs over samples `[0,288000)`:

- alignment is sample zero to sample zero, with no delay search;
- correlation and least-squares gain are `1.0` and polarity is `+1`;
- maximum integer residual at unity gain is `0`;
- normalized residual RMS is `0.0`;
- raw 24-bit PCM is byte-identical.

The independent lineage analysis was run twice. Both complete reports are
byte-identical with SHA-256
`a34e1aadc020a4922f3c042531727b29616d556b63f07a915d3b1285d71d2200`.
Strict RIFF extraction also matches an independent FFmpeg interleaved-PCM
cross-check.

## Root cause and correction

The earliest supported J3R15 failure boundary is
`REGION_NOT_BOUND_TO_EXPECTED_TRACK`. Its later zero-audio export occurred
after the intended source media ceased to be active project audio. The exact
historical GUI action is not recoverable from durable evidence and is not
guessed.

The minimum correction was a fresh disposable project state with both frozen
files explicitly bound as regions on their intended object tracks at project
origin, followed by a mandatory source-exact pre-ADM track-bounce gate.

`J3R15_ROOT_CAUSE = REGION_NOT_BOUND_TO_EXPECTED_TRACK (exact GUI action unresolved)`

## Claim boundary

The admitted PCM may be called authored-object PCM only inside the
metadata-linked ADM authoring representation. That semantic label is not
transferred to ReconstructionBasis rows. `SemanticBindingState::Unresolved`
is unchanged. No DD+ export, JOC carrier generation, JOC decode, object/row
binding, renderer, or warp/vendor-semantic work was performed.

Large derived-artifact generation remained frozen. The run generated
1,728,088 source bytes and a 13,850,456-byte ADM. It retained 1,786,640 logical
bytes of private analysis inputs and core reports at the pre-finalization
boundary; that count excludes the recursively generated storage,
reference-manifest, determinism, and evidence-freeze records. Bounded pre-ADM
exports generated 4,631,480 bytes in total; three failed attempts (2,901,136
bytes) were deleted after their exact hashes and provenance were frozen,
leaving 1,730,344 canonical bytes. No ReconstructionBasis PCM, reference-f64
archive, or unbounded debug array was generated. Filesystem free space was
13,273,214,976 bytes at the predeclared
start boundary and 12,983,005,184 bytes at the evidence-freeze observation
boundary. The disposable Logic project, source oracles, final bounded track
evidence, and one ADM remain private and uncommitted. The final private
evidence aggregate is SHA-256
`75590c49058c688af123993ecde4bc15085a085f1b4ff450bc7d356903260862`.

The preferred next milestone, subject to separate authorization, is to
reproduce the fixed-target companion OFF→ON→OFF JOC intervention from scratch
using this admitted pre-ADM propagation gate.
