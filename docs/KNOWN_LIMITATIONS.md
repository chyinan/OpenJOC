# OpenJOC v0.2.0-rc.1 known limitations

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
  `ffprobe`; non-seekable and fragmented MP4 are not admitted by the 0.2.0-rc.1
  contract. Capture/demux and compatible-base workflows may also require
  `ffmpeg`.
- Machine-readable scene, component, streaming-summary, retention, and
  internal-base manifests carry explicit `openjoc.*.v1` schema identifiers.
  Decode commands refuse to reuse an existing output directory; callers must
  choose a fresh destination and consume artifact paths relative to it.
- This local binary release-candidate workflow is admitted only for
  `aarch64-apple-darwin`. Linux, Windows, and Intel macOS release readiness are
  not claimed.
- The local candidate is not signed with a Developer ID or other user identity
  and is not notarized. Its Mach-O executable has the automatic linker-generated
  ad-hoc signature required by the measured Apple-silicon toolchain. It is not
  an official published release.

See `README.md` in the bundle and `docs/REQUIREMENTS_MATRIX.md` in the
corresponding source archive for the full capability contract.
