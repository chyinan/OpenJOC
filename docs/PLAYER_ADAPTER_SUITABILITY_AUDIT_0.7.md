# OpenJOC player-adapter suitability audit

Status: audit only. No adapter, player module, release artifact, or host-stack
change is implemented by this document.

Audit session: `openjoc-player-adapter-suitability-01`

Audit date: 2026-08-19 (Asia/Shanghai)

## Decision

`PRIMARY_FIRST_ADAPTER`: **GStreamer**

`SECONDARY_FOLLOWUP_ADAPTER`: **FFmpeg-facing external wrapper**, followed by a
native FFmpeg decoder proposal only after the wrapper has frozen the packet and
timestamp observations.

The first GStreamer deliverable should be one installable plugin package with
two small elements:

1. `openjocau`: a bounded E-AC-3 syncframe-to-JOC-AU packetizer that groups one
   independent substream zero and optional dependent substream zero;
2. `openjocdec`: a `GstAudioDecoder` element that accepts exactly one complete
   JOC AU, calls the existing OpenJOC ABI, and emits interleaved float PCM.

This is the smallest architecture that proves OpenJOC inside a real streaming
framework while preserving the actual OpenJOC packet contract. The stock
GStreamer `ac3parse` element is useful upstream, but its documented output is
framed E-AC-3 syncframes, not an OpenJOC AU. It must not be treated as the AU
assembler.

The first plugin must not autoselect ordinary E-AC-3. It should be explicitly
selected after JOC admission, or run with an explicit `joc-only` policy and fail
closed. Ordinary E-AC-3 must remain on the host's normal decoder path.

## Repository baseline

The repository was inspected at the resolved current head, not merely the
published tag:

| Fact | Result |
|---|---|
| Published baseline | `v0.7.0`, commit `cf54088be4c82efdf95388e419213b902b206d4c` |
| Current head | `3c2722c34e4ee9d81c6d5df96056f7f00748b4b7` |
| Head relation | four commits after `v0.7.0`; current head is `master` |
| Current head subject | `test(render): cover 22.2 composition controls` |
| Worktree policy | existing untracked `.DS_Store` files and `references/` were preserved |
| Rust/C API tests | passed: `cargo test -p openjoc-api -p openjoc-capi` |
| Integration foundation | passed: `scripts/verify-integration-foundation.sh` |

The four post-tag commits are renderer/binaural/layout changes and do not
change the session packet/lifecycle contract audited below.

Primary local evidence:

- `crates/openjoc-api/src/lib.rs`
- `crates/openjoc-capi/include/openjoc.h`
- `crates/openjoc-capi/src/lib.rs`
- `docs/INTEGRATION_API_CURRENT_STATE.md`
- `docs/C_API.md`
- `docs/KNOWN_LIMITATIONS.md`
- `crates/openjoc-eac3/src/lib.rs`
- `crates/openjoc-container/src/lib.rs`

## Current OpenJOC integration foundation

The requested capability summary is substantially present, with important
scope limits.

### Present and usable

- `OpenJocSession` and `OpenJocConfig` are headless, instance-owned Rust APIs.
- The experimental C ABI is version `1.1`, with `openjoc.h`, opaque decoder
  handles, numeric status values, instance-owned errors, panic containment, and
  forward-compatible `struct_size` fields.
- Input is a borrowed byte slice for one complete E-AC-3 JOC AU.
- Output is interleaved IEEE float32 PCM.
- Input PTS and output PTS are sample-domain integers at the decoded stream
  sample rate.
- `push_packet`, `receive_frame`, `drain`, `flush`, and `reset` are present.
- Separate sessions have independent state and can run concurrently on separate
  threads; a single session is serial-only.
- Speaker presets include `2.0`, `5.1`, the supported height/rear families,
  `9.1.x`, and `22.2`, with semantic labels exposed by the Rust API and C ABI.
- Binaural mode is two-channel `Left Ear` / `Right Ear` output. Empty SOFA bytes
  select the bundled generic SADIE II HRTF; a caller-owned SOFA buffer selects
  the strict explicit-SOFA path.
- DRC and dialnorm are separate policies. The player default must remain
  `DialnormMode::Default`; offline sample-peak normalization is not part of the
  player path.

### Actual limitations that affect adapter design

1. **The packet boundary is exact, not advisory.** `push_packet` indexes and
   groups the bytes, then rejects a packet unless it contains exactly one
   complete AU. Arbitrary byte fragmentation and multiple AUs per push are
   rejected.
2. **The admitted JOC AU is one I0 plus optional D0.** The lower-level grouping
   code can describe more general ordered substream sequences, but the JOC PCM
   decoder rejects more than one dependent frame and admits only the documented
   JOC topologies.
3. **JOC is not identified by the ordinary E-AC-3 codec name.** The session
   inspects E-AC-3 syncframe structure, JOC `addbsi` signaling, and the bounded
   EMDF/OAMD/JOC carrier. An `E-AC-3` codec tag alone is insufficient.
4. **Ordinary E-AC-3 is not a supported fallback inside the session.** The
   session may decode some base PCM before returning the missing-JOC error, but
   it does not return ordinary E-AC-3 PCM. The adapter must route ordinary E-AC-3
   elsewhere or reject it before selecting OpenJOC.
5. **Preroll is accepted as an input fact but is not currently an output
   discard policy.** `OpenJocSession` decodes a preroll packet normally. An
   adapter can discard its returned frames while priming state, but the session
   does not tag output frames with their input preroll origin.
6. **The C ABI PCM pointer is borrowed.** It remains valid until the next
   send/receive/flush/reset/destroy operation on that decoder handle. A
   framework buffer that can outlive the callback must copy the PCM.
7. **The C ABI is experimental.** It is suitable for the first adapter but is
   not a frozen long-term ABI promise.
8. **Current real-media fidelity remains scoped.** The repository has strong
   public-syntax and synthetic evidence, but does not claim full fidelity for
   every real-world E-AC-3 coding-tool combination or every producer stream.

### Timing and latency contract

The current session reports a fixed latency in samples and does not shift the
logical PTS timeline to hide it:

| Render mode | Current reported latency | Output |
|---|---:|---|
| Speaker | `577 + 32 = 609` samples | selected speaker preset, interleaved F32 |
| Stereo/2.0 speaker | `609` samples | two-channel speaker output |
| Binaural | `577` samples | two-channel binaural F32 |

The 577 samples are the QMF/Base–reconstruction delay. The additional 32
samples are the admitted final linked-gain speaker stage. Drain emits the
reconstruction and, for binaural, FIR tails. Tail length is an EOS concern, not
an ongoing latency value.

PTS behavior is:

- the first timestamp establishes the segment origin;
- each subsequent input PTS must equal the origin plus the accumulated decoded
  samples, unless the packet is marked discontinuous;
- a discontinuity resets stream-derived state before timestamp checking;
- output PTS is the logical rendered sample position plus the segment origin;
- the reported latency is carried separately and must be propagated through the
  host framework's latency mechanism.

### Ownership and threading contract

- Input bytes are borrowed only during `send_packet`; OpenJOC does not retain
  the compressed packet.
- Rust output frames are owned `Vec<f32>` values. The C ABI exposes the current
  frame through a borrowed pointer for the lifetime described above.
- The first C-framework adapter therefore performs one compressed-AU copy into
  its bounded packetizer and one PCM copy into a host-owned output buffer.
- No adapter-global lock is justified. One decoder handle is confined to the
  framework's serialized decoder/streaming context; separate streams use
  separate handles.

## Packet-ingest audit

The relevant unit is not an arbitrary E-AC-3 syncframe. OpenJOC's unit is the
ordered time-aligned group:

```text
I0 syncframe
[optional D0 syncframe]
----------------------
one OpenJOC JOC access unit
```

The current source indexes syncframes using each declared frame size and groups
them by independent/dependent stream type and substream ID. This is the source
of truth for the adapter's packet contract.

| Host input boundary | Can a host packet contain multiple syncframes? | Can it split a syncframe? | Native parser help | OpenJOC framing work |
|---|---|---|---|---|
| FFmpeg `AVPacket` | Yes, depending on demuxer/container and parser path; do not assume one syncframe or one JOC AU | Yes on raw/parser paths | `av_parser_parse2` can emit complete parsed frames | group parsed syncframes into I0/D0 AUs; reject/retain timestamp at I0 |
| GStreamer `ac3parse` output | Its documented `audio/x-eac3` source is `framed=true`, `alignment=frame`; treat each output as a syncframe | Upstream fragmentation is absorbed by `GstBaseParse` | Yes for syncframe framing | still group I0/D0; stock parser does not establish JOC admission |
| mpv | Normally inherits FFmpeg demux/codec packets | Inherits FFmpeg path | FFmpeg parser/decoder path | same FFmpeg/OpenJOC bridge work |
| VLC decoder module | Decoder receives packetized `block_t`/frame objects from VLC's input/packetizer path; exact JOC grouping is not guaranteed by the decoder callback | Packetizer may frame, but JOC grouping must be verified | VLC packetizers help with codec framing | group/validate I0/D0 before OpenJOC |
| DirectShow/LAV | A splitter/decoder graph may deliver one or more E-AC-3 syncframes in an `IMediaSample`; sample boundary is a graph contract, not automatically an AU contract | Possible in byte-stream graphs; a transform filter must not assume otherwise | LAV splitter/parser and DirectShow graph can frame samples | aggregate/validate before calling OpenJOC |

### Required packet rule for every candidate

The adapter must own a bounded accumulator with these rules:

- parse or receive complete syncframes;
- start an AU only at independent substream zero;
- append the immediately following permitted dependent substream zero;
- when the next independent substream zero arrives, finalize the previous AU
  before retaining the new one;
- reject nonsequential, missing, malformed, or unsupported substreams;
- attach the PTS of the independent frame to the complete AU;
- never concatenate two AUs into one OpenJOC call;
- never pass a split syncframe to `OpenJocSession`.

The adapter should not add a general MP4, Matroska, MPEG-TS, or Blu-ray demuxer.
The host demuxer remains responsible for container parsing.

## Candidate audit: FFmpeg-facing integration

### Architecture

There are three materially different FFmpeg models:

1. **Native `AVCodec` integration.** Add an OpenJOC decoder to FFmpeg's
   libavcodec and make it selectable for `AV_CODEC_ID_EAC3` JOC streams.
2. **External wrapper around libavformat/libavcodec.** Use FFmpeg for demuxing
   and packet/timestamp delivery, then use OpenJOC as a separate decoder in a
   host-side wrapper.
3. **FFmpeg-only demux.** Use FFmpeg to supply compressed E-AC-3 packets to an
   application-owned OpenJOC session. This is the smallest FFmpeg experiment,
   but it is not a decoder integration visible to ordinary FFmpeg users.

### External codec-plugin answer

FFmpeg does not document a stable runtime external decoder-plugin mechanism
that lets an independently shipped library register an `AVCodec` and have
`avcodec_find_decoder()` discover it. The current public core API exposes
registered-codec iteration and lookup. The historical explicit registration
functions are deprecated, not a supported external module ABI. FFmpeg's
internal codec headers are explicitly internal and must not be included by
individual decoders.

Therefore:

- a normal native FFmpeg decoder requires FFmpeg source/build integration;
- an out-of-tree `AVCodec` fork can be maintained, but it is tied to FFmpeg's
  source and build internals rather than independently installable;
- an external `libopenjoc` wrapper is practical only as a separate application,
  demux adapter, or host-specific audio path;
- feeding already-decoded PCM from FFmpeg into an OpenJOC filter is wrong because
  the JOC metadata and compressed access-unit semantics have already been
  consumed by the ordinary decoder.

### JOC detection

FFmpeg's stream identity is ordinary E-AC-3 (`AV_CODEC_ID_EAC3`), not a separate
JOC codec ID. Current FFmpeg parsing structures expose E-AC-3 header fields,
including the type-A extension/complexity information, but the OpenJOC adapter
still needs to inspect the complete AU and the EMDF/OAMD/JOC payload. A codec tag
or `AV_CODEC_ID_EAC3` must never be treated as proof that OpenJOC should take
over.

For a native FFmpeg decoder, the admission logic belongs in the codec's source
and must preserve ordinary E-AC-3 fallback. For an external wrapper, use an
OpenJOC admission helper before committing the session, then pass only complete
JOC AUs to the session.

### Packet, timestamps, drain, and flush

- `AVPacket` carries compressed data plus PTS/DTS/duration in the stream time
  base. The decoder API is the modern `avcodec_send_packet` /
  `avcodec_receive_frame` state machine.
- `av_parser_parse2` can split a byte stream into complete parsed frames and
  returns the number of input bytes consumed. This is syncframe assistance, not
  JOC I0/D0 grouping.
- At EOF, the wrapper must send a null packet and receive until `AVERROR_EOF`,
  while separately calling OpenJOC `drain` and forwarding every tail frame.
- A seek requires `avcodec_flush_buffers` plus clearing the wrapper's AU
  accumulator and calling OpenJOC `flush`/`reset`. Preroll packets must be
  decoded to prime state and their output discarded until the post-seek
  presentation point.
- FFmpeg exposes codec delay concepts and frame timestamps, but a normal
  `AVFrame` is not a general host-pipeline latency query. A native decoder can
  describe codec delay through its codec context, but the OpenJOC 609/577
  sample renderer latency and EOS tails still need deliberate FFmpeg/player
  integration policy.

### PCM and channel layouts

`AV_SAMPLE_FMT_FLT` is the packed/interleaved float format suitable for OpenJOC
output; the wrapper must not accidentally select `FLTP` and then claim it is
interleaved. FFmpeg's modern `AVChannelLayout` can carry native layouts up to 63
channels and explicit custom maps, so 24-channel 22.2 and two-channel binaural
can be represented. Downstream filters and audio devices may still reduce or
reinterpret custom layouts, so semantic layout preservation must be asserted in
the wrapper tests.

### Deployment and licensing

- Native integration: rebuild FFmpeg, or consume a distribution that has the
  decoder patch. This is not independently installable as a normal codec DLL.
- External wrapper: ship a separate OpenJOC library and wrapper executable or
  host library next to FFmpeg. This is independently deployable but does not add
  OpenJOC to `ffplay`, `ffmpeg`, or every FFmpeg consumer.
- FFmpeg is primarily LGPL-2.1-or-later with optional GPL components and
  optional configurations that change the resulting obligations. The FFmpeg
  project documents dynamic-linking/source-distribution considerations. A
  native source contribution also has FFmpeg's contribution-license
  expectations. OpenJOC Apache-2.0 is not a reason to assume that a combined
  native FFmpeg source tree has one simple license; the exact source files and
  FFmpeg configuration must be reviewed.

### Assessment

FFmpeg is the highest-reach follow-up and the best demux/timestamp observation
target after the first adapter. It is not the best first real decoder/plugin
target because the desirable native route requires source integration and the
desirable out-of-tree route is not a normal FFmpeg codec plugin.

Classification: **UPSTREAM_REQUIRED** for a normal native decoder;
**OUT_OF_TREE_PRACTICAL** for a separate FFmpeg-demux/OpenJOC wrapper.

## Candidate audit: GStreamer

### Element architecture

The natural raw output contract is:

```text
audio/x-eac3, framed=true, alignment=frame
    -> openjocau
audio/x-eac3, framed=true, openjoc-au=true
    -> openjocdec (GstAudioDecoder)
audio/x-raw, format=F32LE, layout=interleaved,
              rate=..., channels=..., channel-mask=...
```

`GstAudioDecoder` is a strong fit for the decoder half:

- `start`/`stop` map to session creation/destruction;
- `set_format` supplies the input rate and stream format;
- `handle_frame` receives the admitted AU;
- `gst_audio_decoder_finish_frame` pushes decoded audio;
- a drainable decoder can receive a null input buffer at EOS;
- `gst_audio_decoder_set_latency` reports min/max decoder latency and posts a
  latency reconfiguration message when it changes;
- the base class has explicit flush/segment state and tries to maintain output
  timestamps from upstream timestamps.

The packetizer half is also a natural GStreamer component. `GstBaseParse` owns
the adapter, framing, flush, EOS, segment, and parser-query machinery. It can
consume arbitrary upstream byte chunks, identify declared syncframe sizes, and
emit exactly one JOC AU buffer. It must not claim that every E-AC-3 frame is a
JOC AU.

### Stock parser behavior

The official `ac3parse` element advertises `audio/x-eac3` with `framed=true` and
`alignment=frame`. Its documented hierarchy is `GstBaseParse`. That makes it
valuable upstream, but the advertised frame is the parser's E-AC-3 frame. The
OpenJOC AU contract still requires an additional I0/D0 grouping step. The first
adapter should either:

- use `ac3parse ! openjocau ! openjocdec`; or
- make `openjocdec` contain an internal bounded AU accumulator and expose only
  the complete-AU path.

The two-element form is preferred because it makes packet behavior observable,
unit-testable, and reusable by a later decoder implementation.

### Timestamps, latency, flush, EOS, and seek

Recommended mapping:

| GStreamer event/state | OpenJOC operation |
|---|---|
| `start` / `set_format` | create a fresh configured decoder; derive sample rate and output caps |
| first AU PTS | convert nanoseconds to exact sample-domain PTS using the input rate |
| next contiguous AU | pass origin-plus-accumulated-samples |
| `DISCONT` buffer or new segment after flush | mark the next OpenJOC packet discontinuous and reset the AU assembler |
| `FLUSH_START` | stop accepting/forwarding data and discard packetizer/output queues |
| `FLUSH_STOP` | call OpenJOC `flush`/`reset`, clear the segment origin, await the new segment |
| `EOS` / null decoder input | call OpenJOC `drain`, push all available frames, then finish EOS |
| seek with preroll | prime OpenJOC with preroll packets; discard their output; emit only post-seek PCM |
| sample-rate or layout change | end the current decoder format, reset state, and renegotiate; do not continue a session across a format change |

At a known input rate, call `gst_audio_decoder_set_latency()` with equal min/max
values:

- speaker/stereo: `609 / rate` seconds;
- binaural: `577 / rate` seconds.

Do not add the EOS tail to the steady-state latency query. Do not shift logical
buffer PTS by 609 or 577 just because the decoder reports that latency; let the
pipeline latency model perform synchronization.

### PCM and layouts

GStreamer explicitly supports `F32LE` and `layout=interleaved`. For more than
two channels, raw-audio caps require a channel mask. `GstAudioChannelPosition`
includes the SMPTE 2036-2 22.2 positions and can express semantic channel
positions through `GstAudioInfo`/raw caps. This is materially better than a
bare 24-channel count.

Mapping policy:

- 2.0: `FL`, `FR`;
- 5.1: `FL`, `FR`, `FC`, `LFE`, `SL`, `SR` using the OpenJOC semantic order;
- 7.1.4, 9.1.6, and other presets: map every OpenJOC label to a declared
  `GstAudioChannelPosition` and reject any label for which the mapping is not
  exact;
- 22.2: carry all 24 channels with explicit GStreamer positions, including
  both LFE positions where the GStreamer version exposes them. Do not collapse
  LFE1/LFE2 into one LFE or silently relabel a wide/top channel;
- binaural: two channels with `GST_AUDIO_CHANNEL_POSITION_FRONT_LEFT` and
  `FRONT_RIGHT` (or the framework's binaural positions if the selected version
  and downstream support them), with stable left-ear/right-ear order.

### JOC identification and fallback

GStreamer caps identify E-AC-3, not JOC. The plugin must not advertise itself as
an ordinary drop-in E-AC-3 decoder at a rank that causes automatic hijacking.
The first release should use one of these explicit policies:

1. an application probes the stream and chooses `openjocau ! openjocdec` only
   for an admitted JOC stream; or
2. the plugin is explicitly selected with `joc-only=true`, inspects the AU, and
   returns a structured unsupported/error result for non-JOC input while the
   application keeps the normal E-AC-3 branch available.

The plugin must fail closed for ordinary E-AC-3, malformed packets, unsupported
JOC profiles, absent required EMDF/OAMD/JOC payloads, profile changes, and
unsupported topologies. It must not produce a base-only or guessed spatial
render.

### Distribution and license

GStreamer has a real runtime plugin model. A plugin can be built as a shared
library, registered at load time, placed in a per-user or application-bundled
plugin directory, and discovered through the normal plugin search path or
`GST_PLUGIN_PATH`. Users do not need to rebuild the GStreamer core or the whole
player.

Official GStreamer releases cover Linux package ecosystems and provide
Windows/macOS installers; Cerbero supports building packages for Linux, macOS,
and Windows. The plugin must still be built for the host GStreamer ABI and
architecture, and its OpenJOC library must be shipped beside it or made
discoverable.

GStreamer and its official plugin model are LGPL-oriented. The plugin code
should be released under an LGPL-compatible policy if upstream inclusion is
desired. OpenJOC remains Apache-2.0 and should stay a separate library/API
boundary. This is a factual compatibility direction, not legal advice; the
exact static/dynamic link and distribution form should be reviewed before
release.

### Assessment

Classification: **OUT_OF_TREE_PRACTICAL** and potentially upstreamable as a
separate plugin if the plugin's licensing and maintenance expectations are met.

Implementation thickness: **small-to-medium**. The decoder wrapper is thin;
the non-optional additional work is the AU packetizer, explicit JOC admission,
caps/channel-position mapping, timestamp conversion, and lifecycle tests.

## Candidate audit: mpv

mpv does not provide a practical independent codec layer for this use case. Its
technical overview identifies `ad_lavc.c` as the audio decoder using FFmpeg, and
the build requires FFmpeg libraries including libavcodec/libavformat and audio
filter libraries. The normal `--ad` selection is decoder-name selection within
that FFmpeg-backed path, not a general independently loadable decoder ABI.

Practical choices are:

- add OpenJOC to FFmpeg, then let mpv use the new FFmpeg decoder;
- add an mpv-specific audio decoder/filter path, which requires mpv source and
  internal audio-frame integration;
- use a libmpv/application-specific external audio path, which no longer makes
  OpenJOC a normal mpv decoder and complicates A/V synchronization, seek, and
  fallback.

mpv's manual has useful channel-layout and audio-output controls, including
  explicit layout selection and stereo fallback, but those controls do not solve
  codec ownership. Its audio output can accept many formats and may insert
  conversion filters; that is downstream policy, not a JOC integration route.

mpv is therefore not a sensible first adapter. It becomes attractive only after
the FFmpeg decoder path exists, or if a specific mpv fork is the product target.

Classification: **UPSTREAM_REQUIRED** for a normal mpv decoder;
**OUT_OF_TREE_PRACTICAL** only for an application-specific libmpv/audio hook,
which is not recommended as the first OpenJOC integration.

## Candidate audit: VLC

VLC has a genuine module architecture. libVLC is modularized into runtime-loaded
plugins, and codec modules receive packetized decoder data through the decoder
module API, queue PCM audio, and have flush/drain lifecycle hooks in the current
source architecture. A native OpenJOC audio decoder module is conceptually:

```text
VLC E-AC-3 packetizer/block
    -> JOC AU accumulator and admission
    -> OpenJocSession
    -> decoder-owned audio block
    -> VLC audio output
```

The fit is reasonable, but weaker than GStreamer for this first proof:

- the decoder callback's block boundary is not proof of an OpenJOC AU;
- VLC's module headers and module ABI are tied to the VLC/libVLC version and
  source build rather than being a small, separately versioned codec ABI;
- exact decoder latency propagation is less explicit than GStreamer's
  `gst_audio_decoder_set_latency` plus latency message/query model;
- current VLC module/plugin license status must be checked per module; libVLC
  is LGPL2.1, while the complete VLC/player distribution and some modules have
  broader GPL history;
- deterministic integration testing is possible, but a silent/dummy audio
  output test is more host-specific than a GStreamer harness plus appsink.

PCM output can carry ordinary channel counts and VLC audio channel masks, but the
first adapter should not claim that VLC's downstream path preserves every
OpenJOC semantic label through 9.1.6 or 22.2 without a version-specific channel
map audit. Binaural stereo is the safe portable mode.

Classification: **OUT_OF_TREE_PRACTICAL** for a version-pinned VLC module;
**BOTH_PRACTICAL** only if a later upstream module owner accepts the maintenance
and license requirements.

## Candidate audit: DirectShow / LAV Filters

This is a valid Windows-specific option, not a cross-platform first target.

### Filter shape

A native DirectShow transform decoder can receive compressed `IMediaSample`
objects, read their byte payload and timestamps, create output samples, and set
the output media type to float PCM. DirectShow's sample API includes stream
start/end times, media time, sync point, preroll, and discontinuity flags. Flush
and new-segment handling provide the necessary seek/reset hooks.

An independently shipped `OpenJocDecoder.ax` can therefore be placed between a
splitter and an audio renderer without modifying LAV. It must still:

- aggregate complete JOC AUs rather than trusting sample boundaries;
- preserve the I0 PTS when D0 arrives in a later sample;
- use `BeginFlush`/`EndFlush` and new segment state to call OpenJOC reset;
- drain OpenJOC before propagating EOS;
- negotiate float PCM and exact channel metadata;
- register as a decoder only under an explicit JOC selection policy where
  possible, because `MEDIASUBTYPE_EAC3` is also ordinary E-AC-3.

### LAV relationship

LAV Filters are a GPL-2.0 DirectShow splitter/decoder suite based on FFmpeg.
The repository documents install-by-unpack/register behavior, high decoder merit,
and a build that uses a custom FFmpeg fork. A separate OpenJOC DirectShow filter
does not require modifying LAV, but making LAV Audio itself call OpenJOC would
require a LAV source change/fork and its GPL build/distribution context.

The LAV project's own issue history confirms that its ordinary E-AC-3 path does
not automatically mean JOC metadata is spatially rendered: its existing path
has historically treated E-AC-3 JOC as the base-channel fallback rather than an
OpenJOC-style metadata render. That is evidence of user value for a Windows
adapter, but also evidence that the adapter must be explicit and must not claim
normal LAV decoding already provides the requested result.

### PCM layout limits

DirectShow uses media types and `WAVEFORMATEX`/`WAVEFORMATEXTENSIBLE`. The
channel mask is useful for conventional 2.0, 5.1, 7.1, and several height
positions. It is not a complete portable semantic contract for the repository's
24-channel 22.2 label set, especially dual LFE and all wide/top positions.
Twenty-four samples can be transported as a count, but a downstream player may
see an unspecified or nonstandard mask. This is a major penalty relative to
GStreamer.

### Assessment

Classification: **OUT_OF_TREE_PRACTICAL** as an independent Windows `.ax`
filter; **UPSTREAM/modified-LAV work required** if the goal is to make LAV
Audio itself own the decoder route.

It has meaningful Windows reach, including DirectShow players such as
MPC-family applications and PotPlayer configurations that accept external
filters, but it is platform-locked, legacy according to Microsoft, and weak for
cross-platform OpenJOC strategy.

## Cross-candidate policies

### Ordinary E-AC-3 and unsupported input

The winning adapter must use this policy:

| Input | Required behavior |
|---|---|
| Ordinary E-AC-3, no admitted JOC carrier | Do not select OpenJOC; use host decoder or return an explicit unsupported result |
| E-AC-3 with malformed JOC signaling | Fail closed; do not render base-only or guessed spatial PCM |
| E-AC-3 JOC outside current OpenJOC profile/topology | Return unsupported and permit host fallback where the framework supports it |
| Valid admitted JOC | Pass exactly one complete AU per OpenJOC call |
| Format/profile change within session | Stop/reconfigure/reset; do not silently change renderer semantics |
| Seek/preroll | Flush old output, reset state, decode preroll to prime, discard preroll output, then present the new segment |

The first plugin should be explicit rather than trying to make a generic
autoplugger guess from `audio/x-eac3`.

### Speaker and binaural policy

The first adapter should expose both modes but default to:

```text
render-mode = speaker
speaker-layout = 5.1
dialnorm = default
drc = line
validation-profile = auto
```

Do not default to 22.2 merely because the renderer supports it. The host
application should select a speaker preset that matches the downstream device
or explicitly choose binaural. Binaural is the portable two-channel fallback
when the host/device cannot negotiate the requested immersive speaker layout.
That fallback must be an application decision, not hidden renderer behavior.

Custom SOFA is deliberately deferred from the first configuration surface. The
built-in generic HRTF makes basic binaural playback possible without a file
picker or a host-specific resource path. A later property can accept a
caller-owned SOFA byte buffer/path once the host configuration model is proven.

### Real-time performance and copying

The desired path is:

```text
complete compressed JOC AU
    -> OpenJOC
    -> interleaved F32 PCM
    -> host audio pipeline
```

GStreamer can accept F32LE interleaved directly; no WAV/container round-trip or
intermediate sample-format conversion is needed. One copy into a host-owned
GStreamer output buffer is required by the current C ABI lifetime. The AU
accumulator is bounded to one pending AU plus one lookahead syncframe. No global
queue or program-length spool is allowed.

The adapter must not enable offline sample-peak normalization. It should use
OpenJOC's default calibrated dialnorm behavior and let the host's normal volume
and device policy remain downstream.

## Weighted scorecard

Weights use the requested 100-point distribution. Scores are 0–10, and the
weighted total is `criterion weight × score / 10`.

| Candidate | API fit 20 | Size 15 | Cross-platform 15 | Reach 15 | Test 10 | Independent deploy 10 | Timestamp/latency 5 | Layout 5 | License/maintenance 5 | Total / 100 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| **GStreamer** | 9 | 8 | 10 | 8 | 10 | 9 | 10 | 10 | 9 | **90.5** |
| **FFmpeg-facing** | 7 | 4 | 10 | 10 | 9 | 3 | 8 | 9 | 6 | **73.5** |
| **VLC** | 7 | 6 | 9 | 9 | 7 | 7 | 7 | 6 | 6 | **72.5** |
| **DirectShow/LAV** | 8 | 6 | 1 | 7 | 7 | 8 | 8 | 4 | 2 | **59.0** |
| **mpv** | 3 | 3 | 9 | 9 | 6 | 2 | 6 | 8 | 4 | **55.0** |

The GStreamer score is not based on a claim that the packet problem disappears.
It wins because the packetizer, decoder lifecycle, timestamps, latency query,
caps, channel positions, plugin deployment, and deterministic test harness all
have explicit framework homes without rebuilding a large host stack.

## Next-session implementation plan for the winner

### Module and source tree

Create a standalone integration package at:

```text
integrations/gstreamer/openjoc-plugin/
    meson.build
    include/
    src/gstopenjocplugin.c
    src/openjocau.c
    src/openjocau.h
    src/openjocdec.c
    src/openjocdec.h
    tests/
    README.md
```

Keep it as a separate integration build initially so the base Rust workspace
does not require GStreamer development packages for every ordinary OpenJOC
build. Add explicit CI jobs for the integration rather than silently making
`cargo test --workspace` depend on a platform GStreamer installation.

Suggested elements:

- `openjocau`: `GstBaseParse`-based packetizer, rank none;
- `openjocdec`: `GstAudioDecoder`-based decoder, rank none until explicit JOC
  admission/autoplug selection is available;
- one shared plugin library, `libgstopenjoc`/platform equivalent.

### Build integration

- Build `openjoc-capi` with Cargo for the target platform.
- Link the plugin against the published `openjoc.h` and the corresponding
  `libopenjoc_capi` artifact.
- Use Meson to discover GStreamer core/audio/base development libraries and to
  produce the shared plugin.
- Build/test on Linux first, then macOS universal/arm64, then Windows MSVC
  using the official GStreamer development/runtime packages.
- The runtime package contains the plugin plus the matching OpenJOC shared
  library; the host GStreamer installation remains otherwise untouched.

### OpenJOC API entry points

Use only the published C ABI in the first plugin:

```text
openjoc_decoder_config_init
openjoc_decoder_create
openjoc_decoder_get_output_info
openjoc_decoder_send_packet
openjoc_decoder_receive_frame
openjoc_decoder_drain
openjoc_decoder_flush / openjoc_decoder_reset
openjoc_decoder_last_error
openjoc_decoder_get_channel_label
openjoc_decoder_destroy
```

Initialize `render_mode`, `speaker_layout`, DRC, validation profile, and
dialnorm explicitly. Leave SOFA empty for the initial binaural mode so the
bundled generic HRTF is selected.

### Packet flow

```text
demuxer
  -> audio/x-eac3
  -> ac3parse
  -> openjocau: syncframe accumulator
       [I0 + optional D0]
  -> openjocdec
       one openjoc_decoder_send_packet() per AU
  -> receive_frame() until no frame is pending
  -> GstBuffer audio/x-raw F32LE interleaved
```

`openjocau` must retain only a bounded current AU and one next independent
syncframe. It must preserve the I0 PTS and duration, mark a discontinuity on the
first AU after a framework discontinuity, and reject an AU that violates the
current one-I0/optional-D0 contract.

### PCM flow and ownership

- Query the configured output info after decoder creation and after the first
  format is known.
- Allocate one host-owned `GstBuffer` for each returned OpenJOC frame.
- Copy `data_len` float samples from the C ABI frame before the next
  `send/receive` call.
- Set buffer duration from `sample_count / sample_rate` and preserve the
  OpenJOC logical PTS converted back to nanoseconds.
- Use output caps with `format=F32LE`, `layout=interleaved`, exact `rate`, exact
  `channels`, and the exact semantic channel mask.

### Timestamp and latency flow

- Convert incoming GStreamer nanosecond PTS to sample PTS with checked rational
  arithmetic; do not use floating-point rounding for the sample-domain value.
- Pass the first AU PTS as `pts_samples`.
- For contiguous AUs, pass the expected origin-plus-sample-count PTS.
- On an input discontinuity, set `OPENJOC_PACKET_FLAG_DISCONTINUITY` on the
  first new AU.
- Convert returned `pts_samples` back to nanoseconds with checked arithmetic.
- Set equal min/max GStreamer decoder latency to 609 samples for speaker/stereo
  or 577 samples for binaural.
- Keep the output PTS on the logical sample timeline; do not add the latency to
  every buffer timestamp.

### Flush, reset, EOS, and drain

- On `FLUSH_START`, stop the packetizer and discard pending input/output.
- On `FLUSH_STOP`, call `openjoc_decoder_flush` (or reset for a new stream),
  clear the segment origin, and require a new segment before accepting normal
  data.
- On EOS, finalize a pending I0 AU if it is complete, call
  `openjoc_decoder_drain`, receive every frame, then return/propagate EOS.
- On seek, flush before the new segment, pass preroll AUs with the preroll
  policy, receive and discard their output while priming, then begin emitting
  at the first non-preroll AU.
- On stream/file switching, destroy or reset the old handle and create a fresh
  session; never carry parser, JOC profile, QMF, SOFA, or FIR state across files.

### Configuration surface

Initial properties:

```text
render-mode: speaker | stereo | binaural       (default speaker)
speaker-layout: 5.1, 7.1.4, ..., 22.2          (default 5.1)
drc: disabled | line | rf | custom             (default line)
drc-boost-percent / drc-cut-percent            (only custom)
dialnorm: default | digital | analog           (default default)
validation-profile: auto | etsi-strict | observed-vendor-compat
lfe-policy: exclude | equal-power-dual-mono    (binaural only)
joc-only: true                                  (first release)
```

Do not expose offline peak normalization. Do not expose a custom SOFA file in
the first minimal configuration; the built-in generic HRTF is sufficient to
prove the binaural route.

### Error and fallback behavior

- ordinary E-AC-3: explicit `not-supported`/non-admitted result; host chooses
  its normal decoder;
- missing or malformed JOC metadata: hard error for the explicit OpenJOC path;
- unsupported profile/topology: hard error with `openjoc_decoder_last_error()`;
- malformed/truncated AU: packetizer hard error, no partial decode;
- `OUTPUT_PENDING`: receive all frames before sending another AU;
- format/profile change: stop/reconfigure/reset, never silently continue;
- no hidden base-only fallback and no platform spatial renderer.

### Minimal automated tests

Use a repository-approved synthetic legal JOC fixture generated by existing
public-syntax test helpers, or an authorized public fixture if one is later
added. Do not commit private programme media. The GStreamer test pipeline is:

```text
appsrc (deliberately fragmented input)
  -> openjocau
  -> openjocdec
  -> appsink
```

Required assertions:

1. split input chunks reconstruct complete syncframes;
2. one I0 and optional D0 become exactly one OpenJOC send;
3. multiple AUs do not become one OpenJOC send;
4. ordinary E-AC-3 is rejected by the explicit JOC path;
5. 48 kHz sample rate is negotiated;
6. speaker output reports the expected channel count, labels, and 609-sample
   latency;
7. binaural output reports two channels and 577-sample latency;
8. output PTS values are monotonic and map back to sample-domain values;
9. PCM is finite and nonzero for the fixture;
10. EOS emits all delayed/tail frames before EOS reaches appsink;
11. flush/segment restart produces the same result as a fresh session for the
    post-seek region;
12. repeated independent pipeline instances do not share state;
13. 22.2 caps carry 24 channels and exact semantic positions when selected;
14. malformed, profile-changing, and unsupported-topology packets fail closed.

Add a separate host-level test for `openjoc_decoder_get_output_info` and ABI
ownership, because the plugin test alone cannot prove the C pointer lifetime
contract.

### Human acceptance test

Use a small real GStreamer player application that explicitly selects the
OpenJOC branch after stream admission. Test on macOS first, then repeat on
Linux and Windows:

- playback starts without an audible truncated lead-in;
- A/V sync remains stable for at least ten minutes;
- pause/resume does not duplicate or drop a delayed frame;
- seek into the middle of the file primes state and resumes at the requested
  segment without audible stale output;
- reverse/rapid repeated seeks do not deadlock the streaming thread;
- EOS drains the QMF/FIR/linked-gain tail once and then stops;
- switching between two JOC files resets profile, QMF, speaker, and HRTF state;
- ordinary E-AC-3 opens through the normal host decoder instead of the OpenJOC
  path;
- speaker mode negotiates 5.1 and a selected immersive preset when the device
  accepts it;
- binaural mode gives stable two-channel output when the device cannot accept
  the selected speaker layout;
- CPU use is measured in release mode and remains practical for real-time 48
  kHz playback;
- no sample-peak normalization or platform spatial DSP is inserted.

## Pre-adapter API gaps

These are the only gaps identified as materially relevant to the first adapter.

### P0_REQUIRED_BEFORE_ADAPTER: explicit JOC admission/probe

Add a small public admission operation before production adapter work, for
example a Rust `probe_joc_access_unit` plus a C ABI equivalent. It should:

- inspect a complete AU without committing decoder/render state;
- distinguish ordinary E-AC-3, admitted JOC, malformed JOC, and unsupported JOC;
- report the selected validation profile/complexity and the reason for
  rejection;
- not decode base PCM merely to discover that JOC metadata is absent.

This is required for clean ordinary-E-AC-3 fallback in an autoplugging or player
selection path. An explicitly selected first proof pipeline can temporarily
use `joc-only` and fail closed, but it must not be promoted as a general decoder
until admission is explicit.

### P1_USEFUL: output discard/priming policy

Add an optional session operation or output-frame policy that marks/discards
frames generated from preroll input while retaining decoder state. The first
GStreamer adapter can implement this at its own boundary by draining and
discarding output after each preroll send, so it is not a blocker for the first
explicit pipeline.

### P1_USEFUL: transferable output ownership

The current C ABI pointer lifetime is correct but forces a copy into a host
buffer. A later ABI minor could offer a transfer/callback form for owned PCM
buffers. This is not required for the first adapter and must not trigger a
premature session redesign.

### NOT_REQUIRED

- general container demuxing in OpenJOC;
- a platform spatial-renderer bridge;
- offline normalization in the session;
- a custom SOFA UX for the first player plugin;
- a global session lock;
- arbitrary multiple-dependent-substream JOC support before the first adapter;
- a new renderer abstraction.

## Final classification

| Candidate | Classification | Decision |
|---|---|---|
| GStreamer | `OUT_OF_TREE_PRACTICAL` | **PRIMARY_FIRST_ADAPTER** |
| FFmpeg native decoder | `UPSTREAM_REQUIRED` | later, after external wrapper evidence |
| FFmpeg demux/OpenJOC wrapper | `OUT_OF_TREE_PRACTICAL` | **SECONDARY_FOLLOWUP_ADAPTER** |
| VLC module | `OUT_OF_TREE_PRACTICAL` | later follow-up if VLC reach becomes a priority |
| DirectShow/LAV | `OUT_OF_TREE_PRACTICAL`, Windows-only | later Windows-specific adapter |
| mpv normal decoder | `UPSTREAM_REQUIRED` | defer until FFmpeg route exists |

The choice is therefore concrete: implement GStreamer first, then use the
FFmpeg-facing wrapper to learn the higher-reach native codec integration
requirements. No adapter implementation is authorized by this audit.

## Primary-source references

The following are the official/current sources used for the framework audit:

- [FFmpeg decoding API](https://ffmpeg.org/doxygen/trunk/group__lavc__decoding.html)
- [FFmpeg codec core and registered-codec lookup](https://ffmpeg.org/doxygen/trunk/group__lavc__core.html)
- [FFmpeg parser API](https://ffmpeg.org/doxygen/trunk/group__lavc__parsing.html)
- [FFmpeg channel layout API](https://ffmpeg.org/doxygen/trunk/channel__layout_8h_source.html)
- [FFmpeg `AVCodecContext::delay`](https://ffmpeg.org/doxygen/trunk/structAVCodecContext.html)
- [FFmpeg legal/licensing guidance](https://ffmpeg.org/legal.html)
- [FFmpeg developer/API policy](https://ffmpeg.org/developer.html)
- [GStreamer `GstAudioDecoder`](https://gstreamer.freedesktop.org/documentation/audio/gstaudiodecoder.html)
- [GStreamer `GstBaseParse`](https://gstreamer.freedesktop.org/documentation/base/gstbaseparse.html)
- [GStreamer `ac3parse`](https://gstreamer.freedesktop.org/documentation/audioparsers/ac3parse.html)
- [GStreamer audio channel positions](https://gstreamer.freedesktop.org/documentation/audio/gstaudiochannels.html)
- [GStreamer raw audio caps](https://gstreamer.freedesktop.org/documentation/additional/design/mediatype-audio-raw.html)
- [GStreamer event/flush/segment design](https://gstreamer.freedesktop.org/documentation/additional/design/events.html)
- [GStreamer plugin licensing advisory](https://gstreamer.freedesktop.org/documentation/plugin-development/appendix/licensing-advisory.html)
- [GStreamer plugin boilerplate and runtime registration](https://gstreamer.freedesktop.org/documentation/plugin-development/basics/boiler.html)
- [GStreamer downloads and platform packaging](https://gstreamer.freedesktop.org/download/)
- [mpv technical overview](https://github.com/mpv-player/mpv/blob/master/DOCS/tech-overview.txt)
- [mpv build dependencies and license](https://github.com/mpv-player/mpv/blob/master/README.md)
- [mpv audio and channel-layout manual](https://mpv.io/manual/stable/)
- [VideoLAN libVLC/module architecture](https://images.videolan.org/vlc/libvlc.html)
- [VideoLAN legal/licensing page](https://images.videolan.org/legal.html)
- [VLC current codec module source](https://github.com/videolan/vlc/blob/master/include/vlc_codec.h)
- [Microsoft DirectShow filter architecture](https://learn.microsoft.com/en-us/windows/win32/directshow/about-directshow-filters)
- [Microsoft `CTransformFilter::Transform`](https://learn.microsoft.com/en-us/windows/win32/directshow/ctransformfilter-transform)
- [Microsoft DirectShow media samples](https://learn.microsoft.com/en-us/windows/win32/directshow/cmediasample)
- [Microsoft DirectShow media types](https://learn.microsoft.com/en-us/windows/win32/directshow/about-media-types)
- [Microsoft DirectShow timestamps](https://learn.microsoft.com/en-us/windows/win32/directshow/time-stamps)
- [Microsoft DirectShow samples and allocators](https://learn.microsoft.com/en-us/windows/win32/directshow/samples-and-allocators)
- [Microsoft `WAVEFORMATEXTENSIBLE`](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ksmedia/ns-ksmedia-waveformatextensible)
- [LAV Filters repository, install/build/license information](https://github.com/Nevcairiel/LAVFilters)
- [LAV Filters spatial-audio issue documenting ordinary JOC fallback behavior](https://github.com/Nevcairiel/LAVFilters/issues/344)

