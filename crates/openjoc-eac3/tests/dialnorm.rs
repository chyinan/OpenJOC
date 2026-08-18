use openjoc_eac3::{
    ChannelLocation, DecodedAccessUnitPcm, DialnormMode, DialnormState, DownmixMetadata,
};

fn assert_close(actual: f64, expected: f64) {
    let relative = (actual - expected).abs() / expected.max(f64::MIN_POSITIVE);
    assert!(relative <= 2.0e-12, "{actual} != {expected}");
}

#[test]
fn digital_numeric_fixtures_use_the_reserved_zero_fallback() {
    for (encoded, effective, expected) in [
        (31, 31, 1.0),
        (0, 31, 1.0),
        (27, 27, 0.6309573444801932),
        (24, 24, 0.44668359215096315),
        (20, 20, 0.28183829312644537),
        (1, 1, 0.03162277660168379),
        (12, 12, 0.11220184543019636),
    ] {
        let state = DialnormState::new(DialnormMode::Digital, encoded);
        assert_eq!(state.effective_value(), effective);
        assert_close(state.linear_gain(), expected);
    }
}

#[test]
fn analog_is_unity_and_default_matches_digital() {
    for encoded in [0, 1, 20, 24, 27, 31] {
        let analog = DialnormState::new(DialnormMode::Analog, encoded);
        let digital = DialnormState::new(DialnormMode::Digital, encoded);
        let default = DialnormState::new(DialnormMode::Default, encoded);
        assert_eq!(analog.linear_gain(), 1.0);
        assert_eq!(default.effective_value(), digital.effective_value());
        assert_eq!(default.linear_gain(), digital.linear_gain());
    }
}

#[test]
fn lifecycle_state_has_explicit_unity_initial_and_reset_values() {
    let mut state = DialnormState::default();
    assert_eq!(state.mode(), DialnormMode::Default);
    assert_eq!(state.effective_value(), 31);
    assert_eq!(state.linear_gain(), 1.0);

    state.update(20);
    assert_eq!(state.effective_value(), 20);
    assert_close(state.linear_gain(), 0.28183829312644537);

    state.reset();
    assert_eq!(state.mode(), DialnormMode::Default);
    assert_eq!(state.effective_value(), 31);
    assert_eq!(state.linear_gain(), 1.0);
}

#[test]
fn common_scalar_covers_base_object_and_full_program_once() {
    let state = DialnormState::new(DialnormMode::Digital, 11);
    assert_eq!(state.linear_gain(), 0.1);

    let mut base_only = 0.25;
    state.apply_to_samples(std::slice::from_mut(&mut base_only));
    let mut object_only = 0.125;
    state.apply_to_samples(std::slice::from_mut(&mut object_only));
    let mut full = 0.25 + 0.125;
    state.apply_to_samples(std::slice::from_mut(&mut full));

    assert_close(base_only, 0.1 * 0.25);
    assert_close(object_only, 0.1 * 0.125);
    assert_close(full, 0.1 * (0.25 + 0.125));
    assert!((full - 0.1 * 0.1 * (0.25 + 0.125)).abs() > 1.0e-3);
}

#[test]
fn prepared_scalar_scales_the_retained_lfe_plane_with_base() {
    let frame = DecodedAccessUnitPcm {
        sample_rate: 48_000,
        samples: 1,
        channel_locations: vec![ChannelLocation::Left],
        channels: vec![vec![0.25]],
        lfe_location: Some(ChannelLocation::Lfe(0)),
        lfe: Some(vec![0.5]),
        downmix: DownmixMetadata::default(),
        dialnorm: DialnormState::new(DialnormMode::Digital, 24),
    };
    let calibrated = frame.with_dialnorm_applied();
    assert_close(calibrated.channels[0][0], 0.11167089803774079);
    assert_close(
        calibrated.lfe.as_ref().expect("LFE")[0],
        0.22334179607548158,
    );
}
