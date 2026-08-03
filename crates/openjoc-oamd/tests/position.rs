use openjoc_oamd::{
    Distance, OamdError, Position3, ReferenceScreen, RoomPosition, StandardPositionBits,
    decode_absolute_position, decode_depth_factor, decode_differential_position,
    decode_distance_factor, decode_screen_factor, decode_signed_position_delta,
    interpolate_screen_position, project_room_position,
};

fn assert_close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1.0e-12,
        "{actual} != {expected}"
    );
}

fn assert_position_close(actual: Position3, expected: Position3) {
    assert_close(actual.x, expected.x);
    assert_close(actual.y, expected.y);
    assert_close(actual.z, expected.z);
}

#[test]
fn decodes_absolute_positions_and_extended_precision() {
    assert_eq!(
        decode_absolute_position(0, 62, false, 15, [None, Some(0), Some(3)]),
        Ok(Position3 {
            x: 0.0,
            y: 1.0,
            z: -1.0 - 2.0 / 75.0,
        })
    );
    assert_eq!(
        decode_absolute_position(63, 61, true, 15, [Some(1), Some(2), Some(0)]),
        Ok(Position3 {
            x: 1.0,
            y: 61.0 / 62.0 - 1.0 / 310.0,
            z: 1.0 + 1.0 / 75.0,
        })
    );
}

#[test]
fn decodes_every_signed_three_bit_position_delta() {
    let expected = [0, 1, 2, 3, -4, -3, -2, -1];
    for (raw, expected) in expected.into_iter().enumerate() {
        assert_eq!(
            decode_signed_position_delta(u8::try_from(raw).expect("raw")),
            Ok(expected)
        );
    }
    assert_eq!(
        decode_signed_position_delta(8),
        Err(OamdError::InvalidPropertyCode)
    );
}

#[test]
fn decodes_differential_positions_with_normative_clamps() {
    assert_eq!(
        decode_differential_position(
            StandardPositionBits { x: 1, y: 61, z: 15 },
            [4, 3, 3],
            [Some(3), Some(1), Some(1)],
        ),
        Ok(Position3 {
            x: 0.0,
            y: 1.0,
            z: 1.0,
        })
    );
    assert_eq!(
        decode_differential_position(
            StandardPositionBits { x: 0, y: 0, z: -15 },
            [7, 0, 4],
            [None, None, Some(3)],
        ),
        Ok(Position3 {
            x: 0.0,
            y: 0.0,
            z: -1.0,
        })
    );
}

#[test]
fn rejects_values_outside_normative_field_widths() {
    assert_eq!(
        decode_absolute_position(64, 0, true, 0, [None; 3]),
        Err(OamdError::InvalidPropertyCode)
    );
    assert_eq!(
        decode_absolute_position(0, 0, true, 16, [None; 3]),
        Err(OamdError::InvalidPropertyCode)
    );
    assert_eq!(
        decode_absolute_position(0, 0, true, 0, [Some(4), None, None]),
        Err(OamdError::InvalidPropertyCode)
    );
    assert_eq!(
        decode_differential_position(
            StandardPositionBits { x: 63, y: 64, z: 0 },
            [0; 3],
            [None; 3],
        ),
        Err(OamdError::InvalidPropertyCode)
    );
    assert_eq!(
        decode_differential_position(
            StandardPositionBits { x: 0, y: 0, z: 16 },
            [0; 3],
            [None; 3],
        ),
        Err(OamdError::InvalidPropertyCode)
    );
}

#[test]
fn decodes_all_distance_screen_and_depth_factors() {
    let distances = [
        1.1, 1.3, 1.6, 2.0, 2.5, 3.2, 4.0, 5.0, 6.3, 7.9, 10.0, 12.6, 15.8, 20.0, 25.1, 50.1,
    ];
    for (index, expected) in distances.into_iter().enumerate() {
        assert_close(
            decode_distance_factor(u8::try_from(index).expect("index")).expect("distance"),
            expected,
        );
    }
    for bits in 0_u8..=7 {
        assert_close(
            decode_screen_factor(bits).expect("screen factor"),
            f64::from(bits + 1) / 8.0,
        );
    }
    for (index, expected) in [0.25, 0.5, 1.0, 2.0].into_iter().enumerate() {
        assert_close(
            decode_depth_factor(u8::try_from(index).expect("index")).expect("depth"),
            expected,
        );
    }
    assert!(decode_distance_factor(16).is_err());
    assert!(decode_screen_factor(8).is_err());
    assert!(decode_depth_factor(4).is_err());
}

#[test]
fn projects_finite_room_positions_from_the_centre_through_the_boundary() {
    assert_eq!(
        project_room_position(
            Position3 {
                x: 0.25,
                y: 0.75,
                z: 0.25,
            },
            Distance::InsideRoom,
        ),
        Ok(RoomPosition::Finite(Position3 {
            x: 0.25,
            y: 0.75,
            z: 0.25,
        }))
    );
    assert_eq!(
        project_room_position(
            Position3 {
                x: 0.75,
                y: 0.75,
                z: 0.25,
            },
            Distance::Finite(2.0),
        ),
        Ok(RoomPosition::Finite(Position3 {
            x: 1.5,
            y: 1.5,
            z: 1.0,
        }))
    );
    assert_eq!(
        project_room_position(
            Position3 {
                x: 1.0,
                y: 0.5,
                z: 0.0,
            },
            Distance::Finite(2.0),
        ),
        Ok(RoomPosition::Finite(Position3 {
            x: 1.5,
            y: 0.5,
            z: 0.0,
        }))
    );
}

#[test]
fn represents_infinite_room_position_without_nan_coordinates() {
    assert_eq!(
        project_room_position(
            Position3 {
                x: 1.0,
                y: 0.5,
                z: 0.0,
            },
            Distance::Infinity,
        ),
        Ok(RoomPosition::AtInfinity {
            boundary_intersection: Position3 {
                x: 1.0,
                y: 0.5,
                z: 0.0,
            },
        })
    );
}

#[test]
fn rejects_undefined_projection_rays_and_invalid_factors() {
    let centre = Position3 {
        x: 0.5,
        y: 0.5,
        z: 0.0,
    };
    assert_eq!(
        project_room_position(centre, Distance::Finite(1.1)),
        Err(OamdError::UndefinedRoomProjectionDirection)
    );
    assert_eq!(
        project_room_position(
            Position3 {
                x: 1.0,
                y: 0.5,
                z: 0.0,
            },
            Distance::Finite(0.5),
        ),
        Err(OamdError::InvalidRoomDistanceFactor)
    );
}

#[test]
fn interpolates_screen_position_with_normative_diagonal_matrices() {
    let reference_screen = ReferenceScreen {
        bottom_left: Position3 {
            x: 0.1,
            y: 0.0,
            z: -0.5,
        },
        width: 0.8,
        height: 1.0,
    };
    let coded = Position3 {
        x: 0.25,
        y: 0.5,
        z: 0.5,
    };

    assert_position_close(
        interpolate_screen_position(coded, 0.5, 2.0, reference_screen)
            .expect("valid screen interpolation"),
        Position3 {
            x: 0.29375,
            y: 0.5,
            z: 0.28125,
        },
    );
    assert_position_close(
        interpolate_screen_position(coded, 0.0, 2.0, reference_screen)
            .expect("valid room-anchored endpoint"),
        Position3 {
            x: 0.3,
            y: 0.5,
            z: 0.25,
        },
    );
    assert_eq!(
        interpolate_screen_position(coded, 1.0, 0.0, reference_screen),
        Ok(coded)
    );
}

#[test]
fn screen_interpolation_preserves_normative_extended_coordinate_overshoot() {
    let coded = Position3 {
        x: -2.0 / 310.0,
        y: 1.0,
        z: 1.0 + 1.0 / 75.0,
    };
    let screen = ReferenceScreen {
        bottom_left: Position3 {
            x: 0.1,
            y: 0.0,
            z: -0.5,
        },
        width: 0.8,
        height: 1.0,
    };

    assert_eq!(
        interpolate_screen_position(coded, 1.0, 0.25, screen),
        Ok(coded)
    );
}

#[test]
fn screen_interpolation_rejects_nonfinite_depth_mix() {
    let coded = Position3 {
        x: 0.5,
        y: -1.0 / 310.0,
        z: 0.0,
    };
    let screen = ReferenceScreen {
        bottom_left: Position3 {
            x: 0.0,
            y: 0.0,
            z: -0.5,
        },
        width: 1.0,
        height: 1.0,
    };

    assert_eq!(
        interpolate_screen_position(coded, 0.5, 0.25, screen),
        Err(OamdError::InvalidPropertyCode)
    );
}
