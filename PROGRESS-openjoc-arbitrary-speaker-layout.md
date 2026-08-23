# OpenJOC arbitrary speaker layout progress

## Session state

- `START_HEAD`: `33ef4bc47531b32f302443c0225b328070b9d79c`
- Branch: `feat/arbitrary-speaker-layout`
- Public baseline: origin/master at v0.10.0; the independent Windows onboarding branch is not required for this work.
- Published `v0.10.0` tag is unchanged.

## Architecture audit

- Existing `openjoc_scene::SpatialLayout` and `JocSpatialBridge` are already a generic data-driven projector.
- Existing presets are topology data plus semantic output labels; they do not contain a separate DSP engine.
- `OpenJocSession`, the CLI speaker renderer, and the C ABI currently accept preset names only.
- Existing output writers preserve standard WAV masks and CAF labels; arbitrary geometry needs unmasked WAV or truthful CAF coordinate metadata.

## Confirmed gates

- Baseline targeted library tests: PASS (`openjoc-scene`, `openjoc-api`, `openjoc-cli`, `openjoc-capi`).
- Canonical `SpeakerLayout` model: PASS. It wraps the existing `SpatialLayout`, supports presets and validated custom spherical geometry, preserves the existing public `SemanticChannelLayout` shape, and defines versioned JSON parsing.
- Preset regression: PASS. Existing CLI renderer checksum (`default_full_wav_matches_start_head_checksum`) and full preset renderer suite remain unchanged; the public standard preset path still uses the same generic projector.
- Custom JSON/CLI/Rust API/C ABI: PASS. `--layout-file`, `OpenJocConfig::with_speaker_layout`, and ABI 1.4 in-memory geometry descriptors are covered by focused tests.
- Custom Base handling: PASS. Standard codec Base identities are precomputed as explicit route vectors into arbitrary target geometry; LFE remains separate and can be repeated for multiple logical LFE outputs.
- Real JOC preset/custom renders: PASS. Public generated `joc.ec3` rendered as preset 5.1 WAV and custom `studio-irregular` CAF/WAV. Custom WAV was checked with ffprobe and reports `channel_layout=unknown` rather than a false standard mask.
- Independent closure: IN PROGRESS. `266a2bf..c85df64` is report/documentation-only; the 13-preset START-vs-FINAL oracle is bit-identical for every preset. See `OPENJOC_ARBITRARY_LAYOUT_PRESET_ORACLE.md`.
- Partial geometry policy: PASS. Finite out-of-bound x/y source coordinates clamp to edge anchors/rows; z clamps to outer layers; adjacent layers use existing equal-power cosine/sine blending; dynamic targets normalize and remain finite/bounded.
- ABI compatibility: PASS. A real ABI 1.3 declaration/header caller links to the ABI 1.4 library, uses preset 5.1 decode/render, flush/reset/destroy, and preserves a canary around the old config allocation. The legacy initializer now writes only the ABI 1.3 prefix; ABI 1.4 callers use `openjoc_decoder_config_init_v1_4`.
- Closure acceptance: PASS. `OPENJOC_ARBITRARY_LAYOUT_PRESET_ORACLE.md` records 13/13 bit-identical presets; `OPENJOC_ARBITRARY_LAYOUT_INDEPENDENT_REVIEW_CLOSURE.md` records `material_blocker_remaining=NO` and `ready_to_integrate=YES`.
- Quality gates: PASS. `cargo fmt --all -- --check`, `cargo check --workspace`, `cargo clippy --workspace --all-targets --all-features`, `cargo test --workspace`, `scripts/test-c-api.sh`, and the public-fixture `self-test` pass.

## Next actions

1. Integrate with the independent Windows onboarding branch only after its branch is available.
2. Re-run combined integration/release gates before any v0.11 decision.
