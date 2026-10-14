# CLI output serialization contract

`output-contract.json` records the complete file lengths and SHA-256 digests
captured on 2026-10-14 before extracting the CLI output implementation from
`joc_render.rs`. The 618-line output block was unchanged from merged commit
`22c29cc21d009458cc3a520e100442ce11cf58f6`; its exact SHA-256 was
`4ad76acda4a62e76a6f7cc0c06804b06b2e95817d8fedaa44e1736fe26de8187`.
The capture used Rust 1.99.0 (`b940084d7`), the existing locked dependencies,
and no `RUSTFLAGS`. It is a new, independently captured pre-extraction writer
baseline, not a replacement for any historical PCM or session oracle.

The 92 files total 85,718 bytes. They cover all public speaker presets and
binaural output, WAV where its channel semantics are representable, CAF, and
F32/F64/S24/S16 serialization. Each channel receives 17 deterministic F64
samples in two writes, including negative zero and values whose low bits
would be lost in an F32 roundtrip. The frozen digests include the complete
container headers, channel descriptions and interleaved payloads.

Run the two contract tests with:

```sh
cargo test -p openjoc-cli --bin openjoc joc_render::output_contract:: --locked
```

The tests always read and compare the checked-in manifest. There is no
expected-output update mode. Optionally set `OPENJOC_CLI_OUTPUT_CAPTURE` to
an existing empty directory to export the 92 actual files and their actual
manifest for inspection; export uses create-only writes. A digest mismatch
identifies the differing file, not its first differing raw byte.

The publication checks cover deferred publication until `finish`, repeated
finish errors, late destination conflicts, authorized overwrite, abort and
pre-existing staging files. They assert the existing error text and that
failure cleanup preserves previously published files. The separate paired
ADM transaction in `main.rs` is outside this writer contract.
