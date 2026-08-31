# Headless Rust streaming API

`openjoc-api` provides the current high-level embeddable interface. It remains
experimental, with decoder and renderer semantics bounded by the current
capability and limitation contracts.

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
one complete General E-AC-3 JOC access unit: independent substream zero and
ordered dependent substreams D0 through D7, within the bounded maximum. This
matches the JOC metadata, Base/RB alignment, and E-AC-3 inheritance contract.
The session does not retain the compressed input buffer. CMAF and the existing
legacy AC-3 Annex-J combination remain D0-only.

Arbitrary byte fragmentation, file paths, MP4/Matroska demuxing, and multiple
AUs in one call are intentionally outside this first contract.

## Output and layout

The canonical format is interleaved IEEE-754 `f32`. Each `OpenJocPcmFrame`
owns its vector and may be retained by a Rust caller. It reports sample rate,
sample count, sample-domain PTS, render mode, layout name, and ordered semantic
channel labels. Speaker layouts use the repository's canonical public presets;
2.0 is `FL, FR`, 5.1 is `FL, FR, FC, LFE, Ls, Rs`, 22.2 is the canonical
24-channel `FL, FR, FC, LFE1, ... , BtFR` order, and binaural reports
`Left Ear, Right Ear` even when its virtual layout is multichannel.

Physical speaker sessions may also use `SpeakerLayout::custom(...)` and
`OpenJocConfig::with_speaker_layout(...)`. The custom layout keeps the caller's
speaker array order as PCM/channel order, validates finite spherical geometry,
and keeps LFE channels outside the spatial projector. The JSON/CLI form is
documented in [custom speaker layouts](../using/custom-speaker-layouts.md); it is
advanced functionality and does not widen downstream host/device channel
layout support.

For binaural sessions, `BinauralConfig::builtin_generic("7.1.4")` selects the
offline bundled SADIE II generic HRTF without a filesystem path. Use
`BinauralConfig::from_sofa_bytes(...)` for an explicit user SOFA; strict SOFA
validation and fail-closed coverage behavior are unchanged.

`output_info()` is available before the first packet. Sample rate is `None`
until the first AU establishes the stream format.

## Time, latency, drain, and seek

PTS uses the decoded sample domain. If a first packet has PTS `P`, output for
logical sample `n` reports `P + n`; the PTS is not silently moved by the
filterbank or final linked-gain delay. Speaker output reports a 609-sample
delay: the 577-sample QMF/Base-RB delay
plus the admitted 32-sample causal speaker-stage block. Binaural output reports
577 samples because it does not use the speaker FinalLinkedGain stage. These
are public synchronization contracts; dialnorm and offline static
normalization add zero audio-sample latency.
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
supports disabled, line, RF, and custom boost/cut. DRC changes program
dynamics; it is not a final volume or loudness control. `DownmixPolicy`
supports auto, Lo/Ro, and Lt/Rt for stereo output. No CLI enum is reused as a
public library type.

`OpenJocConfig::dialnorm` selects the decoder/program calibration policy:
`DialnormMode::Default` (calibrated default behavior) is the default and is
recommended for normal playback/decoding. `Digital` explicitly selects
encoded digital program-level calibration. `Analog` uses a unity dialnorm
factor and is an advanced compatibility/diagnostic policy; it is not a
recommended louder-output or mastering mode. Dialnorm is separate from
`DrcPolicy`; DRC remains encoded dynamic-range metadata processing.

The selected dialnorm program scalar is applied once to the complete decoded
program before speaker projection, FinalLinkedGain, or SOFA convolution.
FinalLinkedGain is internal renderer headroom behavior, not a user mastering
control. `OpenJocSession` never performs file-export peak normalization or any
other file-oriented output transform; applications may apply their own final
gain policy after receiving PCM. The CLI's `--normalize-peak` is a separate
offline convenience: one static sample-peak gain applied after renderer
processing, not DRC, dialnorm, limiting, compression, LUFS, or true-peak
normalization. The streaming API does not perform file-export peak
normalization or spool a complete program for a file-level transform.

`BinauralConfig` accepts a complete in-memory SimpleFreeFieldHRIR SOFA buffer,
a virtual speaker layout, and an explicit LFE policy. The session does not
retain a filesystem path. The public API currently uses direct convolution;
partitioned convolution is deferred to a later ABI extension.

For frontend parity audits, `OpenJocConfig::effective_config_descriptor()` and
`effective_config_fingerprint()` expose the normalized session-boundary fields.
`trace_access_units()` records each grouped AU's exact byte length, SHA-256,
sample-domain PTS, rate, and independent/dependent frame counts.

## Errors and status

Statuses are numeric and non-error lifecycle outcomes: `NeedMoreInput`,
`FrameAvailable`, `OutputPending`, and `EndOfStream`. Rust errors are typed
`OpenJocError` values. The C adapter maps them to numeric status codes and an
instance-owned diagnostic message.

Malformed packets, format changes, timestamp discontinuities, profile changes,
and render failures are not silently converted into mismatched PCM.
