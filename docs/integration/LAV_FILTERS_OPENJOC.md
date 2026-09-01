# Windows DirectShow / LAV Filters integration

Current OpenJOC publishes an optional downstream LAV Audio Decoder for the
Windows DirectShow ecosystem. The primary validated host is PotPlayer.

## Public source

The source is published as [LAVFilters-OpenJOC](https://github.com/chyinan/LAVFilters-OpenJOC),
a downstream fork of [Nevcairiel/LAVFilters](https://github.com/Nevcairiel/LAVFilters).
The public integration branch is `openjoc-main`, based on LAV Filters 0.83 at
`fefb6987994ed56e4525e8a125f5fbb53707bc52`. Release source is frozen by the
immutable downstream tag `openjoc-0.15.0`.

The public release also includes the
`openjoc-lav-0.15.0-corresponding-source.zip` asset, which carries the full
recursive corresponding-source and third-party license closure.

## Routing behavior

- Ordinary E-AC-3 remains on stock LAV/FFmpeg decoding.
- Ordinary AC-3 remains on stock LAV/FFmpeg decoding.
- E-AC-3 passthrough remains authoritative on the existing LAV bitstream path.
- Only positively confirmed JOC is admitted to OpenJOC.
- Confirmed JOC can be decoded from raw E-AC-3 and MP4 E-AC-3 inputs.
- The standalone core, C ABI, and published LAV fork positively admit the
  standards-defined original-AC-3-I0 plus E-AC-3-D0 JOC shape through the same
  bounded classifier/decoder lane. A transparent deterministic raw fixture
  has been validated through a real DirectShow graph for Stereo and 7.1.4.
- Candidate eligibility covers both AC-3 and E-AC-3 codec identities, but
  ordinary AC-3 and non-JOC E-AC-3 are rejected by the classifier and replayed
  exactly through stock LAV/FFmpeg. Active bitstreaming and AC-3 SPDIF retain
  precedence over OpenJOC probing.
- The known malformed real-world file remains rejected at its truncated AU0
  EMDF boundary; LAV does not skip, pad, truncate, or search past that AU.
- The validated mixed-carriage claim is raw elementary-stream only. An
  AC-3-tagged MP4 wrapper produced by available public tooling rewrites the
  payload, so containerized mixed-carriage integration remains follow-up work.
- The OpenJOC-enabled filter has a separate DirectShow COM identity and can be
  installed side-by-side with stock LAV.

The accepted host checks cover seek forward/backward, EOS, reopen,
stop/reopen, side-by-side installation, uninstall, and restoration of stock
LAV. These claims describe the validated PotPlayer workflow; they do not imply
endorsement by PotPlayer, LAV Filters, FFmpeg, Dolby, Microsoft, or SADIE.

## Output scope

The public DirectShow subset contains exactly eight explicit fixed 48 kHz
IEEE-float PCM policies:

- Stereo (Speakers): 2 channels, mask `0x00000003`, conventional physical
  two-speaker rendering without HRTF;
- Binaural (Headphones): 2 channels, mask `0x00000003`, OpenJOC virtual
  speaker rendering through the embedded SADIE II D1 KU100 HRTF;
- 5.1: 6 channels, mask `0x0000060f`;
- 7.1: 8 channels, mask `0x0000063f`;
- 5.1.2: 8 channels, mask `0x0000560f`;
- 5.1.4: 10 channels, mask `0x0002d60f`;
- 7.1.2: 10 channels, mask `0x0000563f`;
- 7.1.4: 12 channels, mask `0x0002d63f`.

Each policy makes one exact semantic `WAVEFORMATEXTENSIBLE` proposal. There is
no fallback mask or alternate proposal. Raw and MP4 paths passed strict capture
with actual sample delivery, exact channel order/mask/frame sizing, checked
10/12-channel buffers, flush, seek, EOS, reopen, and policy switching.

The endpoint evidence is intentionally separate: VB-Audio WaveOut and Realtek
DirectSound delivered all 14 raw/MP4 attempts; VB-Audio DirectSound rejected
all 14 with `0x8004025C`. A virtual driver or a stereo-configured physical
endpoint does not prove physical multichannel reproduction. Physical
multichannel hardware is not verified.

Automatic downstream semantic layout discovery is `AUTO_NOT_RELIABLE`.
Stereo remains the default and all other layouts, including Binaural, require
explicit selection. Stereo and Binaural both emit two-channel IEEE-float PCM,
but they are not aliases: Binaural applies the existing OpenJOC HRTF path.
Production code does not infer semantics from endpoint/product names, perform
Bass Management, or map physical subwoofer counts to logical LFE channels.
Standalone 7.1.6, 9.1.x, 22.2, and custom-geometry support are not LAV output
claims. The full matrix is in the
[`windows-lav-multichannel-2026-08-25` result](evidence/windows-lav-multichannel-2026-08-25/OPENJOC_LAV_MULTICHANNEL_OUTPUT_RESULT.txt),
beside the machine-readable JSON evidence.

## OpenJOC property page and programme level

The side-by-side filter adds a dedicated **OpenJOC** property page. The existing
eight-policy output selector lives on that page without changing its persisted
numeric values or strict output contracts. Stock LAV has no OpenJOC page or
OpenJOC settings interfaces.

**OpenJOC output** is the PCM speaker layout rendered and sent downstream. It
is a renderer target layout, not physical-endpoint detection or automatic
downmix. Select a layout supported by the downstream renderer/device: use
Stereo (Speakers) for conventional two-speaker playback, Binaural (Headphones)
for built-in-HRTF headphone rendering, 5.1 for a physical 5.1 setup,
7.1 for a physical 7.1 setup, and the corresponding height layout for a
height-capable endpoint. Selecting an unsupported multichannel layout may cause
playback failure, stuttering, or downstream conversion. Selecting 7.1.4 on a
stereo endpoint does not add OpenJOC spatialization; any later stereo downmix,
if accepted, occurs outside OpenJOC.

The page exposes exactly two Dialnorm policies. **Calibrated (Recommended)**
maps to `OPENJOC_DIALNORM_DEFAULT` and respects encoded E-AC-3 programme
dialnorm. **Unity / Compatibility** maps to `OPENJOC_DIALNORM_ANALOG`, disables
dialnorm attenuation, and may sound substantially louder. These policies use
OpenJOC's existing decoder configuration; LAV does not multiply rendered PCM.
Dialnorm policy is not normalization, DRC, a quality mode, or mastering gain.

Output and Dialnorm settings persist only below
`Software\LAV\Audio\OpenJOC`. Dialnorm uses schema version 1 and falls back to
Calibrated for missing, future, mistyped, or invalid registry values. The new
level setting is exposed through a separate versioned COM IID so the published
`ILAVAudioSettings` and `ILAVOpenJocSettings` vtables remain unchanged.

The standard LAV Status page receives read-only volume statistics from valid
strict OpenJOC FP32 buffers without passing those buffers through stock audio
processing. Its existing meter capacity remains eight channels; 10- and
12-channel outputs display the first eight channel indices.

The same Status page exposes OpenJOC, Stock decoder, or Stock decoder
(OpenJOC fallback). Ordinary AC-3 and non-JOC E-AC-3 are normal stock
decoding and show no warning. A real pre-admission OpenJOC failure shows a
warning with a stable reason and bounded detail, including the first failed AU
when known; the warning remains visible for that stream and clears when the
next stream is positively classified. A downstream layout rejection is shown
as Unsupported output layout while preserving the actual OpenJOC state.

## Installation and rollback

Extract the complete Windows LAV package, double-click `install.bat`, accept
UAC, and require `verify.bat` to report **PASS**. Follow the included PotPlayer
quick start, and use `uninstall.bat` to remove only OpenJOC-owned registration
and files. The package installs under an isolated OpenJOC version directory,
registers a separate filter identity, and restores the stock LAV arrangement
on uninstall. Direct PowerShell entry points remain available for automation,
but elevated-PowerShell-only onboarding is not required.

## License boundary

The OpenJOC standalone core, CLI, SDK, and C ABI remain Apache-2.0. The
downstream LAV integration is distributed under the applicable GPL-compatible
upstream terms. The bundled LAV/FFmpeg build has an effective GPL-3.0-only
combined distribution classification. See the release's third-party notices,
license files, and corresponding-source asset for the complete boundary.
