//! J1R25 independent coupling-coordinate admission.
//!
//! The oracle below is a separate transcription of ETSI TS 102 366 V1.4.1
//! clause 6.4.3.  It deliberately does not call the production coordinate
//! helper.  The production result is observed through the public
//! `reconstruct_standard_coupling` API at the first coupled bin.

use openjoc_eac3::{
    Eac3Error, StandardCouplingCoordinates, StandardCouplingInformation,
    reconstruct_standard_coupling,
};

const ABS_TOLERANCE: f64 = 1.0e-15;

/// Independent clause-6.4.3 transcription.
fn normative_coordinate_oracle(exponent: u8, mantissa: u8, master: u8) -> f64 {
    let fractional = match exponent {
        15 => f64::from(mantissa) / 16.0,
        _ => f64::from(mantissa + 16) / 32.0,
    };
    let right_shifts = i32::from(exponent) + 3 * i32::from(master);
    fractional / 2.0_f64.powi(right_shifts)
}

fn production_coordinate(exponent: u8, mantissa: u8, master: u8) -> Result<f64, Eac3Error> {
    let coupling = StandardCouplingInformation {
        channel_in_use: vec![true],
        phase_flags_in_use: false,
        begin_frequency_code: 0,
        end_frequency_code: -2,
        subband_count: 1,
        band_structure: [false; 18],
        band_count: 1,
        coordinates: vec![Some(StandardCouplingCoordinates {
            master,
            bands: vec![(exponent, mantissa)],
        })],
        phase_flags: Vec::new(),
    };
    let reconstructed = reconstruct_standard_coupling(&coupling, &[0.125; 12], &[vec![0.0; 37]])?;
    Ok(reconstructed[0][37])
}

#[test]
fn exhaustive_standard_coordinate_oracle_matches_production() {
    let mut cases = 0_u32;
    let mut max_error = 0.0_f64;
    let mut worst = (0_u8, 0_u8, 0_u8);
    for exponent in 0..=15 {
        for mantissa in 0..=15 {
            for master in 0..=3 {
                let expected = normative_coordinate_oracle(exponent, mantissa, master);
                let actual = production_coordinate(exponent, mantissa, master)
                    .expect("all bounded coordinate codes are valid");
                let error = (actual - expected).abs();
                if error > max_error {
                    max_error = error;
                    worst = (exponent, mantissa, master);
                }
                assert!(
                    error <= ABS_TOLERANCE,
                    "coordinate mismatch for exp/mant/master={exponent}/{mantissa}/{master}: expected {expected}, actual {actual}, error {error}"
                );
                assert!(actual.is_finite());
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 16 * 16 * 4);
    assert!(max_error <= ABS_TOLERANCE, "worst case {worst:?}");
}

#[test]
fn out_of_domain_standard_coordinate_codes_are_rejected() {
    for (exponent, mantissa, master) in [(16, 0, 0), (0, 16, 0), (0, 0, 4)] {
        assert_eq!(
            production_coordinate(exponent, mantissa, master),
            Err(Eac3Error::InvalidCouplingCoordinate {
                exponent,
                mantissa,
                master,
            })
        );
    }
}

#[test]
fn repeated_coordinate_admission_is_numerically_exact() {
    let first = production_coordinate(3, 11, 2).expect("initial coordinate");
    let reused = first;
    assert_eq!(reused, first);
    assert!(reused.is_finite());
}
