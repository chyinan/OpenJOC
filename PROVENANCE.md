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
- Official reference data: none. TS 102 366 pages 114, 126, and 127 and TS
  103 420 pages 68 and 69 were rendered losslessly at 300 DPI using Poppler
  26.02.0 and visually inspected. This verified the fixed acquisition-field
  ordering, 16-bit-word frame-size relationship, sample-rate/block-count
  tables, and exact 7+1+8-bit type-A extension layout.
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
  tested. Full BSI, dependent-substream assembly, EMDF location, CRC, and audio
  decoding remain explicitly incomplete.

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

No ambiguity has been resolved outside the normative sources. New ambiguities must
be added here with the relevant clause, competing readings, selected derivation,
and a test or explicit TODO before implementation proceeds.
