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

## TS 102 366 page 58 rendering ambiguity

The clause 6.2.2.3 `logadd(a, b)` pseudocode contains one layout-sensitive
operator that Poppler 26.02.0 does not render. At 300 DPI the expression appears
as `c = a [blank] b`; layout-preserving text extraction yields control character
`0x01` at the same location. Poppler reports missing display fonts `Symbol` and
`ArialUnicode`, including after a workspace-local Fontconfig mapping to the
installed Windows fonts. OpenJOC does not infer this operator from surrounding
prose. The `logadd` step remains unimplemented until the glyph can be visually
recovered from the authorized specification or corroborated by an authorized
ETSI artifact/test vector. Formula work that does not depend on this glyph may
continue from separately inspected pages.
