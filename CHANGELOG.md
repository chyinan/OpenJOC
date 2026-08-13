# Changelog

## [0.2.0-rc.1] — unreleased

This is a local release candidate, not a final or published release.

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
