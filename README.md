# OpenJOC

OpenJOC is an independent, clean-room, research-grade E-AC-3 JOC metadata
and reconstruction-basis decoder. It implements behavior from public ETSI
specifications and controlled, permitted evidence; it does not copy Dolby
private implementations.

The current 0.x contract is deliberately narrow:

- `scene.json` is metadata-only;
- `diagnostics/reconstruction_rows/row_NNN.wav` contains diagnostic
  `ReconstructionBasis` rows, not verified authored-object PCM;
- `SemanticBindingState` remains `Unresolved`;
- `ETSI_STRICT` is the default and is never silently downgraded;
- `DOLBY_VENDOR_COMPAT` is explicit, partial, and preserves opaque observed
  continuation without assigning vendor semantics.

Read the canonical documentation:

- [Capabilities](docs/CAPABILITIES.md) — what 0.x supports today.
- [Known limitations](docs/KNOWN_LIMITATIONS.md) — what remains out of scope.
- [Architecture](docs/ARCHITECTURE.md) — production data flow and boundaries.
- [Requirements matrix](docs/REQUIREMENTS_MATRIX.md) — engineering truth table.
- [Provenance and clean-room policy](docs/PROVENANCE.md) — why claims are admissible.
- [Roadmap](docs/ROADMAP.md) — future priorities only.
- [Research history](docs/research/README.md) — dated evidence and negative results.

## Build from source

OpenJOC requires Rust 1.85 or newer. The application dependency graph is
recorded in `Cargo.lock`.

```sh
cargo build -p openjoc-cli --release --locked
./target/release/openjoc --help
```

An offline build is supported when all locked registry dependencies are
already present in the selected Cargo cache:

```sh
cargo build -p openjoc-cli --release --locked --offline
```

This is not a claim that a brand-new machine can build without first obtaining
the Rust toolchain and dependencies. The repository declares a minimum Rust
version but does not pin one exact compiler release.

## Install into a prefix

From a clean source checkout or source archive:

```sh
cargo install --path crates/openjoc-cli --locked --root /path/to/prefix
/path/to/prefix/bin/openjoc --help
```

No prebuilt binary, Homebrew formula, or crates.io installation is advertised
by the source repository. The verified 0.x installation path is the workspace
source tree.

## Basic CLI

```sh
openjoc inspect input.ec3
openjoc decode input.ec3 -o output/ --internal-base
openjoc decode input.mp4 -o output/ --internal-base --streaming
openjoc diagnose-tools input.ec3 --vector-id ID --json tools.json
```

Raw EC3 parsing and internal-base decoding run in-process. Some seekable
MP4/M4A and compatible-base paths use `ffprobe` and/or `ffmpeg`; see the
[capability matrix](docs/CAPABILITIES.md) for the exact boundary.

## Assemble a local release candidate

On an Apple-silicon macOS host with Python 3.12+, Rust, and the locked Cargo
dependencies already cached, a clean committed tree can assemble the admitted
local candidate without publishing anything:

```sh
python3 scripts/build-local-release.py --output /path/to/empty/output
cd /path/to/empty/output
shasum -a 256 -c openjoc-0.1.0-aarch64-apple-darwin.SHA256SUMS
tar -xzf openjoc-0.1.0-aarch64-apple-darwin.tar.gz
cd openjoc-0.1.0-aarch64-apple-darwin
./verify.sh
```

The candidate includes the canonical `docs/` tree, uses `git archive HEAD`,
builds with the locked dependency set, and refuses tracked worktree/index
changes. It is local-only, not Developer-ID signed, and not notarized.

## Platform scope

Release packaging is currently exercised on Apple silicon macOS. Windows,
Linux, and Intel macOS release readiness are not claimed without corresponding
CI or host evidence. The local candidate is not Developer-ID signed and is not
notarized.

## Contributing and provenance

Before changing codec behavior, read [CONTRIBUTING.md](CONTRIBUTING.md) and
[the clean-room policy](docs/PROVENANCE.md). The project treats public normative
sources, permitted synthetic tests, and controlled evidence as separate claim
classes.

## License

Apache-2.0. The source archive includes the complete `LICENSE` text, and Cargo
package metadata carries the SPDX license identifier.
