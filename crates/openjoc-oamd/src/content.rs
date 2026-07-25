// pattern: Functional Core

use crate::OamdError;
use openjoc_bitio::{BitRead, BitReader};

/// Speaker assignment for one bed instance (clauses 5.5.3 and 5.6.1.1).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BedAssignment {
    LfeOnly,
    Standard(u16),
    Nonstandard(u32),
}

/// Normative program-assignment alternatives from clause 5.6.0.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContentDescription {
    DynamicOnly {
        lfe_present: bool,
    },
    Mixed {
        bed_channel_distribute: Option<bool>,
        beds: Vec<BedAssignment>,
        intermediate_spatial_format: Option<u8>,
        dynamic_objects: Option<u16>,
    },
}

/// Fully decoded content-description prefix preceding `oa_element_md` blocks.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OamdContentPrefix {
    pub syntax_version: u8,
    pub object_count: u16,
    pub content: ContentDescription,
    pub alternate_object_data_present: bool,
    pub element_count: u8,
    pub consumed_bits: usize,
}

/// Parses clauses 5.5.2 and 5.5.3 through `oa_element_count_bits`.
///
/// This intentionally names itself as a prefix parser: element bodies are not
/// consumed by this API.
///
/// # Errors
///
/// Returns [`OamdError`] for truncation, reserved ISF values, or overflow.
pub fn parse_oamd_content_prefix(payload: &[u8]) -> Result<OamdContentPrefix, OamdError> {
    let mut reader = BitReader::new(payload);
    parse_oamd_content_prefix_reader(&mut reader)
}

pub(crate) fn parse_oamd_content_prefix_reader(
    reader: &mut BitReader<'_>,
) -> Result<OamdContentPrefix, OamdError> {
    let initial_bits = reader.bits_remaining();

    let mut syntax_version = read_u8(reader, 2)?;
    if syntax_version == 3 {
        syntax_version = syntax_version
            .checked_add(read_u8(reader, 3)?)
            .ok_or(OamdError::ValueOverflow)?;
    }
    let mut object_count_bits = u16::from(read_u8(reader, 5)?);
    if object_count_bits == 31 {
        object_count_bits = object_count_bits
            .checked_add(u16::from(read_u8(reader, 7)?))
            .ok_or(OamdError::ValueOverflow)?;
    }
    let object_count = object_count_bits
        .checked_add(1)
        .ok_or(OamdError::ValueOverflow)?;

    let content = parse_program_assignment(reader)?;
    let alternate_object_data_present = reader.read_bit()?;
    let mut element_count = read_u8(reader, 4)?;
    if element_count == 15 {
        element_count = element_count
            .checked_add(read_u8(reader, 5)?)
            .ok_or(OamdError::ValueOverflow)?;
    }
    Ok(OamdContentPrefix {
        syntax_version,
        object_count,
        content,
        alternate_object_data_present,
        element_count,
        consumed_bits: initial_bits - reader.bits_remaining(),
    })
}

fn parse_program_assignment(reader: &mut BitReader<'_>) -> Result<ContentDescription, OamdError> {
    if reader.read_bit()? {
        return Ok(ContentDescription::DynamicOnly {
            lfe_present: reader.read_bit()?,
        });
    }

    let flags = read_u8(reader, 4)?;
    let (bed_channel_distribute, beds) = if flags & 0b1000 != 0 {
        let distribute = reader.read_bit()?;
        let bed_count = if reader.read_bit()? {
            usize::from(read_u8(reader, 3)?) + 2
        } else {
            1
        };
        let mut beds = Vec::with_capacity(bed_count);
        for _ in 0..bed_count {
            let lfe_only = reader.read_bit()?;
            beds.push(if lfe_only {
                BedAssignment::LfeOnly
            } else if reader.read_bit()? {
                BedAssignment::Standard(read_u16(reader, 10)?)
            } else {
                BedAssignment::Nonstandard(read_u32(reader, 17)?)
            });
        }
        (Some(distribute), beds)
    } else {
        (None, Vec::new())
    };
    let intermediate_spatial_format = if flags & 0b0100 != 0 {
        let index = read_u8(reader, 3)?;
        if index > 5 {
            return Err(OamdError::ReservedIntermediateSpatialFormat { index });
        }
        Some(index)
    } else {
        None
    };
    let dynamic_objects = if flags & 0b0010 != 0 {
        let mut count_bits = u16::from(read_u8(reader, 5)?);
        if count_bits == 31 {
            count_bits = count_bits
                .checked_add(u16::from(read_u8(reader, 7)?))
                .ok_or(OamdError::ValueOverflow)?;
        }
        Some(count_bits + 1)
    } else {
        None
    };
    if flags & 1 != 0 {
        let reserved_bytes = usize::from(read_u8(reader, 4)?) + 1;
        let reserved_bits = reserved_bytes
            .checked_mul(8)
            .ok_or(OamdError::ValueOverflow)?;
        let _ = reader.take_bits(reserved_bits)?;
    }
    Ok(ContentDescription::Mixed {
        bed_channel_distribute,
        beds,
        intermediate_spatial_format,
        dynamic_objects,
    })
}

fn read_u8(reader: &mut impl BitRead, width: u8) -> Result<u8, OamdError> {
    Ok(u8::try_from(reader.read_bits(width)?)?)
}

fn read_u16(reader: &mut impl BitRead, width: u8) -> Result<u16, OamdError> {
    Ok(u16::try_from(reader.read_bits(width)?)?)
}

fn read_u32(reader: &mut impl BitRead, width: u8) -> Result<u32, OamdError> {
    Ok(u32::try_from(reader.read_bits(width)?)?)
}
