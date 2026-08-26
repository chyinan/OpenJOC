use openjoc_adm::{
    AdmError, AdmExportPlan, AdmPolicy, StreamingAdmWriter, build_export, validate_adm_bwf,
    write_adm_bwf,
};
use openjoc_joc::ReconstructionBasis;
use openjoc_scene::{
    BindingCodecProfile, DecodedJocBindingFacts, Extent3, MetadataUpdate, OamdBindingObjectClass,
    ObjectClass, ObjectScene, Position, Position3, SemanticBindingState, ZoneConstraint,
};
use std::{fs, path::PathBuf};

fn scene() -> ObjectScene {
    ObjectScene {
        sample_rate: 48_000,
        duration_samples: 4,
        objects: vec![openjoc_scene::MetadataObject {
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
fn best_effort_export_is_deterministic_and_structurally_valid() {
    let first = build_export(&scene(), AdmPolicy::BestEffort).expect("build export");
    let second = build_export(&scene(), AdmPolicy::BestEffort).expect("build export");
    assert_eq!(first.xml, second.xml);
    assert_eq!(
        first.report.generated_signal_identities,
        second.report.generated_signal_identities
    );
    assert_eq!(first.report.dynamic_objects_with_bound_pcm, 0);
    assert_eq!(first.report.reconstructed_signal_count, 1);
    assert_eq!(first.report.bed_direct_speaker_count, 6);
    assert_eq!(first.report.generated_silent_bed_placeholder_count, 5);
    assert!(first.xml.contains("OpenJOC Reconstructed Signal 01"));
    assert!(first.xml.contains("<speakerLabel>RC_LFE</speakerLabel>"));
    assert!(
        first
            .xml
            .contains("xmlns=\"urn:ebu:metadata-schema:ebuCore_2016\"")
    );
    assert!(first.xml.contains("audioProgrammeID=\"APR_1001\""));
    assert!(
        first
            .xml
            .contains("audioBlockFormatID=\"AB_00031007_00000001\"")
    );
    assert!(
        first
            .xml
            .contains("<audioTrackFormatIDRef>AT_00031007_01</audioTrackFormatIDRef>")
    );
    assert!(!first.xml.contains("trackIndex="));

    let path = temp_path("best-effort");
    write_adm_bwf(&path, &scene(), AdmPolicy::BestEffort).expect("write ADM BWF");
    let summary = validate_adm_bwf(&path).expect("validate ADM BWF");
    assert_eq!(summary.container, "RIFF");
    assert_eq!(
        summary.chunks,
        ["JUNK", "fmt ", "data", "axml", "chna", "dbmd"]
    );
    assert_eq!(summary.channels, 7);
    assert_eq!(summary.chna_tracks, 7);
    assert_eq!(summary.data_bytes, 84);
    fs::remove_file(path).expect("remove test artifact");
}

#[test]
fn strict_export_rejects_unresolved_binding() {
    assert!(matches!(
        build_export(&scene(), AdmPolicy::Strict),
        Err(AdmError::StrictUnresolvedBinding)
    ));
}

fn admitted_scene() -> ObjectScene {
    let duration_samples = 4_usize;
    let mut objects = vec![openjoc_scene::MetadataObject {
        object_id: 0,
        class: ObjectClass::Lfe,
    }];
    objects.extend((1..=15).map(|object_id| openjoc_scene::MetadataObject {
        object_id,
        class: ObjectClass::Dynamic,
    }));
    let mut metadata_timeline = Vec::new();
    for object_id in 1..=15 {
        for (start_sample, x) in [(0, 0.0), (2, 1.0)] {
            metadata_timeline.push(MetadataUpdate {
                object_id,
                start_sample,
                ramp_duration: 0,
                active: true,
                position: Position::Room(Position3 { x, y: 0.25, z: 0.0 }),
                size: Extent3 {
                    width: 0.0,
                    depth: 0.0,
                    height: 0.0,
                },
                priority: 0.0,
                gain_db: None,
                channel_lock: false,
                zones: [ZoneConstraint::Include; 6],
                divergence: 0.0,
                trim_disabled: false,
            });
        }
    }
    ObjectScene {
        sample_rate: 48_000,
        duration_samples: 4,
        objects,
        metadata_timeline,
        trim_timeline: Vec::new(),
        reconstruction_basis: Some(ReconstructionBasis {
            rows: (0..15).map(|_| vec![0.125; duration_samples]).collect(),
        }),
        base_lfe_pcm: Some(vec![0.0; duration_samples]),
        semantic_binding: SemanticBindingState::ResolvedWithinCarrier,
    }
}

#[test]
fn admitted_binding_exports_dynamic_position_blocks_and_preserves_base_lfe() {
    let first = build_export(&admitted_scene(), AdmPolicy::BestEffort).expect("build bound export");
    let second =
        build_export(&admitted_scene(), AdmPolicy::BestEffort).expect("build bound export");
    assert_eq!(first.xml, second.xml);
    assert_eq!(first.report.dynamic_objects_with_bound_pcm, 15);
    assert_eq!(first.report.decoded_joc_objects_bound, 15);
    assert_eq!(first.report.decoded_joc_objects_unbound, 0);
    assert!(first.report.dynamic_metadata_exported);
    assert_eq!(
        first.report.decoded_joc_binding_profile,
        Some("E_AC_3_JOC_OBSERVED_ORDINARY_PROFILE")
    );
    assert!(!first.report.original_authored_identity_recovered);
    assert!(
        first
            .report
            .generated_signal_identities
            .iter()
            .any(|name| name == "OpenJOC Reconstructed JOC Object 01")
    );
    assert!(
        first
            .xml
            .contains("<position coordinate=\"X\">-1.000000</position>")
    );
    assert!(
        first
            .xml
            .contains("<position coordinate=\"X\">1.000000</position>")
    );
    assert!(
        first
            .xml
            .contains("<position coordinate=\"Y\">0.500000</position>")
    );
    assert!(first.xml.contains("_00000002\" rtime="));
    assert!(
        first
            .xml
            .contains("<jumpPosition interpolationLength=\"0\">1</jumpPosition>")
    );
    assert!(
        first
            .xml
            .contains("<jumpPosition interpolationLength=\"250\">1</jumpPosition>")
    );
    assert!(first.xml.contains("<speakerLabel>RC_LFE</speakerLabel>"));

    let path = temp_path("bound-dynamic");
    write_adm_bwf(&path, &admitted_scene(), AdmPolicy::Strict).expect("write bound ADM BWF");
    let summary = validate_adm_bwf(&path).expect("validate bound ADM BWF");
    assert_eq!(summary.channels, 21);
    fs::remove_file(path).expect("remove test artifact");
}

#[test]
fn scoped_warp3_compat_profile_exports_decoded_object_metadata_under_strict_policy() {
    let mut classes = vec![OamdBindingObjectClass::BaseLfe];
    classes.extend(std::iter::repeat_n(OamdBindingObjectClass::Dynamic, 15));
    let facts = DecodedJocBindingFacts::new(
        BindingCodecProfile::EAc3JocObservedOrdinaryCompatWarp3,
        15,
        15,
        classes,
    );
    let profile = openjoc_scene::admit_decoded_joc_binding(&facts).expect("scoped profile");
    let source = admitted_scene();
    let mut plan = AdmExportPlan::new(
        source.sample_rate,
        source.duration_samples,
        15,
        true,
        15,
        16,
        SemanticBindingState::ResolvedWithinCarrier,
        AdmPolicy::Strict,
    )
    .expect("plan");

    plan.apply_decoded_joc_binding_metadata(&source.metadata_timeline, &profile)
        .expect("raw3-compatible decoded scene is admitted within scope");
}

#[test]
fn unsupported_dynamic_transition_fails_closed_by_policy() {
    let mut scene = admitted_scene();
    scene.metadata_timeline[0].active = false;
    let best_effort = build_export(&scene, AdmPolicy::BestEffort).expect("best effort fallback");
    assert!(!best_effort.report.dynamic_metadata_exported);
    assert_eq!(best_effort.report.decoded_joc_objects_bound, 0);
    assert!(best_effort.report.unsupported_binding_reason.is_some());
    assert!(best_effort.xml.contains("OpenJOC Reconstructed Signal 01"));
    assert!(matches!(
        build_export(&scene, AdmPolicy::Strict),
        Err(AdmError::UnsupportedDynamicMetadata(_))
    ));
}

#[test]
fn streaming_writer_handles_file_backed_multimegabyte_output() {
    let duration = 400_000_u64;
    let plan = AdmExportPlan::new(
        48_000,
        duration,
        2,
        true,
        2,
        3,
        SemanticBindingState::Unresolved,
        AdmPolicy::BestEffort,
    )
    .expect("plan");
    let path = temp_path("file-backed-streaming");
    let file = fs::File::create(&path).expect("create output");
    let mut writer = StreamingAdmWriter::new(file, plan).expect("writer");
    let mut remaining = duration;
    while remaining > 0 {
        let frames = usize::try_from(remaining.min(1536)).expect("bounded frames");
        writer
            .write_pcm(
                &[vec![0.125; frames], vec![-0.125; frames]],
                Some(&vec![0.0; frames]),
            )
            .expect("bounded chunk");
        remaining -= u64::try_from(frames).expect("bounded frames");
    }
    let (file, _, stats) = writer.finish().expect("finish");
    file.sync_all().expect("sync output");
    assert_eq!(stats.max_chunk_frames, 1536);
    assert!(fs::metadata(&path).expect("metadata").len() > 3_600_000);
    let summary = validate_adm_bwf(&path).expect("validate file-backed output");
    assert_eq!(summary.data_bytes, 9_600_000);
    assert_eq!(summary.channels, 8);
    fs::remove_file(path).expect("remove test artifact");
}

fn temp_path(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "openjoc-adm-test-{label}-{}-{}.wav",
        std::process::id(),
        std::thread::current().name().unwrap_or("thread")
    ))
}
