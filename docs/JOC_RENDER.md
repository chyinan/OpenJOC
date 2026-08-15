# Experimental JOC speaker rendering

OpenJOC 0.4.0 exposes one executable JOC-to-speaker workflow with an
explicit selectable speaker preset:

```sh
openjoc render-joc INPUT.ec3 \
  --layout 7.1.4 \
  --output openjoc-render.wav
```

Seekable ordinary MP4/M4A input is accepted at the same boundary as the other
E-AC-3 commands:

```sh
openjoc render-joc INPUT.m4a \
  --layout 7.1.4 \
  --output openjoc-render.wav
```

Supported presets are `5.1`, `5.1.2`, `7.1`, and `7.1.4`. The `--layout`
argument is required; there is no implicit output-layout default. `5.1` is the
regression anchor for the original 0.4.0 integration.

The command performs container extraction, E-AC-3 Base/LFE decoding, JOC and
OAMD validation/decoding, persistent `JocSpatialBridge` accumulation, and
incremental WAV writing. It does not materialize a duration-sized
`ObjectScene` or reconstruction-basis capture.

The preset is data, not a separate JOC algorithm. It supplies public channel
order and clean normalized geometry to the existing generic `SpatialLayout`
projection. The horizontal coordinate runs from rear-left through the front
to rear-right. Height presets add a normalized height axis from `0` (base) to
`1` (height). These are explicit OpenJOC preset coordinates, not authored
object positions and not a vendor renderer geometry claim.

The public library layer is broader than this CLI preset list. Callers can
construct a validated `openjoc_scene::SpatialLayout` with arbitrary enabled
channels, LFE designation, knot axes, node vectors, and route vectors, then
pass it to the public `JocSpatialBridge::render_coordinates` API. The CLI
does not introduce a custom-layout file format; its stable user-facing names
are convenience presets over that generic layout engine.

## Automatic bridge control and optional override

The ordinary path assembles bridge control from decoded Base/RB codec
coordinates, parsed OAMD metadata, metadata timing, and the clean
codec-coordinate ordering contract. It does not infer authored-object
identity, use PCM statistics, or repair ordinals with a row/object guess.

`CONTROL.json` is optional. Supply `--topology bridge-control.json` only when a
complete explicit override, synthetic fixture, or expert debug control is
desired. The explicit source is used instead of automatic state; the two
sources are never implicitly merged.

The sidecar schema is `openjoc.joc-render-control.v1`. Its topology records
must be in the bridge's explicit codec-coordinate order: decoded Base full-band
channels first, followed by the decoded ReconstructionBasis rows. The record
count must match on every access unit. `updates` is optional and contains
frame-indexed `SpatialCoordinateUpdate` arrays; omitted fields inherit the
persistent bridge state.

A minimal 5-channel Base plus one ReconstructionBasis row control file is:

```json
{
  "schema": "openjoc.joc-render-control.v1",
  "topology": {
    "explicit_groups": [],
    "fixed_layout": [],
    "dynamic_records": [
      {"descriptor":{"source_class":"explicit_channel","identity":"FL","coordinates":[0.5],"spread":null,"paired":null,"raw3":null},"scalar":1.0,"active":true},
      {"descriptor":{"source_class":"explicit_channel","identity":"FR","coordinates":[0.5],"spread":null,"paired":null,"raw3":null},"scalar":1.0,"active":true},
      {"descriptor":{"source_class":"explicit_channel","identity":"FC","coordinates":[0.5],"spread":null,"paired":null,"raw3":null},"scalar":1.0,"active":true},
      {"descriptor":{"source_class":"explicit_channel","identity":"Ls","coordinates":[0.5],"spread":null,"paired":null,"raw3":null},"scalar":1.0,"active":true},
      {"descriptor":{"source_class":"explicit_channel","identity":"Rs","coordinates":[0.5],"spread":null,"paired":null,"raw3":null},"scalar":1.0,"active":true},
      {"descriptor":{"source_class":"explicit_channel","identity":"FC","coordinates":[0.5],"spread":null,"paired":null,"raw3":null},"scalar":1.0,"active":true}
    ]
  },
  "updates": []
}
```

The optional sidecar must be authored for the input stream's decoded
coordinate count; the example is not a universal JOC mapping. Unsupported or
withheld bridge semantics fail explicitly.

## Output contract

Every exposed preset has a deterministic public WAV order. The orders are:

```text
5.1:   FL, FR, FC, LFE, Ls, Rs
5.1.2: FL, FR, FC, LFE, Ls, Rs, TFL, TFR
7.1:   FL, FR, FC, LFE, Ls, Rs, Lb, Rb
7.1.4: FL, FR, FC, LFE, Ls, Rs, Lb, Rb, TFL, TFR, TBL, TBR
```

The order is deterministic and is not the internal E-AC-3 order. For the
original `5.1` path it remains:

```text
0 FL, 1 FR, 2 FC, 3 LFE, 4 Ls, 5 Rs
```

The default output is IEEE float32 WAV. `--reference-f64` selects IEEE float64
WAV. The RcLfe/Base LFE plane is copied only to `LFE`; it is not sent through
ordinary spatial projection and is not double-added. The active bridge planes
are ordered `FL, FR, FC, Ls, Rs` before the public WAV interleave.

The command prints the feature, experimental maturity, unresolved semantic
binding, requested and selected layout, channel count, LFE index, requested
and selected profile, compatibility deviations, sample rate, sample count,
and output channel order. `AUTO` evaluates
`ETSI_STRICT` first and selects `OBSERVED_VENDOR_COMPAT` only when the existing
whitelist admits all deviations. Explicit `ETSI_STRICT` never falls back.

`raw3` remains preserved and excluded from projection arithmetic. The stable
feature name is `JocSpatialBridge`; `SemanticBindingState` remains
`Unresolved`. This workflow makes no official vendor-equivalence, bit-exact,
or fidelity claim.

## SOFA-backed binaural rendering

The same real-JOC command can virtualize one supported speaker preset to stereo
through a caller-supplied admitted SOFA file:

```sh
openjoc render-joc INPUT.m4a \
  --layout 7.1.4 \
  --binaural-sofa HRTF.sofa \
  --lfe-policy equal-power-dual-mono \
  --output OUTPUT-binaural.wav
```

This is speaker-virtualized binaural rendering:

```text
real JOC -> JocSpatialBridge -> selected virtual speaker layout
          -> one fixed exact-direction HRIR per non-LFE speaker -> Left/Right
```

It is not a direct moving-object HRIR renderer and makes no vendor or
reference-product headphone-rendering claim. `--layout` names the virtual
speaker layout, not the two-channel output. The four existing presets remain
available: `5.1`, `5.1.2`, `7.1`, and `7.1.4`; `9.1.6` is not added here.

The SOFA path is local and user-supplied. It is parsed only within the existing
strict `SimpleFreeFieldHRIR`/NetCDF classic CDF-1 scope. Listener basis is
explicitly shared with the renderer contract: local `+X` is right, `+Y` is
front, and `+Z` is up. The virtual directions cover front, side/rear, and
front/rear height positions. Lookup is exact; all required directions are
preflighted before output starts, with no nearest-direction fallback or
interpolation. The decoded JOC sample rate must equal the SOFA HRIR rate;
OpenJOC does not silently resample either stream.

The admitted presets contain LFE, so binaural rendering requires one explicit
renderer-level policy:

- `--lfe-policy exclude` omits the virtual LFE channel.
- `--lfe-policy equal-power-dual-mono` adds the virtual LFE to both ears at
  equal-power gain.

Neither policy is a JOC semantic interpretation or vendor bass-management
claim. The binaural backend defaults to `direct`, the existing causal FIR
reference. `--backend partitioned --partition-size 256` selects the existing
fixed uniform partitioned backend; the partition size must be a power of two.
Both backends preserve streaming state, emit the complete causal HRIR tail,
and write stereo Left-then-Right PCM/WAV without a temporary multichannel
speaker WAV. The default format is IEEE float32; `--reference-f64` selects
IEEE float64.

Diagnostics identify `JocSpatialBridge` as enabled, maturity as experimental,
`SemanticBindingState` as `Unresolved`, the selected validation profile, output
mode, virtual layout, SOFA filename, backend, HRIR coverage, LFE policy, and
tail contract. Private SOFA paths are not embedded in public metadata.

The automatic assembly currently supports explicit-channel beds, fixed-layout
members when the selected library layout supplies matching routes, dynamic
point-like records, and dynamic region records. `raw3` remains opaque with no
assigned semantic name. `AUTO` behavior is unchanged: strict validation is
selected first and the existing compatibility policy is used only where its
current whitelist admits it.

`2.0` is not exposed: the existing bridge keeps Base LFE separate, but the
repository does not define a consumer-style stereo bass-management or LFE
fold-down policy. `9.1.6` is not exposed because this repository has no
admitted clean preset geometry for front-wide and six-height speaker mapping;
the generic bridge API can still accept caller-defined layouts at library
level. There is no fallback from either blocked layout to another preset.

## Professional preset feasibility audit

The following audit covers the broader professional names without treating a
name alone as evidence of a clean geometry definition.

| Preset | Classification | CLI status | Reason / boundary |
|---|---|---|---|
| `2.0` | `BLOCKED_BY_BASS_OR_FOLD_POLICY` | Not exposed | The bridge keeps Base LFE separate and the project has no specified consumer stereo bass-management or LFE fold-down policy. |
| `5.1` | `SUPPORTED_EXISTING_GEOMETRY` | Exposed | Existing 1D normalized geometry and the original public order remain the regression anchor. |
| `5.1.2` | `SUPPORTED_AFTER_PRESET_ADDITION` | Exposed | Uses the admitted project-defined normalized horizontal/height data; no external vendor geometry claim is made. |
| `5.1.4` | `BLOCKED_BY_CLEAN_GEOMETRY_DEFINITION` | Not exposed | No clean project geometry is admitted for the four height positions. |
| `7.1` | `SUPPORTED_AFTER_PRESET_ADDITION` | Exposed | Uses the generic 1D layout with explicit rear-bed positions and public order. |
| `7.1.2` | `BLOCKED_BY_CLEAN_GEOMETRY_DEFINITION` | Not exposed | No clean project geometry is admitted for the two height positions. |
| `7.1.4` | `SUPPORTED_AFTER_PRESET_ADDITION` | Exposed | Uses the generic two-axis layout with explicit lower/height node data and public order. |
| `9.1.4` | `BLOCKED_BY_CLEAN_GEOMETRY_DEFINITION` | Not exposed | No clean project geometry is admitted for front-wide plus four-height mapping. |
| `9.1.6` | `BLOCKED_BY_CLEAN_GEOMETRY_DEFINITION` | Not exposed | No clean project geometry is admitted for front-wide plus six-height mapping. |
| `22.2` | `BLOCKED_BY_CLEAN_GEOMETRY_DEFINITION` | Not exposed | The generic engine can represent a 24-channel 3D layout, but no clean/public 22.2 speaker geometry is admitted in this repository. |

The 22.2 result is not a renderer-domain limitation. If a clean speaker
definition is later admitted, adding its channels, geometry, LFE designation,
and public order is expected to be `DATA_ONLY`; no JOC bridge mathematics or
source-class behavior needs to change. The tests exercise a synthetic
24-channel layout to separate renderer capacity from professional-layout
provenance.

## Large-channel output audit

The renderer and in-memory `RenderedBlock` are N-channel: channel count is
derived from the validated layout and each channel is accumulated in its own
`Vec<f64>`. The ordinary WAV writer accepts the same dynamic channel count;
the current implementation is RIFF-only, so data sizes beyond the 32-bit RIFF
limit are rejected and no RF64 writer is provided. WAV output also carries no
speaker-label or channel-mask metadata in this workflow. The documented public
order is therefore the authoritative OpenJOC interpretation of the PCM
planes, not metadata that third-party DAWs are guaranteed to discover.

This distinguishes renderer support from full container description and from
third-party DAW interoperability. No 22.2 interoperability claim is made.
