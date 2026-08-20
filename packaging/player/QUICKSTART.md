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
launcher uses only the bundle's config directory and relative runtime paths;
it does not require Rust, FFmpeg, OpenJOC, MSYS2, Homebrew, or a source tree.

For build provenance, dependency inventory, licenses, and verification results,
see `BUILD_INFO.txt`, `BUILD_INFO.json`, `DEPENDENCIES.json`, and
`THIRD_PARTY_NOTICES.txt` in this bundle.
