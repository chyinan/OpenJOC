# Versioned C ABI

The distributable header is
[`crates/openjoc-capi/include/openjoc.h`](../crates/openjoc-capi/include/openjoc.h).
It is manually maintained, deterministic, and compiled in both C and C++ by
the repository smoke script. The crate builds `rlib`, static-library, and
dynamic-library targets through Cargo. Platform release archives expose the
consumer-facing subset as `include/openjoc.h` plus
`libopenjoc_capi.a`/`libopenjoc_capi.dylib` on macOS,
`openjoc_capi.lib`/`openjoc_capi.dll.lib`/`openjoc_capi.dll` on Windows, and
the corresponding `.a`/`.so` files on Linux. The `.rlib` is an internal Rust
artifact, not the primary C consumer library.

## ABI policy

The ABI is `1.3-experimental`, independent of the OpenJOC package version.
Major changes may break layout or ownership rules and require an ABI-major
increment. Minor additions must append fields or functions and preserve the
meaning of existing fields. Configuration, PCM-frame, and output-info structs
contain `struct_size`; callers must initialize them and producers must reject a
smaller size. The `dialnorm_mode` field was appended in ABI minor 1. A caller
presenting the ABI 1.0 configuration size is accepted and receives
`OPENJOC_DIALNORM_DEFAULT`. ABI 1.2 appends functions and statuses without
changing any existing structure layout. `openjoc_get_abi_version()` returns
`(major << 16) | minor`.

Experimental means the C surface may evolve during OpenJOC 0.x integration work. It
does not mean that existing decoder correctness claims are withdrawn.

## Ownership and calls

```c
openjoc_decoder_config config;
openjoc_decoder_config_init(&config);

openjoc_decoder *decoder = NULL;
openjoc_decoder_create(&config, &decoder);
openjoc_decoder_send_packet(decoder, bytes, byte_count,
                            OPENJOC_NO_PTS, 0);

openjoc_pcm_frame frame;
openjoc_pcm_frame_init(&frame);
while (openjoc_decoder_receive_frame(decoder, &frame) ==
       OPENJOC_STATUS_FRAME_AVAILABLE) {
    /* frame.data is interleaved float32, valid until the next send/receive/reset */
}
openjoc_decoder_drain(decoder);
openjoc_decoder_destroy(decoder);
```

The decoder is an opaque handle. Packet memory is borrowed only during
`openjoc_decoder_send_packet`; it is never retained. PCM memory is owned by
the decoder and remains valid until the next send, receive, flush, reset, or
destroy on that handle. Applications that need longer ownership copy the
frame. Multiple handles are independent.

ABI 1.2 also provides `openjoc_stream_decoder`, a framework-neutral handle for
adapters whose packet boundaries are not complete access-unit boundaries. Its
`openjoc_stream_decoder_send_chunk()` call accepts arbitrary compressed bytes,
an optional 1/48000 sample-domain PTS, and the existing discontinuity/preroll
flags. The handle reuses the external FFmpeg bridge's single 131,072-byte-
bounded assembler, positive JOC admission, timestamp model, output queue,
semantic channel permutation, and lazy `OpenJocSession` creation. It supports
fragmented AUs and multiple AUs per chunk without exposing any framework type.

`openjoc_stream_decoder_receive_frame()` returns packed float PCM in the order
reported by its semantic channel labels. Output semantics, the exact shared
configuration descriptor/fingerprint, and current bounded staging size are
available before or during decoding. `OPENJOC_STATUS_NOT_JOC` distinguishes a
positive ordinary-E-AC-3 rejection; out-of-memory and external-library
categories have dedicated numeric statuses for host error mapping.

ABI 1.3 adds `openjoc_classifier`, a decode-free, framework-neutral compressed
stream probe. `openjoc_classifier_send_chunk()` shares the bounded access-unit
parser and positive JOC admission rules but never creates an OpenJOC render
session or emits PCM. `openjoc_classifier_finish()` closes the probe so a final
complete one-AU stream can be classified without a following syncframe. The
output is one of `UNKNOWN`, `CONFIRMED_JOC`, `CONFIRMED_NON_JOC`, or
`INVALID_OR_UNSUPPORTED`; the staged and inspected-byte accessors expose
bounded probe accounting. This is intended for players that must choose a
decoder before sending the first packet to a renderer.

ABI 1.3 adds `openjoc_classifier`, a decode-free, framework-neutral compressed
stream probe. `openjoc_classifier_send_chunk()` shares the bounded access-unit
parser and positive JOC admission rules but never creates an OpenJOC render
session or emits PCM. Its output is one of `UNKNOWN`, `CONFIRMED_JOC`,
`CONFIRMED_NON_JOC`, or `INVALID_OR_UNSUPPORTED`; the staged and inspected-byte
accessors expose bounded probe accounting. This is intended for players that
must choose a decoder before sending the first packet to a renderer.

Semantic labels are available through `openjoc_decoder_get_channel_label` and
the output/frame descriptors. The canonical PCM sample format value is `1`
(interleaved float32).

Set `render_mode` to `OPENJOC_RENDER_BINAURAL` with a null/zero `sofa_data` /
`sofa_size` pair to use the bundled offline SADIE II generic HRTF. Supplying a
non-empty SOFA buffer selects the existing strict user-dataset path. The
virtual layout defaults to the configured speaker layout when
`virtual_layout` is null. A native 22.2 speaker session is selected with
`speaker_layout = "22.2"`; its output exposes 24 ordered semantic labels,
including `LFE1` and `LFE2`.

The C adapter inherits the shared session's calibrated Default E-AC-3 dialnorm
program calibration unless `dialnorm_mode` is explicitly set to
`OPENJOC_DIALNORM_DIGITAL` or `OPENJOC_DIALNORM_ANALOG`. Default is recommended
for normal playback/decoding. Digital explicitly selects encoded digital
program-level calibration. Analog uses unity dialnorm gain and is an advanced
compatibility/diagnostic policy, not a recommended louder-output or mastering
mode. Dialnorm is metadata-derived and separate from the existing DRC fields;
DRC changes encoded dynamic-range behavior. FinalLinkedGain is internal
renderer headroom behavior, not a user mastering control.

The C ABI is a streaming PCM interface and does not perform file-export peak
normalization or spool a complete program for a file-level transform.
Applications may apply their own final static gain policy after receiving PCM.
The CLI's
`--normalize-peak` is an offline file-output convenience: it normalizes the
final rendered file to a requested sample peak after decoder and renderer
processing, and is not dialnorm, DRC, a limiter, compressor, LUFS, or true-peak
normalization.

## Failure containment

Every exported operation contains Rust panics before returning. No Rust panic,
Rust error object, or Rust struct layout crosses the ABI. `last_error` is
owned by the decoder instance and is not process-global. Null arguments,
invalid struct sizes, malformed packets, unsupported configurations, format
changes, and render failures return numeric status codes.

The public C header has no third-party generated material and is distributed
under the repository Apache-2.0 license.
