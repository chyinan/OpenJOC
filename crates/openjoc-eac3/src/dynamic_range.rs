// pattern: Functional Core

//! ETSI TS 102 366 clause 6.7 dynamic-range gain decoding.

use crate::Eac3Error;

/// Converts one optional 8-bit `dynrng` word to its linear coefficient gain.
///
/// The three most-significant bits select the signed arithmetic-shift term and
/// the five least-significant bits select the fractional multiplier from
/// clause 6.7.2.2. An absent word is the block-default all-zero word.
pub fn dynamic_range_gain(code: Option<u8>) -> f64 {
    let code = code.unwrap_or(0);
    let top = (code >> 5) & 0x07;
    let signed_top = if top & 0x04 != 0 {
        i32::from(top) - 8
    } else {
        i32::from(top)
    };
    let shift = signed_top + 1;
    let fraction = f64::from(code & 0x1f) + 32.0;
    2.0_f64.powi(shift) * fraction / 64.0
}

/// Applies one effective linear gain to each supplied spectral element.
pub fn apply_dynamic_range_gains(
    elements: &[Vec<f64>],
    gains: &[f64],
) -> Result<Vec<Vec<f64>>, Eac3Error> {
    if elements.len() != gains.len() {
        return Err(Eac3Error::InvalidDynamicRangeGainCount {
            expected: elements.len(),
            actual: gains.len(),
        });
    }
    let mut output = Vec::with_capacity(elements.len());
    for (channel, (values, gain)) in elements.iter().zip(gains.iter().copied()).enumerate() {
        if !gain.is_finite() {
            return Err(Eac3Error::NonFiniteDynamicRangeCoefficient { channel, index: 0 });
        }
        let mut scaled = Vec::with_capacity(values.len());
        for (index, value) in values.iter().copied().enumerate() {
            if !value.is_finite() {
                return Err(Eac3Error::NonFiniteDynamicRangeCoefficient { channel, index });
            }
            let scaled_value = value * gain;
            if !scaled_value.is_finite() {
                return Err(Eac3Error::NonFiniteDynamicRangeCoefficient { channel, index });
            }
            scaled.push(scaled_value);
        }
        output.push(scaled);
    }
    Ok(output)
}
