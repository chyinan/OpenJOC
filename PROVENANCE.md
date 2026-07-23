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

### 64-band complex QMF (in progress)

- Normative source: TS 103 420 clauses 7.2, 7.3, and 7.4, pseudocode 8–17.
- Official reference data: `prot64[640]` from the verified companion file.
- Design rationale: direct f64 equations first: 640-sample analysis state,
  1,280-sample synthesis state, direct complex modulation, and exact state
  slices/window folds from the normative pseudocode. No FFT, substitute window,
  phase adjustment, or inferred normalization is used.
- Validation: direct roundtrip tests measure delay and gain from an impulse and
  evaluate DC, 1 kHz, boundary-adjacent tones, and deterministic white noise.
  Numerical thresholds remain unverified until the current RED/GREEN cycle is
  complete.

## Ambiguities and open normative questions

None have yet been resolved outside the normative sources. New ambiguities must
be added here with the relevant clause, competing readings, selected derivation,
and a test or explicit TODO before implementation proceeds.
