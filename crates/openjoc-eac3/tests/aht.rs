use openjoc_eac3::inverse_aht_dct;

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
