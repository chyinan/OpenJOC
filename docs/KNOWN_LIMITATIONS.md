# OpenJOC 0.3.0-dev known limitations

> Canonical owner: current user-visible limitations and non-claims. Historical
> research and requirement status belong in the linked documents under `docs/`.

This snapshot is part of the local release-candidate contract. It states the
same user-visible boundary as the canonical repository documentation without
claiming capabilities that the current evidence does not support.

- `scene.json` is metadata-only. It does not bind decoded audio to authored
  objects.
- Diagnostic `ReconstructionBasis` rows are available, but they are not
  verified authored-object PCM. `SemanticBindingState` remains `Unresolved`.
- Authored-object PCM and an audio-bound `ObjectScene` are unavailable.
- The explicit `openjoc-render` foundation accepts caller-supplied mono sources
  and provides FL/FR equal-power stereo plus a general validated horizontal
  speaker-layout renderer with public 2D VBAP-style pair gains. It rejects
  uncovered angular gaps, ignores elevation, and has no LFE/bass-management,
  JOC semantic bridge, binaural renderer, room model, or Dolby renderer
  fidelity implementation. Its optional trajectories are sample-accurate
  piecewise-linear azimuth/gain paths on an absolute sample timeline; they do
  not model radius, distance, Doppler, elevation, or acceleration smoothing.
- `HARD_RESEARCH_BLOCKER_ACTIVE_COMPANION_RB_OPERATOR`: signal-dependent,
  window-dependent RB redistribution is admitted, but common gauge,
  row-transfer and rank-1 models are rejected. No implementation-ready
  universal operator is known; this is deferred until it blocks a required
  decoder or renderer capability or new admissible evidence appears.
- `ETSI_STRICT` rejects the observed reserved OAMD warp value `raw=3`.
- `DOLBY_VENDOR_COMPAT` is explicit and partial. Observed continuation remains
  opaque; warp-3 and vendor continuation semantics are unresolved.
- Public-syntax coupling, SPX, AHT, and dependent-substream paths have bounded
  synthetic/numerical admission, but some coding-tool combinations still lack
  activation in the qualified real corpus. Full real-world fidelity is not
  claimed.
- Raw E-AC-3 streaming is supported internally. Seekable ISO BMFF uses
  `ffprobe`; non-seekable and fragmented MP4 are not admitted by the 0.2.0
  contract. Capture/demux and compatible-base workflows may also require
  `ffmpeg`.
- Machine-readable scene, component, streaming-summary, retention, and
  internal-base manifests carry explicit `openjoc.*.v1` schema identifiers.
  Decode commands refuse to reuse an existing output directory; callers must
  choose a fresh destination and consume artifact paths relative to it.
- The OpenJOC 0.2.0 release provides prebuilt assets for `aarch64-apple-darwin`,
  `x86_64-pc-windows-msvc`, and `x86_64-unknown-linux-gnu`. Windows was
  validated natively on Windows 11 x86_64. The GNU/Linux binary was built and
  validated under Ubuntu 20.04.6 LTS on WSL2. This does not claim native Linux
  hardware support or validation across all Linux distributions. The
  validation used the repository's available fixtures; private frozen JOC
  media were not present in this checkout.
- The macOS local candidate is not signed with a Developer ID or other user identity
  and is not notarized. Its Mach-O executable has the automatic linker-generated
  ad-hoc signature required by the measured Apple-silicon toolchain. It is not
  an official published release.

See `README.md` in the bundle and `docs/REQUIREMENTS_MATRIX.md` in the
corresponding source archive for the full capability contract.
