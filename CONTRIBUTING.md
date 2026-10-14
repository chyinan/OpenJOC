# Contributing to OpenJOC

Keep each project fact in its canonical documentation owner:

- user onboarding and commands: `README.md`;
- current support: `docs/CAPABILITIES.md`;
- current boundaries: `docs/KNOWN_LIMITATIONS.md`;
- architecture: `docs/ARCHITECTURE.md`;
- renderer/output behavior: `docs/JOC_RENDER.md`;
- historical requirements/evidence: `docs/archive/requirements/`;
- provenance and clean-room rules: `docs/PROVENANCE.md`;
- future work: `docs/ROADMAP.md`;
- dated research and implementation history: `docs/research/`.

Do not copy a full current status table into a historical report. Summarize and
link to the canonical owner instead.

## Repository and documentation hygiene

Temporary Codex handoffs, audit/review reports, progress files, local QA
evidence, machine-specific paths, and task-result summaries must not be
committed to the repository root. Durable technical facts belong in the
canonical owner document. `CHANGELOG.md` owns release chronology; retained
version-specific acceptance or provenance evidence belongs under
`docs/archive/` only when it has ongoing public value.

Use short-lived branches:

```text
create feature branch
  -> implement and verify
  -> integrate through the repository workflow
  -> prove the work exists in master
  -> delete the completed branch
```

Do not retain completed branches as evidence; commits, tags, releases, and the
archive already provide history. Deliberate long-lived maintenance, upstream
tracking, or release branches are exceptions and should state their purpose.

Before adding a current technical claim, identify its owner. Secondary
documents may summarize and link, but must not create an independently
maintained duplicate specification.

## Research contributions

The [Open Problems & Contribution Opportunities](docs/site/project/open-problems.md)
page is the canonical guide for difficult codec, renderer, storage, and
hardware-validation work. Read it before starting a high-risk semantic change.
In particular, begin major research with a Discussion, state the evidence
boundary, preserve the clean-room rules, and include reproducible regression
coverage. Typo fixes, straightforward documentation changes, and small
deterministic bug fixes do not need a Discussion first.

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
python scripts/check_repository_hygiene.py
python -m unittest discover -s scripts/tests -v
git diff --check
```

If a documentation change moves or retires a file, repair tracked links and
update the release-assembly documentation inventory as part of the same
focused change.

## CI and release automation

GitHub Actions validates pull requests and pushes to `master` with the Linux
quality contract, the documented Rust 1.89 MSRV check, and platform-neutral
Windows x64 / macOS arm64 build and test jobs. Optional container tests that
need `ffmpeg`, `ffprobe`, or MP4Box remain explicitly skipped when those tools
or private fixtures are absent; no private Logic, ADM, DD+, EC-3, or evidence
files are used by CI.

The `scripts-ubuntu-24.04` and `scripts-windows-2025` jobs run the Python
regressions. Windows command and PowerShell tests run on Windows; tests that
inspect the external LAV fork require `OPENJOC_LAV_SOURCE_ROOT` to point to
that checkout. Private runtime and fixture gates retain their explicit skips.

### PCM gate diagnostics

`scripts/verify_pcm_bitexact.py` writes a small `evidence-*/report.json` in
its owned output directory for every paired run. It records source revisions,
lock hashes, the selected toolchain, completed pairs, and the original failure
message and stage. A best-effort working-tree fingerprint includes actual
tracked and non-ignored untracked contents, so different dirty trees at the
same HEAD can be distinguished. Symlinks hash link text without following it.
`toolchain.txt` keeps compiler/Cargo versions, harness and binary hashes, and
shared build flags. Toolchain/source provenance is captured before the builds
and mirrored into the run's evidence directory on completion or failure.

On a probe or comparison failure, the runner copies both sides' existing
manifests and timing files before cleaning up its temporary streams. Partial
manifests remain verbatim. Failure PCM samples share an 8 MiB total budget
across all available f32/f64 streams; `--failure-pcm-bytes` accepts 0 through
64 MiB (in bytes), and 0 omits PCM while retaining reports and manifests.
Samples focus on the first differing byte when one was reported, otherwise
the stream start. The report identifies each sample's original byte offset,
source length, retained length, and SHA-256. Truncated samples are diagnostic
excerpts, not replacement oracle streams. A fresh evidence directory avoids
mixing samples from different runs, and non-regular or out-of-scratch sources
are refused.

New diagnostic write/copy errors produce warnings without changing the
original gate result. The existing explicit `--keep` option still retains
full streams on successful comparison and keeps its original error behavior.
The frozen oracle, comparison rules, and build flags are unchanged.

Both PCM CI jobs always upload job status, available provenance and
measurements, and the evidence directory, with seven-day retention and an
explicit 8 MiB failure-PCM budget. Raw verbose build/probe logs and successful
full PCM streams are excluded. Job status is still uploaded when an earlier
setup step prevents the paired runner from starting.

Releases are human-authorized by pushing a stable `vMAJOR.MINOR.PATCH` tag.
The release workflow validates the tag against Cargo metadata, checks the tag
commit and `Cargo.lock`, runs the configured platform/package verification,
and publishes only the generated artifact set. It does not create tags,
rewrite an existing release, or imply signing/notarization that was not
performed. Configure branch protection only after hosted CI job names have
stabilized; recommended required checks are `quality`, `msrv-1.89`,
`platform-windows`, and `platform-macos-arm64`.

### Required public container coverage

The Linux `quality` job explicitly runs the public CLI container tests with
`OPENJOC_CONTAINER_TEST_MODE=required` and
`OPENJOC_CONTAINER_FIXTURE_MODE=fixed`. This step must execute the three
ordinary E-AC-3 demux, CLI routing, and raw-input cases and the two AAC track
selection cases. It prints the actual case names and FFmpeg/ffprobe versions.
Missing tools, unsuccessful or invalid version probes, unknown mode values,
and missing or changed fixed fixtures fail the step rather than skip it.
The existing private-fixture cases keep their independent opt-in gates.

Run the same public coverage locally with:

```sh
OPENJOC_CONTAINER_TEST_MODE=required OPENJOC_CONTAINER_FIXTURE_MODE=fixed \
  cargo test -p openjoc-cli --test container --locked -- --nocapture
```

With both variables unset, local tests retain optional dynamic generation
using FFmpeg, ffprobe, and MP4Box (`mp4box`); unavailable tools produce an
explicit skip. Either mode variable accepts only its documented values:
`optional` / `required` and `dynamic` / `fixed`. The two AAC cases need only
FFmpeg and ffprobe in either fixture mode. Fixed mode verifies SHA-256 for a
small checked-in ordinary E-AC-3 stream and MP4, then requires FFmpeg's stream
copy to match the already-frozen raw bytes. It never derives the expected raw
bytes from that run's demux result. The fixture and complete creation commands
are documented in
[`crates/openjoc-cli/tests/fixtures/container/README.md`](crates/openjoc-cli/tests/fixtures/container/README.md).
These are public container-boundary bytes, not JOC/Atmos or PCM oracle evidence;
no existing golden output or frozen PCM expectation is replaced.

### Frozen session contracts

The `session-contracts-1.89.0` and `session-contracts-1.99.0` Linux CI jobs
compare every constructor, terminal-error and lifecycle capture against the
independent pre-refactor expectations retained for PR #53. Run locally with
a new output directory:

```sh
python3 scripts/verify_session_contracts.py --toolchain 1.99.0 \
  --output-dir /tmp/openjoc-session-contracts
```

The default is locked/offline; add `--online` to fetch locked dependencies.
This checks exact file sets, complete byte lengths and SHA-256 digests for
92 scenarios (184 PCM and event files). Integrated single-bit negative
controls cover both PCM and events. There is no refresh/bless mode.
The original raw files were not retained in the compact evidence archive,
so this is a full-file digest gate without historical first-differing-byte
localization. It supplements the pinned historical PCM oracle; it does not
replace or rerun the separate historical 22-case or external 204-case gates.
See the [frozen contract provenance](scripts/fixtures/session-contracts/README.md)
for source stages, toolchains, scope and failure handling.
