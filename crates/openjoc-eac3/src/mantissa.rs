// pattern: Functional Core

//! Normative Enhanced AC-3 mantissa expansion and traversal.
//!
//! The tables and equations in this module are from ETSI TS 102 366 V1.4.1,
//! clauses 6.3.2 through 6.3.5 and Tables 6.17 through 6.23. Dither samples
//! are supplied by the caller so this module remains deterministic and does
//! not choose a non-normative random-number generator.

use openjoc_bitio::BitRead;

use crate::Eac3Error;

/// Table 6.17/6.18 quantizer properties for one bap value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MantissaQuantizer {
    /// Number of quantization levels. A zero value denotes bap 0.
    pub levels: u32,
    /// Number of bits occupied by a single word or packed group.
    pub group_bits: u8,
    /// Number of quantized words represented by one packed group.
    pub group_size: u8,
    /// Whether the code indexes Tables 6.19 through 6.23.
    pub symmetric: bool,
}

/// Returns the Table 6.17/6.18 quantizer description for `bap`.
///
/// # Errors
///
/// Returns [`Eac3Error::InvalidMantissaBap`] for values outside 0 through 15.
pub fn mantissa_quantizer(bap: u8) -> Result<MantissaQuantizer, Eac3Error> {
    let quantizer = match bap {
        0 => MantissaQuantizer {
            levels: 0,
            group_bits: 0,
            group_size: 1,
            symmetric: false,
        },
        1 => MantissaQuantizer {
            levels: 3,
            group_bits: 5,
            group_size: 3,
            symmetric: true,
        },
        2 => MantissaQuantizer {
            levels: 5,
            group_bits: 7,
            group_size: 3,
            symmetric: true,
        },
        3 => MantissaQuantizer {
            levels: 7,
            group_bits: 3,
            group_size: 1,
            symmetric: true,
        },
        4 => MantissaQuantizer {
            levels: 11,
            group_bits: 7,
            group_size: 2,
            symmetric: true,
        },
        5 => MantissaQuantizer {
            levels: 15,
            group_bits: 4,
            group_size: 1,
            symmetric: true,
        },
        6 => MantissaQuantizer {
            levels: 32,
            group_bits: 5,
            group_size: 1,
            symmetric: false,
        },
        7 => MantissaQuantizer {
            levels: 64,
            group_bits: 6,
            group_size: 1,
            symmetric: false,
        },
        8 => MantissaQuantizer {
            levels: 128,
            group_bits: 7,
            group_size: 1,
            symmetric: false,
        },
        9 => MantissaQuantizer {
            levels: 256,
            group_bits: 8,
            group_size: 1,
            symmetric: false,
        },
        10 => MantissaQuantizer {
            levels: 512,
            group_bits: 9,
            group_size: 1,
            symmetric: false,
        },
        11 => MantissaQuantizer {
            levels: 1_024,
            group_bits: 10,
            group_size: 1,
            symmetric: false,
        },
        12 => MantissaQuantizer {
            levels: 2_048,
            group_bits: 11,
            group_size: 1,
            symmetric: false,
        },
        13 => MantissaQuantizer {
            levels: 4_096,
            group_bits: 12,
            group_size: 1,
            symmetric: false,
        },
        14 => MantissaQuantizer {
            levels: 16_384,
            group_bits: 14,
            group_size: 1,
            symmetric: false,
        },
        15 => MantissaQuantizer {
            levels: 65_536,
            group_bits: 16,
            group_size: 1,
            symmetric: false,
        },
        actual => return Err(Eac3Error::InvalidMantissaBap { actual }),
    };
    Ok(quantizer)
}

/// Decodes one Table 6.19–6.23 code or one asymmetric two's-complement word.
///
/// The returned value is the fractional mantissa before the exponent right
/// shift described by clause 6.3.2 or 6.3.3.
///
/// # Errors
///
/// Returns a structured error for an invalid bap or code.
pub fn decode_mantissa_code(bap: u8, code: u16) -> Result<f64, Eac3Error> {
    let quantizer = mantissa_quantizer(bap)?;
    if u32::from(code) >= quantizer.levels {
        return Err(Eac3Error::InvalidMantissaCode { bap, actual: code });
    }
    if bap == 0 {
        return Ok(0.0);
    }
    if quantizer.symmetric {
        let midpoint = i64::from(quantizer.levels - 1) / 2;
        let numerator = 2 * (i64::from(code) - midpoint);
        let numerator = i32::try_from(numerator).map_err(|_| Eac3Error::FrameSizeOverflow)?;
        return Ok(f64::from(numerator) / f64::from(quantizer.levels));
    }

    let sign_bit = 1_i32 << (quantizer.group_bits - 1);
    let raw = i32::from(code);
    let signed = if raw & sign_bit != 0 {
        raw - (1_i32 << quantizer.group_bits)
    } else {
        raw
    };
    let denominator = 1_i32 << (quantizer.group_bits - 1);
    Ok(f64::from(signed) / f64::from(denominator))
}

/// Applies the clause 6.3.2/6.3.3 exponent right shift to a fractional value.
///
/// # Errors
///
/// Returns [`Eac3Error::ExponentOutOfRange`] unless `exponent` is in 0 through
/// 24, the range specified for an AC-3 exponent.
pub fn shift_mantissa(value: f64, exponent: u8) -> Result<f64, Eac3Error> {
    if exponent > 24 {
        return Err(Eac3Error::ExponentOutOfRange {
            actual: i16::from(exponent),
        });
    }
    Ok(value / 2_f64.powi(i32::from(exponent)))
}

/// Expands one packed mantissa group in frequency order.
///
/// The equations are the decoder equations printed in clause 6.3.5. A
/// partial group at the end of an exponent set is handled by the traversal
/// function; this function always returns the complete packed group.
///
/// # Errors
///
/// Returns [`Eac3Error::InvalidMantissaGroupCode`] for a group code outside
/// the product of the quantizer levels, or [`Eac3Error::InvalidMantissaBap`]
/// for a non-grouped bap.
pub fn ungroup_mantissa_code(bap: u8, group_code: u16) -> Result<Vec<u16>, Eac3Error> {
    let quantizer = mantissa_quantizer(bap)?;
    let values = match bap {
        1 => {
            if group_code >= 27 {
                return Err(Eac3Error::InvalidMantissaGroupCode {
                    bap,
                    actual: group_code,
                });
            }
            vec![group_code / 9, (group_code % 9) / 3, group_code % 3]
        }
        2 => {
            if group_code >= 125 {
                return Err(Eac3Error::InvalidMantissaGroupCode {
                    bap,
                    actual: group_code,
                });
            }
            vec![group_code / 25, (group_code % 25) / 5, group_code % 5]
        }
        4 => {
            if group_code >= 121 {
                return Err(Eac3Error::InvalidMantissaGroupCode {
                    bap,
                    actual: group_code,
                });
            }
            vec![group_code / 11, group_code % 11]
        }
        _ => {
            return Err(Eac3Error::InvalidMantissaBap { actual: bap });
        }
    };
    debug_assert_eq!(values.len(), usize::from(quantizer.group_size));
    Ok(values)
}

/// Traverses a channel's mantissa words in frequency order.
///
/// `baps`, `exponents`, and `dither_flags` are parallel arrays. For a dithered
/// bap-zero bin, the next value in `dither_values` is used; the caller owns the
/// random sequence and may choose any sequence allowed by clause 6.3.4.
/// Grouped words are kept within contiguous runs of the same bap. A final
/// partial group consumes its packed code and ignores dummy values, as
/// required by clause 6.3.5.
///
/// # Errors
///
/// Returns a structured error for malformed dimensions, invalid exponents or
/// baps, missing dither samples, invalid packed codes, or bitstream truncation.
pub fn decode_mantissas<R: BitRead>(
    bits: &mut R,
    baps: &[u8],
    exponents: &[u8],
    dither_flags: &[bool],
    dither_values: &[f64],
) -> Result<Vec<f64>, Eac3Error> {
    if baps.len() != exponents.len() {
        return Err(Eac3Error::MantissaExponentLengthMismatch {
            baps: baps.len(),
            exponents: exponents.len(),
        });
    }
    if baps.len() != dither_flags.len() {
        return Err(Eac3Error::MantissaDitherLengthMismatch {
            expected: baps.len(),
            actual: dither_flags.len(),
        });
    }
    for &bap in baps {
        let _ = mantissa_quantizer(bap)?;
    }
    for &exponent in exponents {
        if exponent > 24 {
            return Err(Eac3Error::ExponentOutOfRange {
                actual: i16::from(exponent),
            });
        }
    }

    let mut values = Vec::with_capacity(baps.len());
    let mut index = 0;
    let mut dither_index = 0;
    while index < baps.len() {
        let bap = baps[index];
        let quantizer = mantissa_quantizer(bap)?;
        if bap == 0 {
            let value = if dither_flags[index] {
                let value = *dither_values
                    .get(dither_index)
                    .ok_or(Eac3Error::MissingDitherValue { index })?;
                dither_index += 1;
                value
            } else {
                0.0
            };
            values.push(shift_mantissa(value, exponents[index])?);
            index += 1;
            continue;
        }

        let run_end = if quantizer.group_size == 1 {
            index + 1
        } else {
            let mut end = index + 1;
            while end < baps.len() && baps[end] == bap {
                end += 1;
            }
            end
        };
        while index < run_end {
            let code = u16::try_from(bits.read_bits(quantizer.group_bits)?)
                .map_err(|_| Eac3Error::FrameSizeOverflow)?;
            if quantizer.group_size == 1 {
                let value = decode_mantissa_code(bap, code)?;
                values.push(shift_mantissa(value, exponents[index])?);
                index += 1;
            } else {
                let codes = ungroup_mantissa_code(bap, code)?;
                for code in codes {
                    if index == run_end {
                        break;
                    }
                    let value = decode_mantissa_code(bap, code)?;
                    values.push(shift_mantissa(value, exponents[index])?);
                    index += 1;
                }
            }
        }
    }
    Ok(values)
}
