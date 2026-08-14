# OpenJOC 0.3.0-dev capabilities

This is the canonical current capability status for the OpenJOC 0.3.0-dev
development line. The immutable v0.2.0 release remains separately documented
by its acceptance record. This is a release-facing snapshot, not a research
journal. Engineering requirement details belong in
[REQUIREMENTS_MATRIX.md](REQUIREMENTS_MATRIX.md); historical evidence belongs
in [research history](research/README.md).

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
| Rendering | Explicit-scene stereo and general 2D speaker renderer | `ADMITTED_WITH_SCOPE` | `openjoc-render` independent stereo/VBAP oracle, layout, trajectory, continuity and block-partition tests | Caller-supplied mono sources only; arbitrary validated horizontal layouts, adjacent-pair panning, and absolute-sample position/gain trajectories; no JOC bridge, HRTF, room model, or Dolby renderer-fidelity claim |
| Rendering | Explicit-scene 3D speaker topology, VBAP triplet renderer, and sample-accurate trajectories | `ADMITTED_WITH_SCOPE` | `openjoc-render` checked 3×3 public-math and independent great-circle oracle, tetrahedron/octahedron/partial/ambiguity, continuity, and partition tests | Caller supplies speaker order and triplets explicitly; shortest great-circle segments and linear gain only; no automatic triangulation, Delaunay/hull inference, distance, Doppler, listener orientation, LFE, HRTF, JOC bridge, or authored-object identity |
| Rendering | Static explicit-source binaural direct-FIR renderer | `ADMITTED_WITH_SCOPE` | `openjoc-render` exact-direction HRIR/provider validation, independent full-convolution oracle, ear-order, history, tail, reset, failure-atomicity, and input/tail partition tests | Caller supplies finite equal-length HRIR taps and exact static directions; fixed listener orientation, direct causal f64 FIR reference path; no SOFA, interpolation, moving source, room, distance, HRTF database, or JOC bridge |
| Rendering | Static explicit-source uniform partitioned binaural convolution | `ADMITTED_WITH_SCOPE` | `openjoc-render` fixed-FFT backend, Direct FIR equivalence, multiple partition sizes/sources, partial-input, exact-tail and lifecycle regressions | Caller selects one fixed power-of-two `P`; FFT size is `2P`, input is exact `P`-sample partitions plus one final partial, scheduling latency is explicitly `P` samples; no adaptive selection, nonuniform partitions, SOFA, interpolation, moving sources, or JOC bridge |
| Rendering | Strict `SimpleFreeFieldHRIR` SOFA ingestion into `HrirBank` | `ADMITTED_WITH_SCOPE` | `openjoc-sofa` synthetic CDF-1 fixture, coordinate/ear/delay/malformed-file tests, direct and partitioned construction integration | Local read-only NetCDF classic CDF-1 subset; SOFA convention versions 1.0–1.2, exactly two receivers, spherical degree/degree/metre sources, integer sample delays; no HDF5/NetCDF-4, interpolation, nearest lookup, downloads, writing, or JOC bridge |
| Rendering | Audio-bound `ObjectScene` or renderer fidelity | `NOT_ADMITTED` | Semantic binding remains unresolved and no proprietary renderer evidence is admitted | No authored-object/JOC audio binding, binaural parity, or Dolby renderer-fidelity claim |
| Release | OpenJOC 0.2.0 platform binaries | `ADMITTED_WITH_SCOPE` | Published Apple silicon macOS, Windows x86_64, and GNU/Linux x86_64 assets; native Windows validation and Ubuntu WSL2 validation | Linux evidence is limited to Ubuntu 20.04.6 LTS under WSL2; the macOS asset is not Developer-ID signed or notarized |

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
openjoc --version
```

The CLI emits structured failures, never silently downgrades the selected
validation profile, and names diagnostic outputs as reconstruction rows rather
than authored-object stems. Decode output directories are create-once
destinations, and stable machine-readable manifests identify their schema with
`openjoc.*.v1` markers.
