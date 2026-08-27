# Integrations

The adapters own transport, host lifecycle, and output negotiation. OpenJOC owns E-AC-3/JOC decode, scene construction, spatial rendering, output semantics, latency, and drain state.

The repository keeps adapter-specific contracts in their natural locations rather than copying them into a second manually maintained specification. Use these links for the current implementation detail:

| Adapter | Canonical repository documentation |
| --- | --- |
| FFmpeg external bridge | [FFMPEG.md](https://github.com/chyinan/OpenJOC/blob/master/docs/integration/FFMPEG.md) |
| Native FFmpeg `libopenjoc` wrapper | [FFMPEG_NATIVE.md](https://github.com/chyinan/OpenJOC/blob/master/docs/integration/FFMPEG_NATIVE.md) |
| GStreamer | [GSTREAMER.md](https://github.com/chyinan/OpenJOC/blob/master/docs/integration/GSTREAMER.md) |
| mpv | [MPV.md](https://github.com/chyinan/OpenJOC/blob/master/docs/integration/MPV.md) |
| Player bundles | [PLAYER_PACKAGING.md](https://github.com/chyinan/OpenJOC/blob/master/docs/integration/PLAYER_PACKAGING.md) |
| Ecosystem packages | [ECOSYSTEM_PACKAGING.md](https://github.com/chyinan/OpenJOC/blob/master/docs/integration/ECOSYSTEM_PACKAGING.md) |
| Windows DirectShow/LAV | [Windows LAV / PotPlayer](../using/windows-lav-potplayer.md) |

Stock FFmpeg and upstream mpv are not modified by installing OpenJOC. Project-provided patched builds are separate products with their own corresponding-source and third-party notice obligations.
