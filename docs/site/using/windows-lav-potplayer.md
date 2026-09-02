# Windows LAV / PotPlayer

OpenJOC ships an optional downstream LAV Audio Decoder for Windows DirectShow. It has a separate filter identity and installs beside stock LAV. Installing the package does not change PotPlayer automatically.

The primary validated host workflow is PotPlayer. These instructions describe the package behavior recorded for the v0.12+ Windows integration; the OpenJOC release baseline for this site is v0.16.0.

## Install and verify

1. Download the Windows LAV package from the [OpenJOC releases page](https://github.com/chyinan/OpenJOC/releases).
2. Extract the complete ZIP to a writable directory.
3. Double-click `install.bat` and accept the Windows administrator prompt.
4. Double-click `verify.bat` and require **PASS**.
5. Close and reopen PotPlayer if it was running during installation.

The package installs under an isolated OpenJOC version directory and registers only its own DirectShow filter. It does not replace stock LAV, modify `PATH`, or change PowerShell execution policy.

## Select the filter in PotPlayer

1. Open PotPlayer preferences with `F5`.
2. Select **Filter Control** → **Filter Priority (Overall)**.
3. Choose **Add registered filter**.
4. Add **LAV Audio Decoder (OpenJOC)**.
5. Set it to **Prefer**, then select **Apply** and **OK**.

Keep the stock LAV decoder installed. If the OpenJOC filter is not listed, run `verify.bat` and repeat installation only if verification reports a failure.

## Routing behavior

- Ordinary E-AC-3 remains on the stock LAV/FFmpeg path.
- Compressed E-AC-3 passthrough remains authoritative and bypasses OpenJOC.
- Only positively confirmed JOC is admitted to the OpenJOC filter.
- Confirmed JOC can be decoded from raw E-AC-3 and MP4 E-AC-3 input.

The Windows adapter exposes exactly eight fixed 48 kHz IEEE-float PCM policies:

| Policy | Channels | WAVEFORMATEXTENSIBLE mask |
| --- | ---: | ---: |
| Stereo (Speakers) | 2 | `0x00000003` |
| Binaural (Headphones) | 2 | `0x00000003` |
| 5.1 | 6 | `0x0000060f` |
| 7.1 | 8 | `0x0000063f` |
| 5.1.2 | 8 | `0x0000560f` |
| 5.1.4 | 10 | `0x0002d60f` |
| 7.1.2 | 10 | `0x0000563f` |
| 7.1.4 | 12 | `0x0002d63f` |

Each policy makes one exact semantic proposal with no fallback mask. Stereo is the default; select other layouts explicitly. `AUTO_NOT_RELIABLE` is the current automatic downstream layout-discovery status.

## OpenJOC settings

Open the filter properties and select the **OpenJOC** tab. **OpenJOC output**
contains the same eight fixed output policies listed above; moving this control
to its own tab does not change layout, channel-order, media-type, or Stereo
default behavior.

**OpenJOC output** selects the PCM speaker layout that OpenJOC renders and
sends to the downstream audio renderer/device. Choose a layout supported by
that endpoint. **Stereo (Speakers)** is conventional two-speaker playback
without HRTF. **Binaural (Headphones)** uses the built-in SADIE II D1 KU100
HRTF to render the virtual speaker field to two-channel headphone PCM. Both
use the same two-channel PCM transport shape, but they are different
renderers. Selecting an unsupported multichannel layout may cause playback
failure, stuttering, or downstream conversion.

When Binaural is selected, the OpenJOC page also provides an HRTF source. The
default is **Built-in SADIE II D1 (Default)**. **Custom SOFA...** opens a
normal Windows file picker for one local `.sofa` file; OpenJOC validates the
selected dataset before Apply and uses the same strict loader as the standalone
SOFA path. An invalid selection is rejected and the previous known-good HRTF
remains active. If a persisted custom file is later moved, deleted, denied, or
becomes invalid, reopening Binaural reports a bounded HRTF configuration error
until the user selects the built-in source or another valid file.

The Binaural page also selects the virtual speaker layout: **7.1.4
(Default)** or **9.1.6 (Experimental)**. This parameterizes the existing
virtual-speaker stage before the same SOFA/HRTF backend; final output remains
two-channel binaural PCM. 9.1.6 is not claimed to be better or reference
quality, and its extra virtual feeds may cost more CPU.

The canonical 9.1.6 intermediate order is `FL, FR, FC, LFE, Lb, Rb, Ls, Rs,
Lw, Rw, Ltf, Rtf, Ltm, Rtm, Ltr, Rtr`. LFE follows the existing explicit LFE
policy; the other 15 virtual feeds are resolved by the existing exact or
bounded interpolation path, then convolved to the two ear channels. This is a
virtual layout, not a 16-channel downstream speaker output.

OpenJOC does not detect the physical speaker configuration or automatically
downmix to match the endpoint. A larger layout may be rejected or accepted and
converted later by Windows/the downstream renderer; that conversion is outside
OpenJOC and is not equivalent to choosing **Stereo (Speakers)** directly.

For conventional two-speaker playback choose **Stereo (Speakers)**; for
headphones choose **Binaural (Headphones)**.

Examples: conventional two-speaker playback → **Stereo (Speakers)**;
headphones → **Binaural (Headphones)**; physical 5.1 →
**5.1**; physical 7.1 → **7.1**; a height-capable endpoint → the corresponding
height layout. Selecting **7.1.4** on a stereo endpoint does not provide
additional OpenJOC spatialization. Any later stereo downmix, if accepted by the
downstream path, happens outside OpenJOC.

**Dialnorm** has two choices:

- **Calibrated (Recommended)** respects the programme dialnorm carried by
  E-AC-3.
- **Unity / Compatibility** disables dialnorm gain for compatibility and may
  sound substantially louder.

This setting selects the decoder's programme-calibration policy. It is not
normalization, DRC, a quality mode, or mastering gain, and it adds no
post-render gain stage.

The **Status** page uses the standard LAV channel meters. It currently displays
at most the first eight output channels; for 10- and 12-channel policies, the
displayed first eight meters remain channel-index aligned with the PCM output.
It also reports OpenJOC, Stock decoder, or Stock decoder (OpenJOC fallback).
Ordinary stock AC-3/E-AC-3 has no warning. A real OpenJOC fallback displays a
structured reason and bounded detail, including the first failed AU when known;
the detail remains visible for that stream and clears on the next positively
classified stream. Unsupported downstream output layouts are reported as
output-layout errors rather than file corruption.

## Passthrough and hardware boundaries

OpenJOC does not infer a layout from endpoint names, perform Bass Management, or turn multiple physical subwoofers into multiple logical LFE channels. Standalone 7.1.6, 9.1.x, 22.2, and custom geometry are not LAV output claims.

The integration proves PCM sample delivery through the documented host and endpoint checks. It does not claim physical multichannel hardware playback on arbitrary devices.

## Uninstall and rollback

Double-click `uninstall.bat`. The package removes only OpenJOC-owned registration and files and restores the stock LAV arrangement. An already-absent OpenJOC installation is a successful no-op for the package's non-interactive uninstall path.

For the lower-level source, version pins, and engineering evidence, use the repository's [LAV integration contract](https://github.com/chyinan/OpenJOC/blob/master/docs/integration/LAV_FILTERS_OPENJOC.md). Evidence files are intentionally not published in this site.
