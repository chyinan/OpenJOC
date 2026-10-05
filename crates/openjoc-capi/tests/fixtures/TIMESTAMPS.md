# Timestamp regression fixture

`timestamps.ec3` contains the first 3 unchanged 4,096-byte access units from
OpenJOC's public synthetic `crates/openjoc-wasm/testdata/joc.lifecycle.ec3`.
It contains no private or proprietary audio. The source fixture is generated
by `openjoc-ffmpeg`'s `export_synthetic_joc_lifecycle_fixture_when_requested`
test, also invoked by `scripts/generate-player-fixtures.sh`.

The crate-local prefix keeps timestamp tests self-contained when the crate is
packaged independently. Each AU represents 1,536 samples at 48 kHz.

SHA-256: `0b624254ea4511f3d2b13d6d3222e4be9f423553d6588420e16c32b83f6cad16`
