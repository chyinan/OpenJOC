# OpenJOC 0.5.0

OpenJOC is an independent, clean-room E-AC-3 JOC metadata and
reconstruction-basis decoder with an experimental spatial-rendering path.
OpenJOC is not affiliated with, endorsed by, or certified by Dolby
Laboratories, and this release makes no bit-identical Reference Player claim.

## Why upgrade from 0.4.2?

0.5.0 includes major reconstruction and spatial-rendering fidelity corrections.
Users of 0.4.x are strongly encouraged to upgrade. The release corrects QMF
synthesis behavior and the Base/ReconstructionBasis timeline contract, while
retaining the transactional and fail-closed output behavior from 0.4.2.

## Highlights

- Generic full-XYZ speaker projection over data-driven layer, row, and anchor
  topology.
- Public speaker presets through `9.1.6`:
  `5.1`, `5.1.2`, `5.1.4`, `7.1`, `7.1.2`, `7.1.4`, `7.1.6`, `9.1`,
  `9.1.2`, `9.1.4`, and `9.1.6`.
- Semantic CAF multichannel output for richer layouts. WAV output remains
  available only where WAVEFORMATEXTENSIBLE can represent every semantic
  speaker identity exactly.
- Ordinary Dynamic Region/Zone, Dynamic Extent, and Dynamic ChannelLock
  rendering, including admitted Region × Extent and unified Region-first /
  ChannelLock-precedence composition.
- Preserved QMF round-trip latency of 577 samples and zero Base/RB lag at the
  renderer-input boundary.

## Output guidance

```sh
openjoc render-joc INPUT.m4a --layout 7.1.4 --output render-7.1.4.wav
openjoc render-joc INPUT.m4a --layout 7.1.6 --output render-7.1.6.caf
openjoc render-joc INPUT.m4a --layout 9.1.6 --output render-9.1.6.caf
```

Use CAF for `7.1.6` and the `9.1` family. Their richer speaker identities do
not have exact standard WAVEFORMATEXTENSIBLE mask bits, so `.wav` requests fail
closed rather than silently changing channel meaning. See
[`docs/JOC_RENDER.md`](docs/JOC_RENDER.md) for the complete channel matrix and
container contract.

The binaural path is exact-direction SOFA speaker virtualization for
`5.1`, `5.1.2`, `5.1.4`, `7.1`, `7.1.2`, and `7.1.4`. It requires a caller-
supplied supported SOFA file and an explicit LFE policy; it does not use
nearest-speaker lookup or interpolation.

## Dynamic-object behavior

Region selects the effective topology first. If ChannelLock is active, it owns
current target generation and bypasses the Extent target path while preserving
Extent state. Otherwise Extent participates when active; otherwise the ordinary
Point target is used. When ChannelLock is released, inherited Extent behavior
resumes. `effective_position` remains local to the ChannelLock evaluation.

## Limitations

Authored-object binding and the codec-domain JOC operator `T(t)` remain
unresolved. Selector-6 special behavior, Spread/Pair, Fixed/Named routing, rare
Region fallback/tie cases, >2-layer semantics, 22.2, and broader binaural
policies remain withheld. Real-media listening and long-render acceptance are
manual release steps; automated regressions do not establish subjective or
realtime qualification.

## Running from source

```sh
cargo build -p openjoc-cli --release --locked
./target/release/openjoc --version
./target/release/openjoc --help
```

The source tree is the supported installation path. Prebuilt binaries are
distributed only by the human-created GitHub Release workflow after the stable
tag is authorized.
