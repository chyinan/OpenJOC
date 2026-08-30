//! Reconstructed ADM BWF export for the renderer-independent OpenJOC scene.
//!
//! This crate exports a deterministic reconstructed representation. It emits
//! neutral reconstruction signals outside the admitted decoded-JOC/OAMD
//! profile, and emits scoped dynamic ADM objects only after that profile's
//! structural gate and metadata conversion both succeed. Neither path claims
//! recovery of authored ADM identity.

use openjoc_scene::{
    DecodedJocBindingFacts, DecodedJocBindingProfile, MetadataUpdate, ObjectClass, ObjectScene,
    Position, SemanticBindingState, admit_decoded_joc_binding,
};
mod coordinate;
use coordinate::{AdmCartesianPosition, OamdCartesianPosition};
use serde::Serialize;
use std::fmt::Write as _;
use std::{
    collections::{HashMap, HashSet},
    fmt,
    fs::File,
    io::{self, Read, Seek, SeekFrom, Write},
    path::Path,
};

pub const ADM_SCHEMA: &str = "openjoc.adm-reconstruction.v1";
pub const REPORT_SCHEMA: &str = "openjoc.adm-report.v1";
pub const ADM_BWF_CONTAINER_STANDARD: &str = "RIFF/RF64 WAVE with ITU-R BS.2088 metadata chunks";
pub const BW64_STANDARD: &str = "ITU-R BS.2088-2 (11/2025)";
pub const ADM_STANDARD: &str = "ITU-R BS.2076-3 (02/2025)";

const MAX_AXML_BYTES: u64 = 64 * 1024 * 1024;
const MAX_CHNA_BYTES: u64 = 4 * 1024 * 1024;
const MAX_DBMD_BYTES: u64 = 16 * 1024 * 1024;
// Dolby Atmos Master ADM Profile v1.0 represents object updates as discrete
// events: the first block jumps immediately, while subsequent blocks use the
// profile-defined 250-sample renderer smoothing interval. This is a target
// ADM transport rule, not a copy of the source OAMD ramp duration.
const DOLBY_SUBSEQUENT_JUMP_INTERPOLATION_SAMPLES: u64 = 250;

/// Export policy for semantic losses.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdmPolicy {
    /// Emit the recoverable signals and report every unresolved relationship.
    BestEffort,
    /// Reject when the current scene cannot provide an unambiguous ADM
    /// audio-to-spatial-metadata binding.
    Strict,
}

impl AdmPolicy {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BestEffort => "best-effort",
            Self::Strict => "strict",
        }
    }
}

impl std::str::FromStr for AdmPolicy {
    type Err = AdmError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "best-effort" | "best_effort" => Ok(Self::BestEffort),
            "strict" => Ok(Self::Strict),
            other => Err(AdmError::InvalidPolicy(other.to_owned())),
        }
    }
}

/// Explicit status for each semantic mapping considered by the exporter.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MappingStatus {
    Exact,
    Approximated,
    Dropped,
    NotRepresentable,
    NotRecoverable,
    NotApplicable,
    Unresolved,
}

impl MappingStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Exact => "EXACT",
            Self::Approximated => "APPROXIMATED",
            Self::Dropped => "DROPPED",
            Self::NotRepresentable => "NOT_REPRESENTABLE",
            Self::NotRecoverable => "NOT_RECOVERABLE",
            Self::NotApplicable => "NOT_APPLICABLE",
            Self::Unresolved => "UNRESOLVED",
        }
    }
}

/// One entry in the canonical mapping table used by the writer and report.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MappingRecord {
    pub semantic: &'static str,
    pub status: MappingStatus,
    pub detail: &'static str,
}

/// The complete default mapping table for an unresolved or best-effort export.
#[must_use]
pub const fn mapping_table() -> [MappingRecord; 11] {
    [
        MappingRecord {
            semantic: "reconstruction_signal_identity",
            status: MappingStatus::Exact,
            detail: "The ADM track is identified as a local decoded JOC output-object coordinate; authored identity is not recovered.",
        },
        MappingRecord {
            semantic: "audio_to_spatial_metadata_binding",
            status: MappingStatus::Unresolved,
            detail: "Only the exact admitted carrier-local decoded JOC/OAMD profile supplies this association.",
        },
        MappingRecord {
            semantic: "dynamic_object_position_and_trajectory",
            status: MappingStatus::NotRepresentable,
            detail: "Recovered OAMD position updates are attached only after the scoped decoded-object profile and property gate pass.",
        },
        MappingRecord {
            semantic: "bed_and_direct_speaker_identity_for_reconstruction_rows",
            status: MappingStatus::NotRepresentable,
            detail: "A structural row index is not promoted to an authored bed or speaker identity.",
        },
        MappingRecord {
            semantic: "base_lfe_direct_speaker_identity",
            status: MappingStatus::Exact,
            detail: "A separately retained base LFE channel is emitted in the LFE position of a generated 5.1 bed.",
        },
        MappingRecord {
            semantic: "dolby_atmos_bed_transport_placeholders",
            status: MappingStatus::Approximated,
            detail: "When Base LFE is present, five explicitly reported silent channels complete the minimum allowed 5.1 Dolby Atmos DirectSpeakers bed.",
        },
        MappingRecord {
            semantic: "extent_size_channel_lock_divergence_and_zones",
            status: MappingStatus::NotRepresentable,
            detail: "No direct binding to a reconstructed audio signal is established.",
        },
        MappingRecord {
            semantic: "programme_content_and_authored_names",
            status: MappingStatus::NotRecoverable,
            detail: "Neutral deterministic names are generated; original hierarchy and names are absent.",
        },
        MappingRecord {
            semantic: "pcm_sample_timing",
            status: MappingStatus::Exact,
            detail: "Interleaved PCM duration and track order are derived from the scene sample domain.",
        },
        MappingRecord {
            semantic: "pcm_float_to_signed_24_bit_storage",
            status: MappingStatus::Approximated,
            detail: "Deterministic quantization is used without normalization, limiting, or compression.",
        },
        MappingRecord {
            semantic: "final_linked_gain_hrtf_and_speaker_render",
            status: MappingStatus::NotApplicable,
            detail: "ADM export operates before renderer and binaural stages.",
        },
    ]
}

/// Machine-readable reconstruction report written beside every export.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Serialize)]
pub struct AdmExportReport {
    pub schema: &'static str,
    pub openjoc_version: &'static str,
    pub source_format: &'static str,
    pub adm_bwf_container: &'static str,
    pub dolby_authorship_metadata_state: &'static str,
    pub sample_rate: u32,
    pub duration_samples: u64,
    pub duration_seconds: String,
    pub policy: &'static str,
    pub pcm_format: &'static str,
    pub reconstructed_signal_count: usize,
    pub bed_direct_speaker_count: usize,
    pub generated_silent_bed_placeholder_count: usize,
    pub dynamic_object_count: usize,
    pub metadata_object_count: usize,
    pub dynamic_objects_with_bound_pcm: usize,
    pub mapping: Vec<MappingRecord>,
    pub generated_signal_identities: Vec<String>,
    pub unrecoverable_authoring_information: Vec<&'static str>,
    pub approximations: Vec<&'static str>,
    pub omissions: Vec<&'static str>,
    pub warnings: Vec<String>,
    pub source_is_lossy_e_ac_3_joc: bool,
    pub original_adm_master_recovered: bool,
    pub lossless_round_trip: bool,
    pub semantic_binding_state: &'static str,
    pub decoded_joc_object_binding_state: &'static str,
    pub decoded_joc_binding_profile: Option<&'static str>,
    pub decoded_joc_objects_bound: usize,
    pub decoded_joc_objects_unbound: usize,
    pub dynamic_metadata_exported: bool,
    pub original_authored_identity_recovered: bool,
    pub unsupported_binding_reason: Option<String>,
    pub generated_object_ids: Vec<String>,
    pub pcm_headroom_census: Option<PcmHeadroomCensus>,
}

/// Bounded statistics over decoder-domain f64 PCM immediately before S24
/// quantization. No PCM samples are retained by this structure.
#[derive(Clone, Debug, Serialize)]
pub struct PcmHeadroomCensus {
    pub domain: &'static str,
    pub total_samples: u64,
    pub finite_samples: u64,
    pub non_finite_samples: u64,
    pub out_of_range_samples: u64,
    pub samples_above_one: u64,
    pub samples_below_negative_one: u64,
    pub max_positive: Option<f64>,
    pub min_negative: Option<f64>,
    pub longest_out_of_range_run: u64,
    pub peak_abs: f64,
    pub peak_value: f64,
    pub peak_sample: Option<u64>,
    pub peak_stream: Option<String>,
    pub first_out_of_range: Option<PcmHeadroomLocation>,
    pub base_lfe: Option<PcmHeadroomTrackCensus>,
    pub reconstruction: Vec<PcmHeadroomTrackCensus>,
}

/// Location and value of a first non-representable sample.
#[derive(Clone, Debug, Serialize)]
pub struct PcmHeadroomLocation {
    pub stream: String,
    pub row_index: Option<usize>,
    pub sample: u64,
    pub value: f64,
}

/// Per-signal bounded PCM range statistics.
#[derive(Clone, Debug, Serialize)]
pub struct PcmHeadroomTrackCensus {
    pub stream: String,
    pub row_index: Option<usize>,
    pub total_samples: u64,
    pub finite_samples: u64,
    pub non_finite_samples: u64,
    pub out_of_range_samples: u64,
    pub samples_above_one: u64,
    pub samples_below_negative_one: u64,
    pub max_positive: Option<f64>,
    pub min_negative: Option<f64>,
    pub longest_out_of_range_run: u64,
    pub min_value: Option<f64>,
    pub max_value: Option<f64>,
    pub peak_abs: f64,
    pub peak_sample: Option<u64>,
    pub first_out_of_range: Option<PcmHeadroomLocation>,
    #[serde(skip)]
    current_out_of_range_run: u64,
}

impl PcmHeadroomCensus {
    fn new(reconstruction_count: usize, base_lfe_present: bool) -> Self {
        Self {
            domain: "decoder_pcm_f64_before_s24_quantization",
            total_samples: 0,
            finite_samples: 0,
            non_finite_samples: 0,
            out_of_range_samples: 0,
            samples_above_one: 0,
            samples_below_negative_one: 0,
            max_positive: None,
            min_negative: None,
            longest_out_of_range_run: 0,
            peak_abs: 0.0,
            peak_value: 0.0,
            peak_sample: None,
            peak_stream: None,
            first_out_of_range: None,
            base_lfe: base_lfe_present.then(|| PcmHeadroomTrackCensus::new("base_lfe", None)),
            reconstruction: (0..reconstruction_count)
                .map(|row_index| PcmHeadroomTrackCensus::new("reconstruction", Some(row_index)))
                .collect(),
        }
    }

    fn observe_base_lfe(&mut self, sample: u64, value: f64) {
        let location = self
            .base_lfe
            .as_mut()
            .map(|track| observe_track_values(track, sample, value));
        if let Some(location) = location {
            let longest = self
                .base_lfe
                .as_ref()
                .map_or(0, |track| track.longest_out_of_range_run);
            self.observe_global("base_lfe", None, sample, value, location, longest);
        }
    }

    fn observe_reconstruction(&mut self, row_index: usize, sample: u64, value: f64) {
        let location = self
            .reconstruction
            .get_mut(row_index)
            .map(|track| observe_track_values(track, sample, value));
        if let Some(location) = location {
            let longest = self
                .reconstruction
                .get(row_index)
                .map_or(0, |track| track.longest_out_of_range_run);
            self.observe_global(
                "reconstruction",
                Some(row_index),
                sample,
                value,
                location,
                longest,
            );
        }
    }

    fn observe_global(
        &mut self,
        stream: &str,
        row_index: Option<usize>,
        sample: u64,
        value: f64,
        first_out_of_range: Option<PcmHeadroomLocation>,
        track_longest_out_of_range_run: u64,
    ) {
        self.total_samples = self.total_samples.saturating_add(1);
        if !value.is_finite() {
            self.non_finite_samples = self.non_finite_samples.saturating_add(1);
            return;
        }
        self.finite_samples = self.finite_samples.saturating_add(1);
        if value > 0.0 {
            self.max_positive = Some(self.max_positive.map_or(value, |max| max.max(value)));
        } else if value < 0.0 {
            self.min_negative = Some(self.min_negative.map_or(value, |min| min.min(value)));
        }
        let abs = value.abs();
        if abs > self.peak_abs {
            self.peak_abs = abs;
            self.peak_value = value;
            self.peak_sample = Some(sample);
            self.peak_stream = Some(match row_index {
                Some(row_index) => format!("{stream}[{row_index}]"),
                None => stream.to_owned(),
            });
        }
        if let Some(location) = first_out_of_range {
            self.out_of_range_samples = self.out_of_range_samples.saturating_add(1);
            if value > 1.0 {
                self.samples_above_one = self.samples_above_one.saturating_add(1);
            } else {
                self.samples_below_negative_one = self.samples_below_negative_one.saturating_add(1);
            }
            self.longest_out_of_range_run = self
                .longest_out_of_range_run
                .max(track_longest_out_of_range_run);
            if self.first_out_of_range.is_none() {
                self.first_out_of_range = Some(location);
            }
        }
    }
}

fn observe_track_values(
    track: &mut PcmHeadroomTrackCensus,
    sample: u64,
    value: f64,
) -> Option<PcmHeadroomLocation> {
    track.total_samples = track.total_samples.saturating_add(1);
    if !value.is_finite() {
        track.non_finite_samples = track.non_finite_samples.saturating_add(1);
        track.current_out_of_range_run = 0;
        return None;
    }
    track.finite_samples = track.finite_samples.saturating_add(1);
    if value > 0.0 {
        track.max_positive = Some(track.max_positive.map_or(value, |max| max.max(value)));
    } else if value < 0.0 {
        track.min_negative = Some(track.min_negative.map_or(value, |min| min.min(value)));
    }
    track.min_value = Some(track.min_value.map_or(value, |min| min.min(value)));
    track.max_value = Some(track.max_value.map_or(value, |max| max.max(value)));
    let abs = value.abs();
    if abs > track.peak_abs {
        track.peak_abs = abs;
        track.peak_sample = Some(sample);
    }
    if abs > 1.0 {
        track.out_of_range_samples = track.out_of_range_samples.saturating_add(1);
        if value > 1.0 {
            track.samples_above_one = track.samples_above_one.saturating_add(1);
        } else {
            track.samples_below_negative_one = track.samples_below_negative_one.saturating_add(1);
        }
        track.current_out_of_range_run = track.current_out_of_range_run.saturating_add(1);
        track.longest_out_of_range_run = track
            .longest_out_of_range_run
            .max(track.current_out_of_range_run);
        let location = PcmHeadroomLocation {
            stream: track.stream.clone(),
            row_index: track.row_index,
            sample,
            value,
        };
        if track.first_out_of_range.is_none() {
            track.first_out_of_range = Some(location.clone());
        }
        Some(location)
    } else {
        track.current_out_of_range_run = 0;
        None
    }
}

impl PcmHeadroomTrackCensus {
    fn new(stream: &str, row_index: Option<usize>) -> Self {
        Self {
            stream: stream.to_owned(),
            row_index,
            total_samples: 0,
            finite_samples: 0,
            non_finite_samples: 0,
            out_of_range_samples: 0,
            samples_above_one: 0,
            samples_below_negative_one: 0,
            max_positive: None,
            min_negative: None,
            longest_out_of_range_run: 0,
            min_value: None,
            max_value: None,
            peak_abs: 0.0,
            peak_sample: None,
            first_out_of_range: None,
            current_out_of_range_run: 0,
        }
    }
}

/// Result of building an export in memory.
#[derive(Clone, Debug)]
pub struct AdmExport {
    pub xml: String,
    pub report: AdmExportReport,
}

/// Duration-independent metadata required before a streaming ADM BWF write.
#[derive(Clone, Debug)]
pub struct AdmExportPlan {
    sample_rate: u32,
    duration_samples: u64,
    tracks: Vec<TrackDescriptor>,
    xml: String,
    report: AdmExportReport,
    data_bytes: u64,
    total_size: u64,
    container: AdmContainer,
    reconstruction_signal_count: usize,
    base_lfe_present: bool,
}

/// Interoperable WAVE-family container selected from the planned file size.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdmContainer {
    /// 32-bit RIFF sizes are sufficient.
    Riff,
    /// 64-bit RF64 sizes are required.
    Rf64,
}

impl AdmContainer {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Riff => "RIFF",
            Self::Rf64 => "RF64",
        }
    }
}

/// Deterministic bounded-memory evidence collected by a streaming write.
#[derive(Clone, Debug, Default, Serialize)]
pub struct AdmStreamingStats {
    pub frames_written: u64,
    pub chunks_written: u64,
    pub max_chunk_frames: usize,
    pub max_live_input_samples: usize,
    pub max_interleaved_bytes: usize,
    pub pcm_headroom_census: Option<PcmHeadroomCensus>,
}

/// Incremental signed-24-bit RIFF/RF64 ADM BWF writer.
pub struct StreamingAdmWriter<W: Write + Seek> {
    writer: W,
    plan: AdmExportPlan,
    frames_written: u64,
    interleaved: Vec<u8>,
    stats: AdmStreamingStats,
    pcm_headroom_census: PcmHeadroomCensus,
}

#[derive(Clone, Debug)]
struct TrackDescriptor {
    track_index: u16,
    uid: String,
    channel_id: String,
    pack_id: String,
    stream_id: String,
    track_id: String,
    kind: TrackKind,
    bound_decoded_joc_object: bool,
    dynamic_blocks: Vec<AdmDynamicBlock>,
}

#[derive(Clone, Debug)]
struct AdmDynamicBlock {
    start_sample: u64,
    duration_samples: u64,
    position: AdmCartesianPosition,
}

impl TrackDescriptor {
    fn signal_name(&self) -> String {
        match self.kind {
            TrackKind::Bed(channel) => channel.report_name().to_owned(),
            TrackKind::Reconstruction(index) if self.bound_decoded_joc_object => {
                format!("OpenJOC Reconstructed JOC Object {:02}", index + 1)
            }
            TrackKind::Reconstruction(index) => {
                format!("OpenJOC Reconstructed Signal {:02}", index + 1)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TrackKind {
    Bed(BedChannel),
    Reconstruction(usize),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BedChannel {
    Left,
    Right,
    Center,
    Lfe,
    LeftSurround,
    RightSurround,
}

const BED_5_1_CHANNELS: [BedChannel; 6] = [
    BedChannel::Left,
    BedChannel::Right,
    BedChannel::Center,
    BedChannel::Lfe,
    BedChannel::LeftSurround,
    BedChannel::RightSurround,
];

impl BedChannel {
    const fn channel_name(self) -> &'static str {
        match self {
            Self::Left => "RoomCentricLeft",
            Self::Right => "RoomCentricRight",
            Self::Center => "RoomCentricCenter",
            Self::Lfe => "RoomCentricLFE",
            Self::LeftSurround => "RoomCentricLeftSurround",
            Self::RightSurround => "RoomCentricRightSurround",
        }
    }

    const fn speaker_label(self) -> &'static str {
        match self {
            Self::Left => "RC_L",
            Self::Right => "RC_R",
            Self::Center => "RC_C",
            Self::Lfe => "RC_LFE",
            Self::LeftSurround => "RC_Ls",
            Self::RightSurround => "RC_Rs",
        }
    }

    const fn position(self) -> (i8, i8, i8) {
        match self {
            Self::Left => (-1, 1, 0),
            Self::Right => (1, 1, 0),
            Self::Center => (0, 1, 0),
            Self::Lfe => (-1, 1, -1),
            Self::LeftSurround => (-1, -1, 0),
            Self::RightSurround => (1, -1, 0),
        }
    }

    const fn report_name(self) -> &'static str {
        match self {
            Self::Left => "OpenJOC Generated Silent 5.1 Bed Left Placeholder",
            Self::Right => "OpenJOC Generated Silent 5.1 Bed Right Placeholder",
            Self::Center => "OpenJOC Generated Silent 5.1 Bed Center Placeholder",
            Self::Lfe => "OpenJOC Reconstructed Base LFE 01",
            Self::LeftSurround => "OpenJOC Generated Silent 5.1 Bed Left Surround Placeholder",
            Self::RightSurround => "OpenJOC Generated Silent 5.1 Bed Right Surround Placeholder",
        }
    }
}

/// Validation summary for an ADM BWF file.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AdmValidationSummary {
    pub container: &'static str,
    pub chunks: Vec<String>,
    pub sample_rate: u32,
    pub channels: usize,
    pub data_bytes: u64,
    pub axml_bytes: u64,
    pub dbmd_bytes: u64,
    pub dbmd_segment_ids: Vec<u8>,
    pub dolby_reserved_dbmd_segments_present: bool,
    pub chna_tracks: usize,
    pub chna_uids: usize,
    pub identifiers_unique: bool,
}

/// Export and validation errors.
#[derive(Debug)]
pub enum AdmError {
    InvalidPolicy(String),
    InvalidScene(String),
    StrictUnresolvedBinding,
    UnsupportedDynamicMetadata(String),
    NoReconstructionSignals,
    NonFiniteSample {
        track: usize,
        sample: u64,
    },
    SampleOutOfRange {
        track: usize,
        sample: u64,
        value: f64,
    },
    SizeOverflow,
    Io(io::Error),
    InvalidAdmBwf(&'static str),
}

impl fmt::Display for AdmError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPolicy(value) => write!(formatter, "invalid ADM policy {value:?}"),
            Self::InvalidScene(value) => write!(formatter, "invalid scene for ADM export: {value}"),
            Self::StrictUnresolvedBinding => formatter.write_str(
                "strict ADM export requires a verified audio-to-spatial-metadata binding; current ObjectScene is UNRESOLVED",
            ),
            Self::UnsupportedDynamicMetadata(detail) => {
                write!(formatter, "decoded dynamic ADM metadata is unsupported: {detail}")
            }
            Self::NoReconstructionSignals => formatter.write_str("scene contains no reconstruction signals"),
            Self::NonFiniteSample { track, sample } => write!(formatter, "non-finite ADM PCM at track {track}, sample {sample}"),
            Self::SampleOutOfRange { track, sample, value } => write!(formatter, "ADM signed 24-bit PCM requires [-1, 1], got {value} at track {track}, sample {sample}"),
            Self::SizeOverflow => formatter.write_str("ADM BWF size arithmetic overflow"),
            Self::Io(error) => write!(formatter, "ADM BWF I/O error: {error}"),
            Self::InvalidAdmBwf(detail) => write!(formatter, "invalid ADM BWF: {detail}"),
        }
    }
}

impl std::error::Error for AdmError {}

impl From<io::Error> for AdmError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

/// Builds a deterministic reconstructed export without performing any render.
pub fn build_export(scene: &ObjectScene, policy: AdmPolicy) -> Result<AdmExport, AdmError> {
    scene
        .validate()
        .map_err(|error| AdmError::InvalidScene(error.to_string()))?;
    let basis = scene
        .reconstruction_basis
        .as_ref()
        .ok_or(AdmError::NoReconstructionSignals)?;
    if basis.rows.is_empty() {
        return Err(AdmError::NoReconstructionSignals);
    }
    for (index, samples) in basis.rows.iter().enumerate() {
        validate_samples(index, samples)?;
    }
    if let Some(lfe) = &scene.base_lfe_pcm {
        validate_samples(basis.rows.len(), lfe)?;
    }
    let plan = AdmExportPlan::from_scene(scene, policy)?;
    Ok(AdmExport {
        xml: plan.xml,
        report: plan.report,
    })
}

impl AdmExportPlan {
    /// Builds a bounded plan from metadata known before programme PCM decode.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        sample_rate: u32,
        duration_samples: u64,
        reconstruction_signal_count: usize,
        base_lfe_present: bool,
        dynamic_object_count: usize,
        metadata_object_count: usize,
        semantic_binding: SemanticBindingState,
        policy: AdmPolicy,
    ) -> Result<Self, AdmError> {
        if !matches!(sample_rate, 48_000 | 96_000) {
            return Err(AdmError::InvalidScene(
                "Dolby Atmos ADM BWF sample rate must be 48 kHz or 96 kHz".to_owned(),
            ));
        }
        if reconstruction_signal_count == 0 {
            return Err(AdmError::NoReconstructionSignals);
        }
        if policy == AdmPolicy::Strict && semantic_binding == SemanticBindingState::Unresolved {
            return Err(AdmError::StrictUnresolvedBinding);
        }
        if reconstruction_signal_count > 118 {
            return Err(AdmError::InvalidScene(
                "Dolby Atmos ADM profile supports at most 118 object signals".to_owned(),
            ));
        }
        let bed_channel_count = if base_lfe_present {
            BED_5_1_CHANNELS.len()
        } else {
            0
        };
        let track_count = reconstruction_signal_count
            .checked_add(bed_channel_count)
            .ok_or(AdmError::SizeOverflow)?;
        if track_count > 128 {
            return Err(AdmError::InvalidScene(
                "Dolby Atmos ADM profile supports at most 128 PCM tracks".to_owned(),
            ));
        }
        let channels = u16::try_from(track_count).map_err(|_| AdmError::SizeOverflow)?;
        let _block_align = channels.checked_mul(3).ok_or(AdmError::SizeOverflow)?;
        let _byte_rate = sample_rate
            .checked_mul(u32::from(channels))
            .and_then(|value| value.checked_mul(3))
            .ok_or(AdmError::SizeOverflow)?;
        let data_bytes = duration_samples
            .checked_mul(u64::from(channels))
            .and_then(|value| value.checked_mul(3))
            .ok_or(AdmError::SizeOverflow)?;
        let mut tracks = Vec::with_capacity(track_count);
        if base_lfe_present {
            for (index, bed_channel) in BED_5_1_CHANNELS.iter().copied().enumerate() {
                let signal_number = index.checked_add(1).ok_or(AdmError::SizeOverflow)?;
                let custom_number = 0x1001_usize
                    .checked_add(index)
                    .ok_or(AdmError::SizeOverflow)?;
                let format_id = format!("0001{custom_number:04X}");
                tracks.push(TrackDescriptor {
                    track_index: u16::try_from(signal_number)
                        .map_err(|_| AdmError::SizeOverflow)?,
                    uid: format!("ATU_{signal_number:08X}"),
                    channel_id: format!("AC_{format_id}"),
                    pack_id: "AP_00011001".to_owned(),
                    stream_id: format!("AS_{format_id}"),
                    track_id: format!("AT_{format_id}_01"),
                    kind: TrackKind::Bed(bed_channel),
                    bound_decoded_joc_object: false,
                    dynamic_blocks: Vec::new(),
                });
            }
        }
        for reconstruction_index in 0..reconstruction_signal_count {
            let track_offset = tracks.len();
            let signal_number = track_offset.checked_add(1).ok_or(AdmError::SizeOverflow)?;
            let format_number = 0x1001_usize
                .checked_add(track_offset)
                .ok_or(AdmError::SizeOverflow)?;
            let pack_number = 0x1001_usize
                .checked_add(reconstruction_index)
                .and_then(|number| number.checked_add(usize::from(base_lfe_present)))
                .ok_or(AdmError::SizeOverflow)?;
            let format_id = format!("0003{format_number:04X}");
            tracks.push(TrackDescriptor {
                track_index: u16::try_from(signal_number).map_err(|_| AdmError::SizeOverflow)?,
                uid: format!("ATU_{signal_number:08X}"),
                channel_id: format!("AC_{format_id}"),
                pack_id: format!("AP_0003{pack_number:04X}"),
                stream_id: format!("AS_{format_id}"),
                track_id: format!("AT_{format_id}_01"),
                kind: TrackKind::Reconstruction(reconstruction_index),
                bound_decoded_joc_object: false,
                dynamic_blocks: Vec::new(),
            });
        }
        let xml = make_xml(sample_rate, duration_samples, &tracks);
        let axml_len = u64::try_from(xml.len()).map_err(|_| AdmError::SizeOverflow)?;
        let chna_len =
            u64::try_from(chna_payload(&tracks)?.len()).map_err(|_| AdmError::SizeOverflow)?;
        let dbmd_len = u64::try_from(dbmd_payload().len()).map_err(|_| AdmError::SizeOverflow)?;
        let (container, total_size) = adm_bwf_layout(axml_len, chna_len, dbmd_len, data_bytes)?;
        let report = make_report(
            sample_rate,
            duration_samples,
            dynamic_object_count,
            metadata_object_count,
            policy,
            &tracks,
            reconstruction_signal_count,
            base_lfe_present,
            container,
            semantic_binding,
        );
        Ok(Self {
            sample_rate,
            duration_samples,
            tracks,
            xml,
            report,
            data_bytes,
            total_size,
            container,
            reconstruction_signal_count,
            base_lfe_present,
        })
    }

    /// Builds a plan for an explicit, already-materialized diagnostic scene.
    pub fn from_scene(scene: &ObjectScene, policy: AdmPolicy) -> Result<Self, AdmError> {
        scene
            .validate()
            .map_err(|error| AdmError::InvalidScene(error.to_string()))?;
        let basis = scene
            .reconstruction_basis
            .as_ref()
            .ok_or(AdmError::NoReconstructionSignals)?;
        let reconstruction_signal_count = basis.rows.len();
        let mut plan = Self::new(
            scene.sample_rate,
            scene.duration_samples,
            reconstruction_signal_count,
            scene.base_lfe_pcm.is_some(),
            scene
                .objects
                .iter()
                .filter(|object| object.class == ObjectClass::Dynamic)
                .count(),
            scene.objects.len(),
            scene.semantic_binding,
            policy,
        )?;
        if scene.semantic_binding == SemanticBindingState::ResolvedWithinCarrier {
            let classes = scene
                .objects
                .iter()
                .map(|object| object.class)
                .collect::<Vec<_>>();
            let facts =
                DecodedJocBindingFacts::from_scene_classes(reconstruction_signal_count, &classes);
            let profile = admit_decoded_joc_binding(&facts)
                .map_err(|error| AdmError::InvalidScene(error.to_string()))?;
            profile
                .bind_scene_objects(basis, &scene.metadata_timeline)
                .map_err(|error| AdmError::InvalidScene(error.to_string()))?;
            if let Err(error) =
                plan.apply_decoded_joc_binding_metadata(&scene.metadata_timeline, &profile)
            {
                if policy == AdmPolicy::Strict {
                    return Err(error);
                }
                plan.set_decoded_binding_unavailable(error.to_string())?;
            }
        }
        Ok(plan)
    }

    /// Attaches only the admitted decoded-JOC/OAMD metadata relation to the
    /// already planned PCM tracks. Metadata is copied, but decoded PCM is not
    /// retained a second time.
    pub fn apply_decoded_joc_binding_metadata(
        &mut self,
        metadata_timeline: &[MetadataUpdate],
        profile: &DecodedJocBindingProfile,
    ) -> Result<(), AdmError> {
        self.report.decoded_joc_binding_profile = Some(profile.profile_name());
        if profile.joc_object_count() != self.reconstruction_signal_count
            || self.dynamic_object_count() != profile.joc_object_count()
        {
            return Err(AdmError::UnsupportedDynamicMetadata(
                "decoded JOC/OAMD object population differs from the ADM plan".to_owned(),
            ));
        }
        let bound_objects = profile
            .bind_decoded_objects()
            .map_err(|error| AdmError::UnsupportedDynamicMetadata(error.to_string()))?;
        let mut dynamic_blocks = Vec::with_capacity(bound_objects.len());
        for bound in &bound_objects {
            let updates = metadata_timeline
                .iter()
                .filter(|update| {
                    update.object_id == u32::try_from(bound.oamd_total_index.0).unwrap_or(u32::MAX)
                })
                .collect::<Vec<_>>();
            if updates.is_empty() {
                return Err(AdmError::UnsupportedDynamicMetadata(format!(
                    "no OAMD metadata updates for admitted dynamic ordinal {}",
                    bound.oamd_dynamic_ordinal.0
                )));
            }
            if updates.iter().any(|update| !update.active) {
                return Err(AdmError::UnsupportedDynamicMetadata(format!(
                    "inactive transition is not admitted for dynamic ordinal {}",
                    bound.oamd_dynamic_ordinal.0
                )));
            }
            let mut blocks = Vec::with_capacity(updates.len());
            for (index, update) in updates.iter().enumerate() {
                let next_start = updates
                    .get(index + 1)
                    .map_or(self.duration_samples, |next| next.start_sample);
                if update.start_sample >= next_start || next_start > self.duration_samples {
                    return Err(AdmError::UnsupportedDynamicMetadata(format!(
                        "non-contiguous OAMD timing for dynamic ordinal {}",
                        bound.oamd_dynamic_ordinal.0
                    )));
                }
                let position = position_for_adm(&update.position)?;
                blocks.push(AdmDynamicBlock {
                    start_sample: update.start_sample,
                    duration_samples: next_start - update.start_sample,
                    position,
                });
            }
            if blocks[0].start_sample != 0
                || blocks.last().is_none_or(|block| {
                    block.start_sample + block.duration_samples != self.duration_samples
                })
            {
                return Err(AdmError::UnsupportedDynamicMetadata(format!(
                    "dynamic ordinal {} does not cover the complete programme",
                    bound.oamd_dynamic_ordinal.0
                )));
            }
            dynamic_blocks.push(blocks);
        }

        for (index, blocks) in dynamic_blocks.into_iter().enumerate() {
            let track = self
                .tracks
                .iter_mut()
                .find(|track| track.kind == TrackKind::Reconstruction(index))
                .ok_or_else(|| {
                    AdmError::UnsupportedDynamicMetadata(format!(
                        "ADM reconstruction track {index} is missing"
                    ))
                })?;
            track.bound_decoded_joc_object = true;
            track.dynamic_blocks = blocks;
        }
        self.report.mapping = bound_mapping_table();
        self.report.dynamic_objects_with_bound_pcm = self.reconstruction_signal_count;
        self.report.decoded_joc_objects_bound = self.reconstruction_signal_count;
        self.report.decoded_joc_objects_unbound = 0;
        self.report.dynamic_metadata_exported = true;
        self.report.unsupported_binding_reason = None;
        self.report.semantic_binding_state = "resolved_within_carrier";
        self.report.decoded_joc_object_binding_state = "resolved_within_carrier";
        self.report.decoded_joc_binding_profile = Some(profile.profile_name());
        self.report.generated_signal_identities = self
            .tracks
            .iter()
            .map(TrackDescriptor::signal_name)
            .collect();
        self.report.generated_object_ids = generated_object_ids(&self.tracks);
        self.xml = make_xml(self.sample_rate, self.duration_samples, &self.tracks);
        self.refresh_serialized_layout()
    }

    /// Keeps best-effort export neutral while making the binding failure
    /// explicit in the adjacent machine-readable report.
    pub fn set_decoded_binding_unavailable(
        &mut self,
        reason: impl Into<String>,
    ) -> Result<(), AdmError> {
        let reason = reason.into();
        for track in &mut self.tracks {
            track.bound_decoded_joc_object = false;
            track.dynamic_blocks.clear();
        }
        self.report.dynamic_objects_with_bound_pcm = 0;
        self.report.decoded_joc_objects_bound = 0;
        self.report.decoded_joc_objects_unbound = self.dynamic_object_count();
        self.report.dynamic_metadata_exported = false;
        self.report.unsupported_binding_reason = Some(reason);
        if let Some(reason) = self.report.unsupported_binding_reason.as_ref() {
            self.report.warnings.push(format!(
                "Decoded JOC/OAMD dynamic export unavailable: {reason}"
            ));
        }
        self.report.mapping = mapping_table().to_vec();
        self.report.generated_signal_identities = self
            .tracks
            .iter()
            .map(TrackDescriptor::signal_name)
            .collect();
        self.report.generated_object_ids = generated_object_ids(&self.tracks);
        self.xml = make_xml(self.sample_rate, self.duration_samples, &self.tracks);
        self.refresh_serialized_layout()
    }

    fn dynamic_object_count(&self) -> usize {
        self.tracks
            .iter()
            .filter(|track| matches!(track.kind, TrackKind::Reconstruction(_)))
            .count()
    }

    fn refresh_serialized_layout(&mut self) -> Result<(), AdmError> {
        let axml_len = u64::try_from(self.xml.len()).map_err(|_| AdmError::SizeOverflow)?;
        let chna_len =
            u64::try_from(chna_payload(&self.tracks)?.len()).map_err(|_| AdmError::SizeOverflow)?;
        let dbmd_len = u64::try_from(dbmd_payload().len()).map_err(|_| AdmError::SizeOverflow)?;
        let (container, total_size) =
            adm_bwf_layout(axml_len, chna_len, dbmd_len, self.data_bytes)?;
        self.container = container;
        self.total_size = total_size;
        self.report.adm_bwf_container = container.as_str();
        Ok(())
    }

    #[must_use]
    pub const fn expected_data_bytes(&self) -> u64 {
        self.data_bytes
    }

    #[must_use]
    pub const fn duration_samples(&self) -> u64 {
        self.duration_samples
    }

    #[must_use]
    pub fn track_count(&self) -> usize {
        self.tracks.len()
    }

    #[must_use]
    pub const fn container(&self) -> AdmContainer {
        self.container
    }
}

impl<W: Write + Seek> StreamingAdmWriter<W> {
    /// Opens an ADM BWF stream and writes its container/fmt/data prefix.
    pub fn new(mut writer: W, plan: AdmExportPlan) -> Result<Self, AdmError> {
        write_adm_bwf_header(&mut writer, &plan)?;
        let pcm_headroom_census =
            PcmHeadroomCensus::new(plan.reconstruction_signal_count, plan.base_lfe_present);
        Ok(Self {
            writer,
            plan,
            frames_written: 0,
            interleaved: Vec::new(),
            stats: AdmStreamingStats::default(),
            pcm_headroom_census,
        })
    }

    /// Quantizes and interleaves one bounded decoder chunk.
    pub fn write_pcm(
        &mut self,
        reconstruction_rows: &[Vec<f64>],
        base_lfe: Option<&[f64]>,
    ) -> Result<(), AdmError> {
        if reconstruction_rows.len() != self.plan.reconstruction_signal_count
            || base_lfe.is_some() != self.plan.base_lfe_present
        {
            return Err(AdmError::InvalidScene(
                "streaming ADM PCM topology differs from its preflight plan".to_owned(),
            ));
        }
        let chunk_frames = reconstruction_rows
            .first()
            .map_or_else(|| base_lfe.map_or(0, <[f64]>::len), Vec::len);
        if reconstruction_rows
            .iter()
            .any(|row| row.len() != chunk_frames)
            || base_lfe.is_some_and(|lfe| lfe.len() != chunk_frames)
        {
            return Err(AdmError::InvalidScene(
                "streaming ADM tracks have different chunk durations".to_owned(),
            ));
        }
        let chunk_frames_u64 = u64::try_from(chunk_frames).map_err(|_| AdmError::SizeOverflow)?;
        let next_frames = self
            .frames_written
            .checked_add(chunk_frames_u64)
            .ok_or(AdmError::SizeOverflow)?;
        if next_frames > self.plan.duration_samples {
            return Err(AdmError::InvalidScene(
                "streaming ADM PCM exceeds the preflight duration".to_owned(),
            ));
        }
        let required_bytes = chunk_frames
            .checked_mul(self.plan.tracks.len())
            .and_then(|value| value.checked_mul(3))
            .ok_or(AdmError::SizeOverflow)?;
        self.interleaved.clear();
        self.interleaved.reserve(required_bytes);
        for frame in 0..chunk_frames {
            if let Some(lfe) = base_lfe {
                self.interleaved.extend_from_slice(&[0_u8; 9]);
                let sample = self
                    .frames_written
                    .checked_add(u64::try_from(frame).map_err(|_| AdmError::SizeOverflow)?)
                    .ok_or(AdmError::SizeOverflow)?;
                self.pcm_headroom_census
                    .observe_base_lfe(sample, lfe[frame]);
                self.interleaved.extend_from_slice(&quantize_s24(
                    reconstruction_rows.len(),
                    sample,
                    lfe[frame],
                )?);
                self.interleaved.extend_from_slice(&[0_u8; 6]);
            }
            for (track, row) in reconstruction_rows.iter().enumerate() {
                let sample = self
                    .frames_written
                    .checked_add(u64::try_from(frame).map_err(|_| AdmError::SizeOverflow)?)
                    .ok_or(AdmError::SizeOverflow)?;
                self.pcm_headroom_census
                    .observe_reconstruction(track, sample, row[frame]);
                self.interleaved
                    .extend_from_slice(&quantize_s24(track, sample, row[frame])?);
            }
        }
        self.writer.write_all(&self.interleaved)?;
        self.frames_written = next_frames;
        self.stats.frames_written = next_frames;
        self.stats.chunks_written = self
            .stats
            .chunks_written
            .checked_add(1)
            .ok_or(AdmError::SizeOverflow)?;
        self.stats.max_chunk_frames = self.stats.max_chunk_frames.max(chunk_frames);
        self.stats.max_live_input_samples = self.stats.max_live_input_samples.max(
            chunk_frames
                .checked_mul(
                    self.plan
                        .reconstruction_signal_count
                        .checked_add(usize::from(self.plan.base_lfe_present))
                        .ok_or(AdmError::SizeOverflow)?,
                )
                .ok_or(AdmError::SizeOverflow)?,
        );
        self.stats.max_interleaved_bytes =
            self.stats.max_interleaved_bytes.max(self.interleaved.len());
        Ok(())
    }

    /// Verifies exact duration, pads PCM, writes ADM metadata, and flushes.
    pub fn finish(mut self) -> Result<(W, AdmExportReport, AdmStreamingStats), AdmError> {
        if self.plan.report.policy == AdmPolicy::Strict.as_str()
            && self.plan.report.decoded_joc_object_binding_state == "resolved_within_carrier"
            && !self.plan.report.dynamic_metadata_exported
        {
            return Err(AdmError::UnsupportedDynamicMetadata(
                "strict ADM export requires dynamic metadata for every admitted decoded JOC object"
                    .to_owned(),
            ));
        }
        if self.frames_written != self.plan.duration_samples {
            return Err(AdmError::InvalidScene(format!(
                "streaming ADM wrote {} samples per track; preflight requires {}",
                self.frames_written, self.plan.duration_samples
            )));
        }
        if self.plan.data_bytes % 2 != 0 {
            self.writer.write_all(&[0])?;
        }
        write_adm_metadata(&mut self.writer, &self.plan)?;
        let actual_size = self.writer.stream_position()?;
        if actual_size != self.plan.total_size {
            return Err(AdmError::InvalidScene(format!(
                "streaming ADM BWF size {actual_size} differs from planned size {}",
                self.plan.total_size
            )));
        }
        self.writer.flush()?;
        self.plan.report.pcm_headroom_census = Some(self.pcm_headroom_census.clone());
        self.stats.pcm_headroom_census = Some(self.pcm_headroom_census);
        Ok((self.writer, self.plan.report, self.stats))
    }
}

/// Writes a complete interoperable ADM BWF and returns its deterministic report.
pub fn write_adm_bwf(
    path: &Path,
    scene: &ObjectScene,
    policy: AdmPolicy,
) -> Result<AdmExportReport, AdmError> {
    let plan = AdmExportPlan::from_scene(scene, policy)?;
    let basis = scene
        .reconstruction_basis
        .as_ref()
        .ok_or(AdmError::NoReconstructionSignals)?;
    let file = File::create(path)?;
    let mut writer = StreamingAdmWriter::new(file, plan)?;
    writer.write_pcm(&basis.rows, scene.base_lfe_pcm.as_deref())?;
    let (_, report, _) = writer.finish()?;
    Ok(report)
}

/// Backward-compatible name for [`write_adm_bwf`].
pub fn write_bw64(
    path: &Path,
    scene: &ObjectScene,
    policy: AdmPolicy,
) -> Result<AdmExportReport, AdmError> {
    write_adm_bwf(path, scene, policy)
}

/// Validates supported RIFF/RF64/BW64 ADM BWF structure and relationships.
pub fn validate_adm_bwf(path: &Path) -> Result<AdmValidationSummary, AdmError> {
    let mut file = File::open(path)?;
    validate_reader(&mut file)
}

/// Backward-compatible name for [`validate_adm_bwf`].
pub fn validate_bw64(path: &Path) -> Result<AdmValidationSummary, AdmError> {
    validate_adm_bwf(path)
}

fn validate_samples(track: usize, samples: &[f64]) -> Result<(), AdmError> {
    for (sample, value) in samples.iter().copied().enumerate() {
        let sample = u64::try_from(sample).map_err(|_| AdmError::SizeOverflow)?;
        if !value.is_finite() {
            return Err(AdmError::NonFiniteSample { track, sample });
        }
        if !(-1.0..=1.0).contains(&value) {
            return Err(AdmError::SampleOutOfRange {
                track,
                sample,
                value,
            });
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn make_report(
    sample_rate: u32,
    duration_samples: u64,
    dynamic_object_count: usize,
    metadata_object_count: usize,
    policy: AdmPolicy,
    tracks: &[TrackDescriptor],
    reconstruction_signal_count: usize,
    base_lfe_present: bool,
    container: AdmContainer,
    semantic_binding: SemanticBindingState,
) -> AdmExportReport {
    let bed_direct_speaker_count = tracks
        .iter()
        .filter(|track| matches!(track.kind, TrackKind::Bed(_)))
        .count();
    let generated_signal_identities = tracks
        .iter()
        .map(|track| match track.kind {
            TrackKind::Bed(channel) => channel.report_name().to_owned(),
            TrackKind::Reconstruction(index) => {
                format!("OpenJOC Reconstructed Signal {:02}", index + 1)
            }
        })
        .collect();
    let mut approximations = vec!["float reconstruction samples quantized to signed 24-bit PCM"];
    let mut warnings = vec![
        "This is a reconstructed interoperability representation, not the original ADM master."
            .to_owned(),
    ];
    if base_lfe_present {
        approximations.push(
            "five generated silent channels complete the Dolby Atmos profile's minimum allowed bed containing LFE",
        );
        warnings.push(
            "The generated 5.1 bed is a transport structure: only its LFE channel carries recovered PCM; L, R, C, Ls, and Rs are explicit silence placeholders."
                .to_owned(),
        );
    }
    AdmExportReport {
        schema: REPORT_SCHEMA,
        openjoc_version: env!("CARGO_PKG_VERSION"),
        source_format: "lossy E-AC-3 JOC",
        adm_bwf_container: container.as_str(),
        dolby_authorship_metadata_state: "not-generated",
        sample_rate,
        duration_samples,
        duration_seconds: format_time(duration_samples, sample_rate),
        policy: policy.as_str(),
        pcm_format: "signed 24-bit little-endian PCM; no normalization or dynamics processing",
        reconstructed_signal_count: reconstruction_signal_count,
        bed_direct_speaker_count,
        generated_silent_bed_placeholder_count: usize::from(base_lfe_present) * 5,
        dynamic_object_count,
        metadata_object_count,
        dynamic_objects_with_bound_pcm: 0,
        mapping: mapping_table().to_vec(),
        generated_signal_identities,
        unrecoverable_authoring_information: vec![
            "original ADM programme/content hierarchy",
            "original object names and comments",
            "original UID and track assignments",
            "pre-encoding PCM and discarded encoder information",
        ],
        approximations,
        omissions: vec![
            "recovered OAMD position/trajectory is not attached when dynamic metadata export is unavailable",
            "extent, channel lock, divergence, zones, and JOC-specific controls are not represented in ADM",
            "FinalLinkedGain, speaker rendering, and HRTF are not applied",
        ],
        warnings,
        source_is_lossy_e_ac_3_joc: true,
        original_adm_master_recovered: false,
        lossless_round_trip: false,
        semantic_binding_state: semantic_binding_report_name(semantic_binding),
        decoded_joc_object_binding_state: semantic_binding_report_name(semantic_binding),
        decoded_joc_binding_profile: None,
        decoded_joc_objects_bound: 0,
        decoded_joc_objects_unbound: dynamic_object_count,
        dynamic_metadata_exported: false,
        original_authored_identity_recovered: false,
        unsupported_binding_reason: (semantic_binding == SemanticBindingState::Unresolved)
            .then(|| "decoded JOC/OAMD binding is unavailable for this profile".to_owned()),
        generated_object_ids: generated_object_ids(tracks),
        pcm_headroom_census: None,
    }
}

fn semantic_binding_report_name(state: SemanticBindingState) -> &'static str {
    match state {
        SemanticBindingState::Unresolved => "unresolved",
        SemanticBindingState::ResolvedWithinCarrier => "resolved_within_carrier",
    }
}

fn generated_object_ids(tracks: &[TrackDescriptor]) -> Vec<String> {
    tracks
        .iter()
        .filter(|track| matches!(track.kind, TrackKind::Reconstruction(_)))
        .map(|track| {
            let TrackKind::Reconstruction(index) = track.kind else {
                unreachable!();
            };
            format!("AO_{:04X}", 0x100B + index)
        })
        .collect()
}

fn position_for_adm(position: &Position) -> Result<AdmCartesianPosition, AdmError> {
    let oamd_position = match position {
        Position::Room(position) => *position,
        Position::RoomAtInfinity {
            boundary_intersection,
        } => *boundary_intersection,
        Position::Screen {
            interpolated_room, ..
        } => *interpolated_room,
        Position::Speaker(_) | Position::IntermediateSpatial(_) => {
            return Err(AdmError::UnsupportedDynamicMetadata(
                "dynamic ADM export accepts only room-coordinate position updates".to_owned(),
            ));
        }
    };
    let oamd_position = OamdCartesianPosition::try_from(oamd_position).map_err(|error| {
        AdmError::UnsupportedDynamicMetadata(format!("invalid OAMD position: {error}"))
    })?;
    AdmCartesianPosition::try_from(oamd_position).map_err(|error| {
        AdmError::UnsupportedDynamicMetadata(format!("invalid ADM position: {error}"))
    })
}

fn bound_mapping_table() -> Vec<MappingRecord> {
    let mut mapping = mapping_table().to_vec();
    if let Some(record) = mapping
        .iter_mut()
        .find(|record| record.semantic == "audio_to_spatial_metadata_binding")
    {
        record.status = MappingStatus::Exact;
        record.detail = "Within the admitted carrier, typed decoded JOC ordinal j maps to reconstruction row j, OAMD dynamic ordinal j, and OAMD total index j+1.";
    }
    if let Some(record) = mapping
        .iter_mut()
        .find(|record| record.semantic == "dynamic_object_position_and_trajectory")
    {
        record.status = MappingStatus::Exact;
        record.detail = "OAMD position updates are validated in the admitted room domain and converted once to normalized ADM Cartesian coordinates at each decoded sample-domain event boundary.";
    }
    mapping
}

fn make_xml(sample_rate: u32, duration_samples: u64, tracks: &[TrackDescriptor]) -> String {
    let duration = format_adm_time(duration_samples, sample_rate);
    let bed_tracks: Vec<_> = tracks
        .iter()
        .filter(|track| matches!(track.kind, TrackKind::Bed(_)))
        .collect();
    let object_tracks: Vec<_> = tracks
        .iter()
        .filter(|track| matches!(track.kind, TrackKind::Reconstruction(_)))
        .collect();
    let mut xml = String::new();
    xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    xml.push_str("<ebuCoreMain xmlns=\"urn:ebu:metadata-schema:ebuCore_2016\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xsi:schemaLocation=\"urn:ebu:metadata-schema:ebuCore_2016 ebucore.xsd\" xml:lang=\"en\">\n");
    xml.push_str("  <coreMetadata>\n    <format>\n      <audioFormatExtended>\n");
    let _ = writeln!(
        xml,
        "        <audioProgramme audioProgrammeID=\"APR_1001\" audioProgrammeName=\"OpenJOC Reconstructed Programme (not original ADM master)\" start=\"00:00:00.00000\" end=\"{duration}\">"
    );
    xml.push_str(
        "          <audioContentIDRef>ACO_1001</audioContentIDRef>\n        </audioProgramme>\n",
    );
    xml.push_str("        <audioContent audioContentID=\"ACO_1001\" audioContentName=\"OpenJOC Reconstructed Interoperability Representation\">\n");
    if !bed_tracks.is_empty() {
        xml.push_str("          <audioObjectIDRef>AO_1001</audioObjectIDRef>\n");
    }
    for (index, _) in object_tracks.iter().enumerate() {
        let _ = writeln!(
            xml,
            "          <audioObjectIDRef>AO_{:04X}</audioObjectIDRef>",
            0x100B + index
        );
    }
    xml.push_str("          <dialogue mixedContentKind=\"0\">2</dialogue>\n");
    xml.push_str("        </audioContent>\n");
    if !bed_tracks.is_empty() {
        let _ = write!(
            xml,
            "        <audioObject audioObjectID=\"AO_1001\" audioObjectName=\"OpenJOC Generated 5.1 Bed\" start=\"00:00:00.00000\" duration=\"{duration}\">\n          <audioPackFormatIDRef>AP_00011001</audioPackFormatIDRef>\n"
        );
        for track in &bed_tracks {
            let _ = writeln!(
                xml,
                "          <audioTrackUIDRef>{}</audioTrackUIDRef>",
                track.uid
            );
        }
        xml.push_str("        </audioObject>\n");
    }
    for (index, track) in object_tracks.iter().enumerate() {
        let object_name = track.signal_name();
        let _ = write!(
            xml,
            "        <audioObject audioObjectID=\"AO_{:04X}\" audioObjectName=\"{}\" start=\"00:00:00.00000\" duration=\"{duration}\">\n          <audioPackFormatIDRef>{}</audioPackFormatIDRef>\n          <audioTrackUIDRef>{}</audioTrackUIDRef>\n        </audioObject>\n",
            0x100B + index,
            xml_escape(&object_name),
            track.pack_id,
            track.uid
        );
    }
    if !bed_tracks.is_empty() {
        xml.push_str("        <audioPackFormat audioPackFormatID=\"AP_00011001\" audioPackFormatName=\"OpenJOC Generated 5.1 Bed\" typeLabel=\"0001\" typeDefinition=\"DirectSpeakers\">\n");
        for track in &bed_tracks {
            let _ = writeln!(
                xml,
                "          <audioChannelFormatIDRef>{}</audioChannelFormatIDRef>",
                track.channel_id
            );
        }
        xml.push_str("        </audioPackFormat>\n");
    }
    for track in &object_tracks {
        let TrackKind::Reconstruction(_) = track.kind else {
            unreachable!();
        };
        let object_name = track.signal_name();
        let _ = write!(
            xml,
            "        <audioPackFormat audioPackFormatID=\"{}\" audioPackFormatName=\"{}\" typeLabel=\"0003\" typeDefinition=\"Objects\">\n          <audioChannelFormatIDRef>{}</audioChannelFormatIDRef>\n        </audioPackFormat>\n",
            track.pack_id,
            xml_escape(&object_name),
            track.channel_id
        );
    }
    for track in tracks {
        let (channel_name, type_definition, type_label) = match track.kind {
            TrackKind::Bed(channel) => {
                (channel.channel_name().to_owned(), "DirectSpeakers", "0001")
            }
            TrackKind::Reconstruction(_) => (track.signal_name(), "Objects", "0003"),
        };
        let block_id = track.channel_id.replacen("AC_", "AB_", 1);
        let _ = writeln!(
            xml,
            "        <audioChannelFormat audioChannelFormatID=\"{}\" audioChannelFormatName=\"{}\" typeLabel=\"{}\" typeDefinition=\"{}\">",
            track.channel_id,
            xml_escape(&channel_name),
            type_label,
            type_definition
        );
        match track.kind {
            TrackKind::Bed(channel) => {
                let (x, y, z) = channel.position();
                let _ = write!(
                    xml,
                    "          <audioBlockFormat audioBlockFormatID=\"{block_id}_00000001\">\n            <speakerLabel>{}</speakerLabel>\n            <cartesian>1</cartesian>\n            <position coordinate=\"X\">{x}</position>\n            <position coordinate=\"Y\">{y}</position>\n            <position coordinate=\"Z\">{z}</position>\n          </audioBlockFormat>\n",
                    channel.speaker_label()
                );
            }
            TrackKind::Reconstruction(_) => {
                if track.dynamic_blocks.is_empty() {
                    let _ = write!(
                        xml,
                        "          <audioBlockFormat audioBlockFormatID=\"{block_id}_00000001\" rtime=\"00:00:00.00000\" duration=\"{duration}\">\n            <cartesian>1</cartesian>\n            <position coordinate=\"X\">0.000000</position>\n            <position coordinate=\"Y\">0.000000</position>\n            <position coordinate=\"Z\">0.000000</position>\n            <jumpPosition interpolationLength=\"0\">1</jumpPosition>\n          </audioBlockFormat>\n"
                    );
                } else {
                    for (block_index, block) in track.dynamic_blocks.iter().enumerate() {
                        let block_id = format!("{block_id}_{:08X}", block_index + 1);
                        // Block timing stays in the sample domain without
                        // rounding to the five-decimal ADM display form.
                        let rtime = format_time(block.start_sample, sample_rate);
                        let block_duration = format_time(block.duration_samples, sample_rate);
                        let _ = write!(
                            xml,
                            "          <audioBlockFormat audioBlockFormatID=\"{block_id}\" rtime=\"{rtime}\" duration=\"{block_duration}\">\n            <cartesian>1</cartesian>\n            <position coordinate=\"X\">{:.6}</position>\n            <position coordinate=\"Y\">{:.6}</position>\n            <position coordinate=\"Z\">{:.6}</position>\n            <jumpPosition interpolationLength=\"{}\">1</jumpPosition>\n          </audioBlockFormat>\n",
                            block.position.x,
                            block.position.y,
                            block.position.z,
                            if block_index == 0 {
                                0
                            } else {
                                DOLBY_SUBSEQUENT_JUMP_INTERPOLATION_SAMPLES
                            },
                        );
                    }
                }
            }
        }
        xml.push_str("        </audioChannelFormat>\n");
    }
    for track in tracks {
        let channel_name = match track.kind {
            TrackKind::Bed(channel) => channel.channel_name().to_owned(),
            TrackKind::Reconstruction(_) => track.signal_name(),
        };
        let _ = write!(
            xml,
            "        <audioStreamFormat audioStreamFormatID=\"{}\" audioStreamFormatName=\"PCM_{}\" formatLabel=\"0001\" formatDefinition=\"PCM\">\n          <audioChannelFormatIDRef>{}</audioChannelFormatIDRef>\n          <audioPackFormatIDRef>{}</audioPackFormatIDRef>\n          <audioTrackFormatIDRef>{}</audioTrackFormatIDRef>\n        </audioStreamFormat>\n",
            track.stream_id,
            xml_escape(&channel_name),
            track.channel_id,
            track.pack_id,
            track.track_id
        );
    }
    for track in tracks {
        let channel_name = match track.kind {
            TrackKind::Bed(channel) => channel.channel_name().to_owned(),
            TrackKind::Reconstruction(_) => track.signal_name(),
        };
        let _ = write!(
            xml,
            "        <audioTrackFormat audioTrackFormatID=\"{}\" audioTrackFormatName=\"PCM_{}\" formatLabel=\"0001\" formatDefinition=\"PCM\">\n          <audioStreamFormatIDRef>{}</audioStreamFormatIDRef>\n        </audioTrackFormat>\n",
            track.track_id,
            xml_escape(&channel_name),
            track.stream_id
        );
    }
    for track in tracks {
        let _ = write!(
            xml,
            "        <audioTrackUID UID=\"{}\" sampleRate=\"{}\" bitDepth=\"24\">\n          <audioTrackFormatIDRef>{}</audioTrackFormatIDRef>\n          <audioPackFormatIDRef>{}</audioPackFormatIDRef>\n        </audioTrackUID>\n",
            track.uid, sample_rate, track.track_id, track.pack_id
        );
    }
    xml.push_str(
        "      </audioFormatExtended>\n    </format>\n  </coreMetadata>\n</ebuCoreMain>\n",
    );
    xml
}

fn write_adm_bwf_header<W: Write>(writer: &mut W, plan: &AdmExportPlan) -> Result<(), AdmError> {
    let channels = u16::try_from(plan.tracks.len()).map_err(|_| AdmError::SizeOverflow)?;
    match plan.container {
        AdmContainer::Riff => {
            writer.write_all(b"RIFF")?;
            let riff_size = u32::try_from(
                plan.total_size
                    .checked_sub(8)
                    .ok_or(AdmError::SizeOverflow)?,
            )
            .map_err(|_| AdmError::SizeOverflow)?;
            writer.write_all(&riff_size.to_le_bytes())?;
            writer.write_all(b"WAVE")?;
            writer.write_all(b"JUNK")?;
            writer.write_all(&64_u32.to_le_bytes())?;
            writer.write_all(&u64::from(riff_size).to_le_bytes())?;
            writer.write_all(&plan.data_bytes.to_le_bytes())?;
            writer.write_all(&plan.duration_samples.to_le_bytes())?;
            writer.write_all(&0_u32.to_le_bytes())?;
            writer.write_all(&[0_u8; 36])?;
        }
        AdmContainer::Rf64 => {
            writer.write_all(b"RF64")?;
            writer.write_all(&u32::MAX.to_le_bytes())?;
            writer.write_all(b"WAVE")?;
            writer.write_all(b"ds64")?;
            writer.write_all(&28_u32.to_le_bytes())?;
            writer.write_all(&plan.total_size.saturating_sub(8).to_le_bytes())?;
            writer.write_all(&plan.data_bytes.to_le_bytes())?;
            writer.write_all(&plan.duration_samples.to_le_bytes())?;
            writer.write_all(&0_u32.to_le_bytes())?;
        }
    }
    writer.write_all(b"fmt ")?;
    writer.write_all(&16_u32.to_le_bytes())?;
    writer.write_all(&1_u16.to_le_bytes())?;
    writer.write_all(&channels.to_le_bytes())?;
    writer.write_all(&plan.sample_rate.to_le_bytes())?;
    let byte_rate = plan
        .sample_rate
        .checked_mul(u32::from(channels))
        .and_then(|value| value.checked_mul(3))
        .ok_or(AdmError::SizeOverflow)?;
    writer.write_all(&byte_rate.to_le_bytes())?;
    writer.write_all(&(channels.checked_mul(3).ok_or(AdmError::SizeOverflow)?).to_le_bytes())?;
    writer.write_all(&24_u16.to_le_bytes())?;
    writer.write_all(b"data")?;
    let data_size = match plan.container {
        AdmContainer::Riff => u32::try_from(plan.data_bytes).map_err(|_| AdmError::SizeOverflow)?,
        AdmContainer::Rf64 => u32::MAX,
    };
    writer.write_all(&data_size.to_le_bytes())?;
    Ok(())
}

fn write_adm_metadata<W: Write>(writer: &mut W, plan: &AdmExportPlan) -> Result<(), AdmError> {
    write_chunk(writer, *b"axml", plan.xml.as_bytes())?;
    write_chunk(writer, *b"chna", &chna_payload(&plan.tracks)?)?;
    write_chunk(writer, *b"dbmd", &dbmd_payload())
}

fn dbmd_payload() -> Vec<u8> {
    let mut payload = Vec::with_capacity(6);
    payload.extend_from_slice(&0x0100_0000_u32.to_le_bytes());
    payload.push(0);
    payload.push(0);
    payload
}

fn adm_bwf_total_size(
    reserve_payload: u64,
    axml_len: u64,
    chna_len: u64,
    dbmd_len: u64,
    data_bytes: u64,
) -> Result<u64, AdmError> {
    12_u64
        .checked_add(chunk_total_size(reserve_payload))
        .and_then(|value| value.checked_add(8 + 16))
        .and_then(|value| value.checked_add(chunk_total_size(data_bytes)))
        .and_then(|value| value.checked_add(chunk_total_size(axml_len)))
        .and_then(|value| value.checked_add(chunk_total_size(chna_len)))
        .and_then(|value| value.checked_add(chunk_total_size(dbmd_len)))
        .ok_or(AdmError::SizeOverflow)
}

fn adm_bwf_layout(
    axml_len: u64,
    chna_len: u64,
    dbmd_len: u64,
    data_bytes: u64,
) -> Result<(AdmContainer, u64), AdmError> {
    let riff_total = adm_bwf_total_size(64, axml_len, chna_len, dbmd_len, data_bytes)?;
    if u32::try_from(data_bytes).is_ok()
        && riff_total
            .checked_sub(8)
            .is_some_and(|size| u32::try_from(size).is_ok())
    {
        Ok((AdmContainer::Riff, riff_total))
    } else {
        Ok((
            AdmContainer::Rf64,
            adm_bwf_total_size(28, axml_len, chna_len, dbmd_len, data_bytes)?,
        ))
    }
}

fn quantize_s24(track: usize, sample: u64, value: f64) -> Result<[u8; 3], AdmError> {
    if !value.is_finite() {
        return Err(AdmError::NonFiniteSample { track, sample });
    }
    if !(-1.0..=1.0).contains(&value) {
        return Err(AdmError::SampleOutOfRange {
            track,
            sample,
            value,
        });
    }
    let scaled = (value * 8_388_608.0).round();
    let integer = scaled.clamp(-8_388_608.0, 8_388_607.0) as i32;
    let bytes = integer.to_le_bytes();
    Ok([bytes[0], bytes[1], bytes[2]])
}

#[cfg(test)]
fn write_adm_bwf_legacy_inner<W: Write + Seek>(
    writer: &mut W,
    scene: &ObjectScene,
    policy: AdmPolicy,
) -> Result<(), AdmError> {
    let plan = AdmExportPlan::from_scene(scene, policy)?;
    let basis = scene
        .reconstruction_basis
        .as_ref()
        .ok_or(AdmError::NoReconstructionSignals)?;
    let frames = usize::try_from(scene.duration_samples).map_err(|_| AdmError::SizeOverflow)?;
    if basis.rows.iter().any(|track| track.len() != frames)
        || scene
            .base_lfe_pcm
            .as_ref()
            .is_some_and(|track| track.len() != frames)
    {
        return Err(AdmError::InvalidScene(
            "ADM tracks have different durations".to_owned(),
        ));
    }
    write_adm_bwf_header(writer, &plan)?;
    for frame in 0..frames {
        if let Some(lfe) = &scene.base_lfe_pcm {
            writer.write_all(&[0_u8; 9])?;
            writer.write_all(&quantize_s24(
                basis.rows.len(),
                u64::try_from(frame).map_err(|_| AdmError::SizeOverflow)?,
                lfe[frame],
            )?)?;
            writer.write_all(&[0_u8; 6])?;
        }
        for (track_index, track) in basis.rows.iter().enumerate() {
            writer.write_all(&quantize_s24(
                track_index,
                u64::try_from(frame).map_err(|_| AdmError::SizeOverflow)?,
                track[frame],
            )?)?;
        }
    }
    if plan.data_bytes % 2 != 0 {
        writer.write_all(&[0])?;
    }
    write_adm_metadata(writer, &plan)?;
    writer.flush()?;
    Ok(())
}

fn chunk_total_size(payload: u64) -> u64 {
    8 + payload + payload % 2
}

fn write_chunk<W: Write>(writer: &mut W, id: [u8; 4], payload: &[u8]) -> Result<(), AdmError> {
    let size = u32::try_from(payload.len()).map_err(|_| AdmError::SizeOverflow)?;
    writer.write_all(&id)?;
    writer.write_all(&size.to_le_bytes())?;
    writer.write_all(payload)?;
    if payload.len() % 2 != 0 {
        writer.write_all(&[0])?;
    }
    Ok(())
}

fn chna_payload(tracks: &[TrackDescriptor]) -> Result<Vec<u8>, AdmError> {
    let count = u16::try_from(tracks.len()).map_err(|_| AdmError::SizeOverflow)?;
    let mut payload = Vec::with_capacity(4 + tracks.len() * 40);
    payload.extend_from_slice(&count.to_le_bytes());
    payload.extend_from_slice(&count.to_le_bytes());
    for track in tracks {
        payload.extend_from_slice(&track.track_index.to_le_bytes());
        write_fixed(&mut payload, track.uid.as_bytes(), 12)?;
        write_fixed(&mut payload, track.track_id.as_bytes(), 14)?;
        write_fixed(&mut payload, track.pack_id.as_bytes(), 11)?;
        payload.push(0);
    }
    Ok(payload)
}

fn write_fixed(output: &mut Vec<u8>, value: &[u8], width: usize) -> Result<(), AdmError> {
    if value.len() > width {
        return Err(AdmError::SizeOverflow);
    }
    output.extend_from_slice(value);
    output.resize(output.len() + width - value.len(), 0);
    Ok(())
}

/// Validates ADM BWF incrementally without retaining the programme PCM chunk.
pub fn validate_reader<R: Read + Seek>(reader: &mut R) -> Result<AdmValidationSummary, AdmError> {
    let file_len = reader.seek(SeekFrom::End(0))?;
    reader.seek(SeekFrom::Start(0))?;
    let mut header = [0_u8; 12];
    read_exact_adm(reader, &mut header, "truncated RIFF/RF64 WAVE header")?;
    let container = ContainerKind::from_signature(&header[0..4])?;
    if &header[8..12] != b"WAVE" {
        return Err(AdmError::InvalidAdmBwf("missing WAVE form type"));
    }
    let header_size = u32::from_le_bytes(
        header[4..8]
            .try_into()
            .map_err(|_| AdmError::InvalidAdmBwf("invalid container size"))?,
    );
    match container {
        ContainerKind::Riff => {
            if u64::from(header_size).checked_add(8) != Some(file_len) {
                return Err(AdmError::InvalidAdmBwf(
                    "RIFF size disagrees with file length",
                ));
            }
        }
        ContainerKind::Rf64 | ContainerKind::Bw64 if header_size != u32::MAX => {
            return Err(AdmError::InvalidAdmBwf(
                "RF64/BW64 container size is not the 32-bit sentinel",
            ));
        }
        ContainerKind::Rf64 | ContainerKind::Bw64 => {}
    }
    let mut cursor = 12_u64;
    let mut chunks = Vec::new();
    let mut ds64 = None::<Ds64>;
    let mut ds64_table_used = Vec::<bool>::new();
    let mut fmt = None::<Vec<u8>>;
    let mut data_bytes = None;
    let mut axml = None::<Vec<u8>>;
    let mut chna = None::<Vec<u8>>;
    let mut dbmd = None::<Vec<u8>>;
    let mut junk_size = None;
    while cursor < file_len {
        let header_end = cursor.checked_add(8).ok_or(AdmError::SizeOverflow)?;
        if header_end > file_len {
            return Err(AdmError::InvalidAdmBwf("truncated chunk header"));
        }
        reader.seek(SeekFrom::Start(cursor))?;
        let mut chunk_header = [0_u8; 8];
        read_exact_adm(reader, &mut chunk_header, "truncated chunk header")?;
        let id: [u8; 4] = chunk_header[0..4]
            .try_into()
            .map_err(|_| AdmError::InvalidAdmBwf("invalid chunk identifier"))?;
        let declared_size_32 = u32::from_le_bytes(
            chunk_header[4..8]
                .try_into()
                .map_err(|_| AdmError::InvalidAdmBwf("invalid chunk size"))?,
        );
        let declared_size = u64::from(declared_size_32);
        let payload_start = header_end;
        let size = if declared_size_32 == u32::MAX {
            if container == ContainerKind::Riff {
                return Err(AdmError::InvalidAdmBwf(
                    "RIFF chunk uses an RF64 size sentinel",
                ));
            }
            let sizes = ds64
                .as_ref()
                .ok_or(AdmError::InvalidAdmBwf("sentinel chunk precedes ds64"))?;
            if &id == b"data" {
                sizes.data_size
            } else {
                let (index, size) = sizes
                    .table
                    .iter()
                    .enumerate()
                    .find_map(|(index, (table_id, table_size))| {
                        (*table_id == id && !ds64_table_used[index]).then_some((index, *table_size))
                    })
                    .ok_or(AdmError::InvalidAdmBwf(
                        "sentinel chunk is absent from the ds64 table",
                    ))?;
                ds64_table_used[index] = true;
                size
            }
        } else {
            declared_size
        };
        let payload_end = payload_start
            .checked_add(size)
            .ok_or(AdmError::SizeOverflow)?;
        let padded_end = payload_end
            .checked_add(size % 2)
            .ok_or(AdmError::SizeOverflow)?;
        if padded_end > file_len {
            return Err(AdmError::InvalidAdmBwf("chunk exceeds file"));
        }
        chunks.push(String::from_utf8_lossy(&id).to_string());
        match &id {
            b"ds64" => {
                if container == ContainerKind::Riff || cursor != 12 || ds64.is_some() {
                    return Err(AdmError::InvalidAdmBwf(
                        "ds64 is not the unique first RF64/BW64 chunk",
                    ));
                }
                if !(28..=64).contains(&size) {
                    return Err(AdmError::InvalidAdmBwf("unsupported ds64 payload"));
                }
                let payload = read_bounded_chunk(reader, payload_start, size, 1024 * 1024)?;
                let value = parse_ds64(&payload)?;
                if value.riff_size.checked_add(8) != Some(file_len) {
                    return Err(AdmError::InvalidAdmBwf(
                        "ds64 RIFF size disagrees with file",
                    ));
                }
                ds64_table_used.resize(value.table.len(), false);
                ds64 = Some(value);
            }
            b"JUNK" if container == ContainerKind::Riff && cursor == 12 => {
                if size > 64 {
                    return Err(AdmError::InvalidAdmBwf(
                        "Dolby Atmos RIFF JUNK reserve exceeds 64 bytes",
                    ));
                }
                junk_size = Some(size);
            }
            b"fmt " => {
                if fmt.is_some() {
                    return Err(AdmError::InvalidAdmBwf("duplicate fmt chunk"));
                }
                fmt = Some(read_bounded_chunk(reader, payload_start, size, 64)?);
            }
            b"data" if data_bytes.replace(size).is_some() => {
                return Err(AdmError::InvalidAdmBwf("duplicate data chunk"));
            }
            b"axml" => {
                if axml.is_some() {
                    return Err(AdmError::InvalidAdmBwf("duplicate axml chunk"));
                }
                axml = Some(read_bounded_chunk(
                    reader,
                    payload_start,
                    size,
                    MAX_AXML_BYTES,
                )?);
            }
            b"chna" => {
                if chna.is_some() {
                    return Err(AdmError::InvalidAdmBwf("duplicate chna chunk"));
                }
                chna = Some(read_bounded_chunk(
                    reader,
                    payload_start,
                    size,
                    MAX_CHNA_BYTES,
                )?);
            }
            b"dbmd" => {
                if dbmd.is_some() {
                    return Err(AdmError::InvalidAdmBwf("duplicate dbmd chunk"));
                }
                dbmd = Some(read_bounded_chunk(
                    reader,
                    payload_start,
                    size,
                    MAX_DBMD_BYTES,
                )?);
            }
            _ => {}
        }
        cursor = padded_end;
    }
    if cursor != file_len {
        return Err(AdmError::InvalidAdmBwf(
            "chunk layout does not end at file boundary",
        ));
    }
    if container != ContainerKind::Riff
        && (ds64.is_none() || chunks.first().map(String::as_str) != Some("ds64"))
    {
        return Err(AdmError::InvalidAdmBwf(
            "ds64 is not the first RF64/BW64 chunk",
        ));
    }
    if ds64_table_used.iter().any(|used| !used) {
        return Err(AdmError::InvalidAdmBwf("unused ds64 table entry"));
    }
    if container == ContainerKind::Riff
        && (junk_size.is_none() || chunks.first().map(String::as_str) != Some("JUNK"))
    {
        return Err(AdmError::InvalidAdmBwf(
            "JUNK is not the first RIFF ADM BWF chunk",
        ));
    }
    let fmt = fmt.ok_or(AdmError::InvalidAdmBwf("missing fmt chunk"))?;
    if fmt.len() != 16 || read_u16_at(&fmt, 0)? != 1 {
        return Err(AdmError::InvalidAdmBwf(
            "ADM interchange requires a 16-byte integer PCM fmt payload",
        ));
    }
    let channels = usize::from(read_u16_at(&fmt, 2)?);
    let sample_rate = read_u32_at(&fmt, 4)?;
    let byte_rate = read_u32_at(&fmt, 8)?;
    let fmt_block_align = read_u16_at(&fmt, 12)?;
    let bits_per_sample = read_u16_at(&fmt, 14)?;
    let data_bytes = data_bytes.ok_or(AdmError::InvalidAdmBwf("missing data chunk"))?;
    if let Some(ds64) = &ds64 {
        if data_bytes != ds64.data_size {
            return Err(AdmError::InvalidAdmBwf("ds64 data size mismatch"));
        }
    }
    let block_align = u16::try_from(channels)
        .ok()
        .and_then(|value| value.checked_mul(3))
        .ok_or(AdmError::SizeOverflow)?;
    let expected_byte_rate = sample_rate
        .checked_mul(u32::from(block_align))
        .ok_or(AdmError::SizeOverflow)?;
    if !(1..=128).contains(&channels)
        || !matches!(sample_rate, 48_000 | 96_000)
        || bits_per_sample != 24
        || fmt_block_align != block_align
        || byte_rate != expected_byte_rate
    {
        return Err(AdmError::InvalidAdmBwf(
            "inconsistent signed 24-bit PCM fmt",
        ));
    }
    let block_align_u64 = u64::from(block_align);
    if data_bytes % block_align_u64 != 0 {
        return Err(AdmError::InvalidAdmBwf("PCM data is not frame-aligned"));
    }
    let sample_count = data_bytes / block_align_u64;
    if let Some(ds64) = &ds64 {
        if sample_count != ds64.sample_count {
            return Err(AdmError::InvalidAdmBwf("ds64 sample count mismatch"));
        }
    }
    let axml = axml.ok_or(AdmError::InvalidAdmBwf("missing axml chunk"))?;
    let chna = chna.ok_or(AdmError::InvalidAdmBwf("missing chna chunk"))?;
    let dbmd = dbmd.ok_or(AdmError::InvalidAdmBwf("missing Dolby dbmd chunk"))?;
    let dbmd_segment_ids = validate_dbmd(&dbmd)?;
    let dolby_reserved_dbmd_segments_present = dbmd_segment_ids
        .iter()
        .any(|segment_id| matches!(segment_id, 9..=u8::MAX));
    let (tracks, records) = parse_chna(&chna, channels)?;
    validate_adm_xml(&axml, sample_rate, sample_count, &records)?;
    Ok(AdmValidationSummary {
        container: container.as_str(),
        chunks,
        sample_rate,
        channels,
        data_bytes,
        axml_bytes: u64::try_from(axml.len()).map_err(|_| AdmError::SizeOverflow)?,
        dbmd_bytes: u64::try_from(dbmd.len()).map_err(|_| AdmError::SizeOverflow)?,
        dbmd_segment_ids,
        dolby_reserved_dbmd_segments_present,
        chna_tracks: tracks,
        chna_uids: records.len(),
        identifiers_unique: true,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ContainerKind {
    Riff,
    Rf64,
    Bw64,
}

impl ContainerKind {
    fn from_signature(signature: &[u8]) -> Result<Self, AdmError> {
        match signature {
            b"RIFF" => Ok(Self::Riff),
            b"RF64" => Ok(Self::Rf64),
            b"BW64" => Ok(Self::Bw64),
            _ => Err(AdmError::InvalidAdmBwf(
                "missing RIFF, RF64, or legacy BW64 signature",
            )),
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Riff => "RIFF",
            Self::Rf64 => "RF64",
            Self::Bw64 => "BW64",
        }
    }
}

#[derive(Clone, Debug)]
struct Ds64 {
    riff_size: u64,
    data_size: u64,
    sample_count: u64,
    table: Vec<([u8; 4], u64)>,
}

fn parse_ds64(payload: &[u8]) -> Result<Ds64, AdmError> {
    if payload.len() < 28 {
        return Err(AdmError::InvalidAdmBwf("truncated ds64 payload"));
    }
    let table_len =
        usize::try_from(read_u32_at(payload, 24)?).map_err(|_| AdmError::SizeOverflow)?;
    let expected = table_len
        .checked_mul(12)
        .and_then(|size| size.checked_add(28))
        .ok_or(AdmError::SizeOverflow)?;
    if payload.len() != expected {
        return Err(AdmError::InvalidAdmBwf("ds64 table length mismatch"));
    }
    let mut table = Vec::with_capacity(table_len);
    for index in 0..table_len {
        let base = 28 + index * 12;
        let id = payload[base..base + 4]
            .try_into()
            .map_err(|_| AdmError::InvalidAdmBwf("truncated ds64 table entry"))?;
        if &id == b"data" || &id == b"ds64" {
            return Err(AdmError::InvalidAdmBwf(
                "ds64 table contains a reserved chunk ID",
            ));
        }
        let size = read_u64(&payload[base + 4..base + 12])?;
        table.push((id, size));
    }
    Ok(Ds64 {
        riff_size: read_u64(&payload[0..8])?,
        data_size: read_u64(&payload[8..16])?,
        sample_count: read_u64(&payload[16..24])?,
        table,
    })
}

fn validate_dbmd(payload: &[u8]) -> Result<Vec<u8>, AdmError> {
    if payload.len() < 5 || read_u32_at(payload, 0)? == 0 {
        return Err(AdmError::InvalidAdmBwf("invalid dbmd version/header"));
    }
    let mut cursor = 4_usize;
    let mut segment_ids = Vec::new();
    loop {
        let segment_id = *payload
            .get(cursor)
            .ok_or(AdmError::InvalidAdmBwf("dbmd has no end segment"))?;
        cursor = cursor.checked_add(1).ok_or(AdmError::SizeOverflow)?;
        if segment_id == 0 {
            if payload[cursor..].len() > 1 {
                return Err(AdmError::InvalidAdmBwf("invalid dbmd trailing padding"));
            }
            return Ok(segment_ids);
        }
        segment_ids.push(segment_id);
        let size_end = cursor.checked_add(2).ok_or(AdmError::SizeOverflow)?;
        let size_bytes = payload
            .get(cursor..size_end)
            .ok_or(AdmError::InvalidAdmBwf("truncated dbmd segment size"))?;
        let declared =
            usize::from(u16::from_le_bytes(size_bytes.try_into().map_err(|_| {
                AdmError::InvalidAdmBwf("invalid dbmd segment size")
            })?));
        let segment_size = if declared == 0 { 65_535 } else { declared };
        let payload_start = size_end;
        let payload_end = payload_start
            .checked_add(segment_size)
            .ok_or(AdmError::SizeOverflow)?;
        let checksum_end = payload_end.checked_add(1).ok_or(AdmError::SizeOverflow)?;
        let segment = payload
            .get(cursor..checksum_end)
            .ok_or(AdmError::InvalidAdmBwf("truncated dbmd segment"))?;
        if segment
            .iter()
            .fold(0_u8, |sum, byte| sum.wrapping_add(*byte))
            != 0
        {
            return Err(AdmError::InvalidAdmBwf("dbmd segment checksum mismatch"));
        }
        cursor = checksum_end;
    }
}

#[derive(Clone, Debug)]
struct ChnaRecord {
    uid: String,
    track_ref: String,
    pack_ref: String,
}

fn parse_chna(payload: &[u8], channels: usize) -> Result<(usize, Vec<ChnaRecord>), AdmError> {
    if payload.len() < 4 || (payload.len() - 4) % 40 != 0 {
        return Err(AdmError::InvalidAdmBwf("invalid chna length"));
    }
    let tracks = usize::from(read_u16_at(payload, 0)?);
    let uid_count = usize::from(read_u16_at(payload, 2)?);
    let capacity = (payload.len() - 4) / 40;
    if tracks != channels || uid_count == 0 || uid_count > capacity {
        return Err(AdmError::InvalidAdmBwf("chna track/channel count mismatch"));
    }
    let mut records = Vec::with_capacity(uid_count);
    let mut uids = HashSet::with_capacity(uid_count);
    let mut referenced_tracks = HashSet::with_capacity(tracks);
    for index in 0..uid_count {
        let base = 4 + index * 40;
        let record = payload
            .get(base..base + 40)
            .ok_or(AdmError::InvalidAdmBwf("truncated chna audioID"))?;
        let track_index = read_u16_at(record, 0)?;
        if track_index == 0 || usize::from(track_index) > tracks || record[39] != 0 {
            return Err(AdmError::InvalidAdmBwf(
                "invalid chna track index or padding",
            ));
        }
        let uid = read_fixed_id(&record[2..14])?;
        let track_ref = read_fixed_id(&record[14..28])?;
        let pack_ref = read_fixed_id(&record[28..39])?;
        if !is_adm_id(&uid, IdKind::TrackUid)
            || !(is_adm_id(&track_ref, IdKind::TrackFormat)
                || is_adm_id(&track_ref, IdKind::Channel))
            || !is_adm_id(&pack_ref, IdKind::Pack)
        {
            return Err(AdmError::InvalidAdmBwf("invalid ADM identifier in chna"));
        }
        if !uids.insert(uid.clone()) {
            return Err(AdmError::InvalidAdmBwf("duplicate chna UID"));
        }
        referenced_tracks.insert(track_index);
        records.push(ChnaRecord {
            uid,
            track_ref,
            pack_ref,
        });
    }
    if referenced_tracks.len() != tracks {
        return Err(AdmError::InvalidAdmBwf(
            "chna does not reference every PCM track",
        ));
    }
    if payload[4 + uid_count * 40..].iter().any(|byte| *byte != 0) {
        return Err(AdmError::InvalidAdmBwf(
            "unused chna audioID capacity is not zero-filled",
        ));
    }
    Ok((tracks, records))
}

fn read_fixed_id(bytes: &[u8]) -> Result<String, AdmError> {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    if bytes[end..].iter().any(|byte| *byte != 0) {
        return Err(AdmError::InvalidAdmBwf(
            "non-zero bytes follow chna ID terminator",
        ));
    }
    let value = std::str::from_utf8(&bytes[..end])
        .map_err(|_| AdmError::InvalidAdmBwf("chna ID is not UTF-8/ASCII"))?;
    if value.is_empty() || !value.is_ascii() {
        return Err(AdmError::InvalidAdmBwf("empty or non-ASCII chna ID"));
    }
    Ok(value.to_owned())
}

#[derive(Clone, Copy)]
enum IdKind {
    Programme,
    Content,
    Object,
    Pack,
    Channel,
    Block,
    Stream,
    TrackFormat,
    TrackUid,
}

fn is_adm_id(value: &str, kind: IdKind) -> bool {
    let (prefix, hexadecimal) = match kind {
        IdKind::Programme => ("APR_", 4),
        IdKind::Content => ("ACO_", 4),
        IdKind::Object => ("AO_", 4),
        IdKind::Pack => ("AP_", 8),
        IdKind::Channel => ("AC_", 8),
        IdKind::Block => ("AB_", 17),
        IdKind::Stream => ("AS_", 8),
        IdKind::TrackFormat => ("AT_", 11),
        IdKind::TrackUid => ("ATU_", 8),
    };
    let Some(body) = value.strip_prefix(prefix) else {
        return false;
    };
    if body.len() != hexadecimal {
        return false;
    }
    match kind {
        IdKind::Block => {
            body.as_bytes().get(8) == Some(&b'_')
                && body[..8].bytes().all(|byte| byte.is_ascii_hexdigit())
                && body[9..].bytes().all(|byte| byte.is_ascii_hexdigit())
        }
        IdKind::TrackFormat => {
            body.as_bytes().get(8) == Some(&b'_')
                && body[..8].bytes().all(|byte| byte.is_ascii_hexdigit())
                && body[9..].bytes().all(|byte| byte.is_ascii_hexdigit())
        }
        _ => body.bytes().all(|byte| byte.is_ascii_hexdigit()),
    }
}

fn collect_adm_ids(
    scope: roxmltree::Node<'_, '_>,
    element_name: &str,
    attribute_name: &str,
    kind: IdKind,
    all_ids: &mut HashSet<String>,
) -> Result<HashSet<String>, AdmError> {
    let mut ids = HashSet::new();
    for element in scope
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == element_name)
    {
        let id = element
            .attribute(attribute_name)
            .ok_or(AdmError::InvalidAdmBwf("ADM element is missing its ID"))?;
        if !is_adm_id(id, kind) {
            return Err(AdmError::InvalidAdmBwf("ADM element ID has invalid syntax"));
        }
        if !ids.insert(id.to_owned()) || !all_ids.insert(id.to_owned()) {
            return Err(AdmError::InvalidAdmBwf("duplicate ADM element ID"));
        }
    }
    Ok(ids)
}

fn child_ref_values<'a>(node: roxmltree::Node<'a, '_>, reference_name: &str) -> Vec<&'a str> {
    node.children()
        .filter(|child| child.is_element() && child.tag_name().name() == reference_name)
        .filter_map(|child| child.text().map(str::trim))
        .filter(|value| !value.is_empty())
        .collect()
}

fn require_child_refs(
    node: roxmltree::Node<'_, '_>,
    reference_name: &str,
    targets: &HashSet<String>,
) -> Result<Vec<String>, AdmError> {
    let refs = child_ref_values(node, reference_name);
    if refs.is_empty() || refs.iter().any(|reference| !targets.contains(*reference)) {
        return Err(AdmError::InvalidAdmBwf("missing or dangling ADM IDRef"));
    }
    Ok(refs.into_iter().map(ToOwned::to_owned).collect())
}

fn validate_adm_xml(
    payload: &[u8],
    sample_rate: u32,
    sample_count: u64,
    chna: &[ChnaRecord],
) -> Result<(), AdmError> {
    let xml =
        std::str::from_utf8(payload).map_err(|_| AdmError::InvalidAdmBwf("axml is not UTF-8"))?;
    let document = roxmltree::Document::parse(xml)
        .map_err(|_| AdmError::InvalidAdmBwf("malformed ADM XML"))?;
    let root = document.root_element();
    if root.tag_name().name() != "ebuCoreMain"
        || !matches!(
            root.tag_name().namespace(),
            Some("urn:ebu:metadata-schema:ebuCore_2014" | "urn:ebu:metadata-schema:ebuCore_2016")
        )
    {
        return Err(AdmError::InvalidAdmBwf(
            "unsupported EBUCore XML root/namespace",
        ));
    }
    let mut extended = document
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "audioFormatExtended");
    let audio_format_extended = extended
        .next()
        .ok_or(AdmError::InvalidAdmBwf("missing audioFormatExtended"))?;
    if extended.next().is_some()
        || audio_format_extended
            .parent_element()
            .is_none_or(|node| node.tag_name().name() != "format")
        || audio_format_extended
            .parent_element()
            .and_then(|node| node.parent_element())
            .is_none_or(|node| node.tag_name().name() != "coreMetadata")
    {
        return Err(AdmError::InvalidAdmBwf(
            "audioFormatExtended is not uniquely placed under coreMetadata/format",
        ));
    }

    let mut all_ids = HashSet::new();
    let programmes = collect_adm_ids(
        audio_format_extended,
        "audioProgramme",
        "audioProgrammeID",
        IdKind::Programme,
        &mut all_ids,
    )?;
    let contents = collect_adm_ids(
        audio_format_extended,
        "audioContent",
        "audioContentID",
        IdKind::Content,
        &mut all_ids,
    )?;
    let objects = collect_adm_ids(
        audio_format_extended,
        "audioObject",
        "audioObjectID",
        IdKind::Object,
        &mut all_ids,
    )?;
    let packs = collect_adm_ids(
        audio_format_extended,
        "audioPackFormat",
        "audioPackFormatID",
        IdKind::Pack,
        &mut all_ids,
    )?;
    let channels = collect_adm_ids(
        audio_format_extended,
        "audioChannelFormat",
        "audioChannelFormatID",
        IdKind::Channel,
        &mut all_ids,
    )?;
    let blocks = collect_adm_ids(
        audio_format_extended,
        "audioBlockFormat",
        "audioBlockFormatID",
        IdKind::Block,
        &mut all_ids,
    )?;
    let streams = collect_adm_ids(
        audio_format_extended,
        "audioStreamFormat",
        "audioStreamFormatID",
        IdKind::Stream,
        &mut all_ids,
    )?;
    let track_formats = collect_adm_ids(
        audio_format_extended,
        "audioTrackFormat",
        "audioTrackFormatID",
        IdKind::TrackFormat,
        &mut all_ids,
    )?;
    let track_uids = collect_adm_ids(
        audio_format_extended,
        "audioTrackUID",
        "UID",
        IdKind::TrackUid,
        &mut all_ids,
    )?;
    if programmes.len() != 1
        || contents.is_empty()
        || objects.is_empty()
        || packs.is_empty()
        || channels.len() != chna.len()
        || blocks.is_empty()
        || streams.len() != chna.len()
        || track_formats.len() != chna.len()
        || track_uids.len() != chna.len()
    {
        return Err(AdmError::InvalidAdmBwf(
            "ADM element counts violate the Dolby Atmos master profile",
        ));
    }

    if !programmes.contains("APR_1001") {
        return Err(AdmError::InvalidAdmBwf(
            "Dolby Atmos audioProgramme ID is not APR_1001",
        ));
    }
    validate_contiguous_ids(&contents, 0x1001, AdmCounterKind::Content)?;
    validate_contiguous_ids(&packs, 0x1001, AdmCounterKind::Format)?;
    validate_contiguous_ids(&channels, 0x1001, AdmCounterKind::Format)?;
    validate_contiguous_ids(&streams, 0x1001, AdmCounterKind::Format)?;
    validate_contiguous_ids(&track_formats, 0x1001, AdmCounterKind::Format)?;
    validate_contiguous_ids(&track_uids, 1, AdmCounterKind::TrackUid)?;

    let expected_duration_seconds = sample_count as f64 / f64::from(sample_rate);

    for programme in audio_format_extended
        .children()
        .filter(|node| node.is_element() && node.tag_name().name() == "audioProgramme")
    {
        require_child_refs(programme, "audioContentIDRef", &contents)?;
        let start = programme
            .attribute("start")
            .and_then(parse_adm_time_seconds)
            .ok_or(AdmError::InvalidAdmBwf(
                "Dolby Atmos audioProgramme lacks a valid start",
            ))?;
        let end = programme
            .attribute("end")
            .and_then(parse_adm_time_seconds)
            .ok_or(AdmError::InvalidAdmBwf(
                "Dolby Atmos audioProgramme lacks a valid end",
            ))?;
        if end < start
            || ((end - start) - expected_duration_seconds).abs() > duration_tolerance(sample_rate)
        {
            return Err(AdmError::InvalidAdmBwf(
                "audioProgramme duration disagrees with PCM",
            ));
        }
    }
    for content in audio_format_extended
        .children()
        .filter(|node| node.is_element() && node.tag_name().name() == "audioContent")
    {
        require_child_refs(content, "audioObjectIDRef", &objects)?;
        let dialogue: Vec<_> = content
            .children()
            .filter(|node| node.is_element() && node.tag_name().name() == "dialogue")
            .collect();
        if dialogue.len() != 1
            || dialogue[0].attribute("mixedContentKind") != Some("0")
            || dialogue[0].text().map(str::trim) != Some("2")
        {
            return Err(AdmError::InvalidAdmBwf(
                "audioContent lacks Dolby mixed-content dialogue marker",
            ));
        }
    }
    for object in audio_format_extended
        .children()
        .filter(|node| node.is_element() && node.tag_name().name() == "audioObject")
    {
        let pack_refs = require_child_refs(object, "audioPackFormatIDRef", &packs)?;
        let uid_refs = require_child_refs(object, "audioTrackUIDRef", &track_uids)?;
        if pack_refs.len() != 1 {
            return Err(AdmError::InvalidAdmBwf(
                "audioObject must reference exactly one pack",
            ));
        }
        let start = object
            .attribute("start")
            .and_then(parse_adm_time_seconds)
            .ok_or(AdmError::InvalidAdmBwf("invalid ADM object start"))?;
        let duration = object
            .attribute("duration")
            .and_then(parse_adm_time_seconds)
            .ok_or(AdmError::InvalidAdmBwf("invalid ADM object duration"))?;
        if start != 0.0
            || (duration - expected_duration_seconds).abs() > duration_tolerance(sample_rate)
        {
            return Err(AdmError::InvalidAdmBwf("invalid ADM object time/duration"));
        }
        let object_id = object
            .attribute("audioObjectID")
            .and_then(|value| value.strip_prefix("AO_"))
            .and_then(|value| u16::from_str_radix(value, 16).ok())
            .ok_or(AdmError::InvalidAdmBwf("invalid audioObject ID"))?;
        let pack = find_adm_element(
            audio_format_extended,
            "audioPackFormat",
            "audioPackFormatID",
            &pack_refs[0],
        )
        .ok_or(AdmError::InvalidAdmBwf("audioObject pack is missing"))?;
        match pack.attribute("typeDefinition") {
            Some("DirectSpeakers") if (0x1001..=0x1080).contains(&object_id) => {
                if !(2..=10).contains(&uid_refs.len()) {
                    return Err(AdmError::InvalidAdmBwf(
                        "Dolby bed must reference 2 to 10 PCM tracks",
                    ));
                }
            }
            Some("Objects") if (0x100B..=0x1080).contains(&object_id) => {
                if uid_refs.len() != 1 {
                    return Err(AdmError::InvalidAdmBwf(
                        "Dolby object must reference exactly one PCM track",
                    ));
                }
            }
            Some("DirectSpeakers" | "Objects") => {
                return Err(AdmError::InvalidAdmBwf(
                    "audioObject ID is outside its Dolby bed/object range",
                ));
            }
            _ => {
                return Err(AdmError::InvalidAdmBwf(
                    "audioObject references an unsupported pack type",
                ));
            }
        }
    }
    for pack in audio_format_extended
        .children()
        .filter(|node| node.is_element() && node.tag_name().name() == "audioPackFormat")
    {
        require_type_consistency(pack, "audioPackFormatID")?;
        let channel_refs = require_child_refs(pack, "audioChannelFormatIDRef", &channels)?;
        match pack.attribute("typeDefinition") {
            Some("DirectSpeakers") => {
                validate_dolby_bed_configuration(audio_format_extended, &channel_refs)?;
            }
            Some("Objects") if channel_refs.len() == 1 => {}
            Some("Objects") => {
                return Err(AdmError::InvalidAdmBwf(
                    "Objects pack must reference exactly one channel",
                ));
            }
            _ => unreachable!(),
        }
    }
    for channel in audio_format_extended
        .children()
        .filter(|node| node.is_element() && node.tag_name().name() == "audioChannelFormat")
    {
        require_type_consistency(channel, "audioChannelFormatID")?;
        let channel_id = channel
            .attribute("audioChannelFormatID")
            .ok_or(AdmError::InvalidAdmBwf("missing audioChannelFormatID"))?;
        let channel_blocks: Vec<_> = channel
            .children()
            .filter(|node| node.is_element() && node.tag_name().name() == "audioBlockFormat")
            .collect();
        if channel_blocks.is_empty() {
            return Err(AdmError::InvalidAdmBwf("audioChannelFormat has no block"));
        }
        for (block_index, block) in channel_blocks.into_iter().enumerate() {
            let block_id = block
                .attribute("audioBlockFormatID")
                .ok_or(AdmError::InvalidAdmBwf("missing audioBlockFormatID"))?;
            if !block_id[3..11].eq_ignore_ascii_case(&channel_id[3..11]) {
                return Err(AdmError::InvalidAdmBwf(
                    "audioBlockFormat ID does not match its channel",
                ));
            }
            for attribute in ["rtime", "duration"] {
                if block
                    .attribute(attribute)
                    .is_some_and(|value| !is_adm_time(value))
                {
                    return Err(AdmError::InvalidAdmBwf("invalid audioBlockFormat time"));
                }
            }
            let coordinates: HashSet<_> = block
                .children()
                .filter(|node| node.is_element() && node.tag_name().name() == "position")
                .filter_map(|node| node.attribute("coordinate"))
                .collect();
            match channel.attribute("typeDefinition") {
                Some("DirectSpeakers") => {
                    let labels = child_ref_values(block, "speakerLabel");
                    if labels.len() != 1
                        || block.attribute("rtime").is_some()
                        || block.attribute("duration").is_some()
                        || !coordinates.contains("X")
                        || !coordinates.contains("Y")
                        || !validate_dolby_direct_speaker_block(channel, block, labels[0])
                    {
                        return Err(AdmError::InvalidAdmBwf(
                            "DirectSpeakers block violates Dolby room-centric assignment",
                        ));
                    }
                }
                Some("Objects") => {
                    if block.attribute("rtime").is_none()
                        || block.attribute("duration").is_none()
                        || !coordinates.contains("X")
                        || !coordinates.contains("Y")
                        || !has_dolby_jump_position(block, block_index)
                    {
                        return Err(AdmError::InvalidAdmBwf(
                            "Objects block lacks Dolby timing/position/jump metadata",
                        ));
                    }
                }
                _ => {
                    return Err(AdmError::InvalidAdmBwf(
                        "unsupported ADM typeDefinition in interchange subset",
                    ));
                }
            }
        }
    }
    for stream in audio_format_extended
        .children()
        .filter(|node| node.is_element() && node.tag_name().name() == "audioStreamFormat")
    {
        if stream.attribute("formatDefinition") != Some("PCM")
            || stream.attribute("formatLabel") != Some("0001")
        {
            return Err(AdmError::InvalidAdmBwf("audioStreamFormat is not PCM"));
        }
        let channel_refs = require_child_refs(stream, "audioChannelFormatIDRef", &channels)?;
        let pack_refs = require_child_refs(stream, "audioPackFormatIDRef", &packs)?;
        let track_refs = require_child_refs(stream, "audioTrackFormatIDRef", &track_formats)?;
        if channel_refs.len() != 1 || pack_refs.len() != 1 || track_refs.len() != 1 {
            return Err(AdmError::InvalidAdmBwf(
                "audioStreamFormat references are not one-to-one",
            ));
        }
        let stream_id = stream
            .attribute("audioStreamFormatID")
            .ok_or(AdmError::InvalidAdmBwf("audioStreamFormat lacks ID"))?;
        if !same_format_counter(stream_id, &channel_refs[0])
            || !same_format_counter(stream_id, &track_refs[0])
        {
            return Err(AdmError::InvalidAdmBwf(
                "audioStream/Track/Channel format counters disagree",
            ));
        }
        let channel = find_adm_element(
            audio_format_extended,
            "audioChannelFormat",
            "audioChannelFormatID",
            &channel_refs[0],
        )
        .ok_or(AdmError::InvalidAdmBwf("audioStream channel is missing"))?;
        let expected_name = format!(
            "PCM_{}",
            channel
                .attribute("audioChannelFormatName")
                .ok_or(AdmError::InvalidAdmBwf("audioChannelFormat lacks name"))?
        );
        if stream.attribute("audioStreamFormatName") != Some(expected_name.as_str()) {
            return Err(AdmError::InvalidAdmBwf(
                "audioStreamFormatName does not match its channel",
            ));
        }
    }
    for track in audio_format_extended
        .children()
        .filter(|node| node.is_element() && node.tag_name().name() == "audioTrackFormat")
    {
        if track.attribute("formatDefinition") != Some("PCM")
            || track.attribute("formatLabel") != Some("0001")
        {
            return Err(AdmError::InvalidAdmBwf("audioTrackFormat is not PCM"));
        }
        let stream_refs = require_child_refs(track, "audioStreamFormatIDRef", &streams)?;
        if stream_refs.len() != 1 {
            return Err(AdmError::InvalidAdmBwf(
                "audioTrackFormat must reference exactly one stream",
            ));
        }
        let stream = find_adm_element(
            audio_format_extended,
            "audioStreamFormat",
            "audioStreamFormatID",
            &stream_refs[0],
        )
        .ok_or(AdmError::InvalidAdmBwf("audioTrack stream is missing"))?;
        if track.attribute("audioTrackFormatName") != stream.attribute("audioStreamFormatName") {
            return Err(AdmError::InvalidAdmBwf(
                "audioTrackFormatName does not match its stream",
            ));
        }
    }

    let mut uid_links = HashMap::with_capacity(track_uids.len());
    for uid in audio_format_extended
        .children()
        .filter(|node| node.is_element() && node.tag_name().name() == "audioTrackUID")
    {
        let id = uid
            .attribute("UID")
            .ok_or(AdmError::InvalidAdmBwf("audioTrackUID lacks UID"))?;
        if uid.attribute("trackIndex").is_some()
            || uid.attribute("audioPackFormatIDRef").is_some()
            || uid.attribute("audioChannelFormatIDRef").is_some()
        {
            return Err(AdmError::InvalidAdmBwf(
                "audioTrackUID relationships must be XML sub-elements",
            ));
        }
        if uid
            .attribute("sampleRate")
            .and_then(|value| value.parse::<u32>().ok())
            != Some(sample_rate)
            || uid.attribute("bitDepth") != Some("24")
        {
            return Err(AdmError::InvalidAdmBwf(
                "audioTrackUID PCM attributes disagree with fmt",
            ));
        }
        let track_refs = child_ref_values(uid, "audioTrackFormatIDRef");
        let channel_refs = child_ref_values(uid, "audioChannelFormatIDRef");
        if track_refs.len() + channel_refs.len() != 1
            || track_refs
                .iter()
                .any(|reference| !track_formats.contains(*reference))
            || channel_refs
                .iter()
                .any(|reference| !channels.contains(*reference))
        {
            return Err(AdmError::InvalidAdmBwf(
                "audioTrackUID must reference one track/channel format",
            ));
        }
        let pack_refs = require_child_refs(uid, "audioPackFormatIDRef", &packs)?;
        if pack_refs.len() != 1 {
            return Err(AdmError::InvalidAdmBwf(
                "audioTrackUID must reference one pack format",
            ));
        }
        let signal_ref = track_refs
            .first()
            .copied()
            .or_else(|| channel_refs.first().copied())
            .ok_or(AdmError::InvalidAdmBwf(
                "missing audioTrackUID signal reference",
            ))?;
        uid_links.insert(id.to_owned(), (signal_ref.to_owned(), pack_refs[0].clone()));
    }
    if uid_links.len() != chna.len() {
        return Err(AdmError::InvalidAdmBwf("axml/chna UID count mismatch"));
    }
    for record in chna {
        let Some((track_ref, pack_ref)) = uid_links.get(&record.uid) else {
            return Err(AdmError::InvalidAdmBwf("chna UID is absent from axml"));
        };
        if track_ref != &record.track_ref || pack_ref != &record.pack_ref {
            return Err(AdmError::InvalidAdmBwf(
                "chna references disagree with audioTrackUID",
            ));
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum AdmCounterKind {
    Content,
    Format,
    TrackUid,
}

fn validate_contiguous_ids(
    ids: &HashSet<String>,
    first: u32,
    kind: AdmCounterKind,
) -> Result<(), AdmError> {
    let mut counters = Vec::with_capacity(ids.len());
    for id in ids {
        let digits = match kind {
            AdmCounterKind::Content => id.get(4..8),
            AdmCounterKind::Format => id.get(7..11),
            AdmCounterKind::TrackUid => id.get(4..12),
        }
        .ok_or(AdmError::InvalidAdmBwf("ADM counter field is truncated"))?;
        counters.push(
            u32::from_str_radix(digits, 16)
                .map_err(|_| AdmError::InvalidAdmBwf("ADM counter is not hexadecimal"))?,
        );
    }
    counters.sort_unstable();
    for (index, counter) in counters.into_iter().enumerate() {
        let expected = first
            .checked_add(u32::try_from(index).map_err(|_| AdmError::SizeOverflow)?)
            .ok_or(AdmError::SizeOverflow)?;
        if counter != expected {
            return Err(AdmError::InvalidAdmBwf(
                "ADM IDs are not continuous from the Dolby profile start value",
            ));
        }
    }
    Ok(())
}

fn find_adm_element<'a, 'input>(
    scope: roxmltree::Node<'a, 'input>,
    element_name: &str,
    attribute_name: &str,
    id: &str,
) -> Option<roxmltree::Node<'a, 'input>> {
    scope.children().find(|node| {
        node.is_element()
            && node.tag_name().name() == element_name
            && node.attribute(attribute_name) == Some(id)
    })
}

#[allow(clippy::items_after_statements)]
fn validate_dolby_bed_configuration(
    scope: roxmltree::Node<'_, '_>,
    channel_refs: &[String],
) -> Result<(), AdmError> {
    let mut labels = Vec::with_capacity(channel_refs.len());
    for channel_ref in channel_refs {
        let channel = find_adm_element(
            scope,
            "audioChannelFormat",
            "audioChannelFormatID",
            channel_ref,
        )
        .ok_or(AdmError::InvalidAdmBwf("bed channel is missing"))?;
        let block = channel
            .children()
            .find(|node| node.is_element() && node.tag_name().name() == "audioBlockFormat")
            .ok_or(AdmError::InvalidAdmBwf("bed channel block is missing"))?;
        let speaker_labels = child_ref_values(block, "speakerLabel");
        if speaker_labels.len() != 1 {
            return Err(AdmError::InvalidAdmBwf(
                "bed channel speaker label is missing",
            ));
        }
        labels.push(speaker_labels[0]);
    }
    const ALLOWED: [&[&str]; 8] = [
        &["RC_L", "RC_R"],
        &["RC_L", "RC_R", "RC_C"],
        &["RC_L", "RC_R", "RC_C", "RC_Ls", "RC_Rs"],
        &["RC_L", "RC_R", "RC_C", "RC_LFE", "RC_Ls", "RC_Rs"],
        &[
            "RC_L", "RC_R", "RC_C", "RC_Lss", "RC_Rss", "RC_Lrs", "RC_Rrs",
        ],
        &[
            "RC_L", "RC_R", "RC_C", "RC_LFE", "RC_Lss", "RC_Rss", "RC_Lrs", "RC_Rrs",
        ],
        &[
            "RC_L", "RC_R", "RC_C", "RC_Lss", "RC_Rss", "RC_Lrs", "RC_Rrs", "RC_Lts", "RC_Rts",
        ],
        &[
            "RC_L", "RC_R", "RC_C", "RC_LFE", "RC_Lss", "RC_Rss", "RC_Lrs", "RC_Rrs", "RC_Lts",
            "RC_Rts",
        ],
    ];
    if ALLOWED.iter().any(|configuration| *configuration == labels) {
        Ok(())
    } else {
        Err(AdmError::InvalidAdmBwf(
            "unsupported Dolby DirectSpeakers bed configuration/order",
        ))
    }
}

fn validate_dolby_direct_speaker_block(
    channel: roxmltree::Node<'_, '_>,
    block: roxmltree::Node<'_, '_>,
    label: &str,
) -> bool {
    let definition = match label {
        "RC_L" => ("RoomCentricLeft", -1.0, 1.0, 0.0),
        "RC_R" => ("RoomCentricRight", 1.0, 1.0, 0.0),
        "RC_C" => ("RoomCentricCenter", 0.0, 1.0, 0.0),
        "RC_LFE" => ("RoomCentricLFE", -1.0, 1.0, -1.0),
        "RC_Lss" => ("RoomCentricLeftSideSurround", -1.0, 0.0, 0.0),
        "RC_Rss" => ("RoomCentricRightSideSurround", 1.0, 0.0, 0.0),
        "RC_Lrs" => ("RoomCentricLeftRearSurround", -1.0, -1.0, 0.0),
        "RC_Rrs" => ("RoomCentricRightRearSurround", 1.0, -1.0, 0.0),
        "RC_Lts" => ("RoomCentricLeftTopSurround", -1.0, 0.0, 1.0),
        "RC_Rts" => ("RoomCentricRightTopSurround", 1.0, 0.0, 1.0),
        "RC_Ls" => ("RoomCentricLeftSurround", -1.0, -1.0, 0.0),
        "RC_Rs" => ("RoomCentricRightSurround", 1.0, -1.0, 0.0),
        _ => return false,
    };
    if channel.attribute("audioChannelFormatName") != Some(definition.0)
        || child_ref_values(block, "cartesian") != ["1"]
    {
        return false;
    }
    [
        ("X", definition.1),
        ("Y", definition.2),
        ("Z", definition.3),
    ]
    .into_iter()
    .all(|(coordinate, expected)| {
        position_value(block, coordinate).map_or(expected == 0.0, |value| {
            (value - expected).abs() <= f64::EPSILON
        })
    })
}

fn position_value(block: roxmltree::Node<'_, '_>, coordinate: &str) -> Option<f64> {
    block
        .children()
        .find(|node| {
            node.is_element()
                && node.tag_name().name() == "position"
                && node.attribute("coordinate") == Some(coordinate)
        })
        .and_then(|node| node.text())
        .and_then(|value| value.trim().parse::<f64>().ok())
}

fn has_dolby_jump_position(block: roxmltree::Node<'_, '_>, block_index: usize) -> bool {
    let jumps: Vec<_> = block
        .children()
        .filter(|node| node.is_element() && node.tag_name().name() == "jumpPosition")
        .collect();
    let expected_interpolation_length = if block_index == 0 {
        0.0
    } else {
        DOLBY_SUBSEQUENT_JUMP_INTERPOLATION_SAMPLES as f64
    };
    jumps.len() == 1
        && jumps[0].text().map(str::trim) == Some("1")
        && jumps[0]
            .attribute("interpolationLength")
            .and_then(|value| value.parse::<f64>().ok())
            == Some(expected_interpolation_length)
}

fn same_format_counter(left: &str, right: &str) -> bool {
    left.get(3..11)
        .zip(right.get(3..11))
        .is_some_and(|(left, right)| left.eq_ignore_ascii_case(right))
}

fn parse_adm_time_seconds(value: &str) -> Option<f64> {
    if let Some(seconds) = value
        .strip_prefix("PT")
        .and_then(|value| value.strip_suffix('S'))
    {
        return seconds
            .parse::<f64>()
            .ok()
            .filter(|seconds| *seconds >= 0.0);
    }
    let mut fields = value.split(':');
    let hours = fields.next()?.parse::<u64>().ok()?;
    let minutes = fields.next()?.parse::<u8>().ok()?;
    let seconds = fields.next()?.parse::<f64>().ok()?;
    if fields.next().is_some() || minutes >= 60 || !(0.0..60.0).contains(&seconds) {
        return None;
    }
    Some(hours as f64 * 3_600.0 + f64::from(minutes) * 60.0 + seconds)
}

const fn duration_tolerance(sample_rate: u32) -> f64 {
    0.000_01 + 1.0 / sample_rate as f64
}

fn require_type_consistency(
    element: roxmltree::Node<'_, '_>,
    id_attribute: &str,
) -> Result<(), AdmError> {
    let id = element
        .attribute(id_attribute)
        .ok_or(AdmError::InvalidAdmBwf("typed ADM element lacks ID"))?;
    let label = element
        .attribute("typeLabel")
        .ok_or(AdmError::InvalidAdmBwf("typed ADM element lacks typeLabel"))?;
    let definition = element
        .attribute("typeDefinition")
        .ok_or(AdmError::InvalidAdmBwf(
            "typed ADM element lacks typeDefinition",
        ))?;
    if !id[3..7].eq_ignore_ascii_case(label)
        || !matches!(
            (label, definition),
            ("0001", "DirectSpeakers") | ("0003", "Objects")
        )
    {
        return Err(AdmError::InvalidAdmBwf(
            "ADM typeLabel/typeDefinition/ID mismatch",
        ));
    }
    Ok(())
}

fn is_adm_time(value: &str) -> bool {
    if let Some(seconds) = value
        .strip_prefix("PT")
        .and_then(|value| value.strip_suffix('S'))
    {
        return seconds.parse::<f64>().is_ok_and(|seconds| seconds >= 0.0);
    }
    let mut fields = value.split(':');
    let Some(hours) = fields.next() else {
        return false;
    };
    let Some(minutes) = fields.next() else {
        return false;
    };
    let Some(seconds) = fields.next() else {
        return false;
    };
    fields.next().is_none()
        && hours.len() >= 2
        && hours.bytes().all(|byte| byte.is_ascii_digit())
        && minutes.len() == 2
        && minutes.parse::<u8>().is_ok_and(|minutes| minutes < 60)
        && seconds
            .parse::<f64>()
            .is_ok_and(|seconds| (0.0..60.0).contains(&seconds))
}

fn read_u64(bytes: &[u8]) -> Result<u64, AdmError> {
    Ok(u64::from_le_bytes(bytes.try_into().map_err(|_| {
        AdmError::InvalidAdmBwf("invalid 64-bit field")
    })?))
}

fn read_u16_at(bytes: &[u8], offset: usize) -> Result<u16, AdmError> {
    let end = offset.checked_add(2).ok_or(AdmError::SizeOverflow)?;
    Ok(u16::from_le_bytes(
        bytes
            .get(offset..end)
            .ok_or(AdmError::InvalidAdmBwf("invalid 16-bit field"))?
            .try_into()
            .map_err(|_| AdmError::InvalidAdmBwf("invalid 16-bit field"))?,
    ))
}

fn read_u32_at(bytes: &[u8], offset: usize) -> Result<u32, AdmError> {
    let end = offset.checked_add(4).ok_or(AdmError::SizeOverflow)?;
    Ok(u32::from_le_bytes(
        bytes
            .get(offset..end)
            .ok_or(AdmError::InvalidAdmBwf("invalid 32-bit field"))?
            .try_into()
            .map_err(|_| AdmError::InvalidAdmBwf("invalid 32-bit field"))?,
    ))
}

fn read_exact_adm<R: Read>(
    reader: &mut R,
    output: &mut [u8],
    detail: &'static str,
) -> Result<(), AdmError> {
    reader
        .read_exact(output)
        .map_err(|error| match error.kind() {
            io::ErrorKind::UnexpectedEof => AdmError::InvalidAdmBwf(detail),
            _ => AdmError::Io(error),
        })
}

fn read_bounded_chunk<R: Read + Seek>(
    reader: &mut R,
    offset: u64,
    size: u64,
    limit: u64,
) -> Result<Vec<u8>, AdmError> {
    if size > limit {
        return Err(AdmError::InvalidAdmBwf(
            "metadata chunk exceeds validation limit",
        ));
    }
    let length = usize::try_from(size).map_err(|_| AdmError::SizeOverflow)?;
    let mut payload = vec![0_u8; length];
    reader.seek(SeekFrom::Start(offset))?;
    read_exact_adm(reader, &mut payload, "truncated metadata chunk")?;
    Ok(payload)
}

fn format_time(samples: u64, sample_rate: u32) -> String {
    let rate = u64::from(sample_rate);
    let whole = samples / rate;
    let remainder = samples % rate;
    let fractional = remainder.saturating_mul(1_000_000_000_000) / rate;
    if fractional == 0 {
        format!("PT{whole}S")
    } else {
        format!("PT{whole}.{fractional:012}S")
    }
}

fn format_adm_time(samples: u64, sample_rate: u32) -> String {
    let rate = u128::from(sample_rate);
    let units = u128::from(samples)
        .saturating_mul(100_000)
        .saturating_add(rate / 2)
        / rate;
    let whole_seconds = units / 100_000;
    let fraction = units % 100_000;
    let hours = whole_seconds / 3_600;
    let minutes = (whole_seconds / 60) % 60;
    let seconds = whole_seconds % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}.{fraction:05}")
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use openjoc_joc::ReconstructionBasis;
    use openjoc_scene::{IsfLabel, IsfRing, MetadataObject, ObjectClass, Position3, SpeakerLabel};
    use std::io::Cursor;

    fn scene() -> ObjectScene {
        ObjectScene {
            sample_rate: 48_000,
            duration_samples: 4,
            objects: vec![MetadataObject {
                object_id: 0,
                class: ObjectClass::Dynamic,
            }],
            metadata_timeline: Vec::new(),
            trim_timeline: Vec::new(),
            reconstruction_basis: Some(ReconstructionBasis {
                rows: vec![vec![-1.0, -0.25, 0.25, 1.0]],
            }),
            base_lfe_pcm: Some(vec![0.0, 0.1, -0.1, 0.0]),
            semantic_binding: SemanticBindingState::Unresolved,
        }
    }

    #[test]
    fn oamd_center_front_is_converted_to_adm_center_front() {
        let position = Position::Room(Position3 {
            x: 0.5,
            y: 0.0,
            z: 0.0,
        });

        assert_eq!(
            position_for_adm(&position).expect("valid OAMD position"),
            AdmCartesianPosition {
                x: 0.0,
                y: 1.0,
                z: 0.0,
            }
        );
    }

    #[test]
    fn position_for_adm_converts_boundary_and_screen_room_positions() {
        let boundary = Position::RoomAtInfinity {
            boundary_intersection: Position3 {
                x: 0.0,
                y: 1.0,
                z: 0.0,
            },
        };
        assert_eq!(
            position_for_adm(&boundary).expect("valid boundary position"),
            AdmCartesianPosition {
                x: -1.0,
                y: -1.0,
                z: 0.0,
            }
        );

        let screen = Position::Screen {
            coded: Position3 {
                x: 0.25,
                y: 0.5,
                z: 0.75,
            },
            interpolated_room: Position3 {
                x: 0.5,
                y: 0.25,
                z: 0.4,
            },
        };
        assert_eq!(
            position_for_adm(&screen).expect("valid interpolated room position"),
            AdmCartesianPosition {
                x: 0.0,
                y: 0.5,
                z: 0.4,
            }
        );
    }

    #[test]
    fn position_for_adm_rejects_speaker_and_intermediate_spatial_positions() {
        for position in [
            Position::Speaker(SpeakerLabel::RcTfl),
            Position::IntermediateSpatial(IsfLabel {
                ring: IsfRing::Upper,
                index: 2,
            }),
        ] {
            assert!(matches!(
                position_for_adm(&position),
                Err(AdmError::UnsupportedDynamicMetadata(_))
            ));
        }
    }

    #[test]
    fn streaming_writer_is_byte_identical_to_the_in_memory_oracle() {
        let scene = scene();
        let mut legacy = Cursor::new(Vec::new());
        write_adm_bwf_legacy_inner(&mut legacy, &scene, AdmPolicy::BestEffort)
            .expect("in-memory oracle");

        let plan = AdmExportPlan::from_scene(&scene, AdmPolicy::BestEffort).expect("plan");
        let mut writer = StreamingAdmWriter::new(Cursor::new(Vec::new()), plan).expect("writer");
        writer
            .write_pcm(&[vec![-1.0, -0.25]], Some(&[0.0, 0.1]))
            .expect("first bounded chunk");
        writer
            .write_pcm(&[vec![0.25, 1.0]], Some(&[-0.1, 0.0]))
            .expect("second bounded chunk");
        let (streaming, _, stats) = writer.finish().expect("finish");

        assert_eq!(legacy.into_inner(), streaming.into_inner());
        assert_eq!(stats.max_chunk_frames, 2);
        assert_eq!(stats.max_live_input_samples, 4);
        assert_eq!(stats.max_interleaved_bytes, 42);
    }

    #[test]
    fn validator_seeks_over_virtual_large_pcm_without_reading_it() {
        let duration = 100_000_000_u64;
        let plan = AdmExportPlan::new(
            48_000,
            duration,
            1,
            false,
            1,
            1,
            SemanticBindingState::Unresolved,
            AdmPolicy::BestEffort,
        )
        .expect("large plan");
        assert_eq!(plan.data_bytes, 300_000_000);
        let mut header = Cursor::new(Vec::new());
        write_adm_bwf_header(&mut header, &plan).expect("header");
        let prefix = header.into_inner();
        let suffix_offset = u64::try_from(prefix.len())
            .expect("prefix length")
            .checked_add(plan.data_bytes)
            .and_then(|offset| offset.checked_add(plan.data_bytes % 2))
            .expect("suffix offset");
        let mut suffix = Cursor::new(Vec::new());
        write_adm_metadata(&mut suffix, &plan).expect("metadata suffix");
        let mut reader =
            VirtualReader::new(prefix, suffix.into_inner(), suffix_offset, plan.total_size);
        let summary = validate_reader(&mut reader).expect("streaming validation");
        assert_eq!(summary.data_bytes, 300_000_000);
        assert!(
            reader.bytes_read < 128 * 1024,
            "read {} bytes",
            reader.bytes_read
        );
    }

    #[test]
    fn writer_selects_and_validator_checks_rf64_beyond_riff_limits() {
        let duration = 1_500_000_000_u64;
        let plan = AdmExportPlan::new(
            48_000,
            duration,
            1,
            false,
            1,
            1,
            SemanticBindingState::Unresolved,
            AdmPolicy::BestEffort,
        )
        .expect("RF64 plan");
        assert_eq!(plan.container(), AdmContainer::Rf64);
        assert_eq!(plan.data_bytes, 4_500_000_000);
        let mut header = Cursor::new(Vec::new());
        write_adm_bwf_header(&mut header, &plan).expect("RF64 header");
        let prefix = header.into_inner();
        assert_eq!(&prefix[0..4], b"RF64");
        assert_eq!(&prefix[12..16], b"ds64");
        let suffix_offset = u64::try_from(prefix.len())
            .expect("prefix length")
            .checked_add(plan.data_bytes)
            .and_then(|offset| offset.checked_add(plan.data_bytes % 2))
            .expect("suffix offset");
        let mut suffix = Cursor::new(Vec::new());
        write_adm_metadata(&mut suffix, &plan).expect("metadata suffix");
        let mut reader =
            VirtualReader::new(prefix, suffix.into_inner(), suffix_offset, plan.total_size);
        let summary = validate_reader(&mut reader).expect("validate virtual RF64");
        assert_eq!(summary.container, "RF64");
        assert_eq!(summary.data_bytes, 4_500_000_000);
        assert!(reader.bytes_read < 128 * 1024);
    }

    #[test]
    fn validator_rejects_legacy_attribute_based_uid_relationships() {
        let plan = AdmExportPlan::new(
            48_000,
            48_000,
            1,
            false,
            0,
            0,
            SemanticBindingState::Unresolved,
            AdmPolicy::BestEffort,
        )
        .expect("plan");
        let legacy_xml = plan.xml.replacen(
            "sampleRate=\"48000\"",
            "trackIndex=\"1\" sampleRate=\"48000\"",
            1,
        );
        let chna = chna_payload(&plan.tracks).expect("chna");
        let (_, records) = parse_chna(&chna, 1).expect("parse chna");
        assert!(matches!(
            validate_adm_xml(legacy_xml.as_bytes(), 48_000, 48_000, &records),
            Err(AdmError::InvalidAdmBwf(
                "audioTrackUID relationships must be XML sub-elements"
            ))
        ));
    }

    #[test]
    fn validator_rejects_object_id_from_the_dolby_bed_range() {
        let plan = AdmExportPlan::new(
            48_000,
            48_000,
            1,
            false,
            0,
            0,
            SemanticBindingState::Unresolved,
            AdmPolicy::BestEffort,
        )
        .expect("plan");
        let invalid_xml = plan.xml.replace("AO_100B", "AO_1001");
        let chna = chna_payload(&plan.tracks).expect("chna");
        let (_, records) = parse_chna(&chna, 1).expect("parse chna");
        assert!(matches!(
            validate_adm_xml(invalid_xml.as_bytes(), 48_000, 48_000, &records),
            Err(AdmError::InvalidAdmBwf(
                "audioObject ID is outside its Dolby bed/object range"
            ))
        ));
    }

    #[test]
    fn tenfold_duration_does_not_change_pcm_staging_high_watermark() {
        const OLD_CAPTURE_CHANNELS: u64 = 6 + 5 + 1;
        const SHORT_DURATION: u64 = 1_500_000;
        const {
            assert!(
                SHORT_DURATION * OLD_CAPTURE_CHANNELS * 8 > 128 * 1024 * 1024,
                "logical stream must exceed the former diagnostic capture limit"
            );
        }
        let short = write_counted_stream(SHORT_DURATION);
        let long = write_counted_stream(SHORT_DURATION * 10);
        assert_eq!(short.max_chunk_frames, long.max_chunk_frames);
        assert_eq!(short.max_live_input_samples, long.max_live_input_samples);
        assert_eq!(short.max_interleaved_bytes, long.max_interleaved_bytes);
        assert_eq!(short.max_chunk_frames, 1536);
    }

    #[test]
    fn streaming_writer_rejects_out_of_range_without_clipping() {
        let plan = AdmExportPlan::new(
            48_000,
            1,
            1,
            false,
            1,
            1,
            SemanticBindingState::Unresolved,
            AdmPolicy::BestEffort,
        )
        .expect("plan");
        let mut writer = StreamingAdmWriter::new(Cursor::new(Vec::new()), plan).expect("writer");
        assert!(matches!(
            writer.write_pcm(&[vec![1.000_001]], None),
            Err(AdmError::SampleOutOfRange { .. })
        ));
        assert_eq!(writer.pcm_headroom_census.out_of_range_samples, 1);
        assert_eq!(
            writer
                .pcm_headroom_census
                .first_out_of_range
                .as_ref()
                .expect("first out-of-range sample")
                .value,
            1.000_001
        );
    }

    #[test]
    fn streaming_writer_reports_prequantization_headroom_per_signal() {
        let plan = AdmExportPlan::new(
            48_000,
            3,
            1,
            true,
            1,
            2,
            SemanticBindingState::Unresolved,
            AdmPolicy::BestEffort,
        )
        .expect("plan");
        let mut writer = StreamingAdmWriter::new(Cursor::new(Vec::new()), plan).expect("writer");
        writer
            .write_pcm(&[vec![-0.25, 0.75, -1.0]], Some(&[0.0, 0.5, -0.5]))
            .expect("bounded PCM");

        let (_, report, stats) = writer.finish().expect("finish");
        let census = report
            .pcm_headroom_census
            .expect("headroom census in report");
        assert_eq!(census.total_samples, 6);
        assert_eq!(census.finite_samples, 6);
        assert_eq!(census.non_finite_samples, 0);
        assert_eq!(census.out_of_range_samples, 0);
        assert_eq!(census.samples_above_one, 0);
        assert_eq!(census.samples_below_negative_one, 0);
        assert_eq!(census.max_positive, Some(0.75));
        assert_eq!(census.min_negative, Some(-1.0));
        assert_eq!(census.longest_out_of_range_run, 0);
        assert_eq!(census.peak_abs, 1.0);
        assert_eq!(census.peak_value, -1.0);
        assert_eq!(census.peak_sample, Some(2));
        let base_lfe = census.base_lfe.as_ref().expect("base LFE");
        assert_eq!(base_lfe.max_positive, Some(0.5));
        assert_eq!(base_lfe.min_negative, Some(-0.5));
        assert_eq!(base_lfe.peak_abs, 0.5);
        assert_eq!(census.reconstruction[0].peak_abs, 1.0);
        assert_eq!(
            stats
                .pcm_headroom_census
                .expect("headroom census in stats")
                .total_samples,
            6
        );
    }

    #[test]
    fn streaming_writer_records_negative_headroom_before_rejecting_it() {
        let plan = AdmExportPlan::new(
            48_000,
            2,
            1,
            false,
            1,
            1,
            SemanticBindingState::Unresolved,
            AdmPolicy::BestEffort,
        )
        .expect("plan");
        let mut writer = StreamingAdmWriter::new(Cursor::new(Vec::new()), plan).expect("writer");
        let error = writer
            .write_pcm(&[vec![-1.25, -1.5]], None)
            .expect_err("negative out-of-range sample");
        assert!(matches!(error, AdmError::SampleOutOfRange { .. }));
        assert_eq!(writer.pcm_headroom_census.out_of_range_samples, 1);
        assert_eq!(writer.pcm_headroom_census.samples_below_negative_one, 1);
        assert_eq!(writer.pcm_headroom_census.max_positive, None);
        assert_eq!(writer.pcm_headroom_census.min_negative, Some(-1.25));
        assert_eq!(writer.pcm_headroom_census.longest_out_of_range_run, 1);
    }

    fn write_counted_stream(duration: u64) -> AdmStreamingStats {
        let plan = AdmExportPlan::new(
            48_000,
            duration,
            1,
            false,
            1,
            1,
            SemanticBindingState::Unresolved,
            AdmPolicy::BestEffort,
        )
        .expect("plan");
        let mut writer = StreamingAdmWriter::new(CountingSink::default(), plan).expect("writer");
        let mut remaining = duration;
        while remaining > 0 {
            let frames = usize::try_from(remaining.min(1536)).expect("bounded frames");
            writer
                .write_pcm(&[vec![0.0; frames]], None)
                .expect("bounded PCM chunk");
            remaining -= u64::try_from(frames).expect("bounded frames");
        }
        let (_, _, stats) = writer.finish().expect("finish");
        stats
    }

    #[derive(Default)]
    struct CountingSink {
        position: u64,
        length: u64,
    }

    impl Write for CountingSink {
        fn write(&mut self, input: &[u8]) -> io::Result<usize> {
            let length = u64::try_from(input.len()).map_err(io::Error::other)?;
            self.position = self
                .position
                .checked_add(length)
                .ok_or_else(|| io::Error::other("position overflow"))?;
            self.length = self.length.max(self.position);
            Ok(input.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl Seek for CountingSink {
        fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
            let next = match position {
                SeekFrom::Start(value) => i128::from(value),
                SeekFrom::Current(value) => i128::from(self.position) + i128::from(value),
                SeekFrom::End(value) => i128::from(self.length) + i128::from(value),
            };
            self.position = u64::try_from(next)
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid seek"))?;
            Ok(self.position)
        }
    }

    struct VirtualReader {
        prefix: Vec<u8>,
        suffix: Vec<u8>,
        suffix_offset: u64,
        length: u64,
        position: u64,
        bytes_read: usize,
    }

    impl VirtualReader {
        fn new(prefix: Vec<u8>, suffix: Vec<u8>, suffix_offset: u64, length: u64) -> Self {
            Self {
                prefix,
                suffix,
                suffix_offset,
                length,
                position: 0,
                bytes_read: 0,
            }
        }
    }

    impl Read for VirtualReader {
        fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
            if self.position >= self.length {
                return Ok(0);
            }
            let remaining = usize::try_from((self.length - self.position).min(output.len() as u64))
                .map_err(io::Error::other)?;
            output[..remaining].fill(0);
            copy_virtual_region(&self.prefix, 0, self.position, &mut output[..remaining])?;
            copy_virtual_region(
                &self.suffix,
                self.suffix_offset,
                self.position,
                &mut output[..remaining],
            )?;
            self.position += u64::try_from(remaining).map_err(io::Error::other)?;
            self.bytes_read += remaining;
            Ok(remaining)
        }
    }

    fn copy_virtual_region(
        source: &[u8],
        source_offset: u64,
        read_offset: u64,
        output: &mut [u8],
    ) -> io::Result<()> {
        let read_end = read_offset
            .checked_add(u64::try_from(output.len()).map_err(io::Error::other)?)
            .ok_or_else(|| io::Error::other("virtual read overflow"))?;
        let source_end = source_offset
            .checked_add(u64::try_from(source.len()).map_err(io::Error::other)?)
            .ok_or_else(|| io::Error::other("virtual source overflow"))?;
        let start = read_offset.max(source_offset);
        let end = read_end.min(source_end);
        if start < end {
            let output_start = usize::try_from(start - read_offset).map_err(io::Error::other)?;
            let source_start = usize::try_from(start - source_offset).map_err(io::Error::other)?;
            let count = usize::try_from(end - start).map_err(io::Error::other)?;
            output[output_start..output_start + count]
                .copy_from_slice(&source[source_start..source_start + count]);
        }
        Ok(())
    }

    impl Seek for VirtualReader {
        fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
            let next = match position {
                SeekFrom::Start(value) => i128::from(value),
                SeekFrom::Current(value) => i128::from(self.position) + i128::from(value),
                SeekFrom::End(value) => i128::from(self.length) + i128::from(value),
            };
            self.position = u64::try_from(next)
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid seek"))?;
            Ok(self.position)
        }
    }
}
