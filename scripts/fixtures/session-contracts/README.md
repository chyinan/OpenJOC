# Independently frozen session contracts

`frozen.json` preserves the complete `files` dictionaries from three
independent captures made **before** their respective product refactors in
[PR #53](https://github.com/chyinan/OpenJOC/pull/53). No expected digest or
length was derived from this runner's candidate output. The original archive
member hashes, archive hash, input hashes and toolchain records are in the
manifest. All 184 original file entries are retained verbatim as JSON values.

| Suite | Source state when frozen | Cases | PCM bytes | Event bytes |
| --- | --- | ---: | ---: | ---: |
| Constructors | Base `57e4c72be982b893569bc48c7615892c6499bd7d` plus failure-evidence/container-test work, before `SessionOptions` | 16 | 1,504,656 | 150,909 |
| Terminal errors | After `SessionOptions`, before typed terminal-error storage | 28 | 2,842,000 | 237,086 |
| Lifecycle | After typed terminal errors, before lifecycle helper extraction | 48 | 3,259,104 | 602,863 |

These are three distinct pre-refactor source states, not one pristine base
commit or a new baseline captured from merged `22c29cc`. Their original
`session.rs` SHA-256 values are respectively
`4382276b1f32f569d51b7ecdf26468bd8c981aa3ca5912bd8187d62392d569fc`,
`22ba3839b100c3442fe675c946d99baf54f753258c1c57431dac177809edb048` and
`fe493da934ba54b76c6a99646848ba007764eb07fb046b8674ab925fc4efb3bc`.

## Producer and harness identity

The original captures used Rust/Cargo **1.99.0** (`+stable` at capture time),
LLVM 23.1.1, `x86_64-unknown-linux-gnu`, and `cargo test --locked --offline`
in the unoptimized test profile. Debug information and incremental compilation
were disabled, with one build job and no `RUSTFLAGS` or
`CARGO_ENCODED_RUSTFLAGS`. The manifest records exact compiler commits and
the evidence records that established these settings. The runner selects an
explicit toolchain (1.99.0 by default), records actual versions and build
flags, and reuses those profile settings. CI also checks the same unchanged
expectations using the project's Rust 1.89 MSRV. The established PCM platform
is Linux x86-64; other targets are not silently assumed bit-identical.

The existing Rust tests still produce the captures using their create-only
environment variables. This change does not edit them or their codec fixture.
The constructor harness's one historical Clippy correction replaced
`assert!(!result.pcm.is_empty())` with `assert_ne!(result.pcm, [] as [u8; 0])`.
The archived final full-byte comparison established that this exact final
harness still matched all original captures; its evidence hash is preserved.
The terminal and lifecycle harnesses match their original frozen source hashes.

## Run and inspect

```sh
python3 scripts/verify_session_contracts.py --toolchain 1.99.0 \
  --output-dir /tmp/openjoc-session-contracts \
  --target-dir /tmp/openjoc-session-target
```

Use a **new** output directory for every invocation. `--target-dir` can reuse
the Cargo build cache; omit it to put the cache inside the new output directory.
`--online` permits fetching only locked Cargo dependencies. The runner does
not install toolchains or tools. It runs only the existing constructor tests,
terminal-error contract module and `session::lifecycle_contract` module.

Each suite must produce its exact complete set of paired `.f32le` and `.txt`
files. Missing, extra, truncated, modified, non-regular or symlinked captures
fail closed. SHA-256 consumes **every byte**, including the complete event
transcript; no floating-point tolerance, normalization, sampling or approximate
measurement is involved. Each suite also flips one bit in separate disposable
PCM and event copies and requires the same comparator to reject both. The
original captures and manifest are not written by those controls.

`report.json` contains all compared records, source-content hashes, actual
toolchain output, commands, build settings and failure information. The fixture,
lockfile and capture-harness hashes are checked before execution, and source
contents and the manifest must remain unchanged during the run. Captures and
test logs stay in the invocation's directory for local diagnosis; CI uploads
only the report, toolchain descriptions and logs with seven-day retention.

## Evidence boundary and failures

The retained compact archive has complete original lengths/digests, but **no
original raw PCM or event files**. This runner therefore establishes equality
by full-file length plus SHA-256, not a renewed direct comparison against the
missing historical bytes. It cannot identify the first differing historical
byte. A failure identifies the file and expected/actual digest and length,
while retaining the current capture for investigation.

Do not repair a failure by regenerating or replacing expected data from the
candidate. Investigate the source, harness, toolchain or dependency change.
The runner intentionally has no update or bless mode. Any future deliberately
different contract needs its own independently justified provenance, without
silently overwriting these historical expectations.

This suite is distinct from the pinned `15aefe1` PCM oracle and the external
204-case matrix. The latter's original overlay and raw captures were not
retained in this compact archive, so this runner does not claim to execute it.
The existing historical PCM gate, probes, comparison rules and expected outputs
are unchanged. These session captures cover F32 output and full observable
events for the listed scenarios; they do not substitute for CLI F64, native
adapter, private-fixture or all-platform coverage.
