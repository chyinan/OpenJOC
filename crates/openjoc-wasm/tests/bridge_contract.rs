use openjoc_wasm::{Decoder, DecoderStatus, FailureCategory};

#[test]
fn decoder_starts_with_stereo_output_contract() {
    let decoder = Decoder::new().expect("stereo decoder");
    let status = decoder.status();

    assert_eq!(status.decoder, "OpenJOC");
    assert_eq!(status.renderer, "Stereo (Speakers)");
    assert_eq!(status.sample_rate, None);
    assert_eq!(status.output_channels, 2);
    assert!(!status.native_dolby_decoder_used);
}

#[test]
fn decoder_holds_partial_syncframe_input_until_more_bytes_arrive() {
    let mut decoder = Decoder::new().expect("stereo decoder");

    assert_eq!(
        decoder.push_bytes(&[0x0b, 0x77]),
        DecoderStatus::NeedMoreInput
    );
    assert_eq!(decoder.status().queued_audio_ms, 0.0);
}

#[test]
fn flushing_empty_input_returns_bounded_invalid_input_diagnostic() {
    let mut decoder = Decoder::new().expect("stereo decoder");

    let error = decoder.flush().expect_err("empty stream must fail");

    assert_eq!(error.category, FailureCategory::InvalidInput);
    assert!(error.detail.len() <= 256);
    assert_eq!(decoder.status().queued_audio_ms, 0.0);
}
