# GStreamer JOC-aware autoplug design

Status: implementation basis for the OpenJOC GStreamer autoplug phase.

## Official 1.28 basis

This design was audited against the GStreamer 1.28 branch and current official
documentation:

- [`gstdecodebin2.c`](https://gitlab.freedesktop.org/gstreamer/gstreamer/-/blob/1.28/subprojects/gst-plugins-base/gst/playback/gstdecodebin2.c)
  — `gst_decode_bin_autoplug_factories()` filters registry factories by sink
  caps using `gst_element_factory_list_filter()` and orders them by rank;
  `gst_decode_bin_autoplug_select()` tries the first compatible factory by
  default. The same source skips a parser factory already present in the
  current decode chain, specifically to prevent parser self-recursion.
- [`gstparsebin.c`](https://gitlab.freedesktop.org/gstreamer/gstreamer/-/blob/1.28/subprojects/gst-plugins-base/gst/playback/gstparsebin.c)
  — parser/converter caps are delayed while unfixed; parser factories already
  present in the current parse chain are skipped before connection. This is the
  recursion guard used by both `parsebin` and the parser stage inside
  `decodebin3`.
- [`gstdecodebin3.c`](https://gitlab.freedesktop.org/gstreamer/gstreamer/-/blob/1.28/subprojects/gst-plugins-base/gst/playback/gstdecodebin3.c)
  — the input path uses `parsebin` for normal stream inputs, carries parser
  output through the multiqueue, and replaces downstream elements when caps
  change. Its `get_parser_caps_filter()` and
  `gst_decodebin_input_requires_parsebin()` logic were checked specifically so
  the classifier is available to `decodebin3` and `uridecodebin3`.
- [`GstBaseParse`](https://gstreamer.freedesktop.org/documentation/base/gstbaseparse.html)
  — the subclass owns sink/source pad-template caps, chooses a minimum input
  frame size, may wait in `handle_frame()` until a complete frame exists, and
  must set fixed source caps before `gst_base_parse_finish_frame()`.
- [`ac3parse`](https://gstreamer.freedesktop.org/documentation/audioparsers/ac3parse.html)
  — GStreamer 1.28 advertises generic `audio/x-eac3` sink caps and framed
  `audio/x-eac3` source caps with `framed=true` and `alignment=frame`; it does
  not advertise a JOC discriminator.
- [`Caps` design](https://gstreamer.freedesktop.org/documentation/additional/design/caps.html)
  — structure fields are constraints/information, while caps features are a
  separate part of caps intersection. This distinction is material here:
  adding only `openjoc-joc=true` would still allow an unconstrained generic
  `audio/x-eac3` structure to intersect the OpenJOC sink template.
- [`GstPluginFeature` rank](https://gstreamer.freedesktop.org/documentation/gstreamer/gstpluginfeature.html)
  — rank orders otherwise compatible factories and therefore cannot be the
  safety boundary.

The documented decodebin signals (`autoplug-factories` and `autoplug-select`)
are application assistance hooks. OpenJOC does not depend on either callback:
the classifier changes negotiated caps before decoder selection.

## Selected architecture

`openjocclassify` is a `GstBaseParse` element with the following path:

```text
container/demux -> openjocclassify -> classified E-AC-3 -> decoder
                                      |-> ordinary decoder
                                      `-> openjocdec
```

It is intentionally a parser/classifier, not a decoder. It reuses the public
OpenJOC `index_syncframes`, `group_access_units`, and `parse_joc_access_unit`
admission semantics. The shared AU framing helper is used by both the
classifier and `openjocdec`, preserving the existing I0/[D0] contract across
buffers.

The classifier holds data until it can establish one complete access unit. Its
state is explicit:

- `UNKNOWN`: fewer than one complete I0/[D0] AU is available;
- `CONFIRMED_JOC`: `parse_joc_access_unit()` returns positive admitted JOC
  evidence;
- `CONFIRMED_NON_JOC`: the complete valid AU has no admitted JOC carrier;
- `INVALID_OR_UNSUPPORTED`: malformed, truncated, or unsupported input.

The first classification is bounded to one AU, never probes a whole file, and
is cached only for the current parser stream. Sink-caps changes reset it.

On the local macOS release build, the ordinary synthetic control classified
after 768 bytes in 12 microseconds; the private JOC programme classified after
4096 bytes in 213 microseconds during the automatic path. This is one bounded
metadata parse before steady-state decoder processing; no PCM is decoded by
the classifier.

## Caps contract and safety proof

`openjocclassify` emits one of these project-scoped experimental contracts:

```text
ordinary:
audio/x-eac3, framed=true, alignment=frame, openjoc-joc=false

JOC:
audio/x-eac3(openjoc:joc), framed=true, alignment=frame, openjoc-joc=true
```

`openjoc:joc` is not an upstream GStreamer standard; it is an OpenJOC
out-of-tree caps feature. The boolean field makes the semantic decision
visible in `gst-inspect`, while the caps feature makes generic input
ineligible for the OpenJOC sink. `openjocdec` advertises only the JOC caps.

Consequently:

```text
generic audio/x-eac3       ∩ openjocdec sink = EMPTY
classified non-JOC caps    ∩ openjocdec sink = EMPTY
classified JOC caps        ∩ openjocdec sink = NON-EMPTY
```

This remains true when the OpenJOC factory rank is set to an artificially high
value. The rank selects OpenJOC only after the JOC caps feature has positively
classified the stream.

The classifier itself accepts generic `audio/x-eac3` sink caps, so it can be
selected before `ac3parse` (rank 258 versus the installed `ac3parse` rank 257).
GStreamer’s official parser-chain guard prevents the same parser factory from
being inserted recursively after it emits its classified caps.

## Explicit and automatic forms

The canonical explicit engineering form is:

```text
ac3parse ! openjocclassify ! openjocdec
```

The canonical application form is:

```text
filesrc ! decodebin ! raw-audio-sink
```

or the corresponding `playbin`/`decodebin3` URI path. Applications do not
name `openjocdec`; decoder selection is driven by the classified caps.
