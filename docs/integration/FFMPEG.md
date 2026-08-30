# FFmpeg-facing external bridge

OpenJOC provides an experimental external bridge for applications embedding
FFmpeg. `openjoc-ffmpeg` uses libavformat for demux and packet transport, the
Rust `OpenJocSession` directly for all decode/render work, and public
libavutil APIs for owned `AVFrame` output.

This is not a decoder plugin for an installed `ffmpeg` executable. FFmpeg has
no documented stable out-of-tree decoder-plugin ABI comparable to GStreamer's
loadable element model. Stock FFmpeg therefore does not gain
`-c:a openjoc`; no FFmpeg source, registration table, parser, demuxer, or
configure script is patched here. The implemented native libavcodec source
wrapper is documented separately in [FFMPEG_NATIVE.md](FFMPEG_NATIVE.md).

## Supported FFmpeg baseline

The minimum supported baseline is FFmpeg 9.0:

| Component | Minimum | Locally tested |
| --- | --- | --- |
| FFmpeg release | 9.0 | 9.0.1 (2026-08-12) |
| libavutil | 61 | 61.1.101 |
| libavcodec | 63 | 63.1.101 |
| libavformat | 63 | 63.1.101 |

FFmpeg 9 is selected deliberately. It provides the current `AVChannelLayout`
model, `AVFrame.duration`, true `AV_CHAN_BINAURAL_LEFT` /
`AV_CHAN_BINAURAL_RIGHT` identities, and the predefined binaural and 22.2
layouts. Supporting an older line would either lose exact ear semantics or
add version branches without improving the first integration.

The implementation was also audited against official FFmpeg master commit
`3bdd895832244780c250713e49135615ac4de003` (2026-08-19): libavutil 61.5.100,
libavcodec 63.8.101, and libavformat 63.6.100. The public functions and fields
used by this bridge have the same contract on that head. Current source and
API references are the official [download page](https://ffmpeg.org/download.html),
[libavformat demux documentation](https://ffmpeg.org/doxygen/trunk/group__lavf__decoding.html),
[send/receive documentation](https://ffmpeg.org/doxygen/trunk/group__lavc__decoding.html),
[channel-layout header](https://ffmpeg.org/doxygen/trunk/channel__layout_8h_source.html),
and [AVFrame allocation source](https://ffmpeg.org/doxygen/trunk/frame_8c_source.html).
The feature check and complete bridge test suite passed against a locally built
minimal shared SDK from that exact development commit.

## Build

FFmpeg support is opt-in. A generic workspace build does not compile or
advertise a bridge artifact. On Unix-like hosts, install FFmpeg 9 development
files providing these pkg-config modules:

- `libavutil >= 61`;
- `libavcodec >= 63`;
- `libavformat >= 63`.

Then run the authoritative integration path:

```sh
cargo build -p openjoc-ffmpeg --release --features ffmpeg --locked
cargo test -p openjoc-ffmpeg --features ffmpeg --locked
scripts/verify-ffmpeg.sh
```

The Rust bridge core builds without FFmpeg:

```sh
cargo build -p openjoc-ffmpeg --locked
```

The feature build uses a narrow C interop translation unit compiled against
the host's public FFmpeg headers. It exists to keep ABI-owned structures and
allocation in C; Rust calls `OpenJocSession` directly and never routes back
through OpenJOC's C ABI.

## Architecture and ownership

```text
container / raw stream
        ↓
libavformat → borrowed AVPacket bytes + AVStream.time_base
        ↓
bounded packet-to-AU assembler and positive JOC admission
        ↓
OpenJocSession
        ↓
owned interleaved f32 PCM → semantic channel permutation
        ↓
public-API-allocated AVFrame / AVBuffer
```

`PacketRef` borrows packet bytes only for `send_packet`. Bytes that must
survive the call are copied into at most 131,072 bytes of compressed staging.
No `AVPacket *` is retained. The wrapper output queue is bounded to frames
from one session send/drain batch; a caller that does not receive sees
`WouldBlock`. `AvFrame` owns a normal `AVFrame` whose packed audio allocation
comes from `av_frame_get_buffer`; it remains valid independently of the
temporary OpenJOC PCM vector.

One wrapper is serially accessed by one caller. Separate wrappers contain
separate AU, decoder, FinalLinkedGain, HRTF, and timing state and may run on
separate threads. There is no global decoder lock or mutable global HRTF state.

## Packet and access-unit semantics

libavformat defines a packet as containing one or more encoded frames, so the
bridge never equates an `AVPacket` with one OpenJOC AU. The shared OpenJOC
E-AC-3 primitives parse:

```text
independent substream I0 + optional dependent substream D0 = one admitted AU
```

The assembler handles an AU split across packets, I0 and D0 in different
packets, and multiple AUs in one packet. An independent-only AU remains
pending until the next I0 proves its boundary or EOF closes it. Maximum frame,
AU, and staging sizes are explicit; whole programmes are never buffered.

For standards-defined flat-7.X JOC, downmix index 1 is admitted only with the
seven-input Table-47 order `L R C Ls Rs Lrs Rrs`. The bridge retains two
separate internal PCM meanings: I0-only compatibility PCM and assembled
I0+D0 JOC reconstruction-input PCM. Stereo uses the existing I0 compatibility
Lo/Ro or Lt/Rt matrix, including its normative `1 / max_sum` overflow scale;
the rear D0 pair is not directly downmixed. Expanded speaker rendering keeps
using all seven reconstruction inputs. Legacy AC-3 core plus E-AC-3 D0 is
outside this carriage contract.

Raw E-AC-3, MP4 EC-3, and Matroska packetization may differ by demuxer and
file. The same assembler handles all three. Automated acceptance covers a
positive synthetic raw JOC stream plus ordinary E-AC-3 controls demuxed from
raw, MP4, and Matroska. A private real raw programme is also accepted locally.
No redistributable JOC-in-MP4 or JOC-in-Matroska fixture is currently present,
so those positive container cases remain a fixture-dependent acceptance step:

```sh
target/release/openjoc-avdecode input.mp4 --binaural --null --checksum
target/release/openjoc-avdecode input.mkv --layout 7.1.4 --null --checksum
```

## JOC admission and errors

Admission uses `index_syncframes`, `group_access_units`, full audio-frame
parsing, `parse_joc_access_unit`, and the existing strict/vendor-compatible
JOC validators. It exposes `UNKNOWN`, `CONFIRMED_JOC`,
`CONFIRMED_NON_JOC`, and `INVALID_OR_UNSUPPORTED`.

`CONFIRMED_NON_JOC` returns `NotJoc`; no OpenJOC session is created and no PCM
is emitted. Applications may route that stream to FFmpeg's normal E-AC-3
decoder outside this library. Bad sync, a truncated AU at EOF, unsupported
substream order/profile, impossible timing, and staging overflow are distinct
structured errors. Public state-machine calls contain Rust panics and poison
the affected instance rather than unwinding through an external boundary.

## Timestamps

`AVPacket.pts` is the audio presentation timestamp and is the default anchor.
It is interpreted in the selected `AVStream.time_base` and converted directly
to a 48-kHz sample position with checked integer rational arithmetic matching
`av_rescale_q` nearest rounding. No floating-point seconds or incremental
fractional accumulator is used, so absolute sequences at `1/48000`, `1/90000`,
`1/1000`, and other rational bases do not accumulate drift.

Missing PTS remains missing by default. `TimestampPolicy::PtsThenDts` is an
explicit opt-in for applications that have established that DTS equals the
audio presentation timeline; the wrapper never silently substitutes DTS.
`AV_NOPTS_VALUE` maps to absence. Once a timed segment begins, packet anchors
must agree with the exact sample count; AUs grouped inside one packet receive
the documented monotonic sample-count continuation. A segment that begins
untimed cannot acquire a timestamp part-way through without reset.

Output `AVFrame.pts` and `duration` are in `1/48000`: PTS is the logical
OpenJOC output sample position and duration is `nb_samples`. Renderer latency
is not subtracted from timestamps.

## Output format and channel semantics

Every frame is 48 kHz packed/interleaved `AV_SAMPLE_FMT_FLT`. The exact path
does not use libswresample, resample, rematrix, mix, downmix, or normalize.
Consumers that need planar PCM or another rate can use FFmpeg conversion after
the bridge.

Physical speaker stereo and binaural are different configurations. Binaural
defaults to a 7.1.4 virtual layout and the built-in SADIE II D1 HRTF, then
emits `BIL, BIR`. Speaker 2.0 emits physical `FL, FR`.

| OpenJOC target | FFmpeg representation |
| --- | --- |
| 2.0 | predefined `stereo` |
| 5.1 | predefined `5.1(side)` |
| 5.1.2 / 5.1.4 | matching predefined layouts |
| 7.1 / 7.1.2 / 7.1.4 | matching predefined layouts |
| 7.1.6 | custom order with explicit TFL/TFR, TSL/TSR, TBL/TBR |
| 9.1 / 9.1.2 / 9.1.4 / 9.1.6 | custom order with explicit WL/WR |
| 22.2 | predefined `22.2`, with deterministic permutation |
| binaural | predefined `binaural`, BIL/BIR |

FFmpeg's predefined 9.1.4/9.1.6 layouts contain FLC/FRC. OpenJOC's admitted
9.1 family contains Wide identities, so the bridge deliberately uses custom
WL/WR layouts instead of claiming false semantic equivalence.

OpenJOC 22.2 order is:

```text
FL FR FC LFE1 BL BR FLc FRc BC LFE2 SiL SiR TpFL TpFR TpFC TpC
TpBL TpBR TpSiL TpSiR TpBC BtFC BtFL BtFR
```

FFmpeg's native 22.2 order is:

```text
FL FR FC LFE BL BR FLC FRC BC SL SR TC TFL TFC TFR TBL TBC TBR
LFE2 TSL TSR BFC BFL BFR
```

The boundary permutation is
`[0,1,2,3,4,5,6,7,8,10,11,15,12,14,13,16,20,17,9,18,19,21,22,23]`.
It is a pure reorder. Inverse-reorder tests cover every public layout and
full renderer parity tests cover 22.2.

## Drain, flush, seek, latency, and preroll

`drain` closes compressed assembly, rejects a partial final AU, calls
`OpenJocSession::drain`, and emits the complete QMF, FinalLinkedGain, and
binaural convolution tails before EOF. `flush`, `reset`, or an input packet
marked as a discontinuity discards compressed staging, queued PCM, sample
timing, E-AC-3/JOC/QMF state, dialnorm state, FinalLinkedGain state, and HRTF
history.

The demux helper exposes `seek` in stream-time-base units through
`av_seek_frame`. The required application sequence is `demuxer.seek(...)`,
then `decoder.reset()`, then delivery of preroll packets. Seeking the demuxer
without resetting the wrapper is an application error.

The verified steady-state latency is 609 samples for physical speaker output
(577 QMF + 32 FinalLinkedGain) and 577 samples for binaural output. It is
reported separately as samples with a `1/48000` rational, never hidden in PTS.
After a seek, an application should start decoding at least the reported delay
before its desired audible sample when the container permits it, mark those
packets as preroll, and discard preroll output according to its own seek
policy. The bridge decodes preroll normally and does not fabricate or silently
trim samples.

The native decoder uses FFmpeg `AVCodecContext.delay` for this sample count:
it is the number of samples a decoder must output before output is valid and
the amount to decode before a seek target. The external wrapper does not create
a fake `AVCodecContext` merely to store it.

## Proof executable

`openjoc-avdecode` intentionally does only integration proof:

```sh
cargo build -p openjoc-ffmpeg --release --features ffmpeg --locked
target/release/openjoc-avdecode input.mp4 \
  --binaural --output output.wav
target/release/openjoc-avdecode input.mkv \
  --layout 7.1.4 --output output.wav
target/release/openjoc-avdecode input.ec3 \
  --layout 22.2 --null --checksum --trace
```

It opens with `avformat_open_input`, calls `avformat_find_stream_info`, selects
an E-AC-3 audio stream, reads reference-counted packets with `av_read_frame`,
and feeds only the chosen packet bytes into OpenJOC. It never calls
`avcodec_find_decoder(AV_CODEC_ID_EAC3)` or decodes JOC through libavcodec.
`--trace` reports packet timing, AU hashes/sample PTS, AVFrame timing/layout,
and latency as separate facts. WAV is a convenience sink; AVChannelLayout is
the authoritative multichannel semantic contract.
`--semantic-checksum` hashes PCM after the inverse FFmpeg transport
permutation, which is useful for direct-session and cross-framework parity.

## Platforms and licensing

The wrapper and C shim contain no CoreAudio, AudioToolbox, WASAPI, ALSA,
PipeWire, Media Foundation, DirectShow, or platform spatial renderer. macOS
and Linux feature builds use pkg-config. CI installs FFmpeg development
packages on Ubuntu and Homebrew FFmpeg on macOS. Windows core code builds
without FFmpeg. A native Windows feature job for this external bridge is not
qualified; it would require one pinned shared-development SDK exposing
compatible headers, import libraries/DLLs, and pkg-config metadata.
The reproducible audited route is the official FFmpeg 9.0.1 source under
MSYS2/MinGW-w64 with `--enable-shared`, following FFmpeg's
[Windows platform guide](https://ffmpeg.org/platform.html). The current vcpkg
port is still on FFmpeg 8.1 and cannot satisfy this bridge's minimum. A native
Windows feature build/link/run job is not yet qualified.

OpenJOC source remains Apache-2.0. FFmpeg is LGPL-2.1-or-later by default, but
optional build choices can make an FFmpeg build GPL. The locally tested
Homebrew build was configured with `--enable-gpl --enable-version3` and reports
GPLv3-or-later. The repository dynamically links the installed libraries and
does not vendor or redistribute FFmpeg, so `THIRD_PARTY_NOTICES.md` is not
changed for this source-only integration. Anyone distributing combined static
or dynamic binaries must audit the exact FFmpeg configuration and comply with
the applicable terms; see FFmpeg's official
[license guidance](https://ffmpeg.org/legal.html). This is engineering scope,
not legal advice.

## Known limits and next step

- No stock `ffmpeg` command recognizes an OpenJOC codec name.
- There is no positive redistributable JOC MP4/Matroska fixture in this tree.
- The wrapper selects the first E-AC-3 audio stream; richer stream selection
  belongs in an embedding application.
- It does not enumerate audio devices or infer headphones from two channels.
- mpv integration uses the native `libopenjoc` wrapper rather than this
  external embedding bridge.

The bridge proves packet assembly, timing, output ownership, and channel
semantics and remains the embedding/reference frontend. The additional
[native FFmpeg wrapper](FFMPEG_NATIVE.md) now exposes the same semantics through
libavcodec for custom patched FFmpeg builds. Player integration can select that
named decoder after positive JOC probing without duplicating this bridge.

## Local acceptance and performance

A complete private raw programme was run without copying it into the
repository. Binaural, 7.1.4, and 22.2 all completed through libavformat and
owned AVFrames. Full-program 22.2 direct-session, GStreamer, and FFmpeg results
were bit-identical after the FFmpeg transport permutation was inverted:
8,673 frames and 13,320,224 samples per channel (1,278,741,504 packed PCM
bytes). Binaural FFmpeg output was also bit-identical to the retained direct
session oracle.

On the local Apple-silicon macOS host, the approximately 277.5-second programme
measured:

| Path | Wall time | Approximate RTF | Maximum RSS |
| --- | ---: | ---: | ---: |
| FFmpeg bridge, binaural | 228.72 s | 0.824 | 144 MB |
| FFmpeg bridge, 7.1.4 | 82.93 s | 0.299 | 28 MB |
| direct `OpenJocSession`, 22.2 | 88.76 s decode/render | 0.320 | 54 MB process |
| FFmpeg bridge, 22.2 | 92.84 s | 0.335 | 28 MB |
| GStreamer, 22.2 streaming checksum | 88.98 s | 0.321 | 59 MB pipeline |

The instrumented FFmpeg 22.2 run attributed 83.683 s to `OpenJocSession`,
1.816 s to AU assembly plus positive admission, 0.243 s to channel reorder,
0.111 s to libavformat reads, 0.094 s to AVFrame allocation/copy, and 0.007 s
to packet timestamp/staging work. These are single-run development numbers,
not cross-machine performance guarantees. Compressed staging remains capped at
128 KiB; the wrapper retains no whole-program PCM.
