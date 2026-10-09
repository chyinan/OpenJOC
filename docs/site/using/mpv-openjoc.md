# mpv OpenJOC Player Bundle

The OpenJOC Player Bundle is a project-provided, patched mpv/FFmpeg build. It
is not an official upstream mpv or FFmpeg release. Extract the bundle and start
`bin/openjoc-mpv` (or `bin/openjoc-mpv.cmd` on Windows) with a media file.

```text
bin/openjoc-mpv path/to/media
```

Confirmed JOC is selected automatically by the patched player. Ordinary
E-AC-3 remains on mpv's stock `eac3` decoder. Compressed passthrough can be
requested with `--audio-spdif=eac3`; that bypasses OpenJOC rendering.

## Open the settings menu

Press `Ctrl+Alt+J` in the Player Bundle. Use Up/Down to choose a row, Left/Right
to change a value, Enter to select, and Esc to close. The main page includes:

- Output policy: Stereo speakers, Binaural, 5.1, 7.1, 5.1.2, 5.1.4, 7.1.2,
  or 7.1.4
- Dialnorm: Calibrated (recommended) or Unity/Compatibility
- Binaural HRTF: SADIE II D1/KU100, SADIE II D2/KEMAR, or Custom SOFA
- Binaural virtual layout: 7.1.4 or experimental 9.1.6

Menu changes are drafts. Choose **Save selection for next OpenJOC file** to
save the settings in the bundle config directory. Before decoder creation, the
menu merges options into the per-file decoder-option map when a file has an
E-AC-3 audio track. JOC is not yet known at that point, so plain or unselected
E-AC-3 tracks may also receive the options. Because the map is file-local, it
may reach other audio decoders in a mixed-track file and cause unsupported
option warnings. The menu does not select or force `libopenjoc`, change `ad` or
`aid`, or touch `audio-channels`. Files without E-AC-3 bypass this hook. The
menu leaves current playback in place and does not seek or reload it, which is
important for forward-only raw JOC streams.

For Custom SOFA, enter an existing local file path in the text prompt. The
loader still validates the supported `SimpleFreeFieldHRIR` subset when the
decoder opens; see [Binaural and SOFA](binaural-sofa.md).

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
