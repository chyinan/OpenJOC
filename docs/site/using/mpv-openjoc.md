# mpv OpenJOC Player Bundle

The OpenJOC Player Bundle is a project-provided, patched mpv/FFmpeg build. It
is not an official upstream mpv or FFmpeg release. Extract the bundle and start
`bin/openjoc-mpv` on macOS/Linux or `bin/openjoc-mpv.cmd` on Windows with a
media file. Windows can also start the GUI directly with `bin/mpv.exe`; its
adjacent `bin/portable_config` is discovered automatically, so no wrapper or
global mpv configuration change is needed. Direct discovery applies when
`MPV_HOME` is unset; an existing `MPV_HOME` takes precedence. The packaged
launcher explicitly selects the bundle's config directory and does not change
the environment variable.

```text
bin/openjoc-mpv path/to/media
```

Confirmed JOC is selected automatically by the patched player. Ordinary
E-AC-3 remains on mpv's stock `eac3` decoder. Compressed passthrough can be
requested with `--audio-spdif=eac3`; that bypasses OpenJOC rendering.

## Open the settings menu

Press `Ctrl+Alt+J` in the Player Bundle to open the compact settings panel.
Click the arrows and **Set path…** control, or use Up/Down (Tab also moves
focus), Left/Right, and Enter. The main page includes:

- Output policy: Stereo speakers, Binaural, 5.1, 7.1, 5.1.2, 5.1.4, 7.1.2,
  or 7.1.4
- Dialnorm: Calibrated (recommended) or Unity/Compatibility
- Binaural HRTF: SADIE II D1/KU100, SADIE II D2/KEMAR, or Custom SOFA
- Binaural virtual layout: 7.1.4 or experimental 9.1.6

Menu changes are drafts. **Save** stores them in
`bin/portable_config/openjoc-settings.json` for the next file; it does not
reload or seek the current file. **Cancel** discards the draft. Esc or a click
outside the panel closes it while keeping the draft available if you reopen the
panel. The settings panel separates the pending choices from read-only live
mpv decoder/channel properties. Before decoder creation, the
menu merges options into the per-file decoder-option map when a file has an
E-AC-3 audio track. JOC is not yet known at that point, so plain or unselected
E-AC-3 tracks may also receive the options. Because the map is file-local, it
may reach other audio decoders in a mixed-track file and cause unsupported
option warnings. The menu does not select or force `libopenjoc`, change `ad` or
`aid`, or touch `audio-channels`. Files without E-AC-3 bypass this hook. The
menu leaves current playback in place and does not seek or reload it, which is
important for forward-only raw JOC streams.

For Custom SOFA, **Set path…** opens a text prompt; enter an existing local
file path there. The
loader still validates the supported `SimpleFreeFieldHRIR` subset when the
decoder opens; see [Binaural and SOFA](binaural-sofa.md).

If a previous bundle saved settings at `config/openjoc-settings.json`, the
menu reads that file as a migration fallback when the new portable-config file
is absent. It leaves the old file untouched; the next explicit Save writes to
`bin/portable_config/openjoc-settings.json`.

## Output and hardware

Output policy selects the OpenJOC renderer target. Binaural is explicit and
produces two-channel headphone output; a two-channel speaker render is a
separate choice. mpv keeps its normal audio output mapping, which may adapt the
renderer output to the active AO. The menu does not change `audio-channels`,
detect headphones, or change how ordinary media is routed. Configure mpv's
normal channel map separately if you need an exact hardware target.

Settings rows show the pending/saved selection; separate read-only rows report
mpv's current decoder and channel properties, not live JOC metadata or
diagnostics. LAV's post-render output-gain slider is not implemented in the mpv
decoder bridge. mpv's normal volume control is separate and is not an OpenJOC
output gain stage. The LAV live JOC Stream page is also not available in the
current mpv integration.

The [mpv integration guide](https://github.com/chyinan/OpenJOC/blob/master/docs/integration/MPV.md)
documents command-line profiles, exact channel maps, decoder options, and
verification boundaries.
