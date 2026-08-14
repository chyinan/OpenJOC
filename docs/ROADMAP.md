# OpenJOC roadmap

This is the canonical future-work list. It is intentionally not a copy of the
current capability matrix or a historical milestone log.

## Near-term engineering priorities

- Consider a separately bounded automatic topology-generator module only if a
  future maintainer explicitly wants it; `LayoutRenderer3d` intentionally
  keeps caller-declared speaker triplets and performs no triangulation. Do not
  infer 3D rendering from the unresolved JOC semantic bridge.
- Add a pluggable public/user-supplied HRTF path only after the speaker-scene
  contract is stable.
- Extend public-syntax and malformed-input hardening, including fuzz coverage.
- Keep container and output streaming contracts explicit as new input forms are
  considered.
- Improve cross-platform CI before making Linux, Windows, or Intel-macOS
  release claims.
- Add release automation only when signing, notarization, and publication policy
  are explicitly decided by a human maintainer.

## Research priorities

- Obtain admissible evidence for unresolved vendor OAMD continuation without
  weakening `ETSI_STRICT`.
- Separate metadata understanding from any future authored-object binding
  claim; require independent identity, timing, negative-control and repeatability
  evidence.
- Evaluate real-producer coding-tool combinations where public-syntax admission
  currently exceeds controlled real-corpus activation.

## Explicit non-goals for the current v0.2.0 line

- No implicit authored-object PCM claim.
- No audio-bound ObjectScene or proprietary renderer-fidelity claim. The
  post-release development line may expose only the separately scoped
  explicit-scene renderer contract.
- No raw warp-3 alias or guessed Dolby semantic rule.
- No public release action without an explicit human decision.
