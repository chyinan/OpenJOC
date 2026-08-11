# OpenJOC 0.x known limitations

This snapshot is part of the local release-candidate contract. It states the
same user-visible boundary as the canonical repository documentation without
claiming capabilities that the current evidence does not support.

- `scene.json` is metadata-only. It does not bind decoded audio to authored
  objects.
- Diagnostic `ReconstructionBasis` rows are available, but they are not
  verified authored-object PCM. `SemanticBindingState` remains `Unresolved`.
- Authored-object PCM and an audio-bound `ObjectScene` are unavailable.
- `ETSI_STRICT` rejects the observed reserved OAMD warp value `raw=3`.
- `DOLBY_VENDOR_COMPAT` is explicit and partial. Observed continuation remains
  opaque; warp-3 and vendor continuation semantics are unresolved.
- Public-syntax coupling, SPX, AHT, and dependent-substream paths have bounded
  synthetic/numerical admission, but some coding-tool combinations still lack
  activation in the qualified real corpus. Full real-world fidelity is not
  claimed.
- Raw E-AC-3 streaming is supported internally. Seekable ISO BMFF uses
  `ffprobe`; non-seekable and fragmented MP4 are not admitted by the 0.x
  contract. Capture/demux and compatible-base workflows may also require
  `ffmpeg`.
- This local binary release-candidate workflow is admitted only for
  `aarch64-apple-darwin`. Linux, Windows, and Intel macOS release readiness are
  not claimed.
- The local candidate is not signed with a Developer ID or other user identity
  and is not notarized. Its Mach-O executable has the automatic linker-generated
  ad-hoc signature required by the measured Apple-silicon toolchain. It is not
  an official published release.

See `README.md` in the bundle and `REQUIREMENTS_MATRIX.md` in the corresponding
source archive for the full capability contract.
