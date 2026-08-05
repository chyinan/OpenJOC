# OpenJOC Requirements Matrix

Status values are `planned`, `implemented`, `verified`, or `completed`. A row
may be marked `verified` only with fresh test or artifact evidence; `completed`
marks an integrated milestone whose scoped acceptance boundary is closed.

The matrix describes incremental evidence, not a claim that OpenJOC is already
a complete real-world Atmos decoder or renderer. The currently evidenced codec
boundary is raw E-AC-3 plus aligned base PCM plus JOC/OAMD extraction to the
following renderer-independent output boundary:

```text
renderer-independent ObjectScene
  + default-f32 object stems
  + optional explicit reference-f64 object stems
```

The compatible base reference remains explicit FFmpeg `pcm_f64le` PCM and is
not a speaker or binaural render. Container input is a completed first
production increment; legal nonzero real-vector fidelity and rendering remain
open.

The JOC interoperability boundary is explicit:

```text
parse what exists -> JocPayload/ParsedJocAccessUnit
                 -> validate(ETSI_STRICT | DOLBY_VENDOR_COMPAT)
                 -> decode only an accepted representation
```

The parser never normalizes vendor metadata, the strict validator retains all
normative failures, and the decoder has no profile-specific signaling hacks.

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
| TS 103 420 8.1, E.3 | Required E-AC-3 downmix and substream behavior | `openjoc-eac3`, `openjoc-cli` | indexed six-block I0/optional D0 PCM assembly, normative replacement/supplement mapping, exact access-unit rate/sample alignment, and raw five-channel `--internal-base` ObjectScene integration; legal real-vector proof remains | implemented |
| TS 103 420 8.2, Tables 55-56 | Frame-end EMDF OAMD=11/JOC=14 restrictions and placement | `openjoc-emdf`, `openjoc-eac3` | bounded frame-end auxdata profile extraction, same-frame addbsi, last-dependent enforcement, explicit addbsi-without-profile diagnostics, and strict Table 55/56 validation | implemented |
| TS 103 420 8.2, Tables 55-56, TS 102 366 E.1.2.5, Annex H | Bounded audio-block `skipfld` EMDF candidate classification and profile inventory | `openjoc-emdf`, `openjoc-eac3` | exact declared skip-field range, frame-relative/elementary-stream offsets, no byte scanning or cross-carrier concatenation, exact-start Annex H classification, payload-ID/size/group and complete per-payload configuration inventory, malformed/trailing distinction, and access-unit profile extraction API; TS 102 366 calls `skipfld` dummy bytes and TS 103 420 does not expressly designate it as a JOC carrier, so this is diagnostic candidate evidence; four historical external fixtures and the controlled Logic Pro vector fail Table 56 profile validation | implemented |
| TS 103 420 8.2, Tables 55-56, TS 102 366 E.1.2.5, Annex H | All legal EMDF carrier locations and legal nonzero profile acceptance | `openjoc-emdf`, `openjoc-eac3` | complete bounded coverage of every authorized carrier on an authorized real vector, resolved skip-field carriage semantics, valid Table 55/56 pair, dynamic OAMD/JOC parsing, and nonzero reconstruction; current census coverage and invalid real profiles do not close this lane | planned |
| TS 103 420 8.2-8.3, TS 102 366 E.1.2/E.1.2.5/E.1.3.1.2 | External multi-fixture carrier census | `openjoc-cli`, `openjoc-eac3` | gitignored or environment-selected manifest; checked hashes; deterministic per-fixture JSON/text reports; frame-end and skip-field attempts; per-carrier payload distributions and configuration fields; explicit non-EMDF/valid/malformed/unsupported/profile states; no committed programme bytes | implemented |
| TS 102 366 E.1.2/E.2.8.2 | Parse-only audio-block carrier traversal | `openjoc-eac3` | BSI→audfrm→all six `audblk` prefixes and exact declared `skipfld` ranges reached on the four external fixtures without PCM synthesis; grouped mantissa cursor traversal reaches every block with zero malformed/unresolved counts; unsupported syntax remains explicit | implemented |
| TS 102 366 6.3.5, Tables 6.17-6.23 | Grouped mantissa carry across exponent sets and interleaved BAP values | `openjoc-eac3` | packed bap 1/2/4 state is retained across exponent-set calls and channel/coupling/LFE syntax within an audio block; boundary, interleaving, truncation, and four-fixture regression tests pass | implemented |
| TS 102 366 6.3.5, E.2.8.2 | Internal-base first-failure diagnosis | `openjoc-eac3`, `openjoc-cli` | structured element/channel/block/BAP/raw-code/width/grouped/bit-offset context and access-unit/stream offsets; no expansion of legal code domains without normative proof | implemented |
| TS 103 420 8.3 | `addbsi` extension type and complexity index | `openjoc-eac3` | exact 7+1+8 syntax, flag, reserved bits, 0..=16 bounds, and equality with total OAMD object count pass | verified |
| TS 103 420 Annex B | OAMD-to-ADM conversion architecture | `openjoc-scene` | ADM export schema/golden tests | planned |
| TS 102 366 6.7, Annex E.1.2, E.2.8.2 | E-AC-3 syncframe syntax and dynamic-range coefficient processing | `openjoc-eac3` | bounded acquisition header, conditional BSI/addbsi/channel-map extraction, option-4 mixdata bound, complete `audfrm` state, six-block channel/coupling/LFE mantissa traversal, clause 6.7 `dynrng/dynrng2` gain decoding with block reuse, exact skip-field retention pass, direct bounded syncframe-to-PCM entry point, and stateful I0/D0 merge with standard/custom Cs replacement | implemented |
| TS 102 366 6.2, E.2.4.3 | Fixed-point decoder bit allocation | `openjoc-eac3` | exhaustive exponent-to-PSD, Table 6.14 log-addition/integration, Tables 6.6-6.16 and E.2.1 parameter/band/masking/pointer mappings, excitation/masking/delta/BAP, conventional mantissa, and AHT GAQ/VQ traversal tests pass | verified |
| TS 102 366 6.4.3-6.5.4, 6.9.4 | Coupling, rematrix, and inverse TDAC primitives | `openjoc-eac3` | rendered-page coordinate scaling, sub-band expansion, right-channel phase restoration, clause 6.5 rematrix band boundaries/sum-difference restoration, 512/256 inverse-transform vectors, Table 6.33 windowing, overlap/add state, decoded audio-block PCM synthesis, and I0/D0 access-unit PCM merge tests pass | implemented |
| TS 102 366 E.2.6.2-E.2.6.4.3, Tables E.2.11-E.2.12 | Spectral-extension coefficient synthesis | `openjoc-eac3` | rendered-page table-indexed translation, band grouping, RMS/noise blend, coordinate scale, symmetric attenuation notch, frame attenuation retention, and bounded audio-block integration tests pass; legal real-vector fidelity remains pending | implemented |
| TS 102 366 E.1.3.1.2, E.2.8 | Independent/dependent substream relationships | `openjoc-eac3` | access-unit grouping, sequential IDs, immediate dependency, converted-stream exclusion, and timing tests pass | verified |
| TS 102 366 Annex H.2 | EMDF sync/container/config/protection | `openjoc-emdf` | group-offset, conditional config, bounded payload, all protection lengths, version/reserved/padding/truncation tests pass | verified |
| Engineering spec 5.1 | Checked MSB-first bit reader | `openjoc-bitio` | 6 unit/property tests pass; fuzz target remains | implemented |
| Engineering spec 5.2 | Official attachment importer with both SHA-256 gates | `import-etsi-tables` | 4 importer/CLI tests pass; fmt and clippy clean | verified |
| Engineering spec 5.7 | ObjectScene JSON and per-object PCM | `openjoc-scene`, `openjoc-wave` | raw payload-to-scene integration, metadata-complete JSON roundtrip, decoded OAMD/timed PCM assembly, invariants, and lossless f64 WAV byte tests pass; filesystem CLI export remains | implemented |
| Engineering spec 6 | Complete CLI command surface and debug dumps | `openjoc-cli` | actual-binary `decode-payload` and direct `.ec3`/container `decode` write scene/timeline/default-f32 stems/debug artifacts; explicit `--reference-f64` retains reference output; `inspect` reports bounded profile timing/carrier details, `decode --validation-profile` selects the explicit profile, and `--trim-config-count` remains caller-supplied | implemented |
| Engineering spec 6 / OAMD forensic boundary | Bit-exact OAMD entry evidence | `openjoc-cli`, `openjoc-emdf`, `openjoc-oamd` | `diagnose-oamd` records MP4 sample/AU/substream, exact skip-field and EMDF/payload/config/body spans in named coordinate systems, OAMD warp bit/window/raw value, original-byte dumps, all-AU continuity, and explicit trim-count provenance without changing decode semantics | implemented |
| Engineering spec 6 / OAMD forensic round 2 | AU timing, payload-11 differential, independent bit oracle, ADM comparison, diagnostic warp hypotheses | `openjoc-cli` | `--au START..END`, `--diff-payload-11`, deterministic timing/diff JSON/TXT, independent cursor-free oracle, explicit raw-vs-MP4 equality, and diagnostic-only hypotheses; strict parser remains reserved-value failure; private Logic A/B/C/E/F exports remain open | implemented |
| Engineering spec 6 / interoperability boundary | Explicit ETSI and vendor-compatibility profiles | `openjoc-emdf`, `openjoc-eac3`, `openjoc-cli` | parser retains original EMDF; `ETSI_STRICT` never relaxes Table 55/56; `DOLBY_VENDOR_COMPAT` accepts only the observed Logic/Dolby pattern, records every deviation, and manifest expectations gate Logic/future DEE regressions | implemented |
| Engineering spec 6 / input-media boundary | File-signature classification and ISO BMFF/M4A/MP4 E-AC-3 stream-copy demux | `openjoc-container`, `openjoc-cli` | raw EC3 and ISO BMFF detection; unique `eac3` track selection; bounded FFmpeg stream-copy output; independent OpenJOC frame validation; inspect/decode integration and actionable container errors | completed |
| Engineering spec 6 / container diagnostics | Missing, multiple, unsupported, malformed, or failed container tracks | `openjoc-container`, `openjoc-cli` | structured error tests and proof that ISO BMFF never falls through to only an E-AC-3 syncword error | completed |
| Legal DEE real-vector lane | Nonzero JOC/OAMD reconstruction and continuity acceptance | `openjoc-cli`, `openjoc-scene` | user-supplied fixture hash, nonzero side information/stems, dynamic OAMD, moving object, multiple access units, known-stem/ADM-BWF comparison | planned |
| Legal DEE real-vector lane | FFmpeg base-channel versus `--internal-base` fidelity report | `openjoc-cli`, `openjoc-eac3` | per-channel count/order, delay, peak, RMS, and numerical-error comparison on the same legal stream | planned |
| Engineering spec 5.7 | Renderer-independent trim and balance retention | `openjoc-scene`, `openjoc-oamd` | trim warp mode, global/centre/surround/height trims, balance controls, per-object disable flags, and timed trim snapshots preserved without rendering behavior | implemented |
| Engineering spec 5.7 / frame-local staging | Atomic frame-local SceneBuilder staging without accumulated PCM clones | `openjoc-scene` | no per-frame clone of previously accumulated object PCM; metadata/PCM validation occurs before commit; retry atomicity tests pass | implemented |
| Engineering spec 5.7 / scalability | Bounded whole-input retention, metadata-only scene assembly, and streaming PCM/file sinks | `openjoc-scene`, `openjoc-cli`, `openjoc-wave` | bounded input/base PCM retention and streaming object stems/scene metadata; current implementation remains open | planned |
| Engineering spec 5.7 / frame boundary | Borrowed per-frame sink for debug/artifact consumption | `openjoc-scene`, `openjoc-cli` | `PayloadDecoder::decode_frame_with` lends only the committed frame; aligned and internal E-AC-3 paths write debug artifacts without an all-frame `DecodedPayloadFrame` vector; sink/error and timing tests pass | implemented |
| Engineering spec 5.7 / wave output | Explicit f32/f64/s24/s16 sample-format abstraction | `openjoc-wave`, `openjoc-cli` | f32 normal output, explicit reference-f64 option, defined clipping and dither tests; compatible base PCM name | implemented |
| Engineering spec 9 | No panic/OOM/hang on malformed input | fuzz targets | bounded regression corpus and fuzz runs | planned |
| Mandatory DoD | WAV stems, scene, timeline, debug artifacts from real JOC | end-to-end workspace | legal vector artifacts plus fidelity comparison | planned |
| Mandatory DoD | Windows/Linux/macOS CI | `.github/workflows` | all platform jobs green | planned |

## Controlled Logic Pro vector gate result

The first reproducible Logic Pro 12.3 production vector is retained outside
the repository. Its controlled 48 kHz, four-second project contains a stereo
bed and one 997 Hz moving object. The ADM export contains 192,000 PCM samples,
11 channels, two ADM objects, and 197 object-position blocks; its object
channel is sample-identical to the known source stem. This closes the source,
project, ADM-export, and ground-truth inventory portions of the production
procedure, but not the legal JOC reconstruction lane.

The final 768 kbit/s DD+ Atmos MP4 has SHA-256
`704545f313148412d019a8e7e739fccc0ead345ba7afb4b3b32199fde7b79af0`.
Its stream-copied EC-3 is 387,072 bytes with SHA-256
`7ed23a04628c62300a3cc4cee846a308077f8a9117e96366d2b018e6b3ec2249`.
OpenJOC reaches all 126 access units and all 756 audio-block prefixes, with no
unresolved or malformed carrier traversal. Every access unit has one bounded
Annex H candidate containing payload IDs 11, 14, 2, and 1. IDs 11 and 14 share
group 0, but both set `codecdatae=0`; ID 11 also sets
`payload_frame_aligned=0`. `ETSI_STRICT` therefore fails with seven recorded
normative deviations per access unit. `DOLBY_VENDOR_COMPAT` accepts the exact
observed pattern as `accepted_with_deviation`, preserving the original EMDF
configuration and all seven evidence records. Two independent release census
runs produced byte-identical JSON/TXT reports. This establishes the
interoperability boundary; it does not claim that ETSI is wrong or that the
commercial encoder is wrong. OAMD/JOC reconstruction and `--internal-base`
fidelity remain open beyond the profile gate.

The `skipfld` implementation above is intentionally a bounded candidate path.
TS 102 366 describes its bytes as dummy data, and TS 103 420 does not state
that the field carries JOC EMDF. Therefore `implemented` here means exact-range
classification and inventory are available; it does not advance the
`planned` all-carrier or legal real-vector rows.

## Global quality gates

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-features`
- release build without network access
- zero `unsafe` in the initial reference implementation
- no `unwrap`/`expect` on external input paths
- `IMPLEMENTATION_REPORT.md` records exact commands, results, QMF metrics,
  real-vector outcomes, remaining gaps, and known limitations. Current real
  fixture evidence is recorded as an open/failed fidelity lane, not as decoder
  completion.
