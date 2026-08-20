# Native FFmpeg libavcodec wrapper

OpenJOC provides an experimental native libavcodec wrapper for custom FFmpeg
builds configured with `libopenjoc`. Stock FFmpeg binaries do **not** acquire
this decoder by installing OpenJOC; the exported patch must be applied and
FFmpeg must be rebuilt with:

```sh
./configure --enable-version3 --enable-libopenjoc ...
```

The named decoder is `libopenjoc`. It has codec ID `AV_CODEC_ID_EAC3`, long
name `OpenJOC E-AC-3 JOC decoder`, wrapper name `libopenjoc`, packed
`AV_SAMPLE_FMT_FLT` output, and a fixed 48 kHz sample rate. The stock `eac3`
decoder is neither modified nor replaced.

## Architecture and source boundary

```text
AVPacket / ff_decode_get_packet
  -> libavcodec/libopenjocdec.c
  -> openjoc_stream_decoder (C ABI 1.3)
  -> the shared bounded packet/AU bridge
  -> OpenJocSession
  -> semantic channel permutation
  -> ff_get_buffer AVFrame
```

The framework-neutral `openjoc_stream_decoder` accepts compressed chunks,
optional sample-domain PTS, and discontinuity/preroll flags. It exposes PCM,
semantic layout labels, latency, status, and the shared effective-config
fingerprint. It contains no `AVPacket`, `AVFrame`, `AVCodecContext`, or FFmpeg
time-base type. The older `openjoc_decoder` complete-AU API remains available.

Both the external `openjoc-ffmpeg` frontend and native C ABI path delegate to
the same 128 KiB-bounded assembler, positive JOC classifier, timestamp model,
output queue, layout mapping, and channel permutation. The render session is
created lazily only after a complete AU is positively admitted as JOC.

## Reproducible source integration

Pinned bases, local FFmpeg patch commits, the OpenJOC start HEAD, ABI version,
configure flags, and patch SHA-256 are recorded in
[`integrations/ffmpeg/native/BASELINES`](../../integrations/ffmpeg/native/BASELINES).
The patch is:

```text
integrations/ffmpeg/native/patches/
  0001-avcodec-add-experimental-libopenjoc-decoder-wrapper.patch
```

It applies to both FFmpeg `n9.0.1` and the recorded current-master commit. Do
not apply it inside the OpenJOC repository or vendor an FFmpeg source tree.
For a clean separate FFmpeg Git checkout:

```sh
integrations/ffmpeg/native/verify-build.sh \
  /absolute/path/to/clean-ffmpeg \
  /absolute/path/to/empty-build \
  /absolute/path/to/openjoc-prefix \
  /absolute/path/to/optional-positive-joc.ec3
```

The script verifies the pinned source, applies the exported patch, stages the
OpenJOC dynamic C library and `openjoc.pc`, configures/builds FFmpeg, checks
decoder discovery/options, proves generic selection, performs an ordinary
E-AC-3 safety test, and optionally runs positive native decode and API tests.
The isolated native-FFmpeg workflow runs this path only when integration files
change or when manually dispatched.

`--enable-libopenjoc` uses `pkg-config` package `openjoc >= 0.8.0` and probes
`openjoc.h` plus `openjoc_stream_decoder_create`. A requested build fails
clearly if either the header or library is absent. The development staging
script installs `libopenjoc_capi.dylib`/`.so` dynamically; static linkage is not
part of this acceptance.

At runtime, the OpenJOC shared library must be discoverable through a packaged
rpath or the platform's development loader path. There is no fallback to stock
E-AC-3 after `libopenjoc` was explicitly selected.

## Decoder selection safety

`libopenjoc` is deliberately marked `AV_CODEC_CAP_EXPERIMENTAL`. FFmpeg's
generic codec-ID lookup prefers a non-experimental decoder regardless of codec
registry order. Therefore:

```text
avcodec_find_decoder(AV_CODEC_ID_EAC3)       -> eac3
avcodec_find_decoder_by_name("eac3")         -> eac3
avcodec_find_decoder_by_name("libopenjoc")   -> libopenjoc
```

The generated stable registry currently lists `eac3` before `libopenjoc`, but
safety does not depend on that ordering. Explicit opening requires experimental
strictness because this is a local experimental wrapper.

The verified FFmpeg CLI input-decoder syntax places the decoder and its options
before the input they apply to:

```sh
ffmpeg \
  -c:a libopenjoc -strict experimental \
  -render_mode speaker -speaker_layout 7.1.4 \
  -i programme.ec3 \
  -c:a pcm_f32le output.f32
```

For binaural:

```sh
ffmpeg \
  -c:a libopenjoc -strict experimental \
  -render_mode binaural -virtual_layout 7.1.4 \
  -i programme.m4a \
  -c:a pcm_f32le binaural.f32
```

`ffplay` uses its own decoder-selection spelling. The verified muted smoke
command is:

```sh
ffplay -nodisp -autoexit -volume 0 -t 0.05 \
  -acodec libopenjoc -strict experimental \
  -speaker_layout 2.0 programme.ec3
```

Explicit selection on ordinary E-AC-3 returns `AVERROR_INVALIDDATA`, logs a
concise positive-admission rejection, and emits no PCM. Ordinary E-AC-3 without
explicit selection continues through stock `eac3`.

JOC autoselection is intentionally not implemented. A future player must
positively probe JOC and then choose the decoder by name; codec-ID lookup alone
must never hijack ordinary E-AC-3.

## Options and defaults

`ffmpeg -h decoder=libopenjoc` reports:

- `render_mode= speaker | stereo | binaural` (`speaker` by default);
- `speaker_layout` (`5.1` by default), including `2.0`, `5.1`, `7.1.4`,
  `9.1.6`, and `22.2`;
- `virtual_layout=7.1.4` by default for binaural;
- `downmix=auto | loro | ltrt`;
- `drc=disabled | line | rf | custom`, plus `drc_boost`/`drc_cut`;
- `dialnorm=default | digital | analog`;
- `validation=auto | strict | vendor`;
- optional `sofa`; omission selects built-in SADIE II D1;
- `binaural_lfe=exclude | dual_mono`.

There is no offline peak-normalization option in the decoder. Physical `2.0`
and binaural two-channel output remain distinct product choices.

Standard FFmpeg layouts are used where exact, including stereo, 5.1(side),
5.1.2, 5.1.4, 7.1, 7.1.2, 7.1.4, 22.2, and binaural. Custom ordered layouts
cover 7.1.6 and the 9.1 family. Binaural channels are true `BIL`/`BIR`.

## Timing, drain, flush, and seek

The callback model is `FF_CODEC_RECEIVE_FRAME_CB`; it obtains packets through
`ff_decode_get_packet()` and respects libavcodec EAGAIN/backpressure. Each
emitted frame is allocated with `ff_get_buffer()`, and OpenJOC PCM is copied
into standard AVBuffer-owned packed-float storage.

OpenJOC keeps its exact logical timeline in 1/48000 sample units. The wrapper
integer-rescales packet PTS into that domain and rescales frame PTS/duration
back into `AVCodecContext.pkt_timebase`, which is normal libavcodec frame
representation. No floating-point time or latency PTS pre-subtraction is used.

`AVCodecContext.delay` is 609 samples for speaker output and 577 for binaural.
Normal libavcodec operation uses that value once to discard initial decoder
priming. Bit-exact decoder/frontend comparisons use
`AV_CODEC_FLAG2_SKIP_MANUAL` (`-flags2 +skip_manual`) so FFmpeg retains every
decoder sample and exposes skip metadata rather than discarding it.

A NULL packet drains QMF, FinalLinkedGain, reconstruction, and binaural FIR
tails, followed by stable EOF. `avcodec_flush_buffers()` clears compressed
lookahead, output, admission, timestamps, decoder history, gain state, and FIR
state. The verified seek sequence is demux, decode, seek, flush, feed preroll,
then resume; callers remain responsible for choosing adequate preroll for their
seek target.

## Local acceptance

The private 277.504-second raw programme was not copied into the repository.
Raw EC3, locally remuxed MP4, and locally remuxed Matroska all decoded through
the named native decoder. The complete raw-program results after inverse
semantic permutation and manual skip were:

| Mode | Samples/channel | Channels | Semantic float32 SHA-256 | External | Native |
| --- | ---: | ---: | --- | ---: | ---: |
| 7.1.4 | 13,320,224 | 12 | `62d2143094b88323800775d211e2d0ef9759f1443ab2d10be0ad48176ac6ee11` | 79.97 s | 81.67 s |
| 22.2 | 13,320,224 | 24 | `fabdf84bd13d5d728450799f229633930d41e1d0a1dd768e310369b8b470cc53` | 88.26 s | 85.66 s |
| binaural | 13,320,447 | 2 | `fc6a2e5508ae7fe11aceef6459751f27df87bda29ffa77550b2b780a618cb026` | 219.37 s | 217.38 s |

The 22.2 result is 1,278,741,504 packed PCM bytes and joins the already-retained
direct-session, CLI, GStreamer, and external-FFmpeg equivalence result. These
are single-run local timings, not performance guarantees.

The native API verifier additionally covers fragmentation, multiple AUs in one
packet, I0 lookahead, drain/repeated EOF, flush after a partial AU,
multi-instance isolation, delay/PTS, and AVBuffer ownership. A real MP4 seek
resumed at PTS 96,768 for a 96,000 target in the 1/48000 stream clock.

## License and release scope

OpenJOC is Apache-2.0. The integration uses dynamic linkage, and the recorded
FFmpeg configuration is LGPL version 3 or later because `libopenjoc` is placed
in FFmpeg's version-3 external-library set. This records the engineering
configuration rather than offering a legal conclusion; redistribution still
requires a separate license/packaging review.

No standalone FFmpeg distribution is released and no patch is submitted
upstream. The 0.8.0 OpenJOC Player Bundles contain project-provided custom
FFmpeg/mpv integration runtimes for the qualified platforms. The separate
source-only mpv patchset is documented in `docs/integration/MPV.md`.
