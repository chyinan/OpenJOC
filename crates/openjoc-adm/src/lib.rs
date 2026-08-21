//! Reconstructed ADM/BW64 export for the renderer-independent OpenJOC scene.
//!
//! This crate intentionally does not infer an authored-object/audio-row
//! relationship. `ObjectScene` currently records that relation as unresolved,
//! so the export contains deterministic neutral reconstruction signals and a
//! machine-readable report rather than inventing ADM object bindings.

use openjoc_scene::{ObjectClass, ObjectScene, SemanticBindingState};
use serde::Serialize;
use std::fmt::Write as _;
use std::{
    collections::HashSet,
    fmt,
    fs::File,
    io::{self, Read, Seek, SeekFrom, Write},
    path::Path,
};

pub const ADM_SCHEMA: &str = "openjoc.adm-reconstruction.v1";
pub const REPORT_SCHEMA: &str = "openjoc.adm-report.v1";
pub const BW64_STANDARD: &str = "ITU-R BS.2088-2 (11/2025)";
pub const ADM_STANDARD: &str = "ITU-R BS.2076-3 (02/2025)";

const MAX_AXML_BYTES: u64 = 64 * 1024 * 1024;
const MAX_CHNA_BYTES: u64 = 4 * 1024 * 1024;

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

/// The complete mapping table for the 0.9 initial subset.
#[must_use]
pub const fn mapping_table() -> [MappingRecord; 10] {
    [
        MappingRecord {
            semantic: "reconstruction_signal_identity",
            status: MappingStatus::Exact,
            detail: "The ADM track is identified only as a local ReconstructionBasis row.",
        },
        MappingRecord {
            semantic: "audio_to_spatial_metadata_binding",
            status: MappingStatus::Unresolved,
            detail: "ObjectScene records no verified association between a reconstruction row and an OAMD object.",
        },
        MappingRecord {
            semantic: "dynamic_object_position_and_trajectory",
            status: MappingStatus::NotRepresentable,
            detail: "Recovered OAMD updates are retained by OpenJOC but are not attached to PCM in this release.",
        },
        MappingRecord {
            semantic: "bed_and_direct_speaker_identity_for_reconstruction_rows",
            status: MappingStatus::NotRepresentable,
            detail: "A structural row index is not promoted to an authored bed or speaker identity.",
        },
        MappingRecord {
            semantic: "base_lfe_direct_speaker_identity",
            status: MappingStatus::Exact,
            detail: "A separately retained base LFE channel is emitted as ADM LFE1.",
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
#[derive(Clone, Debug, Serialize)]
pub struct AdmExportReport {
    pub schema: &'static str,
    pub openjoc_version: &'static str,
    pub source_format: &'static str,
    pub sample_rate: u32,
    pub duration_samples: u64,
    pub duration_seconds: String,
    pub policy: &'static str,
    pub pcm_format: &'static str,
    pub reconstructed_signal_count: usize,
    pub bed_direct_speaker_count: usize,
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
}

/// Result of building an export in memory.
#[derive(Clone, Debug)]
pub struct AdmExport {
    pub xml: String,
    pub report: AdmExportReport,
}

/// Duration-independent metadata required before a streaming BW64 write.
#[derive(Clone, Debug)]
pub struct AdmExportPlan {
    sample_rate: u32,
    duration_samples: u64,
    tracks: Vec<TrackDescriptor>,
    xml: String,
    report: AdmExportReport,
    data_bytes: u64,
    total_size: u64,
}

/// Deterministic bounded-memory evidence collected by a streaming write.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
pub struct AdmStreamingStats {
    pub frames_written: u64,
    pub chunks_written: u64,
    pub max_chunk_frames: usize,
    pub max_live_input_samples: usize,
    pub max_interleaved_bytes: usize,
}

/// Incremental signed-24-bit BW64 writer.
pub struct StreamingAdmWriter<W: Write + Seek> {
    writer: W,
    plan: AdmExportPlan,
    frames_written: u64,
    interleaved: Vec<u8>,
    stats: AdmStreamingStats,
}

#[derive(Clone, Debug)]
struct TrackDescriptor {
    track_index: u16,
    uid: String,
    channel_id: String,
    pack_id: String,
    direct_speaker: bool,
}

/// Validation summary for a generated BW64 file.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AdmValidationSummary {
    pub container: &'static str,
    pub chunks: Vec<String>,
    pub sample_rate: u32,
    pub channels: usize,
    pub data_bytes: u64,
    pub axml_bytes: u64,
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
    InvalidBw64(&'static str),
}

impl fmt::Display for AdmError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPolicy(value) => write!(formatter, "invalid ADM policy {value:?}"),
            Self::InvalidScene(value) => write!(formatter, "invalid scene for ADM export: {value}"),
            Self::StrictUnresolvedBinding => formatter.write_str(
                "strict ADM export requires a verified audio-to-spatial-metadata binding; current ObjectScene is UNRESOLVED",
            ),
            Self::NoReconstructionSignals => formatter.write_str("scene contains no reconstruction signals"),
            Self::NonFiniteSample { track, sample } => write!(formatter, "non-finite ADM PCM at track {track}, sample {sample}"),
            Self::SampleOutOfRange { track, sample, value } => write!(formatter, "ADM signed 24-bit PCM requires [-1, 1], got {value} at track {track}, sample {sample}"),
            Self::SizeOverflow => formatter.write_str("ADM/BW64 size arithmetic overflow"),
            Self::Io(error) => write!(formatter, "ADM/BW64 I/O error: {error}"),
            Self::InvalidBw64(detail) => write!(formatter, "invalid BW64: {detail}"),
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
        if sample_rate == 0 {
            return Err(AdmError::InvalidScene(
                "ADM sample rate must be non-zero".to_owned(),
            ));
        }
        if reconstruction_signal_count == 0 {
            return Err(AdmError::NoReconstructionSignals);
        }
        if policy == AdmPolicy::Strict && semantic_binding == SemanticBindingState::Unresolved {
            return Err(AdmError::StrictUnresolvedBinding);
        }
        let track_count = reconstruction_signal_count
            .checked_add(usize::from(base_lfe_present))
            .ok_or(AdmError::SizeOverflow)?;
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
        for index in 0..track_count {
            let signal_number = index.checked_add(1).ok_or(AdmError::SizeOverflow)?;
            tracks.push(TrackDescriptor {
                track_index: u16::try_from(signal_number).map_err(|_| AdmError::SizeOverflow)?,
                uid: format!("ATU_{signal_number:08X}"),
                channel_id: format!("AC_{signal_number:08X}_00"),
                pack_id: format!("AP_{signal_number:08X}"),
                direct_speaker: base_lfe_present && index == reconstruction_signal_count,
            });
        }
        let xml = make_xml(sample_rate, duration_samples, &tracks);
        let report = make_report(
            sample_rate,
            duration_samples,
            dynamic_object_count,
            metadata_object_count,
            policy,
            &tracks,
        );
        let axml_len = u64::try_from(xml.len()).map_err(|_| AdmError::SizeOverflow)?;
        let chna_len =
            u64::try_from(chna_payload(&tracks)?.len()).map_err(|_| AdmError::SizeOverflow)?;
        let total_size = bw64_total_size(axml_len, chna_len, data_bytes)?;
        Ok(Self {
            sample_rate,
            duration_samples,
            tracks,
            xml,
            report,
            data_bytes,
            total_size,
        })
    }

    /// Builds a plan for an explicit, already-materialized diagnostic scene.
    pub fn from_scene(scene: &ObjectScene, policy: AdmPolicy) -> Result<Self, AdmError> {
        scene
            .validate()
            .map_err(|error| AdmError::InvalidScene(error.to_string()))?;
        let reconstruction_signal_count = scene
            .reconstruction_basis
            .as_ref()
            .ok_or(AdmError::NoReconstructionSignals)?
            .rows
            .len();
        Self::new(
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
        )
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
}

impl<W: Write + Seek> StreamingAdmWriter<W> {
    /// Opens a BW64 stream and writes every duration-independent chunk.
    pub fn new(mut writer: W, plan: AdmExportPlan) -> Result<Self, AdmError> {
        write_bw64_header(&mut writer, &plan)?;
        Ok(Self {
            writer,
            plan,
            frames_written: 0,
            interleaved: Vec::new(),
            stats: AdmStreamingStats::default(),
        })
    }

    /// Quantizes and interleaves one bounded decoder chunk.
    pub fn write_pcm(
        &mut self,
        reconstruction_rows: &[Vec<f64>],
        base_lfe: Option<&[f64]>,
    ) -> Result<(), AdmError> {
        let plan_has_lfe = self
            .plan
            .tracks
            .last()
            .is_some_and(|track| track.direct_speaker);
        let expected_rows = self.plan.tracks.len() - usize::from(plan_has_lfe);
        if reconstruction_rows.len() != expected_rows || base_lfe.is_some() != plan_has_lfe {
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
            for (track, row) in reconstruction_rows.iter().enumerate() {
                let sample = self
                    .frames_written
                    .checked_add(u64::try_from(frame).map_err(|_| AdmError::SizeOverflow)?)
                    .ok_or(AdmError::SizeOverflow)?;
                self.interleaved
                    .extend_from_slice(&quantize_s24(track, sample, row[frame])?);
            }
            if let Some(lfe) = base_lfe {
                let sample = self
                    .frames_written
                    .checked_add(u64::try_from(frame).map_err(|_| AdmError::SizeOverflow)?)
                    .ok_or(AdmError::SizeOverflow)?;
                self.interleaved.extend_from_slice(&quantize_s24(
                    reconstruction_rows.len(),
                    sample,
                    lfe[frame],
                )?);
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
                .checked_mul(self.plan.tracks.len())
                .ok_or(AdmError::SizeOverflow)?,
        );
        self.stats.max_interleaved_bytes =
            self.stats.max_interleaved_bytes.max(self.interleaved.len());
        Ok(())
    }

    /// Verifies exact per-track duration, pads the data chunk, and flushes.
    pub fn finish(mut self) -> Result<(W, AdmExportReport, AdmStreamingStats), AdmError> {
        if self.frames_written != self.plan.duration_samples {
            return Err(AdmError::InvalidScene(format!(
                "streaming ADM wrote {} samples per track; preflight requires {}",
                self.frames_written, self.plan.duration_samples
            )));
        }
        if self.plan.data_bytes % 2 != 0 {
            self.writer.write_all(&[0])?;
        }
        let actual_size = self.writer.stream_position()?;
        if actual_size != self.plan.total_size {
            return Err(AdmError::InvalidScene(format!(
                "streaming BW64 size {actual_size} differs from planned size {}",
                self.plan.total_size
            )));
        }
        self.writer.flush()?;
        Ok((self.writer, self.plan.report, self.stats))
    }
}

/// Writes a complete BW64 file and returns its deterministic report.
pub fn write_bw64(
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

/// Validates the structural BW64/CHNA subset emitted by this crate.
pub fn validate_bw64(path: &Path) -> Result<AdmValidationSummary, AdmError> {
    let mut file = File::open(path)?;
    validate_reader(&mut file)
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

fn make_report(
    sample_rate: u32,
    duration_samples: u64,
    dynamic_object_count: usize,
    metadata_object_count: usize,
    policy: AdmPolicy,
    tracks: &[TrackDescriptor],
) -> AdmExportReport {
    let bed_direct_speaker_count = tracks.iter().filter(|track| track.direct_speaker).count();
    let generated_signal_identities = tracks
        .iter()
        .enumerate()
        .map(|(index, track)| {
            if track.direct_speaker {
                "OpenJOC Reconstructed Base LFE 01".to_owned()
            } else {
                format!("OpenJOC Reconstructed Signal {:02}", index + 1)
            }
        })
        .collect();
    AdmExportReport {
        schema: REPORT_SCHEMA,
        openjoc_version: env!("CARGO_PKG_VERSION"),
        source_format: "lossy E-AC-3 JOC",
        sample_rate,
        duration_samples,
        duration_seconds: format_time(duration_samples, sample_rate),
        policy: policy.as_str(),
        pcm_format: "signed 24-bit little-endian PCM; no normalization or dynamics processing",
        reconstructed_signal_count: tracks.len(),
        bed_direct_speaker_count,
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
        approximations: vec!["float reconstruction samples quantized to signed 24-bit PCM"],
        omissions: vec![
            "recovered OAMD position/trajectory is not attached to a PCM track while binding is unresolved",
            "extent, channel lock, divergence, zones, and JOC-specific controls are not represented in ADM",
            "FinalLinkedGain, speaker rendering, and HRTF are not applied",
        ],
        warnings: vec![
            "This is a reconstructed interoperability representation, not the original ADM master."
                .to_owned(),
            "Current OpenJOC evidence does not establish the required signal/object association."
                .to_owned(),
        ],
        source_is_lossy_e_ac_3_joc: true,
        original_adm_master_recovered: false,
        lossless_round_trip: false,
        semantic_binding_state: "unresolved",
    }
}

fn make_xml(sample_rate: u32, duration_samples: u64, tracks: &[TrackDescriptor]) -> String {
    let duration = format_time(duration_samples, sample_rate);
    let mut xml = String::new();
    xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    xml.push_str("<ebuCoreMain xmlns=\"urn:ebu:metadata-schema:ebuCore_2011\">\n");
    xml.push_str("  <coreMetadata><format><audioFormatExtended>\n");
    xml.push_str("    <audioProgramme audioProgrammeID=\"APR_00001001\" audioProgrammeName=\"OpenJOC Reconstructed Programme (not original ADM master)\">\n");
    xml.push_str(
        "      <audioContentIDRef>ACO_00001001</audioContentIDRef>\n    </audioProgramme>\n",
    );
    xml.push_str("    <audioContent audioContentID=\"ACO_00001001\" audioContentName=\"OpenJOC Reconstructed Interoperability Representation\">\n");
    for index in 0..tracks.len() {
        let _ = writeln!(
            xml,
            "      <audioObjectIDRef>AO_{:08X}</audioObjectIDRef>",
            index + 1
        );
    }
    xml.push_str("    </audioContent>\n");
    for (index, track) in tracks.iter().enumerate() {
        let number = index + 1;
        let (type_definition, type_label, object_name) = if track.direct_speaker {
            (
                "DirectSpeakers",
                "0001",
                "OpenJOC Reconstructed Base LFE 01".to_owned(),
            )
        } else {
            (
                "Objects",
                "0003",
                format!("OpenJOC Reconstructed Signal {number:02}"),
            )
        };
        let _ = write!(
            xml,
            "    <audioObject audioObjectID=\"AO_{number:08X}\" audioObjectName=\"{}\">\n      <audioPackFormatIDRef>{}</audioPackFormatIDRef>\n      <audioTrackUIDRef>{}</audioTrackUIDRef>\n    </audioObject>\n",
            xml_escape(&object_name),
            track.pack_id,
            track.uid
        );
        let _ = write!(
            xml,
            "    <audioPackFormat audioPackFormatID=\"{}\" audioPackFormatName=\"{}\" typeLabel=\"{}\" typeDefinition=\"{}\">\n      <audioChannelFormatIDRef>{}</audioChannelFormatIDRef>\n    </audioPackFormat>\n",
            track.pack_id,
            xml_escape(&object_name),
            type_label,
            type_definition,
            track.channel_id
        );
        let _ = write!(
            xml,
            "    <audioChannelFormat audioChannelFormatID=\"{}\" audioChannelFormatName=\"{}\" typeLabel=\"{}\" typeDefinition=\"{}\">\n      <audioBlockFormat audioBlockFormatID=\"AB_{number:08X}_01\" rtime=\"PT0S\" duration=\"{}\">\n",
            track.channel_id,
            xml_escape(&object_name),
            type_label,
            type_definition,
            duration
        );
        if track.direct_speaker {
            xml.push_str("        <speakerLabel>LFE1</speakerLabel>\n");
        } else {
            xml.push_str("        <position coordinate=\"cartesian\" X=\"0.000000000000\" Y=\"0.000000000000\" Z=\"0.000000000000\"/>\n");
        }
        xml.push_str("      </audioBlockFormat>\n    </audioChannelFormat>\n");
    }
    for track in tracks {
        let _ = writeln!(
            xml,
            "    <audioTrackUID UID=\"{}\" trackIndex=\"{}\" audioPackFormatIDRef=\"{}\" audioChannelFormatIDRef=\"{}\"/>",
            track.uid, track.track_index, track.pack_id, track.channel_id
        );
    }
    xml.push_str("  </audioFormatExtended></format></coreMetadata>\n</ebuCoreMain>\n");
    xml
}

fn write_bw64_header<W: Write + Seek>(
    writer: &mut W,
    plan: &AdmExportPlan,
) -> Result<(), AdmError> {
    let channels = u16::try_from(plan.tracks.len()).map_err(|_| AdmError::SizeOverflow)?;
    let chna = chna_payload(&plan.tracks)?;
    writer.write_all(b"BW64")?;
    writer.write_all(&u32::MAX.to_le_bytes())?;
    writer.write_all(b"WAVE")?;
    writer.write_all(b"ds64")?;
    writer.write_all(&28_u32.to_le_bytes())?;
    writer.write_all(&plan.total_size.saturating_sub(8).to_le_bytes())?;
    writer.write_all(&plan.data_bytes.to_le_bytes())?;
    writer.write_all(&plan.duration_samples.to_le_bytes())?;
    writer.write_all(&0_u32.to_le_bytes())?;
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
    write_chunk(writer, *b"axml", plan.xml.as_bytes())?;
    write_chunk(writer, *b"chna", &chna)?;
    writer.write_all(b"data")?;
    writer.write_all(&u32::MAX.to_le_bytes())?;
    Ok(())
}

fn bw64_total_size(axml_len: u64, chna_len: u64, data_bytes: u64) -> Result<u64, AdmError> {
    12_u64
        .checked_add(8 + 28)
        .and_then(|value| value.checked_add(8 + 16))
        .and_then(|value| value.checked_add(chunk_total_size(axml_len)))
        .and_then(|value| value.checked_add(chunk_total_size(chna_len)))
        .and_then(|value| value.checked_add(chunk_total_size(data_bytes)))
        .ok_or(AdmError::SizeOverflow)
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
fn write_bw64_legacy_inner<W: Write + Seek>(
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
    write_bw64_header(writer, &plan)?;
    for frame in 0..frames {
        for (track_index, track) in basis.rows.iter().enumerate() {
            writer.write_all(&quantize_s24(
                track_index,
                u64::try_from(frame).map_err(|_| AdmError::SizeOverflow)?,
                track[frame],
            )?)?;
        }
        if let Some(lfe) = &scene.base_lfe_pcm {
            writer.write_all(&quantize_s24(
                basis.rows.len(),
                u64::try_from(frame).map_err(|_| AdmError::SizeOverflow)?,
                lfe[frame],
            )?)?;
        }
    }
    if plan.data_bytes % 2 != 0 {
        writer.write_all(&[0])?;
    }
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
        write_fixed(&mut payload, track.channel_id.as_bytes(), 14)?;
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

/// Validates BW64 incrementally without retaining the programme PCM chunk.
pub fn validate_reader<R: Read + Seek>(reader: &mut R) -> Result<AdmValidationSummary, AdmError> {
    let file_len = reader.seek(SeekFrom::End(0))?;
    reader.seek(SeekFrom::Start(0))?;
    let mut header = [0_u8; 12];
    read_exact_bw64(reader, &mut header, "truncated BW64/WAVE header")?;
    if &header[0..4] != b"BW64" || &header[8..12] != b"WAVE" {
        return Err(AdmError::InvalidBw64("missing BW64/WAVE header"));
    }
    let mut cursor = 12_u64;
    let mut chunks = Vec::new();
    let mut ds64 = None;
    let mut fmt = None::<Vec<u8>>;
    let mut data_bytes = None;
    let mut axml = None::<Vec<u8>>;
    let mut chna = None::<Vec<u8>>;
    while cursor < file_len {
        let header_end = cursor.checked_add(8).ok_or(AdmError::SizeOverflow)?;
        if header_end > file_len {
            return Err(AdmError::InvalidBw64("truncated chunk header"));
        }
        reader.seek(SeekFrom::Start(cursor))?;
        let mut chunk_header = [0_u8; 8];
        read_exact_bw64(reader, &mut chunk_header, "truncated chunk header")?;
        let id: [u8; 4] = chunk_header[0..4]
            .try_into()
            .map_err(|_| AdmError::InvalidBw64("chunk identifier"))?;
        let declared_size = u64::from(u32::from_le_bytes(
            chunk_header[4..8]
                .try_into()
                .map_err(|_| AdmError::InvalidBw64("chunk size"))?,
        ));
        let payload_start = header_end;
        let size = if declared_size == u64::from(u32::MAX) && &id == b"data" {
            ds64.map(|value: Ds64| value.data_size)
                .ok_or(AdmError::InvalidBw64("data precedes ds64"))?
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
            return Err(AdmError::InvalidBw64("chunk exceeds file"));
        }
        chunks.push(String::from_utf8_lossy(&id).to_string());
        match &id {
            b"ds64" if cursor == 12 => {
                if !(28..=1024 * 1024).contains(&size) {
                    return Err(AdmError::InvalidBw64("unsupported ds64 payload"));
                }
                let payload = read_bounded_chunk(reader, payload_start, size, 1024 * 1024)?;
                let value = Ds64 {
                    riff_size: read_u64(&payload[0..8])?,
                    data_size: read_u64(&payload[8..16])?,
                    sample_count: read_u64(&payload[16..24])?,
                };
                if value.riff_size.checked_add(8) != Some(file_len) {
                    return Err(AdmError::InvalidBw64("ds64 RIFF size disagrees with file"));
                }
                ds64 = Some(value);
            }
            b"fmt " => fmt = Some(read_bounded_chunk(reader, payload_start, size, 64)?),
            b"data" => data_bytes = Some(size),
            b"axml" => {
                axml = Some(read_bounded_chunk(
                    reader,
                    payload_start,
                    size,
                    MAX_AXML_BYTES,
                )?);
            }
            b"chna" => {
                chna = Some(read_bounded_chunk(
                    reader,
                    payload_start,
                    size,
                    MAX_CHNA_BYTES,
                )?);
            }
            _ => {}
        }
        cursor = padded_end;
    }
    if cursor != file_len {
        return Err(AdmError::InvalidBw64(
            "chunk layout does not end at file boundary",
        ));
    }
    let ds64 = ds64.ok_or(AdmError::InvalidBw64("ds64 is not the first chunk"))?;
    if chunks.first().map(String::as_str) != Some("ds64") {
        return Err(AdmError::InvalidBw64("ds64 is not the first chunk"));
    }
    let fmt = fmt.ok_or(AdmError::InvalidBw64("missing fmt chunk"))?;
    if fmt.len() != 16 || read_u16_at(&fmt, 0)? != 1 {
        return Err(AdmError::InvalidBw64("unsupported fmt payload"));
    }
    let channels = usize::from(read_u16_at(&fmt, 2)?);
    let sample_rate = read_u32_at(&fmt, 4)?;
    let data_bytes = data_bytes.ok_or(AdmError::InvalidBw64("missing data chunk"))?;
    if data_bytes != ds64.data_size {
        return Err(AdmError::InvalidBw64("ds64 data size mismatch"));
    }
    let block_align = u64::try_from(channels)
        .ok()
        .and_then(|value| value.checked_mul(3))
        .ok_or(AdmError::SizeOverflow)?;
    if block_align == 0
        || data_bytes % block_align != 0
        || data_bytes / block_align != ds64.sample_count
    {
        return Err(AdmError::InvalidBw64("PCM size/sample count mismatch"));
    }
    let axml = axml.ok_or(AdmError::InvalidBw64("missing axml chunk"))?;
    let chna = chna.ok_or(AdmError::InvalidBw64("missing chna chunk"))?;
    if chna.len() < 4 || (chna.len() - 4) % 40 != 0 {
        return Err(AdmError::InvalidBw64("invalid chna length"));
    }
    let tracks = usize::from(read_u16_at(&chna, 0)?);
    let uids = usize::from(read_u16_at(&chna, 2)?);
    if tracks != channels || uids != tracks || chna.len() != 4 + tracks * 40 {
        return Err(AdmError::InvalidBw64("chna track/channel mismatch"));
    }
    let mut identifiers = HashSet::new();
    for index in 0..tracks {
        let base = 4 + index * 40;
        let expected_track = u16::try_from(index + 1).map_err(|_| AdmError::SizeOverflow)?;
        if chna
            .get(base..base + 2)
            .ok_or(AdmError::InvalidBw64("truncated chna track index"))?
            != expected_track.to_le_bytes()
        {
            return Err(AdmError::InvalidBw64("chna track index is not ordered"));
        }
        let uid = chna
            .get(base + 2..base + 14)
            .ok_or(AdmError::InvalidBw64("truncated chna UID"))?;
        if !identifiers.insert(uid.to_vec()) {
            return Err(AdmError::InvalidBw64("duplicate chna UID"));
        }
    }
    if !axml.starts_with(b"<?xml")
        || !axml
            .windows(b"audioFormatExtended".len())
            .any(|window| window == b"audioFormatExtended")
    {
        return Err(AdmError::InvalidBw64("invalid ADM XML marker"));
    }
    Ok(AdmValidationSummary {
        container: "BW64",
        chunks,
        sample_rate,
        channels,
        data_bytes,
        axml_bytes: u64::try_from(axml.len()).map_err(|_| AdmError::SizeOverflow)?,
        chna_tracks: tracks,
        chna_uids: uids,
        identifiers_unique: true,
    })
}

#[derive(Clone, Copy)]
struct Ds64 {
    riff_size: u64,
    data_size: u64,
    sample_count: u64,
}

fn read_u64(bytes: &[u8]) -> Result<u64, AdmError> {
    Ok(u64::from_le_bytes(bytes.try_into().map_err(|_| {
        AdmError::InvalidBw64("invalid 64-bit field")
    })?))
}

fn read_u16_at(bytes: &[u8], offset: usize) -> Result<u16, AdmError> {
    let end = offset.checked_add(2).ok_or(AdmError::SizeOverflow)?;
    Ok(u16::from_le_bytes(
        bytes
            .get(offset..end)
            .ok_or(AdmError::InvalidBw64("invalid 16-bit field"))?
            .try_into()
            .map_err(|_| AdmError::InvalidBw64("invalid 16-bit field"))?,
    ))
}

fn read_u32_at(bytes: &[u8], offset: usize) -> Result<u32, AdmError> {
    let end = offset.checked_add(4).ok_or(AdmError::SizeOverflow)?;
    Ok(u32::from_le_bytes(
        bytes
            .get(offset..end)
            .ok_or(AdmError::InvalidBw64("invalid 32-bit field"))?
            .try_into()
            .map_err(|_| AdmError::InvalidBw64("invalid 32-bit field"))?,
    ))
}

fn read_exact_bw64<R: Read>(
    reader: &mut R,
    output: &mut [u8],
    detail: &'static str,
) -> Result<(), AdmError> {
    reader
        .read_exact(output)
        .map_err(|error| match error.kind() {
            io::ErrorKind::UnexpectedEof => AdmError::InvalidBw64(detail),
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
        return Err(AdmError::InvalidBw64(
            "metadata chunk exceeds validation limit",
        ));
    }
    let length = usize::try_from(size).map_err(|_| AdmError::SizeOverflow)?;
    let mut payload = vec![0_u8; length];
    reader.seek(SeekFrom::Start(offset))?;
    read_exact_bw64(reader, &mut payload, "truncated metadata chunk")?;
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
    use openjoc_scene::{MetadataObject, ObjectClass};
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
    fn streaming_writer_is_byte_identical_to_the_in_memory_oracle() {
        let scene = scene();
        let mut legacy = Cursor::new(Vec::new());
        write_bw64_legacy_inner(&mut legacy, &scene, AdmPolicy::BestEffort).expect("legacy oracle");

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
        assert_eq!(stats.max_interleaved_bytes, 12);
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
        write_bw64_header(&mut header, &plan).expect("header");
        let prefix = header.into_inner();
        let mut reader = VirtualReader::new(prefix, plan.total_size);
        let summary = validate_reader(&mut reader).expect("streaming validation");
        assert_eq!(summary.data_bytes, 300_000_000);
        assert!(
            reader.bytes_read < 128 * 1024,
            "read {} bytes",
            reader.bytes_read
        );
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
        length: u64,
        position: u64,
        bytes_read: usize,
    }

    impl VirtualReader {
        fn new(prefix: Vec<u8>, length: u64) -> Self {
            Self {
                prefix,
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
            let prefix_start = usize::try_from(self.position).unwrap_or(usize::MAX);
            let prefix_available = self
                .prefix
                .len()
                .saturating_sub(prefix_start)
                .min(remaining);
            if prefix_available > 0 {
                output[..prefix_available]
                    .copy_from_slice(&self.prefix[prefix_start..prefix_start + prefix_available]);
            }
            output[prefix_available..remaining].fill(0);
            self.position += u64::try_from(remaining).map_err(io::Error::other)?;
            self.bytes_read += remaining;
            Ok(remaining)
        }
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
