// pattern: Functional Core

use crate::{
    CarrierLocation, ChangeLocation, InspectionAccumulator, InspectionOptions, ProfileSummary,
};
use openjoc_eac3::SyncframeIndexEntry;
use serde::Serialize;

const LIVE_INSPECTION_SCHEMA_VERSION: u32 = 1;
const MAX_ERROR_BYTES: usize = 512;

#[derive(Clone, Debug, Serialize)]
pub struct LiveInspectionSnapshot {
    pub schema_version: u32,
    pub inspection_kind: String,
    pub observation_scope: String,
    pub coverage: String,
    pub observation_epoch: u64,
    pub stream_present: bool,
    pub joc_present: bool,
    pub format: String,
    pub sample_rate_hz: Option<u32>,
    pub current_profile: Option<ProfileSummary>,
    pub reconstruction_carriers: Vec<String>,
    pub programme_topology: Vec<String>,
    pub programme_layout: Option<String>,
    pub dependent_ids: Vec<u8>,
    pub block_partition: Vec<u8>,
    pub lfe_presence: Option<bool>,
    pub lfe_owner: Option<String>,
    pub lfe_semantics: Option<String>,
    pub joc_owner: Option<String>,
    pub object_count: Option<u8>,
    pub complexity: Option<u8>,
    pub carriage_locations: Vec<CarrierLocation>,
    pub etsi_strict: String,
    pub deployed_compatibility: String,
    pub emdf_payloads: Vec<u64>,
    pub dynamic_scene_observed: Option<bool>,
    pub observed_au_count: u64,
    pub malformed_observed_count: u64,
    pub first_observed_metadata_change: Option<ChangeLocation>,
    pub current_timestamp_seconds: Option<f64>,
    pub current_decode_sequence: u64,
    pub last_error_summary: Option<String>,
}

pub struct LiveInspectionObserver {
    accumulator: InspectionAccumulator,
    observation_epoch: u64,
    coverage: &'static str,
    stream_present: bool,
    started_at_stream_beginning: bool,
    end_of_stream: bool,
    explicit_malformed_count: u64,
    sample_rate_hz: Option<u32>,
    current_timestamp_seconds: Option<f64>,
    current_decode_sequence: u64,
    last_error_summary: Option<String>,
}

impl LiveInspectionObserver {
    #[must_use]
    pub fn new() -> Self {
        Self {
            accumulator: InspectionAccumulator::new(InspectionOptions::default()),
            observation_epoch: 1,
            coverage: "partial",
            stream_present: false,
            started_at_stream_beginning: false,
            end_of_stream: false,
            explicit_malformed_count: 0,
            sample_rate_hz: None,
            current_timestamp_seconds: None,
            current_decode_sequence: 0,
            last_error_summary: None,
        }
    }

    pub fn begin_stream(&mut self, started_at_stream_beginning: bool) {
        if !self.stream_present {
            self.stream_present = true;
            self.started_at_stream_beginning = started_at_stream_beginning;
            self.coverage = "partial";
            self.end_of_stream = false;
        }
    }

    pub fn observe_access_unit(
        &mut self,
        bytes: &[u8],
        frames: &[SyncframeIndexEntry],
        sample_rate_hz: u32,
        timestamp_seconds: f64,
        decode_sequence: u64,
    ) {
        if self.end_of_stream {
            return;
        }
        self.begin_stream(false);
        self.accumulator.push(bytes, frames);
        self.sample_rate_hz = Some(sample_rate_hz);
        self.current_timestamp_seconds = Some(timestamp_seconds);
        self.current_decode_sequence = decode_sequence;
    }

    pub fn record_malformed(&mut self, error: &str) {
        if self.end_of_stream {
            return;
        }
        self.begin_stream(false);
        self.explicit_malformed_count = self.explicit_malformed_count.saturating_add(1);
        self.set_last_error(error);
    }

    pub fn set_last_error(&mut self, error: &str) {
        let mut bounded = error.replace('\0', "");
        bounded.truncate(MAX_ERROR_BYTES);
        self.last_error_summary = (!bounded.is_empty()).then_some(bounded);
    }

    pub fn mark_end_of_stream(&mut self) {
        if !self.stream_present {
            return;
        }
        self.end_of_stream = true;
        self.coverage = if self.started_at_stream_beginning {
            "complete_continuous"
        } else {
            "partial"
        };
    }

    pub fn reset_for_discontinuity(&mut self) {
        self.accumulator = InspectionAccumulator::new(InspectionOptions::default());
        self.observation_epoch = self.observation_epoch.saturating_add(1);
        self.coverage = "partial";
        self.stream_present = false;
        self.started_at_stream_beginning = false;
        self.end_of_stream = false;
        self.explicit_malformed_count = 0;
        self.sample_rate_hz = None;
        self.current_timestamp_seconds = None;
        self.current_decode_sequence = 0;
        self.last_error_summary = None;
    }

    #[must_use]
    pub fn snapshot(&self) -> LiveInspectionSnapshot {
        let report = self.accumulator.report();
        let latest_au = self.accumulator.latest_au();
        let joc_present = report.joc.payload_occurrences > 0;
        let current_profile_index =
            latest_au.and_then(|value| value.profile_indices.last().copied());
        let current_profile = current_profile_index.and_then(|index| {
            report
                .joc
                .profiles
                .iter()
                .find(|value| value.value.profile_index == index)
                .map(|value| value.value.clone())
        });
        let current_lfe = latest_au.and_then(|value| value.lfe_ownership.as_deref());
        LiveInspectionSnapshot {
            schema_version: LIVE_INSPECTION_SCHEMA_VERSION,
            inspection_kind: "live_decode_snapshot".to_owned(),
            observation_scope: "live_decode".to_owned(),
            coverage: self.coverage.to_owned(),
            observation_epoch: self.observation_epoch,
            stream_present: self.stream_present,
            joc_present,
            format: if joc_present {
                "eac3_joc".to_owned()
            } else if self.stream_present {
                "eac3".to_owned()
            } else {
                "unknown".to_owned()
            },
            sample_rate_hz: self.sample_rate_hz,
            current_profile,
            reconstruction_carriers: current_profile_index
                .and_then(|index| {
                    report
                        .joc
                        .profiles
                        .iter()
                        .find(|value| value.value.profile_index == index)
                })
                .map_or_else(Vec::new, |value| value.value.carriers.clone()),
            programme_topology: latest_au.map_or_else(Vec::new, |value| value.topology.clone()),
            programme_layout: latest_au.and_then(|value| value.programme_layout.clone()),
            dependent_ids: report.eac3.dependent_ids.clone(),
            block_partition: latest_au.map_or_else(Vec::new, |value| value.block_partition.clone()),
            lfe_presence: current_lfe.map(|value| value != "absent"),
            lfe_owner: current_lfe.and_then(|value| value.split(':').nth(1).map(str::to_owned)),
            lfe_semantics: current_lfe.map(str::to_owned),
            joc_owner: latest_au.and_then(|value| value.joc_owner.clone()),
            object_count: latest_au.and_then(|value| value.object_counts.last().copied()),
            complexity: latest_au.and_then(|value| value.complexity_indices.last().copied()),
            carriage_locations: report
                .carriage
                .locations
                .iter()
                .map(|value| value.value.clone())
                .collect(),
            etsi_strict: report.validation.etsi_strict.status.clone(),
            deployed_compatibility: report.validation.deployed_compatibility.status.clone(),
            emdf_payloads: report.emdf.payloads.iter().map(|value| value.id).collect(),
            dynamic_scene_observed: report.scene.dynamic_metadata_detected,
            observed_au_count: report.eac3.access_unit_count,
            malformed_observed_count: report
                .validation
                .malformed_aus
                .saturating_add(self.explicit_malformed_count),
            first_observed_metadata_change: report.scene.first_change.clone(),
            current_timestamp_seconds: self.current_timestamp_seconds,
            current_decode_sequence: self.current_decode_sequence,
            last_error_summary: self.last_error_summary.clone(),
        }
    }
}

impl Default for LiveInspectionObserver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_snapshot_is_explicit_live_decode_with_no_active_stream() {
        let observer = LiveInspectionObserver::new();

        let snapshot = observer.snapshot();

        assert_eq!(snapshot.schema_version, 1);
        assert_eq!(snapshot.inspection_kind, "live_decode_snapshot");
        assert_eq!(snapshot.observation_scope, "live_decode");
        assert_eq!(snapshot.coverage, "partial");
        assert_eq!(snapshot.observation_epoch, 1);
        assert!(!snapshot.stream_present);
        assert!(!snapshot.joc_present);
        assert_eq!(snapshot.observed_au_count, 0);
        assert_eq!(snapshot.malformed_observed_count, 0);
    }

    #[test]
    fn discontinuity_starts_a_new_epoch_and_clears_old_observations() {
        let mut observer = LiveInspectionObserver::new();
        observer.begin_stream(false);
        observer.record_malformed("bad AU");

        observer.reset_for_discontinuity();
        let snapshot = observer.snapshot();

        assert_eq!(snapshot.observation_epoch, 2);
        assert!(!snapshot.stream_present);
        assert_eq!(snapshot.observed_au_count, 0);
        assert_eq!(snapshot.malformed_observed_count, 0);
        assert_eq!(snapshot.last_error_summary, None);
    }

    #[test]
    fn eos_without_proven_start_remains_partial() {
        let mut observer = LiveInspectionObserver::new();
        observer.begin_stream(false);
        observer.mark_end_of_stream();

        assert_eq!(observer.snapshot().coverage, "partial");
    }

    #[test]
    fn eos_after_zero_timestamp_continuous_start_is_complete() {
        let mut observer = LiveInspectionObserver::new();
        observer.begin_stream(true);
        observer.mark_end_of_stream();

        assert_eq!(observer.snapshot().coverage, "complete_continuous");
    }

    #[test]
    fn error_summary_is_bounded_and_sanitized() {
        let mut observer = LiveInspectionObserver::new();
        observer.set_last_error(&"a\0very long error message".repeat(100));

        let snapshot: LiveInspectionSnapshot = observer.snapshot();

        assert!(
            snapshot
                .last_error_summary
                .as_deref()
                .is_some_and(|value| !value.contains('\0') && value.len() <= 512)
        );
    }
}
