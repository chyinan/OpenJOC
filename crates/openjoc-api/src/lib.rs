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

// pattern: Functional Core

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
    BinauralRenderer, BinauralSourceBlock, CartesianPosition,
    DEFAULT_DYNAMIC_BINAURAL_TRANSITION_SAMPLES, DynamicBinauralRenderer, DynamicBinauralSource,
    FINAL_LINKED_GAIN_BLOCK_SAMPLES, FinalLinkedGain, FinalLinkedGainError, HrirBank, HrirEntry,
    HrirEntryId, MAX_DYNAMIC_BINAURAL_BLOCK_SAMPLES, SourceId, StaticBinauralSource,
};
use openjoc_scene::{
    BaseFullBandCoordinate, BindingCodecProfile, BridgeControlAssembler, DecodedPayloadFrame,
    JocFrameInput, JocSpatialBridge, JocSpatialFrameBridge, PayloadDecoder, PayloadDecoderConfig,
    SemanticChannelLayout, SpeakerLayout, SpeakerLayoutPreset,
};
use openjoc_sofa::{
    BUILTIN_GENERIC_HRTF_SAMPLE_RATE_HZ, load_builtin_hrir_f32, load_builtin_hrir_f32_from_asset,
    parse_simple_free_field_hrir, resample_loaded_hrir_bank, resolve_hrir, resolve_hrir_f32,
};

pub use openjoc_sofa::{BuiltinHrtf, SofaLoadLimits};
use sha2::{Digest, Sha256};
use std::{collections::VecDeque, fmt, fmt::Write as _, time::Duration};
#[cfg(not(target_arch = "wasm32"))]
use std::{sync::OnceLock, time::Instant};

pub mod listener_orientation;
pub use listener_orientation::{
    ListenerOrientation, ListenerOrientationError, ListenerOrientationPrepareError,
    ListenerOrientationPreparer, MAX_DYNAMIC_HRIR_TAPS,
};
pub use openjoc_render::{
    AppliedBinauralUpdate, BinauralResourceIdentity, BinauralUpdateAcceptance,
    BinauralUpdateApplyFailure, PreparedBinauralKernel, PreparedBinauralUpdate, RenderError,
};

/// The first public C ABI is intentionally experimental. This is separate
/// from the Rust package version and may evolve during the OpenJOC 0.x series.
pub const API_MATURITY: &str = "experimental";
/// The declared QMF/Base-RB reconstruction delay in samples.
pub const QMF_LATENCY_SAMPLES: usize = ReconstructionOutputTimeline::qmf_latency_samples();
/// The admitted causal speaker-stage block delay at the 48-kHz adapter.
pub const FINAL_LINKED_GAIN_LATENCY_SAMPLES: usize = FINAL_LINKED_GAIN_BLOCK_SAMPLES;
/// The canonical v1 PCM sample format.
pub const PCM_SAMPLE_FORMAT: PcmSampleFormat = PcmSampleFormat::F32;
/// Maximum number of samples returned by one experimental listener-orientation
/// pull call. Smaller caller-selected limits are supported.
pub const MAX_LISTENER_ORIENTATION_PULL_SAMPLES: usize = 256;
const MAX_DEFERRED_PULL_AU_SAMPLES: usize = 1536;

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
/// selected built-in resource; non-empty bytes select an explicit user SOFA
/// and retain the strict SimpleFreeFieldHRIR parser behavior. Matching-rate
/// taps are preserved; other rates are resampled for binaural output.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BinauralConfig {
    /// Explicit SOFA bytes, or empty to use [`Self::builtin_generic`].
    pub sofa_bytes: Vec<u8>,
    pub virtual_layout: String,
    pub lfe_policy: BinauralLfePolicy,
    pub builtin_hrtf: BuiltinHrtf,
}

impl BinauralConfig {
    /// Selects the default SADIE II D1 / KU100 resource.
    #[must_use]
    pub fn builtin_generic(virtual_layout: impl Into<String>) -> Self {
        Self::builtin(BuiltinHrtf::SadieD1Ku100, virtual_layout)
    }

    /// Selects one checked-in built-in resource without a filesystem path.
    #[must_use]
    pub fn builtin(builtin_hrtf: BuiltinHrtf, virtual_layout: impl Into<String>) -> Self {
        Self {
            sofa_bytes: Vec::new(),
            virtual_layout: virtual_layout.into(),
            lfe_policy: BinauralLfePolicy::Exclude,
            builtin_hrtf,
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
            builtin_hrtf: BuiltinHrtf::SadieD1Ku100,
        }
    }

    #[must_use]
    fn is_builtin(&self) -> bool {
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
    /// than merely user-visible configuration. Custom layouts include ordered
    /// channel roles and fixed/named route vectors.
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
            descriptor.push_str("\ncustom_layout_roles=");
            descriptor.push_str(
                &layout
                    .spatial()
                    .channels()
                    .iter()
                    .map(|channel| if channel.lfe { "lfe" } else { "full_range" })
                    .collect::<Vec<_>>()
                    .join(","),
            );
            let mut routes = layout.spatial().route_vectors().iter().collect::<Vec<_>>();
            routes.sort_by(|left, right| left.identity.cmp(&right.identity));
            let _ = write!(descriptor, "\ncustom_layout_route_count={}", routes.len());
            for (index, route) in routes.iter().enumerate() {
                // Length framing permits opaque route identities; exact IEEE bits
                // retain all validated gain precision and ordered output components.
                let _ = write!(
                    descriptor,
                    "\ncustom_layout_route_{index}={}:{}:{}",
                    route.identity.len(),
                    route.identity,
                    route.vector.len(),
                );
                for gain in &route.vector {
                    let _ = write!(descriptor, ":{:016x}", gain.to_bits());
                }
            }
            for (index, coordinate) in layout.channel_coordinates().iter().enumerate() {
                let _ = write!(
                    descriptor,
                    "\ncustom_layout_coordinate_{index}={:.9},{:.9},{:.9}",
                    coordinate[0], coordinate[1], coordinate[2]
                );
            }
        }
        if let Some(binaural) = &self.binaural {
            let (hrtf_source, hrtf_sha256) = if binaural.is_builtin() {
                let source = match binaural.builtin_hrtf {
                    BuiltinHrtf::SadieD1Ku100 => "builtin:SADIE_II_D1_KU100_v2-2".to_owned(),
                    preset @ BuiltinHrtf::SadieD2Kemar => format!("builtin:{}", preset.id()),
                };
                (
                    source,
                    binaural
                        .builtin_hrtf
                        .asset_metadata()
                        .asset_sha256
                        .to_owned(),
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
        if self.render_mode != RenderMode::Binaural && self.binaural.is_some() {
            return Err(OpenJocError::InvalidConfig(
                "BinauralConfig is only valid for binaural render mode".to_owned(),
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

fn joc_profile_name(profile: JocValidationProfile) -> &'static str {
    match profile {
        JocValidationProfile::EtsiStrict => "etsi-strict",
        JocValidationProfile::ObservedVendorCompat => "observed-vendor-compat",
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

#[cfg(target_arch = "wasm32")]
fn clock_now_ms() -> f64 {
    wasm_clock::now_ms()
}

#[cfg(not(target_arch = "wasm32"))]
fn clock_now_ms() -> f64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_secs_f64() * 1000.0
}

fn elapsed_since_ms(start_ms: f64) -> Duration {
    Duration::from_secs_f64((clock_now_ms() - start_ms).max(0.0) / 1000.0)
}

#[cfg(target_arch = "wasm32")]
mod wasm_clock {
    #![allow(unsafe_code)]

    #[link(wasm_import_module = "env")]
    unsafe extern "C" {
        fn openjoc_wasm_clock_now_ms() -> f64;
    }

    pub fn now_ms() -> f64 {
        // SAFETY: The browser and parity harness provide this named clock
        // import when instantiating the raw WASM module.
        unsafe { openjoc_wasm_clock_now_ms() }
    }
}

/// Borrowed compressed input. A packet is exactly one complete General JOC
/// access unit: ordered I0/D0..Dn programme sets covering six cumulative
/// audio blocks, with short syncframes grouped before JOC processing.
#[derive(Clone, Copy, Debug)]
pub struct OpenJocPacket<'a> {
    pub data: &'a [u8],
    /// Sample-domain PTS for the first sample in this packet. The sample time
    /// base is the decoded stream rate; `None` means no packet timestamp.
    /// The first supplied PTS anchors the segment even after untimed packets:
    /// its origin is this PTS minus the samples already decoded. Previously
    /// returned frames are unchanged. Later supplied PTS must match that origin.
    /// An unrepresentable origin or expected PTS is rejected before decode.
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
            openjoc_eac3::validate_short_access_unit_convsync(stream, &frames, unit)?;
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

/// Stable stream facts retained for adapter diagnostics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpenJocDiagnostics {
    pub profile: Option<&'static str>,
    pub downmix_index: Option<u8>,
    pub object_count: Option<u16>,
    pub complexity_index: Option<u8>,
}

/// Opt-in timing for one successful AU push through the headless session.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OpenJocStageTiming {
    pub decode: Duration,
    pub render: Duration,
    pub binaural: Duration,
    pub total: Duration,
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
    pending_binaural_inputs: VecDeque<RenderedBlock>,
    current_binaural_input: Option<(RenderedBlock, usize)>,
    pending_binaural_input_samples: usize,
    binaural_pull_samples: Option<usize>,
    selected_profile: Option<JocValidationProfile>,
    core_stream_type: Option<StreamType>,
    legacy_core_configuration: Option<LegacyCoreConfiguration>,
    dither_values: Vec<f64>,
    sample_rate: Option<u32>,
    segment_pts: Option<i64>,
    next_input_sample: u64,
    last_output_end: u64,
    downmix_index: Option<u8>,
    object_count: Option<u16>,
    complexity_index: Option<u8>,
    stage_timing_enabled: bool,
    last_stage_timing: OpenJocStageTiming,
    drained: bool,
    terminal_error: Option<String>,
}

impl OpenJocSession {
    /// Creates a validated, independent session. No process-global state is
    /// touched; separate sessions can run concurrently on separate threads.
    pub fn new(config: OpenJocConfig) -> Result<Self, OpenJocError> {
        Self::new_with_builtin_hrtf_asset(config, None)
    }

    /// Creates an explicitly opted-in binaural session with device-independent
    /// listener-orientation preparation and bounded dynamic FIR updates.
    ///
    /// The default [`Self::new`] path remains static. This session expects the
    /// host to prepare poses away from rendering and submit them between calls.
    pub fn new_with_listener_orientation(config: OpenJocConfig) -> Result<Self, OpenJocError> {
        Self::new_with_session_options(config, None, None, true, None, 0, None)
    }

    /// Creates an opt-in orientation session that exposes projected virtual-speaker
    /// PCM through bounded pull-sized binaural output blocks.
    ///
    /// The session retains at most one decoded access unit of pre-binaural PCM;
    /// `receive_binaural_frame` renders no more than `max_pull_samples` per call.
    /// This makes pose updates eligible between those pulls without changing the
    /// virtual-speaker projection, gain policy, or FIR kernels. It is an experimental
    /// scheduling interface, not a sensor-to-sound latency guarantee.
    pub fn new_with_listener_orientation_pull(
        config: OpenJocConfig,
        max_pull_samples: usize,
    ) -> Result<Self, OpenJocError> {
        Self::new_with_listener_orientation_pull_at_epoch(config, max_pull_samples, 0)
    }

    /// Creates an orientation pull session at a caller-maintained stream epoch.
    /// This supports adapters that lazily discard and recreate their session
    /// while continuing to reject pose updates prepared before a reset.
    pub fn new_with_listener_orientation_pull_at_epoch(
        config: OpenJocConfig,
        max_pull_samples: usize,
        stream_epoch: u64,
    ) -> Result<Self, OpenJocError> {
        if max_pull_samples == 0 || max_pull_samples > MAX_LISTENER_ORIENTATION_PULL_SAMPLES {
            return Err(OpenJocError::InvalidConfig(format!(
                "binaural pull block must be in 1..={MAX_LISTENER_ORIENTATION_PULL_SAMPLES} samples"
            )));
        }
        Self::new_with_session_options(
            config,
            None,
            None,
            true,
            Some(max_pull_samples),
            stream_epoch,
            None,
        )
    }

    /// Pull-session constructor that reuses an immutable preparer captured by
    /// an adapter or worker. Resource identity is validated before sharing.
    pub fn new_with_listener_orientation_pull_using_preparer(
        config: OpenJocConfig,
        max_pull_samples: usize,
        stream_epoch: u64,
        preparer: ListenerOrientationPreparer,
    ) -> Result<Self, OpenJocError> {
        if max_pull_samples == 0 || max_pull_samples > MAX_LISTENER_ORIENTATION_PULL_SAMPLES {
            return Err(OpenJocError::InvalidConfig(format!(
                "binaural pull block must be in 1..={MAX_LISTENER_ORIENTATION_PULL_SAMPLES} samples"
            )));
        }
        Self::new_with_session_options(
            config,
            None,
            None,
            true,
            Some(max_pull_samples),
            stream_epoch,
            Some(preparer),
        )
    }

    /// Creates a session from a borrowed, external built-in HRTF asset.
    /// The asset is validated synchronously and is not retained.
    pub fn new_with_hrtf_asset(config: OpenJocConfig, asset: &[u8]) -> Result<Self, OpenJocError> {
        Self::new_with_builtin_hrtf_asset(config, Some(asset))
    }

    /// Creates a session while applying caller-selected limits to explicit
    /// custom SOFA parsing. Built-in HRTF asset loading is unchanged.
    pub fn new_with_sofa_load_limits(
        config: OpenJocConfig,
        sofa_load_limits: SofaLoadLimits,
    ) -> Result<Self, OpenJocError> {
        Self::new_with_session_options(config, None, Some(sofa_load_limits), false, None, 0, None)
    }

    fn new_with_builtin_hrtf_asset(
        config: OpenJocConfig,
        external_asset: Option<&[u8]>,
    ) -> Result<Self, OpenJocError> {
        Self::new_with_session_options(config, external_asset, None, false, None, 0, None)
    }

    fn new_with_session_options(
        config: OpenJocConfig,
        external_asset: Option<&[u8]>,
        custom_sofa_load_limits: Option<SofaLoadLimits>,
        listener_orientation_enabled: bool,
        binaural_pull_samples: Option<usize>,
        initial_stream_epoch: u64,
        orientation_preparer: Option<ListenerOrientationPreparer>,
    ) -> Result<Self, OpenJocError> {
        config.validate()?;
        if listener_orientation_enabled && config.render_mode != RenderMode::Binaural {
            return Err(OpenJocError::InvalidConfig(
                "listener orientation is only valid for binaural rendering".to_owned(),
            ));
        }
        if binaural_pull_samples.is_some() && !listener_orientation_enabled {
            return Err(OpenJocError::InvalidConfig(
                "binaural pull rendering requires listener-orientation support".to_owned(),
            ));
        }
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
            .map(|binaural| {
                if listener_orientation_enabled {
                    BinauralState::new_with_listener_orientation_at_epoch(
                        binaural,
                        external_asset,
                        custom_sofa_load_limits,
                        initial_stream_epoch,
                        orientation_preparer,
                    )
                } else {
                    BinauralState::new(binaural, external_asset, custom_sofa_load_limits)
                }
            })
            .transpose()?;
        let mut audio_decoder = JocAccessUnitPcmDecoder::new();
        audio_decoder.set_dialnorm_mode(config.dialnorm);
        Ok(Self {
            payload_decoder: new_payload_decoder(&config),
            audio_decoder,
            speaker,
            binaural,
            output_queue: VecDeque::new(),
            pending_binaural_inputs: VecDeque::new(),
            current_binaural_input: None,
            pending_binaural_input_samples: 0,
            binaural_pull_samples,
            selected_profile: None,
            core_stream_type: None,
            legacy_core_configuration: None,
            dither_values: dither_values(),
            sample_rate: None,
            segment_pts: None,
            next_input_sample: 0,
            last_output_end: 0,
            downmix_index: None,
            object_count: None,
            complexity_index: None,
            stage_timing_enabled: false,
            last_stage_timing: OpenJocStageTiming::default(),
            drained: false,
            terminal_error: None,
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

    /// Returns the known decoder, reconstruction, and added HRIR conversion delay.
    #[must_use]
    pub fn latency_samples(&self) -> usize {
        if self.config.render_mode == RenderMode::Binaural {
            QMF_LATENCY_SAMPLES
                + self
                    .binaural
                    .as_ref()
                    .map_or(0, |state| state.lfe_delay.delay_samples())
        } else {
            QMF_LATENCY_SAMPLES + FINAL_LINKED_GAIN_LATENCY_SAMPLES
        }
    }

    /// Returns the effective session descriptor, including the opt-in dynamic
    /// orientation contract when this session was explicitly enabled.
    #[must_use]
    pub fn effective_config_descriptor(&self) -> String {
        let mut descriptor = self.config.effective_config_descriptor();
        if self
            .binaural
            .as_ref()
            .is_some_and(BinauralState::listener_orientation_enabled)
        {
            let transition_samples = DEFAULT_DYNAMIC_BINAURAL_TRANSITION_SAMPLES;
            let max_hrir_taps = MAX_DYNAMIC_HRIR_TAPS;
            let max_block_samples = MAX_DYNAMIC_BINAURAL_BLOCK_SAMPLES;
            let _ = write!(
                descriptor,
                "\nlistener_orientation=enabled\nlistener_orientation_transition_samples={transition_samples}\nlistener_orientation_max_hrir_taps={max_hrir_taps}\nlistener_orientation_max_block_samples={max_block_samples}",
            );
            if let Some(pull_samples) = self.binaural_pull_samples {
                let _ = write!(
                    descriptor,
                    "\nlistener_orientation_pull=enabled\nlistener_orientation_pull_max_samples={pull_samples}"
                );
            }
        }
        descriptor
    }

    /// Returns a deterministic fingerprint of this session's effective
    /// configuration, including orientation support only when opted in.
    #[must_use]
    pub fn effective_config_fingerprint(&self) -> String {
        sha256_hex(self.effective_config_descriptor().as_bytes())
    }

    /// Returns a cloneable immutable preparation context when orientation was enabled.
    #[must_use]
    pub fn listener_orientation_preparer(&self) -> Option<ListenerOrientationPreparer> {
        self.binaural
            .as_ref()
            .and_then(BinauralState::listener_orientation_preparer)
    }

    /// Returns the current stream epoch for updates prepared on a worker thread.
    #[must_use]
    pub fn listener_orientation_stream_epoch(&self) -> Option<u64> {
        self.binaural
            .as_ref()
            .and_then(BinauralState::listener_orientation_stream_epoch)
    }

    /// Queues a complete prepared pose for the next not-yet-rendered binaural block.
    ///
    /// On validation failure, the returned error retains the caller's original
    /// update. Successful receipts may also contain retired kernel buffers;
    /// release those away from any audio callback.
    pub fn apply_prepared_listener_orientation(
        &mut self,
        update: PreparedBinauralUpdate,
    ) -> Result<BinauralUpdateAcceptance, BinauralUpdateApplyFailure> {
        if self.terminal_error.is_some() {
            return Err(BinauralUpdateApplyFailure {
                error: openjoc_render::RenderError::BinauralRequiresReset,
                update,
            });
        }
        if self.drained {
            return Err(BinauralUpdateApplyFailure {
                error: openjoc_render::RenderError::BinauralInputAfterTailStart,
                update,
            });
        }
        match self.binaural.as_mut() {
            Some(state) => state.apply_prepared_listener_orientation(update),
            None => Err(BinauralUpdateApplyFailure {
                error: openjoc_render::RenderError::BinauralOrientationNotEnabled,
                update,
            }),
        }
    }

    /// Returns the most recently applied pose and its actual rendered sample boundary.
    #[must_use]
    pub fn last_applied_listener_orientation(&self) -> Option<AppliedBinauralUpdate> {
        self.binaural
            .as_ref()
            .and_then(BinauralState::last_applied_listener_orientation)
    }

    /// Returns a target sequence that has not started affecting rendered PCM.
    #[must_use]
    pub fn pending_listener_orientation_sequence(&self) -> Option<u64> {
        self.binaural
            .as_ref()
            .and_then(BinauralState::pending_listener_orientation_sequence)
    }

    /// Returns the number of projected virtual-speaker input samples still
    /// waiting for pull-side binauralization. `None` means pull mode is disabled.
    #[must_use]
    pub fn pending_binaural_input_samples(&self) -> Option<usize> {
        self.binaural_pull_samples
            .map(|_| self.pending_binaural_input_samples)
    }

    /// Returns the bounded profile/topology facts observed in decoded metadata.
    #[must_use]
    pub fn diagnostics(&self) -> OpenJocDiagnostics {
        OpenJocDiagnostics {
            profile: self.selected_profile.map(joc_profile_name),
            downmix_index: self.downmix_index,
            object_count: self.object_count,
            complexity_index: self.complexity_index,
        }
    }

    /// Enables opt-in decode/render timing for later successful AU pushes.
    pub fn enable_stage_timing(&mut self) {
        self.stage_timing_enabled = true;
        self.last_stage_timing = OpenJocStageTiming::default();
    }

    /// Takes the most recent successful AU stage timing record.
    pub fn take_stage_timing(&mut self) -> OpenJocStageTiming {
        std::mem::take(&mut self.last_stage_timing)
    }

    /// Sends one complete access unit. Caller packet memory is borrowed only
    /// for this call; the session copies only decoded PCM into bounded state.
    pub fn push_packet(
        &mut self,
        packet: OpenJocPacket<'_>,
    ) -> Result<OpenJocStatus, OpenJocError> {
        let total_start = self.stage_timing_enabled.then(clock_now_ms);
        if let Some(message) = &self.terminal_error {
            return Err(OpenJocError::Render(message.clone()));
        }
        if self.drained {
            return Err(OpenJocError::AlreadyDrained);
        }
        if self.has_pending_output() {
            return Ok(OpenJocStatus::OutputPending);
        }
        if packet.data.is_empty() {
            return Err(OpenJocError::InvalidPacket("packet is empty".to_owned()));
        }
        if packet.data.len() > openjoc_eac3::GENERAL_MAX_ACCESS_UNIT_BYTES {
            return Err(OpenJocError::InvalidPacket(format!(
                "packet exceeds the bounded {0}-byte General JOC access-unit limit",
                openjoc_eac3::GENERAL_MAX_ACCESS_UNIT_BYTES
            )));
        }
        if packet.discontinuity {
            if let Err(error) = self.reset_stream_state() {
                self.fail_closed(&error);
                return Err(error);
            }
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

        let decode_start = self.stage_timing_enabled.then(clock_now_ms);
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
        self.downmix_index
            .get_or_insert(parsed_joc.header.downmix_index);
        self.object_count
            .get_or_insert(parsed_oamd.prefix.object_count);
        self.complexity_index
            .get_or_insert(metadata.complexity_index);
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
        let decode_elapsed = decode_start.map(elapsed_since_ms);
        self.next_input_sample = self
            .next_input_sample
            .checked_add(u64::from(unit.samples))
            .ok_or_else(|| OpenJocError::Decode("sample timeline overflow".to_owned()))?;
        let render_start = self.stage_timing_enabled.then(clock_now_ms);
        let rendered = self.speaker.render_frame_aligned(&frame, &pcm_planes)?;
        let render_elapsed = render_start.map(elapsed_since_ms);
        let binaural_start = self.stage_timing_enabled.then(clock_now_ms);
        self.emit_rendered(rendered)?;
        let binaural_elapsed = binaural_start.map(elapsed_since_ms).map(|elapsed| {
            if self.config.render_mode == RenderMode::Binaural {
                elapsed
            } else {
                Duration::ZERO
            }
        });
        if let (Some(total_start), Some(decode), Some(render), Some(binaural)) = (
            total_start,
            decode_elapsed,
            render_elapsed,
            binaural_elapsed,
        ) {
            self.last_stage_timing = OpenJocStageTiming {
                decode,
                render,
                binaural,
                total: elapsed_since_ms(total_start),
            };
        }
        self.core_stream_type = Some(core_stream_type);
        self.legacy_core_configuration = legacy_core_configuration;
        // `preroll` is retained as an explicit input fact for future seek
        // adapters. It is decoded normally in this first ABI because the
        // decoder cannot discard a delayed frame without a caller policy.
        let _ = packet.preroll;
        Ok(if self.has_pending_output() {
            OpenJocStatus::FrameAvailable
        } else {
            OpenJocStatus::NeedMoreInput
        })
    }

    /// Receives one owned PCM frame. The queue is bounded to frames produced
    /// by one send/drain operation; callers should receive before pushing.
    /// Returns `None` in pull-binaural sessions; use
    /// [`Self::receive_binaural_frame`] there.
    pub fn receive_frame(&mut self) -> Option<OpenJocPcmFrame> {
        if self.binaural_pull_samples.is_some() {
            return None;
        }
        self.output_queue.pop_front()
    }

    /// Renders and receives at most the configured pull size from a binaural
    /// orientation session. A returned frame is one chunk of the current AU;
    /// apply pose updates between calls to change the next not-yet-rendered chunk.
    /// Reconstruction and FIR tails are also exposed incrementally after `drain`.
    pub fn receive_binaural_frame(&mut self) -> Result<Option<OpenJocPcmFrame>, OpenJocError> {
        if let Some(message) = &self.terminal_error {
            return Err(OpenJocError::Render(message.clone()));
        }
        let Some(max_pull_samples) = self.binaural_pull_samples else {
            return Err(OpenJocError::InvalidConfig(
                "bounded binaural pull is not enabled for this session".to_owned(),
            ));
        };
        if let Some((frame, offset)) = self.current_binaural_input.take().or_else(|| {
            self.pending_binaural_inputs
                .pop_front()
                .map(|frame| (frame, 0))
        }) {
            let count = max_pull_samples.min(frame.sample_count.saturating_sub(offset));
            if count == 0 {
                self.fail_closed(&OpenJocError::Render(
                    "empty deferred binaural input block".to_owned(),
                ));
                return Err(OpenJocError::Render(
                    "empty deferred binaural input block".to_owned(),
                ));
            }
            let binaural = self.binaural.as_mut().ok_or_else(|| {
                OpenJocError::Render("binaural session state is unavailable".to_owned())
            })?;
            let rendered = match binaural.render_range(&frame, offset, count) {
                Ok(rendered) => rendered,
                Err(error) => {
                    self.fail_closed(&error);
                    return Err(error);
                }
            };
            let result = self.to_pcm_frame(&rendered);
            let pcm = match result {
                Ok(pcm) => pcm,
                Err(error) => {
                    self.fail_closed(&error);
                    return Err(error);
                }
            };
            if offset + count < frame.sample_count {
                self.current_binaural_input = Some((frame, offset + count));
            }
            self.pending_binaural_input_samples =
                self.pending_binaural_input_samples.saturating_sub(count);
            return Ok(Some(pcm));
        }
        if self.drained {
            let start = self.last_output_end;
            let tail = self
                .binaural
                .as_mut()
                .ok_or_else(|| {
                    OpenJocError::Render("binaural session state is unavailable".to_owned())
                })?
                .drain_tail_chunk(max_pull_samples, start);
            match tail {
                Ok(Some(frame)) => {
                    self.last_output_end = self.last_output_end.max(
                        frame
                            .logical_start_sample
                            .saturating_add(frame.sample_count as u64),
                    );
                    return match self.to_pcm_frame(&frame) {
                        Ok(pcm) => Ok(Some(pcm)),
                        Err(error) => {
                            self.fail_closed(&error);
                            Err(error)
                        }
                    };
                }
                Ok(None) => return Ok(None),
                Err(error) => {
                    self.fail_closed(&error);
                    return Err(error);
                }
            }
        }
        Ok(None)
    }

    /// Whether `drain` has completed and no further frame can be received.
    #[must_use]
    pub fn is_drained(&self) -> bool {
        self.drained
            && (self.terminal_error.is_some()
                || (self.output_queue.is_empty()
                    && self.pending_binaural_inputs.is_empty()
                    && self.current_binaural_input.is_none()
                    && (self.binaural_pull_samples.is_none()
                        || self
                            .binaural
                            .as_ref()
                            .is_none_or(BinauralState::has_no_tail))))
    }

    /// Flushes delayed QMF/reconstruction and SOFA FIR tail output.
    pub fn drain(&mut self) -> Result<OpenJocStatus, OpenJocError> {
        if let Some(message) = &self.terminal_error {
            return Err(OpenJocError::Render(message.clone()));
        }
        if self.has_pending_output() {
            return Ok(OpenJocStatus::OutputPending);
        }
        if self.drained {
            if self.has_unrendered_pull_tail() {
                return Ok(OpenJocStatus::FrameAvailable);
            }
            return Ok(OpenJocStatus::EndOfStream);
        }
        self.drained = true;
        let result = (|| {
            if self.sample_rate.is_none() {
                if let Some(binaural) = self.binaural.as_mut() {
                    binaural.begin_drain()?;
                }
                return Ok(OpenJocStatus::EndOfStream);
            }
            if let Some(binaural) = self.binaural.as_mut() {
                binaural.begin_drain()?;
            }
            let payload =
                std::mem::replace(&mut self.payload_decoder, new_payload_decoder(&self.config));
            let (_, reconstruction_tail) = payload.finish_streaming_with_reconstruction_tail()?;
            let rendered = self
                .speaker
                .finish_with_reconstruction_tail(&reconstruction_tail)?;
            self.emit_rendered(rendered)?;
            if self.binaural_pull_samples.is_none() {
                if let Some(binaural) = self.binaural.as_mut() {
                    let mut tail_start = self.last_output_end;
                    for frame in binaural.drain_tail(self.sample_rate.unwrap_or(0), tail_start)? {
                        tail_start = tail_start.saturating_add(frame.sample_count as u64);
                        self.output_queue.push_back(self.to_pcm_frame(&frame)?);
                    }
                }
            }
            Ok(
                if !self.has_pending_output() && !self.has_unrendered_pull_tail() {
                    OpenJocStatus::EndOfStream
                } else {
                    OpenJocStatus::FrameAvailable
                },
            )
        })();
        match result {
            Ok(status) => Ok(status),
            Err(error) => {
                self.fail_closed(&error);
                Err(error)
            }
        }
    }

    /// Discards pending output and all stream-derived decoder state while
    /// retaining the immutable configuration.
    pub fn flush(&mut self) {
        let _ = self.try_flush();
    }

    /// Fallible form of [`Self::flush`] that reports a fail-closed orientation
    /// epoch overflow or other reset error.
    pub fn try_flush(&mut self) -> Result<(), OpenJocError> {
        self.output_queue.clear();
        self.pending_binaural_inputs.clear();
        self.current_binaural_input = None;
        self.pending_binaural_input_samples = 0;
        match self.reset_stream_state() {
            Ok(()) => {
                self.drained = false;
                self.terminal_error = None;
                Ok(())
            }
            Err(error) => {
                self.fail_closed(&error);
                Err(error)
            }
        }
    }

    /// Resets state for a new timeline/seek. Configuration and SOFA data stay
    /// prepared; no state from the previous stream is reused.
    pub fn reset(&mut self) {
        self.flush();
    }

    /// Fallible form of [`Self::reset`].
    pub fn try_reset(&mut self) -> Result<(), OpenJocError> {
        self.try_flush()
    }

    fn reset_stream_state(&mut self) -> Result<(), OpenJocError> {
        self.audio_decoder.reset();
        self.audio_decoder.set_dialnorm_mode(self.config.dialnorm);
        self.payload_decoder = new_payload_decoder(&self.config);
        self.speaker.reset();
        if let Some(binaural) = self.binaural.as_mut() {
            binaural.reset()?;
        }
        self.selected_profile = None;
        self.core_stream_type = None;
        self.legacy_core_configuration = None;
        self.sample_rate = None;
        self.segment_pts = None;
        self.next_input_sample = 0;
        self.last_output_end = 0;
        self.downmix_index = None;
        self.object_count = None;
        self.complexity_index = None;
        Ok(())
    }

    fn fail_closed(&mut self, error: &OpenJocError) {
        self.output_queue.clear();
        self.pending_binaural_inputs.clear();
        self.current_binaural_input = None;
        self.pending_binaural_input_samples = 0;
        self.drained = true;
        self.terminal_error = Some(error.to_string());
    }

    fn check_timestamp(&mut self, pts: Option<i64>) -> Result<(), OpenJocError> {
        let Some(pts) = pts else { return Ok(()) };
        let offset = i64::try_from(self.next_input_sample).map_err(|_| {
            OpenJocError::InvalidPacket("timestamp sample offset exceeds i64".to_owned())
        })?;
        if let Some(origin) = self.segment_pts {
            let expected = origin.checked_add(offset).ok_or_else(|| {
                OpenJocError::InvalidPacket("expected packet timestamp exceeds i64".to_owned())
            })?;
            if expected != pts {
                return Err(OpenJocError::TimestampDiscontinuity {
                    expected,
                    actual: pts,
                });
            }
        } else {
            let origin = pts.checked_sub(offset).ok_or_else(|| {
                OpenJocError::InvalidPacket("timestamp segment origin exceeds i64".to_owned())
            })?;
            self.segment_pts = Some(origin);
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

    fn emit_rendered(&mut self, mut rendered: Vec<RenderedBlock>) -> Result<(), OpenJocError> {
        if self.binaural_pull_samples.is_some() {
            // Push may enqueue only one access unit. Drain runs only after that
            // AU has been pulled, and may then enqueue at most one delayed
            // speaker-frame reconstruction tail. Validate the complete batch
            // before moving any block into the persistent queue.
            rendered.retain(|block| block.sample_count > 0);
            let maximum = MAX_DEFERRED_PULL_AU_SAMPLES;
            let mut batch_samples = 0_usize;
            for block in &rendered {
                let Some(next_samples) = batch_samples.checked_add(block.sample_count) else {
                    let error =
                        OpenJocError::Render("deferred binaural input size overflow".to_owned());
                    self.fail_closed(&error);
                    return Err(error);
                };
                batch_samples = next_samples;
            }
            if batch_samples > maximum || self.pending_binaural_input_samples != 0 {
                let error = OpenJocError::Render(format!(
                    "deferred binaural batch ({batch_samples} samples) exceeds the separate one-AU / one-delayed-frame {maximum}-sample bound or overlaps pending input"
                ));
                self.fail_closed(&error);
                return Err(error);
            }
            self.pending_binaural_input_samples = batch_samples;
            for block in rendered {
                self.last_output_end = self.last_output_end.max(
                    block
                        .logical_start_sample
                        .saturating_add(block.sample_count as u64),
                );
                self.pending_binaural_inputs.push_back(block);
            }
            return Ok(());
        }
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

    fn has_pending_output(&self) -> bool {
        !self.output_queue.is_empty()
            || !self.pending_binaural_inputs.is_empty()
            || self.current_binaural_input.is_some()
    }

    fn has_unrendered_pull_tail(&self) -> bool {
        self.binaural_pull_samples.is_some()
            && self.drained
            && self
                .binaural
                .as_ref()
                .is_some_and(|state| !state.has_no_tail())
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
        let pts_samples = self
            .segment_pts
            .map(|origin| {
                i64::try_from(frame.logical_start_sample)
                    .ok()
                    .and_then(|offset| origin.checked_add(offset))
                    .ok_or_else(|| OpenJocError::Render("output timestamp exceeds i64".to_owned()))
            })
            .transpose()?;
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
    bank: Option<HrirBank>,
    sample_rate_hz: u32,
    mappings: Vec<BinauralMapping>,
    lfe_index: Option<usize>,
    lfe_policy: BinauralLfePolicy,
    lfe_delay: openjoc_render::SampleDelay,
    engine: Option<BinauralRenderer>,
    orientation_preparer: Option<ListenerOrientationPreparer>,
    dynamic_engine: Option<DynamicBinauralRenderer>,
    last_orientation_receipt: Option<AppliedBinauralUpdate>,
    drain_started: bool,
}

#[derive(Clone, Copy, Debug)]
struct BinauralMapping {
    channel_index: usize,
    source_id: SourceId,
    hrir_entry: HrirEntryId,
}

impl BinauralState {
    fn new(
        config: &BinauralConfig,
        external_asset: Option<&[u8]>,
        custom_sofa_load_limits: Option<SofaLoadLimits>,
    ) -> Result<Self, OpenJocError> {
        let mut lfe_delay_samples = 0;
        let (bank, mappings, lfe_index, sample_rate_hz) = if config.is_builtin() {
            let loaded = if let Some(asset) = external_asset {
                load_builtin_hrir_f32_from_asset(config.builtin_hrtf, asset)?
            } else {
                load_builtin_hrir_f32(config.builtin_hrtf)?
            };
            validate_binaural_sample_rate(loaded.metadata.sample_rate_hz)?;
            prepare_binaural_bank(
                loaded.metadata.sample_rate_hz,
                &config.virtual_layout,
                |direction| {
                    // System H includes canonical directions whose containing
                    // triangle falls outside the legacy resolver's small window.
                    // Use the same bounded coverage search as its identity pose.
                    if config.virtual_layout == "22.2" {
                        openjoc_sofa::resolve_hrir_f32_for_listener_orientation(
                            &loaded.bank,
                            direction,
                        )
                    } else {
                        resolve_hrir_f32(&loaded.bank, direction)
                    }
                    .map_err(Into::into)
                },
            )?
        } else {
            let load_limits = custom_sofa_load_limits.unwrap_or_default();
            let loaded = parse_simple_free_field_hrir(&config.sofa_bytes, load_limits)?;
            lfe_delay_samples = openjoc_sofa::hrir_resampling_delay_samples(
                loaded.bank.sample_rate_hz(),
                BUILTIN_GENERIC_HRTF_SAMPLE_RATE_HZ,
            )?;
            let loaded = resample_loaded_hrir_bank(
                loaded,
                BUILTIN_GENERIC_HRTF_SAMPLE_RATE_HZ,
                load_limits,
            )?;
            prepare_binaural_bank(
                loaded.bank.sample_rate_hz(),
                &config.virtual_layout,
                |direction| resolve_hrir(&loaded.bank, direction).map_err(Into::into),
            )?
        };
        Ok(Self {
            bank: Some(bank),
            sample_rate_hz,
            mappings,
            lfe_index,
            lfe_policy: config.lfe_policy,
            lfe_delay: openjoc_render::SampleDelay::new(lfe_delay_samples),
            engine: None,
            orientation_preparer: None,
            dynamic_engine: None,
            last_orientation_receipt: None,
            drain_started: false,
        })
    }

    #[cfg(test)]
    fn new_with_listener_orientation(
        config: &BinauralConfig,
        external_asset: Option<&[u8]>,
        custom_sofa_load_limits: Option<SofaLoadLimits>,
    ) -> Result<Self, OpenJocError> {
        Self::new_with_listener_orientation_at_epoch(
            config,
            external_asset,
            custom_sofa_load_limits,
            0,
            None,
        )
    }

    fn new_with_listener_orientation_at_epoch(
        config: &BinauralConfig,
        external_asset: Option<&[u8]>,
        custom_sofa_load_limits: Option<SofaLoadLimits>,
        initial_stream_epoch: u64,
        shared_preparer: Option<ListenerOrientationPreparer>,
    ) -> Result<Self, OpenJocError> {
        let preparer = if let Some(preparer) = shared_preparer {
            preparer.validate_config(config)?;
            preparer
        } else {
            ListenerOrientationPreparer::new_with_resource_inputs(
                config,
                external_asset,
                custom_sofa_load_limits,
            )?
        };
        let sample_rate_hz = BUILTIN_GENERIC_HRTF_SAMPLE_RATE_HZ;
        let (lfe_index, lfe_delay_samples) = preparer.lfe_info();
        let initial = preparer
            .prepare(ListenerOrientation::IDENTITY, initial_stream_epoch, 0)
            .map_err(|error| OpenJocError::Render(error.to_string()))?;
        let dynamic_sources = initial
            .kernels()
            .iter()
            .map(|kernel| DynamicBinauralSource::new(kernel.source_id(), 1.0))
            .collect::<Result<Vec<_>, _>>()?;
        let mappings = preparer
            .source_bindings()
            .map(|(channel_index, source_id)| BinauralMapping {
                channel_index,
                source_id,
                hrir_entry: HrirEntryId::new(source_id.get()),
            })
            .collect::<Vec<_>>();
        let dynamic_engine = DynamicBinauralRenderer::new(
            sample_rate_hz,
            initial,
            dynamic_sources,
            preparer.max_filter_taps(),
            MAX_DYNAMIC_BINAURAL_BLOCK_SAMPLES,
            DEFAULT_DYNAMIC_BINAURAL_TRANSITION_SAMPLES,
        )?;
        Ok(Self {
            bank: None,
            sample_rate_hz,
            mappings,
            lfe_index,
            lfe_policy: config.lfe_policy,
            lfe_delay: openjoc_render::SampleDelay::new(lfe_delay_samples),
            engine: None,
            orientation_preparer: Some(preparer),
            dynamic_engine: Some(dynamic_engine),
            last_orientation_receipt: None,
            drain_started: false,
        })
    }

    fn listener_orientation_enabled(&self) -> bool {
        self.dynamic_engine.is_some()
    }

    fn listener_orientation_preparer(&self) -> Option<ListenerOrientationPreparer> {
        self.orientation_preparer.clone()
    }

    fn listener_orientation_stream_epoch(&self) -> Option<u64> {
        self.dynamic_engine
            .as_ref()
            .map(DynamicBinauralRenderer::stream_epoch)
    }

    fn apply_prepared_listener_orientation(
        &mut self,
        update: PreparedBinauralUpdate,
    ) -> Result<BinauralUpdateAcceptance, BinauralUpdateApplyFailure> {
        match self.dynamic_engine.as_mut() {
            Some(engine) => engine.apply_prepared(update),
            None => Err(BinauralUpdateApplyFailure {
                error: openjoc_render::RenderError::BinauralOrientationNotEnabled,
                update,
            }),
        }
    }

    fn last_applied_listener_orientation(&self) -> Option<AppliedBinauralUpdate> {
        self.last_orientation_receipt
    }

    fn pending_listener_orientation_sequence(&self) -> Option<u64> {
        self.dynamic_engine
            .as_ref()
            .and_then(DynamicBinauralRenderer::pending_sequence)
    }

    fn begin_drain(&mut self) -> Result<(), OpenJocError> {
        if let Some(engine) = self.dynamic_engine.as_mut() {
            engine.begin_drain()?;
        }
        self.drain_started = true;
        Ok(())
    }

    fn ensure_engine(&mut self, sample_rate: u32) -> Result<&mut BinauralRenderer, OpenJocError> {
        if self.engine.is_none() {
            if self.sample_rate_hz != sample_rate {
                return Err(OpenJocError::FormatChanged {
                    expected: self.sample_rate_hz,
                    actual: sample_rate,
                });
            }
            let bank = self.bank.as_ref().ok_or_else(|| {
                OpenJocError::Render("prepared HRIR bank is unavailable".to_owned())
            })?;
            let sources = self
                .mappings
                .iter()
                .map(|mapping| {
                    let entry = bank
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
            let bank = self.bank.take().ok_or_else(|| {
                OpenJocError::Render("prepared HRIR bank is unavailable".to_owned())
            })?;
            self.engine = Some(BinauralRenderer::new(sample_rate, bank, sources)?);
        }
        self.engine
            .as_mut()
            .ok_or_else(|| OpenJocError::Render("binaural engine not initialized".to_owned()))
    }

    fn render(&mut self, frame: &RenderedBlock) -> Result<RenderedBlock, OpenJocError> {
        self.render_range(frame, 0, frame.sample_count)
    }

    fn render_range(
        &mut self,
        frame: &RenderedBlock,
        sample_offset: usize,
        sample_count: usize,
    ) -> Result<RenderedBlock, OpenJocError> {
        let sample_rate = frame.sample_rate;
        if sample_rate != self.sample_rate_hz {
            return Err(OpenJocError::FormatChanged {
                expected: self.sample_rate_hz,
                actual: sample_rate,
            });
        }
        let range_end = sample_offset
            .checked_add(sample_count)
            .filter(|&end| end <= frame.sample_count)
            .ok_or_else(|| {
                OpenJocError::Render("binaural input slice is out of range".to_owned())
            })?;
        let logical_start_sample = frame
            .logical_start_sample
            .checked_add(sample_offset as u64)
            .ok_or_else(|| OpenJocError::Render("binaural sample index overflow".to_owned()))?;
        // Only these borrowed descriptors are temporary. Output PCM remains owned
        // by the caller; keep the FIR computation and its accumulation order intact.
        let mut storage =
            [BinauralSourceBlock::new(SourceId::new(0), &[]); openjoc_scene::MAX_CUSTOM_SPEAKERS];
        let blocks = storage.get_mut(..self.mappings.len()).ok_or_else(|| {
            OpenJocError::Render("binaural source count exceeds the layout limit".to_owned())
        })?;
        for (block, mapping) in blocks.iter_mut().zip(&self.mappings) {
            let channel = frame
                .channels
                .get(mapping.channel_index)
                .and_then(|samples| samples.get(sample_offset..range_end))
                .ok_or_else(|| {
                    OpenJocError::Render("binaural input channel is out of range".to_owned())
                })?;
            *block = BinauralSourceBlock::new(mapping.source_id, channel);
        }
        let mut left = vec![0.0; sample_count];
        let mut right = vec![0.0; sample_count];
        let applied_receipt = if let Some(engine) = self.dynamic_engine.as_mut() {
            let before = engine.last_applied_update();
            let logical_count_before = engine.logical_sample_count();
            if self.drain_started {
                engine.render_reconstruction_tail_block(blocks, &mut left, &mut right)?;
            } else {
                engine.render_block(blocks, &mut left, &mut right)?;
            }
            let after = engine.last_applied_update();
            if after == before {
                None
            } else {
                after
                    .map(|receipt| {
                        let relative_start = receipt
                            .logical_start_sample
                            .checked_sub(logical_count_before)
                            .ok_or_else(|| {
                                OpenJocError::Render(
                                    "listener-orientation sample receipt moved backwards"
                                        .to_owned(),
                                )
                            })?;
                        let actual_start = logical_start_sample
                            .checked_add(relative_start)
                            .ok_or_else(|| {
                                OpenJocError::Render(
                                    "listener-orientation sample receipt overflow".to_owned(),
                                )
                            })?;
                        Ok::<AppliedBinauralUpdate, OpenJocError>(AppliedBinauralUpdate {
                            sequence: receipt.sequence,
                            logical_start_sample: actual_start,
                        })
                    })
                    .transpose()?
            }
        } else {
            self.ensure_engine(sample_rate)?
                .render_block(blocks, &mut left, &mut right)?;
            None
        };
        if let Some(receipt) = applied_receipt {
            self.last_orientation_receipt = Some(receipt);
        }
        if self.lfe_policy == BinauralLfePolicy::EqualPowerDualMono {
            if let Some(index) = self.lfe_index {
                if let Some(lfe) = frame
                    .channels
                    .get(index)
                    .and_then(|samples| samples.get(sample_offset..range_end))
                {
                    for ((left_value, right_value), lfe_value) in
                        left.iter_mut().zip(&mut right).zip(lfe)
                    {
                        let lfe_value = self.lfe_delay.process_sample(*lfe_value);
                        *left_value += lfe_value * std::f64::consts::FRAC_1_SQRT_2;
                        *right_value += lfe_value * std::f64::consts::FRAC_1_SQRT_2;
                    }
                }
            }
        }
        Ok(RenderedBlock {
            sample_rate,
            logical_start_sample,
            sample_count,
            channels: vec![left, right],
        })
    }

    fn has_no_tail(&self) -> bool {
        let fir_remaining = self.dynamic_engine.as_ref().map_or_else(
            || {
                self.engine
                    .as_ref()
                    .map_or(0, BinauralRenderer::remaining_tail_samples)
            },
            DynamicBinauralRenderer::remaining_tail_samples,
        );
        fir_remaining == 0 && self.lfe_delay.remaining_samples() == 0
    }

    fn drain_tail_chunk(
        &mut self,
        max_samples: usize,
        logical_start_sample: u64,
    ) -> Result<Option<RenderedBlock>, OpenJocError> {
        let fir_remaining = self.dynamic_engine.as_ref().map_or_else(
            || {
                self.engine
                    .as_ref()
                    .map_or(0, BinauralRenderer::remaining_tail_samples)
            },
            DynamicBinauralRenderer::remaining_tail_samples,
        );
        let lfe_remaining = self.lfe_delay.remaining_samples();
        let remaining = fir_remaining.max(lfe_remaining);
        if remaining == 0 {
            return Ok(None);
        }
        let sample_count = remaining.min(max_samples);
        let mut left = vec![0.0; sample_count];
        let mut right = vec![0.0; sample_count];
        let fir_count = fir_remaining.min(sample_count);
        if fir_count > 0 {
            if let Some(engine) = self.dynamic_engine.as_mut() {
                engine.drain_tail_block(&mut left[..fir_count], &mut right[..fir_count])?;
            } else if let Some(engine) = self.engine.as_mut() {
                engine.drain_tail_block(&mut left[..fir_count], &mut right[..fir_count])?;
            }
        }
        for (left_value, right_value) in left
            .iter_mut()
            .zip(&mut right)
            .take(lfe_remaining.min(sample_count))
        {
            let lfe = self.lfe_delay.drain_sample() * std::f64::consts::FRAC_1_SQRT_2;
            *left_value += lfe;
            *right_value += lfe;
        }
        Ok(Some(RenderedBlock {
            sample_rate: self.sample_rate_hz,
            logical_start_sample,
            sample_count,
            channels: vec![left, right],
        }))
    }

    fn drain_tail(
        &mut self,
        sample_rate: u32,
        start: u64,
    ) -> Result<Vec<RenderedBlock>, OpenJocError> {
        if self.engine.is_none() && self.dynamic_engine.is_none() {
            return Ok(Vec::new());
        }
        let mut output = Vec::new();
        let mut cursor = start;
        loop {
            let fir_remaining = self.dynamic_engine.as_ref().map_or_else(
                || {
                    self.engine
                        .as_ref()
                        .map_or(0, BinauralRenderer::remaining_tail_samples)
                },
                DynamicBinauralRenderer::remaining_tail_samples,
            );
            let remaining = fir_remaining.max(self.lfe_delay.remaining_samples());
            if remaining == 0 {
                break;
            }
            let count = remaining.min(1024);
            let mut left = vec![0.0; count];
            let mut right = vec![0.0; count];
            let fir_count = fir_remaining.min(count);
            if fir_count > 0 {
                if let Some(engine) = self.dynamic_engine.as_mut() {
                    engine.drain_tail_block(&mut left[..fir_count], &mut right[..fir_count])?;
                } else if let Some(engine) = self.engine.as_mut() {
                    engine.drain_tail_block(&mut left[..fir_count], &mut right[..fir_count])?;
                }
            }
            for (left, right) in left
                .iter_mut()
                .zip(&mut right)
                .take(self.lfe_delay.remaining_samples())
            {
                let lfe = self.lfe_delay.drain_sample() * std::f64::consts::FRAC_1_SQRT_2;
                *left += lfe;
                *right += lfe;
            }
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

    fn reset(&mut self) -> Result<(), OpenJocError> {
        self.lfe_delay.reset();
        if let Some(engine) = self.dynamic_engine.as_mut() {
            let preparer = self.orientation_preparer.clone().ok_or_else(|| {
                OpenJocError::Render("listener-orientation preparer is unavailable".to_owned())
            })?;
            let next_epoch = engine
                .stream_epoch()
                .checked_add(1)
                .unwrap_or_else(|| engine.stream_epoch());
            let update = preparer
                .prepare(ListenerOrientation::IDENTITY, next_epoch, 0)
                .map_err(|error| OpenJocError::Render(error.to_string()))?;
            let retired = engine
                .reset_to(update)
                .map_err(|failure| OpenJocError::Render(failure.to_string()))?;
            drop(retired);
            self.last_orientation_receipt = None;
        }
        self.drain_started = false;
        if let Some(engine) = self.engine.as_mut() {
            engine.reset();
        }
        Ok(())
    }
}

fn validate_binaural_sample_rate(sample_rate_hz: u32) -> Result<(), OpenJocError> {
    if sample_rate_hz != BUILTIN_GENERIC_HRTF_SAMPLE_RATE_HZ {
        return Err(OpenJocError::InvalidConfig(format!(
            "binaural SOFA sampling rate must be {BUILTIN_GENERIC_HRTF_SAMPLE_RATE_HZ} Hz, got {sample_rate_hz} Hz"
        )));
    }
    Ok(())
}

fn prepare_binaural_bank(
    sample_rate_hz: u32,
    layout_name: &str,
    mut resolve: impl FnMut(CartesianPosition) -> Result<openjoc_sofa::ResolvedHrir, OpenJocError>,
) -> Result<(HrirBank, Vec<BinauralMapping>, Option<usize>, u32), OpenJocError> {
    let preset = SpeakerLayoutPreset::for_name(layout_name)
        .map_err(|error| OpenJocError::InvalidConfig(error.to_string()))?;
    let mut entries = Vec::with_capacity(preset.labels.len());
    let mut mappings = Vec::new();
    for (channel_index, label) in preset.labels.iter().enumerate() {
        if preset.layout.channels()[channel_index].lfe {
            continue;
        }
        let direction = virtual_speaker_direction(&preset, label).ok_or_else(|| {
            OpenJocError::Unsupported(format!("no binaural direction for {label}"))
        })?;
        let resolved = resolve(direction)?;
        let entry_id = HrirEntryId::new(channel_index as u64 + 1);
        entries.push(HrirEntry::new(entry_id, direction, resolved.pair)?);
        mappings.push(BinauralMapping {
            channel_index,
            source_id: SourceId::new(channel_index as u64 + 1),
            hrir_entry: entry_id,
        });
    }
    let bank = HrirBank::new(sample_rate_hz, entries)?;
    Ok((bank, mappings, preset.lfe_index(), sample_rate_hz))
}

fn virtual_speaker_direction(
    preset: &SpeakerLayoutPreset,
    label: &str,
) -> Option<CartesianPosition> {
    preset
        .virtual_speaker_direction(label)
        .map(|direction| CartesianPosition::new(direction.x, direction.y, direction.z))
}

#[cfg(test)]
mod tests {
    use super::*;
    use openjoc_render::{DynamicBinauralRenderer, DynamicBinauralSource};
    use openjoc_scene::SpeakerGeometry;

    fn binaural_probe_frame(channels: usize, count: usize, start: u64) -> RenderedBlock {
        RenderedBlock {
            sample_rate: 48_000,
            logical_start_sample: start,
            sample_count: count,
            channels: (0..channels)
                .map(|channel| {
                    (0..count)
                        .map(|sample| {
                            ((sample as u64 + start + channel as u64 * 17) % 127) as f64 / 127.0
                                - 0.5
                        })
                        .collect()
                })
                .collect(),
        }
    }

    fn listener_orientation_session_at_epoch_max() -> OpenJocSession {
        let config = OpenJocConfig {
            render_mode: RenderMode::Binaural,
            speaker_layout: "7.1.4".to_owned(),
            binaural: Some(BinauralConfig::builtin_generic("7.1.4")),
            ..OpenJocConfig::default()
        };
        let mut session = OpenJocSession::new_with_listener_orientation(config).unwrap();
        let state = session.binaural.as_mut().unwrap();
        let preparer = state.orientation_preparer.clone().unwrap();
        let initial = preparer
            .prepare(ListenerOrientation::IDENTITY, u64::MAX, 0)
            .unwrap();
        let sources = initial
            .kernels()
            .iter()
            .map(|kernel| DynamicBinauralSource::new(kernel.source_id(), 1.0).unwrap())
            .collect();
        state.dynamic_engine = Some(
            DynamicBinauralRenderer::new(
                48_000,
                initial,
                sources,
                preparer.max_filter_taps(),
                MAX_DYNAMIC_BINAURAL_BLOCK_SAMPLES,
                DEFAULT_DYNAMIC_BINAURAL_TRANSITION_SAMPLES,
            )
            .unwrap(),
        );
        session
    }

    #[test]
    fn binaural_block_adapter_preserves_exact_direct_pcm_tail_and_reset() {
        for hrtf in [BuiltinHrtf::SadieD1Ku100, BuiltinHrtf::SadieD2Kemar] {
            for layout in ["7.1.4", "9.1.6", "22.2"] {
                for lfe_policy in [
                    BinauralLfePolicy::Exclude,
                    BinauralLfePolicy::EqualPowerDualMono,
                ] {
                    let mut config = BinauralConfig::builtin(hrtf, layout);
                    config.lfe_policy = lfe_policy;
                    let mut state = BinauralState::new(&config, None, None).unwrap();
                    let mut reference = state.ensure_engine(48_000).unwrap().clone();
                    let channels = SpeakerLayoutPreset::for_name(layout).unwrap().labels.len();
                    for _ in 0..2 {
                        let mut start = 0;
                        for count in [1, 97, 256, 1536, 17] {
                            let frame = binaural_probe_frame(channels, count, start);
                            let blocks: Vec<_> = state
                                .mappings
                                .iter()
                                .map(|mapping| {
                                    BinauralSourceBlock::new(
                                        mapping.source_id,
                                        &frame.channels[mapping.channel_index],
                                    )
                                })
                                .collect();
                            let mut left = vec![0.0; count];
                            let mut right = vec![0.0; count];
                            reference
                                .render_block(&blocks, &mut left, &mut right)
                                .unwrap();
                            if lfe_policy == BinauralLfePolicy::EqualPowerDualMono {
                                for i in 0..count {
                                    let lfe = frame.channels[state.lfe_index.unwrap()][i]
                                        * std::f64::consts::FRAC_1_SQRT_2;
                                    left[i] += lfe;
                                    right[i] += lfe;
                                }
                            }
                            let actual = state.render(&frame).unwrap();
                            for (actual, expected) in actual
                                .channels
                                .iter()
                                .flatten()
                                .zip(left.iter().chain(&right))
                            {
                                assert_eq!(actual.to_bits(), expected.to_bits());
                            }
                            assert_eq!(actual.logical_start_sample, start);
                            assert_eq!(actual.sample_count, count);
                            start += count as u64;
                        }
                        let count = reference.remaining_tail_samples();
                        let mut left = vec![0.0; count];
                        let mut right = vec![0.0; count];
                        reference.drain_tail_block(&mut left, &mut right).unwrap();
                        let tail = state.drain_tail(48_000, start).unwrap();
                        for (channel, expected) in [left, right].iter().enumerate() {
                            let actual: Vec<_> = tail
                                .iter()
                                .flat_map(|block| block.channels[channel].iter().copied())
                                .collect();
                            assert_eq!(actual.len(), expected.len());
                            for (actual, expected) in actual.iter().zip(expected) {
                                assert_eq!(actual.to_bits(), expected.to_bits());
                            }
                        }
                        state.reset().unwrap();
                        reference.reset();
                    }
                }
            }
        }
    }

    #[test]
    fn dynamic_binaural_range_render_matches_static_across_pull_partitions() {
        for hrtf in [BuiltinHrtf::SadieD1Ku100, BuiltinHrtf::SadieD2Kemar] {
            for layout in ["7.1.4", "9.1.6", "22.2"] {
                for lfe_policy in [
                    BinauralLfePolicy::Exclude,
                    BinauralLfePolicy::EqualPowerDualMono,
                ] {
                    let mut config = BinauralConfig::builtin(hrtf, layout);
                    config.lfe_policy = lfe_policy;
                    let mut static_state = BinauralState::new(&config, None, None).unwrap();
                    let mut dynamic_state =
                        BinauralState::new_with_listener_orientation(&config, None, None).unwrap();
                    let channels = SpeakerLayoutPreset::for_name(layout).unwrap().labels.len();
                    let input = binaural_probe_frame(channels, 1536, 20_000);
                    let reference = static_state.render(&input).unwrap();

                    let mut actual_left = Vec::new();
                    let mut actual_right = Vec::new();
                    let partitions = [1, 17, 97, 128, 256];
                    let mut offset = 0;
                    let mut partition_index = 0;
                    while offset < input.sample_count {
                        let count = partitions[partition_index % partitions.len()]
                            .min(input.sample_count - offset);
                        let actual = dynamic_state.render_range(&input, offset, count).unwrap();
                        assert_eq!(
                            actual.logical_start_sample,
                            input.logical_start_sample + offset as u64
                        );
                        actual_left.extend_from_slice(&actual.channels[0]);
                        actual_right.extend_from_slice(&actual.channels[1]);
                        offset += count;
                        partition_index += 1;
                    }
                    for (actual, expected) in actual_left.iter().zip(&reference.channels[0]) {
                        assert_eq!(actual.to_bits(), expected.to_bits());
                    }
                    for (actual, expected) in actual_right.iter().zip(&reference.channels[1]) {
                        assert_eq!(actual.to_bits(), expected.to_bits());
                    }

                    let static_tail = static_state.drain_tail(48_000, 21_536).unwrap();
                    let mut dynamic_tail = Vec::new();
                    let mut tail_start = 21_536;
                    while let Some(block) = dynamic_state.drain_tail_chunk(128, tail_start).unwrap()
                    {
                        tail_start += block.sample_count as u64;
                        dynamic_tail.push(block);
                    }
                    for channel in 0..2 {
                        let expected: Vec<_> = static_tail
                            .iter()
                            .flat_map(|block| block.channels[channel].iter().copied())
                            .collect();
                        let actual: Vec<_> = dynamic_tail
                            .iter()
                            .flat_map(|block| block.channels[channel].iter().copied())
                            .collect();
                        assert_eq!(actual.len(), expected.len());
                        for (actual, expected) in actual.iter().zip(expected) {
                            assert_eq!(actual.to_bits(), expected.to_bits());
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn listener_orientation_is_explicit_opt_in_and_fingerprints_session_mode() {
        let config = OpenJocConfig {
            render_mode: RenderMode::Binaural,
            speaker_layout: "7.1.4".to_owned(),
            binaural: Some(BinauralConfig::builtin_generic("7.1.4")),
            ..OpenJocConfig::default()
        };
        let static_session = OpenJocSession::new(config.clone()).unwrap();
        assert!(static_session.listener_orientation_preparer().is_none());
        assert_eq!(
            static_session.effective_config_descriptor(),
            config.effective_config_descriptor()
        );
        let enabled = OpenJocSession::new_with_listener_orientation(config.clone()).unwrap();
        assert!(enabled.listener_orientation_preparer().is_some());
        assert_eq!(enabled.listener_orientation_stream_epoch(), Some(0));
        assert!(
            enabled
                .effective_config_descriptor()
                .contains("listener_orientation=enabled")
        );
        assert_ne!(
            enabled.effective_config_fingerprint(),
            static_session.effective_config_fingerprint()
        );

        assert!(matches!(
            OpenJocSession::new_with_listener_orientation_pull(config.clone(), 0),
            Err(OpenJocError::InvalidConfig(_))
        ));
        assert!(matches!(
            OpenJocSession::new_with_listener_orientation_pull(
                config.clone(),
                MAX_LISTENER_ORIENTATION_PULL_SAMPLES + 1
            ),
            Err(OpenJocError::InvalidConfig(_))
        ));
        let mut pull =
            OpenJocSession::new_with_listener_orientation_pull(config.clone(), 128).unwrap();
        assert!(
            pull.effective_config_descriptor()
                .contains("listener_orientation_pull_max_samples=128")
        );
        assert!(pull.receive_frame().is_none());
        assert!(pull.receive_binaural_frame().unwrap().is_none());

        let preparer = enabled.listener_orientation_preparer().unwrap();
        let update = preparer
            .prepare(ListenerOrientation::IDENTITY, 0, 1)
            .unwrap();
        let mut enabled = enabled;
        let receipt = enabled.apply_prepared_listener_orientation(update).unwrap();
        assert_eq!(receipt.accepted_sequence, 1);
        assert_eq!(enabled.drain().unwrap(), OpenJocStatus::EndOfStream);
        let post_drain_update = preparer
            .prepare(ListenerOrientation::IDENTITY, 0, 2)
            .unwrap();
        let failure = enabled
            .apply_prepared_listener_orientation(post_drain_update)
            .unwrap_err();
        assert_eq!(
            failure.error,
            openjoc_render::RenderError::BinauralInputAfterTailStart
        );
        assert_eq!(failure.update.sequence(), 2);
        enabled.try_flush().unwrap();
        assert_eq!(enabled.listener_orientation_stream_epoch(), Some(1));
        assert_eq!(enabled.last_applied_listener_orientation(), None);
        assert!(matches!(
            OpenJocSession::new_with_listener_orientation(OpenJocConfig::default()),
            Err(OpenJocError::InvalidConfig(_))
        ));
    }

    #[test]
    fn failed_orientation_reset_clears_queued_pcm_and_fails_closed() {
        let mut session = listener_orientation_session_at_epoch_max();
        session.output_queue.push_back(OpenJocPcmFrame {
            sample_format: PCM_SAMPLE_FORMAT,
            sample_rate: 48_000,
            channel_count: 2,
            channel_labels: vec!["Left Ear".to_owned(), "Right Ear".to_owned()],
            layout_name: "Binaural stereo".to_owned(),
            render_mode: RenderMode::Binaural,
            sample_count: 1,
            pts_samples: Some(0),
            interleaved_f32: vec![0.0, 0.0],
        });
        let error = session.try_flush().unwrap_err();
        assert!(error.to_string().contains("epoch overflowed"));
        assert!(session.receive_frame().is_none());
        assert!(session.is_drained());
        assert!(session.terminal_error.is_some());

        let preparer = session.listener_orientation_preparer().unwrap();
        let rejected = preparer
            .prepare(ListenerOrientation::IDENTITY, u64::MAX, 1)
            .unwrap();
        let failure = session
            .apply_prepared_listener_orientation(rejected)
            .unwrap_err();
        assert_eq!(
            failure.error,
            openjoc_render::RenderError::BinauralRequiresReset
        );
        assert_eq!(failure.update.sequence(), 1);
    }

    #[test]
    fn pull_failure_clears_deferred_pcm_and_rejects_further_receives() {
        let config = OpenJocConfig {
            render_mode: RenderMode::Binaural,
            speaker_layout: "7.1.4".to_owned(),
            binaural: Some(BinauralConfig::builtin_generic("7.1.4")),
            ..OpenJocConfig::default()
        };
        let mut session = OpenJocSession::new_with_listener_orientation_pull(config, 128).unwrap();
        session
            .pending_binaural_inputs
            .push_back(binaural_probe_frame(12, 4, 0));
        session.pending_binaural_input_samples = 4;
        session.current_binaural_input = Some((binaural_probe_frame(12, 8, 4), 0));
        session.pending_binaural_input_samples += 8;

        let failure = OpenJocError::Render("injected terminal pull failure".to_owned());
        session.fail_closed(&failure);
        assert_eq!(session.pending_binaural_input_samples(), Some(0));
        assert!(session.pending_binaural_inputs.is_empty());
        assert!(session.current_binaural_input.is_none());
        assert!(session.is_drained());
        assert!(
            session
                .receive_binaural_frame()
                .unwrap_err()
                .to_string()
                .contains("injected terminal pull failure")
        );
        assert!(session.receive_frame().is_none());
    }

    #[test]
    fn dynamic_session_identity_pcm_and_applied_receipts_match_static_timeline() {
        let mut config = BinauralConfig::builtin(BuiltinHrtf::SadieD1Ku100, "7.1.4");
        config.lfe_policy = BinauralLfePolicy::EqualPowerDualMono;
        let mut static_state = BinauralState::new(&config, None, None).unwrap();
        let mut dynamic_state =
            BinauralState::new_with_listener_orientation(&config, None, None).unwrap();
        let channels = SpeakerLayoutPreset::for_name("7.1.4").unwrap().labels.len();

        let initial = binaural_probe_frame(channels, 128, 100_000);
        let static_output = static_state.render(&initial).unwrap();
        let dynamic_output = dynamic_state.render(&initial).unwrap();
        for (dynamic, static_sample) in dynamic_output
            .channels
            .iter()
            .flatten()
            .zip(static_output.channels.iter().flatten())
        {
            assert_eq!(dynamic.to_bits(), static_sample.to_bits());
        }
        assert_eq!(
            dynamic_state.last_applied_listener_orientation(),
            Some(AppliedBinauralUpdate {
                sequence: 0,
                logical_start_sample: 100_000,
            })
        );

        let preparer = dynamic_state.listener_orientation_preparer().unwrap();
        let first_angle = 5.0_f64.to_radians();
        let first = preparer
            .prepare(
                ListenerOrientation::new(
                    0.0,
                    0.0,
                    (first_angle * 0.5).sin(),
                    (first_angle * 0.5).cos(),
                )
                .unwrap(),
                0,
                1,
            )
            .unwrap();
        let accepted = dynamic_state
            .apply_prepared_listener_orientation(first)
            .unwrap();
        drop(accepted.retired_kernels);
        let first_rotated = binaural_probe_frame(channels, 128, 100_128);
        let _ = dynamic_state.render(&first_rotated).unwrap();
        assert_eq!(
            dynamic_state.last_applied_listener_orientation(),
            Some(AppliedBinauralUpdate {
                sequence: 1,
                logical_start_sample: 100_128,
            })
        );

        let second_angle = 10.0_f64.to_radians();
        let second = preparer
            .prepare(
                ListenerOrientation::new(
                    0.0,
                    0.0,
                    (second_angle * 0.5).sin(),
                    (second_angle * 0.5).cos(),
                )
                .unwrap(),
                0,
                2,
            )
            .unwrap();
        let accepted = dynamic_state
            .apply_prepared_listener_orientation(second)
            .unwrap();
        drop(accepted.retired_kernels);
        let second_rotated = binaural_probe_frame(channels, 256, 100_256);
        let _ = dynamic_state.render(&second_rotated).unwrap();
        assert_eq!(
            dynamic_state.last_applied_listener_orientation(),
            Some(AppliedBinauralUpdate {
                sequence: 2,
                logical_start_sample: 100_368,
            })
        );

        dynamic_state.reset().unwrap();
        assert_eq!(dynamic_state.listener_orientation_stream_epoch(), Some(1));
        assert_eq!(dynamic_state.last_applied_listener_orientation(), None);
        let stale = preparer
            .prepare(ListenerOrientation::IDENTITY, 0, 3)
            .unwrap();
        let failure = dynamic_state
            .apply_prepared_listener_orientation(stale)
            .unwrap_err();
        assert!(matches!(
            failure.error,
            openjoc_render::RenderError::BinauralUpdateEpochMismatch {
                expected: 1,
                actual: 0,
            }
        ));
        assert_eq!(failure.update.sequence(), 3);
    }

    #[test]
    fn binaural_state_reconstruction_tail_keeps_waiting_pose_pending() {
        let config = BinauralConfig::builtin(BuiltinHrtf::SadieD1Ku100, "7.1.4");
        let mut state = BinauralState::new_with_listener_orientation(&config, None, None).unwrap();
        let preparer = state.listener_orientation_preparer().unwrap();
        let target = preparer
            .prepare(
                ListenerOrientation::new(
                    0.0,
                    0.0,
                    (2.0_f64.to_radians() * 0.5).sin(),
                    (2.0_f64.to_radians() * 0.5).cos(),
                )
                .unwrap(),
                0,
                1,
            )
            .unwrap();
        let accepted = state.apply_prepared_listener_orientation(target).unwrap();
        drop(accepted.retired_kernels);
        assert_eq!(state.pending_listener_orientation_sequence(), Some(1));

        state.begin_drain().unwrap();
        let channels = SpeakerLayoutPreset::for_name("7.1.4").unwrap().labels.len();
        let reconstruction_tail = binaural_probe_frame(channels, 128, 8_192);
        let _ = state.render(&reconstruction_tail).unwrap();
        assert_eq!(state.pending_listener_orientation_sequence(), Some(1));
        assert_eq!(
            state.last_applied_listener_orientation(),
            Some(AppliedBinauralUpdate {
                sequence: 0,
                logical_start_sample: 8_192,
            })
        );
    }

    fn timing_percentile_ns(samples: &mut [u128], percentile: usize) -> u128 {
        samples.sort_unstable();
        let rank = samples.len().saturating_mul(percentile).div_ceil(100);
        samples[rank.saturating_sub(1).min(samples.len() - 1)]
    }

    fn print_timing_distribution(label: &str, samples: &mut [u128]) {
        let total_ns: u128 = samples.iter().sum();
        let average_ns = total_ns / samples.len() as u128;
        let p50_ns = timing_percentile_ns(samples, 50);
        let p95_ns = timing_percentile_ns(samples, 95);
        let p99_ns = timing_percentile_ns(samples, 99);
        let max_ns = *samples.last().unwrap_or(&0);
        eprintln!(
            "{label}: avg={:.3} us p50={:.3} us p95={:.3} us p99={:.3} us max={:.3} us",
            average_ns as f64 / 1_000.0,
            p50_ns as f64 / 1_000.0,
            p95_ns as f64 / 1_000.0,
            p99_ns as f64 / 1_000.0,
            max_ns as f64 / 1_000.0,
        );
    }

    fn print_setup_timing_summary(label: &str, samples: &mut [u128]) {
        samples.sort_unstable();
        let min_ns = samples.first().copied().unwrap_or(0);
        let p50_ns = timing_percentile_ns(samples, 50);
        let max_ns = samples.last().copied().unwrap_or(0);
        eprintln!(
            "{label}: n={} min={:.3} us p50={:.3} us max={:.3} us (descriptive setup sample only)",
            samples.len(),
            min_ns as f64 / 1_000.0,
            p50_ns as f64 / 1_000.0,
            max_ns as f64 / 1_000.0,
        );
    }

    #[test]
    #[ignore = "manual release-mode timing probe; no wall-clock CI threshold"]
    fn binaural_adapter_benchmark() {
        const PREP_RUNS: usize = 7;
        const MEASURED_BLOCKS: usize = 1_024;
        const INDEPENDENT_RUNS: usize = 3;
        const BLOCK_SIZE_OPTIONS: [usize; 2] = [128, 256];

        for hrtf in [BuiltinHrtf::SadieD1Ku100, BuiltinHrtf::SadieD2Kemar] {
            for layout in ["7.1.4", "9.1.6"] {
                let config = BinauralConfig::builtin(hrtf, layout);
                let channels = SpeakerLayoutPreset::for_name(layout).unwrap().labels.len();
                let mut prepare_ns = Vec::with_capacity(PREP_RUNS);
                let mut prepared_payload_bytes = 0;
                for _ in 0..PREP_RUNS {
                    let start = std::time::Instant::now();
                    let state = BinauralState::new(&config, None, None).unwrap();
                    prepare_ns.push(start.elapsed().as_nanos());
                    prepared_payload_bytes = state.bank.as_ref().map_or(0, |bank| {
                        bank.entries()
                            .iter()
                            .map(|entry| {
                                (entry.pair().left_taps().len() + entry.pair().right_taps().len())
                                    * std::mem::size_of::<f64>()
                            })
                            .sum()
                    });
                }
                print_setup_timing_summary(
                    &format!("{hrtf:?} {layout} prepare_binaural_state"),
                    &mut prepare_ns,
                );
                eprintln!(
                    "{hrtf:?} {layout}: selected-HRIR tap payload={prepared_payload_bytes} bytes (excludes transient full f32 bank, loader/allocator overhead, and peak init memory)"
                );

                for block_samples in BLOCK_SIZE_OPTIONS {
                    for run in 0..INDEPENDENT_RUNS {
                        let mut state = BinauralState::new(&config, None, None).unwrap();
                        let first_frame = binaural_probe_frame(channels, block_samples, 0);
                        let first_start = std::time::Instant::now();
                        let _ = state.render(&first_frame).unwrap();
                        let first_render_ns = first_start.elapsed().as_nanos();
                        let renderer = state.engine.as_ref().expect("first render initializes FIR");
                        let kernel_storage_bytes = renderer.hrir_kernel_storage_bytes();
                        let history_storage_bytes = renderer.hrir_history_storage_bytes();
                        let mut block_ns = Vec::with_capacity(MEASURED_BLOCKS);
                        let mut digest = 0xcbf2_9ce4_8422_2325_u64;
                        for block in 0..MEASURED_BLOCKS {
                            let start_sample = ((block + 1) * block_samples) as u64;
                            let frame = binaural_probe_frame(channels, block_samples, start_sample);
                            let start = std::time::Instant::now();
                            let output = std::hint::black_box(
                                state.render(std::hint::black_box(&frame)).unwrap(),
                            );
                            block_ns.push(start.elapsed().as_nanos());
                            for sample in output.channels.iter().flatten() {
                                digest =
                                    (digest ^ sample.to_bits()).wrapping_mul(0x0000_0100_0000_01b3);
                            }
                        }
                        let average_ns: u128 =
                            block_ns.iter().sum::<u128>() / block_ns.len() as u128;
                        let block_period_ns =
                            (block_samples as u128 * 1_000_000_000_u128) / 48_000_u128;
                        let rtf = average_ns as f64 / block_period_ns as f64;
                        let half_period_exceedances = block_ns
                            .iter()
                            .filter(|&&elapsed_ns| elapsed_ns > block_period_ns / 2)
                            .count();
                        let period_exceedances = block_ns
                            .iter()
                            .filter(|&&elapsed_ns| elapsed_ns > block_period_ns)
                            .count();
                        print_timing_distribution(
                            &format!(
                                "{hrtf:?} {layout} {block_samples}-sample run {} render_block",
                                run + 1
                            ),
                            &mut block_ns,
                        );
                        eprintln!(
                            "{hrtf:?} {layout} {block_samples}-sample run {}: first_render={:.3} us block_RTF={rtf:.4} block_period={:.3} us >50%_budget={half_period_exceedances}/{MEASURED_BLOCKS} >period={period_exceedances}/{MEASURED_BLOCKS} (timing exceedances, not device underruns) kernel_storage={kernel_storage_bytes} bytes history_storage={history_storage_bytes} bytes PCM digest={digest:016x}",
                            run + 1,
                            first_render_ns as f64 / 1_000.0,
                            block_period_ns as f64 / 1_000.0,
                        );
                    }
                }
            }
        }
    }

    #[test]
    #[ignore = "manual release-mode comparison of static and dynamic direct FIR; no wall-clock CI threshold"]
    fn dynamic_binaural_vs_static_release_probe() {
        const BLOCKS: usize = 256;
        const ACTIVE_BLOCKS: usize = 64;
        const BLOCK_SIZES: [usize; 2] = [128, 256];
        const FADE_SAMPLES: usize = 240;

        for hrtf in [BuiltinHrtf::SadieD1Ku100, BuiltinHrtf::SadieD2Kemar] {
            for layout in ["7.1.4", "9.1.6"] {
                let config = BinauralConfig::builtin(hrtf, layout);
                let preparer = ListenerOrientationPreparer::new(&config).unwrap();
                let channels = SpeakerLayoutPreset::for_name(layout).unwrap().labels.len();
                let source_count = preparer.source_count();

                let mut prepared_updates = Vec::with_capacity(ACTIVE_BLOCKS);
                let mut prepare_ns = Vec::with_capacity(ACTIVE_BLOCKS);
                for sequence in 1..=ACTIVE_BLOCKS {
                    let angle = sequence as f64 * 0.25_f64.to_radians();
                    let orientation = ListenerOrientation::new(
                        0.0,
                        0.0,
                        (angle * 0.5).sin(),
                        (angle * 0.5).cos(),
                    )
                    .unwrap();
                    let start = std::time::Instant::now();
                    let update = preparer.prepare(orientation, 0, sequence as u64).unwrap();
                    prepare_ns.push(start.elapsed().as_nanos());
                    prepared_updates.push(update);
                }
                print_timing_distribution(
                    &format!("{hrtf:?} {layout} HRIR preparation for active-transition probe"),
                    &mut prepare_ns,
                );

                for block_samples in BLOCK_SIZES {
                    let initial = preparer
                        .prepare(ListenerOrientation::IDENTITY, 0, 0)
                        .unwrap();
                    let dynamic_sources = initial
                        .kernels()
                        .iter()
                        .map(|kernel| DynamicBinauralSource::new(kernel.source_id(), 1.0).unwrap())
                        .collect::<Vec<_>>();
                    assert_eq!(dynamic_sources.len(), source_count);
                    let mut static_state = BinauralState::new(&config, None, None).unwrap();
                    let mappings = static_state.mappings.clone();
                    let static_engine = static_state.ensure_engine(48_000).unwrap();
                    let mut dynamic_engine = DynamicBinauralRenderer::new(
                        48_000,
                        initial,
                        dynamic_sources,
                        preparer.max_filter_taps(),
                        block_samples,
                        FADE_SAMPLES,
                    )
                    .unwrap();
                    let frame = binaural_probe_frame(channels, block_samples, 0);
                    let blocks = mappings
                        .iter()
                        .map(|mapping| {
                            BinauralSourceBlock::new(
                                mapping.source_id,
                                &frame.channels[mapping.channel_index],
                            )
                        })
                        .collect::<Vec<_>>();
                    let mut static_left = vec![0.0; block_samples];
                    let mut static_right = vec![0.0; block_samples];
                    let mut dynamic_left = vec![0.0; block_samples];
                    let mut dynamic_right = vec![0.0; block_samples];

                    static_engine
                        .render_block(&blocks, &mut static_left, &mut static_right)
                        .unwrap();
                    dynamic_engine
                        .render_block(&blocks, &mut dynamic_left, &mut dynamic_right)
                        .unwrap();
                    assert_eq!(
                        static_left, dynamic_left,
                        "identity left PCM {hrtf:?} {layout}"
                    );
                    assert_eq!(
                        static_right, dynamic_right,
                        "identity right PCM {hrtf:?} {layout}"
                    );

                    let mut static_ns = Vec::with_capacity(BLOCKS);
                    let mut dynamic_ns = Vec::with_capacity(BLOCKS);
                    let mut static_digest = 0xcbf2_9ce4_8422_2325_u64;
                    let mut dynamic_digest = 0xcbf2_9ce4_8422_2325_u64;
                    for _ in 0..BLOCKS {
                        let start = std::time::Instant::now();
                        static_engine
                            .render_block(
                                std::hint::black_box(&blocks),
                                std::hint::black_box(&mut static_left),
                                std::hint::black_box(&mut static_right),
                            )
                            .unwrap();
                        static_ns.push(start.elapsed().as_nanos());
                        for sample in static_left.iter().chain(&static_right) {
                            static_digest = (static_digest ^ sample.to_bits())
                                .wrapping_mul(0x0000_0100_0000_01b3);
                        }

                        let start = std::time::Instant::now();
                        dynamic_engine
                            .render_block(
                                std::hint::black_box(&blocks),
                                std::hint::black_box(&mut dynamic_left),
                                std::hint::black_box(&mut dynamic_right),
                            )
                            .unwrap();
                        dynamic_ns.push(start.elapsed().as_nanos());
                        for sample in dynamic_left.iter().chain(&dynamic_right) {
                            dynamic_digest = (dynamic_digest ^ sample.to_bits())
                                .wrapping_mul(0x0000_0100_0000_01b3);
                        }
                    }
                    print_timing_distribution(
                        &format!("{hrtf:?} {layout} {block_samples} direct static FIR"),
                        &mut static_ns,
                    );
                    print_timing_distribution(
                        &format!(
                            "{hrtf:?} {layout} {block_samples} direct dynamic FIR, stable kernel"
                        ),
                        &mut dynamic_ns,
                    );
                    eprintln!(
                        "{hrtf:?} {layout} {block_samples}: sources={source_count} taps_bound={} static_kernel_bytes={} dynamic_kernel_bytes={} dynamic_history_bytes={} static_digest={static_digest:016x} dynamic_digest={dynamic_digest:016x}",
                        preparer.max_filter_taps(),
                        static_engine.hrir_kernel_storage_bytes(),
                        dynamic_engine.hrir_kernel_storage_bytes(),
                        dynamic_engine.hrir_history_storage_bytes(),
                    );

                    let mut active_ns = Vec::with_capacity(ACTIVE_BLOCKS);
                    for update in prepared_updates.iter().cloned() {
                        let accepted = dynamic_engine.apply_prepared(update).unwrap();
                        drop(accepted.retired_kernels);
                        let start = std::time::Instant::now();
                        dynamic_engine
                            .render_block(
                                std::hint::black_box(&blocks),
                                std::hint::black_box(&mut dynamic_left),
                                std::hint::black_box(&mut dynamic_right),
                            )
                            .unwrap();
                        active_ns.push(start.elapsed().as_nanos());
                    }
                    print_timing_distribution(
                        &format!(
                            "{hrtf:?} {layout} {block_samples} direct dynamic FIR, active transitions"
                        ),
                        &mut active_ns,
                    );
                    let block_period_ns =
                        (block_samples as u128 * 1_000_000_000_u128) / 48_000_u128;
                    let over_period = active_ns
                        .iter()
                        .filter(|&&elapsed| elapsed > block_period_ns)
                        .count();
                    eprintln!(
                        "{hrtf:?} {layout} {block_samples} active transitions: >period={over_period}/{ACTIVE_BLOCKS}, block_period={:.3} us (direct FIR only; no decode/device scheduling)",
                        block_period_ns as f64 / 1_000.0
                    );
                }
            }
        }
    }

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
    fn resampled_lfe_delay_survives_short_input_tail_and_reset() {
        let mut config = BinauralConfig::builtin_generic("5.1");
        config.lfe_policy = BinauralLfePolicy::EqualPowerDualMono;
        let mut state = BinauralState::new(&config, None, None).unwrap();
        state.lfe_delay = openjoc_render::SampleDelay::new(33);
        for _ in 0..2 {
            let mut actual = Vec::new();
            for start in [0, 1] {
                let mut channels = vec![vec![0.0]; 6];
                channels[3][0] = 1.0;
                let block = state
                    .render(&RenderedBlock {
                        sample_rate: 48_000,
                        logical_start_sample: start,
                        sample_count: 1,
                        channels,
                    })
                    .unwrap();
                actual.extend(block.channels[0].iter().copied());
            }
            for block in state.drain_tail(48_000, 2).unwrap() {
                assert_eq!(block.channels[0], block.channels[1]);
                actual.extend(block.channels[0].iter().copied());
            }
            assert!(actual.len() >= 35);
            for (index, sample) in actual.into_iter().enumerate() {
                let expected = if index == 33 || index == 34 {
                    std::f64::consts::FRAC_1_SQRT_2
                } else {
                    0.0
                };
                assert!((sample - expected).abs() < 1e-12);
            }
            state.reset().unwrap();
        }
    }

    #[test]
    fn builtin_generic_binaural_is_available_without_sofa_bytes() {
        assert_eq!(
            BinauralConfig::builtin_generic("7.1.4").builtin_hrtf,
            BuiltinHrtf::SadieD1Ku100
        );
        let config = OpenJocConfig {
            render_mode: RenderMode::Binaural,
            speaker_layout: "7.1.4".to_owned(),
            binaural: Some(BinauralConfig::builtin_generic("7.1.4")),
            ..OpenJocConfig::default()
        };
        let session = OpenJocSession::new(config).expect("built-in generic HRTF session");
        assert_eq!(session.latency_samples(), QMF_LATENCY_SAMPLES);
        assert!(!session.speaker.common_profile_stereo_enabled);
        let info = session.output_info();
        assert_eq!(info.layout_name, "Binaural stereo");
        assert_eq!(info.channel_labels, ["Left Ear", "Right Ear"]);
    }

    #[test]
    fn external_builtin_asset_constructs_a_binaural_session() {
        let asset = openjoc_sofa::builtin_hrtf_asset_bytes(BuiltinHrtf::SadieD1Ku100)
            .expect("packaged D1 asset");
        let config = OpenJocConfig {
            render_mode: RenderMode::Binaural,
            speaker_layout: "7.1.4".to_owned(),
            binaural: Some(BinauralConfig::builtin(BuiltinHrtf::SadieD1Ku100, "7.1.4")),
            ..OpenJocConfig::default()
        };

        let session = OpenJocSession::new_with_hrtf_asset(config, asset)
            .expect("externally supplied built-in asset");
        assert_eq!(session.output_info().render_mode, RenderMode::Binaural);
    }

    #[test]
    fn external_builtin_asset_rejects_corruption_before_session_creation() {
        let mut asset = openjoc_sofa::builtin_hrtf_asset_bytes(BuiltinHrtf::SadieD1Ku100)
            .expect("packaged D1 asset")
            .to_vec();
        *asset.last_mut().expect("asset payload") ^= 1;
        let config = OpenJocConfig {
            render_mode: RenderMode::Binaural,
            speaker_layout: "7.1.4".to_owned(),
            binaural: Some(BinauralConfig::builtin(BuiltinHrtf::SadieD1Ku100, "7.1.4")),
            ..OpenJocConfig::default()
        };

        assert!(OpenJocSession::new_with_hrtf_asset(config, &asset).is_err());
    }

    #[test]
    fn non_default_built_in_presets_are_valid_session_configurations() {
        let config = OpenJocConfig {
            render_mode: RenderMode::Binaural,
            speaker_layout: "7.1.4".to_owned(),
            binaural: Some(BinauralConfig::builtin(BuiltinHrtf::SadieD2Kemar, "7.1.4")),
            ..OpenJocConfig::default()
        };
        let session = OpenJocSession::new(config).expect("built-in preset session");
        assert_eq!(
            session.output_info().channel_labels,
            ["Left Ear", "Right Ear"]
        );
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
