# Troubleshooting

Use this page to identify which boundary failed before changing configuration
or blaming the renderer.

## First checks

```sh
openjoc --version
openjoc --help
openjoc self-test
```

Keep the input local and seekable when testing an MP4/M4A. Raw E-AC-3 and
seekable ordinary MP4/M4A are the documented input paths; fragmented or
non-seekable MP4 is not admitted by the streaming path. For container inputs,
keep `ffprobe` and `ffmpeg` available as described in [Installation](../getting-started/installation.md).

## Inspect before rendering

```sh
openjoc inspect input.ec3
openjoc decode input.ec3 --output-dir decode-report
```

`inspect` reports the carrier classification, profile, topology, timing, and
rejection boundary without asking the speaker renderer to guess. `decode`
produces metadata manifests and diagnostic ReconstructionBasis row WAVs. A
successful decode does not imply that every speaker layout or container can
represent the result.

## The render command fails

Check the [CLI reference](../reference/cli-reference.md) for the exact option
spelling and run the smallest admitted command first:

```sh
openjoc render-joc input.ec3 --layout 2.0 --output stereo.wav
```

Then move to the intended layout. Standard WAVEFORMATEXTENSIBLE masks are
available only where the channel identities are representable. `7.1.6` and
the `9.1` family require CAF for semantic channel descriptions; `22.2` and
custom geometry use explicit unmasked PCM and are not claims about arbitrary
hardware playback.

If you choose binaural output, remember that it is virtual-speaker rendering:

```sh
openjoc render-joc input.ec3 --binaural --output headphones.wav
```

The bundled SADIE II D1 HRTF is generic. A custom local SOFA file must match
the documented strict `SimpleFreeFieldHRIR` subset; HDF5/NetCDF-4 and automatic
resampling are not supported. See [Binaural and SOFA](binaural-sofa.md).

## The ADM export fails or objects are static

Validate the output independently:

```sh
openjoc export-adm input.ec3 --output reconstructed.wav --adm-policy best-effort
openjoc validate-adm reconstructed.wav
```

Strict export fails closed unless the complete decoded-JOC/OAMD binding profile
is admitted. Best-effort export preserves neutral/static output and records an
`unsupported_binding_reason` when the correspondence cannot be proven. In the
admitted profile, moving reconstructed Objects represent decoded carrier-local
movement, not recovery of the authored Atmos master. Read [Decoded Objects vs
authored Objects](../concepts/decoded-vs-authored-objects.md) and [Reconstructed
ADM export](reconstructed-adm-export.md).

If export reports a PCM24 range or non-finite error, do not expect clipping,
normalization, or a hidden limiter. The writer fails closed at the signed
24-bit boundary; see [PCM24 headroom](../compatibility/pcm24-headroom.md).

## Windows playback does not use OpenJOC

Run the package's `verify.bat`, confirm the OpenJOC filter is registered, and
select **LAV Audio Decoder (OpenJOC)** as **Prefer** in PotPlayer. Ordinary
E-AC-3 and compressed passthrough intentionally remain on the stock path.
Only positively confirmed JOC is admitted to the OpenJOC filter. See [Windows
LAV / PotPlayer](windows-lav-potplayer.md).

## Collect a useful issue report

Include the OpenJOC version, platform, exact command, sanitized `inspect` or
validator output, selected layout/container, and whether the failure is
deterministic. Do not attach private/commercial media or derived PCM unless
you have permission. A passing structural validator is not evidence of native
JOC renderer equivalence; report both observations separately.
