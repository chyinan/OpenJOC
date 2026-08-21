// pattern: Functional Core

use crate::performance::DecodeStageTiming;
use openjoc_container::{InputMediaError, RawEac3AccessUnit, RawEac3AccessUnitReader};
use openjoc_eac3::{
    DecodedAccessUnitPcm, DialnormMode, Eac3Error, InternalBasePolicy, JocAccessUnitPcmDecoder,
    JocMetadataFrame, extract_joc_access_unit_for_profile, extract_joc_addbsi_access_unit,
    group_access_units, index_syncframes, parse_bsi, parse_joc_access_unit,
    validate_complexity_index, validate_joc_access_unit,
};
use openjoc_emdf::JocValidationProfile;
use openjoc_joc::{JocParseError, ReconstructionBasis, parse_joc_payload};
use openjoc_oamd::{
    OAMD_PAYLOAD_ID, OamdDecoderConfig, OamdError, OamdParseProfile, ObjectAnchor,
    parse_oamd_payload_with_config, parse_oamd_payload_with_profile,
};
use openjoc_scene::{
    DecodedPayloadFrame, JocFrameInput, ObjectScene, PayloadDecodeError, PayloadDecoder,
    PayloadDecoderConfig, ProgrammeLayout, StreamingSceneSummary,
};
use openjoc_wave::WavePcm;
use std::{fmt, io::Read, time::Instant};

/// User-facing profile selection policy. The selected value passed downstream
/// is always one of the two existing validation profiles.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ValidationProfileRequest {
    Auto,
    EtsiStrict,
    ObservedVendorCompat,
}

impl ValidationProfileRequest {
    #[must_use]
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "AUTO",
            Self::EtsiStrict => "ETSI_STRICT",
            Self::ObservedVendorCompat => "OBSERVED_VENDOR_COMPAT",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct StreamTiming {
    pub(crate) access_units: u64,
    pub(crate) samples: u64,
    pub(crate) sample_rate: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AdmStreamPreflight {
    pub(crate) access_units: u64,
    pub(crate) duration_samples: u64,
    pub(crate) sample_rate: u32,
    pub(crate) reconstruction_signal_count: usize,
    pub(crate) base_lfe_present: bool,
    pub(crate) dynamic_object_count: usize,
    pub(crate) metadata_object_count: usize,
    pub(crate) max_frame_samples: usize,
    pub(crate) max_au_bytes: usize,
}

pub(crate) fn stream_timing(stream: &[u8]) -> Result<StreamTiming, DecodeEac3Error> {
    let frames = index_syncframes(stream)?;
    let units = group_access_units(&frames)?;
    let first = units.first().ok_or(DecodeEac3Error::EmptyStream)?;
    let samples = units.iter().try_fold(0_u64, |total, unit| {
        total
            .checked_add(u64::from(unit.samples))
            .ok_or(DecodeEac3Error::SampleCountOverflow)
    })?;
    Ok(StreamTiming {
        access_units: u64::try_from(units.len())
            .map_err(|_| DecodeEac3Error::SampleCountOverflow)?,
        samples,
        sample_rate: first.sample_rate,
    })
}

/// Resolves a profile before the stateful decoder is created. AUTO evaluates
/// strict validation first and uses compatibility only when the existing
/// compatibility validator accepts every deviation.
pub(crate) fn resolve_profile_for_stream(
    stream: &[u8],
    config: PayloadDecoderConfig,
    request: ValidationProfileRequest,
) -> Result<JocValidationProfile, DecodeEac3Error> {
    match request {
        ValidationProfileRequest::EtsiStrict => Ok(JocValidationProfile::EtsiStrict),
        ValidationProfileRequest::ObservedVendorCompat => {
            Ok(JocValidationProfile::ObservedVendorCompat)
        }
        ValidationProfileRequest::Auto => {
            let frames = index_syncframes(stream)?;
            let units = group_access_units(&frames)?;
            if units.is_empty() {
                return Err(DecodeEac3Error::EmptyStream);
            }
            let mut selected = JocValidationProfile::EtsiStrict;
            for (access_unit, unit) in units.into_iter().enumerate() {
                let Some(parsed) = parse_joc_access_unit(stream, &frames, unit)? else {
                    match extract_joc_addbsi_access_unit(stream, &frames, unit)? {
                        Some(extension) => {
                            return Err(DecodeEac3Error::JocExtensionWithoutMetadata {
                                access_unit,
                                complexity_index: extension.complexity_index,
                            });
                        }
                        None => return Err(DecodeEac3Error::MissingMetadata { access_unit }),
                    }
                };
                let metadata =
                    match validate_joc_access_unit(&parsed, JocValidationProfile::EtsiStrict) {
                        Ok(metadata) => metadata,
                        Err(strict_error) => match validate_joc_access_unit(
                            &parsed,
                            JocValidationProfile::ObservedVendorCompat,
                        ) {
                            Ok(metadata) => {
                                selected = JocValidationProfile::ObservedVendorCompat;
                                metadata
                            }
                            Err(_) => return Err(strict_error.into()),
                        },
                    };
                let oamd_strict = parse_oamd_for_profile(
                    &metadata.oamd,
                    config.oamd,
                    JocValidationProfile::EtsiStrict,
                );
                if oamd_strict.is_err() {
                    parse_oamd_for_profile(
                        &metadata.oamd,
                        config.oamd,
                        JocValidationProfile::ObservedVendorCompat,
                    )?;
                    selected = JocValidationProfile::ObservedVendorCompat;
                }
            }
            Ok(selected)
        }
    }
}

/// Resolves a single payload profile for `decode-payload`.
pub(crate) fn resolve_profile_for_oamd(
    payload: &[u8],
    config: OamdDecoderConfig,
    request: ValidationProfileRequest,
) -> Result<OamdParseProfile, OamdError> {
    match request {
        ValidationProfileRequest::EtsiStrict => Ok(OamdParseProfile::EtsiStrict),
        ValidationProfileRequest::ObservedVendorCompat => {
            Ok(OamdParseProfile::ObservedVendorCompat)
        }
        ValidationProfileRequest::Auto => {
            match parse_oamd_for_profile(payload, config, JocValidationProfile::EtsiStrict) {
                Ok(_) => Ok(OamdParseProfile::EtsiStrict),
                Err(strict_error) => parse_oamd_for_profile(
                    payload,
                    config,
                    JocValidationProfile::ObservedVendorCompat,
                )
                .map(|_| OamdParseProfile::ObservedVendorCompat)
                .map_err(|_| strict_error),
            }
        }
    }
}

#[derive(Debug)]
pub enum DecodeEac3Error {
    Input(InputMediaError),
    Eac3(Eac3Error),
    Oamd(OamdError),
    Payload(PayloadDecodeError),
    JocPayload(JocParseError),
    EmptyStream,
    SampleCountOverflow,
    InvalidPcmLength {
        expected: usize,
    },
    SampleRateMismatch {
        pcm: u32,
        stream: u32,
    },
    MissingMetadata {
        access_unit: usize,
    },
    JocExtensionWithoutMetadata {
        access_unit: usize,
        complexity_index: u8,
    },
    Sink(String),
    FrameIndexOverflow,
}

impl fmt::Display for DecodeEac3Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Input(error) => {
                write!(formatter, "failed to read E-AC-3 input/container: {error}")
            }
            Self::Eac3(error) => write!(formatter, "failed to decode E-AC-3 frontend: {error}"),
            Self::Oamd(error) => write!(formatter, "failed to validate OAMD profile: {error}"),
            Self::Payload(error) => {
                write!(formatter, "failed to reconstruct JOC basis frame: {error}")
            }
            Self::JocPayload(error) => write!(formatter, "failed to parse JOC payload: {error}"),
            Self::EmptyStream => formatter.write_str("empty E-AC-3 stream"),
            Self::SampleCountOverflow => formatter.write_str("E-AC-3 sample count overflow"),
            Self::InvalidPcmLength { expected } => write!(
                formatter,
                "decoded downmix does not contain exactly {expected} aligned samples per channel"
            ),
            Self::SampleRateMismatch { pcm, stream } => write!(
                formatter,
                "decoded downmix rate {pcm} Hz does not match access-unit rate {stream} Hz"
            ),
            Self::MissingMetadata { access_unit } => {
                write!(
                    formatter,
                    "JOC metadata is absent from access unit {access_unit}"
                )
            }
            Self::JocExtensionWithoutMetadata {
                access_unit,
                complexity_index,
            } => write!(
                formatter,
                "JOC extension complexity index {complexity_index} is signaled in access unit {access_unit}, but its required OAMD/JOC EMDF metadata is absent"
            ),
            Self::Sink(message) => {
                write!(
                    formatter,
                    "failed to write streaming decode artifact: {message}"
                )
            }
            Self::FrameIndexOverflow => formatter.write_str("E-AC-3 frame index overflow"),
        }
    }
}

impl std::error::Error for DecodeEac3Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Input(error) => Some(error),
            Self::Eac3(error) => Some(error),
            Self::Oamd(error) => Some(error),
            Self::Payload(error) => Some(error),
            Self::JocPayload(error) => Some(error),
            _ => None,
        }
    }
}

impl From<InputMediaError> for DecodeEac3Error {
    fn from(value: InputMediaError) -> Self {
        Self::Input(value)
    }
}

impl From<Eac3Error> for DecodeEac3Error {
    fn from(value: Eac3Error) -> Self {
        Self::Eac3(value)
    }
}

impl From<OamdError> for DecodeEac3Error {
    fn from(value: OamdError) -> Self {
        Self::Oamd(value)
    }
}

impl From<PayloadDecodeError> for DecodeEac3Error {
    fn from(value: PayloadDecodeError) -> Self {
        Self::Payload(value)
    }
}

impl From<JocParseError> for DecodeEac3Error {
    fn from(value: JocParseError) -> Self {
        Self::JocPayload(value)
    }
}

fn required_metadata(
    stream: &[u8],
    frames: &[openjoc_eac3::SyncframeIndexEntry],
    unit: openjoc_eac3::AccessUnitIndex,
    access_unit: usize,
    validation_profile: JocValidationProfile,
) -> Result<JocMetadataFrame, DecodeEac3Error> {
    if let Some(metadata) =
        extract_joc_access_unit_for_profile(stream, frames, unit, validation_profile)?
    {
        return Ok(metadata);
    }
    match extract_joc_addbsi_access_unit(stream, frames, unit)? {
        Some(extension) => Err(DecodeEac3Error::JocExtensionWithoutMetadata {
            access_unit,
            complexity_index: extension.complexity_index,
        }),
        None => Err(DecodeEac3Error::MissingMetadata { access_unit }),
    }
}

fn select_metadata_for_request(
    stream: &[u8],
    frames: &[openjoc_eac3::SyncframeIndexEntry],
    unit: openjoc_eac3::AccessUnitIndex,
    access_unit: usize,
    request: ValidationProfileRequest,
) -> Result<(JocMetadataFrame, JocValidationProfile), DecodeEac3Error> {
    match request {
        ValidationProfileRequest::EtsiStrict => Ok((
            required_metadata(
                stream,
                frames,
                unit,
                access_unit,
                JocValidationProfile::EtsiStrict,
            )?,
            JocValidationProfile::EtsiStrict,
        )),
        ValidationProfileRequest::ObservedVendorCompat => Ok((
            required_metadata(
                stream,
                frames,
                unit,
                access_unit,
                JocValidationProfile::ObservedVendorCompat,
            )?,
            JocValidationProfile::ObservedVendorCompat,
        )),
        ValidationProfileRequest::Auto => {
            let Some(parsed) = parse_joc_access_unit(stream, frames, unit)? else {
                match extract_joc_addbsi_access_unit(stream, frames, unit)? {
                    Some(extension) => {
                        return Err(DecodeEac3Error::JocExtensionWithoutMetadata {
                            access_unit,
                            complexity_index: extension.complexity_index,
                        });
                    }
                    None => return Err(DecodeEac3Error::MissingMetadata { access_unit }),
                }
            };
            match validate_joc_access_unit(&parsed, JocValidationProfile::EtsiStrict) {
                Ok(metadata) => Ok((metadata, JocValidationProfile::EtsiStrict)),
                Err(strict_error) => {
                    match validate_joc_access_unit(
                        &parsed,
                        JocValidationProfile::ObservedVendorCompat,
                    ) {
                        Ok(metadata) => Ok((metadata, JocValidationProfile::ObservedVendorCompat)),
                        Err(_) => Err(strict_error.into()),
                    }
                }
            }
        }
    }
}

fn parse_oamd_for_profile(
    payload: &[u8],
    config: OamdDecoderConfig,
    validation_profile: JocValidationProfile,
) -> Result<openjoc_oamd::OamdPayload, OamdError> {
    match validation_profile {
        JocValidationProfile::EtsiStrict => parse_oamd_payload_with_config(payload, config),
        JocValidationProfile::ObservedVendorCompat => parse_oamd_payload_with_profile(
            payload,
            config,
            OamdParseProfile::ObservedVendorCompat,
            OAMD_PAYLOAD_ID,
        ),
    }
}

/// Aligns five non-LFE JOC input channels with a separately supplied base LFE.
/// The LFE is never sent to the JOC QMF matrix; it is bound to an OAMD
/// speaker-anchored entry at the scene boundary.
pub fn decode_aligned_eac3_with_sink_and_lfe<S>(
    stream: &[u8],
    downmix: &WavePcm,
    base_lfe: Option<&WavePcm>,
    config: PayloadDecoderConfig,
    validation_profile: JocValidationProfile,
    sink: S,
) -> Result<ObjectScene, DecodeEac3Error>
where
    S: FnMut(usize, &JocMetadataFrame, &DecodedPayloadFrame) -> Result<(), DecodeEac3Error>,
{
    decode_aligned_eac3_core(
        stream,
        downmix,
        base_lfe,
        config,
        validation_profile,
        false,
        sink,
        PayloadDecoder::finish,
    )
}

/// Streaming variant of [`decode_aligned_eac3_with_sink_and_lfe`]. It emits
/// frame results to the sink and retains only codec plus current-frame state.
#[allow(dead_code)]
pub fn decode_aligned_eac3_streaming_with_sink_and_lfe<S>(
    stream: &[u8],
    downmix: &WavePcm,
    base_lfe: Option<&WavePcm>,
    config: PayloadDecoderConfig,
    validation_profile: JocValidationProfile,
    sink: S,
) -> Result<StreamingSceneSummary, DecodeEac3Error>
where
    S: FnMut(usize, &JocMetadataFrame, &DecodedPayloadFrame) -> Result<(), DecodeEac3Error>,
{
    decode_aligned_eac3_core(
        stream,
        downmix,
        base_lfe,
        config,
        validation_profile,
        true,
        sink,
        PayloadDecoder::finish_streaming,
    )
}

#[allow(clippy::too_many_arguments)]
fn decode_aligned_eac3_core<S, R, F>(
    stream: &[u8],
    downmix: &WavePcm,
    base_lfe: Option<&WavePcm>,
    config: PayloadDecoderConfig,
    validation_profile: JocValidationProfile,
    streaming: bool,
    mut sink: S,
    finish: F,
) -> Result<R, DecodeEac3Error>
where
    S: FnMut(usize, &JocMetadataFrame, &DecodedPayloadFrame) -> Result<(), DecodeEac3Error>,
    F: FnOnce(PayloadDecoder) -> Result<R, PayloadDecodeError>,
{
    let frame_index = index_syncframes(stream)?;
    let units = group_access_units(&frame_index)?;
    if units.is_empty() {
        return Err(DecodeEac3Error::EmptyStream);
    }
    let expected_samples = units.iter().try_fold(0_usize, |total, unit| {
        total
            .checked_add(usize::from(unit.samples))
            .ok_or(DecodeEac3Error::SampleCountOverflow)
    })?;
    if downmix.channels.is_empty()
        || downmix
            .channels
            .iter()
            .any(|channel| channel.len() != expected_samples)
    {
        return Err(DecodeEac3Error::InvalidPcmLength {
            expected: expected_samples,
        });
    }
    if let Some(lfe) = base_lfe {
        if lfe.channels.len() != 1
            || lfe.sample_rate != downmix.sample_rate
            || lfe.channels[0].len() != expected_samples
        {
            return Err(DecodeEac3Error::Payload(PayloadDecodeError::from(
                openjoc_scene::ProgrammeLayoutError::BaseLfeLengthMismatch {
                    expected: expected_samples,
                    actual: lfe.channels.first().map_or(0, Vec::len),
                },
            )));
        }
    }

    let oamd_profile = match validation_profile {
        JocValidationProfile::EtsiStrict => OamdParseProfile::EtsiStrict,
        JocValidationProfile::ObservedVendorCompat => OamdParseProfile::ObservedVendorCompat,
    };
    let mut decoder = if streaming {
        PayloadDecoder::streaming_with_oamd_profile(config, oamd_profile)
    } else {
        PayloadDecoder::with_oamd_profile(config, oamd_profile)
    };
    let mut sample_offset = 0_usize;
    for (unit_index, unit) in units.into_iter().enumerate() {
        if unit.sample_rate != downmix.sample_rate {
            return Err(DecodeEac3Error::SampleRateMismatch {
                pcm: downmix.sample_rate,
                stream: unit.sample_rate,
            });
        }
        let metadata =
            required_metadata(stream, &frame_index, unit, unit_index, validation_profile)?;
        let parsed_oamd = parse_oamd_for_profile(&metadata.oamd, config.oamd, validation_profile)?;
        validate_complexity_index(metadata.complexity_index, parsed_oamd.prefix.object_count)?;
        let end = sample_offset
            .checked_add(usize::from(unit.samples))
            .ok_or(DecodeEac3Error::SampleCountOverflow)?;
        let frame_pcm = downmix
            .channels
            .iter()
            .map(|channel| channel[sample_offset..end].to_vec())
            .collect::<Vec<_>>();
        let frame_lfe = base_lfe.map(|lfe| &lfe.channels[0][sample_offset..end]);
        let frame_number =
            u64::try_from(unit_index).map_err(|_| DecodeEac3Error::FrameIndexOverflow)?;
        decoder.decode_frame_with(
            JocFrameInput {
                sample_rate: unit.sample_rate,
                downmix_pcm: &frame_pcm,
                base_lfe_pcm: frame_lfe,
                joc_payload: &metadata.joc,
                oamd_payload: &metadata.oamd,
                frame_index: frame_number,
            },
            |frame| sink(unit_index, &metadata, frame),
        )?;
        sample_offset = end;
    }
    Ok(finish(decoder)?)
}

/// Explicit-policy variant of the internal base decoder. The policy is
/// carried through the E-AC-3 decoder boundary and is never inferred from
/// input bytes or validation profile.
pub fn decode_internal_eac3_with_base_sink_and_policy<S, B>(
    stream: &[u8],
    config: PayloadDecoderConfig,
    validation_profile: JocValidationProfile,
    dither_values: &[f64],
    base_policy: InternalBasePolicy,
    sink: S,
    base_sink: B,
) -> Result<ObjectScene, DecodeEac3Error>
where
    S: FnMut(usize, &JocMetadataFrame, &DecodedPayloadFrame) -> Result<(), DecodeEac3Error>,
    B: FnMut(usize, &DecodedAccessUnitPcm) -> Result<(), DecodeEac3Error>,
{
    decode_internal_eac3_core(
        stream,
        config,
        validation_profile,
        dither_values,
        base_policy,
        DialnormMode::Default,
        false,
        sink,
        base_sink,
        |_, _, _, _| Ok(()),
        PayloadDecoder::finish,
        None,
    )
}

/// Streaming internal-base variant. The base sink receives each decoded AU;
/// no programme PCM is retained by the decoder core.
#[allow(dead_code)]
pub fn decode_internal_eac3_streaming_with_base_sink_and_policy<S, B>(
    stream: &[u8],
    config: PayloadDecoderConfig,
    validation_profile: JocValidationProfile,
    dither_values: &[f64],
    base_policy: InternalBasePolicy,
    sink: S,
    base_sink: B,
) -> Result<StreamingSceneSummary, DecodeEac3Error>
where
    S: FnMut(usize, &JocMetadataFrame, &DecodedPayloadFrame) -> Result<(), DecodeEac3Error>,
    B: FnMut(usize, &DecodedAccessUnitPcm) -> Result<(), DecodeEac3Error>,
{
    decode_internal_eac3_core(
        stream,
        config,
        validation_profile,
        dither_values,
        base_policy,
        DialnormMode::Default,
        true,
        sink,
        base_sink,
        |_, _, _, _| Ok(()),
        PayloadDecoder::finish_streaming,
        None,
    )
}

/// Streaming internal-base decode with one combined frame sink.
///
/// The combined sink receives the decoded JOC frame and its matching Base/LFE
/// access unit while both are still bounded to the current access unit. This
/// is the integration boundary used by the speaker render command; it does
/// not change the decoder or bridge mathematics.
#[allow(dead_code)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn decode_internal_eac3_streaming_with_render_sink_and_policy<S, B>(
    stream: &[u8],
    config: PayloadDecoderConfig,
    validation_profile: JocValidationProfile,
    dither_values: &[f64],
    base_policy: InternalBasePolicy,
    sink: S,
    base_sink: B,
    timing: Option<&mut DecodeStageTiming>,
) -> Result<(StreamingSceneSummary, ReconstructionBasis), DecodeEac3Error>
where
    S: FnMut(
        usize,
        &JocMetadataFrame,
        &DecodedPayloadFrame,
        &DecodedAccessUnitPcm,
    ) -> Result<(), DecodeEac3Error>,
    B: FnMut(usize, &DecodedAccessUnitPcm) -> Result<(), DecodeEac3Error>,
{
    decode_internal_eac3_streaming_with_render_sink_and_policy_and_dialnorm(
        stream,
        config,
        validation_profile,
        dither_values,
        base_policy,
        DialnormMode::Default,
        sink,
        base_sink,
        timing,
    )
}

/// Streaming internal-base decode with a combined render sink and an
/// explicit decoder dialnorm policy. The rendering order and dialnorm math
/// remain owned by the existing decoder/renderer stages.
#[allow(clippy::too_many_arguments)]
pub(crate) fn decode_internal_eac3_streaming_with_render_sink_and_policy_and_dialnorm<S, B>(
    stream: &[u8],
    config: PayloadDecoderConfig,
    validation_profile: JocValidationProfile,
    dither_values: &[f64],
    base_policy: InternalBasePolicy,
    dialnorm_mode: DialnormMode,
    sink: S,
    base_sink: B,
    timing: Option<&mut DecodeStageTiming>,
) -> Result<(StreamingSceneSummary, ReconstructionBasis), DecodeEac3Error>
where
    S: FnMut(
        usize,
        &JocMetadataFrame,
        &DecodedPayloadFrame,
        &DecodedAccessUnitPcm,
    ) -> Result<(), DecodeEac3Error>,
    B: FnMut(usize, &DecodedAccessUnitPcm) -> Result<(), DecodeEac3Error>,
{
    decode_internal_eac3_core(
        stream,
        config,
        validation_profile,
        dither_values,
        base_policy,
        dialnorm_mode,
        true,
        |_frame_index, _metadata, _frame| Ok(()),
        base_sink,
        sink,
        PayloadDecoder::finish_streaming_with_reconstruction_tail,
        timing,
    )
}

/// Direct sequential raw-E-AC-3 decode path.
///
/// Unlike the legacy slice API, this path owns only one bounded access unit at
/// a time. The J1R18 `PayloadDecoder` and `JocAccessUnitPcmDecoder` remain the
/// sole decoding implementations; this function only supplies their input
/// from the incremental container consumer.
#[allow(dead_code)]
#[allow(clippy::too_many_arguments)]
pub fn decode_internal_eac3_reader_with_base_sink_and_policy<R, S, B>(
    reader: R,
    max_frame_bytes: usize,
    config: PayloadDecoderConfig,
    validation_profile: JocValidationProfile,
    dither_values: &[f64],
    base_policy: InternalBasePolicy,
    sink: S,
    base_sink: B,
) -> Result<StreamingSceneSummary, DecodeEac3Error>
where
    R: Read,
    S: FnMut(usize, &JocMetadataFrame, &DecodedPayloadFrame) -> Result<(), DecodeEac3Error>,
    B: FnMut(usize, &DecodedAccessUnitPcm) -> Result<(), DecodeEac3Error>,
{
    let request = match validation_profile {
        JocValidationProfile::EtsiStrict => ValidationProfileRequest::EtsiStrict,
        JocValidationProfile::ObservedVendorCompat => {
            ValidationProfileRequest::ObservedVendorCompat
        }
    };
    decode_internal_eac3_reader_with_base_sink_and_policy_request(
        reader,
        max_frame_bytes,
        config,
        request,
        dither_values,
        base_policy,
        sink,
        base_sink,
    )
}

/// Sequential metadata/topology pass for a production ADM export.
///
/// Every AU is framed and its JOC/OAMD syntax is validated, but only the first
/// AU is decoded to PCM to establish the actual admitted Base/LFE topology.
/// Retention is bounded to one AU and the returned counters.
#[allow(clippy::too_many_arguments)]
pub(crate) fn preflight_adm_reader<R: Read>(
    reader: R,
    max_frame_bytes: usize,
    config: PayloadDecoderConfig,
    validation_profile: ValidationProfileRequest,
    dither_values: &[f64],
    base_policy: InternalBasePolicy,
) -> Result<AdmStreamPreflight, DecodeEac3Error> {
    let mut access_units = RawEac3AccessUnitReader::new(reader, max_frame_bytes);
    let mut audio_decoder = JocAccessUnitPcmDecoder::new();
    let mut count = 0_u64;
    let mut duration_samples = 0_u64;
    let mut sample_rate = None;
    let mut reconstruction_signal_count = None;
    let mut base_lfe_present = None;
    let mut anchors = None;
    let mut topology = None;
    let mut max_frame_samples = 0_usize;
    let mut max_au_bytes = 0_usize;

    while let Some(access_unit) = access_units.next_access_unit()? {
        if sample_rate.is_some_and(|expected| expected != access_unit.unit.sample_rate) {
            return Err(DecodeEac3Error::Payload(
                PayloadDecodeError::SampleRateChanged {
                    expected: sample_rate.unwrap_or_default(),
                    actual: access_unit.unit.sample_rate,
                },
            ));
        }
        sample_rate.get_or_insert(access_unit.unit.sample_rate);
        duration_samples = duration_samples
            .checked_add(u64::from(access_unit.unit.samples))
            .ok_or(DecodeEac3Error::SampleCountOverflow)?;
        count = count
            .checked_add(1)
            .ok_or(DecodeEac3Error::FrameIndexOverflow)?;
        max_frame_samples = max_frame_samples.max(usize::from(access_unit.unit.samples));
        max_au_bytes = max_au_bytes.max(access_unit.bytes.len());

        let unit_topology = access_unit_topology(&access_unit)?;
        if topology
            .as_ref()
            .is_some_and(|expected| expected != &unit_topology)
        {
            return Err(DecodeEac3Error::Sink(
                "E-AC-3 channel topology changed during ADM preflight".to_owned(),
            ));
        }
        topology.get_or_insert(unit_topology);

        let unit_index =
            usize::try_from(count - 1).map_err(|_| DecodeEac3Error::FrameIndexOverflow)?;
        let (metadata, joc_profile) = select_metadata_for_request(
            &access_unit.bytes,
            &access_unit.frames,
            access_unit.unit,
            unit_index,
            validation_profile,
        )?;
        let oamd_profile =
            resolve_profile_for_oamd(&metadata.oamd, config.oamd, validation_profile)?;
        let selected_profile = if joc_profile == JocValidationProfile::ObservedVendorCompat
            || oamd_profile == OamdParseProfile::ObservedVendorCompat
        {
            JocValidationProfile::ObservedVendorCompat
        } else {
            JocValidationProfile::EtsiStrict
        };
        let parsed_oamd = parse_oamd_for_profile(&metadata.oamd, config.oamd, selected_profile)?;
        validate_complexity_index(metadata.complexity_index, parsed_oamd.prefix.object_count)?;
        let parsed_joc = parse_joc_payload(&metadata.joc)?;
        let rows = usize::from(parsed_joc.header.object_count);
        ProgrammeLayout::from_prefix(&parsed_oamd.prefix)
            .map_err(PayloadDecodeError::from)?
            .validate_reconstruction_basis(rows)
            .map_err(PayloadDecodeError::from)?;
        if reconstruction_signal_count.is_some_and(|expected| expected != rows) {
            return Err(DecodeEac3Error::Sink(
                "reconstruction-basis row count changed during ADM preflight".to_owned(),
            ));
        }
        reconstruction_signal_count.get_or_insert(rows);
        let unit_anchors = parsed_oamd.prefix.object_anchors()?;
        if anchors
            .as_ref()
            .is_some_and(|expected| expected != &unit_anchors)
        {
            return Err(DecodeEac3Error::Sink(
                "OAMD content description changed during ADM preflight".to_owned(),
            ));
        }
        anchors.get_or_insert(unit_anchors);

        if base_lfe_present.is_none() {
            let pcm = audio_decoder.decode_with_policy(
                &access_unit.bytes,
                &access_unit.frames,
                access_unit.unit,
                dither_values,
                base_policy,
            )?;
            pcm.validate_joc_topology()?;
            base_lfe_present = Some(pcm.lfe.is_some());
        }
    }
    if count == 0 {
        return Err(DecodeEac3Error::EmptyStream);
    }
    let anchors = anchors.ok_or(DecodeEac3Error::EmptyStream)?;
    Ok(AdmStreamPreflight {
        access_units: count,
        duration_samples,
        sample_rate: sample_rate.ok_or(DecodeEac3Error::EmptyStream)?,
        reconstruction_signal_count: reconstruction_signal_count
            .ok_or(DecodeEac3Error::EmptyStream)?,
        base_lfe_present: base_lfe_present.ok_or(DecodeEac3Error::EmptyStream)?,
        dynamic_object_count: anchors
            .iter()
            .filter(|anchor| **anchor == ObjectAnchor::Dynamic)
            .count(),
        metadata_object_count: anchors.len(),
        max_frame_samples,
        max_au_bytes,
    })
}

fn access_unit_topology(
    access_unit: &RawEac3AccessUnit,
) -> Result<Vec<(u8, bool, Option<u16>)>, DecodeEac3Error> {
    access_unit
        .frames
        .iter()
        .map(|frame| {
            let end = frame
                .offset
                .checked_add(frame.header.frame_size)
                .ok_or(DecodeEac3Error::SampleCountOverflow)?;
            let bytes = access_unit.bytes.get(frame.offset..end).ok_or(
                DecodeEac3Error::InvalidPcmLength {
                    expected: frame.header.frame_size,
                },
            )?;
            let bsi = parse_bsi(bytes)?;
            Ok((bsi.audio_coding_mode, bsi.lfe_on, bsi.channel_map))
        })
        .collect()
}

/// Direct sequential raw-E-AC-3 decode path with user-facing profile
/// selection. The existing public function above remains an explicit-profile
/// compatibility entry point.
#[allow(clippy::too_many_arguments)]
pub(crate) fn decode_internal_eac3_reader_with_base_sink_and_policy_request<R, S, B>(
    reader: R,
    max_frame_bytes: usize,
    config: PayloadDecoderConfig,
    validation_profile: ValidationProfileRequest,
    dither_values: &[f64],
    base_policy: InternalBasePolicy,
    mut sink: S,
    mut base_sink: B,
) -> Result<StreamingSceneSummary, DecodeEac3Error>
where
    R: Read,
    S: FnMut(usize, &JocMetadataFrame, &DecodedPayloadFrame) -> Result<(), DecodeEac3Error>,
    B: FnMut(usize, &DecodedAccessUnitPcm) -> Result<(), DecodeEac3Error>,
{
    decode_internal_eac3_reader_combined(
        reader,
        max_frame_bytes,
        config,
        validation_profile,
        dither_values,
        base_policy,
        |unit_index, metadata, frame, pcm| {
            sink(unit_index, metadata, frame)?;
            base_sink(unit_index, pcm)
        },
    )
}

/// Direct sequential decode with matching ReconstructionBasis and Base/LFE
/// exposed together while both belong to the current bounded AU.
#[allow(clippy::too_many_arguments)]
pub(crate) fn decode_internal_eac3_reader_with_combined_sink<R, S>(
    reader: R,
    max_frame_bytes: usize,
    config: PayloadDecoderConfig,
    validation_profile: ValidationProfileRequest,
    dither_values: &[f64],
    base_policy: InternalBasePolicy,
    sink: S,
) -> Result<StreamingSceneSummary, DecodeEac3Error>
where
    R: Read,
    S: FnMut(
        usize,
        &JocMetadataFrame,
        &DecodedPayloadFrame,
        &DecodedAccessUnitPcm,
    ) -> Result<(), DecodeEac3Error>,
{
    decode_internal_eac3_reader_combined(
        reader,
        max_frame_bytes,
        config,
        validation_profile,
        dither_values,
        base_policy,
        sink,
    )
}

#[allow(clippy::too_many_arguments)]
fn decode_internal_eac3_reader_combined<R, S>(
    reader: R,
    max_frame_bytes: usize,
    config: PayloadDecoderConfig,
    validation_profile: ValidationProfileRequest,
    dither_values: &[f64],
    base_policy: InternalBasePolicy,
    mut sink: S,
) -> Result<StreamingSceneSummary, DecodeEac3Error>
where
    R: Read,
    S: FnMut(
        usize,
        &JocMetadataFrame,
        &DecodedPayloadFrame,
        &DecodedAccessUnitPcm,
    ) -> Result<(), DecodeEac3Error>,
{
    let mut decoder =
        PayloadDecoder::streaming_with_oamd_profile(config, OamdParseProfile::EtsiStrict);
    let mut audio_decoder = JocAccessUnitPcmDecoder::new();
    let mut access_units = RawEac3AccessUnitReader::new(reader, max_frame_bytes);
    let mut unit_index = 0_usize;

    while let Some(access_unit) = access_units.next_access_unit()? {
        let pcm = audio_decoder.decode_with_policy(
            &access_unit.bytes,
            &access_unit.frames,
            access_unit.unit,
            dither_values,
            base_policy,
        )?;
        pcm.validate_joc_topology()?;
        if pcm.sample_rate != access_unit.unit.sample_rate
            || pcm.samples != access_unit.unit.samples
            || pcm.channels.is_empty()
            || pcm
                .channels
                .iter()
                .any(|channel| channel.len() != usize::from(access_unit.unit.samples))
        {
            return Err(DecodeEac3Error::InvalidPcmLength {
                expected: usize::from(access_unit.unit.samples),
            });
        }
        let (mut metadata, joc_profile) = select_metadata_for_request(
            &access_unit.bytes,
            &access_unit.frames,
            access_unit.unit,
            unit_index,
            validation_profile,
        )?;
        let oamd_profile =
            resolve_profile_for_oamd(&metadata.oamd, config.oamd, validation_profile)?;
        let selected_profile = if joc_profile == JocValidationProfile::ObservedVendorCompat
            || oamd_profile == OamdParseProfile::ObservedVendorCompat
        {
            JocValidationProfile::ObservedVendorCompat
        } else {
            JocValidationProfile::EtsiStrict
        };
        metadata.validation_profile = selected_profile;
        let parsed_oamd = parse_oamd_for_profile(&metadata.oamd, config.oamd, selected_profile)?;
        validate_complexity_index(metadata.complexity_index, parsed_oamd.prefix.object_count)?;
        let frame_number =
            u64::try_from(unit_index).map_err(|_| DecodeEac3Error::FrameIndexOverflow)?;
        decoder.decode_frame_with_profile(
            JocFrameInput {
                sample_rate: access_unit.unit.sample_rate,
                downmix_pcm: &pcm.channels,
                base_lfe_pcm: pcm.lfe.as_deref(),
                joc_payload: &metadata.joc,
                oamd_payload: &metadata.oamd,
                frame_index: frame_number,
            },
            oamd_profile,
            |frame| sink(unit_index, &metadata, frame, &pcm),
        )?;
        unit_index = unit_index
            .checked_add(1)
            .ok_or(DecodeEac3Error::FrameIndexOverflow)?;
    }

    if unit_index == 0 {
        return Err(DecodeEac3Error::EmptyStream);
    }
    Ok(decoder.finish_streaming()?)
}

#[allow(clippy::too_many_arguments)]
fn decode_internal_eac3_core<S, B, C, R, F>(
    stream: &[u8],
    config: PayloadDecoderConfig,
    validation_profile: JocValidationProfile,
    dither_values: &[f64],
    base_policy: InternalBasePolicy,
    dialnorm_mode: DialnormMode,
    streaming: bool,
    mut sink: S,
    mut base_sink: B,
    mut combined_sink: C,
    finish: F,
    mut timing: Option<&mut DecodeStageTiming>,
) -> Result<R, DecodeEac3Error>
where
    S: FnMut(usize, &JocMetadataFrame, &DecodedPayloadFrame) -> Result<(), DecodeEac3Error>,
    B: FnMut(usize, &DecodedAccessUnitPcm) -> Result<(), DecodeEac3Error>,
    C: FnMut(
        usize,
        &JocMetadataFrame,
        &DecodedPayloadFrame,
        &DecodedAccessUnitPcm,
    ) -> Result<(), DecodeEac3Error>,
    F: FnOnce(PayloadDecoder) -> Result<R, PayloadDecodeError>,
{
    let frame_index = index_syncframes(stream)?;
    let units = group_access_units(&frame_index)?;
    if units.is_empty() {
        return Err(DecodeEac3Error::EmptyStream);
    }
    let mut audio_decoder = JocAccessUnitPcmDecoder::new();
    audio_decoder.set_dialnorm_mode(dialnorm_mode);
    let oamd_profile = match validation_profile {
        JocValidationProfile::EtsiStrict => OamdParseProfile::EtsiStrict,
        JocValidationProfile::ObservedVendorCompat => OamdParseProfile::ObservedVendorCompat,
    };
    let mut decoder = if streaming {
        PayloadDecoder::streaming_with_oamd_profile(config, oamd_profile)
    } else {
        PayloadDecoder::with_oamd_profile(config, oamd_profile)
    };
    if timing.is_some() {
        decoder.enable_reconstruction_timing();
        audio_decoder.enable_stage_timing();
    }
    for (unit_index, unit) in units.into_iter().enumerate() {
        let frame_start = Instant::now();
        let decode_start = Instant::now();
        let pcm = audio_decoder.decode_with_policy(
            stream,
            &frame_index,
            unit,
            dither_values,
            base_policy,
        )?;
        if let Some(timing) = timing.as_mut() {
            let elapsed = decode_start.elapsed();
            timing.eac3_decode += elapsed;
            timing.record_eac3_frame(unit_index, elapsed, audio_decoder.take_stage_timing());
        }
        pcm.validate_joc_topology()?;
        if pcm.sample_rate != unit.sample_rate
            || pcm.samples != unit.samples
            || pcm.channels.is_empty()
            || pcm
                .channels
                .iter()
                .any(|channel| channel.len() != usize::from(unit.samples))
        {
            return Err(DecodeEac3Error::InvalidPcmLength {
                expected: usize::from(unit.samples),
            });
        }
        let metadata =
            required_metadata(stream, &frame_index, unit, unit_index, validation_profile)?;
        let parsed_oamd = parse_oamd_for_profile(&metadata.oamd, config.oamd, validation_profile)?;
        validate_complexity_index(metadata.complexity_index, parsed_oamd.prefix.object_count)?;
        let frame_number =
            u64::try_from(unit_index).map_err(|_| DecodeEac3Error::FrameIndexOverflow)?;
        let frame = measure_stage(
            timing
                .as_deref_mut()
                .map(|timing| &mut timing.joc_reconstruction),
            || {
                decoder.decode_frame(JocFrameInput {
                    sample_rate: unit.sample_rate,
                    downmix_pcm: &pcm.channels,
                    base_lfe_pcm: pcm.lfe.as_deref(),
                    joc_payload: &metadata.joc,
                    oamd_payload: &metadata.oamd,
                    frame_index: frame_number,
                })
            },
        )?;
        if let Some(timing) = timing.as_mut() {
            timing
                .reconstruction_stages
                .add_assign(&decoder.take_reconstruction_timing());
        }
        sink(unit_index, &metadata, &frame)?;
        base_sink(unit_index, &pcm)?;
        combined_sink(unit_index, &metadata, &frame, &pcm)?;
        if let Some(timing) = timing.as_mut() {
            if timing.collect_frame_times {
                timing.frame_times.push(frame_start.elapsed());
            }
        }
    }
    Ok(finish(decoder)?)
}

fn measure_stage<T, E>(
    target: Option<&mut std::time::Duration>,
    operation: impl FnOnce() -> Result<T, E>,
) -> Result<T, E> {
    let origin = Instant::now();
    measure_stage_with_clock(target, || origin.elapsed(), operation)
}

fn measure_stage_with_clock<T, E>(
    target: Option<&mut std::time::Duration>,
    mut now: impl FnMut() -> std::time::Duration,
    operation: impl FnOnce() -> Result<T, E>,
) -> Result<T, E> {
    let start = if target.is_some() { Some(now()) } else { None };
    let result = operation();
    if let (Some(target), Some(start)) = (target, start) {
        *target += now().saturating_sub(start);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::{
        DecodeEac3Error, ValidationProfileRequest,
        decode_internal_eac3_reader_with_base_sink_and_policy_request, measure_stage_with_clock,
    };
    use openjoc_eac3::InternalBasePolicy;
    use openjoc_oamd::OamdDecoderConfig;
    use openjoc_scene::PayloadDecoderConfig;
    use std::{io::Cursor, time::Duration};

    #[test]
    fn reconstruction_timing_scope_excludes_the_delivery_callback() {
        let mut ticks = [Duration::from_millis(10), Duration::from_millis(17)].into_iter();
        let mut measured = Duration::ZERO;
        let mut events = Vec::new();
        let frame = measure_stage_with_clock(
            Some(&mut measured),
            || ticks.next().expect("bounded clock read"),
            || {
                events.push("decode");
                Ok::<u8, ()>(7)
            },
        )
        .expect("decode stage");
        events.push("sink");

        assert_eq!(frame, 7);
        assert_eq!(measured, Duration::from_millis(7));
        assert_eq!(events, ["decode", "sink"]);
        assert_eq!(ticks.next(), None, "sink must not read the stage clock");
    }

    #[test]
    fn reports_signaled_extension_without_emdf_as_actionable_error() {
        let error = DecodeEac3Error::JocExtensionWithoutMetadata {
            access_unit: 4,
            complexity_index: 16,
        };
        assert_eq!(
            error.to_string(),
            "JOC extension complexity index 16 is signaled in access unit 4, but its required OAMD/JOC EMDF metadata is absent"
        );
    }

    #[test]
    fn direct_reader_preserves_empty_stream_boundary() {
        let result = decode_internal_eac3_reader_with_base_sink_and_policy_request(
            Cursor::new(Vec::<u8>::new()),
            64,
            PayloadDecoderConfig {
                reference_screen: None,
                oamd: OamdDecoderConfig {
                    trim_configuration_count: None,
                },
            },
            ValidationProfileRequest::EtsiStrict,
            &[],
            InternalBasePolicy::CurrentDefault,
            |_index, _metadata, _frame| Ok(()),
            |_index, _pcm| Ok(()),
        );
        assert!(matches!(result, Err(DecodeEac3Error::EmptyStream)));
    }
}
