# OpenJOC documentation

Use the document that owns the question:

| Question | Canonical document |
|---|---|
| What is OpenJOC and how do I run it? | [root README](../README.md) |
| What changed in each release? | root `CHANGELOG.md` |
| What does the 0.9.2 candidate support? | [CAPABILITIES.md](CAPABILITIES.md) |
| How do I export reconstructed ADM/BW64? | [ADM_EXPORT.md](ADM_EXPORT.md) |
| How are ecosystem packages built? | [integration/ECOSYSTEM_PACKAGING.md](integration/ECOSYSTEM_PACKAGING.md) |
| What is the public smoke fixture? | [PUBLIC_SMOKE_FIXTURE.md](PUBLIC_SMOKE_FIXTURE.md) |
| What does it not support? | [KNOWN_LIMITATIONS.md](KNOWN_LIMITATIONS.md) |
| How do I render a supported JOC stream to speakers? | [JOC_RENDER.md](JOC_RENDER.md) |
| How is production code structured? | [ARCHITECTURE.md](ARCHITECTURE.md) |
| How do I embed the streaming decoder? | [LIBRARY_API.md](LIBRARY_API.md) |
| How do I call it from C/C++? | [C_API.md](C_API.md) |
| How do I use the native GStreamer decoder? | [integration/GSTREAMER.md](integration/GSTREAMER.md) |
| How do I embed the FFmpeg-facing bridge? | [integration/FFMPEG.md](integration/FFMPEG.md) |
| How do I build or verify the OpenJOC Player Bundle? | [integration/PLAYER_PACKAGING.md](integration/PLAYER_PACKAGING.md) |
| Which player adapter comes next? | [integration/FFMPEG_NATIVE_FUTURE.md](integration/FFMPEG_NATIVE_FUTURE.md) |
| What is planned next? | [ROADMAP.md](ROADMAP.md) |
| What clean-room policy and evidence classes govern implementation claims? | [PROVENANCE.md](PROVENANCE.md) |

Contributor rules and verification commands live in
[CONTRIBUTING.md](../CONTRIBUTING.md). Architecture and renderer behavior are
owned by the technical documents above; current status and limitations always
belong in the current snapshot documents.

Current snapshot documents own current truth. Internal research chronology and
workspace-specific provenance are retained in the source repository, not in
the standalone release documentation bundle. `CHANGELOG.md` owns versioned
release history; `KNOWN_LIMITATIONS.md` owns current user-facing limitations;
technical renderer behavior belongs in `JOC_RENDER.md` and
`JOC_SPATIAL_BRIDGE.md`. Historical design notes that are no longer suitable
for the public repository are intentionally not part of the current tree.
