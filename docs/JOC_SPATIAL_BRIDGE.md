# JOC spatial reconstruction bridge

## JOC Spatial Bridge

The stable functional API is `openjoc_scene::JocSpatialBridge`. Its current
implementation maturity is **experimental**; its semantic binding state is
**unresolved**; and its official runtime validation oracle is **not
independently confirmed**. The bridge is opt-in and is not selected by
`ETSI_STRICT`, `OBSERVED_VENDOR_COMPAT`, or `AUTO`: those remain validation and
profile-selection policies upstream of this spatial layer. A caller must
construct the bridge, provide a `SpatialTopologySnapshot` (or reuse the current
binding), provide a validated `SpatialLayout`, and call
`render_coordinates` or `render_codec_basis_frame` into caller-owned output
planes. It is therefore opt-in and cannot silently alter the existing decode
path.

The bridge implements the supported ordinary release domain:

- deterministic explicit-group/fixed-layout/dynamic-record flattening with
  persistent `(topology_epoch, ordinal)` binding state and selective
  inheritance;
- public active non-LFE layout channels, full normalized `(x, y, signed-z)` point
  coordinates, data-driven layer/row/anchor topology, row-local equal-power X
  interpolation, and plane-local equal-power Y interpolation. Spread/Pair and
  Fixed/Named routing are not part of the admitted 0.5.0 dynamic-object
  contract; withheld or malformed variants fail closed;
- ordinary dynamic Region/Zone states are resolved into a constrained
  layer/row/anchor topology before point projection. The default/no-region
  state retains the complete canonical topology; points outside selected
  support use the selected topology's normal endpoint clamp rather than being
  muted, and region target changes use the existing Q32 scheduler;
- ordinary metadata-driven Dynamic Extent is honored for the eleven admitted
  5.1/7.1/9.1-family layouts. XYZ size metadata reduces to one isotropic Q15
  scalar, uses the clean five-knot radius transfer and cached compact field,
  preserves point identity at zero, and submits changed targets through the
  existing Q32 scheduler;
- Dynamic point ChannelLock is evaluated after ordinary point projection and
  takes precedence over ordinary Extent target generation. Region selects the
  effective topology before point projection and ChannelLock candidate
  selection; the current maximum active non-LFE output is mapped to its
  topology anchor; a strict full-XYZ squared-distance test below `0.04`
  produces an exclusive one-hot target and a local effective-position snap.
  Extent semantic state is retained while its current target branch is
  bypassed, and ordinary, locked, and switched targets all use the existing
  Q32 scheduler. `effective_position` is a `LOCAL_ONLY` outcome of the
  ChannelLock evaluation; it is not propagated into Region, Extent, or Q32
  state;
- Q32 gain scheduling with persistent phase across blocks, restart on binding
  rebuild/layout change, and linear `Y = Σ G X` accumulation;
- finite-value, dimension, duplicate, unsupported-class, and malformed-input
  rejection.

The descriptor's raw warp-3 field is preserved as opaque data and is never
used as a projection input. Its public semantic meaning remains unresolved.
The bridge does not make `SemanticBindingState` production-resolved, does not
claim an official spatial oracle, and does not admit a vendor-fidelity result.
The 0.5.0 `render-joc` command composes this function with automatic
decoded JOC/OAMD bridge-control assembly for experimental speaker output. A
complete topology sidecar remains an optional explicit override/test input.
unsupported/default branches, unadmitted preprocessing, and malformed-recovery
semantics are outside this implementation. The admitted Region/Zone subset is
limited to the six named horizontal states and ordinary Top-Bottom inclusion or
exclusion on validated one- or two-plane layouts. On those admitted topologies,
non-default Region and ordinary nonzero Extent compose through the selected
effective topology before the existing Extent crossover, normalization, and
Q32 scheduler when ChannelLock is inactive. With ChannelLock active, the
target reduces to the Region-first ChannelLock branch; retained Extent state
resumes through the existing Extent path when ChannelLock is released. Non-
point ChannelLock sources, special selector-6 behavior, arbitrary region
algebra, and unadmitted layer/fallback combinations remain fail-closed.

The admitted target-generation precedence is therefore:

```text
Region -> effective topology
  ChannelLock active -> exclusive ChannelLock target; bypass Extent target path
  otherwise Extent active -> Extent target
  otherwise -> ordinary Point target
```

Ordinary dynamic point projection is one generic full-XYZ operator. Layout
names select channel identities and topology data; they do not select separate
projection mathematics. The validated topology can store multiple layers,
rows, and unequal X anchor sets, while active layer semantics remain limited to
the admitted bed/top policy. The current public presets use this engine, and
internal clean fixtures cover only the explicitly supported topology family.
The data model does not imply arbitrary-layout product policy or 22.2
projection fidelity; LFE remains outside geometric point projection.

The machine-readable implementation input is not part of the public runtime
contract; the repository exposes only the resulting spatial API, tests, and
this boundary description.

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
the `render-joc` composition also does not convert decoded JOC rows into
authored objects.

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
  release. `JocSpatialFrameBridge::frame` borrows the current frame and allocates
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

## J5R12 codec-grid temporal discriminator

J5R12 kept the same frozen B0/B1/C0/C1 carriers, exact companion PCM, 1536
sample alignment, and W1/W2/W3 windows. It tested only the two predeclared
codec grids: complete 1536-sample access units, followed (because the AU
model failed) by complete 256-sample codec audio blocks. Each segment used
the bounded source-locked pair `s[n]` and `s[n-1]`, with even absolute
samples for fitting and odd samples held out.

The AU model failed in all three windows. The block model also failed: its
normalized full/holdout RB residuals were approximately 0.000455/0.000454 in
W1, 0.003965/0.003965 in W2, and 0.000512/0.000511 in W3, while the frozen
absolute holdout guard is `1e-3`. The B/C repeats were byte-identical and the
Base 997 Hz null remained inside its inherited envelope, so this is not an
alignment or corpus-integrity failure.

The resulting classification is
`J5R12_CODEC_BLOCK_MODEL_INSUFFICIENT_TEMPORAL_STATE_UNRESOLVED`.
The existing corpus therefore does not identify an AU- or 256-sample
block-synchronous source-locked transfer. No finer segmentation, longer FIR,
production empirical coefficient table, RB-row/object binding, or warp-3
interpretation was attempted.

## J5R13 fixed codec-phase template discriminator

J5R13 tested the remaining two globally anchored periodic hypotheses on the
same B0/B1/C0/C1 corpus: a single 256-phase `alpha/beta` template, followed
only after that failure by a single 1536-phase (six-block AU) template. Phase
origin was the absolute decoder sample timeline; no window reset, phase shift,
period search, or smaller-period fallback was used.

The 256-phase template failed its global and per-window holdout gates (global
holdout approximately 0.00321; W1 0.000106, W2 0.005589, W3 0.000281), and
did not meet the four-times improvement or full-coordinate checks against the
J5R12 block baseline. The conditional 1536-phase template also failed (global
holdout approximately 0.001096; W1 0.000999, W2 0.001200, W3 0.001064), with
the full RB coordinate checks failing as well. Repeated templates were
numerically identical, while Base null and corpus integrity remained valid.

The resulting classification is
`J5R13_FIXED_CODEC_PHASE_TEMPLATE_INSUFFICIENT_EXISTING_CORPUS_EXHAUSTED`.
Within this single-tone controlled corpus, fixed 256- or 1536-sample
cyclostationarity is insufficient to explain the remaining source-to-codec
basis temporal behavior. The stop-loss is intentional: no P128/P64/P32/P16,
per-sample template, longer FIR, or empirical production correction was added.
The next choice is a separately authorized new discriminator, a specific
normative investigation, or freezing this blocker; it is not an implicit
RB-row/object or renderer semantic result.

## Boundaries retained

- `SemanticBindingState` remains `Unresolved`.
- `RcLfe` remains separate from dynamic RB rows.
- Dynamic-object count matching RB row count is dimensional compatibility only.
- `ETSI_STRICT` still treats observed OAMD `warp=3` as
  `ReservedWarpMode { raw: 3 }`.
- No proprietary decoder, renderer, or vendor semantic source is used.
- No automatic authored-object mapping or direct-object binaural rendering is
  admitted by this bridge. The explicit `render-joc` composition is
  experimental and covers the documented eleven speaker presets, with
  binaural output limited to the six exact-HRIR layouts.
