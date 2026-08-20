# OpenJOC mpv integration patchset

This directory stores reproducible source patches for custom mpv builds. It
does not vendor mpv, FFmpeg, or OpenJOC.

The patchset was developed and built against:

| Baseline | Source commit | Patch |
| --- | --- | --- |
| mpv 0.41.0 | `41f6a645068483470267271e1d09966ca3b9f413` → `c78b53e3bc` | `patches/mpv-0.41.0-openjoc.patch` |
| mpv master | `e7191f2a65d64af266c5c80793e79d2f4b92b789` → `0be7e69ecd` | `patches/mpv-master-openjoc.patch` |

Both patch files contain the same two-commit source series: the optional
OpenJOC integration and the segment-boundary classification reset. The stable
and master patches are kept separately so upstream source drift is visible.

## Build boundary

The build needs:

- FFmpeg with the native `libopenjoc` decoder patch from
  `integrations/ffmpeg/native/patches/`;
- OpenJOC C ABI 1.3 or newer, discoverable through `pkg-config` as `openjoc`;
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

# Physical stereo; this is not binaural.
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

`--ad-lavc-o` forwards native OpenJOC AVOptions. Advanced controls such as
DRC, dialnorm, and SOFA remain at that decoder boundary; mpv does not duplicate
the OpenJOC configuration surface.

`--audio-spdif=eac3` is an explicit compressed passthrough request. It selects
mpv's SPDIF path and bypasses OpenJOC software rendering.

The focused harness is:

```sh
integrations/mpv/verify-player.sh /absolute/path/to/mpv \
  /absolute/path/to/legal-or-local-fixture-directory
```

Opt-in profile examples are in [`mpv.conf.example`](mpv.conf.example).

Fixtures are intentionally not copied into this repository. Private media is
accepted only as a local test input and is never part of the patchset.
