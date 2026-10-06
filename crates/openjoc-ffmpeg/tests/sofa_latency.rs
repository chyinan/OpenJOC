use openjoc_api::{
    BinauralConfig, BinauralLfePolicy, OpenJocConfig, OpenJocSession, QMF_LATENCY_SAMPLES,
    RenderMode,
};

#[test]
fn custom_sofa_bridge_latency_matches_session_before_and_after_reset() {
    for (rate, bytes) in [
        (24_000, include_bytes!("fixtures/sofa-latency/24000.sofa")),
        (44_100, include_bytes!("fixtures/sofa-latency/44100.sofa")),
        (48_000, include_bytes!("fixtures/sofa-latency/48000.sofa")),
        (96_000, include_bytes!("fixtures/sofa-latency/96000.sofa")),
    ] {
        let config = OpenJocConfig {
            render_mode: RenderMode::Binaural,
            speaker_layout: "5.1".to_owned(),
            binaural: Some(BinauralConfig::from_sofa_bytes(
                bytes.to_vec(),
                "5.1",
                BinauralLfePolicy::EqualPowerDualMono,
            )),
            ..OpenJocConfig::default()
        };
        let expected = OpenJocSession::new(config.clone())
            .unwrap()
            .latency_samples();
        assert_eq!(expected == QMF_LATENCY_SAMPLES, rate == 48_000);
        for pull in [false, true] {
            let mut bridge = if pull {
                openjoc_ffmpeg::FfmpegDecoder::new_with_listener_orientation_pull(
                    config.clone(),
                    128,
                )
            } else {
                openjoc_ffmpeg::FfmpegDecoder::new(config.clone())
            }
            .unwrap();
            assert_eq!(
                bridge.latency_samples(),
                expected,
                "rate={rate} pull={pull}"
            );
            assert_eq!(bridge.latency_time().0, i64::try_from(expected).unwrap());
            bridge.drain().unwrap();
            assert_eq!(bridge.latency_samples(), expected);
            bridge.reset();
            assert_eq!(bridge.latency_samples(), expected);
        }
    }
}
