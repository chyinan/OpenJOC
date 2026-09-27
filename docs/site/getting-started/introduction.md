# Introduction

OpenJOC is an independent Rust decoder for supported E-AC-3 JOC streams. It accepts supported raw E-AC-3 or seekable ordinary MP4/M4A input. It performs E-AC-3 JOC object decoding from in-band EMDF, decoding the supported OAMD metadata prefix/timeline and JOC reconstruction payload into separate decoder-domain spatial metadata and `ReconstructionBasis` audio rows. Its JOC/OAMD object association, experimental speaker/binaural bridge, and reconstructed ADM export each have a documented profile boundary.

The main user-facing surfaces are:

- `openjoc render-joc` for speaker or binaural rendering;
- `openjoc export-adm` for a reconstructed ADM BWF representation;
- `openjoc inspect` and `openjoc decode` for metadata and reconstruction diagnostics;
- the Rust `OpenJocSession` API and versioned C ABI for embedding;
- project-provided FFmpeg, GStreamer, mpv, and Windows DirectShow/LAV integrations.

The project has two important semantic boundaries:

1. A `ReconstructionBasis` row is a decoder-domain, carrier-local output signal. It is not an authored Atmos stem.
2. The reconstructed ADM exporter binds decoded JOC object PCM to decoded OAMD movement only within its admitted profile. It does not recover the source ADM master.

OpenJOC uses a 48 kHz render domain. The ordinary JOC speaker path is experimental and bounded by the documented profiles, layouts, output containers, and validation status. The [capability matrix](../project/capabilities.md) owns the detailed status vocabulary.

## The first successful render

Install the CLI, run `openjoc --help`, then follow [Quick start](quick-start.md). If your input is a seekable M4A or MP4, keep `ffprobe` and `ffmpeg` available as described in [Installation](installation.md).
