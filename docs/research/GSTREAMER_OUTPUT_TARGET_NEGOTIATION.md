# GStreamer output-target negotiation audit

This note records the framework boundary used by the OpenJOC output adapter.
The audit targets GStreamer 1.28.x with the stable `gstreamer-rs` 0.24 API
baseline used by this repository.

## Decoder negotiation

`GstAudioDecoder` calls the subclass lifecycle in `start`, `set_format`, and
`handle_frame`. A subclass supplies its fixed output representation with
`gst_audio_decoder_set_output_format()` and then calls
`gst_audio_decoder_negotiate()`. The base class provides
`gst_audio_decoder_proxy_getcaps()`, which propagates downstream constraints
back toward the decoder's source pad.

The OpenJOC adapter therefore does not attempt to choose an output target in
the compressed-input classifier. `openjocclassify` decides only whether the
input is admitted JOC. An opt-in `render-mode=auto` decoder queries its source
pad peer once, before the first AU is rendered, and uses only a single
semantically fixed `audio/x-raw` structure with a recognized channel mask.
Explicit `render-mode=binaural` and `render-mode=speaker` remain independent of
that query.

## Caps and channel semantics

GStreamer raw-audio channel identity is represented by `GstAudioChannelPosition`
and the `channel-mask` field. A channel count without positions is insufficient
for an immersive target: 12 channels are not evidence of 7.1.4, and 24
channels are not evidence of 22.2. The two-channel no-mask convention is
treated as physical 2.0 by OpenJOC auto mode; it is never treated as headphone
intent.

The adapter maps every currently supported OpenJOC speaker preset to the
corresponding GStreamer positions. OpenJOC's renderer-owned channel order is
kept as the semantic source order. Where a preset is not already in GStreamer's
canonical order, the adapter reorders samples at the transport boundary only.
It does not apply gains, HRTF, mixing, or downmixing there.

## `audioconvert` and sinks

`audioconvert` supports format conversion and channel transformations. It can
also reorder or mix channels when its negotiated sink/src caps require it.
Consequently, a caps-driven exact-target test uses a direct capsfilter and
fakesink/appsink. A player may still use `audioconvert` for harmless sample
representation conversion, but an exact physical layout must be accepted by
the downstream chain without a semantic channel rematrix.

The generic audio sink pad exposes the caps it can accept, but that is not a
portable application-level statement that the selected device is headphones
or that a broad multichannel range is the installed physical layout. Device
enumeration and headphone policy stay in the application/framework adapter.
OpenJOC does not inspect CoreAudio, WASAPI, device names, or platform spatial
renderers.

## Player integration and lifecycle

`playbin` documents the `element-setup` signal for configuring elements created
inside its sub-bins, and also accepts an application-created `audio-sink` or
sink bin. The player should use `element-setup` (or its equivalent for a
custom/decodebin graph) to set `openjocdec`'s existing properties. `gst-launch`
cannot reliably configure an internally-created decoder, so explicit pipelines
are provided for deterministic tests.

Target changes are setup-time policy changes. The adapter flushes the OpenJOC
session on format/flush boundaries; live device switching is intentionally not
implemented. A player must stop/reconfigure/restart or create a new stream
negotiation. This avoids carrying HRTF or native-speaker state across a target
change and keeps seek, EOS, drain, and discontinuity handling in the existing
decoder lifecycle.

Primary references:

- [GstAudioDecoder API](https://gstreamer.freedesktop.org/documentation/audio/gstaudiodecoder.html)
- [GStreamer 1.28 GstAudioDecoder source](https://gitlab.freedesktop.org/gstreamer/gstreamer/-/blob/1.28/subprojects/gst-plugins-base/gst-libs/gst/audio/gstaudiodecoder.c)
- [GStreamer caps negotiation design](https://gstreamer.freedesktop.org/documentation/additional/design/negotiation.html)
- [GstAudioChannelPosition](https://gstreamer.freedesktop.org/documentation/audio/gstaudiochannels.html)
- [audioconvert](https://gstreamer.freedesktop.org/documentation/audioconvert/)
- [playbin](https://gstreamer.freedesktop.org/documentation/playback/playbin.html)
