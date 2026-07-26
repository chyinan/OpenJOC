// pattern: Functional Core

//! Fixed-point Enhanced AC-3 bit-allocation primitives.

use crate::{BitAllocationParameters, Eac3Error};

/// Fixed-point values selected by TS 102 366 tables 6.6 through 6.11.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FixedBitAllocationParameters {
    pub slow_decay: i16,
    pub fast_decay: i16,
    pub slow_gain: i16,
    pub db_per_bit: i16,
    pub floor: i16,
    pub fast_gain: i16,
}

/// Maps transmitted bit-allocation parameter codes through tables 6.6–6.11.
///
/// # Errors
/// Returns [`Eac3Error::InvalidBitAllocationParameterCode`] when any code is
/// outside its normative two- or three-bit table domain.
pub fn decode_bit_allocation_parameters(
    codes: BitAllocationParameters,
    fast_gain_code: u8,
) -> Result<FixedBitAllocationParameters, Eac3Error> {
    const SLOW_DECAY: [i16; 4] = [0x0f, 0x11, 0x13, 0x15];
    const FAST_DECAY: [i16; 4] = [0x3f, 0x53, 0x67, 0x7b];
    const SLOW_GAIN: [i16; 4] = [0x540, 0x4d8, 0x478, 0x410];
    const DB_PER_BIT: [i16; 4] = [0x000, 0x700, 0x900, 0xb00];
    const FLOOR: [i16; 8] = [0x2f0, 0x2b0, 0x270, 0x230, 0x1f0, 0x170, 0x0f0, -0x800];
    const FAST_GAIN: [i16; 8] = [0x080, 0x100, 0x180, 0x200, 0x280, 0x300, 0x380, 0x400];

    Ok(FixedBitAllocationParameters {
        slow_decay: table_value(&SLOW_DECAY, "slow decay", codes.slow_decay_code)?,
        fast_decay: table_value(&FAST_DECAY, "fast decay", codes.fast_decay_code)?,
        slow_gain: table_value(&SLOW_GAIN, "slow gain", codes.slow_gain_code)?,
        db_per_bit: table_value(&DB_PER_BIT, "dB per bit", codes.db_per_bit_code)?,
        floor: table_value(&FLOOR, "floor", codes.floor_code)?,
        fast_gain: table_value(&FAST_GAIN, "fast gain", fast_gain_code)?,
    })
}

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

fn table_value<const N: usize>(
    table: &[i16; N],
    parameter: &'static str,
    code: u8,
) -> Result<i16, Eac3Error> {
    table
        .get(usize::from(code))
        .copied()
        .ok_or(Eac3Error::InvalidBitAllocationParameterCode {
            parameter,
            actual: code,
        })
}
