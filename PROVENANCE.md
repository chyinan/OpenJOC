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

- Normative source: TS 103 420 clauses 5.5.2, 5.5.3, 5.6.0, 5.6.1.1.3–6,
  and 5.6.4.8; Tables 11b, 12, and 13.
- Official reference data: none; fields are decoded directly from normative
  syntax and semantics.
- Design rationale: expose an explicitly named prefix parser until all
  `oa_element` bodies are implemented. Preserve bed masks losslessly, derive
  all `+1`/`+2` counts exactly, reject reserved ISF indices, and consume
  reserved-program bytes using their declared bounded size. Expand the masks
  and ISF index into the normative bed → MULZ ISF → dynamic object order from
  one shared source used by both parsing and the decoder interface. Tables 11b,
  12, and 13 were visually verified on specification pages 36–38 using
  lossless 300 DPI Poppler 26.02.0 renders.
- Validation: dynamic-only/LFE, extended syntax/object/element counts, mixed
  ISF/dynamic content, exhaustive full standard/nonstandard bed label order,
  complete mixed bed/ISF/dynamic anchor order, standard bed assignment, and
  reserved ISF tests.

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

### OAMD size and zone constraints

- Normative source: TS 103 420 clauses 5.2.2, 5.2.6, 5.5.11, 5.6.1.2,
  and 5.6.1.6, tables 17, 20, and 21.
- Design rationale: retain size as three normalized components, implement the
  three normative size modes exactly, and return all six ordered zone
  constraints without collapsing horizontal and elevation semantics.
- Validation: zero/uniform/independent size boundaries, reserved size mode,
  all six valid horizontal zone indices, both elevation states, and both
  reserved zone indices.

### OAMD object position and spatial factors

- Normative source: TS 103 420 clauses 4.2.1, 4.2.2, 5.2.1.2,
  5.2.1.3, 5.6.1.1.7 through
  5.6.1.1.20, tables 14 through 16, and clauses 5.6.6.4.3 through
  5.6.6.4.5, tables 44 through 46.
- Official reference data: none; coordinates and factors are defined directly
  by the normative equations and tables. The layout-sensitive equations on
  specification pages 14, 15, 19, 20, 38 through 41 and extension tables on
  page 54 were also visually verified from lossless 300 DPI PNG renders
  produced by Poppler 26.02.0; text extraction was not used to infer reference
  screen geometry, matrix structure, exponents, ray projection, fraction, or
  min/max grouping.
- Design rationale: retain the previous standard-precision coordinate
  codewords explicitly for differential updates; decode three-bit deltas as
  two's-complement values; add extended-precision signed fifth-steps exactly;
  and apply only the clamps stated by the coordinate equations. Outside-room
  positions are projected from `(0.5, 0.5, 0)` through the first room-boundary
  intersection. Infinite positions retain that finite intersection as their
  ray representation, avoiding undefined floating-point infinity-times-zero
  components. The normative equation does not define a ray for an object
  exactly at the room centre with distance present, so that case is rejected
  explicitly rather than inferred. Screen interpolation retains the two
  normative diagonal transforms explicitly: screen-factor blending precedes
  depth blending by `y` raised to `depth_factor`, using caller-supplied screen
  bottom-left position, width, and height. Validate every raw value at its
  bit-width boundary before arithmetic.
- Validation: both absolute Z signs, all four extended-precision indices, X/Y
  upper clamping, exhaustive three-bit signed deltas, differential lower and
  upper coordinate clamping, invalid field widths, all 16 distance factors,
  finite and infinite room projection, undefined centre rays, invalid finite
  factors, integrated render-info projection, exact screen/room endpoints and
  a non-trivial screen/depth matrix evaluation, all eight screen factors, and
  all four depth factors are tested.

### OAMD object element and update/reuse semantics

- Normative source: TS 103 420 clauses 5.5.5 through 5.5.11 and 5.6.4.6
  through 5.6.4.14, tables 28 through 31.
- Official reference data: none; the syntax, default values, property masks,
  and reuse actions are specified directly by the normative pseudocode and
  tables.
- Design rationale: parse in the standard's object-major/block-minor order;
  resolve default, full-update, full-reuse, and mixed states into complete
  values; keep the standard-precision position codewords needed by subsequent
  differential updates; and make previous-object gain lookup specific to the
  same metadata block. Bed/ISF and inactive render defaults are applied before
  exposing updates. Unknown additional table data is retained within its
  declared byte bound.
- Validation: full dynamic updates, two-block mixed and full reuse, inactive
  defaults, bed/ISF render defaults, previous-object gain, exact bounded
  additional-data retention, truncation, reserved sample-offset coding, and
  nonzero reserved object-element bits are tested.

### OAMD top-level payload and bounded element dispatch

- Normative source: TS 103 420 clauses 5.5.2 through 5.5.5 and 5.6.4.2
  through 5.6.4.5, tables 26 and 27; object ordering follows clause 5.6.4.8.
- Official reference data: none; sizes and dispatch identifiers are defined by
  the normative syntax and tables.
- Design rationale: compose prefix and body parsers on one MSB-first reader;
  restrict each element to exactly `oa_element_size` bytes before reading its
  conditional alternate ID and discard flag; require zero remainder for known
  decoded elements; and retain genuinely unknown bodies as length-bearing bit
  strings. ID 2 dispatches to the trim parser when its externally configured
  cardinality is available. ID 5 binds to a preceding object element, decodes
  its matching object/block grid, and applies high-precision position updates
  without escaping the declared element window.
- Validation: complete unaligned object payload, declared-window truncation,
  nonzero known-element padding, unknown-bit retention, discard intent,
  reserved alternate object data, mixed bed/ISF/dynamic ordering, program/count
  mismatch, dynamic-plus-LFE ordering, and unfinished-known-ID rejection are
  tested.

### OAMD trim element

- Normative source: TS 103 420 clauses 5.5.12 and 5.6.5.1 through
  5.6.5.12, tables 32 through 39.
- Official reference data: none. Specification pages 49 through 52 were
  rendered losslessly at 300 DPI with Poppler 26.02.0 and visually inspected.
  This verified the balance fractions, numerator grouping, sign tables,
  trim-value tables, and reserved code ranges without relying on damaged
  mathematical text extraction.
- Design rationale: decode global/default/disabled/custom modes into explicit
  types, retain each custom trim configuration independently, resolve absent
  per-object disable flags to false, and reject every reserved mode, reserved
  data field, and reserved surround/height code. Because the specification
  leaves the loop cardinality undefined, the bounded parser accepts it as
  explicit decoder configuration rather than hard-coding a guessed value.
- Validation: exhaustive table 35 coverage; every valid and reserved table
  36/37 code; all 32 sign/amount combinations for each sign; a complete custom
  configuration with every optional control; per-object disable flags;
  reserved warp/global modes and reserved bits; and configured top-level
  element-ID 2 dispatch.

### OAMD extended object element

- Normative source: TS 103 420 clauses 5.5.13 through 5.5.15 and 5.6.6,
  tables 40 through 46; high-precision coordinate application additionally
  follows clauses 5.6.1.1.8 through 5.6.1.1.14.
- Official reference data: none. Printed specification pages 34, 35, and 52
  through 54 were rendered as lossless 300 DPI PNGs with Poppler 26.02.0 and
  visually inspected. This established the exact nested syntax, six-bit table,
  presence-bit ordering, reserved entries, and extended-precision semantics.
- Design rationale: retain the ID 5 object-major/block-minor grids explicitly;
  derive active state and object type from the corresponding decoded object
  element; resolve divergence reuse only within the same object's immediately
  preceding metadata block; and keep each position update's raw absolute or
  differential standard-precision coding until the extension is available.
  Position extension is therefore evaluated inside the normative coordinate
  equation before min/max clamping, including boundary cases where applying it
  to an already-clamped coordinate would produce a different answer.
- Validation: all four table 41 entries and all 64 table 42 codes (including
  reserved zero), coarse/fine/reuse modes, missing previous-block rejection,
  reserved mode rejection, absent/inactive/bed zero behavior, X/Y/Z presence
  ordering, pre-clamp differential extension, exact object/block dimensions,
  required preceding object state, and bounded top-level ID 5 dispatch.

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

### Renderer-independent object scene

- Normative source: TS 103 420 clauses 4.2 through 4.4 and 5.2 through 5.3.
- Official reference data: none; this component preserves decoded interface
  values and reconstructed samples rather than introducing codec tables.
- Design rationale: retain stable object IDs, object class, f64 PCM, timed
  metadata, anchor-specific positions (including explicit infinite room rays),
  size, priority, gain, channel lock, zones, divergence, and trim-disable state.
  JSON serialization is renderer-independent and rejects non-finite or
  cross-object/duration-inconsistent data before export. Frame assembly clones
  scene state, appends equal-length object PCM, converts every object/block
  update at `frame_offset + start_sample`, associates the following ID 5
  divergence and current trim-disable flags, validates, then commits atomically.
- Validation: JSON roundtrips cover room, infinite-ray, screen, speaker, and
  ISF anchors; decoded-structure assembly covers PCM, timing, position, gain,
  zones, and channel lock; invalid sample-rate, track-duration, and
  metadata-object boundaries are rejected.

### Object WAV serialization

- Normative source: engineering-spec decoder-interface export requirement;
  RIFF/WAVE serialization is a container concern outside TS 103 420.
- Official reference data: none.
- Design rationale: emit mono 64-bit IEEE-float WAV so reference f64 object PCM
  is preserved without clipping or sample-format quantization. Read PCM
  16/24/32, IEEE-float 32/64, and extensible equivalents into channel-major
  f64 for payload decoding. All RIFF/chunk/frame size arithmetic is checked
  before allocation or slicing.
- Validation: exact RIFF, format, rate, bit-depth, data-size, and sample-byte
  assertions, f64 mono/stereo roundtrips, PCM16 deinterleaving, and invalid-rate
  and non-finite-sample rejection.

### Payload-to-scene orchestration

- Normative source: TS 103 420 clauses 4.3–4.4, 5, 6.4, 6.6, and 7.4.
- Official reference data: the same verified Huffman/QMF companion tables used
  by the called JOC and QMF components; orchestration introduces no tables.
- Design rationale: expose the engineering-spec `JocFrameInput` boundary with
  sample rate, channel-major downmix PCM, raw JOC/OAMD payloads, and frame
  index. Parse both payloads, enforce their object-count agreement, clone JOC
  and scene state, run analysis/reconstruction/synthesis and OAMD assembly,
  then commit both only after the complete frame succeeds.
- Validation: a raw absent-JOC/inactive-OAMD vector traverses both parsers,
  normative zero initial matrix, QMF reconstruction/synthesis, timed metadata,
  and ObjectScene PCM; malformed second-frame OAMD is rejected and the same
  frame index then succeeds, proving atomic retry behavior.

### `decode-payload` CLI and artifact export

- Normative source: engineering specification clause 6 and final acceptance
  scenario 1; codec behavior is delegated to the traced normative components.
- Official reference data: none added at the CLI layer.
- Design rationale: keep file I/O in an imperative shell. Read multichannel WAV
  plus one aligned raw JOC/OAMD frame, invoke `JocFrameInput`, and write a
  PCM-free `scene.json` manifest, complete `metadata/timeline.json`, lossless
  f64 object WAV stems, and retained syntax/reconstruction debug text. Screen
  geometry is optional but must be supplied explicitly if screen anchoring is
  encountered; no non-normative default geometry is inferred.
- Validation: executable integration test invokes the actual `openjoc`
  binary and reopens the emitted object stem to verify all-zero reconstructed
  PCM plus the required scene, timeline, and debug artifact paths.

### EMDF container and payload extraction

- Normative source: TS 102 366 clauses H.2.1.1 through H.2.2.4 and tables
  H.2.1 through H.2.6; TS 103 420 clause 8.2 and table 55 assign OAMD payload
  ID 11 and JOC payload ID 14.
- Official reference data: none. TS 102 366 specification pages 204, 205, 206,
  and 209 and TS 103 420 profile pages 68 and 69
  were rendered losslessly at 300 DPI using Poppler 26.02.0 and visually
  inspected. This verified the syntax nesting, `variable_bits(n)` exponent
  grouping and group offsets, conditional payload configuration, and both
  protection-length tables without inferring damaged superscripts or layout
  from extracted text.
- Design rationale: decode every field through a declared-container-bounded
  MSB-first reader; retain payload bytes for known JOC/OAMD and unknown IDs;
  represent conditional controls with `Option`; enforce the two-group duration
  limit and a 31-group resource/arithmetic limit on otherwise unbounded small
  variable fields; reject nonzero base syntax versions; accept the zero
  reserved codec-data octet required to be present by TS 103 420 table 56 while
  rejecting nonzero reserved bits; retain implementation-defined protection bytes
  without pretending to validate them; and allow only zero padding needed to
  complete the final byte. Payload size uses at most two 8-bit groups because
  the minimum value represented by a third group exceeds the 65,535-byte
  container-length domain.
- Validation: one-, two-, and three-group offset arithmetic; group limits and
  invalid widths; complete sample-offset/duration/group and frame-aligned
  configuration branches; retained ID 11/14 bytes; every primary/secondary
  protection-length combination; reserved primary length, reserved sample
  offset bit, codec data, unsupported version, truncation, nonzero padding,
  and excess full-byte padding are tested. Table 55/56 validation additionally
  requires exactly one ID 11 and one ID 14 payload per frame, equal present
  group IDs, absent sample offset/duration, present codec data, frame alignment,
  zero duplicate flags, highest priority, and no-processing retention.

### Enhanced AC-3 syncframe acquisition and JOC extension

- Normative source: TS 102 366 clauses E.1.2.1, E.1.2.2, E.1.3.1.1 through
  E.1.3.1.6 and tables E.1.1 through E.1.3; TS 103 420 clauses 8.3.1 and
  8.3.2.
- Official reference data: none. TS 102 366 pages 114 through 116, 126, and 127 and TS
  103 420 pages 68 and 69 were rendered losslessly at 300 DPI using Poppler
  26.02.0 and visually inspected. This verified the fixed acquisition-field
  ordering, 16-bit-word frame-size relationship, sample-rate/block-count
  tables, conditional BSI layout, mixing-option-4 byte-boundary equation, and
  exact 7+1+8-bit type-A extension layout.
- Design rationale: parse the fixed prefix with the shared bounded MSB-first
  reader; turn frame size into checked bytes before indexing; advance only by
  declared sizes rather than scanning for sync patterns inside payload data;
  derive frame sample count from the normative 256 new samples per audio
  block; and parse the JOC `addbsi` extension as exactly two bytes with zero
  reserved bits, a set extension flag, and complexity no greater than 16.
- Validation: every sample-rate and block-count code, dependent identity and
  substream ID, consecutive frame offsets, reserved stream/rate values,
  invalid syncword, declared-frame truncation, extension length/flag/reserved
  failures, and both maximum valid and first invalid complexity values are
  tested. Conditional BSI parsing reaches `addbsi` without scanning, including
  a regression proving the 2-to-33-byte option-4 region includes its five-bit
  `mixdeflen` field. Dependent-substream assembly, EMDF location, CRC, and audio
  decoding remain explicitly incomplete.

### Enhanced AC-3 audio-frame state

- Normative source: TS 102 366 clauses E.1.2.3, E.1.3.2, and E.2.4.2;
  tables E.1.8 and E.1.9.
- Official reference data: none. TS 102 366 pages 116 through 118, 137, and
  138 were rendered losslessly at 300 DPI using Poppler 26.02.0 and visually
  inspected. This verified the continuous BSI-to-`audfrm` field order, compact
  syntax flags, per-block and frame-based exponent strategies, complete
  32-row frame exponent table, AHT region eligibility, and the exact
  block-start-information length equation.
- Design rationale: continue the same bounded MSB-first reader from BSI rather
  than reconstructing a cursor; retain typed frame syntax and per-block
  coupling/channel/LFE exponent state needed by `audblk`; derive frame-coded
  strategies directly from table E.1.9; count exponent regions according to
  E.2.4.2 before consuming AHT flags; reject the reserved SNR strategy; and
  retain block-start information bit-exactly so later block traversal can use
  normative offsets without scanning payload bytes.
- Validation: all 32 table E.1.9 rows; one- and six-block exact cursor cases;
  per-block and frame-coded exponent strategies; coupling reuse; LFE and
  converter syntax; AHT single-region eligibility; transient and SPX
  conditionals; reserved-SNR rejection; invalid block-start dimensions; exact
  55-bit retention; and the block-start equation at 128-byte/one-block,
  128-byte/six-block, 130-byte/six-block, and 256-byte/three-block boundaries
  are covered by the 19 `openjoc-eac3` tests. Full E.1.2.4 audio-block,
  exponent, bit-allocation, and mantissa traversal remains incomplete.

### Enhanced AC-3 audio-block dimensions

- Normative source: TS 102 366 clauses 6.1.3 and E.2.3.3 through E.2.3.5.
- Official reference data: none. Pages 53, 54, and 146 were rendered
  losslessly at 300 DPI using Poppler 26.02.0 and visually inspected before
  implementing the end-mantissa and exponent-group equations.
- Design rationale: derive an uncoupled channel's `endmant` directly from its
  bounded `chbwcod`, reject reserved codes 61 through 63, and apply the
  distinct D15/D25/D45 integer group-count equations exactly as printed.
  Decode each seven-bit grouped exponent as three base-5 mapped differentials
  using the printed inverse equations, subtract two from each mapped value,
  accumulate from the four-bit absolute exponent, and replicate each result
  over one, two, or four mantissas for D15, D25, or D45 respectively. Reject
  grouped values above 124, mismatched group counts, and every intermediate
  exponent outside the normative 0 through 24 range.
- Validation: minimum and maximum legal channel bandwidth codes, the first
  reserved code, all three exponent strategies at a common end mantissa, and
  rejection of the reuse strategy are covered. Exact neutral, decreasing,
  and increasing grouped-exponent examples validate differential accumulation
  and D15/D25/D45 replication; malformed dimensions, grouped code 125,
  exponent underflow, and group-count mismatch are rejected. Coupling, SPX,
  bit allocation, and mantissa traversal remain incomplete.

### Enhanced AC-3 spectral-extension dimensions

- Normative source: TS 102 366 clauses E.1.2.4, E.1.3.3.5, and E.1.3.3.6.
- Official reference data: none. Pages 119 and 140 were rendered losslessly
  at 300 DPI using Poppler 26.02.0 and visually inspected before implementing
  the two layout-sensitive piecewise subband equations and Table E.1.10's
  default banding structure.
- Design rationale: map the two three-bit frequency codes directly to the
  half-open active-subband interval printed in `audblk`; reject wider codes
  at the public boundary and reject empty or reversed intervals before they
  can underflow later band-structure loops. Traverse the first audio block
  from the exact `audfrm` cursor through block-switch, dither, dynamic-range,
  SPX strategy, active-channel, band-structure, and coordinate syntax; retain
  raw coordinate fields while continuing the same cursor into coupling.
  Frame initialization uses Table E.1.10's structure, while transmitted bits
  replace only the active subband interval.
- Validation: all 64 legal-width begin/end code combinations are checked
  against the two exhaustive piecewise mapping tables; both first over-width
  codes and every resulting non-forward range are rejected. A bounded
  one-block mono frame validates implicit SPX strategy/participation, default
  dither, explicit six-bit band structure, four coordinate bands, retained
  blend/master/exponent/mantissa values, and the exact post-SPX bit position.

### Enhanced AC-3 coupling strategy and coordinates

- Normative source: TS 102 366 clauses E.1.2.4, E.1.3.3.14 through
  E.1.3.3.22, and E.2.5.5.1.
- Official reference data: none. Syntax pages 120 and 121 and descriptive
  pages 141 through 143 and 145 through 146 were rendered losslessly at 300 DPI using Poppler
  26.02.0 and visually inspected before implementing standard/enhanced range,
  band-structure, coordinate, amplitude, phase, reserved-field, and rematrix
  traversal.
- Design rationale: carry the validated SPX begin code into coupling so the
  possibly negative standard `cplendf` and enhanced terminal subband are
  derived exactly; initialize standard and enhanced structures from Tables
  E.1.12 and E.1.13; treat zero structure entries as band starts and ones as
  merges; retain raw standard coordinates, enhanced amplitudes, and phase
  flags; and require every enhanced reserved bit to be zero. Checked ranges
  prevent empty regions and array overrun before any structure loop. Derive
  the stereo rematrix-flag count from the complete E.2.3.2 piecewise mapping
  and consume it immediately after coupling coordinates.
- Validation: a bounded stereo block covers standard coupling, both implicit
  participating channels, explicit five-subband/three-band structure, two
  complete coordinate sets, phase flags, two derived rematrix flags, and exact
  terminal offset. A
  bounded three-channel block covers enhanced coupling, sparse channel
  participation, the piecewise begin/end mapping, `max(9, begin + 1)` syntax,
  one merged subband, two complete five-band amplitude sets, 36 plus one
  reserved bits, and the exact terminal offset.

### Enhanced AC-3 audio-block exponent payloads

- Normative source: TS 102 366 clauses 6.1.3, E.1.2.4, E.2.3.3 through
  E.2.3.5, E.2.5.2, E.2.5.3, and E.2.6.2; tables E.2.7, E.2.9, and E.2.11.
- Official reference data: none. Pages 53, 54, 122, 146, 157 through 159,
  and 162 were rendered losslessly at 300 DPI using Poppler 26.02.0 and
  visually inspected. This verified the layout-sensitive standard-coupling
  mantissa formulas, enhanced-coupling and SPX subband boundary tables,
  syntax order, absolute-exponent scaling, differential group dimensions,
  and LFE exponent payload.
- Design rationale: consume channel bandwidth codes before exponent payloads
  exactly in syntax order. A channel participating in standard coupling ends
  at `cplbegf * 12 + 37`; enhanced coupling and SPX use the exact table
  boundary at their respective begin subband; only channels in neither tool
  consume `chbwcod`. Standard coupling spans `cplbegf * 12 + 37` through
  `(cplendf + 3) * 12 + 37`, while enhanced coupling uses table E.2.9.
  Double the four-bit coupling absolute exponent, decode one reference plus
  the active coupling bins with the shared D15/D25/D45 differential decoder,
  then omit only that reference from the exposed active-bin vector. Decode
  each channel from bin zero through its derived end mantissa and retain its
  two-bit gain range. Decode the LFE's fixed seven bins from its absolute
  exponent and two D15 groups. Block-zero reuse is rejected because this
  bounded API has no prior exponent state from which normative reuse could
  be performed.
- Validation: bounded first-block frames cover mono SPX, stereo standard
  coupling, sparse three-channel enhanced coupling with one uncoupled
  `chbwcod` channel, and an uncoupled mono-plus-LFE case. Assertions verify
  exact channel/coupling/LFE bounds, group traversal, neutral differential
  decoding, coupling absolute-exponent doubling, gain ranges, bandwidth-code
  presence, and the exact post-exponent bit offset. Together with malformed
  grouped-exponent tests, all 28 `openjoc-eac3` integration tests pass.

### Enhanced AC-3 bit-allocation parameter syntax

- Normative source: TS 102 366 clause E.1.2.4.
- Official reference data: none. Page 122 was rendered losslessly at 300 DPI
  using Poppler 26.02.0 and visually inspected to verify the conditional
  nesting, field order, word sizes, and frame-default assignments.
- Design rationale: when frame syntax enables bit-allocation updates, consume
  `baie` and retain newly transmitted `sdcycod`, `fdcycod`, `sgaincod`,
  `dbpbcod`, and `floorcod` without interpreting their later decoder-table
  meaning. Represent `baie == 0` as no new parameter set so a later stateful
  block decoder can perform normative reuse. When frame syntax disables the
  fields, expose the printed effective defaults `2/1/1/2/7` without consuming
  payload bits.
- Validation: the bounded mono-plus-LFE first-block fixture enables `bamode`,
  transmits all five non-default codes, verifies each retained value, and
  checks the exact SNR-offset boundary. Existing syntax-disabled block cases
  continue to reach the same boundary without consuming parameter bits.

### Enhanced AC-3 block SNR and fast-gain syntax

- Normative source: TS 102 366 clause E.1.2.4.
- Official reference data: none. Pages 122 and 123 were rendered losslessly
  at 300 DPI using Poppler 26.02.0 and visually inspected to verify the nested
  `snroffststr` branches, first-block implicit update, element conditionals,
  and fast-gain field widths and defaults.
- Design rationale: for block-coded SNR strategy 1, retain one six-bit coarse
  code and apply the single four-bit fine code to every active spectral
  element. For strategy 2, consume distinct coupling, full-bandwidth channel,
  and LFE fine codes in printed order. Frame-coded strategy 0 consumes no
  `audblk` bits and remains represented by the `audfrm` state. When fast-gain
  syntax signals new codes, retain each active element's three-bit value;
  otherwise expose the printed effective default code 4 without consuming
  element fields.
- Validation: the bounded mono-plus-LFE fixture uses strategy 2 and verifies
  a non-default coarse code, distinct channel/LFE fine codes, explicit
  channel/LFE fast-gain codes, absent coupling fields, and the exact
  converter-SNR-offset boundary. Existing strategy-0 fixtures verify that
  this stage consumes no block SNR bits.

### Enhanced AC-3 converter, leakage, delta-allocation, and skip syntax

- Normative source: TS 102 366 clauses 4.4.3.47 through 4.4.3.60,
  E.1.2.4, E.1.3.2.9, E.1.3.2.10, E.1.3.2.30, and E.1.3.3.25 through
  E.1.3.3.26; table 4.11.
- Official reference data: none. Syntax pages 123 and 124 were rendered
  losslessly at 300 DPI using Poppler 26.02.0 and visually inspected. The
  first-coupling-leak initialization rule and delta strategy meanings were
  additionally verified from searchable normative prose; no equation was
  reconstructed from extracted text.
- Design rationale: consume the converter SNR extension only for independent
  streams. Because `audfrm` initializes `firstcplleak` to one, the bounded
  first-block parser consumes both three-bit coupling leak codes directly and
  does not invent a transmitted `cplleake` bit. Retain delta strategy and raw
  segment codes separately for coupling and every full-bandwidth channel;
  reject both reuse and reserved strategies in block zero, while an absent
  `deltbaie` produces the normative no-delta strategy. Treat the nine-bit skip
  length as a byte count, perform checked conversion to bits, and retain every
  skipped byte without interpreting its contents.
- Validation: standard and enhanced coupling fixtures verify distinct first
  leak codes. The independent mono-plus-LFE fixture verifies the ten-bit
  converter offset, two complete channel delta segments in bitstream order,
  exact retention of a two-byte unaligned skip field, and the exact mantissa
  boundary. Frame syntax controls continue to prevent absent delta/skip
  branches from consuming bits.

### Enhanced AC-3 exponent-to-PSD mapping

- Normative source: TS 102 366 clause 6.2.2.2.
- Official reference data: none. Page 57 was rendered losslessly at 300 DPI
  using Poppler 26.02.0 and visually inspected before implementing the intact
  fixed-point equation `psd[bin] = 3072 - (exp[bin] << 7)`.
- Design rationale: expose the mapping as a pure checked function over decoded
  exponents, preserve the normative signed integer domain, and reject values
  above the clause 6.1.3 range before shifting.
- Validation: an exhaustive test maps all 25 legal exponents from 0 through 24
  and an explicit malformed case rejects 25.

### Enhanced AC-3 PSD log-addition and integration

- Normative source: TS 102 366 clause 6.2.2.3 and Table 6.14.
- Official reference data: V1.4.1 page 58 was rendered with the installed
  Poppler 26.02.0 `pdftoppm` at 300 DPI; page 63 was rendered separately for
  Table 6.14. The V1.4.1 Type3 glyph is a missing square, so the same official
  clause was cross-checked in ETSI V1.1.1 and V1.2.1, where the identical
  embedded Type3 glyph visibly renders as `~`. Their prose states that the
  operator computes the difference between operands. The official V1.3.1
  artifact confirms the same source position despite extraction loss.
- Design rationale: expose the operation as a pure `log_add` function, retain
  the fixed-point Table 6.14 correction rather than replacing it with a
  floating-point approximation, clamp the address to 255 exactly as printed,
  and expose the clause's non-uniform Table 6.12 integration as a checked pure
  function. The dedicated glyph is documented as a normative log-add operator;
  its internal difference is implemented as `a - b` from the prose definition.
- Validation: asymmetric, commutative, equal-operand, and saturated-address
  log-add cases pass; a full 0..253 PSD interval is integrated across all 50
  exact Table 6.12 bands; empty, out-of-domain, and truncated PSD ranges are
  rejected with structured errors.

### Enhanced AC-3 fixed bit-allocation parameter tables

- Normative source: TS 102 366 tables 6.6 through 6.11.
- Official reference data: none. Pages 61 and 62 were rendered losslessly at
  300 DPI using Poppler 26.02.0 and visually inspected. This verified every
  slow-decay, fast-decay, slow-gain, dB-per-bit, floor, and fast-gain entry,
  including the signed fixed-point interpretation of floor value `0xf800`.
- Design rationale: map the already parsed two- and three-bit codes through a
  pure checked function, retaining signed fixed-point values for the later
  excitation, masking, and pointer computations. Reject invalid public API
  inputs even though conforming syntax cannot transmit them.
- Validation: an exhaustive Cartesian-product test checks all 16,384 legal
  parameter combinations, and malformed tests reject every table's first
  out-of-range address with a parameter-specific error.

### Enhanced AC-3 bit-allocation band structure

- Normative source: TS 102 366 tables 6.12 and 6.13.
- Official reference data: none. Pages 62 and 63 were rendered losslessly at
  300 DPI with Poppler 26.02.0 and visually inspected before transcribing all
  50 band starts and sizes. The legal audio-bin domain ends at bin 252.
- Design rationale: retain Table 6.12 as the single normative band-layout
  source and derive the equivalent bin-to-band lookup from its contiguous
  ranges. Reject Table 6.13's non-audio padding addresses rather than exposing
  their printed zero placeholders as valid bands.
- Validation: all 50 rows and every bin from 0 through 252 are checked; band
  50 and bin 253 are explicit malformed cases.

### Enhanced AC-3 conventional bit-allocation pointers

- Normative source: TS 102 366 clause 6.2.2.7 and table 6.16.
- Official reference data: none. Pages 64 and 65 were rendered losslessly at
  300 DPI with Poppler 26.02.0 and visually inspected together because the
  table continuation on page 65 supplies addresses 28 through 31 and 60
  through 63.
- Design rationale: retain the complete table as an exact 64-entry lookup and
  require its caller to provide the already clamped six-bit address prescribed
  by the clause 6.2.2.7 pseudocode.
- Validation: all 64 addresses are checked exhaustively and address 64 is
  rejected.

### Enhanced AC-3 high-efficiency bit-allocation pointers

- Normative source: TS 102 366 clauses E.2.4.3.1 and E.2.4.3.2, table E.2.1.
- Official reference data: none. Pages 152 and 153 were rendered losslessly at
  300 DPI with Poppler 26.02.0 and visually inspected together to recover all
  64 `hebaptab` entries.
- Design rationale: keep the AHT high-efficiency lookup distinct from the
  conventional Table 6.16 lookup because it produces five-bit `hebap` values
  with different quantizer semantics.
- Validation: all 64 addresses are checked exhaustively and address 64 is
  rejected with a table-specific error.

### Enhanced AC-3 access-unit and substream ordering

- Normative source: TS 102 366 clause E.1.3.1.2 and E.2.8; TS 103 420
  clauses 8.1 and 8.2.
- Official reference data: none. The sequential independent/dependent ID and
  immediate-parent rules were verified from searchable normative prose; no
  layout-sensitive equation is involved.
- Design rationale: a new access unit begins only at independent substream ID
  zero; independent IDs and each parent's dependent IDs must ascend from zero;
  dependents must immediately follow their parent; converted type-2 streams
  cannot own dependents; and every frame in an access unit must share sample
  rate and block count. Grouping operates on previously size-bounded frame
  indices, so payload bytes cannot create false access-unit boundaries.
- Validation: two complete multi-program, multi-dependent access units;
  nonsequential independent and dependent IDs; sample-rate mismatch; and the
  resulting frame spans, rate, and sample count are tested.

### Enhanced AC-3 auxiliary EMDF carrier

- Normative source: TS 102 366 clauses E.1.2.5, E.1.2.6, 4.4.4.1 through
  4.4.4.3, and H.1.
- Official reference data: none. Pages 45, 46, and 125 were rendered
  losslessly at 300 DPI with Poppler 26.02.0 and visually inspected. This
  verified the reverse frame-end ordering, the 14-bit length, forward user-bit
  order, and the fixed 1+16-bit error-check suffix.
- Design rationale: locate `auxdatae` exactly 18 bits before the declared frame
  end; if present, decode the preceding 14-bit `auxdatal`; copy exactly that
  many immediately preceding user bits in forward order; and parse a complete
  octet-sized auxiliary carrier through the bounded Annex H parser. Audio
  mantissas are not decoded or scanned to recover this carrier, matching the
  normative reverse-extraction method.
- Validation: present and absent carriers, exact forward byte order, declared
  length exceeding the frame prefix, and an actual bounded EMDF synchronization
  header/container/protection unit carried inside an E-AC-3 frame are tested.
  Audio-block `skipfld` carriage remains incomplete and is not conflated with
  this path. For that remaining path, TS 102 366 pages 44 and 116 through 124
  were rendered losslessly at 300 DPI with Poppler 26.02.0 and visually
  inspected. Page 117 establishes the frame-level `skipflde`; page 124 places
  `skiple`, the 9-bit byte count, and exactly `skipl × 8` data bits immediately
  before variable-length mantissas; page 44 confirms the byte-count semantics.
  Because later blocks can only be reached after consuming those mantissas,
  the implementation shall use full normative audio-block traversal and shall
  not search for an EMDF syncword or implement a first-block-only shortcut.

### JOC-profile access-unit extraction and placement

- Normative source: TS 103 420 clauses 8.1 through 8.3 and tables 55 and 56;
  TS 102 366 clauses E.1.3.1.2 and H.2.
- Official reference data: none beyond the already documented 300 DPI renders
  of TS 103 420 pages 68 and 69 and TS 102 366 Annex H pages.
- Design rationale: inspect only the size-bounded `auxdata` of frames belonging
  to one already validated access unit; identify containers carrying payload
  ID 11 or 14; require one complete table-55/56 OAMD/JOC pair; require the
  type-A `addbsi` in that same syncframe; and, whenever dependent substreams
  exist, require that carrier to be the last dependent frame. Return owned
  OAMD/JOC bytes together with exact frame rate/sample timing and complexity.
- Validation: a three-frame independent/dependent/dependent access unit yields
  OAMD and JOC bytes from dependent substream 1 with the same-frame complexity
  index; moving the identical profile to dependent substream 0 is rejected
  with the exact required carrier frame. Multiple carriers and missing
  same-frame extension are structurally rejected by the public API.
  Clause 8.3.2.2 is additionally tested at its zero and sixteen-object
  boundaries, with mismatched and over-profile OAMD counts rejected.

### Direct Enhanced AC-3 inspection command

- Normative source: TS 102 366 clauses E.1.2 and E.1.3.1.2 for syncframe
  acquisition, timing, and substream ordering; TS 103 420 clauses 8.1 through
  8.3 for the JOC profile.
- Official reference data: none; the command composes the already documented
  size-bounded parsers and does not introduce codec tables or equations.
- Design rationale: read the complete byte stream once, index frames only by
  declared sizes, group validated access units, and inspect each unit through
  the bounded auxiliary EMDF/profile extractor. Report timing, carrier frame,
  complexity, and payload sizes without searching mantissa bytes for metadata.
- Validation: an actual CLI-process test supplies a bounded synthetic
  Enhanced AC-3 access unit carrying the complete table 55/56 profile and
  verifies frame count, access-unit count, rate, samples, carrier, complexity,
  and both payload sizes.

### Direct Enhanced AC-3 JOC-to-ObjectScene orchestration

- Normative source: TS 103 420 clauses 6.4, 7.2 through 7.4, and 8.1 through
  8.3; TS 102 366 clauses E.1.2 and E.1.3.1.2. The base channel decoder is the
  replaceable boundary explicitly required by the engineering specification.
- Official reference data: the already verified JOC Huffman and `prot64`
  companion tables used by the downstream reconstruction core. No existing
  JOC decoder source, symbols, layout, or behavior were consulted. FFmpeg is
  invoked only as an external black-box base E-AC-3 channel-PCM decoder; it is
  not used to locate, parse, interpret, or reconstruct JOC/OAMD metadata.
- Design rationale: index and group the original byte stream independently of
  PCM decoding; require one bounded profile carrier per access unit; validate
  clause 8.3 complexity against the decoded OAMD programme before advancing
  state; require exact rate and cumulative sample agreement; slice
  channel-major PCM by each access unit's declared block timing; and pass each
  aligned frame through the existing atomic `PayloadDecoder`. The functional
  core accepts decoded PCM directly, while the CLI shell either reads a
  caller-supplied WAV or invokes FFmpeg to create a retained
  `debug/downmix.wav` artifact.
- Validation: an actual CLI-process test supplies one 1,536-sample access unit,
  five-channel aligned PCM, valid inactive OAMD, and valid absent-object JOC;
  the direct `.ec3` command writes a scene, timeline, per-frame debug dumps,
  and an exact 1,536-sample reconstructed object WAV. A legal encoded JOC
  vector and `skipfld` carriage remain required before this path is fully
  verified.

## Ambiguities and open normative questions

Clause 5.5.14 has two internally inconsistent branches: after decoding the
table value for mode 0, a later independent conditional assigns divergence
zero; it also reads the fine code for mode 3. Table 40 instead unambiguously
defines mode 0 as table, mode 1 as previous-block reuse, mode 2 as code, and
mode 3 as reserved. OpenJOC follows table 40 and rejects mode 3. The exhaustive
table tests and parser-mode tests record this interpretation.

The same pseudocode does not explicitly assign a value when the divergence
presence flag is false. OpenJOC resolves it to zero, consistent with the flag
meaning that divergence metadata is absent, the explicit inactive-object
assignment, and the apparent intended zero-setting branch in the malformed
pseudocode. A legal conformance vector remains the compatibility gate for this
absent-property interpretation.

Clause 5.5.12 loops over `NUM_TRIM_CONFIGS`, but TS 103 420 V1.2.1 contains
no definition of that symbol, TS 102 366 V1.4.1 contains no corresponding trim
cardinality, and the official companion archive contains only JOC Huffman and
QMF tables. OpenJOC therefore requires a nonzero trim-configuration count from
the decoder configuration whenever element ID 2 is present. Parsing without
that value returns an explicit error. A legal vector or future corrigendum is
required to establish the intended profile value; a compatibility test should
be added when authoritative evidence becomes available.

Clause 5.6.0.5 defines a dynamic-only program as one or more dynamic objects
plus an optional LFE, while clause 5.6.4.8 lists only bed, ISF, and dynamic
classes when defining ordering and the render-info helper flag. OpenJOC treats
the optional LFE as the first speaker-anchored (`BedOrIsf`) object, followed by
the required dynamic objects. This preserves the general speaker-anchored-first
ordering and prevents dynamic position syntax from being read for an LFE. The
two-object LFE-plus-dynamic test records this interpretation for conformance.

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

Clause H.2.1.2.0 reads `emdf_payload_id` in the `while` condition and exits
directly when it is zero, after which `emdf_protection()` follows. Clause
H.2.2.2.4 nevertheless says that for ID zero all payload-config fields and the
payload-size field "shall be set to 0", even though the printed syntax provides
no such terminator fields to read. OpenJOC follows the normative syntax table:
the five zero ID bits terminate the loop immediately and are followed by
protection data. A compatibility test against an authoritative encoded vector
remains TODO; no decoder implementation was consulted to resolve the wording.

Clause E.1.3.3.19 first initializes `necplbnd` to the number of active
enhanced-coupling subbands, but its next printed pseudocode line assigns it
the sum of `ecplbndstrc` merge bits. That assignment contradicts the same
clause's prose, where zero starts a band and one merges into the previous
band, and E.2.5.5.1's band-index algorithm, which increments only on zero.
OpenJOC therefore computes the band count as active subbands minus the number
of merge bits (equivalently, the number of zero entries in the active range).
The enhanced-coupling test with six subbands and one merge records this
derivation; no decoder implementation was consulted.

No ambiguity has been resolved outside the normative sources. New ambiguities must
be added here with the relevant clause, competing readings, selected derivation,
and a test or explicit TODO before implementation proceeds.
