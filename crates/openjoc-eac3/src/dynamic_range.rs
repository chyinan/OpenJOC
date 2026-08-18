// pattern: Functional Core

//! ETSI TS 102 366 clause 6.7 dynamic-range gain decoding.

use crate::Eac3Error;

/// Decoder policy for the public E-AC-3 dynamic-range control metadata.
///
/// `Custom` scales the signed fixed-point `dynrng` word in its normative
/// signed-fraction domain. The percentage fields are intentionally plain
/// bytes so callers can validate them at their own API boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DynamicRangeControl {
    /// Ignore both `dynrng`/`dynrng2` and RF `compr`/`compr2` metadata.
    Disabled,
    /// Apply the per-audio-block `dynrng`/`dynrng2` words in full.
    Line,
    /// Apply syncframe `compr`/`compr2`, falling back to `dynrng`/`dynrng2`.
    Rf,
    /// Apply independently scaled positive and negative `dynrng` actions.
    Custom { boost_percent: u8, cut_percent: u8 },
}

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

/// Converts one optional 8-bit `compr` word to its linear coefficient gain.
///
/// The four most-significant bits select the signed arithmetic-shift term and
/// the four least-significant bits select the fractional multiplier from
/// clause 6.7.3.2.
pub fn compression_gain(code: Option<u8>) -> f64 {
    let code = code.unwrap_or(0);
    let top = (code >> 4) & 0x0f;
    let signed_top = if top & 0x08 != 0 {
        i32::from(top) - 16
    } else {
        i32::from(top)
    };
    let shift = signed_top + 1;
    let fraction = f64::from(code & 0x0f) + 16.0;
    2.0_f64.powi(shift) * fraction / 32.0
}

/// Applies a custom DRC percentage in the signed Q1.7 domain defined by
/// clause 6.7.2.1. The result is rounded to the nearest representable
/// 8-bit signed fraction before it is decoded by [`dynamic_range_gain`].
pub fn scaled_dynamic_range_code(code: u8, percent: u8) -> u8 {
    let signed = f64::from(i8::from_ne_bytes([code])) * f64::from(percent) / 100.0;
    let rounded = signed.round().clamp(-128.0, 127.0) as i8;
    u8::from_ne_bytes(rounded.to_ne_bytes())
}

/// Computes the effective gain for one channel and one metadata interval.
pub(crate) fn effective_dynamic_range_gain(
    policy: DynamicRangeControl,
    dynrng: Option<u8>,
    compr: Option<u8>,
) -> f64 {
    match policy {
        DynamicRangeControl::Disabled => 1.0,
        DynamicRangeControl::Line => dynamic_range_gain(dynrng),
        DynamicRangeControl::Rf => compr
            .map(|code| compression_gain(Some(code)))
            .unwrap_or_else(|| dynamic_range_gain(dynrng)),
        DynamicRangeControl::Custom {
            boost_percent,
            cut_percent,
        } => {
            let code = dynrng.unwrap_or(0);
            let signed = i8::from_ne_bytes([code]);
            let percent = if signed < 0 {
                cut_percent
            } else {
                boost_percent
            };
            dynamic_range_gain(Some(scaled_dynamic_range_code(code, percent)))
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_spec_gain_fixtures_cover_dynrng_and_compr_domains() {
        assert_eq!(dynamic_range_gain(Some(0x00)), 1.0);
        assert_eq!(dynamic_range_gain(Some(0x60)), 8.0);
        assert_eq!(dynamic_range_gain(Some(0xff)), 63.0 / 64.0);
        assert_eq!(compression_gain(Some(0x00)), 1.0);
        assert_eq!(compression_gain(Some(0x80)), 1.0 / 256.0);
    }

    #[test]
    fn custom_scaling_uses_signed_q1_7_and_directional_identity() {
        assert_eq!(scaled_dynamic_range_code(0x60, 0), 0x00);
        assert_eq!(scaled_dynamic_range_code(0x60, 50), 0x30);
        assert_eq!(scaled_dynamic_range_code(0x60, 100), 0x60);
        assert_eq!(scaled_dynamic_range_code(0xa0, 0), 0x00);
        assert_eq!(scaled_dynamic_range_code(0xa0, 50), 0xd0);
        assert_eq!(scaled_dynamic_range_code(0xa0, 100), 0xa0);
    }

    #[test]
    fn policy_modes_cover_disabled_line_rf_fallback_and_custom() {
        assert_eq!(
            effective_dynamic_range_gain(DynamicRangeControl::Disabled, Some(0x60), Some(0x00)),
            1.0
        );
        assert_eq!(
            effective_dynamic_range_gain(DynamicRangeControl::Line, Some(0x60), Some(0x00)),
            8.0
        );
        assert_eq!(
            effective_dynamic_range_gain(DynamicRangeControl::Rf, Some(0x60), Some(0x00)),
            1.0
        );
        assert_eq!(
            effective_dynamic_range_gain(DynamicRangeControl::Rf, Some(0x60), None),
            8.0
        );
        assert_eq!(
            effective_dynamic_range_gain(
                DynamicRangeControl::Custom {
                    boost_percent: 50,
                    cut_percent: 50,
                },
                Some(0x60),
                None,
            ),
            3.0
        );
    }
}
