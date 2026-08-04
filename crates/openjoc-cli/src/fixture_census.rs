// pattern: Mixed (needs refactoring)
// Reason: this diagnostic module keeps pure report normalization next to the
// thin filesystem/process shell so the opt-in corpus command remains bounded.

use openjoc_container::{InputMediaKind, load_eac3};
use openjoc_eac3::{
    Eac3Error, classify_skip_field_emdf, extract_auxdata, extract_joc_access_unit,
    group_access_units, index_syncframes, inspect_audio_block_carriers, parse_bsi,
    parse_joc_addbsi,
};
use openjoc_emdf::{CarrierClassification, EmdfError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, fs, io,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Clone, Debug, Deserialize)]
pub struct FixtureDescriptor {
    pub label: String,
    pub path: String,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
}

pub type FixtureManifest = Vec<FixtureDescriptor>;

#[derive(Debug)]
pub enum FixtureManifestError {
    Io {
        operation: &'static str,
        path: PathBuf,
        source: io::Error,
    },
    Json(String),
    EmptyManifest,
    InvalidDescriptor(String),
    DuplicateLabel {
        label: String,
    },
    MissingFixture {
        label: String,
        path: PathBuf,
    },
    InvalidExpectedHash {
        label: String,
    },
    HashMismatch {
        label: String,
        expected: String,
        actual: String,
    },
    UnsupportedInput {
        label: String,
        detail: String,
    },
    Container {
        label: String,
        detail: String,
    },
    Probe {
        label: String,
        detail: String,
    },
    ReportIo {
        operation: &'static str,
        path: PathBuf,
        source: io::Error,
    },
    ReportJson(String),
}

impl fmt::Display for FixtureManifestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io {
                operation,
                path,
                source,
            }
            | Self::ReportIo {
                operation,
                path,
                source,
            } => write!(
                formatter,
                "failed to {operation} {}: {source}",
                path.display()
            ),
            Self::Json(detail) => write!(formatter, "invalid real-fixture manifest JSON: {detail}"),
            Self::EmptyManifest => formatter.write_str("real-fixture manifest is empty"),
            Self::InvalidDescriptor(detail) => {
                write!(formatter, "invalid real-fixture descriptor: {detail}")
            }
            Self::DuplicateLabel { label } => {
                write!(formatter, "duplicate real-fixture label: {label}")
            }
            Self::MissingFixture { label, path } => write!(
                formatter,
                "real-fixture {label} is missing: {}",
                path.display()
            ),
            Self::InvalidExpectedHash { label } => {
                write!(
                    formatter,
                    "real-fixture {label} has an invalid SHA-256 expectation"
                )
            }
            Self::HashMismatch {
                label,
                expected,
                actual,
            } => write!(
                formatter,
                "real-fixture {label} SHA-256 mismatch: expected {expected}, got {actual}"
            ),
            Self::UnsupportedInput { label, detail } => {
                write!(
                    formatter,
                    "real-fixture {label} has unsupported input: {detail}"
                )
            }
            Self::Container { label, detail } => {
                write!(
                    formatter,
                    "real-fixture {label} container failure: {detail}"
                )
            }
            Self::Probe { label, detail } => {
                write!(formatter, "real-fixture {label} FFprobe failure: {detail}")
            }
            Self::ReportJson(detail) => {
                write!(formatter, "failed to serialize census report: {detail}")
            }
        }
    }
}

impl std::error::Error for FixtureManifestError {}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CarrierState {
    JocExtensionNotSignaled,
    #[allow(dead_code)]
    ExtensionNoEmdfInValidatedCarriers,
    ValidatedCarriersContainNoEmdf,
    CarrierUnresolved,
    MalformedEmdfCandidate,
    ValidEmdfNoJocProfile,
    EmdfProfileIncomplete,
    ValidProfileFound,
    /// Reserved for the conditional real-vector reconstruction lane.
    #[allow(dead_code)]
    ReconstructionAttempted,
    /// Reserved for ground-truth-backed reconstruction verification.
    #[allow(dead_code)]
    ReconstructionVerified,
}

impl CarrierState {
    fn status_name(self) -> &'static str {
        match self {
            Self::JocExtensionNotSignaled => "joc_extension_not_signaled",
            Self::ExtensionNoEmdfInValidatedCarriers => "extension_no_emdf_in_validated_carriers",
            Self::ValidatedCarriersContainNoEmdf => "validated_carriers_contain_no_emdf",
            Self::CarrierUnresolved => "carrier_unresolved",
            Self::MalformedEmdfCandidate => "malformed_emdf_candidate",
            Self::ValidEmdfNoJocProfile => "valid_emdf_no_joc_profile",
            Self::EmdfProfileIncomplete => "emdf_profile_incomplete",
            Self::ValidProfileFound => "valid_profile_found",
            Self::ReconstructionAttempted => "reconstruction_attempted",
            Self::ReconstructionVerified => "reconstruction_verified",
        }
    }

    fn rank(self) -> u8 {
        report_status_order(self.status_name())
    }
}

pub fn report_status_order(status: &str) -> u8 {
    match status {
        "joc_extension_not_signaled" => 1,
        "extension_no_emdf_in_validated_carriers" | "validated_carriers_contain_no_emdf" => 2,
        "carrier_unresolved" => 3,
        "malformed_emdf_candidate" => 4,
        "valid_emdf_no_joc_profile" => 5,
        "emdf_profile_incomplete" => 6,
        "valid_profile_found" => 7,
        "reconstruction_attempted" => 8,
        "reconstruction_verified" => 9,
        _ => 0,
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct CensusReport {
    pub fixtures: Vec<FixtureReport>,
}

#[derive(Clone, Debug, Serialize)]
pub struct FixtureReport {
    pub label: String,
    pub note: Option<String>,
    pub source_sha256: String,
    pub source_bytes: usize,
    pub input_media: String,
    pub audio_track: Option<AudioTrackReport>,
    pub demuxed_sha256: String,
    pub demuxed_bytes: usize,
    pub syncframe_count: usize,
    pub access_unit_count: usize,
    pub samples_per_access_unit: Vec<u16>,
    pub substream_topology: BTreeMap<String, usize>,
    pub addbsi_presence_count: usize,
    pub addbsi_extension_distribution: BTreeMap<String, usize>,
    pub joc_complexity_distribution: BTreeMap<u8, usize>,
    pub auxdatae_present_count: usize,
    pub auxdatae_absent_count: usize,
    pub bounded_frame_end_auxiliary_containers: usize,
    pub audio_block_skip_field_presence_count: usize,
    pub audio_block_skip_field_examined_count: usize,
    pub audio_block_skip_field_unresolved_count: usize,
    pub skip_field_byte_lengths: BTreeMap<usize, usize>,
    pub skip_field_examined_as_carrier_candidate_count: usize,
    pub skip_field_non_emdf_count: usize,
    pub skip_field_valid_emdf_container_count: usize,
    pub skip_field_malformed_emdf_candidate_count: usize,
    pub skip_field_unsupported_emdf_count: usize,
    pub frame_end_valid_emdf_container_count: usize,
    pub audio_block_malformed_mantissa_count: usize,
    pub audio_block_first_mantissa_failure: Option<AudioBlockMantissaFailureReport>,
    pub emdf_attempts: Vec<CarrierAttempt>,
    pub emdf_payload_id_distribution: BTreeMap<u64, usize>,
    pub frame_end_emdf_payload_id_distribution: BTreeMap<u64, usize>,
    pub skip_field_emdf_payload_id_distribution: BTreeMap<u64, usize>,
    pub payload_id_11_count: usize,
    pub payload_id_14_count: usize,
    pub payload_id_11_located: bool,
    pub payload_id_14_located: bool,
    pub valid_joc_profile_count: usize,
    pub complete_joc_profile_count: usize,
    pub invalid_or_incomplete_profile_count: usize,
    pub malformed_or_truncated_carrier_count: usize,
    pub first_malformed_candidate: Option<CarrierAttempt>,
    pub first_complete_profile: Option<CarrierAttempt>,
    pub first_failure: Option<FirstFailure>,
    pub decoder_first_failure: Option<FirstFailure>,
    pub carrier_state: CarrierState,
}

#[derive(Clone, Debug, Serialize)]
pub struct AudioTrackReport {
    pub stream_index: usize,
    pub codec: String,
    pub sample_rate: Option<u32>,
    pub channels: Option<u16>,
    pub channel_layout: Option<String>,
    pub duration_seconds: Option<f64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CarrierAttempt {
    pub location: String,
    pub access_unit: usize,
    pub syncframe: usize,
    pub substream_id: u8,
    pub audio_block: Option<usize>,
    pub frame_relative_start_bit: usize,
    pub start_bit: usize,
    pub length_bits: usize,
    pub result: String,
    pub payload_ids: Vec<u64>,
    pub payload_sizes: Vec<usize>,
    pub payload_group_ids: Vec<Option<u64>>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct FirstFailure {
    pub phase: String,
    pub access_unit: usize,
    pub syncframe: usize,
    pub audio_block: Option<usize>,
    pub bit_offset: Option<usize>,
    pub elementary_stream_bit_offset: Option<usize>,
    pub block_relative_bit_offset: Option<usize>,
    pub detail: String,
    pub mantissa: Option<MantissaFailureContext>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AudioBlockMantissaFailureReport {
    pub block: usize,
    pub element: String,
    pub channel: Option<u8>,
    pub bin: usize,
    pub bap: u8,
    pub raw_code: u16,
    pub bit_width: u8,
    pub grouped: bool,
    pub bit_offset_bits: usize,
    pub detail: String,
}

#[allow(clippy::struct_excessive_bools)] // These flags are independent diagnostic facts.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MantissaFailureContext {
    pub element: String,
    pub channel: Option<u8>,
    pub block: usize,
    pub bap: u8,
    pub raw_code: u16,
    pub bit_width: u8,
    pub bit_offset_bits: usize,
    pub grouped: bool,
    pub spx_active: bool,
    pub coupling_active: bool,
    pub enhanced_coupling_active: bool,
    pub rematrix_active: bool,
    pub aht_active: bool,
    pub bin_index: usize,
    pub exponent: u8,
    pub psd: Option<i16>,
    pub mask: Option<i16>,
    pub quantizer_levels: u32,
    pub quantizer_group_size: u8,
    pub quantizer_group_bits: u8,
    pub quantizer_symmetric: bool,
    pub group_position: u8,
    pub dither: bool,
    pub block_switch: bool,
    pub exponent_strategy: u8,
    pub exponent_reused: bool,
    pub block_start_offset_bits: usize,
    pub element_start_offset_bits: usize,
    pub block_relative_bit_offset: usize,
    pub element_relative_bit_offset: usize,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
enum ManifestDocument {
    List(Vec<FixtureDescriptor>),
    Wrapped { fixtures: Vec<FixtureDescriptor> },
}

pub fn parse_manifest(bytes: &[u8]) -> Result<FixtureManifest, FixtureManifestError> {
    let document = serde_json::from_slice::<ManifestDocument>(bytes)
        .map_err(|error| FixtureManifestError::Json(error.to_string()))?;
    let mut entries = match document {
        ManifestDocument::List(entries) | ManifestDocument::Wrapped { fixtures: entries } => {
            entries
        }
    };
    if entries.is_empty() {
        return Err(FixtureManifestError::EmptyManifest);
    }
    let mut labels = BTreeSet::new();
    for entry in &mut entries {
        if entry.label.trim().is_empty() {
            return Err(FixtureManifestError::InvalidDescriptor(
                "label must not be empty".to_owned(),
            ));
        }
        if entry.path.trim().is_empty() {
            return Err(FixtureManifestError::InvalidDescriptor(format!(
                "fixture {} has an empty path",
                entry.label
            )));
        }
        if !labels.insert(entry.label.clone()) {
            return Err(FixtureManifestError::DuplicateLabel {
                label: entry.label.clone(),
            });
        }
        if let Some(hash) = &mut entry.sha256 {
            let normalized = hash.trim().to_ascii_lowercase();
            if normalized.is_empty() {
                entry.sha256 = None;
            } else if normalized.len() != 64
                || !normalized.bytes().all(|byte| byte.is_ascii_hexdigit())
            {
                return Err(FixtureManifestError::InvalidExpectedHash {
                    label: entry.label.clone(),
                });
            } else {
                *hash = normalized;
            }
        }
    }
    Ok(entries)
}

pub fn load_manifest(path: &Path) -> Result<FixtureManifest, FixtureManifestError> {
    let bytes = fs::read(path).map_err(|source| FixtureManifestError::Io {
        operation: "read manifest",
        path: path.to_path_buf(),
        source,
    })?;
    parse_manifest(&bytes)
}

pub fn run_census(manifest_path: &Path) -> Result<CensusReport, FixtureManifestError> {
    let mut entries = load_manifest(manifest_path)?;
    entries.sort_by(|left, right| left.label.cmp(&right.label));
    let root = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    entries
        .iter()
        .map(|entry| census_fixture(entry, root))
        .collect::<Result<Vec<_>, _>>()
        .map(|fixtures| CensusReport { fixtures })
}

pub fn write_reports(report: &CensusReport, output: &Path) -> Result<(), FixtureManifestError> {
    fs::create_dir_all(output).map_err(|source| FixtureManifestError::ReportIo {
        operation: "create census report directory",
        path: output.to_path_buf(),
        source,
    })?;
    let json = serde_json::to_string_pretty(report)
        .map_err(|error| FixtureManifestError::ReportJson(error.to_string()))?;
    let json_path = output.join("census.json");
    fs::write(&json_path, format!("{json}\n")).map_err(|source| {
        FixtureManifestError::ReportIo {
            operation: "write machine-readable census report",
            path: json_path,
            source,
        }
    })?;
    let text_path = output.join("census.txt");
    fs::write(&text_path, render_text_report(report)).map_err(|source| {
        FixtureManifestError::ReportIo {
            operation: "write human-readable census report",
            path: text_path,
            source,
        }
    })?;
    Ok(())
}

fn census_fixture(
    entry: &FixtureDescriptor,
    manifest_root: &Path,
) -> Result<FixtureReport, FixtureManifestError> {
    let path = resolve_fixture_path(&entry.path, manifest_root);
    if !path.is_file() {
        return Err(FixtureManifestError::MissingFixture {
            label: entry.label.clone(),
            path,
        });
    }
    let source = fs::read(&path).map_err(|source| FixtureManifestError::Io {
        operation: "read fixture",
        path: path.clone(),
        source,
    })?;
    let source_sha256 = sha256(&source);
    if let Some(expected) = &entry.sha256 {
        if expected != &source_sha256 {
            return Err(FixtureManifestError::HashMismatch {
                label: entry.label.clone(),
                expected: expected.clone(),
                actual: source_sha256.clone(),
            });
        }
    }
    let kind = openjoc_container::detect_media(&source[..source.len().min(12)]);
    let audio_track = if kind == InputMediaKind::IsoBmff {
        Some(probe_audio_track(entry, &path)?)
    } else {
        None
    };
    let media = load_eac3(&path).map_err(|error| match error {
        openjoc_container::InputMediaError::UnsupportedSignature => {
            FixtureManifestError::UnsupportedInput {
                label: entry.label.clone(),
                detail: error.to_string(),
            }
        }
        _ => FixtureManifestError::Container {
            label: entry.label.clone(),
            detail: error.to_string(),
        },
    })?;
    let demuxed_sha256 = sha256(&media.bytes);
    let frames = index_syncframes(&media.bytes).map_err(|error| census_error(entry, error))?;
    let units = group_access_units(&frames).map_err(|error| census_error(entry, error))?;
    let mut report = FixtureReport {
        label: entry.label.clone(),
        note: entry.note.clone(),
        source_sha256,
        source_bytes: source.len(),
        input_media: media_kind_name(media.kind).to_owned(),
        audio_track,
        demuxed_sha256,
        demuxed_bytes: media.bytes.len(),
        syncframe_count: frames.len(),
        access_unit_count: units.len(),
        samples_per_access_unit: units.iter().map(|unit| unit.samples).collect(),
        substream_topology: BTreeMap::new(),
        addbsi_presence_count: 0,
        addbsi_extension_distribution: BTreeMap::new(),
        joc_complexity_distribution: BTreeMap::new(),
        auxdatae_present_count: 0,
        auxdatae_absent_count: 0,
        bounded_frame_end_auxiliary_containers: 0,
        audio_block_skip_field_presence_count: 0,
        audio_block_skip_field_examined_count: 0,
        audio_block_skip_field_unresolved_count: 0,
        skip_field_byte_lengths: BTreeMap::new(),
        skip_field_examined_as_carrier_candidate_count: 0,
        skip_field_non_emdf_count: 0,
        skip_field_valid_emdf_container_count: 0,
        skip_field_malformed_emdf_candidate_count: 0,
        skip_field_unsupported_emdf_count: 0,
        frame_end_valid_emdf_container_count: 0,
        audio_block_malformed_mantissa_count: 0,
        audio_block_first_mantissa_failure: None,
        emdf_attempts: Vec::new(),
        emdf_payload_id_distribution: BTreeMap::new(),
        frame_end_emdf_payload_id_distribution: BTreeMap::new(),
        skip_field_emdf_payload_id_distribution: BTreeMap::new(),
        payload_id_11_count: 0,
        payload_id_14_count: 0,
        payload_id_11_located: false,
        payload_id_14_located: false,
        valid_joc_profile_count: 0,
        complete_joc_profile_count: 0,
        invalid_or_incomplete_profile_count: 0,
        malformed_or_truncated_carrier_count: 0,
        first_malformed_candidate: None,
        first_complete_profile: None,
        first_failure: None,
        decoder_first_failure: None,
        carrier_state: CarrierState::JocExtensionNotSignaled,
    };
    for entry_frame in &frames {
        let key = format!(
            "{:?}/{}",
            entry_frame.header.stream_type, entry_frame.header.substream_id
        );
        *report.substream_topology.entry(key).or_default() += 1;
    }
    let mut frame_units = vec![0_usize; frames.len()];
    for (unit_index, unit) in units.iter().enumerate() {
        let end = unit.first_frame + unit.frame_count;
        for slot in frame_units
            .get_mut(unit.first_frame..end)
            .ok_or_else(|| census_error(entry, Eac3Error::InvalidAccessUnitRange))?
        {
            *slot = unit_index;
        }
    }
    for (frame_index, entry_frame) in frames.iter().enumerate() {
        let unit_index = frame_units[frame_index];
        let frame =
            frame_bytes(&media.bytes, *entry_frame).map_err(|error| census_error(entry, error))?;
        if let Err(error) = inspect_bsi(&mut report, frame) {
            report.malformed_or_truncated_carrier_count += 1;
            record_failure(
                &mut report,
                0,
                frame_index,
                None,
                None,
                "bsi",
                error.to_string(),
            );
        }
        inspect_frame_end_carrier(&mut report, unit_index, frame_index, *entry_frame, frame);
        inspect_first_audio_block(
            &mut report,
            unit_index,
            &media.bytes,
            frame_index,
            *entry_frame,
            frame,
        );
    }
    if let Some(first) = frames.first() {
        let frame =
            frame_bytes(&media.bytes, *first).map_err(|error| census_error(entry, error))?;
        diagnose_first_complete_audio_block(&mut report, *first, frame);
    }
    if report.valid_joc_profile_count != 0 {
        for (unit_index, unit) in units.iter().enumerate() {
            match extract_joc_access_unit(&media.bytes, &frames, *unit) {
                Ok(Some(metadata)) => {
                    report.complete_joc_profile_count += 1;
                    if report.first_complete_profile.is_none() {
                        report.first_complete_profile = report
                            .emdf_attempts
                            .iter()
                            .find(|attempt| {
                                attempt.syncframe == metadata.carrier_frame
                                    && attempt.payload_ids.contains(&11)
                                    && attempt.payload_ids.contains(&14)
                            })
                            .cloned();
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    record_failure(
                        &mut report,
                        unit_index,
                        unit.first_frame,
                        None,
                        None,
                        "joc_profile_extraction",
                        error.to_string(),
                    );
                }
            }
        }
    }
    report.carrier_state = determine_state(&report);
    Ok(report)
}

fn diagnose_first_complete_audio_block(
    report: &mut FixtureReport,
    entry: openjoc_eac3::SyncframeIndexEntry,
    frame: &[u8],
) {
    let dither = vec![0.0_f64; 32_768];
    if let Err(error) = openjoc_eac3::decode_first_audio_block(frame, &dither) {
        let context = mantissa_failure_context(error);
        let bit_offset = context
            .as_ref()
            .map(|value| value.bit_offset_bits)
            .or_else(|| {
                openjoc_eac3::parse_audio_frame(frame)
                    .ok()
                    .map(|audio| audio.audio_blocks_offset_bits)
            });
        let failure = FirstFailure {
            phase: "complete_audio_block_decode".to_owned(),
            access_unit: 0,
            syncframe: 0,
            audio_block: Some(0),
            bit_offset,
            elementary_stream_bit_offset: bit_offset
                .and_then(|offset| entry.offset.checked_mul(8)?.checked_add(offset)),
            block_relative_bit_offset: bit_offset.and_then(|offset| {
                openjoc_eac3::parse_audio_frame(frame)
                    .ok()
                    .and_then(|audio| offset.checked_sub(audio.audio_blocks_offset_bits))
            }),
            detail: error.to_string(),
            mantissa: context,
        };
        if report.first_failure.is_none() {
            report.first_failure = Some(failure.clone());
        }
        report.decoder_first_failure = Some(failure);
    }
}

fn mantissa_failure_context(error: Eac3Error) -> Option<MantissaFailureContext> {
    let Eac3Error::InvalidMantissaDiagnostic {
        element,
        channel,
        block,
        bap,
        actual,
        bit_width,
        bit_offset_bits,
        grouped,
        spx_active,
        coupling_active,
        enhanced_coupling_active,
        rematrix_active,
        aht_active,
        bin_index,
        exponent,
        psd,
        mask,
        quantizer_levels,
        quantizer_group_size,
        quantizer_group_bits,
        quantizer_symmetric,
        group_position,
        dither,
        block_switch,
        exponent_strategy,
        exponent_reused,
        block_start_offset_bits,
        element_start_offset_bits,
    } = error
    else {
        return None;
    };
    Some(MantissaFailureContext {
        element: format!("{element:?}"),
        channel,
        block,
        bap,
        raw_code: actual,
        bit_width,
        bit_offset_bits,
        grouped,
        spx_active,
        coupling_active,
        enhanced_coupling_active,
        rematrix_active,
        aht_active,
        bin_index,
        exponent,
        psd,
        mask,
        quantizer_levels,
        quantizer_group_size,
        quantizer_group_bits,
        quantizer_symmetric,
        group_position,
        dither,
        block_switch,
        exponent_strategy,
        exponent_reused,
        block_start_offset_bits,
        element_start_offset_bits,
        block_relative_bit_offset: bit_offset_bits.saturating_sub(block_start_offset_bits),
        element_relative_bit_offset: bit_offset_bits.saturating_sub(element_start_offset_bits),
    })
}

fn inspect_bsi(report: &mut FixtureReport, frame: &[u8]) -> Result<(), Eac3Error> {
    let bsi = parse_bsi(frame)?;
    let Some(addbsi) = bsi.addbsi else {
        return Ok(());
    };
    report.addbsi_presence_count += 1;
    let key = addbsi
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(":");
    *report.addbsi_extension_distribution.entry(key).or_default() += 1;
    match parse_joc_addbsi(&addbsi) {
        Ok(extension) => {
            *report
                .joc_complexity_distribution
                .entry(extension.complexity_index)
                .or_default() += 1;
        }
        Err(error) => {
            report.malformed_or_truncated_carrier_count += 1;
            return Err(error);
        }
    }
    Ok(())
}

fn inspect_frame_end_carrier(
    report: &mut FixtureReport,
    access_unit: usize,
    frame_index: usize,
    entry: openjoc_eac3::SyncframeIndexEntry,
    frame: &[u8],
) {
    match extract_auxdata(frame) {
        Ok(Some(aux)) => {
            report.auxdatae_present_count += 1;
            report.bounded_frame_end_auxiliary_containers += 1;
            let (frame_relative_start_bit, start_bit, length_bits) =
                auxiliary_span(entry, aux.bit_len);
            let classification = if aux.bit_len % 8 != 0 {
                Err(Eac3Error::AuxDataNotByteAligned { bits: aux.bit_len })
            } else {
                Ok(openjoc_emdf::classify_emdf_carrier(&aux.bytes))
            };
            match classification {
                Ok(CarrierClassification::NonEmdf) => {
                    report.emdf_attempts.push(CarrierAttempt {
                        location: "frame_end_auxdata".to_owned(),
                        access_unit,
                        syncframe: frame_index,
                        substream_id: entry.header.substream_id,
                        audio_block: None,
                        frame_relative_start_bit,
                        start_bit,
                        length_bits,
                        result: "non_emdf".to_owned(),
                        payload_ids: Vec::new(),
                        payload_sizes: Vec::new(),
                        payload_group_ids: Vec::new(),
                        error: None,
                    });
                }
                Ok(CarrierClassification::Parsed(parsed)) => {
                    report.frame_end_valid_emdf_container_count += 1;
                    let (payload_ids, payload_sizes, payload_group_ids) =
                        payload_inventory(&parsed);
                    for id in &payload_ids {
                        *report.emdf_payload_id_distribution.entry(*id).or_default() += 1;
                        *report
                            .frame_end_emdf_payload_id_distribution
                            .entry(*id)
                            .or_default() += 1;
                    }
                    let has_oamd = payload_ids.contains(&11);
                    let has_joc = payload_ids.contains(&14);
                    report.payload_id_11_count += usize::from(has_oamd);
                    report.payload_id_14_count += usize::from(has_joc);
                    report.payload_id_11_located |= has_oamd;
                    report.payload_id_14_located |= has_joc;
                    if has_oamd && has_joc {
                        if openjoc_emdf::validate_joc_profile(&parsed.container).is_ok() {
                            report.valid_joc_profile_count += 1;
                        } else {
                            report.invalid_or_incomplete_profile_count += 1;
                        }
                    } else {
                        report.invalid_or_incomplete_profile_count += 1;
                    }
                    report.emdf_attempts.push(CarrierAttempt {
                        location: "frame_end_auxdata".to_owned(),
                        access_unit,
                        syncframe: frame_index,
                        substream_id: entry.header.substream_id,
                        audio_block: None,
                        frame_relative_start_bit,
                        start_bit,
                        length_bits,
                        result: "parsed".to_owned(),
                        payload_ids,
                        payload_sizes,
                        payload_group_ids,
                        error: None,
                    });
                }
                Ok(CarrierClassification::Malformed(error)) => {
                    report.malformed_or_truncated_carrier_count += 1;
                    report.emdf_attempts.push(CarrierAttempt {
                        location: "frame_end_auxdata".to_owned(),
                        access_unit,
                        syncframe: frame_index,
                        substream_id: entry.header.substream_id,
                        audio_block: None,
                        frame_relative_start_bit,
                        start_bit,
                        length_bits,
                        result: "failed".to_owned(),
                        payload_ids: Vec::new(),
                        payload_sizes: Vec::new(),
                        payload_group_ids: Vec::new(),
                        error: Some(error.to_string()),
                    });
                    record_failure(
                        report,
                        access_unit,
                        frame_index,
                        None,
                        Some(start_bit),
                        "frame_end_auxdata",
                        error.to_string(),
                    );
                }
                Ok(CarrierClassification::TrailingData {
                    container_bytes,
                    carrier_bytes,
                }) => {
                    let error = format!(
                        "EMDF container consumed {container_bytes} of {carrier_bytes} carrier bytes"
                    );
                    report.malformed_or_truncated_carrier_count += 1;
                    report.emdf_attempts.push(CarrierAttempt {
                        location: "frame_end_auxdata".to_owned(),
                        access_unit,
                        syncframe: frame_index,
                        substream_id: entry.header.substream_id,
                        audio_block: None,
                        frame_relative_start_bit,
                        start_bit,
                        length_bits,
                        result: "malformed_emdf_trailing_data".to_owned(),
                        payload_ids: Vec::new(),
                        payload_sizes: Vec::new(),
                        payload_group_ids: Vec::new(),
                        error: Some(error.clone()),
                    });
                    record_failure(
                        report,
                        access_unit,
                        frame_index,
                        None,
                        Some(start_bit),
                        "frame_end_auxdata",
                        error,
                    );
                }
                Err(error) => {
                    report.malformed_or_truncated_carrier_count += 1;
                    report.emdf_attempts.push(CarrierAttempt {
                        location: "frame_end_auxdata".to_owned(),
                        access_unit,
                        syncframe: frame_index,
                        substream_id: entry.header.substream_id,
                        audio_block: None,
                        frame_relative_start_bit,
                        start_bit,
                        length_bits,
                        result: "failed".to_owned(),
                        payload_ids: Vec::new(),
                        payload_sizes: Vec::new(),
                        payload_group_ids: Vec::new(),
                        error: Some(error.to_string()),
                    });
                    record_failure(
                        report,
                        access_unit,
                        frame_index,
                        None,
                        Some(start_bit),
                        "frame_end_auxdata",
                        error.to_string(),
                    );
                }
            }
        }
        Ok(None) => report.auxdatae_absent_count += 1,
        Err(error) => {
            report.malformed_or_truncated_carrier_count += 1;
            record_failure(
                report,
                access_unit,
                frame_index,
                None,
                None,
                "frame_end_auxdata",
                error.to_string(),
            );
        }
    }
}

fn inspect_first_audio_block(
    report: &mut FixtureReport,
    access_unit: usize,
    _stream: &[u8],
    frame_index: usize,
    entry: openjoc_eac3::SyncframeIndexEntry,
    frame: &[u8],
) {
    let examined_before = report.audio_block_skip_field_examined_count;
    let result = inspect_audio_block_carriers(frame, |carrier| {
        report.audio_block_skip_field_examined_count += 1;
        if let Some(skip) = carrier.skip_field.clone() {
            report.audio_block_skip_field_presence_count += 1;
            *report
                .skip_field_byte_lengths
                .entry(skip.bytes.len())
                .or_default() += 1;
            inspect_skip_field_carrier(report, access_unit, frame_index, entry, carrier, &skip);
        }
    });
    match result {
        Ok(summary) => {
            report.audio_block_skip_field_unresolved_count += summary.unresolved_blocks;
            report.audio_block_malformed_mantissa_count += summary.malformed_mantissa_count;
            if report.audio_block_first_mantissa_failure.is_none() {
                if let Some(failure) = summary.first_mantissa_failure {
                    let mapped = AudioBlockMantissaFailureReport {
                        block: failure.block_index,
                        element: format!("{:?}", failure.element),
                        channel: failure.channel,
                        bin: failure.bin_index,
                        bap: failure.bap,
                        raw_code: failure.raw_code,
                        bit_width: failure.bit_width,
                        grouped: failure.grouped,
                        bit_offset_bits: failure.bit_offset_bits,
                        detail: failure.error.to_string(),
                    };
                    if report.first_failure.is_none() {
                        record_failure(
                            report,
                            access_unit,
                            frame_index,
                            Some(failure.block_index),
                            Some(failure.bit_offset_bits),
                            "audio_block_mantissa_traversal",
                            mapped.detail.clone(),
                        );
                        if let Some(first) = &mut report.first_failure {
                            first.elementary_stream_bit_offset = entry
                                .offset
                                .checked_mul(8)
                                .and_then(|offset| offset.checked_add(failure.bit_offset_bits));
                            first.block_relative_bit_offset =
                                openjoc_eac3::parse_audio_frame(frame)
                                    .ok()
                                    .and_then(|audio| {
                                        failure
                                            .bit_offset_bits
                                            .checked_sub(audio.audio_blocks_offset_bits)
                                    });
                        }
                    }
                    report.audio_block_first_mantissa_failure = Some(mapped);
                }
            }
        }
        Err(error) => {
            let examined_in_frame = report
                .audio_block_skip_field_examined_count
                .saturating_sub(examined_before);
            report.audio_block_skip_field_unresolved_count +=
                usize::from(entry.header.audio_blocks).saturating_sub(examined_in_frame);
            record_failure(
                report,
                access_unit,
                frame_index,
                Some(0),
                None,
                "audio_block_prefix",
                error.to_string(),
            );
        }
    }
}

fn inspect_skip_field_carrier(
    report: &mut FixtureReport,
    access_unit: usize,
    frame_index: usize,
    entry: openjoc_eac3::SyncframeIndexEntry,
    carrier: &openjoc_eac3::AudioBlockCarrier,
    skip: &openjoc_eac3::AuxiliaryData,
) {
    report.skip_field_examined_as_carrier_candidate_count += 1;
    let start_bit = entry
        .offset
        .checked_mul(8)
        .and_then(|offset| {
            carrier
                .skip_field_start_offset_bits
                .and_then(|start| offset.checked_add(start))
        })
        .unwrap_or(0);
    let frame_relative_start_bit = carrier.skip_field_start_offset_bits.unwrap_or(0);
    let length_bits = skip.bit_len;
    let mut attempt = CarrierAttempt {
        location: "audio_block_skipfld".to_owned(),
        access_unit,
        syncframe: frame_index,
        substream_id: entry.header.substream_id,
        audio_block: Some(carrier.block_index),
        frame_relative_start_bit,
        start_bit,
        length_bits,
        result: String::new(),
        payload_ids: Vec::new(),
        payload_sizes: Vec::new(),
        payload_group_ids: Vec::new(),
        error: None,
    };
    match classify_skip_field_emdf(skip) {
        CarrierClassification::NonEmdf => {
            report.skip_field_non_emdf_count += 1;
            "non_emdf".clone_into(&mut attempt.result);
        }
        CarrierClassification::Parsed(parsed) => {
            report.skip_field_valid_emdf_container_count += 1;
            let (payload_ids, payload_sizes, payload_group_ids) = payload_inventory(&parsed);
            attempt.payload_ids.clone_from(&payload_ids);
            attempt.payload_sizes = payload_sizes;
            attempt.payload_group_ids = payload_group_ids;
            for id in &payload_ids {
                *report.emdf_payload_id_distribution.entry(*id).or_default() += 1;
                *report
                    .skip_field_emdf_payload_id_distribution
                    .entry(*id)
                    .or_default() += 1;
            }
            let has_oamd = payload_ids.contains(&11);
            let has_joc = payload_ids.contains(&14);
            report.payload_id_11_count += usize::from(has_oamd);
            report.payload_id_14_count += usize::from(has_joc);
            report.payload_id_11_located |= has_oamd;
            report.payload_id_14_located |= has_joc;
            if has_oamd && has_joc {
                if let Err(error) = openjoc_emdf::validate_joc_profile(&parsed.container) {
                    report.invalid_or_incomplete_profile_count += 1;
                    "parsed_incomplete_profile".clone_into(&mut attempt.result);
                    attempt.error = Some(error.to_string());
                } else {
                    report.valid_joc_profile_count += 1;
                    "parsed_complete_profile_container".clone_into(&mut attempt.result);
                    if report.first_complete_profile.is_none() {
                        report.first_complete_profile = Some(attempt.clone());
                    }
                }
            } else if has_oamd || has_joc {
                report.invalid_or_incomplete_profile_count += 1;
                "parsed_incomplete_profile".clone_into(&mut attempt.result);
            } else {
                "parsed_non_profile".clone_into(&mut attempt.result);
            }
        }
        CarrierClassification::Malformed(error) => {
            report.skip_field_malformed_emdf_candidate_count += 1;
            report.malformed_or_truncated_carrier_count += 1;
            report.skip_field_unsupported_emdf_count += usize::from(is_unsupported_emdf(&error));
            attempt.result = if is_unsupported_emdf(&error) {
                "unsupported_emdf".to_owned()
            } else {
                "malformed_emdf".to_owned()
            };
            attempt.error = Some(error.to_string());
            record_skip_failure(report, &attempt);
        }
        CarrierClassification::TrailingData {
            container_bytes,
            carrier_bytes,
        } => {
            report.skip_field_malformed_emdf_candidate_count += 1;
            report.malformed_or_truncated_carrier_count += 1;
            "malformed_emdf_trailing_data".clone_into(&mut attempt.result);
            attempt.error = Some(format!(
                "EMDF container consumed {container_bytes} of {carrier_bytes} carrier bytes"
            ));
            record_skip_failure(report, &attempt);
        }
    }
    report.emdf_attempts.push(attempt);
}

fn is_unsupported_emdf(error: &EmdfError) -> bool {
    matches!(error, EmdfError::UnsupportedVersion { .. })
}

fn payload_inventory(
    parsed: &openjoc_emdf::ParsedEmdf,
) -> (Vec<u64>, Vec<usize>, Vec<Option<u64>>) {
    let ids = parsed
        .container
        .payloads
        .iter()
        .map(|payload| payload.id)
        .collect();
    let sizes = parsed
        .container
        .payloads
        .iter()
        .map(|payload| payload.data.len())
        .collect();
    let group_ids = parsed
        .container
        .payloads
        .iter()
        .map(|payload| payload.config.group_id)
        .collect();
    (ids, sizes, group_ids)
}

fn record_skip_failure(report: &mut FixtureReport, attempt: &CarrierAttempt) {
    if report.first_malformed_candidate.is_none() {
        report.first_malformed_candidate = Some(attempt.clone());
    }
    record_failure(
        report,
        attempt.access_unit,
        attempt.syncframe,
        attempt.audio_block,
        Some(attempt.start_bit),
        "audio_block_skipfld_emdf",
        attempt
            .error
            .clone()
            .unwrap_or_else(|| attempt.result.clone()),
    );
}

fn determine_state(report: &FixtureReport) -> CarrierState {
    if report.addbsi_presence_count == 0 {
        CarrierState::JocExtensionNotSignaled
    } else if report.complete_joc_profile_count > 0 {
        CarrierState::ValidProfileFound
    } else if report.skip_field_malformed_emdf_candidate_count > 0 {
        CarrierState::MalformedEmdfCandidate
    } else if report.payload_id_11_located || report.payload_id_14_located {
        CarrierState::EmdfProfileIncomplete
    } else if report.audio_block_skip_field_unresolved_count > 0 {
        CarrierState::CarrierUnresolved
    } else if report.skip_field_valid_emdf_container_count > 0
        || report.frame_end_valid_emdf_container_count > 0
    {
        CarrierState::ValidEmdfNoJocProfile
    } else {
        CarrierState::ValidatedCarriersContainNoEmdf
    }
}

fn record_failure(
    report: &mut FixtureReport,
    access_unit: usize,
    syncframe: usize,
    audio_block: Option<usize>,
    bit_offset: Option<usize>,
    phase: &str,
    detail: String,
) {
    if report.first_failure.is_none() {
        report.first_failure = Some(FirstFailure {
            phase: phase.to_owned(),
            access_unit,
            syncframe,
            audio_block,
            bit_offset,
            elementary_stream_bit_offset: None,
            block_relative_bit_offset: None,
            detail,
            mantissa: None,
        });
    }
}

fn auxiliary_span(
    entry: openjoc_eac3::SyncframeIndexEntry,
    length_bits: usize,
) -> (usize, usize, usize) {
    let frame_bits = entry.header.frame_size * 8;
    let start = frame_bits.saturating_sub(18 + 14 + length_bits);
    (start, entry.offset * 8 + start, length_bits)
}

fn frame_bytes(
    stream: &[u8],
    entry: openjoc_eac3::SyncframeIndexEntry,
) -> Result<&[u8], Eac3Error> {
    let end = entry
        .offset
        .checked_add(entry.header.frame_size)
        .ok_or(Eac3Error::FrameSizeOverflow)?;
    stream
        .get(entry.offset..end)
        .ok_or(Eac3Error::TruncatedFrame {
            offset: entry.offset,
            declared: entry.header.frame_size,
            available: stream.len().saturating_sub(entry.offset),
        })
}

fn census_error(entry: &FixtureDescriptor, error: Eac3Error) -> FixtureManifestError {
    FixtureManifestError::Container {
        label: entry.label.clone(),
        detail: error.to_string(),
    }
}

fn resolve_fixture_path(path: &str, root: &Path) -> PathBuf {
    let candidate = Path::new(path);
    if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        root.join(candidate)
    }
}

fn sha256(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let digest = Sha256::digest(bytes);
    let mut text = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(text, "{byte:02x}");
    }
    text
}

fn media_kind_name(kind: InputMediaKind) -> &'static str {
    match kind {
        InputMediaKind::RawEac3 => "raw_eac3",
        InputMediaKind::IsoBmff => "iso_bmff",
        InputMediaKind::Unknown => "unknown",
    }
}

#[derive(Deserialize)]
struct ProbeDocument {
    streams: Vec<ProbeStream>,
}

#[derive(Deserialize)]
struct ProbeStream {
    index: usize,
    codec_name: Option<String>,
    codec_type: Option<String>,
    sample_rate: Option<String>,
    channels: Option<u16>,
    channel_layout: Option<String>,
    duration: Option<String>,
}

fn probe_audio_track(
    entry: &FixtureDescriptor,
    path: &Path,
) -> Result<AudioTrackReport, FixtureManifestError> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=index,codec_name,codec_type,sample_rate,channels,channel_layout,duration",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .map_err(|error| FixtureManifestError::Probe {
            label: entry.label.clone(),
            detail: error.to_string(),
        })?;
    if !output.status.success() {
        return Err(FixtureManifestError::Probe {
            label: entry.label.clone(),
            detail: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        });
    }
    let document = serde_json::from_slice::<ProbeDocument>(&output.stdout).map_err(|error| {
        FixtureManifestError::Probe {
            label: entry.label.clone(),
            detail: error.to_string(),
        }
    })?;
    let streams = document
        .streams
        .into_iter()
        .filter(|stream| stream.codec_type.as_deref() == Some("audio"))
        .collect::<Vec<_>>();
    let stream = match streams.as_slice() {
        [stream] => stream,
        [] => {
            return Err(FixtureManifestError::Probe {
                label: entry.label.clone(),
                detail: "no audio stream".to_owned(),
            });
        }
        streams => {
            return Err(FixtureManifestError::Probe {
                label: entry.label.clone(),
                detail: format!("{} audio streams; expected exactly one", streams.len()),
            });
        }
    };
    let codec = stream.codec_name.clone().unwrap_or_default();
    if codec != "eac3" {
        return Err(FixtureManifestError::Probe {
            label: entry.label.clone(),
            detail: format!("selected audio codec is {codec}, expected eac3"),
        });
    }
    Ok(AudioTrackReport {
        stream_index: stream.index,
        codec,
        sample_rate: stream
            .sample_rate
            .as_deref()
            .and_then(|value| value.parse().ok()),
        channels: stream.channels,
        channel_layout: stream.channel_layout.clone(),
        duration_seconds: stream
            .duration
            .as_deref()
            .and_then(|value| value.parse().ok()),
    })
}

fn render_text_report(report: &CensusReport) -> String {
    use std::fmt::Write as _;
    let mut text = String::new();
    text.push_str("comparison:\n");
    text.push_str(
        "  label | syncframes/access-units | complexity | auxdatae present/absent | skip observed/examined/unresolved | skip EMDF non/valid/malformed/unsupported | payload 11/14 | state\n",
    );
    for fixture in &report.fixtures {
        let complexity = fixture
            .joc_complexity_distribution
            .iter()
            .map(|(index, count)| format!("{index}:{count}"))
            .collect::<Vec<_>>()
            .join(",");
        let _ = writeln!(
            text,
            "  {} | {}/{} | {} | {}/{} | {}/{}/{} | {}/{}/{}/{} | {}/{} | {}",
            fixture.label,
            fixture.syncframe_count,
            fixture.access_unit_count,
            if complexity.is_empty() {
                "none"
            } else {
                &complexity
            },
            fixture.auxdatae_present_count,
            fixture.auxdatae_absent_count,
            fixture.audio_block_skip_field_presence_count,
            fixture.audio_block_skip_field_examined_count,
            fixture.audio_block_skip_field_unresolved_count,
            fixture.skip_field_non_emdf_count,
            fixture.skip_field_valid_emdf_container_count,
            fixture.skip_field_malformed_emdf_candidate_count,
            fixture.skip_field_unsupported_emdf_count,
            fixture.payload_id_11_located,
            fixture.payload_id_14_located,
            fixture.carrier_state.status_name(),
        );
    }
    text.push('\n');
    for fixture in &report.fixtures {
        let _ = writeln!(text, "fixture: {}", fixture.label);
        let _ = writeln!(
            text,
            "  source: {} bytes, sha256 {}",
            fixture.source_bytes, fixture.source_sha256
        );
        let _ = writeln!(text, "  input: {}", fixture.input_media);
        let _ = writeln!(
            text,
            "  elementary stream: {} bytes, sha256 {}",
            fixture.demuxed_bytes, fixture.demuxed_sha256
        );
        let _ = writeln!(
            text,
            "  syncframes/access units: {}/{}",
            fixture.syncframe_count, fixture.access_unit_count
        );
        let _ = writeln!(
            text,
            "  carrier state: {} (rank {})",
            fixture.carrier_state.status_name(),
            fixture.carrier_state.rank()
        );
        let _ = writeln!(text, "  addbsi present: {}", fixture.addbsi_presence_count);
        let _ = writeln!(
            text,
            "  frame-end auxdatae present/absent: {}/{}",
            fixture.auxdatae_present_count, fixture.auxdatae_absent_count
        );
        let _ = writeln!(
            text,
            "  skip-field observed/examined/unresolved: {}/{}/{}",
            fixture.audio_block_skip_field_presence_count,
            fixture.audio_block_skip_field_examined_count,
            fixture.audio_block_skip_field_unresolved_count
        );
        let _ = writeln!(
            text,
            "  skip-field carrier candidates examined/non-EMDF/valid/malformed/unsupported: {}/{}/{}/{}/{}",
            fixture.skip_field_examined_as_carrier_candidate_count,
            fixture.skip_field_non_emdf_count,
            fixture.skip_field_valid_emdf_container_count,
            fixture.skip_field_malformed_emdf_candidate_count,
            fixture.skip_field_unsupported_emdf_count,
        );
        let _ = writeln!(
            text,
            "  valid EMDF containers frame-end/skip-field: {}/{}",
            fixture.frame_end_valid_emdf_container_count,
            fixture.skip_field_valid_emdf_container_count,
        );
        let _ = writeln!(
            text,
            "  parse-only malformed mantissa codewords: {}",
            fixture.audio_block_malformed_mantissa_count
        );
        let _ = writeln!(
            text,
            "  payload IDs 11/14 located: {}/{}",
            fixture.payload_id_11_located, fixture.payload_id_14_located
        );
        let _ = writeln!(
            text,
            "  payload IDs 11/14 count: {}/{}; complete access-unit profiles: {}",
            fixture.payload_id_11_count,
            fixture.payload_id_14_count,
            fixture.complete_joc_profile_count,
        );
        if let Some(attempt) = fixture
            .emdf_attempts
            .iter()
            .find(|attempt| !attempt.payload_ids.is_empty())
        {
            let _ = writeln!(
                text,
                "  first parsed carrier: {} at access unit {} syncframe {} block {:?}; ids={:?} sizes={:?} group_ids={:?} result={}",
                attempt.location,
                attempt.access_unit,
                attempt.syncframe,
                attempt.audio_block,
                attempt.payload_ids,
                attempt.payload_sizes,
                attempt.payload_group_ids,
                attempt.result,
            );
        }
        if let Some(failure) = &fixture.first_failure {
            let _ = writeln!(
                text,
                "  first failure: {} at syncframe {} bit {:?}: {}",
                failure.phase, failure.syncframe, failure.bit_offset, failure.detail
            );
            if let Some(mantissa) = &failure.mantissa {
                let _ = writeln!(
                    text,
                    "  mantissa context: element={} channel={:?} block={} bin={} exponent={} bap={} raw={} width={} bit={} block_rel={} element_rel={} grouped={} group_pos={} levels={} group={}x{} symmetric={} psd={:?} mask={:?}",
                    mantissa.element,
                    mantissa.channel,
                    mantissa.block,
                    mantissa.bin_index,
                    mantissa.exponent,
                    mantissa.bap,
                    mantissa.raw_code,
                    mantissa.bit_width,
                    mantissa.bit_offset_bits,
                    mantissa.block_relative_bit_offset,
                    mantissa.element_relative_bit_offset,
                    mantissa.grouped,
                    mantissa.group_position,
                    mantissa.quantizer_levels,
                    mantissa.quantizer_group_size,
                    mantissa.quantizer_group_bits,
                    mantissa.quantizer_symmetric,
                    mantissa.psd,
                    mantissa.mask
                );
                let _ = writeln!(
                    text,
                    "  state: spx={} coupling={} enhanced_coupling={} rematrix={} aht={} dither={} block_switch={} exponent_strategy={} reused={} block_start={} element_start={}",
                    mantissa.spx_active,
                    mantissa.coupling_active,
                    mantissa.enhanced_coupling_active,
                    mantissa.rematrix_active,
                    mantissa.aht_active,
                    mantissa.dither,
                    mantissa.block_switch,
                    mantissa.exponent_strategy,
                    mantissa.exponent_reused,
                    mantissa.block_start_offset_bits,
                    mantissa.element_start_offset_bits,
                );
            }
        }
        if let Some(failure) = &fixture.audio_block_first_mantissa_failure {
            let _ = writeln!(
                text,
                "  parse-only mantissa failure: element={} channel={:?} block={} bin={} bap={} raw={} width={} bit={} grouped={} detail={}",
                failure.element,
                failure.channel,
                failure.block,
                failure.bin,
                failure.bap,
                failure.raw_code,
                failure.bit_width,
                failure.bit_offset_bits,
                failure.grouped,
                failure.detail
            );
        }
        if let Some(failure) = &fixture.decoder_first_failure {
            let _ = writeln!(
                text,
                "  decoder first failure: {} at syncframe {} bit {:?}: {}",
                failure.phase, failure.syncframe, failure.bit_offset, failure.detail
            );
        }
        text.push('\n');
    }
    text
}

#[cfg(test)]
mod tests {
    use super::{
        CarrierAttempt, FixtureManifest, FixtureManifestError, MantissaFailureContext,
        mantissa_failure_context, parse_manifest, payload_inventory, report_status_order,
        run_census,
    };
    use openjoc_eac3::{Eac3Error, MantissaElement};
    use openjoc_emdf::{EmdfContainer, EmdfPayload, EmdfPayloadConfig, EmdfProtection, ParsedEmdf};
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn test_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("openjoc-census-{name}-{nonce}"))
    }

    fn raw_eac3_frame() -> Vec<u8> {
        let mut frame = vec![0_u8; 64];
        frame[0] = 0x0b;
        frame[1] = 0x77;
        // strmtyp/substreamid/frame-size/fscod/numblkscod. The remaining
        // bounded syntax is zero-filled deliberately; census must report any
        // traversal failure rather than treating it as carrier absence.
        frame[2] = 0;
        frame[3] = 31; // 64-byte syncframe: frame_size = 2 * (31 + 1)
        frame
    }

    #[test]
    fn parses_multiple_fixture_descriptors_and_preserves_notes() {
        let manifest = parse_manifest(
            br#"[
                {"label":"b","path":"b.ec3","sha256":"","note":"second"},
                {"label":"a","path":"a.m4a","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","note":"first"}
            ]"#,
        )
        .expect("manifest should parse");
        assert_eq!(manifest.len(), 2);
        assert_eq!(manifest[0].label, "b");
        assert_eq!(manifest[1].note.as_deref(), Some("first"));
    }

    #[test]
    fn rejects_duplicate_labels() {
        let error = parse_manifest(
            br#"[
                {"label":"same","path":"a.ec3"},
                {"label":"same","path":"b.ec3"}
            ]"#,
        )
        .expect_err("duplicate labels must fail");
        assert!(matches!(error, FixtureManifestError::DuplicateLabel { .. }));
    }

    #[test]
    fn report_status_order_is_stable_and_distinguishes_unresolved_carriers() {
        assert_eq!(report_status_order("valid_profile_found"), 7);
        assert!(
            report_status_order("carrier_unresolved") < report_status_order("valid_profile_found")
        );
        let _: Option<FixtureManifest> = None;
    }

    #[test]
    fn carrier_inventory_preserves_payload_ids_sizes_and_group_ids() {
        let parsed = ParsedEmdf {
            container: EmdfContainer {
                version: 0,
                key_id: 0,
                payloads: vec![
                    EmdfPayload {
                        id: 11,
                        config: EmdfPayloadConfig {
                            sample_offset: None,
                            duration: None,
                            group_id: Some(1),
                            codec_data_present: true,
                            discard_unknown_payload: false,
                            payload_frame_aligned: Some(true),
                            create_duplicate: Some(false),
                            remove_duplicate: Some(false),
                            priority: Some(0),
                            processing_allowed: Some(0),
                        },
                        data: vec![1, 2],
                    },
                    EmdfPayload {
                        id: 99,
                        config: EmdfPayloadConfig {
                            sample_offset: None,
                            duration: None,
                            group_id: None,
                            codec_data_present: false,
                            discard_unknown_payload: true,
                            payload_frame_aligned: None,
                            create_duplicate: None,
                            remove_duplicate: None,
                            priority: None,
                            processing_allowed: None,
                        },
                        data: vec![3],
                    },
                ],
                protection: EmdfProtection {
                    primary: Vec::new(),
                    secondary: Vec::new(),
                },
            },
            bytes_consumed: 0,
        };
        assert_eq!(
            payload_inventory(&parsed),
            (vec![11, 99], vec![2, 1], vec![Some(1), None])
        );
    }

    #[test]
    fn carrier_attempt_retains_frame_relative_and_absolute_offsets() {
        let attempt = CarrierAttempt {
            location: "audio_block_skipfld".to_owned(),
            access_unit: 0,
            syncframe: 2,
            substream_id: 0,
            audio_block: Some(1),
            frame_relative_start_bit: 137,
            start_bit: 8_329,
            length_bits: 16,
            result: "non_emdf".to_owned(),
            payload_ids: Vec::new(),
            payload_sizes: Vec::new(),
            payload_group_ids: Vec::new(),
            error: None,
        };
        assert_eq!(attempt.frame_relative_start_bit, 137);
        assert_eq!(attempt.start_bit, 8_329);
    }

    #[test]
    fn retains_normative_mantissa_context_in_fixture_reports() {
        let error = Eac3Error::InvalidMantissaDiagnostic {
            element: MantissaElement::Channel,
            channel: Some(2),
            block: 0,
            bap: 3,
            actual: 7,
            bit_width: 3,
            bit_offset_bits: 2828,
            grouped: false,
            spx_active: false,
            coupling_active: false,
            enhanced_coupling_active: false,
            rematrix_active: false,
            aht_active: false,
            bin_index: 94,
            exponent: 24,
            psd: Some(0),
            mask: Some(1120),
            quantizer_levels: 7,
            quantizer_group_size: 1,
            quantizer_group_bits: 3,
            quantizer_symmetric: true,
            group_position: 0,
            dither: true,
            block_switch: false,
            exponent_strategy: 1,
            exponent_reused: false,
            block_start_offset_bits: 198,
            element_start_offset_bits: 2345,
        };
        let context = mantissa_failure_context(error).expect("diagnostic context");
        assert_eq!(
            context,
            MantissaFailureContext {
                element: "Channel".to_owned(),
                channel: Some(2),
                block: 0,
                bap: 3,
                raw_code: 7,
                bit_width: 3,
                bit_offset_bits: 2828,
                grouped: false,
                spx_active: false,
                coupling_active: false,
                enhanced_coupling_active: false,
                rematrix_active: false,
                aht_active: false,
                bin_index: 94,
                exponent: 24,
                psd: Some(0),
                mask: Some(1120),
                quantizer_levels: 7,
                quantizer_group_size: 1,
                quantizer_group_bits: 3,
                quantizer_symmetric: true,
                group_position: 0,
                dither: true,
                block_switch: false,
                exponent_strategy: 1,
                exponent_reused: false,
                block_start_offset_bits: 198,
                element_start_offset_bits: 2345,
                block_relative_bit_offset: 2630,
                element_relative_bit_offset: 483,
            }
        );
    }

    #[test]
    fn rejects_fixture_hash_mismatch_before_container_processing() {
        let root = test_root("hash");
        fs::create_dir_all(&root).expect("test directory");
        let fixture = root.join("input.ec3");
        fs::write(&fixture, b"fixture").expect("fixture");
        let manifest = root.join("manifest.json");
        fs::write(
            &manifest,
            r#"[{"label":"sample","path":"input.ec3","sha256":"0000000000000000000000000000000000000000000000000000000000000000"}]"#,
        )
        .expect("manifest");
        let error = run_census(&manifest).expect_err("hash mismatch");
        assert!(matches!(error, FixtureManifestError::HashMismatch { .. }));
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn reports_missing_fixture_as_structured_error() {
        let root = test_root("missing");
        fs::create_dir_all(&root).expect("test directory");
        let manifest = root.join("manifest.json");
        fs::write(&manifest, r#"[{"label":"sample","path":"missing.ec3"}]"#).expect("manifest");
        let error = run_census(&manifest).expect_err("missing fixture");
        assert!(matches!(error, FixtureManifestError::MissingFixture { .. }));
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn census_reports_fixture_order_by_stable_label() {
        let root = test_root("order");
        fs::create_dir_all(&root).expect("test directory");
        let frame = raw_eac3_frame();
        fs::write(root.join("a.ec3"), &frame).expect("fixture a");
        fs::write(root.join("b.ec3"), &frame).expect("fixture b");
        let manifest = root.join("manifest.json");
        fs::write(
            &manifest,
            r#"[
                {"label":"b","path":"b.ec3"},
                {"label":"a","path":"a.ec3"}
            ]"#,
        )
        .expect("manifest");
        let report = run_census(&manifest).expect("census");
        let labels = report
            .fixtures
            .iter()
            .map(|fixture| fixture.label.as_str())
            .collect::<Vec<_>>();
        assert_eq!(labels, ["a", "b"]);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn opt_in_external_manifest_runs_the_same_bounded_census() {
        let Some(path) = std::env::var_os("OPENJOC_REAL_FIXTURE_MANIFEST") else {
            return;
        };
        let report = run_census(std::path::Path::new(&path)).expect("external census");
        assert!(!report.fixtures.is_empty());
        assert!(
            report
                .fixtures
                .windows(2)
                .all(|pair| pair[0].label < pair[1].label)
        );
    }
}
