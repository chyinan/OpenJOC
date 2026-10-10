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

fn rotate(direction: [f64; 3], rotation: usize) -> [f64; 3] {
    match rotation {
        0 => direction,
        1 => [direction[2], direction[0], direction[1]],
        2 => [direction[0], -direction[2], direction[1]],
        3 => {
            let axis = [1.0 / 3.0, 2.0 / 3.0, 2.0 / 3.0];
            let projection = dot(direction, axis);
            std::array::from_fn(|index| 2.0 * axis[index] * projection - direction[index])
        }
        _ => {
            let (x, y, z) = (0.37_f64, -0.61_f64, 0.89_f64);
            let first = [
                direction[0],
                direction[1] * x.cos() - direction[2] * x.sin(),
                direction[1] * x.sin() + direction[2] * x.cos(),
            ];
            let second = [
                first[0] * y.cos() + first[2] * y.sin(),
                first[1],
                -first[0] * y.sin() + first[2] * y.cos(),
            ];
            [
                second[0] * z.cos() - second[1] * z.sin(),
                second[0] * z.sin() + second[1] * z.cos(),
                second[2],
            ]
        }
    }
}

fn dot(first: [f64; 3], second: [f64; 3]) -> f64 {
    first.iter().zip(second).map(|(a, b)| a * b).sum()
}

fn unit(direction: [f64; 3]) -> [f64; 3] {
    let magnitude = dot(direction, direction).sqrt();
    direction.map(|axis| axis / magnitude)
}

fn position(direction: [f64; 3], scale: f64) -> CartesianPosition {
    CartesianPosition::new(
        direction[0] * scale,
        direction[1] * scale,
        direction[2] * scale,
    )
}

fn rotated_cap_fixture(elevation: f64, rotation: usize, scale: f64) -> Vec<u8> {
    let elevation = elevation.to_radians();
    let sources = [0.0_f64, 120.0, 240.0]
        .into_iter()
        .enumerate()
        .map(|(index, azimuth)| {
            let azimuth = azimuth.to_radians();
            let direction = rotate(
                [
                    -elevation.cos() * azimuth.sin(),
                    elevation.cos() * azimuth.cos(),
                    elevation.sin(),
                ],
                rotation,
            );
            // Convert from renderer listener coordinates to the canonical
            // SOFA frame; each source also has an independent positive radius.
            let world = [direction[1], -direction[0], direction[2]];
            let radius = dot(world, world).sqrt();
            [
                world[1].atan2(world[0]).to_degrees(),
                (world[2] / radius).asin().to_degrees(),
                radius * scale * [1.0, 1.25, 0.75][index],
            ]
        })
        .collect::<Vec<_>>();
    sparse::spherical_fixture(&sources)
}

#[test]
fn parsed_rotated_shallow_cones_preserve_positive_sum_coefficients() {
    // The +/-1e-6-degree half-turn about [1,2,2] reproduced a valid cone
    // rejected by the relative-only residual check. Use the parsed directions
    // themselves to define a positive-cone target, including platform-specific
    // spherical conversion, rather than assuming exact ideal geometry.
    for elevation in [-1.0e-5, 1.0e-5, -1.0e-6, 1.0e-6, -1.0e-7, 1.0e-7] {
        for rotation in 0..5 {
            for source_scale in [0.125, 1.0, 4096.0] {
                let loaded = parse_simple_free_field_hrir(
                    &rotated_cap_fixture(elevation, rotation, source_scale),
                    SofaLoadLimits::default(),
                )
                .unwrap();
                let sum = loaded.bank.entries().iter().fold([0.0; 3], |sum, entry| {
                    std::array::from_fn(|axis| sum[axis] + entry.direction()[axis])
                });
                let target = unit(sum);
                for request_scale in [0.125, 1.0, 4096.0] {
                    let request = position(target, request_scale);
                    let expected = legacy_interpolation::valid_triangle_coefficient_bits(
                        &loaded.bank,
                        request,
                    );
                    for resolver in [resolve_hrir, resolve_hrir_for_listener_orientation] {
                        let covered = resolver(&loaded.bank, request).unwrap_or_else(|error| {
                            panic!(
                                "elevation={elevation} rotation={rotation} source_scale={source_scale} request_scale={request_scale}: {error}"
                            )
                        });
                        assert_eq!(covered.neighbor_count, 3);
                        assert_eq!(covered.exact_entry, None);
                        assert_eq!(coefficient_bits(&covered), expected);
                        assert!(matches!(
                            resolver(&loaded.bank, position(target, -request_scale)),
                            Err(SofaError::InterpolationOutsideCoverage(_))
                        ));
                    }
                }
            }
        }
    }
}

#[test]
fn parsed_rotated_planar_and_outside_boundary_controls_still_reject() {
    for rotation in 0..5 {
        for source_scale in [0.125, 1.0, 4096.0] {
            let planar = parse_simple_free_field_hrir(
                &rotated_cap_fixture(0.0, rotation, source_scale),
                SofaLoadLimits::default(),
            )
            .unwrap();
            for target in [[0.0, 0.0, 1.0], [0.0, 0.0, -1.0], [0.1, 0.1, 1.0]] {
                for resolver in [resolve_hrir, resolve_hrir_for_listener_orientation] {
                    assert!(matches!(
                        resolver(&planar.bank, position(rotate(target, rotation), 1.0)),
                        Err(SofaError::InterpolationOutsideCoverage(_))
                    ));
                }
            }
            let cap = parse_simple_free_field_hrir(
                &rotated_cap_fixture(20.0, rotation, source_scale),
                SofaLoadLimits::default(),
            )
            .unwrap();
            let entries = cap.bank.entries();
            let edge = unit(std::array::from_fn(|axis| {
                entries[0].direction()[axis] + entries[1].direction()[axis]
            }));
            let third = entries[2].direction();
            let toward_interior = unit(std::array::from_fn(|axis| {
                third[axis] - edge[axis] * dot(third, edge)
            }));
            // Step 0.011 degrees outside the actual parsed edge, far enough
            // beyond both the existing angular and arithmetic uncertainty.
            let outside = unit(std::array::from_fn(|axis| {
                edge[axis] - 2.0e-4 * toward_interior[axis]
            }));
            for resolver in [resolve_hrir, resolve_hrir_for_listener_orientation] {
                assert!(matches!(
                    resolver(&cap.bank, position(outside, 1.0)),
                    Err(SofaError::InterpolationOutsideCoverage(_))
                ));
            }
        }
    }
}
