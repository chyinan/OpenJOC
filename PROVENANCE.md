# OpenJOC Implementation Provenance

OpenJOC is a clean-room implementation. Production behavior is derived only
from the public normative specifications listed below, the official ETSI
companion archive, and public mathematical/DSP literature where explicitly
recorded. No existing JOC decoder source code is an implementation reference.

### JOC interoperability profile provenance

The profile split is an architectural boundary, not a standards judgment.
The parser records the EMDF fields exactly as carried. `ETSI_STRICT` applies
the published TS 103 420 Table 55/56 constraints and retains normative failure
evidence. `DOLBY_VENDOR_COMPAT` is an explicitly named observational profile
for stable Logic Pro/Dolby ecosystem signaling; it accepts only the documented
pattern, preserves the original metadata, and emits one deviation record per
observed field. The decoder receives a validated representation and contains
no hidden compatibility normalization.

The controlled Logic Pro vector is a private, SHA-256-pinned regression input.
Its manifest expects `ETSI_STRICT=failed` and
`DOLBY_VENDOR_COMPAT=accepted_with_deviation`. Future Dolby Encoding Engine
vectors use the same optional manifest fields, so profile outcomes are
regression assertions without placing proprietary media in the repository.

The observed compatibility allowance is deliberately narrow: payload 11 and
payload 14 carry `codecdatae=0`; payload 11 carries
`payload_frame_aligned=0`; payload 11 omits `create_duplicate`,
`remove_duplicate`, `priority`, and `proc_allowed` where strict validation
expects zero. No other deviation is admitted by the vendor profile.

The E-AC-3 CLI exposes the caller-defined OAMD trim cardinality as
`--trim-config-count N`; it is never inferred from vendor metadata. On the
controlled raw Logic stream, candidate counts 1, 2, 3, 4, 5, 6, 8, and 10 all
reach the same first OAMD error (`reserved OAMD warp mode 3`). This downstream
syntax result is kept separate from the accepted-with-deviation JOC profile.

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

Scene admission retains the parser's normative object-major representation,
then materializes the renderer-independent `metadata_timeline` in temporal
block-major order. Because clause 5.3.2 makes each timing block common to all
objects, a two-object/two-block scene is emitted as `t0,t0,t1,t1`; this is a
timeline-ordering invariant, not an object/audio binding claim.

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
  bottom-left position, width, and height. Interpolation does not impose a
  second coordinate clamp: the absolute equations only clamp X/Y above at one,
  and legal negative/positive extended-precision fifth-steps can therefore
  remain slightly outside the nominal interval. Validate every raw value at
  its bit-width boundary before arithmetic.
- Validation: both absolute Z signs, all four extended-precision indices, X/Y
  upper clamping, exhaustive three-bit signed deltas, differential lower and
  upper coordinate clamping, invalid field widths, all 16 distance factors,
  finite and infinite room projection, undefined centre rays, invalid finite
  factors, integrated render-info projection, exact screen/room endpoints and
  a non-trivial screen/depth matrix evaluation, extended-coordinate overshoot
  preservation, non-finite depth-mix rejection, all eight screen factors, and
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
  nonzero reserved object-element bits are tested. Scene assembly separately
  verifies that shared timing blocks remain time-ordered across objects.

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
  Each decoded trim element is also retained in a timed `trim_timeline`,
  including warp mode, global mode, all custom centre/surround/height trims,
  both balance controls, and per-object disable flags. JSON serialization is
  renderer-independent and rejects non-finite or cross-object/duration-
  inconsistent data before export. Frame assembly stages only the current
  frame's PCM references, object updates, and trim snapshots, applies the
  following ID 5 extended-precision positions before converting every
  object/block update at `frame_offset + start_sample`, associates extension
  divergence and trim state, validates, then commits atomically without
  cloning previously accumulated object PCM. A later file sink will remove
  the remaining accumulated-scene retention at the CLI boundary.
- Validation: JSON roundtrips cover room, infinite-ray, screen, speaker, and
  ISF anchors; decoded-structure assembly covers PCM, timing, position, gain,
  zones, channel lock, ID 5 position refinement, and complete trim retention;
  invalid sample-rate, track-duration, metadata-object, trim-object-count,
  and trim-time boundaries are rejected.

### Object WAV serialization

- Normative source: engineering-spec decoder-interface export requirement;
  RIFF/WAVE serialization is a container concern outside TS 103 420.
- Official reference data: none.
- Design rationale: keep f64 as an explicit reference representation while the
  normal CLI object-stem output is f32. The wave API also supports signed
  24-bit and 16-bit PCM. Integer conversion is never silently applied: reject
  or hard-clipping must be selected explicitly, and deterministic triangular
  one-LSB dither is selected explicitly with a seed. Float output preserves
  finite values without clipping or dither. Read PCM 16/24/32, IEEE-float
  32/64, and extensible equivalents into channel-major f64 for payload input.
  All RIFF/chunk/frame size arithmetic is checked before allocation or slicing.
- Validation: exact RIFF, format, rate, bit-depth, data-size, and sample-byte
  assertions, f64 mono/stereo roundtrips, PCM16 deinterleaving, f32/f64/s24/s16
  format tests, explicit integer clipping rejection/hard-clipping tests,
  deterministic dither reproducibility, and invalid-rate/non-finite-sample
  rejection.

### Payload-to-scene orchestration

- Normative source: TS 103 420 clauses 4.3–4.4, 5, 6.4, 6.6, and 7.4.
- Official reference data: the same verified Huffman/QMF companion tables used
  by the called JOC and QMF components; orchestration introduces no tables.
- Design rationale: expose the engineering-spec `JocFrameInput` boundary with
  sample rate, channel-major downmix PCM, raw JOC/OAMD payloads, and frame
  index. Parse both payloads, enforce their object-count agreement, clone
  bounded JOC state, run analysis/reconstruction/synthesis and OAMD assembly,
  then commit both only after the complete frame succeeds. The JOC decoder
  state is cloned because it is bounded codec analysis/synthesis state; the
  accumulated scene PCM and metadata are appended through SceneBuilder's
  frame-local atomic staging and are not cloned per frame.
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
  f64 diagnostic reconstruction-row WAVs, and retained syntax/reconstruction
  debug text. These rows were historically described as object stems before
  the J1R12/J1R13 binding evidence closure; the current contract does not assign
  authored-object identity. Screen
  geometry is optional but must be supplied explicitly if screen anchoring is
  encountered; no non-normative default geometry is inferred.
- Validation: executable integration test invokes the actual `openjoc`
  binary and reopens the emitted diagnostic reconstruction row to verify
  all-zero reconstructed PCM plus the required scene, timeline, and debug
  artifact paths.

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
  are covered by the 19 `openjoc-eac3` tests. The subsequent E.1.2.4
  audio-block parser now consumes exponent, bit-allocation, mantissa, skip,
  coupling, SPX, and AHT branches atomically; its direct syncframe-to-PCM
  boundary is documented in the later transform section.

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
  bit allocation, and mantissa traversal are exercised by the complete
  audio-block integration section below.

### Enhanced AC-3 mantissa expansion and traversal

- Normative source: TS 102 366 clauses 6.3.1 through 6.3.5 and Tables 6.17
  through 6.23.
- Official reference data: none. Pages 65 through 68 of TS 102 366 V1.4.1
  were rendered losslessly at 300 DPI using Poppler 26.02.0 and visually
  inspected. The inspection verified the qntztab entries, fractional
  two's-complement range, all symmetric lookup tables, and the layout-sensitive
  triplet/pair ungrouping equations.
- Design rationale: expose the normative quantizer table as typed data; decode
  asymmetric words by sign-extending the qntztab-width word with the binary
  point left of the MSB; decode symmetric tables as exact level fractions; and
  consume a packed group only at its first mantissa while ignoring dummy values
  in a final partial group. Per TS 102 366 clause 6.3.5, pending bap 1/2/4
  groups are retained across exponent-set boundaries and interleaved other BAP
  values. The grouping state is reset at each audio-block boundary and shared
  by the normative channel, coupling, and LFE mantissa order. Dither is
  injected as caller-supplied deterministic samples so the core does not
  impose a random-number implementation. Exponent shifts are checked against
  the normative 0 through 24 range.
- Validation: every Table 6.17/6.18 bap row, symmetric table endpoint,
  asymmetric sign boundary, legal packed-group endpoint, cross-exponent-set
  grouped traversal, interleaved-BAP traversal, and separate exponent-set
  parse-only calls are covered by `crates/openjoc-eac3/tests/mantissa.rs` and
  the audio-block unit tests. Bap-zero zero/dither behavior, malformed
  dimensions, invalid codes, missing dither, exponent overflow, and the four
  external DEE fixtures' complete six-block traversal are also covered. The
  four fixtures now produce zero malformed mantissa codewords and zero
  unresolved audio blocks; this is decoder/cursor evidence only, not JOC/OAMD
  fidelity evidence.

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

### Enhanced AC-3 SNR-offset initialization

- Normative source: TS 102 366 clause 6.2.2.1 and page-57 initialization
  pseudocode for uncoupled, coupling, and LFE elements.
- Official reference data: V1.4.1 page 57 and the matching V1.1.1, V1.2.1,
  and V1.3.1 pages rendered with Poppler 26.02.0 at 300 DPI.
- Design rationale: the printed `<< 4 + fine` grouping is ambiguous under
  C-like operator precedence. The field widths and fixed-point scale require
  `(((coarse - 15) << 4) + fine) << 2`, implemented as
  `(coarse - 15) * 64 + fine * 4`; the ambiguity is retained explicitly in
  `RESEARCH_NOTES.md` and must be checked against a legal vector or corrigendum.
- Validation: exhaustive legal coarse/fine boundary tests, an all-zero special
  case test, and a representative channel bit-allocation pipeline test.

### Enhanced AC-3 first-block BAP and conventional mantissa traversal

- Normative source: TS 102 366 clauses E.1.2.3, E.1.2.4, 6.2.2, and 6.3;
  E.1.3.3.27 through E.1.3.3.35 and E.1.3.3.61 through E.1.3.3.63.
- Official reference data: none beyond the fixed tables already recorded for
  clauses 6.2 and 6.3. The mantissa ordering and skip-field placement were
  checked from the searchable syntax and the authorized 300-DPI page-124/125
  renders.
- Design rationale: treat `next_offset_bits` as the exact boundary after all
  first-block side information, compute each element's complete BAP array from
  the pure bit-allocation pipeline, then consume conventional mantissas in the
  normative channel-major order with coupling emitted once at its first
  participating channel and LFE last. Frame-strategy-1 coarse/fine SNR codes
  are retained explicitly. AHT syntax is handled by the separate Annex E
  vector/gain state machine documented below; it is never silently interpreted
  as conventional mantissas.
- Validation: uncoupled channel plus LFE and standard-coupling fixtures check
  exact BAP/mantissa lengths and consumed offsets; dither scaling is checked
  against a deterministic half-scale sample; parser tests verify frame-
  strategy-1 SNR values are retained.

### Enhanced AC-3 complete conventional audio-block traversal

- Normative source: TS 102 366 clauses E.1.2.4, E.1.3.2.30,
  E.1.3.3.1 through E.1.3.3.35, 6.1.2 through 6.3, and E.2.4.2.
  The block field order, reuse branches, and channel-major mantissa order were
  independently checked against rendered Annex E syntax pages 119 through 125
  at lossless 300 DPI with Poppler 26.02.0.
- Official reference data: none beyond the fixed bit-allocation and mantissa
  tables already recorded above.
- Design rationale: retain a typed per-syncframe state for spectral extension,
  coupling coordinates/amplitudes, exponent payloads, channel dimensions,
  SNR offsets, bit-allocation parameters, coupling leakage, delta allocation,
  and rematrix flags. Parse only fields authorized by the current block's
  strategy, resolve `reuse` from the immediately preceding block, and advance
  one checked bit cursor through all conventional mantissas. The cursor uses
  the clause-6.3.5 grouping state across exponent-set calls and resets it only
  at an audio-block boundary. The public `decode_audio_blocks` API returns
  every decoded block atomically; the legacy first-block API remains bounded
  to block zero. AHT metadata and reconstructed mantissas are emitted in the
  same atomic block records.
- Validation: a two-block mono fixture exercises exponent, SPX, bandwidth,
  parameter, and mantissa reuse with exact per-block offsets; a focused
  parse-only test covers a grouped word split across separate exponent-set
  calls with an interleaved bap=3 code; and all pre-existing first-block
  coupling, SPX, LFE, delta, dither, and BAP tests remain green. The four
  external DEE fixtures now reach all six blocks with zero malformed or
  unresolved cursor outcomes.

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

### Enhanced AC-3 excitation, masking, delta allocation, and BAP computation

- Normative source: TS 102 366 clauses 6.2.2.4 through 6.2.2.7 and Tables
  6.15 and 6.16.
- Official reference data: V1.4.1 pages 57 through 65 were rendered with
  Poppler 26.02.0 at 300 DPI and visually inspected. The same
  `calc_lowcomp` pseudocode was checked in the authorized V1.1.1, V1.2.1,
  and V1.3.1 PDFs. All four revisions print a semicolon after
  `if ((b0 + 256) == b1);`; taken as C syntax it makes the following block
  unconditional and conflicts with its `else if`. No official corrigendum was
  found in the authorized artifacts. This is recorded as a normative ambiguity.
- Design rationale: implement the only syntactically structured branch
  interpretation for `calc_lowcomp`, preserving the threshold values 384 and
  320 and the decay clamps from the surrounding normative algorithm. Keep the
  excitation, masking, delta, and BAP stages as pure functions over bounded
  fixed-point arrays. The BAP mask truncation uses the rendered `& 0x1fe0`
  operation and Table 6.16's exact 64-entry pointer mapping.
- Validation: tests cover the structured lowcomp branches, uncoupled
  excitation, knee/hearing-threshold masking, positive delta segments, and a
  representative BAP address. A compatibility test remains required if ETSI
  publishes a correction clarifying the semicolon.

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

### Enhanced AC-3 high-efficiency BAP computation

- Normative source: TS 102 366 V1.4.1 clauses E.2.4.3.1 and E.2.4.3.2,
  including the AHT masking/floor address calculation and Table E.2.1.
- Official reference data: pages 151 and 152 of the authorized ETSI PDF were
  rendered losslessly at 300 DPI with Poppler 26.02.0 and visually inspected;
  page 153 was rendered for the continuation of Table E.2.1.
- Design rationale: share the already validated PSD, masking, SNR, floor, and
  delta arithmetic with conventional allocation, but inject the normative
  high-efficiency pointer lookup only at the final address mapping. This keeps
  `hebap[]` distinct from scalar `bap[]` and provides the checked primitive
  used by the integrated AHT VQ/GAQ mantissa path.
- Validation: `computes_high_efficiency_bap_with_the_aht_pointer_table`
  exercises an address where Table E.2.1 differs from Table 6.16; the existing
  exhaustive high-efficiency pointer test covers all 64 legal addresses and
  rejection of address 64. The integrated six-block traversal is validated by
  the AHT channel fixture below.

### Enhanced AC-3 AHT inverse DCT primitive

- Normative source: TS 102 366 V1.4.1 clause E.2.4.5 and the definition of
  `Rj` immediately following its inverse-DCT equation.
- Official reference data: pages 156 and 157 of the authorized ETSI PDF were
  rendered losslessly at 300 DPI with Poppler 26.02.0 and visually inspected
  before transcribing the square-root, cosine, and piecewise `Rj` factors.
- Design rationale: `inverse_aht_dct` is a pure six-point transform over one
  spectral bin at a time. It validates finite input, keeps the normative
  `sqrt(2)` and `R0 = 1/sqrt(2)` factors explicit, and returns block-major
  coefficients without applying exponents prematurely. The function is kept
  separate from conventional mantissa traversal so the decoder can apply it
  before the clause-6.3 exponent shifts.
- Validation: AHT tests cover DC reconstruction, a first-AC cosine sample,
  expected transform energy scaling, and rejection of NaN input. No decoder
  implementation was consulted.

### Enhanced AC-3 GAQ gain-word expansion

- Normative source: TS 102 366 V1.4.1 clauses E.2.4.4.2 and E.1.3.3.27
  through E.1.3.3.34; Tables E.2.3 and E.2.4 define the active `hebap` ranges,
  binary gains, and three-state composite mapping.
- Official reference data: pages 155 and 156 of the authorized ETSI PDF were
  rendered losslessly at 300 DPI with Poppler 26.02.0 and visually inspected.
  The rendered page was used for the composite `M1/M2/M3` arithmetic rather
  than inferring it from prose extraction alone.
- Design rationale: `expand_aht_gaq_gains` returns one attenuation gain per
  six-coefficient section, maps mode 0 to unity, validates the exact word
  count for modes 1–3, and discards only the unused tail of a final composite
  triplet. It does not decode mantissa tags or perform remapping; those remain
  separate normative stages.
- Validation: AHT tests cover all four modes, unity/two/four gain mappings,
  composite word 26, reserved mode rejection, invalid binary words, and the
  existing AHT transform tests. No decoder implementation was consulted.

### Enhanced AC-3 GAQ scalar dequantization

- Normative source: TS 102 366 V1.4.1 clauses E.2.4.4.2 and E.2.4.4.2's
  Tables E.2.5 and E.2.6; the `hebap`-dependent code lengths, switched gain
  attenuation, signed fractional interpretation, and remapping constants are
  all taken from those tables.
- Official reference data: pages 155 and 156 of the authorized ETSI PDF were
  rendered losslessly at 300 DPI with Poppler 26.02.0 and visually inspected
  before transcribing the fixed-point remapping constants, including the
  negative-sign `b` entries and N/A rows.
- Design rationale: `decode_aht_gaq_mantissa` is a pure post-tag primitive. It
  validates the `hebap`/gain/code domain, decodes signed two's-complement
  fractions at the exact small/large code length, attenuates only small
  mantissas for gains two and four, and applies `y = x + a*x + b` only where
  Table E.2.6 requires it. It deliberately does not consume tag bits or
  invent a VQ table.
- Validation: AHT tests cover gain-one unity decoding, a gain-two small code,
  a gain-two large code with the signed table constant, malformed `hebap`,
  gain, and code inputs, and the complete tag traversal across six transform
  coefficients.

### Enhanced AC-3 AHT vector quantization tables

- Normative source: TS 102 366 V1.4.1 clauses E.2.4.4.1 and E.2.4.4.1's
  Table E.2.2 index widths; Tables E.3.1 through E.3.7 provide the complete
  16-bit two's-complement six-value vectors.
- Official reference data: `references/etsi/ts_102366v010401p.pdf`, pages
  175 through 191 rendered as lossless 300 DPI PNGs with Poppler 26.02.0 and
  visually inspected before transcription. The seven table cardinalities are
  4, 8, 16, 32, 128, 256, and 512 entries respectively.
- Design rationale: retain the ETSI hexadecimal words as `u16` constants, check
  the `hebap`-specific index cardinality before indexing, reinterpret each word
  as a signed 16-bit two's-complement fraction with denominator 2^15, and
  return transform-index order without applying exponents. No vector table is
  generated from an implementation-specific or third-party source.
- Validation: AHT tests verify the visually transcribed Table E.3.1 first
  vector, the Table E.3.7 final vector, every supported table's bounds through
  the lookup contract, and rejection of non-VQ `hebap` and out-of-range
  indices. The integrated audio-block fixture exercises these vectors after
  high-efficiency BAP calculation.

### Enhanced AC-3 AHT mantissa traversal and six-block reconstruction

- Normative source: TS 102 366 V1.4.1 clauses E.1.2.4 fields
  `chgaqmod`/`pre_chmant`, `cplgaqmod`/`pre_cplmant`, and
  `lfegaqmod`/`pre_lfemant`; clauses E.2.4.2 through E.2.4.5; Tables E.2.2
  through E.2.6 and E.3.1 through E.3.7.
- Official reference data: pages 144, 147 through 156 of
  `references/etsi/ts_102366v010401p.pdf` were rendered to lossless PNG at
  300 DPI with Poppler 26.02.0 and visually inspected. Page 156 was used for
  the layout-sensitive IDCT equation and gain-remapping table; pages 148–151
  were used for the GAQ active-bin and gain-section pseudocode.
- Design rationale: maintain one typed AHT payload per channel, coupling
  element, and LFE element. On the first participating mantissa position,
  read the two-bit mode, gain words, and pre-mantissas once; inverse-transform
  each spectral-bin vector; then select the current block coefficient and
  apply the exponent shift. Later blocks consume no duplicate AHT payload.
  Coupling is decoded immediately after the first participating channel and
  LFE after all full-bandwidth channels, matching E.1.2.4 ordering. Public
  block records preserve the active `hebap` values and first-pass gain
  metadata.
- Normative ambiguity: E.1.2.4 says that when `chgaqbin == 0`, only
  `pre_chmant[0]` is transmitted, while E.2.4.4 describes six values across a
  DCT block. OpenJOC follows the literal syntax: the transmitted value is
  placed at transform index zero and the other five values are zero. This is
  documented by the scalar-bin compatibility test and must be revisited if
  ETSI publishes clarifying test vectors.
- Validation: six-block AHT fixtures exercise a channel with mode-1 GAQ gains,
  VQ bins, inverse DCT, exponent shifts, and exact no-repeat payload
  consumption; separate coupling and LFE fixtures verify syntax ordering and
  state reuse. Pure AHT tests cover modes 0–3 and large-mantissa tags;
  malformed/truncated syntax returns checked errors.

### Enhanced AC-3 inverse TDAC transform and overlap/add primitives

- Normative source: TS 102 366 V1.4.1 clauses 5.2.10, 5.2.11, and 6.9.4.1
  through 6.9.4.2; Table 6.33 supplies the transform-window sequence.
- Official reference data: pages 82 through 86 of
  `references/etsi/ts_102366v010401p.pdf` were rendered as lossless PNG at
  300 DPI with Poppler 26.02.0 and visually inspected. The implementation
  follows the printed pre-IFFT, complex-IFFT, post-IFFT, window/de-interleave,
  and overlap/add assignments; it does not infer the short-transform index
  layout from damaged text extraction.
- Design rationale: keep the transform as a pure bounded primitive over the
  256 interleaved coefficients already exposed by the audio-block decoder.
  `inverse_transform` returns the 512-sample windowed block for either one
  512-sample transform or two interleaved 256-sample transforms. `overlap_add`
  owns the 256-sample delay state, applies the normative factor of two, and
  advances the delay only after validating both input dimensions. The new
  `AudioPcmSynthesizer` stage pads the bounded channel/LFE spectra to 256 bins,
  applies each full-bandwidth `blksw[ch]`, uses the mandatory long transform
  for LFE, and keeps independent overlap histories across syncframe calls. Its
  state is committed only after a complete block sequence succeeds; the
  `synthesize_audio_blocks` convenience API starts from zero history. The
  current implementation uses direct O(N²) complex sums for auditability; an
  optimized FFT path is deferred until conformance vectors exist.
- Validation: malformed coefficient dimensions, non-finite coefficients,
  zero-valued long/short blocks, a nonzero DC coefficient against the rendered
  ETSI equations and Table 6.33 values, delay-state advancement, two-block
  full-bandwidth PCM synthesis, block-switched synthesis, seven-bin LFE
  synthesis, cross-call delay retention, and reset behavior are covered by
  `crates/openjoc-eac3/tests/transform.rs` and
  `crates/openjoc-eac3/tests/syncframe.rs`. `decode_audio_frame_pcm` is the
  direct bounded syncframe-to-PCM entry point. The access-unit shell still uses
  the replaceable external base E-AC-3 PCM boundary documented below, so this
  stage does not claim a complete independent/dependent-substream decoder.

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

### JOC E-AC-3 access-unit PCM assembly

- Normative source: TS 103 420 V1.2.1 clauses 4.3, 6.3.2.2-6.3.2.3,
  8.1 and E.3; TS 102 366 V1.4.1 clauses E.1.3.1.7-E.1.3.1.8,
  E.2.8.2, and Table 4.3.
- Official reference data: searchable ETSI prose from pages 16, 68-69 of
  `references/etsi/ts_103420v010201p.pdf` and pages 127-128 and 170-171 of
  `references/etsi/ts_102366v010401p.pdf`. No damaged mathematical formula is
  used by this channel-location mapping, so raster formula recovery was not
  required.
- Design rationale: `BitstreamInformation::channel_map` retains the optional
  16-bit dependent `chanmap` in its normative MSB-first representation.
  `JocAccessUnitPcmDecoder` enforces the JOC elementary-stream shape (I0 and
  optional D0) and the E.3 six-audio-block requirement, decodes each source with independent TDAC history, reorders
  the E-AC-3 Table 4.3 base order into the JOC Table 47 order, and replaces
  matching dependent locations (including standard `S`/custom `Cs`) while appending the 7.X or 5.X+2 pair. LFE is
  returned separately because Table 47 explicitly bypasses it in JOC.
  State is cloned and committed only after both source frames and the merge
  pass succeed. The CLI exposes this path through `--internal-base`; the
  replaceable FFmpeg downmix boundary remains available by default.
- Normative limitation: TS 103 420 E.3 permits at most one D0, so streams
  with additional independent/dependent frames are rejected by this JOC
  path rather than silently dropping audio. General TS 102 366 multi-program
  selection remains outside the JOC elementary-stream contract.
- Validation: dependent custom `chanmap` parsing, replacement and supplement
  mapping, indexed I0 syncframe-to-PCM synthesis, channel ordering, sample
  count, six-block enforcement, and finite PCM output checks are covered by
  `crates/openjoc-eac3/tests/syncframe.rs` and the access-unit module tests.
  `crates/openjoc-cli/tests/inspect.rs` drives a legal synthetic five-channel
  I0 frame with carried OAMD/JOC EMDF through `--internal-base` and verifies
  the metadata-only scene and diagnostic reconstruction-row output.

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
  Audio-block `skipfld` carriage is not conflated with this reverse carrier
  extraction path. TS 102 366 calls the `skipfld` bytes dummy data to be
  ignored; TS 103 420 requires an EMDF container for a JOC profile but does not
  expressly designate `skipfld` as a JOC carrier. OpenJOC therefore treats an
  exact reached `skipfld` range as a diagnostic Annex H candidate only. The
  parse-only `inspect_audio_block_carriers` boundary reaches each bounded block
  prefix and declared `skipfld` without PCM synthesis; the complete
  `decode_audio_blocks` traversal remains the path used for full audio decode.
  TS 102 366 pages 44 and 116 through 124 were
  rendered losslessly at 300 DPI with Poppler 26.02.0 and visually inspected:
  page 117 establishes frame-level `skipflde`; page 124 places `skiple`, the
  9-bit byte count, and exactly `skipl × 8` data bits immediately before
  variable-length mantissas; page 44 confirms the byte-count semantics. The
  frame-end classifier continues to use only the normative carrier range and
  never searches mantissa bytes for an EMDF syncword. A carrier that begins
  with the EMDF syncword but has undeclared trailing bytes is reported as a
  bounded trailing-data candidate rather than accepted as padded data.
- The four supplied real fixtures now reach every six-block `audfrm` cursor;
  their bounded `skipfld` fields are observed, byte lengths and both
  frame-relative/elementary-stream offsets are recorded, and each declared
  range is classified by the bounded Annex H parser as a diagnostic candidate.
  The classifier accepts only an exact `0x5838` start and the complete declared
  container; it never scans later bytes, concatenates fields, or assumes
  undeclared carrier padding. In the four fixtures, one exact range per access
  unit parses as an Annex H candidate with payload IDs 11, 14, 2, and 1. The
  ID-11 configuration fails TS 103 420 Table 56 (`codecdatae=0` and
  `payload_frame_aligned=0`), so this is not a valid JOC profile and is not
  proof that `skipfld` is a normative JOC carrier. Coverage of the frame-end
  path and this diagnostic skip-field candidate path is implemented; the
  carriage interpretation and any additional carrier ambiguity remain open.

### Skip-field carriage audit and bounded candidate rule

- Normative pages inspected: TS 102 366 V1.4.1 p.44 (`skiple`, the 9-bit
  `skipl` count, and dummy `skipfld` bytes), p.117 (`skipflde`), and p.124
  (the order `skiple` -> `skipl` -> `skipfld`, followed by the mantissa
  syntax); TS 103 420 V1.2.1 pp.68-69 (Tables 55-56, payload IDs 11/14,
  configuration, `addbsi`, and last-dependent placement); TS 102 366 Annex H
  pp.204-209 (the exact EMDF syncword, declared container length, syntax,
  protection, and padding).
- The `skiple` flag gates a 9-bit `skipl` value. Exactly `skipl * 8` bits are
  read as the `skipfld` data range. OpenJOC preserves the range's
  frame-relative bit offset, elementary-stream absolute bit offset, and
  declared bit length; it does not reinterpret the preceding `skiple` or
  `skipl` bits as payload data. The range may begin at a non-byte frame bit
  offset, but its declared data length is an integral number of bytes.
- Annex H parsing starts at bit zero of the extracted candidate bytes only
  when the caller-declared range begins with the exact `0x5838` syncword. The
  bounded parser consumes the 16-bit syncword, 16-bit declared byte length,
  container syntax, protection, and permitted terminal padding. A range that
  does not begin with the syncword is classified as ordinary non-EMDF data; a
  sync-start range whose bounded syntax fails is a malformed candidate; a
  complete container followed by undeclared bytes is a trailing-data
  candidate. No sliding offset search, padding invention, or cross-range
  concatenation is performed.
- Annex H permits one declared container in the range examined by this API.
  The inspected specifications do not state that one E-AC-3 `skipfld` may
  carry multiple concatenated containers, nonzero user padding after a
  container, or fragments that may be reassembled across blocks, syncframes,
  or substreams. OpenJOC therefore does not implement those interpretations.
  Whether a `skipfld` dummy-data field is an authorized JOC/EMDF carrier is an
  unresolved carriage-semantic question, not an implementation license to
  search for magic bytes.
- A complete Table 55/56 profile is validated within one parsed container:
  exactly one payload ID 11 and one ID 14, matching group IDs, required
  configuration, same-frame type-A `addbsi`, and the required last-dependent
  placement. OpenJOC never combines ID 11 from one candidate with ID 14 from
  another. Same-access-unit duplicate or mixed frame-end/skip-field profile
  candidates are rejected as ambiguous rather than silently ordered.

### JOC-profile access-unit extraction and placement

- Normative source: TS 103 420 clauses 8.1 through 8.3 and tables 55 and 56;
  TS 102 366 clauses E.1.3.1.2 and H.2.
- Official reference data: none beyond the already documented 300 DPI renders
  of TS 103 420 pages 68 and 69 and TS 102 366 Annex H pages.
- Design rationale: inspect only size-bounded frame-end `auxdata` and exact
  reached audio-block `skipfld` candidate ranges belonging to one already
  validated access unit; identify containers carrying payload ID 11 or 14;
  require one complete table-55/56 OAMD/JOC pair; require the type-A `addbsi`
  in that same syncframe; and, whenever dependent substreams exist, require
  that carrier to be the last dependent frame. Never combine payloads from
  separate carriers. The `skipfld` path is deliberately a bounded diagnostic
  candidate path, not a normative assertion that dummy bytes carry JOC. Return
  owned OAMD/JOC bytes together with exact frame rate/sample timing and
  complexity.
- Validation: a three-frame independent/dependent/dependent access unit yields
  OAMD and JOC bytes from dependent substream 1 with the same-frame complexity
  index; moving the identical profile to dependent substream 0 is rejected
  with the exact required carrier frame. Multiple carriers and missing
  same-frame extension are structurally rejected by the public API.
  Clause 8.3.2.2 is additionally tested at its zero and sixteen-object
  boundaries, with mismatched and over-profile OAMD counts rejected. The new
  exact-range classifier has unit coverage for non-EMDF data, truncated or
  malformed sync-start candidates, and undeclared trailing bytes. The external
  four-fixture run parsed one bounded skip-field candidate per access unit and
  retained payload IDs 11/14, but the ID-11 Table 56 configuration was invalid;
  no real-vector reconstruction claim or normative `skipfld`-carriage claim is
  made.

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
  caller-supplied WAV or invokes FFmpeg to create the retained
  `debug/compatible_base.wav` artifact.
- Validation: an actual CLI-process test supplies one 1,536-sample access unit,
  five-channel aligned PCM, valid inactive OAMD, and valid absent-object JOC;
  the direct `.ec3` command writes a scene, timeline, per-frame debug dumps,
  and an exact 1,536-sample diagnostic reconstruction-row WAV. A legal encoded JOC
  vector, complete legal-carrier coverage, and real-vector PCM evidence remain
  required before this path is fully verified.

### External real-DEE fixture census and first-failure diagnostics

- Normative source: TS 102 366 clauses E.1.2, E.1.2.5, E.1.3.1.2, and
  E.2.8.2 for bounded syncframe, access-unit, audio-block, and `skipfld`
  traversal; TS 103 420 clauses 8.2 and 8.3 and Annex H for bounded EMDF,
  OAMD, JOC, and `addbsi` interpretation.
- External tools: FFprobe/FFmpeg are used only through the documented
  input-media boundary for track probing, stream-copy demux, and compatible
  base-channel reference PCM. No external decoder implementation is used to
  interpret codec metadata.
- Design rationale: `OPENJOC_REAL_FIXTURE_MANIFEST` (or an explicit manifest
  argument) loads stable labels, optional hashes, and user notes without
  copying programme bytes into the repository. Entries are sorted by label;
  source and demuxed hashes, bounded frame/index counts, addbsi/complexity,
  frame-end and skip-field carrier attempts, all reached block prefixes,
  skip-field lengths, payload IDs by carrier kind, profile counts, and first
  failures are emitted to JSON and text reports.
  The report uses explicit carrier states so “not found in validated paths”
  cannot be confused with an untraversed carrier.
- Parse-only boundary: `inspect_audio_block_carriers` follows the checked BSI
  and `audfrm` cursor through all six `audblk` side-information prefixes and
  declared skip fields on the four external fixtures without PCM synthesis.
  The clause-6.3.5 grouping state keeps the mantissa cursor bounded across
  exponent sets and interleaved BAP values. Each declared skip-field byte range
  is passed to the existing bounded Annex H classifier at its exact start. No
  bytes are scanned for EMDF outside declared carrier ranges, no fields are
  concatenated, and undeclared trailing bytes are not treated as padding.
- First-failure diagnostics: complete internal-base decode wraps invalid
  mantissa code errors with element, channel, block, BAP, raw code, quantizer
  width, grouped state, and frame-relative bit offset. The census additionally
  records access-unit/syncframe and elementary-stream offsets. This is a
  diagnosis aid, not a correction of the normative decoder.
- Validation: unit tests cover duplicate/empty manifests, hash and missing
  fixture errors, stable report ordering, carrier-state ordering, reached
  prefixes versus unresolved blocks, and existing raw/container paths. An
  opt-in four-fixture external corpus run produced the following evidence;
  programme bytes are not committed. The grouped-mantissa correction in
  commit `2c524d107ae7451b2a6c838e7ca64159a51b375b` changed all four reports
  from `carrier_unresolved` to complete six-block traversal: malformed
  mantissa count is zero and unresolved block count is zero. The subsequent
  skip-field integration in `d900ef13c3c3977d6f0cd861d00293d002f00006`
  classified one bounded Annex H candidate per access unit with IDs 11, 14,
  2, and 1, but the ID-11 Table 56 configuration is invalid; no complete JOC
  profile is accepted.

  | label | source SHA-256 | bytes | frames/access units | addbsi complexity | frame-end auxdatae | skip observed/examined/unresolved | skip EMDF valid/malformed | payload 11/14 | state |
  | --- | --- | ---: | ---: | ---: | ---: | --- | --- | --- |
  | `brainrot` | `2808eecb80353141135000ab499815219a86770e5b02e912dc971dd01e86afd7` | 16,283,910 | 3,910/3,910 | 3,910 × 16 | 0/3,910 | 3,910/23,460/0 | 3,910/0 | 3,910/3,910 | `emdf_profile_incomplete` |
  | `forever_friends` | `67c10f65642f11713f8495026a37cf26fd1f901e9a343d2e3acf5ee879584896` | 32,138,978 | 7,773/7,773 | 7,773 × 16 | 0/7,773 | 7,773/46,638/0 | 7,773/0 | 7,773/7,773 | `emdf_profile_incomplete` |
  | `grand_escape` | `b7a320d2ff14a27e64b9e0262f2092b31145bc217100a2f987d174fef0ef2956` | 44,175,378 | 10,599/10,599 | 10,599 × 16 | 0/10,599 | 10,599/63,594/0 | 10,599/0 | 10,599/10,599 | `emdf_profile_incomplete` |
  | `hitchcock` | `0075ade8f801e38a4f98637d9d9a8099771ea1edd0bb66bd829aa2c0faa3e425` | 29,370,578 | 7,146/7,146 | 7,146 × 16 | 0/7,146 | 7,146/42,876/0 | 7,146/0 | 7,146/7,146 | `emdf_profile_incomplete` |

  All four inputs are ISO BMFF with one 48 kHz six-channel `eac3` stream and
  1,536 samples per access unit. Every inspected frame has `addbsi` bytes
  `01:10`, and frame-end `auxdatae` is absent. Every reached skip-field exact
  range is classified by the bounded parser as an EMDF candidate with IDs 11,
  14, 2, and 1; this is not a normative assertion that `skipfld` dummy bytes
  are an authorized JOC carrier. All candidates fail the ID-11 Table 56
  configuration check. All four reports have zero malformed mantissa codewords
  and zero unresolved audio blocks. No complete JOC profile was extracted, so
  these fixtures are still not legal nonzero JOC/OAMD acceptance vectors.

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

### Standard coupling coordinate reconstruction

- Normative source: TS 102 366 V1.4.1 clauses 6.4.2 through 6.4.4 and
  Table 6.24. Clause 6.4.3 defines the exponent/mantissa/master coordinate
  scale; clause 6.4.4 defines the factor-of-eight channel reconstruction and
  clause 6.4.1 defines 2/0 right-channel phase restoration.
- Official reference data: pages 69 and 70 of
  `references/etsi/ts_102366v010401p.pdf` were rendered as lossless 300-DPI
  PNGs in `.codex-tmp/coupling-render/` and visually inspected. The printed
  pseudo-code, rather than text-extraction guesses, determines the local
  sub-band expansion and coordinate arithmetic.
- Design rationale: `reconstruct_standard_coupling` accepts the independently
  decoded low-frequency channel vectors and the contiguous coupling vector,
  validates all dimensions and coordinate domains, expands local coupling-band
  coordinates through `cplbndstrc`, and returns complete 256-bin channel
  spectra. Phase flags are applied only to channel index one, matching the
  2/0 right-channel rule; no rematrix or spectral-extension behavior is hidden
  inside this function. The audio-block path now applies this standard result,
  and merges enhanced-coupling regions into the same bounded 256-bin channel
  vectors before SPX synthesis.
- Validation: a two-channel fixture checks coordinate value `0.5`, the
  clause-6.4.4 factor of eight, exact 37..73 coupling placement, negation
  of the first right-channel sub-band, and 256-bin audio-block output. The
  enhanced-coupling fixture checks that both sparse reconstructed regions are
  merged into complete channel vectors before the downstream transform and
  PCM stages.

### Enhanced coupling coefficient reconstruction

- Normative source: ETSI TS 102 366 V1.4.1 clauses E.2.5.3, E.2.5.5.1, and
  E.2.5.5.2; Table E.2.9 supplies the enhanced-coupling subband transform
  starts and Table E.2.10 supplies the amplitude mantissa/exponent pairs.
- Official reference data: the authorized ETSI PDF
  `references/etsi/ts_102366v010401p.pdf`, rendered at 300 DPI as
  `.codex-tmp/enhanced-coupling-render/page159-159.png` and
  `.codex-tmp/enhanced-coupling-render/page160-160.png`. The rendered pages
  were visually inspected because the pseudo-code contains fixed-point
  division and right-shift layout that is not safely recoverable from plain
  text extraction.
- Design rationale: `reconstruct_enhanced_coupling` keeps the decoded coupling
  mantissas in their normalized floating-point representation and applies the
  Table E.2.10 value as `mantissa / 32 / 2^exponent`. It expands each active
  band over the exact Table E.2.9 bin interval, preserves the absolute
  `[begin_mantissa, end_mantissa)` range, and returns `None` for channels not
  marked as coupled. This leaves the independently decoded low-frequency
  bins untouched while exposing the normative reconstructed region to the
  later transform/audio pipeline.
- Validation: `reconstructs_enhanced_coupling_coefficients_from_band_amplitudes`
  covers a merged subband, unity gain, finite attenuation, minus-infinity
  amplitude code 31, uncoupled-channel omission, and exact bin counts. The
  enhanced-coupling frame fixture also checks that `DecodedAudioBlock` carries
  the reconstructed 49..121 coefficient region. No existing decoder source
  was consulted.

No ambiguity has been resolved outside the normative sources. New ambiguities must
be added here with the relevant clause, competing readings, selected derivation,
and a test or explicit TODO before implementation proceeds.

### Rematrix reconstruction

- Normative source: ETSI TS 102 366 V1.4.1 clauses 6.5.2 through 6.5.4,
  Tables 6.25 through 6.28, and Annex E clause E.2.3.2. Clause 6.5.4
  requires `left = received left + received right` and
  `right = received left - received right` for each flagged band, with
  operation limited to the lower channel bandwidth. Annex E supplies the
  piecewise rematrix-band count for standard/enhanced coupling and SPX.
- Official reference data: PDF pages 70 through 73 and 145 through 146 of
  `references/etsi/ts_102366v010401p.pdf` were rendered losslessly at 300 DPI
  with Poppler 26.02.0 to `.codex-tmp/rematrix-render/` (the Annex E pages
  were also text-checked) and visually inspected before coding. No third-party
  decoder source was consulted.
- Design rationale: `rematrix_channels` is a pure transform over two decoded
  channel vectors. It derives the exact coefficient ranges from the four
  normative tables, terminates a coupling band at the coupling start, accepts
  every Annex E flag count including no-op flags beyond a short SPX bandwidth,
  clips to the common channel length, validates all inputs for finiteness, and
  leaves unflagged bins unchanged. The audio-block traversal calls it after
  conventional channel mantissas are decoded and before the block is emitted.
- Validation: `crates/openjoc-eac3/tests/coupling.rs` covers Table A and
  standard-coupling boundaries, sum/difference restoration, lower-bandwidth
  clipping behavior, wrong flag counts, and non-finite coefficient rejection;
  the complete E-AC-3 test suite remains green.

### Dynamic-range gain processing

- Normative source: ETSI TS 102 366 V1.4.1 clauses 5.2.9 and 6.7.2.1 through
  6.7.2.2, including Table 6.29. The three-bit signed field selects the
  arithmetic-shift factor `2^(X+1)` and the five-bit field selects the
  fractional factor `(32 + Y) / 64`; the product is the coefficient gain.
  `dynrng2` is independent only in `acmod == 0`, and an absent later-block
  word reuses the previous effective word while block zero defaults to zero.
- Official reference data: PDF pages 74 and 75 of
  `references/etsi/ts_102366v010401p.pdf` were rendered losslessly at 300 DPI
  with Poppler 26.02.0 to `.codex-tmp/dynrng-render/` and visually inspected;
  Table 6.29 and the bit-field layout were checked against the rendered page.
- Design rationale: `dynamic_range_gain` is a pure word-to-linear conversion;
  `apply_dynamic_range_gains` validates dimensions and finite values and
  returns new vectors. Audio-block traversal stores effective dynrng state,
  applies the primary/secondary gains after rematrixing, and scales coupling
  and LFE mantissas with the primary gain before enhanced-coupling expansion.
- Validation: gain endpoints and the fractional mapping are covered by
  `crates/openjoc-eac3/tests/coupling.rs`; the two-block syncframe fixture
  proves a present word is applied and reused when the next block omits it,
  while the existing 0xa5 SPX fixture validates the applied non-unity gain.

### Spectral-extension coefficient synthesis

- Normative source: ETSI TS 102 366 clauses E.1.3.2.24 through E.1.3.2.25,
  E.1.3.3.4 through E.1.3.3.13, and E.2.6.2 through E.2.6.4.3; Tables
  E.1.10, E.2.11, and E.2.12.
- Official reference data: pages 139–140 and 161–167 of
  `references/etsi/ts_102366v010401p.pdf` were rendered losslessly at
  300 DPI with Poppler 26.02.0 to `.codex-tmp/spx-syntax-render/` and
  `.codex-tmp/spx-render/`, then visually inspected. Table E.2.11's low
  transform-coefficient column resolves the previously noted `spxstrtf`
  mapping: the two-bit value indexes the first four entries of the same
  `spxbandtable` used by E.2.6.4.1. Table E.2.12 values were transcribed from
  the rendered rows 0–31, including the continuation on page 166.
- Design rationale: `synthesize_spectral_extension` is a pure, checked
  implementation of the normative ordering: derive 12-coefficient band
  sizes, translate with copy-region wrapping, compute RMS before attenuation,
  apply the symmetric five-tap notch at the baseband and wrap borders, blend
  translated coefficients with caller-provided unit-variance noise, then
  apply each floating-point coordinate and the final factor 32. The parser
  now retains frame-level `chinspxatten`/`spxattencod` values. The integrated
  block path supplies a deterministic zero-mean/unit-variance uniform noise
  sequence because the standard specifies the `noise()` contract but no
  mandated generator; this choice is isolated from the pure synthesis core.
- Validation: `crates/openjoc-eac3/tests/spx.rs` checks table-indexed
  translation, per-band noise blending, coordinate scaling, and the exact
  five-tap attenuation symmetry. The syncframe fixture asserts retention of
  frame attenuation code 17, and workspace tests cover the integrated block
  call. A bounded legacy syntax fixture contains a zero-width synthetic copy
  region (`spxstrtf == spxbegf`); the block shell preserves its parsed baseband
  rather than fabricating coefficients, pending a legal conformance vector.
### Completed input-media boundary and ISO BMFF stream-copy demux

- Normative codec sources: the elementary stream produced at this boundary is
  parsed by the existing TS 102 366 E-AC-3 frontend and TS 103 420 clause 8
  EMDF/JOC/OAMD path. ISO BMFF box syntax and `ec-3` sample carriage are used
  only as container interoperability concerns; they do not define codec
  decoding behavior here.
- External tool provenance: FFmpeg/FFprobe are invoked as black-box container
  tools only. FFprobe selects and reports the audio stream; FFmpeg is invoked
  with `-c:a copy -f eac3` and therefore performs no audio re-encoding. The
  resulting bytes are bounded, then independently indexed and grouped by
  `openjoc-eac3` before any JOC/OAMD decode. No FFmpeg decoder output is used
  as reconstructed object audio.
- Design rationale: inspect the first 12 bytes before calling
  `index_syncframes`; recognize the E-AC-3 syncword only for raw input and the
  ISO BMFF `ftyp`/top-level box signature for containers. Exactly one audio
  stream with codec `eac3` is accepted. Missing, multiple, unsupported, probe,
  demux, oversized, empty, and invalid-elementary-stream conditions have
  distinct errors. Raw `.ec3` bytes retain the pre-existing OpenJOC parser and
  error path.
- Validation: `crates/openjoc-container/tests/input_media.rs` covers pure
  signatures and FFprobe-row parsing. `crates/openjoc-cli/tests/container.rs`
  exercises FFmpeg/MP4Box-generated ISO BMFF, byte-equivalent stream-copy
  output, raw classification, inspect/decode routing, malformed input, and
  missing/multiple audio-track diagnostics. MP4Box is test-fixture tooling
  only and is not an implementation dependency.
- Environment evidence: Poppler 26.02.0 (`pdftoppm`, `pdftotext`, `pdfinfo`)
  and FFmpeg/FFprobe are available in the development environment. No
  proprietary decoder source was inspected.
- Audit refresh: the supplied DEE M4A hash and size were rechecked, the
  fixture-gated container integration test was run with
  `OPENJOC_DEE_FIXTURE` set, and the release `inspect` command again reported
  the ISO BMFF boundary and 7,773 access units. This confirms the container
  increment only; it does not promote the fixture to a nonzero JOC/OAMD vector.

### Controlled Logic Pro vector production and dual-profile acceptance

- Production provenance: Logic Pro 12.3 on macOS produced a new four-second,
  48 kHz controlled Atmos project from deterministic PCM24 sources. The
  project has one stereo bed, one mono 997 Hz object, unity routing, no
  creative plug-ins, Smart Tempo and Flex disabled, and 30 explicit object
  position automation events. Project media were hash-checked against the
  deterministic sources after correcting an initially detected 44.1/48 kHz
  import-rate mismatch. The rejected pre-correction exports remain isolated
  outside the repository.
- ADM ground truth: the final 11-channel PCM24 ADM BWF contains exactly
  192,000 samples at 48 kHz, two ADM objects, 11 track UIDs, and 197 object
  position blocks. The object channel is sample-identical to the 997 Hz source
  (`correlation=1`, `gain=1`, zero residual), while the bed remains distributed
  through the ten-channel bed by Logic's panner. These are source and authoring
  checks, not proof that the encoded EMDF profile is standards-conformant.
- Encoded artifact: the non-committed 768 kbit/s DD+ Atmos MP4 has SHA-256
  `704545f313148412d019a8e7e739fccc0ead345ba7afb4b3b32199fde7b79af0`;
  its independent stream-copy EC-3 is 387,072 bytes with SHA-256
  `7ed23a04628c62300a3cc4cee846a308077f8a9117e96366d2b018e6b3ec2249`.
  FFprobe identifies the codec profile as Dolby Digital Plus + Dolby Atmos,
  48 kHz, six channels, `5.1(side)`.
- Strict census result: all 126 access units contain `addbsi` complexity 16,
  have no frame-end `auxdatae`, and expose one exact bounded `skipfld` Annex H
  candidate after complete six-block traversal. Each candidate contains IDs
  11, 14, 2, and 1. IDs 11 and 14 use group 0 and no duration, but both set
  `codecdatae=0`; ID 11 also sets `payload_frame_aligned=0`. ID 14 is frame
  aligned with duplicate flags false, priority zero, and processing allowed
  zero. This fails TS 103 420 Table 56 in every access unit; no ETSI_STRICT
  profile is accepted. The explicit DOLBY_VENDOR_COMPAT profile accepts the
  same observed pattern with seven deviations and preserves the original bytes
  for the decoder layer.
- Reproducibility: two independent release census runs are byte-identical.
  Their JSON SHA-256 is
  `52302b6fee68e5ad4bcf1c3bbc4c526077efb223126a975c37a732b010035432`;
  the text SHA-256 is
  `5b94f9d45faba8f62a2260fb9ad34857c62a82fd60f8871e29cb75cb2f04f928`.
  The census now retains all per-payload configuration fields in JSON and
  prints the first parsed carrier's configurations in the text report.
- Interpretation boundary: this is evidence of a systematic mismatch between
  this vendor export and the public Table 56 profile constraints, not evidence
  of intent, hidden commercial protection, or permission to relax validation.
  `skipfld` remains a bounded diagnostic candidate because TS 102 366 calls it
  dummy data and TS 103 420 does not expressly designate it as a JOC carrier.

### User-supplied legal DEE fixture (acceptance lane remains open)

### Controlled Logic OAMD round-2 evidence (private, not committed)

- Inputs are the existing controlled Logic raw EC-3, its MP4, and the same
  exported ADM BWF.  New evidence is written only under
  `OpenJOC-Private/reports/oamd_round2/`; the prior forensic/census outputs
  are not a source of decoder semantics and are not overwritten by the CLI.
- The timing report records 126 AUs at 48 kHz/1,536 samples (`0.032 s/AU`),
  first payload-11 change at AU 15 / `0.480 s`, 63 unique payload-11 bodies,
  exact bit intervals/byte changes, and raw-vs-MP4 payload hash equality.
- `OBJ_997HZ` is read from the ADM `axml` chunk as a 197-block Cartesian
  object timeline.  ADM values remain in their source coordinate system; no
  unproven conversion to OAMD coordinates is applied.
- An independent diagnostic bit oracle reports raw warp `3` at
  `[526,528)` and closes the two top-level elements/payload.  It is test-only
  and does not call the formal parser.  The three hypothesis rows are
  diagnostic-only, non-unique bounded closures with semantic fields left
  unresolved.
- Normative reference: ETSI TS 103 420 V1.2.1 Table 32 states `0b1X` is
  reserved for `warp_mode`; no permitted public ETSI erratum changing this
  table was found.  Consequently strict behavior is unchanged and no vendor
  warp profile was added.

- Fixture is not committed to this repository. Stable label:
  `forever_friends` (external user-supplied fixture).
- Recorded SHA-256: `67c10f65642f11713f8495026a37cf26fd1f901e9a343d2e3acf5ee879584896`.
  Size: 32,138,978 bytes. FFprobe reports ISO BMFF with one MJPEG video stream
  (index 0) and one `eac3` audio stream (index 1), 48 kHz, six channels,
  `5.1(side)`, duration 248.736 seconds.
- Independent FFmpeg stream-copy artifact (temporary, not committed):
  31,838,208 bytes, SHA-256
  `2e155599e319d7a6f1ef655684bd872aaae1cd5f73d82097c589a32c572df86a`.
  OpenJOC container demux produced byte-equivalent elementary bytes.
- Current OpenJOC evidence: `inspect` accepts the container and reports 7,773
  E-AC-3 frames/access units at 1,536 samples each. Every frame has the
  §8.3 `addbsi` extension `[0x01, 0x10]`, while every TS 102 366 E.1.2.5
  frame-end `auxdatae` bit is zero. Bento4 `mp4dump` and the public BMFF
  structure show no second audio/metadata track or recognized JOC box. The
  grouped-mantissa and parse-only paths reach all six audio blocks; exactly one
  skip-field candidate per access unit is bounded and classified by the
  Annex H parser as a complete candidate container with payload IDs 11, 14, 2,
  and 1. TS 102 366 calls `skipfld` dummy data and TS 103 420 does not
  expressly assign it as a JOC carrier, so this result is diagnostic evidence,
  not a normative carriage conclusion. The ID-11 Table 56 configuration
  (`codecdatae=0`, `payload_frame_aligned=0`) is invalid under ETSI_STRICT, so
  the strict access-unit profile extractor returns no complete profile. If the CLI prints “JOC
  extension signaled ... EMDF profile absent”, that compatibility wording is
  bounded to profile validation and must be read with the carrier counts; it is
  not a claim that the complete stream contains no EMDF.
- Default FFmpeg base extraction produces six-channel 48 kHz f64 PCM
  (11,939,328 samples/channel). The current `--internal-base` command stops
  before base synthesis because no complete OAMD/JOC EMDF profile is accepted
  from the currently validated carriers; the earlier mantissa-code failure was
  corrected by the grouped-state increment. The
  FFmpeg-versus-internal-base comparison is therefore not available and
  internal-base fidelity remains unverified.
- FFmpeg `astats` records the `5.1(side)` order (FL, FR, FC, LFE, SL, SR) and
  dBFS peak/RMS pairs: FL `-14.066079/-29.027150`, FR `-11.644446/-27.419704`,
  FC `-3.850901/-21.360071`, LFE `-33.119901/-50.094647`, SL
  `-3.784534/-20.646557`, SR `-1.605351/-20.007338`. The internal decoder
  emitted no PCM, so delay, internal peak/RMS, and numerical error are not
  available and must not be treated as zero or equivalent.
- The real-vector lane must not be marked verified until nonzero JOC side
  information, dynamic OAMD, nonzero object PCM, moving-object continuity, and
  known-stem/ADM-BWF comparisons are demonstrated.

### Borrowed frame sink for CLI debug consumption

- Normative/engineering source: the renderer-independent frame boundary in
  ETSI TS 103 420 clause 6.4, together with OpenJOC engineering specification
  section 5.7's atomic frame-staging and bounded-retention requirements.
- Official reference data: no additional codec tables are used. The sink is
  an ownership/lifetime boundary around already decoded normative frame data;
  it does not alter JOC, OAMD, E-AC-3, or QMF behavior.
- Design rationale: `PayloadDecoder::decode_frame_with` commits the same
  checked frame state as `decode_frame`, then lends the single
  `DecodedPayloadFrame` to a callback. `openjoc-cli` uses the callback to write
  each debug directory immediately, while the former all-frame debug vector is
  no longer constructed by the E-AC-3 command. Callback failure is surfaced as
  an actionable sink error; because the decoder state is already committed,
  callers must use a transactional output directory when retry/rollback is
  required. Whole-input retention, compatible-base PCM retention, and
  accumulated renderer-independent scene PCM remain intentionally unsolved;
  metadata-only scene assembly and streaming PCM/file sinks are also open.
- Validation: `crates/openjoc-scene/tests/payload_decoder.rs` proves a
  borrowed callback observes the decoded frame while `finish` retains the
  correct timing. `crates/openjoc-cli/tests/decode_payload.rs` and
  `crates/openjoc-cli/tests/inspect.rs` remain green after the CLI switches to
  immediate frame debug writes. Full workspace format, strict clippy,
  all-feature tests, and offline release build are required before commit.

### Controlled Logic warp-study corpus (2026-08-05, private)

The corpus was created and exported through Logic Pro 12.3 on macOS 26.6
(25G72), using the existing deterministic 48 kHz sources and the same DD+
Atmos export profile. Nothing below is committed to Git. The immutable run
directory is `OpenJOC-Private/reports/runs/2026-08-05T004530Z_vector-corpus_1330681`.
The Logic project hash is the SHA-256 of sorted relative file paths and file
hashes inside each `.logicx` package.

| vector | Logic project hash | ADM SHA-256 | MP4 SHA-256 | stream-copied EC3 SHA-256 |
| --- | --- | --- | --- | --- |
| A static centre | `e883c6614fb2b46a62094a0576b2fa700c1e7d0fe8b689707eb49d39dc04af46` | `e1459458d64717bce300be910d49c076eeccbfe1a9d26a4f99728996dc8530c2` | `a9e7d9d05e8e993297d707d4a93e2cdf4ab389cd060bbb5a8d60ba7f0172f942` | `0c64900c76d213bd8f49066244702167f1dce20d55f605cb974987ce084fe82f` |
| B requested single jump (not canonical) | `7d703722ca39ca31ceac0c0822d66e197fb339be952c700fa36dd4eda50c95dd` | `87ccfa22de9e854e459cd725e050475e666e9b1d9a517e1404bd5eba256ed4df` | `360c267af5e9b82fde0e203b150f3b8a07e40011cf9b0b19d74aa9d494f297d7` | `7ed23a04628c62300a3cc4cee846a308077f8a9117e96366d2b018e6b3ec224` |
| C requested linear ramp (not canonical) | `0e5efc87c47d0c963627cc9d65525ef8c543bf34f73731cc1c69283fd483fe30` | `87ccfa22de9e854e459cd725e050475e666e9b1d9a517e1404bd5eba256ed4df` | `79323cea3d406b5bf688d45c3e90f1a10715dd853f3eacfbff54f68796d89652` | `7ed23a04628c62300a3cc4cee846a308077f8a9117e96366d2b018e6b3ec224` |
| D existing mixed motion | `e4e210ceb0e2915819a1300b5b1411cc33f9b138e450fb2f6fc355abff3d4b50` | `87ccfa22de9e854e459cd725e050475e666e9b1d9a517e1404bd5eba256ed4df` | `704545f313148412d019a8e7e739fccc0ead345ba7afb4b3b32199fde7b79af0` | `7ed23a04628c62300a3cc4cee846a308077f8a9117e96366d2b018e6b3ec224` |
| E no dynamic object | `ab82dedc165f7c2c0005e624598536e3ca915bb591e78540306ce833085adc3e` | `e0c76fd5136b50bf58f329926068dddd63f2ab24570163c0192f68d78f8cf3ec` | `e827b2bc7fac662d38a781834d2bc002c50188b629fbbc5d37298a0613d4c187` | `6aeeac15f30ee08e07df5c2084f6eda5794f084d93c97bd0ed6b7d3bc23853b5` |
| F two objects | `123895cd71becc66c9ebc7377b4387bd737ff5724fa7e64fc8a8e4466f740297` | `601dfa93653c98639ee3b223dce1ff173340bd498a6c1bf988f6d8839477466c` | `251b22ba273af6530f4c7abcb03f946606f80c74c8a7d3b7a45b61c5799dc14b` | `713f99eddfc8951c8aa712f8bd708e5bf2594ce7ae32ce06d6e7d1f1569955b3` |

All six exports produce 126 access units. A/E have one unique payload-11
body; B/C/D/F have 63. The first changed body is AU 15 (zero-based, 0.480 s)
for B/C/D/F. All vectors, including static A and no-dynamic-object E, have
warp distribution `raw=3` in 126/126 AUs. ADM object updates are A:1,
B/C/D:197, E:0, and F:197 per object (`OBJ_997HZ` and `OBJ_2003HZ`). B and C
were cloned exports whose ADM proves the existing D mixed motion remained;
they are retained as evidence but are not claimed to satisfy their requested
single-variable automation semantics.

The normalized raw-EC3/MP4 forensic sequences are byte-identical at the
EMDF/OAMD observation level for every vector. The extracted EC3 hash equals
the raw EC3 hash in each pair. This proves carrier-path equivalence for the
bounded observations, not complete OAMD/JOC decoding.

The independent oracle and direct byte mask report warp bits `[526,528)` with
raw value `3`; the formal ETSI parser still returns `ReservedWarpMode { raw: 3 }`.

Diagnostic assumptions 0, 1, and 2 each close bounded element/payload windows
but remain non-unique and do not produce update/position semantics. No official
ETSI erratum changing the reserved table was found in the permitted sources.

### 2026-08-05 Logic differential refresh

This evidence refresh began at Git HEAD
`952b052d61e23e5b7c7d96d37b41a01f090424b7` on
`codex/logic-warp-differential-corpus`. Computer Use was used to reopen Logic
Pro 12.3 and inspect the private canonical-B copy. An unsaved automation
experiment was discarded before exit; no private project, ADM BWF, MP4/EC3,
manifest, forensic report, census output, `.DS_Store`, or `references/` entry
was committed or overwritten.

The new private batch is
`OpenJOC-Private/reports/runs/2026-08-05T1042Z_logic-warp-evidence_952b052`.
It contains two carrier reports per A-F and one fresh ADM inventory report per
vector. Raw and MP4 observations normalize identically for all 126 AUs. The
payload-11 unique counts are A/E=1 and B/C/D/F=63; the first transition where
present is AU 14 -> 15, and `1536/48000 = 0.032` seconds per AU, so zero-based
AU 15 begins at sample 23,040 and 0.480 seconds.

The evidence package preserves the coordinate systems separately (raw file,
elementary stream, AU, bounded skip field, and EMDF payload). The independent
oracle and direct byte mask agree with the formal/diagnostic entry traces on
warp start 526, end 528, raw bits `11`, integer `3`, and payload end 536.
Diagnostic hypotheses 0/1/2 all close bounded syntax but remain non-unique and
do not yield timing, position, jump, or ramp counts. ADM reports are an
external oracle only: A=1 object update, B/C/D=197, E=0 object channels, and
F=197 updates for each of two objects. No unproven coordinate conversion or
fidelity statement is recorded.

This refresh therefore preserves the existing interoperability boundary:
`ETSI_STRICT` returns `ReservedWarpMode { code: 3 }`; no remap, vendor exception,
or `DOLBY_VENDOR_COMPAT` warp interpretation was added. B/C remain a UI-copy
limitation record rather than canonical single-variable vectors, and the next
required evidence is a genuinely canonical Logic jump/ramp export or an
additional authorized encoder/version.

### Bounded vendor opaque trim retention (2026-08-05)

The explicit `DOLBY_VENDOR_COMPAT` OAMD path now retains a complete declared
element-2 trim body as `OpaqueObservedKnownElement` only after payload ID 11,
element-1/element-2 bounds, and the formal first error
`ReservedWarpMode { code: 3 }` are verified. It preserves raw bits, declared
length, final-byte validity, SHA-256, raw warp/ranges, the original error, and
deviation `LOGIC_OAMD_RESERVED_TRIM_WARP_3`; it does not remap warp or create a
trim timeline. `ETSI_STRICT` remains unchanged and rejects the same value.

In private batch `2026-08-05T0358Z_vendor-opaque-2f5de17`, A/E/D raw and MP4
each show 126/126 strict warp failures and 126/126 opaque vendor acceptances.
Element 1 parses to 16 objects (15 dynamic), one metadata block and 16 object
updates. Payload 14 independently parses 126/126 with 15 output objects,
five-channel downmix, full matrices, 900 codewords per AU and nonzero
codewords. The first downstream boundary is explicit and unchanged by trim:
`JOC declares 15 objects but OAMD declares 16`. No object PCM, complete OAMD
timeline, or fidelity claim is made.

The follow-up private B/C/F raw+MP4 regression run
`2026-08-05T_vendor-opaque-bcf.Y072QA` reports the same 126/126 strict
`trim.warp_mode` boundary, 126/126 vendor opaque acceptances, raw warp `3`
distribution, and 126/126 formal payload-14 parses. Raw/MP4 normalized
observation sequences are equal. B/C remain the previously documented
non-canonical Logic copies; this run does not promote their automation labels
to semantic ground truth.

### Controlled programme cardinality and first PCM boundary (2026-08-05)

The prior object-count error was a cross-chain validation error, not evidence
that a JOC row was missing. The OAMD content-description expansion now derives
typed bindings from the actual anchor sequence. In every private A-F vector the
sequence is:

```text
OAMD[0]     Speaker(RcLfe) -> BaseLfe(channel 0)
OAMD[1..15] Dynamic         -> JOC rows 0..14
```

The resulting cardinalities are `total_oamd_count=16`,
`speaker_anchored_count=1`, `bed_count=0`, `lfe_count=1`, `isf_count=0`,
`dynamic_slot_count=15`, and `joc_output_count=15`. The `addbsi` complexity
index remains checked against total OAMD count 16. A normal bed, ISF,
multiple LFE entries, unexpected LFE order, missing LFE PCM, unequal frame
length, duplicate row, and JOC/dynamic-slot mismatch each have explicit
typed failures; there is no `count - 1` compatibility branch.

The non-LFE compatible-base path is an explicit five-channel
`FL,FR,FC,SL,SR` f64 WAV. A separate FFmpeg `pan=mono|c0=LFE` f64 WAV is
retained and passed only to the scene boundary. It is not sent to QMF/JOC and
is not synthesized when unavailable. A/E/D/F vendor-compatible decodes now
produce 16 scene entries (entry 0 class `lfe`, entries 1-15 class `dynamic`)
over 126 AUs / 193,536 samples, with 15 dynamic JOC signals and one
base-carried LFE signal. The tested LFE source is silent, so its measured
peak/RMS are zero; the source and zero result are both reported rather than
silently fabricating an audio stem.

The OAMD `active` field, not ADM and not PCM energy, is the activity oracle.
All 15 dynamic slots are active according to the observed OAMD updates in
A/E/D/F. E's ADM BWF has zero dynamic objects while its codec still exposes
the same 15 coded slots and nonzero-capable JOC rows; this is an observed
capacity/content distinction, not a claim that those slots are ADM objects.
F's two ADM names (`OBJ_997HZ`, `OBJ_2003HZ`) are retained as a partial
comparison only; measured frequency energy is distributed across several JOC
rows and does not uniquely prove row-to-name identity.

The reproducible private evidence package is
`OpenJOC-Private/reports/runs/2026-08-05T044606Z_object-cardinality_a4f88af_r3`.
Its `programme_layout`, `oamd_joc_bindings`, `base_lfe_inventory`,
`joc_row_metrics`, `object_pcm_metrics`, `scene_inventory`,
`adm_partial_comparison`, and `strict_vs_vendor` JSON/TXT pairs were written
twice under `reports3` and `reports_repeat3`; every pair compares
byte-for-byte. The package does not contain repository media and is not
committed.

This is a vendor-compatible reconstruction boundary, not full Atmos support:
`ETSI_STRICT` still fails raw warp 3; trim/warp semantics and a complete OAMD
timeline remain unresolved; no object identity, ADM position equivalence,
speaker render, or `--internal-base` fidelity claim is made.

## Internal-base fidelity run (2026-08-05)

The first numerical base comparison is private and non-overwriting:
`OpenJOC-Private/reports/runs/2026-08-05T053438Z_internal-base-fidelity_dcfb56c`.
It uses raw EC-3 for A static-centre, E no-dynamic-object, D existing mixed
motion, and F two-objects. MP4 and ADM files are retained by SHA-256 in the
report; decoding uses raw EC-3 because the prior MP4 path has a known
carrier/duration distinction.

FFmpeg 8.1.2 is invoked as a black-box compatible-base reference with
`-map 0:a:0`, `-c:a pcm_f64le`, no resampling, and explicit `pan` filters:
six-channel `FL,FR,FC,LFE,SL,SR`, five-channel JOC input
`FL,FR,FC,SL,SR`, and mono `LFE`. FFmpeg dialnorm/dynrng presentation defaults
are recorded and not silently made equal to OpenJOC. OpenJOC exports matching
reference-f64 `internal_base_full.wav`, `internal_base_joc_input.wav`, and
`internal_base_lfe.wav`; TDAC state remains stateful across AUs and
deterministic dither is used.

All vectors are 126 AUs / 193,536 samples; the chosen delay is 0 and the first
non-LFE differences occur in AU 0/block 0 (FL=9, FR=9, FC=7, SL=12, SR=5).
Raw front/centre SNR is roughly 84.5--90.6 dB; raw side SNR is roughly
38.8--51.3 dB. LFE is present and exactly silent in both paths. These values
show a measurable base-decoder difference, not a pass/fail fidelity result;
the run intentionally does not normalize gain or hide DRC/dialnorm policy.

The unchanged JOC payload and state are compared again under both bases. Per
object row, AU, block, LFE, and frequency evidence is present. The F ADM names
`OBJ_997HZ` and `OBJ_2003HZ` are not promoted to row identities because the
measured energy is distributed across codec rows; E remains a codec-capacity
negative control. `reports1` and `reports2` are byte-identical. No warp=3
interpretation, strict relaxation, complete OAMD timeline, ADM position/trim
fidelity, speaker render, or nonzero-PCM fidelity claim is made.

## Round-7 base-root-cause policy audit (2026-08-05)

The private, non-overwriting run
`OpenJOC-Private/reports/runs/2026-08-05T070007Z_base-root-cause_792d937`
records local FFmpeg 8.1.2 decoder help, build configuration, version, and
the complete R0--R6 command lines. R0 is the implicit default; R1 is
`drc_scale=0`; R2 is `drc_scale=1`; R3/R4 are `cons_noisegen=0/1`; R5 is
`heavy_compr=0`; and R6 is `target_level=0`. R0/R2/R3/R5/R6 are byte-identical
for A/E/D/F. R4 changes all four outputs; R1 changes only E. No FFmpeg option
is treated as a normative oracle.

The internal decoder now exposes an explicit `InternalBasePolicy` boundary.
`CurrentDefault` is the unchanged default CLI behavior. `CodecCore` leaves
syntax parsing, dequantization, coupling, SPX, dither, and TDAC untouched and
sets only optional `dynrng/dynrng2` presentation gain to unity. On A/D/F the
two internal policies are byte-identical; on E the policy change is isolated
to the signaled dynamic-range fields, and `CodecCore` is close to but not
identical with FFmpeg R1. This is a policy distinction, not a fidelity pass.

The first large residual is state-local, not a fixed gain: A/E/D/F all show a
common SL/SR event at block 6 (sample 1536), immediately after the first
1,536-sample syncframe. A private diagnostic probe that resets
`AudioPcmSynthesizer` only before frame/AU 1 reduces that event from roughly
`7e-3` RMS to `1e-7`--`1e-6` RMS and raises side-channel SNR into the mid/high
80 dB range. The probe is not a production change. ETSI TS 102 366 V1.4.1
clause 6.9.4 explicitly overlaps the first half of each windowed block with
the second half of the previous block, so the evidence does not justify
silently resetting TDAC at an AU boundary. The remaining explanation is a
decoder/encoder boundary or priming convention not established by the public
stream syntax. No state reset, FFmpeg algorithm, per-channel gain, or channel
remap was added.

The private stage inventory is opt-in and records the bounded stages exposed
by the current API (exponents, BAP, dequantized coefficients, pre-IMDCT/window,
and overlap/add). Coupling-vs-SPX-vs-rematrix sub-stages are marked unavailable
rather than inferred. `ETSI_STRICT` still rejects OAMD `warp=3`; no vendor warp
rule or semantic remap was introduced.

## TDAC contribution trace and boundary evidence (2026-08-05)

The private, non-overwriting run
`OpenJOC-Private/reports/runs/2026-08-05T_tdac-boundary-corrected_054d3d4`
(repeated in `..._repeat`) uses the opt-in
`AudioPcmSynthesizer::synthesize_with_trace`
sink. It records the bounded transform/window and overlap components without
changing the production path. The core JSON/TXT diagnostics and production
regression hash report compare byte-for-byte across the repeated run; the
private PCM tree used for that hash comparison remains in the earlier
`2026-08-05T_tdac-boundary_054d3d4/internal_rerun` directory. The run
does not modify repository `.DS_Store`, `references/`, private Logic projects,
ADM BWF, MP4/EC3, manifests, or earlier forensic/census output.

The public reference is ETSI TS 102 366 V1.4.1: clauses 5.2.11, 6.9.3 and
6.9.4 (PDF pages 51, 82--85) specify the windowed-block overlap equation and
the delay update; Table 6.33 is on PDF page 86. The trace reports the explicit
identity `output_sum = carry_in + current_windowed_head`, `output = 2 *
output_sum`, and `carry_out = current_windowed_tail`. Floating-point
representation and diagnostic hashes are implementation choices, not ETSI
requirements.

`AudioPcmSynthesizer` owns one 256-sample delay vector per full-bandwidth
codec channel and a separate LFE vector. Calls stage cloned histories and
commit only after every block succeeds. `JocAccessUnitPcmDecoder` keeps
independent and dependent synthesizers separate. A synthetic 12-block stream
and a 6+6 framed partition produce identical PCM and final carry, and the
real A/E/D/F run reports exact carry-out/carry-in equality at all 125 AU
boundaries for all five full-band codec channels. This rules out a lost carry, retry double
advance, AU vector rebuild, or channel-state commit bug.

The trace order is the E-AC-3 syntax order `L,C,R,Ls,Rs`; the six-channel
FFmpeg reference order is `FL,FR,FC,LFE,SL,SR`, so reference indices are
mapped as `[L,R,C,Ls,Rs]` after excluding LFE. At AU0 block5 -> AU1 block0,
current-windowed heads for Ls/Rs reproduce the
FFmpeg-compatible reference when the stored carry is omitted (zero-carry RMS
`1.2581e-7` and `1.2454e-7`), while the normal continuous output has RMS
`0.0075718936` and `0.0073475530`. The stored carry is exact and channel-local
but has near-zero correlation with the black-box inferred reference component
(`0.0347`/`0.0349`, with scalar gain near zero). The same run records FFmpeg's
continuous-vs-isolated AU1 probe as an implementation observation only; it is
not treated as normative ETSI evidence. The remaining classification is
`carry generation / upstream block-5 tail versus external frame-boundary
policy`, unresolved. No production AU reset, remap, gain, or hidden
compatibility mode is permitted by this evidence.

As a non-fix regression check, A/E/D/F `CurrentDefault` internal-base full WAVs
from this increment are byte-identical to the prior
`2026-08-05T070007Z_base-root-cause_792d937` outputs. JOC propagation was not
rerun as a post-fix comparison because no production TDAC correction was
admitted; the earlier object-row measurements remain non-fidelity evidence.

## Independent TDAC oracle and pre-roll controls (2026-08-05)

The non-overwriting private run `2026-08-05T_tdac-oracle-preroll_b18ea4d_r4`
contains the joint decision, synthetic oracle, real AU0/block5 replay, and
Logic virtual-crop metrics. The companion run
`2026-08-05T_tdac-oracle-preroll_b18ea4d_r5` contains the P0/P1/P2/P4 base-only
pre-roll vectors and their inventories. The oracle is a read-only Python
implementation using only standard-library float64 arithmetic and literal
ETSI window/IMDCT equations. It has an independent cursor, state, transform,
window, and overlap implementation; it does not call OpenJOC, FFmpeg, or any
production helper.

The oracle reports 53 synthetic comparisons with no material divergence at
`1e-12`; the continuous 12-block and 6+6 partitioned runs are exactly equal.
The real AU0 block5 replay agrees with production carry tails to at most
`5.12e-17` and AU1 block0 heads to at most `2.00e-15`. These are TDAC
arithmetic checks, not whole-decoder fidelity checks.

P0/P1/P2/P4 use identical active content with 0/1/2/4 silent-AU prefixes.
The active-content SHA-256 is
`921b0e6c84bf798936998279948d2af40e2139e3367ebfea761e57a70cb9518e`.
FFmpeg raw and MP4 decoded PCM are sample-identical in all four controls, and
the controls do not reproduce Logic's approximately `7e-3` Ls/Rs first
boundary event. Cropping the first two Logic AUs is retained only as a
diagnostic observation; it is not production trimming or a compatibility
semantic. Therefore no TDAC correction is justified yet. The remaining
question is AU0/block5 Ls/Rs coefficient provenance or a Logic-specific
upstream/stream-feature boundary, to be tested without changing continuous
TDAC.

## Logic AU0/block5 provenance and controlled pre-roll corpus (2026-08-05)

The non-overwriting private run
`OpenJOC-Private/reports/runs/2026-08-05T125009Z_logic-first-block-provenance_77116e9`
is diagnostic-only. Its starting repository head was
`b18ea4d8dc5a72bc00bbb179cf8484f6291b9211`; no production TDAC, AU-boundary
reset, gain, remap, warp remap, or decoder compatibility branch was added.

Apple's system path is available on this Mac. `afconvert` decodes the Logic
MP4, and `afinfo` reports the six-channel order `L,C,R,Ls,Rs,LFE`, 48 kHz,
1,536 frames/packet. The comparator maps only that declared order to the
canonical OpenJOC order `FL,FR,FC,LFE,SL,SR` using indices `[0,2,1,5,3,4]`.
It does not infer a channel permutation from content or FFT peaks. The run
keeps Apple, FFmpeg MP4, FFmpeg raw EC-3, and OpenJOC metrics separate for
startup, the unaligned AU-1 window, steady state, and a diagnostic selected
delay.

The AU0/block5 probe records the actual upstream tool state before TDAC. In
the target Ls/Rs blocks there is no observed coupling, SPX, rematrix, or AHT;
`BAP=0` bins are classified as dither/noise only when the transmitted dither
flag is set. The first block's exponent strategy/BAP state differs from later
blocks. Exact later-block matches are therefore sparse; relaxed matches that
exclude exponent strategy are explicitly labelled relaxed and are not decoder
semantics. Carry storage remains continuous at every observed AU boundary.

The Logic Pro pre-roll study uses four new copies of the same single-jump
project and source/export profile. Only exact source pre-roll changes: LE0,
LE1, LE2, and LE4 prepend 0, 1, 2, and 4 access units (0, 1,536, 3,072, and
6,144 samples) and retain a four-second selected export. Each vector has 126
access units. The private four-second hashes are:

| vector | MP4 SHA-256 | stream-copy EC3 SHA-256 | ADM BWF SHA-256 |
| --- | --- | --- | --- |
| LE0 | `6a61a841bad73adcd2a9d8e2af3453e2bee52269e4dbd1aeb47d6fa03ffbb0a5` | `918e3ff1aa644d6f31a895f19da4ddfa391774899fd65c6417e6e1ea0e5b24a8` | `407341b2fa3177c7e560e794ca352daa76c149141eccf65c400e4ae453aff69e` |
| LE1 | `bc7d211b9a39772937ce434f99ec3d1d05cbed7fe60ba6cd60f2ab00c09355aa` | `6371b408b9afea4b614a8661325d3e0e0f8449a237dc599d45ebe30930d60986` | `663fb7c080cc6ce2caeeb4c0ae9f1e32413631f4e1cb6d841ec6126c57955cf2` |
| LE2 | `aacfd114cb734b7a25699b4a4079a3b15ef354a3ec3b5f4b70e32b6520c130d8` | `c7b0d4ff323a98eab5480db2c8ef07c87edc062d5c35e0dd674d98993b1903e6` | `e4a0c30ec64034143df65337cebd7bfb3208ae5cab67521dfb3327533262ff7d` |
| LE4 | `0c771d81236910bc054f78331a43f21bbb4460bf3898a083654b9c7fa7980e72` | `44a6b4c0167e18547f090886e44ca80289ae80abf2bb0cb890fc90941a6b2293` | `a51c0d31f01d9ce201972ef779e7504ddf82211db440f5a8e1efae70aa1e69d3` |

For every pre-roll vector, raw EC-3 and MP4 have 126 paired observations,
zero payload-11 body mismatches, one unique payload-11 body, and warp
distribution `{raw: 3, count: 126}`. `ETSI_STRICT` fails all 126 observations
with `reserved OAMD warp mode 3`; `DOLBY_VENDOR_COMPAT` accepts all 126 with
the trim element retained as opaque/unresolved. The independent bit oracle,
diagnostic parser, production parser trace, and direct byte mask all report
the same payload-relative warp span `[526,528)` and raw value `3`.

The three diagnostic hypotheses (`assumed_semantics` 0, 1, and 2) all close
the bounded 8-bit element and payload, but all explicitly report “semantic
hypothesis is non-unique and diagnostic-only”; none reaches normative object
element decoding, timing correspondence, or ADM movement semantics. This is
not evidence that raw 3 aliases any legal warp mode.

The ADM BWF inventory for each LE export contains 192,000 samples, 48 kHz,
two ADM objects (`Master`, `OBJ_997HZ`), one object channel, and one static
four-second object block at `X=-0.0,Y=1.0`. The inventory verifies
carrier/project duration and object identity for this pre-roll control, but it
does not provide a moving-position timeline for these copies. No ADM position
or fidelity claim is inferred from the OAMD hypothesis reports.

At that 2026-08-05 round, the first remaining blocker was the AU0/block5 Ls/Rs pre-IMDCT coefficient
provenance and internal-base fidelity. The private tail backprojection is
ill-conditioned (condition estimate about `8.42e6`) and explicitly
non-unique; its dominant low-bin list is not assigned to a decoder tool. A
complete OAMD timeline, JOC semantic reconstruction, non-zero object PCM
fidelity, ADM positional comparison, or accepted internal-base fidelity is not
claimed.

## Exact target-AU decoder-history corpus (2026-08-05)

The non-overwriting private evidence package is
`2026-08-05T_exact-au-history_e73ef3f_r7`; a second complete OpenJOC harness
run in `_r8` is byte-identical for all deterministic evidence files. The
source is the existing LE0 raw EC-3 vector, not a new Logic re-export. It has
126 indexed access units, 48 kHz, 1,536 samples/AU, and 3,072-byte independent
frames. OpenJOC's indexed parser, rather than syncword search, establishes:

| target | source byte range | byte length | SHA-256 |
| --- | ---: | ---: | --- |
| AU0 | `[0,3072)` | 3072 | `05712ff5440003dfefdf7d203599dcbd97485191b77c58ca25da71c8e3d37856` |
| AU1 | `[3072,6144)` | 3072 | `578f26ca19b5948d40e112fa8e3436e63f95dad4b30a1e377f1924fdc6de94c5` |
| AU0+AU1 | `[0,6144)` | 6144 | `0a14861a3ddcc5539b18fd3a5593ecc4d1355a37ec9c0c73d6125225aa57b991` |

H0 is the original source. H1/H2/H4 prepend one, two, or four exact AU0
copies. HP prepends exact AU0+AU1. The target occurrence is therefore
manifest-mapped to corpus AU indices 0/1, 1/2, 2/3, 4/5, and 2/3
respectively; all target AU0 and AU1 bytes are directly identical across the
five histories. The duplicated streams are explicitly diagnostic byte-history
corpora, not normative programme acceptance vectors.

All five raw streams were remuxed to MP4 without re-encoding. MP4-to-EC3
roundtrip bytes are identical; frame counts are H0=126, H1=127, H2=128,
H4=130, HP=128. No metadata or audio bytes were normalized to make a muxer
accept a stream.

### OpenJOC exact-history result

The diagnostic replay uses the existing parser, `InternalBasePolicy::CodecCore`,
and `AudioPcmSynthesizer::synthesize_with_trace`. For identical target AU
bytes, parsed headers, exponent/BAP state, exposed pre-IMDCT coefficient
hashes, and AU0/block5 Ls/Rs TDAC tail hashes are equal in all histories. For
H1/H2/H4/HP target AU0, the first exposed difference is
`block0_channel3_tdac_carry_in`; this is a TDAC-context/PCM boundary effect,
not a coefficient difference. Target AU1's exposed stages and PCM are stable
after the target AU0 has run.

The snapshot check clones `AudioPcmSynthesizer` immediately before target AU0
and obtains equal stage counts, carry arrays, and PCM. The production staged
commit behavior remains the failure-atomicity boundary. The opt-in diagnostic
trace records raw mantissa tokens, grouped positions, dither values,
dequantized mantissas, and final pre-IMDCT arrays from the same production
cursor; normal decoding allocates no trace. Component transplant is likewise
reported as not performed because those production state components are not
public. No production state API was added.

### Black-box history matrix

| decoder | H0→H1 AU0 | H0→H2 AU0 | H0→H4 AU0 | H0→HP AU0 | target AU1 |
| --- | --- | --- | --- | --- | --- |
| OpenJOC exposed stages | stable | stable | stable | stable | stable |
| OpenJOC PCM | changes at block-0 carry boundary | changes at block-0 carry boundary | changes at block-0 carry boundary | changes at block-0 carry boundary | stable |
| FFmpeg raw | changes, strongest in Ls/Rs | changes, strongest in Ls/Rs | changes, strongest in Ls/Rs | changes | changes at measured output coordinates |
| Apple `afconvert` | stable | stable | stable | stable | stable |

This matrix is an output comparison only. It does not claim visibility into
FFmpeg or Apple internal state. The joint decision is narrowed to: OpenJOC's
exposed target coefficients are history-stable; OpenJOC's first PCM difference
is the expected retained TDAC carry context; FFmpeg exhibits a history-
dependent black-box output; Apple is stable for the same remuxed histories.
No codec-core fix is justified, and strict/vendor OAMD behavior is unchanged.

The remaining blocker is to separate comparator startup/priming coordinates
from Logic AU0/block5 upstream coefficient provenance. Complete OAMD timeline,
JOC semantic reconstruction, object PCM fidelity, ADM position comparison, and
accepted internal-base fidelity remain unclaimed.

## Decoder comparison contract (2026-08-06)

The private evaluation-only package
`2026-08-05T_decoder-comparison-contract_01936ed_r8` consumes the complete
exact-history outputs and prior A/E/D/F three-decoder metrics. It does not
trim or rewrite any decoder WAV; `_r9` is a byte-identical deterministic
repeat of the core JSON/TXT evidence.

The contract defines cold start `[0,1536)`, warm-up `[1536,3072)`, and a
decoder-specific steady-state suffix. OpenJOC reaches observed history
convergence at source AU1: AU0 carry-in is the first difference, while AU1
stages and PCM are stable. A complete decoder-state hash is not exposed, so
this is scoped corpus evidence, not a universal state theorem. Apple is stable
from target AU0 in the observed 1536-sample grid, with 288 trailing samples
absent and PTS unavailable. FFmpeg has no PCM convergence suffix through AU8;
its mapping is manifest/grid based with PTS unavailable.

Sample 1536 is classified as a warm-up/startup comparator boundary. Its
cross-decoder semantic alignment is unproven and it is not retained as a
demonstrated TDAC defect. Steady-state A/E/D/F metrics are reported without
an acceptance threshold. JOC object WAVs remain complete; evaluation slicing
is report-only and object semantic identity remains open.

## Steady-state coding-tool differential (2026-08-06)

The private package `2026-08-06T_steady-state-tool-differential_b62168f` and
deterministic repeat `_r2` compare Logic vectors A/E/D/F on fixed windows S1
(AU2–15), S2 (AU32–63), and S3 (AU80–110). OpenJOC and FFmpeg have a
high-confidence 1536-sample AU mapping; Apple is medium confidence with 288
trailing samples absent and PTS unavailable. External 256-sample block
alignment is not proven, so block metrics are diagnostic only.

OpenJOC versus FFmpeg has median per-channel block RMS residuals near
`0.98e-6`; Apple versus either is around `1e-5` under the same diagnostic grid,
confounded by the unproven block anchor and Apple tail. The representative tool
inventory has no independent per-AU/per-block on/off strata, so no causal
association with coupling, SPX, dither, rematrix, AHT, or exponent strategy is
established. LFE exact silence is excluded. JOC propagation remains
evaluation-only (15 rows, complete object WAVs); semantic object identity is
open. No production codec, TDAC, trim, warp, or compatibility behavior changed.

## Block-anchor and parser tool inventory (2026-08-06)

Private packages `2026-08-06T_block-anchor-tool-inventory_8d38331_r5` and
`_r6` are deterministic repeats. `diagnose-tools` obtains records from the
same parsed prefix, expanded BAP arrays, exponent state, coupling/SPX,
rematrix/AHT and dither state used by decoding; it does not infer labels from
PCM or reparse the bitstream. Failed AUs do not commit partial inventories.

A/E/D/F each contain 126 AU, six blocks, five full-band channels and an
independent LFE record: 4536 records per vector, zero failed AUs. Current
incidence is observational: coupling/SPX/AHT are off, dither is predominantly
on, and exponent reuse is present. No randomized single-tool stratum exists,
so individual effects are not identifiable.

The deterministic 48 kHz 5.1 marker source and independent detector recover
all 480 source full-band blocks at high confidence with exact offsets. This
validates source coordinates only. The subsequent controlled G9 Logic
carrier was decoded through OpenJOC CurrentDefault, OpenJOC CodecCore, FFmpeg
raw/MP4, and an Apple diagnostic path. The source identity and 480/480 source
gate passed, while each required external path recovered 461/480 blocks; all
19 residuals were margin-only near-neighbor ambiguities. External mapping and
anchored residual/tool effects remain unproven and are not upgraded.

## Controlled Logic full-band block-anchor study (2026-08-09)

The deterministic G1--G9 block-anchor research line used a controlled Logic
Dolby Digital Plus Atmos carrier and the four required decode paths (OpenJOC
CurrentDefault, OpenJOC CodecCore, FFmpeg raw EC-3, and FFmpeg MP4), with an
Apple path retained as diagnostic comparison. G9's source semantic identity
passed with permutation margin `0.2461121871`; source detection recovered
480/480 full-band blocks with minimum score `0.6907968032`, minimum
best/second margin `0.3029205927`, and zero jitter. Energy, spectral, and
guard gates also passed, so the source fixture is not the remaining blocker.

The required external paths each recovered `461/480`. Their 19 residuals
were all frozen best/second localization-margin failures; there were no score
or jitter failures. The competing-peak offsets were identical across all four
paths: `-1` sample eight times and `-2` samples eleven times relative to the
source-correct peak. Phase-A full-curve analysis found a stable local
near-neighbor ambiguity and highly consistent OpenJOC/FFmpeg curves, but the
predeclared empirical model class `M2_asymmetric_local_smoothing` failed
cross-validation (minimum Spearman `-0.20040975`; excluding all 19 failures,
classification accuracy `0`). This is diagnostic evidence for the controlled
carrier, not proof of an encoder kernel or universal E-AC-3 behavior.

The final evidence boundary is therefore
`external_block_mapping_established = false`. The 256-sample external block
mapping, anchored coding-tool attribution, and anchored residual effects are
not established. G10 marker construction was not started. No Logic/media
artifacts or private reports are part of this repository provenance entry.

## J1R7A — Spec-anchored normative OAMD position-field milestone (2026-08-09)

This documentation-only milestone records the evidence boundary reached after
the J1 Logic authoring, ADM, carrier, differential, and reconciliation rounds.
The private source freeze is
`20260809T180109Z_j1r7a-spec-anchored-oamd_b6eb1de`; its
`j1r7a_spec_cursor_evidence_freeze.json` SHA-256 is
`572209bcb35cf2b37a512df1c9523b1a8762a2672445f96e57ad48a09257ba4f`.
The J1R6B declared raw-evidence freeze is
`cca9196dcf1f53839b42fbcfa2031c21a81392aa71d0df0e4952c6d286110332`.
The freeze records the prior J1R6C calibration artifact by its actual
SHA-256 (`344b3495c441703c81fa36bb1eb615fdea6bb0ff40123f7dc9864f9bc5705a72`),
the J1R6C-R reconciliation freeze (`82862be433296c86e179bc943c60c040ec8adee092729f38440de226ee216f32`),
and the J1R6D semantic-hypothesis freeze
(`03073c171afea3c78558c0948bf5fc1948772e125d69b5a5bcdbfcd6e1d023a4`).
The J1R6D predeclared protocol hash is
`0713c39a6de066a49a02ab4038dd306ce6c150a4c1d8964ce42064f148edc6f9`.

The normative cursor starts at payload-11 bit 0 and consumes only syntax
authorized by ETSI TS 103 420 V1.2.1. Seven frozen, independently authored and
ADM-qualified sources × 129 access units yielded 903/903 identical bounded
observations. The fully accepted normative prefix ends at payload-relative bit
526 (last accepted bit 525). Two exact spatial fields are therefore identified
without consulting the later trim value:

| field | payload-relative span | evidence |
| --- | --- | --- |
| `pos3D_X` | `[52,58)` | exact six-bit field; controlled X values `-1,-.5,0,+.5,+1` decode as `0,16,31,46,62` |
| `pos3D_Y` | `[58,64)` | exact six-bit field; controlled Y values `+1,0,-1` decode as `0,31,62` |

The earlier J1R6C/J1R6B five-bit `[58,63)` Front/Back summary is retained as
history only: it is the first five bits of the full Y field, not a production
field definition. J1R6C-R is the report-integrity correction; no source bits
or carrier bytes were changed.

The first normative ambiguity is trim `warp_mode` at `[526,528)`, raw bits
`11`, integer `3`. Table 32 marks `0b1X` reserved. Therefore
`ETSI_STRICT` still returns `ReservedWarpMode { raw: 3 }`, while the existing
`DOLBY_VENDOR_COMPAT` profile remains unchanged and adds no warp rule. J1R6D's
H0/H1/H2 branches are diagnostic labels over the same cursor, not semantic
decoders: all 903 observations closed identically, so no hypothesis was
selected.

This closes only normative prefix/field identity and controlled ADM numeric
alignment. It does not close complete OAMD trim or timeline/state semantics,
authored-object ↔ OAMD-slot identity, OAMD ↔ JOC binding, ObjectScene fidelity,
object PCM, ADM/render fidelity, or end-to-end acceptance. The next proposed
line is J1R7B, empirical characterization of the reserved warp-3 boundary;
it is not executed here. Historical statements in earlier dated sections are
preserved as historical observations and are superseded for the current
position-field boundary by this section.

## J1R8 — Controlled Z/elevation numeric calibration (2026-08-10)

This documentation-only milestone records one new, independently authored
Logic fixture, `J1R8_Z_CAL_997.logicx`, derived from the frozen Center
control. The private evidence run is
`20260810T032631Z_j1r8-z-elevation-calibration_c90779b`; its evidence-freeze
aggregate SHA-256 is
`faeaf08c88f2aa8d241262de6edf6ab60e35ccdd959fa91239f6640f94779c8a`.
The fixture used the Logic automation parameter `对象位置提升` with persisted
values `0 → 50 → 100 → 0`. The automation lane was the authoring source of
truth; the Object Panner was used only for independent timeline readback.

ADM independently qualified the same object (`AO_100B` / `J1_OBJ_TAG`) with
X = `-0.0` and Y = `+1.0` throughout, and Z states of baseline, approximately
`+0.5`, `+1.0`, and return to baseline. The exact ETSI normative Z fields
identified in the J1R7A ledger are `pos3D_Z_sign_bits [64,65)` (one bit) and
`pos3D_Z_bits [65,69)` (four-bit magnitude), ETSI TS 103 420 V1.2.1,
clauses 5.5.8–5.5.11 and 5.6.1.1.7–5.6.1.1.9. The observed magnitude-code
sequence was `0,3,6,7,13,14,15,10,3,1,0`; this is controlled numeric
alignment evidence, not a newly asserted formula. X/Y coordinate fields
remained invariant. A separate raw prefix interval `[177,182)` also changed
in the exploratory diff, but no semantics are assigned to it.

The source PCM remained sample-identical to the frozen Center source, and the
two unchanged-project DD+ exports were deterministic after stream-copy:
129 access units × 3072 bytes, identical raw EC3 SHA-256
`714060cf8f2a55d5db6464cbde08e3cd342e4392806d6ff30f3ef52098bc3b84`.
The observed `warp_mode [526,528) = raw 3` remains an ETSI reserved value;
`ETSI_STRICT` still returns `ReservedWarpMode { raw: 3 }`, and
`DOLBY_VENDOR_COMPAT` has no new rule. The empirical suffix `[528,536)` was
`00000000` in all 129 AUs; this does not establish padding or any other
semantics.

The Size branch remains frozen: authoring persistence and ADM propagation are
established, but tested DD+ Size-state semantics, a direct `object_size_idx`
response, and Size-related warp/suffix response remain unresolved/not
observed. Complete OAMD timeline/state semantics, reserved-warp meaning,
OAMD↔JOC binding, verified object PCM, ObjectScene/render fidelity, and
end-to-end acceptance remain open. No production code, parser/profile,
fixture corpus outside this one Logic project, JOC, or ObjectScene behavior
was changed.

## J1R9 — Dual-object multi-tone identity-binding boundary (2026-08-10)

This documentation-only milestone records the sole qualified dual-object
Front-Left/Front-Right swap in the private run
`20260810T104057Z_j1r9-dual-object-multitone-identity_6492301`. Its
two-run evidence freeze is `j1r9_evidence_freeze.json`, aggregate SHA-256
`d9611198677caf2f0d6c56aacc4b2fe70843f8fc7a9489546b9658e697045863`.
No private media, ADM, EC3, manifest, or forensic report is tracked here.

The persisted Logic project contains exactly two independently controlled
objects. ADM establishes that `OBJ_997HZ` moves Front-Left → Front-Right and
`OBJ_2003HZ` moves Front-Right → Front-Left. The source gate proves the
997 Hz and 2003 Hz PCM sources retain distinct identities; the four-second
ADM programme range is qualified. Its recorded transition trajectory has
nonzero Z during the transition, so analysis uses predeclared stable pre- and
post-transition windows rather than normalizing that behaviour. Two
unchanged-project DD+ exports stream-copy to the identical raw EC3 carrier
SHA-256 `d35aee5421e965d2fa0eb80d4b6dd071ba719dcd12686a40bf8a87cacfdc452e`.

The OAMD observation is deliberately limited to Element 1 data before the
known unresolved trim boundary. Slot 0 retains the controlled Front-Left
comparison tuple and slot 3 the Front-Right tuple in both stable windows. The
full authored-object-to-OAMD-slot mapping is not established: the observed
Front-Left → Front-Right trajectory is slot 9, whose paired JOC row is silent
in the stable windows. Element 2 remains opaque; raw `warp_mode [526,528) =
3` remains ETSI-reserved and `[528,536)` remains all zero. No warp or trim
meaning and no vendor rule is inferred.

Using only diagnostic pre-render reconstruction rows, the same two stable
slots show a decisive frequency-identity exchange: row 0 (paired with the
Front-Left slot) changes from 997 Hz to 2003 Hz, and row 3 (paired with the
Front-Right slot) changes from 2003 Hz to 997 Hz. ADM establishes the
opposite authored-object trajectories. The narrow supported decision is
`ONE_ROW_PER_AUTHORED_OBJECT_MODEL_REJECTED`; the complementary scoped
observation is `SPATIAL_ANCHORED_JOC_STRUCTURE_GAINS_SUPPORT`.

This is not a universal spatial-basis model, a complete OAMD/JOC binding,
ObjectScene correctness, renderer fidelity, object PCM fidelity, or a
resolution of raw warp 3. No production parser, profile, decoder, renderer,
or semantic-admission behaviour changed. The next work is an explicit,
testable spatial-basis binding model using the existing qualified corpus.
## J1R12 — Evidence-bounded reconstruction-basis architecture (2026-08-10)

J1R9/J1R10/J1R11 are frozen as the current Logic campaign boundary. J1R9
rejects one-row-per-authored-object semantics; J1R10 leaves the spatial basis
underdetermined; J1R11 changes application-level Logic track order without
changing raw EC3 bytes or the observed OAMD slot trajectories. The narrow
blocker remains: no independently controllable producer-side variable has
been shown to change OAMD dynamic-slot assignment while authored identity,
trajectory, and multi-object context are fixed.

The production model now separates three evidence domains:

```text
OAMD metadata objects/state -> metadata-only ObjectScene
JOC payload               -> ReconstructionBasis rows / diagnostic PCM
semantic audio binding   -> SemanticBindingState::Unresolved
```

`ReconstructionBasis` rows contain structural indices only; they do not carry
authored object IDs. `SceneBuilder` no longer binds rows into
`ObjectTrack::pcm`; the old implicit row-index path is removed. Base-carried
LFE PCM remains a separate field. CLI output uses
`diagnostics/reconstruction_rows/row_NNN.wav` and never labels it a verified
authored-object stem. Metadata-only scene validation is admissible, whereas an
audio-bound ObjectScene and verified authored-object PCM remain inadmissible.

This round changes no Logic fixture, private media, raw warp behavior, JOC
semantic inference, or profile rule. `ETSI_STRICT` continues to reject raw
warp 3 as reserved, and the vendor profile remains unchanged.

## J1R13 semantic-binding evidence boundary

OpenJOC now records proposed semantic relations through an explicit evidence
contract rather than treating structural or empirical observations as a
binding. Evidence is classified as `STRUCTURAL`, `EMPIRICAL`, or `VERIFIED`,
with scope, allowed provenance, observations, contradictions, negative
controls, producer/carrier constraints, required dimensions, and a falsifier.
The production scene remains `SemanticBindingState::Unresolved`; no real
evidence package in this campaign is admitted as verified.

The minimum future admission dimensions are WHO, WHERE, SLOT, ROW/BASIS,
audio identity, context, time, repeatability, negative control, and cross-state
coverage. Equal cardinality/index, a dominant row, one fixture, or one spatial
state is explicitly insufficient. Metadata-only ObjectScene and separate
diagnostic ReconstructionBasis rows remain valid; audio-bound ObjectScene and
verified authored-object PCM remain blocked. Evidence must be independently
supportable through normative/public or controlled clean-room sources; private,
decompiled, leaked, or unknown-constant material is not provenance.

## J1R15 ReconstructionBasis numerical acceptance (2026-08-10)

The existing frozen controlled corpus was audited and numerically exercised
within the declared ReconstructionBasis scope. Nine usable carriers retained
structural rows, finite samples, 1536-sample AU shape, stateful QMF history,
and deterministic repeated signatures. A fresh Center export produced
byte-identical reference-f64 row WAV hashes on repeat; f32 and reference-f64
have the same row/sample shape but intentionally different sample formats.

The decision is `RECONSTRUCTION_BASIS_NUMERICAL_ACCEPTANCE_ESTABLISHED`, scoped
to numerical and structural correctness. Startup samples are retained, RcLfe
is separate, and diagnostic files remain `row_NNN.wav` reconstruction rows.
This does not admit authored-object PCM, audio-bound ObjectScene, or row/slot
semantic identity. `SemanticBindingState::Unresolved` and ETSI raw `warp=3`
reserved behavior are unchanged. Private evidence freeze:
`20260810T151025Z_j1r15-reconstruction-basis-acceptance_ef3c43f/j1r15_evidence_freeze.json`.

## J1R16 Existing-corpus end-to-end acceptance (2026-08-10)

The nine independently qualified frozen carriers were evaluated across input
recognition, AU framing, base PCM, normative metadata, metadata-only scene
assembly, ReconstructionBasis output, determinism, and profile outcomes. All
nine reached the declared numerical/metadata boundaries; J1R14 timeline
ordering and J1R15 ReconstructionBasis acceptance remained passing. No
implementation defect or nondeterministic acceptance failure was found.

The evidence-bounded decision is `EXISTING_CORPUS_ACCEPTANCE_PARTIAL`:
`ETSI_STRICT` continues to reject the observed raw `warp=3` as an expected
normative boundary, while `DOLBY_VENDOR_COMPAT` accepts observed signaling only
with deviations and leaves the post-warp vendor continuation opaque. This
does not change warp/profile semantics. `SemanticBindingState::Unresolved`,
metadata-only ObjectScene admission, diagnostic ReconstructionBasis rows,
authored-object PCM inadmissibility, and audio-bound ObjectScene
inadmissibility are unchanged. No new Logic fixture or media was created.
Private evidence freeze:
`20260810T153638Z_j1r16-existing-corpus-acceptance_f845fdd0/j1r16_evidence_freeze.json`.

## J1R17 opaque vendor-continuation preservation (2026-08-10)

J1R17 adds an explicit lossless representation for the already bounded
continuation of an observed OAMD element after the ETSI-reserved raw warp
value. The vendor-compatible parser retains the complete declared element
body and exposes a non-owning bit view from the end of warp_mode to the
validated element boundary, together with payload-relative bounds, an exact
bit-window SHA-256, and provenance/status fields. The view is opaque and
unresolved; it is not an ETSI continuation, trim interpretation, padding
claim, or vendor semantic rule.

ETSI_STRICT remains unchanged and returns ReservedWarpMode { raw: 3 }.
DOLBY_VENDOR_COMPAT still requires explicit selection and only preserves the
observed raw-3 element as opaque_lossless_bounded; it does not feed the
continuation into OAMD timelines, ObjectScene binding, ReconstructionBasis
semantics, JOC rows, renderer state, or PCM. Existing qualified carriers were
rechecked without new media; no Logic fixture or export was created.
Private evidence freeze:
`20260810T155539Z_j1r17-opaque-vendor-continuation_f480e05d/j1r17_evidence_freeze.json`.

## J1R18 bounded streaming decode (2026-08-11)

J1R18 adds an explicit streaming retention mode to the payload/scene core.
`PayloadDecoder::streaming*` preserves codec, QMF, OAMD, and frame validation
state while dropping programme-duration metadata timelines, ReconstructionBasis
PCM, and base-LFE history after each sink delivery. Existing constructors
remain capture mode and retain their full-result API. Streaming returns only a
bounded `StreamingSceneSummary`; it cannot be finalized as a captured scene.

The E-AC-3 streaming entry points expose the same distinction, but the current
container/index layer still materializes input bytes and the AU index, while
WAV/diagnostic writers remain explicit capture paths. This establishes a
bounded decode core, not full input-to-output streaming. No new media or Logic
fixture was created.

## J1R19 incremental input/output boundaries (2026-08-11)

The input/output boundary now includes a reader-based raw E-AC-3 syncframe
framer (`RawEac3FrameReader`) that requests only the current header/declared
frame bytes, reports bounded carry high-watermarks, and preserves explicit
truncated-frame errors across arbitrary read chunk boundaries. It is a
framing primitive; the legacy `load_eac3`/CLI path still returns a complete
elementary-stream buffer and AU index for existing inspect/capture consumers.

`WaveWriter` provides a seekable incremental RIFF writer with identical sample
format, clipping, and dither policy. Captured scene row/LFE exports now append
chunks and patch sizes at finalization. ISO BMFF demux still materializes the
stream-copy payload and retains the current container/AU index boundary; no
seekless MP4 claim is made. No new fixture or media was created.

## J1R23 — E-AC-3 coding-tool admission boundary (2026-08-10)

J1R23 audits the production E-AC-3 coding-tool paths against the locally held
ETSI TS 102 366 V1.4.1 reference and existing tests. The frozen diagnostic
inventory observes block switching (121 block/channel records), dither
(14,878), exponent reuse (12,051), grouped mantissa state (15,305), and LFE
topology across four previously qualified diagnostic carriers. Coupling, SPX,
AHT, rematrix, and dependent-substream coding-tool effects are not exercised
by that controlled inventory; their parser/DSP/unit evidence is therefore
classified `IMPLEMENTED_BUT_UNVALIDATED`, not passed by absence.

The resulting release boundary is `EAC3_CODING_TOOL_COVERAGE_PARTIAL`. No new
media was created, `SemanticBindingState` remains `Unresolved`, and ETSI
strict raw warp 3 remains `ReservedWarpMode { raw: 3 }`. This matrix is not a
claim of full E-AC-3 coding-tool fidelity or authored-object identity.

## J1R24 — public-syntax coding-tool activation (2026-08-10)

The test-only `PublicSyntaxCase` harness uses public ETSI-shaped API structures
and existing frame helpers; it is not a general encoder and does not create
media. Coupling, SPX, AHT, rematrix, and dependent-substream production paths
now have explicit activation/state evidence. Rematrix additionally matches a
separate public sum/difference formula oracle for the tested band case.

This improves branch/state admission but does not upgrade real controlled
corpus activation: those target coding-tool effects remain
`NOT_EXERCISED_BY_CONTROLLED_CORPUS`. The narrow decisions are
`PUBLIC_SYNTAX_CODING_TOOL_ACTIVATION_HARNESS_ESTABLISHED` and
`EAC3_CODING_TOOL_STATE_ADMISSION_STRENGTHENED`; full E-AC-3 coding-tool
fidelity remains unclaimed.

## J1R25 — coupling state and coordinate admission (2026-08-10)

J1R25 adds a test-only, structurally independent float64 transcription of
TS 102 366 V1.4.1 clause 6.4.3. It exhaustively compares all 16 exponent ×
16 mantissa × 4 master-coordinate combinations against the public production
reconstruction API, with a predeclared `1e-15` absolute tolerance and explicit
rejection tests for out-of-domain fields. Existing parser-level synthetic
frames, including six-block coordinate reuse, are retained and now assert that
the reused coupling state is equal to the first admitted state.

This establishes public-syntax coordinate and bounded reuse evidence only. The
real controlled Logic corpus still has coupling activation
`NOT_EXERCISED_BY_CONTROLLED_CORPUS`; full coupled-PCM fidelity and semantic
object binding remain unestablished. `SemanticBindingState` remains
`Unresolved`, and strict warp raw 3 remains reserved.

## J1R26 — SPX state and reconstruction admission (2026-08-10)

J1R26 adds a test-only SPX oracle transcribed from ETSI TS 102 366 V1.4.1
Annex E.2.6.3/E.2.6.4. The bounded oracle covers all four `spxstrtf` copy
indices and all 16 × 4 × 4 legal exponent/mantissa/master coordinate values
for an isolated one-band translation-and-scaling path (1,024 cases), with
zero noise and no attenuation so the independently derived expected values
remain auditable. Existing SPX tests continue to cover band-size mapping,
noise blending, and the Table E.2.12 notch values; invalid coordinate,
attenuation, noise-length, and dimension inputs are explicitly rejected.

The result is intentionally scoped: parser activation and band/coordinate
reconstruction are admitted for public synthetic syntax, while cross-block
`spxcoe=0` reuse/reset and full real-stream SPX PCM fidelity remain open. The
controlled Logic corpus remains `REAL_CONTROLLED_CORPUS_SPX_ACTIVATION_NOT_OBSERVED`.

## J1R27 — SPX reuse, carry, and reset admission (2026-08-10)

The parser-level synthetic regression follows the normative E.1.3.3 state
fields: `spxstre=1` introduces block information, `spxstre=0` reuses the
previous active SPX parameters, `spxcoe=0` reuses per-channel coordinates,
and `spxcoe=1` supplies a replacement. The authorized PDF states that
coordinates are sent at least once per audio frame and may be sent once per
block (E.2.6.3, PDF page 162); band structure is defaulted on first use and
reused when omitted in a later block (E.1.3.3.7, PDF page 140).

The bounded fixture proves explicit A → two coordinate reuses → explicit B
(including a changed copy-region code) → reuse B → disable, plus disable →
fresh re-enable and an independent next-frame no-inheritance check. Effective
`SpectralExtensionInformation` is compared exactly, not by finite/approximate
output. A 256-repeat sequence remains exactly deterministic; the parser keeps
current frame/channel state rather than an accumulating history. This is a
public-syntax state-lifetime admission only: the Logic corpus remains SPX-off,
multi-channel participation and parser-specific truncation cases remain open,
and full real-stream SPX PCM fidelity is not established.

## J1R28 — SPX multi-channel participation and parser errors (2026-08-11)

The authorized normative source remains ETSI TS 102 366 V1.4.1. Annex
E.1.3.2.28 defines the first-coordinate condition per channel; E.1.3.3.1-.8
define block strategy, participation, and shared copy/band configuration;
E.1.3.3.9-.13 define per-channel coordinate reuse and values. E.2.6.3 states
that coordinates are carried for participating channels at least once per
audio frame. PDF pages 139-140 and 162 were inspected; `references/` remained
read-only.

A new test-only stereo public-syntax builder drives the production parser,
not an injected state object. It proves per-channel participation isolation,
fresh state after absence, independent reuse/replacement, structural
end-of-input rejection at participation and coordinate boundaries, recovery
on a fresh call, exact path equivalence, and bounded deterministic repetition.
No proprietary or third-party decoder source and no new media were used.

The evidence does not close dependent-substream/config reset because no
persistent cross-substream SPX parser state is exposed by the current API. It
also does not change the real-corpus SPX-off observation or establish full
real-stream PCM fidelity.

## J1R29 — AHT production reconstruction and numerical admission (2026-08-11)

The authorized normative source remains ETSI TS 102 366 V1.4.1. Annex E
pages 148–157 and the complete vector-quantizer tables on pages 175–191 were
visually inspected. A test-only transcription independently locks all 64
Table E.2.1 high-efficiency pointers and SHA-256 digests of all 956 vectors
(5,736 signed 16-bit words) in Tables E.3.1–E.3.7. It also exhaustively
compares 99,302 legal GAQ codewords with an independent Table E.2.5/E.2.6
float64 oracle and checks the E.2.4.5 six-point inverse DCT.

Production AHT does not stop at pointer selection: six-block public syntax
selects high-efficiency BAP, decodes VQ/GAQ mantissas once, applies the inverse
DCT, carries the six coefficients across their blocks, applies exponents, and
feeds the ordinary downstream coefficient/synthesis path. A parser-level
regression anchors one VQ bin across all six blocks to the independent formula
and proves direct, pre-parsed, and repeated decode equivalence. The enabled
and disabled fixtures take distinct reconstruction paths.

This is synthetic public-syntax reconstruction admission, not a real-producer
fidelity claim. The controlled Logic corpus remains
`REAL_CONTROLLED_CORPUS_AHT_ACTIVATION_NOT_OBSERVED`; full real-stream AHT PCM
fidelity is not established. No production AHT expression changed, no media
was created, `SemanticBindingState::Unresolved` remains unchanged, and ETSI
strict raw warp 3 remains reserved.

## J1R30 — dependent-substream assembly and channel topology (2026-08-11)

The authorized normative sources are ETSI TS 102 366 V1.4.1 Annex E and ETSI
TS 103 420 V1.2.1 clauses 8.1/E.3 and Table 47. The public 16-bit `chanmap`
was independently transcribed MSB-first from Table E.1.4 and compared with the
production mapper for all 65,536 bit patterns. Sentinel PCM tests separately
lock matching-location replacement, Lrs/Rrs and Vhl/Vhr supplementation,
canonical ordering, LFE replacement, and LFE/LFE2 distinction. The complete
JOC path now explicitly admits only Table 47 5.X, 7.X, and 5.X+2 topologies;
the lower E-AC-3 diagnostic mapper continues to represent the complete public
Table E.1.4 domain without promoting every map to a valid JOC layout.

Production fixes reset only the affected substream's TDAC history when its
rate/acmod/LFE/chanmap configuration changes, preserve channel-location labels
through `DecodedAccessUnitPcm`, and let the CLI capture a valid seven-channel
base instead of rejecting every non-five-channel result. Failed dependent
decodes remain atomic. Capture and AU-local PCM decoding compare exactly, and
the incremental AU reader retains one bounded lookahead frame across 128
I0/D0 units. Multiple dependent substreams remain outside TS 103 420 E.3's
one-I0/optional-D0 JOC contract and are rejected by the JOC decoder.

Decision: `DEPENDENT_SUBSTREAM_CHANNEL_ASSEMBLY_ADMISSION_ESTABLISHED` for the
public-syntax JOC scope. Existing controlled Logic carriers do not activate a
dependent substream, so `FULL_REAL_STREAM_DEPENDENT_SUBSTREAM_FIDELITY_ESTABLISHED`
is not claimed. No media was created, semantic binding remains unresolved,
and strict raw warp 3 remains reserved.
## J1R31 capability-contract provenance

The canonical 0.x matrix in `REQUIREMENTS_MATRIX.md` is a consolidation of
existing frozen evidence, not a new semantic experiment. Coding-tool claims are
bounded by the J1R23–J1R30 evidence freezes: coupling has the public normative
coordinate/state admission; SPX has public-syntax numerical, state,
multichannel, and partial error/substream scope; AHT has bounded independent
table/GAQ/IDCT and integrated-bin evidence; rematrix has the scoped public
sum/difference oracle; dependent-substream assembly has the exhaustive chanmap
oracle and one-I0/optional-D0 Table-47 assembly evidence. None has a newly
created real-media activation claim.

The J1R31 CLI changes are presentation/contract changes only. Error categories
are derived from existing typed input, E-AC-3, profile, OAMD, payload, WAV, and
I/O failures. Help and streaming summaries expose already implemented path
boundaries. No codec expression, parser meaning, profile rule, vendor mapping,
semantic binding, renderer, or fixture was added.

## J1R32 packaging provenance

The committed runtime table files were emitted by the existing
`import-etsi-tables` tool from the authorized official TS 103 420 V1.2.1
companion archive. The importer first verifies ZIP SHA-256
`a79cf108c4529b7d9ca9525c871183a70b1732ed6df03a3d85b2f31be46eeced`
and source SHA-256
`4db8ae83e3c2e9269e88365be92a1a3ed6a9e6ee3851afac8ca03902723b1fcd`.
Both checked-in generated files are byte-identical and record the latter hash.
The attachment itself remains outside Git and outside all source/package
archives.

Release evidence is derived from a tracked-file `git archive`, Cargo's package
file lists and package verification, isolated target/prefix directories,
`shasum -a 256`, `cmp`, `file`, `otool -L`, the installed CLI help surface, and
the existing Rust gates. The tested host is Apple-silicon macOS 26.6 with
Homebrew Rust/Cargo 1.94.0. The workspace declares Rust 1.85 as its minimum but
does not pin the exact compiler, so reproducibility is explicitly scoped to the
recorded host/toolchain/environment.

FFmpeg/FFprobe 8.1.2 are external runtime tools only for the documented
container/compatible-base paths. Logic, ADM tools, Poppler, Python, private
fixtures, and the ETSI attachment importer are not production CLI runtime
requirements. Nothing was published, tagged, uploaded, or released.

## J1R33 local release-candidate provenance

The local candidate builder starts from `git archive HEAD`, builds the CLI in a
fresh temporary target with the locked offline graph, and copies that exact
binary into the bundle assembled by the same process. Python 3.10+ is public
release-assembly tooling only; the candidate verifier uses the macOS shell and
`shasum` and does not read the repository, `.git`, private evidence, or the
network.

The machine-readable manifests declare the source commit, Cargo.lock digest,
target, Rust/Cargo versions, binary and archive digests, exact bundle inventory,
runtime-tool boundaries, and identity-signing/notarization status. A declared
commit is not a signed attestation. Reproducibility remains scoped to the same
committed source, host, target, toolchain, and cached dependency inputs. No
candidate is published, tagged, uploaded, or committed.

On the admitted Apple-silicon host, `codesign -dv` reports the Rust-linked
Mach-O as `adhoc,linker-signed`. This automatic linker signature uses no user or
Developer ID credential. The manifest records it separately from
`developer_identity_signed=false` and `notarized=false`.
