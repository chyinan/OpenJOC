# Stream Inspector

`openjoc inspect` reads observable E-AC-3, JOC, EMDF, and OAMD syntax without modifying input media or synthesizing PCM. It accepts ordinary E-AC-3 as well as JOC. File extensions and external codec labels do not establish JOC presence.

The Windows LAV integration also exposes a read-only **JOC Stream** property
page. It displays programme layout separately from JOC reconstruction carriers;
its values come from E-AC-3/JOC bytes observed by OpenJOC during the
current DirectShow decode session, not from MediaInfo, ffprobe, a filename, or
a reopened source file. Live counters are labeled observed-so-far. A seek,
flush, or media change starts a new observation epoch; EOS is marked
`complete_continuous` only when continuous coverage from the beginning is
proven. The page is therefore a playback diagnostic view, while
`openjoc inspect` remains the authoritative offline/full-stream tool.

## Commands

```sh
openjoc inspect input.ec3
openjoc inspect input.mp4 --json
openjoc inspect input.ec3 --aus
openjoc inspect input.ec3 --au 15 --objects --emdf
openjoc inspect input.ec3 --au-range 10:20 --json
openjoc inspect input.ec3 --verbose
```

`--json` writes one schema-versioned document to stdout. Errors go to stderr. `--aus` retains AU details; `--au` and the inclusive `--au-range START:END` select zero-based AU indices without reducing the full-stream census. `--objects` includes per-OAMD-slot statistics. `--emdf` expands human configuration details; JSON always includes the EMDF census and configurations. `--verbose` adds component configuration and bounded diagnostic explanations. The existing expert `--trim-config-count N` override is retained.

Exit zero means the input traversal completed, including a traversal that found malformed metadata. Consumers must inspect `validation` and `diagnostics`, not just the exit code. A truncated/unreadable compressed stream emits its safely known partial report and exits nonzero. An input that cannot be opened or selected by the container backend fails before a stream report is available. `--verify-render` is not implemented; use the existing render commands for PCM verification.

## Inputs and container declarations

Raw AC-3/E-AC-3 frames use OpenJOC's bounded frame reader. Long AUs retain all observed independent/dependent programmes; short AUs use the existing six-block grouping rules. An incomplete or inconsistent candidate preserves its safely parsed frame and payload census. Framing failure stops at the first undecidable byte boundary; the inspector does not guess resynchronization points.

MP4 and fragmented ISO BMFF/CMAF use the existing seekable FFprobe packet cursor. FFprobe supplies packet byte locations and separately reported container declarations. All E-AC-3/JOC classification is performed on those packet bytes by OpenJOC. This path requires `ffprobe` on PATH and inherits its track selection and edit-list handling; raw inspection has no external-tool dependency.

`container` reports the sample entry, track ID, timescale, duration, sample count, and `dec3` information when exposed by the backend. Builds of FFprobe that expose a complete `dec3` box allow the existing OpenJOC `dec3` parser to validate it. Unexposed fields remain `null`, including fragmentation/CMAF conformance when the backend supplies no authoritative declaration. Inspecting a fragmented file is not itself a CMAF conformance certificate.

`CONTAINER_STREAM_MISMATCH` identifies a definitive disagreement between exposed `dec3` JOC/complexity signaling and in-band results. Incomplete inspection is not proof of absent JOC. The inspector does not fabricate missing container declarations or add a second ISO BMFF parser.

## Human output

The default summary covers input timing, topology, actual block partitions, JOC profile/carriers, payload census/order, validation, and observed scene changes. A compact excerpt from the project-owned synthetic golden is:

```text
OpenJOC Stream Inspector

Input
  Format                 E-AC-3 JOC
  Duration               0.032000 s
  Access units           1
  Total samples          1536
  Total bitrate          1024.000 kbps

E-AC-3 topology
  Programme              I0 (1 AUs; first 0)
  Block partition        6 (1 AUs)

JOC
  Present                Yes
  Profile                5.X (idx0)
  Reconstruction inputs  L R C Ls Rs
```

The full human golden and compact JSON contract golden live in `crates/openjoc-inspect/tests/goldens/`.

## Topology, channels, and timing

Components expose stream type, ID, observed frame lengths, bitrate, sample rates, block counts, `numblkscod` when carried, `acmod`, `lfeon`, `chanmap`, and decoded channel locations. `I0 + D0 + D1` preserves D1 rather than reducing the programme to a single dependent. For an additional independent programme, `I1/D0` distinguishes its D0 from I0's D0.

Channel locations use the existing decoder's Table E.1.4 mapping. Dependent channels can replace independent locations or supplement them. Conflicts between dependents are reported. LFE ownership distinguishes absent, independent-owned, dependent supplementation, dependent replacement, and ambiguous/invalid. LFE is never counted as a JOC reconstruction input.

Original-syntax AC-3 core carriage with JOC is named **E-AC-3 JOC with legacy/original-syntax AC-3 core**.

An AU is the existing six-block programme interval (1536 samples). A short partition remains `1 + 2 + 3`, `2 + 2 + 2`, or its exact observed sequence. Repeated independent programme sets contribute timing once; dependent frames do not multiply duration. Component bitrate uses that component's own accumulated sample duration. Frame continuity is syntax-derived; raw streams do not supply absolute presentation timestamps.

## JOC profiles

| Index | Machine value | Meaning | Reconstruction carriers |
| --- | --- | --- | --- |
| 0 | `five_x` | 5.X | L R C Ls Rs |
| 1 | `flat_7x` | Flat-7.X | L R C Ls Rs Lrs Rrs |
| 2 | `five_x_plus_two` | 5.X+2 | L R C Ls Rs Tfl Tfr |
| 3 | `five_x_phase` | 5.X + Phase Signaling | L R C Ls Rs |
| 4 | `five_x_plus_two_phase` | 5.X+2 + Phase Signaling | L R C Ls Rs Tfl Tfr |
| 5–7 | `reserved` | Reserved / invalid for supported public profile | Unavailable |

Phase profiles report signaling semantics; the inspector does not imply an additional decoder phase transform. Component channel-map labels retain Table E.1.4's `Vhl/Vhr`; the JOC profile carrier names use `Tfl/Tfr`.

JOC coded-row counts and OAMD programme object counts are reported separately. Complexity validation uses the OAMD programme count, as the existing public validator requires; it does not assume that complexity equals the JOC row count.

`joc.present` means at least one JOC payload parsed successfully. `presence_status` distinguishes `present`, `not_present`, `malformed_payload`, and `unavailable`. `false` alone is not proof of absence: incomplete skip-field traversal can leave presence unavailable. Addbsi-only signaling is counted separately. A valid JOC payload remains observable even when its carrier owner or signaling profile is invalid.

## EMDF signaling and carriage

The census examines exact frame-end `auxdatae` and audio-block `skipfld` carrier ranges through the existing parsers. It never scans arbitrary bytes for a favorable syncword. Additional payload IDs are retained even when the container is not a JOC profile candidate.

Every payload has occurrence count, affected-AU count, first AU, minimum/maximum length, and a length histogram. Payload orders retain counts and first occurrences. Public configuration includes sample offset, duration, group ID, `codecdatae`, frame alignment, duplicate controls, priority, processing permission, and discard-unknown behavior. A payload can occur more than once per AU: occurrences and affected AUs intentionally differ.

**ETSI Strict** and **Deployed Compatibility** are independent evaluations of parsed EMDF signaling. Strict FAIL does not mean “not JOC.” Compatibility PASS does not make the input ETSI-conformant. Both may pass on strict signaling. These are EMDF profile results; separate JOC/OAMD syntax failures remain visible in stream validation.

Each semantic deviation reports payload ID, field, observed/expected values, first failing AU, and affected-AU count. Repeated copies in one AU do not inflate the count. Carriage ownership errors do not suppress signaling validation. An I0/skipfld stream can pass strict validation; location does not select a compatibility profile.

## Object metadata

The inspector compares resolved OAMD activity, gain/priority, position, size, distance, zone, screen, and channel-lock properties. Raw coding choices and opaque bytes do not establish dynamics. All parsed object-element timing blocks participate; updates are ordered by sample time, and later same-time element updates replace earlier ones. Bounded future updates are merged with the following AU.

`--objects` reports OAMD slot index, first/last active AU, update count, first/last change, dynamic status, and coded position ranges for dynamic-class objects. Bed/ISF slots have no invented position ranges. “Dynamic metadata” includes an activity or gain change; it does not exclusively mean spatial movement. An unchanged single observation is “no change observed,” not proof of a static authored master.

These slots are not recovered original authored identities and are not automatically bindings to JOC reconstruction rows. The inspector does not recover original ADM, source stems, original automation, or proprietary encoder state. Opaque vendor trim remains explicitly unresolved.

## JSON contract and Rust API

`schema_version` is integer **1**. Required top-level objects are `input`, `container`, `eac3`, `joc`, `carriage`, `emdf`, `scene`, `validation`, and `diagnostics`; `access_units` is always an array, empty unless requested. The [JSON Schema](stream-inspector.schema.json) covers the stable required surface and permits additive fields.

Numeric rates/counts/IDs are numbers, booleans are booleans, and unavailable values are `null`. Validation status uses `pass`, `fail`, and `not_applicable`; syntax coverage additionally uses `partial`. Timing uses `continuous`, `discontinuous`, `sample_rate_changes`, or `unavailable`. Machine consumers use explicit profile indices and enum values rather than parsing human display names. Deviation observed/expected values are canonical semantic strings supplied by the public validator.

Lists of observed variants retain first-occurrence order; scalar sets and payload IDs use deterministic ordering. Paths and timestamps from the host are excluded. Only the supplied input basename is included, so rename private filenames before sharing if necessary. There is no separate bug-report bundle.

```rust
use openjoc_inspect::{InspectionOptions, inspect_path, inspect_reader};

let options = InspectionOptions::default();
let report = inspect_path(std::path::Path::new("input.ec3"), options)?;
let json = serde_json::to_string_pretty(&report)?;

// A reader API does not require a filename or CLI process.
let report = inspect_reader(compressed_bytes.as_slice(), options);
# Ok::<(), Box<dyn std::error::Error>>(())
```

`InspectionAccumulator` also accepts locally indexed bounded AU candidates for Rust integrations. C ABI, Browser, and JOCForge interfaces are unchanged in this milestone.

## Validation and resource limits

`decoder_admissible` is `true` only when every AU passes the existing JOC compressed-audio contract and an evaluated signaling profile admits it. `null` means complete admission coverage is unavailable, including ordinary E-AC-3 without a JOC contract. `false` indicates a known failure. `decoder_checked_aus` and `decoder_admitted_aus` expose the coverage. Admission does not guarantee finite rendered PCM; `render_verified` is always false.

Default inspection retains one compressed candidate (at most 72 frames / 288 KiB), bounded OAMD state, and summary counters. No full-file compressed buffer or PCM scene is retained. Distinct summary variants are capped at 256, with `aggregation_truncated` set if observation variants cannot be retained. Diagnostic examples are capped at 64 while total counts continue. Explicit AU details grow with the selected AU range.

Metadata errors do not discard previous census results. A malformed candidate can be followed by later independently delimited valid candidates. A truncated final syncframe preserves preceding complete AUs. Unknown or unsupported audio-block traversal produces partial coverage, not invented proof of malformed JOC or absence.

The tests use project-owned bitstream builders: ordinary and short/mixed E-AC-3; profiles 0–4 and reserved values; strict and deployed skip fields; multiple independent and dependent streams; legacy core; LFE ownership; malformed metadata, ordering, and truncated tails; object activity changes; stable text/JSON; and MP4/fragmented container delivery. External container tests require FFmpeg tools. No private music is committed.

An initial Windows release-build measurement inspected a 307.2-second project-owned synthetic stream (9,600 AUs, 37.5 MiB) in 0.620 seconds, about 15,475 AUs/s. Observed peak working set was 5.67 MiB and maximum retained AU data was 4,096 bytes. The file repeats the public WASM fixture; this is a reproducible throughput check, not a claim about every stream or machine.
