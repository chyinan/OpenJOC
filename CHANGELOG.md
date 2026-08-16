# Changelog

## [0.4.2] — 2026-08-16

OpenJOC 0.4.2 is a focused patch release for JOC reconstruction diagnostics
and safer `render-joc` output handling. It preserves the 0.4.x feature freeze
and does not change JOC decoding or rendering semantics.

### Improved

- JOC QMF reconstruction caches invariant phase and prototype tables while
  retaining identical reconstruction output checksums.
- Versioned performance reports include opt-in JOC reconstruction-stage timing
  diagnostics, including QMF analysis/synthesis and matrix reconstruction.
- `render-joc` preflights WAV/report collisions and input/output aliases,
  prompts interactively with `[y/N]`, and supports `--overwrite` for scripts
  and non-interactive use.
- Authorized output replacement remains transactional: a failed render keeps
  the previous final output and performance report intact.

### Known limitations

- The QMF and speaker/WAV measurements are synthetic engineering diagnostics;
  representative real E-AC-3/JOC media still requires a real-media performance
  retest. These measurements do not establish realtime readiness.
- `JocSpatialBridge` remains Experimental and `SemanticBindingState` remains
  `Unresolved`; no renderer-fidelity equivalence with Dolby is claimed.

## [0.4.1] — 2026-08-16

OpenJOC 0.4.1 is a patch release focused on real-world usability, diagnostics,
and OAMD configuration correctness.

### Fixed

- Ordinary valid `render-joc` input no longer requires users to provide
  `--trim-config-count` when it is omitted. The shared normative OAMD default
  resolves `NUM_TRIM_CONFIGS` to `9`; the explicit option remains available as
  an expert override.
- OAMD configuration resolution is consistent across render, decode, and
  inspect paths without changing `AUTO` profile selection or rendering
  semantics.

### Improved

- `render-joc` provides TTY-aware terminal progress on stderr and supports
  `--no-progress` for explicit opt-out without corrupting stdout diagnostics.
- `--performance-report` writes versioned machine-readable stage timings and
  frame percentile diagnostics.
- Low-risk render and WAV allocation improvements reduce avoidable per-frame
  work while retaining the existing output contract.

### Known limitations

- Synthetic benchmarks improved substantially, but real E-AC-3/JOC performance
  still requires qualification on representative media. A real-media
  performance retest is required.
- `JocSpatialBridge` remains Experimental and `SemanticBindingState` remains
  `Unresolved`; no renderer-fidelity equivalence with Dolby is claimed.

## [0.4.0] — 2026-08-15

OpenJOC 0.4.0 is a feature release that makes the experimental JOC spatial
rendering path usable from decoded real-JOC input while preserving explicit
semantic and fidelity boundaries.

### Added

- End-to-end experimental JOC speaker rendering through the existing
  `JocSpatialBridge`, with explicit bridge-control topology, persistent AU
  state, selectable 5.1/5.1.2/7.1/7.1.4 WAV output, and truthful render
  diagnostics.
- Automatic real-JOC bridge-control assembly from decoded metadata and
  codec-coordinate state; `CONTROL.json` is now an optional complete explicit
  override/test input rather than a normal rendering requirement.
- Real-JOC SOFA-backed binaural rendering through static virtual-speaker
  directions, exact HRIR preflight, direct or partitioned convolution, complete
  causal tail draining, and an explicit renderer-level LFE policy.

### Changed

- `AUTO` remains the normal user-facing validation default. It evaluates
  `ETSI_STRICT` first and selects `OBSERVED_VENDOR_COMPAT` only for the existing
  fully admitted compatibility set; explicit `ETSI_STRICT` never falls back.
- `CONTROL.json` remains an optional complete explicit override/test input.
  Ordinary supported real-JOC rendering assembles bridge control automatically
  from decoded JOC/OAMD/reconstruction state.

### Rendering

- `render-joc` exposes deterministic `5.1`, `5.1.2`, `7.1`, and `7.1.4`
  speaker presets with stable public WAV channel order and separate LFE
  handling.
- The generic library layout engine remains broader than the admitted CLI
  preset list. Unadmitted 2.0, 5.1.4, 7.1.2, 9.1.4, 9.1.6, and 22.2 CLI
  outputs are not introduced by this release.

### Binaural

- Real JOC binaural output first renders a selected virtual speaker layout and
  then applies user-supplied exact-direction SOFA HRIR data. `direct` is the
  reference backend; `partitioned` is the efficient fixed-partition backend.
- The renderer requires an explicit LFE policy: `exclude` or
  `equal-power-dual-mono`. These are OpenJOC renderer policies, not JOC
  semantics or vendor bass-management behavior.

### Experimental

- The workflow uses the existing `AUTO`, `ETSI_STRICT`, and
  `OBSERVED_VENDOR_COMPAT` profile policy. `SemanticBindingState` remains
  `Unresolved`; no authored-object mapping or vendor-fidelity claim is added.

### Known Limitations

- The preset geometry is data consumed by the generic bridge; 2.0 remains
  blocked by unspecified LFE/bass fold-down policy. The generic library
  accepts arbitrary validated N-channel `SpatialLayout` data, including
  multi-axis and high-channel-count layouts; the CLI exposes only the four
  admitted convenience presets. 5.1.4, 7.1.2, 9.1.4, 9.1.6, and 22.2 remain
  blocked by missing admitted clean geometry. Ordinary WAV output remains
  RIFF-only without speaker-label metadata.
- JOC binaural output is stereo speaker virtualization through a user-supplied
  exact-direction SOFA bank; it makes no official vendor-fidelity or bit-exact
  reference-renderer claim, does not resolve semantic binding, and requires
  matching SOFA/input sample rates. Public real-media smoke fixtures may
  remain unavailable.

## [0.3.0] — 2026-08-15

OpenJOC 0.3.0 is a feature release focused on a spatial-rendering foundation,
an experimental JOC spatial bridge, and clearer user-facing profile selection.

### Added

- Explicit spatial-scene rendering with validated 2D speaker layouts, 3D
  explicit-triplet layouts, sample-accurate trajectories, and caller-owned
  block outputs.
- Static binaural rendering with a direct FIR reference backend, a uniform
  partitioned-convolution backend, and strict local `SimpleFreeFieldHRIR`
  SOFA import for the supported NetCDF classic CDF-1 subset.
- `JocSpatialBridge` for codec-coordinate binding, spatial projection, Q32 gain
  scheduling, and linear accumulation in the supported ordinary domain.

### Changed

- User-facing `decode` and `decode-payload` profile selection now defaults to
  `AUTO`, which tries `ETSI_STRICT` before selecting a fully admitted
  `OBSERVED_VENDOR_COMPAT` fallback.
- `ETSI_STRICT` remains explicit and strict; it never falls back. The canonical
  observed-deviation policy name is `OBSERVED_VENDOR_COMPAT`.
- Public bridge and profile names are stable functional names; maturity,
  validation evidence, and unresolved semantics are documented separately.

### Experimental

- 0.3.0 introduces an experimental implementation of the JOC spatial bridge
  for the currently specified supported domain. Its
  `SemanticBindingState` remains `Unresolved`, and official runtime-oracle
  fidelity is not independently confirmed.

### Compatibility

- `AUTO` falls back only when every blocking deviation is admitted by the
  observed-vendor compatibility whitelist. Malformed, unknown, or
  non-whitelisted failures remain failures.
- Legacy compatibility spellings remain accepted only as intentional input
  aliases where documented; canonical output uses `OBSERVED_VENDOR_COMPAT`.
- Raw warp value 3 remains opaque and preserved, excluded from ordinary
  projection arithmetic, and unresolved in meaning.

### Known Limitations

- The bridge does not claim official vendor equivalence, bit-exact reference
  renderer equivalence, or a resolved JOC semantic binding.
- The supported domain is narrower than all JOC content. Some bridge rules and
  constants remain experimentally specified rather than independently
  vendor-validated, and no official runtime reference confirmation is
  established.
- Explicit renderer workflows use caller-supplied sources and do not turn
  unresolved reconstruction rows into authored-object audio.
- Platform validation and publication remain scoped to the local Apple-silicon
  macOS release-candidate workflow; no 0.3.0 tag or remote release is created
  by this source closure.

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
