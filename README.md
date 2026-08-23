<p align="center">
  <img src="docs/assets/openjoc-header.png" alt="OpenJOC — open-source E-AC-3 JOC decoder and renderer" width="100%">
</p>

# OpenJOC

OpenJOC is an independent, clean-room E-AC-3 JOC decoder and spatial renderer.
It decodes the E-AC-3 base programme, OAMD metadata, and JOC reconstruction
data, then renders admitted speaker or binaural outputs through one
platform-neutral engine. OpenJOC is not affiliated with, endorsed by, or
sponsored by Dolby Laboratories.

The current release includes:

- E-AC-3 JOC decoding and bounded reconstruction;
- automatic OAMD/JOC bridge control within the documented unresolved semantic
  boundary;
- speaker presets `2.0`, `5.1`, `5.1.2`, `5.1.4`, `7.1`, `7.1.2`, `7.1.4`,
  `7.1.6`, `9.1`, `9.1.2`, `9.1.4`, `9.1.6`, and `22.2`;
- custom speaker geometry with up to 64 output channels in caller-defined order;
- truthful WAV/CAF output, built-in SADIE II binaural rendering, and custom
  SOFA input;
- reconstructed ADM BWF interoperability output;
- Rust and versioned C ABI embedding surfaces;
- FFmpeg, GStreamer, mpv, and Windows DirectShow/LAV/PotPlayer integrations.

The detailed status and evidence boundary for every claim is in the
[capability matrix](docs/CAPABILITIES.md).

## Quick start: CLI

Build or download OpenJOC, then render a JOC programme to 7.1.4:

```sh
openjoc render-joc input.m4a --layout 7.1.4 -o output.wav
```

Useful first commands:

```sh
openjoc inspect input.ec3
openjoc render-joc input.m4a --layout 2.0 -o stereo.wav
openjoc render-joc input.m4a --binaural -o headphones.wav
openjoc export-adm input.m4a -o reconstructed.wav
openjoc validate-adm reconstructed.wav
```

OpenJOC uses calibrated default dialnorm behavior. For a convenient offline
file level, add `--normalize-peak -0.1`; this applies one static sample-peak
gain after rendering and is not DRC, loudness normalization, limiting, or
true-peak processing. See [JOC rendering](docs/JOC_RENDER.md) for the complete
output and level contract.

## Quick start: PotPlayer on Windows

The current Windows package provides an isolated OpenJOC-enabled LAV Audio
Decoder. It does not replace stock LAV or change PotPlayer automatically.

1. Download the Windows LAV package from the
   [latest OpenJOC release](https://github.com/chyinan/OpenJOC/releases/latest).
2. Extract the complete ZIP.
3. Double-click `install.bat` and accept the Windows UAC prompt.
4. Double-click `verify.bat` and require **PASS**.
5. Follow the included `POTPLAYER-QUICKSTART.md` to add
   **LAV Audio Decoder (OpenJOC)** at **Prefer** priority.

Double-click `uninstall.bat` to remove only the OpenJOC-owned filter and files.
The validated PotPlayer/DirectShow output boundary is **48 kHz stereo float
PCM**. Standalone OpenJOC multichannel and custom-geometry capabilities do not
imply arbitrary LAV output. See the
[Windows integration contract](docs/integration/LAV_FILTERS_OPENJOC.md).

## Speaker rendering

Presets are the ordinary path:

```sh
openjoc render-joc input.m4a --layout 5.1.4 -o output.wav
openjoc render-joc input.m4a --layout 9.1.6 -o output.caf
openjoc render-joc input.m4a --layout 22.2 -o output.wav
```

Advanced users can provide versioned JSON geometry. The `speakers` array
defines semantic labels and interleaved PCM order:

```sh
openjoc render-joc input.m4a \
  --layout-file studio-layout.json \
  -o studio.caf
```

Custom layouts support up to 64 output channels in declared order. Custom WAV output is deliberately
unmasked because a standard speaker mask would misrepresent arbitrary
geometry; CAF preserves coordinate descriptions. Preset-specific WAV/CAF
rules, channel order, latency, DRC, dialnorm, normalization, and optional
`--topology` override behavior belong to [JOC rendering](docs/JOC_RENDER.md).
The JSON schema and coordinate convention belong to
[custom speaker layouts](docs/CUSTOM_SPEAKER_LAYOUTS.md).

## Binaural and SOFA

`--binaural` virtualizes a speaker field to two-channel headphone output. The
default virtual layout is 7.1.4 and the default HRTF is the bundled offline
SADIE II D1 dataset:

```sh
openjoc render-joc input.m4a --binaural -o headphones.wav
openjoc render-joc input.m4a \
  --binaural --virtual-layout 9.1.6 --sofa listener.sofa \
  -o custom-headphones.wav
```

Custom SOFA input is fail-closed and limited to the documented local
`SimpleFreeFieldHRIR` subset. It must match the input sample rate and cover
every requested non-LFE direction exactly or through admitted interpolation.
Physical `2.0` and binaural are different renders even though both transport
two PCM channels.

## Reconstructed ADM interoperability

`export-adm` writes a reconstructed RIFF/RF64 ADM BWF representation and an
adjacent semantic report. It is not recovery of the original ADM master.
Authored-object audio binding remains unresolved, so OpenJOC does not invent
object identities, discarded source information, or Dolby authoring
provenance.

The validated workflow is OpenJOC ADM import into Logic Pro followed by a
Logic-authored re-export accepted by Dolby Encoding Engine. Direct DEE ingest
of the byte-exact OpenJOC-authored file is not claimed. See
[ADM export](docs/ADM_EXPORT.md).

## Ecosystem integrations

- [Rust API](docs/LIBRARY_API.md) — serial `OpenJocSession` lifecycle,
  complete-access-unit input, owned interleaved `f32` output, and explicit
  latency/reset/drain behavior.
- [C ABI](docs/C_API.md) — opaque handles, complete-AU and bounded stream
  decoders, positive JOC classifier, custom in-memory speaker geometry, and
  panic containment.
- [FFmpeg external bridge](docs/integration/FFMPEG.md) — libavformat transport
  for embedding applications; it does not modify an installed FFmpeg.
- [Native FFmpeg wrapper](docs/integration/FFMPEG_NATIVE.md) — explicit
  `libopenjoc` decoder for patched custom FFmpeg builds; ordinary E-AC-3 stays
  on `eac3`.
- [GStreamer](docs/integration/GSTREAMER.md) — JOC-aware classification and a
  native `GstAudioDecoder`; ordinary E-AC-3 remains on the normal path.
- [mpv](docs/integration/MPV.md) — source patch and qualified OpenJOC Player
  Bundles with positive JOC selection and passthrough isolation.
- [Windows LAV/DirectShow](docs/integration/LAV_FILTERS_OPENJOC.md) — isolated
  PotPlayer-validated stereo-float host integration and onboarding.

These adapters own transport and host lifecycle. OpenJOC owns E-AC-3/JOC
decode, spatial rendering, output semantics, and renderer state.

## Important boundaries

- `ReconstructionBasis` rows are decoder coordinates, not verified
  authored-object stems. `SemanticBindingState` remains `Unresolved`.
- OpenJOC does not claim Dolby renderer fidelity, bit-identical reference
  output, certification, or endorsement.
- Ordinary E-AC-3 isolation and compressed passthrough remain explicit in
  player integrations.
- Custom renderer geometry does not widen the layout capabilities of FFmpeg,
  GStreamer, mpv, DirectShow/LAV, an audio device, or an output container.
- The implementation follows the project's public-evidence and separated
  clean-room policy. Proprietary implementation code, decompiler output,
  assembly, private symbols or layouts, and copied expressions are forbidden
  implementation inputs.

Read [known limitations](docs/KNOWN_LIMITATIONS.md) before treating any output
as an interchange, monitoring, or production deliverable, and see
[provenance](docs/PROVENANCE.md) for the evidence policy and history.

## Documentation

The [documentation index](docs/README.md) maps each topic to its canonical
owner. In particular:

- [Capabilities](docs/CAPABILITIES.md) owns current support status.
- [Known limitations](docs/KNOWN_LIMITATIONS.md) owns current user-visible
  boundaries and non-claims.
- [Architecture](docs/ARCHITECTURE.md) owns the production data flow.
- [JOC rendering](docs/JOC_RENDER.md) owns renderer and output behavior.
- [CHANGELOG](CHANGELOG.md) owns release chronology.
- [Roadmap](docs/ROADMAP.md) contains future work only.
- [`docs/archive/`](docs/archive/README.md) contains retained historical
  contracts that are not current documentation.

## Build from source

Use the Rust toolchain requirement declared in `Cargo.toml`. From a clean
checkout:

```sh
cargo build -p openjoc-cli --release --locked
./target/release/openjoc --help
```

Install into a chosen prefix with:

```sh
cargo install --path crates/openjoc-cli --locked --root /path/to/prefix
```

Contributors should follow [CONTRIBUTING.md](CONTRIBUTING.md) and run the full
workspace and repository-hygiene gates before committing.

## License and notices

OpenJOC core code is licensed under [Apache-2.0](LICENSE). Integration bundles
may include components under additional terms; see
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) and the package-specific
notices. Dolby, Dolby Atmos, SADIE, FFmpeg, GStreamer, mpv, LAV Filters,
PotPlayer, Windows, and related names are marks of their respective owners.
