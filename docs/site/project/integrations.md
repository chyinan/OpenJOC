# Integrations

The adapters own transport, host lifecycle, and output negotiation. OpenJOC owns E-AC-3/JOC decode, scene construction, spatial rendering, output semantics, latency, and drain state.

The repository keeps adapter-specific contracts in their natural locations rather than copying them into a second manually maintained specification. Use these links for the current implementation detail:

| Adapter | Canonical repository documentation |
| --- | --- |
| FFmpeg external bridge | [FFMPEG.md](https://github.com/chyinan/OpenJOC/blob/master/docs/integration/FFMPEG.md) |
| Native FFmpeg `libopenjoc` wrapper | [FFMPEG_NATIVE.md](https://github.com/chyinan/OpenJOC/blob/master/docs/integration/FFMPEG_NATIVE.md) |
| GStreamer | [GSTREAMER.md](https://github.com/chyinan/OpenJOC/blob/master/docs/integration/GSTREAMER.md) |
| mpv | [MPV.md](https://github.com/chyinan/OpenJOC/blob/master/docs/integration/MPV.md) |
| mpv user guide | [OpenJOC Player Bundle](../using/mpv-openjoc.md) |
| Player bundles | [PLAYER_PACKAGING.md](https://github.com/chyinan/OpenJOC/blob/master/docs/integration/PLAYER_PACKAGING.md) |
| Ecosystem packages | [ECOSYSTEM_PACKAGING.md](https://github.com/chyinan/OpenJOC/blob/master/docs/integration/ECOSYSTEM_PACKAGING.md) |
| Windows DirectShow/LAV | [Windows LAV / PotPlayer](../using/windows-lav-potplayer.md) |

Stock FFmpeg and upstream mpv are not modified by installing OpenJOC. Project-provided patched builds are separate products with their own corresponding-source and third-party notice obligations.

## WebAssembly bridge

`openjoc-wasm` exposes the browser decoder ABI. Decode/render timing means,
P95, maxima, and realtime factor cover the most recent 4096 decoded access
units (about 131 seconds at 48 kHz). Access-unit, output-frame, and sample
counters cover the whole session. Reset clears both. Repeated metric reads
reuse a cached summary until another access unit is recorded.

The WASM CI workflow builds both embedded and external HRTF configurations
for `wasm32-unknown-unknown`. Its Node.js smoke test instantiates the actual
module and checks Stereo, D1/D2 binaural, Custom SOFA, fragmented input,
reset, drain, PCM ownership, and invalid-input rejection. Browser audio-device
scheduling remains a separate host concern.
