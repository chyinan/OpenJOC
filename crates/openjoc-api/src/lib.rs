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
use openjoc_joc::{ParsedJocProof, ReconstructionBasis, ReconstructionOutputTimeline};
use openjoc_oamd::{
    OAMD_PAYLOAD_ID, OamdDecoderConfig, OamdElement, OamdError, OamdParseProfile, OamdPayload,
    ParsedOamdProof,
};
#[cfg(test)]
use openjoc_oamd::{parse_oamd_payload_with_config, parse_oamd_payload_with_profile};
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

mod binaural;
#[cfg(not(target_arch = "wasm32"))]
mod decode_timing;
#[cfg(not(target_arch = "wasm32"))]
pub use decode_timing::OpenJocDecodeStageTiming;
#[cfg(not(target_arch = "wasm32"))]
pub use openjoc_eac3::Eac3DecodeStageTiming;
#[cfg(not(target_arch = "wasm32"))]
pub use openjoc_joc::ReconstructionStageTiming;
pub mod listener_orientation;
#[cfg(not(target_arch = "wasm32"))]
mod render_timing;
mod session;
mod speaker;
pub use listener_orientation::{
    ListenerOrientation, ListenerOrientationError, ListenerOrientationPrepareError,
    ListenerOrientationPreparer, MAX_DYNAMIC_HRIR_TAPS,
};
pub use openjoc_render::{
    AppliedBinauralUpdate, BinauralResourceIdentity, BinauralUpdateAcceptance,
    BinauralUpdateApplyFailure, PreparedBinauralKernel, PreparedBinauralUpdate, RenderError,
};
#[cfg(not(target_arch = "wasm32"))]
pub use render_timing::{OpenJocRenderOperation, OpenJocRenderStageTiming};

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
    /// channel roles and fixed/named route vectors. Custom channel labels use
    /// `count:byte_length:label:byte_length:label...` framing (UTF-8 byte lengths),
    /// so punctuation in labels cannot alias channel boundaries. This replaces
    /// the former comma-separated labels and changes custom-layout fingerprints;
    /// preset descriptors are unchanged.
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
            let labels = layout.channel_labels();
            let _ = write!(descriptor, "\ncustom_layout_channels={}", labels.len());
            for label in labels {
                let _ = write!(descriptor, ":{}:{label}", label.len());
            }
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

// Keep the original category and fields internally, while preserving the
// historical Render wrapper (including its repeated prefix) at the API boundary.
struct TerminalError {
    original: OpenJocError,
}

impl TerminalError {
    fn legacy_error(&self) -> OpenJocError {
        OpenJocError::Render(self.original.to_string())
    }
}

impl fmt::Debug for TerminalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // OpenJocSession derives Debug; keep its former Option<String> output.
        fmt::Debug::fmt(&self.original.to_string(), formatter)
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
    #[cfg(not(target_arch = "wasm32"))]
    decode_timing: decode_timing::DecodeTimingState,
    #[cfg(not(target_arch = "wasm32"))]
    render_timing: render_timing::RenderTimingState,
    // Drain has started (or failed closed); output and FIR tails may remain.
    drained: bool,
    terminal_error: Option<TerminalError>,
}

impl OpenJocSession {
    fn clear_pending_output(&mut self) {
        self.output_queue.clear();
        self.pending_binaural_inputs.clear();
        self.current_binaural_input = None;
        self.pending_binaural_input_samples = 0;
    }

    fn fail_closed(&mut self, error: &OpenJocError) {
        self.clear_pending_output();
        self.drained = true;
        self.terminal_error = Some(TerminalError {
            original: error.clone(),
        });
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

#[cfg(test)]
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
    dialnorm_segments: Vec<openjoc_eac3::DialnormSegment>,
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
    #[cfg(not(target_arch = "wasm32"))]
    render_timing: render_timing::SpeakerTimingState,
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
mod tests;

#[cfg(all(test, feature = "embedded-builtin-hrtf"))]
mod terminal_error_contract;

#[cfg(test)]
mod terminal_error_record_tests;
