# Player adapter assessment

The GStreamer integration, external FFmpeg-facing bridge, native FFmpeg source
wrapper, source-level mpv player patchset, and local/CI player packaging route
are implemented with documented constraints.

| Adapter | Input boundary | Output boundary | Is the C ABI sufficient? | Thin work still required |
| --- | --- | --- | --- | --- |
| FFmpeg external | arbitrary demuxed packet boundaries plus `AVStream.time_base` | owned packed-float `AVFrame` | Implemented | stock FFmpeg registration is intentionally absent |
| GStreamer | arbitrary `GstBuffer` boundaries | negotiated interleaved float buffers | Implemented | no player work required |
| FFmpeg native | libavcodec packet input | ordinary libavcodec packed-float frame output | Implemented through C ABI 1.4 packet-stream/classifier handles | custom patched FFmpeg build; native decoder remains explicitly named; arbitrary renderer geometry is not promised through FFmpeg channel negotiation |
| mpv/player | normal libavcodec decoder path | mpv audio frame | Implemented through the optional source patchset in `integrations/mpv/` | broader long-run hardware and target-host acceptance remains |
| DirectShow/LAV-style Windows | media sample from the upstream splitter/decoder graph | negotiated `IMediaSample` multichannel float output | Yes for a native C wrapper | COM filter, media-type negotiation, allocator, graph seeking/discontinuity plumbing |

The player distribution phase is implemented with constraints. The source
patchset positively identifies JOC and selects `libopenjoc` without duplicating
packet assembly, channel mapping, timing, or spatial DSP inside mpv. Remaining
work is target-host qualification and later release hardening, not a new audio
frontend design.
