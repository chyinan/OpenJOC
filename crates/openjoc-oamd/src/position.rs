// pattern: Functional Core

use crate::OamdError;

/// Decoded Cartesian object position from TS 103 420 clause 5.6.1.1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Position3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

/// Room distance signalled by clauses 5.6.1.1.15 through 5.6.1.1.17.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Distance {
    InsideRoom,
    Finite(f64),
    Infinity,
}

/// Clause 5.2.1.2 room-anchored decoder-interface position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RoomPosition {
    Finite(Position3),
    AtInfinity { boundary_intersection: Position3 },
}

/// Previous standard-precision position codewords used by differential coding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StandardPositionBits {
    pub x: u8,
    pub y: u8,
    pub z: i8,
}

/// Standard-precision coding needed to apply a corresponding ID 5 extension.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PositionCoding {
    Absolute(StandardPositionBits),
    Differential {
        previous: StandardPositionBits,
        delta: [u8; 3],
    },
}

impl PositionCoding {
    /// Applies tables 44 through 46 in the coordinate equation's normative
    /// pre-clamp position.
    ///
    /// # Errors
    /// Returns an OAMD error for an invalid standard or extension codeword.
    pub fn decode(self, extended: [Option<u8>; 3]) -> Result<Position3, OamdError> {
        match self {
            Self::Absolute(bits) => decode_absolute_position(
                bits.x,
                bits.y,
                bits.z >= 0,
                bits.z.unsigned_abs(),
                extended,
            ),
            Self::Differential { previous, delta } => {
                decode_differential_position(previous, delta, extended)
            }
        }
    }
}

/// Decodes clauses 5.6.1.1.8 through 5.6.1.1.11.
///
/// `z_positive` is true for `pos3D_Z_sign_bits == 1`. Optional extended
/// precision values are transmitted table 44 through 46 indices.
///
/// # Errors
/// Returns [`OamdError`] when a value exceeds its normative field width.
pub fn decode_absolute_position(
    x: u8,
    y: u8,
    z_positive: bool,
    z: u8,
    extended: [Option<u8>; 3],
) -> Result<Position3, OamdError> {
    if x > 63 || y > 63 || z > 15 {
        return Err(OamdError::InvalidPropertyCode);
    }
    let [ext_x, ext_y, ext_z] = decode_extended(extended)?;
    let z_sign = if z_positive { 1.0 } else { -1.0 };
    Ok(Position3 {
        x: (f64::from(x) / 62.0 + f64::from(ext_x) / 310.0).min(1.0),
        y: (f64::from(y) / 62.0 + f64::from(ext_y) / 310.0).min(1.0),
        z: z_sign * f64::from(z) / 15.0 + f64::from(ext_z) / 75.0,
    })
}

/// Decodes clauses 5.6.1.1.12 through 5.6.1.1.14.
///
/// # Errors
/// Returns [`OamdError`] when a previous value, delta, or extension exceeds its
/// normative field width.
pub fn decode_differential_position(
    previous: StandardPositionBits,
    delta: [u8; 3],
    extended: [Option<u8>; 3],
) -> Result<Position3, OamdError> {
    if previous.x > 63 || previous.y > 63 || !(-15..=15).contains(&previous.z) {
        return Err(OamdError::InvalidPropertyCode);
    }
    let dx = decode_signed_position_delta(delta[0])?;
    let dy = decode_signed_position_delta(delta[1])?;
    let dz = decode_signed_position_delta(delta[2])?;
    let [ext_x, ext_y, ext_z] = decode_extended(extended)?;
    Ok(Position3 {
        x: (f64::from(previous.x) / 62.0 + f64::from(dx) / 62.0 + f64::from(ext_x) / 310.0)
            .clamp(0.0, 1.0),
        y: (f64::from(previous.y) / 62.0 + f64::from(dy) / 62.0 + f64::from(ext_y) / 310.0)
            .clamp(0.0, 1.0),
        z: (f64::from(previous.z) / 15.0 + f64::from(dz) / 15.0 + f64::from(ext_z) / 75.0)
            .clamp(-1.0, 1.0),
    })
}

/// Interprets a three-bit two's-complement differential position codeword.
///
/// # Errors
/// Returns [`OamdError`] for values wider than three bits.
pub fn decode_signed_position_delta(raw: u8) -> Result<i8, OamdError> {
    if raw > 7 {
        return Err(OamdError::InvalidPropertyCode);
    }
    let value = i8::try_from(raw).map_err(|_| OamdError::InvalidPropertyCode)?;
    Ok(if raw & 4 == 0 { value } else { value - 8 })
}

/// Applies table 15.
///
/// # Errors
/// Returns [`OamdError`] for an index wider than four bits.
pub fn decode_distance_factor(index: u8) -> Result<f64, OamdError> {
    const FACTORS: [f64; 16] = [
        1.1, 1.3, 1.6, 2.0, 2.5, 3.2, 4.0, 5.0, 6.3, 7.9, 10.0, 12.6, 15.8, 20.0, 25.1, 50.1,
    ];
    FACTORS
        .get(usize::from(index))
        .copied()
        .ok_or(OamdError::InvalidPropertyCode)
}

/// Applies clause 5.6.1.1.19.
///
/// # Errors
/// Returns [`OamdError`] for a value wider than three bits.
pub fn decode_screen_factor(bits: u8) -> Result<f64, OamdError> {
    if bits > 7 {
        return Err(OamdError::InvalidPropertyCode);
    }
    Ok(f64::from(bits + 1) / 8.0)
}

/// Applies table 16.
///
/// # Errors
/// Returns [`OamdError`] for an index wider than two bits.
pub fn decode_depth_factor(index: u8) -> Result<f64, OamdError> {
    [0.25, 0.5, 1.0, 2.0]
        .get(usize::from(index))
        .copied()
        .ok_or(OamdError::InvalidPropertyCode)
}

/// Projects coded room coordinates through the room boundary per clause 5.2.1.2.
///
/// An infinite position retains the finite boundary intersection defining its
/// ray, avoiding undefined infinity-times-zero floating-point components.
///
/// # Errors
/// Returns an OAMD error for non-finite inputs, an invalid finite distance
/// factor, or a distance-specified object at the exact room centre.
pub fn project_room_position(
    coded: Position3,
    distance: Distance,
) -> Result<RoomPosition, OamdError> {
    const ORIGIN: Position3 = Position3 {
        x: 0.5,
        y: 0.5,
        z: 0.0,
    };

    if !coded.x.is_finite() || !coded.y.is_finite() || !coded.z.is_finite() {
        return Err(OamdError::InvalidPropertyCode);
    }
    if distance == Distance::InsideRoom {
        return Ok(RoomPosition::Finite(coded));
    }
    let direction = Position3 {
        x: coded.x - ORIGIN.x,
        y: coded.y - ORIGIN.y,
        z: coded.z - ORIGIN.z,
    };
    let mut boundary_scale = f64::INFINITY;
    for scale in [
        axis_boundary_scale(direction.x, 0.5),
        axis_boundary_scale(direction.y, 0.5),
        axis_boundary_scale(direction.z, 1.0),
    ]
    .into_iter()
    .flatten()
    {
        boundary_scale = boundary_scale.min(scale);
    }
    if !boundary_scale.is_finite() {
        return Err(OamdError::UndefinedRoomProjectionDirection);
    }
    let boundary_intersection = Position3 {
        x: ORIGIN.x + boundary_scale * direction.x,
        y: ORIGIN.y + boundary_scale * direction.y,
        z: ORIGIN.z + boundary_scale * direction.z,
    };
    match distance {
        Distance::InsideRoom => unreachable!(),
        Distance::Infinity => Ok(RoomPosition::AtInfinity {
            boundary_intersection,
        }),
        Distance::Finite(factor) => {
            if !factor.is_finite() || factor <= 1.0 {
                return Err(OamdError::InvalidRoomDistanceFactor);
            }
            Ok(RoomPosition::Finite(Position3 {
                x: ORIGIN.x + factor * (boundary_intersection.x - ORIGIN.x),
                y: ORIGIN.y + factor * (boundary_intersection.y - ORIGIN.y),
                z: ORIGIN.z + factor * (boundary_intersection.z - ORIGIN.z),
            }))
        }
    }
}

fn axis_boundary_scale(direction: f64, half_extent: f64) -> Option<f64> {
    (direction != 0.0).then_some(half_extent / direction.abs())
}

fn decode_extended(values: [Option<u8>; 3]) -> Result<[i8; 3], OamdError> {
    let decode = |value| match value {
        None => Ok(0),
        Some(0) => Ok(1),
        Some(1) => Ok(2),
        Some(2) => Ok(-1),
        Some(3) => Ok(-2),
        Some(_) => Err(OamdError::InvalidPropertyCode),
    };
    let [x, y, z] = values;
    Ok([decode(x)?, decode(y)?, decode(z)?])
}
