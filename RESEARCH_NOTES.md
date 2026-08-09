# OpenJOC Research Notes

## Clean-room boundary

OpenJOC is an independent implementation based only on the normative ETSI
documents and their official companion data. Proprietary Dolby source and vendor
decoder implementations are excluded as implementation inputs. Informative
research may only cross-check architecture after normative behavior is derived.

## Normative sources verified locally

- ETSI TS 103 420 V1.2.1 (2018-10), `references/etsi/ts_103420v010201p.pdf`.
  Local SHA-256: `e532bfc4f8be4a97c7c9cdd9f6bcc40634ecf8ef93a1dc490fcb15c162fec2aa`.
- ETSI TS 102 366 V1.4.1 (2017-09), `references/etsi/ts_102366v010401p.pdf`.
  Local SHA-256: `0229e151dfd9f8cec427f234798cac679a66fdec096feecc4d5ce455bb06c2796`.
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
`0229e151dfd9f8cec427f234798cac679a66fdec096feecc4d5ce455bb06c2796`) the
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
