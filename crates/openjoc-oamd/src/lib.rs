// pattern: Functional Core

//! Clean-room OAMD decoding from ETSI TS 103 420 clause 5.

use openjoc_bitio::{BitError, BitRead};
use std::fmt;

mod basic_properties;
mod content;
mod timing;
pub use basic_properties::{
    Extent3, Gain, ZoneConstraint, decode_gain, decode_priority, decode_size,
    decode_zone_constraints,
};
pub use content::{
    BedAssignment, ContentDescription, OamdContentPrefix, parse_oamd_content_prefix,
};
pub use timing::{
    MetadataBlockTiming, MetadataTimelineState, MetadataTiming, TimedMetadataBlock,
    parse_metadata_timing,
};

/// Checked failures while decoding OAMD syntax.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OamdError {
    /// The underlying bounded bitstream ended or rejected a width.
    Bit(BitError),
    /// Clause 5.5.1 requires a positive width and group limit.
    InvalidVariableBits { width: u8, max_groups: u8 },
    /// The decoded variable-length integer cannot be represented by `u64`.
    ValueOverflow,
    /// ISF table 11b reserves indices 6 and 7.
    ReservedIntermediateSpatialFormat { index: u8 },
    /// Clause 5.6.2.1 reserves sample-offset code 3.
    ReservedSampleOffsetCode,
    /// Adding the normative 1,536-sample codec-frame size overflowed.
    FrameOffsetOverflow,
    /// A conditionally required gain codeword was absent.
    MissingGainBits,
    /// A conditionally required priority codeword was absent.
    MissingPriorityBits,
    /// A property code was outside its normative bit-width/table domain.
    InvalidPropertyCode,
    /// Conditional object-size fields were absent.
    MissingSizeBits,
    /// Table 17 reserves object-size index 3.
    ReservedSizeIndex,
    /// Table 20 reserves zone indices 6 and 7.
    ReservedZoneIndex { index: u8 },
}

impl fmt::Display for OamdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bit(error) => write!(formatter, "failed to read OAMD bitstream: {error}"),
            Self::InvalidVariableBits { width, max_groups } => write!(
                formatter,
                "invalid OAMD variable-bits configuration: width {width}, maximum groups {max_groups}"
            ),
            Self::ValueOverflow => formatter.write_str("OAMD variable-length value overflow"),
            Self::ReservedIntermediateSpatialFormat { index } => {
                write!(
                    formatter,
                    "reserved OAMD intermediate spatial format {index}"
                )
            }
            Self::ReservedSampleOffsetCode => {
                formatter.write_str("reserved OAMD sample offset code 3")
            }
            Self::FrameOffsetOverflow => formatter.write_str("OAMD frame offset overflow"),
            Self::MissingGainBits => formatter.write_str("missing OAMD object gain bits"),
            Self::MissingPriorityBits => formatter.write_str("missing OAMD object priority bits"),
            Self::InvalidPropertyCode => formatter.write_str("invalid OAMD property code"),
            Self::MissingSizeBits => formatter.write_str("missing OAMD object size bits"),
            Self::ReservedSizeIndex => formatter.write_str("reserved OAMD object size index 3"),
            Self::ReservedZoneIndex { index } => {
                write!(formatter, "reserved OAMD zone constraint index {index}")
            }
        }
    }
}

impl std::error::Error for OamdError {}

impl From<BitError> for OamdError {
    fn from(value: BitError) -> Self {
        Self::Bit(value)
    }
}

impl From<std::num::TryFromIntError> for OamdError {
    fn from(_: std::num::TryFromIntError) -> Self {
        Self::ValueOverflow
    }
}

/// Decodes TS 103 420 clause 5.5.1 `variable_bits_max`.
///
/// # Errors
///
/// Returns [`OamdError`] for invalid bounds, truncation, or arithmetic overflow.
pub fn variable_bits_max(
    reader: &mut impl BitRead,
    width: u8,
    max_groups: u8,
) -> Result<u64, OamdError> {
    if width == 0 || width > 63 || max_groups == 0 {
        return Err(OamdError::InvalidVariableBits { width, max_groups });
    }

    let mut value = reader.read_bits(width)?;
    let mut read_more = reader.read_bit()?;
    let mut groups = 1_u8;
    if max_groups > groups && read_more {
        value = continue_value(value, width)?;
        while read_more {
            value = value
                .checked_add(reader.read_bits(width)?)
                .ok_or(OamdError::ValueOverflow)?;
            read_more = reader.read_bit()?;
            groups += 1;
            if groups >= max_groups {
                break;
            }
            if read_more {
                value = continue_value(value, width)?;
            }
        }
    }
    Ok(value)
}

fn continue_value(value: u64, width: u8) -> Result<u64, OamdError> {
    value
        .checked_shl(u32::from(width))
        .and_then(|shifted| shifted.checked_add(1_u64 << width))
        .ok_or(OamdError::ValueOverflow)
}
