// pattern: Functional Core

use crate::OamdError;

/// Decoded Cartesian object position from TS 103 420 clause 5.6.1.1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Position3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

/// Previous standard-precision position codewords used by differential coding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StandardPositionBits {
    pub x: u8,
    pub y: u8,
    pub z: i8,
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
