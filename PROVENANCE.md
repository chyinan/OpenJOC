# OpenJOC Implementation Provenance

OpenJOC is a clean-room implementation. Production behavior is derived only
from the public normative specifications listed below, the official ETSI
companion archive, and public mathematical/DSP literature where explicitly
recorded. No existing JOC decoder source code is an implementation reference.

## Forbidden-source policy

Cavern source code, forks, mirrors, and all other existing JOC decoders are
excluded. Decompiled or disassembled Dolby software and historical proprietary
Dolby, MediaTek, or Broadcom implementation code are excluded. Cavern may be
used only later as a separately executed black-box comparator after normative
implementation is complete.

## Component record

### Checked bit reader

- Normative use: common syntax-reading prerequisite for TS 103 420 clauses 5
  and 6, and TS 102 366 Annexes E and H.
- Reference data: none.
- Design rationale: non-owning MSB-first reader with checked length/cursor
  arithmetic, atomic failure on truncated fields, and structured errors.
- Validation: example and property tests cover cross-byte order, alignment,
  invalid widths, truncation without consumption, boolean-oracle equivalence,
  and consumption invariants.

### ETSI companion importer

- Normative source: TS 103 420 Annex A.1 and clause 7.4.
- Official reference data: `ts_103420v010201p0.zip`, expected SHA-256
  `a79cf108c4529b7d9ca9525c871183a70b1732ed6df03a3d85b2f31be46eeced`;
  contained `ts_103420_tables.c`, expected SHA-256
  `4db8ae83e3c2e9269e88365be92a1a3ed6a9e6ee3851afac8ca03902723b1fcd`.
- Design rationale: reject before parsing unless both hashes match; require the
  single expected archive member; parse only the seven named declarations;
  validate all element counts; generate local Rust with hash provenance.
- Validation: official archive import/count test, modified-byte rejection, all
  generated declaration names/provenance test, and CLI generation test.

### JOC Huffman decoding

- Normative source: TS 103 420 clause 6.6.3 pseudocode 4 and Annex A.1.
- Official reference data: six trees imported from the verified companion file.
- Design rationale: start at node zero, consume MSB-first, follow positive node
  indices, and map a non-positive leaf to `-node-1` exactly as pseudocode 4.
  Bounds and cycle checks turn malformed trees/input into errors.
- Validation: DFS enumerates every leaf of every official tree; every path is
  decoded to the normative symbol; paths are unique and prefix-free; every
  truncated path and an invalid node reference return structured errors.

### JOC syntax and reconstruction matrix

- Normative source: TS 103 420 clauses 6.2, 6.3, 6.5, and 6.6.2–6.6.6;
  tables 47–54; pseudocode 2–7.
- Official reference data: the six verified Annex A.1 Huffman trees.
- Design rationale: retain header/info fields and every codeword for dumps;
  reject reserved header values; keep sparse and full differential decoding as
  separate pure functions; use the exact rational dequantizer; encode table 54
  as its grouped subband widths; return interpolation state explicitly; clear
  object output buffers before complex matrix accumulation.
- Validation: full and sparse parser branches (5/7 channels, 96/192 steps,
  smooth/steep, one/two data points), malformed header/padding cases, distinct
  sparse/full differential examples, all 288 valid dequantizer inputs, all 512
  table 54 inputs, interpolation boundary/state tests, and complex identity,
  mixing, zero, and dimension tests.

### Stateful JOC frame pipeline

- Normative source: TS 103 420 clauses 6.3.3.3, 6.6.1–6.6.6.
- Official reference data: retained Annex A codewords already decoded by the
  parser.
- Design rationale: perform parser → distinct differential path → dequantizer →
  table 54 mapping/interpolation → complex matrix multiply as one atomic frame
  transaction. Previous matrices are initialized/reset to zero, committed only
  after successful reconstruction, and reset on sequence zero, counter gaps, or
  channel/object configuration changes. Every intermediate matrix remains
  available for debug dumps.
- Validation: a present frame establishes matrix state; a consecutive absent
  frame reuses it; sequence zero resets it; a counter discontinuity starts
  smooth interpolation from zero; quantized and interpolated stages are checked.

### Per-object inverse QMF

- Normative source: TS 103 420 clauses 6.6.6, 7.3, and 7.4, pseudocode 13–17.
- Official reference data: the verified `prot64[640]` table imported from the
  official ETSI companion archive.
- Design rationale: retain one clause 7.3 synthesis history per object, emit 64
  PCM samples for every reconstructed object-QMF timeslot, and reset synthesis
  histories on the same sequence/configuration discontinuities as matrix state.
  Synthesis runs against cloned histories so decoder state is committed only
  after the complete frame succeeds.
- Validation: integrated PCM is sample-exact against direct calls to the
  normative reference synthesizer across consecutive frames, and sequence zero
  clears residual synthesis history.

### PCM-to-object reconstruction boundary

- Normative source: TS 103 420 clauses 6.4 and 7.2–7.4.
- Official reference data: the verified `prot64[640]` table used by the direct
  analysis and synthesis implementations.
- Design rationale: accept channel-major downmix PCM in exact 64-sample QMF
  blocks, retain one analysis history per input channel, and feed the resulting
  complex timeslots through the stateful JOC and inverse-QMF pipeline. Analysis
  histories are cloned and committed only after complete frame reconstruction.
- Validation: integrated output is sample-exact with separately staged normative
  analysis plus JOC decoding; partial blocks are rejected without advancing state.

### OAMD bounded variable-length fields

- Normative source: TS 103 420 clause 5.5.1; TS 102 366 clauses H.2.1.2.1 and
  H.2.2.2.1 provide the underlying `variable_bits` definition and group-offset
  semantics.
- Official reference data: none; this component is defined directly by normative
  pseudocode.
- Design rationale: decode MSB-first groups using checked `u64` arithmetic and
  enforce the caller-supplied group count before any additional group can drive
  allocation or indexing.
- Validation: one- and multi-group values, exact maximum-group stopping,
  truncation, invalid bounds, and arithmetic overflow are tested.

TS 103 420 clause 5.5.1 initializes `num_group` to one but its printed pseudocode
does not increment it immediately after the first continuation. Read literally,
that loop can consume one more group than `max_num_groups`. The parameter name,
the bounded-purpose variant, and TS 102 366 H.2.2.2.1's definition that each
continuation introduces exactly one additional group support treating
`max_num_groups` as the total permitted group count. OpenJOC enforces that bound;
the explicit maximum-group test records this interpretation for conformance.

### OAMD content-description prefix

- Normative source: TS 103 420 clauses 5.5.2, 5.5.3, and 5.6.0; bed channel
  masks are retained according to clauses 5.6.1.1.3–5.6.1.1.6.
- Official reference data: none; fields are decoded directly from normative
  syntax and semantics.
- Design rationale: expose an explicitly named prefix parser until all
  `oa_element` bodies are implemented. Preserve bed masks losslessly, derive
  all `+1`/`+2` counts exactly, reject reserved ISF indices, and consume
  reserved-program bytes using their declared bounded size.
- Validation: dynamic-only/LFE, extended syntax/object/element counts, mixed
  ISF/dynamic content, standard bed assignment, and reserved ISF tests.

### OAMD metadata timing

- Normative source: TS 103 420 clauses 5.3, 5.5.6–5.5.7, and 5.6.2,
  tables 22–25.
- Design rationale: retain every update block, derive `start_sample` as
  `sample_offset + 32 * block_offset_factor`, and decode all fixed, indexed,
  and explicit ramp-duration forms without collapsing multiple updates.
- Validation: one four-update vector covers table/offset arithmetic and every
  ramp coding form; reserved and malformed coverage remains part of the next
  stateful timing slice.

Stateful timing follows clause 5.3.2's requirement to add 1,536 samples for
each processed codec frame. The implementation commits that increment only
after complete timing syntax succeeds, exposes the clause 4.4 `frame_offset`,
and provides an explicit discontinuity reset. Consecutive, rejected, and reset
frame behavior is covered by integration tests.

### OAMD basic object properties

- Normative source: TS 103 420 clauses 5.5.10, 5.6.1.3, and 5.6.1.4,
  tables 18 and 19.
- Design rationale: represent negative-infinity gain as an enum rather than a
  floating sentinel, preserve integer-dB values exactly, and make the table 18
  previous-object reuse dependency explicit at the function boundary.
- Validation: exhaustive 64-code explicit gain coverage, all 32 priority codes,
  default gain/priority, negative infinity, reuse, and missing-field errors.

### 64-band complex QMF (in progress)

- Normative source: TS 103 420 clauses 7.2, 7.3, and 7.4, pseudocode 8–17.
- Official reference data: `prot64[640]` from the verified companion file.
- Design rationale: direct f64 equations first: 640-sample analysis state,
  1,280-sample synthesis state, direct complex modulation, and exact state
  slices/window folds from the normative pseudocode. No FFT, substitute window,
  phase adjustment, or inferred normalization is used.
- Validation: direct roundtrip tests measure delay and gain from an impulse and
  evaluate DC, 1 kHz, boundary-adjacent tones, and deterministic white noise.
  The deterministic metrics are regression-checked rather than compared to an
  invented perfect-reconstruction threshold absent from clause 7.

## Ambiguities and open normative questions

Clause 7 specifies the complete transform equations but no analysis/synthesis
error or unity-gain threshold. The literal reference equations with the official
`prot64` data produce a 514-sample peak delay and signal-dependent gain/error.
These results are retained and reported rather than normalized or modified.
Conformance must ultimately be cross-checked with a legal normative test vector.

Clause 6.6.5 pseudocode 6 indexes steep data-point coefficients with `sb`, even
though `joc_mix_mtx_dq` is defined over parameter bands and the same pseudocode
requires `sb_to_pb(sb)` for smooth interpolation and state update. Direct `sb`
indexing is undefined for subbands above the selected band count. OpenJOC uses
`sb_to_pb(sb)` for steep data points as required by the declared matrix shape,
clause 6.5, and the final state assignment. A legal conformance vector remains
the external confirmation gate for this textual inconsistency.

Clause 6.3.3.4 defines `b_joc_obj_present` as presence of side information, but
clause 6.6 does not separately state the absent-object interpolation operation.
OpenJOC retains the previous matrix when side information is absent, consistent
with the flag's stated meaning and the required cross-frame matrix state. On a
first frame or detected splice that retained matrix is the normative all-zero
initial state. This interpretation remains subject to legal-vector conformance.

No ambiguity has been resolved outside the normative sources. New ambiguities must
be added here with the relevant clause, competing readings, selected derivation,
and a test or explicit TODO before implementation proceeds.
