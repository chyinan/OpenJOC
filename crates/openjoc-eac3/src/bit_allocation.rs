// pattern: Functional Core

//! Fixed-point Enhanced AC-3 bit-allocation primitives.

use crate::Eac3Error;

/// Maps clause 6.2.2.2 decoded exponents to 13-bit log PSD values.
///
/// # Errors
/// Returns [`Eac3Error::ExponentOutOfRange`] for an exponent above 24.
pub fn exponents_to_psd(exponents: &[u8]) -> Result<Vec<i16>, Eac3Error> {
    exponents
        .iter()
        .copied()
        .map(|exponent| {
            if exponent > 24 {
                return Err(Eac3Error::ExponentOutOfRange {
                    actual: i16::from(exponent),
                });
            }
            Ok(3_072 - (i16::from(exponent) << 7))
        })
        .collect()
}
