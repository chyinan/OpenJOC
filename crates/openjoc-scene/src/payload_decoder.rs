// pattern: Functional Core

use crate::{ObjectScene, SceneBuildError, SceneBuilder};
use openjoc_joc::{DecodedJocFrame, JocDecodeError, JocDecoderState, JocFrame, parse_joc_payload};
use openjoc_oamd::{
    OamdDecoderConfig, OamdError, OamdPayload, ReferenceScreen, parse_oamd_payload_with_config,
};
use std::fmt;

/// Low-level frame boundary required by the engineering specification.
#[derive(Clone, Copy, Debug)]
pub struct JocFrameInput<'a> {
    pub sample_rate: u32,
    pub downmix_pcm: &'a [Vec<f64>],
    pub joc_payload: &'a [u8],
    pub oamd_payload: &'a [u8],
    pub frame_index: u64,
}

/// Decoder-interface configuration not carried by the two payloads.
#[derive(Clone, Copy, Debug)]
pub struct PayloadDecoderConfig {
    pub reference_screen: Option<ReferenceScreen>,
    pub oamd: OamdDecoderConfig,
}

/// Retained syntax and reconstruction stages for one decoded payload frame.
#[derive(Clone, Debug, PartialEq)]
pub struct DecodedPayloadFrame {
    pub joc: JocFrame,
    pub oamd: OamdPayload,
    pub decoded: DecodedJocFrame,
}

/// Failures in the raw payload-to-scene orchestration boundary.
#[derive(Debug)]
pub enum PayloadDecodeError {
    Joc(JocDecodeError),
    Oamd(OamdError),
    Scene(SceneBuildError),
    UnexpectedFrameIndex { expected: u64, actual: u64 },
    SampleRateChanged { expected: u32, actual: u32 },
    ObjectCountMismatch { joc: u8, oamd: u16 },
    FrameIndexOverflow,
    EmptyStream,
}

impl fmt::Display for PayloadDecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Joc(error) => write!(formatter, "failed to decode JOC frame: {error}"),
            Self::Oamd(error) => write!(formatter, "failed to decode OAMD frame: {error}"),
            Self::Scene(error) => write!(formatter, "failed to assemble object scene: {error}"),
            Self::UnexpectedFrameIndex { expected, actual } => {
                write!(
                    formatter,
                    "expected frame {expected}, received frame {actual}"
                )
            }
            Self::SampleRateChanged { expected, actual } => write!(
                formatter,
                "sample rate changed from {expected} Hz to {actual} Hz without reset"
            ),
            Self::ObjectCountMismatch { joc, oamd } => {
                write!(
                    formatter,
                    "JOC declares {joc} objects but OAMD declares {oamd}"
                )
            }
            Self::FrameIndexOverflow => formatter.write_str("payload frame index overflow"),
            Self::EmptyStream => formatter.write_str("cannot finish an empty payload stream"),
        }
    }
}

impl std::error::Error for PayloadDecodeError {}

impl From<JocDecodeError> for PayloadDecodeError {
    fn from(value: JocDecodeError) -> Self {
        Self::Joc(value)
    }
}

impl From<OamdError> for PayloadDecodeError {
    fn from(value: OamdError) -> Self {
        Self::Oamd(value)
    }
}

impl From<SceneBuildError> for PayloadDecodeError {
    fn from(value: SceneBuildError) -> Self {
        Self::Scene(value)
    }
}

/// Stateful, atomic decoder for aligned downmix/JOC/OAMD frames.
#[derive(Clone, Debug)]
pub struct PayloadDecoder {
    config: PayloadDecoderConfig,
    joc: JocDecoderState,
    builder: Option<SceneBuilder>,
    sample_rate: Option<u32>,
    next_frame_index: u64,
}

impl PayloadDecoder {
    /// Creates an empty payload decoder with normative zero matrix/QMF state.
    #[must_use]
    pub fn new(config: PayloadDecoderConfig) -> Self {
        Self {
            config,
            joc: JocDecoderState::new(),
            builder: None,
            sample_rate: None,
            next_frame_index: 0,
        }
    }

    /// Parses and decodes one aligned payload frame, committing only on success.
    ///
    /// # Errors
    /// Returns [`PayloadDecodeError`] for syntax, reconstruction, alignment,
    /// configuration, or scene-invariant failures.
    pub fn decode_frame(
        &mut self,
        input: JocFrameInput<'_>,
    ) -> Result<DecodedPayloadFrame, PayloadDecodeError> {
        if input.frame_index != self.next_frame_index {
            return Err(PayloadDecodeError::UnexpectedFrameIndex {
                expected: self.next_frame_index,
                actual: input.frame_index,
            });
        }
        if let Some(expected) = self.sample_rate
            && input.sample_rate != expected
        {
            return Err(PayloadDecodeError::SampleRateChanged {
                expected,
                actual: input.sample_rate,
            });
        }
        let joc = parse_joc_payload(input.joc_payload).map_err(JocDecodeError::Parse)?;
        let oamd = parse_oamd_payload_with_config(input.oamd_payload, self.config.oamd)?;
        if u16::from(joc.header.object_count) != oamd.prefix.object_count {
            return Err(PayloadDecodeError::ObjectCountMismatch {
                joc: joc.header.object_count,
                oamd: oamd.prefix.object_count,
            });
        }

        let mut next_joc = self.joc.clone();
        let decoded = next_joc.decode_pcm_frame(&joc, input.downmix_pcm)?;
        let next_frame_index = self
            .next_frame_index
            .checked_add(1)
            .ok_or(PayloadDecodeError::FrameIndexOverflow)?;
        if let Some(builder) = self.builder.as_mut() {
            builder.append_frame(&decoded.object_pcm, &oamd, self.config.reference_screen)?;
        } else {
            let mut builder = SceneBuilder::new(input.sample_rate, &oamd.prefix)?;
            builder.append_frame(&decoded.object_pcm, &oamd, self.config.reference_screen)?;
            self.builder = Some(builder);
        }

        self.joc = next_joc;
        self.sample_rate = Some(input.sample_rate);
        self.next_frame_index = next_frame_index;
        Ok(DecodedPayloadFrame { joc, oamd, decoded })
    }

    /// Decodes one frame and lends the committed frame result to a sink.
    ///
    /// The sink is called only after the JOC state, scene state, sample rate,
    /// and frame counter have been committed. This keeps the codec's atomic
    /// retry guarantee while allowing callers to consume debug or PCM data
    /// immediately instead of retaining every frame result. A sink failure is
    /// returned to the caller; it cannot roll back an already committed codec
    /// frame, so callers should write into a transactional output directory.
    ///
    /// # Errors
    /// Returns [`PayloadDecodeError`] (converted into `E`) for decoder failure,
    /// or the sink's own error after a successful frame commit.
    pub fn decode_frame_with<S, E>(
        &mut self,
        input: JocFrameInput<'_>,
        mut sink: S,
    ) -> Result<(), E>
    where
        S: FnMut(&DecodedPayloadFrame) -> Result<(), E>,
        E: From<PayloadDecodeError>,
    {
        let frame = self.decode_frame(input).map_err(E::from)?;
        sink(&frame)
    }

    /// Finalizes the accumulated renderer-independent object scene.
    ///
    /// # Errors
    /// Returns [`PayloadDecodeError::EmptyStream`] before any successful frame,
    /// or a scene error if final validation fails.
    pub fn finish(self) -> Result<ObjectScene, PayloadDecodeError> {
        self.builder
            .ok_or(PayloadDecodeError::EmptyStream)?
            .finish()
            .map_err(Into::into)
    }
}
