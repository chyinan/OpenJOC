# JOC spatial reconstruction bridge

OpenJOC's primary product path is E-AC-3 JOC input, not WAV scene input:

```text
E-AC-3 JOC
    ↓
decoder Base + ReconstructionBasis + OAMD
    ↓
JOC spatial reconstruction operator T(t)   ← unresolved
    ↓
ExplicitSpatialScene / matrix-domain renderer composition
    ↓
speaker or binaural PCM
```

The `render-scene` command and the `openjoc-render` crate remain useful
caller-bound WAV/reference workflows. They are independent renderer oracles;
they do not claim to convert decoded JOC rows into authored objects.

## Codec-domain bridge

`openjoc-scene` now exposes the borrowed
`openjoc.joc-spatial-reconstruction.v1` boundary:

- `CodecBasisBlock` carries explicitly labelled Base full-band PCM, indexed
  `ReconstructionBasis` rows, and separate `RcLfe` PCM.
- `JocSpatialMetadataFrame` carries the current OAMD payload and structural
  `ProgrammeLayout` without treating its entries as audio bindings.
- `SampleRange` gives every committed payload frame an absolute half-open
  sample interval. It is not reconstructed later from vector lengths.
- `JocSpatialOperatorState::Unresolved` is the only production state in this
  release. `JocSpatialBridge::frame` borrows the current frame and allocates
  no duration-sized copy.

The bridge validates finite values, coordinate cardinality, row/channel
lengths, `RcLfe` separation, and absolute timing before a consumer can inspect
the frame. `require_resolved_operator()` is an explicit hard gate and fails
while the operator is unresolved. There is no automatic conversion to
`ExplicitSpatialScene`, no `ReconstructionBasis row == authored object`
assumption, and no default matrix or permutation.

## Readiness census

The machine-readable census is
[`joc_reconstruction_readiness.json`](joc_reconstruction_readiness.json).
It records the implemented decoder inputs, the typed unresolved reasons, the
existing-fixture observations, and the one remaining operator fact. The
current classification is
`J5R10_JOC_MATRIX_BRIDGE_FOUNDATION_ADMITTED_OPERATOR_UNRESOLVED`.

The missing fact is not “which row is object 1.” It is the complete,
independently testable time-varying operator `T(t)` and its state inputs. J4R8
and J4R9 already rule out a common fixed row/object transfer model. A future
milestone must target that single blocker using authorized clean-room
evidence; this stage does not add a vendor warp rule or new producer media.

## J5R11 local-column discriminator

The first source-locked test on the frozen J4R8 B/C corpus is deliberately
negative and bounded. With the exact authored 2003 Hz companion PCM and the
fixed 1536-sample source/decoder alignment, the B/C `ReconstructionBasis`
delta remains strongly low-rank in each of W1/W2/W3, while the Base 997 Hz
null remains within its inherited envelope. The source-locked instantaneous
model `ΔRB_w[n] = a_w s_companion[n]` does not meet its predeclared fit and
holdout gates. The only authorized two-tap fallback also fails those gates.

The resulting classification is
`J5R11_EXISTING_CORPUS_INSUFFICIENT_TO_IDENTIFY_OPERATOR_TEMPORAL_CLASS`.
This does not mean that the companion has no codec-basis effect; it means the
existing corpus does not identify whether that effect is an instantaneous
source-locked column or the permitted two-tap temporal class. No coefficient
is an object identifier, no RB row is an authored object, and no inverse
`T(t)` or renderer scene is admitted.

## Boundaries retained

- `SemanticBindingState` remains `Unresolved`.
- `RcLfe` remains separate from dynamic RB rows.
- Dynamic-object count matching RB row count is dimensional compatibility only.
- `ETSI_STRICT` still treats observed OAMD `warp=3` as
  `ReservedWarpMode { raw: 3 }`.
- No proprietary decoder, renderer, or vendor semantic source is used.
- No real-JOC speaker/binaural rendering is admitted by this bridge.
