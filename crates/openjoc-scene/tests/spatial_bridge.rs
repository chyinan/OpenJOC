use openjoc_scene::{
    GainScheduler, JocSpatialBridge, SemanticBindingState, SpatialBindingRecord,
    SpatialBindingState, SpatialBridgeError, SpatialCoordinateUpdate, SpatialDescriptor,
    SpatialDescriptorPatch, SpatialExplicitGroup, SpatialExplicitMember, SpatialLayout,
    SpatialLayoutChannel, SpatialLayoutNode, SpatialPairedGeometry, SpatialRouteVector,
    SpatialSourceClass, SpatialSpreadProfile, SpatialSpreadSample, SpatialTopologySnapshot,
};

fn descriptor(
    class: SpatialSourceClass,
    identity: &str,
    coordinates: Vec<f64>,
) -> SpatialDescriptor {
    SpatialDescriptor {
        source_class: class,
        identity: identity.to_owned(),
        coordinates,
        spread: None,
        paired: None,
        raw3: Some(vec![3, 7]),
        extent: None,
        zones: None,
    }
}

#[test]
fn spatial_bridge_schema_uses_the_stable_function_name() {
    assert_eq!(
        openjoc_scene::JOC_SPATIAL_BRIDGE_SCHEMA,
        "openjoc.joc-spatial-bridge.v1"
    );
}

fn record(class: SpatialSourceClass, identity: &str, scalar: f64) -> SpatialBindingRecord {
    SpatialBindingRecord {
        descriptor: descriptor(class, identity, vec![0.0]),
        scalar,
        active: true,
    }
}

fn topology() -> SpatialTopologySnapshot {
    SpatialTopologySnapshot {
        explicit_groups: vec![
            SpatialExplicitGroup {
                group_order: 1,
                members: vec![
                    SpatialExplicitMember {
                        canonical_label: "b".to_owned(),
                        record: record(SpatialSourceClass::DynamicPoint, "b", 2.0),
                    },
                    SpatialExplicitMember {
                        canonical_label: "a".to_owned(),
                        record: record(SpatialSourceClass::DynamicPoint, "a", 1.0),
                    },
                ],
            },
            SpatialExplicitGroup {
                group_order: 0,
                members: vec![SpatialExplicitMember {
                    canonical_label: "z".to_owned(),
                    record: record(SpatialSourceClass::ExplicitChannel, "left", 3.0),
                }],
            },
        ],
        fixed_layout: vec![record(SpatialSourceClass::FixedLayout, "fixed", 4.0)],
        dynamic_records: vec![record(SpatialSourceClass::DynamicPoint, "dynamic", 5.0)],
    }
}

fn layout() -> SpatialLayout {
    SpatialLayout::new(
        vec![
            SpatialLayoutChannel {
                identity: "left".to_owned(),
                enabled: true,
                lfe: false,
            },
            SpatialLayoutChannel {
                identity: "right".to_owned(),
                enabled: true,
                lfe: false,
            },
            SpatialLayoutChannel {
                identity: "disabled".to_owned(),
                enabled: false,
                lfe: false,
            },
            SpatialLayoutChannel {
                identity: "lfe".to_owned(),
                enabled: true,
                lfe: true,
            },
        ],
        vec![vec![0.0, 1.0]],
        vec![
            SpatialLayoutNode {
                knot_indices: vec![0],
                vector: vec![1.0, 0.0],
            },
            SpatialLayoutNode {
                knot_indices: vec![1],
                vector: vec![0.0, 1.0],
            },
        ],
        vec![SpatialRouteVector {
            identity: "fixed".to_owned(),
            vector: vec![0.25, 0.75],
        }],
    )
    .expect("valid spatial layout")
}

fn irregular_public_order_layout() -> SpatialLayout {
    SpatialLayout::new(
        vec![
            SpatialLayoutChannel {
                identity: "RIGHT".to_owned(),
                enabled: true,
                lfe: false,
            },
            SpatialLayoutChannel {
                identity: "LEFT".to_owned(),
                enabled: true,
                lfe: false,
            },
            SpatialLayoutChannel {
                identity: "LFE".to_owned(),
                enabled: true,
                lfe: true,
            },
        ],
        vec![vec![0.0, 1.0]],
        vec![
            SpatialLayoutNode {
                knot_indices: vec![0],
                vector: vec![0.0, 1.0],
            },
            SpatialLayoutNode {
                knot_indices: vec![1],
                vector: vec![1.0, 0.0],
            },
        ],
        Vec::new(),
    )
    .expect("valid irregular public-order layout")
}

fn high_channel_layout(channel_count: usize) -> SpatialLayout {
    let channels = (0..channel_count)
        .map(|index| SpatialLayoutChannel {
            identity: format!("C{index}"),
            enabled: true,
            lfe: false,
        })
        .collect::<Vec<_>>();
    let knots = (0..channel_count)
        .map(|index| index as f64)
        .collect::<Vec<_>>();
    let nodes = (0..channel_count)
        .map(|index| {
            let mut vector = vec![0.0; channel_count];
            vector[index] = 1.0;
            SpatialLayoutNode {
                knot_indices: vec![index],
                vector,
            }
        })
        .collect::<Vec<_>>();
    SpatialLayout::new(channels, vec![knots], nodes, Vec::new())
        .expect("valid high-channel-count layout")
}

#[test]
fn binding_flattens_reuses_inherits_overrides_rebuilds_and_resets() {
    let mut state = SpatialBindingState::new();
    let first = state
        .apply(Some(&topology()), None, 3)
        .expect("initial topology");
    assert_eq!(
        first.transition,
        openjoc_scene::SpatialBindingTransition::Init
    );
    assert_eq!(state.snapshot().unwrap().topology_epoch, 1);
    assert_eq!(
        state
            .snapshot()
            .unwrap()
            .records
            .iter()
            .map(|record| record.descriptor.identity.as_str())
            .collect::<Vec<_>>(),
        ["left", "a", "b", "fixed", "dynamic"]
    );
    assert_eq!(state.snapshot().unwrap().active_count, 3);

    let reuse = state.apply(None, None, 99).expect("no-new-payload reuse");
    assert_eq!(
        reuse.transition,
        openjoc_scene::SpatialBindingTransition::Reuse
    );
    assert_eq!(state.snapshot().unwrap().active_count, 5);

    let update = SpatialCoordinateUpdate {
        ordinal: 1,
        descriptor: Some(SpatialDescriptorPatch {
            coordinates: Some(vec![1.0]),
            ..SpatialDescriptorPatch::default()
        }),
        scalar: Some(7.0),
        active: None,
    };
    state
        .apply(None, Some(std::slice::from_ref(&update)), 99)
        .expect("selective update");
    assert_eq!(state.snapshot().unwrap().records[1].scalar, 7.0);
    assert_eq!(
        state.snapshot().unwrap().records[1].descriptor.raw3,
        Some(vec![3, 7])
    );

    let inherited = SpatialCoordinateUpdate {
        ordinal: 1,
        descriptor: None,
        scalar: None,
        active: None,
    };
    state
        .apply(None, Some(std::slice::from_ref(&inherited)), 99)
        .expect("same-coordinate inheritance");
    assert_eq!(state.snapshot().unwrap().records[1].scalar, 7.0);

    let rebuild = SpatialCoordinateUpdate {
        ordinal: 1,
        descriptor: Some(SpatialDescriptorPatch {
            source_class: Some(SpatialSourceClass::NamedLayout),
            identity: Some("new".to_owned()),
            ..SpatialDescriptorPatch::default()
        }),
        scalar: None,
        active: None,
    };
    let transition = state
        .apply(None, Some(std::slice::from_ref(&rebuild)), 99)
        .expect("topology signature rebuild");
    assert_eq!(
        transition.transition,
        openjoc_scene::SpatialBindingTransition::Rebuild
    );
    assert_eq!(state.snapshot().unwrap().topology_epoch, 2);

    state.reset();
    assert!(state.snapshot().is_none());
}

#[test]
fn projection_covers_endpoints_midpoint_tensor_clamp_exclusion_spread_and_pair() {
    let layout = layout();
    let left = descriptor(SpatialSourceClass::ExplicitChannel, "left", vec![0.75]);
    let projected = layout.project(&left).expect("active channel unit vector");
    assert_eq!(projected, vec![1.0, 0.0]);

    let midpoint = descriptor(SpatialSourceClass::DynamicPoint, "p", vec![0.5]);
    let projected = layout.project(&midpoint).expect("midpoint interpolation");
    let root = 0.5_f64.sqrt();
    assert!((projected[0] - root).abs() < 1e-12);
    assert!((projected[1] - root).abs() < 1e-12);
    assert!((projected[0].mul_add(projected[0], projected[1] * projected[1]) - 1.0).abs() < 1e-12);

    let clamped = descriptor(SpatialSourceClass::DynamicPoint, "p", vec![2.0]);
    assert_eq!(layout.project(&clamped).unwrap(), vec![0.0, 1.0]);

    let mut spread = descriptor(SpatialSourceClass::DynamicRegion, "region", vec![0.5]);
    spread.spread = Some(SpatialSpreadProfile {
        samples: vec![
            SpatialSpreadSample {
                position: vec![0.0],
                weight: 0.5,
            },
            SpatialSpreadSample {
                position: vec![1.0],
                weight: 0.5,
            },
        ],
    });
    let spread_vector = layout.project(&spread).expect("spread composition");
    assert!((spread_vector[0] - root).abs() < 1e-12);
    assert!((spread_vector[1] - root).abs() < 1e-12);

    let mut paired = descriptor(SpatialSourceClass::DynamicPoint, "pair", vec![0.0]);
    paired.paired = Some(SpatialPairedGeometry {
        first: vec![1.0, 0.0],
        second: vec![0.0, 1.0],
        blend: 0.5,
    });
    let paired_vector = layout.project(&paired).expect("paired geometry");
    assert!((paired_vector[0] - root).abs() < 1e-12);
    assert!((paired_vector[1] - root).abs() < 1e-12);

    let inactive = descriptor(SpatialSourceClass::Inactive, "inactive", vec![0.5]);
    assert_eq!(layout.project(&inactive).unwrap(), vec![0.0, 0.0]);
}

#[test]
fn scheduler_has_q32_boundaries_restart_reset_and_partition_invariance() {
    let mut whole = GainScheduler::new();
    whole.set_target(1.0, true, 64, 48_000).expect("Q32 target");
    let mut expected = vec![0.0; 96];
    whole.process(&mut expected);
    assert_eq!(expected[0], 0.0);
    assert!((expected[31] - 31.0 / 64.0).abs() < 1e-12);
    assert!((expected[63] - 63.0 / 64.0).abs() < 1e-12);
    assert_eq!(expected[64], 1.0);

    let mut split = GainScheduler::new();
    split.set_target(1.0, true, 64, 48_000).expect("Q32 target");
    let mut actual = Vec::new();
    for size in [1, 7, 32, 5, 51] {
        let mut block = vec![0.0; size];
        split.process(&mut block);
        actual.extend(block);
    }
    assert_eq!(actual, expected);

    split
        .set_target(0.0, true, 0, 48_000)
        .expect("immediate completion");
    assert_eq!(split.next_sample(), 0.0);
    split.reset();
    assert_eq!(split.next_sample(), 0.0);
}

#[test]
fn accumulation_is_linear_multi_coordinate_and_keeps_semantic_state_unresolved() {
    let topology = SpatialTopologySnapshot {
        explicit_groups: Vec::new(),
        fixed_layout: Vec::new(),
        dynamic_records: vec![
            SpatialBindingRecord {
                descriptor: descriptor(SpatialSourceClass::ExplicitChannel, "left", vec![0.0]),
                scalar: 0.5,
                active: true,
            },
            SpatialBindingRecord {
                descriptor: descriptor(SpatialSourceClass::ExplicitChannel, "right", vec![0.0]),
                scalar: 1.0,
                active: true,
            },
        ],
    };
    let input = [vec![2.0, 3.0, 4.0], vec![5.0, 6.0, 7.0]];
    let refs: Vec<&[f64]> = input.iter().map(Vec::as_slice).collect();
    let mut left = vec![0.0; 3];
    let mut right = vec![0.0; 3];
    let mut outputs: Vec<&mut [f64]> = vec![&mut left, &mut right];
    let mut bridge = JocSpatialBridge::new();
    bridge
        .render_coordinates(
            &refs,
            Some(&topology),
            None,
            &layout(),
            0,
            48_000,
            &mut outputs,
        )
        .expect("spatial render");
    assert_eq!(left, vec![1.0, 1.5, 2.0]);
    assert_eq!(right, vec![5.0, 6.0, 7.0]);
    assert_eq!(bridge.semantic_binding(), SemanticBindingState::Unresolved);
    assert!(!bridge.is_production_resolved());
}

#[test]
fn arbitrary_layout_preserves_public_order_independently_of_geometry_order() {
    let layout = irregular_public_order_layout();
    assert_eq!(
        layout
            .channels()
            .iter()
            .map(|channel| channel.identity.as_str())
            .collect::<Vec<_>>(),
        ["RIGHT", "LEFT", "LFE"]
    );
    let topology = SpatialTopologySnapshot {
        dynamic_records: vec![SpatialBindingRecord {
            descriptor: SpatialDescriptor::new(
                SpatialSourceClass::DynamicPoint,
                "irregular",
                vec![0.0],
            ),
            scalar: 1.0,
            active: true,
        }],
        ..SpatialTopologySnapshot::default()
    };
    let input = [vec![2.0, 3.0]];
    let coordinates = input.iter().map(Vec::as_slice).collect::<Vec<_>>();
    let mut outputs = vec![vec![0.0; 2]; layout.active_channel_count()];
    let mut output_refs = outputs
        .iter_mut()
        .map(Vec::as_mut_slice)
        .collect::<Vec<_>>();
    JocSpatialBridge::new()
        .render_coordinates(
            &coordinates,
            Some(&topology),
            None,
            &layout,
            0,
            48_000,
            &mut output_refs,
        )
        .expect("generic irregular layout render");
    assert_eq!(outputs, [vec![0.0, 0.0], vec![2.0, 3.0]]);
}

#[test]
fn generic_spatial_layout_supports_twenty_four_output_channels() {
    let layout = high_channel_layout(24);
    let topology = SpatialTopologySnapshot {
        dynamic_records: vec![SpatialBindingRecord {
            descriptor: SpatialDescriptor::new(
                SpatialSourceClass::DynamicPoint,
                "high-channel",
                vec![12.0],
            ),
            scalar: 1.0,
            active: true,
        }],
        ..SpatialTopologySnapshot::default()
    };
    let input = [vec![0.75]];
    let coordinates = input.iter().map(Vec::as_slice).collect::<Vec<_>>();
    let mut outputs = vec![vec![0.0; 1]; layout.active_channel_count()];
    let mut output_refs = outputs
        .iter_mut()
        .map(Vec::as_mut_slice)
        .collect::<Vec<_>>();
    JocSpatialBridge::new()
        .render_coordinates(
            &coordinates,
            Some(&topology),
            None,
            &layout,
            0,
            48_000,
            &mut output_refs,
        )
        .expect("generic 24-channel layout render");
    assert_eq!(outputs.len(), 24);
    assert_eq!(outputs[12], vec![0.75]);
    assert!(
        outputs
            .iter()
            .enumerate()
            .all(|(index, output)| index == 12 || output[0].abs() < 1e-12)
    );
}

#[test]
fn invalid_spatial_inputs_are_rejected_without_profile_reinterpretation() {
    let bad = SpatialLayout::new(
        vec![SpatialLayoutChannel {
            identity: "left".to_owned(),
            enabled: false,
            lfe: false,
        }],
        vec![vec![0.0, 1.0]],
        vec![],
        vec![],
    );
    assert!(bad.is_err());

    let mut bridge = JocSpatialBridge::new();
    let input = [vec![1.0, 1.0]];
    let refs: Vec<&[f64]> = input.iter().map(Vec::as_slice).collect();
    let mut left = vec![0.0; 2];
    let mut right = vec![0.0; 2];
    let mut outputs: Vec<&mut [f64]> = vec![&mut left, &mut right];
    let error = bridge
        .render_coordinates(&refs, None, None, &layout(), 0, 48_000, &mut outputs)
        .expect_err("missing topology must not guess a semantic state");
    assert!(matches!(error, SpatialBridgeError::Binding(_)));
}
