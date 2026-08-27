# JOC speaker and binaural rendering

This document owns the current `render-joc` renderer, output-container,
timeline, and output-level contract. Current support status belongs to
[capability matrix](../project/capabilities.md); custom JSON/API geometry belongs to
[custom speaker layouts](custom-speaker-layouts.md).

`render-joc` decodes admitted E-AC-3 JOC input, aligns Base and
ReconstructionBasis PCM, assembles bridge control from decoded JOC/OAMD state,
and renders one semantic speaker layout or a binaural virtualization:

```text
raw EC-3 / seekable ordinary ISO BMFF
  -> bounded E-AC-3 access units
  -> Base + RcLfe + JOC/OAMD decode
  -> Base/RB timeline alignment
  -> automatic bridge-control assembly
  -> persistent JocSpatialBridge
  -> speaker FinalLinkedGain or SOFA binaural convolution
  -> transactional WAV/CAF output
```

The bridge is an admitted ordinary-domain projection with experimental
maturity. It does not resolve authored-object identity or the codec-domain
operator `T(t)`. `ReconstructionBasis` rows remain decoder coordinates, not
verified object stems.

## Ordinary speaker workflow

Use a preset for normal rendering:

```text
openjoc render-joc input.m4a --layout 7.1.4 -o output.wav
```

The current presets are:

- `2.0`
- `5.1`, `5.1.2`, `5.1.4`
- `7.1`, `7.1.2`, `7.1.4`, `7.1.6`
- `9.1`, `9.1.2`, `9.1.4`, `9.1.6`
- `22.2`

All presets use the same generic full-XYZ, multi-layer projector. A preset
defines semantic speaker identities, geometry, LFE ownership, and ordered PCM
channels; it does not select a separate renderer implementation.

`2.0` means physical `FL, FR` speakers and is not binaural. Base channels use
the selected E-AC-3 stereo downmix policy while reconstructed contributions
project to the physical stereo layout:

```text
openjoc render-joc input.m4a --layout 2.0 --downmix auto -o stereo.wav
openjoc render-joc input.m4a --layout 2.0 --downmix loro -o stereo-loro.wav
openjoc render-joc input.m4a --layout 2.0 --downmix ltrt -o stereo-ltrt.wav
```

The ordinary path derives complete bridge control from decoded metadata and
component state. `--topology bridge-control.json` is an advanced complete
override/test input. Automatic and explicit control are never implicitly
merged.

## Custom speaker geometry

Advanced users may replace the preset with versioned JSON:

```sh
openjoc render-joc input.m4a \
  --layout-file studio-layout.json \
  -o studio.caf
```

`--layout` and `--layout-file` are mutually exclusive. Custom layouts use the
same renderer and support up to 64 ordered output channels. The JSON
`speakers` array is the semantic label order and interleaved PCM order. LFE
entries remain logical outputs outside the spatial projector.

Coordinate ranges, validation, projection coverage, Rust construction, C ABI
1.4 descriptors, and examples are canonical in
[custom speaker layouts](custom-speaker-layouts.md). Renderer support does
not imply that a player, framework, audio device, or container accepts the
same arbitrary geometry.

## WAV and CAF truthfulness

The destination extension selects the container:

| Layout/output | Contract |
|---|---|
| Exact standard preset to `.wav` | WAVEFORMATEXTENSIBLE with truthful identities and mask |
| `7.1.6` or `9.1` family | semantic CAF only; WAV fails closed |
| `22.2` to `.wav` | explicit unmasked 24-channel PCM in canonical order |
| Custom layout to `.wav` | explicit unmasked PCM in declared order |
| Preset/custom layout to `.caf` | semantic labels; custom geometry uses coordinate channel descriptions |

No channel identity is substituted and no fabricated WAV mask is written.
Container order never changes the renderer's canonical semantic order.

The 22.2 preset is ITU-R BS.2051 Sound System H with 22 spatial speakers and
two semantic LFE destinations. LFE channels are never projection vertices.

## Binaural and SOFA

`--binaural` renders a virtual speaker field to two output channels. With no
SOFA path, OpenJOC uses the bundled offline SADIE II D1 generic HRTF. The
default virtual layout is 7.1.4:

```text
openjoc render-joc input.m4a --binaural -o headphones.wav
openjoc render-joc input.m4a \
  --binaural --virtual-layout 9.1.6 \
  -o headphones-916.wav
```

Use `--sofa` for a user-provided dataset:

```text
openjoc render-joc input.m4a \
  --binaural --sofa listener.sofa --virtual-layout 7.1.4 \
  --lfe-policy exclude -o custom-headphones.wav
```

The loader accepts the documented local `SimpleFreeFieldHRIR` NetCDF classic
CDF-1 subset. Every non-LFE virtual direction must be exact or safely
interpolatable, the SOFA and input sample rates must match, and unsupported
coverage fails closed. No resampling, download, HDF5/NetCDF-4 fallback, or
omitted-channel substitution occurs.

The LFE policy is explicit: `exclude` or `equal-power-dual-mono`. The CLI
defaults to `exclude`. Binaural output is always two-channel speaker
virtualization; it is not a direct-object or proprietary renderer-fidelity
claim.

`--backend direct|partitioned` selects the admitted convolution backend.
Direct FIR is the numerical reference. Partitioned convolution uses the
requested fixed power-of-two partition and preserves complete final partial
input and FIR-tail behavior.

## DRC, dialnorm, and file level

The recommended signal order is:

```text
encoded DRC policy -> programme dialnorm -> JOC rendering
  -> speaker FinalLinkedGain (speaker only)
  -> optional static sample-peak normalization -> file
```

`--drc disabled|line|rf|custom` controls encoded E-AC-3 dynamic-range
metadata. Custom mode accepts `--drc-boost` and `--drc-cut` percentages from
0 through 100. DRC is not a generic compressor or output normalizer.

`--dialnorm default` is the recommended calibrated behavior. `digital`
explicitly selects encoded digital programme calibration. `analog` applies
unity dialnorm for advanced compatibility/diagnostics; it is not a
higher-quality, raw, lossless, or mastering mode and may drive
FinalLinkedGain more heavily on hot material.

`--normalize-peak TARGET_DBFS` is optional and disabled by default. The CLI
performs one canonical render while spooling bounded renderer-native PCM,
measures the sample peak, then applies one common static scalar after renderer
processing. It supports boost or attenuation. It is not DRC, dialnorm, a
limiter, compressor, LUFS normalization, or true-peak normalization; an
inter-sample peak may exceed the target.

`--diagnostic-contribution base-only|reconstruction-only` isolates one
contribution for engineering evidence. It is diagnostic-only and must not be
described as an authored bed or object stem.

## Timeline, latency, drain, and reset

Logical PTS remains in the decoded 48 kHz sample domain and is not shifted to
hide renderer availability delay:

- speaker output reports 609 samples: 577 samples of QMF/Base-RB alignment
  plus the admitted 32-sample causal FinalLinkedGain block;
- binaural output reports 577 samples because it does not use speaker
  FinalLinkedGain; the finite SOFA FIR tail is drained separately;
- dialnorm and static file normalization add zero audio-sample latency.

Drain emits all QMF/reconstruction, FinalLinkedGain, and binaural FIR tails.
Flush/reset/discontinuity clears access-unit, decoder, timeline, gain, and
HRTF state before the next segment. Integrations remain responsible for
container seek, preroll choice, and discard-output policy.

## Progress, reports, and output safety

Interactive progress is written to stderr and is disabled automatically for
non-TTY output. Use `--no-progress` to disable it or
`--performance-report report.json` to record versioned stage timing and
realtime diagnostics.

Output is transactional. Existing destinations require interactive
confirmation or `--overwrite`; input/output aliasing is rejected. A failed
decode, render, validation, range check, or write does not publish a
successful-looking canonical output.

## Integration boundary

The Rust API and supported framework adapters share the same session renderer,
but each host owns its transport and layout negotiation. In particular, the
validated Windows DirectShow/LAV/PotPlayer integration outputs 48 kHz stereo
float PCM only. The 64-channel renderer limit and standalone preset matrix are
not DirectShow/LAV output claims.

See [known limitations](../compatibility/known-limitations.md) for current non-claims and
[architecture](../concepts/architecture.md) for component ownership.
