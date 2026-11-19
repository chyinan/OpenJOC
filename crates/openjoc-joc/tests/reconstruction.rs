use num_complex::Complex64;
use openjoc_joc::{
    JocBandCount, QuantMode, ReconstructionError, Slope, decode_full_coefficients,
    decode_sparse_coefficients, dequantize, interpolate_matrix, qmf_subband_to_parameter_band,
    reconstruct_basis_qmf,
};

fn assert_close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < f64::EPSILON);
}

#[test]
fn sparse_and_full_differential_paths_follow_distinct_clause_6_6_2_rules() {
    let sparse = decode_sparse_coefficients(5, QuantMode::Coarse96, 1, &[1, 2], &[0, 1, 95])
        .expect("valid sparse values");
    assert_eq!(sparse.len(), 5);
    assert_eq!(sparse[1][0], 50);
    assert_eq!(sparse[2][1], 51);
    assert_eq!(sparse[3][2], 49);
    for (channel, coefficients) in sparse.iter().enumerate() {
        for (band, coefficient) in coefficients.iter().copied().enumerate() {
            if !matches!((channel, band), (1, 0) | (2, 1) | (3, 2)) {
                assert_eq!(coefficient, 48);
            }
        }
    }

    let full = decode_full_coefficients(QuantMode::Coarse96, &[vec![0, 1, 95], vec![95, 2, 1]])
        .expect("valid full values");
    assert_eq!(full, vec![vec![48, 49, 48], vec![47, 49, 50]]);
}

#[test]
fn sparse_bands_have_at_most_one_nonzero_gain() {
    for (mode, offset) in [(QuantMode::Coarse96, 50), (QuantMode::Fine192, 100)] {
        for channels in [5, 7] {
            let zero_symbol = mode.steps() / 2 + mode.steps() - offset;
            // The first selected coefficient wraps to zero. The second stays on
            // that channel. The third switches and starts from offset again.
            let matrix =
                decode_sparse_coefficients(channels, mode, 0, &[0, 1], &[zero_symbol, 1, 0])
                    .unwrap();
            for (band, expected) in [0, 1, 1].into_iter().enumerate() {
                let count = matrix
                    .iter()
                    .filter(|row| dequantize(row[band], mode).unwrap() != 0.0)
                    .count();
                assert_eq!(count, expected);
            }
            assert_eq!(matrix[0][1], mode.steps() / 2 + 1);
            assert_eq!(matrix[1][2], offset);
        }
    }
}

#[test]
fn differential_decoders_reject_malformed_dimensions_and_symbols() {
    assert!(matches!(
        decode_sparse_coefficients(5, QuantMode::Coarse96, 5, &[], &[0]),
        Err(ReconstructionError::InvalidChannel { .. })
    ));
    assert!(matches!(
        decode_sparse_coefficients(5, QuantMode::Coarse96, 0, &[1], &[0]),
        Err(ReconstructionError::DimensionMismatch { .. })
    ));
    assert!(matches!(
        decode_full_coefficients(QuantMode::Coarse96, &[vec![96]]),
        Err(ReconstructionError::QuantizedValueOutOfRange { .. })
    ));
}

#[test]
fn every_96_and_192_step_dequantized_value_matches_pseudocode_5() {
    for mode in [QuantMode::Coarse96, QuantMode::Fine192] {
        let steps = mode.steps();
        let mut previous = f64::NEG_INFINITY;
        for quantized in 0..steps {
            let actual = dequantize(quantized, mode).expect("in-range quantized value");
            let index = if mode == QuantMode::Coarse96 {
                0.0
            } else {
                1.0
            };
            let expected =
                (f64::from(quantized) - f64::from(steps) / 2.0) * 820.0 / (4096.0 * (1.0 + index));
            assert!(actual.is_finite());
            assert_close(actual, expected);
            assert!(actual > previous);
            previous = actual;
        }
        assert_close(dequantize(steps / 2, mode).expect("centre value"), 0.0);
        assert!(dequantize(steps, mode).is_err());
    }
}

fn expected_parameter_band(count: JocBandCount, subband: u8) -> u8 {
    let groups: &[u8] = match count {
        JocBandCount::One => &[64],
        JocBandCount::Three => &[3, 11, 50],
        JocBandCount::Five => &[1, 2, 6, 14, 41],
        JocBandCount::Seven => &[1, 1, 2, 4, 6, 9, 41],
        JocBandCount::Nine => &[1, 1, 1, 2, 2, 2, 5, 9, 41],
        JocBandCount::Twelve => &[1, 1, 1, 1, 2, 2, 3, 3, 4, 5, 12, 29],
        JocBandCount::Fifteen => &[1, 1, 1, 1, 1, 1, 1, 1, 1, 2, 3, 4, 5, 12, 29],
        JocBandCount::TwentyThree => &[
            1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 4, 5, 6, 7, 16,
        ],
    };
    let mut boundary = 0_u8;
    for (parameter_band, width) in groups.iter().copied().enumerate() {
        boundary += width;
        if subband < boundary {
            return u8::try_from(parameter_band).expect("at most 23 bands");
        }
    }
    panic!("subband outside table");
}

#[test]
fn all_512_table_54_mappings_are_exact() {
    for count in JocBandCount::ALL {
        for subband in 0..64 {
            assert_eq!(
                qmf_subband_to_parameter_band(count, subband),
                Ok(expected_parameter_band(count, subband)),
                "{count:?}, subband {subband}"
            );
        }
        assert!(qmf_subband_to_parameter_band(count, 64).is_err());
    }
}

#[test]
fn smooth_and_steep_interpolation_preserve_cross_frame_state() {
    let count = JocBandCount::One;
    let previous = vec![[0.0; 64]];
    let smooth = interpolate_matrix(
        &[vec![vec![8.0]]],
        &previous,
        Slope::Smooth,
        &[None],
        count,
        4,
    )
    .expect("smooth interpolation");
    assert_eq!(
        smooth
            .matrix
            .iter()
            .map(|slot| slot[0][0])
            .collect::<Vec<_>>(),
        [2.0, 4.0, 6.0, 8.0]
    );
    assert_close(smooth.next_previous[0][0], 8.0);

    let two_point = interpolate_matrix(
        &[vec![vec![4.0]], vec![vec![8.0]]],
        &previous,
        Slope::Smooth,
        &[None, None],
        count,
        4,
    )
    .expect("two-point smooth interpolation");
    assert_eq!(
        two_point
            .matrix
            .iter()
            .map(|slot| slot[0][0])
            .collect::<Vec<_>>(),
        [2.0, 4.0, 6.0, 8.0]
    );

    let steep = interpolate_matrix(
        &[vec![vec![4.0]], vec![vec![9.0]]],
        &previous,
        Slope::Steep,
        &[Some(2), Some(4)],
        count,
        5,
    )
    .expect("steep interpolation");
    assert_eq!(
        steep
            .matrix
            .iter()
            .map(|slot| slot[0][63])
            .collect::<Vec<_>>(),
        [0.0, 0.0, 4.0, 4.0, 9.0]
    );
    assert_close(steep.next_previous[0][63], 9.0);
}

#[test]
fn basis_qmf_reconstruction_zeroes_outputs_and_performs_complex_matrix_multiply() {
    let inputs = vec![
        vec![[Complex64::new(1.0, 2.0); 64]],
        vec![[Complex64::new(3.0, -1.0); 64]],
    ];
    let matrices = vec![vec![vec![[2.0; 64]], vec![[-0.5; 64]]]];

    let output = reconstruct_basis_qmf(&inputs, &matrices).expect("matching dimensions");
    assert_eq!(output.len(), 1);
    assert!(
        output[0][0]
            .iter()
            .all(|sample| *sample == Complex64::new(0.5, 4.5))
    );

    let zero = reconstruct_basis_qmf(&inputs, &[vec![vec![[0.0; 64]], vec![[0.0; 64]]]])
        .expect("zero matrix");
    assert!(zero[0][0].iter().all(|sample| *sample == Complex64::ZERO));
}

#[test]
fn basis_qmf_reconstruction_preserves_three_matrix_ordinals_including_zero_middle_row() {
    let inputs = vec![
        vec![[Complex64::new(2.0, 3.0); 64]],
        vec![[Complex64::new(-5.0, 7.0); 64]],
    ];
    let matrices = vec![
        vec![vec![[1.0; 64]], vec![[0.0; 64]]],
        vec![vec![[0.0; 64]], vec![[0.0; 64]]],
        vec![vec![[0.0; 64]], vec![[1.0; 64]]],
    ];

    let output = reconstruct_basis_qmf(&inputs, &matrices).expect("matching dimensions");

    assert_eq!(output.len(), 3);
    assert!(
        output[0][0]
            .iter()
            .all(|sample| *sample == Complex64::new(2.0, 3.0))
    );
    assert!(output[1][0].iter().all(|sample| *sample == Complex64::ZERO));
    assert!(
        output[2][0]
            .iter()
            .all(|sample| *sample == Complex64::new(-5.0, 7.0))
    );
}
