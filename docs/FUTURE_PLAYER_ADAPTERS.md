# Player adapter assessment

The GStreamer integration, external FFmpeg-facing bridge, native FFmpeg source
wrapper, and source-level mpv player patchset are implemented with documented
constraints.

| Adapter | Input boundary | Output boundary | Is the C ABI sufficient? | Thin work still required |
| --- | --- | --- | --- | --- |
| FFmpeg external | arbitrary demuxed packet boundaries plus `AVStream.time_base` | owned packed-float `AVFrame` | Implemented | stock FFmpeg registration is intentionally absent |
| GStreamer | arbitrary `GstBuffer` boundaries | negotiated interleaved float buffers | Implemented | no player work required |
| FFmpeg native | libavcodec packet input | ordinary libavcodec packed-float frame output | Implemented through C ABI 1.3 packet-stream/classifier handles | custom patched FFmpeg build; native decoder remains explicitly named |
| mpv/player | normal libavcodec decoder path | mpv audio frame | Implemented through the optional source patchset in `integrations/mpv/` | broader long-run, physical-hardware, Linux/Windows, and packaged-player acceptance remains |
| DirectShow/LAV-style Windows | media sample from the upstream splitter/decoder graph | negotiated `IMediaSample` multichannel float output | Yes for a native C wrapper | COM filter, media-type negotiation, allocator, graph seeking/discontinuity plumbing |

Recommended next phase after mpv hardening: **PLAYER_DISTRIBUTION_AND_PACKAGING**.
The source patchset positively identifies JOC and selects `libopenjoc` without
duplicating packet assembly, channel mapping, timing, or spatial DSP inside
mpv; the remaining work is reproducible player/dependency packaging and wider
platform acceptance.
