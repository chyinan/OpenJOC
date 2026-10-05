# Canonical 22.2 listener-orientation regression record

Base: `15f9281d888c0b4538e48fd8107d26981f28b034` (PRs #12–#14 merged).
Validation date: 2026-10-05; Linux x86_64, Rust 1.89.0.

## Geometry and resolver behavior

`SpeakerLayoutPreset::virtual_speaker_direction` is the shared API/CLI source
of virtual directions. For 22.2 it reads the canonical BS.2051 Sound System H
topology in `speaker_layouts.rs`, converting topology coordinates to +X right,
+Y front, +Z up. All 22 non-LFE labels are checked against the canonical
midpoint azimuth/elevation values. In particular FL/FR are ±52.5°, rather than
the ±45° directions retained for the established layouts. LFE1/LFE2 return no
direction; neither becomes an orientation source or HRIR kernel.

The built-in static 22.2 API/CLI path uses the orientation resolver's existing
bounded candidate expansion. The legacy static search can reject System H
positions despite a nearest measurement only 2.24° away. No nearest-neighbor
fallback, fabricated coordinates or coverage bypass is introduced. Other
presets retain their prior direction and static resolver behavior. Custom SOFA
coverage remains resource-dependent and uncovered requests still fail.

## Regressions

- Scene: every public preset has directions for exactly its non-LFE channels;
  all 22 System H angles, both LFE exclusions and prior stereo FL are checked.
- D1/D2: complete 22-source identity updates and yaw/pitch/roll on each axis
  at −30°, −15°, 0°, +15°, +30°; source IDs/order and world directions checked.
- Identity: prepared HRIR pairs equal static pairs exactly; PCM and FIR tails
  are bit-identical across irregular pull partitions for Exclude and
  EqualPowerDualMono policies. Existing LFE mixing policy is unchanged.
- Public Rust: standard and bounded-pull orientation sessions create, prepare,
  apply two updates and reset their stream epochs with D1/D2.
- C ABI: decoder and stream handle tests exercise D1/D2 22.2, update ownership,
  retirement of 22 kernels and epoch fencing. An actual C11 caller creates,
  prepares and applies identity/yaw updates through the installed header.
- CLI: built-in f32 D1/D2 construction uses the shared 22.2 geometry and maps
  exactly 22 non-LFE sources.

Reproduce the regressions:

```sh
cargo test -p openjoc-api -p openjoc-scene -p openjoc-cli -p openjoc-capi --lib --tests
bash scripts/test-c-api.sh
cargo clippy -p openjoc-api -p openjoc-scene -p openjoc-cli -p openjoc-capi --all-targets --all-features --locked -- -D warnings
cargo fmt --all -- --check
python -m mkdocs build --strict
```

## Short profile coverage diagnostic

```sh
cargo run --release -j 2 -p openjoc-api --features orientation-profile --example orientation_profile -- 16
```

The example clamps this argument to **20 measured calls per combination**,
after 30 warmup calls. The attached [raw CSV](coverage-profile.csv) contains
40 HRTF/layout/trajectory combinations (280 stage rows): built-in D1/D2 ×
2.0/5.1/7.1.4/9.1.6/22.2 × identity/rapid/nearby/smooth. All 40 combinations,
including all eight 22.2 combinations, constructed and returned **zero
preparation errors**. No `unsupported` combination was printed.

This is a short coverage diagnostic with profiling enabled, run alongside
other checks. Its timing columns do **not** establish a 4 ms p95 latency pass,
long-run stability, arbitrary-pose coverage, perceptual quality or actual
sensor/device latency for 22.2. Earlier dated performance evidence remains
historical and is not rewritten.
