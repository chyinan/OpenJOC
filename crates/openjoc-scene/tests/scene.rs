use openjoc_joc::ReconstructionBasis;
use openjoc_oamd::{GlobalTrim, TrimElement, WarpMode};
use openjoc_scene::{
    BindingAdmissionRequirements, BindingEvidenceClass, BindingEvidenceDimensions,
    BindingProvenance, BindingRelationKind, Extent3, IsfLabel, IsfRing, MetadataObject,
    MetadataUpdate, ObjectClass, ObjectScene, Position, Position3, SceneError,
    SemanticBindingEvidence, SemanticBindingState, SpeakerLabel, TrimUpdate, ZoneConstraint,
};

fn scene() -> ObjectScene {
    ObjectScene {
        sample_rate: 48_000,
        duration_samples: 3,
        objects: vec![MetadataObject {
            object_id: 0,
            class: ObjectClass::Dynamic,
        }],
        metadata_timeline: vec![MetadataUpdate {
            object_id: 0,
            start_sample: 1,
            ramp_duration: 2,
            active: true,
            position: Position::Room(Position3 {
                x: 0.5,
                y: 0.75,
                z: -0.25,
            }),
            size: Extent3 {
                width: 0.1,
                depth: 0.2,
                height: 0.3,
            },
            priority: 0.5,
            gain_db: Some(-6.0),
            channel_lock: false,
            zones: [ZoneConstraint::Include; 6],
            divergence: 0.0,
            trim_disabled: false,
        }],
        trim_timeline: vec![TrimUpdate {
            start_sample: 0,
            trim: TrimElement {
                warp_mode: WarpMode::DoubleY,
                global_trim: GlobalTrim::Disabled,
                disable_trim_per_object: vec![true],
                consumed_bits: 9,
            },
        }],
        reconstruction_basis: Some(ReconstructionBasis {
            rows: vec![vec![0.25, -0.5, 1.0]],
        }),
        base_lfe_pcm: None,
        semantic_binding: SemanticBindingState::Unresolved,
    }
}

#[test]
fn scene_json_roundtrips_without_losing_timeline_or_basis() {
    let expected = scene();
    expected.validate().expect("valid scene");

    let json = expected.to_json_pretty().expect("finite scene JSON");
    let decoded = ObjectScene::from_json(&json).expect("scene JSON");

    assert_eq!(decoded, expected);
}

#[test]
fn metadata_scene_is_admissible_without_audio_basis() {
    let mut metadata_only = scene();
    metadata_only.reconstruction_basis = None;
    metadata_only
        .validate()
        .expect("metadata-only scene is valid");
    assert_eq!(
        metadata_only.semantic_binding,
        SemanticBindingState::Unresolved
    );
    let json = metadata_only.to_json_pretty().expect("metadata JSON");
    assert!(!json.contains("\"pcm\""));
    assert_eq!(
        ObjectScene::from_json(&json).unwrap().reconstruction_basis,
        None
    );
}

#[test]
fn reconstruction_basis_has_rows_without_authored_object_identity() {
    let basis = ReconstructionBasis {
        rows: vec![vec![1.0, 2.0], vec![3.0, 4.0]],
    };
    assert_eq!(basis.rows.len(), 2);
    let json = serde_json::to_string(&basis).unwrap();
    assert!(!json.contains("object_id"));
}

#[test]
fn structural_and_empirical_evidence_cannot_admit_binding() {
    for evidence_class in [
        BindingEvidenceClass::Structural,
        BindingEvidenceClass::Empirical,
    ] {
        let evidence = SemanticBindingEvidence::new(
            BindingRelationKind::OamdSlotToRow,
            "J1R12 controlled corpus",
            evidence_class,
            BindingProvenance::ControlledCleanroomEmpirical,
        );
        let error = evidence
            .try_admit(&BindingAdmissionRequirements::default())
            .expect_err("non-verified evidence must not admit semantic binding");
        assert!(matches!(
            error,
            openjoc_scene::BindingAdmissionError::EvidenceClassNotVerified { .. }
        ));
    }
}

#[test]
fn synthetic_admission_contract_is_explicit_but_does_not_change_scene_state() {
    let mut evidence = SemanticBindingEvidence::new(
        BindingRelationKind::AuthoredObjectToRow,
        "synthetic contract test only",
        BindingEvidenceClass::Verified,
        BindingProvenance::ControlledCleanroomEmpirical,
    );
    evidence
        .supporting_observations
        .push("synthetic observation".into());
    evidence
        .negative_controls
        .push("synthetic negative control".into());
    evidence
        .producer_constraints
        .push("synthetic deterministic carrier".into());
    evidence.falsifier = "a counterexample invalidates the relation".into();
    evidence.dimensions = BindingEvidenceDimensions {
        who: true,
        where_: true,
        slot: true,
        row_or_basis: true,
        audio_identity: true,
        context: true,
        time: true,
        repeatability: true,
        negative_control: true,
        cross_state: true,
    };
    let admission = evidence
        .try_admit(&BindingAdmissionRequirements::default())
        .expect("complete synthetic contract should mint a capability token");
    assert_eq!(
        admission.relation(),
        BindingRelationKind::AuthoredObjectToRow
    );
    assert_eq!(admission.scope(), "synthetic contract test only");
    assert_eq!(
        evidence.admission_status(),
        openjoc_scene::BindingAdmissionStatus::NotAdmitted
    );
    assert_eq!(
        scene().semantic_binding,
        SemanticBindingState::Unresolved,
        "the contract token cannot silently upgrade production scenes"
    );
}

#[test]
fn incomplete_verified_evidence_reports_missing_dimensions_and_controls() {
    let evidence = SemanticBindingEvidence::new(
        BindingRelationKind::OamdSlotToRow,
        "incomplete synthetic record",
        BindingEvidenceClass::Verified,
        BindingProvenance::PublicReference,
    );
    let error = evidence
        .try_admit(&BindingAdmissionRequirements::default())
        .expect_err("verified label alone is not enough");
    assert!(matches!(
        error,
        openjoc_scene::BindingAdmissionError::MissingDimensions { .. }
    ));
}

#[test]
fn artifact_json_separates_metadata_from_reconstruction_rows() {
    let scene = scene();
    let manifest = scene.to_manifest_json_pretty().expect("scene manifest");
    let timeline = scene.to_timeline_json_pretty().expect("metadata timeline");

    assert!(manifest.contains("diagnostics/reconstruction_basis.json"));
    assert!(!manifest.contains("\"pcm\""));
    assert!(manifest.contains("metadata/timeline.json"));
    assert!(timeline.contains("\"start_sample\": 1"));
}

#[test]
fn artifact_json_retains_decoded_trim_state() {
    let scene = scene();
    let json = scene.to_trim_timeline_json_pretty().expect("trim timeline");
    assert!(json.contains("double_y"));
    assert!(json.contains("disable_trim_per_object"));
    assert_eq!(
        ObjectScene::from_json(&scene.to_json_pretty().unwrap()),
        Ok(scene)
    );
}

#[test]
fn validation_rejects_inconsistent_scene_boundaries() {
    let mut invalid_rate = scene();
    invalid_rate.sample_rate = 0;
    assert_eq!(invalid_rate.validate(), Err(SceneError::InvalidSampleRate));

    let mut invalid_duration = scene();
    invalid_duration.reconstruction_basis.as_mut().unwrap().rows[0].pop();
    assert_eq!(
        invalid_duration.validate(),
        Err(SceneError::ReconstructionRowDurationMismatch {
            row_index: 0,
            expected: 3,
            actual: 2,
        })
    );

    let mut unknown_object = scene();
    unknown_object.metadata_timeline[0].object_id = 7;
    assert_eq!(
        unknown_object.validate(),
        Err(SceneError::UnknownMetadataObject { object_id: 7 })
    );
}

#[test]
fn scene_json_roundtrips_every_normative_position_anchor() {
    let positions = [
        Position::RoomAtInfinity {
            boundary_intersection: Position3 {
                x: 1.0,
                y: 0.5,
                z: 0.0,
            },
        },
        Position::Screen {
            coded: Position3 {
                x: 0.25,
                y: 0.5,
                z: 0.75,
            },
            interpolated_room: Position3 {
                x: 0.3,
                y: 0.5,
                z: 0.6,
            },
        },
        Position::Speaker(SpeakerLabel::RcTfl),
        Position::IntermediateSpatial(IsfLabel {
            ring: IsfRing::Upper,
            index: 2,
        }),
    ];
    let mut expected = scene();
    expected.metadata_timeline = positions
        .into_iter()
        .map(|position| MetadataUpdate {
            position,
            ..expected.metadata_timeline[0].clone()
        })
        .collect();

    let json = expected.to_json_pretty().expect("all anchors serialize");
    assert_eq!(ObjectScene::from_json(&json), Ok(expected));
}
