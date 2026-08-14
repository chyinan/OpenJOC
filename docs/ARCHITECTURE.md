# OpenJOC production architecture

This is the canonical description of the current production data flow. It
describes implemented boundaries, not a promise that every historical design
goal is complete.

## Data flow

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
                              metadata-only ObjectScene
```

The parser reads what is present in the carrier. Validation then applies an
explicit profile. The decoder consumes an accepted representation; it does
not hide vendor compatibility decisions.

## Layer boundaries

### Input and container

Raw E-AC-3 uses the in-process incremental reader. Ordinary seekable ISO BMFF
uses a sample cursor with container ownership kept separate from the AU
consumer. Non-seekable and fragmented MP4 are outside the v0.2.0 contract.

### E-AC-3 base

The base decoder keeps frame, audio-block, coupling, SPX, AHT, rematrix,
substream and TDAC state explicit. Channel labels and `RcLfe` are retained as
base-carried information; `RcLfe` is not a dynamic reconstruction row.

### OAMD and profiles

OAMD metadata is parsed into typed state and timed updates. `ETSI_STRICT`
enforces the published validation rules. `DOLBY_VENDOR_COMPAT` is explicit and
partial: it preserves original metadata and records deviations, but does not
assign meaning to unresolved vendor continuation.

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
renderer ignores elevation and has no LFE/bass-management path. HRTF/binaural
rendering and any JOC semantic bridge remain later capabilities.

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
cannot upgrade semantic binding. See [PROVENANCE.md](PROVENANCE.md) for claim
admission and [REQUIREMENTS_MATRIX.md](REQUIREMENTS_MATRIX.md) for the
engineering truth table.
