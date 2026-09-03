// pattern: Functional Core

use openjoc_api::{BinauralConfig, DialnormMode, OpenJocConfig, RenderMode};
use openjoc_wasm::{BINAURAL_HRTF_SOURCE, BINAURAL_VIRTUAL_LAYOUT, Decoder, WasmRenderer};

#[test]
fn binaural_decoder_uses_fixed_browser_renderer_contract() {
    let decoder =
        Decoder::new_with_dialnorm_and_renderer(DialnormMode::Default, WasmRenderer::Binaural)
            .expect("binaural decoder");
    let status = decoder.status();

    assert_eq!(status.renderer, "Binaural (Headphones)");
    assert_eq!(status.virtual_layout, Some(BINAURAL_VIRTUAL_LAYOUT));
    assert_eq!(status.hrtf, Some(BINAURAL_HRTF_SOURCE));
    assert_eq!(status.sample_rate, None);
    assert_eq!(status.output_channels, 2);
    assert_eq!(status.latency_samples, 577);
    assert!(!status.native_dolby_decoder_used);
}

#[test]
fn unknown_renderer_modes_are_not_constructible_through_the_public_enum() {
    assert_ne!(WasmRenderer::Stereo.code(), WasmRenderer::Binaural.code());
}

#[test]
fn non_binaural_modes_reject_binaural_configuration() {
    let config = OpenJocConfig {
        render_mode: RenderMode::Stereo,
        speaker_layout: String::from("2.0"),
        binaural: Some(BinauralConfig::builtin_generic("7.1.4")),
        ..OpenJocConfig::default()
    };
    assert!(config.validate().is_err());
}
