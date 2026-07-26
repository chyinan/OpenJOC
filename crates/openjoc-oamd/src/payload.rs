// pattern: Functional Core

use crate::{
    ExtendedObjectElement, OamdContentPrefix, OamdError, ObjectAnchor, ObjectClass, ObjectElement,
    TrimElement, content::parse_oamd_content_prefix_reader,
    extended_object::parse_extended_object_element_reader,
    object_element::parse_object_element_reader, trim::parse_trim_element_reader,
    variable_bits_max,
};
use openjoc_bitio::{BitRead, BitReader};
use std::num::NonZeroU8;

/// Configuration for standard constants left undefined by TS 103 420 V1.2.1.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OamdDecoderConfig {
    pub trim_configuration_count: Option<NonZeroU8>,
}

/// Lossless representation of an opaque, MSB-first bit sequence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpaqueBits {
    pub bytes: Vec<u8>,
    pub bit_len: usize,
}

/// Normative element bodies currently exposed by the top-level OAMD parser.
#[derive(Clone, Debug, PartialEq)]
pub enum OamdElement {
    Objects(ObjectElement),
    Trim(TrimElement),
    Extended(ExtendedObjectElement),
    Unknown(OpaqueBits),
}

/// Metadata and decoded body for one clause 5.5.4 `oa_element_md` block.
#[derive(Clone, Debug, PartialEq)]
pub struct OamdElementMetadata {
    pub id: u8,
    pub alternate_data_id: Option<u8>,
    pub discard_unknown: bool,
    pub element: OamdElement,
}

/// Complete top-level clause 5.5.2 object-audio metadata payload.
#[derive(Clone, Debug, PartialEq)]
pub struct OamdPayload {
    pub prefix: OamdContentPrefix,
    pub object_classes: Vec<ObjectClass>,
    pub elements: Vec<OamdElementMetadata>,
    pub consumed_bits: usize,
}

/// Parses the top-level OAMD payload and bounded `oa_element_md` windows.
///
/// Object elements are decoded. Unknown element IDs are retained losslessly.
/// Known trim and extended-object IDs return an explicit error until their
/// normative parsers are connected; they are never treated as opaque unknowns.
///
/// # Errors
/// Returns [`OamdError`] for malformed content description, reserved alternate
/// data, size/truncation errors, nonzero padding, or unfinished known elements.
pub fn parse_oamd_payload(payload: &[u8]) -> Result<OamdPayload, OamdError> {
    parse_oamd_payload_with_config(payload, OamdDecoderConfig::default())
}

/// Parses a top-level OAMD payload with explicit undefined standard constants.
///
/// # Errors
///
/// Returns an OAMD error for malformed syntax, bounded-element violations,
/// or a trim element without an explicitly configured cardinality.
pub fn parse_oamd_payload_with_config(
    payload: &[u8],
    config: OamdDecoderConfig,
) -> Result<OamdPayload, OamdError> {
    let mut reader = BitReader::new(payload);
    let initial_bits = reader.bits_remaining();
    let prefix = parse_oamd_content_prefix_reader(&mut reader)?;
    let object_classes = derive_object_classes(&prefix)?;
    let mut elements: Vec<OamdElementMetadata> =
        Vec::with_capacity(usize::from(prefix.element_count));
    for _ in 0..prefix.element_count {
        let id = read_u8(&mut reader, 4)?;
        let size_minus_one = variable_bits_max(&mut reader, 4, 4)?;
        let size_bytes = size_minus_one
            .checked_add(1)
            .ok_or(OamdError::ValueOverflow)?;
        let size_bits = usize::try_from(size_bytes)?
            .checked_mul(8)
            .ok_or(OamdError::ValueOverflow)?;
        let mut element_reader = reader.take_bits(size_bits)?;
        let alternate_data_id = if prefix.alternate_object_data_present {
            Some(read_u8(&mut element_reader, 4)?)
        } else {
            None
        };
        if id == 1 && alternate_data_id.is_some_and(|alternate| alternate != 0) {
            return Err(OamdError::ReservedAlternateObjectData {
                id: alternate_data_id.unwrap_or_default(),
            });
        }
        let discard_unknown = element_reader.read_bit()?;
        let element = match id {
            1 => {
                let objects = parse_object_element_reader(&mut element_reader, &object_classes)?;
                consume_zero_padding(&mut element_reader)?;
                OamdElement::Objects(objects)
            }
            2 => {
                let count = config
                    .trim_configuration_count
                    .ok_or(OamdError::MissingTrimConfigurationCount)?;
                let trim =
                    parse_trim_element_reader(&mut element_reader, prefix.object_count, count)?;
                consume_zero_padding(&mut element_reader)?;
                OamdElement::Trim(trim)
            }
            5 => {
                let object_index = elements
                    .iter()
                    .rposition(|metadata| matches!(metadata.element, OamdElement::Objects(_)))
                    .ok_or(OamdError::MissingObjectElementForExtension)?;
                let extension = {
                    let OamdElement::Objects(objects) = &elements[object_index].element else {
                        unreachable!();
                    };
                    parse_extended_object_element_reader(
                        &mut element_reader,
                        objects,
                        &object_classes,
                    )?
                };
                consume_zero_padding(&mut element_reader)?;
                let OamdElement::Objects(objects) = &mut elements[object_index].element else {
                    unreachable!();
                };
                extension.apply_positions(objects)?;
                OamdElement::Extended(extension)
            }
            _ => OamdElement::Unknown(read_opaque(&mut element_reader)?),
        };
        elements.push(OamdElementMetadata {
            id,
            alternate_data_id,
            discard_unknown,
            element,
        });
    }
    consume_zero_padding(&mut reader)?;
    Ok(OamdPayload {
        prefix,
        object_classes,
        elements,
        consumed_bits: initial_bits,
    })
}

fn derive_object_classes(prefix: &OamdContentPrefix) -> Result<Vec<ObjectClass>, OamdError> {
    Ok(prefix
        .object_anchors()?
        .into_iter()
        .map(|anchor| match anchor {
            ObjectAnchor::Dynamic => ObjectClass::Dynamic,
            ObjectAnchor::Speaker(_) | ObjectAnchor::IntermediateSpatial(_) => {
                ObjectClass::BedOrIsf
            }
        })
        .collect())
}

fn consume_zero_padding(reader: &mut impl BitRead) -> Result<(), OamdError> {
    while reader.bits_remaining() != 0 {
        if reader.read_bit()? {
            return Err(OamdError::NonzeroPadding);
        }
    }
    Ok(())
}

fn read_opaque(reader: &mut impl BitRead) -> Result<OpaqueBits, OamdError> {
    let bit_len = reader.bits_remaining();
    let byte_len = bit_len.checked_add(7).ok_or(OamdError::ValueOverflow)? / 8;
    let mut bytes = vec![0_u8; byte_len];
    for bit_index in 0..bit_len {
        if reader.read_bit()? {
            bytes[bit_index / 8] |= 0x80 >> (bit_index % 8);
        }
    }
    Ok(OpaqueBits { bytes, bit_len })
}

fn read_u8(reader: &mut impl BitRead, width: u8) -> Result<u8, OamdError> {
    Ok(u8::try_from(reader.read_bits(width)?)?)
}
