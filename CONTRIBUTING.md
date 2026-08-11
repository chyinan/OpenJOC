# Contributing to OpenJOC

Keep each project fact in its canonical documentation owner:

- user onboarding and commands: `README.md`;
- current support: `docs/CAPABILITIES.md`;
- current boundaries: `docs/KNOWN_LIMITATIONS.md`;
- architecture: `docs/ARCHITECTURE.md`;
- detailed engineering design: `docs/ENGINEERING_SPEC.md`;
- requirements and status: `docs/REQUIREMENTS_MATRIX.md`;
- provenance and clean-room rules: `docs/PROVENANCE.md`;
- future work: `docs/ROADMAP.md`;
- dated research and implementation history: `docs/research/`.

Do not copy a full current status table into a historical report. Summarize and
link to the canonical owner instead.

Codec changes must remain explainable from permitted public normative sources,
public mathematics, or explicitly authorized controlled evidence. Do not use
proprietary/decompiled decoder sources or private research material as
implementation provenance.

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
