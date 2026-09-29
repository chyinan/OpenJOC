#[allow(dead_code)]
#[path = "../../openjoc-wasm/tests/support/sofa.rs"]
mod sofa_fixture;

use openjoc_api::{
    BinauralConfig, BinauralLfePolicy, OpenJocConfig, OpenJocSession, QMF_LATENCY_SAMPLES,
    RenderMode,
};

#[test]
fn custom_sofa_initialization_reports_added_resampling_latency() {
    for rate in [24_000, 44_100, 48_000, 96_000] {
        let bytes = sofa_fixture::custom_hdf5_sofa_fixture(rate, false);
        let config = OpenJocConfig {
            render_mode: RenderMode::Binaural,
            speaker_layout: "5.1".to_owned(),
            binaural: Some(BinauralConfig::from_sofa_bytes(
                bytes,
                "5.1",
                BinauralLfePolicy::EqualPowerDualMono,
            )),
            ..OpenJocConfig::default()
        };
        let mut session = OpenJocSession::new(config).unwrap();
        let added = openjoc_sofa::hrir_resampling_delay_samples(rate, 48_000).unwrap();
        assert_eq!(session.latency_samples(), QMF_LATENCY_SAMPLES + added);
        assert_eq!(
            session.output_info().latency_samples,
            QMF_LATENCY_SAMPLES + added
        );
        session.drain().unwrap();
        assert_eq!(session.latency_samples(), QMF_LATENCY_SAMPLES + added);
    }
}
