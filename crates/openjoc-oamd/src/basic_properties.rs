// pattern: Functional Core

use crate::OamdError;

/// Gain representation preserving the normative negative-infinity value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Gain {
    Decibels(i16),
    NegativeInfinity,
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
