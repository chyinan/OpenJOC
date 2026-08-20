use openjoc_adm::{AdmError, AdmPolicy, build_export, validate_bw64, write_bw64};
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

fn temp_path(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "openjoc-adm-test-{label}-{}-{}.bw64",
        std::process::id(),
        std::thread::current().name().unwrap_or("thread")
    ))
}
