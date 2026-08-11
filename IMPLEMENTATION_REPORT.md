# OpenJOC implementation report

## Evidence boundary

The repository currently supports a clean-room raw E-AC-3 path with aligned
base-channel PCM, JOC/OAMD extraction, a metadata-only `ObjectScene`, and a
separately named diagnostic `ReconstructionBasis` row export. Reconstruction
rows are not verified authored-object PCM; semantic audio binding is
explicitly unresolved.
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
configuration fails the strict profile (`codecdatae=0`,
`payload_frame_aligned=0`), so no complete ETSI_STRICT profile is accepted.
The observed Logic/Dolby pattern is admitted only by the explicit vendor
profile and only with deviations. TS 102 366 describes `skipfld` as dummy
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
`emdf_profile_incomplete`. DOLBY_VENDOR_COMPAT accepts this exact pattern with
seven recorded deviations and preserves the bytes for the decoder layer. Two
final release census runs are byte-identical:
JSON SHA-256
`52302b6fee68e5ad4bcf1c3bbc4c526077efb223126a975c37a732b010035432`
and text SHA-256
`5b94f9d45faba8f62a2260fb9ad34857c62a82fd60f8871e29cb75cb2f04f928`.

At the initial strict-only gate this was the first normative blocker: no OAMD
parse, JOC parse, object-stem reconstruction, continuity comparison, or
`--internal-base` fidelity run could begin, and the validator was not weakened.
The result is compatible with a vendor-specific convention, encoder/version
defect, or an unresolved public specification/carriage interpretation; it does
not establish deliberate commercial obfuscation.

## Implemented increment: explicit ETSI/vendor profile boundary

The Logic result is now represented by two independent validations over the
same parsed, unmodified EMDF container:

```text
parse -> ParsedJocAccessUnit/JocPayload
      -> validate(ETSI_STRICT)
      -> validate(DOLBY_VENDOR_COMPAT)
      -> decode
```

For all 126 access units, `ETSI_STRICT` fails with seven evidence records:
`codecdatae=0` on payloads 11 and 14; `payload_frame_aligned=0` on payload 11;
and absent payload-11 duplicate, priority, and processing controls where the
strict profile requires zero. The vendor profile reports the same seven
deviations and returns `accepted_with_deviation`. No bytes are rewritten and
no decoder branch inspects vendor flags to silently normalize them.

The private manifest declares these expected results. The release census was
run twice against the same SHA-256-pinned MP4; both JSON and text reports were
byte-identical. The manifest schema also accepts the same two expected profile
results for future Dolby Encoding Engine fixtures, while keeping all media
outside the repository.

The raw Logic EC-3 then reaches the decoder's OAMD boundary under
`DOLBY_VENDOR_COMPAT`. E-AC-3 `decode` now accepts the caller-defined
`--trim-config-count N` explicitly, matching the existing payload decoder API.
Across controlled candidate counts 1, 2, 3, 4, 5, 6, 8, and 10, the first
downstream error is the same reserved OAMD warp mode 3. This is an OAMD syntax
boundary, not a JOC profile failure; no count or compatibility fallback is
guessed or hidden.

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
  configuration fails the ETSI_STRICT Table 56 profile, so no complete strict
  profile is accepted. TS 102
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
  the required ETSI_STRICT OAMD/JOC EMDF profile is not located in the
  currently validated carriers. The earlier mantissa-code failure is resolved,
  but the
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
s24, and s16. CLI diagnostic reconstruction-row WAVs default to f32;
`--reference-f64` selects the
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
implied. Scene metadata objects are separate from the diagnostic
`ReconstructionBasis` rows written under `diagnostics/`; the scene carries an
explicit `semantic_binding: unresolved` state by default. No row is exported
as a verified authored-object stem.

## Implemented increment: frame-local atomic staging

`SceneBuilder::append_frame` now stages the current frame's metadata, trim
updates, and reconstruction rows before mutating the accumulated scene. It
validates row finiteness, timing bounds, trim cardinality, and numeric controls
first, then appends rows and metadata without cloning prior reconstruction
audio. `PayloadDecoder` likewise uses the atomic builder in place; only its
bounded JOC codec state is copied for retry semantics. The row export is
diagnostic and carries no authored-object identity.

## J1R14 — Normative OAMD timeline/state admission (2026-08-10)

The normative OAMD state audit is scoped to the existing parser and
metadata-only scene path. TS 103 420 clauses 5.3.1-5.3.2 and 5.5.5-5.5.11
remain the authoritative source for shared update timing, object-major syntax,
defaults, full reuse, mixed update/reuse, inactive handling, and
previous-object gain. Existing parser tests cover those transitions; timing
state advances 1,536 samples per successfully decoded codec frame, with
atomic failure and explicit reset behavior.

One production ordering defect was corrected: `SceneBuilder` previously
flattened object-major storage directly into `ObjectScene.metadata_timeline`,
which produced `t0,t1,t0,t1` for two objects and two shared timing blocks. The
assembler now retains object-major parser storage but emits the timeline
block-major (`t0,t0,t1,t1`) and has a regression test for that invariant. No
field is guessed, omitted state is invented, or cross-AU semantic binding is
performed. `SemanticBindingState` remains `Unresolved`; authored-object PCM,
ObjectScene audio binding, complete trim semantics, and warp-3 interpretation
remain out of scope.

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

## Implemented increment: bit-exact OAMD entry forensic trace

The next downstream failure was investigated without changing either profile or
decoder semantics. `openjoc diagnose-oamd FILE -o DIR` now emits a JSON and a
text report from a separate observational trace layer. The command accepts an
explicit `--access-unit N`, `--all-access-units`, and caller-supplied
`--trim-config-count N`; the trim count remains an experimental diagnostic
parameter and is never inferred or turned into a hidden decoder rule.

The trace names every coordinate system. All spans are MSB-first, half-open
`[start_bit,end_bit)` ranges: frame-relative `skipfld`, original-file,
elementary-stream, access-unit, bounded skip-field, EMDF-container, and
OAMD-payload coordinates are kept separate. For ISO BMFF, FFprobe packet
`size,pos` rows are checked against the exact stream-copy byte sequence, so the
MP4 sample index and original-file bit offset are present only when that
mapping closes. Payload reports include ID, configuration, size, and body spans
in every mapped coordinate; OAMD reports include payload, element, warp-field,
64-bit surrounding window, and elementary/original-file byte dumps.

Against the private controlled Logic vector, all 126 access units were traced
from both the raw EC-3 and the MP4. The MP4 packet mapping is one packet/sample
per access unit, sample indices 0 through 125, and the raw and demuxed stream
bytes are length-identical. In every AU:

```text
EMDF payload IDs: 11, 14, 2, 1
payload-11 body: 536 bits (67 bytes), config: 9 bits
OAMD payload: EMDF bits [60,596), 536 bits
OAMD top-level elements: ID 1 then ID 2; object_count=16; element_count=2
trim element body: OAMD bits [525,533)
warp field: OAMD bits [526,528), raw value=3
validator with trim-config-count=1: reserved OAMD warp mode 3
```

The bounded skip-field length equals the complete EMDF container length in all
126 observations; every payload body span equals its declared byte length, and
the four payload boundaries close without padding or cross-carrier reads. The
payload configurations are repeated in every AU rather than inherited in this
vector: ID 11 has `group=0`, `codecdatae=0`, `payload_frame_aligned=0`; ID 14
has `group=0`, `codecdatae=0`, `payload_frame_aligned=1` with zero duplicate,
priority, and processing controls; ID 2 is discarded; ID 1 carries duration
1536. Payload-11 body changes first at AU 15, while the OAMD entry geometry and
warp value remain unchanged. Earlier explicit trim-count experiments 1, 2, 3,
4, 5, 6, 8, and 10 all reached the same first downstream error.

The initial trace implementation was itself corrected: the trim warp field is
after the element's `discard_unknown` bit (and any alternate-data ID), not at
the first body bit. A focused regression fixture proves that the trace records
raw value 3 at the exact bit while the normative parser still returns
`ReservedWarpMode{code: 3}`.

ADM BWF inventory remains an external oracle only: it records 11 channels,
two ADM objects (one direct-speaker master and `OBJ_997HZ`), 197 object-position
blocks, 48 kHz, and 192,000 samples. The current OAMD entry trace is consistent
and bounded, but it does not yet decode the object element through the rejected
trim element; no object-scene, PCM, ADM waveform, or fidelity claim is made.

This is conclusion C for the current evidence set. A stable raw value of 3 is
established across all AUs of one Logic export and its MP4/raw representations,
but no second independent real encoder/sample has been supplied. Therefore no
Dolby compatibility syntax rule, warp remapping, offset constant, or reserved
value exception is added. At that dated round, the first real blocker was the exact OAMD
`ReservedWarpMode{3}` boundary. Private reports are retained outside Git at
`OpenJOC-Private/reports/oamd_forensics_raw` and
`OpenJOC-Private/reports/oamd_forensics_mp4`; their final JSON SHA-256 values
are respectively
`fe4cace04ce7cf5a33515ae16e6ecedb69cb9379b3eb9b367eca8d2147fc2b32` and
`0978b86e1dc908645d1453c8c126a22e18567c673fc7ec17d64fff88dee9ba46`.

The 2026-08-05 quality gate for this increment passed
`cargo fmt --all -- --check`, workspace strict clippy, the serialized
all-feature workspace test suite, the offline workspace release build, and
`git diff --check`. The private manifest census was run twice after the release
build and remained byte-identical (`52302b6f…5432` JSON and
`5b94f9d4…f928` text).

## Known limitations and next goals

### Round-2 controlled Logic OAMD evidence (2026-08-05)

The round-2 diagnostic lane is deliberately separate from decoding.  It adds
`diagnose-oamd --au START..END --diff-payload-11 --json PATH` and an explicit
`--warp-hypotheses --adm-reference PATH` diagnostic report.  The independent
oracle starts at the bounded payload-11 body and does not share either formal
OAMD or trace-layer cursors.  On all 126 raw and MP4 access units it reports
the same payload-relative warp span `[526,528)`, raw bits `11`, integer value
`3`, and payload closure at bit 536; its direct byte/mask extraction agrees.

The time grid is exact: 1,536 samples / 48,000 Hz = 0.032 seconds.  Zero-based
AU 15 starts at sample 23,040 (0.480 seconds) and is the first payload-11 hash
transition.  AU 14, 15, 16, and 17 are reported explicitly, as are later
transitions.  The raw and MP4 payload-11 hash sequences and warp spans are
identical; this is a carrier-path equality check, not a file-container hash
claim.

The external ADM BWF oracle was read without changing the existing inventory.
`OBJ_997HZ` has 197 object block formats in Cartesian coordinates.  An ADM
block exists at 0.480 seconds and an explicit position boundary begins at
0.500 seconds.  This records timing/ordering evidence only; no Cartesian-to-
OAMD conversion or fidelity conclusion is made.

For raw warp 3, diagnostic-only hypotheses 0, 1, and 2 all close the bounded
top-level element and payload windows, retain `raw_warp=3`, and remain
non-unique because the object-element update/position grammar has not been
decoded.  They therefore report no update/position/jump/ramp counts and no
ADM semantic correspondence.  No production remap or vendor exception was
added.  The official TS 103 420 V1.2.1 Table 32 still defines `0b1X` as
reserved; no official erratum changing that table was found in the permitted
ETSI PDF/companion-file and public ETSI deliverable search.  The strict parser
continues to return `ReservedWarpMode{code: 3}`.

Computer Use successfully created the private copy
`OpenJOC-Private/logic/warp-study/Vector_D_existing_mixed_motion.logicx`
through Logic Pro Save As.  The later A/B/C/E/F copies and exports are
retained outside Git; B/C are explicitly non-canonical because their ADM
exports still contain D's mixed automation.  No private media, manifest,
census, forensic, or ADM file is committed.

All-carrier EMDF discovery, the real-vector acceptance lane,
FFmpeg-versus-internal-base fidelity report, metadata-only scene assembly, and
streaming PCM/file sinks remain open. The borrowed frame sink only removes the
all-frame debug vector; it is not the complete constant-memory streaming
design. Speaker and binaural renderers are later non-normative components and
are deliberately outside the current decoder increments.

## Controlled Logic warp-study corpus (2026-08-05)

This increment started at commit `13306818f854ab29709bac27929194f1442b1b6a`
and kept the pre-existing `.DS_Store` and `references/` entries untouched.
Computer Use was used for Logic Pro project duplication, track/automation
editing in copies, and all spatial exports. The private run is
`OpenJOC-Private/reports/runs/2026-08-05T004530Z_vector-corpus_1330681`.

The corpus contains A static-centre, B requested single-jump, C requested
linear-ramp, D existing mixed motion, E no dynamic object, and F two objects.
A, D, E, and F satisfy their stated control semantics. The Logic UI copy
operation for B/C retained D's mixed automation; their ADM exports contain
197 `OBJ_997HZ` blocks and identical payload-11 transition structure, so they
are explicitly marked non-canonical B/C evidence rather than being treated as
single-variable proof.

Every vector has 126 AUs, 48 kHz, 1,536 samples/AU, and exactly 0.032 s/AU.
The payload-11 body is unique once for A/E and 63 times for B/C/D/F. The first
changed body for B/C/D/F is zero-based AU 15, start sample 23,040, time
0.480 s; the report includes AU 14/15/16/17 and all observed transitions.
Normalized raw-EC3 versus MP4 observations are equal for all six vectors, and
each MP4 stream-copy EC3 has the same hash as its raw EC3 input. This excludes
carrier demux/offset drift for the bounded fields but does not imply semantic
OAMD decoding.

The static and no-dynamic-object vectors are important negative controls:
both still emit payload 11 with warp raw `3` in all 126 AUs. F has two ADM
objects and still has the same warp distribution. Thus the current data is
consistent with an encoder/profile-level convention, but it does not identify
the convention's meaning.

### OAMD entry decision

Four independent observations agree on the first failure:

```text
payload-relative warp span: [526,528)
raw bits: 11
raw integer: 3
formal ETSI result: ReservedWarpMode { raw: 3 }
elements: ID 1 then ID 2, exact bounded closure
object_count: 16
```

The direct byte/mask calculation and the independent test oracle do not share
the formal or diagnostic cursor. The three diagnostic-only hypotheses
(assuming semantic 0, 1, or 2 while retaining `raw_warp=3`) all close the
bounded element and payload and all remain non-unique: no update/position/
jump/ramp count is available before the normative object grammar is entered.
No production remap, offset magic, hidden trim selection, or vendor warp
compatibility rule was added. `ETSI_STRICT` behavior is unchanged; no
`DOLBY_VENDOR_COMPAT` warp extension was added. OAMD timeline, JOC parsing,
nonzero PCM, ADM position comparison, and fidelity remain unverified.

### ADM oracle boundary

The ADM BWFs were read from RIFF/axml/chna without modification. A has one
static `OBJ_997HZ` block, B/C/D have 197 `OBJ_997HZ` blocks, E has no object
channel, and F has 197 blocks each for `OBJ_997HZ` and `OBJ_2003HZ`. The ADM
reports retain Cartesian coordinates, jump-position attributes,
interpolation lengths, gain, and update times. No unproven conversion to an
OAMD coordinate system is performed, and no fidelity claim is made while the
OAMD object-element parser is blocked.

### Forensic report overwrite protection

`diagnose-oamd` now refuses to overwrite either JSON or text report targets by
default and returns `AlreadyExists`; an explicit, auditable `--force` is
required for replacement. A regression test covers both refusal and explicit
force behavior. This protects the private forensic history and does not alter
bitstream or decode semantics.

### Quality gates for this increment

- `cargo fmt --all -- --check`: passed
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: passed
- `CARGO_BUILD_JOBS=1 cargo test --workspace --all-features -- --test-threads=1`: passed
- `CARGO_BUILD_JOBS=1 cargo build --workspace --release --offline`: passed
- `git diff --check`: passed
- controlled private manifest run 1/run 2: JSON and TXT byte-identical
  (`52302b6f…5432`, `5b94f9d4…f928`)

### Round-3 Logic differential refresh (2026-08-05)

This refresh started at `952b052d61e23e5b7c7d96d37b41a01f090424b7` on
`codex/logic-warp-differential-corpus`. The tracked worktree was clean; the
pre-existing untracked `.DS_Store` and `references/` entries were not touched.
The recorded environment is macOS 26.6 (25G72), Logic Pro 12.3, Rust/Cargo
1.94.0, and FFmpeg/FFprobe 8.1.2.
Computer Use reopened Logic Pro 12.3 and the private canonical-B copy after
an unsaved editing experiment was discarded. No new B/C export is labelled
canonical in this round; the earlier B/C exports remain explicitly
non-canonical because their ADM and payload evidence retain D's mixed motion.

The new non-overwriting batch is
`OpenJOC-Private/reports/runs/2026-08-05T1042Z_logic-warp-evidence_952b052`.
It contains raw-EC3 and MP4 reports for A static-centre, B single-jump-copy,
C linear-ramp-copy, D mixed-motion, E no-dynamic-object, and F two-object
controls, plus a fresh RIFF/axml/chna ADM report for each vector. Every carrier
report has 126 access units and `timing_grid_seconds=0.032`; raw and MP4
normalized observations are equal. Payload-11 unique counts are A=1, B=63,
C=63, D=63, E=1, F=63. Where a transition exists, the first transition is
AU 14 -> AU 15 (zero-based AU 15 starts at sample 23,040 and 0.480 s).

The independent oracle and direct byte mask agree for every one of the 12
carrier reports: OAMD payload-relative warp `[526,528)`, raw bits `11`, raw
integer `3`, payload end bit `536`, element IDs 1 and 2, and exact bounded
closure. Diagnostic hypotheses 0, 1, and 2 each close only bounded syntax,
retain `raw_warp=3`, and are explicitly marked non-unique and diagnostic-only;
none produces update, position, jump, or ramp counts. ADM summaries remain an
external oracle: A has one `OBJ_997HZ` update, B/C/D have 197, E has no object
channel, and F has 197 updates for each of `OBJ_997HZ` and `OBJ_2003HZ`.

The decision remains unresolved-unknown. No parser offset fix, warp remap,
trim magic, hidden compatibility branch, or `DOLBY_VENDOR_COMPAT` warp rule
was added. `ETSI_STRICT` still rejects `ReservedWarpMode { code: 3 }`; OAMD
timeline generation, JOC reconstruction, nonzero PCM, object-scene comparison,
and ADM fidelity remain blocked at that exact boundary.

Round-3 quality gates passed: `cargo fmt --all -- --check`, strict workspace
clippy, serialized all-feature workspace tests, offline workspace release
build, and `git diff --check`. The private manifest census was run twice into
new output directories; JSON hash `52302b6fee68e5ad4bcf1c3bbc4c526077efb223126a975c37a732b010035432`
and TXT hash `5b94f9d45faba8f62a2260fb9ad34857c62a82fd60f8871e29cb75cb2f04f928`
matched byte-for-byte.

## Round-4 bounded Dolby vendor partial-metadata path (2026-08-05)

This increment keeps `ETSI_STRICT` unchanged and adds an explicit
`DOLBY_VENDOR_COMPAT` OAMD parser path. When the validated payload-11 carrier
contains top-level element 1 followed by a complete element-2 trim window and
the formal trim parser's first error is exactly `ReservedWarpMode { code: 3 }`,
the complete declared element-2 body is retained as
`OpaqueObservedKnownElement`. The raw body, declared length, valid final-byte
bits, SHA-256, raw warp value/range, first error, and deviation code
`LOGIC_OAMD_RESERVED_TRIM_WARP_3` are preserved. No warp remap, default trim,
semantic hypothesis, or hidden offset is used; trim timeline remains
unavailable and renderer fidelity is ineligible. Other reserved values,
truncation, malformed preceding elements, invalid carrier ID, or unrelated
errors do not enter the fallback.

The stateful scene decoder now accepts this representation only through an
explicit profile. Element 1 remains formally parsed while element 2 is
opaque, and the same access unit's payload 14 is parsed independently. On
private A/E/D raw and MP4 carriers, all 126 AUs reach this boundary with
`accepted_with_deviation`, 126 opaque element-2 bodies (one unique body hash
`8d33f520a3c4cef80d2453aef81b612bfe1cb44c8b2025630ad38662763f13d3`), element
1 object count 16 (15 dynamic), one metadata block and 16 object updates.
Payload 14 formally parses 126/126 with five-channel downmix, 15 output
objects, full matrices, 900 codewords per AU, and nonzero codewords; the first
real downstream blocker is the explicit cross-chain mismatch `JOC declares 15
objects but OAMD declares 16`. No object PCM or ObjectScene is emitted from
these real vectors, and no fidelity claim is made.

`inspect` always prints both carrier validation profiles. With the explicit
`--trim-config-count N` option it also prints the corresponding OAMD strict or
vendor partial status; without that caller-supplied count it reports that OAMD
partial parsing was not attempted, rather than inferring a trim cardinality.

After the A/E/D gate, a new non-overwriting private B/C/F regression run
(`2026-08-05T_vendor-opaque-bcf.Y072QA`) covered both raw EC-3 and MP4. Each
carrier had 126 AUs, 63 unique payload-11 bodies, first change AU 14 -> 15,
strict stage `trim.warp_mode` in 126/126, vendor opaque acceptance in 126/126,
raw warp `3` in 126/126, and payload-14 parse success in 126/126 with output
object count 15. Normalized raw/MP4 observation sequences were equal for all
six reports. This extends the opaque-boundary evidence without asserting that
B/C are canonical single-variable Logic exports.

Raw/MP4 normalized metadata sequences are byte-identical for A/E/D. Strict
forensic stages remain `trim.warp_mode` for 126/126; the existing carrier
profile deviations are not relaxed. The private non-overwriting run is
`OpenJOC-Private/reports/runs/2026-08-05T0358Z_vendor-opaque-2f5de17`.

## Round-5 typed programme layout and first nonzero PCM (2026-08-05)

This round resolves the cross-chain cardinality boundary without changing
either JOC syntax or ETSI validation. The previous generic equality check
(`JOC declares 15 objects but OAMD declares 16`) is replaced by three scoped
checks:

```text
addbsi complexity == total OAMD programme count
JOC output count  == OAMD dynamic-slot count
scene entry count == total OAMD programme count
```

`ProgrammeLayout` derives typed structural `ProgrammeLayoutEntry` values from
`OamdContentPrefix::object_anchors()`. For every private A-F Logic carrier the
observed layout is `RcLfe` at OAMD index 0 followed by 15 dynamic anchors. The
structural source categories for this layout are therefore:

```text
OAMD[0]     -> ProgrammeAudioSource::BaseLfe { channel_index: 0 }
OAMD[1+i]   -> ProgrammeAudioSource::ReconstructionRow { row_index: i }, i=0..14
```

This is an anchor/order-derived structural layout, not a `count - 1` exception
and not semantic authored-object/audio binding. No `bind_audio` fallback is
available in production. Ordinary beds and ISF anchors remain explicitly
unsupported at this boundary; multiple LFE entries, misplaced LFE, and
JOC/dynamic-slot mismatch have dedicated structural errors. The layout is
checked again against the OAMD prefix before a frame can commit, while the
scene retains rows and base LFE separately.
Ordinary beds and ISF anchors remain explicitly unsupported at this boundary;
multiple LFE entries, misplaced LFE, JOC/dynamic-slot mismatch, missing or
unequal LFE PCM, duplicate rows, and row bounds have dedicated errors. The
layout is checked again against the OAMD prefix before a frame can commit.
`PayloadDecoder` stages layout validation, JOC state, scene metadata, dynamic
PCM, and optional LFE together, preserving frame retry atomicity.

The compatible-base CLI now probes the six-channel E-AC-3 input, exports the
five non-LFE channels as `FL,FR,FC,SL,SR`, and retains a separate LFE WAV. The
LFE is passed to the scene boundary only; it never enters the five-channel JOC
QMF matrix. No LFE audio is fabricated when the source is unavailable. The
scene model serializes the speaker-anchored entry as `lfe` and dynamic entries
as `dynamic`.

Private run
`OpenJOC-Private/reports/runs/2026-08-05T044606Z_object-cardinality_a4f88af_r3`
contains A-F forensic reports and A/E/D/F compatible-base decodes. Each
decoded vector has 126 AUs, 48 kHz, 1,536 samples/AU, 193,536 total samples,
16 scene entries, 15 JOC-reconstructed dynamic signals, and one base-carried
LFE entry. The LFE WAV is retained as evidence and is silent for the tested
source (measured peak/RMS zero); that result is not a synthetic zero stem.
The first dynamic stems are nonzero (for example A row 1 has peak about
0.603 and RMS about 0.172; D/F show strong 997 Hz energy), while later slots
may be PCM-silent. All non-finite counts are zero and AU-boundary continuity
metrics are recorded in the private JSON/TXT reports.

The OAMD `active` flag is kept separate from ADM object count and PCM energy.
In the observed frames all 15 dynamic slots are flagged active, including E,
whose ADM BWF contains zero dynamic object channels. F's two ADM names and
997/2003 Hz measurements are retained only as partial evidence; energy is
distributed across multiple rows and does not establish a unique row-to-name
mapping. The private reports explicitly record this codec-slot-capacity versus
ADM-content distinction.

The strict/vendor boundary is unchanged: `ETSI_STRICT` still fails the raw
OAMD warp value 3, and `DOLBY_VENDOR_COMPAT` retains the declared trim body
opaqely with raw value 3 and deviation code
`LOGIC_OAMD_RESERVED_TRIM_WARP_3`. No trim timeline, warp semantics,
speaker-render fidelity, complete OAMD semantic timeline, or
`--internal-base` fidelity claim is made. The first blocker after layout and
JOC/QMF scene assembly is now unresolved trim/warp semantics and the absence
of a validated ADM/render comparison, not object cardinality.

## Round-6 internal-base numerical comparison (2026-08-05)

This increment adds an auditable base-only sink to the internal E-AC-3 path.
For each AU the sink is called only after the corresponding JOC frame commits,
so `internal_base_full.wav` cannot advance after a failed frame. The exported
orders are explicit: full base `FL,FR,FC,LFE,SL,SR`; JOC input
`FL,FR,FC,SL,SR`; separate `LFE`. The accumulator checks AU sequence, sample
rate, channel count, and frame lengths and records 126 x 1,536 samples for
each A/E/D/F vector.

The private evidence run is
`OpenJOC-Private/reports/runs/2026-08-05T053438Z_internal-base-fidelity_dcfb56c`.
FFmpeg 8.1.2 compatible-base is generated with `-map 0:a:0`, explicit `pan`
filters, `pcm_f64le`, and no resampling. Its default dialnorm/dynrng
presentation behavior is recorded rather than copied into the internal path.
OpenJOC uses `--internal-base --reference-f64 --trim-config-count 1` with the
explicit vendor profile; warp=3 remains raw/opaque and no new compatibility
rule is present.

The strict numerical result is deliberately separated from semantic claims:

| plane | result |
| --- | --- |
| base PCM | all vectors diverge at zero delay in AU 0/block 0; FL/FR/FC first samples 9/9/7, SL/SR 12/5; raw SNR about 84.5--90.6 dB front/centre and 38.8--51.3 dB side; LFE exactly silent |
| JOC propagation | same payload 11/14 and state are run through both bases; per-row/AU/block metrics are private, but no threshold converts them into fidelity acceptance |
| OAMD activation | 16 scene entries (1 LFE + 15 dynamic) are emitted under vendor compatibility; this is not a complete OAMD timeline |
| ADM position/trim | not compared; warp semantics remain unresolved |

The A/E/D/F vectors contain 15 dynamic codec rows. Frequency matrices use the
manifest's 440/659/997/2003 Hz source set and report target/off-target energy.
F's two ADM object names are not treated as a proven row map because energy is
distributed across rows; E is retained as a no-dynamic-object codec-capacity
control. Both report trees (`reports1`, `reports2`) are byte-identical. The
first remaining fidelity blocker is the measurable base-path difference plus
unresolved FFmpeg/internal DRC/dialnorm/delay policy; no JOC, OAMD, or ADM
semantic conclusion is inferred from it.

## Round-7 FFmpeg/internal-base root-cause increment (2026-08-05)

The starting commit was `792d937297f44a1a5d4b25613831fad5e529d572` on
`codex/logic-warp-differential-corpus`; the private run is
`OpenJOC-Private/reports/runs/2026-08-05T070007Z_base-root-cause_792d937`.
The run does not touch repository `.DS_Store` or `references/`, and no media,
manifest, ADM, forensic, census, or fidelity output is tracked.

### Policy matrix

The local FFmpeg 8.1.2 decoder help was captured before running an orthogonal
matrix. R0 is the implicit default; R1/R2 set `drc_scale` to 0/1; R3/R4 set
`cons_noisegen` to 0/1; R5 sets `heavy_compr=0`; R6 sets `target_level=0`.
All outputs are explicit six-channel `pcm_f64le` with
`FL,FR,FC,LFE,SL,SR`, 48 kHz, 193,536 samples. R0/R2/R3/R5/R6 are
byte-identical for every A/E/D/F vector. R4 changes every vector but does not
materially remove the residual. R1 changes E, the vector that carries
dynamic-range metadata, and does not explain A/D/F. No single FFmpeg
presentation option explains the base difference.

### Internal policy boundary

`InternalBasePolicy::CurrentDefault` is the unchanged default path. The new
explicit `CodecCore` policy disables only optional `dynrng/dynrng2` gain; it
does not alter mantissa decoding, coupling, SPX, dither, or transform state.
The synthetic regression proves that both policies retain identical syntax,
BAP, and exponent state while differing exactly at the optional gain stage.
On A/D/F the two internal outputs are byte-identical; on E only the dynamic
range policy changes. The CLI requires an explicit
`--internal-base-policy codec-core` to select the core policy.

### First state-local residual

The global first differing samples remain AU 0/block 0: FL=9, FR=9, FC=7,
SL=12, SR=5. The most informative residual, however, is a later local event:
all four vectors have an SL/SR residual at block 6 (sample 1536, the first
block after the first 1,536-sample syncframe) of approximately `7e-3` RMS,
while neighboring blocks are near `1e-6` RMS. A private transform probe that
resets the overlap state only before AU/frame 1 removes that event and raises
side SNR to approximately 85--90 dB, without changing the front/centre
metrics. This is diagnostic evidence that the large side residual is carried
by TDAC history, not a fixed scalar gain, permutation, sign, DRC option, or
random-noise realization.

The public ETSI TS 102 366 V1.4.1 clause 6.9.4 overlap/add rule uses the
second half of the previous windowed block. Because the private reset probe
would contradict that rule if generalized, no production reset was made.
The remaining blocker is an unresolved first-frame/encoder priming or
decoder-boundary convention in this Logic carrier. The opt-in private stage
inventory records bounded decoded-exponent/BAP/coefficient/transform/window/
overlap evidence and explicitly marks unavailable internal sub-stages; it
does not claim FFmpeg stage equivalence.

Strict OAMD validation, raw `warp=3`, vendor opaque trim handling, and all
unverified OAMD/JOC/ADM/fidelity boundaries are unchanged.

## Round-8 TDAC boundary investigation (2026-08-05)

The increment starts at commit `054d3d4566c46a3ab308d0599eb1215b78171cc2` on
`codex/logic-warp-differential-corpus`. The private run is
`OpenJOC-Private/reports/runs/2026-08-05T_tdac-boundary-corrected_054d3d4`; its
repeated tree is `..._repeat`, and the core JSON/TXT diagnostics plus production
regression hash report compare byte-for-byte. The private PCM tree used for the
hash comparison remains in the earlier
`2026-08-05T_tdac-boundary_054d3d4/internal_rerun` directory. Repository
`.DS_Store` and `references/` remain untracked and untouched.

### Normative model and state audit

ETSI TS 102 366 V1.4.1 clauses 5.2.11, 6.9.3, and 6.9.4 (PDF pages 51 and
82--85) define a windowed 512-sample block, overlap of its first half with the
previous block's second half, `pcm[n] = 2 * (x[n] + delay[n])`, and
`delay[n] = x[N/2+n]`; Table 6.33 (PDF page 86) supplies the symmetric window
sequence. Saturation arithmetic is normative; the current f64 path did not
encounter overflow in this run, so saturation was not used to classify the
boundary. Hashes, f64 arrays, and JSON are diagnostic implementation choices.

The state key is codec channel index, not programme/object index. Full-band
channels each own a 256-sample delay; LFE owns a separate delay. A call clones
the existing state, advances staged state block-by-block, and commits all
channels only after successful synthesis. A failed call or retry cannot expose
partial state. Independent and dependent JOC substreams use separate
synthesizers, and there is no AU-boundary reset. Previous/current block-switch
flags are retained only to make the opt-in trace auditable; they are not a
hidden compatibility rule.

### Contribution evidence

`AudioPcmSynthesizer::synthesize_with_trace` is opt-in and reports
pre-window IMDCT, head/tail window coefficients, windowed head/tail,
carry-in/out, output sum, and scaled output. The normal `synthesize` path
continues to use the direct transform and overlap/add implementation. A
synthetic deterministic 12-block run equals a 6+6 framed run exactly, including
the final carry and the frame-boundary carry-in.

The real private A/E/D/F vectors each contain 126 AUs. The trace order is
E-AC-3 syntax `L,C,R,Ls,Rs`; the FFmpeg reference order is
`FL,FR,FC,LFE,SL,SR`, mapped as `[L,R,C,Ls,Rs]` after removing LFE. For all
125 AU n -> n+1 boundaries and all five codec channels, `carry_out` hashes
equal the next `carry_in` hashes. At AU0 block5 -> AU1 block0, AU0 block5 output itself is
within about `0.93e-6--1.28e-6` RMS of the FFmpeg reference. The large error
appears only when its stored Ls/Rs tail participates at AU1:

```text
                 normal RMS        zero-carry RMS
A/E/D Ls          0.0075718936      0.000000126--0.000000181
A/E/D Rs          0.0073475530      0.000000125--0.000000181
F Ls              0.0071960116      0.000000181
F Rs              0.0069830427      0.000000180
```

The black-box inferred reference carry is explicitly marked
`inferred_black_box_component=true`; correlation with stored Ls/Rs carry is
only about `0.0227--0.0349`, with scalar gain approximately zero. Therefore:

```text
carry storage / frame commit / channel mapping: verified correct
current AU1 head for Ls/Rs: agrees with FFmpeg when carry is omitted
stored carry versus FFmpeg inferred component: differs
root cause: unresolved upstream block-5 tail versus external frame-boundary policy
```

An FFmpeg continuous-vs-isolated AU1 probe is retained as a black-box
observation, not as an ETSI rule. It cannot justify a production reset. No
production reset, per-channel gain, sample-1536 special case, remap, or FFmpeg
algorithm was added. OAMD strict/vendor behavior and raw warp `3` are unchanged.

Complete OAMD timeline, JOC semantic fidelity, ADM position/trim comparison,
and accepted internal-base fidelity remain open. The first remaining blocker is
the normative/independent explanation for the side-channel block-5 tail at the
first AU boundary, not state continuity.

As a regression-only check, the A/E/D/F `CurrentDefault` internal-base full WAVs
generated in this run are byte-identical to the prior base-root-cause outputs.
Because no production TDAC fix was accepted, a second post-fix JOC propagation
claim is intentionally not made; the existing object-row comparison remains
non-fidelity evidence.

## Independent TDAC and pre-roll decision (2026-08-05)

This increment is an evidence package rather than a decoder semantic change.
The private pure-math oracle independently reimplements the direct type-IV
IMDCT, ETSI Table 6.33 window, overlap/add, and carry update. It reports 53
synthetic comparisons with no material divergence at `1e-12`, and exact
12-block versus 6+6 partition invariance. A separate real-vector replay of
AU0 block 5 and AU1 block 0 agrees with production tails and heads at
`5.12e-17` and `2.00e-15` maximum absolute error respectively.

The P0/P1/P2/P4 base-only controls vary only a 0/1/2/4-AU silent prefix. Their
active PCM content is identical by hash; raw EC-3 and MP4 FFmpeg outputs are
sample-identical for each vector. Their first-boundary errors remain in the
approximately `1e-6--5e-5` range and do not reproduce Logic's approximately
`7e-3` Ls/Rs event. A diagnostic Logic crop excluding the first two AUs lowers
the residual, but no samples are trimmed in production and this is not a
fidelity result.

The joint decision is therefore: production TDAC arithmetic is independently
supported; a generic TDAC state/IMDCT/window defect is not supported; a generic
FFmpeg priming explanation is not established by the base-only controls; and
Logic encoder/upstream or stream-feature-specific provenance remains
unresolved. No TDAC reset, gain, remap, sample special case, warp remap, or
vendor profile change was made. Strict validation, raw metadata retention,
complete OAMD timeline, JOC semantic fidelity, non-zero PCM fidelity, and
ADM comparison remain open.

## Logic AU0/block5 provenance round (2026-08-05)

This round is recorded in the private, non-overwriting run
`OpenJOC-Private/reports/runs/2026-08-05T125009Z_logic-first-block-provenance_77116e9`.
The repository started at `b18ea4d8dc5a72bc00bbb179cf8484f6291b9211` and the
run deliberately leaves production TDAC, AU state lifecycle, gain, channel
mapping, strict/vendor warp handling, and decoder semantics unchanged.

### Lane A: Apple and three-decoder comparison

Apple `afconvert` is available and is used as a real macOS decoder comparator,
not as a normative oracle. Its `afinfo` channel layout is explicitly
`L,C,R,Ls,Rs,LFE`; the comparator maps it to OpenJOC's
`FL,FR,FC,LFE,SL,SR` with `[0,2,1,5,3,4]`. Apple valid frames, FFmpeg MP4,
FFmpeg raw EC-3, and OpenJOC internal-base WAVs are reported with sample
counts, AU-1 windows, startup/steady ranges, selected diagnostic delay, and
aligned residuals. Unaligned AU-1 differences are not interpreted as decoder
errors because the container paths expose different priming/delay coordinates.

The four new four-second Logic vectors have 192,000 Apple/FFmpeg-MP4 samples
and 193,536 raw/OpenJOC samples. Raw-versus-OpenJOC AU-1 Ls/Rs windows are
approximately `0.005756/0.005756` RMS for LE0/LE1/LE2/LE4; the selected delay
is zero and the aligned steady residual is approximately `0.0002093` RMS.
These values are measurements only and do not define a fidelity threshold.

### Lane B: Logic pre-roll controls

Logic Pro 12.3 was operated through its spatial-export UI on four copies of
the same project. The selected four-second exports differ only by source
pre-roll of 0, 1, 2, or 4 access units (LE0/LE1/LE2/LE4). The first attempted
“project” scope produced 256 seconds/8001 AUs and is excluded from the corpus;
the accepted corpus uses the four-second “selection” scope. Each accepted
vector has a private MP4, stream-copy EC3, and ADM BWF, with hashes and source
manifest outside Git.

The deterministic forensic summaries report 126 AUs, one unique payload-11
body, no payload-11 changes, and raw warp distribution `{3:126}` for every
pre-roll. Raw and MP4 observations have zero payload-11 body mismatches. The
ADM inventory is 48 kHz/192,000 samples with `Master` and `OBJ_997HZ`; the
pre-roll copies' object channel has one static four-second block, so this
corpus is a boundary control, not a moving-position ground truth.

### Lane C: coefficient/tool provenance and backprojection

The target AU0/block5 signature has `block_switch=00000`, dither enabled,
coupling/SPX/rematrix/AHT absent, and exponent strategy `1,1,1,2,2`; the
side-channel BAP-zero count is 46 in the four pre-roll probes. Exact later
matches are not stable across all vectors, while relaxed matches excluding
exponent strategy are explicitly labelled diagnostic. The probe writes hashes
for mantissas, pre-IMDCT, window/carry stages, and AU1 heads without exposing
private coefficients in the repository.

The tail backprojection uses FFmpeg only as a black-box output and the private
independent TDAC oracle as the transform map. Its condition estimate is about
`8.42e6`; the explainable ratio is high but the inverse is ill-conditioned and
non-unique. Dominant bins are therefore not assigned to coupling, SPX,
rematrix, dither, or an Apple/FFmpeg internal tool.

### Decision

The independent oracle, formal parser, diagnostic parser, and direct byte mask
all agree on payload-relative warp `[526,528)` and raw value `3`. Hypotheses
0/1/2 all close the bounded element but are explicitly non-unique and never
reach normative object-element semantics. `ETSI_STRICT` continues to return
`ReservedWarpMode { raw: 3 }`; vendor compatibility preserves raw 3 and keeps
trim metadata opaque. No vendor warp rule is added. The first real blocker is
still AU0/block5 Ls/Rs coefficient provenance and internal-base fidelity; this
round does not claim complete OAMD timeline, JOC reconstruction, non-zero PCM
fidelity, or ADM positional fidelity.

## Exact target-AU history replay (2026-08-05)

This increment uses the private non-overwriting run
`2026-08-05T_exact-au-history_e73ef3f_r7`, with `_r8` as a deterministic
repeat. It does not create new Logic media. The LE0 raw EC-3 source is indexed
through OpenJOC's `index_syncframes` and `group_access_units`; AU0 and AU1 are
3,072-byte frames with hashes recorded in the private manifest. H0 is the
original stream; H1/H2/H4 prepend one/two/four exact AU0 copies; HP prepends
exact AU0+AU1. Target bytes are identical across all histories by direct
range extraction and SHA-256. The corpus is diagnostic only because repeated
frame counters and metadata continuity are not a normative programme claim.

All five raw streams remux successfully with FFmpeg `-c:a copy`; extracting
EC3 from each MP4 is byte-identical to its raw input. This proves carrier
preservation, not semantic acceptance.

The new diagnostic example replays every history through the existing E-AC-3
parser and `CodecCore` base policy, then traces TDAC contributions. For the
same target bytes, parsed header, exponent/BAP state, exposed pre-IMDCT
coefficients, and AU0/block5 Ls/Rs tail hashes are stable. H1/H2/H4/HP target
AU0 first diverges at `block0_channel3_tdac_carry_in`; the divergence is in
the retained TDAC context and final PCM boundary, not in the exposed target
coefficients. Target AU1's exposed stages and PCM are stable. A cloned
`AudioPcmSynthesizer` snapshot immediately before target AU0 replays with
identical stage counts, carry arrays, and PCM.

The opt-in diagnostic trace records raw mantissa tokens, grouped-state
positions, dither values, dequantized mantissas, and final pre-coupling/
pre-IMDCT arrays from the same production cursor. Normal decoding allocates no
trace. Component transplant was not performed because production state
components are not public; no diagnostic state transplant was smuggled into
production.

FFmpeg raw target comparisons vary with history (strongest at side-channel
AU0), while Apple `afconvert` accepts all five remuxed MP4 histories and is
sample-stable at target AU0/AU1 under the declared
`L,C,R,Ls,Rs,LFE -> FL,FR,FC,LFE,SL,SR` mapping. These are black-box output
observations, not internal-state claims or fidelity thresholds.

### Decision

The experiment narrows, but does not solve, the provenance question:

- OpenJOC exposed target coefficients are history-stable.
- OpenJOC AU0 final PCM changes first at the retained block-0 TDAC carry-in;
  no TDAC change is warranted.
- FFmpeg output is history-dependent in this corpus; Apple output is stable.
- No production fix, state reset, gain, remap, AU special case, vector/hash
  special case, OAMD/JOC profile change, or warp alias was added.

The first remaining blocker is distinguishing fixed decoder priming/history
coordinates from the Logic AU0/block5 Ls/Rs upstream coefficient provenance.
Complete OAMD timeline, JOC semantic fidelity, object PCM fidelity, ADM
position comparison, and accepted internal-base fidelity remain open.

## Decoder comparison contract (2026-08-06)

This increment adds an evaluation-only comparison contract with explicit cold,
warm-up, and steady-state regions. It does not modify decoder output or add
production trimming. The private package
`2026-08-05T_decoder-comparison-contract_01936ed_r8` is repeated in `_r9` with
byte-identical core JSON/TXT evidence.

Measured exact-history convergence is decoder-specific:

- OpenJOC: observed convergence at source AU1; AU0 differs at legal TDAC
  carry-in, while AU1 stages and PCM are stable. Full decoder-state hash is
  unavailable.
- FFmpeg: no PCM convergence suffix through source AU8 in the tested window.
- Apple: stable from target AU0 in the observed AU grid, but 288 trailing
  samples are absent and PTS is unavailable.

The original sample-1536 event is downgraded to a warm-up/startup comparator
disagreement. Cross-decoder semantic alignment at that absolute sample is
unproven, so it is not a demonstrated TDAC defect. A/E/D/F cold and
steady-state metrics are reported without an acceptance threshold. JOC object
WAVs remain complete; region slicing is evaluation-only and complete OAMD/JOC
semantic fidelity remains open.

## Steady-state coding-tool differential (2026-08-06)

This increment adds only private, evaluation-only evidence in
`2026-08-06T_steady-state-tool-differential_b62168f` (repeat `_r2`). The
selected windows are S1 AU2–15, S2 AU32–63, and S3 AU80–110. AU mapping is
high confidence for OpenJOC and FFmpeg and medium confidence for Apple; Apple
has 288 missing trailing samples and no PTS. The external meaning of the
internal 256-sample block grid remains unproven.

OpenJOC and FFmpeg are close in the steady windows (median channel block RMS
residual approximately `0.98e-6`), while Apple is approximately `1e-5` from
each under the same diagnostic mapping. These numbers are not pass/fail
criteria. Existing tool evidence is representative only: complete independent
per-AU/per-block strata for coupling, SPX, dither, rematrix, AHT, and exponent
strategy are unavailable, so no tool-level causal claim is made. LFE exact
silence is excluded. JOC propagation has 15 diagnostic rows and complete object
WAVs, but semantic identity remains unresolved. The next blocker is a parser-
emitted full tool inventory plus a trusted external block anchor; no production
decoder change is justified.

## Block-anchor and parser tool inventory (2026-08-06)

This increment adds the opt-in `diagnose-tools` path and
`CodingToolBlockInventory`. It is built from parser-emitted state after the
ordinary block decode; the default PCM path does not allocate or consume the
inventory. The CLI refuses to overwrite an existing diagnostic JSON.

Private A/E/D/F runs report 126 access units and 4536 records per vector
(six blocks × five full-band channels plus LFE), with no failed access units.
The schema records explicit versus reused state and formulas for derived BAP
histograms/counts. The corpus cannot isolate tools: coupling, SPX and AHT have
no on stratum, dither is mostly on, and exponent reuse has no randomized
control. No coding-tool effect size is reported.

A deterministic 48 kHz 5.1 marker source and independent detector recover
480/480 source blocks at high confidence and exact offsets. The subsequent G9
Logic carrier was decoded through OpenJOC CurrentDefault, OpenJOC CodecCore,
FFmpeg raw/MP4, and Apple diagnostic paths. Each required path recovered
461/480 blocks; all 19 residuals were frozen margin-only near-neighbor
ambiguities, so the external mapping remains explicitly unproven and
anchored metrics/effects remain unavailable. No production decoder, TDAC,
warp, or vendor behavior changed.

## J1R7A — Normative OAMD spatial-field boundary (2026-08-09)

This is a documentation-only closure. No production parser, DSP, profile,
CLI, fixture, or test changed. The private evidence run is
`20260809T180109Z_j1r7a-spec-anchored-oamd_b6eb1de`; its two-run byte-identical
freeze is `j1r7a_spec_cursor_evidence_freeze.json`, SHA-256
`572209bcb35cf2b37a512df1c9523b1a8762a2672445f96e57ad48a09257ba4f`.

The run replays seven frozen Logic/ADM-qualified sources, 129 AUs each (903
observations), from payload-11 bit 0 using only ETSI-authorized syntax. Every
observation reaches and closes the same normative prefix `[0,526)`. The
precise field result is:

| result | span / value | status |
| --- | --- | --- |
| `pos3D_X` | payload-relative `[52,58)`, six bits | verified in all 903 observations |
| `pos3D_Y` | payload-relative `[58,64)`, six bits | verified in all 903 observations |
| ADM numeric alignment | X `-1,-.5,0,+.5,+1` → `0,16,31,46,62`; Y `+1,0,-1` → `0,31,62` | verified for the controlled corpus |
| first unresolved syntax | `warp_mode [526,528)`, raw `11` = `3` | ETSI Table 32 reserved |

The historical J1R6C/J1R6B `[58,63)` scalar is retained as a five-bit prefix
observation only; J1R6C-R reconciled the representation and did not alter the
carrier. J1R6D's H0/H1/H2 branches all close identically because they attach
labels to the same cursor; they are not evidence for a semantic alias.

The strict parser therefore remains unchanged:
`ETSI_STRICT -> ReservedWarpMode { raw: 3 }`. The vendor profile remains
unchanged and no vendor warp rule is added. The current real-vector boundary
is now narrower and explicit: normative OAMD prefix and X/Y field identity are
validated; trim warp-3 meaning, post-warp continuation, complete trim/timeline/
state semantics, authored-object/OAMD-slot identity, OAMD/JOC binding, object
PCM, ObjectScene/render fidelity, and end-to-end acceptance remain open. The
next proposed milestone is J1R7B, which is intentionally not part of this
round.

## J1R8 — Controlled Z/elevation numeric calibration (2026-08-10)

This is a docs-only closure of the private J1R8 evidence run
`20260810T032631Z_j1r8-z-elevation-calibration_c90779b`; the aggregate
evidence-freeze SHA-256 is
`faeaf08c88f2aa8d241262de6edf6ab60e35ccdd959fa91239f6640f94779c8a`.
One Center-derived Logic fixture was authored using the automation parameter
`对象位置提升` with the independently verified persisted sequence
`0 → 50 → 100 → 0`. ADM qualified the corresponding Z sequence as baseline,
approximately `0.5`, `1.0`, and baseline again, with X = `-0.0` and Y = `+1.0`
unchanged. The J1R7A ETSI cursor fields tested were
`pos3D_Z_sign_bits [64,65)` and `pos3D_Z_bits [65,69)`; observed magnitude
codes were `0,3,6,7,13,14,15,10,3,1,0`. This is controlled calibration
evidence, not a claimed formula or complete timeline decoder.

The 997 Hz source PCM passed sample-identity control. Unchanged-project DD+
R0/R1 stream-copied raw EC3 was deterministic at 129 × 3072 bytes. The
strict parser boundary is unchanged: `warp [526,528) = raw 3` remains
`ReservedWarpMode { raw: 3 }`; no `DOLBY_VENDOR_COMPAT` rule was added. The
post-warp empirical suffix `[528,536)` remained `00000000` for every AU and
is not assigned a semantic name. An exploratory prefix interval `[177,182)`
also changed, but no interpretation is claimed.

The Size line remains frozen with the narrow status: authoring persistence
and ADM propagation are established; tested DD+ Size-state semantics,
deactivation as an intra-stream payload-11 transition, direct
`object_size_idx` response, and Size-related warp/suffix response are not
established. No production parser, DSP, profile, JOC, ObjectScene, or second
Z fixture changed. Complete OAMD timeline/state semantics, OAMD↔JOC binding,
verified object PCM, render fidelity, and end-to-end acceptance remain open.

## J1R9 — Dual-object pre-render row identity boundary (2026-08-10)

This docs-only closure records the private run
`20260810T104057Z_j1r9-dual-object-multitone-identity_6492301`, whose two-run
evidence-freeze aggregate SHA-256 is
`d9611198677caf2f0d6c56aacc4b2fe70843f8fc7a9489546b9658e697045863`.
No production code, test, fixture, carrier, parser, or profile setting changed.

The sole four-second dual-object Logic fixture preserves two
sample-identified sources while exchanging positions: 997 Hz is authored
FL→FR and 2003 Hz is authored FR→FL. ADM confirms those identities and stable
positions. A real nonzero-Z transition trajectory is retained and excluded
from the predeclared stable analysis windows. Two unchanged-project DD+
exports produce the same stream-copied raw EC3 SHA-256
`d35aee5421e965d2fa0eb80d4b6dd071ba719dcd12686a40bf8a87cacfdc452e`.

The diagnostic OAMD path reads Element 1 fields only before opaque Element 2:
slot 0 stays at the Front-Left comparison tuple and slot 3 at Front-Right in
both stable windows. This is evidence of stable spatial slots, not complete
authored-object/OAMD-slot binding. Raw `warp_mode [526,528) = 3` remains ETSI
reserved and the eight post-warp bits remain raw zero. Strict and vendor
profile behaviour is unchanged.

The evaluation-only JOC reconstruction uses the FFmpeg-compatible 5.1 base
and declared Table-47 non-LFE mapping. It reports all rows, but only two carry
high stable-window energy: row 0 (paired with FL slot 0) changes 997→2003 Hz
and row 3 (paired with FR slot 3) changes 2003→997 Hz. Since ADM shows the
opposite authored-object trajectories, the supported conclusion is
`ONE_ROW_PER_AUTHORED_OBJECT_MODEL_REJECTED`. The associated scoped
observation is `SPATIAL_ANCHORED_JOC_STRUCTURE_GAINS_SUPPORT`.

This does not claim a universal JOC spatial basis, full OAMD/JOC mapping,
ObjectScene correctness, renderer fidelity, verified final object PCM, or
warp-3 semantics. Next work is an explicit spatial-basis binding model over
the existing corpus; no second dual-object fixture is warranted here.
## J1R12 — Evidence-bounded reconstruction-basis architecture (2026-08-10)

J1R9, J1R10, and J1R11 are now treated as a frozen evidence boundary. J1R9
rejected the one-row-per-authored-object model; J1R10 left the spatial basis
underdetermined; J1R11 changed Logic application-level track order but left
the raw EC3 carrier and all observed OAMD slot trajectories unchanged. No
independently controllable producer-side variable has been demonstrated that
changes OAMD dynamic-slot assignment while authored identity, trajectory, and
multi-object context remain fixed.

The production architecture therefore has three explicit layers:

1. OAMD metadata objects and timed state (`MetadataObject`,
   `MetadataUpdate`, `ObjectScene` metadata/timelines).
2. JOC `ReconstructionBasis` rows and QMF/PCM diagnostics, with structural row
   indices only and no authored-object ID.
3. `SemanticBindingState`, defaulting to `Unresolved`.

`SceneBuilder` retains the two audio domains separately: JOC rows remain under
`reconstruction_basis`, and a base-carried LFE remains `base_lfe_pcm`. The
former `object_pcm` → `bind_audio` → `ObjectTrack::pcm` chain has been removed;
there is no row-index, dominant-row, or spatial-observation fallback. CLI WAV
artifacts are named diagnostic reconstruction rows, not verified object stems.
Metadata-only scenes are admissible (`METADATA_OBJECTSCENE_ADMISSIBLE`), while
audio-bound ObjectScene admission remains blocked
(`AUDIO_BOUND_OBJECTSCENE_NOT_ADMISSIBLE`).

Regression tests cover metadata-only scenes, row-only basis construction,
unresolved binding, structural LFE/row cardinality, atomic row staging, and
diagnostic row export. Strict raw warp 3 behavior and the explicit vendor
profile are unchanged; no new Logic fixture, JOC semantic inference, or
production warp rule was introduced.

## J1R13 — Semantic binding evidence contract

`openjoc-scene` now exposes a small evidence/admission API separate from the
production binding state. `SemanticBindingEvidence` carries the proposed
relation, scope, evidence class, provenance, observations, contradictions,
negative controls, producer/carrier constraints, evidence dimensions, and a
falsifier. `try_admit` rejects non-verified or incomplete evidence and returns
only a private-field capability token; it cannot silently mutate an
`ObjectScene`. The production state is still only `Unresolved`.

CLI partial-status reports now distinguish
`semantic_object_audio_binding: unresolved`, metadata-scene availability, and
diagnostic reconstruction-row availability. No authored-object PCM or
audio-bound ObjectScene is emitted. Metadata-only scenes, separate
ReconstructionBasis rows, and RcLfe separation are unchanged. The J1R9–J1R11
Logic campaign remains frozen because independent slot assignment has not been
demonstrated; no fixture, ADM, DD+, EC3, warp, or Size behavior changed.

## J1R15 — ReconstructionBasis numerical acceptance

The full ReconstructionBasis path was audited from JOC input through QMF
state, row aggregation, scene validation, and diagnostic WAV export. The
existing frozen corpus passed finite-sample, row-shape, state-carry, repeated
signature, and export checks. A direct Center re-run confirmed byte-identical
reference-f64 row WAV hashes; f32/reference-f64 output uses the intended
precision-aware container boundary. No production defect was found or fixed,
and no new test/media fixture was needed. The milestone decision is
`RECONSTRUCTION_BASIS_NUMERICAL_ACCEPTANCE_ESTABLISHED`.

This is not an authored-object or renderer acceptance. Rows retain structural
indices only, `RcLfe` remains separate, `SemanticBindingState::Unresolved`
remains the production state, and no authored-object PCM or audio-bound
ObjectScene is admissible. ETSI strict raw `warp=3` rejection and the vendor
profile remain unchanged.

## J1R16 — Existing-corpus end-to-end acceptance matrix

J1R16 reused the nine independently qualified frozen carriers and did not
create Logic, ADM, DD+, EC3, or other new media. Every carrier reached the
declared input/AU, base PCM numerical, metadata-only scene, and structural
ReconstructionBasis boundaries. Existing J1R14 timeline ordering and J1R15
numerical regressions remained passing; repeated frozen evidence found no
nondeterministic acceptance failure or production implementation defect.

The resulting decision is `EXISTING_CORPUS_ACCEPTANCE_PARTIAL`. This narrow
classification distinguishes expected `ETSI_STRICT` rejection of raw
`warp=3` from the unresolved vendor continuation left opaque by
`DOLBY_VENDOR_COMPAT`. No new vendor rule, semantic binding, authored-object
PCM, audio-bound ObjectScene, or renderer claim was added. The private matrix
and evidence freeze are under
`OpenJOC-Private/reports/runs/20260810T153638Z_j1r16-existing-corpus-acceptance_f845fdd0/`.

## J1R17 — Opaque vendor-continuation preservation

The OAMD payload layer now exposes OpaqueVendorContinuation as a borrowed,
bit-addressed view over the existing retained element body. The vendor
fallback records the body span, warp span, continuation span in both element
and payload coordinates, an exact bit-window SHA-256, and explicit
opaque_lossless_bounded / vendor_observed_normative_unresolved / unresolved
status. The CLI forensic and partial-status artifacts serialize the same
neutral evidence.

The implementation does not copy or rewrite the source bits, map raw warp 3
to 0/1/2, continue ETSI interpretation, or route opaque data into scene,
binding, reconstruction, renderer, or PCM semantics. Strict validation remains
unchanged. Unit and CLI regressions cover a non-byte-aligned continuation,
exact bit access/hash distinction, raw-3 retention, and explicit profile
behavior. Existing qualified carriers were exercised only as private evidence;
no new media was created.
Private evidence freeze:
`20260810T155539Z_j1r17-opaque-vendor-continuation_f480e05d/j1r17_evidence_freeze.json`.

## J1R18 — Bounded streaming decode and memory admission

`SceneBuilder` now has an explicit streaming retention mode. It runs the same
per-frame structural, finite-value, layout, timing, and content-description
checks as capture mode, but does not extend `metadata_timeline`,
`trim_timeline`, ReconstructionBasis rows, or base-LFE PCM. The bounded
`StreamingSceneSummary` records only duration, frame count, object count,
metadata/trim event counts, and per-frame maxima. Calling `finish()` on a
streaming builder is rejected; callers must request the summary explicitly.

`PayloadDecoder::streaming*` and the E-AC-3 streaming entry points reuse the
existing JOC/QMF/OAMD state machine and sink each committed frame. Regression
tests cover capture/stream frame equality and a 128-frame plateau. Existing
capture APIs remain unchanged. The input/container layer still materializes
the byte stream and AU index, while WAV/debug writers remain capture sinks;
therefore the decision is `BOUNDED_STREAMING_DECODE_CORE_ESTABLISHED`, not
full input-to-output streaming. No semantic binding, authored-object PCM,
warp interpretation, or new media was added.

## J1R19 — Incremental input/container streaming and output finalization

`openjoc-container` now exposes `RawEac3FrameReader<R: Read>`. It never reads
past the current header or declared frame, retains at most one compressed
syncframe, exposes deterministic carry/frame high-watermarks, and maps EOF
mid-frame to an explicit structured error. Chunk-boundary regressions cover
split syncwords, headers, bodies, multiple frames, exact EOF, and truncation.

`openjoc-wave` now exposes seekable `WaveWriter`, which writes a placeholder
RIFF header, appends bounded interleaved/channel chunks, and patches data/RIFF
sizes only at successful finalization. CLI captured reconstruction-row and
RcLfe WAV artifacts use this writer, preserving existing sample formats and
quantization policy.

The current `load_eac3` and FFmpeg ISO BMFF path remain capture/index APIs: they
materialize stream-copy bytes, and the E-AC-3 decoder still consumes complete
borrowed stream/index slices. Non-seekable MP4 streaming is not admitted. The
milestone therefore stops at `STREAMING_INPUT_OUTPUT_ADMISSION_PARTIAL` rather
than claiming universal O(1) container memory. SemanticBindingState, warp
behavior, and all prior numerical/binding boundaries are unchanged.

## J1R20 — Incremental AU consumer / container ownership closure

`RawEac3AccessUnitReader<R: Read>` now bridges the bounded raw syncframe reader
to the existing J1R18 streaming decoder. It retains one complete local AU and
one frame of lookahead for the next independent substream-zero boundary;
programme-wide input, syncframe, and AU indexes are not built on the direct
raw path. The explicit CLI mode is `decode ... --internal-base --streaming`.
The legacy `load_eac3`/slice/index APIs remain available as capture and
random-access contracts rather than being silently changed.

On the frozen Center 997 Hz carrier, direct and legacy decode artifacts are
byte-identical for base full/JOC-input/LFE WAVs, inventories, and 1,161 shared
per-frame diagnostics. Their dimensions agree at 48 kHz, 129 AUs, 198,144
samples, 16 metadata objects, 2,064 metadata events, and 15 reconstruction
basis rows. Chunk, lookahead, exact-EOF, truncation, invalid-start, and
128-AU plateau regressions pass. ISO BMFF sample-table/index retention remains
a declared limitation. No second decoder, semantic binding upgrade, warp rule,
or new media was introduced.

## J1R21 — Seekable ISO BMFF sample delivery and index ownership

The explicit `decode --streaming --internal-base` path now admits seekable
ISO BMFF E-AC-3 input. FFprobe supplies packet offsets and sizes, while
`SeekableIsoBmffEc3Reader` seeks and reads one current compressed sample at a
time and feeds the existing `RawEac3FrameReader`/AU consumer. It does not
materialize the `mdat` or a complete elementary-stream `Vec<u8>`; the derived
packet-location index remains an explicit O(samples) container-metadata cost.
The frozen Center, Front Right, Rear Center, and Center 2003 carriers each
match their stream-copy EC-3 byte-for-byte (129 packets of 3072 bytes).

Malformed packet rows, wrong-track rows, out-of-bounds samples, bounded sample
reads, exact EOF, and a frozen real MP4 reader regression are covered. Generic
non-seekable and fragmented MP4 are not admitted. J1R20's existing decode
equivalence, J1R14/J1R15/J1R17/J1R18 architecture, `SemanticBindingState`,
and ETSI strict raw warp=3 reservation are unchanged. The narrow decision is
`SEEKABLE_ISOBMFF_STREAMING_ADMISSION_ESTABLISHED_WITH_INDEXED_METADATA`;
this does not claim O(1) container index memory or semantic object binding.
Private evidence:
`OpenJOC-Private/reports/runs/20260810T174335Z_j1r21-isobmff-streaming_bbee0a5/j1r21_evidence_freeze.json`.

## J1R22 — Lazy ISO BMFF cursor and derived-index elimination

The ordinary seekable ISO BMFF path no longer expands FFprobe packet rows into
an OpenJOC `Vec<IsoBmffSample>`. `IsoBmffSampleCursor` consumes the packet
probe stdout incrementally with one reusable line buffer, and
`SeekableIsoBmffEc3Reader::from_cursor` seeks the current offset and releases
the current compressed sample before advancing. The old `new(..., Vec<...>)`
constructor remains an explicit indexed/capture adapter for random access.

Across four frozen carriers, the former 129-entry derived index (about 2,064
bytes per carrier at the current descriptor shape) is replaced by one bounded
cursor state entry, with identical ordered packet bytes. This removes
duplicate OpenJOC metadata, but does not claim that FFprobe's own native
stco/co64/stsc/stsz/stts tables are constant-memory; that external/container
metadata remains a separately declared duration-proportional cost. The
decision is `DERIVED_ISOBMFF_SAMPLE_INDEX_ELIMINATED_FOR_SEQUENTIAL_DECODE`
with `BOUNDED_ISOBMFF_SAMPLE_CURSOR_ESTABLISHED`. No semantic, warp, or
binding behavior changed.

## J1R23 — E-AC-3 coding-tool admission matrix

The existing `CodingToolBlockInventory` and E-AC-3 implementation paths were
audited without adding media. Four frozen diagnostic carriers provide
observational activation for block switching, dither, exponent reuse, grouped
mantissa state, and LFE. Existing unit/synthetic tests cover public table,
branch, transform, coupling, SPX, AHT, rematrix, and malformed-input behavior,
but no authorized real/reference vector activates the high-risk coupling/SPX/
AHT/rematrix/dependent-substream effects in the controlled inventory.

Accordingly the release status is `EAC3_CODING_TOOL_COVERAGE_PARTIAL`, not
`FULL_EAC3_CODING_TOOL_FIDELITY_ESTABLISHED`. Parser presence is not treated as
DSP validation, and DSP implementation is not treated as causal corpus
coverage. `SemanticBindingState::Unresolved` and strict raw warp-3 rejection
are unchanged.

## J1R24 — public-syntax coding-tool activation harness

The new test-only `PublicSyntaxCase` harness is intentionally narrow: it uses
existing public E-AC-3 structures and calls production coupling, SPX, AHT, and
rematrix DSP APIs without implementing an encoder. The dependent-substream
cases reuse the production parser/state/merge tests. Determinism and finite
shape invariants are checked; a separate public sum/difference oracle admits
the rematrix band formula at `L4` for the tested case.

This is branch/state evidence, not real-corpus prevalence or full coding-tool
fidelity. Coupling, SPX, AHT, rematrix, and dependent-substream effects remain
absent from the frozen controlled inventory. The decisions are
`PUBLIC_SYNTAX_CODING_TOOL_ACTIVATION_HARNESS_ESTABLISHED` and
`EAC3_CODING_TOOL_STATE_ADMISSION_STRENGTHENED`.

## J1R25 — coupling state and coordinate admission

J1R25 adds `tests/coupling_admission.rs`, a test-only float64 oracle
independently transcribed from TS 102 366 V1.4.1 clause 6.4.3. It exhaustively
compares all 1,024 legal standard coordinate codes with the public production
reconstruction API and checks explicit rejection of out-of-domain codes.
The six-block parser fixture now asserts that coupling coordinates/state are
exactly reused after the first block. No production coupling expression was
changed.

The resulting acceptance is scoped to public syntax, parser/state reuse, and
coordinate numerics. The Logic controlled corpus still has no coupling
activation, so full coupled-PCM fidelity and semantic object binding remain
open. `SemanticBindingState::Unresolved` and ETSI strict raw warp-3 handling
are unchanged.

## J1R26 — SPX state and reconstruction admission

`tests/spx_admission.rs` adds a structurally separate float64 oracle for the
public SPX translation and coordinate-scale path. It enumerates four copy
indices × 16 exponents × 4 mantissas × 4 master values (1,024 cases), checks
finite deterministic output, and rejects invalid coordinate, attenuation,
noise-length, and band-dimension inputs. The oracle uses only the isolated
one-band, zero-noise, no-attenuation boundary; existing SPX tests cover the
remaining blend and attenuation primitives independently.

The evidence level is `SPX_STATE_ADMISSION_ESTABLISHED_NUMERICAL_MAPPING_PARTIAL`.
Cross-block coordinate reuse/reset and full real-stream SPX PCM fidelity are
not claimed; the controlled Logic corpus remains SPX-off.

## J1R27 — SPX reuse, carry, and reset admission

`tests/spx_state.rs` drives the production parser through a six-block
synthetic sequence: explicit A, two exact coordinate reuses, explicit B with
a different `spxstrtf`, reuse B, and `spxinu=0` disable. A second sequence
proves disable → fresh re-enable, and separate frame decoding proves no SPX
state is inherited across the frame boundary. The expected state is compared
as the complete public `SpectralExtensionInformation` value. A 256-repeat
sequence is exactly deterministic and therefore exercises bounded current
state rather than a growing history.

Decision: `SPX_STATE_REUSE_AND_RESET_ADMISSION_ESTABLISHED` for the exercised
mono public-syntax path, combined with J1R26's scoped numerical mapping. This
does not establish multi-channel participation, parser-specific truncation,
real Logic SPX activation, or full real-stream SPX PCM fidelity.

## J1R28 — SPX multi-channel participation and parser errors

`tests/spx_multichannel.rs` adds a bounded stereo public-syntax harness. Its
six blocks establish independent A/B coordinates, exact dual-channel reuse,
A-only replacement, B-only fresh entry, A fresh entry while B reuses, and A
reuse while B is replaced. Shared start/begin/end/band configuration remains
stable while `channel_in_use` and the two coordinate slots follow the encoded
participation state exactly. Separate disabled and A-only first blocks cover
activation baselines.

The harness records bit offsets 127, 144, and 175 for the participation and
coordinate boundaries. Declared-frame truncation at the corresponding byte
limits is rejected as bounded end-of-input. A malformed call cannot poison a
fresh frame decode, invalid coordinate dimensions return the structured
dimension error, direct/pre-parsed/diagnostic paths compare exactly, and 256
repetitions remain deterministic.

Decision:
`SPX_MULTICHANNEL_STATE_ADMISSION_ESTABLISHED_ERROR_PATH_PARTIAL`. No
production source change was required. Dependent-substream/config transition
reset is still unresolved because the current parser state is frame-local and
the public API exposes no persistent SPX state across substreams. The combined
public-syntax evidence is therefore parser/state/numerical admission with that
declared limitation, not real-stream PCM fidelity.

## J1R29 — AHT production reconstruction and numerical admission

Added `crates/openjoc-eac3/tests/aht_admission.rs` as a test-only independent
normative oracle. It locks the complete high-efficiency pointer and VQ table
domains, exhaustively traverses the implemented GAQ codeword domain, validates
all gain-word symbols, checks bounded truncation, and compares the production
six-point inverse DCT with a separately written float64 formula.

The existing syncframe builder now has a conventional AHT-disabled companion.
New integration regressions prove that enabled and disabled frames select
different production reconstruction paths, that direct/pre-parsed/repeated
decodes are exact, and that one independently transcribed VQ bin reaches the
correct exponent-shifted coefficient in each of six audio blocks. Existing
callers retain the original AHT-enabled helper behavior.

No production implementation change was required. The accepted level is
`AHT_L4_INDEPENDENT_NORMATIVE_ORACLE` for bounded table/GAQ/IDCT and one
integrated bin, with `AHT_L2_RECONSTRUCTION_VALUES` established through the
production parser. Real-stream AHT PCM fidelity remains unestablished because
the frozen controlled corpus does not activate AHT.

## J1R30 — dependent-substream assembly and topology admission

`JocAccessUnitPcmDecoder` continues to decode I0 and optional D0 with separate
TDAC synthesizers, but now records each substream's sample-rate/acmod/LFE/map
configuration. A changed configuration resets the affected synthesizer before
decode; decode and merge still operate on clones and commit only after the
complete AU succeeds. `DecodedAccessUnitPcm` now carries canonical
`ChannelLocation` values and a separate LFE location alongside PCM, preventing
7.X and 5.X+2 from becoming indistinguishable channel-count-only results.

The low-level mapper covers the complete public Table E.1.4 bit domain. The
complete JOC path calls `validate_joc_topology` to admit only Table 47 5.X,
7.X, and 5.X+2. Matching dependent locations replace I0 PCM as specified;
new locations supplement it. Distinct LFE and LFE2 locations are not silently
collapsed. Sample mismatch diagnostics now report the actual mismatched
channel length.

The CLI internal-base collector no longer assumes exactly five full-band
channels. It retains the first AU's labels, supports valid seven-channel JOC
input, inserts the separately bypassed LFE after C for diagnostics, and rejects
later topology changes before mutating accumulated PCM. Regression coverage
includes exhaustive map comparison, parser activation, sentinel merges,
configuration reset/isolation, malformed input atomicity, exact capture versus
AU-local PCM, and bounded incremental I0/D0 grouping.

Decision: `DEPENDENT_SUBSTREAM_CHANNEL_ASSEMBLY_ADMISSION_ESTABLISHED` for
public syntax. Real controlled-corpus activation and full real-stream fidelity
remain unavailable.
## J1R31 — OpenJOC 0.x capability and CLI contract

J1R31 consolidates the current evidence boundary into the canonical
`REQUIREMENTS_MATRIX.md` capability table. The table separates production
status from evidence class and explicitly covers raw/ISO-BMFF input, base
E-AC-3 tools, OAMD/JOC profiles, ReconstructionBasis, metadata-only scenes,
and capture/streaming output. It does not promote public-syntax tests to
real-stream fidelity.

The CLI contract was tightened without changing codec semantics. Root and
per-command help now state that capture output is a metadata-only scene plus
diagnostic ReconstructionBasis rows; rows are not authored-object PCM.
`--streaming` is accurately scoped to raw EC3 or seekable ordinary ISO BMFF,
requires `--internal-base`, and does not capture a scene or reconstruction
rows. Streaming summaries now report the actual input kind/delivery mechanism
instead of labelling an ISO-BMFF sample path as raw input.

Top-level failures now expose stable categories (`usage`, `invalid-argument`,
`unsupported-input`, `malformed-input`, `profile-rejection`,
`unsupported-feature`, `decode-failure`, `output-failure`, and `io-failure`)
while retaining a single zero/non-zero process-status convention. Strict
profile rejection remains explicit and is never auto-downgraded. The only
actionable vendor hint says that vendor compatibility is partial/opaque and
does not promise semantics.

The package description/banner no longer claims to open authored objects or
rebuild a rendered space. Output naming remains `reconstruction_rows`,
`base_lfe`, and metadata artifacts. `SemanticBindingState::Unresolved`, strict
raw-warp rejection, opaque vendor continuation, authored-object PCM
inadmissibility, and audio-bound ObjectScene inadmissibility are unchanged.

## J1R32 — clean-source packaging, install, and reproducibility

The first `git archive` release build exposed a real packaging defect: the JOC
and QMF build scripts required an official ETSI companion ZIP from the
untracked `references/` directory. OpenJOC now commits the importer's
deterministic Rust output in both consuming crates. Each generated file records
the companion C-source SHA-256, and importer tests reproduce/compare the files
when `OPENJOC_ETSI_TABLE_ARCHIVE` (or the development-tree attachment) is
available. Production compilation no longer reads an external reference file.

A second defect prevented Cargo packaging: workspace path dependencies lacked
version requirements. Every internal `0.1.0` dependency now declares both its
path and version, and package metadata includes descriptions and the public
repository. `cargo package --workspace --locked --offline` can therefore
assemble and verify the full eleven-crate dependency chain locally. The CLI
package carries the new source/install README. No crate was published.

An absolute private-fixture path in one opt-in CLI integration test was removed
in favor of `OPENJOC_PRIVATE_J1_FIXTURE_DIR`. Complete package inventories
contain no private media, private directories, `references/`, `.DS_Store`, or
temporary artifacts, and production-source scans contain no developer absolute
paths. Historical documentation may still name private evidence locations as
provenance without embedding the evidence itself.

Two release builds from separate clean source and target directories produced
the same executable bytes on Rust 1.94.0/aarch64 macOS. An isolated-prefix
`cargo install` produced that same binary, and root plus all six subcommand help
paths passed from `/tmp`. Two workspace packaging runs also produced identical
hashes for all eleven `.crate` files. These are same-host/same-toolchain results,
not clean-machine or cross-platform certification.

No codec expression, validation profile, semantic binding, renderer behavior,
or media changed. `SemanticBindingState::Unresolved` and strict raw-warp
rejection remain unchanged.
