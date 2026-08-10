# OpenJOC Research Notes

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
