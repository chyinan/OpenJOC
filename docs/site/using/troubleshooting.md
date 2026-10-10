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

```text
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
the documented `SimpleFreeFieldHRIR` CDF-1 or NetCDF-4/HDF5 subset. HRIRs are
converted to 48 kHz before rendering; fractional source delays are rejected.
See [Binaural and SOFA](binaural-sofa.md).

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

## PCM noise with an old LAV Splitter {#pcm-noise-with-an-old-lav-splitter}

**Stop playback and mute or lower the output volume before investigating loud
static.** If you need to reopen the file to inspect the active filters, keep
the output muted.

The OpenJOC Windows package updates the audio decoder, not LAV Splitter or LAV
Video. In one confirmed case, **LAV Splitter 0.76.1** misidentified an MP4/MOV
`ipcm` track: MediaInfo reported signed **32-bit little-endian PCM**, 48 kHz,
2 channels, **3072 kb/s**, but LAV Audio's **Input** showed **16-bit big-endian
PCM (S16BE)** at **1536 kb/s**. Playback produced harsh static even with an
updated OpenJOC audio decoder. Updating the splitter to official **0.83**
resolved that case. This is a confirmed compatibility example, not a diagnosis
of every noise problem or a universal minimum-version claim.

1. In PotPlayer's active filter list, inspect **LAV Splitter / LAV Splitter
   Source** and **LAV Audio Decoder (OpenJOC)** separately. Record each loaded
   version and file path; the audio decoder's version does not establish the
   splitter's version. A manually configured external filter may still point
   at an old `LAVSplitter.ax` after an update elsewhere.
2. Compare the source track's MediaInfo details with LAV Audio's **Input**
   format, especially PCM bit depth, byte order, sample rate, and channel
   count. Do not confuse this with **Output**: legitimate output conversion
   can produce 16-bit PCM even when the source is 32-bit. For the example above,
   the incorrect **Input** format is the warning sign.
3. If the loaded splitter is old or the PCM input is misidentified, follow
   [Update only LAV Splitter](windows-lav-potplayer.md#update-only-lav-splitter).
   Keep the complete official x64 package separate from OpenJOC and preserve
   the OpenJOC audio filter priority. After restarting, verify the actual
   splitter version/path and corrected input format while muted, then test
   briefly at low volume.

If the input format is correct and noise remains, stop playback and collect
the source track details, active filter versions/paths, and Input/Output
status for an issue report. Redact personal path components before sharing.

## Collect a useful issue report

Include the OpenJOC version, platform, exact command, sanitized `inspect` or
validator output, selected layout/container, and whether the failure is
deterministic. Do not attach private/commercial media or derived PCM unless
you have permission. A passing structural validator is not evidence of native
JOC renderer equivalence; report both observations separately.
