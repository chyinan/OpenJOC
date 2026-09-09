// pattern: Functional Core

use crate::model::{
    AuDetail, CarriageSummary, CarrierLocation, ChangeLocation, ComponentSummary, ContainerSummary,
    DeviationSummary, Diagnostic, Diagnostics, Eac3Summary, EmdfSummary, InputSummary,
    InspectionOptions, JocSummary, ObjectStatistics, Observation, PayloadConfiguration,
    PayloadSummary, ProfileSummary, SceneSummary, StreamInspection, ValidationSummary,
};
use openjoc_eac3::{self as eac3, StreamType, SyncframeIndexEntry, programme_layout_name};
use openjoc_emdf::{CarrierClassification, EmdfContainer, JocValidationProfile};
use openjoc_oamd::{OamdElement, ObjectUpdate};
use std::collections::{BTreeMap, BTreeSet};

const MAX_VARIANTS: usize = 256;

/// One-AU working state plus bounded full-stream aggregates.
pub struct InspectionAccumulator {
    report: StreamInspection,
    latest_au: Option<AuDetail>,
    options: InspectionOptions,
    previous_objects: BTreeMap<usize, ObjectUpdate>,
    previous_metadata_sample: Option<u64>,
    object_stats: BTreeMap<usize, ObjectStatistics>,
    offset: u64,
    future_events: BTreeMap<(u64, usize), SceneEvent>,
    validation_au: [Option<u64>; 2],
    validation_failed_au: [Option<u64>; 2],
    deviation_seen: BTreeSet<(usize, u64, String, String, String)>,
    pending_objects: Vec<(openjoc_oamd::ObjectElement, Vec<openjoc_oamd::ObjectClass>)>,
}

struct SceneEvent {
    update: ObjectUpdate,
    class: Option<openjoc_oamd::ObjectClass>,
    when: ChangeLocation,
}

struct ProgrammeObservation {
    lfe_ownership: String,
    layout: String,
}

fn unique<T: PartialEq + Ord>(values: &mut Vec<T>, value: T) {
    if !values.contains(&value) && values.len() < MAX_VARIANTS {
        values.push(value);
        values.sort();
    }
}

fn observe<T: PartialEq>(values: &mut Vec<Observation<T>>, value: T, au: u64) -> bool {
    if let Some(entry) = values.iter_mut().find(|entry| entry.value == value) {
        entry.occurrences += 1;
    } else if values.len() < MAX_VARIANTS {
        values.push(Observation {
            value,
            occurrences: 1,
            first_au: au,
        });
    } else {
        return false;
    }
    true
}

fn owner(header: eac3::SyncframeHeader) -> String {
    format!(
        "{}{}",
        if header.stream_type == StreamType::Dependent {
            "D"
        } else {
            "I"
        },
        header.substream_id
    )
}

fn profile(index: u8) -> ProfileSummary {
    let (name, label, count, phase) = match index {
        0 => ("five_x", "5.X", Some(5), Some(false)),
        1 => ("flat_7x", "Flat-7.X", Some(7), Some(false)),
        2 => ("five_x_plus_two", "5.X+2", Some(7), Some(false)),
        3 => ("five_x_phase", "5.X + Phase Signaling", Some(5), Some(true)),
        4 => (
            "five_x_plus_two_phase",
            "5.X+2 + Phase Signaling",
            Some(7),
            Some(true),
        ),
        _ => (
            "reserved",
            "Reserved / invalid for supported public profile",
            None,
            None,
        ),
    };
    let mut carriers = Vec::new();
    if count.is_some() {
        carriers.extend(["L", "R", "C", "Ls", "Rs"].map(str::to_owned));
    }
    if index == 1 {
        carriers.extend(["Lrs", "Rrs"].map(str::to_owned));
    }
    if matches!(index, 2 | 4) {
        carriers.extend(["Tfl", "Tfr"].map(str::to_owned));
    }
    ProfileSummary {
        profile_index: index,
        profile: name.into(),
        display_name: format!("{label} (idx{index})"),
        reconstruction_input_count: count,
        carriers,
        phase_signaling: phase,
    }
}

impl InspectionAccumulator {
    pub fn new(options: InspectionOptions) -> Self {
        Self {
            report: StreamInspection {
                schema_version: 1,
                input: InputSummary { format: "unknown".into(), classification_authority: "in_band_syntax".into(), ..InputSummary::default() },
                container: ContainerSummary { kind: "raw_eac3".into(), ..ContainerSummary::default() },
                eac3: Eac3Summary::default(), joc: JocSummary::default(), carriage: CarriageSummary::default(),
                emdf: EmdfSummary::default(), scene: SceneSummary::default(),
                validation: ValidationSummary { stream_parse: "pass".into(), frame_timing_continuity: "continuous".into(), metadata_timing_continuity: "unavailable".into(), ..ValidationSummary::default() },
                diagnostics: Diagnostics { complete: true, limitations: vec![
                    "Inspection does not synthesize PCM; decoder admission is not render verification.".into(),
                    "Object indices describe decoded OAMD slots, not original authored identity or a JOC-row binding.".into(),
                    "Unknown container fields are null; raw timestamps are derived from programme samples.".into(),
                    "Distinct summary variants are capped at 256; diagnostic examples at 64.".into(),
                ], ..Diagnostics::default() }, access_units: Vec::new(),
            }, latest_au: None, options, previous_objects: BTreeMap::new(), previous_metadata_sample: None,
            object_stats: BTreeMap::new(), offset: 0, pending_objects: Vec::new(),
            validation_au: [None;2], validation_failed_au: [None;2], deviation_seen: BTreeSet::new(), future_events: BTreeMap::new(),
        }
    }

    pub fn report_mut(&mut self) -> &mut StreamInspection {
        &mut self.report
    }

    #[must_use]
    pub fn report(&self) -> &StreamInspection {
        &self.report
    }

    #[must_use]
    pub fn latest_au(&self) -> Option<&AuDetail> {
        self.latest_au.as_ref()
    }

    pub fn next_au_index(&self) -> u64 {
        self.report.eac3.access_unit_count
    }

    fn issue(&mut self, code: &str, message: String) {
        let issue = Diagnostic {
            code: code.into(),
            au: self.next_au_index(),
            elementary_byte_offset: self.offset,
            message,
        };
        self.report.diagnostics.issue_count += 1;
        if self.report.diagnostics.first_failure.is_none() {
            self.report.diagnostics.first_failure = Some(issue.clone());
        }
        if self.report.diagnostics.issues.len() < 64 {
            self.report.diagnostics.issues.push(issue);
        }
    }

    /// Records a framing failure after all safely delimited preceding AUs.
    pub fn framing_failure(&mut self, message: String) {
        self.record_framing_failure(message, false);
    }

    /// Records a tail failure belonging to the already-counted short candidate.
    pub fn incomplete_candidate_tail(&mut self, message: String) {
        self.record_framing_failure(message, true);
    }

    fn record_framing_failure(&mut self, message: String, already_counted: bool) {
        let retained = self.report.diagnostics.issues.len();
        self.issue("TRUNCATED_OR_MALFORMED_EAC3", message);
        if already_counted {
            if let Some(issue) = self.report.diagnostics.issues.get_mut(retained) {
                issue.au = issue.au.saturating_sub(1);
            }
        } else {
            self.report.validation.malformed_aus += 1;
        }
        self.report.validation.stream_parse = "fail".into();
        self.report.validation.decoder_admissible = Some(false);
        self.report.validation.frame_timing_continuity = "discontinuous".into();
        self.report
            .validation
            .first_timing_discontinuity
            .get_or_insert(self.next_au_index());
        self.report.diagnostics.complete = false;
    }

    /// Examines a bounded candidate interval, using the existing codec grouping rules.
    /// Malformed candidates retain their frame/EMDF census and do not stop later AUs.
    pub fn push(&mut self, bytes: &[u8], frames: &[SyncframeIndexEntry]) {
        let au = self.next_au_index();
        self.deviation_seen.clear();
        let starting_issues = self.report.diagnostics.issue_count;
        let starting_unresolved = self.report.carriage.skip_unresolved;
        let timestamp = self.report.eac3.duration_seconds;
        let start_sample = self.report.eac3.total_samples;
        let mut detail = AuDetail {
            au,
            timestamp_seconds: timestamp,
            elementary_byte_offset: self.offset,
            frames: frames.len(),
            samples: 0,
            block_partition: Vec::new(),
            topology: Vec::new(),
            joc_owner: None,
            profile_indices: Vec::new(),
            object_counts: Vec::new(),
            complexity_indices: Vec::new(),
            payload_ids: Vec::new(),
            lfe_ownership: None,
            programme_layout: None,
            status: "pass".into(),
        };
        let grouping = eac3::group_access_units(frames).and_then(|units| {
            if units.len() == 1 && units[0].frame_count == frames.len() {
                Ok(units[0])
            } else {
                Err(eac3::Eac3Error::InvalidAccessUnitRange)
            }
        });
        let unit = match grouping {
            Ok(unit) => {
                detail.samples = unit.samples;
                Some(unit)
            }
            Err(error) => {
                self.issue("INCONSISTENT_PROGRAMME_TOPOLOGY", error.to_string());
                self.report.validation.frame_timing_continuity = "discontinuous".into();
                self.report
                    .validation
                    .first_timing_discontinuity
                    .get_or_insert(au);
                if matches!(
                    error,
                    eac3::Eac3Error::NonsequentialDependentSubstream { .. }
                ) {
                    self.report.eac3.dependent_ids_sequential = Some(false);
                }
                None
            }
        };
        let mut payloads_in_au = BTreeSet::new();
        let mut information = Vec::new();
        let mut addbsi = false;
        let mut parsed_header = None;
        let mut programme_id = 0;
        for entry in frames {
            let Some(frame) = entry
                .offset
                .checked_add(entry.header.frame_size)
                .and_then(|end| bytes.get(entry.offset..end))
            else {
                self.issue(
                    "TRUNCATED_EAC3",
                    "Indexed frame exceeds candidate bounds".into(),
                );
                continue;
            };
            if entry.header.stream_type != StreamType::Dependent {
                programme_id = entry.header.substream_id;
            }
            let name = if entry.header.stream_type == StreamType::Dependent && programme_id != 0 {
                format!("I{programme_id}/{}", owner(entry.header))
            } else {
                owner(entry.header)
            };
            if entry.header.stream_type == StreamType::Dependent {
                unique(
                    &mut self.report.eac3.dependent_ids,
                    entry.header.substream_id,
                );
            } else {
                if entry.header.substream_id == 0 {
                    detail.block_partition.push(entry.header.audio_blocks);
                }
                if !information.is_empty() {
                    let observation = self.programme(&information);
                    detail.lfe_ownership = Some(observation.lfe_ownership);
                    detail.programme_layout = Some(observation.layout);
                    information.clear();
                }
            }
            if !detail.topology.contains(&name) {
                detail.topology.push(name.clone());
            }
            self.report.eac3.legacy_core |=
                entry.header.stream_type == StreamType::LegacyIndependent;
            unique(
                &mut self.report.eac3.sample_rates_hz,
                entry.header.sample_rate,
            );
            match eac3::parse_bsi(frame) {
                Ok(bsi) => {
                    if let Some(raw) = &bsi.addbsi {
                        if let Ok(extension) = eac3::parse_joc_addbsi(raw) {
                            addbsi = true;
                            unique(
                                &mut self.report.joc.complexity_indices,
                                extension.complexity_index,
                            );
                            unique(&mut detail.complexity_indices, extension.complexity_index);
                        }
                    }
                    self.component(&bsi, &name);
                    information.push(bsi);
                }
                Err(error) => self.issue("MALFORMED_EAC3_BSI", error.to_string()),
            }
            match eac3::classify_aux_emdf(frame) {
                Ok(Some(classification)) => {
                    self.report.carriage.aux_present += 1;
                    self.carrier(
                        classification,
                        &name,
                        "frame_end_auxdatae",
                        &mut payloads_in_au,
                        &mut detail,
                        &mut parsed_header,
                        start_sample,
                        timestamp,
                        entry.header.sample_rate,
                    );
                }
                Ok(None) => self.report.carriage.aux_absent += 1,
                Err(error) => self.issue("MALFORMED_AUXDATA", error.to_string()),
            }
            let mut carriers = Vec::new();
            let traversal = eac3::inspect_audio_block_carriers(frame, |block| {
                if let Some(skip) = &block.skip_field {
                    carriers.push(eac3::classify_skip_field_emdf(skip));
                }
            });
            self.report.carriage.skip_observed += carriers.len() as u64;
            for classification in carriers {
                self.carrier(
                    classification,
                    &name,
                    "audio_block_skipfld",
                    &mut payloads_in_au,
                    &mut detail,
                    &mut parsed_header,
                    start_sample,
                    timestamp,
                    entry.header.sample_rate,
                );
            }
            match traversal {
                Ok(report) => {
                    self.report.carriage.skip_examined += report.examined_blocks as u64;
                    self.report.carriage.skip_unresolved += report.unresolved_blocks as u64;
                }
                Err(_) => {
                    // Existing traversal can be unsupported despite valid BSI/aux metadata.
                    // Preserve this uncertainty; do not call it proof of malformed audio.
                    self.report.carriage.skip_unresolved += u64::from(entry.header.audio_blocks);
                }
            }
        }
        if !information.is_empty() {
            let observation = self.programme(&information);
            detail.lfe_ownership = Some(observation.lfe_ownership);
            detail.programme_layout = Some(observation.layout);
        }
        self.objects(
            start_sample,
            timestamp,
            frames.first().map_or(48000, |f| f.header.sample_rate),
        );
        if addbsi {
            self.report.joc.addbsi_signaled_aus += 1;
        }
        for id in payloads_in_au {
            if let Some(payload) = self.report.emdf.payloads.iter_mut().find(|p| p.id == id) {
                payload.affected_aus += 1;
            }
        }
        if let Some(unit) = unit {
            if let Err(error) = eac3::validate_short_access_unit_convsync(bytes, frames, unit) {
                self.issue("INVALID_SHORT_BLOCK_GROUPING", error.to_string());
            }
            let candidate = if addbsi || detail.payload_ids.contains(&14) {
                eac3::parse_joc_access_unit(bytes, frames, unit)
            } else {
                Ok(None)
            };
            match candidate {
                Ok(Some(parsed)) => {
                    detail.joc_owner = frames
                        .get(parsed.carrier_frame)
                        .map(|entry| owner(entry.header));
                    if let Some((index, count)) = parsed_header {
                        self.report.validation.decoder_checked_aus += 1;
                        match eac3::validate_joc_access_unit_decoder_contract(
                            bytes, frames, unit, index, count,
                        ) {
                            Ok(()) => {
                                if self.validation_failed_au == [Some(au), Some(au)] {
                                    self.report.validation.decoder_admissible = Some(false);
                                } else {
                                    self.report.validation.decoder_admitted_aus += 1;
                                }
                                if self.report.validation.decoder_admissible.is_none() {
                                    self.report.validation.decoder_admissible = Some(true);
                                }
                            }
                            Err(error) => {
                                self.report.validation.decoder_admissible = Some(false);
                                // Admission failure is orthogonal to metadata syntax validity.
                                self.report
                                    .diagnostics
                                    .limitations
                                    .retain(|v| !v.starts_with("Decoder admission:"));
                                self.report
                                    .diagnostics
                                    .limitations
                                    .push(format!("Decoder admission: {error}"));
                            }
                        }
                    }
                }
                Ok(None) => {
                    if detail.payload_ids.contains(&14) {
                        self.issue(
                            "INVALID_JOC_CARRIAGE",
                            "JOC payload found but no admitted JOC access-unit carrier".into(),
                        );
                    }
                }
                Err(error) => self.issue("INVALID_JOC_CARRIAGE", error.to_string()),
            }
            self.report.eac3.programme_interval_count += 1;
            self.report.eac3.total_samples += u64::from(unit.samples);
            self.report.eac3.duration_seconds +=
                f64::from(unit.samples) / f64::from(unit.sample_rate);
            if self.report.eac3.dependent_ids_sequential.is_none() {
                self.report.eac3.dependent_ids_sequential = Some(true);
            }
        }
        if self.report.eac3.sample_rates_hz.len() > 1 {
            self.report.validation.frame_timing_continuity = "sample_rate_changes".into();
            self.report
                .validation
                .first_timing_discontinuity
                .get_or_insert(au);
        }
        let retained = observe(
            &mut self.report.eac3.topologies,
            detail.topology.clone(),
            au,
        ) & observe(
            &mut self.report.eac3.block_partitions,
            detail.block_partition.clone(),
            au,
        );
        self.report.diagnostics.aggregation_truncated |= !retained;
        unique(&mut self.report.eac3.frames_per_au, frames.len());
        if self.report.diagnostics.issue_count > starting_issues {
            self.report.validation.malformed_aus += 1;
            self.report.validation.stream_parse = "fail".into();
            detail.status = "malformed".into();
        } else if self.report.carriage.skip_unresolved > starting_unresolved {
            detail.status = "partial".into();
        }
        self.latest_au = Some(detail.clone());
        if self.options.aus
            && self
                .options
                .au_range
                .is_none_or(|(first, last)| au >= first && au <= last)
        {
            self.report.access_units.push(detail);
        }
        self.report.eac3.access_unit_count += 1;
        self.report.eac3.frame_count += frames.len() as u64;
        self.report.eac3.total_bytes += bytes.len() as u64;
        self.offset += bytes.len() as u64;
        self.report.diagnostics.max_au_bytes =
            self.report.diagnostics.max_au_bytes.max(bytes.len());
        self.report.diagnostics.max_au_frames =
            self.report.diagnostics.max_au_frames.max(frames.len());
    }

    fn component(&mut self, bsi: &eac3::BitstreamInformation, component_name: &str) {
        let header = bsi.header;
        let name = component_name.to_owned();
        if !self.report.eac3.components.iter().any(|c| c.owner == name) {
            self.report.eac3.components.push(ComponentSummary {
                owner: name.clone(),
                stream_type: match header.stream_type {
                    StreamType::Independent => "independent",
                    StreamType::Dependent => "dependent",
                    StreamType::LegacyIndependent => "legacy_independent",
                    StreamType::ConvertedIndependent => "converted_independent",
                }
                .into(),
                substream_id: header.substream_id,
                frame_count: 0,
                bytes: 0,
                samples: 0,
                duration_seconds: 0.0,
                bitrate_bps: 0.0,
                frame_bytes: Vec::new(),
                sample_rates_hz: Vec::new(),
                block_counts: Vec::new(),
                numblkscod: Vec::new(),
                acmod: Vec::new(),
                lfeon: Vec::new(),
                chanmap: Vec::new(),
                channel_locations: Vec::new(),
            });
        }
        let locations = eac3::inspect_channel_locations(bsi);
        if let Err(error) = &locations {
            self.issue("INVALID_CHANNEL_MAP", error.to_string());
        }
        let c = self
            .report
            .eac3
            .components
            .iter_mut()
            .find(|c| c.owner == name)
            .expect("component inserted");
        c.frame_count += 1;
        c.bytes += header.frame_size as u64;
        c.samples += u64::from(header.samples);
        c.duration_seconds += f64::from(header.samples) / f64::from(header.sample_rate);
        c.bitrate_bps = c.bytes as f64 * 8.0 / c.duration_seconds;
        unique(&mut c.frame_bytes, header.frame_size);
        unique(&mut c.sample_rates_hz, header.sample_rate);
        unique(&mut c.block_counts, header.audio_blocks);
        if header.stream_type != StreamType::LegacyIndependent && header.sample_rate >= 32_000 {
            unique(
                &mut c.numblkscod,
                match header.audio_blocks {
                    1 => 0,
                    2 => 1,
                    3 => 2,
                    _ => 3,
                },
            );
        }
        unique(&mut c.acmod, bsi.audio_coding_mode);
        unique(&mut c.lfeon, bsi.lfe_on);
        unique(&mut c.chanmap, bsi.channel_map);
        if let Ok(locations) = locations {
            unique(
                &mut c.channel_locations,
                locations
                    .into_iter()
                    .map(|l| l.label().to_owned())
                    .collect(),
            );
        }
    }

    fn programme(&mut self, infos: &[eac3::BitstreamInformation]) -> ProgrammeObservation {
        let au = self.next_au_index();
        let first = &infos[0];
        if let Err(error) = eac3::inspect_programme_channels(first, &infos[1..]) {
            self.report.eac3.channel_conflicts += 1;
            self.issue("CHANNEL_OWNERSHIP_CONFLICT", error.to_string());
            self.report.diagnostics.aggregation_truncated |= !observe(
                &mut self.report.eac3.lfe_ownership,
                "ambiguous_invalid".into(),
                au,
            );
            return ProgrammeObservation {
                lfe_ownership: "ambiguous_invalid".into(),
                layout: "Extended (ambiguous programme channels)".into(),
            };
        }
        let (full_band, lfe_location) = eac3::inspect_programme_channels(first, &infos[1..])
            .expect("programme channels were validated above");
        let independent_lfe = eac3::inspect_channel_locations(first)
            .is_ok_and(|v| v.iter().any(|l| matches!(l, eac3::ChannelLocation::Lfe(_))));
        let dependent_owner = infos.iter().skip(1).find(|info| {
            eac3::inspect_channel_locations(info)
                .is_ok_and(|v| v.iter().any(|l| matches!(l, eac3::ChannelLocation::Lfe(_))))
        });
        let semantics = match (independent_lfe, dependent_owner) {
            (false, None) => "absent".into(),
            (true, None) => format!("independent_owned:{}", owner(first.header)),
            (false, Some(info)) => format!("dependent_supplementation:{}", owner(info.header)),
            (true, Some(info)) => format!("dependent_replacement:{}", owner(info.header)),
        };
        let layout = programme_layout_name(&full_band, lfe_location);
        self.report.diagnostics.aggregation_truncated |=
            !observe(&mut self.report.eac3.lfe_ownership, semantics.clone(), au);
        self.report.diagnostics.aggregation_truncated |=
            !observe(&mut self.report.eac3.programme_layouts, layout.clone(), au);
        ProgrammeObservation {
            lfe_ownership: semantics,
            layout,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn carrier(
        &mut self,
        classification: CarrierClassification,
        owner: &str,
        location: &str,
        seen: &mut BTreeSet<u64>,
        detail: &mut AuDetail,
        header: &mut Option<(u8, u8)>,
        sample: u64,
        seconds: f64,
        rate: u32,
    ) {
        let aux = location == "frame_end_auxdatae";
        match classification {
            CarrierClassification::NonEmdf => {
                if aux {
                    self.report.carriage.aux_non_emdf += 1;
                } else {
                    self.report.carriage.skip_non_emdf += 1;
                }
            }
            CarrierClassification::Parsed(parsed) => {
                if aux {
                    self.report.carriage.aux_parsed += 1;
                } else {
                    self.report.carriage.skip_parsed += 1;
                }
                let au = self.next_au_index();
                self.report.diagnostics.aggregation_truncated |= !observe(
                    &mut self.report.carriage.locations,
                    CarrierLocation {
                        owner: owner.into(),
                        location: location.into(),
                    },
                    au,
                );
                self.emdf(
                    &parsed.container,
                    owner,
                    seen,
                    detail,
                    header,
                    sample,
                    seconds,
                    rate,
                );
            }
            other => {
                if aux {
                    self.report.carriage.aux_malformed += 1;
                } else {
                    self.report.carriage.skip_malformed += 1;
                }
                let message = match other {
                    CarrierClassification::Malformed(error) => error.to_string(),
                    CarrierClassification::TrailingData { .. } => {
                        "EMDF declared length does not exhaust the carrier".into()
                    }
                    _ => unreachable!(),
                };
                self.issue("MALFORMED_EMDF", format!("{owner} {location}: {message}"));
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn emdf(
        &mut self,
        container: &EmdfContainer,
        owner: &str,
        seen: &mut BTreeSet<u64>,
        detail: &mut AuDetail,
        header: &mut Option<(u8, u8)>,
        _sample: u64,
        _seconds: f64,
        _rate: u32,
    ) {
        let au = self.next_au_index();
        if container.payloads.iter().any(|p| p.id == 14) {
            self.validate_profile(container, JocValidationProfile::EtsiStrict);
            self.validate_profile(container, JocValidationProfile::ObservedVendorCompat);
        }
        self.report.emdf.containers += 1;
        self.report.diagnostics.aggregation_truncated |= !observe(
            &mut self.report.emdf.payload_orders,
            container.payloads.iter().map(|p| p.id).collect(),
            au,
        );
        for p in &container.payloads {
            seen.insert(p.id);
            unique(&mut detail.payload_ids, p.id);
            if !self.report.emdf.payloads.iter().any(|v| v.id == p.id)
                && self.report.emdf.payloads.len() < MAX_VARIANTS
            {
                self.report.emdf.payloads.push(PayloadSummary {
                    id: p.id,
                    name: match p.id {
                        11 => Some("OAMD".into()),
                        14 => Some("JOC".into()),
                        _ => None,
                    },
                    occurrences: 0,
                    affected_aus: 0,
                    first_au: au,
                    length_min: p.data.len(),
                    length_max: p.data.len(),
                    unique_lengths: 0,
                    length_counts: BTreeMap::new(),
                });
            }
            if let Some(summary) = self.report.emdf.payloads.iter_mut().find(|v| v.id == p.id) {
                summary.occurrences += 1;
                summary.length_min = summary.length_min.min(p.data.len());
                summary.length_max = summary.length_max.max(p.data.len());
                // EMDF sizes are syntax bounded; keep an exact bounded length histogram.
                *summary.length_counts.entry(p.data.len()).or_default() += 1;
                summary.unique_lengths = summary.length_counts.len();
            } else {
                self.report.diagnostics.aggregation_truncated = true;
            }
            let c = &p.config;
            self.report.diagnostics.aggregation_truncated |= !observe(
                &mut self.report.emdf.configurations,
                PayloadConfiguration {
                    payload_id: p.id,
                    sample_offset: c.sample_offset,
                    duration: c.duration,
                    group_id: c.group_id,
                    codecdatae: c.codec_data_present,
                    discard_unknown_payload: c.discard_unknown_payload,
                    payload_frame_aligned: c.payload_frame_aligned,
                    create_duplicate: c.create_duplicate,
                    remove_duplicate: c.remove_duplicate,
                    priority: c.priority,
                    proc_allowed: c.processing_allowed,
                },
                au,
            );
            if p.id == 14 {
                self.report.joc.payload_occurrences += 1;
                unique(&mut self.report.joc.owners, owner.into());
                unique(&mut self.report.carriage.joc_owners, owner.into());
                match openjoc_joc::parse_joc_payload(&p.data) {
                    Ok(joc) => {
                        self.report.joc.present = true;
                        self.report.joc.parsed_payloads += 1;
                        self.report.diagnostics.aggregation_truncated |= !observe(
                            &mut self.report.joc.profiles,
                            profile(joc.header.downmix_index),
                            au,
                        );
                        unique(
                            &mut self.report.joc.coded_object_counts,
                            joc.header.object_count,
                        );
                        unique(
                            &mut self.report.joc.reconstruction_rows,
                            joc.header.object_count,
                        );
                        unique(&mut detail.profile_indices, joc.header.downmix_index);
                        unique(&mut detail.object_counts, joc.header.object_count);
                        *header = Some((joc.header.downmix_index, joc.header.channel_count));
                    }
                    Err(error) => {
                        if let openjoc_joc::JocParseError::ReservedDownmix { index } = error {
                            self.report.diagnostics.aggregation_truncated |=
                                !observe(&mut self.report.joc.profiles, profile(index), au);
                            unique(&mut detail.profile_indices, index);
                            self.issue("UNSUPPORTED_JOC_PROFILE", error.to_string());
                        } else {
                            self.issue("MALFORMED_JOC_METADATA", error.to_string());
                        }
                    }
                }
            }
            if p.id == 11 {
                self.report.scene.metadata_present = true;
                unique(&mut self.report.carriage.oamd_owners, owner.into());
                let config = openjoc_oamd::OamdDecoderConfig::with_trim_configuration_count(
                    self.options.trim_configuration_count,
                );
                let result =
                    openjoc_oamd::parse_oamd_payload_with_config(&p.data, config).or_else(|_| {
                        openjoc_oamd::parse_oamd_payload_with_profile(
                            &p.data,
                            config,
                            openjoc_oamd::OamdParseProfile::ObservedVendorCompat,
                            11,
                        )
                    });
                match result {
                    Ok(payload) => {
                        self.report.scene.parsed_payloads += 1;
                        unique(
                            &mut self.report.scene.metadata_object_counts,
                            payload.prefix.object_count,
                        );
                        for complexity in detail.complexity_indices.clone() {
                            if let Err(error) = eac3::validate_complexity_index(
                                complexity,
                                payload.prefix.object_count,
                            ) {
                                self.issue("INVALID_COMPLEXITY", error.to_string());
                            }
                        }

                        for element in payload.elements {
                            match element.element {
                                OamdElement::Objects(objects) => self
                                    .pending_objects
                                    .push((objects, payload.object_classes.clone())),
                                OamdElement::OpaqueObservedKnownElement(_) => {
                                    self.report.scene.opaque_trim_present = true;
                                }
                                _ => {}
                            }
                        }
                    }
                    Err(error) => self.issue("MALFORMED_OAMD_METADATA", error.to_string()),
                }
            }
        }
    }

    fn validate_profile(&mut self, emdf: &EmdfContainer, profile: JocValidationProfile) {
        let au = self.next_au_index();
        let (failed, deviations) = match openjoc_emdf::validate_joc_profile_for(emdf, profile) {
            Ok(value) => (false, value.deviations),
            Err(failure) => (true, failure.deviations),
        };
        let profile_slot = match profile {
            JocValidationProfile::EtsiStrict => 0,
            JocValidationProfile::ObservedVendorCompat => 1,
        };
        let target = match profile {
            JocValidationProfile::EtsiStrict => &mut self.report.validation.etsi_strict,
            JocValidationProfile::ObservedVendorCompat => {
                &mut self.report.validation.deployed_compatibility
            }
        };
        if self.validation_au[profile_slot] != Some(au) {
            target.tested_aus += 1;
            self.validation_au[profile_slot] = Some(au);
        }
        if failed && self.validation_failed_au[profile_slot] != Some(au) {
            self.validation_failed_au[profile_slot] = Some(au);
            target.failed_aus += 1;
            target.first_failing_au.get_or_insert(au);
        }
        target.status = if target.failed_aus == 0 {
            "pass"
        } else {
            "fail"
        }
        .into();
        for d in deviations {
            let field = d.field.to_string();
            let observed = d.actual.to_string();
            let expected = d.expected_by_etsi.to_string();
            if !self.deviation_seen.insert((
                profile_slot,
                d.payload_id,
                field.clone(),
                observed.clone(),
                expected.clone(),
            )) {
                continue;
            }
            if let Some(v) = target.deviations.iter_mut().find(|v| {
                v.payload_id == d.payload_id
                    && v.field == field
                    && v.observed == observed
                    && v.expected == expected
            }) {
                v.affected_aus += 1;
            } else if target.deviations.len() < MAX_VARIANTS {
                target.deviations.push(DeviationSummary {
                    payload_id: d.payload_id,
                    field,
                    observed,
                    expected,
                    affected_aus: 1,
                    first_au: au,
                });
            } else {
                self.report.diagnostics.aggregation_truncated = true;
            }
        }
    }

    fn objects(&mut self, sample: u64, seconds: f64, rate: u32) {
        let au = self.next_au_index();
        let pending = std::mem::take(&mut self.pending_objects);
        if pending.is_empty() {
            return;
        }
        self.report.scene.object_metadata_present = true;
        self.report
            .scene
            .dynamic_metadata_detected
            .get_or_insert(false);

        for (objects, classes) in pending {
            for (index, updates) in objects.objects.into_iter().enumerate() {
                for (block, update) in updates.into_iter().enumerate() {
                    let offset = objects
                        .timing
                        .blocks
                        .get(block)
                        .map_or(0, |v| v.start_sample);
                    // Keep events beyond the interval for chronological merging with the next AU.
                    let when = ChangeLocation {
                        au,
                        sample: sample + u64::from(offset),
                        seconds: seconds + f64::from(offset) / f64::from(rate),
                    };
                    self.future_events.insert(
                        (when.sample, index),
                        SceneEvent {
                            update,
                            class: classes.get(index).copied(),
                            when,
                        },
                    );
                }
            }
        }
        self.flush_events(sample + 1536);
    }

    fn flush_events(&mut self, before: u64) {
        while self
            .future_events
            .first_key_value()
            .is_some_and(|((sample, _), _)| *sample < before)
        {
            let Some((
                (current, index),
                SceneEvent {
                    update,
                    class,
                    when,
                },
            )) = self.future_events.pop_first()
            else {
                break;
            };
            let au = when.au;
            if self
                .previous_metadata_sample
                .is_some_and(|previous| current < previous)
            {
                self.report.validation.metadata_timing_continuity = "discontinuous".into();
                self.report
                    .validation
                    .first_timing_discontinuity
                    .get_or_insert(au);
            } else if self.report.validation.metadata_timing_continuity == "unavailable" {
                self.report.validation.metadata_timing_continuity = "continuous".into();
            }
            self.previous_metadata_sample = Some(current);
            let stats = self
                .object_stats
                .entry(index)
                .or_insert_with(|| ObjectStatistics {
                    object_index: index,
                    first_active_au: None,
                    last_active_au: None,
                    metadata_update_count: 0,
                    dynamic: false,
                    first_change: None,
                    last_change: None,
                    position_min: None,
                    position_max: None,
                });
            stats.metadata_update_count += 1;
            self.report.scene.metadata_update_count += 1;
            // Compare resolved observable properties, excluding opaque bytes and coding representation.
            let changed = self.previous_objects.get(&index).is_some_and(|old| {
                old.active != update.active
                    || (update.active
                        && (old.basic != update.basic
                            || old.render.position != update.render.position
                            || old.render.distance != update.render.distance
                            || old.render.size != update.render.size
                            || old.render.zones != update.render.zones
                            || old.render.screen_anchor != update.render.screen_anchor
                            || old.render.screen_factor != update.render.screen_factor
                            || old.render.depth_factor != update.render.depth_factor
                            || old.render.channel_lock != update.render.channel_lock))
            });
            if changed {
                stats.dynamic = true;
                stats.first_change.get_or_insert_with(|| when.clone());
                stats.last_change = Some(when.clone());
                self.report.scene.dynamic_metadata_detected = Some(true);
                if self
                    .report
                    .scene
                    .first_change
                    .as_ref()
                    .is_none_or(|v| when.sample < v.sample)
                {
                    self.report.scene.first_change = Some(when.clone());
                }
                if self
                    .report
                    .scene
                    .last_change
                    .as_ref()
                    .is_none_or(|v| when.sample >= v.sample)
                {
                    self.report.scene.last_change = Some(when);
                }
            }
            if update.active {
                stats.first_active_au.get_or_insert(au);
                stats.last_active_au = Some(au);
                if class == Some(openjoc_oamd::ObjectClass::Dynamic) {
                    let p = [
                        update.render.position.x,
                        update.render.position.y,
                        update.render.position.z,
                    ];
                    let min = stats.position_min.get_or_insert(p);
                    let max = stats.position_max.get_or_insert(p);
                    for axis in 0..3 {
                        min[axis] = min[axis].min(p[axis]);
                        max[axis] = max[axis].max(p[axis]);
                    }
                }
            }
            self.previous_objects.insert(index, update);
        }
    }

    pub fn finish(mut self) -> StreamInspection {
        self.flush_events(u64::MAX);
        if self.report.eac3.access_unit_count == 0 && self.report.validation.malformed_aus == 0 {
            self.framing_failure("No E-AC-3 access units".into());
        }
        self.report.joc.presence_status = if self.report.joc.present {
            "present"
        } else if self.report.joc.payload_occurrences > 0 {
            "malformed_payload"
        } else if self.report.carriage.skip_unresolved > 0
            || !self.report.diagnostics.complete
            || self.report.validation.malformed_aus > 0
        {
            "unavailable"
        } else {
            "not_present"
        }
        .into();
        if self.report.carriage.skip_unresolved > 0 && self.report.validation.stream_parse == "pass"
        {
            self.report.validation.stream_parse = "partial".into();
        }
        self.report.input.format = if self.report.joc.present {
            if self.report.eac3.legacy_core {
                "eac3_joc_legacy_ac3_core"
            } else {
                "eac3_joc"
            }
        } else if self.report.joc.payload_occurrences > 0 {
            "malformed_joc_metadata"
        } else if self.report.eac3.frame_count > 0 {
            "eac3"
        } else {
            "unknown"
        }
        .into();
        if self.report.eac3.duration_seconds > 0.0 {
            self.report.eac3.bitrate_bps =
                Some(self.report.eac3.total_bytes as f64 * 8.0 / self.report.eac3.duration_seconds);
        }
        if self.report.validation.malformed_aus > 0 {
            self.report.validation.decoder_admissible = Some(false);
        }
        if self.report.validation.decoder_admissible == Some(true)
            && self.report.validation.decoder_admitted_aus != self.report.eac3.access_unit_count
        {
            self.report.validation.decoder_admissible = None;
        }
        self.report.diagnostics.retained_au_details = self.report.access_units.len();
        self.report.emdf.payloads.sort_by_key(|p| p.id);
        if self.options.objects {
            self.report.scene.objects = self.object_stats.into_values().collect();
        }
        self.report
    }
}
