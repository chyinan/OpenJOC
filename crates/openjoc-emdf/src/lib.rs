// pattern: Functional Core

//! Clean-room EMDF decoding from ETSI TS 102 366 Annex H.

use core::fmt;
use openjoc_bitio::{BitError, BitRead, BitReader};

const SYNCWORD: u16 = 0x5838;
const MAX_EXTENDED_GROUPS: u8 = 31;

/// Checked failures while decoding bounded Annex H syntax.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EmdfError {
    /// The underlying bounded reader rejected a field or reached its end.
    Bit(BitError),
    /// The 16-bit synchronization word was not `0x5838`.
    InvalidSyncword { actual: u16 },
    /// Header or declared-container length arithmetic overflowed.
    LengthOverflow,
    /// The input does not contain the complete declared container.
    TruncatedContainer { declared: usize, available: usize },
    /// A variable field was requested with invalid implementation bounds.
    InvalidVariableBits { width: u8, max_groups: u8 },
    /// A variable field did not terminate within its explicit group bound.
    VariableBitsGroupLimit { width: u8, limit: u8 },
    /// Variable-field or extended-identifier arithmetic overflowed `u64`.
    ValueOverflow,
    /// This decoder implements the Annex H base syntax, whose version is zero.
    UnsupportedVersion { version: u64 },
    /// Annex H.2.1.3 requires this reserved bit to be zero.
    NonzeroReservedData,
    /// Annex H base-version payload configuration requires `codecdatae = 0`.
    UnsupportedCodecData,
    /// Table H.2.5 reserves primary protection-length code zero.
    ReservedPrimaryProtectionLength,
    /// Padding through the declared byte boundary was not all zero.
    NonzeroPadding,
    /// More than the partial-byte padding permitted by H.2.2.1.2 remained.
    ExcessPadding { bits: usize },
}

impl fmt::Display for EmdfError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bit(error) => write!(formatter, "failed to read EMDF bitstream: {error}"),
            Self::InvalidSyncword { actual } => {
                write!(formatter, "invalid EMDF syncword 0x{actual:04x}")
            }
            Self::LengthOverflow => formatter.write_str("EMDF length arithmetic overflow"),
            Self::TruncatedContainer {
                declared,
                available,
            } => write!(
                formatter,
                "truncated EMDF container: declared {declared} bytes with {available} available"
            ),
            Self::InvalidVariableBits { width, max_groups } => write!(
                formatter,
                "invalid EMDF variable-bits configuration: width {width}, maximum groups {max_groups}"
            ),
            Self::VariableBitsGroupLimit { width, limit } => write!(
                formatter,
                "EMDF variable-bits({width}) exceeds {limit}-group limit"
            ),
            Self::ValueOverflow => formatter.write_str("EMDF variable-length value overflow"),
            Self::UnsupportedVersion { version } => {
                write!(formatter, "unsupported EMDF syntax version {version}")
            }
            Self::NonzeroReservedData => formatter.write_str("nonzero reserved EMDF data"),
            Self::UnsupportedCodecData => {
                formatter.write_str("unsupported EMDF codec-specific payload configuration")
            }
            Self::ReservedPrimaryProtectionLength => {
                formatter.write_str("reserved EMDF primary protection length")
            }
            Self::NonzeroPadding => formatter.write_str("nonzero EMDF padding"),
            Self::ExcessPadding { bits } => {
                write!(formatter, "excess EMDF byte-boundary padding: {bits} bits")
            }
        }
    }
}

impl std::error::Error for EmdfError {}

impl From<BitError> for EmdfError {
    fn from(value: BitError) -> Self {
        Self::Bit(value)
    }
}

/// Timing, grouping, and transcoding controls for one EMDF payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmdfPayloadConfig {
    pub sample_offset: Option<u16>,
    pub duration: Option<u64>,
    pub group_id: Option<u64>,
    pub discard_unknown_payload: bool,
    pub payload_frame_aligned: Option<bool>,
    pub create_duplicate: Option<bool>,
    pub remove_duplicate: Option<bool>,
    pub priority: Option<u8>,
    pub processing_allowed: Option<u8>,
}

/// One length-bounded payload, including unknown payload identifiers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmdfPayload {
    pub id: u64,
    pub config: EmdfPayloadConfig,
    pub data: Vec<u8>,
}

/// Opaque implementation-defined Annex H protection bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmdfProtection {
    pub primary: Vec<u8>,
    pub secondary: Vec<u8>,
}

/// Complete declared EMDF container.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmdfContainer {
    pub version: u64,
    pub key_id: u64,
    pub payloads: Vec<EmdfPayload>,
    pub protection: EmdfProtection,
}

/// Result of parsing one synchronization header and its declared container.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedEmdf {
    pub container: EmdfContainer,
    /// Header plus declared container bytes; trailing caller data is untouched.
    pub bytes_consumed: usize,
}

/// Decodes clause H.2.1.2.1 using an explicit resource bound.
///
/// # Errors
/// Returns an error for invalid bounds, truncation, unterminated values, or
/// arithmetic overflow.
pub fn variable_bits(
    reader: &mut impl BitRead,
    width: u8,
    max_groups: u8,
) -> Result<u64, EmdfError> {
    if width == 0 || width > 63 || max_groups == 0 {
        return Err(EmdfError::InvalidVariableBits { width, max_groups });
    }

    let mut value = 0_u64;
    for group in 1..=max_groups {
        value = value
            .checked_add(reader.read_bits(width)?)
            .ok_or(EmdfError::ValueOverflow)?;
        if !reader.read_bit()? {
            return Ok(value);
        }
        if group == max_groups {
            return Err(EmdfError::VariableBitsGroupLimit {
                width,
                limit: max_groups,
            });
        }
        value = value
            .checked_shl(u32::from(width))
            .and_then(|shifted| shifted.checked_add(1_u64 << width))
            .ok_or(EmdfError::ValueOverflow)?;
    }
    unreachable!("positive group bound always returns")
}

/// Parses clause H.2.1.1 synchronization data and one byte-bounded container.
///
/// # Errors
/// Returns an error for invalid synchronization, truncated declared data, any
/// malformed conditional field, reserved value, or nonzero byte padding.
pub fn parse_emdf_sync(bytes: &[u8]) -> Result<ParsedEmdf, EmdfError> {
    if bytes.len() < 4 {
        return Err(EmdfError::TruncatedContainer {
            declared: 4,
            available: bytes.len(),
        });
    }
    let syncword = u16::from_be_bytes([bytes[0], bytes[1]]);
    if syncword != SYNCWORD {
        return Err(EmdfError::InvalidSyncword { actual: syncword });
    }
    let declared = usize::from(u16::from_be_bytes([bytes[2], bytes[3]]));
    let end = 4_usize
        .checked_add(declared)
        .ok_or(EmdfError::LengthOverflow)?;
    if end > bytes.len() {
        return Err(EmdfError::TruncatedContainer {
            declared,
            available: bytes.len() - 4,
        });
    }

    let mut reader = BitReader::new(&bytes[4..end]);
    let container = parse_container(&mut reader)?;
    let padding_bits = reader.bits_remaining();
    if padding_bits > 7 {
        return Err(EmdfError::ExcessPadding { bits: padding_bits });
    }
    while reader.bits_remaining() > 0 {
        if reader.read_bit()? {
            return Err(EmdfError::NonzeroPadding);
        }
    }
    Ok(ParsedEmdf {
        container,
        bytes_consumed: end,
    })
}

fn parse_container(reader: &mut impl BitRead) -> Result<EmdfContainer, EmdfError> {
    let version_base = reader.read_bits(2)?;
    let version = extended_value(reader, version_base, 3, 2)?;
    if version != 0 {
        return Err(EmdfError::UnsupportedVersion { version });
    }
    let key_base = reader.read_bits(3)?;
    let key_id = extended_value(reader, key_base, 7, 3)?;
    let mut payloads = Vec::new();
    loop {
        let id_base = reader.read_bits(5)?;
        let id = extended_value(reader, id_base, 31, 5)?;
        if id == 0 {
            break;
        }
        let config = parse_payload_config(reader)?;
        let payload_size = variable_bits(reader, 8, 2)?;
        let payload_size = usize::try_from(payload_size).map_err(|_| EmdfError::ValueOverflow)?;
        let mut data = Vec::with_capacity(payload_size);
        for _ in 0..payload_size {
            data.push(u8::try_from(reader.read_bits(8)?).map_err(|_| EmdfError::ValueOverflow)?);
        }
        payloads.push(EmdfPayload { id, config, data });
    }
    let protection = parse_protection(reader)?;
    Ok(EmdfContainer {
        version,
        key_id,
        payloads,
        protection,
    })
}

fn extended_value(
    reader: &mut impl BitRead,
    base: u64,
    escape: u64,
    width: u8,
) -> Result<u64, EmdfError> {
    if base != escape {
        return Ok(base);
    }
    base.checked_add(variable_bits(reader, width, MAX_EXTENDED_GROUPS)?)
        .ok_or(EmdfError::ValueOverflow)
}

fn parse_payload_config(reader: &mut impl BitRead) -> Result<EmdfPayloadConfig, EmdfError> {
    let sample_offset = if reader.read_bit()? {
        let value = u16::try_from(reader.read_bits(11)?).map_err(|_| EmdfError::ValueOverflow)?;
        if reader.read_bit()? {
            return Err(EmdfError::NonzeroReservedData);
        }
        Some(value)
    } else {
        None
    };
    let duration = if reader.read_bit()? {
        Some(variable_bits(reader, 11, 2)?)
    } else {
        None
    };
    let group_id = if reader.read_bit()? {
        Some(variable_bits(reader, 2, MAX_EXTENDED_GROUPS)?)
    } else {
        None
    };
    if reader.read_bit()? {
        let _reserved = reader.read_bits(8)?;
        return Err(EmdfError::UnsupportedCodecData);
    }

    let discard_unknown_payload = reader.read_bit()?;
    let mut payload_frame_aligned = None;
    let mut create_duplicate = None;
    let mut remove_duplicate = None;
    let mut priority = None;
    let mut processing_allowed = None;
    if !discard_unknown_payload {
        if sample_offset.is_none() {
            let aligned = reader.read_bit()?;
            payload_frame_aligned = Some(aligned);
            if aligned {
                create_duplicate = Some(reader.read_bit()?);
                remove_duplicate = Some(reader.read_bit()?);
            }
        }
        if sample_offset.is_some() || payload_frame_aligned == Some(true) {
            priority =
                Some(u8::try_from(reader.read_bits(5)?).map_err(|_| EmdfError::ValueOverflow)?);
            processing_allowed =
                Some(u8::try_from(reader.read_bits(2)?).map_err(|_| EmdfError::ValueOverflow)?);
        }
    }
    Ok(EmdfPayloadConfig {
        sample_offset,
        duration,
        group_id,
        discard_unknown_payload,
        payload_frame_aligned,
        create_duplicate,
        remove_duplicate,
        priority,
        processing_allowed,
    })
}

fn parse_protection(reader: &mut impl BitRead) -> Result<EmdfProtection, EmdfError> {
    let primary_code = u8::try_from(reader.read_bits(2)?).map_err(|_| EmdfError::ValueOverflow)?;
    let secondary_code =
        u8::try_from(reader.read_bits(2)?).map_err(|_| EmdfError::ValueOverflow)?;
    let primary_len = match primary_code {
        0 => return Err(EmdfError::ReservedPrimaryProtectionLength),
        1 => 1,
        2 => 4,
        3 => 16,
        _ => unreachable!("two-bit field"),
    };
    let secondary_len = match secondary_code {
        0 => 0,
        1 => 1,
        2 => 4,
        3 => 16,
        _ => unreachable!("two-bit field"),
    };
    Ok(EmdfProtection {
        primary: read_octets(reader, primary_len)?,
        secondary: read_octets(reader, secondary_len)?,
    })
}

fn read_octets(reader: &mut impl BitRead, count: usize) -> Result<Vec<u8>, EmdfError> {
    (0..count)
        .map(|_| u8::try_from(reader.read_bits(8)?).map_err(|_| EmdfError::ValueOverflow))
        .collect()
}
