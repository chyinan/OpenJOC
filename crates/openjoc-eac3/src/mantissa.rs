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

/// Opt-in trace for one conventional mantissa element.
///
/// The production decoder does not allocate this representation. It is used
/// only by the diagnostic exact-AU history harness to prove raw codeword,
/// grouping, dither, and dequantization provenance without a second cursor.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MantissaDecodeTrace {
    pub raw_codes: Vec<u16>,
    pub grouped: Vec<bool>,
    pub group_positions: Vec<u8>,
    pub dither_values: Vec<f64>,
    pub dequantized: Vec<f64>,
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

/// Carries grouped mantissa state across exponent sets in one audio block.
///
/// TS 102 366 clause 6.3.5 says that a partial bap 1/2/4 group is shared with
/// the next exponent set. The state is reset by the audio-block caller, not by
/// each channel or exponent-set slice.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct MantissaGroupingState {
    values: [[u16; 3]; 16],
    raw_codes: [u16; 16],
    positions: [u8; 16],
}

/// One quantized mantissa value and the packed word that supplied it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MantissaCode {
    pub(crate) value: u16,
    pub(crate) raw_code: u16,
    pub(crate) grouped: bool,
    pub(crate) group_position: u8,
}

impl MantissaGroupingState {
    /// Reads or reuses one mantissa code in clause 6.3.5 frequency order.
    ///
    /// Grouped codewords are tracked independently for bap 1, 2, and 4, so a
    /// different bap may occur between values in the same pending group.
    pub(crate) fn next_code<R: BitRead>(
        &mut self,
        bits: &mut R,
        bap: u8,
    ) -> Result<MantissaCode, Eac3Error> {
        let quantizer = mantissa_quantizer(bap)?;
        if bap == 0 {
            return Ok(MantissaCode {
                value: 0,
                raw_code: 0,
                grouped: false,
                group_position: 0,
            });
        }
        if quantizer.group_size == 1 {
            let raw_code = u16::try_from(bits.read_bits(quantizer.group_bits)?)
                .map_err(|_| Eac3Error::FrameSizeOverflow)?;
            if u32::from(raw_code) >= quantizer.levels {
                return Err(Eac3Error::InvalidMantissaCode {
                    bap,
                    actual: raw_code,
                });
            }
            return Ok(MantissaCode {
                value: raw_code,
                raw_code,
                grouped: false,
                group_position: 0,
            });
        }

        let index = usize::from(bap);
        let group_position = self.positions[index];
        let raw_code = if group_position == 0 {
            let raw_code = u16::try_from(bits.read_bits(quantizer.group_bits)?)
                .map_err(|_| Eac3Error::FrameSizeOverflow)?;
            let max_code = quantizer
                .levels
                .checked_pow(u32::from(quantizer.group_size))
                .ok_or(Eac3Error::FrameSizeOverflow)?;
            if u32::from(raw_code) >= max_code {
                return Err(Eac3Error::InvalidMantissaGroupCode {
                    bap,
                    actual: raw_code,
                });
            }
            let values = ungroup_mantissa_code(bap, raw_code)?;
            for (slot, value) in values.into_iter().enumerate() {
                self.values[index][slot] = value;
            }
            self.raw_codes[index] = raw_code;
            raw_code
        } else {
            // The pending packed word is reused without consuming bits.
            self.raw_codes[index]
        };
        let value = self.values[index][usize::from(group_position)];
        let next_position = group_position + 1;
        self.positions[index] = if next_position == quantizer.group_size {
            0
        } else {
            next_position
        };
        Ok(MantissaCode {
            value,
            raw_code,
            grouped: true,
            group_position,
        })
    }

    /// Advances a copy of this state without reading bits, returning the bit
    /// width and group position for one frequency-ordered mantissa.
    pub(crate) fn next_width(&mut self, bap: u8) -> Result<(u8, u8), Eac3Error> {
        let quantizer = mantissa_quantizer(bap)?;
        if bap == 0 {
            return Ok((0, 0));
        }
        if quantizer.group_size == 1 {
            return Ok((quantizer.group_bits, 0));
        }
        let index = usize::from(bap);
        let group_position = self.positions[index];
        let width = if group_position == 0 {
            quantizer.group_bits
        } else {
            0
        };
        let next_position = group_position + 1;
        self.positions[index] = if next_position == quantizer.group_size {
            0
        } else {
            next_position
        };
        Ok((width, group_position))
    }
}

/// Traverses a channel's mantissa words in frequency order.
///
/// `baps`, `exponents`, and `dither_flags` are parallel arrays. For a dithered
/// bap-zero bin, the next value in `dither_values` is used; the caller owns the
/// random sequence and may choose any sequence allowed by clause 6.3.4.
/// Grouped words are shared across interleaved bap values and exponent-set
/// calls. A final partial group consumes its packed code and ignores dummy
/// values, as required by clause 6.3.5.
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
    let mut grouping = MantissaGroupingState::default();
    decode_mantissas_with_state(
        bits,
        baps,
        exponents,
        dither_flags,
        dither_values,
        &mut grouping,
    )
}

/// Decodes one exponent-set slice while retaining grouped state supplied by
/// the audio-block caller.
pub(crate) fn decode_mantissas_with_state<R: BitRead>(
    bits: &mut R,
    baps: &[u8],
    exponents: &[u8],
    dither_flags: &[bool],
    dither_values: &[f64],
    grouping: &mut MantissaGroupingState,
) -> Result<Vec<f64>, Eac3Error> {
    decode_mantissas_with_state_and_trace(
        bits,
        baps,
        exponents,
        dither_flags,
        dither_values,
        grouping,
        None,
    )
}

pub(crate) fn decode_mantissas_with_state_and_trace<R: BitRead>(
    bits: &mut R,
    baps: &[u8],
    exponents: &[u8],
    dither_flags: &[bool],
    dither_values: &[f64],
    grouping: &mut MantissaGroupingState,
    mut trace: Option<&mut MantissaDecodeTrace>,
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
        if bap == 0 {
            let dither_value = if dither_flags[index] {
                let value = *dither_values
                    .get(dither_index)
                    .ok_or(Eac3Error::MissingDitherValue { index })?;
                dither_index += 1;
                value
            } else {
                0.0
            };
            let dequantized = shift_mantissa(dither_value, exponents[index])?;
            if let Some(trace) = trace.as_deref_mut() {
                trace.raw_codes.push(0);
                trace.grouped.push(false);
                trace.group_positions.push(0);
                trace.dither_values.push(dither_value);
                trace.dequantized.push(dequantized);
            }
            values.push(dequantized);
            index += 1;
            continue;
        }

        let code = grouping.next_code(bits, bap)?;
        let value = decode_mantissa_code(bap, code.value)?;
        let dequantized = shift_mantissa(value, exponents[index])?;
        if let Some(trace) = trace.as_deref_mut() {
            trace.raw_codes.push(code.raw_code);
            trace.grouped.push(code.grouped);
            trace.group_positions.push(code.group_position);
            trace.dither_values.push(0.0);
            trace.dequantized.push(dequantized);
        }
        values.push(dequantized);
        index += 1;
    }
    Ok(values)
}
