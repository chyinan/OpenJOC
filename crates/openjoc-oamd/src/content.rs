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

/// Speaker-coordinate label from clauses 5.2.1.4 and 5.6.1.1.4–5.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpeakerLabel {
    RcL,
    RcR,
    RcC,
    RcLfe,
    RcLs,
    RcRs,
    RcLb,
    RcRb,
    RcTfl,
    RcTfr,
    RcTsl,
    RcTsr,
    RcTbl,
    RcTbr,
    RcLw,
    RcRw,
    RcLfe2,
}

/// Stacked-ring class in a Table 11b ISF label.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IsfRing {
    Middle,
    Upper,
    Lower,
    Zenith,
}

/// One intermediate-spatial-format coordinate in MULZ order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IsfLabel {
    pub ring: IsfRing,
    pub index: u8,
}

/// Normative position-anchor identity for one content-description object.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObjectAnchor {
    Speaker(SpeakerLabel),
    IntermediateSpatial(IsfLabel),
    Dynamic,
}

impl BedAssignment {
    /// Expands Tables 12 and 13 in the required ascending bit-index order.
    ///
    /// # Errors
    /// Returns an OAMD property error if a manually constructed assignment has
    /// bits outside its normative 10- or 17-bit syntax width.
    pub fn speaker_labels(&self) -> Result<Vec<SpeakerLabel>, OamdError> {
        use SpeakerLabel::{
            RcC, RcL, RcLb, RcLfe, RcLfe2, RcLs, RcLw, RcR, RcRb, RcRs, RcRw, RcTbl, RcTbr, RcTfl,
            RcTfr, RcTsl, RcTsr,
        };
        const STANDARD: [&[SpeakerLabel]; 10] = [
            &[RcLfe2],
            &[RcLw, RcRw],
            &[RcTbl, RcTbr],
            &[RcTsl, RcTsr],
            &[RcTfl, RcTfr],
            &[RcLb, RcRb],
            &[RcLs, RcRs],
            &[RcLfe],
            &[RcC],
            &[RcL, RcR],
        ];
        const NONSTANDARD: [SpeakerLabel; 17] = [
            RcLfe2, RcRw, RcLw, RcTbr, RcTbl, RcTsr, RcTsl, RcTfr, RcTfl, RcRb, RcLb, RcRs, RcLs,
            RcLfe, RcC, RcR, RcL,
        ];
        match self {
            Self::LfeOnly => Ok(vec![RcLfe]),
            Self::Standard(mask) => {
                if mask & !0x03ff != 0 {
                    return Err(OamdError::InvalidPropertyCode);
                }
                Ok(STANDARD
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| mask & (1 << index) != 0)
                    .flat_map(|(_, labels)| labels.iter().copied())
                    .collect())
            }
            Self::Nonstandard(mask) => {
                if mask & !0x1ffff != 0 {
                    return Err(OamdError::InvalidPropertyCode);
                }
                Ok(NONSTANDARD
                    .iter()
                    .copied()
                    .enumerate()
                    .filter_map(|(index, label)| (mask & (1 << index) != 0).then_some(label))
                    .collect())
            }
        }
    }
}

impl OamdContentPrefix {
    /// Expands clauses 5.6.0 and 5.6.4.8 into object-order anchor identities.
    ///
    /// # Errors
    /// Returns an OAMD error for an invalid mask/index or a content-description
    /// count inconsistent with `object_count`.
    pub fn object_anchors(&self) -> Result<Vec<ObjectAnchor>, OamdError> {
        let mut anchors = Vec::with_capacity(usize::from(self.object_count));
        match &self.content {
            ContentDescription::DynamicOnly { lfe_present } => {
                if *lfe_present && self.object_count < 2 {
                    return Err(OamdError::ObjectCountMismatch {
                        declared: self.object_count,
                        described: 2,
                    });
                }
                if *lfe_present {
                    anchors.push(ObjectAnchor::Speaker(SpeakerLabel::RcLfe));
                }
                anchors.resize(usize::from(self.object_count), ObjectAnchor::Dynamic);
            }
            ContentDescription::Mixed {
                beds,
                intermediate_spatial_format,
                dynamic_objects,
                ..
            } => {
                for bed in beds {
                    anchors.extend(bed.speaker_labels()?.into_iter().map(ObjectAnchor::Speaker));
                }
                if let Some(index) = intermediate_spatial_format {
                    anchors.extend(
                        isf_labels(*index)?
                            .into_iter()
                            .map(ObjectAnchor::IntermediateSpatial),
                    );
                }
                anchors.extend(std::iter::repeat_n(
                    ObjectAnchor::Dynamic,
                    usize::from(dynamic_objects.unwrap_or(0)),
                ));
            }
        }
        if anchors.len() != usize::from(self.object_count) {
            return Err(OamdError::ObjectCountMismatch {
                declared: self.object_count,
                described: u16::try_from(anchors.len())?,
            });
        }
        Ok(anchors)
    }
}

fn isf_labels(index: u8) -> Result<Vec<IsfLabel>, OamdError> {
    use IsfRing::{Lower, Middle, Upper, Zenith};
    let counts = match index {
        0 => [3, 1, 0, 0],
        1 => [5, 3, 0, 0],
        2 => [7, 3, 0, 0],
        3 => [9, 5, 0, 0],
        4 => [7, 5, 3, 0],
        5 => [15, 9, 5, 1],
        _ => return Err(OamdError::ReservedIntermediateSpatialFormat { index }),
    };
    let mut labels = Vec::new();
    for (ring, count) in [Middle, Upper, Lower, Zenith].into_iter().zip(counts) {
        labels.extend((1..=count).map(|label_index| IsfLabel {
            ring,
            index: label_index,
        }));
    }
    Ok(labels)
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
