// pattern: Functional Core

use std::num::NonZeroU8;

use crate::OamdError;
use openjoc_bitio::{BitRead, BitReader};
use serde::{Deserialize, Serialize};

/// Clause 5.6.5.1 object-Y adjustment applied before rendering.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WarpMode {
    None,
    DoubleY,
}

/// Optional custom controls for one trim configuration.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct TrimControls {
    pub centre_db: Option<f64>,
    pub surround_db: Option<f64>,
    pub height_db: Option<f64>,
    pub top_bottom_y_balance: Option<f64>,
    pub listener_y_balance: Option<f64>,
}

/// Resolved per-configuration trim mode.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub enum TrimConfiguration {
    Default,
    Disabled,
    Custom(TrimControls),
}

/// Clause 5.6.5.2 global trim mode and custom configuration data.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum GlobalTrim {
    Default,
    Disabled,
    Custom(Vec<TrimConfiguration>),
}

/// Decoded clause 5.5.12 trim element.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TrimElement {
    pub warp_mode: WarpMode,
    pub global_trim: GlobalTrim,
    pub disable_trim_per_object: Vec<bool>,
    pub consumed_bits: usize,
}

/// Decodes table 35 centre trim in dB.
///
/// # Errors
///
/// Returns an invalid-property error outside the four-bit domain.
pub fn decode_trim_centre(code: u8) -> Result<f64, OamdError> {
    const VALUES: [f64; 16] = [
        6.0, 3.0, 1.5, 0.75, -0.75, -1.5, -3.0, -4.5, -6.0, -7.5, -9.0, -10.5, -12.0, -13.5, -16.0,
        -36.0,
    ];
    VALUES
        .get(usize::from(code))
        .copied()
        .ok_or(OamdError::InvalidPropertyCode)
}

/// Decodes tables 36 and 37 surround/height trim in dB.
///
/// # Errors
///
/// Returns a reserved-code error for codes 0 through 3 and an
/// invalid-property error outside the four-bit domain.
pub fn decode_trim_surround_or_height(code: u8) -> Result<f64, OamdError> {
    const VALUES: [f64; 12] = [
        -0.75, -1.5, -3.0, -4.5, -6.0, -7.5, -9.0, -10.5, -12.0, -13.5, -16.0, -36.0,
    ];
    if code < 4 {
        return Err(OamdError::ReservedTrimCode { code });
    }
    VALUES
        .get(usize::from(code - 4))
        .copied()
        .ok_or(OamdError::InvalidPropertyCode)
}

/// Decodes clauses 5.6.5.9 through 5.6.5.12 Y-axis balance.
///
/// # Errors
///
/// Returns an invalid-property error outside the one-bit sign or four-bit
/// amount domains.
pub fn decode_y_balance(sign_code: u8, amount: u8) -> Result<f64, OamdError> {
    let sign = match sign_code {
        0 => -1.0,
        1 => 1.0,
        _ => return Err(OamdError::InvalidPropertyCode),
    };
    if amount > 15 {
        return Err(OamdError::InvalidPropertyCode);
    }
    Ok(sign * (f64::from(amount) + 1.0) / 16.0)
}

/// Parses clause 5.5.12 using an explicit trim-configuration cardinality.
///
/// The shared OAMD decoder supplies the normative nine-configuration value;
/// this lower-level function keeps the cardinality explicit for callers that
/// need to exercise an expert override.
///
/// # Errors
///
/// Returns an OAMD error for truncated input and every reserved mode, field,
/// or table code.
pub fn parse_trim_element(
    payload: &[u8],
    object_count: u16,
    trim_configuration_count: NonZeroU8,
) -> Result<TrimElement, OamdError> {
    let mut reader = BitReader::new(payload);
    parse_trim_element_reader(&mut reader, object_count, trim_configuration_count)
}

pub(crate) fn parse_trim_element_reader(
    reader: &mut BitReader<'_>,
    object_count: u16,
    trim_configuration_count: NonZeroU8,
) -> Result<TrimElement, OamdError> {
    let initial_bits = reader.bits_remaining();
    let warp_mode = match read_u8(reader, 2)? {
        0 => WarpMode::None,
        1 => WarpMode::DoubleY,
        code => return Err(OamdError::ReservedWarpMode { code }),
    };
    if reader.read_bits(2)? != 0 {
        return Err(OamdError::NonzeroReservedData);
    }
    let global_trim = match read_u8(reader, 2)? {
        0 => GlobalTrim::Default,
        1 => GlobalTrim::Disabled,
        2 => {
            let mut configurations =
                Vec::with_capacity(usize::from(trim_configuration_count.get()));
            for _ in 0..trim_configuration_count.get() {
                configurations.push(parse_configuration(reader)?);
            }
            GlobalTrim::Custom(configurations)
        }
        3 => return Err(OamdError::ReservedGlobalTrimMode),
        _ => unreachable!(),
    };
    let mut disable_trim_per_object = vec![false; usize::from(object_count)];
    if reader.read_bit()? {
        for disabled in &mut disable_trim_per_object {
            *disabled = reader.read_bit()?;
        }
    }
    Ok(TrimElement {
        warp_mode,
        global_trim,
        disable_trim_per_object,
        consumed_bits: initial_bits - reader.bits_remaining(),
    })
}

fn parse_configuration(reader: &mut impl BitRead) -> Result<TrimConfiguration, OamdError> {
    if reader.read_bit()? {
        return Ok(TrimConfiguration::Default);
    }
    if reader.read_bit()? {
        return Ok(TrimConfiguration::Disabled);
    }
    let presence = read_u8(reader, 5)?;
    Ok(TrimConfiguration::Custom(TrimControls {
        centre_db: if presence & 0b1_0000 != 0 {
            Some(decode_trim_centre(read_u8(reader, 4)?)?)
        } else {
            None
        },
        surround_db: if presence & 0b0_1000 != 0 {
            Some(decode_trim_surround_or_height(read_u8(reader, 4)?)?)
        } else {
            None
        },
        height_db: if presence & 0b0_0100 != 0 {
            Some(decode_trim_surround_or_height(read_u8(reader, 4)?)?)
        } else {
            None
        },
        top_bottom_y_balance: if presence & 0b0_0010 != 0 {
            Some(decode_y_balance(read_u8(reader, 1)?, read_u8(reader, 4)?)?)
        } else {
            None
        },
        listener_y_balance: if presence & 0b0_0001 != 0 {
            Some(decode_y_balance(read_u8(reader, 1)?, read_u8(reader, 4)?)?)
        } else {
            None
        },
    }))
}

fn read_u8(reader: &mut impl BitRead, width: u8) -> Result<u8, OamdError> {
    Ok(u8::try_from(reader.read_bits(width)?)?)
}
