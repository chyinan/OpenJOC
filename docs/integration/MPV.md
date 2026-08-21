# mpv player integration

OpenJOC can be used as the JOC decoder in custom mpv builds linked against the
native `libopenjoc` FFmpeg integration. This is a local/source integration;
mpv does not officially support OpenJOC and no upstream submission is implied.

## Baselines and architecture

The verified baselines are mpv 0.41.0 (`41f6a645...`) and current master
(`e7191f2...` on 2026-08-20). mpv's official architecture uses demux packets,
an audio decoder wrapper, FFmpeg/libavcodec's `ad_lavc` frontend, audio channel
maps, an audio filter/output chain, and an AO. Containerized inputs follow that
boundary:

```text
demux packet
    -> bounded OpenJOC classifier (compressed metadata only)
    -> eac3 for ordinary input | libopenjoc for confirmed JOC
    -> mpv audio frame/channel-map path
    -> normal mpv AO
```

The classifier is the new framework-neutral ABI 1.3 entry point. It shares the
existing OpenJOC E-AC-3 AU parser and positive admission rules, retains at
most 131,072 compressed bytes, and never creates an OpenJOC render session.
The packets used for classification are retained as owned mpv frames and
replayed to the selected decoder in their original order. Decoder-failure
fallback is not used to identify JOC.

Raw E-AC-3 has an earlier positive-only path. The lavf format probe's
non-destructive bytes are fed incrementally to the same classifier, without
repeating prefixes and with the same 131,072-byte ceiling. Only
`CONFIRMED_JOC` can admit a raw stream whose FFmpeg probe score is otherwise
too low. That admission is carried in a dedicated codec-parameter field; it is
not inferred from a filename or hidden in `codec_tag`. The raw path retains
FFmpeg's normal E-AC-3 parser, skips `avformat_find_stream_info`, and prevents
timestamp seeking from consuming a bounded one-AU input before delivery.

The player patch is optional. If `pkg-config` cannot find `openjoc`, mpv builds
with its normal decoder-selection behavior and no OpenJOC source dependency.
The native FFmpeg decoder remains explicitly named `libopenjoc`; mpv does not
globally reorder it ahead of `eac3`.

## Decoder policy

| Stream/request | Result |
| --- | --- |
| ordinary E-AC-3, no override | positive classifier rejects JOC; stock `eac3` |
| confirmed raw JOC, no override | pre-demux positive admission; `libopenjoc` |
| confirmed container JOC, no override | packet classifier admits JOC; `libopenjoc` |
| `--ad=libopenjoc` | normal mpv explicit override; useful for debugging |
| `--ad=eac3` | normal mpv explicit stock-decoder override |
| `--audio-spdif=eac3` | mpv compressed passthrough; OpenJOC is bypassed |
| non-E-AC-3 | unchanged mpv decoder selection |

Selecting `libopenjoc` explicitly is a positive player decision, so the patch
also enables FFmpeg's experimental decoder compliance flag for that decoder
only. It does not alter strictness for other FFmpeg decoders.

## Output policy

Decoder choice and spatial output target are independent. OpenJOC renders the
requested target before mpv transports the resulting PCM; mpv is not asked to
downmix a 7.1.4 render into a stereo or 5.1 target.

The small product surface is the native decoder AVOption set forwarded through
`--ad-lavc-o`:

```text
render_mode=speaker|stereo|binaural
speaker_layout=2.0|5.1|7.1.4|9.1.6|22.2|...
virtual_layout=7.1.4|...
drc=disabled|line|rf|custom
dialnorm=default|digital|analog
sofa=/absolute/path/to/file.sofa
```

There are deliberately no mpv options duplicating every OpenJOC DSP control.
Use `--audio-channels` to state exact mpv output intent where a physical target
is required. `auto-safe` and broad AO capabilities do not imply headphones or
any particular physical speaker layout.

Useful exact-target examples are in
[`integrations/mpv/README.md`](../../integrations/mpv/README.md). mpv 0.41.0's
built-in layout table has `22.2` but not named `7.1.4`/`9.1.6` entries, so the
latter are expressed as explicit semantic channel lists. A two-channel
physical speaker render uses `speaker_layout=2.0`; it is not binaural.

## Channel semantics

The native FFmpeg decoder reports truthful `AVChannelLayout` identities. The
mpv patch adds custom-layout conversion for physical layouts and recognizes
FFmpeg's `BIL`/`BIR` identities. mpv's portable AO channel model has no
platform-independent ear endpoints, so the already-rendered BIL/BIR streams
are represented as ordinary stereo only at the transport map. The samples are
not mixed, crossfed, HRTF-processed, or otherwise changed; the source AVFrame
remains truthfully labelled.

Physical 7.1.4 reaches the null AO as 12 channels, 9.1.6 as 16 channels, and
22.2 as 24 channels when the corresponding exact channel map is requested.
The focused harness rejects a post-render `libswresample` Remix on the exact
7.1.4 and 22.2 paths.

## Timing and lifecycle

The native decoder's reported delay is consumed through normal FFmpeg decoder
semantics. mpv does not add an OpenJOC-specific `--audio-delay`. Seek/reset
uses libavcodec's normal flush path; the OpenJOC decoder owns the reset of AU,
QMF, FinalLinkedGain, and HRTF state. Pause/resume does not reset the decoder.

The classifier is instance-local and is discarded with the decoder wrapper. It
is not cached by filename. Its retained packet queue is bounded and replayed
before new demux packets are read, preventing first-packet loss.

## Verification

The source-only patch commits are exported under `integrations/mpv/patches/`.
The local harness verifies:

- `--ad=help` exposes both `eac3` and `libopenjoc` with the patched FFmpeg;
- ordinary E-AC-3 positive non-JOC classification and stock decoder selection;
- raw single- and multi-AU positive pre-admission and automatic `libopenjoc`;
- MP4 packet classification/replay and automatic `libopenjoc`;
- byte-identical rendered WAVE output for the raw and MP4 one-AU controls;
- explicit `--ad=eac3` selection on positively identified raw JOC;
- binaural stereo transport;
- physical 7.1.4/22.2 channel counts without an output Remix;
- explicit E-AC-3 passthrough.

```sh
integrations/mpv/verify-player.sh /path/to/patched/mpv /path/to/fixtures
```

The repository does not contain programme media. Public synthetic/legal
fixtures or local private media may be supplied through the second argument;
private files, derived PCM, and identifying hashes must remain outside the
repository.

## Known constraints

- A custom FFmpeg build and OpenJOC C ABI are required for the feature.
- Positively admitted raw JOC is exposed as forward-only because raw E-AC-3
  has no reliable timestamp seek surface; containerized JOC retains normal
  container seeking.
- No device-name heuristic infers headphones. Binaural is explicit through
  `render_mode=binaural`.
- Physical multichannel playback requires an AO/device that genuinely accepts
  the requested map. `--ao=null` is the deterministic non-hardware check.
- Standalone FFmpeg and mpv builds remain source integrations. The 0.9.1
  OpenJOC Player Bundles contain project-provided custom builds for the
  qualified platforms, but are not official upstream mpv/FFmpeg releases and
  no upstream mpv change has been submitted.
