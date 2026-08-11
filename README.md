# OpenJOC

OpenJOC is an independent, research-grade E-AC-3 JOC metadata and
reconstruction-basis decoder. Its 0.x command-line contract deliberately
separates supported decoding from unresolved authored-object semantics.

The canonical support matrix is in
[`REQUIREMENTS_MATRIX.md`](REQUIREMENTS_MATRIX.md). In particular:

- `scene.json` is a metadata-only scene;
- `diagnostics/reconstruction_rows/row_NNN.wav` contains diagnostic
  `ReconstructionBasis` rows, not verified authored-object PCM;
- `SemanticBindingState` remains `Unresolved`;
- `ETSI_STRICT` is the default and is never silently downgraded;
- `DOLBY_VENDOR_COMPAT` is explicit, partial, and preserves opaque observed
  continuation without assigning vendor semantics.

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

No prebuilt binary, Homebrew formula, or crates.io installation is currently
advertised. The verified 0.x installation path is the workspace source tree.

## Runtime tools and input scope

Raw EC3 parsing and internal-base decoding run in-process. Some paths use
external FFmpeg tools:

- ordinary seekable MP4/M4A inspection or capture uses `ffprobe` and/or
  `ffmpeg` for container selection/demux;
- seekable ISO BMFF streaming uses `ffprobe` for the sample cursor;
- compatible-base generation uses `ffprobe` and `ffmpeg`.

Non-seekable and fragmented MP4 are not admitted by the 0.x contract. Logic
Pro, ADM authoring tools, Poppler, Python, and private research fixtures are not
runtime dependencies of the installed CLI.

## Platform scope

Release packaging is currently exercised on Apple silicon macOS. Rust code may
be portable to other supported Rust targets, but Windows and Linux release
readiness are not claimed without corresponding CI or host evidence.

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

The build script uses `git archive HEAD`, a fresh target directory, and
`cargo build --release --locked --offline`. It refuses tracked worktree/index
changes. Python is release-assembly tooling, not a runtime dependency of the
OpenJOC executable or bundled verifier. The candidate is not Developer-ID
signed and is not notarized; the Mach-O carries only the toolchain's automatic
linker ad-hoc signature. It is local-only and scoped to
`aarch64-apple-darwin`; the command does not tag, upload, or publish anything.

## License

Apache-2.0. The source archive includes the complete `LICENSE` text, and Cargo
package metadata carries the SPDX license identifier.
