# OpenJOC Research Notes

> Historical research record. This file is not the canonical current
> capability, limitation, requirement, or roadmap document. Dated statements
> describe what was known at the time; current truth belongs in `docs/`.

## Clean-room boundary

OpenJOC is an independent implementation based only on the normative ETSI
documents and their official companion data. Proprietary Dolby source and vendor
decoder implementations are excluded as implementation inputs. Informative
research may only cross-check architecture after normative behavior is derived.

## Normative sources verified locally

- ETSI TS 103 420 V1.2.1 (2018-10), `references/etsi/ts_103420v010201p.pdf`.
  Local SHA-256: `e532bfc4f8be4a97c7c9cdd9f6bcc40634ecf8ef93a1dc490fcb15c16265daa7`.
- ETSI TS 102 366 V1.4.1 (2017-09), `references/etsi/ts_102366v010401p.pdf`.
  Local SHA-256: `0229e151dfd9f8cec427f234798cac679a66fdec096feecc4d5ce455bbe3cadf`.
- TS 103 420 companion archive,
  `references/etsi/ts_103420v010201p0.zip`. Verified SHA-256:
  `a79cf108c4529b7d9ca9525c871183a70b1732ed6df03a3d85b2f31be46eeced`.
  The archive contains only `ts_103420_tables.c`; its extracted hash must be
  verified by the importer before parsing.

The PDFs and companion archive are research inputs and must not be redistributed
as project source. Generated tables remain local until their redistribution
status is separately reviewed.

## Normative implementation map

- TS 103 420 clause 4 defines OBA coordinate systems, decoding, and the decoder
  interface. It establishes the renderer-independent object-essence boundary.
- Clause 5 defines OAMD structure, syntax, semantics, timed/reused property
  updates, positions, extents, gain, priority, channel lock, zones, divergence,
  trim, and extended-precision positions.
- Clauses 6.2 and 6.3 define the retained JOC syntax and field semantics.
- Clause 6.5 and Table 54 define the exact mapping from 64 QMF subbands to each
  allowed parameter-band count; equal-width bands are non-conforming.
- Clauses 6.6.2 through 6.6.6 define differential reconstruction, Annex A
  Huffman use, 96/192-level dequantization, temporal interpolation with
  cross-frame state, and complex-domain object reconstruction.
- Clause 7 defines the direct reference complex QMF analysis and synthesis,
  including 64 bands, 640 prototype coefficients, and state handling.
- Clause 8 restricts the E-AC-3 integration and assigns EMDF payload IDs 11
  (OAMD) and 14 (JOC). TS 102 366 Annex E supplies E-AC-3 syncframe syntax and
  Annex H supplies EMDF container syntax and semantics.
- TS 103 420 Annex A names the six normative Huffman tables supplied by the
  companion archive. Annex B is informative ADM conversion guidance.

## Companion data expectations

The importer must verify the archive and extracted-file hashes before accepting
data, then validate these exact declarations:

| Declaration | Elements |
| --- | ---: |
| `joc_huff_code_coarse_generic` | 95 nodes |
| `joc_huff_code_fine_generic` | 191 nodes |
| `joc_huff_code_coarse_coeff_sparse` | 95 nodes |
| `joc_huff_code_fine_coeff_sparse` | 191 nodes |
| `joc_huff_code_5ch_pos_index_sparse` | 4 nodes |
| `joc_huff_code_7ch_pos_index_sparse` | 6 nodes |
| `prot64` | 640 coefficients |

## Verification policy

Every normative behavior receives a clause reference in rustdoc and at least one
behavioral test. Exhaustive domains (Huffman leaves, dequantization levels, and
Table 54's 512 mappings) are tested exhaustively. Parser boundaries use checked
arithmetic, bounded allocation, structured errors, and fuzz targets. Completion
requires a legally generated real JOC vector and cannot be inferred from
synthetic tests or successful compilation.

## Open external dependency

No legal real-world `.ec3`/`.eac3` JOC vector is currently present in the
workspace. This does not block reference-core implementation, but it does block
the Mandatory end-to-end acceptance gate until such a vector and its authoring
ground truth are supplied or legally generated.

## TS 102 366 page 58 operator recovery

The clause 6.2.2.3 `logadd(a, b)` pseudocode uses a layout-sensitive operator.
In V1.4.1 (local SHA-256
`0229e151dfd9f8cec427f234798cac679a66fdec096feecc4d5ce455bbe3cadf`) the
300-DPI Poppler render shows a missing-glyph square and layout extraction emits
control byte `0x01`; object inspection identifies embedded Type3 font `T13`
with a solid 33x41 placeholder bitmap. This is not sufficient evidence on its
own.

As an independent authorized-artifact check, the official ETSI V1.1.1, V1.2.1,
and V1.3.1 PDFs were downloaded from the ETSI delivery URLs and their matching
clause pages were rendered at 300 DPI. V1.1.1 and V1.2.1 contain the same
embedded 21x6 Type3 glyph, visibly rendered as the dedicated `~` operator;
V1.3.1's extraction again loses it but retains the same operator position. The
surrounding normative prose defines the operation as computing the difference
between the operands, and the sign branch selects the larger operand. OpenJOC
therefore models the glyph as the named `log_add` primitive with `c = a - b`,
the clamped `abs(c) >> 1` address, and Table 6.14 correction. This records the
dedicated glyph rather than silently treating it as an ordinary source-language
operator; no decoder implementation was consulted.

## TS 102 366 bit-allocation page inspection

Authorized pages 59 through 65 and Annex E pages 151 through 157 were rendered
losslessly at 300 DPI with Poppler 26.02.0 and visually inspected. Pages 61 and
62 make Tables 6.6 through 6.12 unambiguous. Pages 64 and 65 together complete
Table 6.16: the continuation values are addresses 28 through 30 mapping to 9,
31 mapping to 10, and addresses 60 through 63 mapping to 15. Annex E pages 152
and 153 make the complete 64-entry `hebaptab` and hebap quantizer mapping
legible. Layout-sensitive AHT/GAQ expressions on pages 154 through 157 were
inspected but are not yet implemented.

## TS 102 366 `calc_lowcomp` ambiguity

The excitation-function pages were independently rendered at 300 DPI with
Poppler 26.02.0: V1.1.1 page 53, V1.2.1 page 54, V1.3.1 page 58, and V1.4.1
pages 58-59. Every render visibly prints `if ((b0 + 256) == b1);` in the
first `bin < 7` branch, while the corresponding `bin < 20` branch omits the
semicolon and uses a normal `else if`. Literal C interpretation would make
the first block unconditional and leave an invalid `else if`; the normative
algorithm's branch structure requires the condition to govern the block.
OpenJOC therefore implements the structured branch interpretation and keeps
this as an explicit compatibility/TODO item pending an ETSI correction. No
decoder implementation was consulted.

## TS 102 366 SNR-offset shift ambiguity

The initialization pseudocode on V1.4.1 page 57 prints the uncoupled,
coupling, and LFE expressions as
`((csnroffst - 15) << 4 + <fine>) << 2`. The same source layout is present in
the inspected V1.1.1, V1.2.1, and V1.3.1 artifacts. In C-like precedence,
the unparenthesized `+` would be part of the right shift count, which is not
consistent with the defined coarse/fine fixed-point fields or the bounded
SNR-offset domain. The only dimensionally consistent reading is
`(((csnroffst - 15) << 4) + fine) << 2`, i.e. `(coarse - 15) * 64 + fine * 4`.
OpenJOC records this as an explicit normative ambiguity and uses that reading
for the pure offset helper. A legal conformance vector or ETSI correction
remains the compatibility gate; no decoder implementation was consulted.

## Enhanced AC-3 AHT boundary

TS 102 366 Annex E.2.4 defines Adaptive Hybrid Transform (AHT) vector/gain
syntax separately from conventional mantissas. The current first-block decoder
detects any frame/channel/coupling/LFE AHT flag and returns a structured
unsupported-feature error before consuming mantissa bits. This is an explicit
implementation boundary, not a conventional-mantissa fallback. AHT requires
independent page-151 through page-156 formula inspection and conformance
vectors before it can be added.

## Controlled external block-anchor investigation (G1--G10 Phase-A)

This research line is suspended at a principled evidence boundary. It is not
abandoned or declared complete: further marker redesign is not justified
without a validated, generalizable mechanism. The next documented transition
is J1 -- Single-Object Semantic Binding Corpus; J1 is not implemented by this
milestone.

## J1R8 controlled Z/elevation calibration (2026-08-10)

J1R8 is a one-fixture, evidence-first calibration of the already identified
ETSI position Z syntax. The private run
`20260810T032631Z_j1r8-z-elevation-calibration_c90779b` is frozen by an
aggregate SHA-256 of
`faeaf08c88f2aa8d241262de6edf6ab60e35ccdd959fa91239f6640f94779c8a`.
The Logic automation parameter was `对象位置提升`; the independently
verified persisted values were `0, 50, 100, 0`. ADM showed the intended
baseline → approximately 0.5 → 1.0 → baseline Z timeline while X = -0.0
and Y = +1.0 remained fixed. The raw normative Z spans are sign
`[64,65)` and magnitude `[65,69)`; the magnitude codes over the full AU
timeline were `0,3,6,7,13,14,15,10,3,1,0`. This establishes controlled
numeric alignment and return-to-baseline evidence without asserting a linear
conversion formula.

The source PCM is sample-identical to the frozen Center control. R0/R1
stream-copied raw EC3 is byte-identical (129 AUs, 3072 bytes/AU). X/Y
coordinate values remain invariant. `warp [526,528) = raw 3` remains an
ETSI-reserved value and no vendor rule is added; the empirical suffix
`[528,536) = 00000000` remains invariant for this Z control and is not called
padding or otherwise assigned semantics. An exploratory raw prefix interval
`[177,182)` changes, but it is deliberately unnamed.

The Size branch is frozen rather than rescued: Object Size authoring and ADM
propagation are established, while tested DD+ Size-state semantics,
deactivation as a payload-11 transition, direct size-index response, and
Size-related warp/suffix response remain unresolved or unobserved. No JOC,
ObjectScene, object PCM, production parser, or profile behavior was entered.
Controlled 3D X/Y/Z position calibration is closed for this evidence scope;
complete OAMD timeline/state semantics and downstream binding/fidelity remain
open. No second Z fixture is justified by this calibration.

### G1 -- G3: repair the fixture, then freeze semantic identity

G1 was an invalid Logic fixture, not decoder behavior: the surround anchor
track was muted, `OBJ_997HZ` was soloed, and the anchor region was not at the
timeline origin. G2 repaired routing and produced six nonzero decoded
channels, but the marker design still had insufficient surround orthogonality
and unsuitable LFE marker energy. G3 established unique six-channel semantic
identity across OpenJOC, FFmpeg, and the Apple diagnostic path under the
controlled corpus; the source detector recovered 480/480 blocks. This became a
frozen control, not a universal channel-order claim.

### G4 -- G6: reject energy-only fixes and expose the trade-off

G4's uniform Layer-B amplitude increase failed the frozen Layer-A source
identity margin and was stopped before Logic. G5 introduced an energy-neutral
distributed marker and solved source/guard visibility, but one low-score
`C/AU11/block5` result remained after the external decode. G6 improved codec
survivability above the score floor while degrading localization through
competing correlation peaks. The result was a survivability/localization
trade-off, not evidence of a TDAC defect.

### G7 -- G8: freeze scorer-native constraints and reject weak proxies

G7 evaluated 96 predeclared BPSK candidates. All passed energy and source
480/480, but none passed the complete Layer-A identity gate; the best margin
was `0.1985783149948018`, below the frozen `0.20`. G8 rejected a simple
waveform/Layer-A-subspace projection hypothesis because it remained below the
predeclared `0.40` partial-mechanism threshold. No Logic candidate was
admitted by either result, and neither detector threshold nor identity margin
was relaxed.

### G9: scorer-native source solution, external local ambiguity

The scorer audit separated self-normalization dilution (`41.27%`) from cross
confusion (`58.73%`), dominated by the temporal-code score; the recurring
second-best permutation was `L ↔ Rs`. This moderate scorer-native mechanism
admitted one frozen G9 family: 32 predeclared global candidates, nine source
eligible, winner `triple56_k2_n100_e075`. Anti-overfit exclusions left the
winner unchanged. The formal source passed identity margin `0.2461121871`,
480/480 detection, minimum score `0.6907968032`, minimum localization margin
`0.3029205927`, zero jitter, energy, spectral, and guard gates.

The G9 Logic carrier was then decoded through the four required paths and the
Apple diagnostic path. Each required path recovered 461/480 blocks. Every
remaining failure was margin-only (`<0.01`); no required path failed score or
jitter. The Phase-A full-curve audit found the same local competing-peak
histogram on every required path: `-1 × 8`, `-2 × 11`. FFmpeg raw and MP4
curves were identical, and OpenJOC curves were nearly identical to them. This
supports a controlled-carrier, cross-decoder diagnostic observation only; it
does not establish a Dolby encoder kernel or universal E-AC-3 shift rule.

### G10 Phase-A: stable structure, no predictive mechanism

The predeclared global classes selected `M2_asymmetric_local_smoothing` for
all four required paths, but that model failed the frozen generalization
gate: minimum cross-validated Spearman was `-0.20040975`, and training while
excluding all 19 known failures yielded classification accuracy `0`. Thus a
stable offset structure was observed, but it is not a validated predictive
broadening model. G10 marker construction was not justified and was not
started; no second Logic candidate was tested.

### Rejected explanations and preservation rules

- Do not reopen generic TDAC as the anchor explanation without contradictory
  evidence. The independent TDAC arithmetic, carry continuity, and exact-AU
  history experiments remain separately supported.
- Do not lower frozen detector thresholds, identity margins, or block gates.
- Do not treat `461/480` as complete external mapping.
- Do not infer an encoder smoothing kernel from the `-1/-2` histogram.
- Do not use ordinary waveform projection as the G7 identity-loss mechanism.
- Do not solve fixture failures with per-channel, per-block, or sample-specific
  exceptions.
- FFmpeg and Apple remain black-box comparators; ETSI remains normative where
  applicable.
- `raw warp=3` remains opaque: ETSI strict rejection and bounded vendor
  compatibility are unchanged, with no alias or remapping.

### Current boundary and next line

`external_block_mapping_established = false`. Parser-emitted coding-tool
inventory is implemented and diagnostic; exact external block-wise causal
attribution is still open. JOC remains evaluation-only: no complete OAMD
semantic timeline, authored-object/JOC-row verification, object PCM fidelity,
ADM/render fidelity, or resolved `warp=3` semantics is claimed. The active
mainline now transitions from external block-anchor refinement to controlled
JOC/OAMD semantic binding (J1), without implementing J1 in this commit.

## J1 semantic-binding / OAMD spatial-field investigation (2026-08-09)

This line is a controlled evidence chain, not a decoder-relaxation effort. It
keeps the human-assisted Logic authoring protocol explicit: the automation lane
is the authoring source of truth, the Object Panner is timeline readback only,
and Codex independently verifies the value before and after save/reopen. No
private Logic project, ADM BWF, MP4/EC3, manifest, or forensic output is part
of this repository.

J1R3 established persisted Front-Left automation after the earlier failed
authoring attempt was correctly classified as `GUI_CAPABILITY_LIMITATION`,
not a Logic persistence failure. J1R4 qualified the persisted position through
ADM; J1R5 established deterministic DD+ carrier propagation. J1R6B then
qualified the four-position carrier corpus (Center, Front Left, Front Right,
Rear Center) and established raw LR/Front-Back differential structure, while
deliberately stopping before normative field identity.
J1R6C added independently ADM-qualified half-step controls and a Y-mid control.
J1R6C's frozen five-bit Front/Back summary discrepancy was reconciled by
J1R6C-R: the canonical field is the full six-bit Y field, and the historical
`[58,63)` value was only its first five bits. J1R6D tested hypotheses H0/H1/H2
without allowing them to move the cursor; all 7 fixtures × 129 AUs (903
observations) closed identically, so semantic selection was not discriminated.

### J1R7A normative cursor result

The private run
`20260809T180109Z_j1r7a-spec-anchored-oamd_b6eb1de` is frozen by
`j1r7a_spec_cursor_evidence_freeze.json` (SHA-256
`572209bcb35cf2b37a512df1c9523b1a8762a2672445f96e57ad48a09257ba4f`). Its
cursor starts at payload-11 bit 0 and follows only the authorized ETSI
syntax. Across seven frozen sources × 129 AUs, all 903 observations share the
same bounded payload/element/field boundaries and close through the normative
prefix `[0,526)`.

The first normative ambiguity is trim `warp_mode` at payload-relative
`[526,528)`: raw bits `11`, integer `3`. ETSI TS 103 420 V1.2.1 Table 32
defines `0b1X` as reserved, so the strict parser remains
`ReservedWarpMode { raw: 3 }`. No vendor alias, remap, or new compatibility
rule is admitted. The result is not a claim that the Logic encoder is
non-conforming; it is a precise boundary between published normative syntax
and an observed commercial carrier.

Before that blocker, the cursor reaches two exact six-bit spatial fields:
`pos3D_X_bits = [52,58)` and `pos3D_Y_bits = [58,64)`. The independently
authored and ADM-qualified controls align numerically with these fields:
X = -1, -.5, 0, +.5, +1 maps to 0, 16, 31, 46, 62; Y = +1, 0, -1 maps to
0, 31, 62. This is controlled field-identity and numeric-alignment evidence,
not a complete OAMD timeline or an OAMD-to-JOC identity proof.

### Current J1 boundary

Completed for the controlled corpus: normative cursor prefix and X/Y field
identity, ADM-qualified numeric alignment, raw warp location/value, and the
negative result that H0/H1/H2 do not discriminate semantics. Still open:
meaning of reserved raw warp 3, post-warp vendor continuation, trim/timeline/
previous-state semantics, authored-object to OAMD-slot identity, OAMD-to-JOC
binding, object PCM, ObjectScene/render fidelity, and end-to-end acceptance.
The next proposed research line is **J1R7B — Reserved Warp-3 Empirical
Boundary Characterization**; it is documented only and is not executed by this
milestone.

## J1R9 dual-object multi-tone identity-binding result (2026-08-10)

The J1R9 private run
`20260810T104057Z_j1r9-dual-object-multitone-identity_6492301` is frozen by
aggregate SHA-256
`d9611198677caf2f0d6c56aacc4b2fe70843f8fc7a9489546b9658e697045863`.
It uses one qualified four-second dual-object fixture: authenticated 997 Hz
and 2003 Hz sources exchange Front-Left/Front-Right positions. ADM verifies
the two authored-object trajectories. A nonzero-Z trajectory during the
transition is retained as observed; predeclared stable windows before and
after it are the only intervals used for the identity conclusion. R0/R1 raw
EC3 is byte-identical.

The OAMD observation remains bounded at raw warp. Element 1 slot 0 is the
Front-Left comparison tuple and slot 3 Front-Right in both stable windows.
This supports stable spatial slots but does not identify every authored object
with an OAMD slot. The visible FL→FR trajectory is slot 9 and its paired JOC
row has zero stable-window energy. Element 2 stays opaque:
`warp_mode [526,528) = raw 3` is still ETSI-reserved and `[528,536)` remains
raw zero without assigned semantics.

Diagnostic pre-render reconstruction rows supply the new narrow binding
evidence. Row 0 paired with stable FL changes dominant energy 997→2003 Hz;
row 3 paired with stable FR changes 2003→997 Hz. Since ADM independently
proves the authored objects follow the opposite trajectories,
`ONE_ROW_PER_AUTHORED_OBJECT_MODEL_REJECTED` for this FL/FR experiment.
The evidence instead gives scoped support to spatially anchored JOC-row
structure.

This does not establish complete OAMD/JOC semantics, a universal basis,
ObjectScene correctness, renderer or object-PCM fidelity, or a semantic
interpretation of raw warp 3. No production code changed. The next line is an
explicit, testable spatial-basis binding model from the existing corpus before
considering ObjectScene admission.
## J1R12 — Evidence-bounded reconstruction-basis architecture (2026-08-10)

The J1R9/J1R10/J1R11 Logic campaign is formally frozen. J1R9 rejects the
one-row-per-authored-object model, J1R10 leaves the spatial basis
underdetermined, and J1R11 shows that changing Logic application-level track
order did not change the raw EC3 carrier or the observed OAMD slot
trajectories. No independently controllable producer-side variable has been
demonstrated that changes dynamic-slot assignment under fixed authored
identity, trajectory, and multi-object context.

The implementation boundary is therefore explicit and evidence-bounded:

```text
metadata object/state      -> metadata-only ObjectScene
JOC reconstruction output  -> ReconstructionBasis rows
semantic audio binding     -> Unresolved
```

Rows are diagnostic reconstruction-basis rows, not authored-object PCM or
object stems. Structural 15-slot/15-row cardinality and the separate
`RcLfe` base-carrier distinction remain available for diagnostics only. No
row-index, dominant-row, FL/FR, or spatial-observation fallback is allowed.
The strict raw warp 3 reservation and vendor profile behavior are unchanged;
no Logic fixture or new semantic inference was performed.

## J1R13 — Semantic binding evidence contract

J1R12's unresolved boundary is now an auditable contract. Structural
slot/row relations and empirical spatial/context correlations are evidence
classes, not semantic state transitions. A future verified admission must
identify WHO, WHERE, SLOT, ROW/BASIS, audio identity, context, time,
repeatability, negative controls, and cross-state behavior, with allowed
provenance and a falsifier. The validator rejects equal-count/index,
dominant-row, single-fixture, and field-name arguments. The current Logic
campaign is frozen; no new fixture or semantic binding was created in J1R13.

## J1R15 — ReconstructionBasis numerical acceptance

J1R15 closes a narrower numerical milestone without reopening semantic
binding. The nine usable frozen carriers have finite, structurally stable
ReconstructionBasis output with deterministic repeated evidence. The decoder
keeps QMF analysis/synthesis history across AU boundaries, resets only for
sequence/configuration discontinuities, stages state atomically, and emits
rows without hidden padding or startup trimming. `RcLfe` remains base-carried
and separate. Controlled tone projections remain reproducible numerical
signatures only; they are not object identity evidence.

Decision: `RECONSTRUCTION_BASIS_NUMERICAL_ACCEPTANCE_ESTABLISHED` within this
declared numerical/structural scope. `SemanticBindingState::Unresolved`,
authored-object PCM inadmissibility, audio-bound ObjectScene inadmissibility,
and ETSI reserved `warp=3` behavior are unchanged. No new media or fixtures
were created. The private evidence package is under
`OpenJOC-Private/reports/runs/20260810T151025Z_j1r15-reconstruction-basis-acceptance_ef3c43f/`.

## J1R16 — Existing-corpus end-to-end acceptance matrix

The nine qualified frozen carriers all pass the declared input, AU framing,
base numerical, metadata-only scene, and ReconstructionBasis structural
boundaries. J1R14 timeline ordering and J1R15 numerical acceptance regressions
remain intact, and no implementation defect was reproduced. The matrix
deliberately records the two profile boundaries separately: `ETSI_STRICT`
expects the raw-3 reserved rejection, while `DOLBY_VENDOR_COMPAT` remains
partial because the bounded vendor continuation after the reserved warp is
opaque.

Decision: `EXISTING_CORPUS_ACCEPTANCE_PARTIAL`. This is not a semantic-binding
or renderer acceptance. `SemanticBindingState::Unresolved`, diagnostic row
status, authored-object PCM inadmissibility, audio-bound ObjectScene
inadmissibility, and raw-warp behavior are unchanged. No new media or fixture
was created. The highest-value next non-fixture blocker is admissible evidence
for the bounded vendor continuation after raw warp 3, or continued explicit
opacity. Evidence package:
`OpenJOC-Private/reports/runs/20260810T153638Z_j1r16-existing-corpus-acceptance_f845fdd0/`.

## J1R17 — Opaque vendor-continuation preservation contract

The remaining raw-3 boundary is now represented without pretending that the
normative cursor continues through it. For an explicitly selected
DOLBY_VENDOR_COMPAT parse, element 2's complete declared body remains the
lossless source. A non-owning OpaqueVendorContinuation view covers only the
bits after the two-bit raw warp field and before the validated enclosing
element end; it carries exact payload-relative bounds, an exact bit-window
hash, and unresolved provenance. This intentionally excludes payload trailing
bits outside the element body.

The strict parser still stops at warp_mode [526,528) = 3 with
ReservedWarpMode { raw: 3 }. No trim, padding, checksum, coordinate, or
other semantic name is assigned. The opaque view is diagnostic/preservation
evidence only and cannot create an OAMD timeline, semantic object binding,
authored-object PCM, renderer state, or JOC meaning. Nine existing qualified
carriers were rechecked; no new fixture or media was generated.
Private evidence freeze:
`20260810T155539Z_j1r17-opaque-vendor-continuation_f480e05d/j1r17_evidence_freeze.json`.

## J1R18 — Bounded streaming decode and memory admission

The production core now has two explicit retention modes. Capture mode keeps
the legacy complete ObjectScene and diagnostic PCM contract. Streaming mode
uses the same frame parser/reconstruction/history path, delivers each decoded
frame to a sink, validates metadata against bounded scene state, and retains
only counters, object anchors, current codec state, and current-frame data.
The 128-frame logical sequence test reaches a constant high-watermark rather
than accumulating rows or metadata events, and streaming/capture frame outputs
are identical for the synthetic API-level regression.

The result is deliberately narrower than full end-to-end streaming: raw/MP4
input loading and syncframe/AU indexing still use duration-proportional storage,
and WAV/diagnostic export remains an explicit capture path. J1R14 block-major
metadata ordering, J1R15 numerical row boundaries, J1R17 opaque continuation,
and `SemanticBindingState::Unresolved` remain unchanged. No new media or
fixture was generated.

## J1R19 — Incremental input/container streaming and output finalization

The raw elementary-stream boundary now has a Reader-based syncframe framer.
It probes only enough bytes to parse the fixed header, then requests exactly
the declared frame remainder; arbitrary underlying read chunk sizes produce
byte-identical frames. A 128-frame logical sequence shows a constant carry
high-watermark and truncated final frames remain explicit errors.

This is intentionally not called full raw input-to-J1R18 delivery: the
existing `load_eac3` API and CLI still materialize the input bytes and build a
complete AU index because downstream extraction APIs currently consume those
borrowed slices. ISO BMFF stream-copy payload and sample-table/index metadata
remain separate duration-proportional boundaries. `WaveWriter` is now used by
captured scene row/LFE exports, with header finalization errors propagated.
No media or fixture was created.

## J1R20 — Incremental AU consumer / container ownership closure

The raw sequential path now separates an explicit capture/index contract from
an incremental contract. `RawEac3AccessUnitReader<R: Read>` consumes complete
syncframes from `RawEac3FrameReader`, retains one bounded access unit plus one
frame of boundary lookahead, and emits locally indexed bytes to the existing
J1R18 decoder. The explicit CLI `--streaming --internal-base` mode uses this
path; no second decoder or hidden warp rule was added.

On the frozen Center 997 Hz carrier, direct and legacy paths produced
byte-identical base/LFE WAVs, inventories, and all shared per-frame diagnostic
files. The streaming summary matched sample rate, duration, frame count,
object cardinality, metadata-event count, and ReconstructionBasis dimensions.
Synthetic chunk, lookahead, truncation, exact-EOF, and 128-AU high-watermark
tests pass. ISO BMFF and legacy capture/index APIs remain explicit
duration-proportional boundaries. `SemanticBindingState::Unresolved` and ETSI
strict raw warp=3 reservation are unchanged; no new media or fixture was made.

## J1R21 — Seekable ISO BMFF boundary

J1R21 separates seekable media payload ownership from container index
ownership. The new reader retains an explicit O(samples) vector of FFprobe
packet locations, but seeks and reads only the current E-AC-3 packet before
passing its bytes into the existing incremental frame/AU consumer. Four
existing frozen carriers (Center 997, Front Right 997, Rear Center 997, and
Center 2003) produce packet sequences byte-identical to their raw stream-copy
companions. This is evidence for delivery correctness, not a claim that the
sample table is constant-memory.

Generic non-seekable ISO BMFF and fragmented MP4 remain outside the admitted
contract. The decoder, OAMD timeline handling, ReconstructionBasis meaning,
SemanticBindingState, and ETSI strict raw warp=3 behavior were not changed.
The decision is
`SEEKABLE_ISOBMFF_STREAMING_ADMISSION_ESTABLISHED_WITH_INDEXED_METADATA`.

## J1R22 — Avoidable derived sample-index state

The previous seekable reader expanded every FFprobe packet row into an
OpenJOC-owned `Vec<IsoBmffSample>`. J1R22 keeps that constructor only as an
explicit indexed/capture mode and changes ordinary sequential delivery to a
lazy `IsoBmffSampleCursor`: FFprobe packet stdout is read line-by-line, the
current offset/size is used once, and no prior descriptor is retained. The
four-carrier byte-equivalence evidence remains passing.

This is a precise ownership improvement, not a native ISO BMFF parser claim.
FFprobe still owns the underlying native-table interpretation, so stco/co64,
stsc, stsz, and related metadata remain an external duration-proportional
boundary. Semantic binding, ReconstructionBasis meaning, and raw warp=3
strict behavior remain unchanged.

## J1R23 — Normative E-AC-3 coding-tool admission matrix

The coding-tool audit separates syntax parsing, DSP execution, unit validation,
controlled-corpus activation, and state/reset evidence. Existing frozen
diagnostics show block switching, deterministic caller-supplied dither,
exponent reuse, grouped mantissa traversal, and LFE topology. They do not
activate coupling, SPX, AHT, rematrix, or dependent-substream coding-tool
effects. Those paths remain implemented but unvalidated for release purposes;
absence from the corpus is not a pass.

Decision: `EAC3_CODING_TOOL_COVERAGE_PARTIAL`. The next non-fixture blocker is
an integrated, public-syntax-only activation/state-transition evidence task
for the under-exercised tools. No vendor semantics, raw warp-3 interpretation,
or semantic binding change is implied.

## J1R24 — public-syntax activation harness

`crates/openjoc-eac3/tests/coding_tool_activation.rs` is a deliberately small
test-only harness. It activates production API DSP paths with public
ETSI-shaped structures and checks repeatability/finite bounded output. Existing
syncframe and access-unit tests continue to provide parser/state coverage.
Rematrix has a separate small public sum/difference oracle; coupling and SPX
remain invariant/state-only evidence, while AHT and dependent-substream retain
their existing bounded state tests.

The real controlled corpus remains unchanged and does not exercise the target
coding-tool effects. Synthetic activation is not a Dolby-encoder or authored
object claim.

## J1R25 — coupling state and coordinate admission

The coupling boundary is now split into parser/state evidence, independent
coordinate evidence, and final coupled-PCM fidelity. ETSI TS 102 366 V1.4.1
clause 6.4.3 was transcribed into a separate test-only float64 oracle and
compared exhaustively against the public standard-coupling reconstruction API
(1,024 legal code combinations). Existing six-block parser coverage now also
asserts exact coupling-state reuse after the first block. This is a bounded
public-syntax admission result, not evidence that the Logic controlled corpus
activates coupling and not an authored-object or ObjectScene result.

Decision and limitation: `COUPLING_STATE_AND_COORDINATE_ADMISSION_ESTABLISHED`
within the exercised parser/API scope; `FULL_COUPLED_PCM_FIDELITY_NOT_ESTABLISHED`.
`SemanticBindingState::Unresolved` and strict raw warp-3 reservation are
unchanged.

## J1R26 — SPX state and reconstruction admission

The SPX production path is now compared with an independent Annex E.2.6
translation/coordinate oracle across 1,024 legal public code combinations.
The oracle intentionally isolates one legal band with zero noise and no
attenuation; existing tests retain the separate noise-blend and notch checks.
Parser syntax, sub-band derivation, band grouping, finite output, and invalid
input behavior are covered without creating media.

This is `SPX_STATE_ADMISSION_ESTABLISHED_NUMERICAL_MAPPING_PARTIAL`: the
cross-block `spxcoe=0` reuse/reset case and full real-stream SPX PCM fidelity
are not yet independently established. The frozen Logic corpus still has
SPX off, and no authored-object or semantic-binding conclusion follows.

## J1R27 — SPX reuse, carry, and reset admission

J1R27 closes a bounded parser-level state sequence in synthetic public syntax:
explicit coordinates A, two consecutive `spxcoe=0` reuses, explicit B with a
changed copy-region code, reuse B, disable, fresh re-enable, and independent
frame reset. Reused `SpectralExtensionInformation` values match the last
explicit state exactly. A 256-repeat long sequence is byte- and value-stable
at the API boundary, with no historical state collection.

The result is `SPX_STATE_REUSE_AND_RESET_ADMISSION_ESTABLISHED` for the
exercised mono public-syntax scope, combined with J1R26 as
`SPX_PUBLIC_SYNTAX_STATE_AND_NUMERICAL_ADMISSION_ESTABLISHED`. Channel-local
participation, parser-specific truncation, real controlled-corpus SPX
activation, and full real-stream SPX PCM fidelity remain unestablished.

## J1R28 — SPX multi-channel participation and parser errors

J1R28 drives a bounded stereo six-block public-syntax frame through distinct
channel states A/B, exact reuse, A-only and B-only participation, fresh state
after a channel leaves and returns, one-channel replacement while the other
reuses, and a final mixed reuse/replacement block. The complete parsed
`SpectralExtensionInformation` is compared at every block. Separate all-off
and initial A-only frames establish the activation controls.

Exact declared-frame truncations at the second participation flag and each
per-channel coordinate boundary return `BitError::EndOfInput`. A failed parse
cannot poison a fresh decode; direct, pre-parsed, and diagnostic paths agree,
and 256 complete repetitions remain exact. Invalid coordinate dimensions are
rejected without indexing or fallback. No production defect was exposed.

The narrow decision is
`SPX_MULTICHANNEL_STATE_ADMISSION_ESTABLISHED_ERROR_PATH_PARTIAL`: parser-level
participation, isolation, central truncation, and fresh-call reset are covered,
but a dependent-substream/configuration transition is not representable as a
persistent SPX parser-state transition in the current public API. The Logic
corpus remains SPX-off and full real-stream SPX PCM fidelity remains open.

## J1R29 — AHT production reconstruction and numerical admission

The audit found a complete production reconstruction path rather than a
pointer-only stub: high-efficiency BAP drives VQ/GAQ payload decoding, the
six-point inverse AHT DCT materializes one coefficient per audio block, and
ordinary exponent shifting and downstream synthesis consume those values.
The frame-local `AhtElementState` owns exactly the six-block lifetime; later
blocks reuse the reconstructed array without rereading the first-block AHT
payload.

Independent tests now cover all 64 high-efficiency pointer addresses, all 956
VQ entries by PDF-derived table digests, 99,302 GAQ codewords, all gain-word
symbols, and 54 IDCT outputs. GAQ comparisons are exact; the largest observed
IDCT absolute error is `2.220446049250313e-16` under the frozen `1e-12`
tolerance. A parser-level bin independently follows Table E.3.4 → E.2.4.5
IDCT → exponent shift across blocks 0–5. A matched disabled case confirms that
the AHT flag is not merely parsed and ignored.

No production defect was found. The result is deliberately limited to
synthetic public syntax and reconstructed values. Existing real Logic
carriers remain AHT-off, so full real-stream AHT PCM fidelity remains open;
no object identity or binding conclusion follows.

## J1R30 — dependent-substream assembly observations

TS 102 366 Annex E establishes parent-first ordering and Table E.1.4 channel
locations; matching locations from a dependent substream replace the
independent location rather than forming an invalid duplicate. TS 103 420 E.3
narrows JOC to one I0 plus optional D0, six blocks, and a Table 47 output
configuration. The test oracle therefore keeps two distinct questions:
complete low-level `chanmap` representation and the smaller admitted JOC
topology set.

All 65,536 `chanmap` words match an independent table transcription. Sentinel
PCM proves replacement and supplements reach the declared logical channels,
including rear and height alternatives. The height carrier locations retain
Table E.1.4's `Vhl/Vhr` labels; Table 47 separately names the 5.X+2 output
pair `Tfl/Tfr`. The audit found that TDAC history had
been keyed only by substream role, so a D0 chanmap change could carry overlap
history from an old logical channel into a new one. A configuration signature
now resets only that substream; an independent I0 control proves its history
continues. A separate CLI defect rejected valid seven-channel assembled PCM
and could not report its labels; the capture boundary now retains topology and
rejects mid-stream shape changes before mutation.

This is a codec-channel result only. A substream ID or channel location is not
an Atmos object identity. The controlled Logic corpus remains D0-off, so the
result cannot establish real-encoder prevalence or full real-stream fidelity.
## J1R31 — Capability/evidence separation and CLI observability

The 0.x support contract now uses a bounded status vocabulary and records the
strongest evidence class separately. In particular, coupling, SPX, AHT,
rematrix, and dependent-substream work derived from public ETSI syntax remains
scoped even when its production path is admitted; absence of activation in the
controlled Logic corpus and absence of full real-stream fidelity remain visible.

Observed vendor behavior is not normative evidence. `ETSI_STRICT` continues to
reject the reserved raw warp value 3. `DOLBY_VENDOR_COMPAT` remains an explicit,
partial structural/deviation-preservation profile with opaque continuation and
unresolved interpretation. Neither CLI help nor an error hint changes this
scientific boundary.

Metadata object identity, diagnostic reconstruction-row identity, and authored
object identity remain three separate domains. The public CLI names only the
first two. A metadata-only ObjectScene is admissible; authored-object PCM and an
audio-bound ObjectScene are not.

## J1R32 — clean source is not a clean machine

The packaging experiment deliberately distinguishes four claims. A committed
source archive is closed when it builds without the developer worktree,
untracked references, or `.git`. An isolated install is usable when its binary
runs outside that source tree. Offline build means the locked registry
dependencies are already cached, not that a fresh machine needs no dependency
download. Binary/archive reproducibility is claimed only for the same source,
host, target, and Rust toolchain that were measured.

Checking generated normative tables into the consuming crates does not weaken
their provenance: the generator and both official hashes remain the trust
boundary, while a regression compares committed output to freshly imported
output whenever the authorized companion archive is available. It removes a
developer-machine build dependency without embedding the external attachment.

The package audit also treats test-only absolute private paths as leaks even
when they cannot affect production. Replacing that path with an explicit opt-in
environment variable keeps the scientific fixture private and the public
package location-independent.

## J1R33 — release verification is not semantic verification

The local bundle closes a distribution-mechanics question only: can a user
account for every byte, verify exact inventory and digests, and run the admitted
CLI help surface without the developer repository? It does not strengthen the
meaning of ReconstructionBasis rows, bind audio to authored objects, interpret
reserved warp 3, or convert opaque vendor continuation into semantics.

The manifest's source commit is a declaration supported by same-host
reproducible-build evidence, not a cryptographic signature. The automatic
linker ad-hoc Mach-O signature is recorded separately from absent Developer-ID
signing and notarization. Artifact reproducibility is evaluated independently
from codec fidelity and only under the measured host/toolchain inputs.

## J3R12 — human-assisted six-run N3 producer batch

The replacement queue was executed as exactly six Logic Pro 12.3 Dolby Digital
Plus Atmos Music/768 kbps/48 kHz exports: static Front Left, static Front Right,
and dual-object swap, each as a two-run pair. Human assistance was limited to a
single mechanical Save click after controller verification; all six outputs
were observed once in the frozen discovery realm with consumed nonces and
`RUN_VERIFIED` controller states.

The stream-copied raw EC-3 carrier was byte-identical within all three pairs
(129/129 AU for each static pair and 126/126 AU for the dual-object pair).
MP4 container hashes differed within pairs and were not used as the
determinism boundary. The bounded structural audit continued to report
`ETSI_STRICT` failure and explicit `DOLBY_VENDOR_COMPAT`
  `accepted_with_deviation`; no warp rule, semantic binding, JOC/ObjectScene
  interpretation, authored-object PCM, or renderer claim was added.

## J3R13 — exact-condition N3 context analysis

J3R13 uses the three J3R12 byte-identical raw-EC3 producer pairs as an exact
full-complex null envelope and reproduces the frozen J2R7 C1/C2 target
definitions with the fixed 48 kHz, 997/2003 Hz sine/cosine estimator. Both
contrasts exceed that envelope. C1 Base/RB/joint residuals are
0.0034848789188314825 / 0.0031748106398913228 / 0.2974217716899793; C2 is
0.0035417176701007364 / 0.9999992742845898 / 0.857715331578793.

The result is scoped context-dependence admission only. It does not establish
object, slot, authored-PCM, renderer, or universal frequency semantics.
`SemanticBindingState` remains `Unresolved`, RcLfe remains separate, C3 is
not analyzed, and warp raw 3 remains the ETSI strict reserved value.

## J3R14 — context dependence requires support-creating coordinate mixing

J3R14 froze a support threshold from the exact N3 producer null and the prior
N0/N1 numerical envelope before evaluating the C1/C2 target vectors. Both dual
contexts contain ReconstructionBasis coordinates that are below that floor in
their corresponding static controls. A global gauge, support-preserving
diagonal operator, and bounded common-additive cross-prediction are rejected
within the tested corpus. No common row permutation was admitted, and the
dual vectors do not match or lie within the frozen static-position atlas at the
exact producer envelope.

The result is deliberately narrow:
`J3R14_CONTEXT_DEPENDENCE_REQUIRES_SUPPORT_CREATING_CROSS_COORDINATE_MIXING`.
It describes a coordinate-level ReconstructionBasis observation, not an
authored-object, slot, renderer, or Dolby semantic binding. `SemanticBindingState`
remains `Unresolved`; no new media or Logic fixture was used; and warp raw 3
remains ETSI `ReservedWarpMode`.

## J3R15 — companion intervention was not propagated by the producer

J3R15 attempted one fixed-topology, within-carrier intervention: a continuous
997 Hz Front Right target with a Front Left companion whose frozen PCM was
silence, then 2003 Hz, then silence. The source and Logic-embedded PCM are
sample-identical, and save/reopen plus ADM independently establish the fixed
positions. Human assistance was mechanical value entry only.

The qualified ADM object tracks are nevertheless digital zero throughout, and
the DD+ carrier does not show the required 2003 Hz OFF/ON/OFF signal pattern.
The primary result is therefore
`J3R15_EXPERIMENT_EXECUTION_OR_LINEAGE_INADMISSIBLE`, not a no-effect result.
Diagnostic row differences cannot be assigned to companion-signal causality.
J3R14's context-conditioned class remains underdetermined,
`SemanticBindingState` remains `Unresolved`, and warp raw 3 remains ETSI
`ReservedWarpMode`.

## J4R1 — ADM object-channel PCM propagation gate

J4R1 admitted a reproducible source-to-ADM authoring contract. Deterministic
997 Hz and zero/2003 Hz/zero mono source PCM payloads were recovered
byte-for-byte first from bounded Logic track bounces and then from their
AXML/CHNA-linked ADM object channels at sample origin. The eight-second ADM
contains the declared six-second source interval followed by a two-second zero
producer tail.

The prior J3R15 failure was localized to
`REGION_NOT_BOUND_TO_EXPECTED_TRACK`: project lineage records show the source
media changing from active audio to unused media before the all-zero
object-channel export, although the exact GUI action remains unresolved. The
old J3R15 carrier remains inadmissible. No DD+, JOC decode, renderer,
object/row binding, or vendor-semantic work was performed, and
`SemanticBindingState::Unresolved` remains unchanged.

See [J4R1_ADM_OBJECT_PCM_PROPAGATION.md](J4R1_ADM_OBJECT_PCM_PROPAGATION.md)
and [its machine-readable record](J4R1_ADM_OBJECT_PCM_PROPAGATION.json).

## J4R2 — source-verified companion-signal intervention

J4R2 generated exactly one source-verified, fixed-topology DD+ Atmos carrier.
Its companion 2003 Hz ON interval is strongly visible, but both authored-silent
OFF windows exceed the absolute observability floor frozen before export. The
predeclared propagation gate therefore fails as
`J4R2_COMPANION_SIGNAL_PROPAGATION_FAILED`; this is neither a no-effect result
nor an admission of signal-dependent cross-coordinate mixing.

The carrier is structurally valid across 251 access units and its payload-11
body and observed object/element topology are invariant. Warp raw 3 remains
ETSI `ReservedWarpMode`; no vendor interpretation was added. Downstream target
997 Hz ON/OFF inference is not admitted, ReconstructionBasis rows remain
coordinate labels, and `SemanticBindingState::Unresolved` is unchanged.

See [J4R2_COMPANION_SIGNAL_INTERVENTION.md](J4R2_COMPANION_SIGNAL_INTERVENTION.md)
and [its machine-readable record](J4R2_COMPANION_SIGNAL_INTERVENTION.json).

## J4R3 — metric-compatible propagation floor and OFF-state history

J4R3 independently admits companion 2003 Hz carrier propagation without
changing J4R2's correctly failed preregistered result. Exact source OFF nulls,
an independently checked two-frequency estimator, two OFF calibration windows,
two holdouts, and symmetric neighboring-frequency controls establish a
metric-compatible joint floor of `1.6745250526579124e-4`. The ON joint norm is
`0.2788167423372097`, about 1,832 times the largest OFF observation. The OFF
nuisance is classified as `DETERMINISTIC_CODEC_SPECTRAL_FLOOR`, with bounded
black-box compatible-base corroboration.

The frozen compatible-N1 classifier supports OFF-state reversibility because
all cross-state residuals fall below 110% of the larger within-state residual.
Within-B variation is nevertheless about 0.8%, roughly 504 times the within-A
joint residual; the supported result is therefore explicitly scoped to that
broad compatible envelope. J4R3 closes as
`J4R3_COMPANION_PROPAGATION_CONFIRMED_AND_OFF_REVERSIBILITY_SUPPORTED`. No
target causal ON/OFF result, object/row identity, vendor warp interpretation,
or renderer claim was added; `SemanticBindingState::Unresolved` is unchanged.

See [J4R3_PROPAGATION_FLOOR_AND_HISTORY.md](J4R3_PROPAGATION_FLOOR_AND_HISTORY.md)
and [its machine-readable record](J4R3_PROPAGATION_FLOOR_AND_HISTORY.json).

## J4R4 — fixed-topology target-997 causal decomposition

J4R4 applies the J4R3 compatible-N1 calibration to four independently frozen
target-997 comparisons on the source-verified J4R2 carrier. Companion 2003 Hz
propagation, fixed payload-11 topology, target observability, alignment, and
same-HEAD gates pass. Every Base, ReconstructionBasis, and labeled Joint
ON-versus-OFF projective residual remains within its frozen envelope, and all
four target RB support masks remain `row_000..row_008`. The primary result is
therefore `J4R4_COMPANION_PCM_EFFECT_NOT_OBSERVED_WITH_FIXED_TOPOLOGY`, with
`NO_RB_SIGNAL_EFFECT_OBSERVED`; it does not prove physical independence or
exclude effects below the broad compatible-N1 sensitivity.

An older static Front-Right carrier has only `row_000` target support, whereas
both dual-topology companion-silent windows have `row_000..row_008`. This
supports the secondary descriptive classification `STRUCTURAL_CONTEXT_ONLY`
and the bounded model label
`OBJECT_POPULATION_OR_STRUCTURAL_CONTEXT_ESTABLISHES_RB_SUPPORT`. It is not a
strict object-population or shell-only causal claim: the static and J4R2 target
sources are not sample-identical and differ in envelope, amplitude, and
duration. `SemanticBindingState::Unresolved`, RcLfe separation, and the ETSI
reserved status of warp raw 3 remain unchanged.

See [J4R4_TARGET997_CAUSAL_DECOMPOSITION.md](J4R4_TARGET997_CAUSAL_DECOMPOSITION.md)
and [its machine-readable record](J4R4_TARGET997_CAUSAL_DECOMPOSITION.json).

## J4R5 — source-matched silent-companion structural intervention

J4R5 isolates the silent authored-shell question with one frozen target source,
a cloned target-track lineage, and four fresh-process producer exports. The A
condition contains the target Object track only; the B condition adds one
Object-routed six-second exact-zero companion region. Immediate track bounces
prove the target source exact in all four runs and the companion exact zero in
both B runs.

All four stream-copied raw EC-3 carriers are byte-identical. The observed OAMD
structure, Base/RB/Joint target-997 coefficient vectors, RcLfe, and RB support
mask `row_000..row_008` are likewise unchanged in every one of twelve
predeclared A×B×window comparisons. J4R5 therefore closes as
`J4R5_STRUCTURAL_EFFECT_NOT_REPRODUCED_WITH_SOURCE_MATCHED_CONTROL`. The older
J4R4 S0→S1 association is downgraded as source/project-lineage confounded; it
is not retained as a silent-shell causal result. `SemanticBindingState::Unresolved`
and the ETSI-reserved status of warp raw 3 remain unchanged.

See [J4R5_SILENT_COMPANION_STRUCTURAL_INTERVENTION.md](J4R5_SILENT_COMPANION_STRUCTURAL_INTERVENTION.md)
and [its machine-readable record](J4R5_SILENT_COMPANION_STRUCTURAL_INTERVENTION.json).
