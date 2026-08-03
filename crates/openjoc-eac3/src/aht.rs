// pattern: Functional Core

//! Pure Adaptive Hybrid Transform support primitives.

use crate::Eac3Error;

/// Expands the E.2.3 GAQ gain words into one gain per six-bin DCT section.
///
/// Mode 0 has no transmitted words and assigns gain one. Modes 1 and 2 use
/// one bit per active section and map zero to gain one and one to gain two or
/// four respectively. Mode 3 packs three mapped gain states into each five-
/// bit word as specified by E.2.4.4.2.
///
/// # Errors
/// Returns an error for a reserved mode, a gain word outside its transmitted
/// width, or a word count that cannot cover `sections`.
pub fn expand_aht_gaq_gains(mode: u8, words: &[u8], sections: usize) -> Result<Vec<u8>, Eac3Error> {
    match mode {
        0 => {
            if !words.is_empty() {
                return Err(Eac3Error::FrameSizeOverflow);
            }
            Ok(vec![1; sections])
        }
        1 | 2 => {
            if words.len() != sections {
                return Err(Eac3Error::FrameSizeOverflow);
            }
            let amplified_gain = if mode == 1 { 2 } else { 4 };
            words
                .iter()
                .copied()
                .map(|word| match word {
                    0 => Ok(1),
                    1 => Ok(amplified_gain),
                    actual => Err(Eac3Error::InvalidAhtGaqGainWord { actual }),
                })
                .collect()
        }
        3 => {
            let required_words = sections
                .checked_add(2)
                .ok_or(Eac3Error::FrameSizeOverflow)?
                / 3;
            if words.len() != required_words {
                return Err(Eac3Error::FrameSizeOverflow);
            }
            let mut gains = Vec::with_capacity(sections);
            for word in words {
                if *word > 26 {
                    return Err(Eac3Error::InvalidAhtGaqGainWord { actual: *word });
                }
                let first = *word / 9;
                let second = (*word % 9) / 3;
                let third = *word % 3;
                for mapped in [first, second, third] {
                    if gains.len() == sections {
                        break;
                    }
                    gains.push(match mapped {
                        0 => 1,
                        1 => 2,
                        2 => 4,
                        _ => unreachable!("three-state GAQ mapping"),
                    });
                }
            }
            if gains.len() != sections {
                return Err(Eac3Error::FrameSizeOverflow);
            }
            Ok(gains)
        }
        actual => Err(Eac3Error::InvalidAhtGaqMode { actual }),
    }
}

/// Decodes one scalar GAQ mantissa code after any large-mantissa tag has been
/// consumed. The code is interpreted as a signed two's-complement fraction,
/// then remapped with Table E.2.6 where required. `large` selects the large
/// quantizer for gains two and four; gain one has only the single quantizer.
///
/// # Errors
/// Returns an error when `hebap`, `gain`, or the codeword is outside the
/// quantizer domain, or when Table E.2.6 marks the requested remapping N/A.
pub fn decode_aht_gaq_mantissa(
    hebap: u8,
    gain: u8,
    large: bool,
    code: u16,
) -> Result<f64, Eac3Error> {
    let bits = aht_gaq_code_bits(hebap, gain, large)?;
    let code_limit = 1_u32 << bits;
    if u32::from(code) >= code_limit {
        return Err(Eac3Error::InvalidAhtGaqCode { actual: code });
    }
    let value = signed_fraction(code, bits);
    if gain == 1 {
        return remap_aht_gaq(hebap, gain, value);
    }
    if large {
        remap_aht_gaq(hebap, gain, value)
    } else {
        Ok(value / f64::from(gain))
    }
}

fn aht_gaq_code_bits(hebap: u8, gain: u8, large: bool) -> Result<u8, Eac3Error> {
    let mantissa_bits = match hebap {
        8..=16 => hebap - 5,
        17 => 12,
        18 => 14,
        19 => 16,
        actual => return Err(Eac3Error::InvalidAhtGaqHebap { actual }),
    };
    match gain {
        1 => Ok(mantissa_bits),
        2 => {
            if hebap > 16 {
                return Err(Eac3Error::InvalidAhtGaqGain { actual: gain });
            }
            Ok(mantissa_bits - 1)
        }
        4 => {
            if hebap > 16 {
                return Err(Eac3Error::InvalidAhtGaqGain { actual: gain });
            }
            if large {
                Ok(mantissa_bits)
            } else {
                Ok(mantissa_bits - 2)
            }
        }
        actual => Err(Eac3Error::InvalidAhtGaqGain { actual }),
    }
}

fn signed_fraction(code: u16, bits: u8) -> f64 {
    let sign = 1_i32 << (bits - 1);
    let raw = i32::from(code);
    let signed = if raw & sign != 0 {
        raw - (1_i32 << bits)
    } else {
        raw
    };
    f64::from(signed) / f64::from(1_i32 << (bits - 1))
}

fn remap_aht_gaq(hebap: u8, gain: u8, value: f64) -> Result<f64, Eac3Error> {
    const G1_A: [u16; 12] = [
        0x1249, 0x0889, 0x0421, 0x0208, 0x0102, 0x0081, 0x0040, 0x0020, 0x0010, 0x0008, 0x0002,
        0x0000,
    ];
    const G2_A: [u16; 9] = [
        0xd555, 0xc925, 0xc444, 0xc211, 0xc104, 0xc081, 0xc040, 0xc020, 0xc010,
    ];
    const G2_B_POS: [u16; 9] = [
        0x4000, 0x4000, 0x4000, 0x4000, 0x4000, 0x4000, 0x4000, 0x4000, 0x4000,
    ];
    const G2_B_NEG: [u16; 9] = [
        0xeaab, 0xd249, 0xc889, 0xc421, 0xc208, 0xc102, 0xc081, 0xc040, 0xc020,
    ];
    const G4_A: [u16; 9] = [
        0xedb7, 0xe666, 0xe319, 0xe186, 0xe0c2, 0xe060, 0xe030, 0xe018, 0xe00c,
    ];
    const G4_B_POS: [u16; 9] = [
        0x2000, 0x2000, 0x2000, 0x2000, 0x2000, 0x2000, 0x2000, 0x2000, 0x2000,
    ];
    const G4_B_NEG: [u16; 9] = [
        0xfb6e, 0xeccd, 0xe632, 0xe30c, 0xe183, 0xe0c1, 0xe060, 0xe030, 0xe018,
    ];
    let index = usize::from(hebap - 8);
    let (a, b) = match gain {
        1 => (G1_A[index], 0),
        2 if index < G2_A.len() => (
            G2_A[index],
            if value >= 0.0 {
                G2_B_POS[index]
            } else {
                G2_B_NEG[index]
            },
        ),
        4 if index < G4_A.len() => (
            G4_A[index],
            if value >= 0.0 {
                G4_B_POS[index]
            } else {
                G4_B_NEG[index]
            },
        ),
        _ => return Err(Eac3Error::InvalidAhtGaqGain { actual: gain }),
    };
    let a = signed_fraction(a, 16);
    let b = signed_fraction(b, 16);
    Ok(value + a * value + b)
}
