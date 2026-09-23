# Spatial portability

OpenJOC implements its spatial rendering DSP directly rather than delegating
object rendering to platform-specific spatial audio engines. Operating-system
audio APIs may be used for integration and I/O, but they do not define
OpenJOC's spatial rendering result.

One renderer. Same spatial semantics across platforms.

## Native 22.2

`22.2` is the generic OpenJOC speaker preset for ITU-R BS.2051-3 Sound System H
(9+10+3): 24 semantic channels, 22 spatial speakers, and two semantic LFEs.
The canonical OpenJOC order is:

```text
FL, FR, FC, LFE1, BL, BR, FLc, FRc, BC, LFE2, SiL, SiR,
TpFL, TpFR, TpFC, TpC, TpBL, TpBR, TpSiL, TpSiR, TpBC,
BtFC, BtFL, BtFR
```

The public source identifies the Sound System H speakers and their admissible
azimuth/elevation ranges in Table 10 of
[ITU-R BS.2051-3](https://www.itu.int/dms_pubrec/itu-r/rec/bs/R-REC-BS.2051-3-202205-I%21%21PDF-E.pdf).
OpenJOC uses deterministic midpoints of those public ranges for its normalized
layout data. Bottom, middle, upper, and top are four layers in the same generic
`SpatialLayout` projector; there is no 22.2-specific renderer.

LFE1 and LFE2 are never spatial projection vertices. The current E-AC-3 input
boundary supplies one base LFE plane; for a 22.2 physical output that plane is
copied to both explicitly labeled LFE destinations. Objects cannot enter either
LFE through point, Region, Extent, Spread, Pair, or ChannelLock projection.

WAV output is 24-channel PCM without a fabricated WAVEFORMATEXTENSIBLE mask,
because the standard mask cannot faithfully describe the full 22.2 identity
set. The CLI reports this limitation. CAF output carries ordered channel
descriptions, using standard CAF labels where available and explicit
coordinate descriptions for the remaining 22.2 positions.

## Built-in generic binaural

`--binaural` without `--sofa` selects the bundled SADIE II D1 KU100 generic
resource. OpenJOC also ships a SADIE II D2 KEMAR built-in profile in the
API/plugin selectors. These are offline data
resources, not external renderers. Built-in v2 assets are decoded directly to
the same `HrirBank` representation used by the existing runtime spherical
interpolation, delay-aware causal FIR convolution, and tail-drain path. User
`--sofa FILE` remains an explicit SOFA input and malformed files still fail
closed.

License gate: `HRTF_REDISTRIBUTION_VERIFIED`.

SADIE II D1 and D2 each contain 8,802 measured directions plus 15 exact
canonical virtual-speaker aliases. The 56-byte `.ojhrtf` v2 envelope binds preset ID, payload
length, and checksum. Its direct payload stores local XYZ directions, ear
ordering, per-ear sample delays, and every tap; it does not contain a SOFA or
NetCDF file. The offline HDF5-to-CDF1 intermediate converter is
`tools/generate-builtin-hrtf.py`, followed by the Rust
`pack-hrtf-asset` example. Native rendering needs neither HDF5 nor a runtime
network download. The Browser Standard package keeps renderer WASM separate
and bundles both D1 and D2. It verifies the selected local asset before use, so
both profiles work offline after installation. There is no separate Full
offline package or runtime HRTF download.

Reproducibility record for the authorized upstream file used for this bundle:

```text
upstream source: SADIE II Zenodo 10886409
derived CDF1 intermediate: preserved hashes in docs/hrtf.md
final format: OJHRTF v2 direct records; SHA-256 pinned in BuiltinHrtf registry
format regeneration: generate-builtin-hrtf.py + pack-hrtf-asset example
```

The standard default virtual layout is `7.1.4`. The resource is native 48 kHz,
so no built-in-only resampler is introduced. HRIR delays are zero in this
source; the renderer nevertheless preserves its general delay-aware SOFA
contract.

## API behavior

Rust callers use `BinauralConfig::builtin_generic(...)` for the default or
`BinauralConfig::from_sofa_bytes(...)` for an explicit user dataset. Existing
`BinauralConfig { sofa_bytes: ... }` construction remains valid: empty bytes
select the built-in resource and non-empty bytes select the user SOFA. C ABI
callers pass a null/zero SOFA buffer for the built-in resource and a non-empty
buffer for a user SOFA; the ABI struct layout and size-compatibility rules are
unchanged.
