# Player adapter assessment

The GStreamer integration and external FFmpeg-facing bridge are implemented.

| Adapter | Input boundary | Output boundary | Is the C ABI sufficient? | Thin work still required |
| --- | --- | --- | --- | --- |
| FFmpeg external | arbitrary demuxed packet boundaries plus `AVStream.time_base` | owned packed-float `AVFrame` | Implemented | stock FFmpeg registration is intentionally absent |
| GStreamer | arbitrary `GstBuffer` boundaries | negotiated interleaved float buffers | Implemented | no player work required |
| FFmpeg native | libavcodec packet input | ordinary libavcodec frame output | External bridge proves the internals | FFmpeg source wrapper, codec selection/registration and AVOptions |
| mpv/player | normal libavcodec decoder path | mpv audio frame | External C/Rust APIs alone are insufficient for normal mpv selection | native FFmpeg wrapper first, then mpv selection/UI/build work |
| DirectShow/LAV-style Windows | media sample from the upstream splitter/decoder graph | negotiated `IMediaSample` multichannel float output | Yes for a native C wrapper | COM filter, media-type negotiation, allocator, graph seeking/discontinuity plumbing |

Recommended next integration: **FFMPEG_NATIVE_LIBAVCODEC_WRAPPER**. The
external bridge has already proven libavformat packet transport, AU assembly,
rational timing, owned AVFrames, and channel layouts. A native wrapper should
now make that behavior available through the decoder lifecycle mpv already
uses. mpv integration should follow rather than duplicating the native codec
lifecycle inside one player.
