# Supported platforms

OpenJOC's core is a Rust workspace with platform-neutral decode, scene, renderer, ADM, and wave/CAF crates. The release and integration surface is narrower than the core crate graph.

| Surface | Current scope |
| --- | --- |
| CLI and Rust API | Build from the workspace on the targets supported by the current Rust toolchain and the selected optional dependencies. |
| Release assets | macOS arm64, Windows x86_64, and GNU/Linux x86_64 are the current release targets. |
| Speaker renderer | Preset and custom geometry render to OpenJOC-owned WAV/CAF output, subject to the [output contract](../reference/output-formats.md). |
| Binaural renderer | Two-channel virtual-speaker output using the bundled SADIE II D1 resource or a supported local SOFA file. |
| FFmpeg | External bridge and a separate native `libopenjoc` wrapper for custom FFmpeg builds; stock FFmpeg is not modified by installing OpenJOC. |
| GStreamer | Optional native plugin with an OpenJOC-specific classification caps feature and a matching GStreamer runtime. |
| mpv | Project-provided patched builds and player bundles; not official upstream mpv or FFmpeg distributions. |
| Windows DirectShow/LAV | Optional isolated filter with seven explicit 48 kHz IEEE-float PCM policies. |

Multichannel PCM generation and transport are qualified within the documented test surfaces. Physical multichannel playback on arbitrary hardware is not claimed. Automatic endpoint or device layout discovery is not part of the OpenJOC contract.

See the [capability matrix](../project/capabilities.md) for evidence boundaries and the [integration overview](../project/integrations.md) for links to adapter-specific repository documentation.
