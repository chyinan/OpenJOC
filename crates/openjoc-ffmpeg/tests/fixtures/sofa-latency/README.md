# Synthetic SOFA latency fixtures

These four valid HDF5 SOFA files were generated from the repository-owned
`crates/openjoc-wasm/tests/support/sofa.rs` function
`custom_hdf5_sofa_fixture(rate, false)` at source commit
`08f989a63ee50c03f42d6fee544cbf9775c07d03`, using the existing locked
`hdf5-pure` 0.47.0 writer. The rate is the filename without `.sofa`:
24000, 44100, 48000, or 96000 Hz. Geometry, impulse samples and attributes
are the generator's synthetic defaults; no external audio or HRTF dataset
is included. This is the same generator used by the core API latency tests.

To regenerate, import that support module in a Rust program with the existing
locked `hdf5-pure` dependency and write each function result directly to its
corresponding file. No conversion, compression override, or postprocessing
is performed. Rust 1.98.1 was used for this generation.

The Rust bridge and C stream tests share these bytes through `include_bytes!`.
Freezing the fixtures avoids adding direct test dependencies or changing the
workspace's locked dependency graph.

SHA-256:

- 24000.sofa: 89923520a87d9867adc74eb8d4e34ee70e4876aeda700a24f6afcf5116bc6d34
- 44100.sofa: cb4c4954d02031cc520bb14b32b20e364e1d51d0ab8394cb0e52b6866dc93f70
- 48000.sofa: b05b125e0a012f62ad855d627d7ec8956377c3194c2307bcce2f03c41431d39a
- 96000.sofa: 973bd7dff85c7d67c2c74c6448840eb86e25f9d597f5b7ef8731aa002e2d4658
