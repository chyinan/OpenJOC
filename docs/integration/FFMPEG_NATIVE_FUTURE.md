# Future native FFmpeg OpenJOC decoder wrapper

This is a design note, not an implementation. No FFmpeg source is modified by
the current repository.

A future FFmpeg-source integration would add one small `FFCodec` wrapper whose
private context owns an `OpenJocSession` and the already-proven bounded AU
assembler. Configure and `codec_list.c` generation would register it at build
time; there is no external runtime registration ABI to target.

The minimum native surface is:

- explicit decoder selection for E-AC-3/JOC rather than replacing ordinary
  E-AC-3 globally;
- positive JOC admission before session creation;
- `send_packet` copying/referencing compressed bytes only as FFmpeg ownership
  permits and `receive_frame` preserving EAGAIN/EOF rules;
- public AVOptions for render mode, physical or virtual layout, DRC, dialnorm,
  validation profile, HRTF, and LFE policy, all producing the shared effective
  config fingerprint;
- packet PTS conversion from `AVCodecContext.pkt_timebase` and sample-domain
  logical frame PTS with no latency shift;
- packed `AV_SAMPLE_FMT_FLT`, 48 kHz, and the proven `AVChannelLayout`
  mappings/permutations;
- `AVCodecContext.delay = 609` for physical speakers or `577` for binaural,
  updated after options are finalized and before decoding;
- null-packet drain through the complete OpenJOC tail;
- `flush` resetting AU, E-AC-3/JOC/QMF, gain, HRTF, and timing state;
- structured OpenJOC failures mapped to specific AVERROR values without
  collapsing positive `NOT_JOC` selection behavior into corrupt data.

Codec selection is the main policy question. A separate explicit codec name
is safer for an initial patch than silently replacing FFmpeg's normal E-AC-3
decoder. Autoselection would require an upstream-reviewed mechanism that can
inspect a complete AU and fall back to the ordinary decoder without consuming
or corrupting the stream.

Once this native path passes the external bridge's AU, fingerprint, PTS, PCM,
layout, drain, and seek vectors, mpv can consume it through its normal FFmpeg
audio-decoder path. Implementing an mpv-only packet callback first would
duplicate lifecycle logic and would not provide the same ecosystem boundary.
