# Changelog

## [0.4.0-dev] — Unreleased

### Added

- End-to-end experimental JOC speaker rendering through the existing
  `JocSpatialBridge`, with explicit bridge-control topology, persistent AU
  state, deterministic 5.1 WAV output, and truthful render diagnostics.

### Scope

- The workflow uses the existing `AUTO`, `ETSI_STRICT`, and
  `OBSERVED_VENDOR_COMPAT` profile policy. `SemanticBindingState` remains
  `Unresolved`; no authored-object mapping or vendor-fidelity claim is added.
- Only the documented 5.1 speaker layout is exposed. Binaural rendering and
  additional layouts remain follow-up work.

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
