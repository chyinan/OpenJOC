# OpenJOC-enabled mpv

This archive is distributed as an OpenJOC Player Bundle; its user-facing
launcher is intentionally named `openjoc-mpv` because the underlying player is
mpv.

This is an OpenJOC-enabled mpv build. It is not an official mpv or FFmpeg
release. Extract the archive anywhere and run the packaged launcher from this
directory:

```text
bin/openjoc-mpv path/to/media
```

On Windows, the GUI can also be started directly:

```text
bin/mpv.exe "path/to/media"
```

Its adjacent `bin/portable_config` contains the same OpenJOC menu and profiles
used by the launcher, so `Ctrl+Alt+J` works without a wrapper or a change to
your regular mpv settings. The launcher remains available as
`bin/openjoc-mpv.cmd` when you want a console window.

This automatic lookup applies when `MPV_HOME` is not set. If you already use
`MPV_HOME`, mpv honors that override instead of the adjacent `portable_config`,
so the direct launch may not load this bundle's menu. Keep your environment
unchanged and launch with `bin/openjoc-mpv.cmd`; it explicitly selects the
bundle config directory.

Ordinary media remains ordinary mpv media. The patched player positively
classifies E-AC-3 packets: ordinary E-AC-3 uses stock `eac3`, while confirmed
JOC selects `libopenjoc` automatically. For engineering control, `--ad=eac3`
and `--ad=libopenjoc` remain available. `--audio-spdif=eac3` requests
compressed passthrough and bypasses OpenJOC rendering.

Output rendering is explicit. The bundle does not infer headphones from a
two-channel device and does not force binaural output:

```text
bin/openjoc-mpv --profile=openjoc-headphones media-with-joc
bin/openjoc-mpv --profile=openjoc-stereo media-with-joc
bin/openjoc-mpv --profile=openjoc-51 media-with-joc
bin/openjoc-mpv --profile=openjoc-714 media-with-joc
bin/openjoc-mpv --profile=openjoc-916 media-with-joc
bin/openjoc-mpv --profile=openjoc-222 --ao=null media-with-joc
```

`openjoc-headphones` means binaural output from a virtual 7.1.4 scene, two
output channels, and the built-in SADIE II D1 KU100 HRTF. The physical profiles
mean native speaker layouts with no HRTF; the device must accept the requested
channel map. `--ao=null` is useful for deterministic 7.1.4/9.1.6/22.2 checks
when hardware is unavailable.

The built-in HRTF is embedded in the OpenJOC library and works offline. The
bundle launchers use `bin/portable_config` and relative runtime paths; on
Windows, direct `mpv.exe` launch auto-discovers that directory. The bundle does
not require Rust, FFmpeg, OpenJOC, MSYS2, Homebrew, or a source tree at runtime.

Press `Ctrl+Alt+J` while the packaged player is open to choose OpenJOC output,
Dialnorm, HRTF, or virtual-layout settings. Use arrow keys to navigate and
Enter to select. Choose **Save selection for next OpenJOC file** to persist the
menu choices. Before decoder creation, the menu merges options into the
per-file decoder-option map when an E-AC-3 audio track exists. JOC is not yet
known at that point, so plain or unselected E-AC-3 tracks may also receive the
options. Since the map is file-local, other audio decoders in a mixed-track
file may also receive them and log unsupported-option warnings. The current
file is left alone; the menu does not select or force OpenJOC or change `ad`,
`aid`, or `audio-channels`. mpv keeps its normal audio output
mapping, so it may adapt the selected renderer layout to the active audio
output. Custom SOFA uses a text prompt for an existing local path. LAV's
post-render output gain and live JOC diagnostics are not part of the mpv menu;
normal mpv volume remains a separate control.

For build provenance, dependency inventory, licenses, and verification results,
see `BUILD_INFO.txt`, `BUILD_INFO.json`, `DEPENDENCIES.json`, and
`THIRD_PARTY_NOTICES.txt` in this bundle.
