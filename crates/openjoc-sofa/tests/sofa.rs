// pattern: Mixed (unavoidable)
// Reason: this integration test owns a deterministic in-memory SOFA fixture
// and exercises the pure parser/renderer boundary, with one opt-in exporter
// for the native LAV smoke.

use openjoc_render::{
    BinauralRenderer, BinauralSourceBlock, CartesianPosition, HrirBank, HrirEntry, HrirEntryId,
    HrirPair, SourceId, StaticBinauralSource, UniformPartitionedConfig,
    UniformPartitionedConvolver,
};
use openjoc_sofa::{
    BuiltinHrtf, BuiltinHrtfMetadata, SofaError, SofaLoadLimits, builtin_hrtf_asset_bytes,
    load_builtin_generic_hrir, load_builtin_hrir, load_builtin_hrir_f32,
    load_builtin_hrir_from_asset, load_simple_free_field_hrir, parse_simple_free_field_hrir,
    resolve_hrir, resolve_hrir_f32,
};

#[test]
fn valid_reversed_receivers_map_coordinates_and_delays() {
    let data = fixture("SimpleFreeFieldHRIR", [0.0, 1.0], false);
    let loaded =
        parse_simple_free_field_hrir(&data, SofaLoadLimits::default()).expect("valid fixture");
    assert_eq!(loaded.metadata.measurement_count, 3);
    assert_eq!(loaded.metadata.sample_rate_hz, 48_000);
    assert_eq!(loaded.bank.entries().len(), 3);
    assert_eq!(loaded.bank.entries()[0].direction(), [0.0, 1.0, 0.0]);
    assert_eq!(loaded.bank.entries()[1].direction(), [1.0, 0.0, 0.0]);
    assert_eq!(loaded.bank.entries()[2].direction(), [0.0, 0.0, 1.0]);
    assert_eq!(
        loaded.bank.entries()[0].pair().left_taps(),
        &[0.0, 3.0, 4.0]
    );
    assert_eq!(
        loaded.bank.entries()[0].pair().right_taps(),
        &[1.0, 2.0, 0.0]
    );
    assert_eq!(
        loaded.bank.entries()[1].pair().left_taps(),
        &[0.0, 7.0, 8.0]
    );
}

#[test]
fn per_measurement_equal_delays_are_accepted() {
    let data = fixture("SimpleFreeFieldHRIR", [0.0, 1.0], true);
    let loaded = parse_simple_free_field_hrir(&data, SofaLoadLimits::default())
        .expect("per-measurement delays");
    assert_eq!(
        loaded.bank.entries()[2].pair().right_taps(),
        &[9.0, 10.0, 0.0]
    );
}

#[test]
fn builtin_hrtf_metadata_preserves_the_original_public_struct_shape() {
    let metadata = BuiltinHrtfMetadata {
        id: "legacy-preset",
        display_name: "Legacy preset",
        dataset: "Legacy dataset",
        subject: "Legacy subject",
        source: "https://example.invalid/source",
        doi: None,
        license: "Apache-2.0",
        sample_rate_hz: 48_000,
        measurement_count: 1,
        ir_length: 256,
        notes: "Legacy metadata literal",
    };
    assert_eq!(metadata.id, "legacy-preset");
}

#[test]
fn duplicate_direction_error_reports_original_measurement_indices() {
    let data = duplicate_direction_fixture();
    assert!(matches!(
        parse_simple_free_field_hrir(&data, SofaLoadLimits::default()),
        Err(SofaError::DuplicateDirection {
            first: 0,
            second: 2
        })
    ));
}

#[test]
fn duplicate_directions_are_rejected_before_expanded_tap_limits() {
    let data = fixture_with_duplicate_direction(
        "SimpleFreeFieldHRIR",
        [1_000_000.0, 1_000_000.0],
        false,
        true,
    );
    let limits = SofaLoadLimits {
        max_total_coefficients: 1,
        ..SofaLoadLimits::default()
    };
    assert!(matches!(
        parse_simple_free_field_hrir(&data, limits),
        Err(SofaError::DuplicateDirection {
            first: 0,
            second: 2
        })
    ));
}

#[test]
fn deterministic_load_and_renderer_integration() {
    let data = fixture("SimpleFreeFieldHRIR", [0.0, 1.0], false);
    let first = parse_simple_free_field_hrir(&data, SofaLoadLimits::default()).expect("first load");
    let second =
        parse_simple_free_field_hrir(&data, SofaLoadLimits::default()).expect("second load");
    assert_eq!(first, second);
    let bank: HrirBank = first.bank.clone();
    let sources = vec![
        StaticBinauralSource::new(
            SourceId::new(1),
            CartesianPosition::new(0.0, 1.0, 0.0),
            1.0,
            bank.entries()[0].id(),
        )
        .expect("source"),
    ];
    let _direct =
        BinauralRenderer::new(48_000, bank.clone(), sources.clone()).expect("direct integration");
    let config = UniformPartitionedConfig::new(4).expect("partition config");
    let _partitioned = UniformPartitionedConvolver::new(48_000, config, bank, sources)
        .expect("partition integration");
}

#[test]
fn built_in_and_synthetic_custom_hrtf_produce_distinct_finite_pcm() {
    let builtin = load_builtin_generic_hrir().expect("built-in HRTF").bank;
    let custom = parse_simple_free_field_hrir(&dense_binaural_fixture(), SofaLoadLimits::default())
        .expect("synthetic custom HRTF")
        .bank;
    let direction = CartesianPosition::new(0.0, 1.0, 0.0);
    let render = |bank: HrirBank| {
        let entry = bank
            .entries()
            .iter()
            .find(|entry| {
                let vector = entry.direction();
                vector[0] * direction.x + vector[1] * direction.y + vector[2] * direction.z
                    > 1.0 - 1.0e-12
            })
            .expect("front measurement");
        let source = StaticBinauralSource::new(SourceId::new(1), direction, 1.0, entry.id())
            .expect("static source");
        let mut renderer = BinauralRenderer::new(48_000, bank, vec![source]).expect("renderer");
        let samples = [1.0, 0.0, 0.0, 0.0];
        let block = BinauralSourceBlock::new(SourceId::new(1), &samples);
        let mut left = vec![0.0; samples.len()];
        let mut right = vec![0.0; samples.len()];
        renderer
            .render_block(&[block], &mut left, &mut right)
            .expect("finite binaural render");
        (left, right)
    };
    let builtin_pcm = render(builtin);
    let custom_pcm = render(custom);
    assert!(
        builtin_pcm
            .0
            .iter()
            .chain(&builtin_pcm.1)
            .all(|sample| sample.is_finite())
    );
    assert!(
        custom_pcm
            .0
            .iter()
            .chain(&custom_pcm.1)
            .all(|sample| sample.is_finite())
    );
    assert_ne!(builtin_pcm, custom_pcm);
}

#[test]
fn every_builtin_preset_runs_front_impulse_smoke() {
    for preset in BuiltinHrtf::all() {
        let asset = builtin_hrtf_asset_bytes(*preset).expect("packaged HRTF asset");
        assert_eq!(asset.len(), preset.asset_metadata().asset_size_bytes);
        let loaded = load_builtin_hrir_from_asset(*preset, asset).expect("external HRTF asset");
        let direction = CartesianPosition::new(0.0, 1.0, 0.0);
        for sanity_direction in [
            direction,
            CartesianPosition::new(-1.0, 0.0, 0.0),
            CartesianPosition::new(1.0, 0.0, 0.0),
            CartesianPosition::new(0.0, -1.0, 0.0),
            CartesianPosition::new(0.0, 0.0, 1.0),
        ] {
            let pair = resolve_hrir(&loaded.bank, sanity_direction)
                .expect("built-in direction coverage")
                .pair;
            assert!(
                pair.left_taps()
                    .iter()
                    .chain(pair.right_taps())
                    .all(|sample| sample.is_finite())
            );
            assert!(
                pair.left_taps()
                    .iter()
                    .chain(pair.right_taps())
                    .any(|sample| sample.abs() > 0.0)
            );
        }
        let resolved = resolve_hrir(&loaded.bank, direction).expect("front direction");
        let entry = HrirEntry::new(HrirEntryId::new(u64::MAX), direction, resolved.pair)
            .expect("prepared front HRIR");
        let source = StaticBinauralSource::new(SourceId::new(1), direction, 1.0, entry.id())
            .expect("front source");
        let bank = HrirBank::new(loaded.bank.sample_rate_hz(), vec![entry]).expect("front bank");
        let mut renderer = BinauralRenderer::new(48_000, bank, vec![source]).expect("renderer");
        let input = [1.0, 0.0, 0.0, 0.0];
        let mut left = [0.0; 4];
        let mut right = [0.0; 4];
        renderer
            .render_block(
                &[BinauralSourceBlock::new(SourceId::new(1), &input)],
                &mut left,
                &mut right,
            )
            .expect("front impulse render");
        assert!(left.iter().chain(&right).all(|sample| sample.is_finite()));
        assert!(left.iter().chain(&right).any(|sample| sample.abs() > 0.0));
    }
}

#[test]
fn f32_resident_builtin_banks_match_f64_oracle_and_render_pcm() {
    let reference_directions = [
        CartesianPosition::new(0.0, 1.0, 0.0),
        CartesianPosition::new(0.0, -1.0, 0.0),
        CartesianPosition::new(-1.0, 0.0, 0.0),
        CartesianPosition::new(1.0, 0.0, 0.0),
        CartesianPosition::new(0.0, 0.0, 1.0),
        CartesianPosition::new(0.0, 1.0, 1.0),
    ];

    for preset in BuiltinHrtf::all() {
        let f64_oracle = load_builtin_hrir(*preset).expect("canonical f64 oracle");
        let resident_f32 = load_builtin_hrir_f32(*preset).expect("f32-resident built-in");
        assert_eq!(
            resident_f32.bank.sample_rate_hz(),
            f64_oracle.bank.sample_rate_hz()
        );
        assert_eq!(
            resident_f32.bank.direction_count(),
            f64_oracle.bank.entries().len()
        );
        assert_eq!(
            resident_f32.bank.tap_storage_bytes(),
            resident_f32.bank.tap_sample_count() * std::mem::size_of::<f32>()
        );
        assert!(
            resident_f32.bank.tap_storage_bytes()
                < resident_f32.bank.tap_sample_count() * std::mem::size_of::<f64>()
        );

        let mut directions = reference_directions.to_vec();
        directions.push(midpoint_of_nearest_measurements(&f64_oracle.bank, 0));
        directions.push(midpoint_of_nearest_measurements(&f64_oracle.bank, 1));
        let mut max_error = 0.0_f64;
        let mut square_error = 0.0_f64;
        let mut error_count = 0_usize;
        let mut max_ild_error_db = 0.0_f64;
        let mut max_itd_error_samples = 0_i128;
        let mut pcm_max_error = 0.0_f64;
        let mut pcm_square_error = 0.0_f64;
        let mut pcm_error_count = 0_usize;
        let mut oracle_pairs = Vec::with_capacity(directions.len());
        let mut resident_pairs = Vec::with_capacity(directions.len());
        for direction in directions.iter().copied() {
            let expected = resolve_hrir(&f64_oracle.bank, direction).expect("f64 oracle direction");
            let actual = resolve_hrir_f32(&resident_f32.bank, direction).expect("f32 direction");
            assert_eq!(
                actual.pair,
                expected.pair,
                "{} HRIR must be bit-identical",
                preset.id()
            );
            if oracle_pairs.len() >= reference_directions.len() {
                assert_eq!(
                    expected.exact_entry, None,
                    "arbitrary target must use interpolation"
                );
            }
            let expected_left_delay =
                i128::try_from(expected.pair.delay_samples(openjoc_render::HrirEar::Left))
                    .expect("test delay fits i128");
            let expected_right_delay =
                i128::try_from(expected.pair.delay_samples(openjoc_render::HrirEar::Right))
                    .expect("test delay fits i128");
            let actual_left_delay =
                i128::try_from(actual.pair.delay_samples(openjoc_render::HrirEar::Left))
                    .expect("test delay fits i128");
            let actual_right_delay =
                i128::try_from(actual.pair.delay_samples(openjoc_render::HrirEar::Right))
                    .expect("test delay fits i128");
            assert_eq!(actual_left_delay, expected_left_delay);
            assert_eq!(actual_right_delay, expected_right_delay);
            max_itd_error_samples = max_itd_error_samples.max(
                ((expected_left_delay - expected_right_delay)
                    - (actual_left_delay - actual_right_delay))
                    .abs(),
            );
            assert_eq!(actual.neighbor_count, expected.neighbor_count);
            assert_eq!(actual.exact_entry, expected.exact_entry);
            assert_eq!(
                actual.pair.left_taps().len(),
                expected.pair.left_taps().len()
            );
            assert_eq!(
                actual.pair.right_taps().len(),
                expected.pair.right_taps().len()
            );
            for (reference, converted) in expected
                .pair
                .left_taps()
                .iter()
                .chain(expected.pair.right_taps())
                .zip(
                    actual
                        .pair
                        .left_taps()
                        .iter()
                        .chain(actual.pair.right_taps()),
                )
            {
                let error = reference - converted;
                max_error = max_error.max(error.abs());
                square_error += error * error;
                error_count += 1;
            }
            let energy =
                |samples: &[f64]| samples.iter().map(|sample| sample * sample).sum::<f64>();
            let ild_db =
                |left: &[f64], right: &[f64]| 10.0 * (energy(left) / energy(right)).log10();
            let ild_error = (ild_db(expected.pair.left_taps(), expected.pair.right_taps())
                - ild_db(actual.pair.left_taps(), actual.pair.right_taps()))
            .abs();
            max_ild_error_db = max_ild_error_db.max(ild_error);
            assert!(ild_error < 1.0e-10);
            assert!(
                actual
                    .pair
                    .left_taps()
                    .iter()
                    .chain(actual.pair.right_taps())
                    .all(|tap| tap.is_finite())
            );
            oracle_pairs.push((direction, expected.pair));
            resident_pairs.push((direction, actual.pair));
        }
        let rms_error = (square_error / error_count as f64).sqrt();
        assert!(
            max_error == 0.0,
            "{} maximum tap error {max_error:e}",
            preset.id()
        );
        assert!(
            rms_error == 0.0,
            "{} RMS tap error {rms_error:e}",
            preset.id()
        );
        assert_eq!(
            max_itd_error_samples,
            0,
            "{} ITD must be bit-identical",
            preset.id()
        );
        assert_eq!(
            max_ild_error_db,
            0.0,
            "{} ILD must be bit-identical",
            preset.id()
        );

        for object_count in [1, directions.len()] {
            let oracle_pcm = render_fixed_source_layout(&oracle_pairs, object_count);
            let resident_pcm = render_fixed_source_layout(&resident_pairs, object_count);
            assert_eq!(oracle_pcm.len(), resident_pcm.len());
            let (pcm_max, pcm_square_sum) = oracle_pcm.iter().zip(&resident_pcm).fold(
                (0.0_f64, 0.0_f64),
                |(maximum, sum), (reference, actual)| {
                    let error = reference - actual;
                    (maximum.max(error.abs()), sum + error * error)
                },
            );
            let pcm_rms = (pcm_square_sum / oracle_pcm.len() as f64).sqrt();
            pcm_max_error = pcm_max_error.max(pcm_max);
            pcm_square_error += pcm_square_sum;
            pcm_error_count += oracle_pcm.len();
            assert!(pcm_max == 0.0, "{} PCM max error {pcm_max:e}", preset.id());
            assert!(pcm_rms == 0.0, "{} PCM RMS error {pcm_rms:e}", preset.id());
            assert!(resident_pcm.iter().all(|sample| sample.is_finite()));
        }
        let moving_oracle_pcm = render_moving_hrir_trajectory(&oracle_pairs);
        let moving_resident_pcm = render_moving_hrir_trajectory(&resident_pairs);
        assert_eq!(moving_oracle_pcm, moving_resident_pcm);
        assert!(moving_resident_pcm.iter().all(|sample| sample.is_finite()));
        let moving_pcm_max_error = moving_oracle_pcm
            .iter()
            .zip(&moving_resident_pcm)
            .map(|(expected, actual)| (expected - actual).abs())
            .fold(0.0_f64, f64::max);
        let moving_pcm_square_error = moving_oracle_pcm
            .iter()
            .zip(&moving_resident_pcm)
            .map(|(expected, actual)| (expected - actual).powi(2))
            .sum::<f64>();
        let moving_pcm_rms_error =
            (moving_pcm_square_error / moving_oracle_pcm.len() as f64).sqrt();
        assert_eq!(moving_pcm_max_error, 0.0);
        assert_eq!(moving_pcm_rms_error, 0.0);
        let pcm_rms_error = (pcm_square_error / pcm_error_count as f64).sqrt();
        println!(
            "f32_oracle preset={} max_tap_error={max_error:.12e} rms_tap_error={rms_error:.12e} max_pcm_error={pcm_max_error:.12e} rms_pcm_error={pcm_rms_error:.12e} moving_pcm_max_error={moving_pcm_max_error:.12e} moving_pcm_rms_error={moving_pcm_rms_error:.12e} max_itd_error_samples={max_itd_error_samples} max_ild_error_db={max_ild_error_db:.12e}",
            preset.id(),
        );
    }
}

fn midpoint_of_nearest_measurements(bank: &HrirBank, first_index: usize) -> CartesianPosition {
    let first = bank.entries()[first_index].direction();
    let second = bank
        .entries()
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != first_index)
        .max_by(|(_, left), (_, right)| {
            dot3(first, left.direction()).total_cmp(&dot3(first, right.direction()))
        })
        .expect("neighbor measurement")
        .1
        .direction();
    let midpoint = [
        first[0] + second[0],
        first[1] + second[1],
        first[2] + second[2],
    ];
    let length =
        (midpoint[0] * midpoint[0] + midpoint[1] * midpoint[1] + midpoint[2] * midpoint[2]).sqrt();
    CartesianPosition::new(
        midpoint[0] / length,
        midpoint[1] / length,
        midpoint[2] / length,
    )
}

fn dot3(left: [f64; 3], right: [f64; 3]) -> f64 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

fn render_fixed_source_layout(
    resolved: &[(CartesianPosition, HrirPair)],
    object_count: usize,
) -> Vec<f64> {
    let selected = &resolved[..object_count];
    let entries = selected
        .iter()
        .enumerate()
        .map(|(index, (direction, pair))| {
            HrirEntry::new(HrirEntryId::new(index as u64 + 1), *direction, pair.clone())
                .expect("resolved renderer entry")
        })
        .collect::<Vec<_>>();
    let sources = selected
        .iter()
        .enumerate()
        .map(|(index, (direction, _))| {
            StaticBinauralSource::new(
                SourceId::new(index as u64 + 1),
                *direction,
                1.0,
                HrirEntryId::new(index as u64 + 1),
            )
            .expect("resolved renderer source")
        })
        .collect::<Vec<_>>();
    let bank = HrirBank::new(48_000, entries).expect("resolved renderer bank");
    let mut renderer = BinauralRenderer::new(48_000, bank, sources).expect("binaural renderer");
    let mut output = Vec::new();
    for block_index in 0..8 {
        let samples = (0..object_count)
            .map(|source_index| {
                (0..128)
                    .map(|sample_index| {
                        if source_index == block_index % object_count {
                            if sample_index == 0 { 1.0 } else { 0.0 }
                        } else {
                            0.0
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let blocks = samples
            .iter()
            .enumerate()
            .map(|(index, input)| BinauralSourceBlock::new(SourceId::new(index as u64 + 1), input))
            .collect::<Vec<_>>();
        let mut left = vec![0.0; 128];
        let mut right = vec![0.0; 128];
        renderer
            .render_block(&blocks, &mut left, &mut right)
            .expect("render trajectory block");
        output.extend(
            left.into_iter()
                .zip(right)
                .flat_map(|(left, right)| [left, right]),
        );
    }
    output
}

fn render_moving_hrir_trajectory(resolved: &[(CartesianPosition, HrirPair)]) -> Vec<f64> {
    const BLOCK_SAMPLES: usize = 128;
    let sample_count = resolved.len() * BLOCK_SAMPLES;
    let input = (0..sample_count)
        .map(|sample| match sample % BLOCK_SAMPLES {
            0 => 1.0,
            64 => -0.5,
            _ => 0.0,
        })
        .collect::<Vec<_>>();
    let mut output = Vec::with_capacity(sample_count * 2);
    for sample in 0..sample_count {
        let pair = &resolved[sample / BLOCK_SAMPLES].1;
        let left = pair
            .left_taps()
            .iter()
            .enumerate()
            .take(sample + 1)
            .map(|(tap, coefficient)| input[sample - tap] * coefficient)
            .sum::<f64>();
        let right = pair
            .right_taps()
            .iter()
            .enumerate()
            .take(sample + 1)
            .map(|(tap, coefficient)| input[sample - tap] * coefficient)
            .sum::<f64>();
        output.extend([left, right]);
    }
    output
}

#[test]
fn builtin_lateral_acoustic_sanity_preserves_left_right_cues() {
    for preset in BuiltinHrtf::all() {
        let loaded = load_builtin_hrir(*preset).expect("built-in HRTF");
        for (label, direction, left_dominant) in [
            ("left", CartesianPosition::new(-1.0, 0.0, 0.0), true),
            ("right", CartesianPosition::new(1.0, 0.0, 0.0), false),
        ] {
            let pair = resolve_hrir(&loaded.bank, direction)
                .unwrap_or_else(|error| panic!("{} {label}: {error}", preset.id()))
                .pair;
            let left_energy = pair
                .left_taps()
                .iter()
                .map(|sample| sample * sample)
                .sum::<f64>();
            let right_energy = pair
                .right_taps()
                .iter()
                .map(|sample| sample * sample)
                .sum::<f64>();
            let left_peak = pair
                .left_taps()
                .iter()
                .enumerate()
                .max_by(|(_, left), (_, right)| left.abs().total_cmp(&right.abs()))
                .map_or(0, |(index, _)| index);
            let right_peak = pair
                .right_taps()
                .iter()
                .enumerate()
                .max_by(|(_, left), (_, right)| left.abs().total_cmp(&right.abs()))
                .map_or(0, |(index, _)| index);
            assert!(left_energy.is_finite() && right_energy.is_finite());
            assert!(left_energy > 0.0 && right_energy > 0.0);
            if left_dominant {
                assert!(
                    left_energy > right_energy,
                    "{} left energy mapping",
                    preset.id()
                );
            } else {
                assert!(
                    right_energy > left_energy,
                    "{} right energy mapping",
                    preset.id()
                );
            }
            println!(
                "preset={} direction={label} left_energy={left_energy:.6e} right_energy={right_energy:.6e} left_peak={left_peak} right_peak={right_peak}",
                preset.id(),
            );
        }
    }
}

#[test]
fn malformed_and_unsupported_inputs_are_rejected() {
    assert!(matches!(
        parse_simple_free_field_hrir(b"not SOFA", SofaLoadLimits::default()),
        Err(SofaError::UnsupportedContainerOrEncoding)
    ));
    let unsupported = fixture("GeneralFIR", [0.0, 1.0], false);
    assert!(matches!(
        parse_simple_free_field_hrir(&unsupported, SofaLoadLimits::default()),
        Err(SofaError::UnsupportedSofaConvention(_))
    ));
    let fractional = fixture("SimpleFreeFieldHRIR", [0.5, 1.0], false);
    assert!(matches!(
        parse_simple_free_field_hrir(&fractional, SofaLoadLimits::default()),
        Err(SofaError::UnsupportedFractionalSofaDelay { .. })
    ));
    let limited = SofaLoadLimits {
        max_file_bytes: 16,
        ..SofaLoadLimits::default()
    };
    assert!(matches!(
        parse_simple_free_field_hrir(&fixture("SimpleFreeFieldHRIR", [0.0, 1.0], false), limited),
        Err(SofaError::ResourceLimitExceeded("file bytes"))
    ));
}

#[test]
fn hdf5_signature_is_rejected_without_native_dependencies() {
    let hdf5 = [0x89, b'H', b'D', b'F', 0x0d, 0x0a, 0x1a, 0x0a];
    assert!(matches!(
        parse_simple_free_field_hrir(&hdf5, SofaLoadLimits::default()),
        Err(SofaError::UnsupportedContainerOrEncoding)
    ));
}

#[test]
fn file_path_loader_uses_local_file_only() {
    let path = std::env::temp_dir().join(format!("openjoc-sofa-test-{}.sofa", std::process::id()));
    std::fs::write(&path, fixture("SimpleFreeFieldHRIR", [0.0, 1.0], false))
        .expect("write fixture");
    let loaded = load_simple_free_field_hrir(&path, SofaLoadLimits::default()).expect("path load");
    std::fs::remove_file(&path).expect("remove fixture");
    assert_eq!(loaded.metadata.convention_version, "1.2");
}

#[test]
fn interpolation_preserves_exact_identity_and_aligns_delays() {
    let bank = synthetic_bank(vec![
        (
            CartesianPosition::new(0.0, 1.0, 0.0),
            HrirPair::new_with_delays(
                48_000,
                vec![0.0, 1.0, 2.0, 0.0, 0.0],
                vec![0.0, 2.0, 4.0, 0.0, 0.0],
                [1, 1],
            )
            .unwrap(),
        ),
        (
            CartesianPosition::new(1.0, 0.0, 0.0),
            HrirPair::new_with_delays(
                48_000,
                vec![0.0, 0.0, 0.0, 3.0, 4.0],
                vec![0.0, 0.0, 0.0, 6.0, 8.0],
                [3, 3],
            )
            .unwrap(),
        ),
    ]);
    let exact = resolve_hrir(&bank, CartesianPosition::new(0.0, 1.0, 0.0)).unwrap();
    assert_eq!(exact.exact_entry, Some(HrirEntryId::new(1)));
    assert_eq!(exact.pair, bank.entries()[0].pair().clone());

    let midpoint = resolve_hrir(&bank, CartesianPosition::new(1.0, 1.0, 0.0)).unwrap();
    assert_eq!(midpoint.exact_entry, None);
    assert_eq!(midpoint.neighbor_count, 2);
    assert_eq!(
        midpoint.pair.delay_samples(openjoc_render::HrirEar::Left),
        2
    );
    assert_eq!(midpoint.pair.left_taps(), &[0.0, 0.0, 2.0, 3.0, 0.0, 0.0]);
    assert_eq!(midpoint.pair.right_taps(), &[0.0, 0.0, 4.0, 6.0, 0.0, 0.0]);
}

#[test]
fn interpolation_handles_azimuth_wrap_and_fails_closed_for_sparse_or_outside_data() {
    let wrap_bank = synthetic_bank(vec![
        (
            azimuth(179.0),
            HrirPair::new(48_000, vec![1.0, 0.0], vec![1.0, 0.0]).unwrap(),
        ),
        (
            azimuth(-179.0),
            HrirPair::new(48_000, vec![3.0, 0.0], vec![3.0, 0.0]).unwrap(),
        ),
    ]);
    let wrapped = resolve_hrir(&wrap_bank, CartesianPosition::new(0.0, -1.0, 0.0)).unwrap();
    assert_eq!(wrapped.pair.left_taps()[0], 2.0);

    let sparse = synthetic_bank(vec![(
        CartesianPosition::new(0.0, 1.0, 0.0),
        HrirPair::new(48_000, vec![1.0], vec![1.0]).unwrap(),
    )]);
    assert!(matches!(
        resolve_hrir(&sparse, CartesianPosition::new(1.0, 0.0, 0.0)),
        Err(SofaError::InsufficientInterpolationData { .. })
    ));
    assert!(matches!(
        resolve_hrir(&wrap_bank, CartesianPosition::new(0.0, 1.0, 0.0)),
        Err(SofaError::InterpolationOutsideCoverage(_))
    ));
}

#[test]
fn internal_coordinate_model_resolves_cardinal_directions() {
    let directions = [
        CartesianPosition::new(0.0, 1.0, 0.0),  // front
        CartesianPosition::new(-1.0, 0.0, 0.0), // left
        CartesianPosition::new(1.0, 0.0, 0.0),  // right
        CartesianPosition::new(0.0, -1.0, 0.0), // rear
        CartesianPosition::new(0.0, 0.0, 1.0),  // top
        CartesianPosition::new(0.0, 0.0, -1.0), // bottom
    ];
    let bank = synthetic_bank(
        directions
            .into_iter()
            .map(|direction| {
                (
                    direction,
                    HrirPair::new(48_000, vec![1.0], vec![1.0]).unwrap(),
                )
            })
            .collect(),
    );
    for direction in directions {
        assert!(
            resolve_hrir(&bank, direction)
                .unwrap()
                .exact_entry
                .is_some()
        );
    }
}

fn synthetic_bank(entries: Vec<(CartesianPosition, HrirPair)>) -> HrirBank {
    HrirBank::new(
        48_000,
        entries
            .into_iter()
            .enumerate()
            .map(|(index, (direction, pair))| {
                HrirEntry::new(HrirEntryId::new(index as u64 + 1), direction, pair).unwrap()
            })
            .collect(),
    )
    .unwrap()
}

fn azimuth(degrees: f64) -> CartesianPosition {
    let radians = degrees.to_radians();
    CartesianPosition::new(radians.sin(), radians.cos(), 0.0)
}

fn fixture(convention: &str, delays: [f64; 2], per_measurement_delay: bool) -> Vec<u8> {
    fixture_with_duplicate_direction(convention, delays, per_measurement_delay, false)
}

fn duplicate_direction_fixture() -> Vec<u8> {
    fixture_with_duplicate_direction("SimpleFreeFieldHRIR", [0.0, 1.0], false, true)
}

fn fixture_with_duplicate_direction(
    convention: &str,
    delays: [f64; 2],
    per_measurement_delay: bool,
    duplicate_direction: bool,
) -> Vec<u8> {
    let dimensions = vec![("M", 3usize), ("R", 2), ("N", 2), ("C", 3), ("One", 1)];
    let dim_id = |name: &str| {
        dimensions
            .iter()
            .position(|(candidate, _)| *candidate == name)
            .expect("dimension")
    };
    let listener_position = [10.0, 0.0, 0.0];
    let mut source = [
        (1.0_f64.atan2(10.0).to_degrees(), 0.0, 101.0_f64.sqrt()),
        (0.0, 0.0, 11.0),
        (0.0, 1.0_f64.atan2(10.0).to_degrees(), 101.0_f64.sqrt()),
    ];
    if duplicate_direction {
        source[2] = source[0];
    }
    let receiver = [10.1, 0.0, 0.0, 9.9, 0.0, 0.0];
    let delay_values = if per_measurement_delay {
        vec![
            delays[0], delays[1], delays[0], delays[1], delays[0], delays[1],
        ]
    } else {
        delays.to_vec()
    };
    let mut variables = vec![
        Var::new(
            "Data.IR",
            vec![dim_id("M"), dim_id("R"), dim_id("N")],
            &doubles(&[
                1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0,
            ]),
            vec![],
        ),
        Var::new(
            "Data.SamplingRate",
            vec![dim_id("One")],
            &doubles(&[48_000.0]),
            vec![text_attr("Units", "hertz")],
        ),
        Var::new(
            "Data.Delay",
            if per_measurement_delay {
                vec![dim_id("M"), dim_id("R")]
            } else {
                vec![dim_id("R")]
            },
            &delay_values,
            vec![text_attr("Units", "samples")],
        ),
        Var::new(
            "SourcePosition",
            vec![dim_id("M"), dim_id("C")],
            &doubles(
                &source
                    .iter()
                    .flat_map(|(a, e, d)| [*a, *e, *d])
                    .collect::<Vec<_>>(),
            ),
            vec![
                text_attr("Type", "spherical"),
                text_attr("Units", "degree, degree, metre"),
            ],
        ),
        Var::new(
            "ListenerPosition",
            vec![dim_id("C")],
            &doubles(&listener_position),
            vec![text_attr("Type", "cartesian"), text_attr("Units", "metre")],
        ),
        Var::new(
            "ListenerView",
            vec![dim_id("C")],
            &doubles(&[0.0, 1.0, 0.0]),
            vec![text_attr("Type", "cartesian"), text_attr("Units", "metre")],
        ),
        Var::new(
            "ListenerUp",
            vec![dim_id("C")],
            &doubles(&[0.0, 0.0, 1.0]),
            vec![text_attr("Type", "cartesian"), text_attr("Units", "metre")],
        ),
        Var::new(
            "ReceiverPosition",
            vec![dim_id("R"), dim_id("C")],
            &doubles(&receiver),
            vec![text_attr("Type", "cartesian"), text_attr("Units", "metre")],
        ),
        Var::new(
            "EmitterPosition",
            vec![dim_id("C")],
            &doubles(&[0.0, 0.0, 0.0]),
            vec![text_attr("Type", "cartesian"), text_attr("Units", "metre")],
        ),
    ];
    let globals = vec![
        text_attr("Conventions", "SOFA"),
        text_attr("SOFAConventions", convention),
        text_attr("SOFAConventionsVersion", "1.2"),
        text_attr("DataType", "FIR"),
        text_attr("RoomType", "free field"),
        text_attr("Title", "OpenJOC synthetic fixture"),
        text_attr("License", "Apache-2.0 project-owned synthetic data"),
    ];
    cdf1(&dimensions, &globals, &mut variables)
}

fn dense_binaural_fixture() -> Vec<u8> {
    let dimensions = vec![("M", 15usize), ("R", 2), ("N", 2), ("C", 3)];
    let dim_id = |name: &str| {
        dimensions
            .iter()
            .position(|(candidate, _)| *candidate == name)
            .expect("dimension")
    };
    let directions = [
        [-1.0, 1.0, 0.0],
        [1.0, 1.0, 0.0],
        [0.0, 1.0, 0.0],
        [-1.0, -1.0, 0.0],
        [1.0, -1.0, 0.0],
        [-1.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [-1.0, 0.67767333984375, 0.0],
        [1.0, 0.67767333984375, 0.0],
        [-1.0, 1.0, 1.0],
        [1.0, 1.0, 1.0],
        [-1.0, 0.0, 1.0],
        [1.0, 0.0, 1.0],
        [-1.0, -1.0, 1.0],
        [1.0, -1.0, 1.0],
    ];
    let source_values = directions
        .iter()
        .flat_map(|direction| {
            let length = direction
                .iter()
                .map(|value| value * value)
                .sum::<f64>()
                .sqrt();
            let x = direction[0] / length;
            let y = direction[1] / length;
            let z = direction[2] / length;
            [
                y.atan2(x).to_degrees(),
                z.atan2((x * x + y * y).sqrt()).to_degrees(),
                1.0,
            ]
        })
        .collect::<Vec<_>>();
    let ir_values = (0..directions.len())
        .flat_map(|index| [1.0 + index as f64, 0.0, 2.0 + index as f64, 0.0])
        .collect::<Vec<_>>();
    let mut variables = vec![
        Var::new(
            "Data.IR",
            vec![dim_id("M"), dim_id("R"), dim_id("N")],
            &ir_values,
            vec![],
        ),
        Var::new(
            "Data.SamplingRate",
            vec![dim_id("R")],
            &[48_000.0, 48_000.0],
            vec![text_attr("Units", "hertz")],
        ),
        Var::new(
            "Data.Delay",
            vec![dim_id("R")],
            &[0.0, 0.0],
            vec![text_attr("Units", "samples")],
        ),
        Var::new(
            "SourcePosition",
            vec![dim_id("M"), dim_id("C")],
            &source_values,
            vec![
                text_attr("Type", "spherical"),
                text_attr("Units", "degree, degree, metre"),
            ],
        ),
        Var::new(
            "ListenerPosition",
            vec![dim_id("C")],
            &[0.0, 0.0, 0.0],
            vec![text_attr("Type", "cartesian"), text_attr("Units", "metre")],
        ),
        Var::new(
            "ListenerView",
            vec![dim_id("C")],
            &[0.0, 1.0, 0.0],
            vec![text_attr("Type", "cartesian"), text_attr("Units", "metre")],
        ),
        Var::new(
            "ListenerUp",
            vec![dim_id("C")],
            &[0.0, 0.0, 1.0],
            vec![text_attr("Type", "cartesian"), text_attr("Units", "metre")],
        ),
        Var::new(
            "ReceiverPosition",
            vec![dim_id("R"), dim_id("C")],
            &[-0.1, 0.0, 0.0, 0.1, 0.0, 0.0],
            vec![text_attr("Type", "cartesian"), text_attr("Units", "metre")],
        ),
        Var::new(
            "EmitterPosition",
            vec![dim_id("C")],
            &[0.0, 0.0, 0.0],
            vec![text_attr("Type", "cartesian"), text_attr("Units", "metre")],
        ),
    ];
    let globals = vec![
        text_attr("Conventions", "SOFA"),
        text_attr("SOFAConventions", "SimpleFreeFieldHRIR"),
        text_attr("SOFAConventionsVersion", "1.2"),
        text_attr("DataType", "FIR"),
        text_attr("RoomType", "free field"),
        text_attr("Title", "OpenJOC synthetic binaural fixture"),
        text_attr("License", "Apache-2.0 project-owned synthetic data"),
    ];
    cdf1(&dimensions, &globals, &mut variables)
}

#[test]
#[ignore = "native LAV smoke fixture export"]
fn export_dense_binaural_fixture_for_native_smoke() {
    let output = std::env::var_os("OPENJOC_TEST_SOFA_OUTPUT").expect("output path");
    std::fs::write(output, dense_binaural_fixture()).expect("write synthetic SOFA");
}

#[derive(Clone)]
struct Attr {
    name: String,
    value: String,
}
fn text_attr(name: &str, value: &str) -> Attr {
    Attr {
        name: name.to_string(),
        value: value.to_string(),
    }
}
struct Var {
    name: &'static str,
    dims: Vec<usize>,
    data: Vec<u8>,
    attrs: Vec<Attr>,
}
impl Var {
    fn new(name: &'static str, dims: Vec<usize>, values: &[f64], attrs: Vec<Attr>) -> Self {
        Self {
            name,
            dims,
            data: values
                .iter()
                .flat_map(|value| value.to_be_bytes())
                .collect(),
            attrs,
        }
    }
}
fn doubles(values: &[f64]) -> Vec<f64> {
    values.to_vec()
}

fn cdf1(dimensions: &[(&str, usize)], globals: &[Attr], variables: &mut [Var]) -> Vec<u8> {
    let mut header = Vec::new();
    header.extend_from_slice(b"CDF\x01");
    put_u32(&mut header, 0);
    put_u32(&mut header, 10);
    put_u32(&mut header, dimensions.len() as u32);
    for (name, length) in dimensions {
        put_string(&mut header, name);
        put_u32(&mut header, *length as u32);
    }
    put_attrs(&mut header, globals);
    put_u32(&mut header, 11);
    put_u32(&mut header, variables.len() as u32);
    let mut begin_positions = Vec::new();
    for variable in variables.iter() {
        put_string(&mut header, variable.name);
        put_u32(&mut header, variable.dims.len() as u32);
        for dim in &variable.dims {
            put_u32(&mut header, *dim as u32);
        }
        put_attrs(&mut header, &variable.attrs);
        put_u32(&mut header, 6);
        let padded = (variable.data.len() + 3) & !3;
        put_u32(&mut header, padded as u32);
        begin_positions.push(header.len());
        put_u32(&mut header, 0);
    }
    let data_start = (header.len() + 3) & !3;
    header.resize(data_start, 0);
    let mut cursor = data_start;
    for (index, variable) in variables.iter().enumerate() {
        let begin = cursor as u32;
        header[begin_positions[index]..begin_positions[index] + 4]
            .copy_from_slice(&begin.to_be_bytes());
        header.extend_from_slice(&variable.data);
        while header.len() % 4 != 0 {
            header.push(0);
        }
        cursor = header.len();
    }
    header
}
fn put_u32(target: &mut Vec<u8>, value: u32) {
    target.extend_from_slice(&value.to_be_bytes());
}
fn put_string(target: &mut Vec<u8>, value: &str) {
    put_u32(target, value.len() as u32);
    target.extend_from_slice(value.as_bytes());
    while target.len() % 4 != 0 {
        target.push(0);
    }
}
fn put_attrs(target: &mut Vec<u8>, attrs: &[Attr]) {
    if attrs.is_empty() {
        put_u32(target, 0);
        return;
    }
    put_u32(target, 12);
    put_u32(target, attrs.len() as u32);
    for attr in attrs {
        put_string(target, &attr.name);
        put_u32(target, 2);
        put_u32(target, attr.value.len() as u32);
        target.extend_from_slice(attr.value.as_bytes());
        while target.len() % 4 != 0 {
            target.push(0);
        }
    }
}
