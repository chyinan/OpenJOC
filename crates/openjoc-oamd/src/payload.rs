// pattern: Functional Core

use crate::{
    ExtendedObjectElement, OamdContentPrefix, OamdError, ObjectAnchor, ObjectClass, ObjectElement,
    TrimElement, content::parse_oamd_content_prefix_reader,
    extended_object::parse_extended_object_element_reader,
    object_element::parse_object_element_reader, trim::parse_trim_element_reader,
    variable_bits_max,
};
use openjoc_bitio::{BitRead, BitReader};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::num::NonZeroU8;

/// EMDF payload identifier required for the observed-vendor compatibility path.
pub const OAMD_PAYLOAD_ID: u64 = 11;

/// Explicit OAMD parser profile. The strict profile is the default and never
/// retains a reserved trim element as decoded metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OamdParseProfile {
    EtsiStrict,
    ObservedVendorCompat,
}

/// ETSI's fixed number of trim configurations.
///
/// TS 103 420 V1.2.1 uses `NUM_TRIM_CONFIGS` in clause 5.5.12 without
/// defining it. The same OAMD trim syntax defines the helper as nine in
/// TS 103 190-2 V1.2.1 clause 6.3.9.10.4.
pub const NUM_TRIM_CONFIGS: u8 = 9;

/// Configuration for OAMD decoding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OamdDecoderConfig {
    pub trim_configuration_count: Option<NonZeroU8>,
}

impl OamdDecoderConfig {
    /// Builds a configuration using the normative count unless an expert
    /// override is supplied.
    #[must_use]
    pub fn with_trim_configuration_count(override_count: Option<NonZeroU8>) -> Self {
        Self {
            trim_configuration_count: override_count.or_else(|| NonZeroU8::new(NUM_TRIM_CONFIGS)),
        }
    }
}

impl Default for OamdDecoderConfig {
    fn default() -> Self {
        Self::with_trim_configuration_count(None)
    }
}

/// Lossless representation of an opaque, MSB-first bit sequence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpaqueBits {
    pub bytes: Vec<u8>,
    pub bit_len: usize,
}

impl OpaqueBits {
    /// Returns one bit from this MSB-first lossless bit sequence.
    #[must_use]
    pub fn bit(&self, index: usize) -> Option<bool> {
        (index < self.bit_len).then(|| self.bytes[index / 8] & (0x80 >> (index % 8)) != 0)
    }
}

/// A lossless view into an already-preserved opaque bit sequence.
///
/// The view does not allocate or copy another byte buffer. Its range is
/// relative to [`Self::source`], so callers can inspect a non-byte-aligned
/// continuation without mistaking the enclosing body bytes for the exact
/// source slice.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OpaqueVendorContinuation<'a> {
    pub source: &'a OpaqueBits,
    pub start_bit: usize,
    pub end_bit: usize,
    pub raw_warp: u8,
    pub provenance: &'static str,
    pub interpretation_status: &'static str,
}

impl OpaqueVendorContinuation<'_> {
    #[must_use]
    pub const fn bit_len(self) -> usize {
        self.end_bit - self.start_bit
    }

    #[must_use]
    pub fn bit(&self, index: usize) -> Option<bool> {
        (index < self.bit_len())
            .then(|| self.source.bit(self.start_bit + index))
            .flatten()
    }
}

/// A known trim element whose declared body is retained without interpreting
/// the unresolved vendor warp syntax.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpaqueObservedKnownElement {
    pub element_id: u8,
    pub alternate_data_id: Option<u8>,
    pub discard_unknown: bool,
    pub declared_bits: usize,
    pub declared_bytes: usize,
    pub valid_bits_in_last_byte: u8,
    pub raw_body: OpaqueBits,
    pub raw_body_sha256: String,
    /// Absolute payload-relative bounds of the validated enclosing body.
    pub body_payload_start_bit: usize,
    pub body_payload_end_bit: usize,
    pub first_parser_error: OamdError,
    pub raw_warp: u8,
    pub warp_element_relative_start_bit: usize,
    pub warp_element_relative_end_bit: usize,
    pub warp_payload_start_bit: usize,
    pub warp_payload_end_bit: usize,
    /// The opaque continuation is a view into `raw_body`, not a second buffer.
    pub continuation_element_relative_start_bit: usize,
    pub continuation_element_relative_end_bit: usize,
    pub continuation_payload_start_bit: usize,
    pub continuation_payload_end_bit: usize,
    pub continuation_sha256: String,
    pub preservation_status: &'static str,
    pub provenance: &'static str,
    pub interpretation_status: &'static str,
    pub deviation_code: &'static str,
}

impl OpaqueObservedKnownElement {
    #[must_use]
    pub fn vendor_continuation(&self) -> OpaqueVendorContinuation<'_> {
        OpaqueVendorContinuation {
            source: &self.raw_body,
            start_bit: self.continuation_element_relative_start_bit,
            end_bit: self.continuation_element_relative_end_bit,
            raw_warp: self.raw_warp,
            provenance: self.provenance,
            interpretation_status: self.interpretation_status,
        }
    }
}

/// Normative element bodies currently exposed by the top-level OAMD parser.
#[derive(Clone, Debug, PartialEq)]
pub enum OamdElement {
    Objects(ObjectElement),
    Trim(TrimElement),
    OpaqueObservedKnownElement(Box<OpaqueObservedKnownElement>),
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

/// Exact top-level bit spans for one bounded OAMD element.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct OamdElementBitTrace {
    pub index: usize,
    pub id: u8,
    pub header_start_bit: usize,
    pub header_end_bit: usize,
    pub body_start_bit: usize,
    pub body_end_bit: usize,
    pub warp_mode_start_bit: Option<usize>,
    pub warp_mode_raw: Option<u8>,
}

/// Exact top-level OAMD bit spans used to diagnose bounded element entry.
///
/// Offsets are relative to the first bit of the OAMD payload. This is a
/// syntax trace only; it does not accept reserved values or change the normal
/// OAMD validator.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct OamdBitTrace {
    pub payload_bits: usize,
    pub prefix_end_bit: usize,
    pub object_count: u16,
    pub element_count: u8,
    pub elements: Vec<OamdElementBitTrace>,
}

/// Traces top-level OAMD element boundaries and the raw trim warp code.
///
/// The function intentionally stops at declared element windows and does not
/// interpret the element body beyond the first two trim warp bits. It is used
/// for forensic evidence when the full OAMD parser rejects a reserved value.
pub fn trace_oamd_payload(payload: &[u8]) -> Result<OamdBitTrace, OamdError> {
    let payload_bits = payload
        .len()
        .checked_mul(8)
        .ok_or(OamdError::ValueOverflow)?;
    let mut reader = BitReader::new(payload);
    let prefix = parse_oamd_content_prefix_reader(&mut reader)?;
    let prefix_end_bit = payload_bits - reader.bits_remaining();
    let mut elements = Vec::with_capacity(usize::from(prefix.element_count));
    for index in 0..usize::from(prefix.element_count) {
        let header_start_bit = payload_bits - reader.bits_remaining();
        let id = read_u8(&mut reader, 4)?;
        let size_minus_one = variable_bits_max(&mut reader, 4, 4)?;
        let size_bytes = size_minus_one
            .checked_add(1)
            .ok_or(OamdError::ValueOverflow)?;
        let size_bits = usize::try_from(size_bytes)?
            .checked_mul(8)
            .ok_or(OamdError::ValueOverflow)?;
        let header_end_bit = payload_bits - reader.bits_remaining();
        let body_start_bit = header_end_bit;
        let mut body_reader = reader.take_bits(size_bits)?;
        if prefix.alternate_object_data_present {
            let _alternate_data_id = body_reader.read_bits(4)?;
        }
        let _discard_unknown = body_reader.read_bit()?;
        let (warp_mode_start_bit, warp_mode_raw) = if id == 2 {
            let body_prefix_bits = usize::from(prefix.alternate_object_data_present) * 4 + 1;
            let warp_mode_start_bit = body_start_bit + body_prefix_bits;
            let warp_mode_raw = read_u8(&mut body_reader, 2)?;
            (Some(warp_mode_start_bit), Some(warp_mode_raw))
        } else {
            (None, None)
        };
        let body_end_bit = payload_bits - reader.bits_remaining();
        elements.push(OamdElementBitTrace {
            index,
            id,
            header_start_bit,
            header_end_bit,
            body_start_bit,
            body_end_bit,
            warp_mode_start_bit,
            warp_mode_raw,
        });
    }
    Ok(OamdBitTrace {
        payload_bits,
        prefix_end_bit,
        object_count: prefix.object_count,
        element_count: prefix.element_count,
        elements,
    })
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

/// Parses a top-level OAMD payload with an explicit decoder configuration.
///
/// # Errors
///
/// Returns an OAMD error for malformed syntax, bounded-element violations,
/// or a trim element when the supplied configuration intentionally has no
/// cardinality.
pub fn parse_oamd_payload_with_config(
    payload: &[u8],
    config: OamdDecoderConfig,
) -> Result<OamdPayload, OamdError> {
    parse_oamd_payload_inner(payload, config, OamdParseProfile::EtsiStrict, None)
}

/// Parses OAMD using an explicit profile and EMDF payload identifier.
///
/// The observed-vendor profile is deliberately narrow: it only retains a complete
/// element-2 trim body opaquely when the bounded, formal trim parser's first
/// error is reserved warp value 3. It does not remap or otherwise interpret
/// that value. The payload identifier is required so callers cannot enable the
/// observed fallback for an unrelated EMDF payload.
pub fn parse_oamd_payload_with_profile(
    payload: &[u8],
    config: OamdDecoderConfig,
    profile: OamdParseProfile,
    payload_id: u64,
) -> Result<OamdPayload, OamdError> {
    if profile == OamdParseProfile::ObservedVendorCompat && payload_id != OAMD_PAYLOAD_ID {
        return Err(OamdError::VendorProfilePayloadId { payload_id });
    }
    parse_oamd_payload_inner(payload, config, profile, Some(payload_id))
}

fn parse_oamd_payload_inner(
    payload: &[u8],
    config: OamdDecoderConfig,
    profile: OamdParseProfile,
    payload_id: Option<u64>,
) -> Result<OamdPayload, OamdError> {
    let payload_bits = payload
        .len()
        .checked_mul(8)
        .ok_or(OamdError::ValueOverflow)?;
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
        let header_end_bit = payload_bits - reader.bits_remaining();
        let body_start_bit = header_end_bit;
        let mut element_reader = reader.take_bits(size_bits)?;
        let body_end_bit = payload_bits - reader.bits_remaining();
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
                match parse_trim_element_reader(&mut element_reader, prefix.object_count, count) {
                    Ok(trim) => {
                        consume_zero_padding(&mut element_reader)?;
                        OamdElement::Trim(trim)
                    }
                    Err(error @ OamdError::ReservedWarpMode { code: 3 })
                        if profile == OamdParseProfile::ObservedVendorCompat
                            && payload_id == Some(OAMD_PAYLOAD_ID)
                            && alternate_data_id.is_none_or(|alternate| alternate == 0) =>
                    {
                        let warp_relative_start =
                            usize::from(prefix.alternate_object_data_present) * 4 + 1;
                        let warp_relative_end = warp_relative_start + 2;
                        if warp_relative_end > size_bits {
                            return Err(error);
                        }
                        let warp_start = body_start_bit + warp_relative_start;
                        let raw_warp = read_window_u8(payload, warp_start, 2)?;
                        if raw_warp != 3 {
                            return Err(error);
                        }
                        let raw_body = copy_bit_window(payload, body_start_bit, body_end_bit)?;
                        let valid_bits_in_last_byte = if raw_body.bit_len % 8 == 0 {
                            8
                        } else {
                            u8::try_from(raw_body.bit_len % 8)?
                        };
                        let raw_body_sha256 = sha256_hex(&raw_body.bytes);
                        let continuation_element_relative_start_bit = warp_relative_end;
                        let continuation_element_relative_end_bit = size_bits;
                        let continuation_payload_start_bit = warp_start + 2;
                        let continuation_payload_end_bit = body_end_bit;
                        let continuation_sha256 = sha256_bit_window(
                            &raw_body,
                            continuation_element_relative_start_bit,
                            continuation_element_relative_end_bit,
                        )?;
                        OamdElement::OpaqueObservedKnownElement(Box::new(
                            OpaqueObservedKnownElement {
                                element_id: id,
                                alternate_data_id,
                                discard_unknown,
                                declared_bits: size_bits,
                                declared_bytes: usize::try_from(size_bytes)?,
                                valid_bits_in_last_byte,
                                raw_body,
                                raw_body_sha256,
                                body_payload_start_bit: body_start_bit,
                                body_payload_end_bit: body_end_bit,
                                first_parser_error: error,
                                raw_warp,
                                warp_element_relative_start_bit: warp_relative_start,
                                warp_element_relative_end_bit: warp_relative_end,
                                warp_payload_start_bit: warp_start,
                                warp_payload_end_bit: warp_start + 2,
                                continuation_element_relative_start_bit,
                                continuation_element_relative_end_bit,
                                continuation_payload_start_bit,
                                continuation_payload_end_bit,
                                continuation_sha256,
                                preservation_status: "opaque_lossless_bounded",
                                provenance: "vendor_observed_normative_unresolved",
                                interpretation_status: "unresolved",
                                deviation_code: "LOGIC_OAMD_RESERVED_TRIM_WARP_3",
                            },
                        ))
                    }
                    Err(error) => return Err(error),
                }
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

fn read_window_u8(payload: &[u8], start_bit: usize, width: u8) -> Result<u8, OamdError> {
    let end_bit = start_bit
        .checked_add(usize::from(width))
        .ok_or(OamdError::ValueOverflow)?;
    let total_bits = payload
        .len()
        .checked_mul(8)
        .ok_or(OamdError::ValueOverflow)?;
    if end_bit > total_bits || width > 8 {
        return Err(OamdError::Bit(openjoc_bitio::BitError::EndOfInput {
            requested: usize::from(width),
            remaining: total_bits.saturating_sub(start_bit),
        }));
    }
    let mut value = 0_u8;
    for bit in start_bit..end_bit {
        value = (value << 1) | u8::from(payload[bit / 8] & (0x80 >> (bit % 8)) != 0);
    }
    Ok(value)
}

fn copy_bit_window(
    payload: &[u8],
    start_bit: usize,
    end_bit: usize,
) -> Result<OpaqueBits, OamdError> {
    if end_bit < start_bit {
        return Err(OamdError::ValueOverflow);
    }
    let bit_len = end_bit - start_bit;
    let total_bits = payload
        .len()
        .checked_mul(8)
        .ok_or(OamdError::ValueOverflow)?;
    if end_bit > total_bits {
        return Err(OamdError::Bit(openjoc_bitio::BitError::EndOfInput {
            requested: bit_len,
            remaining: total_bits.saturating_sub(start_bit),
        }));
    }
    let byte_len = bit_len.checked_add(7).ok_or(OamdError::ValueOverflow)? / 8;
    let mut bytes = vec![0_u8; byte_len];
    for offset in 0..bit_len {
        if payload[(start_bit + offset) / 8] & (0x80 >> ((start_bit + offset) % 8)) != 0 {
            bytes[offset / 8] |= 0x80 >> (offset % 8);
        }
    }
    Ok(OpaqueBits { bytes, bit_len })
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(64);
    for byte in digest {
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn sha256_bit_window(
    bits: &OpaqueBits,
    start_bit: usize,
    end_bit: usize,
) -> Result<String, OamdError> {
    if end_bit < start_bit || end_bit > bits.bit_len {
        return Err(OamdError::ValueOverflow);
    }
    let window = copy_bit_window(&bits.bytes, start_bit, end_bit)?;
    let mut hasher = Sha256::new();
    hasher.update((window.bit_len as u64).to_be_bytes());
    hasher.update(window.bytes);
    let digest = hasher.finalize();
    let mut output = String::with_capacity(64);
    for byte in digest {
        let _ = write!(output, "{byte:02x}");
    }
    Ok(output)
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
