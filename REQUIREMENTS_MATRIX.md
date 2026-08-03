# OpenJOC Requirements Matrix

Status values are `planned`, `implemented`, or `verified`. A row may be marked
`verified` only with fresh test or artifact evidence.

| Normative source | Requirement | Production target | Required evidence | Status |
| --- | --- | --- | --- | --- |
| TS 103 420 4.2-4.4 | Coordinate models and renderer-independent decoder interface | `openjoc-scene` | anchor-preserving scene model, all-anchor JSON roundtrips, atomic decoded-OAMD/PCM assembly, and invariant tests pass | implemented |
| TS 103 420 5.1-5.4 | OAMD content, properties, timed update/reuse model | `openjoc-oamd` | multi-update and reuse integration tests | planned |
| TS 103 420 5.5 | Complete OAMD bitstream syntax | `openjoc-oamd` | syntax vectors, truncation and malformed tests | planned |
| TS 103 420 5.5.2-5.5.5, 5.6.4.2-5.6.4.5 | Top-level payload and bounded element metadata/dispatch | `openjoc-oamd` | integrated object/trim/extended elements, unaligned declared sizes, zero padding, unknown retention, discard/alternate IDs and object-class derivation pass | implemented |
| TS 103 420 5.5.5-5.5.11 | Object element, per-object updates, basic/render information | `openjoc-oamd` | full, inactive, bed/ISF, mixed/reuse, previous-object gain, bounded additional-data, truncation and reserved-bit tests pass | verified |
| TS 103 420 5.6.0, 5.6.1.1.3-5, 5.6.4.8, Tables 11b-13 | OAMD content-description and ordered anchor semantics | `openjoc-oamd` | dynamic-only/LFE, exhaustive standard/nonstandard speaker labels, ISF MULZ, bed→ISF→dynamic ordering, extended counts, and integrated class/count tests pass | verified |
| TS 103 420 4.2.1-4.2.2, 5.2.1.2-5.2.1.3, 5.6.1 | Position, size, priority, gain, lock, zones | `openjoc-oamd` | boundary/exhaustive tables, finite/infinite outside-room projection, exact screen/depth matrix interpolation, invalid centre-ray/factor, and integrated object-update tests pass | implemented |
| TS 103 420 5.6.1.3-5.6.1.4 | Priority and gain semantics | `openjoc-oamd` | exhaustive 32 priority and 64 gain codes, defaults, infinity and reuse pass | verified |
| TS 103 420 5.6.1.2, 5.6.1.6 | Size and zone constraints | `openjoc-oamd` | all size modes/boundaries and every valid/reserved zone index pass | verified |
| TS 103 420 5.6.2 | Metadata timing and ramp semantics | `openjoc-oamd` | four updates, offset arithmetic, all ramp forms, atomic 1,536-sample frame advancement and reset pass | verified |
| TS 103 420 5.6.4-5.6.6 | Additional metadata, trim, divergence, precision extension | `openjoc-oamd` | exhaustive divergence tables, reuse/default/reserved modes, extension presence/order, pre-clamp application, dimension and top-level dispatch tests pass | verified |
| TS 103 420 5.5.12, 5.6.5, Tables 32-39 | Trim syntax, modes, controls, balance, and per-object disable | `openjoc-oamd` | exhaustive trim/balance tables, custom syntax, reserved-value rejection, explicit cardinality ambiguity, and ID 2 dispatch pass | verified |
| TS 103 420 5.5.1 | Bounded `variable_bits_max` decoding | `openjoc-oamd` | group values, exact bound, truncation, invalid configuration, overflow tests pass | verified |
| TS 103 420 6.2 | Complete JOC payload syntax | `openjoc-joc` | full/sparse retained-codeword vectors and padding tests pass | verified |
| TS 103 420 6.3 | Complete JOC field semantics and validation | `openjoc-joc` | 5/7 channel, 96/192, smooth/steep, reserved/range tests pass | verified |
| TS 103 420 6.4 | JOC input/output/control boundary | `openjoc-joc`, `openjoc-scene` | public `JocFrameInput`; raw JOC/OAMD + channel PCM → analysis QMF → object QMF/PCM → timed ObjectScene and atomic retry tests pass | verified |
| TS 103 420 6.5, Table 54 | Exact 1/3/5/7/9/12/15/23 band mapping | `openjoc-joc` | exhaustive 8 x 64 test passes | verified |
| TS 103 420 6.6.2 | Separate sparse and full differential decoding | `openjoc-joc` | distinct pseudocode 2/3 and malformed-input tests pass | verified |
| TS 103 420 6.6.3, Annex A | Six MSB-first Huffman trees and symbol mapping | importer, `openjoc-joc` | all leaves, uniqueness, prefix-free, truncation and malformed-reference tests pass | verified |
| TS 103 420 6.6.4 | Exact 96/192-level dequantization | `openjoc-joc` | all 288 inputs: oracle/finite/monotonic/centre tests pass | verified |
| TS 103 420 6.6.5 | Smooth/steep interpolation and previous-frame state | `openjoc-joc` | 1/2 point, offsets, absent reuse, sequence-zero/gap resets and next-state tests pass | verified |
| TS 103 420 6.6.6 | Complex QMF object matrix reconstruction | `openjoc-joc` | mixing, zeroing and dimension tests pass | verified |
| TS 103 420 7.2 | 64-band/640-tap complex QMF analysis | `openjoc-qmf` | direct pseudocode 8-12 path; impulse/DC/sines/noise numerical suite passes | verified |
| TS 103 420 7.3 | Complex QMF synthesis and resettable state | `openjoc-qmf` | direct pseudocode 13-17 path; reset and deterministic delay/gain/max/RMS regression tests pass | verified |
| TS 103 420 7.4 | Per-object inverse-QMF integration | `openjoc-joc`, `openjoc-qmf` | sample-exact integrated/reference synthesis, continuity, and splice-reset tests pass | verified |
| TS 103 420 7.4 | Official `prot64` coefficients | importer, `openjoc-qmf` | importer hash/count/provenance tests pass; QMF use remains | implemented |
| TS 103 420 8.1 | Required E-AC-3 downmix and substream behavior | `openjoc-eac3`, `openjoc-cli` | exact access-unit/PCM rate and sample alignment reaches ObjectScene; base PCM uses replaceable external FFmpeg boundary; legal real-vector proof remains | implemented |
| TS 103 420 8.2, Tables 55-56 | EMDF OAMD=11/JOC=14 restrictions and placement | `openjoc-emdf`, `openjoc-eac3` | bounded auxdata profile extraction, same-frame addbsi, last-dependent enforcement, and exact `skipfld` retention pass; merging all legal carrier locations remains | implemented |
| TS 103 420 8.3 | `addbsi` extension type and complexity index | `openjoc-eac3` | exact 7+1+8 syntax, flag, reserved bits, 0..=16 bounds, and equality with total OAMD object count pass | verified |
| TS 103 420 Annex B | OAMD-to-ADM conversion architecture | `openjoc-scene` | ADM export schema/golden tests | planned |
| TS 102 366 Annex E.1.2 | E-AC-3 syncframe syntax, size and timing | `openjoc-eac3` | bounded acquisition header, conditional BSI/addbsi extraction, option-4 mixdata bound, complete `audfrm` state, and six-block channel/coupling/LFE mantissa traversal with exact skip-field retention pass; internal PCM post-processing remains a separate boundary | implemented |
| TS 102 366 6.2, E.2.4.3 | Fixed-point decoder bit allocation | `openjoc-eac3` | exhaustive exponent-to-PSD, Table 6.14 log-addition/integration, Tables 6.6-6.16 and E.2.1 parameter/band/masking/pointer mappings, excitation/masking/delta/BAP, conventional mantissa, and AHT GAQ/VQ traversal tests pass | verified |
| TS 102 366 6.4.3-6.4.4, 6.9.4 | Standard coupling and inverse TDAC primitives | `openjoc-eac3` | rendered-page coordinate scaling, sub-band expansion, right-channel phase restoration, 512/256 inverse-transform vectors, Table 6.33 windowing, and overlap/add state tests pass; full de-coupling/rematrix/SPX/dynrng/PCM integration remains planned | implemented |
| TS 102 366 E.1.3.1.2, E.2.8 | Independent/dependent substream relationships | `openjoc-eac3` | access-unit grouping, sequential IDs, immediate dependency, converted-stream exclusion, and timing tests pass | verified |
| TS 102 366 Annex H.2 | EMDF sync/container/config/protection | `openjoc-emdf` | group-offset, conditional config, bounded payload, all protection lengths, version/reserved/padding/truncation tests pass | verified |
| Engineering spec 5.1 | Checked MSB-first bit reader | `openjoc-bitio` | 6 unit/property tests pass; fuzz target remains | implemented |
| Engineering spec 5.2 | Official attachment importer with both SHA-256 gates | `import-etsi-tables` | 4 importer/CLI tests pass; fmt and clippy clean | verified |
| Engineering spec 5.7 | ObjectScene JSON and per-object PCM | `openjoc-scene`, `openjoc-wave` | raw payload-to-scene integration, metadata-complete JSON roundtrip, decoded OAMD/timed PCM assembly, invariants, and lossless f64 WAV byte tests pass; filesystem CLI export remains | implemented |
| Engineering spec 6 | Complete CLI command surface and debug dumps | `openjoc-cli` | actual-binary `decode-payload` and direct `.ec3` `decode` write scene/timeline/f64 stem/debug artifacts; `inspect` reports bounded profile timing/carrier details; dump commands remain | implemented |
| Engineering spec 9 | No panic/OOM/hang on malformed input | fuzz targets | bounded regression corpus and fuzz runs | planned |
| Mandatory DoD | WAV stems, scene, timeline, debug artifacts from real JOC | end-to-end workspace | legal vector artifacts plus fidelity comparison | planned |
| Mandatory DoD | Windows/Linux/macOS CI | `.github/workflows` | all platform jobs green | planned |

## Global quality gates

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-features`
- release build without network access
- zero `unsafe` in the initial reference implementation
- no `unwrap`/`expect` on external input paths
- `IMPLEMENTATION_REPORT.md` records exact commands, results, QMF metrics,
  real-vector outcomes, remaining gaps, and known limitations
