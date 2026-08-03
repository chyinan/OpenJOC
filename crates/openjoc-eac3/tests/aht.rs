use openjoc_eac3::{Eac3Error, decode_aht_gaq_mantissa, expand_aht_gaq_gains, inverse_aht_dct};

#[test]
fn inverse_aht_dct_reconstructs_dc_and_preserves_bin_energy() {
    let dc = inverse_aht_dct(&[[1.0, 0.0, 0.0, 0.0, 0.0, 0.0]]).expect("DC AHT spectrum");
    assert!(dc[0].iter().all(|sample| (*sample - 1.0).abs() < 1.0e-12));

    let first_ac =
        inverse_aht_dct(&[[0.0, 1.0, 0.0, 0.0, 0.0, 0.0]]).expect("first AHT AC spectrum");
    let energy = first_ac[0]
        .iter()
        .map(|sample| sample * sample)
        .sum::<f64>();
    assert!((energy - 6.0).abs() < 1.0e-12);
    let expected = 2.0_f64.sqrt() * (core::f64::consts::PI / 12.0).cos();
    assert!((first_ac[0][0] - expected).abs() < 1.0e-12);
}

#[test]
fn inverse_aht_dct_rejects_nonfinite_input() {
    assert!(inverse_aht_dct(&[[f64::NAN, 0.0, 0.0, 0.0, 0.0, 0.0]]).is_err());
}

#[test]
fn expands_all_normative_aht_gaq_gain_modes() {
    assert_eq!(expand_aht_gaq_gains(0, &[], 3), Ok(vec![1, 1, 1]));
    assert_eq!(expand_aht_gaq_gains(1, &[0, 1, 0], 3), Ok(vec![1, 2, 1]));
    assert_eq!(expand_aht_gaq_gains(2, &[1, 0], 2), Ok(vec![4, 1]));
    assert_eq!(expand_aht_gaq_gains(3, &[26], 2), Ok(vec![4, 4]));
}

#[test]
fn rejects_invalid_aht_gaq_gain_syntax() {
    assert_eq!(
        expand_aht_gaq_gains(4, &[], 0),
        Err(Eac3Error::InvalidAhtGaqMode { actual: 4 })
    );
    assert_eq!(
        expand_aht_gaq_gains(1, &[2], 1),
        Err(Eac3Error::InvalidAhtGaqGainWord { actual: 2 })
    );
}

#[test]
fn decodes_aht_gaq_small_and_large_mantissas() {
    assert_eq!(decode_aht_gaq_mantissa(8, 1, false, 0), Ok(0.0));

    let small = decode_aht_gaq_mantissa(8, 2, false, 1).expect("GAQ small code");
    assert!((small - 0.25).abs() < 1.0e-12);

    let large = decode_aht_gaq_mantissa(8, 2, true, 1).expect("GAQ large code");
    let expected = 0.5 + 0.5 * (1.0 - 0x2aabu16 as f64 / 32768.0);
    assert!((large - expected).abs() < 1.0e-12);
}

#[test]
fn rejects_invalid_aht_gaq_quantizer_inputs() {
    assert_eq!(
        decode_aht_gaq_mantissa(7, 1, false, 0),
        Err(Eac3Error::InvalidAhtGaqHebap { actual: 7 })
    );
    assert_eq!(
        decode_aht_gaq_mantissa(8, 3, false, 0),
        Err(Eac3Error::InvalidAhtGaqGain { actual: 3 })
    );
    assert_eq!(
        decode_aht_gaq_mantissa(8, 1, false, 8),
        Err(Eac3Error::InvalidAhtGaqCode { actual: 8 })
    );
}
