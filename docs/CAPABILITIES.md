# OpenJOC v0.1.0 capabilities

This is the canonical current capability status for OpenJOC v0.1.0. It is a
release-facing snapshot, not a research journal. Engineering requirement
details belong in [REQUIREMENTS_MATRIX.md](REQUIREMENTS_MATRIX.md); historical
evidence belongs in [research history](research/README.md).

## Status vocabulary

- `ADMITTED` — supported within the stated contract.
- `ADMITTED_WITH_SCOPE` — supported only within an explicit bounded scope.
- `DIAGNOSTIC_ONLY` — emitted for analysis, not a semantic product claim.
- `PARTIAL` — an explicit subset is supported; the full capability is not.
- `UNRESOLVED` — evidence is insufficient for an implementation claim.
- `NOT_ADMITTED` — deliberately outside the current contract.
- `EXPECTED_STRICT_REJECTION` — rejection is the correct result for the input.

## Current matrix

| Area | Capability | Status | Evidence boundary | Important scope |
|---|---|---|---|---|
| Input | Raw E-AC-3 parsing and bounded streaming | `ADMITTED` | Controlled carriers and public syntax | Full real-stream codec fidelity remains scoped |
| Input | Seekable ordinary MP4/M4A with one E-AC-3 track | `ADMITTED_WITH_SCOPE` | Container and sample-cursor regressions | Uses `ffprobe`/`ffmpeg`; non-seekable and fragmented MP4 are not admitted |
| Base E-AC-3 | Ordinary base decode and channel/LFE labels | `ADMITTED_WITH_SCOPE` | Public syntax, topology, TDAC and state tests | Not a speaker renderer; cross-decoder fidelity remains incomplete |
| Coding tools | Coupling, SPX, AHT, rematrix | `ADMITTED_WITH_SCOPE` | Normative/public-syntax numerical and state harnesses | Some real-producer activation and full PCM fidelity remain open |
| Substreams | One I0 plus optional D0 assembly | `ADMITTED_WITH_SCOPE` | Chanmap, atomic assembly and reset tests | Multiple dependents are not admitted |
| OAMD | Normative metadata prefix and metadata-only timeline | `ADMITTED_WITH_SCOPE` | Normative parser and controlled state tests | Complete vendor trim continuation is unavailable |
| OAMD | `ETSI_STRICT` profile | `ADMITTED` | Published ETSI validation rules | Observed raw `warp=3` is `ReservedWarpMode` and is rejected |
| OAMD | `DOLBY_VENDOR_COMPAT` profile | `PARTIAL` | Explicit observed-signaling acceptance and deviation evidence | Continuation is retained opaquely; no vendor semantic interpretation |
| Scene | Metadata-only `ObjectScene` | `ADMITTED` | Schema, timeline, assembly and atomicity tests | No audio binding is implied |
| Reconstruction | `ReconstructionBasis` rows | `DIAGNOSTIC_ONLY` | Deterministic numerical, continuity and WAV-export tests | Rows are not authored-object PCM or object stems |
| Components | Typed decoded-component manifest | `ADMITTED` | `diagnostics/components.json` separates Base, Base LFE, indexed RB coordinates, RcLfe boundary and unresolved binding | PCM-free layout; no authored-object identity |
| Semantics | Authored-object binding and verified object PCM | `NOT_ADMITTED` | One-row-per-authored-object model rejected | `SemanticBindingState::Unresolved` remains the production state |
| Rendering | Audio-bound `ObjectScene` or renderer fidelity | `NOT_ADMITTED` | Insufficient semantic evidence | No speaker/binaural renderer or fidelity claim |
| Release | Local macOS-arm64 0.1.0 candidate | `ADMITTED_WITH_SCOPE` | Double assembly, offline verification, package/install | Local engineering candidate only; not published, Developer-ID signed, or notarized |

The matrix deliberately separates production status from evidence class. A
numerically valid reconstruction row is not an authored object, and a real
carrier accepted by the vendor profile is not proof that ETSI strict semantics
are wrong.

## User-visible entry points

```text
openjoc inspect FILE
openjoc decode FILE -o DIR [--internal-base] [--streaming]
openjoc decode-payload --downmix FILE --joc FILE --oamd FILE -o DIR
openjoc diagnose-tools FILE --vector-id ID --json OUTPUT
openjoc census [MANIFEST] -o DIR
openjoc diagnose-oamd FILE [OPTIONS]
```

The CLI emits structured failures, never silently downgrades the selected
validation profile, and names diagnostic outputs as reconstruction rows rather
than authored-object stems.
