# Changelog

## [0.9.1] — 2026-08-21

OpenJOC 0.9.1 is a focused post-release hotfix for real compressed-input ADM
export and Windows ecosystem runtime qualification.

### Hotfixes

- Fixed `export-adm` compressed-input staging so the decoder receives an
  uncreated `root/scene` beneath an owned temporary root; decode failures and
  successful exports both clean the root without weakening overwrite safety.
- Added end-to-end compressed-input ADM export coverage, adjacent report and
  validator checks, and success/failure staging-cleanup regressions.
- Fixed Windows SDK and custom FFmpeg ecosystem packages to ship the complete
  recursive non-system PE DLL closure for their runtime roots.
- Made ecosystem verification hermetic, added Windows negative missing-DLL
  smoke coverage, recursive zero-missing closure audits, and isolated
  Linux/macOS loader-path verification.
- Made `reconstructed.wav` the primary ADM BWF filename convention while
  retaining `.bw64`; both extensions emit and validate the same BW64 WAVE-family
  container.

## [0.9.0] — 2026-08-21

OpenJOC 0.9.0 — Interchange & Ecosystem adds reconstructed ADM/BW64 export,
qualified ecosystem package surfaces, and a public self-test workflow around
the existing decoder, renderer, and player integrations.

### Reconstructed ADM/BW64

- Added `openjoc export-adm` and `openjoc validate-adm` with deterministic
  `ds64`, `fmt `, `data`, `axml`, and `chna` output plus an adjacent
  `*.adm-report.json` loss/provenance report.
- Kept the renderer-independent boundary honest: `ReconstructionBasis` rows
  are exported as neutral reconstructed signals while the unresolved
  audio-to-spatial-metadata binding is reported as `UNRESOLVED`; strict policy
  rejects it rather than guessing.
- Documented ITU-R BS.2076-3, ITU-R BS.2088-2, EBU `chna` semantics, generated
  names, loss boundaries, deterministic timing, and deferred DAW testing.

### Ecosystem and self-test

- Added deterministic SDK, custom FFmpeg, and GStreamer plugin pack builders
  with package manifests, dependency/license inventories, checksums, private
  path scans, extraction verification, and target runtime smoke gates.
- Added `openjoc self-test` and a project-owned public synthetic JOC fixture
  generation/qualification procedure.
- Preserved the experimental C ABI at 1.3; the package version does not imply
  permanent ABI stability during 0.x.

### Release limitations

- The ADM output is reconstructed interoperability metadata, not original ADM
  master recovery or a lossless JOC round trip.
- The FFmpeg archives are custom OpenJOC builds, not official upstream FFmpeg
  releases; GStreamer packs require the recorded matching runtime.
- Real DAW import testing and physical Linux/Windows multichannel hardware
  playback remain outside the automated 0.9 acceptance boundary.

## [0.8.0] — 2026-08-20

OpenJOC 0.8.0 — Cross-Platform Player Integration extends the shared OpenJOC
renderer from an embeddable library surface into a qualified cross-platform
player-capable stack. Users of 0.7.0 are encouraged to upgrade for the new
player integrations, native 22.2 output, built-in binaural resource, and
accumulated integration fixes.

### Highlights

- One OpenJOC renderer supplies the same spatial semantics to the CLI, Rust and
  C APIs, GStreamer, FFmpeg integrations, and the OpenJOC-enabled mpv bundles.
- Native 22.2 rendering follows ITU-R BS.2051-3 Sound System H with 24 PCM
  channels, 22 spatial speakers, and separate LFE1/LFE2 destinations.
- `--binaural` works offline with the bundled generic SADIE II D1 KU100 HRIR
  resource at 48 kHz and 256 taps; compatible custom SOFA input remains
  supported.

### Spatial Rendering

- Generalized the multilayer projection path for the 22.2 bottom, middle,
  upper, and top speaker layers while preserving semantic speaker identity
  equivalence and the existing DSP contracts.
- Closed the real-media 22.2 rendering path with explicit 24-channel output,
  semantic channel ordering, and LFE1/LFE2 handling.
- Preserved the distinction between physical 2.0 `FL`/`FR` rendering and
  binaural virtual-speaker rendering to Left Ear/Right Ear. Both are stereo
  PCM, but they are not the same render.

### Binaural

- Added the built-in SADIE II D1 KU100 generic HRTF with no runtime network
  dependency or personalized-HRTF claim.
- Kept the existing interpolation, integer delay alignment, direct/partitioned
  convolution, LFE policy, sample-rate checks, and custom SOFA override path.
- Added required SADIE II publisher, license, attribution, citation, and
  resource-hash records to the release notice surfaces.

### GStreamer

- Added the native Rust `gst-plugin-openjoc` integration with `openjocclassify`
  and `openjocdec`, positive JOC autoplugging, ordinary E-AC-3 isolation,
  decoder lifecycle/EOS/drain/seek handling, output-target negotiation,
  physical speaker rendering, binaural output, and semantic channel masks.
- Preserved exact frontend PCM parity by keeping OpenJOC responsible for the
  spatial DSP while GStreamer supplies buffer and device transport.

### FFmpeg

- Added the external libavformat-facing packet/frame bridge with bounded
  packet-to-access-unit assembly, rational timestamps, AVFrame output,
  channel-layout semantics, binaural/22.2 targets, drain/flush/seek handling,
  and controlled PCM parity.
- Added the native `libopenjoc` libavcodec wrapper as a reproducible source
  patch for FFmpeg 9.0.1 and the recorded master baseline. Stock `eac3` is
  preserved and the OpenJOC decoder remains explicitly named.
- Published the exact pinned FFmpeg bases, patch hashes, configure policy, and
  OpenJOC C ABI integration metadata without vendoring FFmpeg source.

### mpv / Player Integration

- Added the pinned mpv 0.41.0 OpenJOC patchset and master compatibility patch,
  including positive JOC selection, ordinary E-AC-3 isolation, explicit
  decoder overrides, passthrough separation, binaural transport, and physical
  2.0/5.1/7.1.4/9.1.6/22.2 profiles.
- Added `openjoc-mpv` extract-and-run bundles for qualified macOS arm64, Linux
  x86_64, and Windows x64 environments. Ordinary E-AC-3 continues through
  `eac3`; confirmed JOC selects `libopenjoc`.
- Release candidates are named `openjoc-mpv-0.8.0-<platform>` and record the
  exact OpenJOC commit, pinned stack, dependency closure, package checksums,
  and qualification metadata in `BUILD_INFO` and related manifests.

### Cross-Platform Packaging

- Retained the OpenJOC CLI/library platform assets, public `openjoc.h`, C ABI
  1.3 libraries, and checksums while adding the player bundle asset surface.
- Qualified macOS arm64, Linux x86_64, and Windows x64 player paths with
  reproducible archive creation, runtime/dependency audits, third-party
  notices, HRTF identity checks, and private-path scans.
- macOS packages are ad-hoc signed where required and are not Developer-ID
  signed or notarized. Linux/Windows software paths are CI-qualified; physical
  multichannel speaker playback has not been separately validated on
  Linux/Windows hardware.

### API / ABI

- Kept the experimental versioned C ABI at 1.3; the package version is 0.8.0
  and does not change the ABI major/minor.
- Preserved classifier, streaming session, native FFmpeg consumer,
  struct-size compatibility, panic containment, and multi-instance contracts.

### Fixes

- Hardened cross-platform package dependency closure, license inventory,
  runner-path sanitization, Windows DLL checks, archive/checksum naming, and
  portable synthetic qualification fixtures.
- Kept the cross-terminal CLI progress refresh fix and reconciled release
  metadata warnings without changing DSP behavior.

### Known Limitations

- The C ABI remains experimental during OpenJOC 0.x.
- FFmpeg/mpv integrations are project-provided custom source patches/builds;
  upstream FFmpeg and mpv do not officially ship OpenJOC, and the bundles are
  not official upstream distributions.
- Linux/Windows physical speaker-system playback is outside this qualification
  boundary; CI qualifies PCM generation and transport instead.
- The macOS player bundle is ad-hoc signed and not notarized. Linux packages
  target the Ubuntu 24.04/glibc baseline recorded in `BUILD_INFO`; Windows
  packages use the qualified MSYS2 MinGW-w64 extract-and-run DLL model.
- Existing packet/input, semantic-binding, custom-SOFA coverage, and other
  fail-closed limitations remain as documented in `docs/KNOWN_LIMITATIONS.md`.

### Upgrade Notes

Users of 0.7.0 should upgrade to 0.8.0 for the new GStreamer, FFmpeg, and mpv
integration surfaces, native 22.2 rendering, built-in generic binaural HRTF,
cross-platform player packages, and accumulated correctness and packaging
hardening. The 0.7.0 library and CLI contract remains the foundation of the
upgrade; the C ABI remains experimental.

## [0.7.0] — 2026-08-19

OpenJOC 0.7.0 — Library Integration & Output Fidelity makes the decode/render
engine embeddable and completes important speaker-output fidelity and level
policy work. Users of 0.6.0 are strongly encouraged to upgrade.

### Highlights

- A headless Rust `OpenJocSession` / `OpenJocConfig` API accepts one complete
  E-AC-3 JOC access unit per packet and returns owned interleaved `f32` PCM with
  sample-domain timestamps, semantic channel labels, drain, flush, reset, and
  discontinuity handling.
- The CLI continues to use the shared engine, while separate sessions remain
  independent and the core does not require CLI parsing, terminal access, or
  filesystem output.

### Library & C Integration

- Added the experimental C ABI 1.1 with public `openjoc.h`, opaque decoder
  handles, numeric status codes, instance-owned errors, panic containment, and
  `struct_size` forward compatibility. ABI 1.0-size configuration callers are
  accepted with calibrated Default dialnorm behavior.
- Extended the source integration surface to experimental ABI 1.2 with a
  framework-neutral, bounded compressed-stream decoder handle. It reuses the
  proven packet/AU bridge for fragmentation, multi-AU input, positive JOC
  admission, semantic output order, drain, and reset while preserving all ABI
  1.0/1.1 callers.
- Platform release archives now carry the public header and the generated
  static/shared C ABI libraries, including the Windows import library where
  produced.
- The Rust API reports the public output delay: 609 samples for speaker output
  and 577 samples for binaural output.

### Output Fidelity

- Corrected the real-media 2.0 spatial projection path while retaining semantic
  `FL`/`FR` speaker identities.
- Applied the public Lo/Ro and Lt/Rt overload-protection scaling for supported
  Base downmixes; Auto continues to follow `dmixmod`.
- Added common linked speaker-output headroom behavior after combined speaker
  contribution. This is output headroom behavior, not a mastering limiter.

### Dialnorm & Output-Level Policies

- Completed calibrated program-level dialnorm handling across Base, Object, and
  full-program paths. Default is the recommended calibrated decoder behavior;
  Digital explicitly selects encoded digital calibration; Analog is an
  advanced unity-dialnorm compatibility/diagnostic policy.
- Added optional `--normalize-peak TARGET_DBFS` as a disabled-by-default,
  offline post-render sample-peak normalization step using one common static
  gain. It is separate from DRC and dialnorm and is not LUFS or true-peak
  normalization.

### CLI / Developer Experience

- Unified render diagnostics identify physical speaker output separately from
  binaural Left Ear/Right Ear output and report layout, channel order, policy,
  latency, and PCM frame/sample counts.
- Documented the offline workflow:
  `openjoc render-joc input.m4a --layout 7.1.4 --normalize-peak -0.1 -o output.wav`.
- Clarified that 0.7.0 provides the foundation for external-player and media-
  framework adapters; FFmpeg, GStreamer, mpv, VLC, DirectShow/LAV, and
  PotPlayer adapters are not shipped.

### Known Limitations

- The C ABI remains experimental and may evolve during OpenJOC 0.x.
- The packet API requires exactly one complete JOC access unit per push;
  arbitrary byte fragmentation and multiple AUs per push are unsupported.
- Binaural output still requires a compatible user-provided SOFA dataset, with
  finite supported container/profile boundaries and fail-closed sparse or
  out-of-domain coverage.
- JOC clip-gain rendering semantics remain outside the public renderer.

### Upgrade Notes

OpenJOC 0.7.0 includes important corrections to stereo downmix gain staging,
final speaker-output headroom behavior, and dialnorm program-level calibration.
Users of 0.6.0 are strongly encouraged to upgrade.

## [0.6.0] — 2026-08-18

OpenJOC 0.6.0 — Stereo, Binaural & Decoder Policy expands the public
speaker, source-routing, decoder-policy, and binaural workflows. Users of
earlier 0.x releases are strongly encouraged to upgrade due to the combined
reconstruction, routing, stereo, and binaural improvements.

### Added

- Fixed source routing and Named direct routing for the admitted neutral route
  identifiers, including deterministic Named fallback families where the
  current layout supplies a valid target set.
- E-AC-3 metadata-driven dynamic-range controls: Disabled, Line, RF, and
  Custom boost/cut percentages.
- Public `2.0` speaker stereo with `Auto`, Lo/Ro, and Lt/Rt downmix policies,
  `dmixmod` selection, public mix metadata, and optional metadata-gated LFE
  fold-down.
- Deterministic SOFA HRIR interpolation for safely covered spherical segments
  and triangles, including delay/ITD-aware ear alignment.

### Changed

- Dynamic Spread/Pair projection now uses the corrected admitted pair
  geometry and deterministic fail-closed handling for unsupported combinations.
- Binaural rendering now defaults to the internal virtual speaker field
  `7.1.4`, supports larger public virtual fields when the supplied SOFA data
  covers them, and always writes two-channel Left Ear/Right Ear output.
- Physical speaker layout and binaural virtual layout are documented and
  validated as separate concepts. The simple binaural CLI form is
  `--binaural --sofa FILE`; `--virtual-layout` selects only the internal field.
- Release-facing capability, routing, decoder-policy, stereo, binaural, and
  limitation documentation now describes the 0.6.0 public contract.

### Fixed

- Corrected automatic 2.0 JOC rendering so Base channels remain on the
  normative Auto/Lo/Ro/Lt/Rt downmix path while reconstructed JOC objects use
  generic spatial projection to the physical `FL`/`FR` pair.

### Known limitations

- Binaural rendering requires a user-provided compatible SOFA file; no generic
  HRTF is bundled. The admitted SimpleFreeFieldHRIR/NetCDF CDF-1 boundary,
  sample-rate match, sparse/outside-domain fail-closed behavior, and no
  silent resampling policy remain in force.
- `2.0` Base reductions remain constrained to the publicly specified channel
  set; unsupported Base back/height combinations fail closed and no playback
  crossover or bass-management DSP is invented.
- Friendly Named display names remain unavailable; neutral route identifiers
  are used. Unsupported Named cells and malformed route inputs fail closed.
- `dialnorm` remains decoded metadata and is not applied as calibrated
  playback-level normalization.

## [0.5.0] — 2026-08-17

OpenJOC 0.5.0 is a feature release focused on reconstruction fidelity and the
generic spatial-rendering path. Users of 0.4.x are strongly encouraged to
upgrade because this release corrects reconstruction timing and synthesis
behavior in addition to adding new output and dynamic-object coverage.

### Added

- Public speaker presets for `5.1.4`, `7.1.2`, `7.1.6`, and the `9.1`, `9.1.2`,
  `9.1.4`, and `9.1.6` family, extending the admitted preset set through
  `9.1.6`.
- Semantic Core Audio Format multichannel output for layouts whose speaker
  identities cannot be represented exactly by WAVEFORMATEXTENSIBLE.
- Ordinary Dynamic Region/Zone, Dynamic Extent, and Dynamic ChannelLock
  rendering for the admitted topology family, including Region × Extent and
  the unified Region-first/ChannelLock-precedence composition.

### Changed

- The speaker path now uses one generic full-XYZ, data-driven layer/row/anchor
  projector. Layout names select topology data and semantic channel order;
  they do not select separate projection mathematics.
- Speaker output now has an explicit semantic channel-order contract. WAV
  output remains fail-closed when a standard speaker mask cannot represent the
  requested layout; CAF preserves the richer semantic channel descriptions.
- Release-facing capability, limitation, provenance, installation, and
  platform documentation now identify 0.5.0 as the current release line.

### Fixed

- Corrected QMF reconstruction synthesis behavior and retained the 577-sample
  round-trip contract.
- Corrected Base/ReconstructionBasis timeline handling so the renderer input
  has zero lag under the established R2 alignment contract.
- Corrected generic point projection across X, Y, and Z, including endpoint,
  symmetry, normalization, wide-channel, and upper-row behavior.
- Corrected 7.1 and 7.1.4 speaker identity/order so back-left/back-right
  precede side-left/side-right in the public channel sequence.
- Preserved transactional output, overwrite checks, and input/output alias
  protection, including the empty-overwrite-prompt fix.

### Performance

- Retained the landed E-AC-3 reconstruction and JOC-stage performance work,
  including invariant QMF table reuse and versioned performance diagnostics.

### Known limitations

- Authored-object binding and the codec-domain JOC operator `T(t)` remain
  unresolved. The speaker bridge is experimental and makes no Dolby or
  Reference Player equivalence claim.
- Selector-6 special behavior, Spread/Pair, Fixed/Named routing, rare Region
  fallback/tie cases, >2-layer semantics, 22.2, and broader binaural policies
  remain withheld. Binaural output remains limited to the six exact-HRIR
  layouts; 7.1.6 and the 9.1 family are CAF speaker-output paths only.
- Real-media subjective listening and long-render acceptance remain manual
  release steps; synthetic regressions do not establish realtime readiness.

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
