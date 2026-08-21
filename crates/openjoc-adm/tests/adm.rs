use openjoc_adm::{
    AdmError, AdmExportPlan, AdmPolicy, StreamingAdmWriter, build_export, validate_bw64, write_bw64,
};
use openjoc_joc::ReconstructionBasis;
use openjoc_scene::{ObjectClass, ObjectScene, SemanticBindingState};
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
    assert!(first.xml.contains("OpenJOC Reconstructed Signal 01"));
    assert!(first.xml.contains("<speakerLabel>LFE1</speakerLabel>"));

    let path = temp_path("best-effort");
    write_bw64(&path, &scene(), AdmPolicy::BestEffort).expect("write BW64");
    let summary = validate_bw64(&path).expect("validate BW64");
    assert_eq!(summary.container, "BW64");
    assert_eq!(summary.channels, 2);
    assert_eq!(summary.chna_tracks, 2);
    assert_eq!(summary.data_bytes, 24);
    fs::remove_file(path).expect("remove test artifact");
}

#[test]
fn strict_export_rejects_unresolved_binding() {
    assert!(matches!(
        build_export(&scene(), AdmPolicy::Strict),
        Err(AdmError::StrictUnresolvedBinding)
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
    let summary = validate_bw64(&path).expect("validate file-backed output");
    assert_eq!(summary.data_bytes, 3_600_000);
    assert_eq!(summary.channels, 3);
    fs::remove_file(path).expect("remove test artifact");
}

fn temp_path(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "openjoc-adm-test-{label}-{}-{}.bw64",
        std::process::id(),
        std::thread::current().name().unwrap_or("thread")
    ))
}
