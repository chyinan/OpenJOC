# GStreamer integration

OpenJOC provides an experimental native Rust GStreamer plugin named
`gst-plugin-openjoc`. It registers the `openjocdec` element as a
`GstAudioDecoder` subclass and keeps the signal path deliberately narrow:

```text
container/demuxer -> ac3parse -> openjocdec -> raw interleaved F32 PCM
```

`openjocdec` is an explicit-use decoder for framed E-AC-3 JOC. It is registered
at `GST_RANK_NONE`, so it does not take over ordinary `audio/x-eac3` streams in
`decodebin` or `playbin`. Ordinary E-AC-3 is rejected with a decoder error and
remains the host pipeline's responsibility.

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
cargo build -p gst-plugin-openjoc --release --features gstreamer
```

The resulting library is `target/release/libgstopenjoc.dylib` on macOS,
`libgstopenjoc.so` on Linux, and the corresponding Windows DLL artifact. For
development or manual loading, point GStreamer at the artifact directory:

```sh
GST_PLUGIN_PATH="$PWD/target/release" gst-inspect-1.0 openjocdec
```

## Input and access-unit contract

The sink pad accepts:

```text
audio/x-eac3, framed=true, alignment=frame
```

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

JOC admission is performed by `OpenJocSession`, reusing the existing public
E-AC-3/JOC parser and validation path. There is no generic E-AC-3 fallback and
no second JOC detector in the plugin.

## Decoder lifecycle

The element maps the GStreamer lifecycle as follows:

| GStreamer callback | OpenJOC operation |
|---|---|
| `start` | Create an instance-owned `OpenJocSession`, enable drainability |
| `set_format` | Require framed E-AC-3 and flush the session for the new format |
| `parse` | Assemble one complete I0/optional-D0 access unit |
| `handle_frame(Some)` | Convert the input PTS, push one AU, copy owned PCM to GStreamer buffers |
| `handle_frame(None)` | Call `OpenJocSession::drain` and emit every tail buffer |
| `flush` | Flush/reset QMF, reconstruction, dialnorm, speaker, and SOFA/FIR state |
| `stop` | Release the session and output state |

The plugin uses one session per element instance and no adapter-global lock.
GStreamer callback access is serialized by the element state mutex.

## Output modes and channel positions

The focused property surface is:

```text
render-mode=speaker|stereo|binaural       (default speaker)
speaker-layout=2.0|5.1|...|22.2           (default 5.1)
drc=disabled|line|rf                      (default line)
dialnorm=default|digital|analog           (default default)
validation-profile=auto|etsi-strict|observed-vendor-compat
downmix=auto|loro|ltrt                    (stereo only)
lfe-policy=exclude|equal-power-dual-mono (binaural only)
```

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

The output buffer owns one copied PCM representation. This is required because
OpenJOC's Rust frame is owned by the session and must not be borrowed beyond
the next session operation.

## Explicit pipeline

For a raw/framed `.ec3` or `.eac3` elementary stream:

```sh
GST_PLUGIN_PATH="$PWD/target/release" gst-launch-1.0 -e \
  filesrc location=/path/to/joc.ec3 ! \
  ac3parse ! \
  openjocdec render-mode=binaural ! \
  audioconvert ! audioresample ! autoaudiosink
```

For a container, use its standard demuxer before `ac3parse`; demuxing is not
part of `openjocdec`. The repository helper runs `gst-inspect-1.0` and an EOS
smoke pipeline:

```sh
scripts/verify-gstreamer.sh /path/to/joc.ec3
```

The repository does not include a commercial or private programme fixture.
The helper therefore requires a user-supplied legal/public JOC file. A
successful run demonstrates plugin loading, caps negotiation, JOC admission,
PCM flow, EOS drain, and clean pipeline termination.

## Known limitations and next phase

- The first adapter is rank-none by policy; generic E-AC-3 autoplugging is not
  changed.
- It is explicitly 48 kHz only because the current OpenJOC speaker-stage
  latency implementation is 48 kHz constrained.
- Preroll data is decoded as normal session input; the current public session
  API does not expose an output discard policy for preroll-origin frames.
- Custom SOFA properties and a JOC-aware parser/caps discriminator are not
  implemented here.
- Cross-platform builds use the same platform-neutral Rust code, but the
  platform SDK/runtime package must be installed for each target.

The repository's existing CI checks the workspace on Linux, macOS, and Windows,
and builds the feature-enabled plugin on Linux and macOS. Windows coverage is
limited to the platform-neutral workspace check until the official GStreamer
Windows installer/development package can be provisioned reproducibly in CI; no
Windows-specific decoder code is used.

The next phase can evaluate a JOC-aware parser that adds an explicit caps
discriminator, an upstream `ac3parse` field, or application-level factory
selection. None of those autoplug mechanisms belongs in this first adapter.

OpenJOC remains Apache-2.0. The GStreamer Rust bindings are MIT or
Apache-2.0; the host GStreamer libraries retain their upstream LGPL licensing.
SADIE II attribution remains governed by the existing
`THIRD_PARTY_NOTICES.md`.
