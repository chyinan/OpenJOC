# Versioned C ABI

The distributable header is
[`crates/openjoc-capi/include/openjoc.h`](../crates/openjoc-capi/include/openjoc.h).
It is manually maintained, deterministic, and compiled in both C and C++ by
the repository smoke script. The crate builds `rlib`, static-library, and
dynamic-library targets through Cargo.

## ABI policy

The ABI is `1.0-experimental`, independent of the OpenJOC package version.
Major changes may break layout or ownership rules and require an ABI-major
increment. Minor additions must append fields or functions and preserve the
meaning of existing fields. Configuration, PCM-frame, and output-info structs
contain `struct_size`; callers must initialize them and producers must reject a
smaller size. `openjoc_get_abi_version()` returns `(major << 16) | minor`.

Experimental means the C surface may evolve during 0.7 integration work. It
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

Semantic labels are available through `openjoc_decoder_get_channel_label` and
the output/frame descriptors. The canonical PCM sample format value is `1`
(interleaved float32).

## Failure containment

Every exported operation contains Rust panics before returning. No Rust panic,
Rust error object, or Rust struct layout crosses the ABI. `last_error` is
owned by the decoder instance and is not process-global. Null arguments,
invalid struct sizes, malformed packets, unsupported configurations, format
changes, and render failures return numeric status codes.

The public C header has no third-party generated material and is distributed
under the repository Apache-2.0 license.
