// pattern: Functional Core

use crate::OamdError;

/// Gain representation preserving the normative negative-infinity value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Gain {
    Decibels(i16),
    NegativeInfinity,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Extent3 {
    pub width: f64,
    pub depth: f64,
    pub height: f64,
}

impl Extent3 {
    pub const ZERO: Self = Self {
        width: 0.0,
        depth: 0.0,
        height: 0.0,
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ZoneConstraint {
    Include,
    Exclude,
}

/// Applies tables 18 and 19 to one object gain code.
///
/// # Errors
/// Returns [`OamdError`] for missing conditional bits or out-of-range codes.
pub fn decode_gain(
    index: u8,
    bits: Option<u8>,
    previous_object: Option<Gain>,
) -> Result<Gain, OamdError> {
    match index {
        0 => Ok(Gain::Decibels(0)),
        1 => Ok(Gain::NegativeInfinity),
        2 => {
            let bits = bits.ok_or(OamdError::MissingGainBits)?;
            if bits > 63 {
                return Err(OamdError::InvalidPropertyCode);
            }
            let db = if bits <= 14 {
                15 - i16::from(bits)
            } else {
                14 - i16::from(bits)
            };
            Ok(Gain::Decibels(db))
        }
        3 => Ok(previous_object.unwrap_or(Gain::Decibels(0))),
        _ => Err(OamdError::InvalidPropertyCode),
    }
}

/// Applies clauses 5.6.1.3.1 and 5.6.1.3.2.
///
/// # Errors
/// Returns [`OamdError`] when non-default priority bits are absent or invalid.
pub fn decode_priority(default: bool, bits: Option<u8>) -> Result<f64, OamdError> {
    if default {
        return Ok(1.0);
    }
    let bits = bits.ok_or(OamdError::MissingPriorityBits)?;
    if bits > 31 {
        return Err(OamdError::InvalidPropertyCode);
    }
    Ok(f64::from(bits) / 32.0)
}

/// Applies table 17 object-size coding.
///
/// # Errors
/// Returns [`OamdError`] for reserved mode, missing fields, or values above 31.
pub fn decode_size(
    index: u8,
    uniform: Option<u8>,
    components: Option<[u8; 3]>,
) -> Result<Extent3, OamdError> {
    let scale = |value: u8| -> Result<f64, OamdError> {
        if value > 31 {
            return Err(OamdError::InvalidPropertyCode);
        }
        Ok(f64::from(value) / 31.0)
    };
    match index {
        0 => Ok(Extent3::ZERO),
        1 => {
            let value = scale(uniform.ok_or(OamdError::MissingSizeBits)?)?;
            Ok(Extent3 {
                width: value,
                depth: value,
                height: value,
            })
        }
        2 => {
            let [width, depth, height] = components.ok_or(OamdError::MissingSizeBits)?;
            Ok(Extent3 {
                width: scale(width)?,
                depth: scale(depth)?,
                height: scale(height)?,
            })
        }
        3 => Err(OamdError::ReservedSizeIndex),
        _ => Err(OamdError::InvalidPropertyCode),
    }
}

/// Applies horizontal table 20 and elevation table 21.
///
/// # Errors
/// Returns [`OamdError`] for reserved indices 6 and 7 or wider inputs.
pub fn decode_zone_constraints(
    index: u8,
    elevation: bool,
) -> Result<[ZoneConstraint; 6], OamdError> {
    use ZoneConstraint::{Exclude, Include};
    let horizontal = match index {
        0 => [Include, Include, Include, Include, Include],
        1 => [Include, Include, Include, Exclude, Include],
        2 => [Include, Exclude, Include, Include, Include],
        3 => [Exclude, Exclude, Exclude, Exclude, Include],
        4 => [Include, Exclude, Exclude, Exclude, Exclude],
        5 => [Exclude, Exclude, Include, Exclude, Exclude],
        6 | 7 => return Err(OamdError::ReservedZoneIndex { index }),
        _ => return Err(OamdError::InvalidPropertyCode),
    };
    Ok([
        horizontal[0],
        horizontal[1],
        horizontal[2],
        horizontal[3],
        horizontal[4],
        if elevation { Include } else { Exclude },
    ])
}
