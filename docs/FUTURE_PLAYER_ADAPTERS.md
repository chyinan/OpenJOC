# Player adapter assessment

The GStreamer integration, external FFmpeg-facing bridge, and experimental
native FFmpeg source wrapper are implemented.

| Adapter | Input boundary | Output boundary | Is the C ABI sufficient? | Thin work still required |
| --- | --- | --- | --- | --- |
| FFmpeg external | arbitrary demuxed packet boundaries plus `AVStream.time_base` | owned packed-float `AVFrame` | Implemented | stock FFmpeg registration is intentionally absent |
| GStreamer | arbitrary `GstBuffer` boundaries | negotiated interleaved float buffers | Implemented | no player work required |
| FFmpeg native | libavcodec packet input | ordinary libavcodec packed-float frame output | Implemented through C ABI 1.2 packet-stream handle | custom patched FFmpeg build; application must select `libopenjoc` explicitly after positive JOC probing |
| mpv/player | normal libavcodec decoder path | mpv audio frame | The native wrapper now supplies the required decoder boundary | positive JOC probe, named-decoder selection, build/configuration and user-facing render-target policy |
| DirectShow/LAV-style Windows | media sample from the upstream splitter/decoder graph | negotiated `IMediaSample` multichannel float output | Yes for a native C wrapper | COM filter, media-type negotiation, allocator, graph seeking/discontinuity plumbing |

Recommended next integration: **MPV_INTEGRATION**. The native wrapper now makes
OpenJOC available through the ordinary decoder lifecycle mpv already uses.
Player work should positively identify JOC, explicitly select `libopenjoc`,
and expose render-target policy without duplicating packet assembly, channel
mapping, timing, or spatial DSP inside mpv.
