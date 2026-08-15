# Changelog

## [0.3.0-dev] — unreleased

### Added

- Added the codec-domain `openjoc.joc-spatial-reconstruction.v1` bridge with
  borrowed Base/RB/RcLfe/OAMD inputs, absolute sample ranges, typed unresolved
  operator state, strict dimension/finite validation, and a hard gate against
  automatic JOC-to-scene conversion. Added the reconstruction readiness census.

### Research boundary

- J5R11 tested a source-locked companion-induced codec-basis column on the
  existing frozen corpus. The instantaneous and permitted two-tap temporal
  models both failed their predeclared fit/holdout gates, so the operator
  temporal class remains unresolved; no RB-row/object or renderer semantic
  claim was added.

- J5R12 tested the same source-locked two-tap transfer only at the two
  authorized codec grids: 1536-sample access units and, after AU failure,
  256-sample codec audio blocks. Both failed the frozen source/holdout and
  absolute-quality gates despite byte-identical repeats and a passing Base
  null. The minimum temporal/state class remains unresolved; no finer model,
  production coefficient table, RB-row/object binding, or warp-3 semantic
  rule was added.

- J5R13 tested globally anchored 256-phase and conditional 1536-phase
  source-locked templates on the same frozen corpus. Both failed their
  preregistered cross-block/AU holdout and full-coordinate gates, so fixed
  codec-phase periodicity is insufficient within this corpus. Smaller-period
  searches, longer FIRs, empirical production templates, and semantic binding
  remain out of scope.

- Added `openjoc render-scene` with the versioned `openjoc.render-scene.v1`
  explicit-source contract, strict SOFA-backed static binaural rendering,
  transactional float32 WAV output, and `openjoc sofa inspect`.

- Initial explicit spatial-scene and streaming stereo renderer foundation.
- General validated horizontal speaker layouts with deterministic adjacent-pair
  public VBAP-style panning and caller-owned planar block outputs.
- Sample-accurate, absolute-timeline position/gain trajectories for stereo and
  general 2D layouts, with explicit azimuth paths and block-partition invariance.
- Explicit 3D speaker order and triplet topology with checked 3×3 VBAP gains,
  deterministic exact-speaker hits, ambiguity rejection, and bounded planar
  block rendering. Automatic triangulation is intentionally not included.
- Sample-accurate 3D source trajectories using shortest great-circle segments,
  linear gain ramps, explicit intermediate keyframes around antipodes, and
  byte-identical block-partition-invariant triplet rendering.
- Caller-supplied exact-direction HRIR banks, static explicit binaural source
  registration, direct causal f64 FIR streaming, bounded history, and complete
  block-partition-invariant tail draining. Direct FIR remains the compact
  numerical oracle.
- Fixed uniform FFT partitioned binaural convolution with caller-selected
  power-of-two `P`, `2P` transforms, explicit one-partition scheduling latency,
  final partial input, bounded exact tail draining, and reset/lifecycle
  semantics. No SOFA, interpolation, moving source, or adaptive backend
  selection is included.
- Strict, read-only `SimpleFreeFieldHRIR` SOFA ingestion into `HrirBank`,
  using a portable NetCDF classic CDF-1 reader with listener-basis conversion,
  geometry-derived ear ordering, integer sample-delay materialization, and
  resource limits. HDF5/NetCDF-4, interpolation, and nearest-direction lookup
  remain explicit non-features.

### Changed

- Renamed the full spatial projection surface to `JocSpatialBridge` and the
  borrowed frame facade to `JocSpatialFrameBridge`; the module is now
  `joc_spatial_bridge.rs`, and its schema label is now
  `openjoc.joc-spatial-bridge.v1`. The bridge function remains experimental
  and semantically unresolved, while those states are documented separately
  from the stable names. No schema version bump was required because the
  payload shape is unchanged and no committed artifact parser accepts the old
  label.
- Renamed the canonical compatibility policy from `DOLBY_VENDOR_COMPAT` to
  `OBSERVED_VENDOR_COMPAT` because the former implied stronger vendor-specific
  semantics than OpenJOC establishes. New CLI help, diagnostics, manifests,
  and documentation use the canonical name. The former CLI and fixture-manifest
  spellings remain accepted only as input aliases during this 0.x migration.
- Renamed the binding provenance value `ControlledCleanroomEmpirical` to
  `ControlledEmpirical`; the former serialized value remains accepted as an
  input alias and new serialization emits `CONTROLLED_EMPIRICAL`.
- Renamed the unresolved bridge reason `ExperimentalSemanticAmbiguity` to
  `SemanticAmbiguity`; the former serialized value remains accepted as an
  input alias.

### Scope

- Renderer inputs are caller-supplied explicit mono sources; unresolved JOC
  ReconstructionBasis rows are not converted into authored-object sources.
- The 2D renderer ignores elevation, excludes LFE/bass management, rejects
  uncovered angular gaps, and does not provide a JOC semantic bridge.
- The 3D trajectory path remains directional and explicit-topology only: it has
  no distance, Doppler, listener orientation, room, LFE, HRTF, or JOC bridge.
- The binaural path is a fixed-listener, static-source reference renderer: it
  accepts only caller-supplied synthetic/test or runtime HRIR taps, requires
  exact direction and sample-rate matches, and does not claim proprietary HRTF
  or perceptual renderer fidelity.
- SOFA loading is local-file-only and construction-time-only. The strict
  loader supports the project-tested `SimpleFreeFieldHRIR` 1.0–1.2 contract
  in portable NetCDF classic CDF-1 form, with exactly two receivers and
  spherical degree/degree/metre source positions. It does not write SOFA,
  download datasets, or grant rights to a caller-supplied dataset.

## [0.2.0] — 2026-08-13

Prepared as a local release freeze. Tagging and publication remain separate
human-controlled actions.

### Added

- Truthful decoded-component manifests that keep Base, Base LFE, indexed
  ReconstructionBasis coordinates, and RcLfe boundaries explicit.
- Bounded-memory streaming component export and versioned machine-readable
  output contracts.
- Deterministic `openjoc --version` / `-V` output and safe create-once output
  directory behavior.

### Changed

- ReconstructionBasis terminology and semantic boundaries now explicitly
  separate diagnostic rows from authored-object identity.
- CLI and release documentation describe the current candidate line rather
  than the historical 0.1.0 release.

### Fixed / hardened

- Checked offsets, sizes, malformed-input paths, transactional failure
  behavior, and sink-error propagation across raw and container streaming.

### Known limitations

- `SemanticBindingState` remains `Unresolved`; authored-object PCM, an
  audio-bound `ObjectScene`, and renderer fidelity are not admitted.
- The active-companion ReconstructionBasis operator remains a hard research
  blocker; no vendor warp-3 semantics or raw3 compatibility rule was added.
- Platform, signing, and notarization scope remain those documented in the
  candidate capability snapshot.

## [0.1.0] — historical

The original 0.1.0 release remains the historical baseline and tag. Its
artifacts and evidence are preserved; this candidate does not rewrite that
release history.
