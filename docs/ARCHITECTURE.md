# OpenJOC production architecture

This is the canonical description of the current production data flow. It
describes implemented boundaries, not a promise that every historical design
goal is complete.

OpenJOC implements its spatial rendering DSP directly rather than delegating
object rendering to platform-specific spatial audio engines. Operating-system
audio APIs may be used for integration and I/O, but they do not define
OpenJOC's spatial rendering result. One renderer. Same spatial semantics across
platforms.

The CLI, GStreamer plugin, and external FFmpeg bridge are transport frontends
over the same `OpenJocSession`; they do not contain separate render pipelines.
GStreamer owns its buffer/caps/segment lifecycle. The FFmpeg bridge uses
libavformat for demux and public libavutil AVFrame allocation. In both cases
OpenJOC retains E-AC-3/JOC decode, scene construction, DRC/dialnorm, speaker
rendering, binaural HRTF rendering, latency, and drain state.

## Data flow

The explicit render-scene workflow is implemented in `openjoc-render-scene`.
It depends on `openjoc-render`, `openjoc-sofa`, and `openjoc-wave`; it is
deliberately separate from `openjoc-scene`, so decoder metadata and
ReconstructionBasis rows cannot enter the caller-bound source contract.

```text
raw EC-3 / seekable ISO BMFF
          │
          ▼
input/container ownership and access-unit delivery
          │
          ├── E-AC-3 base decode ──► channel-labelled PCM / RcLfe
          │
          └── EMDF payloads
                  ├── OAMD ──► metadata objects and timed state
                  └── JOC  ──► reconstruction-basis rows
                                      │
                                      ▼
                         codec-domain JOC bridge (T(t) unresolved)
                                       │
                                       ▼
                              metadata-only ObjectScene

The 0.9.0 `render-joc` workflow adds an explicit experimental speaker
branch after the decoded component boundary:

```text
raw EC-3 / seekable ISO BMFF
          ↓
bounded AU delivery → E-AC-3 Base + RcLfe + JOC/OAMD decode
          ↓
decoded Base/RB codec-coordinate bundle + automatic bridge-control assembly
          ↓
persistent JocSpatialBridge → active N-channel speaker planes
  ├── Base LFE/RcLfe → LFE plane only
  └── active planes + LFE → shared final linked speaker gain
                              ↓
                      incremental semantic WAV/CAF output
```

The CLI presets are data-only registrations over the generic `SpatialLayout`
and `JocSpatialBridge` projection path. The public library can consume a
caller-defined channel registry, geometry, and output order without a
preset-specific projection algorithm; the CLI does not currently serialize
that custom layout as a file format.

The `22.2` preset is ITU-R BS.2051-3 Sound System H (9+10+3): a four-layer
bottom/middle/upper/top topology with 22 spatial speakers and two semantic
LFE destinations. It uses the same N-layer point projector and semantic
operation boundaries as the established layouts; LFE channels are never
projection vertices.

Automatic assembly derives codec-coordinate control from validated OAMD and
decoded Base/RB state. A complete sidecar is optional and takes precedence as
an explicit override/test source; automatic and explicit sources are not
implicitly merged. Topology/count, coordinate dimensions, metadata updates,
and Base topology changes are validated at the integration boundary; no
guessed row/object renderer is constructed. The public semantic PCM order is
selected by the explicit preset; exact speaker WAV masks remain backend
specific. `5.1` remains `FL, FR, FC, LFE, Ls, Rs`.
```

The parser reads what is present in the carrier. Validation then applies an
explicit profile. The decoder consumes an accepted representation; it does
not hide vendor compatibility decisions.

## Layer boundaries

### Input and container

Raw E-AC-3 uses the in-process incremental reader. Ordinary seekable ISO BMFF
uses a sample cursor with container ownership kept separate from the AU
consumer. Non-seekable and fragmented MP4 are outside the 0.9.0 contract.

### E-AC-3 base

The base decoder keeps frame, audio-block, coupling, SPX, AHT, rematrix,
substream and TDAC state explicit. Channel labels and `RcLfe` are retained as
base-carried information; `RcLfe` is not a dynamic reconstruction row.

### OAMD and profiles

OAMD metadata is parsed into typed state and timed updates. `ETSI_STRICT`
enforces the published validation rules. `OBSERVED_VENDOR_COMPAT` is explicit and
partial: it preserves original metadata and records deviations, but does not
assign meaning to unresolved vendor continuation.

User-facing `decode` and `decode-payload` commands expose a separate `AUTO`
selection policy. It parses once, evaluates strict validation first, and uses
the existing compatibility policy only when every blocking deviation is
already whitelisted. Malformed, unsafe, unknown, and non-whitelisted failures
remain failures. Selection diagnostics include the requested and selected
profiles, strict status, deviation set, and reason. `AUTO` is not a parser or
renderer profile; explicit `ETSI_STRICT` never falls back, while normative
inspection remains strict by purpose.

The observed OAMD `warp_mode` value `raw=3` remains reserved under ETSI strict
parsing. No production alias, offset, or trim guess is present.

### Scene and binding

`ObjectScene` is metadata-only. Its metadata objects and timed positions are
not automatically associated with audio rows. `SemanticBindingState` remains
`Unresolved`; there is no implicit `row == authored object`, slot identity, or
dominant-row fallback.

### JOC ReconstructionBasis

JOC reconstruction produces rows with structural indices and deterministic
numerical behavior. The rows are an exposed reconstruction basis for analysis,
not verified authored-object PCM. Diagnostic WAV export uses
`diagnostics/reconstruction_rows/row_NNN.wav`.

The stable component boundary uses `ReconstructionBasisRowIndex` as a local
decoder-coordinate identity. `DecodedComponentLayout` and the CLI
`diagnostics/components.json` manifest distinguish Base full-band channels,
separate Base LFE, indexed RB rows, and `SemanticBindingState::Unresolved`
without retaining another PCM copy. Operations requiring authored-object audio
identity fail explicitly while binding is unresolved; component-domain decode
and streaming remain available.

### JOC spatial reconstruction bridge

`openjoc-scene` exposes `JocSpatialFrameBridge` and the versioned
`openjoc.joc-spatial-reconstruction.v1` codec-domain contract. A borrowed
`CodecBasisBlock` carries explicitly labelled Base full-band PCM, indexed
ReconstructionBasis rows, and separate RcLfe; `JocSpatialMetadataFrame` carries
the current OAMD payload and structural programme dimensions; and
`SampleRange` gives each committed decoder frame an absolute half-open sample
interval. The bridge is streaming and retains no duration-proportional PCM.

The semantic operation is modelled as `o(t) = T(t)c(t)` followed by the
independent renderer operator. `T(t)` is not known: `JocSpatialOperatorState`
therefore remains `Unresolved`, and `require_resolved_operator()` is a hard
gate. There is no automatic conversion from decoded components to
`ExplicitSpatialScene`, no fixed RB-row/object mapping, and no implicit matrix
or permutation. The readiness census is in
[`joc_reconstruction_readiness.json`](joc_reconstruction_readiness.json).

The explicitly activated `JocSpatialBridge` is a downstream spatial function
with experimental maturity. It consumes a losslessly retained topology/
coordinate snapshot, projects into a caller-supplied public layout, applies the
Q32 gain scheduler, and accumulates linearly into caller-owned buffers. It does
not change profile validation, assign authored-object identity, or resolve
`SemanticBindingState`; its raw warp-3 field is retained as opaque data and
excluded from projection arithmetic. The supported ordinary domain and
activation surface are documented in
[`JOC_SPATIAL_BRIDGE.md`](JOC_SPATIAL_BRIDGE.md).

After Base and ReconstructionBasis contributions have been accumulated into
the final semantic speaker planes, the shared renderer applies a causal,
common FinalLinkedGain stage for the admitted 48-kHz 32-sample adapter blocks.
It includes active LFE in the linked channel set, adds one block of speaker
output history, and is reset with the stream/timeline lifecycle. The stage is
not applied to the SOFA binaural path; Base downmix overload protection and
pre-gain contribution linearity remain separate contracts.

### Explicit spatial renderer

The `openjoc-render` crate is a separate Layer-A/Layer-B foundation. It accepts
only caller-supplied `ExplicitSpatialSource` blocks with an opaque source ID,
mono PCM, explicit Cartesian position, and explicit linear gain. Its initial
renderer maps the front horizontal hemisphere to `FL, FR` with a public
equal-power law and mixes borrowed blocks into caller-owned floating-point
buffers. It has no dependency on `openjoc-scene`, `DecodedJocComponents`, or
`ReconstructionBasis`, so unresolved decoder rows cannot become authored
spatial sources through an implicit conversion.

The initial `StereoRenderer` rejects rear-hemisphere and undefined horizontal
directions, ignores elevation for stereo, performs no distance/room/occlusion
processing, and does not clip by default. `SpeakerLayout2d` and
`LayoutRenderer2d` add arbitrary validated horizontal layouts with deterministic
adjacent-pair, checked 2x2 VBAP-style gains. The caller's speaker order is the
public planar output order; unsupported angular gaps fail explicitly. The 2D
renderer ignores elevation and has no LFE/bass-management path. The separate
binaural renderer and experimental JOC spatial bridge do not change this 2D
contract or provide automatic JOC semantic binding.

`SpatialState2d`, `TrajectorySegment2d`, and `SourceTrajectory2d` add an
explicit, piecewise-linear automation contract. Segment endpoints are inclusive
absolute sample indices; azimuth follows an explicit shortest/increasing/
decreasing path policy and source gain is interpolated linearly in the linear
domain. `StereoRenderer::render_trajectory_block` and
`LayoutRenderer2d::render_trajectory_block` evaluate that state per sample, so
one block, irregular blocks, and one-sample blocks have the same result for the
same absolute timeline. Trajectory blocks borrow PCM and caller-owned output
planes, perform bounded preflight validation, and allocate neither per sample
nor for the full timeline. The trajectory is directional only: radius, z,
distance, Doppler, room effects, elevation, and HRTF are not rendered.

`Speaker3d`, `SpeakerTriplet3d`, and `SpeakerLayout3d` add an explicit
three-dimensional topology contract. The caller supplies the public speaker
order and every admissible VBAP triplet; `LayoutRenderer3d` never performs
Delaunay, hull, coverage, or “best triplet” inference. Each declared triplet
is solved as the public 3×3 system `S g = p`, with finite/non-singular checks,
non-negative gain checks, and unit-energy normalization. Exact speaker hits
are deterministic one-hot gains. If multiple declared triplets cover a
direction, their complete public-order gain vectors must agree or rendering
fails with an ambiguity error. Partial layouts fail explicitly for unsupported
directions, and LFE/bass management remains outside this renderer contract.
The 3D renderer accepts only explicit sources and caller-owned planar `f64`
outputs. `SpatialState3d`, `TrajectorySegment3d`, `SourceTrajectory3d`, and
`TrajectorySourceBlock3d` add an additive sample-accurate dynamic path over
that same immutable topology. Each segment uses shortest great-circle SLERP
between canonical unit directions, a stable small-angle branch, linear gain
interpolation, and explicit rejection of antipodal ambiguity. Callers supply
intermediate keyframes for longer routes; no path inference is performed.
`LayoutRenderer3d::render_trajectory_block` evaluates absolute sample indices
and preserves static output equivalence, endpoint/keyframe continuity, and
byte-identical block-partition invariance. It preflights every sample before
clearing caller-owned planar `f64` outputs and performs no per-sample heap
allocation. Distance, Doppler, listener orientation, room effects, LFE,
HRTF/binaural rendering, JOC, ObjectScene, and authored-object bridges remain
outside the contract.

`HrirPair`, `HrirEntry`, and `HrirBank` provide a compact caller-supplied
exact-direction HRIR contract, with optional explicit per-ear delay metadata
retained for construction-time interpolation. `StaticBinauralSource` binds a
fixed explicit source ID, canonical direction, linear gain, and HRIR entry; it
does not infer authored-object identity. `BinauralRenderer` uses the fixed
listener convention (`+Y` forward, `+X` right, `+Z` up), emits `LEFT_EAR` then
`RIGHT_EAR`, and performs direct causal time-domain FIR convolution. It
preserves leading HRIR delay, keeps bounded per-source history across
caller-owned input blocks, and exposes explicit tail draining and reset
semantics. `openjoc-sofa` resolves exact directions first and performs bounded
delay-aligned spherical interpolation before registering static sources.
`PartitionedBinauralRenderer` is an additive,
caller-selected uniform overlap-add backend: its fixed power-of-two partition
`P` uses a `2P` FFT, reports one-partition scheduling latency, accepts exactly
`P`-sample input partitions plus one explicit final partial operation, and
drains exactly the largest registered `M-1` causal tail. It precomputes HRIR
spectra and keeps only bounded filter-length frequency/time state; no
duration-proportional PCM history or adaptive backend selection is used.
Direct FIR remains the numerical oracle, so cross-backend validation is
numerical rather than a promise of bit identity.

The separate `openjoc-sofa` crate is a construction-time, read-only adapter
from a deliberately narrow `SimpleFreeFieldHRIR` SOFA contract into
`HrirBank`. It depends on `openjoc-render`; the renderer remains independent
of file parsing, NetCDF/HDF5 libraries, and OS-specific APIs. The current
portable reader accepts the project-tested NetCDF classic CDF-1 subset, fixed
listener pose, spherical degree/degree/metre source positions, exactly two
receivers, common sample rate, and integer sample delays. Receiver geometry,
not array order, determines left/right ear mapping. Exact lookup is preferred;
non-exact requests use a deterministic local spherical segment/triangle with
shared ear weights, while sparse/outside coverage fails closed. After
construction no SOFA file handle is retained and neither renderer performs
file I/O per audio block. HDF5/NetCDF-4 remains outside the portable runtime
reader; the bundled SADIE II resource is converted offline to the same CDF-1
path and needs no network access at render time. Resampling, moving sources,
SOFA writing, and any JOC semantic bridge remain outside this boundary.

### Capture and streaming

Capture mode may retain metadata and diagnostic artifacts. Streaming mode uses
bounded AU/frame state and incremental output finalization; it does not silently
capture an unbounded ObjectScene or reconstruction-row vector.
Malformed or truncated raw E-AC-3 and consumed ISO BMFF structures are rejected
with bounded diagnostics; streaming output is staged and promoted only after a
complete successful decode, so failure cannot publish a canonical partial.

## Error and evidence model

Malformed input, unsupported container shapes, strict profile violations and
output failures are classified separately. A diagnostic or empirical result
cannot upgrade semantic binding. Current claim boundaries are summarized in
`CAPABILITIES.md` and `KNOWN_LIMITATIONS.md`.
