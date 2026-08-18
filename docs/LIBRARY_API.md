# Headless Rust streaming API

`openjoc-api` provides the first high-level embeddable interface. It is
experimental for OpenJOC 0.7 work, while the decoder and renderer semantics
remain the existing 0.6 implementation.

## Lifecycle

```rust
use openjoc_api::{OpenJocConfig, OpenJocPacket, OpenJocSession};

let mut session = OpenJocSession::new(OpenJocConfig::default())?;
let status = session.push_packet(OpenJocPacket {
    data: complete_access_unit,
    pts_samples: Some(0),
    discontinuity: false,
    preroll: false,
})?;
while let Some(frame) = session.receive_frame() {
    consume_interleaved_f32(frame.interleaved_f32, frame.sample_rate);
}
let _ = session.drain()?;
while let Some(frame) = session.receive_frame() {
    consume_interleaved_f32(frame.interleaved_f32, frame.sample_rate);
}
# Ok::<(), Box<dyn std::error::Error>>(())
```

The session is serially accessed. Separate sessions are independent and may
run concurrently. Immutable configuration is copied into the session; no
process-global decoder, layout, SOFA object, or error buffer exists.

## Input contract

`OpenJocPacket` is borrowed for the duration of `push_packet`. `data` must be
one complete E-AC-3 JOC access unit: independent substream zero and optional
dependent substream zero. This matches the smallest robust unit required by
JOC metadata, Base/RB alignment, and E-AC-3 inheritance. The session does not
retain the compressed input buffer.

Arbitrary byte fragmentation, file paths, MP4/Matroska demuxing, and multiple
AUs in one call are intentionally outside this first contract.

## Output and layout

The canonical format is interleaved IEEE-754 `f32`. Each `OpenJocPcmFrame`
owns its vector and may be retained by a Rust caller. It reports sample rate,
sample count, sample-domain PTS, render mode, layout name, and ordered semantic
channel labels. Speaker layouts use the repository's canonical public presets;
2.0 is `FL, FR`, 5.1 is `FL, FR, FC, LFE, Ls, Rs`, and binaural is physical
stereo `Left, Right` even when its virtual layout is multichannel.

`output_info()` is available before the first packet. Sample rate is `None`
until the first AU establishes the stream format.

## Time, latency, drain, and seek

PTS uses the decoded sample domain. If a first packet has PTS `P`, output for
logical sample `n` reports `P + n`; the PTS is not silently moved by the
filterbank or final linked-gain delay. Speaker output reports the deterministic
577-sample QMF/Base-RB delay plus the admitted 32-sample causal speaker-stage
block; binaural output remains on the existing QMF-only latency contract.
This makes availability delay explicit without forcing callers to reverse-
engineer it from frame counts.

- `drain()` flushes QMF/reconstruction state and the direct SOFA FIR tail.
- `flush()` discards pending PCM and resets stream-derived state while keeping
  configuration and prepared SOFA data.
- `reset()` has the same reusable-session semantics and is the intended seek or
  discontinuity boundary.
- A packet with `discontinuity = true` performs the stream reset before decode.
- `preroll = true` is accepted to prime decoder state; this first ABI does not
  suppress the delayed frame automatically.

The output queue is bounded. A caller must receive pending PCM before pushing
another packet; `OpenJocStatus::OutputPending` is returned otherwise.

## Policies

`DrcPolicy` maps directly to the existing E-AC-3 `InternalBasePolicy` and
supports disabled, line, RF, and custom boost/cut. `DownmixPolicy` supports
auto, Lo/Ro, and Lt/Rt for stereo output. No CLI enum is reused as a public
library type.

The session automatically applies the supported Default E-AC-3 dialnorm
program scalar from each valid independent syncframe to the complete decoded
program before speaker projection, FinalLinkedGain, or SOFA convolution.
Dialnorm is separate from `DrcPolicy`; no new public configuration field is
required for this first integration.

`BinauralConfig` accepts a complete in-memory SimpleFreeFieldHRIR SOFA buffer,
a virtual speaker layout, and an explicit LFE policy. The session does not
retain a filesystem path. The public API currently uses direct convolution;
partitioned convolution is deferred to a later ABI extension.

## Errors and status

Statuses are numeric and non-error lifecycle outcomes: `NeedMoreInput`,
`FrameAvailable`, `OutputPending`, and `EndOfStream`. Rust errors are typed
`OpenJocError` values. The C adapter maps them to numeric status codes and an
instance-owned diagnostic message.

Malformed packets, format changes, timestamp discontinuities, profile changes,
and render failures are not silently converted into mismatched PCM.
