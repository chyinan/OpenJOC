# Custom transport regression fixture

`timestamps.ec3` contains the first 6 unchanged 4,096-byte access units from
OpenJOC's public synthetic `crates/openjoc-wasm/testdata/joc.lifecycle.ec3`.
It contains no private or proprietary audio. The source fixture is generated
by `openjoc-ffmpeg`'s `export_synthetic_joc_lifecycle_fixture_when_requested`
test, also invoked by `scripts/generate-player-fixtures.sh`.

The crate-local prefix keeps custom transport tests self-contained when the crate is
packaged independently. Each AU represents 1,536 samples at 48 kHz.

SHA-256: `29f1dabc13c6e4faff42dee1e18419309e48503d8f18c00e596088838d5585a4`
