# JOC rendering

The canonical speaker and binaural rendering guide is maintained in the [OpenJOC documentation site](site/using/speaker-rendering.md).

This repository path is retained as a short compatibility pointer for existing links.

### File sample encoding

`decode`, `decode-payload`, and `render-joc` accept `--output-format f32|f64`.
The default remains `f32`; `--reference-f64` is retained as an alias for
`--output-format f64`. Repeating the same selection is allowed; conflicting
selections are rejected regardless of order. This selects file encoding only,
not another renderer or a higher-precision decoding path. In particular, the
normal `OpenJocSession` render path supplies f32 PCM, which is widened when
writing an f64 file; widening does not restore precision. The output path's
extension still selects WAV or CAF where supported.

Performance report schema v1 retains the `output_conversion_wav_write` key
for compatibility. Its duration covers output conversion and file writing for
both WAV and CAF.
