# Future player adapter assessment

No framework plugin is implemented in the integration-foundation milestone.

| Adapter | Input boundary | Output boundary | Is the C ABI sufficient? | Thin work still required |
| --- | --- | --- | --- | --- |
| FFmpeg | `AVPacket` containing one complete demuxed E-AC-3 JOC AU | `AVFrame` float/interleaved or planar conversion owned by the codec wrapper | Yes for a first prototype | codec registration, AVCodecContext lifecycle, FFmpeg sample-format negotiation, timestamp/time-base translation |
| GStreamer | `GstBuffer`/adapter output containing one complete AU | `GstAudioBuffer` with negotiated multichannel float caps | Yes | element state machine, caps negotiation, segment/flush events, buffer PTS conversion |
| mpv/player | demuxed packet callback or a small libavcodec-side bridge | player audio-frame callback | Yes | player-specific callback and build integration; no file demux in OpenJOC |
| DirectShow/LAV-style Windows | media sample from the upstream splitter/decoder graph | negotiated `IMediaSample` multichannel float output | Yes for a native C wrapper | COM filter, media-type negotiation, allocator, graph seeking/discontinuity plumbing |

Recommended first prototype: **GStreamer**. It has an explicit buffer/caps/
segment model that exercises packet ownership, PTS, drain, flush, format change,
and multichannel float output without requiring a full desktop player. The
adapter can remain thin: split upstream buffers at complete AU boundaries,
forward them to one `openjoc_decoder` handle, drain output before accepting
more input, and translate `GstSegment`/flush events to the ABI's timestamp and
reset calls. FFmpeg is an equally suitable second prototype once the GStreamer
state-machine behavior is proven.
