// pattern: Functional Core

//! Clean-room OAMD decoding from ETSI TS 103 420 clause 5.

use openjoc_bitio::{BitError, BitRead};
use std::fmt;

mod basic_properties;
mod content;
mod extended_object;
mod object_element;
mod payload;
mod position;
mod timing;
mod trim;
pub use basic_properties::{
    Extent3, Gain, ZoneConstraint, decode_gain, decode_priority, decode_size,
    decode_zone_constraints,
};
pub use content::{
    BedAssignment, ContentDescription, IsfLabel, IsfRing, OamdContentPrefix, ObjectAnchor,
    SpeakerLabel, parse_oamd_content_prefix,
};
pub use extended_object::{
    ExtendedObjectElement, decode_object_divergence_code, decode_object_divergence_table,
    parse_extended_object_element,
};
pub use object_element::{
    ObjectBasicInfo, ObjectClass, ObjectElement, ObjectRenderInfo, ObjectUpdate,
    parse_object_element,
};
pub use payload::{
    OAMD_PAYLOAD_ID, OamdBitTrace, OamdDecoderConfig, OamdElement, OamdElementBitTrace,
    OamdElementMetadata, OamdParseProfile, OamdPayload, OpaqueBits, OpaqueObservedKnownElement,
    OpaqueVendorContinuation, parse_oamd_payload, parse_oamd_payload_with_config,
    parse_oamd_payload_with_profile, trace_oamd_payload,
};
pub use position::{
    Distance, Position3, PositionCoding, ReferenceScreen, RoomPosition, StandardPositionBits,
    decode_absolute_position, decode_depth_factor, decode_differential_position,
    decode_distance_factor, decode_screen_factor, decode_signed_position_delta,
    interpolate_screen_position, project_room_position,
};
pub use timing::{
    MetadataBlockTiming, MetadataTimelineState, MetadataTiming, TimedMetadataBlock,
    parse_metadata_timing,
};
pub use trim::{
    GlobalTrim, TrimConfiguration, TrimControls, TrimElement, WarpMode, decode_trim_centre,
    decode_trim_surround_or_height, decode_y_balance, parse_trim_element,
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
    /// Reserved OAMD syntax bits were not zero.
    NonzeroReservedData,
    /// A reuse status appeared without a preceding metadata update.
    MissingPreviousObjectUpdate,
    /// A declared object count disagreed with the program assignment.
    ObjectCountMismatch { declared: u16, described: u16 },
    /// An object element used a reserved alternate-data identifier.
    ReservedAlternateObjectData { id: u8 },
    /// A known normative element has not yet been connected to the payload parser.
    UnsupportedKnownElement { id: u8 },
    /// TS 103 420 leaves the symbolic trim-configuration count undefined.
    MissingTrimConfigurationCount,
    /// Table 32 reserves warp modes 2 and 3.
    ReservedWarpMode { code: u8 },
    /// Table 33 reserves global trim mode 3.
    ReservedGlobalTrimMode,
    /// Tables 36 and 37 reserve surround/height codes 0 through 3.
    ReservedTrimCode { code: u8 },
    /// Table 40 reserves divergence mode 3.
    ReservedObjectDivergenceMode,
    /// Table 42 reserves divergence code zero.
    ReservedObjectDivergenceCode,
    /// Divergence reuse appeared in the first object information block.
    MissingPreviousObjectDivergence,
    /// ID 5 dimensions disagreed with the corresponding object element.
    ExtendedObjectShapeMismatch,
    /// An ID 5 element appeared before the object state it extends.
    MissingObjectElementForExtension,
    /// A distance-specified object was coded at the exact room centre.
    UndefinedRoomProjectionDirection,
    /// A finite outside-room distance factor was non-finite or not greater than one.
    InvalidRoomDistanceFactor,
    /// A bounded known element or payload ended with a nonzero padding bit.
    NonzeroPadding,
    /// The vendor OAMD profile was requested for a payload other than ID 11.
    VendorProfilePayloadId { payload_id: u64 },
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
            Self::NonzeroReservedData => formatter.write_str("nonzero reserved OAMD data"),
            Self::MissingPreviousObjectUpdate => {
                formatter.write_str("missing previous OAMD object update")
            }
            Self::ObjectCountMismatch {
                declared,
                described,
            } => write!(
                formatter,
                "OAMD object count mismatch: declared {declared}, described {described}"
            ),
            Self::ReservedAlternateObjectData { id } => {
                write!(
                    formatter,
                    "reserved OAMD alternate object data identifier {id}"
                )
            }
            Self::UnsupportedKnownElement { id } => {
                write!(formatter, "unsupported known OAMD element {id}")
            }
            Self::MissingTrimConfigurationCount => {
                formatter.write_str("missing OAMD trim configuration count")
            }
            Self::ReservedWarpMode { code } => {
                write!(formatter, "reserved OAMD warp mode {code}")
            }
            Self::ReservedGlobalTrimMode => formatter.write_str("reserved OAMD global trim mode"),
            Self::ReservedTrimCode { code } => {
                write!(formatter, "reserved OAMD trim code {code}")
            }
            Self::ReservedObjectDivergenceMode => {
                formatter.write_str("reserved OAMD object divergence mode")
            }
            Self::ReservedObjectDivergenceCode => {
                formatter.write_str("reserved OAMD object divergence code")
            }
            Self::MissingPreviousObjectDivergence => {
                formatter.write_str("missing previous OAMD object divergence")
            }
            Self::ExtendedObjectShapeMismatch => {
                formatter.write_str("OAMD extended object shape mismatch")
            }
            Self::MissingObjectElementForExtension => {
                formatter.write_str("missing OAMD object element for extension")
            }
            Self::UndefinedRoomProjectionDirection => {
                formatter.write_str("undefined OAMD room projection direction")
            }
            Self::InvalidRoomDistanceFactor => {
                formatter.write_str("invalid OAMD room distance factor")
            }
            Self::NonzeroPadding => formatter.write_str("nonzero OAMD padding"),
            Self::VendorProfilePayloadId { payload_id } => write!(
                formatter,
                "Dolby vendor OAMD profile requires payload ID 11, got {payload_id}"
            ),
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
