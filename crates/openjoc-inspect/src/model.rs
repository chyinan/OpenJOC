// pattern: Functional Core

use serde::Serialize;
use std::collections::BTreeMap;

/// Version-one diagnostic contract. Additive fields may appear within a version.
#[derive(Clone, Debug, Serialize)]
pub struct StreamInspection {
    pub schema_version: u32,
    pub input: InputSummary,
    pub container: ContainerSummary,
    pub eac3: Eac3Summary,
    pub joc: JocSummary,
    pub carriage: CarriageSummary,
    pub emdf: EmdfSummary,
    pub scene: SceneSummary,
    pub validation: ValidationSummary,
    pub diagnostics: Diagnostics,
    /// Empty unless AU detail was requested. Indices are zero-based.
    pub access_units: Vec<AuDetail>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct InputSummary {
    pub filename: Option<String>,
    pub format: String,
    pub classification_authority: String,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct ContainerSummary {
    pub kind: String,
    pub sample_entry: Option<String>,
    pub ec3_present: Option<bool>,
    pub dec3_present: Option<bool>,
    pub track_id: Option<String>,
    pub timescale: Option<u64>,
    pub duration_seconds: Option<f64>,
    pub sample_count: Option<u64>,
    pub fragmented: Option<bool>,
    pub cmaf_status: Option<String>,
    pub declared_joc: Option<bool>,
    pub declared_complexity_index: Option<u8>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Eac3Summary {
    pub access_unit_count: u64,
    pub programme_interval_count: u64,
    pub frame_count: u64,
    pub total_samples: u64,
    pub duration_seconds: f64,
    pub total_bytes: u64,
    pub bitrate_bps: Option<f64>,
    pub sample_rates_hz: Vec<u32>,
    pub topologies: Vec<Observation<Vec<String>>>,
    pub block_partitions: Vec<Observation<Vec<u8>>>,
    pub frames_per_au: Vec<usize>,
    pub components: Vec<ComponentSummary>,
    pub dependent_ids: Vec<u8>,
    pub dependent_ids_sequential: Option<bool>,
    pub legacy_core: bool,
    pub lfe_ownership: Vec<Observation<String>>,
    pub channel_conflicts: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct Observation<T> {
    pub value: T,
    pub occurrences: u64,
    pub first_au: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct ComponentSummary {
    pub owner: String,
    pub stream_type: String,
    pub substream_id: u8,
    pub frame_count: u64,
    pub bytes: u64,
    pub samples: u64,
    pub duration_seconds: f64,
    pub bitrate_bps: f64,
    pub frame_bytes: Vec<usize>,
    pub sample_rates_hz: Vec<u32>,
    pub block_counts: Vec<u8>,
    pub numblkscod: Vec<u8>,
    pub acmod: Vec<u8>,
    pub lfeon: Vec<bool>,
    pub chanmap: Vec<Option<u16>>,
    pub channel_locations: Vec<Vec<String>>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct JocSummary {
    pub present: bool,
    pub presence_status: String,
    pub payload_occurrences: u64,
    pub parsed_payloads: u64,
    pub profiles: Vec<Observation<ProfileSummary>>,
    pub coded_object_counts: Vec<u8>,
    pub reconstruction_rows: Vec<u8>,
    pub complexity_indices: Vec<u8>,
    pub addbsi_signaled_aus: u64,
    pub owners: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ProfileSummary {
    pub profile_index: u8,
    pub profile: String,
    pub display_name: String,
    pub reconstruction_input_count: Option<u8>,
    pub carriers: Vec<String>,
    pub phase_signaling: Option<bool>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct CarriageSummary {
    pub locations: Vec<Observation<CarrierLocation>>,
    pub joc_owners: Vec<String>,
    pub oamd_owners: Vec<String>,
    pub aux_present: u64,
    pub aux_absent: u64,
    pub aux_parsed: u64,
    pub aux_non_emdf: u64,
    pub aux_malformed: u64,
    pub skip_observed: u64,
    pub skip_examined: u64,
    pub skip_unresolved: u64,
    pub skip_parsed: u64,
    pub skip_non_emdf: u64,
    pub skip_malformed: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CarrierLocation {
    pub owner: String,
    pub location: String,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct EmdfSummary {
    pub containers: u64,
    pub payloads: Vec<PayloadSummary>,
    pub payload_orders: Vec<Observation<Vec<u64>>>,
    pub configurations: Vec<Observation<PayloadConfiguration>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PayloadSummary {
    pub id: u64,
    pub name: Option<String>,
    pub occurrences: u64,
    pub affected_aus: u64,
    pub first_au: u64,
    pub length_min: usize,
    pub length_max: usize,
    pub unique_lengths: usize,
    pub length_counts: BTreeMap<usize, u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PayloadConfiguration {
    pub payload_id: u64,
    pub sample_offset: Option<u16>,
    pub duration: Option<u64>,
    pub group_id: Option<u64>,
    pub codecdatae: bool,
    pub discard_unknown_payload: bool,
    pub payload_frame_aligned: Option<bool>,
    pub create_duplicate: Option<bool>,
    pub remove_duplicate: Option<bool>,
    pub priority: Option<u8>,
    pub proc_allowed: Option<u8>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[allow(clippy::struct_excessive_bools)] // Independent observable facts in the public JSON contract.
pub struct SceneSummary {
    pub metadata_present: bool,
    pub parsed_payloads: u64,
    pub metadata_object_counts: Vec<u16>,
    pub object_metadata_present: bool,
    pub dynamic_metadata_detected: Option<bool>,
    pub metadata_update_count: u64,
    pub first_change: Option<ChangeLocation>,
    pub last_change: Option<ChangeLocation>,
    pub opaque_trim_present: bool,
    pub original_authored_identity_recovered: bool,
    pub objects: Vec<ObjectStatistics>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ChangeLocation {
    pub au: u64,
    pub sample: u64,
    pub seconds: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct ObjectStatistics {
    pub object_index: usize,
    pub first_active_au: Option<u64>,
    pub last_active_au: Option<u64>,
    pub metadata_update_count: u64,
    pub dynamic: bool,
    pub first_change: Option<ChangeLocation>,
    pub last_change: Option<ChangeLocation>,
    pub position_min: Option<[f64; 3]>,
    pub position_max: Option<[f64; 3]>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct ValidationSummary {
    pub stream_parse: String,
    pub etsi_strict: ProfileValidation,
    pub deployed_compatibility: ProfileValidation,
    pub malformed_aus: u64,
    pub decoder_admissible: Option<bool>,
    pub decoder_checked_aus: u64,
    pub decoder_admitted_aus: u64,
    pub render_verified: bool,
    pub frame_timing_continuity: String,
    pub metadata_timing_continuity: String,
    pub first_timing_discontinuity: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProfileValidation {
    pub status: String,
    pub tested_aus: u64,
    pub failed_aus: u64,
    pub first_failing_au: Option<u64>,
    pub deviations: Vec<DeviationSummary>,
}

impl Default for ProfileValidation {
    fn default() -> Self {
        Self {
            status: "not_applicable".into(),
            tested_aus: 0,
            failed_aus: 0,
            first_failing_au: None,
            deviations: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct DeviationSummary {
    pub payload_id: u64,
    pub field: String,
    pub observed: String,
    pub expected: String,
    pub affected_aus: u64,
    pub first_au: u64,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Diagnostics {
    pub complete: bool,
    pub first_failure: Option<Diagnostic>,
    pub issues: Vec<Diagnostic>,
    pub issue_count: u64,
    pub retained_au_details: usize,
    pub max_au_bytes: usize,
    pub max_au_frames: usize,
    pub aggregation_truncated: bool,
    pub limitations: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Diagnostic {
    pub code: String,
    pub au: u64,
    pub elementary_byte_offset: u64,
    pub message: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct AuDetail {
    pub au: u64,
    pub timestamp_seconds: f64,
    pub elementary_byte_offset: u64,
    pub frames: usize,
    pub samples: u16,
    pub block_partition: Vec<u8>,
    pub topology: Vec<String>,
    pub joc_owner: Option<String>,
    pub profile_indices: Vec<u8>,
    pub object_counts: Vec<u8>,
    pub complexity_indices: Vec<u8>,
    pub payload_ids: Vec<u64>,
    pub lfe_ownership: Option<String>,
    pub status: String,
}

/// Output selection changes retained detail, never the extent of the census.
#[derive(Clone, Copy, Debug, Default)]
#[allow(clippy::struct_excessive_bools)] // Independent output selections.
pub struct InspectionOptions {
    pub aus: bool,
    pub objects: bool,
    pub emdf: bool,
    pub verbose: bool,
    pub au_range: Option<(u64, u64)>,
    pub trim_configuration_count: Option<std::num::NonZeroU8>,
}
