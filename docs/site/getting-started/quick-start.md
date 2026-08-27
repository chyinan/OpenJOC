# Quick start

This guide takes a supported JOC programme from input to a first file render.

## Render a speaker layout

```sh
openjoc render-joc input.m4a \\
  --layout 7.1.4 \\
  --output output.wav
```

The output extension selects the container. Standard layouts can use WAV when their channel identities are representable by `WAVEFORMATEXTENSIBLE`; use CAF when semantic channel descriptions are required.

## Try stereo or binaural output

```sh
openjoc render-joc input.m4a --layout 2.0 --output stereo.wav
openjoc render-joc input.m4a --binaural --output headphones.wav
```

The default binaural virtual field is 7.1.4 and the default LFE policy is `exclude`. A custom SOFA file and a different virtual layout are available through the options documented in the [CLI reference](../reference/cli-reference.md).

## Inspect before rendering

```sh
openjoc inspect input.ec3
openjoc --help
```

Use `inspect` to see whether the input is admitted as a JOC carrier and which profile or rejection boundary applies. Use `decode` when you need metadata manifests or diagnostic ReconstructionBasis row WAVs rather than a final speaker render.

## Export a reconstructed ADM file

```sh
openjoc export-adm input.m4a --output reconstructed.wav
openjoc validate-adm reconstructed.wav
```

The exporter also writes an adjacent `.adm-report.json`. Read [Reconstructed ADM export](../using/reconstructed-adm-export.md) before using the file in a DAW or interchange workflow.

## Pick the right next page

- Speaker setup: [Speaker rendering](../using/speaker-rendering.md)
- Custom geometry: [Custom speaker layouts](../using/custom-speaker-layouts.md)
- Headphones or SOFA: [Binaural and SOFA](../using/binaural-sofa.md)
- Windows playback: [Windows LAV / PotPlayer](../using/windows-lav-potplayer.md)
- Output semantics: [Output formats](../reference/output-formats.md)
