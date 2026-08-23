# Custom speaker layouts

Built-in presets remain the recommended path for ordinary renders:

```sh
openjoc render-joc input.m4a --layout 7.1.4 --output render.wav
```

Advanced users can supply a versioned JSON layout without changing the
renderer or creating a second DSP path:

```sh
openjoc render-joc input.m4a \
  --layout-file fixtures/speaker-layouts/studio-irregular.json \
  --output render.caf
```

`--layout` and `--layout-file` are mutually exclusive. A custom layout is
ordered exactly as its `speakers` array; that order is the interleaved PCM
order and the semantic label order reported by the Rust and C APIs.

## Format and coordinates

The current format is JSON `version: 1`:

```json
{
  "version": 1,
  "name": "My Studio",
  "speakers": [
    {"name": "FL", "azimuth": 35.0, "elevation": 0.0},
    {"name": "FR", "azimuth": -35.0, "elevation": 0.0},
    {"name": "Sub", "azimuth": 0.0, "elevation": -20.0, "role": "lfe"}
  ]
}
```

The renderer uses the existing OpenJOC normalized Cartesian convention. In
the spherical input form, azimuth is in degrees, positive toward the OpenJOC
left side, with `0` straight ahead; elevation is in degrees, positive above
the listener. Valid ranges are azimuth `-180..=180` and elevation
`-90..=90`. Internally, front/rear is `y=0..1`, left/right is `x=0..1`, and
bottom/top is signed `z=-QMAX..QMAX`. There is no second coordinate system in
the projector.

`role` is `full_range` by default or explicitly `lfe`. LFE channels are
logical output channels, remain in the declared order, and are not spatially
panned. The decoded base LFE is copied to each declared logical LFE output,
matching the existing multiple-LFE preset behavior. This feature does not
add crossover, bass management, delay, gain calibration, or room correction.

The implementation admits up to 64 output channels and requires at least two
full-range speakers. Names must be unique and non-empty. Coordinates must be
finite and in range; duplicate or near-degenerate full-range directions,
empty layouts, malformed JSON, unknown fields, and unknown versions are
rejected before rendering. JSON numbers are never allowed to become NaN or
Infinity PCM.

## Projection coverage policy

The existing generic projector defines coverage for structurally valid custom
layouts; it does not add a second fallback renderer. For finite source
coordinates outside a layout's rectangular support, `x` is clamped to the
first/last anchor in the selected row and `y` is clamped to the first/last row.
For multi-layer layouts, `z` is clamped to the lowest/highest layer. Between
adjacent layers, target vectors are blended with the existing equal-power
cosine/sine law. The resulting dynamic target is normalized by the existing
projector, so supported finite sweeps remain deterministic, finite, and
bounded. Structurally unusable layouts are rejected during construction;
finite out-of-bound source positions are defined boundary projections rather
than undefined behavior.

## API and container boundary

Rust callers can construct the same validated object directly:

```rust
use openjoc_api::{OpenJocConfig, OpenJocSession};
use openjoc_scene::{SpeakerGeometry, SpeakerLayout};

let layout = SpeakerLayout::custom(
    "studio",
    vec![
        SpeakerGeometry::full_range("A", -40.0, 0.0),
        SpeakerGeometry::full_range("B", 8.0, 6.0),
        SpeakerGeometry::full_range("C", 48.0, 0.0),
    ],
)?;
let session = OpenJocSession::new(OpenJocConfig::default().with_speaker_layout(layout))?;
```

The C ABI 1.4 appends `custom_speaker_layout` to
`openjoc_decoder_config`. It points to an ordered array of
`openjoc_custom_speaker` records and is copied/validated during decoder
creation. Existing preset callers can leave it null. The ABI does not require
temporary JSON files.

For custom physical layouts, WAV is written as deterministic, truthful
unmasked PCM in the declared channel order; a
standard WAVEFORMATEXTENSIBLE speaker mask would falsely claim standard
identities. CAF is recommended when downstream interchange must preserve
geometry because OpenJOC writes coordinate channel descriptions there.
Downstream players, FFmpeg channel-layout negotiation,
GStreamer, DirectShow/LAV, and physical devices may still have narrower
geometry contracts; renderer support does not imply host/device support.
