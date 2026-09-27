![OpenJOC](assets/openjoc-header.png){ .openjoc-hero }

# OpenJOC

OpenJOC is an open-source, clean-room **E-AC-3 JOC Object decoder and spatial renderer** written in Rust. It extracts EMDF and decodes OAMD object metadata and JOC reconstruction data directly from supported Dolby Atmos E-AC-3 JOC streams, then renders to speakers or binaural output, or exports reconstructed ADM.

[Get started](getting-started/quick-start.md){ .md-button .md-button--primary }
[Install](getting-started/installation.md){ .md-button }
[View source](https://github.com/chyinan/OpenJOC){ .md-button }

!!! warning "Read the boundary before using exported ADM"
    Reconstructed ADM is not recovery of the original Atmos authoring master. It preserves a decoded, carrier-local object scene within a documented profile. Start with [decoded Objects vs authored Objects](concepts/decoded-vs-authored-objects.md) and [renderer equivalence](compatibility/renderer-equivalence.md) if you need interchange or monitoring guidance.

## What OpenJOC does

<div class="grid cards" markdown>

-   :material-waveform: **Decode JOC Objects**

    Extract in-band EMDF, decode supported OAMD metadata and JOC reconstruction data, then reconstruct carrier-local object signals from supported E-AC-3 JOC streams.

-   :material-speaker-multiple: **Render speaker layouts**

    Render to supported presets from stereo through 22.2, or provide validated custom geometry with up to 64 output channels.

-   :material-headphones: **Render binaural output**

    Virtualize a speaker field to two-channel headphone output with the bundled SADIE II D1 HRTF or a supported local SOFA file.

-   :material-file-music: **Export reconstructed ADM**

    Write decoded JOC object PCM with supported OAMD movement into a validated RIFF/RF64 ADM BWF representation.

-   :material-language-rust: **Embed the decoder**

    Use the Rust session API or the versioned C ABI. FFmpeg, GStreamer, mpv, and Windows LAV adapters share the same core session boundary.

</div>

## Choose a path

| You want to… | Start here |
| --- | --- |
| Render your first programme | [Quick start](getting-started/quick-start.md) |
| Install the CLI or build from source | [Installation](getting-started/installation.md) |
| Use PotPlayer on Windows | [Windows LAV / PotPlayer](using/windows-lav-potplayer.md) |
| Understand object identity | [Decoded Objects vs authored Objects](concepts/decoded-vs-authored-objects.md) |
| Export ADM for another tool | [Reconstructed ADM export](using/reconstructed-adm-export.md) |
| Integrate OpenJOC into software | [Rust API](reference/rust-api.md) or [C ABI](reference/c-abi.md) |

## Current release

The repository baseline for this site is **v0.18.0**. Support is intentionally scoped. See the [capability matrix](project/capabilities.md) and [known limitations](compatibility/known-limitations.md) before treating a render or export as a production deliverable.

OpenJOC is not affiliated with, endorsed by, or sponsored by Dolby Laboratories. Third-party names belong to their respective owners.
