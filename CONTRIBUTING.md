# Contributing to OpenJOC

Keep each project fact in its canonical documentation owner:

- user onboarding and commands: `README.md`;
- current support: `docs/CAPABILITIES.md`;
- current boundaries: `docs/KNOWN_LIMITATIONS.md`;
- architecture: `docs/ARCHITECTURE.md`;
- requirements and status: `docs/REQUIREMENTS_MATRIX.md`;
- provenance and clean-room rules: `docs/PROVENANCE.md`;
- future work: `docs/ROADMAP.md`;
- dated research and implementation history: `docs/research/`.

Do not copy a full current status table into a historical report. Summarize and
link to the canonical owner instead.

## Public naming

Public names should describe stable function or semantics and must not encode
temporary evidence maturity, validation status, research provenance, or
stronger vendor authority than the project has established. Represent mutable
maturity, evidence, and validation as explicit state or documentation rather
than baking them into stable public identifiers.

The canonical user-facing profile names are `AUTO`, `ETSI_STRICT`, and
`OBSERVED_VENDOR_COMPAT`. Legacy compatibility spellings are input-only aliases
where intentionally retained; they must not be emitted as canonical output.
The stable JOC spatial function name is `JocSpatialBridge`; its experimental
maturity and unresolved semantic state remain separate from that name.

Codec changes must remain explainable from permitted public normative sources,
public mathematics, or an explicitly admitted behavioral clean-room
specification. Authorized contaminated analysis may inspect controlled
reverse-engineering or proprietary evidence only inside the separated Analyst
environment when public evidence is insufficient; that material is analysis
evidence, not implementation provenance. Implementers must not receive or use
proprietary/decompiled decoder sources, assembly, private symbols or addresses,
private research artifacts, copied implementation expressions, or proprietary
structure layouts. Only the sanitized implementation-necessary behavioral
rules, contracts, constants, and acceptance tests may cross the boundary.

Before a change is committed, run:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
CARGO_BUILD_JOBS=1 cargo test --workspace --all-features -- --test-threads=1
CARGO_BUILD_JOBS=1 cargo build --workspace --release --offline
git diff --check
```

If a documentation change moves or retires a file, repair tracked links and
update the release-assembly documentation inventory as part of the same
focused change.

## CI and release automation

GitHub Actions validates pull requests and pushes to `master` with the Linux
quality contract, the documented Rust 1.85 MSRV check, and platform-neutral
Windows x64 / macOS arm64 build and test jobs. Optional container tests that
need `ffmpeg`, `ffprobe`, or MP4Box remain explicitly skipped when those tools
or private fixtures are absent; no private Logic, ADM, DD+, EC-3, or evidence
files are used by CI.

Releases are human-authorized by pushing a stable `vMAJOR.MINOR.PATCH` tag.
The release workflow validates the tag against Cargo metadata, checks the tag
commit and `Cargo.lock`, reuses the canonical macOS-arm64 release builder, runs
the bundle verifier, and publishes only the generated artifact set. It does
not create tags, publish Linux/Windows binaries, sign, notarize, or overwrite
an existing GitHub Release. Configure branch protection only after hosted CI
job names have stabilized; recommended required checks are `quality`,
`msrv-1.85`, `platform-windows`, and `platform-macos-arm64`.
