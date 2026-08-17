# OpenJOC roadmap

This is the canonical future-work list. It is intentionally not a copy of the
current capability matrix or a historical milestone log.

## Near-term engineering priorities

- Consider a separately bounded automatic topology-generator module only if a
  future maintainer explicitly wants it; `LayoutRenderer3d` intentionally
  keeps caller-declared speaker triplets and performs no triangulation. Do not
  infer 3D rendering from the unresolved JOC semantic bridge.
- Keep the admitted 3D trajectory contract stable: shortest great-circle
  segments, explicit intermediate keyframes, and absolute-sample partition
  invariance. Optimization requires measured byte-identical regressions.
- Keep J5R6 direct-FIR binaural rendering as the compact numerical oracle:
  exact caller-supplied HRIR lookup, static source registration, bounded
  history, and complete tail draining remain the reference contract. Any
  future optimization must preserve byte-identical direct-path regressions.
- Uniform partitioned binaural convolution is now an explicit opt-in backend
  alongside the J5R6 Direct FIR oracle. It uses one fixed power-of-two input
  partition and a `2P` FFT, exposes one-partition scheduling latency, and
  preserves exact final partial-input and `M-1` tail semantics. It must not
  silently add SOFA parsing, interpolation, moving sources, or listener pose
  semantics; backend selection remains caller-owned.
- Extend the admitted user-supplied HRTF path only through separately bounded
  contracts; J5R8 currently provides strict local `SimpleFreeFieldHRIR` SOFA
  loading into `HrirBank`, while interpolation and direction resolution remain
  future milestones.
- Extend public-syntax and malformed-input hardening, including fuzz coverage.
- Keep container and output streaming contracts explicit as new input forms are
  considered.
- Improve cross-platform CI before making Linux, Windows, or Intel-macOS
  release claims.
- Add release automation only when signing, notarization, and publication policy
  are explicitly decided by a human maintainer.

## Research priorities

- Resolve the single JOC spatial bridge blocker: the independently testable
  time-varying operator `T(t)` and its complete state inputs. The codec-domain
  bridge is now explicit. J5R11 narrowed the next question to the temporal
  class of one companion-induced forward column. J5R12 then rejected both
  authorized AU-synchronous and 256-sample codec-block-synchronous
  source-locked models; do not bypass this blocker with a finer ad-hoc model
  or a fixed RB-row/object assumption. J5R13 additionally rejected the only
  authorized global 256-phase and 1536-phase cyclostationary templates, so
  the single-tone corpus is exhausted for this temporal-class question.
- Obtain admissible evidence for unresolved vendor OAMD continuation without
  weakening `ETSI_STRICT`.
- Separate metadata understanding from any future authored-object binding
  claim; require independent identity, timing, negative-control and repeatability
  evidence.
- Evaluate real-producer coding-tool combinations where public-syntax admission
  currently exceeds controlled real-corpus activation.

## Deferred from 0.5.0: candidates for 0.6.0

- Spread/Pair and Fixed/Named dynamic routing.
- Selector-6 behavior, rare Region fallback/tie handling, and broader Region
  algebra.
- More than two-layer semantics, 22.2, broader exact-HRIR/binaural policies,
  and additional player/integration validation.

These items remain backlog only; they are not part of the 0.5.0 release
contract.

## Historical non-goals for the OpenJOC 0.3.0 release

- No implicit authored-object PCM claim.
- No audio-bound ObjectScene or proprietary renderer-fidelity claim. The
  post-release development line may expose only the separately scoped
  explicit-scene renderer contract.
- No raw warp-3 alias or guessed Dolby semantic rule.
- No public release action without an explicit human decision.
