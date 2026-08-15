# Experimental JOC speaker rendering

OpenJOC 0.4.0-dev exposes one executable JOC-to-speaker workflow:

```sh
openjoc render-joc INPUT.ec3 \
  --topology bridge-control.json \
  --layout 5.1 \
  --output openjoc-render.wav
```

Seekable ordinary MP4/M4A input is accepted at the same boundary as the other
E-AC-3 commands:

```sh
openjoc render-joc INPUT.m4a \
  --topology bridge-control.json \
  --layout 5.1 \
  --output openjoc-render.wav
```

The command performs container extraction, E-AC-3 Base/LFE decoding, JOC and
OAMD validation/decoding, persistent `JocSpatialBridge` accumulation, and
incremental WAV writing. It does not materialize a duration-sized
`ObjectScene` or reconstruction-basis capture.

## Explicit bridge control

The current public decoder does not admit an automatic mapping from OAMD
authored-object order to ReconstructionBasis row order. The command therefore
requires a bridge-control sidecar. This is an explicit codec-coordinate input,
not an authored-object binding and not a guessed `row == object` renderer.

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

The sidecar must be authored for the input stream's decoded coordinate count;
the example is not a universal JOC mapping. Unsupported or withheld bridge
semantics fail explicitly.

## Output contract

Only the standard `5.1` speaker layout is currently exposed. The WAV channel
order is deterministic and is not the internal E-AC-3 order:

```text
0 FL, 1 FR, 2 FC, 3 LFE, 4 Ls, 5 Rs
```

The default output is IEEE float32 WAV. `--reference-f64` selects IEEE float64
WAV. The RcLfe/Base LFE plane is copied only to `LFE`; it is not sent through
ordinary spatial projection and is not double-added. The active bridge planes
are ordered `FL, FR, FC, Ls, Rs` before the public WAV interleave.

The command prints the feature, experimental maturity, unresolved semantic
binding, requested and selected profile, compatibility deviations, layout,
sample rate, sample count, and output channel order. `AUTO` evaluates
`ETSI_STRICT` first and selects `OBSERVED_VENDOR_COMPAT` only when the existing
whitelist admits all deviations. Explicit `ETSI_STRICT` never falls back.

`raw3` remains preserved and excluded from projection arithmetic. The stable
feature name is `JocSpatialBridge`; `SemanticBindingState` remains
`Unresolved`. This workflow makes no official Dolby, vendor-equivalence,
bit-exact, or fidelity claim. Binaural rendering and layouts other than 5.1
remain follow-up work.
