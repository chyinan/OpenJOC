// pattern: Functional Core

use openjoc_render::{CartesianPosition, HrirBank, HrirEar, HrirEntry, HrirEntryId, HrirPair};
use openjoc_sofa::{
    LoadedSofaHrirBank, SofaError, SofaHrirMetadata, SofaLoadLimits, hrir_resampling_delay_samples,
    resample_loaded_hrir_bank, resolve_hrir,
};

#[test]
fn common_filter_delay_is_explicit_and_rejects_invalid_rates() {
    for (source, target, delay) in [
        (48_000, 48_000, 0),
        (24_000, 48_000, 33),
        (44_100, 48_000, 19),
        (96_000, 48_000, 17),
        (3_000, 48_000, 257),
        (48_000, 3_000, 17),
    ] {
        assert_eq!(
            hrir_resampling_delay_samples(source, target).unwrap(),
            delay
        );
    }
    for (source, target) in [
        (0, 48_000),
        (48_000, 0),
        (0, 0),
        (2_999, 48_000),
        (48_000, 2_999),
    ] {
        assert!(matches!(
            hrir_resampling_delay_samples(source, target),
            Err(SofaError::InvalidSamplingRate(_))
        ));
    }
}

fn bank(rate: u32, left: Vec<f64>, right: Vec<f64>, delays: [usize; 2]) -> LoadedSofaHrirBank {
    let len = left.len();
    let pair = HrirPair::new_with_delays(rate, left, right, delays).unwrap();
    let entries = [-1.0, 1.0]
        .into_iter()
        .enumerate()
        .map(|(index, y)| {
            HrirEntry::new(
                HrirEntryId::new(index as u64),
                CartesianPosition::new(1.0, y, 0.0),
                pair.clone(),
            )
            .unwrap()
        })
        .collect();
    LoadedSofaHrirBank {
        bank: HrirBank::new(rate, entries).unwrap(),
        metadata: SofaHrirMetadata {
            convention_version: "1.0".into(),
            title: None,
            database_name: None,
            listener_short_name: None,
            license: None,
            measurement_count: 2,
            original_fir_length: len - delays[0].max(delays[1]),
            expanded_max_tap_length: len,
            sample_rate_hz: rate,
        },
    }
}

fn impulse_bank(rate: u32, delays: [usize; 2]) -> LoadedSofaHrirBank {
    let mut left = vec![0.0; 129];
    let mut right = left.clone();
    left[delays[0]] = 1.0;
    right[delays[1]] = 1.0;
    bank(rate, left, right, delays)
}

#[test]
fn matching_rate_preserves_all_coefficient_bits_and_metadata() {
    let taps = vec![0.0, -0.0, f64::from_bits(1), 0.1, -1.2];
    let source = bank(48_000, taps.clone(), taps.clone(), [0, 0]);
    let expected = source.clone();
    let actual = resample_loaded_hrir_bank(source, 48_000, SofaLoadLimits::default()).unwrap();
    assert_eq!(actual.metadata, expected.metadata);
    assert_eq!(actual.bank, expected.bank);
    for entry in actual.bank.entries() {
        for ear in [entry.pair().left_taps(), entry.pair().right_taps()] {
            assert_eq!(
                ear.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
                taps.iter().map(|x| x.to_bits()).collect::<Vec<_>>()
            );
        }
    }
}

#[test]
fn rate_conversion_preserves_unit_impulse_dc_gain_including_the_first_tap() {
    for rate in [24_000, 32_000, 44_100, 47_999, 96_000, 192_000] {
        for delay in [0, 1, 64, 65] {
            let actual = resample_loaded_hrir_bank(
                impulse_bank(rate, [delay, delay]),
                48_000,
                SofaLoadLimits::default(),
            )
            .unwrap();
            let sum: f64 = actual.bank.entries()[0].pair().left_taps().iter().sum();
            assert!(
                (sum - 1.0).abs() < 2e-4,
                "rate={rate}, delay={delay}, DC gain={sum}"
            );
            assert_eq!(actual.metadata.sample_rate_hz, rate);
            assert_eq!(actual.bank.sample_rate_hz(), 48_000);
        }
    }
}

#[test]
fn resampled_delay_prefix_stays_zero_and_identical_neighbors_interpolate_unchanged() {
    for rate in [24_000, 44_100, 96_000] {
        let actual = resample_loaded_hrir_bank(
            impulse_bank(rate, [64, 65]),
            48_000,
            SofaLoadLimits::default(),
        )
        .unwrap();
        let exact = actual.bank.entries()[0].pair();
        let midpoint = resolve_hrir(&actual.bank, CartesianPosition::new(1.0, 0.0, 0.0)).unwrap();
        for (ear, taps, interpolated) in [
            (HrirEar::Left, exact.left_taps(), midpoint.pair.left_taps()),
            (
                HrirEar::Right,
                exact.right_taps(),
                midpoint.pair.right_taps(),
            ),
        ] {
            let delay = exact.delay_samples(ear);
            assert!(
                taps[..delay].iter().all(|&x| x == 0.0),
                "nonzero prefix: rate={rate}, ear={ear:?}"
            );
            let error = taps
                .iter()
                .zip(interpolated)
                .map(|(a, b)| (a - b).abs())
                .fold(0.0, f64::max);
            assert!(
                error < 1e-12,
                "identical neighbors: rate={rate}, ear={ear:?}, error={error}"
            );
            // The existing interpolator pads both ears to a common tail when
            // their delays differ. Any additional coefficients must be zero.
            assert!(interpolated[taps.len()..].iter().all(|&x| x == 0.0));
        }
    }
}

#[test]
fn early_and_delayed_impulses_keep_the_same_complete_filter_shape() {
    for (rate, shift) in [(24_000, 128), (96_000, 32)] {
        let early = resample_loaded_hrir_bank(
            impulse_bank(rate, [0, 0]),
            48_000,
            SofaLoadLimits::default(),
        )
        .unwrap();
        let delayed = resample_loaded_hrir_bank(
            impulse_bank(rate, [64, 64]),
            48_000,
            SofaLoadLimits::default(),
        )
        .unwrap();
        let early = early.bank.entries()[0].pair().left_taps();
        let delayed = delayed.bank.entries()[0].pair().left_taps();
        for (index, value) in delayed.iter().enumerate() {
            let expected = index
                .checked_sub(shift)
                .and_then(|i| early.get(i))
                .copied()
                .unwrap_or(0.0);
            assert!(
                (value - expected).abs() < 1e-12,
                "rate={rate}, sample={index}: {value} != {expected}"
            );
        }
    }
}

fn magnitude(taps: &[f64], frequency: f64) -> f64 {
    let (real, imag) = taps
        .iter()
        .enumerate()
        .fold((0.0, 0.0), |(real, imag), (index, tap)| {
            let phase = std::f64::consts::TAU * frequency * index as f64 / 48_000.0;
            (real + tap * phase.cos(), imag - tap * phase.sin())
        });
    real.hypot(imag)
}

#[test]
fn downsampled_impulse_keeps_audio_passband_gain() {
    let actual = resample_loaded_hrir_bank(
        impulse_bank(96_000, [64, 64]),
        48_000,
        SofaLoadLimits::default(),
    )
    .unwrap();
    let taps = actual.bank.entries()[0].pair().left_taps();
    for frequency in [0.0, 1000.0, 8000.0, 16000.0, 20000.0] {
        let gain = magnitude(taps, frequency);
        assert!(
            (gain - 1.0).abs() < 0.002,
            "frequency={frequency}, gain={gain}"
        );
    }
}

#[test]
fn converted_fir_respects_coefficient_and_expanded_tap_budgets() {
    for limits in [
        SofaLoadLimits {
            max_total_coefficients: 1200,
            ..SofaLoadLimits::default()
        },
        SofaLoadLimits {
            max_fir_samples: 300,
            max_delay_samples: 0,
            ..SofaLoadLimits::default()
        },
    ] {
        assert!(matches!(
            resample_loaded_hrir_bank(impulse_bank(24_000, [0, 0]), 48_000, limits),
            Err(SofaError::ResourceLimitExceeded(_))
        ));
    }
}

fn centroid(taps: &[f64]) -> f64 {
    taps.iter()
        .enumerate()
        .map(|(i, x)| i as f64 * x)
        .sum::<f64>()
        / taps.iter().sum::<f64>()
}

#[test]
fn fractional_ratio_preserves_interaural_delay_in_the_complete_taps() {
    let actual = resample_loaded_hrir_bank(
        impulse_bank(44_100, [64, 65]),
        48_000,
        SofaLoadLimits::default(),
    )
    .unwrap();
    let pair = actual.bank.entries()[0].pair();
    let left = centroid(pair.left_taps());
    let right = centroid(pair.right_taps());
    assert!(
        (left - (19.0 + 64.0 * 48_000.0 / 44_100.0)).abs() < 0.002,
        "left centroid={left}"
    );
    assert!(
        (right - left - 48_000.0 / 44_100.0).abs() < 0.002,
        "ITD={}",
        right - left
    );
}

#[test]
fn last_nonzero_input_tap_keeps_the_complete_resampling_tail() {
    for rate in [24_000, 44_100, 96_000] {
        let actual = resample_loaded_hrir_bank(
            impulse_bank(rate, [128, 128]),
            48_000,
            SofaLoadLimits::default(),
        )
        .unwrap();
        let taps = actual.bank.entries()[0].pair().left_taps();
        let gain: f64 = taps.iter().sum();
        assert!(
            (gain - 1.0).abs() < 2e-4,
            "truncated tail: rate={rate}, gain={gain}"
        );
        assert_eq!(actual.metadata.expanded_max_tap_length, taps.len());
    }
}

#[test]
fn downsampling_rejects_energy_above_the_target_nyquist_frequency() {
    let energy = |frequency: f64| {
        let taps: Vec<_> = (0..1024)
            .map(|i| {
                let window = 0.5 * (1.0 - (std::f64::consts::TAU * i as f64 / 1023.0).cos());
                window * (std::f64::consts::TAU * frequency * i as f64 / 96_000.0).cos()
            })
            .collect();
        let result = resample_loaded_hrir_bank(
            bank(96_000, taps.clone(), taps, [0, 0]),
            48_000,
            SofaLoadLimits::default(),
        )
        .unwrap();
        result.bank.entries()[0]
            .pair()
            .left_taps()
            .iter()
            .map(|x| x * x)
            .sum::<f64>()
    };
    let rejection = energy(36_000.0) / energy(6_000.0);
    assert!(rejection < 1e-6, "aliased energy ratio={rejection}");
}
