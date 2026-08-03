use openjoc_eac3::{Eac3Error, inverse_transform, overlap_add};

#[test]
fn inverse_transform_rejects_wrong_coefficient_dimensions() {
    assert_eq!(
        inverse_transform(&[0.0; 255], false),
        Err(Eac3Error::InvalidTransformCoefficientLength {
            expected: 256,
            actual: 255,
        })
    );
    assert_eq!(
        inverse_transform(&[0.0; 255], true),
        Err(Eac3Error::InvalidTransformCoefficientLength {
            expected: 256,
            actual: 255,
        })
    );
}

#[test]
fn inverse_transform_rejects_nonfinite_coefficients_at_the_entry_point() {
    let mut coefficients = [0.0; 256];
    coefficients[17] = f64::NAN;
    assert_eq!(
        inverse_transform(&coefficients, false),
        Err(Eac3Error::NonFiniteTransformCoefficient { index: 17 })
    );
}

#[test]
fn inverse_transform_zero_coefficients_are_zero_for_both_block_modes() {
    for block_switch in [false, true] {
        let output = inverse_transform(&[0.0; 256], block_switch).expect("zero transform");
        assert_eq!(output.len(), 512);
        assert!(output.iter().all(|value| *value == 0.0));
    }
}

#[test]
fn inverse_transform_dc_coefficient_matches_the_rendered_etsi_equations() {
    let mut coefficients = [0.0; 256];
    coefficients[0] = 1.0;
    let long = inverse_transform(&coefficients, false).expect("long IMDCT");
    let short = inverse_transform(&coefficients, true).expect("short IMDCT");
    let expected_long = [
        -0.00009869077125262666,
        -0.00016813651054637958,
        -0.0002575855386518413,
        -0.0003527972217657205,
    ];
    let expected_short = [
        -0.00013999736453956417,
        -0.00023995933963099763,
        -0.0003698258844754045,
        -0.0005095296411538492,
    ];
    for (actual, expected) in long[..4].iter().zip(expected_long) {
        assert!((actual - expected).abs() < 1.0e-15);
    }
    for (actual, expected) in short[..4].iter().zip(expected_short) {
        assert!((actual - expected).abs() < 1.0e-15);
    }
}

#[test]
fn overlap_add_emits_one_half_and_retains_the_next_half() {
    let windowed = (0..512).map(f64::from).collect::<Vec<_>>();
    let mut delay = vec![1.0; 256];
    let pcm = overlap_add(&windowed, &mut delay).expect("overlap/add");
    assert_eq!(pcm.len(), 256);
    assert_eq!(pcm[0], 2.0);
    assert_eq!(pcm[255], 512.0);
    assert_eq!(delay[0], 256.0);
    assert_eq!(delay[255], 511.0);
}
