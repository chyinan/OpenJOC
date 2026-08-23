# Roadmap

This document contains future or explicitly deferred work only. It is not a
commitment or schedule. Implemented capability status belongs to
[CAPABILITIES.md](CAPABILITIES.md), and release chronology belongs to the
[CHANGELOG](../CHANGELOG.md).

## Active engineering priorities

- Extend malformed-input hardening and fuzz coverage without weakening
  fail-closed profile, size, topology, and output rules.
- Expand controlled real-producer coverage for E-AC-3 coding-tool combinations
  whose public-syntax admission currently exceeds the available real-corpus
  activation evidence.
- Keep input/container and incremental output ownership explicit if new
  container forms are considered. Non-seekable and fragmented MP4 remain
  outside the current contract unless separately designed and admitted.
- Improve cross-platform hardware and long-run acceptance where current CI
  proves PCM generation/transport but not every physical speaker device or
  audio-output stack.
- Decide signing, notarization, attestation, and publication-policy changes
  explicitly before making broader distribution claims.

## Research priorities

- Resolve the remaining codec-domain JOC spatial operator `T(t)` from
  independently testable, admissible evidence. Existing negative results rule
  out fixed row/object and tested source-locked/cyclostationary models; do not
  bypass that boundary with an ad hoc mapping.
- Obtain admissible evidence for unresolved vendor OAMD continuation without
  weakening `ETSI_STRICT` or assigning semantics to opaque fields.
- Keep metadata understanding separate from authored-object binding. Any
  future binding claim requires independent identity, timing, negative-control,
  and repeatability evidence.

## Deferred candidates

These items are speculative and have no committed schedule:

- broader SOFA convention/container support and sample-rate conversion;
- additional Region fallback/tie behavior and richer presentation names;
- live integration target/layout switching with explicit renderer-state
  transitions;
- additional player, device, and platform qualification;
- an automatic 3D topology generator, but only as a separately bounded module
  that does not change caller-declared `LayoutRenderer3d` topology semantics.

Completed speaker presets, custom 64-channel geometry, C ABI 1.4, FFmpeg,
GStreamer, mpv/player packaging, DirectShow/LAV/PotPlayer onboarding, and
release automation are intentionally absent from this future-work list.
