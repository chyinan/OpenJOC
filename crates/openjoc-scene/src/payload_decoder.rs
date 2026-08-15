// pattern: Functional Core

use crate::{
    ObjectScene, ProgrammeLayout, ProgrammeLayoutError, SampleRange, SceneBuildError, SceneBuilder,
    StreamingSceneSummary,
};
use openjoc_joc::{DecodedJocFrame, JocDecodeError, JocDecoderState, JocFrame, parse_joc_payload};
use openjoc_oamd::{
    OAMD_PAYLOAD_ID, OamdDecoderConfig, OamdError, OamdParseProfile, OamdPayload, ReferenceScreen,
    parse_oamd_payload_with_config, parse_oamd_payload_with_profile,
};
use std::fmt;

/// Low-level frame boundary required by the engineering specification.
#[derive(Clone, Copy, Debug)]
pub struct JocFrameInput<'a> {
    pub sample_rate: u32,
    pub downmix_pcm: &'a [Vec<f64>],
    pub base_lfe_pcm: Option<&'a [f64]>,
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
    /// Sequential codec-frame identity, retained independently of metadata
    /// object identity.
    pub frame_index: u64,
    pub sample_rate: u32,
    /// Absolute half-open PCM interval for this committed frame.
    pub sample_range: SampleRange,
    pub joc: JocFrame,
    pub oamd: OamdPayload,
    pub decoded: DecodedJocFrame,
    pub programme_layout: ProgrammeLayout,
}

/// Failures in the raw payload-to-scene orchestration boundary.
#[derive(Debug)]
pub enum PayloadDecodeError {
    Joc(JocDecodeError),
    Oamd(OamdError),
    Scene(SceneBuildError),
    ProgrammeLayout(ProgrammeLayoutError),
    UnexpectedFrameIndex { expected: u64, actual: u64 },
    SampleRateChanged { expected: u32, actual: u32 },
    FrameIndexOverflow,
    SampleRangeOverflow,
    EmptyStream,
}

impl fmt::Display for PayloadDecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Joc(error) => write!(formatter, "failed to decode JOC frame: {error}"),
            Self::Oamd(error) => write!(formatter, "failed to decode OAMD frame: {error}"),
            Self::Scene(error) => write!(formatter, "failed to assemble object scene: {error}"),
            Self::ProgrammeLayout(error) => {
                write!(
                    formatter,
                    "failed to bind OAMD/JOC programme layout: {error}"
                )
            }
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
            Self::FrameIndexOverflow => formatter.write_str("payload frame index overflow"),
            Self::SampleRangeOverflow => formatter.write_str("payload sample range overflow"),
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

impl From<ProgrammeLayoutError> for PayloadDecodeError {
    fn from(value: ProgrammeLayoutError) -> Self {
        Self::ProgrammeLayout(value)
    }
}

/// Stateful, atomic decoder for aligned downmix/JOC/OAMD frames.
#[derive(Clone, Debug)]
pub struct PayloadDecoder {
    config: PayloadDecoderConfig,
    oamd_profile: OamdParseProfile,
    joc: JocDecoderState,
    builder: Option<SceneBuilder>,
    streaming: bool,
    sample_rate: Option<u32>,
    next_frame_index: u64,
    next_sample: u64,
}

impl PayloadDecoder {
    /// Creates an empty payload decoder with normative zero matrix/QMF state.
    #[must_use]
    pub fn new(config: PayloadDecoderConfig) -> Self {
        Self {
            config,
            oamd_profile: OamdParseProfile::EtsiStrict,
            joc: JocDecoderState::new(),
            builder: None,
            streaming: false,
            sample_rate: None,
            next_frame_index: 0,
            next_sample: 0,
        }
    }

    /// Creates a decoder with an explicit OAMD parser profile. The default
    /// [`Self::new`] constructor remains ETSI strict.
    #[must_use]
    pub fn with_oamd_profile(config: PayloadDecoderConfig, profile: OamdParseProfile) -> Self {
        Self {
            oamd_profile: profile,
            ..Self::new(config)
        }
    }

    /// Creates a decoder that validates and emits each frame without
    /// retaining programme-duration scene history.
    #[must_use]
    pub fn streaming(config: PayloadDecoderConfig) -> Self {
        Self {
            streaming: true,
            ..Self::new(config)
        }
    }

    /// Streaming constructor with an explicit OAMD validation profile.
    #[must_use]
    pub fn streaming_with_oamd_profile(
        config: PayloadDecoderConfig,
        profile: OamdParseProfile,
    ) -> Self {
        Self {
            streaming: true,
            ..Self::with_oamd_profile(config, profile)
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
        if let Some(expected) = self.sample_rate {
            if input.sample_rate != expected {
                return Err(PayloadDecodeError::SampleRateChanged {
                    expected,
                    actual: input.sample_rate,
                });
            }
        }
        let joc = parse_joc_payload(input.joc_payload).map_err(JocDecodeError::Parse)?;
        let oamd = match self.oamd_profile {
            OamdParseProfile::EtsiStrict => {
                parse_oamd_payload_with_config(input.oamd_payload, self.config.oamd)?
            }
            OamdParseProfile::DolbyVendorCompat => parse_oamd_payload_with_profile(
                input.oamd_payload,
                self.config.oamd,
                OamdParseProfile::DolbyVendorCompat,
                OAMD_PAYLOAD_ID,
            )?,
        };
        let layout = ProgrammeLayout::from_prefix(&oamd.prefix)?;
        layout.validate_reconstruction_basis(usize::from(joc.header.object_count))?;

        let mut next_joc = self.joc.clone();
        let decoded = next_joc.decode_pcm_frame(&joc, input.downmix_pcm)?;
        let frame_samples = input.downmix_pcm.first().map_or(0, Vec::len);
        let sample_range_end = self
            .next_sample
            .checked_add(
                u64::try_from(frame_samples)
                    .map_err(|_| PayloadDecodeError::SampleRangeOverflow)?,
            )
            .ok_or(PayloadDecodeError::SampleRangeOverflow)?;
        let sample_range = SampleRange::new(self.next_sample, sample_range_end)
            .map_err(|_| PayloadDecodeError::SampleRangeOverflow)?;
        let next_frame_index = self
            .next_frame_index
            .checked_add(1)
            .ok_or(PayloadDecodeError::FrameIndexOverflow)?;
        if let Some(builder) = self.builder.as_mut() {
            builder.append_frame_with_layout(
                &decoded.reconstruction_basis.rows,
                input.base_lfe_pcm,
                &oamd,
                self.config.reference_screen,
                &layout,
            )?;
        } else {
            let mut builder = if self.streaming {
                SceneBuilder::new_streaming(input.sample_rate, &oamd.prefix)?
            } else {
                SceneBuilder::new(input.sample_rate, &oamd.prefix)?
            };
            builder.append_frame_with_layout(
                &decoded.reconstruction_basis.rows,
                input.base_lfe_pcm,
                &oamd,
                self.config.reference_screen,
                &layout,
            )?;
            self.builder = Some(builder);
        }

        self.joc = next_joc;
        self.sample_rate = Some(input.sample_rate);
        self.next_frame_index = next_frame_index;
        self.next_sample = sample_range_end;
        Ok(DecodedPayloadFrame {
            frame_index: input.frame_index,
            sample_rate: input.sample_rate,
            sample_range,
            joc,
            oamd,
            decoded,
            programme_layout: layout,
        })
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

    /// Decodes one frame under a caller-selected validation profile while
    /// preserving the decoder's prior profile for the next frame.
    ///
    /// This is used by a single-pass AUTO policy: the caller selects strict or
    /// compatibility after validating the current lossless carrier, without
    /// turning either profile into a spatial-rendering algorithm.
    pub fn decode_frame_with_profile<S, E>(
        &mut self,
        input: JocFrameInput<'_>,
        profile: OamdParseProfile,
        mut sink: S,
    ) -> Result<(), E>
    where
        S: FnMut(&DecodedPayloadFrame) -> Result<(), E>,
        E: From<PayloadDecodeError>,
    {
        let previous = self.oamd_profile;
        self.oamd_profile = profile;
        let result = self
            .decode_frame(input)
            .map_err(E::from)
            .and_then(|frame| sink(&frame));
        self.oamd_profile = previous;
        result
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

    /// Finalizes a streaming decoder without materializing a full scene.
    pub fn finish_streaming(self) -> Result<StreamingSceneSummary, PayloadDecodeError> {
        self.builder
            .ok_or(PayloadDecodeError::EmptyStream)?
            .finish_streaming()
            .map_err(Into::into)
    }
}
