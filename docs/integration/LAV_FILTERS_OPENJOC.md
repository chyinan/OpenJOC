# Windows DirectShow / LAV Filters integration

OpenJOC v0.10.0 publishes an optional downstream LAV Audio Decoder for the
Windows DirectShow ecosystem. The primary validated host is PotPlayer.

## Public source

The source is published as [LAVFilters-OpenJOC](https://github.com/chyinan/LAVFilters-OpenJOC),
a downstream fork of [Nevcairiel/LAVFilters](https://github.com/Nevcairiel/LAVFilters).
The public integration branch is `openjoc-main`, based on LAV Filters 0.83 at
`fefb6987994ed56e4525e8a125f5fbb53707bc52`. The immutable integration tag is
[`openjoc-0.10.0`](https://github.com/chyinan/LAVFilters-OpenJOC/releases/tag/openjoc-0.10.0).

The public release also includes the
`openjoc-lav-0.10.0-corresponding-source.zip` asset, which carries the full
recursive corresponding-source and third-party license closure.

## Routing behavior

- Ordinary E-AC-3 remains on stock LAV/FFmpeg decoding.
- E-AC-3 passthrough remains authoritative on the existing LAV bitstream path.
- Only positively confirmed JOC is admitted to OpenJOC.
- Confirmed JOC can be decoded from raw E-AC-3 and MP4 E-AC-3 inputs.
- The OpenJOC-enabled filter has a separate DirectShow COM identity and can be
  installed side-by-side with stock LAV.

The accepted host checks cover seek forward/backward, EOS, reopen,
stop/reopen, side-by-side installation, uninstall, and restoration of stock
LAV. These claims describe the validated PotPlayer workflow; they do not imply
endorsement by PotPlayer, LAV Filters, FFmpeg, Dolby, Microsoft, or SADIE.

## Output scope

The current OpenJOC output through this DirectShow/LAV integration is 48 kHz
stereo float PCM. The standalone OpenJOC renderer supports additional
multichannel and binaural workflows, but those renderer capabilities are not
claims about DirectShow/LAV output in v0.10.0.

## Installation and rollback

Use the `install.ps1`, `verify.ps1`, and `uninstall.ps1` scripts in the
`openjoc-lav-0.10.0-windows-x64.zip` asset from an elevated PowerShell prompt.
The package installs under an isolated OpenJOC version directory, registers a
separate filter identity, and provides an uninstall path that restores the
stock LAV arrangement.

## License boundary

The OpenJOC standalone core, CLI, SDK, and C ABI remain Apache-2.0. The
downstream LAV integration is distributed under the applicable GPL-compatible
upstream terms. The bundled LAV/FFmpeg build has an effective GPL-3.0-only
combined distribution classification. See the release's third-party notices,
license files, and corresponding-source asset for the complete boundary.
