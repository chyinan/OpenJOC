# OpenJOC Requirements Matrix

Status values are `planned`, `implemented`, `verified`, `completed`, `open`, or
`partially evidenced`. A row may be marked `verified` only with fresh test or
artifact evidence; `completed` marks an integrated milestone whose scoped
acceptance boundary is closed. `open` and `partially evidenced` preserve an
explicit boundary when source or diagnostic evidence exists without closing
the external acceptance gate.

The matrix describes incremental evidence, not a claim that OpenJOC is already
a complete real-world Atmos decoder or renderer. The currently evidenced codec
boundary is raw E-AC-3 plus aligned base PCM plus JOC/OAMD extraction to the
following renderer-independent output boundary:

```text
metadata-only ObjectScene
  + separately named diagnostic ReconstructionBasis rows
  + optional separately retained base-LFE PCM
  + semantic audio binding state (Unresolved by default)
```

The current implementation deliberately does not call reconstruction rows
authored-object PCM or export them as verified object stems. J1R9--J1R11
established spatially anchored row evidence but not a semantic
OAMD-object/audio-row binding. `METADATA_OBJECTSCENE_ADMISSIBLE` is therefore
distinct from `AUDIO_BOUND_OBJECTSCENE_NOT_ADMISSIBLE`.

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
| TS 103 420 5.1-5.4 | OAMD content, properties, timed update/reuse model | `openjoc-oamd` / `openjoc-scene` | multi-update, reuse, and shared-timing block-order integration tests | implemented; semantic binding open |
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
| TS 103 420 6.4 | JOC input/output/control boundary | `openjoc-joc`, `openjoc-scene` | public `JocFrameInput`; raw JOC/OAMD + channel PCM → analysis QMF → reconstruction-basis QMF/PCM rows → metadata-only ObjectScene and atomic retry tests pass; authored-object binding remains unresolved | implemented |
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
| TS 102 366 6.4.3-6.5.4, 6.9.4 | Coupling, rematrix, and inverse TDAC primitives | `openjoc-eac3` | rendered-page coordinate scaling, sub-band expansion, right-channel phase restoration, clause 6.5 rematrix band boundaries/sum-difference restoration, 512/256 inverse-transform vectors, Table 6.33 windowing, overlap/add state, decoded audio-block PCM synthesis, and I0/D0 access-unit PCM merge tests pass; J1R25 independently exhaustively checks the standard coordinate code domain and bounded six-block reuse | implemented with scoped coordinate/state admission; real-corpus coupling and full coupled-PCM fidelity remain open |
| TS 102 366 E.2.6.2-E.2.6.4.3, Tables E.2.11-E.2.12 | Spectral-extension coefficient synthesis | `openjoc-eac3` | rendered-page table-indexed translation, band grouping, RMS/noise blend, coordinate scale, symmetric attenuation notch, frame attenuation retention, and bounded audio-block integration tests pass; legal real-vector fidelity remains pending | implemented |
| TS 102 366 E.1.3.1.2, E.2.8 | Independent/dependent substream relationships | `openjoc-eac3` | access-unit grouping, sequential IDs, immediate dependency, converted-stream exclusion, and timing tests pass | verified |
| TS 102 366 Annex H.2 | EMDF sync/container/config/protection | `openjoc-emdf` | group-offset, conditional config, bounded payload, all protection lengths, version/reserved/padding/truncation tests pass | verified |
| Engineering spec 5.1 | Checked MSB-first bit reader | `openjoc-bitio` | 6 unit/property tests pass; fuzz target remains | implemented |
| Engineering spec 5.2 | Official attachment importer with both SHA-256 gates | `import-etsi-tables` | 4 importer/CLI tests pass; fmt and clippy clean | verified |
| Engineering spec 5.7 | ObjectScene metadata and diagnostic reconstruction basis | `openjoc-scene`, `openjoc-wave` | metadata-only JSON roundtrip, separate reconstruction-basis rows, explicit unresolved binding state, invariants, and diagnostic row WAV export pass; verified authored-object PCM remains inadmissible | implemented |
| Engineering spec 6 | Complete CLI command surface and debug dumps | `openjoc-cli` | actual-binary `decode-payload` and direct `.ec3`/container `decode` write metadata scene/timeline/diagnostic reconstruction-row WAVs/debug artifacts; explicit `--reference-f64` retains reference row output; `inspect` reports bounded profile timing/carrier details, `decode --validation-profile` selects the explicit profile, and `--trim-config-count` remains caller-supplied | implemented |
| Engineering spec 6 / OAMD forensic boundary | Bit-exact OAMD entry evidence | `openjoc-cli`, `openjoc-emdf`, `openjoc-oamd` | `diagnose-oamd` records MP4 sample/AU/substream, exact skip-field and EMDF/payload/config/body spans in named coordinate systems, OAMD warp bit/window/raw value, original-byte dumps, all-AU continuity, and explicit trim-count provenance without changing decode semantics | implemented |
| Engineering spec 6 / OAMD forensic round 2 | AU timing, payload-11 differential, independent bit oracle, ADM comparison, diagnostic warp hypotheses | `openjoc-cli` | `--au START..END`, `--diff-payload-11`, deterministic timing/diff JSON/TXT, independent cursor-free oracle, explicit raw-vs-MP4 equality, and diagnostic-only hypotheses; strict parser remains reserved-value failure; private A-F corpus evidence is retained outside Git, with B/C canonical automation semantics explicitly unresolved | implemented |
| Engineering spec 6 / OAMD forensic round 3 | Reproducible raw/MP4/ADM refresh and Logic canonical-copy audit | private controlled run + public docs | New non-overwriting batch `2026-08-05T1042Z_logic-warp-evidence_952b052` covers A-F (12 carrier reports, six ADM reports); all 126 AUs close; four-way warp oracle remains `[526,528) = 3`; B/C remain explicitly non-canonical after a discarded Logic editing experiment; no vendor rule added | evidence refresh; semantics unresolved |
| Engineering spec 6 / interoperability boundary | Explicit ETSI and vendor-compatibility profiles | `openjoc-emdf`, `openjoc-eac3`, `openjoc-cli` | parser retains original EMDF; `ETSI_STRICT` never relaxes Table 55/56; `DOLBY_VENDOR_COMPAT` accepts only the observed Logic/Dolby pattern, records every deviation, and manifest expectations gate Logic/future DEE regressions | implemented |
| TS 103 420 5.6.0, 5.6.1, 6.4, 8.3 / controlled vendor programme boundary | Typed OAMD programme layout and reconstruction-row structure | `openjoc-scene`, `openjoc-cli` | OAMD cardinalities derive from anchors; `RcLfe` remains separately retained as base LFE; dynamic-slot↔row cardinality/order is structural only; `addbsi` complexity is checked against total OAMD count; semantic binding is unresolved | implemented |
| Engineering spec 6 / input-media boundary | File-signature classification and ISO BMFF/M4A/MP4 E-AC-3 stream-copy demux | `openjoc-container`, `openjoc-cli` | raw EC3 and ISO BMFF detection; unique `eac3` track selection; bounded FFmpeg stream-copy output; independent OpenJOC frame validation; inspect/decode integration and actionable container errors | completed |
| Engineering spec 6 / container diagnostics | Missing, multiple, unsupported, malformed, or failed container tracks | `openjoc-container`, `openjoc-cli` | structured error tests and proof that ISO BMFF never falls through to only an E-AC-3 syncword error | completed |
| Legal DEE real-vector lane | Nonzero JOC/OAMD reconstruction and continuity acceptance | `openjoc-cli`, `openjoc-scene` | user-supplied fixture hash, nonzero side information/stems, dynamic OAMD, moving object, multiple access units, known-stem/ADM-BWF comparison | planned |
| Legal DEE real-vector lane | FFmpeg base-channel versus `--internal-base` fidelity report | `openjoc-cli`, `openjoc-eac3` | per-channel count/order, delay, peak, RMS, and numerical-error comparison on the same legal stream | planned |
| Engineering spec 5.7 | Renderer-independent trim and balance retention | `openjoc-scene`, `openjoc-oamd` | trim warp mode, global/centre/surround/height trims, balance controls, per-object disable flags, and timed trim snapshots preserved without rendering behavior | implemented |
| Engineering spec 5.7 / frame-local staging | Atomic frame-local SceneBuilder staging without accumulated row clones | `openjoc-scene` | no per-frame clone of previously accumulated reconstruction rows; metadata/row validation occurs before commit; retry atomicity tests pass | implemented |
| Engineering spec 5.7 / scalability | Bounded whole-input retention, metadata-only scene assembly, and streaming PCM/file sinks | `openjoc-scene`, `openjoc-cli`, `openjoc-wave` | bounded input/base PCM retention and streaming object stems/scene metadata; current implementation remains open | planned |
| Engineering spec 5.7 / frame boundary | Borrowed per-frame sink for debug/artifact consumption | `openjoc-scene`, `openjoc-cli` | `PayloadDecoder::decode_frame_with` lends only the committed frame; aligned and internal E-AC-3 paths write debug artifacts without an all-frame `DecodedPayloadFrame` vector; sink/error and timing tests pass | implemented |
| Engineering spec 5.7 / wave output | Explicit f32/f64/s24/s16 sample-format abstraction | `openjoc-wave`, `openjoc-cli` | f32 normal output, explicit reference-f64 option, defined clipping and dither tests; compatible base PCM name | implemented |
| Engineering spec 9 | No panic/OOM/hang on malformed input | fuzz targets | bounded regression corpus and fuzz runs | planned |
| Mandatory DoD | WAV stems, scene, timeline, debug artifacts from real JOC | end-to-end workspace | legal vector artifacts plus fidelity comparison | planned |
| Mandatory DoD | Windows/Linux/macOS CI | `.github/workflows` | all platform jobs green | planned |

## J1R12 — Evidence-bounded reconstruction basis architecture (2026-08-10)

The Logic black-box campaign is frozen at the J1R9/J1R10/J1R11 evidence
boundary. J1R9 rejected the one-row-per-authored-object model; J1R10 left the
spatial basis underdetermined; J1R11 changed Logic application-level track
order but produced byte-identical EC3 and unchanged OAMD slot trajectories.
No independently controllable producer-side variable has been demonstrated
that changes OAMD dynamic-slot assignment while holding authored identity,
trajectory, and multi-object context fixed.

The production boundary is now explicit:

```text
OAMD -> metadata objects/state -> metadata-only ObjectScene
JOC  -> ReconstructionBasis rows -> diagnostic row PCM/WAV
semantic audio binding -> Unresolved (default)
```

Structural cardinality/order (15 dynamic slots ↔ 15 JOC rows, and OAMD[0]
`RcLfe` ↔ separately retained base-LFE carrier) remains available as
`ProgrammeLayout` evidence. It is never promoted to authored-object identity.
There is no row-index fallback, dominant-row fallback, FL/FR observation
fallback, or implicit `ObjectScene.objects[i].pcm` path. The strict OAMD
profile still rejects raw warp 3; no vendor rule or warp interpretation was
changed. Metadata-only scene admission is supported; audio-bound scene
admission and verified authored-object PCM remain blocked.

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

## Controlled Logic warp-study corpus (2026-08-05)

The private run `2026-08-05T004530Z_vector-corpus_1330681` contains Logic Pro
12.3 exports A-F, ADM BWF inventories, MP4 and stream-copied EC-3 hashes, and
raw/MP4 OAMD forensic reports. Every vector has 126 OpenJOC access units at
48 kHz and 1,536 samples per AU (`0.032 s/AU`). Normalized raw-versus-MP4
EMDF/OAMD observations are equal for all six vectors; carrier file offsets and
MP4 sample indices are intentionally excluded from that equality check.

| vector | ADM objects / object updates | payload-11 unique | first payload change | warp distribution | status |
| --- | --- | ---: | --- | --- | --- |
| A static centre | `OBJ_997HZ` / 1 | 1 | none | `3:126` | static export evidence |
| B requested single jump | `OBJ_997HZ` / 197 | 63 | AU 15, 0.480 s | `3:126` | export is still D mixed-motion; not canonical B |
| C requested linear ramp | `OBJ_997HZ` / 197 | 63 | AU 15, 0.480 s | `3:126` | export is still D mixed-motion; not canonical C |
| D existing mixed motion | `OBJ_997HZ` / 197 | 63 | AU 15, 0.480 s | `3:126` | control vector |
| E no dynamic object | none / 0 | 1 | none | `3:126` | warp remains present without object blocks |
| F two objects | `OBJ_997HZ`, `OBJ_2003HZ` / 197 each | 63 | AU 15, 0.480 s | `3:126` | two-object export evidence |

The independent bit oracle, diagnostic trace, formal entry trace, and direct
byte/mask calculation agree on payload-relative warp bits `[526,528)` and raw
value `3`. Elements 1 and 2 close at the same declared boundaries and the
three diagnostic hypotheses (assumed 0/1/2) all close only as non-unique,
diagnostic-only syntax. `ETSI_STRICT` therefore remains blocked by
`ReservedWarpMode { raw: 3 }`; no vendor warp rule was added. OAMD timeline,
JOC reconstruction, nonzero object PCM, and ADM fidelity remain unverified.

## J1R7A controlled normative OAMD boundary (2026-08-09)

The following rows are deliberately scoped to the seven frozen Logic/ADM
sources (129 AUs each, 903 observations) in private run
`20260809T180109Z_j1r7a-spec-anchored-oamd_b6eb1de`. They do not promote the
complete OAMD, JOC, or renderer requirements to completion.

| requirement | evidence | status |
| --- | --- | --- |
| Normative payload-11 cursor prefix | ETSI-anchored cursor reaches and closes `[0,526)` in 903/903 observations; no vendor branch or hypothesis cursor is used | completed |
| `pos3D_X` field identity | exact six-bit payload-relative `[52,58)`; ADM-qualified X controls decode `0,16,31,46,62` for `-1,-.5,0,+.5,+1` | verified |
| `pos3D_Y` field identity | exact six-bit payload-relative `[58,64)`; ADM-qualified Y controls decode `0,31,62` for `+1,0,-1` | verified |
| ADM ↔ X/Y numeric alignment | controlled numeric alignment only; no unproved coordinate conversion or renderer claim | verified |
| Trim `warp_mode` raw 3 | `[526,528)` raw `11`; ETSI Table 32 `0b1X` is reserved; strict result remains structured `ReservedWarpMode { raw: 3 }` | open |
| H0/H1/H2 semantic selection | all branches are labels over the same cursor and close identically; no semantic hypothesis selected | open |
| Complete OAMD trim/timeline/state semantics | post-warp continuation and reserved warp remain unadmitted; normative defaults/reuse and shared timing order are admitted for tested paths | partial; open |
| OAMD ↔ JOC identity, object PCM, ObjectScene/render fidelity | no end-to-end real-vector acceptance or fidelity gate is claimed | open |

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

## Dolby vendor partial-metadata boundary (2026-08-05)

| requirement | implementation/evidence | status |
| --- | --- | --- |
| ETSI strict raw warp 3 rejection | Formal element-2 trim parser still returns `ReservedWarpMode { code: 3 }` at payload-relative `[526,528)`; A/E/D forensic reports remain 126/126 rejected | implemented |
| Explicit vendor opaque trim retention | `DOLBY_VENDOR_COMPAT` requires payload ID 11, complete element-2 declared window, exact first error raw warp 3, and retains the full body/hash without remap | implemented/verified |
| Partial OAMD state | Element 1 is formally parsed; trim is `opaque_unresolved`; trim timeline and renderer fidelity are unavailable | implemented/verified |
| Inspect profile visibility | `inspect` always shows both carrier profiles; explicit `--trim-config-count N` additionally shows strict/vendor OAMD partial status without inferring a count | implemented |
| Independent payload-14/JOC chain | A/E/D/F compatible-base runs parse payload 14 across 126/126 AUs; JOC declares 15 reconstruction rows, structurally cardinal with 15 OAMD dynamic slots while the separate OAMD LFE remains base-carried; authored identity is unresolved | evidence |
| B/C/F vendor regression | Follow-up raw/MP4 private run repeats 126 AUs, raw warp `3:126`, opaque vendor acceptance `126/126`, normalized carrier equality, and payload-14 parse `126/126` | verified |
| Reconstruction rows and fidelity | A/E/D/F vendor-compatible runs emit 16-entry metadata scenes, 15 diagnostic reconstruction rows, and one separately retained base-LFE row; no verified authored-object PCM, trim/warp semantics, internal-base fidelity, or ADM rendering equivalence is claimed | evidence; fidelity open |

### J1R13 semantic-binding evidence contract

| Requirement | Current evidence boundary | Status |
|---|---|---|
| Evidence and production state are separate | `SemanticBindingEvidence` records structural/empirical/verified classes while `ObjectScene.semantic_binding` remains `unresolved` | implemented |
| Admission is explicit and falsifiable | Required WHO/WHERE/SLOT/ROW-or-basis/audio/context/time/repeatability/negative-control/cross-state dimensions plus provenance, contradictions, and a falsifier | implemented |
| Unsupported identity is rejected | Row-count/index equality, dominant-row correlation, one fixture, and field-name similarity cannot admit binding | implemented |
| Metadata-only scene and diagnostic rows | Both remain exportable without authored-object PCM identity | implemented |
| Verified authored-object PCM / audio-bound ObjectScene | Requires future admitted semantic binding; current Logic campaign is frozen | blocked |

## Controlled Logic programme-cardinality result (2026-08-05)

The typed scene boundary distinguishes the counts that were previously
collapsed into one equality:

```text
addbsi complexity == total OAMD programme entries
JOC output rows    == OAMD dynamic slot count
scene entries      == total OAMD programme entries
```

For all private Logic A-F carriers, the parsed anchor sequence is
`OAMD[0] = RcLfe` followed by `OAMD[1..15] = Dynamic`. The resulting binding
is `OAMD[0] -> BaseLfe(channel 0)` and dynamic slot `i -> JOC row i` for
`i = 0..14`. No row is deleted, synthesized, or shifted inside the JOC core;
the base LFE is never sent to the five-channel JOC matrix. The scene manifest
serializes entry 0 as `lfe`, followed by 15 `dynamic` entries.

The private run
`2026-08-05T044606Z_object-cardinality_a4f88af_r3` contains raw forensic
regressions for A-F and compatible-base decodes for A/E/D/F. Each decode has
126 AUs, 193,536 samples, 16 scene entries, 15 JOC-reconstructed dynamic
signals, and one base-carried LFE entry. The extracted LFE WAV is retained
and auditable; it is silent for these source programmes, not a fabricated
zero stem. OAMD `active` flags report all 15 dynamic slots in the observed
frames, including E where ADM exports zero dynamic object channels. This is
recorded as a codec-slot-capacity versus ADM-content distinction; PCM energy
is never used to infer activity.

This closes the Logic cardinality and first nonzero PCM boundary only under
`DOLBY_VENDOR_COMPAT`. `ETSI_STRICT` still rejects the unchanged raw warp 3,
the trim body remains opaque, complete OAMD timeline semantics are not
claimed, and no ADM speaker/render or internal-base fidelity result is
available.

## Internal-base fidelity evidence (2026-08-05)

The private run `OpenJOC-Private/reports/runs/2026-08-05T053438Z_internal-base-fidelity_dcfb56c`
compares the same raw EC-3 through FFmpeg 8.1.2 `pcm_f64le` compatible-base
and OpenJOC `--internal-base` paths. FFmpeg selects `0:a:0`, disables
resampling, and explicitly pans `FL,FR,FC,LFE,SL,SR`; the JOC input is
`FL,FR,FC,SL,SR`, with LFE retained separately. Zero-delay and bounded
 +/-4096-sample metrics are both retained; no gain is applied to raw metrics
and no pass threshold is asserted.

All A/E/D/F vectors have 126 AUs, 48 kHz, 1,536 samples/AU, and 193,536
samples. The numerical first divergence is AU 0/block 0 in every non-LFE
channel (FL sample 9, FR 9, FC 7, SL 12, SR 5); selected bounded delay is 0.
Front/centre raw SNR is approximately 84.5--90.6 dB, side raw SNR
approximately 38.8--51.3 dB. Both LFE paths are present and exactly silent.
These are measurements, not a fidelity acceptance claim: FFmpeg
presentation DRC/dialnorm defaults and OpenJOC policy remain distinct, and
decoder delay is not independently exposed.

The same payload 11/14 and JOC state are propagated through both bases.
Private reports include per-channel, per-AU, per-256-sample-block, object-row,
LFE, and deterministic frequency matrices for manifest frequencies 440, 659,
997, and 2003 Hz. F's ADM names remain an external oracle only because energy
is distributed across codec rows; E remains a no-dynamic-object negative
control. Two report trees are byte-identical. Strict validation, warp=3
handling, complete OAMD timeline, ADM/render fidelity, and the legal fidelity
lane remain unchanged/open.

## Round-7 FFmpeg/internal-base policy boundary (2026-08-05)

| requirement | implementation/evidence | status |
| --- | --- | --- |
| Explicit base policy boundary | `InternalBasePolicy::CurrentDefault` preserves the previous CLI behavior; `CodecCore` disables only optional `dynrng/dynrng2` gain and is selected explicitly with `--internal-base-policy codec-core` | implemented/tested |
| FFmpeg option audit | FFmpeg 8.1.2 decoder help/build/version and R0--R6 commands are recorded in private run `2026-08-05T070007Z_base-root-cause_792d937` | verified |
| Orthogonal policy matrix | R0/R2/R3/R5/R6 are byte-identical for A/E/D/F; R4 changes all four; R1 changes E only | measured; no normative policy adopted |
| First numerical boundary | all vectors first differ in AU 0/block 0 at FL/FR/FC/SL/SR samples 9/9/7/12/5; zero-delay selection remains 0 | measured |
| TDAC boundary probe | Resetting overlap state only before frame/AU 1 removes the large SL/SR block-6 residual in a diagnostic probe, while ETSI requires overlap with the previous block; no production reset was added | unresolved black-box interoperability |
| Stage evidence | Private opt-in stage inventory records decoded exponents, BAP, dequantized coefficients, transform/window, and overlap outputs; unavailable sub-stages are explicitly marked | diagnostic-only |
| Fidelity conclusion | FFmpeg remains an independent black-box comparator; no decoder algorithm, gain, channel remap, or state reset is copied; strict/vendor OAMD behavior and raw warp `3` are unchanged | open |

## TDAC boundary decomposition (2026-08-05)

| requirement | implementation/evidence | status |
| --- | --- | --- |
| Normative overlap model | ETSI TS 102 366 V1.4.1 clauses 5.2.11, 6.9.3, 6.9.4.1/2: `pcm[n] = 2 * (current_head[n] + previous_tail[n])`, then `carry_out[n] = current_tail[n]`; Table 6.33 is the 256-value symmetric window | recorded |
| Opt-in contribution trace | `AudioPcmSynthesizer::synthesize_with_trace` exposes pre-window IMDCT, window coefficients, windowed head/tail, carry-in/out, output sum, and scaled output; normal synthesis remains the direct production path | implemented/tested |
| Carry lifecycle | Per-channel 256-sample state; LFE is separate; frame calls stage cloned state and commit only after success; failed calls leave state unchanged; no AU reset | verified |
| Synthetic partition invariant | Deterministic 12-block continuous run equals 6+6 framed run sample-for-sample and with identical final carry; transition flags are channel-local | verified |
| Real AU boundary carry continuity | Corrected private run `2026-08-05T_tdac-boundary-corrected_054d3d4` (repeated in `_repeat`) covers A/E/D/F, 126 AUs and all 125 boundaries: every E-AC-3 `L/C/R/Ls/Rs` state equals the next `carry_in`; the report explicitly maps it to FFmpeg `FL/FR/FC/SL/SR` as `[L,R,C,Ls,Rs]` after LFE removal | verified |
| AU0 block5 -> AU1 block0 decomposition | With the corrected syntax/reference mapping, E-AC-3 `Ls/Rs` normal RMS is `0.0075718936/0.0073475530`; zero-carry RMS is `1.2581e-7/1.2454e-7`; stored-vs-inferred black-box carry correlation is only `0.0347/0.0349`; `L/C/R` remain small | localized; unresolved |
| Production correction | No reset, gain, remap, sample-index special case, or FFmpeg algorithm was added. The evidence rules out storage/commit loss but does not prove whether the remaining difference is an upstream block-5 transform/tail issue or FFmpeg frame-boundary policy | open |
| CurrentDefault regression | A/E/D/F `internal_base_full.wav` outputs are byte-identical to the prior base-root-cause run; no production TDAC behavior changed | verified |
| JOC propagation after a fix | Not rerun as a post-fix comparison because no production TDAC fix was admitted; prior JOC comparison remains the current non-fidelity evidence | pending |

## Independent TDAC oracle and base-only pre-roll (2026-08-05)

| requirement | implementation/evidence | status |
| --- | --- | --- |
| Independent mathematical oracle | Private pure-stdlib Python oracle uses a literal ETSI Table 6.33 window, direct type-IV IMDCT, windowing, overlap/add, and carry update. It does not import OpenJOC, FFmpeg, production state, cursor, or cosine helpers. | verified |
| Synthetic TDAC coverage | T0--T7 long/short IMDCT, window, windowed head/tail, overlap, and non-zero carry stages: 53 comparisons, no material divergence at the declared `1e-12` threshold. | verified |
| Partition invariance | A continuous 12-block run and a 6+6 framed run have identical PCM and final carry in the independent oracle. | verified |
| Real AU0 block5 replay | Independent replay agrees with production carry-out tail (`max abs <= 5.12e-17`) and AU1 block0 current head (`max abs <= 2.00e-15`) for A/E/D/F and L/C/R/Ls/Rs. | verified; TDAC arithmetic only |
| Base-only pre-roll controls | Private P0/P1/P2/P4 vectors keep the same active content and vary only 0/1/2/4 silent AUs. FFmpeg raw and MP4 PCM are sample-identical for every vector; none reproduces Logic's approximately `7e-3` first side-channel boundary event. | verified; no priming rule established |
| Logic virtual crop | Diagnostic exclusion of the first two Logic AUs reduces the residual to approximately `1.26e-6--3.50e-6` RMS, but does not alter production output and is not a fidelity claim. | diagnostic only |
| Production TDAC change | No reset, gain, remap, sample special case, or stream-specific compatibility rule was added. | verified |
| OAMD/warp behavior | `ETSI_STRICT` and existing vendor behavior are unchanged; raw warp `3` remains preserved/rejected according to the selected profile. | verified |
| Current conclusion | TDAC tail/head arithmetic is independently supported. A generic TDAC bug and a generic FFmpeg first-frame priming rule are not established. Logic encoder/upstream or stream-feature-specific provenance remains the first blocker. | open |

## Logic AU0/block5 coefficient provenance (2026-08-05)

| requirement | implementation/evidence | status |
| --- | --- | --- |
| Apple/macOS comparator | `afconvert` is available and decodes the Logic MP4 path; afinfo supplies `L,C,R,Ls,Rs,LFE`, remapped explicitly to `FL,FR,FC,LFE,SL,SR` as `[0,2,1,5,3,4]` | verified; comparator evidence |
| Tri-decoder boundary | Private run `OpenJOC-Private/reports/runs/2026-08-05T125009Z_logic-first-block-provenance_77116e9` compares Apple, FFmpeg MP4/raw, and OpenJOC without changing production; delay-aligned and unaligned AU-1 windows are reported separately | verified; diagnostic |
| Tool/stage inventory | AU0/block5 Ls/Rs contains no observed coupling, SPX, rematrix, or AHT; BAP=0 bins are classified as dither/noise only when the dither flag is set; frame-0 exponent/BAP state differs from later blocks | verified; provenance remains unresolved |
| Matched later blocks and state lifecycle | Exact and relaxed match sets are recorded for A/E/D/F and Logic LE0/LE1/LE2/LE4; relaxed matching requires excluding exponent strategy, and no state-reset or hidden AU boundary is introduced | verified; diagnostic |
| Independent tail backprojection | Private inverse is explicitly ill-conditioned/non-unique and uses only FFmpeg black-box output plus the independent TDAC oracle; dominant bins are not treated as tool attribution | verified; diagnostic only |
| Controlled Logic pre-roll corpus | Logic Pro 12.3 copies LE0/LE1/LE2/LE4 use the same source/project/export profile with 0/1/2/4 AU (0/1536/3072/6144 samples) source pre-roll; each is 126 AU/4 seconds, with private MP4, stream-copy EC3, and ADM BWF hashes | verified; media private |
| Raw/MP4 carrier equality | For all LE vectors, raw and MP4 diagnostics have 126 paired AUs, zero payload-11 body mismatches, and identical raw warp distribution `{3:126}` | verified; diagnostic |
| Diagnostic warp hypotheses | Assumed semantics 0, 1, and 2 each close the bounded element but are explicitly non-unique and do not reach normative object-element decoding; no hypothesis is selected | verified; semantics unresolved |
| Production behavior | `ETSI_STRICT` continues to reject `ReservedWarpMode { raw: 3 }`; vendor compatibility keeps the raw value and opaque trim deviation; no warp remap, TDAC reset, gain, channel remap, or magic offset was added | verified |
| Historical base/TDAC blocker | AU0/block5 Ls/Rs provenance and internal-base fidelity remain unresolved in the dated 2026-08-05 evidence; the current OAMD-specific blocker is reserved warp-3 semantics and post-warp continuation | open |

## Exact target-AU decoder-history experiment (2026-08-05)

| requirement | implementation/evidence | status |
| --- | --- | --- |
| Exact target bytes | OpenJOC `index_syncframes` + `group_access_units` extracted LE0 AU0/AU1 (3072 bytes each); SHA-256 `05712ff5...37856` / `578f26ca...e94c5`; direct extraction matches in H0/H1/H2/H4/HP | verified |
| History corpus | Diagnostic-only H0/H1/H2/H4/HP streams use the complete original source preceded by 0/1/2/4 AU0 copies or AU0+AU1; target occurrence indices and sample ranges are manifest-defined | verified; not normative programme vectors |
| Raw/MP4 carrier | All five raw streams remuxed with `-c:a copy`; MP4-to-EC3 roundtrip is byte-identical and frame counts are 126/127/128/130/128 | verified |
| OpenJOC replay | Diagnostic example replays every history through the existing E-AC-3 parser, `CodecCore` coefficients, and traced TDAC; no production reset or state mutation was added | verified; diagnostic |
| OpenJOC exposed stages | Parsed header, exponent/BAP state, pre-IMDCT coefficient hashes, and AU0/block5 Ls/Rs TDAC tail hashes are equal for identical target bytes across histories | verified |
| OpenJOC first divergence | For H1/H2/H4/HP target AU0, first exposed difference is `block0_channel3_tdac_carry_in`; target AU1 exposed stages and PCM are stable | verified; expected TDAC-context effect |
| Snapshot/replay | `AudioPcmSynthesizer` clone-before-target replay is deterministic; stage trace count, carry arrays, and PCM are equal; staged commit semantics preserve failed-call atomicity | verified |
| Raw mantissa/intermediate stages | Opt-in diagnostic trace records raw mantissa tokens, grouped state, dither values, dequantized mantissas, and final pre-IMDCT arrays from the production cursor; normal decode allocates no trace | verified; diagnostic |
| Component transplant | No production state components are public; transplant is explicitly reported as not performed rather than approximated | open; no production change |
| FFmpeg black box | Exact target occurrence comparison changes with history, especially side-channel AU0; this is an output observation only, not an internal-state claim | verified; black-box |
| Apple black box | `afconvert` accepted all remuxed histories; target AU0/AU1 output is sample-stable across H0/H1/H2/H4/HP under declared channel mapping | verified; black-box |
| Joint decision | OpenJOC coefficient stages are history-stable; OpenJOC AU0 PCM boundary follows TDAC carry context; FFmpeg is history-dependent; Apple is history-stable | narrowed; no codec-core fix |
| Production behavior | No TDAC change, reset, gain, remap, sample special case, vector/hash special case, OAMD/JOC profile change, or warp alias | verified |
| Remaining blocker | Separate fixed decoder priming/history coordinates from Logic AU0/block5 upstream coefficient provenance; component transplant and complete OAMD/JOC/fidelity claims remain open | open |

## Decoder comparison contract (2026-08-06)

| requirement | implementation/evidence | status |
| --- | --- | --- |
| Evaluation-only regions | `openjoc-cli::comparison` defines serializable `cold_start`, `warmup`, and `steady_state` regions with range/hash validation; it cannot alter decode or trimming | verified |
| Exact-history convergence | Private contract run `2026-08-05T_decoder-comparison-contract_01936ed_r8` maps target AUs by indexed manifest ranges, not packet ordinal guesses | verified; private |
| OpenJOC convergence | AU0 differs first at legal TDAC carry-in; AU1 stages and PCM are history-stable; decoder-state hash API remains unavailable | measured; scoped to this corpus |
| FFmpeg convergence | No PCM convergence suffix through source AU8 under the declared raw E-AC-3 command; PTS is unavailable | measured; mapping uncertainty recorded |
| Apple convergence | Target PCM is stable from AU0 in the observed 1536-sample grid; 288 trailing samples are absent and PTS is unavailable | measured; not a normative oracle |
| Sample 1536 interpretation | Reclassified as a warm-up/startup comparator boundary; cross-decoder semantic alignment is unproven and it is not a demonstrated TDAC defect | downgraded |
| Steady-state base metrics | A/E/D/F metrics are separated from cold/warm windows; values are reported without an acceptance threshold | `steady_state_reported` |
| JOC region metrics | Object WAVs remain complete; evaluation slicing is private/report-only; 15 rows and base-error propagation are recorded, semantic object identity remains open | diagnostic |
| Production behavior | No TDAC, mantissa, coupling, SPX, dither, rematrix, warp, reset, gain, remap, or silent trim change | verified |

## Steady-state coding-tool differential (2026-08-06)

| requirement | implementation/evidence | status |
| --- | --- | --- |
| AU/sample mapping | Indexed 1536-sample AU grid is high confidence for OpenJOC and FFmpeg; Apple has medium confidence with 288 trailing samples absent | partial; block alignment unproven externally |
| Fixed windows | S1=AU2–15, S2=AU32–63, S3=AU80–110, selected before measurement and below Apple valid tail | verified; evaluation-only |
| Per-block/channel metrics | CurrentDefault, FFmpeg raw, and Apple are sliced on the declared internal 256-sample block grid; external block semantic equivalence is explicitly not claimed | reported; scoped |
| Tool inventory | Existing parser evidence records representative target-block coupling/SPX/AHT/rematrix state and dither/BAP=0; complete independent per-AU tool strata are not exposed | incomplete; no causal inference |
| Tool association | No controlled on/off strata; no significant coupling/SPX/dither/rematrix/AHT effect size can be estimated | none established |
| Residual bands | Existing tail-bin attribution is retained as diagnostic evidence only; band overlap does not prove tool causality | unresolved |
| Decoder relationship | OpenJOC≈FFmpeg in steady window metrics; Apple differs materially from both under current mapping and cannot be an oracle | measured; non-unique |
| JOC propagation | 15 object rows and complete object WAVs retained; evaluation slicing does not alter output; semantic object identity remains open | diagnostic |
| Production behavior | No startup trim, TDAC reset, gain, remap, warp alias, AU exception, file rule, or FFmpeg-fitting constant added | verified |

## Block-anchor and parser tool inventory (2026-08-06)

| requirement | implementation/evidence | status |
| --- | --- | --- |
| Parser-emitted inventory | `openjoc-eac3::emit_coding_tool_inventory` consumes decoded prefix/BAP/AHT state without reparsing; CLI `diagnose-tools` is opt-in and failure-atomic | implemented; diagnostic |
| Inventory coverage | A/E/D/F each report 126 AU × 6 blocks × 5 full-band channels plus independent LFE records (4536 records/vector) | verified; private |
| Explicit/reused provenance | Stable provenance enum and reuse source fields are serialized; BAP counts are marked derived from expanded BAP arrays | verified |
| Inventory invariants | Six blocks/AU, channel dimensions, BAP histograms, coupling/SPX off ranges, and no partial failed-AU commit are checked | verified |
| Strata coverage | A/E/D/F are observational; coupling, SPX, and AHT are off; dither is mostly on; exponent reuse is observed but no randomized single-tool controls exist | confounded; no causal inference |
| Anchor source/detector | Deterministic 48 kHz 5.1 source with 16 AU × 6 × 256 markers; source detector recovers 480/480 blocks at high confidence | verified; source-only |
| Logic encoded carrier and external mapping | G9 controlled Logic carrier: source 480/480; OpenJOC CurrentDefault, OpenJOC CodecCore, FFmpeg raw, and FFmpeg MP4 each 461/480; all remaining 19 are frozen margin-only near-neighbor ambiguities; external blocker remains | partially evidenced |
| Anchored metrics/effects | Suppressed while external mapping is unproven | unavailable |
| Production behavior | No TDAC, DSP, trim, warp, vendor, or decoder semantic change | verified |

## Final external anchor boundary (2026-08-09)

The controlled source fixture is independently established: six-channel
semantic identity passes, source block detection is 480/480, and the source
energy, spectral, identity, and guard gates pass. Semantic identity is scoped
to the controlled 5.1(side) corpus; physical output index order may differ by
decoder, as demonstrated by the Apple diagnostic path. FFmpeg and Apple remain
black-box comparators, not normative oracles.

The external mapping requirement remains **OPEN / PARTIALLY EVIDENCED**:

```text
external_block_mapping_established = false
OpenJOC CurrentDefault = 461/480
OpenJOC CodecCore      = 461/480
FFmpeg raw             = 461/480
FFmpeg MP4             = 461/480
remaining 19           = best-second margin only
score failures         = 0 on required paths
jitter failures        = 0 on required paths
```

The four required paths share a stable local competing-peak structure
(`-1 × 8`, `-2 × 11`), but no cross-validated generalizable broadening model
was established. Consequently anchored coding-tool attribution and exact
external block-wise residual attribution remain **OPEN**. Existing completed
TDAC arithmetic, carry continuity, decoder-history, parser inventory, and
controlled semantic-channel requirements retain their independent statuses;
the unresolved anchor does not reopen or downgrade them.

JOC remains evaluation-only. This matrix does not claim a complete OAMD
timeline, authored-object/JOC-row identity, verified object PCM, ADM/render
fidelity, or resolved Logic `warp=3` semantics.

## J1R8 controlled 3D position calibration (2026-08-10)

| requirement | evidence | status |
| --- | --- | --- |
| Normative Z field identity | `pos3D_Z_sign_bits [64,65)` and `pos3D_Z_bits [65,69)` from the J1R7A ETSI cursor ledger; ETSI TS 103 420 V1.2.1 clauses 5.5.8–5.5.11 and 5.6.1.1.7–5.6.1.1.9 | validated for the controlled vector |
| Controlled Z/elevation calibration | One Center-derived Logic fixture, persisted `对象位置提升` values `0 → 50 → 100 → 0`; ADM Z baseline → ~0.5 → 1.0 → baseline | established; numeric formula not claimed |
| X/Y orthogonality | ADM X = -0.0 and Y = +1.0 throughout; normative X/Y values remain invariant while Z changes | verified for the controlled vector |
| Return-to-baseline | Final ADM baseline corresponds to the original normative Z magnitude code `0` | observed |
| Source control | 997 Hz source PCM is sample-identical to frozen Center control | passed |
| Carrier determinism | Stream-copied R0/R1 raw EC3 identical; 129 AU × 3072 bytes | passed |
| Reserved warp boundary | `warp [526,528) = raw 3` for 129/129; `ETSI_STRICT` remains `ReservedWarpMode { raw: 3 }` | unchanged; unresolved |
| Empirical post-warp suffix | `[528,536) = 00000000` for 129/129; no semantics assigned | invariant under this Z control |
| Size branch | Object Size authoring persistence and ADM propagation established; tested DD+ Size semantics, direct size-index response, and Size-related warp/suffix response not established | frozen; open |
| Remaining OAMD/JOC scope | Complete OAMD timeline/state semantics, OAMD↔JOC identity, verified object PCM, ObjectScene/render fidelity, and end-to-end acceptance | open |

The private run and its aggregate evidence freeze are recorded in
`PROVENANCE.md`; no production decoder or profile behavior was changed.

## J1R9 dual-object row-identity boundary (2026-08-10)

| requirement | evidence | status |
| --- | --- | --- |
| Dual authored-object control | One persisted Logic project with 997 Hz moving FL→FR and 2003 Hz moving FR→FL; source PCM identities are independently authenticated | verified for this fixture |
| ADM ground truth | Four-second ADM qualifies both object/track identities and stable FL/FR positions; transition Z is retained as an exception and excluded from stable-window claims | verified; scoped |
| Carrier determinism | Unchanged-project R0/R1 raw EC3 are byte-identical (`d35aee54…c452e`) | verified |
| OAMD boundary | Element 1 slot 0 remains FL and slot 3 remains FR in stable windows; full authored-object-to-slot identity is not established | partially evidenced |
| Pre-render JOC row observation | Row 0 changes 997→2003 while paired with stable FL; row 3 changes 2003→997 while paired with stable FR | verified; diagnostic and scoped |
| Authored-object-per-row model | The two high-energy rows exchange authenticated audio identity under an authored-object swap; they do not retain authored-object identity | `ONE_ROW_PER_AUTHORED_OBJECT_MODEL_REJECTED` for tested FL/FR swap |
| Spatially anchored structure | These observations support spatially anchored JOC-row structure, not a global row/basis theorem | partially evidenced; scoped |
| Strict/vendor warp boundary | `warp [526,528) = raw 3` remains ETSI-reserved; no vendor rule or production parser change | unchanged; unresolved |
| ObjectScene, renderer, and fidelity | No ObjectScene admission, complete OAMD/JOC binding, object PCM claim, or rendering/fidelity comparison | open |

## J1R15 ReconstructionBasis numerical acceptance (2026-08-10)

| requirement | evidence | status |
| --- | --- | --- |
| Existing-corpus ReconstructionBasis audit | Nine already-frozen carriers; no new Logic/ADM/DD+/EC3 media | passed; numerical scope only |
| Structural row shape | 15 structural rows where present; 129-AU controls = 198144 samples/row, J1R8 = 128 AUs, J1R9 = 125 AUs; no truncation/padding | passed |
| Stateful AU handling | Sequence/configuration reset rules and QMF history are covered by stateful tests and frozen traces | passed; no invented waveform-continuity threshold |
| Determinism | J1R10 primary/repeat reports and J1R9 pre-render matrix are byte-identical; fresh Center reference-f64 row WAV hashes repeat exactly | passed |
| Numerical health | No NaN/Inf, clipping rejection, row-length mismatch, or amplitude-growth defect; inactive zero rows remain diagnostic only | passed |
| Precision boundary | Internal reconstruction remains f64; f32/reference-f64 WAV formats differ intentionally with equal row/sample shape | passed; precision-scoped |
| RcLfe separation | Base-carried RcLfe remains separate from dynamic ReconstructionBasis rows | passed |
| Diagnostic export | `diagnostics/reconstruction_rows/row_NNN.wav`, no authored-object stem claim | passed |
| Semantic binding | Numerical acceptance does not identify authored object, slot, row, or PCM stem | unchanged; `SemanticBindingState::Unresolved` |
| Warp/profile behavior | `warp [526,528) = raw 3`; no vendor rule or production parser change | unchanged; unresolved |

## J1R16 Existing-corpus end-to-end acceptance (2026-08-10)

| requirement | evidence | status |
| --- | --- | --- |
| Qualified carrier population | Nine independently frozen J1R10/J1R15 carriers; no new Logic/ADM/DD+/EC3 media | passed |
| Input/AU/base decode boundary | All nine recognized, framed, and reached the existing base PCM numerical contract | passed; declared scope |
| Metadata-only scene | J1R14 timeline/state regression remains passing; metadata scene remains admissible | passed |
| ReconstructionBasis | J1R15 finite, shaped, deterministic structural rows remain available | passed; diagnostic only |
| ETSI strict profile | Raw `warp=3` remains `ReservedWarpMode`; strict rejection is expected, not a decoder defect | expected rejection |
| Dolby vendor profile | Observed signaling is accepted with deviations; bounded trim continuation remains opaque | partial / unresolved |
| Semantic binding | No authored-object PCM or audio-bound ObjectScene admission | unchanged; `SemanticBindingState::Unresolved` |
| Production defects | Existing corpus found no defect meeting the fix policy | none observed |

The release-readiness decision is `EXISTING_CORPUS_ACCEPTANCE_PARTIAL`, not a
percentage-based compatibility claim. Private evidence freeze:
`20260810T153638Z_j1r16-existing-corpus-acceptance_f845fdd0/j1r16_evidence_freeze.json`.

## J1R17 opaque vendor-continuation preservation (2026-08-10)

| requirement | evidence | status |
| --- | --- | --- |
| Bounded enclosing element retained | Declared element-2 body bounds are copied losslessly before vendor fallback | passed |
| Exact continuation view | Non-owning view spans element-relative warp_end..body_end; payload-relative bounds and bit length are explicit | passed |
| Bit-exact evidence | Continuation SHA-256 includes the exact bit length and MSB-first packed window; non-byte-aligned synthetic regression passes | passed |
| Provenance/status | opaque_lossless_bounded, vendor_observed_normative_unresolved, and unresolved are serialized | passed |
| Strict profile | Raw warp 3 remains ReservedWarpMode; no strict relaxation | unchanged; expected rejection |
| Vendor profile | Explicit Dolby-compatible profile preserves observed raw-3 continuation only; no alias or semantic interpretation | passed; opaque only |
| Downstream isolation | Opaque continuation is not consumed by timeline, ObjectScene binding, ReconstructionBasis semantics, renderer, or PCM paths | passed |
| Existing corpus | Nine qualified carriers rechecked without new Logic/ADM/DD+/EC3 media | passed |

This milestone establishes preservation, not meaning. The post-warp bits remain
normatively unresolved and do not justify a warp-3 alias or vendor semantic
rule.
Private evidence freeze:
`20260810T155539Z_j1r17-opaque-vendor-continuation_f480e05d/j1r17_evidence_freeze.json`.

## J1R18 bounded streaming decode (2026-08-11)

| requirement | evidence | status |
| --- | --- | --- |
| Explicit streaming mode | `PayloadDecoder::streaming*` and bounded scene summary | passed |
| Capture compatibility | Existing constructors/API retain full scene results | passed |
| Programme-duration scene retention | Streaming builder drops timeline, rows, and LFE after sink delivery | passed |
| Cross-AU codec history | Streaming and capture frame outputs are identical; state remains in `JocDecoderState` | passed |
| High-watermark invariant | 128 logical frames retain max one row, 64 samples/frame, and no historical event vector | passed |
| Input/container boundary | Current E-AC-3 indexing still materializes bytes and AU index | declared limitation |
| Output writers | Existing WAV/diagnostic paths remain explicit capture mode | declared limitation |
| Semantic binding | `SemanticBindingState::Unresolved` unchanged | unchanged |
| Warp/profile behavior | Strict raw warp 3 rejection and opaque vendor continuation unchanged | unchanged |

The strongest result is `BOUNDED_STREAMING_DECODE_CORE_ESTABLISHED`; this is
not a claim of full end-to-end input-to-output streaming.

## J1R19 incremental input/container/output boundary (2026-08-11)

| requirement | evidence | status |
| --- | --- | --- |
| Raw EC-3 reader framing | `RawEac3FrameReader` reads only header/declared frame bytes | passed; primitive |
| Raw chunk boundaries | 1/2/3/7/31/257/4096-byte chunk tests, split header/body, exact EOF and truncation | passed |
| Raw high-watermark | 128-frame logical sequence retains at most one current frame | passed |
| MP4 payload materialization | FFmpeg stream-copy output remains a complete `Vec<u8>` | declared limitation |
| ISO BMFF index | Current container/AU index remains duration-proportional metadata | declared limitation |
| Non-seekable MP4 | No admission claim | not admitted |
| Incremental WAV writer | Seekable `WaveWriter` patches RIFF/data sizes at close | passed |
| Scene row/LFE output | CLI captured exports use chunked `WaveWriter`; capture scene remains explicit | passed; capture boundary |
| Semantic binding/profile behavior | J1R13/J1R17/J1R18 contracts unchanged | unchanged |

The precise result is `STREAMING_INPUT_OUTPUT_ADMISSION_PARTIAL`: raw framing
and seekable output writing are bounded primitives, while the existing CLI
input/container path still has whole-stream and indexed metadata storage.

## J1R20 incremental AU consumer / container ownership closure (2026-08-11)

| requirement | evidence | status |
| --- | --- | --- |
| Sequential raw AU consumer | `RawEac3AccessUnitReader<R: Read>` emits locally indexed one-AU batches with one-frame lookahead | passed |
| Existing decoder reuse | Direct reader path calls the existing `JocAccessUnitPcmDecoder` and J1R18 `PayloadDecoder::streaming*` | passed |
| Capture/direct equivalence | Frozen Center 997 direct and legacy base/LFE WAVs, inventories, and shared frame diagnostics are byte-identical | passed |
| Memory plateau | Chunk/lookahead/truncation tests plus 128-AU sequence retain bounded carry/AU/lookahead state | passed |
| Finalization/error propagation | Exact EOF, truncated frame, invalid AU start, and streaming decoder finalization remain structured | passed |
| ISO BMFF ownership | Existing FFmpeg/sample-table/index boundary remains duration-proportional and explicitly declared | limitation retained |
| Legacy API | `load_eac3` and slice/index APIs remain available as explicit capture/random-access contracts | passed |
| Semantic binding | `SemanticBindingState::Unresolved`; authored-object PCM and audio-bound ObjectScene remain inadmissible | unchanged |
| Warp/profile behavior | ETSI strict raw warp 3 remains `ReservedWarpMode`; no vendor rule or continuation interpretation | unchanged |

The narrow decision is `DIRECT_RAW_EC3_STREAMING_DECODE_PATH_ESTABLISHED` for
the sequential raw internal-base path. This does not claim O(1) ISO BMFF
container metadata, authored-object identity, or resolved warp semantics.

## J1R21 seekable ISO BMFF delivery (2026-08-10)

| requirement | evidence | status |
| --- | --- | --- |
| Seekable MP4 sample delivery | `SeekableIsoBmffEc3Reader<R: Read + Seek>` reads packet offsets/sizes and releases each current sample | passed |
| Elementary-stream equivalence | Four frozen MP4/EC-3 pairs, 129 packets each, byte-identical packet sequence | passed |
| Media working set | No full `mdat` or complete EC-3 payload is materialized; one current sample retained | passed |
| Container index ownership | Derived packet-location entries are retained and reported as O(samples) metadata | explicit limitation |
| Existing AU consumer | Reader adapter feeds J1R20 `RawEac3FrameReader`/AU path; one sample is not assumed to equal one AU | passed |
| Non-seekable / fragmented MP4 | No generic fallback or fragmented-MP4 expansion | not admitted / out of scope |
| Error and EOF boundaries | Malformed rows, wrong stream, bounds, sample limit, exact EOF regressions | passed |
| Architecture/profile boundary | SemanticBindingState unresolved; strict raw warp=3 remains ReservedWarpMode; no vendor rule | unchanged |

The narrow decision is `SEEKABLE_ISOBMFF_STREAMING_ADMISSION_ESTABLISHED_WITH_INDEXED_METADATA`.
This is not a claim of O(1) sample-table/index memory or authored-object
semantic binding.

## J1R22 lazy sample cursor (2026-08-10)

| requirement | evidence | status |
| --- | --- | --- |
| Derived index audit | Pre-change `Vec<IsoBmffSample>` measured at 129 entries / ~2,064 bytes per representative carrier | passed |
| Sequential ownership | `IsoBmffSampleCursor` reads one FFprobe packet row at a time; ordinary path retains zero derived entries | passed |
| Bounded cursor | One child/reader/line-buffer cursor state; no descriptor history | passed |
| Elementary stream equivalence | Four frozen MP4/EC3 pairs retain identical ordered bytes and packet sizes | passed |
| Random access | Explicit indexed constructor remains available for capture/random access | passed |
| Native table accounting | FFprobe-owned stco/co64/stsc/stsz/stts metadata remains external and explicitly not claimed O(1) | limitation retained |
| Malformed/error boundaries | Row, stream, bounds, sample-limit, probe-failure, and exact-EOF paths remain structured | passed |
| Semantic/profile boundary | SemanticBindingState unresolved; ETSI raw warp=3 remains ReservedWarpMode | unchanged |

The narrow decisions are `DERIVED_ISOBMFF_SAMPLE_INDEX_ELIMINATED_FOR_SEQUENTIAL_DECODE`
and `BOUNDED_ISOBMFF_SAMPLE_CURSOR_ESTABLISHED`. This is not a claim of O(1)
ISO BMFF container metadata.

## J1R23 — Coding-tool coverage and admission (2026-08-10)

| requirement | evidence | status |
| --- | --- | --- |
| Syntax/DSP audit | `CodingToolBlockInventory`, E-AC-3 source paths, local ETSI TS 102 366 reference | passed with explicit scope columns |
| Existing controlled activation | A/D/E/F frozen inventories: block switch, dither, exponent reuse, grouped mantissa, LFE observed | passed as observational evidence |
| Under-exercised high-risk tools | coupling, SPX, AHT, rematrix, dependent-substream effects absent from inventory | `IMPLEMENTED_BUT_UNVALIDATED` |
| Unit/synthetic behavior | existing public-syntax/table/arithmetic tests | `PASS_WITH_DECLARED_LIMITATION` |
| Public media vectors | no authorized decoded vector in `references/` | not exercised |
| Release decision | no full-fidelity claim; next blocker is integrated public-syntax activation/state evidence | `EAC3_CODING_TOOL_COVERAGE_PARTIAL` |

This milestone does not alter J1R14–J1R22 streaming contracts, semantic
binding, or warp behavior.

## J1R24 — public-syntax activation harness (2026-08-10)

| requirement | evidence | status |
| --- | --- | --- |
| Reusable test-only activation | `PublicSyntaxCase` plus existing public production APIs | established with declared parser bypasses |
| Coupling/SPX/AHT activation | deterministic finite DSP cases and existing syntax/state tests | activated; numerical scope remains partial |
| Rematrix independent oracle | separate public sum/difference formula | `L4_INDEPENDENT_NORMATIVE_ORACLE_VALIDATED` for tested band formula |
| Dependent-substream state | existing parser/chanmap/merge/reset tests | `L2_STATE_TRANSITION_VALIDATED` with scope limit |
| Real controlled corpus | unchanged A/D/E/F evidence | target effects remain not exercised |
| Decision | harness and state admission only; no full-fidelity claim | `PUBLIC_SYNTAX_CODING_TOOL_ACTIVATION_HARNESS_ESTABLISHED` |
