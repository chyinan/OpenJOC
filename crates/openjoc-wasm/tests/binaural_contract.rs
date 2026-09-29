// pattern: Functional Core

use openjoc_api::{BinauralConfig, DialnormMode, OpenJocConfig, RenderMode};
use openjoc_wasm::{
    BINAURAL_HRTF_SOURCE, BINAURAL_VIRTUAL_LAYOUT, Decoder, DecoderStatus, WasmRenderer,
};

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

#[test]
fn custom_sofa_decoder_uses_the_existing_strict_sofa_parser() {
    let result = Decoder::new_with_dialnorm_renderer_and_custom_sofa(
        DialnormMode::Default,
        WasmRenderer::Binaural,
        b"not a SOFA file",
    );

    assert!(result.is_err(), "malformed custom SOFA must fail closed");
}

#[test]
fn custom_sofa_decoder_accepts_supported_local_sofa_bytes() {
    let sofa = custom_sofa_fixture();
    let decoder = Decoder::new_with_dialnorm_renderer_and_custom_sofa(
        DialnormMode::Default,
        WasmRenderer::Binaural,
        &sofa,
    )
    .expect("compatible local SOFA should initialize the existing binaural path");

    assert_eq!(decoder.status().hrtf, Some("Custom SOFA"));
    assert_eq!(decoder.status().output_channels, 2);
}

#[test]
fn custom_sofa_decoder_rejects_expanded_hrir_banks_over_the_wasm_memory_budget() {
    let sofa = custom_sofa_fixture_with_delay(2000.0);
    let result = Decoder::new_with_dialnorm_renderer_and_custom_sofa(
        DialnormMode::Default,
        WasmRenderer::Binaural,
        &sofa,
    );

    assert!(
        result.is_err(),
        "an expanded f64 HRIR bank over the WASM limit must fail closed"
    );
}

#[test]
fn custom_sofa_decoder_renders_real_joc_frames_to_finite_stereo_pcm() {
    let sofa = custom_sofa_fixture();
    let mut decoder = Decoder::new_with_dialnorm_renderer_and_custom_sofa(
        DialnormMode::Default,
        WasmRenderer::Binaural,
        &sofa,
    )
    .expect("compatible local SOFA should initialize");
    let fixture = include_bytes!("../testdata/joc.ec3");
    let mut frames = Vec::new();
    for chunk in fixture.chunks(97) {
        assert_ne!(decoder.push_bytes(chunk), DecoderStatus::Error);
        while let Some(frame) = decoder.receive_pcm() {
            frames.push(frame);
        }
    }
    loop {
        let status = decoder.flush().expect("custom SOFA bridge drain");
        while let Some(frame) = decoder.receive_pcm() {
            frames.push(frame);
        }
        if status == DecoderStatus::EndOfStream {
            break;
        }
    }
    assert!(!frames.is_empty());
    assert!(
        frames
            .iter()
            .any(|frame| frame.interleaved_f32.iter().any(|sample| *sample != 0.0))
    );
    assert!(frames.iter().all(|frame| {
        frame.sample_rate == 48_000
            && frame.channel_count == 2
            && frame
                .interleaved_f32
                .iter()
                .all(|sample| sample.is_finite())
    }));
}

#[path = "support/sofa.rs"]
mod sofa;
use sofa::{custom_sofa_fixture, custom_sofa_fixture_with_delay};
