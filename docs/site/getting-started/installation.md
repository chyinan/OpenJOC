# Installation

You can use a release archive or build the CLI from the repository.

## Release archive

Download the matching platform asset from the [OpenJOC releases page](https://github.com/chyinan/OpenJOC/releases). Extract it into a directory you control and put the CLI on your `PATH`, or invoke it by its full path.

The v0.14.0 release workflow targets macOS arm64, Windows x86_64, and GNU/Linux x86_64. Ecosystem packages have their own runtime and licensing boundaries; use the package quick-start material that ships with each archive.

## Build from source

The workspace declares the Rust edition, version, repository, and minimum supported Rust version in the root `Cargo.toml`. From a clean checkout:

```sh
cargo build -p openjoc-cli --release --locked
./target/release/openjoc --version
```

On Windows, run the equivalent binary from `target\\release\\openjoc.exe`.

To install into a chosen prefix:

```sh
cargo install --path crates/openjoc-cli --locked --root /path/to/prefix
```

## Input tools

Raw E-AC-3 input is handled by OpenJOC's bounded reader. Seekable ordinary MP4/M4A input uses the repository's container boundary and may require `ffprobe` or `ffmpeg`. Non-seekable and fragmented MP4 input is not admitted for the documented streaming path.

## Windows LAV package

Windows playback through DirectShow is a separate optional package. It installs an OpenJOC-owned filter beside stock LAV and does not modify PotPlayer automatically. Follow [Windows LAV / PotPlayer](../using/windows-lav-potplayer.md) after extracting that package.

## Check the installation

```sh
openjoc --help
openjoc self-test
```

`self-test` reports a missing optional public fixture as `NOT_APPLICABLE`; that result is not silent success for a fixture-dependent check.
