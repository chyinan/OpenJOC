// pattern: Functional Core

use openjoc_eac3::{
    Eac3Error, extract_aux_joc_access_unit, group_access_units, index_syncframes,
    validate_complexity_index,
};
use openjoc_oamd::{OamdError, parse_oamd_payload_with_config};
use openjoc_scene::{
    DecodedPayloadFrame, JocFrameInput, ObjectScene, PayloadDecodeError, PayloadDecoder,
    PayloadDecoderConfig,
};
use openjoc_wave::WavePcm;
use std::fmt;

pub struct DecodedEac3 {
    pub scene: ObjectScene,
    pub frames: Vec<DecodedPayloadFrame>,
}

#[derive(Debug)]
pub enum DecodeEac3Error {
    Eac3(Eac3Error),
    Oamd(OamdError),
    Payload(PayloadDecodeError),
    EmptyStream,
    SampleCountOverflow,
    InvalidPcmLength { expected: usize },
    SampleRateMismatch { pcm: u32, stream: u32 },
    MissingMetadata { access_unit: usize },
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

/// Aligns already decoded channel PCM with size-bounded E-AC-3 JOC metadata.
///
/// # Errors
/// Returns a checked frontend, metadata, timing, PCM-shape, or reconstruction
/// error without advancing partially decoded scene state.
pub fn decode_aligned_eac3(
    stream: &[u8],
    downmix: &WavePcm,
    config: PayloadDecoderConfig,
) -> Result<DecodedEac3, DecodeEac3Error> {
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
    let mut decoded_frames = Vec::with_capacity(units.len());
    let mut sample_offset = 0_usize;
    for (unit_index, unit) in units.into_iter().enumerate() {
        if unit.sample_rate != downmix.sample_rate {
            return Err(DecodeEac3Error::SampleRateMismatch {
                pcm: downmix.sample_rate,
                stream: unit.sample_rate,
            });
        }
        let metadata = extract_aux_joc_access_unit(stream, &frame_index, unit)?.ok_or(
            DecodeEac3Error::MissingMetadata {
                access_unit: unit_index,
            },
        )?;
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
        decoded_frames.push(decoder.decode_frame(JocFrameInput {
            sample_rate: unit.sample_rate,
            downmix_pcm: &frame_pcm,
            joc_payload: &metadata.joc,
            oamd_payload: &metadata.oamd,
            frame_index: frame_number,
        })?);
        sample_offset = end;
    }
    Ok(DecodedEac3 {
        scene: decoder.finish()?,
        frames: decoded_frames,
    })
}
