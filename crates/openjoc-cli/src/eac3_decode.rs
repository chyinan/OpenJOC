// pattern: Functional Core

use openjoc_eac3::{
    Eac3Error, JocAccessUnitPcmDecoder, JocMetadataFrame, extract_joc_access_unit,
    extract_joc_addbsi_access_unit, group_access_units, index_syncframes,
    validate_complexity_index,
};
use openjoc_oamd::{OamdError, parse_oamd_payload_with_config};
use openjoc_scene::{
    DecodedPayloadFrame, JocFrameInput, ObjectScene, PayloadDecodeError, PayloadDecoder,
    PayloadDecoderConfig,
};
use openjoc_wave::WavePcm;
use std::fmt;

#[derive(Debug)]
pub enum DecodeEac3Error {
    Eac3(Eac3Error),
    Oamd(OamdError),
    Payload(PayloadDecodeError),
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
            Self::Eac3(error) => write!(formatter, "failed to decode E-AC-3 frontend: {error}"),
            Self::Oamd(error) => write!(formatter, "failed to validate OAMD profile: {error}"),
            Self::Payload(error) => {
                write!(formatter, "failed to reconstruct object frame: {error}")
            }
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

impl std::error::Error for DecodeEac3Error {}

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

fn required_metadata(
    stream: &[u8],
    frames: &[openjoc_eac3::SyncframeIndexEntry],
    unit: openjoc_eac3::AccessUnitIndex,
    access_unit: usize,
) -> Result<JocMetadataFrame, DecodeEac3Error> {
    if let Some(metadata) = extract_joc_access_unit(stream, frames, unit)? {
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

/// Aligns already decoded channel PCM with size-bounded E-AC-3 JOC metadata.
///
/// Aligns decoded channel PCM with E-AC-3 JOC metadata and lends each
/// successfully reconstructed frame to a sink immediately.
///
/// The codec scene still retains its renderer-independent PCM until the
/// separate metadata-only/PCM-file-sink increment lands. This API removes the
/// additional all-frame `DecodedPayloadFrame` retention from callers such as
/// the CLI debug exporter.
///
/// # Errors
/// Returns the same checked frontend, metadata, timing, PCM-shape, or
/// reconstruction error as this module's E-AC-3 frontend, or the sink error after a
/// frame has been committed.
pub fn decode_aligned_eac3_with_sink<S>(
    stream: &[u8],
    downmix: &WavePcm,
    config: PayloadDecoderConfig,
    mut sink: S,
) -> Result<ObjectScene, DecodeEac3Error>
where
    S: FnMut(usize, &DecodedPayloadFrame) -> Result<(), DecodeEac3Error>,
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

    let mut decoder = PayloadDecoder::new(config);
    let mut sample_offset = 0_usize;
    for (unit_index, unit) in units.into_iter().enumerate() {
        if unit.sample_rate != downmix.sample_rate {
            return Err(DecodeEac3Error::SampleRateMismatch {
                pcm: downmix.sample_rate,
                stream: unit.sample_rate,
            });
        }
        let metadata = required_metadata(stream, &frame_index, unit, unit_index)?;
        let parsed_oamd = parse_oamd_payload_with_config(&metadata.oamd, config.oamd)?;
        validate_complexity_index(metadata.complexity_index, parsed_oamd.prefix.object_count)?;
        let end = sample_offset
            .checked_add(usize::from(unit.samples))
            .ok_or(DecodeEac3Error::SampleCountOverflow)?;
        let frame_pcm = downmix
            .channels
            .iter()
            .map(|channel| channel[sample_offset..end].to_vec())
            .collect::<Vec<_>>();
        let frame_number =
            u64::try_from(unit_index).map_err(|_| DecodeEac3Error::FrameIndexOverflow)?;
        decoder.decode_frame_with(
            JocFrameInput {
                sample_rate: unit.sample_rate,
                downmix_pcm: &frame_pcm,
                joc_payload: &metadata.joc,
                oamd_payload: &metadata.oamd,
                frame_index: frame_number,
            },
            |frame| sink(unit_index, frame),
        )?;
        sample_offset = end;
    }
    Ok(decoder.finish()?)
}

/// Decodes the normative JOC elementary-stream audio path without an external
/// base decoder. TS 103 420 E.3 permits exactly I0 and optional D0; the
/// E-AC-3 access-unit decoder merges those channel locations before the PCM is
/// passed to the JOC/ObjectScene boundary.
///
/// Decodes the normative E-AC-3 base path and lends each reconstructed JOC
/// frame to a sink immediately. The base decoder remains an independently
/// implemented, fidelity-unverified path until a legal real vector passes the
/// required comparison.
pub fn decode_internal_eac3_with_sink<S>(
    stream: &[u8],
    config: PayloadDecoderConfig,
    dither_values: &[f64],
    mut sink: S,
) -> Result<ObjectScene, DecodeEac3Error>
where
    S: FnMut(usize, &DecodedPayloadFrame) -> Result<(), DecodeEac3Error>,
{
    let frame_index = index_syncframes(stream)?;
    let units = group_access_units(&frame_index)?;
    if units.is_empty() {
        return Err(DecodeEac3Error::EmptyStream);
    }
    let mut audio_decoder = JocAccessUnitPcmDecoder::new();
    let mut decoder = PayloadDecoder::new(config);
    for (unit_index, unit) in units.into_iter().enumerate() {
        let pcm = audio_decoder.decode(stream, &frame_index, unit, dither_values)?;
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
        let metadata = required_metadata(stream, &frame_index, unit, unit_index)?;
        let parsed_oamd = parse_oamd_payload_with_config(&metadata.oamd, config.oamd)?;
        validate_complexity_index(metadata.complexity_index, parsed_oamd.prefix.object_count)?;
        let frame_number =
            u64::try_from(unit_index).map_err(|_| DecodeEac3Error::FrameIndexOverflow)?;
        decoder.decode_frame_with(
            JocFrameInput {
                sample_rate: unit.sample_rate,
                downmix_pcm: &pcm.channels,
                joc_payload: &metadata.joc,
                oamd_payload: &metadata.oamd,
                frame_index: frame_number,
            },
            |frame| sink(unit_index, frame),
        )?;
    }
    Ok(decoder.finish()?)
}

#[cfg(test)]
mod tests {
    use super::DecodeEac3Error;

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
}
