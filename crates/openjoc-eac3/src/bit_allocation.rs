// pattern: Functional Core

//! Fixed-point Enhanced AC-3 bit-allocation primitives.

use crate::{BitAllocationParameters, Eac3Error};

const BIT_ALLOCATION_BANDS: [BitAllocationBand; 50] = [
    BitAllocationBand { start: 0, size: 1 },
    BitAllocationBand { start: 1, size: 1 },
    BitAllocationBand { start: 2, size: 1 },
    BitAllocationBand { start: 3, size: 1 },
    BitAllocationBand { start: 4, size: 1 },
    BitAllocationBand { start: 5, size: 1 },
    BitAllocationBand { start: 6, size: 1 },
    BitAllocationBand { start: 7, size: 1 },
    BitAllocationBand { start: 8, size: 1 },
    BitAllocationBand { start: 9, size: 1 },
    BitAllocationBand { start: 10, size: 1 },
    BitAllocationBand { start: 11, size: 1 },
    BitAllocationBand { start: 12, size: 1 },
    BitAllocationBand { start: 13, size: 1 },
    BitAllocationBand { start: 14, size: 1 },
    BitAllocationBand { start: 15, size: 1 },
    BitAllocationBand { start: 16, size: 1 },
    BitAllocationBand { start: 17, size: 1 },
    BitAllocationBand { start: 18, size: 1 },
    BitAllocationBand { start: 19, size: 1 },
    BitAllocationBand { start: 20, size: 1 },
    BitAllocationBand { start: 21, size: 1 },
    BitAllocationBand { start: 22, size: 1 },
    BitAllocationBand { start: 23, size: 1 },
    BitAllocationBand { start: 24, size: 1 },
    BitAllocationBand { start: 25, size: 1 },
    BitAllocationBand { start: 26, size: 1 },
    BitAllocationBand { start: 27, size: 1 },
    BitAllocationBand { start: 28, size: 3 },
    BitAllocationBand { start: 31, size: 3 },
    BitAllocationBand { start: 34, size: 3 },
    BitAllocationBand { start: 37, size: 3 },
    BitAllocationBand { start: 40, size: 3 },
    BitAllocationBand { start: 43, size: 3 },
    BitAllocationBand { start: 46, size: 3 },
    BitAllocationBand { start: 49, size: 6 },
    BitAllocationBand { start: 55, size: 6 },
    BitAllocationBand { start: 61, size: 6 },
    BitAllocationBand { start: 67, size: 6 },
    BitAllocationBand { start: 73, size: 6 },
    BitAllocationBand { start: 79, size: 6 },
    BitAllocationBand {
        start: 85,
        size: 12,
    },
    BitAllocationBand {
        start: 97,
        size: 12,
    },
    BitAllocationBand {
        start: 109,
        size: 12,
    },
    BitAllocationBand {
        start: 121,
        size: 12,
    },
    BitAllocationBand {
        start: 133,
        size: 24,
    },
    BitAllocationBand {
        start: 157,
        size: 24,
    },
    BitAllocationBand {
        start: 181,
        size: 24,
    },
    BitAllocationBand {
        start: 205,
        size: 24,
    },
    BitAllocationBand {
        start: 229,
        size: 24,
    },
];

const BIT_ALLOCATION_POINTERS: [u8; 64] = [
    0, 1, 1, 1, 1, 1, 2, 2, 3, 3, 3, 4, 4, 5, 5, 6, 6, 6, 6, 7, 7, 7, 7, 8, 8, 8, 8, 9, 9, 9, 9,
    10, 10, 10, 10, 11, 11, 11, 11, 12, 12, 12, 12, 13, 13, 13, 13, 14, 14, 14, 14, 14, 14, 14, 14,
    15, 15, 15, 15, 15, 15, 15, 15, 15,
];

/// One row of TS 102 366 Table 6.12.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BitAllocationBand {
    pub start: u16,
    pub size: u8,
}

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

/// Returns one normative bit-allocation band by band number.
///
/// # Errors
/// Returns [`Eac3Error::InvalidBitAllocationTableIndex`] for band numbers above
/// 49.
pub fn bit_allocation_band(index: u8) -> Result<BitAllocationBand, Eac3Error> {
    BIT_ALLOCATION_BANDS.get(usize::from(index)).copied().ok_or(
        Eac3Error::InvalidBitAllocationTableIndex {
            table: "band",
            actual: u16::from(index),
        },
    )
}

/// Maps an audio transform bin to its normative Table 6.12 band number.
///
/// # Errors
/// Returns [`Eac3Error::InvalidBitAllocationTableIndex`] for bins above 252.
pub fn bit_allocation_band_for_bin(bin: u16) -> Result<u8, Eac3Error> {
    BIT_ALLOCATION_BANDS
        .iter()
        .position(|band| bin >= band.start && bin < band.start + u16::from(band.size))
        .and_then(|index| u8::try_from(index).ok())
        .ok_or(Eac3Error::InvalidBitAllocationTableIndex {
            table: "bin",
            actual: bin,
        })
}

/// Maps a clamped six-bit address through TS 102 366 Table 6.16.
///
/// # Errors
/// Returns [`Eac3Error::InvalidBitAllocationTableIndex`] for addresses above
/// 63.
pub fn bit_allocation_pointer(address: u8) -> Result<u8, Eac3Error> {
    BIT_ALLOCATION_POINTERS
        .get(usize::from(address))
        .copied()
        .ok_or(Eac3Error::InvalidBitAllocationTableIndex {
            table: "pointer",
            actual: u16::from(address),
        })
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
