use openjoc_eac3::{
    SpectralExtensionCoordinates, SpectralExtensionInformation, synthesize_spectral_extension,
};

fn info() -> SpectralExtensionInformation {
    SpectralExtensionInformation {
        channel_in_use: vec![true],
        start_copy_frequency_code: 0,
        begin_frequency_code: 0,
        begin_subband: 2,
        end_subband: 5,
        band_structure: [
            false, false, false, false, false, false, false, false, true, false, true, false, true,
            false, true, false, true,
        ],
        band_count: 3,
        coordinates: vec![Some(SpectralExtensionCoordinates {
            blend: 32,
            master: 0,
            bands: vec![(0, 3), (0, 3), (0, 3)],
        })],
    }
}

#[test]
fn translates_banded_coefficients_from_the_table_indexed_copy_region() {
    let base = (0..49).map(f64::from).collect::<Vec<_>>();
    let output = synthesize_spectral_extension(
        &base,
        &info(),
        &info().coordinates[0].clone().unwrap(),
        None,
        &[0.0; 36],
    )
    .expect("SPX synthesis");

    assert_eq!(output.len(), 85);
    assert_eq!(
        &output[49..61],
        &(25..37)
            .map(|value| f64::from(value) * 28.0)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        &output[61..73],
        &(37..49)
            .map(|value| f64::from(value) * 28.0)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        &output[73..85],
        &(25..37)
            .map(|value| f64::from(value) * 28.0)
            .collect::<Vec<_>>()
    );
}

#[test]
fn indexes_all_four_start_copy_codes_in_table_e211_order() {
    let table = [25_usize, 37, 49, 61];
    for (code, expected_start) in table.into_iter().enumerate() {
        let mut information = info();
        information.start_copy_frequency_code = u8::try_from(code).expect("two-bit code");
        information.begin_subband = 5;
        information.end_subband = 6;
        information.band_count = 1;
        information.coordinates[0] = Some(SpectralExtensionCoordinates {
            blend: 32,
            master: 0,
            bands: vec![(5, 3)],
        });
        let base = (0..85).map(f64::from).collect::<Vec<_>>();
        let output = synthesize_spectral_extension(
            &base,
            &information,
            &information.coordinates[0].clone().unwrap(),
            None,
            &[0.0; 12],
        )
        .expect("SPX synthesis");
        assert_eq!(output.len(), 97);
        assert_eq!(output[85], expected_start as f64 * 0.875);
    }
}

#[test]
fn applies_coordinate_scaling_and_noise_blending_per_band() {
    let mut coordinates = info().coordinates[0].clone().unwrap();
    coordinates.blend = 0;
    coordinates.bands = vec![(0, 3), (0, 3), (0, 3)];
    let base = vec![1.0; 49];
    let noise = vec![2.0; 36];
    let output = synthesize_spectral_extension(&base, &info(), &coordinates, None, &noise)
        .expect("SPX synthesis");

    // The first band's midpoint is 55, giving nratio=55.0/85.0.
    let ratio: f64 = 55.0 / 85.0;
    let expected = ((1.0 - ratio).sqrt() + 2.0 * ratio.sqrt()) * 28.0;
    assert!((output[49] - expected).abs() < 1e-12);
}

#[test]
fn applies_the_symmetric_five_tap_attenuation_notch() {
    let base = vec![1.0; 49];
    let mut information = info();
    information.coordinates[0] = Some(SpectralExtensionCoordinates {
        blend: 32,
        master: 0,
        bands: vec![(5, 3), (5, 3), (5, 3)],
    });
    let output = synthesize_spectral_extension(
        &base,
        &information,
        &information.coordinates[0].clone().unwrap(),
        Some(0),
        &[0.0; 36],
    )
    .expect("SPX synthesis");

    let attenuation = [
        0.954841604,
        0.911722489,
        0.870550563,
        0.911722489,
        0.954841604,
    ];
    assert!((output[47] - attenuation[0]).abs() < 1e-12);
    assert!((output[48] - attenuation[1]).abs() < 1e-12);
    assert!((output[49] - attenuation[2] * 0.875).abs() < 1e-12);
    assert!((output[50] - attenuation[1] * 0.875).abs() < 1e-12);
    assert!((output[51] - attenuation[0] * 0.875).abs() < 1e-12);
}
