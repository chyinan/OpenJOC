# OpenJOC implementation report

## Evidence boundary

The repository currently supports a clean-room raw E-AC-3 path with aligned
base-channel PCM, JOC/OAMD extraction, a renderer-independent `ObjectScene`,
default-f32 object stems, and optional explicit reference-f64 object stems.
This report does not claim a complete real-world Atmos decoder or a
speaker/binaural renderer. The compatible base reference remains explicit
FFmpeg `pcm_f64le` PCM and is not a final render.

## Completed increment: input media and DEE containers

Implemented in `openjoc-container` and `openjoc-cli`:

- signature classification distinguishes raw E-AC-3 (`0b77`) from ISO BMFF
  (`ftyp`/recognized top-level box) before `index_syncframes`;
- FFprobe selects exactly one audio stream and requires codec `eac3`;
- FFmpeg is invoked only for `-c:a copy -f eac3` stream-copy demux;
- stdout is bounded, empty/oversized output is rejected, and OpenJOC indexes
  and groups the demuxed elementary stream independently;
- `inspect` and `decode` use the same boundary; raw `.ec3` remains direct;
- container, missing-track, multiple-track, unsupported-codec, malformed, and
  failed-demux diagnostics are structured and do not collapse to syncword
  errors;
- the generated compatible base-channel reference is now named
  `debug/compatible_base.wav`; it remains explicit `pcm_f64le` reference PCM,
  not a final render.

## Tests added

- `crates/openjoc-container/tests/input_media.rs`: raw/ISO/unknown signatures
  and probe-row parsing.
- `crates/openjoc-cli/tests/container.rs`: FFmpeg + MP4Box fixture demux,
  byte-equivalent stream-copy comparison, raw classification, inspect/decode
  routing, malformed container, AAC-only track, and multiple-track errors.

The MP4Box command is test-fixture tooling only; it is not an OpenJOC runtime
dependency. Runtime container behavior uses FFprobe and FFmpeg as external
black boxes and never uses their decoded PCM as object reconstruction.

## Completed increment: container audit evidence refresh

The installed toolchain was rechecked on the current worktree: Poppler
26.02.0 (`pdftoppm`, `pdftotext`, and `pdfinfo`) and FFmpeg/FFprobe are
available on `PATH`. The earlier audit of the supplied external DEE fixture
labelled `forever_friends` recorded SHA-256
`67c10f65642f11713f8495026a37cf26fd1f901e9a343d2e3acf5ee879584896` and size
32,138,978 bytes. Running
`OPENJOC_DEE_FIXTURE=<that path> cargo test -p openjoc-cli --test container
user_supplied_dee_fixture_uses_container_boundary_when_enabled` passed; the
test independently demuxed and indexed all 7,773 access units. A release
`inspect` run reports `ISO BMFF (stream-copied E-AC-3)`, 7,773 frames, and
7,773 access units. At that point the diagnostic wording “JOC extension
signaled ... EMDF profile absent” was bounded to frame-end auxiliary
inspection and did not establish EMDF absence from every legal E-AC-3 carrier.
This historical audit did not yet classify audio-block `skipfld`; the later
skip-field increment below supersedes that limited carrier result. No separate
metadata or recognized JOC box was found at the ISO BMFF track/box level. The
container goal remains completed and the real-vector acceptance lane remains
open.

## Implemented increment: external multi-fixture carrier census

`openjoc census [MANIFEST] -o DIR` (or
`OPENJOC_REAL_FIXTURE_MANIFEST=<path>`) accepts a local JSON manifest of
user-supplied raw EC3/M4A/MP4 fixtures. The manifest is gitignored by default;
labels, optional SHA-256 values, and notes are checked before any demux. The
command emits deterministic `census.json` and `census.txt` reports without
copying programme bytes into the repository. Missing manifests/files, duplicate
labels, hash mismatches, unsupported inputs, probe failures, and bounded demux
failures are structured errors.

Each fixture report records source and stream-copy hashes and sizes, selected
track metadata, syncframe/access-unit/sample counts, substream topology,
`addbsi` and complexity distributions, frame-end and skip-field carrier
attempts, payload-ID distributions by carrier kind, skip-field reachability,
unresolved blocks, malformed/unsupported candidates, profile counts, and the
first bounded failure. Carrier states distinguish a valid but incomplete
profile from malformed or unresolved traversal; a carrier state is never an
EMDF-absence claim beyond the paths actually examined. The text report starts
with a cross-fixture comparison table.

The current opt-in external corpus contains four non-committed DEE outputs.
The grouped-mantissa correction in commit
`2c524d107ae7451b2a6c838e7ca64159a51b375b` still accounts for the complete
six-block traversal: malformed mantissa count is zero and unresolved block
count is zero. The later bounded-carrier integration in `d900ef1` classifies
one exact skip-field range per access unit as an Annex H EMDF candidate. Each
candidate exposes payload IDs 11, 14, 2, and 1, but the ID-11 Table 56
configuration is invalid (`codecdatae=0`, `payload_frame_aligned=0`), so no
complete JOC profile is accepted. TS 102 366 describes `skipfld` as dummy
bytes to ignore, and TS 103 420 does not expressly designate that field as a
JOC carrier; this is a bounded diagnostic candidate result, not proof of
normative skip-field carriage.

| label | source SHA-256 | bytes | frames/access units | addbsi complexity | frame-end auxdatae | skip observed/examined/unresolved | payload 11/14 counts | state |
| --- | --- | ---: | ---: | ---: | ---: | --- | --- | --- |
| `forever_friends` | `67c10f65642f11713f8495026a37cf26fd1f901e9a343d2e3acf5ee879584896` | 32,138,978 | 7,773/7,773 | 7,773 × 16 | 0/7,773 | 7,773/46,638/0 | 7,773/7,773 | `emdf_profile_incomplete` |
| `hitchcock` | `0075ade8f801e38a4f98637d9d9a8099771ea1edd0bb66bd829aa2c0faa3e425` | 29,370,578 | 7,146/7,146 | 7,146 × 16 | 0/7,146 | 7,146/42,876/0 | 7,146/7,146 | `emdf_profile_incomplete` |
| `grand_escape` | `b7a320d2ff14a27e64b9e0262f2092b31145bc217100a2f987d174fef0ef2956` | 44,175,378 | 10,599/10,599 | 10,599 × 16 | 0/10,599 | 10,599/63,594/0 | 10,599/10,599 | `emdf_profile_incomplete` |
| `brainrot` | `2808eecb80353141135000ab499815219a86770e5b02e912dc971dd01e86afd7` | 16,283,910 | 3,910/3,910 | 3,910 × 16 | 0/3,910 | 3,910/23,460/0 | 3,910/3,910 | `emdf_profile_incomplete` |

Two release census runs over this opt-in manifest were byte-identical:
`census.json` SHA-256
`668edc62157103438c8b2c516bae8ab8bfcd671fef757648f196797296b1daac` and
`census.txt` SHA-256
`da61afb27ec2043f07b43792c49da2f1b848a4519d481f0e028f5028f2d04fa8`.

All four are ISO BMFF files with one 48 kHz six-channel `eac3` stream and
1,536 samples per access unit. Frame-end `auxdatae` is absent in every frame;
each reached skip-field exact range is parsed as an Annex H candidate with
payload IDs `11,14,2,1`, and every one fails the same ID-11 Table 56
configuration requirement. The parse-only walker reaches all six audio-block
prefixes and declared skip fields without scanning mantissa bytes for EMDF.
The result is not a claim that `skipfld` is a normative JOC carrier or that
other legal carrier locations are absent.
The earlier `bap` 3/5 mantissa failures were caused by grouped state being
reset at exponent-set boundaries; they are now regression-covered and absent
from the four-fixture census. This is not evidence of nonzero JOC/OAMD
reconstruction or internal-base fidelity.

## Controlled Logic Pro vector: production complete, profile gate failed

On 2026-08-04 the first controlled private vector was produced in Logic Pro
12.3 from deterministic PCM24 sources. The final project is 48 kHz and four
seconds, with a stereo bed, a mono 997 Hz Atmos object, unity routing, no
creative plug-ins, Smart Tempo/Flex disabled, and 30 explicit object-position
automation events. An initially detected 44.1/48 kHz package-media mismatch
was corrected before the accepted exports; rejected outputs were quarantined.

The final ADM BWF is 11-channel PCM24, 48 kHz, and exactly 192,000 samples. Its
ADM inventory contains two audio objects, 11 track UIDs, one direct-speaker
pack, one object pack, and 197 object position blocks. The known 997 Hz source
matches ADM channel 11 sample-for-sample (`correlation=1`, `gain=1`, residual
RMS 0), proving that the final source/project/ADM path is controlled. Logic's
bed panner distributes the stereo bed across the ten-channel bed, so no
unsupported sample-identity claim is made for the bed channels.

The final 768 kbit/s DD+ Atmos MP4 is 390,839 bytes, SHA-256
`704545f313148412d019a8e7e739fccc0ead345ba7afb4b3b32199fde7b79af0`.
Its independent stream-copy EC-3 is 387,072 bytes, SHA-256
`7ed23a04628c62300a3cc4cee846a308077f8a9117e96366d2b018e6b3ec2249`,
with 126 access units of 1,536 samples. The stream's frame-aligned duration is
4.032 seconds; the authored/ADM duration is exactly four seconds.

OpenJOC reaches all 756 audio-block prefixes with zero unresolved blocks and
classifies one exact `skipfld` Annex H candidate per access unit. Every
candidate parses cleanly with payload IDs 11, 14, 2, and 1. The complete
per-payload configuration inventory shows, consistently across all 126 access
units:

```text
ID 11: group=0 duration=none codecdatae=0 frame_aligned=0
ID 14: group=0 duration=none codecdatae=0 frame_aligned=1
       create_duplicate=0 remove_duplicate=0 priority=0 proc_allowed=0
```

TS 103 420 Table 56 requires `codecdatae=1` and frame alignment for the
profile payloads. Therefore `valid_joc_profile_count=0`,
`complete_joc_profile_count=0`, and
`invalid_or_incomplete_profile_count=126`; the carrier state is
`emdf_profile_incomplete`. Two final release census runs are byte-identical:
JSON SHA-256
`02dec9bb84ae88aac9abb276f95c13467997afda46964bd50d8bd00847d8b78d`
and text SHA-256
`4fab15ce9c736ab4083d1ebffa1936fe373bed6d4f796780b683f2eb28d55e0e`.

This is the first normative blocker. No OAMD parse, JOC parse, object-stem
reconstruction, continuity comparison, or `--internal-base` fidelity run was
performed, and the validator was not weakened. The result is compatible with
a vendor-specific convention, encoder/version defect, or an unresolved public
specification/carriage interpretation; it does not establish deliberate
commercial obfuscation.

### Normative skip-field carrier audit

The authorized raster evidence was rechecked before this increment: TS 102 366
V1.4.1 p.44 shows `skiple`, the 9-bit `skipl` count, and dummy `skipfld` data;
p.117 shows `skipflde`; p.124 places `skiple`, `skipl`, and exactly
`skipl × 8` data bits before the mantissa syntax. TS 103 420 V1.2.1 pp.68-69
provide Tables 55-56, payload IDs 11/14, `addbsi`, and placement rules; TS 102
366 Annex H pp.204-209 provide the exact EMDF synchronization and bounded
container syntax.

The walker reads only the declared `skipl × 8` bits. It retains both the
frame-relative and elementary-stream absolute bit offsets, and it never scans
mantissas or neighboring fields. The Annex H classifier starts at the first bit
of that extracted range only: no `0x5838` at the exact start is ordinary
non-EMDF data; an exact sync start followed by bounded syntax failure is a
malformed candidate; a complete container with undeclared trailing bytes is a
trailing-data candidate. No sliding offset search, cross-field concatenation,
implicit padding, or multiple-container interpretation is implemented.

TS 102 366 calls `skipfld` dummy bytes to ignore, while TS 103 420 does not
expressly identify that field as a JOC/EMDF carrier. Consequently the current
skip-field path is an implemented bounded diagnostic candidate classification,
not proof that `skipfld` is a normative JOC carrier or that all legal carrier
locations have been covered. Profile extraction still requires IDs 11 and 14
with all Table 55/56 restrictions in one candidate container, same-frame
`addbsi`, and last-dependent placement; it never combines separate carriers.

## Implemented increment: normative grouped mantissa carry

TS 102 366 V1.4.1 clause 6.3.5 requires packed bap 1/2/4 groups to continue
across exponent-set boundaries and interleaved other BAP values, with state
reset at each audio-block boundary. `MantissaGroupingState` now retains one
pending group per grouped BAP and is shared by conventional channel, coupling,
and LFE traversal in both complete decode and parse-only carrier inspection.
No code-domain was widened and no arbitrary byte scan was introduced.

Regression coverage includes grouped endpoints, an interleaved bap=3 value,
separate parse-only exponent-set calls, truncation, and malformed code
diagnostics. On the four external DEE fixtures, all six blocks per syncframe
are now bounded, skip-field presence and byte lengths are recorded, and both
malformed mantissa and unresolved-block counts are zero. The subsequent
skip-field classifier finds bounded Annex H candidates but rejects their
incomplete Table 56 JOC configuration. The resulting census state is
`emdf_profile_incomplete`; legal nonzero profile acceptance and the skip-field
carriage interpretation or any additional carrier ambiguity remain open.

## User-supplied DEE fixture evidence

The external fixture is intentionally not committed (stable label:
`forever_friends`).

- SHA-256: `67c10f65642f11713f8495026a37cf26fd1f901e9a343d2e3acf5ee879584896`
- size: 32,138,978 bytes
- FFprobe: one MJPEG video stream and one `eac3` audio stream (index 1), 48
  kHz, six channels, `5.1(side)`, 248.736 seconds
- independent stream-copy EC-3: 31,838,208 bytes, SHA-256
  `2e155599e319d7a6f1ef655684bd872aaae1cd5f73d82097c589a32c572df86a`
- OpenJOC `inspect`: 7,773 frames/access units, 1,536 samples each, container
  accepted; every frame carries `addbsi=[0x01,0x10]`, every normative
  E.1.2.5 frame-end `auxdatae` bit is zero, and the bounded `skipfld` path
  examines one exact candidate range per access unit. Each range parses as an
  Annex H candidate exposing payload IDs 11, 14, 2, and 1; the ID-11
  configuration fails Table 56, so no complete profile is accepted. TS 102
  366 calls these bytes dummy data, and TS 103 420 does not expressly assign
  them as a JOC carrier. If the CLI prints the compatibility phrase “EMDF
  profile absent”, it is bounded to the profile validator and must be read
  together with the carrier counts.
- ISO BMFF inspection found one `ec-3` audio track, no dependent substream,
  and no separate metadata/JOC box or recognized metadata box. Under TS
  103 420 §8.2 the complexity index in `addbsi` is not a substitute for a
  valid EMDF profile. The bounded audio-block walker reaches all six blocks,
  records each declared `skipfld` range, and passes only that exact range to
  Annex H. No sliding search, cross-field concatenation, or implicit padding
  is used. The ID-11 Table 56 failure is a profile validation result, not
  proof that this programme is a legal JOC acceptance vector.
- default FFmpeg base path: six-channel 48 kHz f64 PCM, 11,939,328 samples per
  channel (temporary WAV SHA-256
  `a065dc5d303b44e97943d3d8fa95e784559f157b2c220208112fd31b4a5997e2`)
- `--internal-base`: the current command stops before base synthesis because
  the required OAMD/JOC EMDF profile is not located in the currently validated
  carriers. The earlier mantissa-code failure is resolved, but the
  FFmpeg-versus-internal-base comparison is still not available and
  internal-base fidelity is not verified

FFmpeg `astats` on that reference WAV reports the expected `5.1(side)` order
(FL, FR, FC, LFE, SL, SR), 11,939,328 samples/channel, and these peak/RMS
levels in dBFS:

```text
FL  peak -14.066079  RMS -29.027150
FR  peak -11.644446  RMS -27.419704
FC  peak  -3.850901  RMS -21.360071
LFE peak -33.119901  RMS -50.094647
SL  peak  -3.784534  RMS -20.646557
SR  peak  -1.605351  RMS -20.007338
```

The internal decoder emitted no PCM because no complete Table 55/56 JOC/OAMD
profile was accepted from the currently validated carriers; delay, per-channel
numerical error, and an internal peak/RMS vector are explicitly `not
available`. This is the required comparison failure record, not a pass.

This is a useful container/diagnostic failure report, not a completion claim.
The nonzero JOC/OAMD acceptance lane remains open until a legal DEE fixture
with actual EMDF OAMD/JOC payloads is supplied or independently located from
authorized sources; the carrier extraction and internal base path must then be
validated against known ground truth.

## Implemented increment: wave output semantics

`openjoc-wave` now exposes an explicit `SampleFormat` abstraction for f32, f64,
s24, and s16. CLI object stems default to f32; `--reference-f64` selects the
lossless reference representation. Integer output requires an explicit
clipping policy (`Reject` or `Hard`) and an explicit dither policy (`None` or
seeded triangular one-LSB dither); no integer clipping or dither is implicit.
The compatible base-channel debug artifact remains an explicit FFmpeg
`pcm_f64le` reference and is named `debug/compatible_base.wav`.

Wave tests cover all four output formats, integer range handling, dither
reproducibility, and f64 compatibility. This increment does not change the
renderer-independent scene boundary or claim nonzero JOC/OAMD reconstruction.

## Implemented increment: renderer-independent trim retention

`ObjectScene` now retains each decoded OAMD trim snapshot in a timed
`trim_timeline`, including warp mode, global trim mode/configurations, centre,
surround, height, top/bottom and listener-Y controls, and per-object trim
disable flags. CLI scene artifacts write this data to
`metadata/trim_timeline.json`; no speaker or binaural rendering behavior is
implied. Scene validation checks trim timing, object cardinality, and finite
custom controls. Assembly and JSON roundtrip tests cover the retained state.

## Implemented increment: frame-local atomic staging

`SceneBuilder::append_frame` now stages the current frame's metadata and trim
updates before mutating the accumulated scene. It validates PCM finiteness,
timing bounds, trim cardinality, and numeric controls first, then appends PCM
and metadata without cloning prior object audio. `PayloadDecoder` likewise
uses the atomic builder in place; only its bounded JOC codec state is copied
for retry semantics. A later CLI file-sink increment is still required to
avoid retaining the complete input, base PCM, scene PCM, and debug frames.

## Implemented increment: borrowed frame sink

`PayloadDecoder::decode_frame_with` now lends each committed
`DecodedPayloadFrame` to a callback instead of requiring callers to retain an
owned frame result. The E-AC-3 aligned and `--internal-base` command paths use
the callback to write `debug/frame_NNN` artifacts immediately, so the CLI no
longer builds a complete `Vec<DecodedPayloadFrame>` before writing debug
output. The callback is invoked only after codec, OAMD, JOC state, and scene
staging commit; sink failures are reported explicitly and do not claim to
roll back committed decoder state.

This is a bounded-retention increment, not the complete streaming design. The
current CLI still loads the elementary stream and compatible base WAV into
memory, and `SceneBuilder` still retains the complete reconstructed scene PCM
for the renderer-independent `ObjectScene`. Metadata-only scene assembly and
streaming WAV sinks remain open and are tracked separately in the requirements
matrix.

## Verification commands

The current container and diagnostic checks were run with:

```text
cargo test -p openjoc-container
cargo test -p openjoc-cli --test container -- --nocapture
cargo test -p openjoc-cli
```

With the local ignored manifest selected, the opt-in corpus test also passed.
The controlled Logic vector was then run twice through the release census:

```text
OPENJOC_REAL_FIXTURE_MANIFEST=<local manifest> cargo test -p openjoc-cli --all-features fixture_census -- --nocapture
OPENJOC_REAL_FIXTURE_MANIFEST=<local manifest> cargo run -p openjoc-cli --release --offline -- --no-banner census <local manifest> -o <local report directory>
```

The latest full tracked-workspace quality gates were rerun in the main worktree
on 2026-08-04 and passed. No private source, project, ADM, encoded programme,
manifest, census output, or screenshot was added to the repository. The test
and build commands used `CARGO_BUILD_JOBS=1` and serialized test execution:

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
CARGO_BUILD_JOBS=1 cargo test --workspace --all-features -- --test-threads=1
CARGO_BUILD_JOBS=1 cargo build --workspace --release --offline
git diff --check
```

This includes the workspace-wide all-feature test suite, strict clippy, the
offline release build, and a clean diff check. Rust 1.94's new
`needless_range_loop` findings were corrected mechanically in coupling tests
and interpolation state assembly; behavior is unchanged. The production
diagnostic change adds per-payload configuration fields to census JSON/text.

## Revision and verification ledger

- Container implementation: `74faa5600387a34b073513cef62a987925d3d86d`.
- Wave output formats: `42e4af73da0fbd02574ab23e62a265f3a61089ea`.
- Trim retention: `2303f33a2b7e86656020992015a1f88ab5ec1e6f`.
- Frame-local scene staging: `241cb0334ec735dde7516613db0987b101b86f06`.
- Borrowed per-frame debug sink: `a2b96aa4475da3522794c82e189c5602046dcf3b`.
- Bounded E-AC-3 carrier inspection and first-failure diagnostics:
  `d38c00c81740db15062256d1c5651c40a295f279`.
- External multi-fixture census harness and CLI report integration:
  `7f43db05d6876314d8d5ec5415840605c3204d54`.
- Opt-in manifest-gated external census test:
  `b9cab25a5df0e8ab3b3344dd2cbad71f7c120017`.
- Grouped mantissa carry and complete four-fixture audio-block traversal:
  `2c524d107ae7451b2a6c838e7ca64159a51b375b`.
- Controlled Logic vector census configuration inventory:
  `79e659b6f2cc654c6f7eba5a21165f0a277b88c5`.
- Rust 1.94 clippy compatibility cleanup:
  `fbfc56b8d4f317017bab348559a687a66ca1201d`.
- Grouped traversal evidence/documentation: documentation-only commit
  `a8e736eb7097fb1d2e76be52f9dca68b58d0cfaa`.
- Container evidence/status audit: documentation-only commit
  `cf9dcd4bbb31e13dd6f47c807aba15f6e0460c30`.
- Earlier evidence/status documentation commits:
  `f19a0cc6c56d54788dad2b796e2237bea975de7f`,
  `53ef9b0dcea6342edb8d9c975144fb65050fef03`, and
  `96760273fd5544a072aaf7859a4d83cb284ab51d`.
- Bounded skip-field/Annex H carrier increment: test and implementation
  commits `88c2fb8ce6f334330a6d0f79608823035c06a574`,
  `d900ef13c3c3977d6f0cd861d00293d002f00006`,
  `3447bc7837fec1f6b13f92814d8e9edf3784f244`,
  `b234caa6bde1339e7a48c585b612e4534cc36305` (frame-end classification),
  `bb2d6cb294061ac6a19ca5949e3e0de7ee41d374`,
  `bb39eb3a1e475bcc8587d944f5a1c5c2207fc8d`,
  `c1f3fa621f9712df04cd3ebfcffed80f83619f1d`,
  `3662b21549ed387d487f92dd12a86fe26e6f8920`, and
  `4b3d061c540b8e9f43df12632b7cda4a43a6c692`.
- The previous quality-gate ledger remains tied to code HEAD
  `4b3d061c540b8e9f43df12632b7cda4a43a6c692`. The 2026-08-04 quality-gate
  evidence applies to code HEAD
  `fbfc56b8d4f317017bab348559a687a66ca1201d`. Implementation and
  status/documentation commits remain distinct.

## Known limitations and next goals

All-carrier EMDF discovery, the real-vector acceptance lane,
FFmpeg-versus-internal-base fidelity report, metadata-only scene assembly, and
streaming PCM/file sinks remain open. The borrowed frame sink only removes the
all-frame debug vector; it is not the complete constant-memory streaming
design. Speaker and binaural renderers are later non-normative components and
are deliberately outside the current decoder increments.
