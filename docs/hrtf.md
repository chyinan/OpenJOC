# Built-in HRTF library

OpenJOC provides two generic, non-individual built-in HRTF profiles and
continues to accept a user-provided Custom SOFA file:

| ID | Display name | Subject | Source directions | Rate / IR | License |
| --- | --- | --- | ---: | --- | --- |
| `sadie-ii-d1-ku100` | SADIE II — KU100 | D1 / Neumann KU100 | 8,802 measured + 15 OpenJOC aliases | 48 kHz / 256 taps | Apache-2.0 |
| `sadie-ii-d2-kemar` | SADIE II — KEMAR | D2 / KEMAR | 8,802 measured + 15 OpenJOC aliases | 48 kHz / 256 taps | Apache-2.0 |

D1 remains the default. Different listeners may prefer different non-individual
HRTFs because HRTF perception depends strongly on individual anatomy. OpenJOC
therefore provides these two built-in profiles and allows importing
personal SOFA datasets. No profile is advertised as best for everyone or as
equivalent to a proprietary renderer.

## Provenance and transformations

The checked-in resources use the version-2 OpenJOC `.ojhrtf` format. Its
56-byte envelope contains magic, format version, stable preset code, payload
length, and payload SHA-256; the registry also pins the complete asset size
and SHA-256. The payload is a direct record table, not a SOFA container: its
40-byte payload prefix contains its own magic, sample rate, direction count,
maximum row tap count, and `u32` codes (currently value `1`)
for listener-local XYZ coordinates, left/right channel order, f64 directions,
f32 taps, and integer sample delays. Each row stores three little-endian f64
unit-vector components, pair tap count, left/right delay samples, then all
left-ear and right-ear taps as little-endian f32. Runtime no longer parses
NetCDF/SOFA metadata for a built-in profile. Custom SOFA still uses the
existing strict CDF-1 loader.

The offline pipeline is [`tools/generate-builtin-hrtf.py`](../tools/generate-builtin-hrtf.py)
followed by `cargo run -p openjoc-sofa --example pack-hrtf-asset --
<preset-id> <prepared-CDF-1.sofa> <output.ojhrtf>`. The packer rejects taps
that are not exactly representable as f32; source Data.IR is f32, so widening
and re-packing the built-in measurements is bit-exact. There is no lossy
compression, spatial downsampling, tap truncation, resampling, EQ, or
subjective tuning.

The Browser Standard package compiles the renderer with HRTF embedding disabled
and includes both D1 and D2 `.ojhrtf` assets with a local, provenance-bearing
manifest. Its Cargo build uses
`--no-default-features --features external-builtin-hrtf-assets`; enabling the
external feature without disabling Cargo defaults would still embed the HRTFs.
The Browser loads a selected asset from the extension package and verifies its
size and SHA-256 before renderer preparation. It does not fetch or persist HRTF
assets, and there is no separate Full package. Both profiles work offline
immediately after installation. Custom SOFA remains the public import format
and is not replaced by `.ojhrtf`.

The parsed f32 bank is not persisted. The native `openjoc-api`, C ABI, CLI,
and LAV-facing API remain offline and do not fetch assets.

On a preset change, the Browser resets the previous convolution state and
prepares a replacement decoder. The current decoder object remains owned until
the replacement has initialized; successful replacement destroys the old
instance. A failed HRTF load restores the prior HRTF selection and restarts its
existing decoder session. Switching is not crossfaded, so a brief playback gap
is possible while loading or initializing D2. No old convolution tail is
carried into the new preset.

### SADIE II D1 / KU100

- Official source: <https://sofacoustics.org/data/database/sadie/D1_48K_24bit_256tap_FIR_SOFA.sofa>
- Dataset record: <https://zenodo.org/records/10886409>
- Dataset: SADIE II, D1 / Neumann KU100, University of York Audio Lab.
- License: Apache License 2.0; retain University of York/SADIE II attribution.
- Upstream SHA-256: `e6c72a84dd947b5ef75438ab96a9c2a32ed10f033472b9c4c11a49aff00a8a31`
- Prepared CDF-1 intermediate SHA-256: `b9bcecd8a07e7eed4474a9b063c47672384339e83605bd245ff0adc098869fab`
- `.ojhrtf` v2: 18,374,724 bytes; SHA-256 `78d048a68f84d34051578c262e401e35baa0e718901f85349afe0232f985d4df`
- OpenJOC modification: the existing HDF5 SOFA was converted to CDF-1 and 15
  exact canonical virtual-speaker aliases were retained. No EQ, resampling,
  phase change, or subjective tuning was added.

### SADIE II D2 / KEMAR

- Official source: <https://sofacoustics.org/data/database/sadie/D2_48K_24bit_256tap_FIR_SOFA.sofa>
- Dataset record and license text: <https://zenodo.org/records/10886409>
- Dataset: SADIE II, D2 / KEMAR, University of York Audio Lab; DOI
  <https://doi.org/10.5281/zenodo.10886409> and associated paper DOI
  <https://doi.org/10.3390/app8112029>.
- License: Apache License 2.0; retain the University of York/SADIE II
  attribution and license text.
- Upstream SHA-256: `bb5980288fc5c990c821e02c5ddfa274759079b723973cd6ca47ac577243aa0c`
- Prepared CDF-1 intermediate SHA-256: `88cdc843aff4a69c90465e36ab573e9b073c0b7675ce909d6900dbbc49a5ed95`
- `.ojhrtf` v2: 18,374,724 bytes; SHA-256 `b2f42ca2ce9ef2dfa7e3eff263543c4f306d0ac95bd684cf5ca344c88d6bd461`
- OpenJOC modification: HDF5 SOFA to CDF-1, plus the same 15 exact
  virtual-speaker aliases as D1. Data.Delay, receiver ordering, ITD, and HRIR
  samples are preserved; no EQ, resampling, phase change, or subjective
  tuning was added.

The prepared resources are intentionally loaded lazily when their preset is
selected. Switching or resetting a session discards the old renderer state;
the renderer does not share convolution state between presets.

## Representation and measured cost

Custom SOFA continues through the CDF-1 `SimpleFreeFieldHRIR` parser and its
existing f64 representation. Built-in `.ojhrtf` v2 files instead decode a
direct direction/tap record table and do not parse SOFA/NetCDF at runtime. The
offline packer preserves the full direction set, ear order, integer sample
delays, and original f32 taps; it performs no EQ, resampling, quantization, or
tap truncation. The runtime built-in bank stores taps as f32 and compact
direction/delay records. Only the selected measurement HRIRs or the small
interpolated virtual-speaker kernels are widened to f64 for the unchanged
renderer. The full f64 tap bank is not allocated on production renderer paths.
After the 7.1.4 virtual-source kernels are prepared, the temporary f32 bank and
input asset staging buffer are released.

Measurements below are one Windows x64 release run. Native heap figures come
from the `hrtf-memory` example's counting allocator; Browser memory figures are
the actual WebAssembly linear-memory size and Node process RSS. Renderer timings
use 8 sources and 51,200 input samples; Browser decode timings are the bridge's
per-frame binaural timing statistic and are not directly comparable to those
native sample-loop timings.

| Profile | Native asset read / f32 parse | Native steady render, 51,200 samples / 8 sources | Browser asset read / JS SHA-256 / WASM instantiate / decoder init | WASM linear memory after selection (fresh instance) | WASM binaural mean |
| --- | ---: | ---: | ---: | ---: | ---: |
| SADIE II D1 / KU100 | 4.6 ms / 21.5 ms | 236 ms | 4.9 ms / 14.2 ms / 1.9 ms / 479 ms | 38,076,416 bytes | 9.91 ms |
| SADIE II D2 / KEMAR | 5.1 ms / 21.6 ms | 235 ms | 5.0 ms / 13.9 ms / 14.1 ms / 465 ms | 38,076,416 bytes | 9.90 ms |

The Browser WASM file is 1,190,977 bytes and contains renderer code only. The
v0.1.6 Chromium Standard ZIP is 38,430,822 bytes and includes both HRTF assets
(36,749,448 bytes total). There is no separate Full package.

The f64-bank implementation remains the comparison oracle in tests. Across
D1 and D2, front/rear/left/right/top/front-top plus two arbitrary
non-measurement directions, resolved HRIR pairs are bit-identical: maximum and
RMS tap error 0, ITD difference 0 samples, and ILD difference 0 dB. The
production renderer smoke tests cover one and eight fixed sources with rotating
impulse activity. A separate test-only time-varying FIR oracle changes the
resolved direction every 128 samples while preserving input history; its
moving-direction PCM max/RMS error is also exactly 0. This checks direction
trajectory interpolation but does not add moving-source updates to the
production renderer. All compared PCM is finite. Separate 90° left/right
energy sanity checks also pass.

The Browser no longer has a remote HRTF download or persistent HRTF-cache path.
Selection reads the packaged asset and verifies it before the renderer uses it.

Lossless packaging tests found gzip-9 sizes of 14.15 MB (D1) and 14.09 MB
(D2). Zstandard level 1 yielded 14.35 MB and 14.30 MB respectively. The release
assets stay uncompressed and avoid a new runtime codec.

The headless CLI selects a built-in with `--binaural --binaural-hrtf ID` using
the stable IDs above. `--binaural-sofa FILE` remains the explicit Custom SOFA
path and takes precedence over a built-in resource.
