# OpenJOC mpv integration patchset

This directory stores reproducible source patches for custom mpv builds. It
does not vendor mpv, FFmpeg, or OpenJOC.

The patchset was developed and built against:

| Baseline | Source commit | Patch |
| --- | --- | --- |
| mpv 0.41.0 | `41f6a645068483470267271e1d09966ca3b9f413` | `patches/mpv-0.41.0-openjoc.patch` |
| mpv master | `e7191f2a65d64af266c5c80793e79d2f4b92b789` | `patches/mpv-master-openjoc.patch` |

Both patch files contain the same three-part architecture: the optional
OpenJOC integration, the segment-boundary classification reset, and bounded
positive pre-demux admission for raw JOC. The stable and master patches are
kept separately so upstream source drift is visible.

## Build boundary

The build needs:

- FFmpeg with the native `libopenjoc` decoder patch from
  `integrations/ffmpeg/native/patches/`;
- OpenJOC C ABI 1.6 or newer, discoverable through `pkg-config` as `openjoc`;
- normal mpv dependencies, including Meson, Ninja, libass, and libplacebo.

Without the `openjoc` pkg-config module, mpv builds normally and does not add
the classifier or any OpenJOC behavior.

Apply one patch to a clean matching mpv checkout:

```sh
git apply /absolute/path/to/OpenJOC/integrations/mpv/patches/mpv-0.41.0-openjoc.patch
meson setup build --buildtype=debugoptimized -Dtests=false \
  -Dmanpage-build=disabled -Dhtml-build=disabled -Dpdf-build=disabled
meson compile -C build
```

The patch does not change the video or subtitle pipelines.

## Verified player commands

The native FFmpeg decoder is explicitly named `libopenjoc`; the ordinary
`eac3` decoder remains the default candidate. The patched player automatically
probes E-AC-3 packets only when no explicit decoder override or E-AC-3
passthrough request is active.

```sh
# Automatic JOC selection, binaural OpenJOC rendering.
mpv joc.mp4 --ad-lavc-o=render_mode=binaural

# Explicit decoder debugging.
mpv joc.mp4 --ad=libopenjoc

# Physical 2.0 speaker rendering; distinct from the menu's LAV
# "Stereo (Speakers)" policy, which uses render_mode=stereo.
mpv joc.mp4 --audio-channels=2.0 \
  --ad-lavc-o=render_mode=speaker,speaker_layout=2.0

# Physical 5.1. FFmpeg/mpv's side-surround transport layout is explicit.
mpv joc.mp4 '--audio-channels=5.1(side)' \
  --ad-lavc-o=render_mode=speaker,speaker_layout=5.1

# Physical 7.1.4 through an explicit 12-channel mpv map.
mpv joc.mp4 \
  --audio-channels=fl-fr-fc-lfe-bl-br-sl-sr-tfl-tfr-tbl-tbr \
  --ad-lavc-o=render_mode=speaker,speaker_layout=7.1.4

# Physical 9.1.6 through an explicit 16-channel mpv map.
mpv joc.mp4 \
  --audio-channels=fl-fr-fc-lfe-bl-br-sl-sr-wl-wr-tfl-tfr-tsl-tsr-tbl-tbr \
  --ad-lavc-o=render_mode=speaker,speaker_layout=9.1.6

# 22.2 validation without multichannel hardware.
mpv joc.mp4 --ao=null --audio-channels=22.2 \
  --ad-lavc-o=render_mode=speaker,speaker_layout=22.2
```

`--ad-lavc-o` forwards native OpenJOC AVOptions. The GUI menu covers the
output, Dialnorm, HRTF/SOFA, virtual-layout and independent post-render gain
controls. Gain uses a named `lavfi` volume filter only for a confirmed
selected OpenJOC E-AC-3 decoder; it never changes mpv's master volume or the
decoder AVOption map. Other decoder options such as DRC and validation remain
available only through the native FFmpeg option boundary.

`--audio-spdif=eac3` is an explicit compressed passthrough request. It selects
mpv's SPDIF path and bypasses OpenJOC software rendering.

The focused harness is:

```sh
integrations/mpv/verify-player.sh /absolute/path/to/mpv \
  /absolute/path/to/legal-or-local-fixture-directory
```

Opt-in profile examples are in [`mpv.conf.example`](mpv.conf.example).

## OpenJOC settings menu in the Player Bundle

The project-provided Player Bundle includes a Lua/OSD settings menu. Press
`Ctrl+Alt+J`, move with the arrow keys, and press Enter to choose an item.
The main page mirrors the LAV OpenJOC choices: Stereo speakers, Binaural,
5.1, 7.1, 5.1.2, 5.1.4, 7.1.2, or 7.1.4; Calibrated or Unity/Compatibility
Dialnorm; D1/KU100, D2/KEMAR, or a local Custom SOFA file; and the 7.1.4 or
experimental 9.1.6 binaural virtual layout. The LAV live JOC Stream page is
not available in this mpv integration.

**Save selection for next OpenJOC file** stores draft decoder settings in the
bundle config directory and merges them into the file-local FFmpeg decoder
option map before decoder creation for files with an E-AC-3 audio track. The
gain value is stored independently under `output_gain_tenths_db`; old JSON
files that omit it default to exact 0 dB. JOC cannot be identified before
decoder creation, so plain or unselected E-AC-3 tracks can also receive decoder
options. Since the map is file-local, options may reach other audio decoders in
a mixed-track file and produce unsupported-AVOption warnings. **Apply Current**
explicitly updates that file-local option map for a currently confirmed
OpenJOC E-AC-3 track; mpv rebuilds the audio chain, so a brief gap may occur,
but the file is not sought or reloaded. This is separate from saving the
next-file defaults. The menu does not select or force a decoder, change mpv's
`audio-channels` setting, or seek the current stream. Files without E-AC-3
bypass the pre-load hook and keep mpv's usual decoder selection and
audio-output mapping. The selected renderer layout may still be adapted to the
active AO according to mpv's normal channel negotiation. The SOFA chooser is a
text prompt for an existing local file path, not a platform file-picker.

The Live output gain row spans −20.0 to +20.0 dB in 0.1 dB steps and has a
Reset control. It previews only while the selected track is confirmed as
E-AC-3 decoded by `libopenjoc`; FLAC, PCM, ordinary E-AC-3 and compressed
passthrough do not receive the named gain filter. Cancel restores the gain
captured before the unsaved preview. Esc, outside-click or closing the menu
keeps a draft and any current-playback preview, but does not persist it; Save
stores the value for later OpenJOC files. Pending or failed live updates are
reported explicitly. The packaged D1/D2 binaural exact-unity qualification for
this candidate remains a CI release gate and has not yet been reported green.

Settings rows show the pending/saved selection; separate read-only rows report
mpv's current decoder and channel properties, not live JOC metadata or
diagnostics. To use an exact hardware channel map, configure mpv's normal audio
output separately; the menu intentionally does not change that setting for
other media. mpv's normal volume remains separate from the independent
post-render OpenJOC gain stage.

Fixtures are intentionally not copied into this repository. Private media is
accepted only as a local test input and is never part of the patchset.
