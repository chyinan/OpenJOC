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
    ChannelLocation, DecodedAccessUnitPcm, DecodedJocAccessUnitPcm, DialnormState, DownmixMetadata,
    InternalBasePolicy, JocAccessUnitPcmDecoder, JocMetadataFrame, StereoDownmixMode, StreamType,
    extract_joc_access_unit_for_profile, group_access_units, index_syncframes, parse_bsi,
    parse_joc_access_unit, stereo_downmix_matrix, validate_complexity_index,
    validate_joc_access_unit,
};
use openjoc_emdf::{
    JOC_PAYLOAD_ID, JocProfileDeviation, JocProfileField, JocProfileValue, JocValidationProfile,
};
use openjoc_joc::{ReconstructionBasis, ReconstructionOutputTimeline, parse_joc_payload};
use openjoc_oamd::{
    OAMD_PAYLOAD_ID, OamdDecoderConfig, OamdElement, OamdError, OamdParseProfile, OamdPayload,
    parse_oamd_payload_with_config, parse_oamd_payload_with_profile,
};
use openjoc_render::{
    BinauralRenderer, BinauralSourceBlock, CartesianPosition, FINAL_LINKED_GAIN_BLOCK_SAMPLES,
    FinalLinkedGain, FinalLinkedGainError, HrirBank, HrirEntry, HrirEntryId, SourceId,
    StaticBinauralSource,
};
use openjoc_scene::{
    BaseFullBandCoordinate, BindingCodecProfile, BridgeControlAssembler, DecodedPayloadFrame,
    JocFrameInput, JocSpatialBridge, JocSpatialFrameBridge, PayloadDecoder, PayloadDecoderConfig,
    SemanticChannelLayout, SpeakerLayout, SpeakerLayoutPreset,
};
use openjoc_sofa::{
    SofaLoadLimits, load_builtin_generic_hrir, parse_simple_free_field_hrir, resolve_hrir,
};
use sha2::{Digest, Sha256};
use std::{collections::VecDeque, fmt, fmt::Write as _};

/// The first public C ABI is intentionally experimental. This is separate
/// from the Rust package version and may evolve during the OpenJOC 0.x series.
pub const API_MATURITY: &str = "experimental";
/// The declared QMF/Base-RB reconstruction delay in samples.
pub const QMF_LATENCY_SAMPLES: usize = ReconstructionOutputTimeline::qmf_latency_samples();
/// The admitted causal speaker-stage block delay at the 48-kHz adapter.
pub const FINAL_LINKED_GAIN_LATENCY_SAMPLES: usize = FINAL_LINKED_GAIN_BLOCK_SAMPLES;
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

/// Decoder/program calibration policy for encoded E-AC-3 dialnorm metadata.
/// This is intentionally separate from [`DrcPolicy`] and from any file-export
/// gain policy in an application.
pub use openjoc_eac3::DialnormMode;

impl DrcPolicy {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Line => "line",
            Self::Rf => "rf",
            Self::Custom { .. } => "custom",
        }
    }

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

/// In-memory binaural configuration. An empty `sofa_bytes` value selects the
/// bundled SADIE II generic resource; non-empty bytes select an explicit user
/// SOFA and retain the existing fail-closed parser behavior.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BinauralConfig {
    /// Explicit SOFA bytes, or empty to use [`Self::builtin_generic`].
    pub sofa_bytes: Vec<u8>,
    pub virtual_layout: String,
    pub lfe_policy: BinauralLfePolicy,
}

impl BinauralConfig {
    /// Selects the offline built-in generic HRTF without a filesystem path.
    #[must_use]
    pub fn builtin_generic(virtual_layout: impl Into<String>) -> Self {
        Self {
            sofa_bytes: Vec::new(),
            virtual_layout: virtual_layout.into(),
            lfe_policy: BinauralLfePolicy::Exclude,
        }
    }

    /// Selects a caller-owned SOFA buffer. The existing strict SOFA checks
    /// remain in force when the session is created.
    #[must_use]
    pub fn from_sofa_bytes(
        sofa_bytes: Vec<u8>,
        virtual_layout: impl Into<String>,
        lfe_policy: BinauralLfePolicy,
    ) -> Self {
        Self {
            sofa_bytes,
            virtual_layout: virtual_layout.into(),
            lfe_policy,
        }
    }

    #[must_use]
    fn is_builtin_generic(&self) -> bool {
        self.sofa_bytes.is_empty()
    }
}

/// Stable high-level session configuration.
#[derive(Clone, Debug)]
pub struct OpenJocConfig {
    pub render_mode: RenderMode,
    /// Speaker layout or virtual speaker layout. Stereo always uses `2.0`.
    pub speaker_layout: String,
    /// Optional validated custom physical speaker layout. When present in
    /// speaker mode it takes precedence over `speaker_layout`.
    pub speaker_layout_definition: Option<SpeakerLayout>,
    pub downmix: DownmixPolicy,
    pub drc: DrcPolicy,
    pub dialnorm: DialnormMode,
    pub validation_profile: ValidationProfile,
    pub oamd: OamdDecoderConfig,
    pub binaural: Option<BinauralConfig>,
}

impl Default for OpenJocConfig {
    fn default() -> Self {
        Self {
            render_mode: RenderMode::Speaker,
            speaker_layout: "5.1".to_owned(),
            speaker_layout_definition: None,
            downmix: DownmixPolicy::Auto,
            drc: DrcPolicy::Line,
            dialnorm: DialnormMode::Default,
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
        } else if let Some(layout) = &self.speaker_layout_definition {
            layout.name()
        } else {
            self.speaker_layout.as_str()
        }
    }

    fn effective_speaker_layout(&self) -> Result<SpeakerLayout, OpenJocError> {
        if self.render_mode == RenderMode::Speaker {
            if let Some(layout) = &self.speaker_layout_definition {
                return Ok(layout.clone());
            }
            return SpeakerLayout::preset(&self.speaker_layout)
                .map_err(|error| OpenJocError::InvalidConfig(error.to_string()));
        }
        if self.speaker_layout_definition.is_some() {
            return Err(OpenJocError::InvalidConfig(
                "custom speaker geometry is only valid for speaker render mode".to_owned(),
            ));
        }
        let layout = if self.render_mode == RenderMode::Stereo {
            "2.0"
        } else {
            self.binaural
                .as_ref()
                .map_or(self.speaker_layout.as_str(), |binaural| {
                    binaural.virtual_layout.as_str()
                })
        };
        SpeakerLayout::preset(layout)
            .map_err(|error| OpenJocError::InvalidConfig(error.to_string()))
    }

    /// Selects a validated custom speaker layout for physical speaker output.
    #[must_use]
    pub fn with_speaker_layout(mut self, layout: SpeakerLayout) -> Self {
        layout.name().clone_into(&mut self.speaker_layout);
        self.speaker_layout_definition = Some(layout);
        self
    }

    /// Returns the stable, field-by-field representation of the settings that
    /// reach an OpenJOC session. Fields that are intentionally ignored by a
    /// selected mode are omitted, so frontends can compare effective rather
    /// than merely user-visible configuration.
    #[must_use]
    pub fn effective_config_descriptor(&self) -> String {
        let mut descriptor = format!(
            "openjoc-effective-config-v1\nrender_mode={}\nlayout={}\ndownmix={}\ndrc={}",
            self.render_mode.as_str(),
            self.effective_layout(),
            self.downmix.as_str(),
            self.drc.as_str(),
        );
        if let DrcPolicy::Custom {
            boost_percent,
            cut_percent,
        } = self.drc
        {
            let _ = write!(
                descriptor,
                "\ndrc_boost_percent={boost_percent}\ndrc_cut_percent={cut_percent}"
            );
        }
        let _ = write!(
            descriptor,
            "\ndialnorm={}\nvalidation_profile={}\noamd_trim_configuration_count={}",
            dialnorm_name(self.dialnorm),
            validation_profile_name(self.validation_profile),
            self.oamd
                .trim_configuration_count
                .map_or_else(|| "none".to_owned(), |value| value.get().to_string()),
        );
        if let Some(layout) = &self.speaker_layout_definition {
            descriptor.push_str("\ncustom_layout_channels=");
            descriptor.push_str(&layout.channel_labels().join(","));
            for (index, coordinate) in layout.channel_coordinates().iter().enumerate() {
                let _ = write!(
                    descriptor,
                    "\ncustom_layout_coordinate_{index}={:.9},{:.9},{:.9}",
                    coordinate[0], coordinate[1], coordinate[2]
                );
            }
        }
        if let Some(binaural) = &self.binaural {
            let (hrtf_source, hrtf_sha256) = if binaural.is_builtin_generic() {
                (
                    "builtin:SADIE_II_D1_KU100_v2-2".to_owned(),
                    "builtin-resource".to_owned(),
                )
            } else {
                (
                    "custom-sofa-bytes".to_owned(),
                    sha256_hex(&binaural.sofa_bytes),
                )
            };
            let _ = write!(
                descriptor,
                "\nbinaural_virtual_layout={}\nbinaural_lfe_policy={}\nbinaural_hrtf_source={hrtf_source}\nbinaural_hrtf_sha256={hrtf_sha256}\nbinaural_backend=direct\nfinal_linked_gain=disabled",
                binaural.virtual_layout,
                binaural_lfe_policy_name(binaural.lfe_policy),
            );
        } else {
            descriptor.push_str("\nfinal_linked_gain=enabled");
        }
        descriptor
    }

    /// Returns a deterministic SHA-256 fingerprint of the effective session
    /// configuration. This is intended for adapter parity logs and tests.
    #[must_use]
    pub fn effective_config_fingerprint(&self) -> String {
        sha256_hex(self.effective_config_descriptor().as_bytes())
    }

    /// Validates the effective session configuration without allocating any
    /// decoder, renderer, or HRTF stream state.
    pub fn validate(&self) -> Result<(), OpenJocError> {
        self.effective_speaker_layout()?;
        if self.render_mode == RenderMode::Binaural && self.binaural.is_none() {
            return Err(OpenJocError::InvalidConfig(
                "binaural mode requires BinauralConfig (built-in generic or explicit SOFA)"
                    .to_owned(),
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

fn dialnorm_name(mode: DialnormMode) -> &'static str {
    match mode {
        DialnormMode::Default => "default",
        DialnormMode::Digital => "digital",
        DialnormMode::Analog => "analog",
    }
}

fn validation_profile_name(profile: ValidationProfile) -> &'static str {
    match profile {
        ValidationProfile::Auto => "auto",
        ValidationProfile::EtsiStrict => "etsi-strict",
        ValidationProfile::ObservedVendorCompat => "observed-vendor-compat",
    }
}

fn binaural_lfe_policy_name(policy: BinauralLfePolicy) -> &'static str {
    match policy {
        BinauralLfePolicy::Exclude => "exclude",
        BinauralLfePolicy::EqualPowerDualMono => "equal-power-dual-mono",
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    let mut output = String::with_capacity(64);
    for byte in digest.finalize() {
        let _ = write!(output, "{byte:02x}");
    }
    output
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

/// Deterministic audit record for one grouped JOC access unit. Frontends can
/// use this record to prove that packet grouping, byte ownership, and sample
/// timestamps agree before comparing PCM.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpenJocAccessUnitTrace {
    pub index: usize,
    pub byte_length: usize,
    pub sha256: String,
    pub pts_samples: Option<i64>,
    pub sample_count: u16,
    pub sample_rate: u32,
    pub independent_frame_count: usize,
    pub dependent_frame_count: usize,
}

/// Groups and fingerprints every complete JOC access unit in an elementary
/// stream. `pts_origin_samples` is the first AU's sample-domain PTS; later
/// PTS values are advanced by each AU's declared sample count.
pub fn trace_access_units(
    stream: &[u8],
    pts_origin_samples: Option<i64>,
) -> Result<Vec<OpenJocAccessUnitTrace>, OpenJocError> {
    let frames = index_syncframes(stream)?;
    let units = group_access_units(&frames)?;
    let mut sample_offset = 0_u64;
    units
        .into_iter()
        .enumerate()
        .map(|(index, unit)| {
            let first = frames[unit.first_frame];
            let last = frames[unit.first_frame + unit.frame_count - 1];
            let end = last
                .offset
                .checked_add(last.header.frame_size)
                .ok_or_else(|| {
                    OpenJocError::InvalidPacket("access-unit byte range overflow".to_owned())
                })?;
            let bytes = stream.get(first.offset..end).ok_or_else(|| {
                OpenJocError::InvalidPacket("access-unit byte range is outside input".to_owned())
            })?;
            let independent_frame_count = frames
                [unit.first_frame..unit.first_frame + unit.frame_count]
                .iter()
                .filter(|entry| {
                    matches!(
                        entry.header.stream_type,
                        openjoc_eac3::StreamType::LegacyIndependent
                            | openjoc_eac3::StreamType::Independent
                    )
                })
                .count();
            let dependent_frame_count = unit.frame_count.saturating_sub(independent_frame_count);
            let pts_samples = pts_origin_samples.map(|origin| {
                origin.saturating_add(i64::try_from(sample_offset).unwrap_or(i64::MAX))
            });
            let trace = OpenJocAccessUnitTrace {
                index,
                byte_length: bytes.len(),
                sha256: sha256_hex(bytes),
                pts_samples,
                sample_count: unit.samples,
                sample_rate: unit.sample_rate,
                independent_frame_count,
                dependent_frame_count,
            };
            sample_offset = sample_offset
                .checked_add(u64::from(unit.samples))
                .ok_or_else(|| {
                    OpenJocError::InvalidPacket("sample timeline overflow".to_owned())
                })?;
            Ok(trace)
        })
        .collect()
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LegacyCoreConfiguration {
    bitstream_id: u8,
    bitstream_mode: Option<u8>,
    audio_coding_mode: u8,
    lfe_on: bool,
    sample_rate: u32,
    frame_size: usize,
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
    core_stream_type: Option<StreamType>,
    legacy_core_configuration: Option<LegacyCoreConfiguration>,
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
        let speaker_layout = config.effective_speaker_layout()?;
        let speaker = SpeakerRenderer::new_with_linked_gain(
            speaker_layout,
            config.downmix,
            config.render_mode != RenderMode::Binaural,
            config.render_mode != RenderMode::Binaural,
        );
        let binaural = config
            .binaural
            .as_ref()
            .map(BinauralState::new)
            .transpose()?;
        let mut audio_decoder = JocAccessUnitPcmDecoder::new();
        audio_decoder.set_dialnorm_mode(config.dialnorm);
        Ok(Self {
            payload_decoder: new_payload_decoder(&config),
            audio_decoder,
            speaker,
            binaural,
            output_queue: VecDeque::new(),
            selected_profile: None,
            core_stream_type: None,
            legacy_core_configuration: None,
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
            latency_samples: self.latency_samples(),
        }
    }

    /// Returns the known deterministic decoder/reconstruction delay.
    #[must_use]
    pub fn latency_samples(&self) -> usize {
        if self.config.render_mode == RenderMode::Binaural {
            QMF_LATENCY_SAMPLES
        } else {
            QMF_LATENCY_SAMPLES + FINAL_LINKED_GAIN_LATENCY_SAMPLES
        }
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
        let core_stream_type = frames[unit.first_frame].header.stream_type;
        if self
            .core_stream_type
            .is_some_and(|expected| expected != core_stream_type)
        {
            return Err(OpenJocError::ProfileChanged);
        }
        let legacy_core_configuration = if core_stream_type == StreamType::LegacyIndependent {
            let core = frames[unit.first_frame];
            let core_end = core
                .offset
                .checked_add(core.header.frame_size)
                .ok_or_else(|| {
                    OpenJocError::InvalidPacket("legacy core range overflow".to_owned())
                })?;
            let core_bytes = packet.data.get(core.offset..core_end).ok_or_else(|| {
                OpenJocError::InvalidPacket("legacy core range overflow".to_owned())
            })?;
            let bsi = parse_bsi(core_bytes)?;
            Some(LegacyCoreConfiguration {
                bitstream_id: bsi.bitstream_id,
                bitstream_mode: bsi.bitstream_mode,
                audio_coding_mode: bsi.audio_coding_mode,
                lfe_on: bsi.lfe_on,
                sample_rate: bsi.header.sample_rate,
                frame_size: bsi.header.frame_size,
            })
        } else {
            None
        };
        if self.legacy_core_configuration.is_some()
            && self.legacy_core_configuration != legacy_core_configuration
        {
            return Err(OpenJocError::ProfileChanged);
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

        let pcm_planes = self.audio_decoder.decode_pcm_planes_with_policy(
            packet.data,
            &frames,
            unit,
            &self.dither_values,
            self.config.drc.internal(),
        )?;
        let pcm = &pcm_planes.joc_input_pcm;
        pcm.validate_joc_topology()?;
        let (metadata, profile, oamd_profile) = self.select_metadata(packet.data, &frames, unit)?;
        let parsed_joc = parse_joc_payload(&metadata.joc)
            .map_err(|error| OpenJocError::Decode(error.to_string()))?;
        pcm.validate_joc_downmix_topology(parsed_joc.header.downmix_index)?;
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
            base_lfe_pcm: pcm_planes.compatibility_pcm.lfe.as_deref(),
            joc_payload: &metadata.joc,
            oamd_payload: &metadata.oamd,
            frame_index: frame_number,
        };
        let mut decoded = None;
        let binding_profile = classify_binding_codec_profile_for_frame(
            &metadata,
            &parsed_oamd,
            profile,
            oamd_profile,
        );
        self.payload_decoder
            .decode_frame_with_profile_and_binding_profile(
                input,
                oamd_profile,
                binding_profile,
                |frame| {
                    decoded = Some(frame.clone());
                    Ok::<(), OpenJocError>(())
                },
            )?;
        let frame = decoded.ok_or(OpenJocError::Decode(
            "payload decoder returned no frame".to_owned(),
        ))?;
        self.next_input_sample = self
            .next_input_sample
            .checked_add(u64::from(unit.samples))
            .ok_or_else(|| OpenJocError::Decode("sample timeline overflow".to_owned()))?;
        let rendered = self.speaker.render_frame_aligned(&frame, &pcm_planes)?;
        self.emit_rendered(rendered)?;
        self.core_stream_type = Some(core_stream_type);
        self.legacy_core_configuration = legacy_core_configuration;
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
        self.audio_decoder.set_dialnorm_mode(self.config.dialnorm);
        self.payload_decoder = new_payload_decoder(&self.config);
        self.speaker.reset();
        if let Some(binaural) = self.binaural.as_mut() {
            binaural.reset();
        }
        self.selected_profile = None;
        self.core_stream_type = None;
        self.legacy_core_configuration = None;
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

const OBSERVED_COMPAT_DEVIATIONS: [(u64, JocProfileField, JocProfileValue, JocProfileValue); 7] = [
    (
        OAMD_PAYLOAD_ID,
        JocProfileField::CodecDataPresent,
        JocProfileValue::Bool(false),
        JocProfileValue::Bool(true),
    ),
    (
        OAMD_PAYLOAD_ID,
        JocProfileField::PayloadFrameAligned,
        JocProfileValue::Bool(false),
        JocProfileValue::Bool(true),
    ),
    (
        OAMD_PAYLOAD_ID,
        JocProfileField::CreateDuplicate,
        JocProfileValue::Absent,
        JocProfileValue::Bool(false),
    ),
    (
        OAMD_PAYLOAD_ID,
        JocProfileField::RemoveDuplicate,
        JocProfileValue::Absent,
        JocProfileValue::Bool(false),
    ),
    (
        OAMD_PAYLOAD_ID,
        JocProfileField::Priority,
        JocProfileValue::Absent,
        JocProfileValue::Unsigned(0),
    ),
    (
        OAMD_PAYLOAD_ID,
        JocProfileField::ProcessingAllowed,
        JocProfileValue::Absent,
        JocProfileValue::Unsigned(0),
    ),
    (
        JOC_PAYLOAD_ID,
        JocProfileField::CodecDataPresent,
        JocProfileValue::Bool(false),
        JocProfileValue::Bool(true),
    ),
];

fn exact_observed_compat_deviations(deviations: &[JocProfileDeviation]) -> bool {
    deviations.len() == OBSERVED_COMPAT_DEVIATIONS.len()
        && OBSERVED_COMPAT_DEVIATIONS.iter().all(|expected| {
            deviations
                .iter()
                .filter(|actual| {
                    (
                        actual.payload_id,
                        actual.field,
                        actual.actual,
                        actual.expected_by_etsi,
                    ) == *expected
                })
                .count()
                == 1
        })
}

fn exact_opaque_warp3_element(oamd: &OamdPayload) -> bool {
    let mut object_elements = 0_usize;
    let mut opaque_warp3_elements = 0_usize;
    for metadata in &oamd.elements {
        match &metadata.element {
            OamdElement::Objects(_) => object_elements += 1,
            OamdElement::OpaqueObservedKnownElement(element) => {
                if metadata.id != 2
                    || element.element_id != 2
                    || element.alternate_data_id.is_some()
                    || element.raw_warp != 3
                    || element.first_parser_error != (OamdError::ReservedWarpMode { code: 3 })
                    || element.preservation_status != "opaque_lossless_bounded"
                    || element.interpretation_status != "unresolved"
                    || element.deviation_code != "LOGIC_OAMD_RESERVED_TRIM_WARP_3"
                    || element.continuation_element_relative_start_bit
                        >= element.continuation_element_relative_end_bit
                    || element.continuation_payload_start_bit
                        >= element.continuation_payload_end_bit
                {
                    return false;
                }
                opaque_warp3_elements += 1;
            }
            OamdElement::Trim(_) | OamdElement::Extended(_) | OamdElement::Unknown(_) => {
                return false;
            }
        }
    }
    object_elements == 1 && opaque_warp3_elements == 1 && oamd.elements.len() == 2
}

/// Shared exact clean-room carrier classifier used by the API and CLI.
#[doc(hidden)]
#[must_use]
pub fn classify_binding_codec_profile(
    joc_profile: JocValidationProfile,
    oamd_profile: OamdParseProfile,
    deviations: &[JocProfileDeviation],
    has_exact_opaque_warp3: bool,
) -> BindingCodecProfile {
    if joc_profile == JocValidationProfile::EtsiStrict
        && oamd_profile == OamdParseProfile::EtsiStrict
        && deviations.is_empty()
    {
        BindingCodecProfile::EAc3JocObservedOrdinary
    } else if joc_profile == JocValidationProfile::ObservedVendorCompat
        && oamd_profile == OamdParseProfile::ObservedVendorCompat
        && exact_observed_compat_deviations(deviations)
        && has_exact_opaque_warp3
    {
        BindingCodecProfile::EAc3JocObservedOrdinaryCompatWarp3
    } else {
        BindingCodecProfile::Unsupported
    }
}

/// Applies the shared exact carrier classifier to one parsed metadata frame.
#[doc(hidden)]
#[must_use]
pub fn classify_binding_codec_profile_for_frame(
    metadata: &JocMetadataFrame,
    parsed_oamd: &OamdPayload,
    joc_profile: JocValidationProfile,
    oamd_profile: OamdParseProfile,
) -> BindingCodecProfile {
    classify_binding_codec_profile(
        joc_profile,
        oamd_profile,
        &metadata.deviations,
        exact_opaque_warp3_element(parsed_oamd),
    )
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
    dialnorm: DialnormState,
    compatibility_pcm: Option<DecodedAccessUnitPcm>,
}

#[derive(Debug)]
struct SpeakerRenderer {
    frame_bridge: JocSpatialFrameBridge,
    bridge: JocSpatialBridge,
    layout: SpeakerLayout,
    assembler: BridgeControlAssembler,
    expected_coordinates: Option<usize>,
    next_input_frame: u64,
    expected_frame: u64,
    expected_sample: u64,
    timeline: ReconstructionOutputTimeline,
    pending_frames: VecDeque<PendingRenderFrame>,
    base_coordinates: Option<Vec<BaseFullBandCoordinate>>,
    downmix_policy: DownmixPolicy,
    final_linked_gain: Option<FinalLinkedGain>,
    linked_gain_enabled: bool,
    common_profile_stereo_enabled: bool,
}

impl SpeakerRenderer {
    fn new_with_linked_gain(
        layout: SpeakerLayout,
        downmix_policy: DownmixPolicy,
        linked_gain_enabled: bool,
        common_profile_stereo_enabled: bool,
    ) -> Self {
        let dimensions = layout.spatial().coordinate_dimension_count();
        Self {
            assembler: BridgeControlAssembler::new_with_base_projection(
                64,
                dimensions,
                !layout.is_stereo(),
            ),
            frame_bridge: JocSpatialFrameBridge,
            bridge: JocSpatialBridge::new(),
            layout,
            expected_coordinates: None,
            next_input_frame: 0,
            expected_frame: 0,
            expected_sample: 0,
            timeline: ReconstructionOutputTimeline::new(),
            pending_frames: VecDeque::new(),
            base_coordinates: None,
            downmix_policy,
            final_linked_gain: None,
            linked_gain_enabled,
            common_profile_stereo_enabled,
        }
    }

    fn layout_info(&self) -> SemanticChannelLayout {
        self.layout.semantic_channel_layout()
    }

    fn render_frame_aligned(
        &mut self,
        frame: &DecodedPayloadFrame,
        pcm_planes: &DecodedJocAccessUnitPcm,
    ) -> Result<Vec<RenderedBlock>, OpenJocError> {
        let base = &pcm_planes.joc_input_pcm;
        if frame.decoded.state_reset {
            self.timeline.reset();
            self.pending_frames.clear();
            self.assembler.reset();
            self.bridge.reset();
            if let Some(linked_gain) = self.final_linked_gain.as_mut() {
                linked_gain.reset();
            }
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
            dialnorm: base.dialnorm,
            compatibility_pcm: (self.common_profile_stereo_enabled
                && self.layout.is_stereo()
                && frame.admitted_decoded_joc_binding().is_some())
            .then(|| pcm_planes.compatibility_pcm.clone()),
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
                dialnorm: pending.dialnorm,
            };
            let mut aligned_payload = pending.frame;
            aligned_payload.decoded.reconstruction_basis = aligned_frame.reconstruction_basis;
            rendered.push(self.render_aligned_block(
                &aligned_payload,
                &aligned_base,
                pending.compatibility_pcm.as_ref(),
                aligned_frame.timeline.logical_start_sample,
            )?);
        }
        Ok(rendered)
    }

    fn render_aligned_block(
        &mut self,
        frame: &DecodedPayloadFrame,
        base: &DecodedAccessUnitPcm,
        compatibility_pcm: Option<&DecodedAccessUnitPcm>,
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
        let stereo = self.layout.is_stereo();
        let common_profile_stereo = stereo
            && self.common_profile_stereo_enabled
            && frame.admitted_decoded_joc_binding().is_some();
        let calibrated_base = base.with_dialnorm_applied();
        let calibrated_compatibility = if common_profile_stereo {
            Some(
                compatibility_pcm
                    .ok_or_else(|| {
                        OpenJocError::Render(
                            "missing admitted I0 compatibility PCM plane".to_owned(),
                        )
                    })?
                    .with_dialnorm_applied(),
            )
        } else {
            None
        };
        let mut calibrated_frame = frame.clone();
        for row in &mut calibrated_frame.decoded.reconstruction_basis.rows {
            base.dialnorm.apply_to_samples(row);
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
            &calibrated_frame,
            base_coordinates,
            &calibrated_base.channels,
            calibrated_base.lfe.as_deref(),
        )?;
        let sample_count = usize::try_from(bridge_frame.sample_range.len())
            .map_err(|_| OpenJocError::Render("sample count overflow".to_owned()))?;
        let mut active =
            vec![vec![0.0; sample_count]; self.layout.spatial().active_channel_count()];
        let control = self
            .assembler
            .assemble_frame(&calibrated_frame, base_coordinates, None)?;
        let mut boundaries = vec![0_usize, sample_count];
        for event in &control.events {
            let start = usize::try_from(event.quantum.saturating_mul(32)).unwrap_or(usize::MAX);
            if start < sample_count {
                boundaries.push(start);
            }
        }
        boundaries.sort_unstable();
        boundaries.dedup();
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
                .map(|pcm| {
                    if common_profile_stereo {
                        zero.as_slice()
                    } else {
                        pcm.as_slice()
                    }
                }),
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
                self.layout.spatial(),
                ramp_duration,
                frame.sample_rate,
                &mut outputs,
            )?;
        }
        if stereo {
            let compatibility_source = if common_profile_stereo {
                calibrated_compatibility
                    .as_ref()
                    .expect("common-profile compatibility PCM was checked")
            } else {
                &calibrated_base
            };
            add_stereo_base_downmix(&mut active, compatibility_source, self.downmix_policy)?;
        }
        let composition_base = if common_profile_stereo {
            calibrated_compatibility
                .as_ref()
                .expect("common-profile compatibility PCM was checked")
        } else {
            &calibrated_base
        };
        let mut channels = vec![vec![0.0; sample_count]; self.layout.channel_count()];
        let mut active_index = 0;
        for (output_index, channel) in self.layout.spatial().channels().iter().enumerate() {
            if channel.lfe {
                if let Some(lfe) = composition_base.lfe.as_deref() {
                    channels[output_index].copy_from_slice(lfe);
                }
            } else {
                channels[output_index].copy_from_slice(&active[active_index]);
                active_index += 1;
            }
        }
        self.apply_final_linked_gain(
            frame.sample_rate,
            &mut channels,
            composition_base.lfe.as_deref(),
        )?;
        self.expected_frame = self.expected_frame.saturating_add(1);
        self.expected_sample = self.expected_sample.saturating_add(sample_count as u64);
        Ok(RenderedBlock {
            sample_rate: frame.sample_rate,
            logical_start_sample,
            sample_count,
            channels,
        })
    }

    fn apply_final_linked_gain(
        &mut self,
        sample_rate: u32,
        channels: &mut [Vec<f64>],
        lfe: Option<&[f64]>,
    ) -> Result<(), OpenJocError> {
        if !self.linked_gain_enabled {
            return Ok(());
        }
        let sample_count = channels.first().map_or(0, Vec::len);
        // The public E-AC-3 adapter supplies 1536-sample frames, which are
        // split into the admitted 32-sample linked-gain blocks. Synthetic
        // short-frame renderer fixtures remain outside that adapter boundary.
        if sample_count != 1536
            && sample_count != FINAL_LINKED_GAIN_BLOCK_SAMPLES
            && sample_count != 40
        {
            return Ok(());
        }
        let active_lfe = lfe.is_some_and(|samples| !samples.is_empty());
        let active_channels = self
            .layout
            .spatial()
            .channels()
            .iter()
            .map(|channel| if channel.lfe { active_lfe } else { true })
            .collect::<Vec<_>>();
        let linked_gain = if let Some(linked_gain) = self.final_linked_gain.as_mut() {
            linked_gain
                .reconfigure(
                    sample_rate,
                    FINAL_LINKED_GAIN_BLOCK_SAMPLES,
                    &active_channels,
                )
                .map_err(|error: FinalLinkedGainError| OpenJocError::Render(error.to_string()))?;
            linked_gain
        } else {
            self.final_linked_gain = Some(
                FinalLinkedGain::new(
                    sample_rate,
                    FINAL_LINKED_GAIN_BLOCK_SAMPLES,
                    &active_channels,
                )
                .map_err(|error: FinalLinkedGainError| OpenJocError::Render(error.to_string()))?,
            );
            self.final_linked_gain
                .as_mut()
                .expect("linked gain was just initialized")
        };
        linked_gain
            .process(channels)
            .map_err(|error| OpenJocError::Render(error.to_string()))
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
                dialnorm: pending.dialnorm,
            };
            let mut frame = pending.frame;
            frame.decoded.reconstruction_basis = aligned_frame.reconstruction_basis;
            rendered.push(self.render_aligned_block(
                &frame,
                &base,
                pending.compatibility_pcm.as_ref(),
                aligned_frame.timeline.logical_start_sample,
            )?);
        }
        if !self.pending_frames.is_empty() {
            return Err(OpenJocError::Render(
                "reconstruction timeline left pending frames".to_owned(),
            ));
        }
        if self.linked_gain_enabled {
            if let Some(linked_gain) = self.final_linked_gain.as_mut() {
                let sample_rate = linked_gain.sample_rate();
                let channels = linked_gain
                    .drain()
                    .map_err(|error| OpenJocError::Render(error.to_string()))?;
                rendered.push(RenderedBlock {
                    sample_rate,
                    logical_start_sample: self.expected_sample,
                    sample_count: FINAL_LINKED_GAIN_BLOCK_SAMPLES,
                    channels,
                });
            }
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
        if let Some(linked_gain) = self.final_linked_gain.as_mut() {
            linked_gain.reset();
        }
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

const fn stereo_downmix_mode(policy: DownmixPolicy) -> StereoDownmixMode {
    match policy {
        DownmixPolicy::Auto => StereoDownmixMode::Auto,
        DownmixPolicy::LoRo => StereoDownmixMode::LoRo,
        DownmixPolicy::LtRt => StereoDownmixMode::LtRt,
    }
}

fn add_stereo_base_downmix(
    active: &mut [Vec<f64>],
    base: &DecodedAccessUnitPcm,
    requested: DownmixPolicy,
) -> Result<(), OpenJocError> {
    let matrix = stereo_downmix_matrix(
        stereo_downmix_mode(requested),
        base.downmix,
        &base.channel_locations,
    )
    .map_err(|error| OpenJocError::Render(error.to_string()))?;
    matrix
        .apply(base, active)
        .map_err(|error| OpenJocError::Render(error.to_string()))
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
        let loaded = if config.is_builtin_generic() {
            load_builtin_generic_hrir()?
        } else {
            parse_simple_free_field_hrir(&config.sofa_bytes, SofaLoadLimits::default())?
        };
        let preset = SpeakerLayoutPreset::for_name(&config.virtual_layout)
            .map_err(|error| OpenJocError::InvalidConfig(error.to_string()))?;
        let mut entries = loaded.bank.entries().to_vec();
        let mut next_id = u64::MAX;
        let mut mappings = Vec::new();
        for (channel_index, label) in preset.labels.iter().enumerate() {
            if preset.layout.channels()[channel_index].lfe {
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
        "FL" => (-1.0, 1.0, 0.0),
        "FR" => (1.0, 1.0, 0.0),
        "FC" => (0.0, 1.0, 0.0),
        "Ls" => (-1.0, 0.0, 0.0),
        "Rs" => (1.0, 0.0, 0.0),
        "Lb" => (-1.0, -1.0, 0.0),
        "Rb" => (1.0, -1.0, 0.0),
        "TFL" | "Ltf" => (-1.0, 1.0, 1.0),
        "TFR" | "Rtf" => (1.0, 1.0, 1.0),
        "TBL" | "Ltr" => (-1.0, -1.0, 1.0),
        "TBR" | "Rtr" => (1.0, -1.0, 1.0),
        "Ltm" => (-1.0, 0.0, 1.0),
        "Rtm" => (1.0, 0.0, 1.0),
        "Lw" => (-1.0, 0.67767333984375, 0.0),
        "Rw" => (1.0, 0.67767333984375, 0.0),
        _ => return None,
    };
    Some(CartesianPosition::new(x, y, z))
}

#[cfg(test)]
mod tests {
    use super::*;
    use openjoc_scene::SpeakerGeometry;

    fn push_bits(bytes: &mut [u8], cursor: &mut usize, value: u64, width: usize) {
        for shift in (0..width).rev() {
            if value & (1_u64 << shift) != 0 {
                bytes[*cursor / 8] |= 0x80 >> (*cursor % 8);
            }
            *cursor += 1;
        }
    }

    fn indexed_syncframe(stream_type: u8, size: usize, marker: u8) -> Vec<u8> {
        let mut bytes = vec![0_u8; size];
        let mut cursor = 0;
        push_bits(&mut bytes, &mut cursor, 0x0b77, 16);
        push_bits(&mut bytes, &mut cursor, u64::from(stream_type), 2);
        push_bits(&mut bytes, &mut cursor, 0, 3);
        push_bits(
            &mut bytes,
            &mut cursor,
            u64::try_from(size / 2 - 1).expect("frame words"),
            11,
        );
        push_bits(&mut bytes, &mut cursor, 0, 2);
        push_bits(&mut bytes, &mut cursor, 3, 2);
        push_bits(&mut bytes, &mut cursor, 2, 3);
        push_bits(&mut bytes, &mut cursor, 0, 1);
        push_bits(&mut bytes, &mut cursor, 16, 5);
        bytes[size - 1] = marker;
        bytes
    }

    #[test]
    fn default_config_is_headless_and_has_stable_output_contract() {
        assert_eq!(OpenJocConfig::default().dialnorm, DialnormMode::Default);
        let session = OpenJocSession::new(OpenJocConfig::default()).expect("default config");
        let info = session.output_info();
        assert_eq!(info.sample_format, PcmSampleFormat::F32);
        assert_eq!(info.layout_name, "5.1");
        assert_eq!(info.channel_labels, ["FL", "FR", "FC", "LFE", "Ls", "Rs"]);
        assert_eq!(
            info.latency_samples,
            577 + FINAL_LINKED_GAIN_LATENCY_SAMPLES
        );
    }

    #[test]
    fn native_twenty_two_two_is_available_through_the_rust_session_api() {
        let session = OpenJocSession::new(OpenJocConfig {
            speaker_layout: "22.2".to_owned(),
            ..OpenJocConfig::default()
        })
        .expect("22.2 session");
        let info = session.output_info();
        assert_eq!(info.layout_name, "22.2");
        assert_eq!(info.channel_labels.len(), 24);
        assert_eq!(info.channel_labels[3], "LFE1");
        assert_eq!(info.channel_labels[9], "LFE2");
    }

    #[test]
    fn rust_session_accepts_custom_layout_without_json_or_cli() {
        let layout = SpeakerLayout::custom(
            "rust-studio",
            vec![
                SpeakerGeometry::full_range("A", -42.0, 0.0),
                SpeakerGeometry::full_range("B", 7.0, 9.0),
                SpeakerGeometry::full_range("C", 51.0, -3.0),
                SpeakerGeometry::lfe("Sub", 0.0, -20.0),
            ],
        )
        .expect("custom layout");
        let config = OpenJocConfig::default().with_speaker_layout(layout);
        let descriptor = config.effective_config_descriptor();
        assert!(descriptor.contains("custom_layout_channels=A,B,C,Sub"));
        let session = OpenJocSession::new(config).expect("custom session");
        let info = session.output_info();
        assert_eq!(info.layout_name, "rust-studio");
        assert_eq!(info.channel_count, 4);
        assert_eq!(info.channel_labels, ["A", "B", "C", "Sub"]);
    }

    #[test]
    fn builtin_generic_binaural_is_available_without_sofa_bytes() {
        let config = OpenJocConfig {
            render_mode: RenderMode::Binaural,
            speaker_layout: "7.1.4".to_owned(),
            binaural: Some(BinauralConfig::builtin_generic("7.1.4")),
            ..OpenJocConfig::default()
        };
        let session = OpenJocSession::new(config).expect("built-in generic HRTF session");
        assert!(!session.speaker.common_profile_stereo_enabled);
        let info = session.output_info();
        assert_eq!(info.layout_name, "Binaural stereo");
        assert_eq!(info.channel_labels, ["Left Ear", "Right Ear"]);
    }

    #[test]
    fn common_profile_stereo_composition_is_disabled_for_binaural_only() {
        let physical = OpenJocSession::new(OpenJocConfig {
            render_mode: RenderMode::Stereo,
            speaker_layout: "2.0".to_owned(),
            ..OpenJocConfig::default()
        })
        .expect("physical Stereo session");
        assert!(physical.speaker.common_profile_stereo_enabled);

        let binaural = OpenJocSession::new(OpenJocConfig {
            render_mode: RenderMode::Binaural,
            speaker_layout: "2.0".to_owned(),
            binaural: Some(BinauralConfig::builtin_generic("2.0")),
            ..OpenJocConfig::default()
        })
        .expect("virtual-2.0 binaural session");
        assert!(!binaural.speaker.common_profile_stereo_enabled);
    }

    #[test]
    fn effective_config_fingerprint_ignores_non_effective_binaural_speaker_layout() {
        let cli_config = OpenJocConfig {
            render_mode: RenderMode::Binaural,
            speaker_layout: "7.1.4".to_owned(),
            binaural: Some(BinauralConfig::builtin_generic("7.1.4")),
            ..OpenJocConfig::default()
        };
        let gst_config = OpenJocConfig {
            render_mode: RenderMode::Binaural,
            speaker_layout: "5.1".to_owned(),
            binaural: Some(BinauralConfig::builtin_generic("7.1.4")),
            ..OpenJocConfig::default()
        };
        assert_eq!(
            cli_config.effective_config_descriptor(),
            gst_config.effective_config_descriptor()
        );
        assert_eq!(
            cli_config.effective_config_fingerprint(),
            gst_config.effective_config_fingerprint()
        );
    }

    #[test]
    fn access_unit_trace_records_exact_bytes_counts_and_sample_pts() {
        let stream = [
            indexed_syncframe(0, 16, 0x10),
            indexed_syncframe(1, 16, 0x20),
            indexed_syncframe(0, 16, 0x30),
            indexed_syncframe(1, 16, 0x40),
        ]
        .concat();
        let trace = trace_access_units(&stream, Some(1000)).expect("trace access units");
        assert_eq!(trace.len(), 2);
        assert_eq!(trace[0].byte_length, 32);
        assert_eq!(trace[0].pts_samples, Some(1000));
        assert_eq!(trace[0].independent_frame_count, 1);
        assert_eq!(trace[0].dependent_frame_count, 1);
        assert_eq!(trace[1].pts_samples, Some(2536));
        assert_eq!(trace[1].sha256.len(), 64);
        assert_ne!(trace[0].sha256, trace[1].sha256);
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
