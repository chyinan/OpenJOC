# OpenJOC 0.9.2 known limitations

> Canonical owner: current user-visible limitations and non-claims. Historical
> research and implementation chronology belong in the source-only documents.

OpenJOC 0.9.2 is an independent, experimental interoperability project. It
does not claim Dolby endorsement, certification, a licensed implementation, or
bit-identical Reference Player output.

- `scene.json` is metadata-only. It does not bind decoded audio to authored
  objects. `SemanticBindingState` remains `Unresolved`, and
  `ReconstructionBasis` rows are diagnostic reconstruction coordinates rather
  than verified authored-object PCM.
- `export-adm` writes a reconstructed RIFF/RF64 ADM BWF interoperability representation,
  not the original ADM/BWF master. Best-effort output keeps reconstruction
  signals neutral and reports the unresolved audio-to-spatial-metadata binding;
  strict mode rejects it. Original names, hierarchy, UIDs, and discarded source
  information cannot be recovered.
- The Dolby Atmos master profile has no mono LFE bed. When Base LFE is present,
  `export-adm` creates the minimum legal 5.1 transport bed: only LFE contains
  recovered PCM; L, R, C, Ls, and Rs are deterministic silence placeholders
  explicitly identified in the report. They are not recovered authored bed
  content. The generated `dbmd` contains only the public EBU Supplement 6
  envelope, not reserved Atmos-specific segment payloads.
- R2 imports successfully in Logic Pro, but Dolby Encoding Engine rejects the
  byte-exact OpenJOC file after parsing with `Content was not authored with
  Dolby tools.` OpenJOC does not forge Dolby authoring provenance. A re-export
  from an authorized Atmos authoring tool is a separate derived workflow. The
  maintainer verified that Logic's re-export is accepted by DEE; direct DEE
  ingest of OpenJOC output remains unsupported and unclaimed.
- Compressed `export-adm` uses bounded-memory production streaming. Explicit
  `ObjectScene` JSON and scene-directory inputs remain diagnostic formats and
  may already materialize programme-duration PCM before the ADM writer sees
  them.
- The embeddable packet API accepts exactly one complete E-AC-3 JOC access unit
  per push. Arbitrary byte fragmentation, multiple AUs in one push, demuxing,
  CLI parsing, and filesystem output remain outside `OpenJocSession`.
- The C ABI is experimental ABI 1.3. It is intended for integration during the
  OpenJOC 0.x series; layout and compatibility details may evolve. The
  published header and platform libraries are the supported C consumer
  surface. The repository provides GStreamer integration, an external FFmpeg
  bridge, a native `libopenjoc` source patch, and a source-only mpv patchset;
  stock FFmpeg binaries do not contain it, and no custom player binary is
  publicly released as upstream mpv/FFmpeg products. Reproducible OpenJOC
  Player Bundle release candidates exist for the qualified macOS arm64, Linux
  x86_64, and Windows x64 surfaces; they are not official mpv/FFmpeg releases.
- The codec-domain JOC operator `T(t)` remains unresolved. The separate
  `render-joc` path is an experimental ordinary-domain speaker projection that
  assembles control from decoded JOC/OAMD state; it does not infer authored
  object identity or resolve the codec-domain operator.
- The public `render-joc` presets are `5.1`, `5.1.2`, `5.1.4`, `7.1`, `7.1.2`,
  `7.1.4`, `7.1.6`, `9.1`, `9.1.2`, `9.1.4`, `9.1.6`, and `22.2`. `7.1.6` and the
  `9.1` family require semantic CAF output. WAV requests for layouts without
  an exact WAVEFORMATEXTENSIBLE mask fail closed; no identity substitution or
  fabricated mask is used. `2.0` and the current `5.1`-through-`9.1.6` family
  are public CLI presets; `22.2` uses explicit unmasked WAV or rich CAF
  metadata because no complete standard WAVEFORMATEXTENSIBLE mask exists.
- E-AC-3 dynamic-range control is metadata-driven. `--drc` selects Disabled,
  Line, RF, or Custom signed-fraction scaling; it remains separate from
  dialnorm. OpenJOC's automatic Default dialnorm policy applies the supported
  E-AC-3 metadata-derived calibrated program scalar; this is not content-
  dependent LUFS/peak normalization, a signal-level compressor, or a limiter.
- Calibrated dialnorm can request substantial attenuation for some programs;
  a lower absolute offline file level is not by itself a decoder bug. When a
  conveniently loud offline file is wanted, use optional post-render
  `--normalize-peak TARGET_DBFS`. `--dialnorm analog` remains available as an
  advanced unity-dialnorm compatibility/diagnostic policy, not as the default
  louder-output workflow.
- The admitted `2.0` speaker output uses the generic two-speaker projector for
  reconstructed/object coordinates and public ETSI Lo/Ro/Lt/Rt matrices for
  supported 5.1 Base channels. Optional E-AC-3 LFE metadata may fold LFE into
  stereo; absent metadata excludes LFE. Base back/height channels are rejected
  for full 2.0 output because the public 6.8 matrix does not define their
  reduction. No crossover, subwoofer redirect, or other bass-management DSP is
  performed.
- The generic `SpatialLayout`/`JocSpatialBridge` library API accepts caller-
  defined layouts, but the CLI has no custom-layout file format. The public
  22.2 contract is renderer/container-specific and does not imply third-party
  DAW interoperability.
- The admitted Dynamic Region/Zone contract is limited to six horizontal
  states (`NoConstraints`, `BackExcluded`, `SideExcluded`, `CentreAndBack`,
  `ScreenOnly`, and `SurroundOnly`) plus independent Top-Bottom
  include/exclude on validated one- or two-plane layouts. Region selects a
  constrained topology before projection, and points outside selected support
  clamp to its endpoints.
- Ordinary Dynamic Extent is supported on the admitted 5.1-through-22.2 layouts. It
  reduces the three size components to one isotropic scalar, preserves the
  point target at zero, and uses the existing Q32 target scheduler. Region ×
  Extent uses the Region-first effective topology and retains the authored
  center for the Extent path.
- Ordinary point Dynamic ChannelLock is supported across the current one- and
  two-layer topology family. Region is applied first; an active ChannelLock
  owns current target generation and bypasses the Extent target branch while
  retaining Extent state. When ChannelLock is released, inherited Extent
  behavior resumes. Non-point ChannelLock, selector-6 special behavior, rare
  Region fallback/tie cases, arbitrary region algebra, and unadmitted
  layer/fallback combinations remain withheld and fail closed.
- Fixed routing is supported when a validated neutral family/member key and an
  exact current-layout route row are supplied; authored coordinates do not
  participate and missing rows fail closed. Named routing accepts neutral
  `named/<0..15>` identities on the public layouts, including the admitted 22.2
  topology. Supplied direct
  rows are copied unchanged; authorized fallback families derive semantic
  non-LFE vectors from the current layout and use the existing Q32 scheduler.
  The explicit LFE-target cells, zero-survivor fallback families, and
  malformed or out-of-domain identities remain fail closed. Friendly Named
  display names are intentionally not exposed.
- Real-JOC binaural output uses the bundled SADIE II D1 generic HRTF by
  default, or a user-provided compatible SOFA override. It is speaker
  virtualization through a strict `SimpleFreeFieldHRIR` SOFA bank. The default
  virtual field is `7.1.4`; the
  public `5.1`/`7.1`/`9.1`/`22.2` families are eligible when every non-LFE direction is
  exact or safely interpolatable from the selected dataset. Interpolation is a
  bounded spherical-local segment/triangle method with shared ear weights and
  separate integer delay alignment; sparse or outside-domain requests fail
  closed. HRIR resampling and omitted-channel fallback are not used, and a
  SOFA/input sample-rate mismatch is rejected rather than silently converted.
  An
  explicit `exclude` or `equal-power-dual-mono` LFE policy is required.
- The strict `openjoc-sofa` loader accepts only the tested local
  `SimpleFreeFieldHRIR` NetCDF classic CDF-1 subset: fixed listener pose,
  spherical metre/degree coordinates, two receivers, common sample rate, and
  integer nonnegative delays. HDF5/NetCDF-4, other conventions, writing,
  downloads, moving sources, and resampling are not supported. Dataset support
  remains capability-dependent; the loader does not claim universal coverage.
- The independent `openjoc-render` foundation remains caller-bound. It
  supports explicit mono-source 2D/3D rendering and static exact-direction
  binaural reference paths, but does not provide JOC semantic binding, room or
  distance modeling, Doppler, head tracking, or a vendor renderer.
- JOC clip-gain fields are parsed as part of the public syntax boundary, but
  clip-gain rendering semantics are not applied by the public renderer.
- `ETSI_STRICT` rejects the observed reserved OAMD warp value `raw=3`.
  `OBSERVED_VENDOR_COMPAT` is explicit and partial; opaque continuation is
  retained without assigning vendor semantics. Malformed, unsafe, unknown, or
  non-whitelisted metadata remains a failure.
- Raw E-AC-3 streaming and seekable ordinary ISO BMFF input are supported
  within the documented boundaries. Non-seekable and fragmented MP4 are not
  admitted. Some capture/demux and compatible-base paths require `ffmpeg` or
  `ffprobe`.
- Public syntax coding-tool support has bounded synthetic/numerical coverage;
  full real-world activation and fidelity for every E-AC-3 coding-tool
  combination are not claimed.
- The QMF and reconstruction diagnostics establish the 577-sample round-trip
  and zero Base/RB bridge lag contracts. They are engineering regressions, not
  a subjective real-media or realtime qualification. Real-media listening and
  long-render acceptance remain manual release steps.
- Decode output directories are create-once destinations. Render replacement
  requires interactive confirmation or `--overwrite`, and transactional output
  and input/output alias protection remain in force.
- The release workflow targets macOS arm64, Windows x86_64, and GNU/Linux
  x86_64, while the player packaging workflow qualifies macOS arm64, Linux
  x86_64, and Windows x64. Multichannel PCM generation and transport are
  qualified in CI; physical speaker-system playback has not been separately
  validated on Linux/Windows hardware. The macOS artifact is ad-hoc signed,
  not Developer-ID signed, and not notarized. Linux compatibility is bounded
  by the Ubuntu 24.04/glibc baseline recorded in each package's `BUILD_INFO`;
  Windows uses the qualified MSYS2 MinGW-w64 extract-and-run DLL model.

See the [capability matrix](CAPABILITIES.md) and [JOC rendering contract](JOC_RENDER.md)
for the corresponding positive support claims.
