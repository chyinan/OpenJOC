//! Headless, packet-oriented OpenJOC integration API.
//!
//! The session in this crate is the public Rust integration boundary. It owns
//! the existing E-AC-3 frontend, JOC/OAMD decoder, reconstruction timeline,
//! and the existing spatial bridge. It does not open files, write PCM
//! containers, print diagnostics, or depend on the CLI crate.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::too_many_lines)]

use openjoc_eac3::{
    ChannelLocation, DecodedAccessUnitPcm, DownmixMetadata, InternalBasePolicy,
    JocAccessUnitPcmDecoder, JocMetadataFrame, extract_joc_access_unit_for_profile,
    group_access_units, index_syncframes, parse_joc_access_unit, validate_complexity_index,
    validate_joc_access_unit,
};
use openjoc_emdf::JocValidationProfile;
use openjoc_joc::{ReconstructionBasis, ReconstructionOutputTimeline};
use openjoc_oamd::{
    OAMD_PAYLOAD_ID, OamdDecoderConfig, OamdParseProfile, parse_oamd_payload_with_config,
    parse_oamd_payload_with_profile,
};
use openjoc_render::{
    BinauralRenderer, BinauralSourceBlock, CartesianPosition, HrirBank, HrirEntry, HrirEntryId,
    SourceId, StaticBinauralSource,
};
use openjoc_scene::{
    BaseFullBandCoordinate, BridgeControlAssembler, DecodedPayloadFrame, JocFrameInput,
    JocSpatialBridge, JocSpatialFrameBridge, PayloadDecoder, PayloadDecoderConfig,
    SemanticChannelLayout, SpeakerLayoutPreset,
};
use openjoc_sofa::{SofaLoadLimits, parse_simple_free_field_hrir, resolve_hrir};
use std::{collections::VecDeque, fmt};

/// The first public C ABI is intentionally experimental. This is separate
/// from the Rust package version and may evolve within the 0.7 line.
pub const API_MATURITY: &str = "experimental";
/// The declared QMF/Base-RB reconstruction delay in samples.
pub const QMF_LATENCY_SAMPLES: usize = ReconstructionOutputTimeline::qmf_latency_samples();
/// The canonical v1 PCM sample format.
pub const PCM_SAMPLE_FORMAT: PcmSampleFormat = PcmSampleFormat::F32;

/// Public rendering choice. Binaural is static SOFA virtualization of the
/// selected virtual speaker layout; it does not claim direct-object fidelity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderMode {
    Speaker,
    Stereo,
    Binaural,
}

impl RenderMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Speaker => "speaker",
            Self::Stereo => "stereo",
            Self::Binaural => "binaural",
        }
    }
}

/// Channel-based stereo policy. `Auto` follows the E-AC-3 `dmixmod` field.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DownmixPolicy {
    #[default]
    Auto,
    LoRo,
    LtRt,
}

impl DownmixPolicy {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::LoRo => "loro",
            Self::LtRt => "ltrt",
        }
    }
}

/// Public dynamic-range policy mapped directly to the existing E-AC-3 core.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DrcPolicy {
    Disabled,
    #[default]
    Line,
    Rf,
    Custom {
        boost_percent: u8,
        cut_percent: u8,
    },
}

impl DrcPolicy {
    fn internal(self) -> InternalBasePolicy {
        let control = match self {
            Self::Disabled => openjoc_eac3::DynamicRangeControl::Disabled,
            Self::Line => openjoc_eac3::DynamicRangeControl::Line,
            Self::Rf => openjoc_eac3::DynamicRangeControl::Rf,
            Self::Custom {
                boost_percent,
                cut_percent,
            } => openjoc_eac3::DynamicRangeControl::Custom {
                boost_percent,
                cut_percent,
            },
        };
        InternalBasePolicy::DynamicRange(control)
    }
}

/// Validation profile selection. Auto selects one profile for a session and
/// rejects a later profile change instead of silently changing semantics.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ValidationProfile {
    #[default]
    Auto,
    EtsiStrict,
    ObservedVendorCompat,
}

/// Binaural LFE handling at the physical stereo output boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BinauralLfePolicy {
    Exclude,
    EqualPowerDualMono,
}

/// In-memory SOFA configuration. The parser copies validated HRIR data into
/// the renderer; no path or file handle is retained by a session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BinauralConfig {
    pub sofa_bytes: Vec<u8>,
    pub virtual_layout: String,
    pub lfe_policy: BinauralLfePolicy,
}

/// Stable high-level session configuration.
#[derive(Clone, Debug)]
pub struct OpenJocConfig {
    pub render_mode: RenderMode,
    /// Speaker layout or virtual speaker layout. Stereo always uses `2.0`.
    pub speaker_layout: String,
    pub downmix: DownmixPolicy,
    pub drc: DrcPolicy,
    pub validation_profile: ValidationProfile,
    pub oamd: OamdDecoderConfig,
    pub binaural: Option<BinauralConfig>,
}

impl Default for OpenJocConfig {
    fn default() -> Self {
        Self {
            render_mode: RenderMode::Speaker,
            speaker_layout: "5.1".to_owned(),
            downmix: DownmixPolicy::Auto,
            drc: DrcPolicy::Line,
            validation_profile: ValidationProfile::Auto,
            oamd: OamdDecoderConfig::default(),
            binaural: None,
        }
    }
}

impl OpenJocConfig {
    fn effective_layout(&self) -> &str {
        if self.render_mode == RenderMode::Stereo {
            "2.0"
        } else if self.render_mode == RenderMode::Binaural {
            self.binaural
                .as_ref()
                .map_or(self.speaker_layout.as_str(), |binaural| {
                    binaural.virtual_layout.as_str()
                })
        } else {
            self.speaker_layout.as_str()
        }
    }

    fn validate(&self) -> Result<(), OpenJocError> {
        SpeakerLayoutPreset::for_name(self.effective_layout())
            .map_err(|error| OpenJocError::InvalidConfig(error.to_string()))?;
        if self.render_mode == RenderMode::Binaural && self.binaural.is_none() {
            return Err(OpenJocError::InvalidConfig(
                "binaural mode requires an in-memory SOFA configuration".to_owned(),
            ));
        }
        if self.render_mode != RenderMode::Stereo && self.downmix != DownmixPolicy::Auto {
            return Err(OpenJocError::InvalidConfig(
                "explicit downmix policy is only valid for stereo output".to_owned(),
            ));
        }
        Ok(())
    }
}

/// Borrowed compressed input. A packet is exactly one complete JOC access
/// unit: independent substream zero and optional dependent substream zero.
#[derive(Clone, Copy, Debug)]
pub struct OpenJocPacket<'a> {
    pub data: &'a [u8],
    /// Sample-domain PTS for the first sample in this packet. The sample time
    /// base is the decoded stream rate; `None` means untimestamped input.
    pub pts_samples: Option<i64>,
    pub discontinuity: bool,
    pub preroll: bool,
}

/// Canonical output sample representation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcmSampleFormat {
    F32,
}

/// Semantic output layout description.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpenJocOutputInfo {
    pub sample_format: PcmSampleFormat,
    pub sample_rate: Option<u32>,
    pub channel_count: usize,
    pub channel_labels: Vec<String>,
    pub layout_name: String,
    pub render_mode: RenderMode,
    pub latency_samples: usize,
}

/// One owned interleaved PCM frame. The session owns the frame after
/// `receive_frame`; Rust callers may retain it indefinitely.
#[derive(Clone, Debug, PartialEq)]
pub struct OpenJocPcmFrame {
    pub sample_format: PcmSampleFormat,
    pub sample_rate: u32,
    pub channel_count: usize,
    pub channel_labels: Vec<String>,
    pub layout_name: String,
    pub render_mode: RenderMode,
    pub sample_count: usize,
    pub pts_samples: Option<i64>,
    pub interleaved_f32: Vec<f32>,
}

/// Non-error result of a push/drain operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpenJocStatus {
    Ok,
    NeedMoreInput,
    FrameAvailable,
    EndOfStream,
    OutputPending,
}

/// Structured failures from the headless API.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OpenJocError {
    InvalidConfig(String),
    InvalidPacket(String),
    Decode(String),
    Render(String),
    FormatChanged { expected: u32, actual: u32 },
    TimestampDiscontinuity { expected: i64, actual: i64 },
    ProfileChanged,
    EmptyStream,
    AlreadyDrained,
    OutputPending,
    Unsupported(String),
}

impl fmt::Display for OpenJocError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig(message) => {
                write!(formatter, "invalid OpenJOC configuration: {message}")
            }
            Self::InvalidPacket(message) => write!(formatter, "invalid OpenJOC packet: {message}"),
            Self::Decode(message) => write!(formatter, "OpenJOC decode error: {message}"),
            Self::Render(message) => write!(formatter, "OpenJOC render error: {message}"),
            Self::FormatChanged { expected, actual } => {
                write!(
                    formatter,
                    "sample rate changed from {expected} Hz to {actual} Hz"
                )
            }
            Self::TimestampDiscontinuity { expected, actual } => {
                write!(
                    formatter,
                    "timestamp discontinuity: expected {expected}, received {actual}"
                )
            }
            Self::ProfileChanged => {
                formatter.write_str("validation profile changed within a session")
            }
            Self::EmptyStream => formatter.write_str("cannot drain an empty OpenJOC stream"),
            Self::AlreadyDrained => formatter.write_str("OpenJOC session is already drained"),
            Self::OutputPending => {
                formatter.write_str("output must be received before pushing more input")
            }
            Self::Unsupported(message) => {
                write!(formatter, "unsupported OpenJOC operation: {message}")
            }
        }
    }
}

impl std::error::Error for OpenJocError {}

impl From<openjoc_eac3::Eac3Error> for OpenJocError {
    fn from(error: openjoc_eac3::Eac3Error) -> Self {
        Self::Decode(error.to_string())
    }
}

impl From<openjoc_scene::PayloadDecodeError> for OpenJocError {
    fn from(error: openjoc_scene::PayloadDecodeError) -> Self {
        Self::Decode(error.to_string())
    }
}

impl From<openjoc_scene::BridgeError> for OpenJocError {
    fn from(error: openjoc_scene::BridgeError) -> Self {
        Self::Render(error.to_string())
    }
}

impl From<openjoc_scene::BridgeControlAssemblyError> for OpenJocError {
    fn from(error: openjoc_scene::BridgeControlAssemblyError) -> Self {
        Self::Render(error.to_string())
    }
}

impl From<openjoc_scene::SpatialBridgeError> for OpenJocError {
    fn from(error: openjoc_scene::SpatialBridgeError) -> Self {
        Self::Render(error.to_string())
    }
}

impl From<openjoc_joc::ReconstructionTimelineError> for OpenJocError {
    fn from(error: openjoc_joc::ReconstructionTimelineError) -> Self {
        Self::Render(error.to_string())
    }
}

impl From<openjoc_sofa::SofaError> for OpenJocError {
    fn from(error: openjoc_sofa::SofaError) -> Self {
        Self::Render(error.to_string())
    }
}

impl From<openjoc_render::RenderError> for OpenJocError {
    fn from(error: openjoc_render::RenderError) -> Self {
        Self::Render(error.to_string())
    }
}

/// The one canonical headless decode/render session.
#[derive(Debug)]
pub struct OpenJocSession {
    config: OpenJocConfig,
    audio_decoder: JocAccessUnitPcmDecoder,
    payload_decoder: PayloadDecoder,
    speaker: SpeakerRenderer,
    binaural: Option<BinauralState>,
    output_queue: VecDeque<OpenJocPcmFrame>,
    selected_profile: Option<JocValidationProfile>,
    dither_values: Vec<f64>,
    sample_rate: Option<u32>,
    segment_pts: Option<i64>,
    next_input_sample: u64,
    last_output_end: u64,
    drained: bool,
}

impl OpenJocSession {
    /// Creates a validated, independent session. No process-global state is
    /// touched; separate sessions can run concurrently on separate threads.
    pub fn new(config: OpenJocConfig) -> Result<Self, OpenJocError> {
        config.validate()?;
        let layout = config.effective_layout().to_owned();
        let speaker = SpeakerRenderer::new(&layout, config.downmix)?;
        let binaural = config
            .binaural
            .as_ref()
            .map(BinauralState::new)
            .transpose()?;
        Ok(Self {
            payload_decoder: new_payload_decoder(&config),
            audio_decoder: JocAccessUnitPcmDecoder::new(),
            speaker,
            binaural,
            output_queue: VecDeque::new(),
            selected_profile: None,
            dither_values: dither_values(),
            sample_rate: None,
            segment_pts: None,
            next_input_sample: 0,
            last_output_end: 0,
            drained: false,
            config,
        })
    }

    /// Returns output semantics without exposing internal decoder structs.
    #[must_use]
    pub fn output_info(&self) -> OpenJocOutputInfo {
        let (layout_name, channel_labels) = self.output_layout_info();
        OpenJocOutputInfo {
            sample_format: PCM_SAMPLE_FORMAT,
            sample_rate: self.sample_rate,
            channel_count: channel_labels.len(),
            channel_labels,
            layout_name,
            render_mode: self.config.render_mode,
            latency_samples: QMF_LATENCY_SAMPLES,
        }
    }

    /// Returns the known deterministic decoder/reconstruction delay.
    #[must_use]
    pub const fn latency_samples(&self) -> usize {
        QMF_LATENCY_SAMPLES
    }

    /// Sends one complete access unit. Caller packet memory is borrowed only
    /// for this call; the session copies only decoded PCM into bounded state.
    pub fn push_packet(
        &mut self,
        packet: OpenJocPacket<'_>,
    ) -> Result<OpenJocStatus, OpenJocError> {
        if self.drained {
            return Err(OpenJocError::AlreadyDrained);
        }
        if !self.output_queue.is_empty() {
            return Ok(OpenJocStatus::OutputPending);
        }
        if packet.data.is_empty() {
            return Err(OpenJocError::InvalidPacket("packet is empty".to_owned()));
        }
        if packet.discontinuity {
            self.reset_stream_state();
        }
        self.check_timestamp(packet.pts_samples)?;
        let frames = index_syncframes(packet.data)?;
        let units = group_access_units(&frames)?;
        let unit = units.first().copied().ok_or(OpenJocError::InvalidPacket(
            "packet does not contain an access unit".to_owned(),
        ))?;
        if units.len() != 1 || unit.first_frame != 0 || unit.frame_count != frames.len() {
            return Err(OpenJocError::InvalidPacket(
                "a packet must contain exactly one complete JOC access unit".to_owned(),
            ));
        }
        if let Some(expected) = self.sample_rate {
            if expected != unit.sample_rate {
                return Err(OpenJocError::FormatChanged {
                    expected,
                    actual: unit.sample_rate,
                });
            }
        } else {
            self.sample_rate = Some(unit.sample_rate);
        }

        let pcm = self.audio_decoder.decode_with_policy(
            packet.data,
            &frames,
            unit,
            &self.dither_values,
            self.config.drc.internal(),
        )?;
        pcm.validate_joc_topology()?;
        let (metadata, profile, oamd_profile) = self.select_metadata(packet.data, &frames, unit)?;
        if let Some(previous) = self.selected_profile {
            if previous != profile {
                return Err(OpenJocError::ProfileChanged);
            }
        } else {
            self.selected_profile = Some(profile);
        }
        let parsed_oamd = parse_oamd_for_profile(&metadata.oamd, self.config.oamd, oamd_profile)
            .map_err(|error| OpenJocError::Decode(error.to_string()))?;
        validate_complexity_index(metadata.complexity_index, parsed_oamd.prefix.object_count)?;
        let frame_number = self.next_input_sample / u64::from(unit.samples);
        let input = JocFrameInput {
            sample_rate: unit.sample_rate,
            downmix_pcm: &pcm.channels,
            base_lfe_pcm: pcm.lfe.as_deref(),
            joc_payload: &metadata.joc,
            oamd_payload: &metadata.oamd,
            frame_index: frame_number,
        };
        let mut decoded = None;
        self.payload_decoder
            .decode_frame_with_profile(input, oamd_profile, |frame| {
                decoded = Some(frame.clone());
                Ok::<(), OpenJocError>(())
            })?;
        let frame = decoded.ok_or(OpenJocError::Decode(
            "payload decoder returned no frame".to_owned(),
        ))?;
        self.next_input_sample = self
            .next_input_sample
            .checked_add(u64::from(unit.samples))
            .ok_or_else(|| OpenJocError::Decode("sample timeline overflow".to_owned()))?;
        let rendered = self.speaker.render_frame_aligned(&frame, &pcm)?;
        self.emit_rendered(rendered)?;
        // `preroll` is retained as an explicit input fact for future seek
        // adapters. It is decoded normally in this first ABI because the
        // decoder cannot discard a delayed frame without a caller policy.
        let _ = packet.preroll;
        Ok(if self.output_queue.is_empty() {
            OpenJocStatus::NeedMoreInput
        } else {
            OpenJocStatus::FrameAvailable
        })
    }

    /// Receives one owned PCM frame. The queue is bounded to frames produced
    /// by one send/drain operation; callers should receive before pushing.
    pub fn receive_frame(&mut self) -> Option<OpenJocPcmFrame> {
        self.output_queue.pop_front()
    }

    /// Whether `drain` has completed and no further frame can be received.
    #[must_use]
    pub fn is_drained(&self) -> bool {
        self.drained && self.output_queue.is_empty()
    }

    /// Flushes delayed QMF/reconstruction and SOFA FIR tail output.
    pub fn drain(&mut self) -> Result<OpenJocStatus, OpenJocError> {
        if self.drained {
            return Ok(OpenJocStatus::EndOfStream);
        }
        if !self.output_queue.is_empty() {
            return Ok(OpenJocStatus::OutputPending);
        }
        if self.sample_rate.is_none() {
            self.drained = true;
            return Ok(OpenJocStatus::EndOfStream);
        }
        let payload =
            std::mem::replace(&mut self.payload_decoder, new_payload_decoder(&self.config));
        let (_, reconstruction_tail) = payload.finish_streaming_with_reconstruction_tail()?;
        let rendered = self
            .speaker
            .finish_with_reconstruction_tail(&reconstruction_tail)?;
        self.emit_rendered(rendered)?;
        if let Some(binaural) = self.binaural.as_mut() {
            let mut tail_start = self.last_output_end;
            for frame in binaural.drain_tail(self.sample_rate.unwrap_or(0), tail_start)? {
                tail_start = tail_start.saturating_add(frame.sample_count as u64);
                self.output_queue.push_back(self.to_pcm_frame(&frame)?);
            }
        }
        self.drained = true;
        Ok(if self.output_queue.is_empty() {
            OpenJocStatus::EndOfStream
        } else {
            OpenJocStatus::FrameAvailable
        })
    }

    /// Discards pending output and all stream-derived decoder state while
    /// retaining the immutable configuration.
    pub fn flush(&mut self) {
        self.reset_stream_state();
        self.output_queue.clear();
        self.drained = false;
    }

    /// Resets state for a new timeline/seek. Configuration and SOFA data stay
    /// prepared; no state from the previous stream is reused.
    pub fn reset(&mut self) {
        self.flush();
    }

    fn reset_stream_state(&mut self) {
        self.audio_decoder.reset();
        self.payload_decoder = new_payload_decoder(&self.config);
        self.speaker.reset();
        if let Some(binaural) = self.binaural.as_mut() {
            binaural.reset();
        }
        self.selected_profile = None;
        self.sample_rate = None;
        self.segment_pts = None;
        self.next_input_sample = 0;
        self.last_output_end = 0;
    }

    fn check_timestamp(&mut self, pts: Option<i64>) -> Result<(), OpenJocError> {
        let Some(pts) = pts else { return Ok(()) };
        if let Some(origin) = self.segment_pts {
            let offset = i64::try_from(self.next_input_sample).unwrap_or(i64::MAX);
            let expected = origin.saturating_add(offset);
            if expected != pts {
                return Err(OpenJocError::TimestampDiscontinuity {
                    expected,
                    actual: pts,
                });
            }
        } else {
            self.segment_pts = Some(pts);
        }
        Ok(())
    }

    fn select_metadata(
        &self,
        stream: &[u8],
        frames: &[openjoc_eac3::SyncframeIndexEntry],
        unit: openjoc_eac3::AccessUnitIndex,
    ) -> Result<(JocMetadataFrame, JocValidationProfile, OamdParseProfile), OpenJocError> {
        let try_profile = |profile| -> Result<(JocMetadataFrame, OamdParseProfile), OpenJocError> {
            let metadata = extract_joc_access_unit_for_profile(stream, frames, unit, profile)?
                .ok_or_else(|| OpenJocError::InvalidPacket("JOC metadata is absent".to_owned()))?;
            let oamd_profile = match profile {
                JocValidationProfile::EtsiStrict => {
                    parse_oamd_payload_with_config(&metadata.oamd, self.config.oamd)
                        .map(|_| OamdParseProfile::EtsiStrict)
                        .map_err(|error| OpenJocError::Decode(error.to_string()))?
                }
                JocValidationProfile::ObservedVendorCompat => parse_oamd_payload_with_profile(
                    &metadata.oamd,
                    self.config.oamd,
                    OamdParseProfile::ObservedVendorCompat,
                    OAMD_PAYLOAD_ID,
                )
                .map(|_| OamdParseProfile::ObservedVendorCompat)
                .map_err(|error| OpenJocError::Decode(error.to_string()))?,
            };
            Ok((metadata, oamd_profile))
        };
        match self.config.validation_profile {
            ValidationProfile::EtsiStrict => {
                let (metadata, oamd_profile) = try_profile(JocValidationProfile::EtsiStrict)?;
                Ok((metadata, JocValidationProfile::EtsiStrict, oamd_profile))
            }
            ValidationProfile::ObservedVendorCompat => {
                let (metadata, oamd_profile) =
                    try_profile(JocValidationProfile::ObservedVendorCompat)?;
                Ok((
                    metadata,
                    JocValidationProfile::ObservedVendorCompat,
                    oamd_profile,
                ))
            }
            ValidationProfile::Auto => {
                let parsed = parse_joc_access_unit(stream, frames, unit)?.ok_or_else(|| {
                    OpenJocError::InvalidPacket("JOC metadata is absent".to_owned())
                })?;
                if let Ok(metadata) =
                    validate_joc_access_unit(&parsed, JocValidationProfile::EtsiStrict)
                {
                    if let Ok(oamd_profile) =
                        parse_oamd_payload_with_config(&metadata.oamd, self.config.oamd)
                    {
                        let _ = oamd_profile;
                        return Ok((
                            metadata,
                            JocValidationProfile::EtsiStrict,
                            OamdParseProfile::EtsiStrict,
                        ));
                    }
                }
                let (metadata, oamd_profile) =
                    try_profile(JocValidationProfile::ObservedVendorCompat)?;
                Ok((
                    metadata,
                    JocValidationProfile::ObservedVendorCompat,
                    oamd_profile,
                ))
            }
        }
    }

    fn emit_rendered(&mut self, rendered: Vec<RenderedBlock>) -> Result<(), OpenJocError> {
        for block in rendered {
            let frame = if let Some(binaural) = self.binaural.as_mut() {
                binaural.render(&block)?
            } else {
                block
            };
            self.last_output_end = self.last_output_end.max(
                frame
                    .logical_start_sample
                    .saturating_add(frame.sample_count as u64),
            );
            self.output_queue.push_back(self.to_pcm_frame(&frame)?);
        }
        Ok(())
    }

    fn to_pcm_frame(&self, frame: &RenderedBlock) -> Result<OpenJocPcmFrame, OpenJocError> {
        let channels = frame.channels.len();
        let samples = frame.sample_count;
        let (layout_name, labels) = self.output_layout_info();
        if channels != labels.len()
            || frame
                .channels
                .iter()
                .any(|channel| channel.len() != samples)
        {
            return Err(OpenJocError::Render(
                "renderer returned inconsistent PCM shape".to_owned(),
            ));
        }
        let mut interleaved = Vec::with_capacity(samples.saturating_mul(channels));
        for sample in 0..samples {
            for channel in &frame.channels {
                let value = channel[sample];
                if !value.is_finite() {
                    return Err(OpenJocError::Render(
                        "renderer returned non-finite PCM".to_owned(),
                    ));
                }
                interleaved.push(value as f32);
            }
        }
        let pts_samples = self.segment_pts.map(|origin| {
            origin.saturating_add(i64::try_from(frame.logical_start_sample).unwrap_or(i64::MAX))
        });
        Ok(OpenJocPcmFrame {
            sample_format: PCM_SAMPLE_FORMAT,
            sample_rate: frame.sample_rate,
            channel_count: channels,
            channel_labels: labels,
            layout_name,
            render_mode: self.config.render_mode,
            sample_count: samples,
            pts_samples,
            interleaved_f32: interleaved,
        })
    }

    fn output_layout_info(&self) -> (String, Vec<String>) {
        if self.config.render_mode == RenderMode::Binaural {
            (
                "Binaural stereo".to_owned(),
                vec!["Left Ear".to_owned(), "Right Ear".to_owned()],
            )
        } else {
            let layout = self.speaker.layout_info();
            (layout.name, layout.labels)
        }
    }
}

fn new_payload_decoder(config: &OpenJocConfig) -> PayloadDecoder {
    PayloadDecoder::streaming_with_oamd_profile(
        PayloadDecoderConfig {
            reference_screen: None,
            oamd: config.oamd,
        },
        OamdParseProfile::EtsiStrict,
    )
}

fn parse_oamd_for_profile(
    payload: &[u8],
    config: OamdDecoderConfig,
    profile: OamdParseProfile,
) -> Result<openjoc_oamd::OamdPayload, openjoc_oamd::OamdError> {
    match profile {
        OamdParseProfile::EtsiStrict => parse_oamd_payload_with_config(payload, config),
        OamdParseProfile::ObservedVendorCompat => parse_oamd_payload_with_profile(
            payload,
            config,
            OamdParseProfile::ObservedVendorCompat,
            OAMD_PAYLOAD_ID,
        ),
    }
}

fn dither_values() -> Vec<f64> {
    let mut state = 0x6d2b_79f5_u32;
    (0..32_768)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (f64::from(state) / f64::from(u32::MAX) - 0.5) * 0.5
        })
        .collect()
}

#[derive(Clone, Debug)]
struct RenderedBlock {
    sample_rate: u32,
    logical_start_sample: u64,
    sample_count: usize,
    channels: Vec<Vec<f64>>,
}

#[derive(Clone, Debug)]
struct PendingRenderFrame {
    frame: DecodedPayloadFrame,
    channel_locations: Vec<ChannelLocation>,
    lfe_location: Option<ChannelLocation>,
    downmix: DownmixMetadata,
}

#[derive(Debug)]
struct SpeakerRenderer {
    frame_bridge: JocSpatialFrameBridge,
    bridge: JocSpatialBridge,
    preset: SpeakerLayoutPreset,
    assembler: BridgeControlAssembler,
    expected_coordinates: Option<usize>,
    next_input_frame: u64,
    expected_frame: u64,
    expected_sample: u64,
    timeline: ReconstructionOutputTimeline,
    pending_frames: VecDeque<PendingRenderFrame>,
    base_coordinates: Option<Vec<BaseFullBandCoordinate>>,
    downmix_policy: DownmixPolicy,
}

impl SpeakerRenderer {
    fn new(layout: &str, downmix_policy: DownmixPolicy) -> Result<Self, OpenJocError> {
        let preset = SpeakerLayoutPreset::for_name(layout)
            .map_err(|error| OpenJocError::InvalidConfig(error.to_string()))?;
        let dimensions = preset.layout.coordinate_dimension_count();
        Ok(Self {
            assembler: BridgeControlAssembler::new_with_base_projection(
                64,
                dimensions,
                preset.name != "2.0",
            ),
            frame_bridge: JocSpatialFrameBridge,
            bridge: JocSpatialBridge::new(),
            preset,
            expected_coordinates: None,
            next_input_frame: 0,
            expected_frame: 0,
            expected_sample: 0,
            timeline: ReconstructionOutputTimeline::new(),
            pending_frames: VecDeque::new(),
            base_coordinates: None,
            downmix_policy,
        })
    }

    fn layout_info(&self) -> SemanticChannelLayout {
        self.preset.semantic_channel_layout()
    }

    fn render_frame_aligned(
        &mut self,
        frame: &DecodedPayloadFrame,
        base: &DecodedAccessUnitPcm,
    ) -> Result<Vec<RenderedBlock>, OpenJocError> {
        if frame.decoded.state_reset {
            self.timeline.reset();
            self.pending_frames.clear();
            self.assembler.reset();
            self.bridge.reset();
            self.base_coordinates = None;
            self.expected_coordinates = None;
            self.expected_frame = frame.frame_index;
            self.expected_sample = frame.sample_range.start_sample;
        }
        if frame.frame_index != self.next_input_frame {
            return Err(OpenJocError::Render(format!(
                "expected input frame {}, received {}",
                self.next_input_frame, frame.frame_index
            )));
        }
        let base_coordinates = base
            .channel_locations
            .iter()
            .copied()
            .map(base_coordinate)
            .collect::<Result<Vec<_>, _>>()?;
        let aligned = self.timeline.push_frame(
            frame.frame_index,
            frame.sample_rate,
            frame.sample_range.start_sample,
            frame.sample_range.end_sample,
            &base.channels,
            &frame.decoded.reconstruction_basis,
            base.lfe.as_deref(),
            false,
        )?;
        self.pending_frames.push_back(PendingRenderFrame {
            frame: frame.clone(),
            channel_locations: base.channel_locations.clone(),
            lfe_location: base.lfe_location,
            downmix: base.downmix,
        });
        self.next_input_frame = self.next_input_frame.saturating_add(1);
        if let Some(previous) = &self.base_coordinates {
            if previous != &base_coordinates {
                return Err(OpenJocError::Render(
                    "Base channel topology changed within a stream".to_owned(),
                ));
            }
        } else {
            self.base_coordinates = Some(base_coordinates);
        }
        let mut rendered = Vec::new();
        for aligned_frame in aligned {
            let pending = self.pending_frames.pop_front().ok_or_else(|| {
                OpenJocError::Render("reconstruction timeline queue underflow".to_owned())
            })?;
            let aligned_base = DecodedAccessUnitPcm {
                sample_rate: aligned_frame.timeline.sample_rate,
                samples: u16::try_from(
                    aligned_frame.base_full_band_pcm.first().map_or(0, Vec::len),
                )
                .unwrap_or(u16::MAX),
                channel_locations: pending.channel_locations,
                channels: aligned_frame.base_full_band_pcm,
                lfe_location: pending.lfe_location,
                lfe: aligned_frame.lfe_pcm,
                downmix: pending.downmix,
            };
            let mut aligned_payload = pending.frame;
            aligned_payload.decoded.reconstruction_basis = aligned_frame.reconstruction_basis;
            rendered.push(self.render_aligned_block(
                &aligned_payload,
                &aligned_base,
                aligned_frame.timeline.logical_start_sample,
            )?);
        }
        Ok(rendered)
    }

    fn render_aligned_block(
        &mut self,
        frame: &DecodedPayloadFrame,
        base: &DecodedAccessUnitPcm,
        logical_start_sample: u64,
    ) -> Result<RenderedBlock, OpenJocError> {
        if frame.sample_range.start_sample != self.expected_sample {
            return Err(OpenJocError::Render(format!(
                "expected sample {}, received {}",
                self.expected_sample, frame.sample_range.start_sample
            )));
        }
        let base_coordinates = self
            .base_coordinates
            .as_ref()
            .ok_or_else(|| OpenJocError::Render("missing Base coordinate topology".to_owned()))?;
        if base.sample_rate != frame.sample_rate {
            return Err(OpenJocError::FormatChanged {
                expected: frame.sample_rate,
                actual: base.sample_rate,
            });
        }
        let coordinate_count = base_coordinates
            .len()
            .checked_add(frame.decoded.reconstruction_basis.rows.len())
            .ok_or_else(|| OpenJocError::Render("coordinate count overflow".to_owned()))?;
        if let Some(expected) = self.expected_coordinates {
            if expected != coordinate_count {
                return Err(OpenJocError::Render(format!(
                    "coordinate count changed from {expected} to {coordinate_count}"
                )));
            }
        } else {
            self.expected_coordinates = Some(coordinate_count);
        }
        let bridge_frame = self.frame_bridge.frame(
            frame,
            base_coordinates,
            &base.channels,
            base.lfe.as_deref(),
        )?;
        let sample_count = usize::try_from(bridge_frame.sample_range.len())
            .map_err(|_| OpenJocError::Render("sample count overflow".to_owned()))?;
        let mut active = vec![vec![0.0; sample_count]; self.preset.layout.active_channel_count()];
        let control = self
            .assembler
            .assemble_frame(frame, base_coordinates, None)?;
        let mut boundaries = vec![0_usize, sample_count];
        for event in &control.events {
            let start = usize::try_from(event.quantum.saturating_mul(32)).unwrap_or(usize::MAX);
            if start < sample_count {
                boundaries.push(start);
            }
        }
        boundaries.sort_unstable();
        boundaries.dedup();
        let stereo = self.preset.name == "2.0";
        let zero = vec![0.0; sample_count];
        let mut coordinates = Vec::with_capacity(coordinate_count);
        for pcm in bridge_frame.basis.base_full_band_pcm {
            coordinates.push(if stereo {
                zero.as_slice()
            } else {
                pcm.as_slice()
            });
        }
        coordinates.extend(
            bridge_frame
                .basis
                .reconstruction_basis
                .rows
                .iter()
                .map(Vec::as_slice),
        );
        for window in boundaries.windows(2) {
            let start = window[0];
            let end = window[1];
            if start == end {
                continue;
            }
            let event = control.events.iter().find(|event| {
                usize::try_from(event.quantum.saturating_mul(32)).ok() == Some(start)
            });
            let updates = event.map(|event| event.updates.as_slice());
            let ramp_duration = event.map_or(0, |event| u64::from(event.ramp_duration));
            let sliced = coordinates
                .iter()
                .map(|coordinate| &coordinate[start..end])
                .collect::<Vec<_>>();
            let mut outputs = active
                .iter_mut()
                .map(|channel| &mut channel[start..end])
                .collect::<Vec<_>>();
            let topology = (start == 0)
                .then_some(control.initial_topology.as_ref())
                .flatten();
            self.bridge.render_coordinates(
                &sliced,
                topology,
                updates,
                &self.preset.layout,
                ramp_duration,
                frame.sample_rate,
                &mut outputs,
            )?;
        }
        if stereo {
            add_stereo_base_downmix(&mut active, base, self.downmix_policy)?;
        }
        let mut channels = vec![vec![0.0; sample_count]; self.preset.channel_count()];
        let mut active_index = 0;
        for (output_index, channel) in self.preset.layout.channels().iter().enumerate() {
            if channel.lfe {
                if let Some(lfe) = base.lfe.as_deref() {
                    channels[output_index].copy_from_slice(lfe);
                }
            } else {
                channels[output_index].copy_from_slice(&active[active_index]);
                active_index += 1;
            }
        }
        self.expected_frame = self.expected_frame.saturating_add(1);
        self.expected_sample = self.expected_sample.saturating_add(sample_count as u64);
        Ok(RenderedBlock {
            sample_rate: frame.sample_rate,
            logical_start_sample,
            sample_count,
            channels,
        })
    }

    fn finish_with_reconstruction_tail(
        &mut self,
        tail: &ReconstructionBasis,
    ) -> Result<Vec<RenderedBlock>, OpenJocError> {
        let aligned = self.timeline.finish(tail)?;
        let mut rendered = Vec::new();
        for aligned_frame in aligned {
            let pending = self.pending_frames.pop_front().ok_or_else(|| {
                OpenJocError::Render("reconstruction timeline queue underflow".to_owned())
            })?;
            let base = DecodedAccessUnitPcm {
                sample_rate: aligned_frame.timeline.sample_rate,
                samples: u16::try_from(
                    aligned_frame.base_full_band_pcm.first().map_or(0, Vec::len),
                )
                .unwrap_or(u16::MAX),
                channel_locations: pending.channel_locations,
                channels: aligned_frame.base_full_band_pcm,
                lfe_location: pending.lfe_location,
                lfe: aligned_frame.lfe_pcm,
                downmix: pending.downmix,
            };
            let mut frame = pending.frame;
            frame.decoded.reconstruction_basis = aligned_frame.reconstruction_basis;
            rendered.push(self.render_aligned_block(
                &frame,
                &base,
                aligned_frame.timeline.logical_start_sample,
            )?);
        }
        if !self.pending_frames.is_empty() {
            return Err(OpenJocError::Render(
                "reconstruction timeline left pending frames".to_owned(),
            ));
        }
        Ok(rendered)
    }

    fn reset(&mut self) {
        self.bridge.reset();
        self.assembler.reset();
        self.timeline.reset();
        self.pending_frames.clear();
        self.expected_coordinates = None;
        self.next_input_frame = 0;
        self.expected_frame = 0;
        self.expected_sample = 0;
        self.base_coordinates = None;
    }
}

fn base_coordinate(location: ChannelLocation) -> Result<BaseFullBandCoordinate, OpenJocError> {
    Ok(match location {
        ChannelLocation::Left => BaseFullBandCoordinate::Left,
        ChannelLocation::Right => BaseFullBandCoordinate::Right,
        ChannelLocation::Centre => BaseFullBandCoordinate::Centre,
        ChannelLocation::LeftSurround => BaseFullBandCoordinate::LeftSurround,
        ChannelLocation::RightSurround => BaseFullBandCoordinate::RightSurround,
        ChannelLocation::LeftBack => BaseFullBandCoordinate::LeftBack,
        ChannelLocation::RightBack => BaseFullBandCoordinate::RightBack,
        ChannelLocation::TopFrontLeft => BaseFullBandCoordinate::TopFrontLeft,
        ChannelLocation::TopFrontRight => BaseFullBandCoordinate::TopFrontRight,
        ChannelLocation::Other(value) => BaseFullBandCoordinate::Other(value),
        ChannelLocation::Lfe(_) => {
            return Err(OpenJocError::Render(
                "LFE is not a spatial bridge coordinate".to_owned(),
            ));
        }
    })
}

fn selected_stereo_policy(requested: DownmixPolicy, metadata: DownmixMetadata) -> DownmixPolicy {
    match requested {
        DownmixPolicy::Auto => match metadata.dmixmod {
            Some(1) => DownmixPolicy::LtRt,
            _ => DownmixPolicy::LoRo,
        },
        explicit => explicit,
    }
}

fn mix_level(code: Option<u8>, table: [f64; 8], default: f64, reserved: f64) -> f64 {
    match code {
        Some(code @ 0..=7) => {
            let value = table[usize::from(code)];
            if value.is_finite() { value } else { reserved }
        }
        _ => default,
    }
}

fn add_stereo_base_downmix(
    active: &mut [Vec<f64>],
    base: &DecodedAccessUnitPcm,
    requested: DownmixPolicy,
) -> Result<(), OpenJocError> {
    const DEFAULT_LEVEL: f64 = 0.707;
    const CENTER_LEVELS: [f64; 8] = [1.414, 1.189, 1.0, 0.841, 0.707, 0.595, 0.5, 0.0];
    const SURROUND_LEVELS: [f64; 8] = [f64::NAN, f64::NAN, f64::NAN, 0.841, 0.707, 0.595, 0.5, 0.0];
    if active.len() != 2 {
        return Err(OpenJocError::Render(
            "stereo output needs two active channels".to_owned(),
        ));
    }
    let selected = selected_stereo_policy(requested, base.downmix);
    let center = match selected {
        DownmixPolicy::LoRo => mix_level(
            base.downmix.loro_center_mix_level,
            CENTER_LEVELS,
            DEFAULT_LEVEL,
            DEFAULT_LEVEL,
        ),
        _ => mix_level(
            base.downmix.ltrt_center_mix_level,
            CENTER_LEVELS,
            DEFAULT_LEVEL,
            DEFAULT_LEVEL,
        ),
    };
    let surround = match selected {
        DownmixPolicy::LoRo => mix_level(
            base.downmix.loro_surround_mix_level,
            SURROUND_LEVELS,
            DEFAULT_LEVEL,
            0.841,
        ),
        _ => mix_level(
            base.downmix.ltrt_surround_mix_level,
            SURROUND_LEVELS,
            DEFAULT_LEVEL,
            0.841,
        ),
    };
    let lfe = base
        .downmix
        .lfe_mix_level_code
        .map(|code| 10.0_f64.powf((10.0 - f64::from(code) - 4.5) / 20.0));
    for (channel, location) in base.channels.iter().zip(&base.channel_locations) {
        let (left, right) = match *location {
            ChannelLocation::Left => (1.0, 0.0),
            ChannelLocation::Right => (0.0, 1.0),
            ChannelLocation::Centre => (center, center),
            ChannelLocation::LeftSurround => match selected {
                DownmixPolicy::LoRo => (surround, 0.0),
                _ => (-surround, surround),
            },
            ChannelLocation::RightSurround => match selected {
                DownmixPolicy::LoRo => (0.0, surround),
                _ => (-surround, surround),
            },
            ChannelLocation::Other(3) => match selected {
                DownmixPolicy::LoRo => (0.7 * surround, 0.7 * surround),
                _ => (-surround, surround),
            },
            unsupported => {
                return Err(OpenJocError::Render(format!(
                    "2.0 downmix does not admit Base channel {}",
                    unsupported.label()
                )));
            }
        };
        for (index, value) in channel.iter().copied().enumerate() {
            active[0][index] += left * value;
            active[1][index] += right * value;
        }
    }
    if let (Some(channel), Some(gain)) = (base.lfe.as_deref(), lfe) {
        for (index, value) in channel.iter().copied().enumerate() {
            active[0][index] += gain * value;
            active[1][index] += gain * value;
        }
    }
    Ok(())
}

#[derive(Debug)]
struct BinauralState {
    bank: HrirBank,
    mappings: Vec<BinauralMapping>,
    lfe_index: Option<usize>,
    lfe_policy: BinauralLfePolicy,
    engine: Option<BinauralRenderer>,
}

#[derive(Clone, Copy, Debug)]
struct BinauralMapping {
    channel_index: usize,
    source_id: SourceId,
    hrir_entry: HrirEntryId,
}

impl BinauralState {
    fn new(config: &BinauralConfig) -> Result<Self, OpenJocError> {
        let loaded = parse_simple_free_field_hrir(&config.sofa_bytes, SofaLoadLimits::default())?;
        let preset = SpeakerLayoutPreset::for_name(&config.virtual_layout)
            .map_err(|error| OpenJocError::InvalidConfig(error.to_string()))?;
        let mut entries = loaded.bank.entries().to_vec();
        let mut next_id = u64::MAX;
        let mut mappings = Vec::new();
        for (channel_index, label) in preset.labels.iter().enumerate() {
            if preset.lfe_index() == Some(channel_index) {
                continue;
            }
            let direction = virtual_speaker_direction(label).ok_or_else(|| {
                OpenJocError::Unsupported(format!("no binaural direction for {label}"))
            })?;
            let resolved = resolve_hrir(&loaded.bank, direction)?;
            let entry_id = if let Some(id) = resolved.exact_entry {
                id
            } else {
                while entries
                    .iter()
                    .any(|entry| entry.id() == HrirEntryId::new(next_id))
                {
                    next_id = next_id.saturating_sub(1);
                }
                let id = HrirEntryId::new(next_id);
                next_id = next_id.saturating_sub(1);
                entries.push(HrirEntry::new(id, direction, resolved.pair)?);
                id
            };
            mappings.push(BinauralMapping {
                channel_index,
                source_id: SourceId::new(channel_index as u64 + 1),
                hrir_entry: entry_id,
            });
        }
        let bank = HrirBank::new(loaded.metadata.sample_rate_hz, entries)?;
        Ok(Self {
            bank,
            mappings,
            lfe_index: preset.lfe_index(),
            lfe_policy: config.lfe_policy,
            engine: None,
        })
    }

    fn ensure_engine(&mut self, sample_rate: u32) -> Result<&mut BinauralRenderer, OpenJocError> {
        if self.engine.is_none() {
            if self.bank.sample_rate_hz() != sample_rate {
                return Err(OpenJocError::FormatChanged {
                    expected: self.bank.sample_rate_hz(),
                    actual: sample_rate,
                });
            }
            let sources = self
                .mappings
                .iter()
                .map(|mapping| {
                    let entry = self
                        .bank
                        .entries()
                        .iter()
                        .find(|entry| entry.id() == mapping.hrir_entry)
                        .ok_or_else(|| OpenJocError::Render("missing prepared HRIR".to_owned()))?;
                    StaticBinauralSource::new(
                        mapping.source_id,
                        CartesianPosition::new(
                            entry.direction()[0],
                            entry.direction()[1],
                            entry.direction()[2],
                        ),
                        1.0,
                        mapping.hrir_entry,
                    )
                    .map_err(OpenJocError::from)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let bank = self.bank.clone();
            self.engine = Some(BinauralRenderer::new(sample_rate, bank, sources)?);
        }
        self.engine
            .as_mut()
            .ok_or_else(|| OpenJocError::Render("binaural engine not initialized".to_owned()))
    }

    fn render(&mut self, frame: &RenderedBlock) -> Result<RenderedBlock, OpenJocError> {
        let sample_count = frame.sample_count;
        let sample_rate = frame.sample_rate;
        let mappings = self.mappings.clone();
        let blocks = mappings
            .iter()
            .map(|mapping| {
                BinauralSourceBlock::new(mapping.source_id, &frame.channels[mapping.channel_index])
            })
            .collect::<Vec<_>>();
        let engine = self.ensure_engine(sample_rate)?;
        let mut left = vec![0.0; sample_count];
        let mut right = vec![0.0; sample_count];
        engine.render_block(&blocks, &mut left, &mut right)?;
        if self.lfe_policy == BinauralLfePolicy::EqualPowerDualMono {
            if let Some(index) = self.lfe_index {
                if let Some(lfe) = frame.channels.get(index) {
                    for ((left_value, right_value), lfe_value) in
                        left.iter_mut().zip(&mut right).zip(lfe)
                    {
                        *left_value += *lfe_value * std::f64::consts::FRAC_1_SQRT_2;
                        *right_value += *lfe_value * std::f64::consts::FRAC_1_SQRT_2;
                    }
                }
            }
        }
        Ok(RenderedBlock {
            sample_rate,
            logical_start_sample: frame.logical_start_sample,
            sample_count,
            channels: vec![left, right],
        })
    }

    fn drain_tail(
        &mut self,
        sample_rate: u32,
        start: u64,
    ) -> Result<Vec<RenderedBlock>, OpenJocError> {
        let Some(engine) = self.engine.as_mut() else {
            return Ok(Vec::new());
        };
        let mut output = Vec::new();
        let mut cursor = start;
        while engine.remaining_tail_samples() > 0 {
            let count = engine.remaining_tail_samples().min(1024);
            let mut left = vec![0.0; count];
            let mut right = vec![0.0; count];
            engine.drain_tail_block(&mut left, &mut right)?;
            output.push(RenderedBlock {
                sample_rate,
                logical_start_sample: cursor,
                sample_count: count,
                channels: vec![left, right],
            });
            cursor = cursor.saturating_add(count as u64);
        }
        Ok(output)
    }

    fn reset(&mut self) {
        if let Some(engine) = self.engine.as_mut() {
            engine.reset();
        }
    }
}

fn virtual_speaker_direction(label: &str) -> Option<CartesianPosition> {
    let (x, y, z) = match label {
        "FL" => (-1.0, 0.0, 0.0),
        "FR" => (1.0, 0.0, 0.0),
        "FC" => (0.0, -1.0, 0.0),
        "Ls" | "Lb" => (-1.0, 1.0, 0.0),
        "Rs" | "Rb" => (1.0, 1.0, 0.0),
        "TFL" | "Ltf" => (-1.0, 0.0, 1.0),
        "TFR" | "Rtf" => (1.0, 0.0, 1.0),
        "TBL" | "Ltr" => (-1.0, 1.0, 1.0),
        "TBR" | "Rtr" => (1.0, 1.0, 1.0),
        "Ltm" => (-1.0, 0.5, 1.0),
        "Rtm" => (1.0, 0.5, 1.0),
        "Lw" => (-1.0, -0.2, 0.0),
        "Rw" => (1.0, -0.2, 0.0),
        _ => return None,
    };
    Some(CartesianPosition::new(x, y, z))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_headless_and_has_stable_output_contract() {
        let session = OpenJocSession::new(OpenJocConfig::default()).expect("default config");
        let info = session.output_info();
        assert_eq!(info.sample_format, PcmSampleFormat::F32);
        assert_eq!(info.layout_name, "5.1");
        assert_eq!(info.channel_labels, ["FL", "FR", "FC", "LFE", "Ls", "Rs"]);
        assert_eq!(info.latency_samples, 577);
    }

    #[test]
    fn invalid_and_pending_lifecycle_is_structured() {
        let mut session = OpenJocSession::new(OpenJocConfig::default()).expect("session");
        assert_eq!(
            session.drain().expect("empty drain"),
            OpenJocStatus::EndOfStream
        );
        assert_eq!(
            session.drain().expect("second drain"),
            OpenJocStatus::EndOfStream
        );
        let error = session
            .push_packet(OpenJocPacket {
                data: &[0x0b],
                pts_samples: None,
                discontinuity: false,
                preroll: false,
            })
            .expect_err("drained session rejects input");
        assert_eq!(error, OpenJocError::AlreadyDrained);
    }
}
