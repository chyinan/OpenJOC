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

const HIGH_EFFICIENCY_BIT_ALLOCATION_POINTERS: [u8; 64] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 8, 8, 8, 9, 9, 9, 10, 10, 10, 10, 11, 11, 11, 11, 12, 12, 12, 12,
    13, 13, 13, 13, 14, 14, 14, 14, 15, 15, 15, 15, 16, 16, 16, 16, 17, 17, 17, 17, 18, 18, 18, 18,
    18, 18, 18, 18, 19, 19, 19, 19, 19, 19, 19, 19, 19,
];

// TS 102 366 V1.4.1 Table 6.14, indexed as 10 * A + B. The printed table
// contains ten columns for A=0..25; addresses above 255 are unreachable
// because the normative caller clamps the address to 255.
const LOG_ADDITION_TABLE: [[i16; 10]; 26] = [
    [
        0x0040, 0x003f, 0x003e, 0x003d, 0x003c, 0x003b, 0x003a, 0x0039, 0x0038, 0x0037,
    ],
    [
        0x0036, 0x0035, 0x0034, 0x0034, 0x0033, 0x0032, 0x0031, 0x0030, 0x002f, 0x002f,
    ],
    [
        0x002e, 0x002d, 0x002c, 0x002c, 0x002b, 0x002a, 0x0029, 0x0029, 0x0028, 0x0027,
    ],
    [
        0x0026, 0x0026, 0x0025, 0x0024, 0x0024, 0x0023, 0x0023, 0x0022, 0x0021, 0x0021,
    ],
    [
        0x0020, 0x0020, 0x001f, 0x001e, 0x001e, 0x001d, 0x001d, 0x001c, 0x001c, 0x001b,
    ],
    [
        0x001b, 0x001a, 0x001a, 0x0019, 0x0019, 0x0018, 0x0018, 0x0017, 0x0017, 0x0016,
    ],
    [
        0x0016, 0x0015, 0x0015, 0x0015, 0x0014, 0x0014, 0x0013, 0x0013, 0x0013, 0x0012,
    ],
    [
        0x0012, 0x0012, 0x0011, 0x0011, 0x0011, 0x0010, 0x0010, 0x0010, 0x000f, 0x000f,
    ],
    [
        0x000f, 0x000e, 0x000e, 0x000e, 0x000d, 0x000d, 0x000d, 0x000d, 0x000c, 0x000c,
    ],
    [
        0x000c, 0x000c, 0x000b, 0x000b, 0x000b, 0x000b, 0x000a, 0x000a, 0x000a, 0x000a,
    ],
    [
        0x000a, 0x0009, 0x0009, 0x0009, 0x0009, 0x0009, 0x0008, 0x0008, 0x0008, 0x0008,
    ],
    [
        0x0008, 0x0008, 0x0007, 0x0007, 0x0007, 0x0007, 0x0007, 0x0007, 0x0006, 0x0006,
    ],
    [
        0x0006, 0x0006, 0x0006, 0x0006, 0x0006, 0x0006, 0x0005, 0x0005, 0x0005, 0x0005,
    ],
    [
        0x0005, 0x0005, 0x0005, 0x0005, 0x0004, 0x0004, 0x0004, 0x0004, 0x0004, 0x0004,
    ],
    [
        0x0004, 0x0004, 0x0004, 0x0004, 0x0004, 0x0003, 0x0003, 0x0003, 0x0003, 0x0003,
    ],
    [
        0x0003, 0x0003, 0x0003, 0x0003, 0x0003, 0x0003, 0x0003, 0x0003, 0x0003, 0x0002,
    ],
    [
        0x0002, 0x0002, 0x0002, 0x0002, 0x0002, 0x0002, 0x0002, 0x0002, 0x0002, 0x0002,
    ],
    [
        0x0002, 0x0002, 0x0002, 0x0002, 0x0002, 0x0002, 0x0002, 0x0002, 0x0001, 0x0001,
    ],
    [
        0x0001, 0x0001, 0x0001, 0x0001, 0x0001, 0x0001, 0x0001, 0x0001, 0x0001, 0x0001,
    ],
    [
        0x0001, 0x0001, 0x0001, 0x0001, 0x0001, 0x0001, 0x0001, 0x0001, 0x0001, 0x0001,
    ],
    [
        0x0001, 0x0001, 0x0001, 0x0001, 0x0001, 0x0001, 0x0001, 0x0001, 0x0001, 0x0001,
    ],
    [
        0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000,
    ],
    [
        0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000,
    ],
    [
        0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000,
    ],
    [
        0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000,
    ],
    [
        0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000,
    ],
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

/// Applies TS 102 366 clause 6.2.2.3's `logadd(a, b)` operation.
///
/// The ETSI pseudocode prints the internal difference as a dedicated `~`
/// glyph in the earlier official revisions and as a missing Type-3 glyph in
/// V1.4.1. The surrounding normative text defines that value as the
/// difference between the operands. This implementation therefore computes
/// `c = a - b`, addresses Table 6.14 with `min(abs(c) >> 1, 255)`, and adds
/// the table value to the larger operand.
#[must_use]
pub fn log_add(a: i16, b: i16) -> i16 {
    let difference = i32::from(a) - i32::from(b);
    let address = usize::try_from((difference.abs() >> 1).min(255)).unwrap_or(255);
    let correction = LOG_ADDITION_TABLE[address / 10][address % 10];
    if difference >= 0 {
        a.saturating_add(correction)
    } else {
        b.saturating_add(correction)
    }
}

/// Integrates a fine-grain PSD interval into the 50 Table 6.12 bands.
///
/// The returned vector always has one entry per normative band. Bands outside
/// `start..end` remain zero; callers use the same interval to limit later
/// excitation and masking stages. `end` is exclusive, so the largest legal
/// end value is 253 (the last represented bin is 252).
///
/// # Errors
/// Returns [`Eac3Error::InvalidPsdRange`] when the interval is empty, extends
/// beyond the Table 6.12 audio-bin domain, or is not present in `psd`.
pub fn integrate_psd(psd: &[i16], start: usize, end: usize) -> Result<Vec<i16>, Eac3Error> {
    if start >= end || end > 253 || end > psd.len() {
        return Err(Eac3Error::InvalidPsdRange { start, end });
    }
    let first_band = usize::from(bit_allocation_band_for_bin(
        u16::try_from(start).map_err(|_| Eac3Error::InvalidPsdRange { start, end })?,
    )?);
    let mut integrated = vec![0_i16; BIT_ALLOCATION_BANDS.len()];
    let mut bin = start;
    let mut band = first_band;
    while bin < end {
        let definition = BIT_ALLOCATION_BANDS
            .get(band)
            .ok_or(Eac3Error::InvalidPsdRange { start, end })?;
        let last_bin = (usize::from(definition.start) + usize::from(definition.size)).min(end);
        let mut value = *psd
            .get(bin)
            .ok_or(Eac3Error::InvalidPsdRange { start, end })?;
        bin += 1;
        while bin < last_bin {
            value = log_add(
                value,
                *psd.get(bin)
                    .ok_or(Eac3Error::InvalidPsdRange { start, end })?,
            );
            bin += 1;
        }
        integrated[band] = value;
        band += 1;
    }
    Ok(integrated)
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

/// Maps a clamped six-bit address through TS 102 366 Table E.2.1.
///
/// # Errors
/// Returns [`Eac3Error::InvalidBitAllocationTableIndex`] for addresses above
/// 63.
pub fn high_efficiency_bit_allocation_pointer(address: u8) -> Result<u8, Eac3Error> {
    HIGH_EFFICIENCY_BIT_ALLOCATION_POINTERS
        .get(usize::from(address))
        .copied()
        .ok_or(Eac3Error::InvalidBitAllocationTableIndex {
            table: "high-efficiency pointer",
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

/// Evaluates the clause 6.2.2.1 coarse/fine SNR-offset expression.
///
/// The V1.4.1 pseudocode prints `((coarse - 15) << 4 + fine) << 2` without
/// grouping the addition. The field widths and fixed-point scale require the
/// dimensionally consistent interpretation `(((coarse - 15) << 4) + fine) <<
/// 2`, which is `(coarse - 15) * 64 + fine * 4`. This ambiguity is recorded in
/// `RESEARCH_NOTES.md` and must remain visible until a legal conformance vector
/// or ETSI correction settles it.
///
/// # Errors
/// Returns [`Eac3Error::InvalidBitAllocationParameterCode`] when either code
/// exceeds its normative field width.
pub fn snr_offset(coarse_code: u8, fine_code: u8) -> Result<i16, Eac3Error> {
    if coarse_code > 63 {
        return Err(Eac3Error::InvalidBitAllocationParameterCode {
            parameter: "coarse SNR offset",
            actual: coarse_code,
        });
    }
    if fine_code > 15 {
        return Err(Eac3Error::InvalidBitAllocationParameterCode {
            parameter: "fine SNR offset",
            actual: fine_code,
        });
    }
    Ok((i16::from(coarse_code) - 15) * 64 + i16::from(fine_code) * 4)
}

/// Checks the clause 6.2.2.1 all-zero SNR special case.
///
/// When the coarse code and every active fine code are zero, the normative
/// decoder sets every element of `bap[]` to zero and skips the remaining
/// parametric allocation stages for that block. The caller supplies all active
/// fine codes, including coupling and LFE when present.
///
/// # Errors
/// Returns [`Eac3Error::InvalidBitAllocationParameterCode`] for a value wider
/// than its transmitted field.
pub fn snr_offsets_are_zero(coarse_code: u8, fine_codes: &[u8]) -> Result<bool, Eac3Error> {
    let _ = snr_offset(coarse_code, 0)?;
    for &fine_code in fine_codes {
        let _ = snr_offset(15, fine_code)?;
    }
    Ok(coarse_code == 0 && fine_codes.iter().all(|&fine| fine == 0))
}

/// Computes one element's complete clause 6.2.2 parametric `bap[]` array.
///
/// `exponents` is a full-bin exponent array; only `start..end` participates in
/// this element. `coupling_leaks` selects the coupling initialization path and
/// contains the transmitted three-bit fast and slow leak codes. `delta` is the
/// optional element-specific delta-bit-allocation segment list. The caller is
/// responsible for applying the all-zero SNR special case before invoking this
/// function.
///
/// # Errors
/// Returns a checked E-AC-3 error for malformed exponent dimensions, parameter
/// codes, spectral ranges, or delta allocation.
pub fn compute_element_bap(
    exponents: &[u8],
    start: usize,
    end: usize,
    parameter_codes: crate::BitAllocationParameters,
    fast_gain_code: u8,
    coarse_snr_code: u8,
    fine_snr_code: u8,
    fscod: u8,
    delta: Option<&crate::DeltaBitAllocationElement>,
    coupling_leaks: Option<(u8, u8)>,
) -> Result<Vec<u8>, Eac3Error> {
    let psd = exponents_to_psd(exponents)?;
    if start >= end || end > psd.len() || end > 253 {
        return Err(Eac3Error::InvalidPsdRange { start, end });
    }
    let bndpsd = integrate_psd(&psd, start, end)?;
    let parameters = decode_bit_allocation_parameters(parameter_codes, fast_gain_code)?;
    let initial_leaks = coupling_leaks
        .map(|(fast_code, slow_code)| {
            if fast_code > 7 {
                return Err(Eac3Error::InvalidBitAllocationParameterCode {
                    parameter: "coupling fast leak",
                    actual: fast_code,
                });
            }
            if slow_code > 7 {
                return Err(Eac3Error::InvalidBitAllocationParameterCode {
                    parameter: "coupling slow leak",
                    actual: slow_code,
                });
            }
            Ok((
                (i16::from(fast_code) << 8) + 768,
                (i16::from(slow_code) << 8) + 768,
            ))
        })
        .transpose()?;
    let excite = compute_excitation(&psd, &bndpsd, start, end, parameters, initial_leaks)?;
    let mask = compute_masking_curve(&bndpsd, &excite, start, end, fscod, parameters.db_per_bit)?;
    let mask = match delta {
        Some(delta) => apply_delta_bit_allocation(&mask, delta)?,
        None => mask,
    };
    compute_bap(
        &psd,
        &mask,
        start,
        end,
        snr_offset(coarse_snr_code, fine_snr_code)?,
        parameters.floor,
    )
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

/// Evaluates TS 102 366 clause 6.2.2.4's low-frequency compensation helper.
///
/// Every published TS 102 366 revision inspected for this implementation
/// prints a semicolon after the first `if` condition. Taken literally that
/// would make the following block unconditional and make its `else` invalid;
/// the surrounding structure and the V1.3.1/V1.4.1 pseudocode therefore leave
/// a documented specification ambiguity. OpenJOC uses the only structured
/// branch interpretation, while retaining this note for a future corrigendum.
#[must_use]
pub fn calc_lowcomp(a: i16, b0: i16, b1: i16, bin: usize) -> i16 {
    let value = if bin < 7 {
        if i32::from(b0) + 256 == i32::from(b1) {
            384
        } else if b0 > b1 {
            i32::from(a) - 64
        } else {
            i32::from(a)
        }
    } else if bin < 20 {
        if i32::from(b0) + 256 == i32::from(b1) {
            320
        } else if b0 > b1 {
            i32::from(a) - 64
        } else {
            i32::from(a)
        }
    } else {
        i32::from(a) - 128
    };
    i16::try_from(value.max(0)).unwrap_or(i16::MAX)
}

fn active_band_range(
    start: usize,
    end: usize,
    psd_len: usize,
    band_len: usize,
) -> Result<(usize, usize), Eac3Error> {
    if start >= end || end > 253 || end > psd_len {
        return Err(Eac3Error::InvalidPsdRange { start, end });
    }
    let first = usize::from(bit_allocation_band_for_bin(
        u16::try_from(start).map_err(|_| Eac3Error::InvalidPsdRange { start, end })?,
    )?);
    let last = usize::from(bit_allocation_band_for_bin(
        u16::try_from(end - 1).map_err(|_| Eac3Error::InvalidPsdRange { start, end })?,
    )?) + 1;
    if last > band_len {
        return Err(Eac3Error::InvalidPsdRange { start, end });
    }
    Ok((first, last))
}

/// Computes the clause 6.2.2.4 excitation function.
///
/// `None` selects the uncoupled fbw/lfe path and requires an active range
/// beginning in band zero. `Some((fastleak, slowleak))` selects the coupling
/// path and supplies its clause 6.2.2.1 leak initialization values. The result
/// is a 50-band array with inactive entries left at zero.
pub fn compute_excitation(
    psd: &[i16],
    bndpsd: &[i16],
    start: usize,
    end: usize,
    parameters: FixedBitAllocationParameters,
    initial_leaks: Option<(i16, i16)>,
) -> Result<Vec<i16>, Eac3Error> {
    let (bndstrt, bndend) = active_band_range(start, end, psd.len(), bndpsd.len())?;
    if bndpsd.len() < 50 {
        return Err(Eac3Error::InvalidPsdRange { start, end });
    }
    let mut excite = vec![0_i16; 50];
    if let Some((mut fastleak, mut slowleak)) = initial_leaks {
        for bin in bndstrt..bndend {
            fastleak = fastleak.saturating_sub(parameters.fast_decay);
            fastleak = fastleak.max(bndpsd[bin].saturating_sub(parameters.fast_gain));
            slowleak = slowleak.saturating_sub(parameters.slow_decay);
            slowleak = slowleak.max(bndpsd[bin].saturating_sub(parameters.slow_gain));
            excite[bin] = fastleak.max(slowleak);
        }
        return Ok(excite);
    }
    if bndstrt != 0 || bndend < 7 {
        return Err(Eac3Error::InvalidPsdRange { start, end });
    }

    let mut lowcomp = 0_i16;
    lowcomp = calc_lowcomp(lowcomp, bndpsd[0], bndpsd[1], 0);
    excite[0] = bndpsd[0]
        .saturating_sub(parameters.fast_gain)
        .saturating_sub(lowcomp);
    lowcomp = calc_lowcomp(lowcomp, bndpsd[1], bndpsd[2], 1);
    excite[1] = bndpsd[1]
        .saturating_sub(parameters.fast_gain)
        .saturating_sub(lowcomp);

    let mut begin = 7_usize;
    let mut fastleak = 0_i16;
    let mut slowleak = 0_i16;
    for bin in 2..7 {
        if bndend != 7 || bin != 6 {
            lowcomp = calc_lowcomp(lowcomp, bndpsd[bin], bndpsd[bin + 1], bin);
        }
        fastleak = bndpsd[bin].saturating_sub(parameters.fast_gain);
        slowleak = bndpsd[bin].saturating_sub(parameters.slow_gain);
        excite[bin] = fastleak.saturating_sub(lowcomp);
        if (bndend != 7 || bin != 6) && bndpsd[bin] <= bndpsd[bin + 1] {
            begin = bin + 1;
            break;
        }
    }
    for bin in begin..bndend.min(22) {
        if bndend != 7 || bin != 6 {
            lowcomp = calc_lowcomp(lowcomp, bndpsd[bin], bndpsd[bin + 1], bin);
        }
        fastleak = fastleak
            .saturating_sub(parameters.fast_decay)
            .max(bndpsd[bin].saturating_sub(parameters.fast_gain));
        slowleak = slowleak
            .saturating_sub(parameters.slow_decay)
            .max(bndpsd[bin].saturating_sub(parameters.slow_gain));
        excite[bin] = fastleak.saturating_sub(lowcomp).max(slowleak);
    }
    for bin in 22..bndend {
        fastleak = fastleak
            .saturating_sub(parameters.fast_decay)
            .max(bndpsd[bin].saturating_sub(parameters.fast_gain));
        slowleak = slowleak
            .saturating_sub(parameters.slow_decay)
            .max(bndpsd[bin].saturating_sub(parameters.slow_gain));
        excite[bin] = fastleak.max(slowleak);
    }
    Ok(excite)
}

/// Computes TS 102 366 clause 6.2.2.5's 50-band masking curve.
pub fn compute_masking_curve(
    bndpsd: &[i16],
    excite: &[i16],
    start: usize,
    end: usize,
    fscod: u8,
    dbknee: i16,
) -> Result<Vec<i16>, Eac3Error> {
    let (bndstrt, bndend) = active_band_range(start, end, 253, bndpsd.len())?;
    if excite.len() < bndend {
        return Err(Eac3Error::InvalidPsdRange { start, end });
    }
    let hearing = match fscod {
        0 => [
            0x04d0, 0x04d0, 0x0440, 0x0400, 0x03e0, 0x03c0, 0x03b0, 0x03b0, 0x03a0, 0x03a0, 0x03a0,
            0x03a0, 0x03a0, 0x0390, 0x0390, 0x0390, 0x0380, 0x0380, 0x0370, 0x0370, 0x0360, 0x0360,
            0x0350, 0x0350, 0x0340, 0x0340, 0x0330, 0x0320, 0x0310, 0x0300, 0x02f0, 0x02f0, 0x02f0,
            0x02f0, 0x0300, 0x0310, 0x0340, 0x0390, 0x03e0, 0x0420, 0x0460, 0x0490, 0x04a0, 0x0460,
            0x0440, 0x0440, 0x0520, 0x0800, 0x0840, 0x0840,
        ],
        1 => [
            0x04f0, 0x04f0, 0x0460, 0x0410, 0x03e0, 0x03d0, 0x03c0, 0x03b0, 0x03b0, 0x03a0, 0x03a0,
            0x03a0, 0x03a0, 0x03a0, 0x0390, 0x0390, 0x0390, 0x0380, 0x0380, 0x0380, 0x0370, 0x0370,
            0x0360, 0x0360, 0x0350, 0x0350, 0x0340, 0x0340, 0x0320, 0x0310, 0x0300, 0x02f0, 0x02f0,
            0x02f0, 0x02f0, 0x0300, 0x0320, 0x0350, 0x0390, 0x03e0, 0x0420, 0x0450, 0x04a0, 0x0490,
            0x0460, 0x0440, 0x0480, 0x0630, 0x0840, 0x0840,
        ],
        2 => [
            0x0580, 0x0580, 0x04b0, 0x0450, 0x0420, 0x03f0, 0x03e0, 0x03d0, 0x03c0, 0x03b0, 0x03b0,
            0x03b0, 0x03a0, 0x03a0, 0x03a0, 0x03a0, 0x03a0, 0x03a0, 0x03a0, 0x03a0, 0x0390, 0x0390,
            0x0390, 0x0390, 0x0380, 0x0380, 0x0380, 0x0370, 0x0360, 0x0350, 0x0340, 0x0330, 0x0320,
            0x0310, 0x0300, 0x02f0, 0x02f0, 0x02f0, 0x0300, 0x0310, 0x0330, 0x0350, 0x03c0, 0x0410,
            0x0470, 0x04a0, 0x0460, 0x0440, 0x0450, 0x04e0,
        ],
        _ => return Err(Eac3Error::ReservedSampleRate),
    };
    let mut mask = vec![0_i16; 50];
    for band in bndstrt..bndend {
        let knee = if bndpsd[band] < dbknee {
            (i32::from(dbknee) - i32::from(bndpsd[band])) >> 2
        } else {
            0
        };
        let value = i32::from(excite[band]) + knee;
        mask[band] = i16::try_from(value.max(i32::from(hearing[band]))).unwrap_or(i16::MAX);
    }
    Ok(mask)
}

/// Applies one channel's clause 6.2.2.6 delta-bit-allocation segments.
pub fn apply_delta_bit_allocation(
    mask: &[i16],
    delta: &crate::DeltaBitAllocationElement,
) -> Result<Vec<i16>, Eac3Error> {
    if mask.len() < 50 {
        return Err(Eac3Error::InvalidPsdRange {
            start: mask.len(),
            end: 50,
        });
    }
    if delta.strategy == 2 {
        return Ok(mask.to_vec());
    }
    if delta.strategy != 1 {
        return Err(Eac3Error::InvalidDeltaBitAllocationStrategy {
            actual: delta.strategy,
        });
    }
    let mut adjusted = mask.to_vec();
    let mut band = 0_usize;
    for segment in &delta.segments {
        band = band
            .checked_add(usize::from(segment.offset))
            .ok_or(Eac3Error::FrameSizeOverflow)?;
        let length = usize::from(segment.length);
        let end = band
            .checked_add(length)
            .ok_or(Eac3Error::FrameSizeOverflow)?;
        if end > 50 || segment.delta > 7 {
            return Err(Eac3Error::InvalidPsdRange {
                start: band,
                end: band.saturating_add(length),
            });
        }
        let delta_value = if segment.delta >= 4 {
            (i16::from(segment.delta) - 3) << 7
        } else {
            (i16::from(segment.delta) - 4) << 7
        };
        for value in &mut adjusted[band..end] {
            *value = value.saturating_add(delta_value);
        }
        band = end;
    }
    Ok(adjusted)
}

/// Computes the clause 6.2.2.7 conventional `bap[]` array.
pub fn compute_bap(
    psd: &[i16],
    mask: &[i16],
    start: usize,
    end: usize,
    snroffset: i16,
    floor: i16,
) -> Result<Vec<u8>, Eac3Error> {
    compute_bap_with_pointer(
        psd,
        mask,
        start,
        end,
        snroffset,
        floor,
        bit_allocation_pointer,
    )
}

/// Computes the clause E.2.4.3.1 high-efficiency `hebap[]` array.
///
/// This uses the same PSD, masking, SNR, and floor arithmetic as conventional
/// allocation, but maps each clamped six-bit address through Table E.2.1.
/// The returned five-bit pointers are consumed by the AHT VQ/GAQ mantissa
/// path; they must not be passed to the conventional scalar quantizer.
///
/// # Errors
/// Returns a checked E-AC-3 error for malformed PSD/mask dimensions or an
/// invalid table address.
pub fn compute_high_efficiency_bap(
    psd: &[i16],
    mask: &[i16],
    start: usize,
    end: usize,
    snroffset: i16,
    floor: i16,
) -> Result<Vec<u8>, Eac3Error> {
    compute_bap_with_pointer(
        psd,
        mask,
        start,
        end,
        snroffset,
        floor,
        high_efficiency_bit_allocation_pointer,
    )
}

fn compute_bap_with_pointer(
    psd: &[i16],
    mask: &[i16],
    start: usize,
    end: usize,
    snroffset: i16,
    floor: i16,
    pointer: fn(u8) -> Result<u8, Eac3Error>,
) -> Result<Vec<u8>, Eac3Error> {
    let (bndstrt, bndend) = active_band_range(start, end, psd.len(), mask.len())?;
    if mask.len() < 50 {
        return Err(Eac3Error::InvalidPsdRange { start, end });
    }
    let mut bap = vec![0_u8; psd.len()];
    let mut bin = start;
    for band in bndstrt..bndend {
        let mut adjusted = i32::from(mask[band]) - i32::from(snroffset) - i32::from(floor);
        if adjusted < 0 {
            adjusted = 0;
        }
        adjusted = (adjusted & 0x1fe0) + i32::from(floor);
        let last = (usize::from(bit_allocation_band(band as u8)?.start)
            + usize::from(bit_allocation_band(band as u8)?.size))
        .min(end);
        while bin < last {
            let address = ((i32::from(psd[bin]) - adjusted) >> 5).clamp(0, 63) as u8;
            bap[bin] = pointer(address)?;
            bin += 1;
        }
    }
    Ok(bap)
}
