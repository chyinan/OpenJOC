// pattern: Functional Core

//! Clean-room EMDF decoding from ETSI TS 102 366 Annex H.

use core::fmt;
use openjoc_bitio::{BitError, BitRead, BitReader};

const SYNCWORD: u16 = 0x5838;
const MAX_EXTENDED_GROUPS: u8 = 31;

/// TS 103 420 table 55 OAMD payload identifier.
pub const OAMD_PAYLOAD_ID: u64 = 11;
/// TS 103 420 table 55 JOC payload identifier.
pub const JOC_PAYLOAD_ID: u64 = 14;

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
    /// Table H.2.5 reserves primary protection-length code zero.
    ReservedPrimaryProtectionLength,
    /// Padding through the declared byte boundary was not all zero.
    NonzeroPadding,
    /// More than the partial-byte padding permitted by H.2.2.1.2 remained.
    ExcessPadding { bits: usize },
    /// Table 55 requires exactly one OAMD and one JOC payload per frame.
    JocProfilePayloadCount { oamd: usize, joc: usize },
    /// An OAMD/JOC payload violates table 56 or uses a different group ID.
    JocProfileConfiguration,
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
            Self::ReservedPrimaryProtectionLength => {
                formatter.write_str("reserved EMDF primary protection length")
            }
            Self::NonzeroPadding => formatter.write_str("nonzero EMDF padding"),
            Self::ExcessPadding { bits } => {
                write!(formatter, "excess EMDF byte-boundary padding: {bits} bits")
            }
            Self::JocProfilePayloadCount { oamd, joc } => write!(
                formatter,
                "invalid JOC-profile payload count: {oamd} OAMD and {joc} JOC"
            ),
            Self::JocProfileConfiguration => {
                formatter.write_str("invalid JOC-profile EMDF payload configuration")
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
    /// The reserved codec-data octet was present and verified as zero.
    pub codec_data_present: bool,
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

/// Exact bit spans for one payload in a declared EMDF container.
///
/// All offsets are relative to the first bit of the EMDF carrier, including
/// the 32-bit synchronization header. The trace is observational only and
/// does not alter the parsed payload representation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmdfPayloadBitTrace {
    pub payload_id: u64,
    pub payload_id_start_bit: usize,
    pub payload_id_end_bit: usize,
    pub config_start_bit: usize,
    pub config_end_bit: usize,
    pub payload_size_start_bit: usize,
    pub payload_size_end_bit: usize,
    pub payload_body_start_bit: usize,
    pub payload_body_end_bit: usize,
}

/// Parsed EMDF plus exact payload/config bit spans.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedEmdfBitTrace {
    pub parsed: ParsedEmdf,
    pub payloads: Vec<EmdfPayloadBitTrace>,
}

/// Classification of one caller-declared carrier range.
///
/// The classifier deliberately examines only the first two bits-as-bytes of
/// the supplied range. It never searches later bytes for an EMDF syncword and
/// it never combines separate ranges. `TrailingData` is kept distinct from a
/// successful parse because Annex H defines the container length, while the
/// E-AC-3 carrier syntax does not grant this API an implicit padding rule.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CarrierClassification {
    /// The exact carrier start does not contain the Annex H syncword.
    NonEmdf,
    /// The declared range is exactly one complete EMDF container.
    Parsed(ParsedEmdf),
    /// The exact start has the EMDF syncword, but bounded Annex H parsing
    /// failed within the declared range.
    Malformed(EmdfError),
    /// A complete container ended before the declared carrier range ended.
    /// No unmentioned carrier-padding rule is assumed here.
    TrailingData {
        container_bytes: usize,
        carrier_bytes: usize,
    },
}

/// Table 55 payload bytes after all table 56 restrictions are validated.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JocPayloadPair<'a> {
    pub oamd: &'a [u8],
    pub joc: &'a [u8],
}

/// Explicit validation policy applied after bounded EMDF parsing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JocValidationProfile {
    /// Published TS 103 420 tables 55 and 56 without interoperability exceptions.
    EtsiStrict,
    /// Explicitly observed producer signaling patterns with every ETSI deviation retained.
    ObservedVendorCompat,
}

impl JocValidationProfile {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EtsiStrict => "ETSI_STRICT",
            Self::ObservedVendorCompat => "OBSERVED_VENDOR_COMPAT",
        }
    }
}

impl fmt::Display for JocValidationProfile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Successful validation outcome. Observed-vendor validation reports normative
/// input separately from input accepted only through a documented deviation set.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JocValidationStatus {
    NormativeCompliant,
    AcceptedWithDeviation,
}

impl JocValidationStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NormativeCompliant => "normative_compliant",
            Self::AcceptedWithDeviation => "accepted_with_deviation",
        }
    }
}

/// One TS 103 420 table 56 field retained in validation evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JocProfileField {
    SampleOffset,
    Duration,
    GroupId,
    CodecDataPresent,
    DiscardUnknownPayload,
    PayloadFrameAligned,
    CreateDuplicate,
    RemoveDuplicate,
    Priority,
    ProcessingAllowed,
}

impl JocProfileField {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SampleOffset => "smploffste",
            Self::Duration => "duratione",
            Self::GroupId => "groupid",
            Self::CodecDataPresent => "codecdatae",
            Self::DiscardUnknownPayload => "discard_unknown_payload",
            Self::PayloadFrameAligned => "payload_frame_aligned",
            Self::CreateDuplicate => "create_duplicate",
            Self::RemoveDuplicate => "remove_duplicate",
            Self::Priority => "priority",
            Self::ProcessingAllowed => "proc_allowed",
        }
    }
}

impl fmt::Display for JocProfileField {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Typed actual/expected value used by profile evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JocProfileValue {
    Absent,
    Present,
    Bool(bool),
    Unsigned(u64),
}

impl fmt::Display for JocProfileValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Absent => formatter.write_str("absent"),
            Self::Present => formatter.write_str("present"),
            Self::Bool(value) => formatter.write_str(if *value { "1" } else { "0" }),
            Self::Unsigned(value) => write!(formatter, "{value}"),
        }
    }
}

/// One observed field value that differs from the published ETSI profile.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JocProfileDeviation {
    pub payload_id: u64,
    pub field: JocProfileField,
    pub actual: JocProfileValue,
    pub expected_by_etsi: JocProfileValue,
}

/// Evidence returned when a selected validation profile rejects a parsed
/// container. Parsing has already succeeded and no source fields are changed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JocProfileValidationFailure {
    pub profile: JocValidationProfile,
    pub oamd_payload_count: usize,
    pub joc_payload_count: usize,
    pub deviations: Vec<JocProfileDeviation>,
}

impl fmt::Display for JocProfileValidationFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} validation failed", self.profile)?;
        if self.oamd_payload_count != 1 || self.joc_payload_count != 1 {
            return write!(
                formatter,
                ": expected one OAMD payload and one JOC payload, found {} and {}",
                self.oamd_payload_count, self.joc_payload_count
            );
        }
        for (index, deviation) in self.deviations.iter().enumerate() {
            formatter.write_str(if index == 0 { ": " } else { "; " })?;
            write!(
                formatter,
                "payload {} {}={} where ETSI requires {}",
                deviation.payload_id, deviation.field, deviation.actual, deviation.expected_by_etsi
            )?;
        }
        Ok(())
    }
}

impl std::error::Error for JocProfileValidationFailure {}

/// References into the original parsed container after an explicit profile
/// accepts it. Configuration fields remain unchanged and deviations are
/// carried alongside the payload bytes for reporting and decoder provenance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedJocProfile<'a> {
    pub profile: JocValidationProfile,
    pub status: JocValidationStatus,
    pub oamd: &'a EmdfPayload,
    pub joc: &'a EmdfPayload,
    pub deviations: Vec<JocProfileDeviation>,
}

/// Validates a parsed EMDF container under one explicit policy.
///
/// ObservedVendorCompat accepts the strict profile plus only the exact Logic
/// Pro/Dolby signaling deviations currently evidenced by controlled and
/// external fixtures. It never mutates or synthesizes configuration fields.
///
/// # Errors
/// Returns complete payload-count and field-level evidence when the selected
/// policy rejects the parsed representation.
pub fn validate_joc_profile_for(
    container: &EmdfContainer,
    profile: JocValidationProfile,
) -> Result<ValidatedJocProfile<'_>, JocProfileValidationFailure> {
    let oamd: Vec<_> = container
        .payloads
        .iter()
        .filter(|payload| payload.id == OAMD_PAYLOAD_ID)
        .collect();
    let joc: Vec<_> = container
        .payloads
        .iter()
        .filter(|payload| payload.id == JOC_PAYLOAD_ID)
        .collect();
    if oamd.len() != 1 || joc.len() != 1 {
        return Err(JocProfileValidationFailure {
            profile,
            oamd_payload_count: oamd.len(),
            joc_payload_count: joc.len(),
            deviations: Vec::new(),
        });
    }
    let oamd = oamd[0];
    let joc = joc[0];
    let mut deviations = profile_config_deviations(oamd);
    deviations.extend(profile_config_deviations(joc));
    if oamd.config.group_id != joc.config.group_id {
        deviations.push(JocProfileDeviation {
            payload_id: JOC_PAYLOAD_ID,
            field: JocProfileField::GroupId,
            actual: option_u64_value(joc.config.group_id),
            expected_by_etsi: option_u64_value(oamd.config.group_id),
        });
    }
    let accepted = match profile {
        JocValidationProfile::EtsiStrict => deviations.is_empty(),
        JocValidationProfile::ObservedVendorCompat => {
            deviations.iter().all(is_allowed_vendor_deviation)
        }
    };
    if !accepted {
        return Err(JocProfileValidationFailure {
            profile,
            oamd_payload_count: 1,
            joc_payload_count: 1,
            deviations,
        });
    }
    let status = if deviations.is_empty() {
        JocValidationStatus::NormativeCompliant
    } else {
        JocValidationStatus::AcceptedWithDeviation
    };
    Ok(ValidatedJocProfile {
        profile,
        status,
        oamd,
        joc,
        deviations,
    })
}

/// Applies every TS 103 420 table 55 and table 56 restriction.
///
/// The otherwise omitted `smploffste` and `payload_frame_aligned` values are
/// structurally implied by the presence of `create_duplicate`,
/// `remove_duplicate`, `priority`, and `proc_allowed` in table 56.
///
/// # Errors
/// Returns an error unless exactly one OAMD and one JOC payload are present,
/// both have the prescribed configuration, and their group IDs match.
pub fn validate_joc_profile(container: &EmdfContainer) -> Result<JocPayloadPair<'_>, EmdfError> {
    let validated = validate_joc_profile_for(container, JocValidationProfile::EtsiStrict).map_err(
        |failure| {
            if failure.oamd_payload_count != 1 || failure.joc_payload_count != 1 {
                EmdfError::JocProfilePayloadCount {
                    oamd: failure.oamd_payload_count,
                    joc: failure.joc_payload_count,
                }
            } else {
                EmdfError::JocProfileConfiguration
            }
        },
    )?;
    Ok(JocPayloadPair {
        oamd: &validated.oamd.data,
        joc: &validated.joc.data,
    })
}

fn profile_config_deviations(payload: &EmdfPayload) -> Vec<JocProfileDeviation> {
    let config = &payload.config;
    let mut deviations = Vec::new();
    push_deviation(
        &mut deviations,
        payload.id,
        JocProfileField::SampleOffset,
        option_u16_value(config.sample_offset),
        JocProfileValue::Absent,
    );
    push_deviation(
        &mut deviations,
        payload.id,
        JocProfileField::Duration,
        option_u64_value(config.duration),
        JocProfileValue::Absent,
    );
    push_deviation(
        &mut deviations,
        payload.id,
        JocProfileField::GroupId,
        option_u64_value(config.group_id),
        JocProfileValue::Present,
    );
    push_deviation(
        &mut deviations,
        payload.id,
        JocProfileField::CodecDataPresent,
        JocProfileValue::Bool(config.codec_data_present),
        JocProfileValue::Bool(true),
    );
    push_deviation(
        &mut deviations,
        payload.id,
        JocProfileField::DiscardUnknownPayload,
        JocProfileValue::Bool(config.discard_unknown_payload),
        JocProfileValue::Bool(false),
    );
    push_deviation(
        &mut deviations,
        payload.id,
        JocProfileField::PayloadFrameAligned,
        option_bool_value(config.payload_frame_aligned),
        JocProfileValue::Bool(true),
    );
    push_deviation(
        &mut deviations,
        payload.id,
        JocProfileField::CreateDuplicate,
        option_bool_value(config.create_duplicate),
        JocProfileValue::Bool(false),
    );
    push_deviation(
        &mut deviations,
        payload.id,
        JocProfileField::RemoveDuplicate,
        option_bool_value(config.remove_duplicate),
        JocProfileValue::Bool(false),
    );
    push_deviation(
        &mut deviations,
        payload.id,
        JocProfileField::Priority,
        option_u8_value(config.priority),
        JocProfileValue::Unsigned(0),
    );
    push_deviation(
        &mut deviations,
        payload.id,
        JocProfileField::ProcessingAllowed,
        option_u8_value(config.processing_allowed),
        JocProfileValue::Unsigned(0),
    );
    deviations
}

fn push_deviation(
    deviations: &mut Vec<JocProfileDeviation>,
    payload_id: u64,
    field: JocProfileField,
    actual: JocProfileValue,
    expected_by_etsi: JocProfileValue,
) {
    let matches = actual == expected_by_etsi
        || matches!(expected_by_etsi, JocProfileValue::Present)
            && !matches!(actual, JocProfileValue::Absent);
    if !matches {
        deviations.push(JocProfileDeviation {
            payload_id,
            field,
            actual,
            expected_by_etsi,
        });
    }
}

fn option_bool_value(value: Option<bool>) -> JocProfileValue {
    value.map_or(JocProfileValue::Absent, JocProfileValue::Bool)
}

fn option_u8_value(value: Option<u8>) -> JocProfileValue {
    value.map_or(JocProfileValue::Absent, |value| {
        JocProfileValue::Unsigned(u64::from(value))
    })
}

fn option_u16_value(value: Option<u16>) -> JocProfileValue {
    value.map_or(JocProfileValue::Absent, |value| {
        JocProfileValue::Unsigned(u64::from(value))
    })
}

fn option_u64_value(value: Option<u64>) -> JocProfileValue {
    value.map_or(JocProfileValue::Absent, JocProfileValue::Unsigned)
}

fn is_allowed_vendor_deviation(deviation: &JocProfileDeviation) -> bool {
    matches!(
        (
            deviation.payload_id,
            deviation.field,
            deviation.actual,
            deviation.expected_by_etsi,
        ),
        (
            OAMD_PAYLOAD_ID | JOC_PAYLOAD_ID,
            JocProfileField::CodecDataPresent,
            JocProfileValue::Bool(false),
            JocProfileValue::Bool(true),
        ) | (
            OAMD_PAYLOAD_ID,
            JocProfileField::PayloadFrameAligned,
            JocProfileValue::Bool(false),
            JocProfileValue::Bool(true),
        ) | (
            OAMD_PAYLOAD_ID,
            JocProfileField::CreateDuplicate | JocProfileField::RemoveDuplicate,
            JocProfileValue::Absent,
            JocProfileValue::Bool(false),
        ) | (
            OAMD_PAYLOAD_ID,
            JocProfileField::Priority | JocProfileField::ProcessingAllowed,
            JocProfileValue::Absent,
            JocProfileValue::Unsigned(0),
        )
    )
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
    Ok(parse_emdf_sync_with_bit_trace(bytes)?.parsed)
}

/// Parses one EMDF container and retains exact payload/config bit spans.
///
/// Offsets in the returned traces are relative to the first bit of `bytes`,
/// including the 32-bit EMDF synchronization header. The same bounded syntax
/// and padding checks as [`parse_emdf_sync`] are used.
pub fn parse_emdf_sync_with_bit_trace(bytes: &[u8]) -> Result<ParsedEmdfBitTrace, EmdfError> {
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
    let (container, payloads) = parse_container_with_bit_trace(&mut reader)?;
    let padding_bits = reader.bits_remaining();
    if padding_bits > 7 {
        return Err(EmdfError::ExcessPadding { bits: padding_bits });
    }
    while reader.bits_remaining() > 0 {
        if reader.read_bit()? {
            return Err(EmdfError::NonzeroPadding);
        }
    }
    Ok(ParsedEmdfBitTrace {
        parsed: ParsedEmdf {
            container,
            bytes_consumed: end,
        },
        payloads,
    })
}

/// Classifies an exact, bounded EMDF carrier range.
///
/// This is the common entry point for E-AC-3 caller-declared data ranges such
/// as frame-end `auxdata` and exact audio-block `skipfld` candidate ranges. TS
/// 102 366 describes `skipfld` as dummy data; classifying that range here is a
/// bounded diagnostic operation, not an assertion that it is a JOC carrier. A
/// range that does not begin with `0x5838` is ordinary non-EMDF data. Once that
/// syncword is present, every failure is retained as a malformed candidate; the
/// function does not fall back to searching for another syncword.
#[must_use]
pub fn classify_emdf_carrier(bytes: &[u8]) -> CarrierClassification {
    if bytes.len() < 2 || u16::from_be_bytes([bytes[0], bytes[1]]) != SYNCWORD {
        return CarrierClassification::NonEmdf;
    }
    match parse_emdf_sync(bytes) {
        Ok(parsed) if parsed.bytes_consumed == bytes.len() => CarrierClassification::Parsed(parsed),
        Ok(parsed) => CarrierClassification::TrailingData {
            container_bytes: parsed.bytes_consumed,
            carrier_bytes: bytes.len(),
        },
        Err(error) => CarrierClassification::Malformed(error),
    }
}

fn parse_container_with_bit_trace(
    reader: &mut impl BitRead,
) -> Result<(EmdfContainer, Vec<EmdfPayloadBitTrace>), EmdfError> {
    let initial_bits = reader.bits_remaining();
    let version_base = reader.read_bits(2)?;
    let version = extended_value(reader, version_base, 3, 2)?;
    if version != 0 {
        return Err(EmdfError::UnsupportedVersion { version });
    }
    let key_base = reader.read_bits(3)?;
    let key_id = extended_value(reader, key_base, 7, 3)?;
    let mut payloads = Vec::new();
    let mut traces = Vec::new();
    loop {
        let payload_id_start_bit = 32 + (initial_bits - reader.bits_remaining());
        let id_base = reader.read_bits(5)?;
        let id = extended_value(reader, id_base, 31, 5)?;
        if id == 0 {
            break;
        }
        let payload_id_end_bit = 32 + (initial_bits - reader.bits_remaining());
        let config_start_bit = payload_id_end_bit;
        let config = parse_payload_config(reader)?;
        let config_end_bit = 32 + (initial_bits - reader.bits_remaining());
        let payload_size_start_bit = config_end_bit;
        let payload_size = variable_bits(reader, 8, 2)?;
        let payload_size_end_bit = 32 + (initial_bits - reader.bits_remaining());
        let payload_size = usize::try_from(payload_size).map_err(|_| EmdfError::ValueOverflow)?;
        let payload_body_start_bit = payload_size_end_bit;
        let mut data = Vec::with_capacity(payload_size);
        for _ in 0..payload_size {
            data.push(u8::try_from(reader.read_bits(8)?).map_err(|_| EmdfError::ValueOverflow)?);
        }
        let payload_body_end_bit = 32 + (initial_bits - reader.bits_remaining());
        traces.push(EmdfPayloadBitTrace {
            payload_id: id,
            payload_id_start_bit,
            payload_id_end_bit,
            config_start_bit,
            config_end_bit,
            payload_size_start_bit,
            payload_size_end_bit,
            payload_body_start_bit,
            payload_body_end_bit,
        });
        payloads.push(EmdfPayload { id, config, data });
    }
    let protection = parse_protection(reader)?;
    Ok((
        EmdfContainer {
            version,
            key_id,
            payloads,
            protection,
        },
        traces,
    ))
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
    let codec_data_present = reader.read_bit()?;
    if codec_data_present && reader.read_bits(8)? != 0 {
        return Err(EmdfError::NonzeroReservedData);
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
        codec_data_present,
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
