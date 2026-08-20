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
    io::{self, Read, Seek, Write},
    path::Path,
};

pub const ADM_SCHEMA: &str = "openjoc.adm-reconstruction.v1";
pub const REPORT_SCHEMA: &str = "openjoc.adm-report.v1";
pub const BW64_STANDARD: &str = "ITU-R BS.2088-2 (11/2025)";
pub const ADM_STANDARD: &str = "ITU-R BS.2076-3 (02/2025)";

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
    tracks: Vec<TrackRecord>,
}

#[derive(Clone, Debug)]
struct TrackRecord {
    track_index: u16,
    uid: String,
    channel_id: String,
    pack_id: String,
    samples: Vec<f64>,
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
        sample: usize,
    },
    SampleOutOfRange {
        track: usize,
        sample: usize,
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
    if policy == AdmPolicy::Strict && scene.semantic_binding == SemanticBindingState::Unresolved {
        return Err(AdmError::StrictUnresolvedBinding);
    }

    let mut tracks =
        Vec::with_capacity(basis.rows.len() + usize::from(scene.base_lfe_pcm.is_some()));
    for (index, samples) in basis.rows.iter().enumerate() {
        validate_samples(index, samples)?;
        let signal_number = index + 1;
        tracks.push(TrackRecord {
            track_index: u16::try_from(signal_number).map_err(|_| AdmError::SizeOverflow)?,
            uid: format!("ATU_{signal_number:08X}"),
            channel_id: format!("AC_{signal_number:08X}_00"),
            pack_id: format!("AP_{signal_number:08X}"),
            samples: samples.clone(),
            direct_speaker: false,
        });
    }
    if let Some(lfe) = &scene.base_lfe_pcm {
        validate_samples(tracks.len(), lfe)?;
        let signal_number = tracks.len() + 1;
        tracks.push(TrackRecord {
            track_index: u16::try_from(signal_number).map_err(|_| AdmError::SizeOverflow)?,
            uid: format!("ATU_{signal_number:08X}"),
            channel_id: format!("AC_{signal_number:08X}_00"),
            pack_id: format!("AP_{signal_number:08X}"),
            samples: lfe.clone(),
            direct_speaker: true,
        });
    }

    let report = make_report(scene, policy, &tracks);
    let xml = make_xml(scene, &tracks);
    Ok(AdmExport {
        xml,
        report,
        tracks,
    })
}

/// Writes a complete BW64 file and returns its deterministic report.
pub fn write_bw64(
    path: &Path,
    scene: &ObjectScene,
    policy: AdmPolicy,
) -> Result<AdmExportReport, AdmError> {
    let export = build_export(scene, policy)?;
    let mut file = File::create(path)?;
    write_bw64_inner(&mut file, &export)?;
    Ok(export.report)
}

/// Validates the structural BW64/CHNA subset emitted by this crate.
pub fn validate_bw64(path: &Path) -> Result<AdmValidationSummary, AdmError> {
    let mut file = File::open(path)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    validate_bytes(&bytes)
}

fn validate_samples(track: usize, samples: &[f64]) -> Result<(), AdmError> {
    for (sample, value) in samples.iter().copied().enumerate() {
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

fn make_report(scene: &ObjectScene, policy: AdmPolicy, tracks: &[TrackRecord]) -> AdmExportReport {
    let dynamic_object_count = scene
        .objects
        .iter()
        .filter(|object| object.class == ObjectClass::Dynamic)
        .count();
    let bed_direct_speaker_count = usize::from(scene.base_lfe_pcm.is_some());
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
        sample_rate: scene.sample_rate,
        duration_samples: scene.duration_samples,
        duration_seconds: format_time(scene.duration_samples, scene.sample_rate),
        policy: policy.as_str(),
        pcm_format: "signed 24-bit little-endian PCM; no normalization or dynamics processing",
        reconstructed_signal_count: tracks.len(),
        bed_direct_speaker_count,
        dynamic_object_count,
        metadata_object_count: scene.objects.len(),
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

fn make_xml(scene: &ObjectScene, tracks: &[TrackRecord]) -> String {
    let duration = format_time(scene.duration_samples, scene.sample_rate);
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

fn write_bw64_inner<W: Write + Seek>(writer: &mut W, export: &AdmExport) -> Result<(), AdmError> {
    let channels = u16::try_from(export.tracks.len()).map_err(|_| AdmError::SizeOverflow)?;
    let sample_rate = export
        .tracks
        .first()
        .map(|track| track.samples.len())
        .ok_or(AdmError::NoReconstructionSignals)?;
    if export
        .tracks
        .iter()
        .any(|track| track.samples.len() != sample_rate)
    {
        return Err(AdmError::InvalidScene(
            "ADM tracks have different durations".to_owned(),
        ));
    }
    let sample_rate_hz = extract_sample_rate(&export.report);
    let data_bytes = u64::try_from(sample_rate)
        .ok()
        .and_then(|frames| {
            frames
                .checked_mul(u64::from(channels))
                .and_then(|v| v.checked_mul(3))
        })
        .ok_or(AdmError::SizeOverflow)?;
    let fmt_payload = 16_u64;
    let axml = export.xml.as_bytes();
    let chna_payload = chna_payload(&export.tracks)?;
    let axml_len = u64::try_from(axml.len()).map_err(|_| AdmError::SizeOverflow)?;
    let chna_len = u64::try_from(chna_payload.len()).map_err(|_| AdmError::SizeOverflow)?;
    let total_size = 12_u64
        .checked_add(8 + 28)
        .and_then(|v| v.checked_add(8 + fmt_payload))
        .and_then(|v| v.checked_add(chunk_total_size(axml_len)))
        .and_then(|v| v.checked_add(chunk_total_size(chna_len)))
        .and_then(|v| v.checked_add(chunk_total_size(data_bytes)))
        .ok_or(AdmError::SizeOverflow)?;
    writer.write_all(b"BW64")?;
    writer.write_all(&0xFFFF_FFFF_u32.to_le_bytes())?;
    writer.write_all(b"WAVE")?;
    writer.write_all(b"ds64")?;
    writer.write_all(&28_u32.to_le_bytes())?;
    writer.write_all(&total_size.saturating_sub(8).to_le_bytes())?;
    writer.write_all(&data_bytes.to_le_bytes())?;
    writer.write_all(
        &u64::try_from(sample_rate)
            .map_err(|_| AdmError::SizeOverflow)?
            .to_le_bytes(),
    )?;
    writer.write_all(&0_u32.to_le_bytes())?;
    writer.write_all(b"fmt ")?;
    writer.write_all(&16_u32.to_le_bytes())?;
    writer.write_all(&1_u16.to_le_bytes())?;
    writer.write_all(&channels.to_le_bytes())?;
    writer.write_all(&sample_rate_hz.to_le_bytes())?;
    let byte_rate = sample_rate_hz
        .checked_mul(u32::from(channels))
        .and_then(|v| v.checked_mul(3))
        .ok_or(AdmError::SizeOverflow)?;
    writer.write_all(&byte_rate.to_le_bytes())?;
    writer.write_all(&(channels.checked_mul(3).ok_or(AdmError::SizeOverflow)?).to_le_bytes())?;
    writer.write_all(&24_u16.to_le_bytes())?;
    write_chunk(writer, *b"axml", axml)?;
    write_chunk(writer, *b"chna", &chna_payload)?;
    writer.write_all(b"data")?;
    writer.write_all(&0xFFFF_FFFF_u32.to_le_bytes())?;
    for frame in 0..sample_rate {
        for track in &export.tracks {
            let value = track.samples[frame];
            let scaled = (value * 8_388_608.0).round();
            let integer = scaled.clamp(-8_388_608.0, 8_388_607.0) as i32;
            let bytes = integer.to_le_bytes();
            writer
                .write_all(&bytes[..3])
                .map_err(AdmError::Io)
                .map_err(|error| match error {
                    AdmError::Io(io_error) => AdmError::Io(io_error),
                    other => other,
                })?;
        }
    }
    writer.write_all(&[])?;
    writer.flush()?;
    Ok(())
}

fn extract_sample_rate(report: &AdmExportReport) -> u32 {
    report.sample_rate
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

fn chna_payload(tracks: &[TrackRecord]) -> Result<Vec<u8>, AdmError> {
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

fn validate_bytes(bytes: &[u8]) -> Result<AdmValidationSummary, AdmError> {
    if bytes.get(0..4) != Some(b"BW64") || bytes.get(8..12) != Some(b"WAVE") {
        return Err(AdmError::InvalidBw64("missing BW64/WAVE header"));
    }
    let mut cursor = 12_usize;
    let mut chunks = Vec::new();
    let mut ds64 = false;
    let mut fmt = None;
    let mut data = None;
    let mut axml = None;
    let mut chna = None;
    while cursor < bytes.len() {
        let end = cursor.checked_add(8).ok_or(AdmError::SizeOverflow)?;
        if end > bytes.len() {
            return Err(AdmError::InvalidBw64("truncated chunk header"));
        }
        let id = &bytes[cursor..cursor + 4];
        let size = u64::from(u32::from_le_bytes(
            bytes[cursor + 4..cursor + 8]
                .try_into()
                .map_err(|_| AdmError::InvalidBw64("chunk size"))?,
        ));
        let payload_start = cursor + 8;
        let payload_end = if size == u64::from(u32::MAX) && id == b"data" {
            bytes.len()
        } else {
            payload_start
                .checked_add(usize::try_from(size).map_err(|_| AdmError::SizeOverflow)?)
                .ok_or(AdmError::SizeOverflow)?
        };
        if payload_end > bytes.len() {
            return Err(AdmError::InvalidBw64("chunk exceeds file"));
        }
        let name = String::from_utf8_lossy(id).to_string();
        chunks.push(name.clone());
        match id {
            b"ds64" if cursor == 12 => ds64 = true,
            b"fmt " => fmt = Some(&bytes[payload_start..payload_end]),
            b"data" => data = Some(&bytes[payload_start..payload_end]),
            b"axml" => axml = Some(&bytes[payload_start..payload_end]),
            b"chna" => chna = Some(&bytes[payload_start..payload_end]),
            _ => {}
        }
        cursor = payload_end + usize::try_from(size % 2).map_err(|_| AdmError::SizeOverflow)?;
        if id == b"data" {
            break;
        }
    }
    if !ds64 {
        return Err(AdmError::InvalidBw64("ds64 is not the first chunk"));
    }
    let fmt = fmt.ok_or(AdmError::InvalidBw64("missing fmt chunk"))?;
    if fmt.len() != 16 || u16::from_le_bytes(fmt[0..2].try_into().unwrap()) != 1 {
        return Err(AdmError::InvalidBw64("unsupported fmt payload"));
    }
    let channels = usize::from(u16::from_le_bytes(fmt[2..4].try_into().unwrap()));
    let sample_rate = u32::from_le_bytes(fmt[4..8].try_into().unwrap());
    let data = data.ok_or(AdmError::InvalidBw64("missing data chunk"))?;
    let axml = axml.ok_or(AdmError::InvalidBw64("missing axml chunk"))?;
    let chna = chna.ok_or(AdmError::InvalidBw64("missing chna chunk"))?;
    if chna.len() < 4 || (chna.len() - 4) % 40 != 0 {
        return Err(AdmError::InvalidBw64("invalid chna length"));
    }
    let tracks = usize::from(u16::from_le_bytes(chna[0..2].try_into().unwrap()));
    let uids = usize::from(u16::from_le_bytes(chna[2..4].try_into().unwrap()));
    if tracks != channels || uids != tracks || chna.len() != 4 + tracks * 40 {
        return Err(AdmError::InvalidBw64("chna track/channel mismatch"));
    }
    let mut identifiers = HashSet::new();
    for index in 0..tracks {
        let base = 4 + index * 40;
        if chna[base..base + 2] != u16::try_from(index + 1).unwrap().to_le_bytes() {
            return Err(AdmError::InvalidBw64("chna track index is not ordered"));
        }
        if !identifiers.insert(chna[base + 2..base + 14].to_vec()) {
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
        data_bytes: u64::try_from(data.len()).map_err(|_| AdmError::SizeOverflow)?,
        axml_bytes: u64::try_from(axml.len()).map_err(|_| AdmError::SizeOverflow)?,
        chna_tracks: tracks,
        chna_uids: uids,
        identifiers_unique: true,
    })
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
