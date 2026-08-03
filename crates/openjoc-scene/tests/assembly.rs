use openjoc_oamd::{
    ContentDescription, Distance, ExtendedObjectElement, Extent3 as OamdExtent3, Gain, GlobalTrim,
    MetadataBlockTiming, MetadataTiming, OamdContentPrefix, OamdElement, OamdElementMetadata,
    OamdPayload, ObjectBasicInfo, ObjectClass as OamdObjectClass, ObjectElement, ObjectRenderInfo,
    ObjectUpdate, Position3 as OamdPosition3, PositionCoding, ReferenceScreen,
    StandardPositionBits, TrimElement, WarpMode, ZoneConstraint as OamdZoneConstraint,
};
use openjoc_scene::{ObjectClass, Position, Position3, SceneBuilder, ZoneConstraint};

fn payload() -> OamdPayload {
    let bits = StandardPositionBits { x: 31, y: 31, z: 0 };
    OamdPayload {
        prefix: OamdContentPrefix {
            syntax_version: 0,
            object_count: 1,
            content: ContentDescription::DynamicOnly { lfe_present: false },
            alternate_object_data_present: false,
            element_count: 1,
            consumed_bits: 0,
        },
        object_classes: vec![OamdObjectClass::Dynamic],
        elements: vec![OamdElementMetadata {
            id: 1,
            alternate_data_id: None,
            discard_unknown: false,
            element: OamdElement::Objects(ObjectElement {
                timing: MetadataTiming {
                    sample_offset: 0,
                    blocks: vec![MetadataBlockTiming {
                        start_sample: 1,
                        ramp_duration: 2,
                    }],
                },
                objects: vec![vec![ObjectUpdate {
                    active: true,
                    basic: ObjectBasicInfo {
                        gain: Gain::Decibels(-6),
                        priority: 0.5,
                    },
                    render: ObjectRenderInfo {
                        position: OamdPosition3 {
                            x: 0.5,
                            y: 0.5,
                            z: 0.0,
                        },
                        standard_position: bits,
                        position_coding: PositionCoding::Absolute(bits),
                        distance: Distance::InsideRoom,
                        zones: [OamdZoneConstraint::Include; 6],
                        size: OamdExtent3 {
                            width: 0.1,
                            depth: 0.2,
                            height: 0.3,
                        },
                        screen_anchor: false,
                        screen_factor: 0.0,
                        depth_factor: 1.0,
                        channel_lock: true,
                    },
                    additional_table_data: None,
                }]],
                consumed_bits: 0,
            }),
        }],
        consumed_bits: 0,
    }
}

#[test]
fn assembles_reconstructed_pcm_and_timed_oamd_into_scene() {
    let oamd = payload();
    let mut builder = SceneBuilder::new(48_000, &oamd.prefix).expect("valid content");
    builder
        .append_frame(
            &[vec![0.25, -0.5, 1.0]],
            &oamd,
            Some(ReferenceScreen {
                bottom_left: OamdPosition3 {
                    x: 0.1,
                    y: 0.0,
                    z: -0.5,
                },
                width: 0.8,
                height: 1.0,
            }),
        )
        .expect("aligned frame");
    let scene = builder.finish().expect("valid scene");

    assert_eq!(scene.duration_samples, 3);
    assert_eq!(scene.objects[0].class, ObjectClass::Dynamic);
    assert_eq!(scene.objects[0].pcm, vec![0.25, -0.5, 1.0]);
    let update = &scene.metadata_timeline[0];
    assert_eq!(update.start_sample, 1);
    assert_eq!(update.ramp_duration, 2);
    assert_eq!(
        update.position,
        Position::Room(Position3 {
            x: 0.5,
            y: 0.5,
            z: 0.0
        })
    );
    assert_eq!(update.gain_db, Some(-6.0));
    assert_eq!(update.zones, [ZoneConstraint::Include; 6]);
    assert!(update.channel_lock);
}

#[test]
fn applies_extended_precision_before_scene_position_conversion() {
    let mut oamd = payload();
    if let OamdElement::Objects(objects) = &mut oamd.elements[0].element {
        objects.timing.blocks[0].start_sample = 0;
    }
    oamd.elements.push(OamdElementMetadata {
        id: 5,
        alternate_data_id: None,
        discard_unknown: false,
        element: OamdElement::Extended(ExtendedObjectElement {
            divergence: None,
            extended_precision: Some(vec![vec![[Some(1), None, None]]]),
            consumed_bits: 0,
        }),
    });
    let mut builder = SceneBuilder::new(48_000, &oamd.prefix).expect("valid content");
    builder
        .append_frame(&[vec![0.0]], &oamd, None)
        .expect("aligned frame");
    let scene = builder.finish().expect("valid scene");

    assert_eq!(
        scene.metadata_timeline[0].position,
        Position::Room(Position3 {
            x: 31.0 / 62.0 + 2.0 / 310.0,
            y: 0.5,
            z: 0.0,
        })
    );
}

#[test]
fn retains_complete_trim_state_in_the_renderer_independent_scene() {
    let mut oamd = payload();
    if let OamdElement::Objects(objects) = &mut oamd.elements[0].element {
        objects.timing.blocks[0].start_sample = 0;
    }
    oamd.elements.push(OamdElementMetadata {
        id: 2,
        alternate_data_id: None,
        discard_unknown: false,
        element: OamdElement::Trim(TrimElement {
            warp_mode: WarpMode::DoubleY,
            global_trim: GlobalTrim::Disabled,
            disable_trim_per_object: vec![true],
            consumed_bits: 9,
        }),
    });
    let mut builder = SceneBuilder::new(48_000, &oamd.prefix).expect("valid content");
    builder
        .append_frame(&[vec![0.0]], &oamd, None)
        .expect("aligned frame");
    let scene = builder.finish().expect("valid scene");

    assert_eq!(scene.trim_timeline.len(), 1);
    assert_eq!(scene.trim_timeline[0].start_sample, 0);
    assert_eq!(scene.trim_timeline[0].trim.warp_mode, WarpMode::DoubleY);
    assert_eq!(
        scene.trim_timeline[0].trim.global_trim,
        GlobalTrim::Disabled
    );
    assert_eq!(
        scene.trim_timeline[0].trim.disable_trim_per_object,
        vec![true]
    );
}
