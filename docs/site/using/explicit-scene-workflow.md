# Explicit render-scene workflow

OpenJOC can render a portable, static scene made from caller-bound mono WAV
sources and a locally supplied `SimpleFreeFieldHRIR` SOFA file. The scene is
versioned as `openjoc.render-scene.v1`; it does not contain JOC objects,
ReconstructionBasis rows, OAMD slots, or backend settings.

```json
{
  "schema": "openjoc.render-scene.v1",
  "sample_rate_hz": 48000,
  "source_semantics": "explicit_spatial_sources",
  "sources": [
    {"id":"voice","input_wav":"audio/voice.wav","start_sample":0,
     "position":{"x":0.0,"y":1.0,"z":0.0},"gain":1.0}
  ]
}
```

Source paths are relative to the scene file. Absolute paths, parent traversal,
symlink escapes, duplicate IDs, unknown directions, and source-WAV sample-rate
mismatches are rejected before output promotion. Supported source WAVs are mono
PCM16/24/32 and mono IEEE-float32; they must match the scene rate. OpenJOC does
not resample, normalize, clip, or dither source audio.

Inspect a supported SOFA file first:

```text
openjoc sofa inspect listener.sofa --json
```

Render with an explicit backend:

```text
openjoc render-scene scene.json --binaural-sofa listener.sofa \
  --backend direct --output render-direct
openjoc render-scene scene.json --binaural-sofa listener.sofa \
  --backend partitioned --partition-size 256 --output render-partitioned
```

The output directory is transactional and contains `binaural.wav` (stereo
IEEE-float32, FL then FR) and `render.json` (`openjoc.render-result.v1`).
Output length is the scene input timeline plus the complete causal HRIR tail
(`N + M - 1`); no leading latency or tail trim is hidden. Backend selection is
explicit and never automatic.

The SOFA boundary is intentionally narrow: SimpleFreeFieldHRIR, admitted
versions 1.0/1.1/1.2, and the read-only NetCDF classic CDF-1 or NetCDF-4/HDF5
subset. HRIRs are converted to the scene sample rate; inputs already at that
rate retain their source coefficients bit-for-bit. Other SOFA conventions, interpolation,
nearest-direction fallback, moving sources, and downloads are not supported.
Users remain responsible for
the licensing and provenance of locally supplied SOFA data.

When rates differ, conversion preserves convolution gain and includes a common
causal filter delay in both HRIRs and the output tail. The result manifest adds
this delay to the backend's `algorithmic_latency_samples`. See the conversion delay
and HDF5 chunk limits in [Binaural and SOFA](binaural-sofa.md#sofa-scope).

The workflow is independent of unresolved JOC semantic binding:
`joc_semantic_binding` is `unresolved_not_used` in the result manifest.
