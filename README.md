<p align="center">
  <img src="docs/site/assets/openjoc-header.png" alt="OpenJOC — open-source E-AC-3 JOC decoder and renderer" width="100%">
</p>

# OpenJOC

OpenJOC is an independent, clean-room E-AC-3 JOC decoder and spatial renderer written in Rust. It decodes supported E-AC-3/JOC input, reconstructs carrier-local object signals, and renders speaker or binaural output through one platform-neutral engine.

Download the [latest release](https://github.com/chyinan/OpenJOC/releases/latest). OpenJOC is not affiliated with, endorsed by, or sponsored by Dolby Laboratories.

## Documentation

**[Read the OpenJOC documentation site](https://chyinan.github.io/OpenJOC/)** · **[阅读简体中文文档](https://chyinan.github.io/OpenJOC/zh/)**

## What it supports

- E-AC-3 JOC decoding with bounded reconstruction;
- supported speaker presets from `2.0` through `22.2` and custom geometry up to 64 output channels;
- two-channel virtual-speaker binaural rendering with the bundled SADIE II D1 HRTF or a supported local SOFA file;
- reconstructed ADM BWF interoperability output with decoded JOC/OAMD binding within a documented profile;
- Rust and versioned C ABI embedding surfaces;
- project-provided FFmpeg, GStreamer, mpv, and Windows DirectShow/LAV integrations.

Read the [capability matrix](docs/site/project/capabilities.md) for the evidence boundary behind each claim.

## Quick start

Build or download OpenJOC, then render a JOC programme:

```sh
openjoc render-joc input.m4a --layout 7.1.4 --output output.wav
openjoc render-joc input.m4a --binaural --output headphones.wav
openjoc export-adm input.m4a --output reconstructed.wav
openjoc validate-adm reconstructed.wav
```

Use `openjoc inspect input.ec3` to inspect a carrier before rendering. The [quick-start guide](docs/site/getting-started/quick-start.md) covers the first render and points to the detailed output contracts.

For custom geometry, use `--layout-file LAYOUT.json`; the documented limit is 64 output channels.

## Windows playback

The optional Windows package provides an isolated OpenJOC-enabled LAV Audio Decoder. It installs beside stock LAV and does not change PotPlayer automatically:

1. Extract the package from the [OpenJOC releases page](https://github.com/chyinan/OpenJOC/releases).
2. Run `install.bat`, then require `verify.bat` to report **PASS**.
3. In PotPlayer, add **LAV Audio Decoder (OpenJOC)** in **Filter Control** → **Filter Priority (Overall)** and set it to **Prefer**.

The [Windows LAV / PotPlayer guide](docs/site/using/windows-lav-potplayer.md) documents the seven fixed PCM policies, passthrough behavior, rollback, and hardware boundary.

## Important boundaries

Reconstructed ADM is an interoperability-oriented representation of the decoded JOC object scene. It is not recovery of the original authored Atmos master. OpenJOC does not recover original authoring identity, source-stem PCM, unquantized automation, Dolby authoring provenance, or a lossless JOC-to-ADM round trip.

The [decoded Objects vs authored Objects](docs/site/concepts/decoded-vs-authored-objects.md) page explains the identity boundary. The [renderer-equivalence limitation](docs/site/compatibility/renderer-equivalence.md) explains why a generic ADM renderer is not guaranteed to localize exactly like native JOC playback.

## API and integrations

- [Rust API](docs/site/reference/rust-api.md) — serial `OpenJocSession` lifecycle and owned interleaved `f32` output.
- [C ABI](docs/site/reference/c-abi.md) — opaque handles, bounded stream decoding, custom geometry, and panic containment.
- [Integration overview](docs/site/project/integrations.md) — current FFmpeg, GStreamer, mpv, player-bundle, and Windows contracts.

## Build from source

Use the Rust toolchain declared in `Cargo.toml`:

```sh
cargo build -p openjoc-cli --release --locked
./target/release/openjoc --version
```

Contributors should follow [CONTRIBUTING.md](CONTRIBUTING.md) and run the workspace quality gates before committing.

## Help wanted

OpenJOC is usable today, but several research and validation problems remain
open. Contributions are especially welcome around native-renderer equivalence,
reconstructed-PCM headroom, and physical multichannel hardware validation.

See [Open Problems & Contribution Opportunities](docs/site/project/open-problems.md)
before starting work on codec or renderer semantics. For difficult research,
starting a Discussion first is recommended so work does not repeat an already
investigated path.

## License and notices

OpenJOC core code is licensed under [Apache-2.0](LICENSE). Integration bundles may include components under additional terms; see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) and package-specific notices. Dolby, Dolby Atmos, SADIE, FFmpeg, GStreamer, mpv, LAV Filters, PotPlayer, Windows, and related names are marks of their respective owners.
