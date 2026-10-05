# Versioned C ABI

The distributable header is the [canonical `openjoc.h` header](https://github.com/chyinan/OpenJOC/blob/master/crates/openjoc-capi/include/openjoc.h).
It is manually maintained, deterministic, and compiled in both C and C++ by
the repository smoke script. The crate builds `rlib`, static-library, and
dynamic-library targets through Cargo. Platform release archives expose the
consumer-facing subset as `include/openjoc.h` plus
`libopenjoc_capi.a`/`libopenjoc_capi.dylib` on macOS,
`openjoc_capi.lib`/`openjoc_capi.dll.lib`/`openjoc_capi.dll` on Windows, and
the corresponding `.a`/`.so` files on Linux. The `.rlib` is an internal Rust
artifact, not the primary C consumer library.

## ABI policy

The ABI is `1.7-experimental`, independent of the OpenJOC package version.
Major changes may break layout or ownership rules and require an ABI-major
increment. Minor additions must append fields or functions and preserve the
meaning of existing fields. Configuration, PCM-frame, and output-info structs
contain `struct_size`; callers must initialize them and producers must reject a
smaller size. The `dialnorm_mode` field was appended in ABI minor 1. A caller
presenting the ABI 1.0 configuration size is accepted and receives
`OPENJOC_DIALNORM_DEFAULT`. ABI 1.2 appends functions and statuses without
changing any existing structure layout. `openjoc_get_abi_version()` returns
`(major << 16) | minor`.

ABI 1.4 appends `custom_speaker_layout` to `openjoc_decoder_config`. Set it to
an in-memory `openjoc_custom_speaker_layout` whose ordered
`openjoc_custom_speaker` array contains finite azimuth/elevation degrees and a
`OPENJOC_SPEAKER_FULL_RANGE` or `OPENJOC_SPEAKER_LFE` role. The descriptor and
all strings are borrowed only during `openjoc_decoder_create`; the decoder
copies the validated layout and reports the same order through output labels.
Existing callers leave the field null and retain preset behavior. The custom
layout contract, coordinate convention, validation limits, and WAV/CAF
metadata boundary are documented in
[custom speaker layouts](../using/custom-speaker-layouts.md).

The stream API has a narrower transport boundary than direct `openjoc_decoder`:
`openjoc_stream_decoder_create` requires custom names with existing OpenJOC-to-FFmpeg
channel mappings, unique mapped identities, and matching LFE/full-range roles.
Representable custom definitions preserve their order and override the preset
field; unsupported names, alias collisions, or role mismatches return
`OPENJOC_STATUS_INVALID_ARGUMENT` at creation, before any audio is submitted.
The stream reports FFmpeg channel labels and does not transport custom angles.
See the [custom-layout transport boundary](rust-api.md#custom-layout-transport-boundary).

`openjoc_decoder_config_init()` remains the legacy-safe ABI 1.3 prefix
initializer: it never writes fields appended in ABIs 1.4 or 1.6, so an ABI 1.3
caller may link it against a newer library without a struct over-write. ABI
1.4 callers use `openjoc_decoder_config_init_v1_4()` for the custom-layout
field. ABI 1.6 callers use `openjoc_decoder_config_init_v1_6()` for the v1.6
prefix and explicit HRTF selection. ABI 1.7 callers use
`openjoc_decoder_config_init_v1_7()`. The v1.7 orientation field follows a
reserved v1.6 alignment word, so legacy trailing padding cannot accidentally
enable orientation mode; the v1.6 initializer writes exactly the v1.6 prefix.

ABI 1.5 adds the read-only `openjoc_live_inspection_snapshot` surface for
`openjoc_stream_decoder`. `openjoc_stream_decoder_get_live_inspection_snapshot`
returns bounded semantic fields observed by the same in-band decoder path:
profile/carriers, programme topology, block partition, dependent IDs, LFE and
JOC ownership, programme layout, coded object/complexity values, EMDF payload IDs, dynamic-scene
observation, malformed and AU counters, timestamps, and an observation epoch.
The snapshot explicitly reports `live_decode_snapshot`, `live_decode`, and
`partial`/`complete_continuous` coverage. A seek/flush/reset starts a new epoch;
EOS is not full-stream proof unless the session began at sample PTS zero without
a discontinuity. `openjoc_stream_decoder_copy_live_inspection_json` performs
bounded JSON serialization only when the caller requests it, so the decode
observer does not serialize JSON on the audio path.

ABI 1.6 appends `hrtf_preset` to `openjoc_decoder_config`. Value `0` selects
SADIE II D1/KU100, the default; value `1` selects SADIE II D2/KEMAR. Callers
with an older `struct_size` continue to use D1. The retired Aachen value `2`
is accepted as D1 for compatibility with earlier callers.

ABI 1.7 appends `listener_orientation_pull_samples`. Zero preserves the
existing fixed-pose binaural path; `1..=256` enables the experimental,
device-independent listener-orientation API and bounds samples per pulled
output block. This does not change CLI, WASM, DirectShow/LAV, or other default
rendering behavior.

Experimental means the C surface may evolve during OpenJOC 0.x integration work. It
does not mean that existing decoder correctness claims are withdrawn.

## Ownership and calls

```c
openjoc_decoder_config config;
openjoc_decoder_config_init_v1_7(&config);

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

For `openjoc_decoder_send_packet`, `pts_samples` describes the packet's first
sample; `OPENJOC_NO_PTS` omits that packet's timestamp. The first provided PTS
anchors the segment even if earlier packets were untimestamped, subtracting
samples already decoded to obtain the origin. Previously returned or copied
frame timestamps are unchanged; later output, including delayed earlier PCM,
uses that origin. Further provided PTS must agree with sample-count
continuation. Unrepresentable origins or expected packet PTS are rejected
before decoding; an unrepresentable output-frame PTS returns a render error
rather than wrapping or clamping. Reset, flush, or discontinuity clears the
anchor. `INT64_MIN` is reserved for `OPENJOC_NO_PTS`: if a late anchor would
produce an actual output-frame PTS of `INT64_MIN`, receive returns
`OPENJOC_STATUS_RENDER_ERROR` without writing the output frame, and the handle
requires reset or flush before further decoding. Rust's `Option<i64>` has no
such sentinel restriction. This is the
complete-AU packet API; `openjoc_stream_decoder` retains the stricter
[packet-stream timestamp contract](https://github.com/chyinan/OpenJOC/blob/master/docs/integration/FFMPEG.md#timestamps).

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

Semantic labels are available through `openjoc_decoder_get_channel_label` and
the output/frame descriptors. The canonical PCM sample format value is `1`
(interleaved float32).

Set `render_mode` to `OPENJOC_RENDER_BINAURAL` with a null/zero `sofa_data` /
`sofa_size` pair to use the bundled offline HRTF selected by `hrtf_preset`
(`OPENJOC_HRTF_SADIE_D1_KU100` remains the default). Supplying a non-empty
SOFA buffer selects the existing strict user-dataset path. The
virtual layout defaults to the configured speaker layout when
`virtual_layout` is null. A native 22.2 speaker session is selected with
`speaker_layout = "22.2"`; its output exposes 24 ordered semantic labels,
including `LFE1` and `LFE2`.

### Experimental listener orientation

Set a nonzero ABI 1.7 pull limit only for a binaural session. The compressed
stream bridge also supports this option and still delays renderer admission
until a positive JOC access unit. Before handing a decoder to the render
thread, obtain its immutable preparation handle. Prepare poses on a worker
without reading or mutating the decoder:

```c
#include "openjoc.h"
#include <stdio.h>

int main(void) {
    openjoc_decoder_config config = {0};
    openjoc_decoder *decoder = NULL;
    openjoc_listener_orientation_preparer *preparer = NULL;
    openjoc_listener_orientation_update *update = NULL;
    openjoc_listener_orientation_retired *retired = NULL;
    openjoc_status status;
    int result = 1;

    status = openjoc_decoder_config_init_v1_7(&config);
    if (status != OPENJOC_STATUS_OK) goto cleanup;
    config.render_mode = OPENJOC_RENDER_BINAURAL;
    config.speaker_layout = "7.1.4";
    config.listener_orientation_pull_samples = 128;
    status = openjoc_decoder_create(&config, &decoder);
    if (status != OPENJOC_STATUS_OK) goto cleanup;

    status = openjoc_decoder_get_listener_orientation_preparer(decoder, &preparer);
    if (status != OPENJOC_STATUS_OK) goto cleanup;
    openjoc_listener_orientation_state state;
    status = openjoc_listener_orientation_state_init(&state);
    if (status != OPENJOC_STATUS_OK) goto cleanup;
    status = openjoc_decoder_get_listener_orientation_state(decoder, &state);
    if (status != OPENJOC_STATUS_OK) goto cleanup;

    openjoc_listener_orientation pose;
    status = openjoc_listener_orientation_init(&pose);
    if (status != OPENJOC_STATUS_OK) goto cleanup;
    pose.x = 0.0; pose.y = 0.0; pose.z = 0.0; pose.w = 1.0;
    status = openjoc_listener_orientation_prepare(
        preparer, &pose, state.stream_epoch, 1, &update);
    if (status != OPENJOC_STATUS_OK) {
        fprintf(stderr, "%s\n", openjoc_listener_orientation_preparer_last_error(preparer));
        goto cleanup;
    }
    uint64_t accepted, superseded;
    status = openjoc_decoder_apply_listener_orientation(
        decoder, &update, &accepted, &superseded, &retired);
    if (status != OPENJOC_STATUS_OK) {
        fprintf(stderr, "%s\n", openjoc_decoder_last_error(decoder));
        goto cleanup; /* On failure, update is still owned here. */
    }
    /* Success sets update to NULL and returns the same allocation as retired. */
    result = 0;

cleanup:
    if (update) openjoc_listener_orientation_update_destroy(update);
    if (retired) openjoc_listener_orientation_retired_destroy(retired);
    if (preparer) openjoc_listener_orientation_preparer_destroy(preparer);
    if (decoder) openjoc_decoder_destroy(decoder);
    return result;
}
```

The quaternion struct is size-versioned `(x, y, z, w)` and finite values are
scale-normalized. It describes the active rotation from listener-local axes to
scene axes (`+Y` forward, `+X` right, `+Z` up). The implementation applies the
inverse rotation to fixed virtual-speaker directions before resolving the
complete HRIR set. Sequence numbers increase monotonically within
`stream_epoch`; reset starts a new epoch, and stale prepared updates are
rejected without consuming the update handle.

In pull mode, direct decoder receives emit at most the configured sample
count. The compressed stream bridge does not render ahead of its first receive;
each receive returns at most that same count. `pending_binaural_input_samples`
reports projected virtual-speaker PCM waiting for binauralization (bounded to
one 1,536-sample AU); do not feed the next AU until it is consumed. A pose accepted between receives becomes eligible for a not-yet-rendered chunk. If a 240-sample crossfade is active, its transition waits until that fade completes; one latest target may wait or be superseded. No update changes PCM already returned. Drain exposes reconstruction and FIR tails incrementally.
This queue is a PCM/control boundary; it provides neither a sensor timestamp
nor a sensor-to-sound latency guarantee.

On apply validation/lifecycle failure, the original update pointer remains
usable. On success it becomes null and `retired` receives the same allocation,
even when there are no retired kernels. Destroy retired handles away from an
audio callback. A contained panic poisons the decoder; the update may be empty
and should be destroyed rather than retried. Calls to `prepare` and
`preparer_last_error` on the same preparer handle must be serialized because
the diagnostic pointer is replaced by the next prepare; distinct handles may
prepare concurrently. Preparation errors are available through
`openjoc_listener_orientation_preparer_last_error()`.

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

The live snapshot is not the offline Inspector JSON contract. It describes the
current decoder session and observed-so-far coverage; it must not be presented
as a whole-file census or used to infer unavailable container metadata.

## Failure containment

Every exported operation contains Rust panics before returning. No Rust panic,
Rust error object, or Rust struct layout crosses the ABI. `last_error` is
owned by the decoder instance and is not process-global. Null arguments,
invalid struct sizes, malformed packets, unsupported configurations, format
changes, and render failures return numeric status codes.

The public C header has no third-party generated material and is distributed
under the repository Apache-2.0 license.
