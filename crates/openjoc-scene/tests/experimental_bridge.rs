use openjoc_scene::{
    CleanBindingRecord, CleanBindingState, CleanCoordinateUpdate, CleanDescriptorPatch,
    CleanExplicitGroup, CleanExplicitMember, CleanLayoutChannel, CleanLayoutNode,
    CleanPairedGeometry, CleanRouteScheduler, CleanRouteVector, CleanSourceClass,
    CleanSpatialBridgeError, CleanSpatialDescriptor, CleanSpatialLayout, CleanSpreadProfile,
    CleanSpreadSample, CleanTopologySnapshot, ExperimentalCleanSpatialBridge, SemanticBindingState,
};

fn descriptor(
    class: CleanSourceClass,
    identity: &str,
    coordinates: Vec<f64>,
) -> CleanSpatialDescriptor {
    CleanSpatialDescriptor {
        source_class: class,
        identity: identity.to_owned(),
        coordinates,
        spread: None,
        paired: None,
        raw3: Some(vec![3, 7]),
    }
}

fn record(class: CleanSourceClass, identity: &str, scalar: f64) -> CleanBindingRecord {
    CleanBindingRecord {
        descriptor: descriptor(class, identity, vec![0.0]),
        scalar,
        active: true,
    }
}

fn topology() -> CleanTopologySnapshot {
    CleanTopologySnapshot {
        explicit_groups: vec![
            CleanExplicitGroup {
                group_order: 1,
                members: vec![
                    CleanExplicitMember {
                        canonical_label: "b".to_owned(),
                        record: record(CleanSourceClass::DynamicPoint, "b", 2.0),
                    },
                    CleanExplicitMember {
                        canonical_label: "a".to_owned(),
                        record: record(CleanSourceClass::DynamicPoint, "a", 1.0),
                    },
                ],
            },
            CleanExplicitGroup {
                group_order: 0,
                members: vec![CleanExplicitMember {
                    canonical_label: "z".to_owned(),
                    record: record(CleanSourceClass::ExplicitChannel, "left", 3.0),
                }],
            },
        ],
        fixed_layout: vec![record(CleanSourceClass::FixedLayout, "fixed", 4.0)],
        dynamic_records: vec![record(CleanSourceClass::DynamicPoint, "dynamic", 5.0)],
    }
}

fn layout() -> CleanSpatialLayout {
    CleanSpatialLayout::new(
        vec![
            CleanLayoutChannel {
                identity: "left".to_owned(),
                enabled: true,
                lfe: false,
            },
            CleanLayoutChannel {
                identity: "right".to_owned(),
                enabled: true,
                lfe: false,
            },
            CleanLayoutChannel {
                identity: "disabled".to_owned(),
                enabled: false,
                lfe: false,
            },
            CleanLayoutChannel {
                identity: "lfe".to_owned(),
                enabled: true,
                lfe: true,
            },
        ],
        vec![vec![0.0, 1.0]],
        vec![
            CleanLayoutNode {
                knot_indices: vec![0],
                vector: vec![1.0, 0.0],
            },
            CleanLayoutNode {
                knot_indices: vec![1],
                vector: vec![0.0, 1.0],
            },
        ],
        vec![CleanRouteVector {
            identity: "fixed".to_owned(),
            vector: vec![0.25, 0.75],
        }],
    )
    .expect("valid clean layout")
}

#[test]
fn binding_flattens_reuses_inherits_overrides_rebuilds_and_resets() {
    let mut state = CleanBindingState::new();
    let first = state
        .apply(Some(&topology()), None, 3)
        .expect("initial topology");
    assert_eq!(
        first.transition,
        openjoc_scene::CleanBindingTransition::Init
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
        openjoc_scene::CleanBindingTransition::Reuse
    );
    assert_eq!(state.snapshot().unwrap().active_count, 5);

    let update = CleanCoordinateUpdate {
        ordinal: 1,
        descriptor: Some(CleanDescriptorPatch {
            coordinates: Some(vec![1.0]),
            ..CleanDescriptorPatch::default()
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

    let inherited = CleanCoordinateUpdate {
        ordinal: 1,
        descriptor: None,
        scalar: None,
        active: None,
    };
    state
        .apply(None, Some(std::slice::from_ref(&inherited)), 99)
        .expect("same-coordinate inheritance");
    assert_eq!(state.snapshot().unwrap().records[1].scalar, 7.0);

    let rebuild = CleanCoordinateUpdate {
        ordinal: 1,
        descriptor: Some(CleanDescriptorPatch {
            source_class: Some(CleanSourceClass::NamedLayout),
            identity: Some("new".to_owned()),
            ..CleanDescriptorPatch::default()
        }),
        scalar: None,
        active: None,
    };
    let transition = state
        .apply(None, Some(std::slice::from_ref(&rebuild)), 99)
        .expect("topology signature rebuild");
    assert_eq!(
        transition.transition,
        openjoc_scene::CleanBindingTransition::Rebuild
    );
    assert_eq!(state.snapshot().unwrap().topology_epoch, 2);

    state.reset();
    assert!(state.snapshot().is_none());
}

#[test]
fn projection_covers_endpoints_midpoint_tensor_clamp_exclusion_spread_and_pair() {
    let layout = layout();
    let left = descriptor(CleanSourceClass::ExplicitChannel, "left", vec![0.75]);
    let projected = layout.project(&left).expect("active channel unit vector");
    assert_eq!(projected, vec![1.0, 0.0]);

    let midpoint = descriptor(CleanSourceClass::DynamicPoint, "p", vec![0.5]);
    let projected = layout.project(&midpoint).expect("midpoint interpolation");
    let root = 0.5_f64.sqrt();
    assert!((projected[0] - root).abs() < 1e-12);
    assert!((projected[1] - root).abs() < 1e-12);
    assert!((projected[0].mul_add(projected[0], projected[1] * projected[1]) - 1.0).abs() < 1e-12);

    let clamped = descriptor(CleanSourceClass::DynamicPoint, "p", vec![2.0]);
    assert_eq!(layout.project(&clamped).unwrap(), vec![0.0, 1.0]);

    let mut spread = descriptor(CleanSourceClass::DynamicRegion, "region", vec![0.5]);
    spread.spread = Some(CleanSpreadProfile {
        samples: vec![
            CleanSpreadSample {
                position: vec![0.0],
                weight: 0.5,
            },
            CleanSpreadSample {
                position: vec![1.0],
                weight: 0.5,
            },
        ],
    });
    let spread_vector = layout.project(&spread).expect("spread composition");
    assert!((spread_vector[0] - root).abs() < 1e-12);
    assert!((spread_vector[1] - root).abs() < 1e-12);

    let mut paired = descriptor(CleanSourceClass::DynamicPoint, "pair", vec![0.0]);
    paired.paired = Some(CleanPairedGeometry {
        first: vec![1.0, 0.0],
        second: vec![0.0, 1.0],
        blend: 0.5,
    });
    let paired_vector = layout.project(&paired).expect("paired geometry");
    assert!((paired_vector[0] - root).abs() < 1e-12);
    assert!((paired_vector[1] - root).abs() < 1e-12);

    let inactive = descriptor(CleanSourceClass::Inactive, "inactive", vec![0.5]);
    assert_eq!(layout.project(&inactive).unwrap(), vec![0.0, 0.0]);
}

#[test]
fn scheduler_has_q32_boundaries_restart_reset_and_partition_invariance() {
    let mut whole = CleanRouteScheduler::new();
    whole.set_target(1.0, true, 64, 48_000).expect("Q32 target");
    let mut expected = vec![0.0; 96];
    whole.process(&mut expected);
    assert_eq!(expected[0], 0.0);
    assert!((expected[31] - 31.0 / 64.0).abs() < 1e-12);
    assert!((expected[63] - 63.0 / 64.0).abs() < 1e-12);
    assert_eq!(expected[64], 1.0);

    let mut split = CleanRouteScheduler::new();
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
    let topology = CleanTopologySnapshot {
        explicit_groups: Vec::new(),
        fixed_layout: Vec::new(),
        dynamic_records: vec![
            CleanBindingRecord {
                descriptor: descriptor(CleanSourceClass::ExplicitChannel, "left", vec![0.0]),
                scalar: 0.5,
                active: true,
            },
            CleanBindingRecord {
                descriptor: descriptor(CleanSourceClass::ExplicitChannel, "right", vec![0.0]),
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
    let mut bridge = ExperimentalCleanSpatialBridge::new();
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
        .expect("experimental clean render");
    assert_eq!(left, vec![1.0, 1.5, 2.0]);
    assert_eq!(right, vec![5.0, 6.0, 7.0]);
    assert_eq!(bridge.semantic_binding(), SemanticBindingState::Unresolved);
    assert!(!bridge.is_production_resolved());
}

#[test]
fn invalid_clean_inputs_are_rejected_without_profile_reinterpretation() {
    let bad = CleanSpatialLayout::new(
        vec![CleanLayoutChannel {
            identity: "left".to_owned(),
            enabled: false,
            lfe: false,
        }],
        vec![vec![0.0, 1.0]],
        vec![],
        vec![],
    );
    assert!(bad.is_err());

    let mut bridge = ExperimentalCleanSpatialBridge::new();
    let input = [vec![1.0, 1.0]];
    let refs: Vec<&[f64]> = input.iter().map(Vec::as_slice).collect();
    let mut left = vec![0.0; 2];
    let mut right = vec![0.0; 2];
    let mut outputs: Vec<&mut [f64]> = vec![&mut left, &mut right];
    let error = bridge
        .render_coordinates(&refs, None, None, &layout(), 0, 48_000, &mut outputs)
        .expect_err("missing topology must not guess a semantic state");
    assert!(matches!(error, CleanSpatialBridgeError::Binding(_)));
}
