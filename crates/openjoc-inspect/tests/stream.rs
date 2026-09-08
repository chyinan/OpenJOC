// pattern: Imperative Shell

mod support;
use openjoc_inspect::{InspectionOptions, inspect_reader};
use support::{
    absent_joc, active_object_oamd, five_channel_audio_frame, inactive_oamd, joc_emdf,
    joc_emdf_for_profile, joc_frame, short_mono_frame_with_options, skip_field_joc_frame,
};

#[test]
fn profiles_are_parsed_without_reconstruction_or_filename_heuristics() {
    for (index, name, channels) in [
        (0, "five_x", 5),
        (1, "flat_7x", 7),
        (2, "five_x_plus_two", 7),
        (3, "five_x_phase", 5),
        (4, "five_x_plus_two_phase", 7),
    ] {
        let mut joc = absent_joc();
        joc[0] = index << 5;
        let frame = joc_frame(&joc_emdf(&inactive_oamd(), &joc), 1);
        let r = inspect_reader(frame.as_slice(), InspectionOptions::default());
        assert!(r.joc.present, "idx{index}: {:?}", r.diagnostics);
        let p = &r.joc.profiles[0].value;
        assert_eq!(p.profile, name);
        assert_eq!(p.reconstruction_input_count, Some(channels));
        assert_eq!(p.carriers.len(), usize::from(channels));
        assert!(!p.carriers.iter().any(|c| c == "LFE"));
        assert_eq!(r.validation.etsi_strict.status, "pass");
        assert!(r.access_units.is_empty());
    }
}

#[test]
fn ordinary_short_and_mixed_partitions_are_not_normalized() {
    for partition in [vec![1; 6], vec![2; 3], vec![3; 2], vec![1, 2, 3]] {
        let bytes = partition
            .iter()
            .enumerate()
            .flat_map(|(i, b)| short_mono_frame_with_options(0, *b, i == 0, None, None, None))
            .collect::<Vec<_>>();
        let r = inspect_reader(
            bytes.as_slice(),
            InspectionOptions {
                aus: true,
                ..InspectionOptions::default()
            },
        );
        assert_eq!(r.eac3.access_unit_count, 1);
        assert_eq!(r.eac3.total_samples, 1536);
        assert_eq!(r.eac3.block_partitions[0].value, partition);
        assert_eq!(r.input.format, "eac3");
        assert!(!r.joc.present);
        assert_eq!(r.validation.stream_parse, "pass", "{:?}", r.diagnostics);
    }
}

#[test]
fn strict_failure_does_not_hide_joc_or_define_carriage() {
    for vendor in [false, true] {
        let emdf = joc_emdf_for_profile(&inactive_oamd(), &absent_joc(), vendor);
        let bytes = skip_field_joc_frame(&emdf);
        let r = inspect_reader(bytes.as_slice(), InspectionOptions::default());
        assert!(r.joc.present, "{:?}", r.diagnostics);
        assert_eq!(
            r.carriage.locations[0].value.location,
            "audio_block_skipfld"
        );
        assert_eq!(
            r.validation.etsi_strict.status,
            if vendor { "fail" } else { "pass" }
        );
        assert_eq!(r.validation.deployed_compatibility.status, "pass");
        assert_eq!(r.emdf.payloads[0].affected_aus, 1);
        assert_eq!(r.emdf.payload_orders[0].value, [11, 14]);
    }
}

#[test]
fn truncated_tail_preserves_complete_preceding_census() {
    let frame = five_channel_audio_frame(&joc_emdf(&inactive_oamd(), &absent_joc()));
    let bytes = [frame.as_slice(), frame.as_slice(), &frame[..31]].concat();
    let r = inspect_reader(bytes.as_slice(), InspectionOptions::default());
    assert_eq!(r.eac3.access_unit_count, 2);
    assert_eq!(r.emdf.payloads[0].occurrences, 2);
    assert_eq!(r.validation.malformed_aus, 1);
    assert_eq!(r.diagnostics.first_failure.as_ref().unwrap().au, 2);
    assert!(!r.diagnostics.complete);
}

#[test]
fn reserved_profiles_and_malformed_emdf_are_reported_without_panics() {
    for index in 5..=7 {
        let mut joc = absent_joc();
        joc[0] = index << 5;
        let bytes = joc_frame(&joc_emdf(&inactive_oamd(), &joc), 1);
        let r = inspect_reader(bytes.as_slice(), InspectionOptions::default());
        assert_eq!(r.joc.profiles[0].value.profile, "reserved");
        assert_eq!(r.joc.profiles[0].value.profile_index, index);
        assert_eq!(r.validation.malformed_aus, 1);
    }
    let bytes = joc_frame(&[0x58, 0x38, 0xff, 0xff], 1);
    let r = inspect_reader(bytes.as_slice(), InspectionOptions::default());
    assert_eq!(r.carriage.aux_malformed, 1);
    assert_eq!(r.joc.presence_status, "unavailable");
}

#[test]
fn metadata_changes_are_observed_and_static_repeats_stay_static() {
    let frame =
        |active| five_channel_audio_frame(&joc_emdf(&active_object_oamd(active), &absent_joc()));
    for (states, dynamic) in [
        (vec![true, true, true], false),
        (vec![false, true, true], true),
    ] {
        let bytes = states.into_iter().flat_map(frame).collect::<Vec<_>>();
        let r = inspect_reader(
            bytes.as_slice(),
            InspectionOptions {
                objects: true,
                ..InspectionOptions::default()
            },
        );
        assert_eq!(
            r.scene.dynamic_metadata_detected,
            Some(dynamic),
            "{:?}",
            r.diagnostics
        );
        assert_eq!(r.scene.objects[0].dynamic, dynamic);
        if dynamic {
            assert_eq!(r.scene.first_change.as_ref().unwrap().au, 1);
        }
        assert!(!r.scene.original_authored_identity_recovered);
    }
}

#[test]
fn deterministic_schema_and_range_selection_preserve_summary() {
    let frame = five_channel_audio_frame(&joc_emdf(&inactive_oamd(), &absent_joc()));
    let bytes = frame.repeat(3);
    let options = InspectionOptions {
        aus: true,
        au_range: Some((1, 1)),
        ..InspectionOptions::default()
    };
    let r = inspect_reader(bytes.as_slice(), options);
    assert_eq!(r.access_units.len(), 1);
    assert_eq!(r.access_units[0].au, 1);
    assert_eq!(r.eac3.access_unit_count, 3);
    assert_eq!(
        serde_json::to_vec(&r).unwrap(),
        serde_json::to_vec(&inspect_reader(bytes.as_slice(), options)).unwrap()
    );
    assert_eq!(r.diagnostics.max_au_bytes, frame.len());
    assert!(!r.validation.render_verified);
}

#[test]
fn multi_independent_programmes_keep_distinct_dependent_owners() {
    let bytes = [
        support::topology_frame(0, 0, 2, false, None, None),
        support::topology_frame(1, 0, 1, false, None, None),
        support::topology_frame(0, 1, 2, false, None, None),
        support::topology_frame(1, 0, 1, false, None, None),
    ]
    .concat();
    let r = inspect_reader(bytes.as_slice(), InspectionOptions::default());
    assert_eq!(r.eac3.access_unit_count, 1);
    assert_eq!(r.eac3.total_samples, 1536);
    assert_eq!(r.eac3.topologies[0].value, ["I0", "D0", "I1", "I1/D0"]);
    assert_eq!(r.eac3.components.len(), 4);
    assert_eq!(r.validation.malformed_aus, 0);
    assert_eq!(r.joc.presence_status, "unavailable");
}

#[test]
fn multi_dependent_owner_and_signaling_are_independent() {
    let emdf = joc_emdf(&inactive_oamd(), &absent_joc());
    for wrong_owner in [false, true] {
        let bytes = [
            support::topology_frame(0, 0, 7, false, None, None),
            support::topology_frame(
                1,
                0,
                1,
                false,
                Some(0x8000),
                wrong_owner.then_some(emdf.as_slice()),
            ),
            support::topology_frame(
                1,
                1,
                1,
                false,
                Some(0x4000),
                (!wrong_owner).then_some(emdf.as_slice()),
            ),
        ]
        .concat();
        let r = inspect_reader(
            bytes.as_slice(),
            InspectionOptions {
                aus: true,
                ..InspectionOptions::default()
            },
        );
        assert!(r.joc.present);
        assert_eq!(r.eac3.topologies[0].value, ["I0", "D0", "D1"]);
        assert_eq!(r.validation.etsi_strict.status, "pass");
        assert_eq!(r.validation.etsi_strict.tested_aus, 1);
        assert_eq!(
            r.carriage.joc_owners,
            [if wrong_owner { "D0" } else { "D1" }]
        );
        if wrong_owner {
            assert!(
                r.diagnostics
                    .issues
                    .iter()
                    .any(|i| i.code == "INVALID_JOC_CARRIAGE")
            );
        } else {
            assert_eq!(r.access_units[0].joc_owner.as_deref(), Some("D1"));
        }
    }
}

#[test]
fn invalid_dependent_ids_keep_census_and_resume_next_interval() {
    let emdf = joc_emdf(&inactive_oamd(), &absent_joc());
    let bytes = [
        support::topology_frame(0, 0, 7, false, None, None),
        support::topology_frame(1, 1, 1, false, None, Some(&emdf)),
        five_channel_audio_frame(&emdf),
    ]
    .concat();
    let r = inspect_reader(bytes.as_slice(), InspectionOptions::default());
    assert_eq!(r.eac3.access_unit_count, 2);
    assert_eq!(r.emdf.payloads[0].occurrences, 2);
    assert_eq!(r.validation.malformed_aus, 1);
    assert_eq!(r.eac3.dependent_ids_sequential, Some(false));
}

#[test]
fn lfe_ownership_matches_decoder_semantics_and_legacy_name_is_truthful() {
    let emdf = joc_emdf(&inactive_oamd(), &absent_joc());
    for (base_lfe, dependent_lfe, expected) in [
        (false, false, "absent"),
        (true, false, "independent_owned:I0"),
        (false, true, "dependent_supplementation:D0"),
        (true, true, "dependent_replacement:D0"),
    ] {
        let bytes = [
            support::topology_frame(0, 0, 7, base_lfe, None, None),
            support::topology_frame(1, 0, 1, dependent_lfe, None, Some(&emdf)),
        ]
        .concat();
        let r = inspect_reader(bytes.as_slice(), InspectionOptions::default());
        assert_eq!(
            r.eac3.lfe_ownership[0].value, expected,
            "{:?}",
            r.diagnostics
        );
    }
    let bytes = [
        support::decodable_ac3_frame_for(7, true, None, false),
        support::topology_frame(1, 0, 1, false, None, Some(&emdf)),
    ]
    .concat();
    let r = inspect_reader(bytes.as_slice(), InspectionOptions::default());
    assert!(r.eac3.legacy_core);
    assert_eq!(r.input.format, "eac3_joc_legacy_ac3_core");
    assert!(
        openjoc_inspect::format_summary(&r, InspectionOptions::default())
            .contains("E-AC-3 JOC with legacy/original-syntax AC-3 core")
    );
}

#[test]
fn human_and_json_contract_goldens() {
    let frame = five_channel_audio_frame(&joc_emdf(&inactive_oamd(), &absent_joc()));
    let options = InspectionOptions::default();
    let r = inspect_reader(frame.as_slice(), options);
    let human = openjoc_inspect::format_summary(&r, options);
    let value = serde_json::to_value(&r).unwrap();
    for key in [
        "input",
        "container",
        "eac3",
        "joc",
        "carriage",
        "emdf",
        "scene",
        "validation",
        "diagnostics",
    ] {
        assert!(value[key].is_object());
    }
    let contract = serde_json::json!({
        "schema_version": r.schema_version,
        "format": r.input.format,
        "topology": r.eac3.topologies,
        "partitions": r.eac3.block_partitions,
        "profiles": r.joc.profiles,
        "etsi_strict": r.validation.etsi_strict.status,
        "deployed_compatibility": r.validation.deployed_compatibility.status,
        "dynamic_metadata_detected": r.scene.dynamic_metadata_detected,
        "retained_au_details": r.diagnostics.retained_au_details,
    });
    let json = serde_json::to_string_pretty(&contract).unwrap() + "\n";
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/goldens");
    if std::env::var_os("UPDATE_INSPECT_GOLDENS").is_some() {
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("summary.txt"), &human).unwrap();
        std::fs::write(directory.join("contract.json"), &json).unwrap();
    }
    assert_eq!(
        human,
        std::fs::read_to_string(directory.join("summary.txt"))
            .unwrap()
            .replace("\r\n", "\n")
    );
    assert_eq!(
        json,
        std::fs::read_to_string(directory.join("contract.json"))
            .unwrap()
            .replace("\r\n", "\n")
    );
}

#[test]
fn malformed_short_group_and_conflicting_channels_are_bounded() {
    let bytes = [
        short_mono_frame_with_options(0, 2, true, None, None, None),
        short_mono_frame_with_options(0, 3, false, None, None, None),
    ]
    .concat();
    let r = inspect_reader(bytes.as_slice(), InspectionOptions::default());
    assert_eq!(r.validation.malformed_aus, 1);
    assert_eq!(r.eac3.block_partitions[0].value, [2, 3]);
    let bytes = [
        support::topology_frame(0, 0, 7, false, None, None),
        support::topology_frame(1, 0, 1, false, None, None),
        support::topology_frame(1, 1, 1, false, None, None),
    ]
    .concat();
    let r = inspect_reader(bytes.as_slice(), InspectionOptions::default());
    assert_eq!(r.eac3.channel_conflicts, 1);
    assert!(
        r.diagnostics
            .issues
            .iter()
            .any(|i| i.code == "CHANNEL_OWNERSHIP_CONFLICT")
    );
}

#[test]
fn mp4_and_fragmented_cmaf_reuse_the_existing_demux_path() {
    use std::{fs, process::Command};
    if Command::new("ffprobe").arg("-version").output().is_err()
        || Command::new("ffmpeg").arg("-version").output().is_err()
    {
        return;
    }
    let root =
        std::env::temp_dir().join(format!("openjoc-inspect-containers-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let frame = five_channel_audio_frame(&joc_emdf(&inactive_oamd(), &absent_joc()));
    let bytes = [
        support::cmaf_init_segment_for_e2e(&support::cmaf_dec3_box()),
        support::cmaf_fragment_for_e2e(&frame, 1, 0),
        support::cmaf_fragment_for_e2e(&frame, 2, 1536),
    ]
    .concat();
    let fragmented = root.join("fragmented.mp4");
    fs::write(&fragmented, bytes).unwrap();
    let f = openjoc_inspect::inspect_path(&fragmented, InspectionOptions::default()).unwrap();
    assert_eq!(f.eac3.access_unit_count, 2, "{:?}", f.diagnostics);
    assert!(f.joc.present);
    assert_eq!(f.container.kind, "iso_bmff");
    assert_eq!(f.container.sample_entry.as_deref(), Some("ec-3"));
    assert_eq!(f.container.timescale, Some(48000));
    let ordinary = root.join("ordinary.mp4");
    let result = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(&fragmented)
        .args(["-c:a", "copy", "-use_editlist", "0", "-y"])
        .arg(&ordinary)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let o = openjoc_inspect::inspect_path(&ordinary, InspectionOptions::default()).unwrap();
    // Suppress muxer-generated edit lists so both synthetic samples are presented.
    assert!(o.eac3.access_unit_count > 0);
    assert!(o.joc.present);
    assert_eq!(o.container.sample_count, Some(o.eac3.access_unit_count));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn whole_stream_admission_requires_coverage_and_empty_presence_is_unknown() {
    let bytes = [
        five_channel_audio_frame(&joc_emdf(&inactive_oamd(), &absent_joc())),
        support::topology_frame(0, 0, 2, false, None, None),
    ]
    .concat();
    let r = inspect_reader(bytes.as_slice(), InspectionOptions::default());
    assert_eq!(r.validation.decoder_checked_aus, 1);
    assert_eq!(r.validation.decoder_admissible, None);
    let empty = inspect_reader(&[][..], InspectionOptions::default());
    assert_eq!(empty.joc.presence_status, "unavailable");
    assert!(!empty.diagnostics.complete);
}

#[test]
fn truncated_short_candidate_counts_one_malformed_interval() {
    let frame = short_mono_frame_with_options(0, 2, true, None, None, None);
    let bytes = [frame.as_slice(), &frame[..8]].concat();
    let r = inspect_reader(bytes.as_slice(), InspectionOptions::default());
    assert_eq!(r.validation.malformed_aus, 1);
    assert_eq!(r.diagnostics.first_failure.as_ref().unwrap().au, 0);
    assert!(!r.diagnostics.complete);
    assert_ne!(r.eac3.dependent_ids_sequential, Some(false));
}

#[test]
fn malformed_bit_mutations_return_reports_without_crashing() {
    let source = five_channel_audio_frame(&joc_emdf(&active_object_oamd(true), &absent_joc()));
    for index in (0..source.len()).step_by(11) {
        let mut bytes = source.clone();
        bytes[index] ^= 0xff;
        let r = inspect_reader(bytes.as_slice(), InspectionOptions::default());
        assert!(r.diagnostics.issues.len() <= 64);
        assert!(r.access_units.is_empty());
    }
}

#[test]
fn complexity_uses_oamd_programme_count_not_joc_row_count() {
    let mut joc = absent_joc();
    joc[1] |= 0x80; // Two absent JOC rows; existing padding supplies the second flag.
    let bytes = five_channel_audio_frame(&joc_emdf(&inactive_oamd(), &joc));
    let r = inspect_reader(bytes.as_slice(), InspectionOptions::default());
    assert_eq!(r.joc.coded_object_counts, [2]);
    assert_eq!(r.scene.metadata_object_counts, [1]);
    assert_eq!(r.joc.complexity_indices, [1]);
    assert!(
        !r.diagnostics
            .issues
            .iter()
            .any(|i| i.code == "INVALID_COMPLEXITY")
    );
}

#[test]
fn extra_payload_orders_lengths_and_au_occurrence_denominators_are_exact() {
    let oamd = inactive_oamd();
    let joc = absent_joc();
    let a = support::emdf_payloads(&[(11, &oamd), (14, &joc), (2, &[5]), (1, &[7])], false);
    let b = support::emdf_payloads(&[(14, &joc), (2, &[5, 6]), (11, &oamd), (1, &[7])], false);
    let bytes = [joc_frame(&a, 1), joc_frame(&b, 1)].concat();
    let r = inspect_reader(bytes.as_slice(), InspectionOptions::default());
    assert_eq!(r.emdf.payload_orders.len(), 2);
    assert_eq!(r.emdf.payload_orders[0].value, [11, 14, 2, 1]);
    assert_eq!(r.emdf.payload_orders[1].first_au, 1);
    let extra = r.emdf.payloads.iter().find(|p| p.id == 2).unwrap();
    assert_eq!(
        (
            extra.occurrences,
            extra.affected_aus,
            extra.length_min,
            extra.length_max,
            extra.unique_lengths
        ),
        (2, 2, 1, 2, 2)
    );

    let emdf = joc_emdf_for_profile(&oamd, &joc, true);
    let duplicate = [
        support::topology_frame(0, 0, 7, false, None, Some(&emdf)),
        support::topology_frame(1, 0, 1, false, None, Some(&emdf)),
    ]
    .concat();
    let r = inspect_reader(duplicate.as_slice(), InspectionOptions::default());
    assert_eq!(r.emdf.payloads[0].occurrences, 2);
    assert_eq!(r.emdf.payloads[0].affected_aus, 1);
    assert_eq!(r.validation.etsi_strict.tested_aus, 1);
    assert_eq!(r.validation.etsi_strict.failed_aus, 1);
    assert!(
        r.validation
            .etsi_strict
            .deviations
            .iter()
            .all(|d| d.affected_aus == 1)
    );
}
