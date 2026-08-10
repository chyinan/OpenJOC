//! J1R26 independent public-syntax SPX admission.
//!
//! The oracle is a separate transcription of ETSI TS 102 366 V1.4.1
//! E.2.6.3/E.2.6.4.1/E.2.6.4.3.  It uses the one-band, zero-noise,
//! no-attenuation boundary so translation and coordinate scaling can be
//! compared without borrowing the production implementation's helpers.

use openjoc_eac3::{
    Eac3Error, SpectralExtensionCoordinates, SpectralExtensionInformation,
    synthesize_spectral_extension,
};

const SPX_BAND_TABLE: [usize; 18] = [
    25, 37, 49, 61, 73, 85, 97, 109, 121, 133, 145, 157, 169, 181, 193, 205, 217, 229,
];
const ABS_TOLERANCE: f64 = 1.0e-12;

fn normative_coordinate_oracle(exponent: u8, mantissa: u8, master: u8) -> f64 {
    let fractional = match exponent {
        15 => f64::from(mantissa) / 4.0,
        _ => f64::from(mantissa + 4) / 8.0,
    };
    let right_shifts = i32::from(exponent) + 3 * i32::from(master);
    fractional / 2.0_f64.powi(right_shifts)
}

fn one_band_info(
    start_copy_frequency_code: u8,
    exponent: u8,
    mantissa: u8,
    master: u8,
) -> (SpectralExtensionInformation, SpectralExtensionCoordinates) {
    let coordinates = SpectralExtensionCoordinates {
        blend: 32,
        master,
        bands: vec![(exponent, mantissa)],
    };
    let information = SpectralExtensionInformation {
        channel_in_use: vec![true],
        start_copy_frequency_code,
        begin_frequency_code: 3,
        begin_subband: 5,
        end_subband: 6,
        band_structure: [false; 17],
        band_count: 1,
        coordinates: vec![Some(coordinates.clone())],
    };
    (information, coordinates)
}

#[test]
fn exhaustive_spx_coordinate_and_translation_oracle_matches_production() {
    let base = (0..85).map(|value| value as f64 + 1.0).collect::<Vec<_>>();
    let mut cases = 0_u32;
    let mut max_error = 0.0_f64;
    for start_copy_frequency_code in 0..=3 {
        let copy_start = SPX_BAND_TABLE[usize::from(start_copy_frequency_code)];
        for exponent in 0..=15 {
            for mantissa in 0..=3 {
                for master in 0..=3 {
                    let (information, coordinates) =
                        one_band_info(start_copy_frequency_code, exponent, mantissa, master);
                    let output = synthesize_spectral_extension(
                        &base,
                        &information,
                        &coordinates,
                        None,
                        &[0.0; 12],
                    )
                    .expect("bounded one-band SPX case");
                    let scale = normative_coordinate_oracle(exponent, mantissa, master) * 32.0;
                    for bin in 0..12 {
                        let expected = base[copy_start + bin] * scale;
                        let error = (output[85 + bin] - expected).abs();
                        max_error = max_error.max(error);
                        assert!(
                            error <= ABS_TOLERANCE,
                            "SPX mismatch start/exp/mant/master={start_copy_frequency_code}/{exponent}/{mantissa}/{master}, bin={bin}: expected {expected}, actual {}, error {error}",
                            output[85 + bin]
                        );
                    }
                    cases += 1;
                }
            }
        }
    }
    assert_eq!(cases, 4 * 16 * 4 * 4);
    assert!(max_error <= ABS_TOLERANCE);
}

#[test]
fn rejects_invalid_spx_coordinate_attenuation_and_noise_inputs() {
    let (mut information, mut coordinates) = one_band_info(0, 0, 0, 0);
    let base = vec![1.0; 85];
    coordinates.bands[0] = (0, 4);
    assert_eq!(
        synthesize_spectral_extension(&base, &information, &coordinates, None, &[0.0; 12]),
        Err(Eac3Error::InvalidSpectralExtensionCoordinate {
            exponent: 0,
            mantissa: 4,
            master: 0,
        })
    );
    coordinates.bands[0] = (0, 0);
    assert_eq!(
        synthesize_spectral_extension(&base, &information, &coordinates, Some(32), &[0.0; 12]),
        Err(Eac3Error::InvalidSpectralExtensionCode {
            begin_code: 32,
            end_code: information.end_subband,
        })
    );
    assert_eq!(
        synthesize_spectral_extension(&base, &information, &coordinates, None, &[0.0; 11]),
        Err(Eac3Error::MissingSpectralExtensionNoise {
            expected: 12,
            actual: 11,
        })
    );
    information.band_count = 2;
    assert!(matches!(
        synthesize_spectral_extension(&base, &information, &coordinates, None, &[0.0; 12]),
        Err(Eac3Error::InvalidSpectralExtensionCoordinateDimensions { .. })
    ));
}

#[test]
fn repeated_spx_synthesis_is_deterministic_and_finite() {
    let (information, coordinates) = one_band_info(0, 1, 2, 3);
    let base = (0..85).map(f64::from).collect::<Vec<_>>();
    let first = synthesize_spectral_extension(&base, &information, &coordinates, None, &[0.25; 12])
        .expect("SPX first pass");
    let second =
        synthesize_spectral_extension(&base, &information, &coordinates, None, &[0.25; 12])
            .expect("SPX second pass");
    assert_eq!(first, second);
    assert!(first.iter().all(|sample| sample.is_finite()));
}
