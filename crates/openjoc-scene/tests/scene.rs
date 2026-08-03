use openjoc_oamd::{GlobalTrim, TrimElement, WarpMode};
use openjoc_scene::{
    Extent3, IsfLabel, IsfRing, MetadataUpdate, ObjectClass, ObjectScene, ObjectTrack, Position,
    Position3, SceneError, SpeakerLabel, TrimUpdate, ZoneConstraint,
};

fn scene() -> ObjectScene {
    ObjectScene {
        sample_rate: 48_000,
        duration_samples: 3,
        objects: vec![ObjectTrack {
            object_id: 0,
            class: ObjectClass::Dynamic,
            pcm: vec![0.25, -0.5, 1.0],
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
    }
}

#[test]
fn scene_json_roundtrips_without_losing_timeline_or_pcm() {
    let expected = scene();
    expected.validate().expect("valid scene");

    let json = expected.to_json_pretty().expect("finite scene JSON");
    let decoded = ObjectScene::from_json(&json).expect("scene JSON");

    assert_eq!(decoded, expected);
}

#[test]
fn artifact_json_references_wav_stems_and_separates_timeline() {
    let scene = scene();
    let manifest = scene.to_manifest_json_pretty().expect("scene manifest");
    let timeline = scene.to_timeline_json_pretty().expect("metadata timeline");

    assert!(manifest.contains("objects/object_000.wav"));
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
    invalid_duration.objects[0].pcm.pop();
    assert_eq!(
        invalid_duration.validate(),
        Err(SceneError::TrackDurationMismatch {
            object_id: 0,
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
