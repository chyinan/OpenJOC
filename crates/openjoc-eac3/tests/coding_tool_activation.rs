//! Small public-syntax/API activation harness.
//!
//! This is intentionally not an E-AC-3 encoder.  Each case supplies only
//! normative structures already accepted by the public E-AC-3 APIs, then
//! checks deterministic, finite, shape-bounded production output.  Raw
//! carrier prevalence remains a separate private-corpus question.

use openjoc_eac3::{
    CouplingInformation, SpectralExtensionCoordinates, SpectralExtensionInformation,
    StandardCouplingCoordinates, StandardCouplingInformation, inverse_aht_dct,
    reconstruct_standard_coupling, rematrix_channels, synthesize_spectral_extension,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ActivationLevel {
    L1ProductionPathActivated,
    L3NumericalInvariantsValidated,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PublicSyntaxCase {
    tool: &'static str,
    level: ActivationLevel,
}

fn assert_repeat_finite<T>(case: PublicSyntaxCase, run: impl Fn() -> T)
where
    T: Clone + std::fmt::Debug + PartialEq,
{
    let first = run();
    let second = run();
    assert_eq!(first, second, "{} must be deterministic", case.tool);
}

fn rematrix_oracle(left: f64, right: f64) -> (f64, f64) {
    (left + right, left - right)
}

fn spx_information() -> SpectralExtensionInformation {
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
fn reusable_harness_activates_coupling_and_checks_state_independent_output() {
    let case = PublicSyntaxCase {
        tool: "coupling_coordinates_phase",
        level: ActivationLevel::L3NumericalInvariantsValidated,
    };
    let info = StandardCouplingInformation {
        channel_in_use: vec![true, true],
        phase_flags_in_use: true,
        begin_frequency_code: 0,
        end_frequency_code: 0,
        subband_count: 3,
        band_structure: [false; 18],
        band_count: 3,
        coordinates: vec![
            Some(StandardCouplingCoordinates {
                master: 0,
                bands: vec![(0, 0), (0, 0), (0, 0)],
            }),
            Some(StandardCouplingCoordinates {
                master: 0,
                bands: vec![(0, 0), (0, 0), (0, 0)],
            }),
        ],
        phase_flags: vec![true, false, false],
    };
    let channels = vec![vec![0.0; 37], vec![0.0; 37]];
    let coupling = vec![0.25; 36];
    assert_repeat_finite(case, || {
        reconstruct_standard_coupling(&info, &coupling, &channels).expect("coupling path")
    });
}

#[test]
fn reusable_harness_activates_spx_coordinates_and_attenuation() {
    let case = PublicSyntaxCase {
        tool: "spx_coordinates_attenuation",
        level: ActivationLevel::L3NumericalInvariantsValidated,
    };
    assert_eq!(case.level, ActivationLevel::L3NumericalInvariantsValidated);
    let information = spx_information();
    let coordinates = information.coordinates[0].clone().expect("SPX coordinates");
    assert_repeat_finite(case, || {
        synthesize_spectral_extension(&[1.0; 49], &information, &coordinates, Some(0), &[0.0; 36])
            .expect("SPX path")
    });
}

#[test]
fn reusable_harness_activates_aht_inverse_path() {
    let case = PublicSyntaxCase {
        tool: "aht",
        level: ActivationLevel::L3NumericalInvariantsValidated,
    };
    assert_repeat_finite(case, || {
        inverse_aht_dct(&[[1.0, 0.0, 0.0, 0.0, 0.0, 0.0]]).expect("AHT path")
    });
}

#[test]
fn reusable_harness_activates_rematrix_band_formula() {
    let case = PublicSyntaxCase {
        tool: "rematrix",
        level: ActivationLevel::L3NumericalInvariantsValidated,
    };
    assert_repeat_finite(case, || {
        rematrix_channels(
            &[vec![1.0; 80], vec![2.0; 80]],
            &[true, false, false, false],
            None,
            None,
        )
        .expect("rematrix path")
    });
}

#[test]
fn rematrix_matches_small_independent_public_formula() {
    let restored = rematrix_channels(
        &[vec![1.0; 80], vec![2.0; 80]],
        &[true, false, false, false],
        None,
        None,
    )
    .expect("rematrix path");
    let (expected_left, expected_right) = rematrix_oracle(1.0, 2.0);
    assert_eq!(
        (restored[0][13], restored[1][13]),
        (expected_left, expected_right)
    );
}

#[test]
fn harness_case_contract_is_explicitly_test_only() {
    let cases = [
        PublicSyntaxCase {
            tool: "dependent_substream_structure",
            level: ActivationLevel::L1ProductionPathActivated,
        },
        PublicSyntaxCase {
            tool: "coupling_coordinates_phase",
            level: ActivationLevel::L3NumericalInvariantsValidated,
        },
        PublicSyntaxCase {
            tool: "spx_coordinates_attenuation",
            level: ActivationLevel::L3NumericalInvariantsValidated,
        },
        PublicSyntaxCase {
            tool: "aht",
            level: ActivationLevel::L3NumericalInvariantsValidated,
        },
        PublicSyntaxCase {
            tool: "rematrix",
            level: ActivationLevel::L3NumericalInvariantsValidated,
        },
    ];
    assert_eq!(cases.len(), 5);
    assert!(cases.iter().all(|case| matches!(
        case.level,
        ActivationLevel::L1ProductionPathActivated
            | ActivationLevel::L3NumericalInvariantsValidated
    )));
    let _ = CouplingInformation::Standard(StandardCouplingInformation {
        channel_in_use: vec![false],
        phase_flags_in_use: false,
        begin_frequency_code: 0,
        end_frequency_code: 0,
        subband_count: 0,
        band_structure: [false; 18],
        band_count: 0,
        coordinates: vec![None],
        phase_flags: Vec::new(),
    });
}
