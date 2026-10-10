// Functional Core: parsed sparse SOFA coverage and exact coefficient regressions.
#[path = "support/legacy_interpolation.rs"]
mod legacy_interpolation;
#[path = "support/sparse.rs"]
mod sparse;

use openjoc_render::{CartesianPosition, HrirEar};
use openjoc_sofa::{
    ResolvedHrir, SofaError, SofaLoadLimits, parse_simple_free_field_hrir, resolve_hrir,
    resolve_hrir_for_listener_orientation,
};

fn coefficient_bits(resolved: &ResolvedHrir) -> Vec<u64> {
    resolved
        .pair
        .left_taps()
        .iter()
        .chain(resolved.pair.right_taps())
        .map(|tap| tap.to_bits())
        .collect()
}

#[test]
fn parsed_sparse_caps_reject_the_antipode_and_preserve_covered_coefficients() {
    let mut wrongly_accepted = 0;
    for elevation in [-20.0_f64, 20.0, -1.0e-5, 1.0e-5] {
        let loaded = parse_simple_free_field_hrir(
            &sparse::cap_fixture(elevation, false),
            SofaLoadLimits::default(),
        )
        .unwrap();
        let sign = elevation.signum();
        assert!(
            loaded
                .bank
                .entries()
                .iter()
                .all(|entry| entry.direction()[2] * sign > 0.0)
        );
        for resolver in [resolve_hrir, resolve_hrir_for_listener_orientation] {
            let covered = resolver(&loaded.bank, CartesianPosition::new(0.0, 0.0, sign)).unwrap();
            assert_eq!(covered.neighbor_count, 3);
            assert_eq!(covered.exact_entry, None);
            assert_eq!(covered.pair.sample_rate_hz(), 48_000);
            for ear in [HrirEar::Left, HrirEar::Right] {
                assert_eq!(covered.pair.delay_samples(ear), 0);
            }
            let expected = legacy_interpolation::valid_triangle_coefficient_bits(
                &loaded.bank,
                CartesianPosition::new(0.0, 0.0, sign),
            );
            assert_eq!(coefficient_bits(&covered), expected);
            match resolver(&loaded.bank, CartesianPosition::new(0.0, 0.0, -sign)) {
                Err(SofaError::InterpolationOutsideCoverage(_)) => {}
                Ok(_) => wrongly_accepted += 1,
                Err(error) => panic!("unexpected resolver failure: {error}"),
            }
        }
    }
    assert_eq!(
        wrongly_accepted, 0,
        "a positive spherical cone cannot contain its antipode"
    );
}

#[test]
fn containing_triangles_can_cross_the_targets_tangent_plane() {
    for sign in [-1.0, 1.0] {
        let loaded = parse_simple_free_field_hrir(
            &sparse::spherical_fixture(&[
                [0.0, sign * 60.0, 1.0],
                [120.0, sign * 60.0, 1.0],
                [240.0, sign * -20.0, 1.0],
            ]),
            SofaLoadLimits::default(),
        )
        .unwrap();
        assert!(loaded.bank.entries()[2].direction()[2] * sign < 0.0);
        for resolver in [resolve_hrir, resolve_hrir_for_listener_orientation] {
            let covered = resolver(&loaded.bank, CartesianPosition::new(0.0, 0.0, sign)).unwrap();
            assert_eq!(covered.neighbor_count, 3);
            // Keep the valid wide-triangle coefficients exactly equal to the
            // pre-fix arithmetic over this platform's parsed directions.
            assert_eq!(
                coefficient_bits(&covered),
                legacy_interpolation::valid_triangle_coefficient_bits(
                    &loaded.bank,
                    CartesianPosition::new(0.0, 0.0, sign),
                )
            );
        }
    }
}

#[test]
fn planar_sofa_rejects_polar_and_oblique_directions_despite_roundoff() {
    let loaded =
        parse_simple_free_field_hrir(&sparse::cap_fixture(0.0, false), SofaLoadLimits::default())
            .unwrap();
    assert!(
        loaded
            .bank
            .entries()
            .iter()
            .all(|entry| entry.direction()[2] == 0.0)
    );
    for direction in [
        CartesianPosition::new(0.0, 0.0, -1.0),
        CartesianPosition::new(0.0, 0.0, 1.0),
        CartesianPosition::new(0.1, 0.1, 1.0),
        CartesianPosition::new(-0.1, -0.1, -1.0),
        CartesianPosition::new(0.01, 0.0, 1.0),
        CartesianPosition::new(-0.01, 0.0, -1.0),
    ] {
        for resolver in [resolve_hrir, resolve_hrir_for_listener_orientation] {
            assert!(matches!(
                resolver(&loaded.bank, direction),
                Err(SofaError::InterpolationOutsideCoverage(_))
            ));
        }
    }
}
