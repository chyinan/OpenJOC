# GStreamer integration

OpenJOC provides an experimental native Rust GStreamer plugin named
`gst-plugin-openjoc`. It registers `openjocclassify` as a `GstBaseParse`
classifier and `openjocdec` as a `GstAudioDecoder` subclass:

```text
container/demuxer -> openjocclassify -> decoder -> raw interleaved F32 PCM
```

`openjocclassify` is registered at rank 258, just above the installed
`ac3parse` rank of 257. It buffers only enough compressed data for one admitted
I0/[D0] access unit, reuses OpenJOC's public E-AC-3/JOC admission parser, and
emits an explicit classification before decoder autoplugging.

`openjocdec` is registered at `GST_RANK_PRIMARY`, but its sink caps are
JOC-only. Ordinary E-AC-3 remains on the normal GStreamer decoder path before
OpenJOC consumes any ordinary data. The exact design/source audit is recorded
in [the autoplug design note](../research/GSTREAMER_JOC_AUTOPLUG_DESIGN.md).

## Version and build matrix

The first adapter uses the stable GStreamer 1.x API baseline below:

| Item | Choice |
|---|---|
| Minimum GStreamer | 1.20 |
| Tested target | 1.28.6 (Homebrew macOS SDK) |
| gstreamer-rs | 0.24.5 |
| Rust MSRV | OpenJOC workspace MSRV 1.85 |

The `v1_20` Rust feature is sufficient for this adapter. GStreamer 1.28.6 is
the locally tested stable-series target; no 1.28-only API is required. gstreamer-rs 0.24.5
was selected to preserve OpenJOC's Rust 1.85 MSRV; the newer 0.25 line has a
higher compiler requirement and is not needed by this plugin.

Install the matching GStreamer runtime and development packages for the host,
including GStreamer core, GStreamer Base, and GStreamer Audio. The plugin does
not modify a global GStreamer installation.

Build the loadable artifact with:

```sh
gstreamer_target="${OPENJOC_GSTREAMER_TARGET_DIR:-$PWD/target-gstreamer}"
CARGO_TARGET_DIR="$gstreamer_target" cargo build -p gst-plugin-openjoc --release --features gstreamer
```

The resulting authoritative library is
`$gstreamer_target/release/libgstopenjoc.dylib` on macOS,
`$gstreamer_target/release/libgstopenjoc.so` on Linux, and the corresponding
Windows DLL artifact. Keep this feature-enabled artifact isolated from the
generic workspace `target/release` output. For development or manual loading,
point GStreamer at the isolated artifact directory:

```sh
GST_PLUGIN_PATH="$gstreamer_target/release" gst-inspect-1.0 openjocclassify
GST_PLUGIN_PATH="$gstreamer_target/release" gst-inspect-1.0 openjocdec
```

## Input and access-unit contract

`openjocclassify` accepts generic E-AC-3 input:

```text
audio/x-eac3
```

After one complete access unit it emits one of these experimental classified
caps contracts:

```text
ordinary: audio/x-eac3, framed=true, alignment=frame, openjoc-joc=false
JOC:      audio/x-eac3(openjoc:joc), framed=true, alignment=frame, openjoc-joc=true
```

`openjoc:joc` is an OpenJOC-scoped caps feature, not an upstream GStreamer
standard. It is the safety discriminator: generic E-AC-3 caps and ordinary
classified caps cannot intersect the `openjocdec` sink template. The boolean
field makes the result visible in `gst-inspect-1.0`.

`openjocdec` accepts only the JOC-classified form above. This means the old
manual `ac3parse ! openjocdec` form is intentionally replaced by
`ac3parse ! openjocclassify ! openjocdec`.

The decoder's bounded `parse` callback verifies complete syncframes and emits
one OpenJOC access unit at a time. The first version admits the current
OpenJOC contract:

```text
I0 independent substream zero
[optional D0 dependent substream zero]
```

It never assumes that an incoming `GstBuffer` is already an OpenJOC access
unit. A split syncframe is held until complete; a second independent substream
zero starts the next unit. Unsupported substream order, a truncated frame, or
more than the admitted dependent topology fails closed. The current native
renderer path is 48 kHz only, matching the existing OpenJOC FinalLinkedGain
contract.

JOC admission is performed by the existing public E-AC-3/JOC parser and
validation path. There is no generic E-AC-3 fallback and no second JOC
detector in the plugin. For a stream that has not yet supplied enough bytes,
the classifier remains `UNKNOWN`; it never emits a premature JOC or non-JOC
decision.

### Need-data and EOS semantics

`GstAudioDecoder` has no separate `NEED_DATA` return value. While the bounded
adapter contains an incomplete syncframe or a complete I0 whose possible D0 has
not arrived, `openjocdec` returns Rust `FlowError::Eos`. The GStreamer base
class maps that to `GST_FLOW_EOS` internally, interprets it as “no frame yet,”
keeps the adapter contents, and returns to the upstream chain without emitting
an EOS event or transitioning the pipeline to EOS. This is the documented
`GstAudioDecoder` parser contract, not end-of-stream signalling; see the
[official decoder source](https://gitlab.freedesktop.org/gstreamer/gstreamer/-/blob/1.28/subprojects/gst-plugins-base/gst-libs/gst/audio/gstaudiodecoder.c)
and [decoder API documentation](https://gstreamer.freedesktop.org/documentation/audio/gstaudiodecoder.html).

Genuine upstream EOS sets the decoder parse-state EOS flag. A complete
independent-only unit is then finalized and drained. A partial syncframe or
partial dependent frame fails closed with a decoder error; it is not silently
converted into an independent-only unit. The GStreamer base class clears its
adapter during flush/reset, while the plugin flushes the OpenJOC session, so a
flush, seek, or discontinuity cannot retain compressed AU bytes or renderer
history. The focused framing tests cover independent-only input, I0/D0 across
buffers, consecutive AUs, partial EOS, flush, and discontinuity reset.

## Decoder lifecycle

The element maps the GStreamer lifecycle as follows:

| GStreamer callback | OpenJOC operation |
|---|---|
| `start` | Create an instance-owned `OpenJocSession`, enable drainability |
| `set_format` | Require framed E-AC-3 and flush the session for the new format |
| `parse` | Assemble one complete I0/optional-D0 access unit |
| `handle_frame(Some)` | Convert the input PTS, push one AU, copy owned PCM to GStreamer buffers |
| `handle_frame(None)` | Call `OpenJocSession::drain` and emit every tail buffer with the GStreamer forced-drain frame completion semantics |
| `flush` | Flush/reset QMF, reconstruction, dialnorm, speaker, and SOFA/FIR state |
| `stop` | Release the session and output state |

The plugin uses one session per element instance and no adapter-global lock.
GStreamer callback access is serialized by the element state mutex.

## Output modes and channel positions

The focused property surface is:

```text
render-mode=speaker|stereo|binaural       (default speaker)
speaker-layout=2.0|5.1|...|22.2           (default 5.1)
virtual-layout=5.1|7.1.4|...|22.2        (default 7.1.4; binaural only)
drc=disabled|line|rf|custom                (default line)
drc-boost=0..100, drc-cut=0..100           (custom only; default 100/100)
dialnorm=default|digital|analog           (default default)
validation-profile=auto|etsi-strict|observed-vendor-compat
downmix=auto|loro|ltrt                    (stereo only)
lfe-policy=exclude|equal-power-dual-mono (binaural only)
```

In binaural mode, `virtual-layout` is the effective speaker field sent to the
session; `speaker-layout` remains the ordinary speaker/stereo property. The
default `7.1.4` matches `openjoc render-joc --binaural`.

An automatically created `openjocdec` uses its existing decoder defaults:
`render-mode=speaker` and `speaker-layout=5.1`. Autoplug selection does not
change output-layout policy. Applications that require deterministic two
channel binaural output should set `render-mode=binaural` and, when needed,
`virtual-layout=7.1.4` on the created decoder, or use the explicit classified
engineering pipeline shown below.

No offline peak/LUFS/true-peak normalization is exposed. Custom SOFA loading
is deferred; `render-mode=binaural` uses the existing built-in SADIE II generic
HRTF without a filesystem or network dependency.

Output is `audio/x-raw`, `format=F32LE`, `layout=interleaved`, 48 kHz, with
truthful channel positions. OpenJOC's semantic channel order is retained and
mapped once at the adapter boundary:

- `2.0` and binaural use Front Left / Front Right transport positions;
- `5.1` uses Front Left, Front Right, Front Center, LFE1, Side Left, Side Right;
- `7.1.4` uses Rear Left/Right for `Lb`/`Rb`, Side Left/Right for `Ls`/`Rs`,
  and the standard top-front/top-rear positions;
- `9.1.6` maps the wide channels to Wide Left/Right and the middle-height
  channels to Top Side Left/Right;
- `22.2` maps the OpenJOC BS.2051-3 H identities independently to GStreamer
  positions, including both LFE positions, top-side positions, and bottom
  positions.

For binaural, Front Left/Right are ordinary two-channel transport positions;
they are not a claim that the decoded signal is physical FL/FR loudspeaker
rendering. The semantic identities remain Left Ear and Right Ear inside
OpenJOC.

## Timing, latency, flush, and EOS

GStreamer nanosecond timestamps are converted to sample positions with checked
integer arithmetic at the admitted sample rate. `GstAudioDecoder` then owns the
output timestamp timeline and derives each output PTS from the input segment
origin plus samples already pushed. Buffer durations and latency are converted
with rational rounding; timestamps are never shifted by decoder latency. The
decoder reports latency separately through `GstAudioDecoder::set_latency`:

- speaker/stereo: 609 samples, 12,687,500 ns at 48 kHz;
- binaural: 577 samples, 12,020,833 ns at 48 kHz.

`drain` emits the reconstruction tail and the built-in binaural FIR tail. The
tail is not added to steady-state latency. A flush clears pending parser state,
OpenJOC reconstruction history, FinalLinkedGain state, dialnorm state, and
binaural convolution state before the next segment.

The adapter completes each admitted compressed AU with
`finish_subframe(NULL)`, so no GstAudioDecoder input frame remains pending when
GStreamer calls `handle_frame(NULL)` for forced drain. Drain PCM is therefore
emitted with `finish_frame(buffer, -1)`: in the current GstAudioDecoder source,
`-1` resolves relative to the pending input-frame queue and consumes all pending
input frames. With this adapter's empty queue it consumes none while still
accounting and timestamping the delayed PCM as a full output frame. A
zero-frame `finish_frame` call is invalid, and `finish_subframe(buffer)` would
incorrectly require a pending compressed input frame. An empty OpenJOC drain
returns normally without a dummy completion call.

The output buffer owns one copied PCM representation. This is required because
OpenJOC's Rust frame is owned by the session and must not be borrowed beyond
the next session operation.

## Explicit pipeline

For a raw/framed `.ec3` or `.eac3` elementary stream:

```sh
GST_PLUGIN_PATH="${OPENJOC_GSTREAMER_TARGET_DIR:-$PWD/target-gstreamer}/release" gst-launch-1.0 -e \
  filesrc location=/path/to/joc.ec3 ! \
  ac3parse ! openjocclassify ! \
  openjocdec render-mode=binaural ! \
  audioconvert ! audioresample ! autoaudiosink
```

For automatic JOC-aware decoder selection, the application does not name
`openjocdec`:

```sh
GST_PLUGIN_PATH="${OPENJOC_GSTREAMER_TARGET_DIR:-$PWD/target-gstreamer}/release" gst-launch-1.0 -e \
  filesrc location=/path/to/joc.ec3 ! decodebin ! \
  audioconvert ! audioresample ! autoaudiosink
```

The same classifier is available through `decodebin3`, `uridecodebin3`, and
`playbin3` where those playback elements are installed. Use the repository
helper for a private real-media test that inspects both `openjocclassify` and
the automatically instantiated `openjocdec`:

```sh
scripts/verify-gstreamer-autoplug.sh /path/to/private-joc.ec3 /path/to/ordinary.eac3
```

For a container, use its standard demuxer before `ac3parse`; demuxing is not
part of `openjocdec`. The repository helper runs `gst-inspect-1.0` and an EOS
smoke pipeline:

```sh
scripts/verify-gstreamer.sh /path/to/joc.ec3
```

The repository does not include a commercial or private programme fixture.
The helper therefore requires a user-supplied legal/public JOC file. A
successful explicit run demonstrates plugin loading, classified caps
negotiation, JOC admission, PCM flow, EOS drain, and clean pipeline
termination; the autoplug helper additionally proves the decoder was
instantiated by the playback stack.

## Known limitations and next phase

- The JOC caps feature is OpenJOC-specific and experimental; it is not an
  upstream GStreamer media-type convention.
- It is explicitly 48 kHz only because the current OpenJOC speaker-stage
  latency implementation is 48 kHz constrained.
- Preroll data is decoded as normal session input; the current public session
  API does not expose an output discard policy for preroll-origin frames.
- Custom SOFA properties remain deferred.
- Cross-platform builds use the same platform-neutral Rust code, but the
  platform SDK/runtime package must be installed for each target.

The repository's existing CI checks the workspace on Linux, macOS, and Windows,
and builds the feature-enabled plugin on Linux and macOS. Windows coverage is
limited to the platform-neutral workspace check until the official GStreamer
Windows installer/development package can be provisioned reproducibly in CI; no
Windows-specific decoder code is used.

The autoplug phase does not claim support for any specific commercial player.
Player-specific acceptance and an FFmpeg wrapper remain later phases.

OpenJOC remains Apache-2.0. The GStreamer Rust bindings are MIT or
Apache-2.0; the host GStreamer libraries retain their upstream LGPL licensing.
SADIE II attribution remains governed by the existing
`THIRD_PARTY_NOTICES.md`.
