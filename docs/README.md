# OpenJOC documentation

Each current technical fact has one canonical owner. Secondary documents
summarize host- or workflow-specific implications and link back to that owner.

## Getting started

- [Project README](../README.md) — product overview, CLI and PotPlayer quick
  starts, build instructions, and navigation.
- [Capabilities](CAPABILITIES.md) — canonical current support/status matrix.
- [Known limitations](KNOWN_LIMITATIONS.md) — canonical current user-visible
  limitations and non-claims.
- [Public smoke fixture](PUBLIC_SMOKE_FIXTURE.md) — installation and bounded
  synthetic health checks.

## Renderer and output

- [JOC speaker and binaural rendering](JOC_RENDER.md) — canonical
  `render-joc` contract, preset/output behavior, latency, and level policies.
- [Custom speaker layouts](CUSTOM_SPEAKER_LAYOUTS.md) — versioned JSON, Rust,
  and C geometry contract up to 64 ordered output channels.
- [Spatial portability](SPATIAL_PORTABILITY.md) — 22.2 and binaural portability
  boundaries.
- [JOC spatial bridge](JOC_SPATIAL_BRIDGE.md) — supporting bridge activation
  and unresolved semantic boundary.
- [Explicit render-scene workflow](RENDER_SCENE.md) — caller-bound sources;
  separate from JOC authored-object semantics.

## Developer APIs

- [Rust library API](LIBRARY_API.md) — `OpenJocSession`, packet ownership,
  output frames, latency, lifecycle, and policies.
- [C ABI](C_API.md) — current ABI 1.4, opaque handles, stream/classifier
  surfaces, ownership, custom geometry, and failure containment.
- [Production architecture](ARCHITECTURE.md) — canonical data flow and
  component ownership.

## Integrations

- [Windows DirectShow / LAV / PotPlayer](integration/LAV_FILTERS_OPENJOC.md)
- [FFmpeg external bridge](integration/FFMPEG.md)
- [Native FFmpeg `libopenjoc` wrapper](integration/FFMPEG_NATIVE.md)
- [GStreamer](integration/GSTREAMER.md)
- [mpv](integration/MPV.md)
- [OpenJOC Player Bundle packaging](integration/PLAYER_PACKAGING.md)
- [Ecosystem packages](integration/ECOSYSTEM_PACKAGING.md)

Integration documents own only their framework transport, lifecycle,
selection, and host/output boundaries. Renderer capabilities remain owned by
[JOC_RENDER.md](JOC_RENDER.md) and [CAPABILITIES.md](CAPABILITIES.md).

## ADM and interchange

- [Reconstructed ADM BWF export](ADM_EXPORT.md) — current export, validation,
  and semantic boundary.

## Architecture, provenance, and planning

- [Architecture](ARCHITECTURE.md) — current production ownership.
- [Provenance](PROVENANCE.md) — clean-room/evidence policy and retained
  provenance chronology.
- [Roadmap](ROADMAP.md) — future or explicitly deferred work only.
- [Research history](research/README.md) — dated experiments and negative
  results; never the current capability owner.

## Release and historical material

- [CHANGELOG](../CHANGELOG.md) owns release-by-release chronology.
- [`docs/release/`](release/) contains current release packaging,
  corresponding-source, and distribution evidence.
- [Historical archive](archive/README.md) contains retained release contracts
  and requirement/evidence documents that no longer describe current
  behavior.

Contributor verification and repository-documentation hygiene rules are in
[CONTRIBUTING.md](../CONTRIBUTING.md).
