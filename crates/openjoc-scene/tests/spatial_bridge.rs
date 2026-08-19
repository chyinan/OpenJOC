use openjoc_scene::{
    FixedRouteKey, GainScheduler, JocSpatialBridge, NamedFallbackParameterTuple, NamedTargetId,
    RegionHorizontalState, RegionSemanticState, RegionTopBottomState, RegionTopologySelector,
    SPEAKER_LAYOUT_PRESET_NAMES, SemanticBindingState, SpatialBindingRecord, SpatialBindingState,
    SpatialBridgeError, SpatialCoordinateUpdate, SpatialDescriptor, SpatialDescriptorPatch,
    SpatialExplicitGroup, SpatialExplicitMember, SpatialLayout, SpatialLayoutAnchor,
    SpatialLayoutChannel, SpatialLayoutLayer, SpatialLayoutNode, SpatialLayoutRow,
    SpatialLayoutTopology, SpatialPairedGeometry, SpatialRouteStatus, SpatialRouteVector,
    SpatialSourceClass, SpatialSpreadProfile, SpatialSpreadSample, SpatialTopologySnapshot,
    SpeakerLayoutPreset, named_fallback_gain, named_fallback_product_q12, named_fallback_q12,
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
        pair_span_q15: None,
        raw3: Some(vec![3, 7]),
        extent: None,
        zones: None,
        channel_lock: false,
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

fn three_anchor_pair_layout() -> SpatialLayout {
    SpatialLayout::new(
        vec![
            SpatialLayoutChannel {
                identity: "left".to_owned(),
                enabled: true,
                lfe: false,
            },
            SpatialLayoutChannel {
                identity: "centre".to_owned(),
                enabled: true,
                lfe: false,
            },
            SpatialLayoutChannel {
                identity: "right".to_owned(),
                enabled: true,
                lfe: false,
            },
        ],
        vec![vec![0.0, 0.25, 1.0]],
        vec![
            SpatialLayoutNode {
                knot_indices: vec![0],
                vector: vec![1.0, 0.0, 0.0],
            },
            SpatialLayoutNode {
                knot_indices: vec![1],
                vector: vec![0.0, 1.0, 0.0],
            },
            SpatialLayoutNode {
                knot_indices: vec![2],
                vector: vec![0.0, 0.0, 1.0],
            },
        ],
        Vec::new(),
    )
    .expect("valid three-anchor Pair topology")
}

fn normalize_target(mut target: Vec<f64>) -> Vec<f64> {
    let norm = target.iter().map(|value| value * value).sum::<f64>().sqrt();
    for value in &mut target {
        *value /= norm;
    }
    target
}

fn descriptor_at(layout: &SpatialLayout, identity: &str, center: [f64; 3]) -> SpatialDescriptor {
    let coordinates = if layout.coordinate_dimension_count() == 1 {
        vec![center[0]]
    } else {
        center.to_vec()
    };
    descriptor(SpatialSourceClass::DynamicPoint, identity, coordinates)
}

#[test]
fn red_semantic_pair_uses_two_symmetric_endpoint_targets() {
    let layout = three_anchor_pair_layout();
    let mut pair = descriptor(SpatialSourceClass::DynamicPoint, "pair", vec![0.5]);
    pair.pair_span_q15 = Some(32_767);

    let target = layout.project(&pair).expect("semantic Pair target");
    let expected = normalize_target(vec![1.0, 0.0, 1.0]);
    for (actual, expected) in target.iter().zip(expected) {
        assert!(
            (actual - expected).abs() < 2.0e-12,
            "{actual} != {expected}"
        );
    }
}

#[test]
fn red_semantic_pair_shrinks_one_shared_span_at_the_wall() {
    let layout = three_anchor_pair_layout();
    let mut pair = descriptor(SpatialSourceClass::DynamicPoint, "pair", vec![0.2]);
    pair.pair_span_q15 = Some(16_384);

    let target = layout.project(&pair).expect("boundary Pair target");
    let left = layout
        .project(&descriptor(
            SpatialSourceClass::DynamicPoint,
            "left",
            vec![0.0],
        ))
        .expect("left endpoint target");
    let right = layout
        .project(&descriptor(
            SpatialSourceClass::DynamicPoint,
            "right",
            vec![0.4],
        ))
        .expect("right endpoint target");
    let expected = normalize_target(
        left.iter()
            .zip(right)
            .map(|(left, right)| left + right)
            .collect(),
    );
    for (actual, expected) in target.iter().zip(expected) {
        assert!(
            (actual - expected).abs() < 2.0e-12,
            "{actual} != {expected}"
        );
    }
}

#[test]
fn red_semantic_pair_differs_from_the_legacy_trigonometric_vector_blend() {
    let layout = three_anchor_pair_layout();
    let mut legacy = descriptor(SpatialSourceClass::DynamicPoint, "legacy", vec![0.5]);
    legacy.paired = Some(SpatialPairedGeometry {
        first: vec![1.0, 0.0, 0.0],
        second: vec![0.0, 0.0, 1.0],
        blend: 0.25,
    });
    let legacy_target = layout.project(&legacy).expect("legacy vector-pair target");

    let mut semantic = descriptor(SpatialSourceClass::DynamicPoint, "semantic", vec![0.5]);
    semantic.pair_span_q15 = Some(32_767);
    let clean_target = layout
        .project(&semantic)
        .expect("clean semantic Pair target");

    assert!((legacy_target[0] - (std::f64::consts::PI / 8.0).cos()).abs() < 2.0e-12);
    assert!((legacy_target[2] - (std::f64::consts::PI / 8.0).sin()).abs() < 2.0e-12);
    assert!((clean_target[0] - 0.5_f64.sqrt()).abs() < 2.0e-12);
    assert!((clean_target[2] - 0.5_f64.sqrt()).abs() < 2.0e-12);
}

fn expected_semantic_pair_target(
    layout: &SpatialLayout,
    center: [f64; 3],
    pair_span_q15: u16,
) -> Vec<f64> {
    let point = descriptor_at(layout, "expected-point", center);
    if pair_span_q15 == 0 {
        return layout.project(&point).expect("expected point target");
    }
    let requested_span = f64::from(pair_span_q15) / 32_768.0;
    let effective_span = requested_span.min(center[0]).min(1.0 - center[0]);
    if effective_span == 0.0 {
        return layout.project(&point).expect("expected wall point target");
    }
    let mut endpoint_a = point.clone();
    endpoint_a.coordinates[0] = center[0] - effective_span;
    let mut endpoint_b = point;
    endpoint_b.coordinates[0] = center[0] + effective_span;
    let a = layout.project(&endpoint_a).expect("expected A target");
    let b = layout.project(&endpoint_b).expect("expected B target");
    normalize_target(a.into_iter().zip(b).map(|(a, b)| a + b).collect())
}

#[test]
fn semantic_pair_q15_mapping_and_shared_boundary_cap_match_clean_equations() {
    let layout = three_anchor_pair_layout();
    for (center, pair_span_q15) in [
        ([0.5, 0.5, 0.0], 0),
        ([0.5, 0.5, 0.0], 1),
        ([0.5, 0.5, 0.0], 8_192),
        ([0.2, 0.5, 0.0], 16_384),
        ([0.8, 0.5, 0.0], 16_384),
        ([0.5, 0.5, 0.0], 32_767),
        ([0.0, 0.5, 0.0], 32_767),
    ] {
        let mut pair = descriptor_at(&layout, "pair", center);
        pair.pair_span_q15 = Some(pair_span_q15);
        let actual = layout.project(&pair).expect("clean Pair target");
        let expected = expected_semantic_pair_target(&layout, center, pair_span_q15);
        for (actual, expected) in actual.iter().zip(expected) {
            assert!(
                (actual - expected).abs() < 2.0e-12,
                "{actual} != {expected}"
            );
        }
    }
}

#[test]
fn semantic_pair_zero_and_wall_degeneracies_are_exact_point_identities() {
    for name in [
        "5.1", "5.1.2", "5.1.4", "7.1", "7.1.2", "7.1.4", "7.1.6", "9.1", "9.1.2", "9.1.4", "9.1.6",
    ] {
        let layout = executable_layout(name);
        for center in [[0.5, 0.5, 0.0], [0.0, 0.0, 0.0], [1.0, 1.0, 0.5]] {
            let point = descriptor(SpatialSourceClass::DynamicPoint, "point", center.to_vec());
            let mut pair = point.clone();
            pair.pair_span_q15 = Some(if center[0] == 0.5 { 0 } else { 32_767 });
            assert_eq!(
                layout.project(&pair),
                layout.project(&point),
                "{name} {center:?}"
            );
        }
    }
}

#[test]
fn semantic_pair_endpoint_targets_are_ordinary_point_targets_and_sum_once() {
    let layout = three_anchor_pair_layout();
    let center = [0.2, 0.5, 0.0];
    let pair_span_q15 = 16_384;
    let mut pair = descriptor_at(&layout, "pair", center);
    pair.pair_span_q15 = Some(pair_span_q15);
    let actual = layout.project(&pair).expect("clean Pair target");
    let expected = expected_semantic_pair_target(&layout, center, pair_span_q15);
    assert_eq!(actual.len(), 3);
    assert!(
        actual
            .iter()
            .all(|value| value.is_finite() && *value >= 0.0)
    );
    assert!((actual.iter().map(|value| value * value).sum::<f64>() - 1.0).abs() < 2.0e-12);
    for (actual, expected) in actual.iter().zip(expected) {
        assert!(
            (actual - expected).abs() < 2.0e-12,
            "{actual} != {expected}"
        );
    }
}

#[test]
fn semantic_pair_mirrors_under_symmetric_two_speaker_topology() {
    let layout = layout();
    let mut left = descriptor(SpatialSourceClass::DynamicPoint, "left-pair", vec![0.2]);
    left.pair_span_q15 = Some(8_192);
    let mut right = descriptor(SpatialSourceClass::DynamicPoint, "right-pair", vec![0.8]);
    right.pair_span_q15 = Some(8_192);
    let left_target = layout.project(&left).expect("left Pair target");
    let right_target = layout.project(&right).expect("right Pair target");
    assert!((left_target[0] - right_target[1]).abs() < 2.0e-12);
    assert!((left_target[1] - right_target[0]).abs() < 2.0e-12);
}

#[test]
fn semantic_pair_uses_one_operator_across_the_admitted_layout_matrix() {
    for name in [
        "5.1", "5.1.2", "5.1.4", "7.1", "7.1.2", "7.1.4", "7.1.6", "9.1", "9.1.2", "9.1.4", "9.1.6",
    ] {
        let layout = executable_layout(name);
        let mut pair = dynamic_point(0.5, 0.5, 0.0);
        pair.pair_span_q15 = Some(8_192);
        let target = layout.project(&pair).expect("admitted layout Pair target");
        assert_eq!(target.len(), layout.active_channel_count(), "{name}");
        assert_unit_l2(&target);
    }
}

fn three_layer_pair_layout() -> SpatialLayout {
    SpatialLayout::from_topology(
        clean_channels(&["A", "B", "C"], None),
        SpatialLayoutTopology {
            layers: vec![
                SpatialLayoutLayer {
                    z: 0.0,
                    rows: vec![clean_row(0.0, vec![clean_anchor("A", 0.5, 0.0, 0.0)])],
                },
                SpatialLayoutLayer {
                    z: 0.5,
                    rows: vec![clean_row(0.0, vec![clean_anchor("B", 0.5, 0.0, 0.5)])],
                },
                SpatialLayoutLayer {
                    z: 1.0,
                    rows: vec![clean_row(0.0, vec![clean_anchor("C", 0.5, 0.0, 1.0)])],
                },
            ],
            aliases: Vec::new(),
        },
        Vec::new(),
    )
    .expect("valid three-layer storage topology")
}

#[test]
fn semantic_pair_withheld_combinations_and_malformed_state_fail_closed() {
    let layout = executable_layout("7.1.4");
    let mut pair = dynamic_point(0.5, 0.5, 0.0);
    pair.pair_span_q15 = Some(8_192);

    let mut region_pair = pair.clone();
    region_pair.zones = Some(region_zones(
        RegionHorizontalState::ScreenOnly,
        RegionTopBottomState::Include,
    ));
    assert_eq!(
        layout.project(&region_pair),
        Err(openjoc_scene::SpatialProjectionError::InvalidPair)
    );

    let mut locked_pair = pair.clone();
    locked_pair.channel_lock = true;
    assert_eq!(
        layout.project(&locked_pair),
        Err(openjoc_scene::SpatialProjectionError::UnsupportedChannelLock)
    );

    let mut extent_pair = pair.clone();
    extent_pair.extent = Some([0.5; 3]);
    assert_eq!(
        layout.project(&extent_pair),
        Err(openjoc_scene::SpatialProjectionError::UnsupportedExtent)
    );

    let mut legacy_collision = pair.clone();
    legacy_collision.paired = Some(SpatialPairedGeometry {
        first: vec![1.0; layout.active_channel_count()],
        second: vec![0.0; layout.active_channel_count()],
        blend: 0.5,
    });
    assert_eq!(
        layout.project(&legacy_collision),
        Err(openjoc_scene::SpatialProjectionError::InvalidPair)
    );

    let mut malformed = pair.clone();
    malformed.pair_span_q15 = Some(32_768);
    assert_eq!(
        layout.project(&malformed),
        Err(openjoc_scene::SpatialProjectionError::InvalidPair)
    );

    let mut dynamic_region = pair.clone();
    dynamic_region.source_class = SpatialSourceClass::DynamicRegion;
    assert_eq!(
        layout.project(&dynamic_region),
        Err(openjoc_scene::SpatialProjectionError::InvalidPair)
    );

    let three_layers = three_layer_pair_layout();
    let three_layer_target = three_layers
        .project(&pair)
        .expect("Pair uses the same generic multilayer projector");
    assert_unit_l2(&three_layer_target);
}

#[test]
fn semantic_pair_state_inherits_atomically_and_resets_without_endpoint_history() {
    let initial = {
        let mut descriptor = dynamic_point(0.3, 0.5, 0.0);
        descriptor.pair_span_q15 = Some(8_192);
        descriptor
    };
    let topology = single_record_topology(initial);
    let mut state = SpatialBindingState::new();
    state
        .apply(Some(&topology), None, 1)
        .expect("Pair state initialization");

    let same_snapshot = SpatialCoordinateUpdate {
        ordinal: 0,
        descriptor: Some(SpatialDescriptorPatch {
            coordinates: Some(vec![0.7, 0.5, 0.0]),
            pair_span_q15: Some(Some(16_384)),
            ..SpatialDescriptorPatch::default()
        }),
        scalar: None,
        active: None,
    };
    state
        .apply(None, Some(std::slice::from_ref(&same_snapshot)), 1)
        .expect("same-snapshot center/span update");
    let effective = &state.snapshot().expect("effective Pair state").records[0].descriptor;
    assert_eq!(effective.coordinates, vec![0.7, 0.5, 0.0]);
    assert_eq!(effective.pair_span_q15, Some(16_384));

    let inherit_center = SpatialCoordinateUpdate {
        ordinal: 0,
        descriptor: Some(SpatialDescriptorPatch {
            coordinates: Some(vec![0.2, 0.5, 0.0]),
            ..SpatialDescriptorPatch::default()
        }),
        scalar: None,
        active: None,
    };
    state
        .apply(None, Some(std::slice::from_ref(&inherit_center)), 1)
        .expect("center-only inheritance");
    assert_eq!(
        state.snapshot().unwrap().records[0]
            .descriptor
            .pair_span_q15,
        Some(16_384)
    );

    state.reset();
    assert!(state.snapshot().is_none());
}

#[test]
fn semantic_pair_mode_and_span_changes_use_only_the_existing_q32_scheduler() {
    let layout = three_anchor_pair_layout();
    let mut initial = descriptor(SpatialSourceClass::DynamicPoint, "pair", vec![0.5]);
    initial.pair_span_q15 = Some(0);
    let topology = single_record_topology(initial);
    let mut bridge = JocSpatialBridge::new();
    let _initial_output = render_block(&mut bridge, &layout, Some(&topology), None, 0, 1);
    let point_target = layout
        .project(&descriptor(
            SpatialSourceClass::DynamicPoint,
            "point",
            vec![0.5],
        ))
        .expect("point target");

    let pair_on = SpatialCoordinateUpdate {
        ordinal: 0,
        descriptor: Some(SpatialDescriptorPatch {
            pair_span_q15: Some(Some(8_192)),
            ..SpatialDescriptorPatch::default()
        }),
        scalar: None,
        active: None,
    };
    let first_pair = render_block(
        &mut bridge,
        &layout,
        None,
        Some(std::slice::from_ref(&pair_on)),
        0,
        1,
    );
    let pair_target = layout
        .project(&{
            let mut descriptor = descriptor(SpatialSourceClass::DynamicPoint, "pair", vec![0.5]);
            descriptor.pair_span_q15 = Some(8_192);
            descriptor
        })
        .expect("Pair target");
    let max_pair_target = layout
        .project(&{
            let mut descriptor = descriptor(SpatialSourceClass::DynamicPoint, "pair", vec![0.5]);
            descriptor.pair_span_q15 = Some(32_767);
            descriptor
        })
        .expect("maximum Pair target");
    assert_eq!(
        first_pair
            .iter()
            .map(|channel| channel[0])
            .collect::<Vec<_>>(),
        pair_target
    );

    let span_update = SpatialCoordinateUpdate {
        ordinal: 0,
        descriptor: Some(SpatialDescriptorPatch {
            pair_span_q15: Some(Some(32_767)),
            ..SpatialDescriptorPatch::default()
        }),
        scalar: None,
        active: None,
    };
    let span_ramp = render_block(
        &mut bridge,
        &layout,
        None,
        Some(std::slice::from_ref(&span_update)),
        64,
        64,
    );
    assert!((span_ramp[0][0] - pair_target[0]).abs() < 2.0e-12);
    assert!((span_ramp[0][63] - pair_target[0]).abs() > 1.0e-5);

    let pair_off = SpatialCoordinateUpdate {
        ordinal: 0,
        descriptor: Some(SpatialDescriptorPatch {
            pair_span_q15: Some(Some(0)),
            ..SpatialDescriptorPatch::default()
        }),
        scalar: None,
        active: None,
    };
    let off_ramp = render_block(
        &mut bridge,
        &layout,
        None,
        Some(std::slice::from_ref(&pair_off)),
        64,
        64,
    );
    let max_pair_difference = off_ramp[0]
        .iter()
        .zip(&point_target)
        .map(|(actual, expected)| (actual - expected).abs())
        .fold(0.0, f64::max);
    assert!(max_pair_difference > 1.0e-5);
    assert!((off_ramp[0][0] - max_pair_target[0]).abs() < 2.0e-12);
}

#[test]
fn region_metadata_affects_projection() {
    let layout = executable_layout("7.1.4");
    let default_descriptor = dynamic_point(0.25, 0.5, 0.0);
    let mut screen_only = default_descriptor.clone();
    screen_only.zones = Some([true, false, false, false, false, true]);

    let default_target = layout
        .project(&default_descriptor)
        .expect("default point target");
    let constrained_target = layout.project(&screen_only).expect("screen-only target");

    assert_ne!(
        default_target, constrained_target,
        "non-default semantic region must not be ignored"
    );
}

#[test]
fn red_dynamic_channel_lock_inside_gate_should_emit_exclusive_target() {
    let layout = executable_layout("5.1");
    let mut locked = dynamic_point(0.5, 0.0, 0.0);
    locked.channel_lock = true;

    let target = layout
        .project(&locked)
        .expect("active ChannelLock should project");

    assert_eq!(target[active_index(&layout, "FC")], 1.0);
    assert!(
        target
            .iter()
            .enumerate()
            .all(|(index, value)| index == active_index(&layout, "FC") || *value == 0.0)
    );
}

#[test]
fn red_dynamic_channel_lock_exact_threshold_should_preserve_ordinary_target() {
    let layout = executable_layout("5.1");
    let ordinary = layout
        .project(&dynamic_point(0.2, 0.0, 0.0))
        .expect("ordinary point target");
    let mut locked = dynamic_point(0.2, 0.0, 0.0);
    locked.channel_lock = true;

    let target = layout
        .project(&locked)
        .expect("active ChannelLock should project");

    assert_eq!(target, ordinary);
}

#[test]
fn red_dynamic_channel_lock_switch_should_use_current_dominant_output() {
    let layout = executable_layout("5.1");
    let mut first = dynamic_point(0.0, 0.0, 0.0);
    first.channel_lock = true;
    let mut second = dynamic_point(0.5, 0.0, 0.0);
    second.channel_lock = true;

    let first_target = layout.project(&first).expect("first locked target");
    let second_target = layout.project(&second).expect("second locked target");

    assert_eq!(first_target[active_index(&layout, "FL")], 1.0);
    assert_eq!(second_target[active_index(&layout, "FC")], 1.0);
    assert_ne!(first_target, second_target);
}

#[test]
fn channel_lock_outcome_snaps_only_the_local_effective_position() {
    let layout = executable_layout("5.1.4");
    let authored = [TOP_INNER_LEFT_CLEAN, TOP_FRONT_Y_CLEAN, QMAX_CLEAN];
    let mut descriptor = dynamic_point(authored[0], authored[1], authored[2]);
    descriptor.channel_lock = true;

    let outcome = layout
        .project_with_outcome(&descriptor)
        .expect("upper anchor ChannelLock outcome");

    assert_eq!(outcome.target[active_index(&layout, "TFL")], 1.0);
    assert_eq!(outcome.locked_output, Some(active_index(&layout, "TFL")));
    assert_eq!(
        outcome.effective_position,
        Some([TOP_INNER_LEFT_CLEAN, TOP_FRONT_Y_CLEAN, QMAX_CLEAN])
    );
    assert_eq!(descriptor.coordinates, authored);
}

#[test]
fn channel_lock_uses_ordinary_maximum_instead_of_nearest_anchor() {
    let layout = SpatialLayout::from_topology(
        clean_channels(&["A", "B"], None),
        SpatialLayoutTopology {
            layers: vec![clean_bed(vec![
                clean_row(0.0, vec![clean_anchor("A", 0.0, 0.0, 0.0)]),
                clean_row(0.15, vec![clean_anchor("B", 0.15, 0.15, 0.0)]),
            ])],
            aliases: Vec::new(),
        },
        Vec::new(),
    )
    .expect("selection-order fixture");
    let mut descriptor = dynamic_point(0.0, 0.1, 0.0);
    descriptor.channel_lock = true;

    let outcome = layout
        .project_with_outcome(&descriptor)
        .expect("selection-order outcome");

    assert_eq!(outcome.target[active_index(&layout, "B")], 1.0);
    assert_eq!(outcome.locked_output, Some(active_index(&layout, "B")));
    assert_eq!(outcome.effective_position, Some([0.15, 0.15, 0.0]));
}

#[test]
fn channel_lock_threshold_is_strict_and_uses_full_xyz_distance() {
    let layout = executable_layout("5.1");

    let mut inside = dynamic_point(0.19999999999999998, 0.0, 0.0);
    inside.channel_lock = true;
    let inside_outcome = layout
        .project_with_outcome(&inside)
        .expect("just-inside threshold outcome");
    assert_eq!(
        inside_outcome.locked_output,
        Some(active_index(&layout, "FL"))
    );

    let ordinary = layout
        .project(&dynamic_point(0.2, 0.0, 0.0))
        .expect("ordinary exact-threshold fixture");
    let mut equal = dynamic_point(0.2, 0.0, 0.0);
    equal.channel_lock = true;
    let equal_outcome = layout
        .project_with_outcome(&equal)
        .expect("exact-threshold outcome");
    assert_eq!(equal_outcome.locked_output, None);
    assert_eq!(equal_outcome.target, ordinary);
    assert_eq!(equal_outcome.effective_position, Some([0.2, 0.0, 0.0]));

    let mut vertical = dynamic_point(TOP_INNER_LEFT_CLEAN, TOP_FRONT_Y_CLEAN, 0.79);
    vertical.channel_lock = true;
    let vertical_outcome = executable_layout("5.1.4")
        .project_with_outcome(&vertical)
        .expect("full XYZ upper fixture");
    assert_eq!(vertical_outcome.locked_output, None);
    assert_eq!(
        vertical_outcome.effective_position,
        Some([TOP_INNER_LEFT_CLEAN, TOP_FRONT_Y_CLEAN, 0.79])
    );
}

#[test]
fn channel_lock_excludes_lfe_and_fails_closed_for_withheld_source_geometry() {
    let layout = executable_layout("5.1");
    let mut locked = dynamic_point(0.5, 0.0, 0.0);
    locked.channel_lock = true;
    let target = layout.project(&locked).expect("standalone ChannelLock");
    assert_eq!(target.len(), layout.active_channel_count());
    assert_eq!(target[active_index(&layout, "FC")], 1.0);
    assert_eq!(
        layout.channels().iter().position(|channel| channel.lfe),
        Some(3)
    );

    let mut region = locked.clone();
    region.zones = Some(region_zones(
        RegionHorizontalState::ScreenOnly,
        RegionTopBottomState::Include,
    ));
    let region_target = layout.project(&region).expect("ChannelLock × Region");
    assert_eq!(region_target, target);

    let mut extent = locked.clone();
    extent.extent = Some([0.1, 0.0, 0.0]);
    let extent_target = layout.project(&extent).expect("ChannelLock × Extent");
    assert_eq!(extent_target, target);

    let mut region_extent = region.clone();
    region_extent.extent = Some([0.1, 0.0, 0.0]);
    assert_eq!(
        layout
            .project(&region_extent)
            .expect("ChannelLock × Region × Extent"),
        target
    );

    let mut spread = locked.clone();
    spread.spread = Some(SpatialSpreadProfile {
        samples: vec![SpatialSpreadSample {
            position: vec![0.5, 0.0, 0.0],
            weight: 1.0,
        }],
    });
    assert_eq!(
        layout.project(&spread),
        Err(openjoc_scene::SpatialProjectionError::UnsupportedChannelLock)
    );

    let mut paired = locked.clone();
    paired.paired = Some(SpatialPairedGeometry {
        first: vec![0.0; layout.active_channel_count()],
        second: vec![0.0; layout.active_channel_count()],
        blend: 0.5,
    });
    assert_eq!(
        layout.project(&paired),
        Err(openjoc_scene::SpatialProjectionError::UnsupportedChannelLock)
    );

    let mut explicit = descriptor(SpatialSourceClass::ExplicitChannel, "FC", Vec::new());
    explicit.channel_lock = true;
    assert_eq!(
        layout.project(&explicit),
        Err(openjoc_scene::SpatialProjectionError::UnsupportedChannelLock)
    );
}

#[test]
fn channel_lock_off_is_an_exact_point_identity_across_admitted_layouts() {
    for name in [
        "5.1", "5.1.2", "5.1.4", "7.1", "7.1.2", "7.1.4", "7.1.6", "9.1", "9.1.2", "9.1.4", "9.1.6",
    ] {
        let layout = executable_layout(name);
        for position in [
            (0.5, 0.0, 0.0),
            (0.0, 0.5, 0.0),
            (QMAX_CLEAN, QMAX_CLEAN, 0.0),
            (0.5, 0.5, QMAX_CLEAN),
        ] {
            let descriptor = dynamic_point(position.0, position.1, position.2);
            let ordinary = layout.project(&descriptor).expect("ordinary point target");
            let outcome = layout
                .project_with_outcome(&descriptor)
                .expect("ChannelLock OFF outcome");
            assert_eq!(outcome.target, ordinary, "{name} {position:?}");
            assert_eq!(
                outcome.effective_position,
                Some([position.0, position.1, position.2])
            );
            assert_eq!(outcome.locked_output, None);
        }
    }
}

#[test]
fn channel_lock_maps_every_current_layout_anchor_without_layout_branches() {
    for name in [
        "5.1", "5.1.2", "5.1.4", "7.1", "7.1.2", "7.1.4", "7.1.6", "9.1", "9.1.2", "9.1.4", "9.1.6",
    ] {
        let layout = executable_layout(name);
        for layer in &layout.topology().layers {
            for row in &layer.rows {
                for anchor in &row.anchors {
                    let mut descriptor = dynamic_point(anchor.x, anchor.y, anchor.z);
                    descriptor.channel_lock = true;
                    let outcome = layout
                        .project_with_outcome(&descriptor)
                        .expect("exact anchor ChannelLock outcome");
                    let selected = active_index(&layout, &anchor.identity);
                    assert_eq!(outcome.target[selected], 1.0, "{name} {}", anchor.identity);
                    assert!(
                        outcome
                            .target
                            .iter()
                            .enumerate()
                            .all(|(index, value)| index == selected || *value == 0.0)
                    );
                    assert_eq!(outcome.locked_output, Some(selected));
                    assert_eq!(
                        outcome.effective_position,
                        Some([anchor.x, anchor.y, anchor.z])
                    );
                }
            }
        }
    }
}

#[test]
fn channel_lock_configuration_inherits_without_inheriting_an_acquired_target() {
    let mut state = SpatialBindingState::new();
    let mut initial = topology();
    initial.dynamic_records[0].descriptor.channel_lock = true;
    state
        .apply(Some(&initial), None, initial.dynamic_records.len())
        .expect("initial ChannelLock configuration");

    let update = SpatialCoordinateUpdate {
        ordinal: 4,
        descriptor: Some(SpatialDescriptorPatch {
            coordinates: Some(vec![0.25]),
            ..SpatialDescriptorPatch::default()
        }),
        scalar: None,
        active: None,
    };
    state
        .apply(None, Some(std::slice::from_ref(&update)), 1)
        .expect("selective update inherits ChannelLock");
    assert!(state.snapshot().unwrap().records[4].descriptor.channel_lock);

    let off = SpatialCoordinateUpdate {
        ordinal: 4,
        descriptor: Some(SpatialDescriptorPatch {
            channel_lock: Some(Some(false)),
            ..SpatialDescriptorPatch::default()
        }),
        scalar: None,
        active: None,
    };
    state
        .apply(None, Some(std::slice::from_ref(&off)), 1)
        .expect("explicit OFF update");
    assert!(!state.snapshot().unwrap().records[4].descriptor.channel_lock);
}

#[test]
fn red_region_projection_must_not_be_a_post_projection_mask() {
    let layout = executable_layout("7.1.4");
    let mut screen_only = dynamic_point(0.25, QMAX_CLEAN, 0.0);
    screen_only.zones = Some([true, false, false, false, false, true]);

    let constrained_target = layout.project(&screen_only).expect("screen-only target");
    let screen_indices = [
        active_index(&layout, "FL"),
        active_index(&layout, "FC"),
        active_index(&layout, "FR"),
    ];
    let mut naive = layout
        .project(&dynamic_point(0.25, QMAX_CLEAN, 0.0))
        .expect("full-layout target");
    for (index, value) in naive.iter_mut().enumerate() {
        if !screen_indices.contains(&index) {
            *value = 0.0;
        }
    }
    let norm = naive.iter().map(|value| value * value).sum::<f64>().sqrt();
    if norm > 0.0 {
        for value in &mut naive {
            *value /= norm;
        }
    }

    assert_ne!(
        constrained_target, naive,
        "constrained-topology projection must differ from a full-vector post-mask"
    );
}

#[test]
fn red_region_outside_support_must_clamp_instead_of_mute() {
    let layout = executable_layout("7.1.4");
    let mut screen_only = dynamic_point(0.25, QMAX_CLEAN, 0.0);
    screen_only.zones = Some([true, false, false, false, false, true]);

    let target = layout
        .project(&screen_only)
        .expect("screen-only outside-support target");

    assert!(
        target.iter().any(|value| *value > 0.0),
        "a valid selected topology must clamp outside support, not mute"
    );
}

fn region_zones(horizontal: RegionHorizontalState, top_bottom: RegionTopBottomState) -> [bool; 6] {
    RegionSemanticState {
        horizontal,
        top_bottom,
    }
    .to_decoded_zones()
}

fn ids(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

#[test]
fn region_semantic_adapter_accepts_only_the_six_clean_states() {
    for horizontal in [
        RegionHorizontalState::NoConstraints,
        RegionHorizontalState::BackExcluded,
        RegionHorizontalState::SideExcluded,
        RegionHorizontalState::CentreAndBack,
        RegionHorizontalState::ScreenOnly,
        RegionHorizontalState::SurroundOnly,
    ] {
        for top_bottom in [RegionTopBottomState::Include, RegionTopBottomState::Exclude] {
            let state = RegionSemanticState {
                horizontal,
                top_bottom,
            };
            assert_eq!(
                RegionSemanticState::from_decoded_zones(state.to_decoded_zones()),
                Ok(state)
            );
        }
    }
    assert!(
        RegionSemanticState::from_decoded_zones([false, true, false, true, false, true]).is_err()
    );
}

#[test]
fn region_topology_rebuilds_membership_before_projection() {
    let layout = executable_layout("7.1.4");
    let mut selector = RegionTopologySelector::new();
    let mut bed_identities = |state| {
        let selected = selector
            .select(
                &layout,
                RegionSemanticState {
                    horizontal: state,
                    top_bottom: RegionTopBottomState::Include,
                },
            )
            .expect("admitted constrained topology");
        selected.topology().layers[0]
            .rows
            .iter()
            .flat_map(|row| row.anchors.iter().map(|anchor| anchor.identity.as_str()))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };

    assert_eq!(
        bed_identities(RegionHorizontalState::NoConstraints),
        ids(&["FL", "FC", "FR", "Ls", "Rs", "Lb", "Rb"])
    );
    assert_eq!(
        bed_identities(RegionHorizontalState::BackExcluded),
        ids(&["FL", "FC", "FR", "Ls", "Rs"])
    );
    assert_eq!(
        bed_identities(RegionHorizontalState::SideExcluded),
        ids(&["FL", "FC", "FR", "Lb", "Rb"])
    );
    assert_eq!(
        bed_identities(RegionHorizontalState::CentreAndBack),
        ids(&["FC", "Lb", "Rb"])
    );
    assert_eq!(
        bed_identities(RegionHorizontalState::ScreenOnly),
        ids(&["FL", "FC", "FR"])
    );
    assert_eq!(
        bed_identities(RegionHorizontalState::SurroundOnly),
        ids(&["Ls", "Rs"])
    );
    assert_eq!(selector.cached_topology_count(), 5);
}

#[test]
fn region_top_bottom_is_independent_and_keeps_the_canonical_output_vector() {
    let layout = executable_layout("7.1.4");
    let mut include = dynamic_point(0.5, 0.25, QMAX_CLEAN);
    include.zones = Some(region_zones(
        RegionHorizontalState::ScreenOnly,
        RegionTopBottomState::Include,
    ));
    let mut exclude = include.clone();
    exclude.zones = Some(region_zones(
        RegionHorizontalState::ScreenOnly,
        RegionTopBottomState::Exclude,
    ));
    let included = layout.project(&include).expect("upper-inclusive target");
    let excluded = layout.project(&exclude).expect("bed-only target");

    assert!(included[active_index(&layout, "TFL")] > 0.0);
    assert!(included[active_index(&layout, "TFR")] > 0.0);
    assert_eq!(included[active_index(&layout, "FC")], 0.0);
    assert!(excluded[active_index(&layout, "FC")] > 0.0);
    assert_eq!(excluded[active_index(&layout, "TFL")], 0.0);
    assert_eq!(excluded.len(), layout.active_channel_count());
}

#[test]
fn region_default_identity_holds_across_public_layouts() {
    for name in [
        "5.1", "5.1.2", "5.1.4", "7.1", "7.1.2", "7.1.4", "7.1.6", "9.1", "9.1.2", "9.1.4", "9.1.6",
    ] {
        let layout = executable_layout(name);
        let mut default_region = dynamic_point(0.37, 0.61, 0.23);
        default_region.zones = Some(region_zones(
            RegionHorizontalState::NoConstraints,
            RegionTopBottomState::Include,
        ));
        let ordinary = {
            let mut descriptor = default_region.clone();
            descriptor.zones = None;
            layout.project(&descriptor).expect("ordinary target")
        };
        let through_region = layout
            .project(&default_region)
            .expect("default region target");
        assert_eq!(ordinary, through_region, "{name}");
        assert_eq!(ordinary.len(), layout.active_channel_count(), "{name}");
    }
}

#[test]
fn region_outside_support_uses_selected_endpoint_and_not_forbidden_speakers() {
    let layout = executable_layout("7.1.4");
    let mut descriptor = dynamic_point(0.25, QMAX_CLEAN, 0.0);
    descriptor.zones = Some(region_zones(
        RegionHorizontalState::ScreenOnly,
        RegionTopBottomState::Exclude,
    ));
    let target = layout
        .project(&descriptor)
        .expect("selected endpoint clamp");
    assert!(target[active_index(&layout, "FL")] > 0.0);
    assert!(target[active_index(&layout, "FC")] > 0.0);
    for identity in ["Ls", "Rs", "Lb", "Rb", "TFL", "TFR", "TBL", "TBR"] {
        assert_eq!(target[active_index(&layout, identity)], 0.0, "{identity}");
    }
    assert_unit_l2(&target);
}

#[test]
fn region_selector_cache_is_bounded_and_epoch_resettable() {
    let layout = executable_layout("7.1.4");
    let mut selector = RegionTopologySelector::new();
    let state = RegionSemanticState {
        horizontal: RegionHorizontalState::ScreenOnly,
        top_bottom: RegionTopBottomState::Include,
    };
    selector.select(&layout, state).expect("first topology");
    selector.select(&layout, state).expect("cached topology");
    assert_eq!(selector.cached_topology_count(), 1);
    selector.clear();
    assert_eq!(selector.cached_topology_count(), 0);
}

#[test]
fn invalid_region_update_is_atomic_and_retains_the_last_valid_snapshot() {
    let valid = region_zones(
        RegionHorizontalState::ScreenOnly,
        RegionTopBottomState::Include,
    );
    let mut state = SpatialBindingState::new();
    let topology = SpatialTopologySnapshot {
        dynamic_records: vec![SpatialBindingRecord {
            descriptor: {
                let mut descriptor = dynamic_point(0.25, 0.5, 0.0);
                descriptor.zones = Some(valid);
                descriptor
            },
            scalar: 1.0,
            active: true,
        }],
        ..SpatialTopologySnapshot::default()
    };
    state.apply(Some(&topology), None, 1).expect("valid region");
    let invalid = SpatialCoordinateUpdate {
        ordinal: 0,
        descriptor: Some(SpatialDescriptorPatch {
            zones: Some(Some([false, true, false, true, false, true])),
            ..SpatialDescriptorPatch::default()
        }),
        scalar: None,
        active: None,
    };
    assert_eq!(
        state.apply(None, Some(std::slice::from_ref(&invalid)), 1),
        Err(openjoc_scene::SpatialBindingError::InvalidRegionState([
            false, true, false, true, false, true
        ]))
    );
    assert_eq!(
        state.snapshot().unwrap().records[0].descriptor.zones,
        Some(valid)
    );
}

#[test]
fn region_target_changes_use_the_existing_q32_scheduler() {
    let layout = executable_layout("7.1.4");
    let mut initial = dynamic_point(0.25, 0.5, 0.0);
    initial.extent = Some([0.25; 3]);
    let topology = SpatialTopologySnapshot {
        dynamic_records: vec![SpatialBindingRecord {
            descriptor: initial,
            scalar: 1.0,
            active: true,
        }],
        ..SpatialTopologySnapshot::default()
    };
    let input = vec![1.0; 64];
    let mut bridge = JocSpatialBridge::new();
    let mut storage = vec![vec![0.0; input.len()]; layout.active_channel_count()];
    let mut outputs = storage
        .iter_mut()
        .map(Vec::as_mut_slice)
        .collect::<Vec<_>>();
    bridge
        .render_coordinates(
            &[input.as_slice()],
            Some(&topology),
            None,
            &layout,
            0,
            48_000,
            &mut outputs,
        )
        .expect("initial target");

    let update = SpatialCoordinateUpdate {
        ordinal: 0,
        descriptor: Some(SpatialDescriptorPatch {
            zones: Some(Some(region_zones(
                RegionHorizontalState::ScreenOnly,
                RegionTopBottomState::Include,
            ))),
            ..SpatialDescriptorPatch::default()
        }),
        scalar: None,
        active: None,
    };
    bridge
        .render_coordinates(
            &[input.as_slice()],
            None,
            Some(std::slice::from_ref(&update)),
            &layout,
            64,
            48_000,
            &mut outputs,
        )
        .expect("region target event");
    drop(outputs);
    let left = active_index(&layout, "Ls");
    assert!(storage[left][0] > storage[left][63]);
}

#[test]
fn extent_only_changes_use_the_existing_q32_scheduler_without_an_extent_ramp() {
    let layout = executable_layout("7.1.4");
    let mut initial = dynamic_point(0.25, 0.5, 0.0);
    initial.zones = Some(region_zones(
        RegionHorizontalState::ScreenOnly,
        RegionTopBottomState::Exclude,
    ));
    initial.extent = Some([0.5; 3]);
    let topology = SpatialTopologySnapshot {
        dynamic_records: vec![SpatialBindingRecord {
            descriptor: initial,
            scalar: 1.0,
            active: true,
        }],
        ..SpatialTopologySnapshot::default()
    };
    let input = vec![1.0; 64];
    let mut bridge = JocSpatialBridge::new();
    let mut storage = vec![vec![0.0; input.len()]; layout.active_channel_count()];
    let mut outputs = storage
        .iter_mut()
        .map(Vec::as_mut_slice)
        .collect::<Vec<_>>();
    bridge
        .render_coordinates(
            &[input.as_slice()],
            Some(&topology),
            None,
            &layout,
            0,
            48_000,
            &mut outputs,
        )
        .expect("initial extent target");
    let old_target = outputs
        .iter()
        .map(|channel| channel[63])
        .collect::<Vec<_>>();

    let update = SpatialCoordinateUpdate {
        ordinal: 0,
        descriptor: Some(SpatialDescriptorPatch {
            extent: Some(Some([0.0; 3])),
            ..SpatialDescriptorPatch::default()
        }),
        scalar: None,
        active: None,
    };
    bridge
        .render_coordinates(
            &[input.as_slice()],
            None,
            Some(std::slice::from_ref(&update)),
            &layout,
            32,
            48_000,
            &mut outputs,
        )
        .expect("extent-only target event");
    let point = layout
        .project(&SpatialDescriptor {
            source_class: SpatialSourceClass::DynamicPoint,
            identity: "point".to_owned(),
            coordinates: vec![0.25, 0.5, 0.0],
            spread: None,
            paired: None,
            pair_span_q15: None,
            raw3: None,
            extent: Some([0.0; 3]),
            zones: Some(region_zones(
                RegionHorizontalState::ScreenOnly,
                RegionTopBottomState::Exclude,
            )),
            channel_lock: false,
        })
        .expect("zero extent point target");
    let changed_channel = old_target
        .iter()
        .zip(&point)
        .position(|(old, target)| (old - target).abs() >= 1.0e-4)
        .expect("extent target must differ from point target");
    assert!((outputs[changed_channel][0] - old_target[changed_channel]).abs() < 1.0e-12);
    assert!((outputs[changed_channel][63] - point[changed_channel]).abs() < 1.0e-12);
}

#[test]
fn simultaneous_position_and_region_updates_form_one_projection_snapshot() {
    let layout = executable_layout("7.1.4");
    let mut initial = dynamic_point(0.5, 0.0, 0.0);
    initial.extent = Some([0.1; 3]);
    let topology = SpatialTopologySnapshot {
        dynamic_records: vec![SpatialBindingRecord {
            descriptor: initial,
            scalar: 1.0,
            active: true,
        }],
        ..SpatialTopologySnapshot::default()
    };
    let input = vec![1.0; 1];
    let mut bridge = JocSpatialBridge::new();
    let mut storage = vec![vec![0.0; 1]; layout.active_channel_count()];
    let mut outputs = storage
        .iter_mut()
        .map(Vec::as_mut_slice)
        .collect::<Vec<_>>();
    bridge
        .render_coordinates(
            &[input.as_slice()],
            Some(&topology),
            None,
            &layout,
            0,
            48_000,
            &mut outputs,
        )
        .expect("initial snapshot");

    let update = SpatialCoordinateUpdate {
        ordinal: 0,
        descriptor: Some(SpatialDescriptorPatch {
            coordinates: Some(vec![0.25, 0.5, 0.0]),
            zones: Some(Some(region_zones(
                RegionHorizontalState::ScreenOnly,
                RegionTopBottomState::Exclude,
            ))),
            extent: Some(Some([0.25; 3])),
            ..SpatialDescriptorPatch::default()
        }),
        scalar: None,
        active: None,
    };
    bridge
        .render_coordinates(
            &[input.as_slice()],
            None,
            Some(std::slice::from_ref(&update)),
            &layout,
            0,
            48_000,
            &mut outputs,
        )
        .expect("atomic position and region snapshot");
    drop(outputs);

    let mut expected = dynamic_point(0.25, 0.5, 0.0);
    expected.zones = Some(region_zones(
        RegionHorizontalState::ScreenOnly,
        RegionTopBottomState::Exclude,
    ));
    expected.extent = Some([0.25; 3]);
    let expected = layout.project(&expected).expect("atomic expected target");
    for (channel, value) in storage.iter().enumerate() {
        assert!(
            (value[0] - expected[channel]).abs() < 2.0e-12,
            "channel {channel}"
        );
    }
}

#[test]
fn red_channel_lock_region_uses_the_selected_region_topology() {
    let layout = executable_layout("7.1.4");
    let mut descriptor = dynamic_point(0.5, 0.0, 0.0);
    descriptor.zones = Some(region_zones(
        RegionHorizontalState::ScreenOnly,
        RegionTopBottomState::Include,
    ));
    descriptor.channel_lock = true;

    let target = layout
        .project(&descriptor)
        .expect("admitted ChannelLock × Region composition");

    assert_eq!(target[active_index(&layout, "FC")], 1.0);
    assert!(
        target
            .iter()
            .enumerate()
            .all(|(index, value)| { index == active_index(&layout, "FC") || *value == 0.0 })
    );
}

#[test]
fn red_channel_lock_extent_uses_channel_lock_precedence() {
    let layout = executable_layout("5.1");
    let mut descriptor = dynamic_point(0.5, 0.0, 0.0);
    descriptor.extent = Some([0.75; 3]);
    descriptor.channel_lock = true;

    let target = layout
        .project(&descriptor)
        .expect("admitted ChannelLock × Extent composition");

    let mut standalone = descriptor.clone();
    standalone.extent = None;
    assert_eq!(
        target,
        layout.project(&standalone).expect("standalone lock")
    );
}

#[test]
fn red_channel_lock_region_extent_uses_the_unified_precedence_graph() {
    let layout = executable_layout("7.1.4");
    let mut descriptor = dynamic_point(0.5, 0.0, 0.0);
    descriptor.zones = Some(region_zones(
        RegionHorizontalState::ScreenOnly,
        RegionTopBottomState::Include,
    ));
    descriptor.extent = Some([0.75; 3]);
    descriptor.channel_lock = true;

    let target = layout
        .project(&descriptor)
        .expect("admitted ChannelLock × Region × Extent composition");

    let mut region_only = descriptor.clone();
    region_only.extent = None;
    assert_eq!(
        target,
        layout
            .project(&region_only)
            .expect("ChannelLock × Region reduction")
    );
}

#[test]
fn channel_lock_region_does_not_lock_an_excluded_canonical_dominant_output() {
    let layout = executable_layout("7.1.4");
    let mut descriptor = dynamic_point(0.0, 0.5, 0.0);
    descriptor.zones = Some(region_zones(
        RegionHorizontalState::ScreenOnly,
        RegionTopBottomState::Include,
    ));
    descriptor.channel_lock = true;

    let outcome = layout
        .project_with_outcome(&descriptor)
        .expect("Region-first ChannelLock outcome");

    assert_eq!(outcome.locked_output, None);
    assert_eq!(outcome.target[active_index(&layout, "Ls")], 0.0);
    assert!(outcome.target[active_index(&layout, "FL")] > 0.0);
}

#[test]
fn channel_lock_region_keeps_the_standalone_strict_distance_gate() {
    let layout = executable_layout("7.1.4");
    let mut inside = dynamic_point(0.5, 0.0, 0.0);
    inside.zones = Some(region_zones(
        RegionHorizontalState::CentreAndBack,
        RegionTopBottomState::Include,
    ));
    inside.coordinates[0] = 0.6999999999999999;
    inside.channel_lock = true;
    let inside_outcome = layout
        .project_with_outcome(&inside)
        .expect("inside constrained gate");
    assert_eq!(
        inside_outcome.locked_output,
        Some(active_index(&layout, "FC"))
    );

    let mut above = inside.clone();
    above.coordinates[0] = 0.7000000000000001;
    let above_outcome = layout
        .project_with_outcome(&above)
        .expect("above constrained gate");
    assert_eq!(above_outcome.locked_output, None);
}

#[test]
fn channel_lock_extent_is_invariant_across_admitted_extent_values() {
    let layout = executable_layout("7.1.4");
    let mut descriptor = dynamic_point(0.5, 0.0, 0.0);
    descriptor.channel_lock = true;
    let mut standalone = descriptor.clone();
    standalone.extent = None;
    let expected = layout
        .project(&standalone)
        .expect("standalone ChannelLock target");

    for extent in [[0.1; 3], [0.5; 3], [0.75; 3]] {
        descriptor.extent = Some(extent);
        assert_eq!(
            layout.project(&descriptor).expect("active lock target"),
            expected,
            "extent {extent:?} must be latent while ChannelLock is active"
        );
    }
}

#[test]
fn seven_composition_reductions_match_the_existing_operator_branches() {
    let layout = executable_layout("7.1.4");

    let mut point = dynamic_point(0.37, 0.61, 0.23);
    let ordinary_point = layout.project(&point).expect("ordinary Point");

    let mut zero_extent = point.clone();
    zero_extent.extent = Some([0.0; 3]);
    assert_eq!(layout.project(&zero_extent).unwrap(), ordinary_point);

    let mut extent = point.clone();
    extent.extent = Some([0.5; 3]);
    let extent_target = layout.project(&extent).expect("standalone Extent");

    let mut locked = point.clone();
    locked.coordinates = vec![0.5, 0.0, 0.0];
    locked.channel_lock = true;
    let lock_target = layout.project(&locked).expect("standalone ChannelLock");
    let mut lock_with_extent = locked.clone();
    lock_with_extent.extent = Some([0.75; 3]);
    assert_eq!(layout.project(&lock_with_extent).unwrap(), lock_target);

    let mut region = point.clone();
    region.zones = Some(region_zones(
        RegionHorizontalState::ScreenOnly,
        RegionTopBottomState::Include,
    ));
    let region_target = layout.project(&region).expect("standalone Region");
    let mut region_zero = region.clone();
    region_zero.extent = Some([0.0; 3]);
    assert_eq!(layout.project(&region_zero).unwrap(), region_target);

    let mut region_extent = region.clone();
    region_extent.extent = Some([0.5; 3]);
    let region_extent_target = layout
        .project(&region_extent)
        .expect("existing Region × Extent");
    assert_unit_l2(&region_extent_target);

    let mut region_lock = region.clone();
    region_lock.coordinates = vec![0.5, 0.0, 0.0];
    region_lock.channel_lock = true;
    let region_lock_target = layout
        .project(&region_lock)
        .expect("existing Region × ChannelLock");
    let mut triple = region_lock.clone();
    triple.extent = Some([0.5; 3]);
    assert_eq!(layout.project(&triple).unwrap(), region_lock_target);

    point.extent = None;
    point.channel_lock = false;
    assert_eq!(layout.project(&point).unwrap(), ordinary_point);
    assert_ne!(extent_target, ordinary_point);
}

#[test]
fn channel_lock_composition_covers_the_admitted_public_layouts() {
    for name in [
        "5.1", "5.1.2", "5.1.4", "7.1", "7.1.2", "7.1.4", "7.1.6", "9.1", "9.1.2", "9.1.4",
        "9.1.6", "22.2",
    ] {
        let layout = executable_layout(name);
        let mut lock = dynamic_point(0.5, 0.0, 0.0);
        lock.channel_lock = true;
        let standalone = layout.project(&lock).expect("standalone ChannelLock");
        let mut lock_extent = lock.clone();
        lock_extent.extent = Some([0.75; 3]);
        assert_eq!(layout.project(&lock_extent).unwrap(), standalone, "{name}");

        lock.zones = Some(region_zones(
            RegionHorizontalState::ScreenOnly,
            RegionTopBottomState::Include,
        ));
        let region_lock = layout.project(&lock).expect("ChannelLock × Region");
        lock_extent.zones = lock.zones;
        assert_eq!(
            layout.project(&lock_extent).unwrap(),
            region_lock,
            "ChannelLock × Region × Extent {name}"
        );
    }
}

#[test]
fn channel_lock_state_restores_inherited_extent_after_release() {
    let layout = executable_layout("7.1.4");
    let mut initial = dynamic_point(0.5, 0.0, 0.0);
    initial.extent = Some([0.75; 3]);
    let topology = single_record_topology(initial.clone());
    let mut bridge = JocSpatialBridge::new();

    let extent_target = layout.project(&initial).expect("initial Extent target");
    let initial_output = render_block(&mut bridge, &layout, Some(&topology), None, 0, 1);
    assert_eq!(
        initial_output
            .iter()
            .map(|channel| channel[0])
            .collect::<Vec<_>>(),
        extent_target
    );

    let lock_on = SpatialCoordinateUpdate {
        ordinal: 0,
        descriptor: Some(SpatialDescriptorPatch {
            channel_lock: Some(Some(true)),
            ..SpatialDescriptorPatch::default()
        }),
        scalar: None,
        active: None,
    };
    let lock_output = render_block(
        &mut bridge,
        &layout,
        None,
        Some(std::slice::from_ref(&lock_on)),
        0,
        1,
    );
    assert_eq!(
        lock_output[active_index(&layout, "FC")][0],
        1.0,
        "ChannelLock owns the active target"
    );
    assert_eq!(
        bridge.binding_state().snapshot().unwrap().records[0]
            .descriptor
            .extent,
        Some([0.75; 3])
    );

    let lock_off = SpatialCoordinateUpdate {
        ordinal: 0,
        descriptor: Some(SpatialDescriptorPatch {
            channel_lock: Some(Some(false)),
            ..SpatialDescriptorPatch::default()
        }),
        scalar: None,
        active: None,
    };
    let restored_output = render_block(
        &mut bridge,
        &layout,
        None,
        Some(std::slice::from_ref(&lock_off)),
        0,
        1,
    );
    assert_eq!(
        restored_output
            .iter()
            .map(|channel| channel[0])
            .collect::<Vec<_>>(),
        extent_target
    );
}

#[test]
fn extent_updates_while_channel_lock_is_active_are_retained_but_bypassed() {
    let layout = executable_layout("7.1.4");
    let mut initial = dynamic_point(0.5, 0.0, 0.0);
    initial.extent = Some([0.1; 3]);
    initial.channel_lock = true;
    let topology = single_record_topology(initial.clone());
    let mut bridge = JocSpatialBridge::new();

    let lock_target = layout.project(&initial).expect("initial lock target");
    let initial_output = render_block(&mut bridge, &layout, Some(&topology), None, 0, 1);
    assert_eq!(
        initial_output
            .iter()
            .map(|channel| channel[0])
            .collect::<Vec<_>>(),
        lock_target
    );

    let extent_update = SpatialCoordinateUpdate {
        ordinal: 0,
        descriptor: Some(SpatialDescriptorPatch {
            extent: Some(Some([0.75; 3])),
            ..SpatialDescriptorPatch::default()
        }),
        scalar: None,
        active: None,
    };
    let active_output = render_block(
        &mut bridge,
        &layout,
        None,
        Some(std::slice::from_ref(&extent_update)),
        0,
        1,
    );
    assert_eq!(
        active_output
            .iter()
            .map(|channel| channel[0])
            .collect::<Vec<_>>(),
        lock_target
    );
    assert_eq!(
        bridge.binding_state().snapshot().unwrap().records[0]
            .descriptor
            .extent,
        Some([0.75; 3])
    );

    let lock_off = SpatialCoordinateUpdate {
        ordinal: 0,
        descriptor: Some(SpatialDescriptorPatch {
            channel_lock: Some(Some(false)),
            ..SpatialDescriptorPatch::default()
        }),
        scalar: None,
        active: None,
    };
    let restored_output = render_block(
        &mut bridge,
        &layout,
        None,
        Some(std::slice::from_ref(&lock_off)),
        0,
        1,
    );
    let mut expected = initial;
    expected.channel_lock = false;
    expected.extent = Some([0.75; 3]);
    assert_eq!(
        restored_output
            .iter()
            .map(|channel| channel[0])
            .collect::<Vec<_>>(),
        layout.project(&expected).expect("updated Extent target")
    );
}

#[test]
fn region_changes_while_channel_lock_is_active_use_one_new_topology_snapshot() {
    let layout = executable_layout("7.1.4");
    let mut initial = dynamic_point(0.5, 0.0, 0.0);
    initial.zones = Some(region_zones(
        RegionHorizontalState::ScreenOnly,
        RegionTopBottomState::Include,
    ));
    initial.extent = Some([0.75; 3]);
    initial.channel_lock = true;
    let topology = single_record_topology(initial.clone());
    let mut bridge = JocSpatialBridge::new();
    let initial_output = render_block(&mut bridge, &layout, Some(&topology), None, 0, 1);
    assert_eq!(initial_output[active_index(&layout, "FC")][0], 1.0);

    let update = SpatialCoordinateUpdate {
        ordinal: 0,
        descriptor: Some(SpatialDescriptorPatch {
            coordinates: Some(vec![0.0, 0.5, 0.0]),
            zones: Some(Some(region_zones(
                RegionHorizontalState::SurroundOnly,
                RegionTopBottomState::Include,
            ))),
            ..SpatialDescriptorPatch::default()
        }),
        scalar: None,
        active: None,
    };
    let changed_output = render_block(
        &mut bridge,
        &layout,
        None,
        Some(std::slice::from_ref(&update)),
        0,
        1,
    );
    assert_eq!(changed_output[active_index(&layout, "Ls")][0], 1.0);
    assert_eq!(changed_output[active_index(&layout, "FC")][0], 0.0);
}

#[test]
fn simultaneous_composition_updates_select_one_latest_semantic_snapshot() {
    let layout = executable_layout("7.1.4");
    let mut initial = dynamic_point(0.5, 0.0, 0.0);
    initial.extent = Some([0.1; 3]);
    initial.zones = Some(region_zones(
        RegionHorizontalState::ScreenOnly,
        RegionTopBottomState::Include,
    ));
    let topology = single_record_topology(initial);
    let mut bridge = JocSpatialBridge::new();
    render_block(&mut bridge, &layout, Some(&topology), None, 0, 1);

    let update = SpatialCoordinateUpdate {
        ordinal: 0,
        descriptor: Some(SpatialDescriptorPatch {
            coordinates: Some(vec![0.0, 0.5, 0.0]),
            zones: Some(Some(region_zones(
                RegionHorizontalState::SurroundOnly,
                RegionTopBottomState::Include,
            ))),
            extent: Some(Some([0.75; 3])),
            channel_lock: Some(Some(true)),
            ..SpatialDescriptorPatch::default()
        }),
        scalar: None,
        active: None,
    };
    let output = render_block(
        &mut bridge,
        &layout,
        None,
        Some(std::slice::from_ref(&update)),
        0,
        1,
    );
    let mut expected = dynamic_point(0.0, 0.5, 0.0);
    expected.zones = Some(region_zones(
        RegionHorizontalState::SurroundOnly,
        RegionTopBottomState::Include,
    ));
    expected.extent = Some([0.75; 3]);
    expected.channel_lock = true;
    assert_eq!(
        output.iter().map(|channel| channel[0]).collect::<Vec<_>>(),
        layout.project(&expected).expect("coherent latest snapshot")
    );
}

#[test]
fn effective_channel_lock_position_is_local_when_extent_resumes() {
    let layout = executable_layout("5.1");
    let mut initial = dynamic_point(0.1, 0.0, 0.0);
    initial.extent = Some([0.75; 3]);
    initial.channel_lock = true;
    let topology = single_record_topology(initial.clone());
    let mut bridge = JocSpatialBridge::new();
    let lock_output = render_block(&mut bridge, &layout, Some(&topology), None, 0, 1);
    assert_eq!(lock_output[active_index(&layout, "FL")][0], 1.0);
    assert_eq!(
        bridge.binding_state().snapshot().unwrap().records[0]
            .descriptor
            .coordinates,
        vec![0.1, 0.0, 0.0]
    );

    let lock_off = SpatialCoordinateUpdate {
        ordinal: 0,
        descriptor: Some(SpatialDescriptorPatch {
            channel_lock: Some(Some(false)),
            ..SpatialDescriptorPatch::default()
        }),
        scalar: None,
        active: None,
    };
    let extent_output = render_block(
        &mut bridge,
        &layout,
        None,
        Some(std::slice::from_ref(&lock_off)),
        0,
        1,
    );
    let mut expected = initial;
    expected.channel_lock = false;
    assert_eq!(
        extent_output
            .iter()
            .map(|channel| channel[0])
            .collect::<Vec<_>>(),
        layout
            .project(&expected)
            .expect("authored-center Extent target")
    );
}

#[test]
fn channel_lock_extent_transitions_use_q32_without_a_composition_ramp() {
    let layout = executable_layout("5.1");
    let mut initial = dynamic_point(0.5, 0.0, 0.0);
    initial.extent = Some([0.75; 3]);
    let topology = single_record_topology(initial.clone());
    let mut bridge = JocSpatialBridge::new();
    let initial_output = render_block(&mut bridge, &layout, Some(&topology), None, 0, 1);
    let old_fc = initial_output[active_index(&layout, "FC")][0];

    let lock_on = SpatialCoordinateUpdate {
        ordinal: 0,
        descriptor: Some(SpatialDescriptorPatch {
            channel_lock: Some(Some(true)),
            ..SpatialDescriptorPatch::default()
        }),
        scalar: None,
        active: None,
    };
    let locked_output = render_block(
        &mut bridge,
        &layout,
        None,
        Some(std::slice::from_ref(&lock_on)),
        64,
        64,
    );
    assert!((locked_output[active_index(&layout, "FC")][0] - old_fc).abs() < 1.0e-12);
    let expected_locked_last = old_fc + (1.0 - old_fc) * 63.0 / 64.0;
    assert!(
        (locked_output[active_index(&layout, "FC")][63] - expected_locked_last).abs() < 1.0e-12
    );

    let lock_off = SpatialCoordinateUpdate {
        ordinal: 0,
        descriptor: Some(SpatialDescriptorPatch {
            channel_lock: Some(Some(false)),
            ..SpatialDescriptorPatch::default()
        }),
        scalar: None,
        active: None,
    };
    let restored_output = render_block(
        &mut bridge,
        &layout,
        None,
        Some(std::slice::from_ref(&lock_off)),
        64,
        64,
    );
    assert!((restored_output[active_index(&layout, "FC")][0] - 1.0).abs() < 1.0e-12);
    let expected_last = 1.0 + (old_fc - 1.0) * 63.0 / 64.0;
    assert!((restored_output[active_index(&layout, "FC")][63] - expected_last).abs() < 1.0e-12);
}

#[test]
fn region_extent_preserves_default_and_zero_extent_identities() {
    let layout = executable_layout("7.1.4");
    let mut extent = dynamic_point(0.37, 0.61, 0.23);
    extent.extent = Some([0.25; 3]);
    let mut default_region = extent.clone();
    default_region.zones = Some(region_zones(
        RegionHorizontalState::NoConstraints,
        RegionTopBottomState::Include,
    ));
    assert_eq!(
        layout.project(&default_region),
        layout.project(&extent),
        "default Region must preserve standalone Extent"
    );

    let mut region = dynamic_point(0.25, 0.5, 0.0);
    region.zones = Some(region_zones(
        RegionHorizontalState::ScreenOnly,
        RegionTopBottomState::Exclude,
    ));
    let mut zero = region.clone();
    zero.extent = Some([0.0; 3]);
    assert_eq!(layout.project(&zero), layout.project(&region));
}

#[test]
fn region_extent_uses_one_constrained_topology_for_point_and_diffuse_branches() {
    let layout = executable_layout("7.1.4");
    let mut constrained = dynamic_point(0.25, 0.5, 0.0);
    constrained.zones = Some(region_zones(
        RegionHorizontalState::ScreenOnly,
        RegionTopBottomState::Exclude,
    ));
    constrained.extent = Some([0.25; 3]);
    let target = layout.project(&constrained).expect("constrained target");
    assert_unit_l2(&target);
    for identity in ["Ls", "Rs", "Lb", "Rb", "TFL", "TFR", "TBL", "TBR"] {
        assert_eq!(target[active_index(&layout, identity)], 0.0, "{identity}");
    }

    let mut full_extent = dynamic_point(0.25, 0.5, 0.0);
    full_extent.extent = Some([0.25; 3]);
    let mut post_mask = layout.project(&full_extent).expect("full Extent target");
    for (index, value) in post_mask.iter_mut().enumerate() {
        let identity = layout
            .channels()
            .iter()
            .filter(|channel| channel.enabled && !channel.lfe)
            .nth(index)
            .expect("active channel");
        if !["FL", "FC", "FR"].contains(&identity.identity.as_str()) {
            *value = 0.0;
        }
    }
    let norm = post_mask
        .iter()
        .map(|value| value * value)
        .sum::<f64>()
        .sqrt();
    for value in &mut post_mask {
        *value /= norm;
    }
    assert_ne!(target, post_mask);
}

#[test]
fn region_extent_retains_authored_outside_center_and_compensates_selected_support() {
    let layout = executable_layout("7.1.4");
    let mut outside = dynamic_point(0.25, QMAX_CLEAN, 0.0);
    outside.zones = Some(region_zones(
        RegionHorizontalState::ScreenOnly,
        RegionTopBottomState::Exclude,
    ));
    outside.extent = Some([0.25; 3]);
    let target = layout.project(&outside).expect("outside-center target");
    assert_unit_l2(&target);
    for identity in ["Ls", "Rs", "Lb", "Rb", "TFL", "TFR", "TBL", "TBR"] {
        assert_eq!(target[active_index(&layout, identity)], 0.0, "{identity}");
    }

    let mut clamped = outside.clone();
    clamped.coordinates[1] = 0.0;
    let clamped_target = layout.project(&clamped).expect("clamped-center target");
    assert_ne!(target, clamped_target);

    for extent in [0.05, 0.25, 0.75] {
        let mut boundary = dynamic_point(0.25, 0.0, 0.0);
        boundary.zones = outside.zones;
        boundary.extent = Some([extent; 3]);
        let boundary_target = layout.project(&boundary).expect("boundary target");
        assert_unit_l2(&boundary_target);
        for identity in ["Ls", "Rs", "Lb", "Rb", "TFL", "TFR", "TBL", "TBR"] {
            assert_eq!(
                boundary_target[active_index(&layout, identity)],
                0.0,
                "{identity} at extent {extent}"
            );
        }
    }
}

#[test]
fn region_extent_composition_covers_each_public_layout() {
    for name in [
        "5.1", "5.1.2", "5.1.4", "7.1", "7.1.2", "7.1.4", "7.1.6", "9.1", "9.1.2", "9.1.4",
        "9.1.6", "22.2",
    ] {
        let layout = executable_layout(name);
        let mut descriptor = dynamic_point(0.37, 0.61, 0.0);
        descriptor.zones = Some(region_zones(
            RegionHorizontalState::ScreenOnly,
            RegionTopBottomState::Exclude,
        ));
        descriptor.extent = Some([0.25; 3]);
        let target = layout
            .project(&descriptor)
            .expect("admitted layout composition");
        assert_eq!(target.len(), layout.active_channel_count(), "{name}");
        assert_unit_l2(&target);
    }
}

#[test]
fn twenty_two_two_admits_point_region_extent_spread_pair_and_channel_lock() {
    let layout = executable_layout("22.2");
    let point = dynamic_point(0.5, 0.5, 0.0);
    assert_unit_l2(&layout.project(&point).expect("22.2 Point"));

    let mut region = point.clone();
    region.zones = Some(region_zones(
        RegionHorizontalState::ScreenOnly,
        RegionTopBottomState::Exclude,
    ));
    region.extent = Some([0.25; 3]);
    assert_unit_l2(&layout.project(&region).expect("22.2 Region × Extent"));

    let mut spread = point.clone();
    spread.spread = Some(SpatialSpreadProfile {
        samples: vec![
            SpatialSpreadSample {
                position: vec![0.35, 0.5, 0.0],
                weight: 0.5,
            },
            SpatialSpreadSample {
                position: vec![0.65, 0.5, 0.0],
                weight: 0.5,
            },
        ],
    });
    assert_unit_l2(&layout.project(&spread).expect("22.2 Spread"));

    let mut pair = point.clone();
    pair.pair_span_q15 = Some(8_192);
    assert_unit_l2(&layout.project(&pair).expect("22.2 Pair"));

    let mut locked = point;
    locked.channel_lock = true;
    let locked_target = layout.project(&locked).expect("22.2 ChannelLock");
    assert_unit_l2(&locked_target);
    assert_eq!(locked_target.len(), 22);
}

#[test]
fn explicit_base_semantic_channels_use_equivalent_output_identities_without_xyz() {
    for (layout_name, source_identity, output_identity) in [
        ("22.2", "Ls", "SiL"),
        ("22.2", "Rs", "SiR"),
        ("22.2", "Lb", "BL"),
        ("22.2", "Rb", "BR"),
        ("22.2", "TFL", "TpFL"),
        ("22.2", "TFR", "TpFR"),
        ("22.2", "TBL", "TpBL"),
        ("22.2", "TBR", "TpBR"),
        ("7.1.4", "SiL", "Ls"),
        ("7.1.4", "SiR", "Rs"),
    ] {
        let layout = executable_layout(layout_name);
        let projected = layout
            .project(&descriptor(
                SpatialSourceClass::ExplicitChannel,
                source_identity,
                Vec::new(),
            ))
            .unwrap_or_else(|error| {
                panic!(
                    "{layout_name} {source_identity} must remain a discrete semantic source: {error}"
                )
            });
        assert_eq!(projected[active_index(&layout, output_identity)], 1.0);
        assert_eq!(projected.iter().filter(|value| **value != 0.0).count(), 1);
    }
}

#[test]
fn red_admitted_region_extent_cases_are_not_blanket_rejected() {
    let layout = executable_layout("7.1.4");
    let cases = [
        (0.5, 0.5, 0.0, 0.05),
        (0.25, QMAX_CLEAN, 0.0, 0.25),
        (0.25, 0.0, 0.0, 0.75),
    ];
    for (x, y, z, extent) in cases {
        let mut descriptor = dynamic_point(x, y, z);
        descriptor.zones = Some(region_zones(
            RegionHorizontalState::ScreenOnly,
            RegionTopBottomState::Exclude,
        ));
        descriptor.extent = Some([extent; 3]);
        let target = layout
            .project(&descriptor)
            .expect("admitted Region × Extent case");
        assert_unit_l2(&target);
    }
}

#[test]
fn red_nonzero_uniform_extent_generates_a_diffuse_target() {
    let layout = executable_layout("7.1.4");
    let mut extent = dynamic_point(0.5, 0.5, 0.0);
    extent.extent = Some([5_285.0 / 32_768.0; 3]);

    let target = layout
        .project(&extent)
        .expect("ordinary default-region extent target");
    assert_unit_l2(&target);
    assert!(target.iter().filter(|value| **value > 1.0e-9).count() > 1);
}

#[test]
fn red_equivalent_single_axis_extents_have_the_same_target() {
    let layout = executable_layout("7.1.4");
    let mut x_only = dynamic_point(0.5, 0.5, 0.0);
    x_only.extent = Some([15_855.0 / 32_768.0, 0.0, 0.0]);
    let mut y_only = x_only.clone();
    y_only.extent = Some([0.0, 15_855.0 / 32_768.0, 0.0]);
    let mut z_only = x_only.clone();
    z_only.extent = Some([0.0, 0.0, 15_855.0 / 32_768.0]);

    let x_target = layout.project(&x_only).expect("x-only extent target");
    let y_target = layout.project(&y_only).expect("y-only extent target");
    let z_target = layout.project(&z_only).expect("z-only extent target");
    assert_eq!(x_target, y_target);
    assert_eq!(x_target, z_target);
}

#[test]
fn red_extent_boundary_uses_bounded_compensation() {
    let layout = executable_layout("7.1.4");
    let mut boundary = dynamic_point(0.0, 0.5, 0.0);
    boundary.extent = Some([8_192.0 / 32_768.0; 3]);

    let target = layout
        .project(&boundary)
        .expect("ordinary boundary extent target");
    assert_unit_l2(&target);
    assert!(target.iter().any(|value| *value > 0.0));
}

#[test]
fn extent_zero_is_an_exact_point_identity_across_admitted_layouts() {
    for name in [
        "5.1", "5.1.2", "5.1.4", "7.1", "7.1.2", "7.1.4", "7.1.6", "9.1", "9.1.2", "9.1.4", "9.1.6",
    ] {
        let layout = executable_layout(name);
        for position in [
            (0.5, 0.5, 0.0),
            (0.0, 0.0, 0.0),
            (1.0, 1.0, 0.0),
            (0.5, 0.5, -0.5),
            (0.5, 0.5, 0.5),
        ] {
            let point = dynamic_point(position.0, position.1, position.2);
            let mut zero = point.clone();
            zero.extent = Some([0.0; 3]);
            assert_eq!(
                layout.project(&zero),
                layout.project(&point),
                "{name} {position:?}"
            );
        }
    }
}

#[test]
fn maximum_extent_remains_layout_and_center_dependent() {
    let layout = executable_layout("7.1.4");
    let mut center = dynamic_point(0.5, 0.5, 0.0);
    center.extent = Some([32_767.0 / 32_768.0; 3]);
    let mut side = center.clone();
    side.coordinates[0] = 0.25;
    let center_target = layout.project(&center).expect("maximum center target");
    let side_target = layout.project(&side).expect("maximum side target");
    assert_unit_l2(&center_target);
    assert_unit_l2(&side_target);
    assert_ne!(center_target, side_target);
    assert!(center_target.iter().any(|value| *value > 0.0));
    assert!(side_target.iter().any(|value| *value > 0.0));
}

#[test]
fn extent_field_excludes_lfe_and_keeps_public_channel_count() {
    for name in ["5.1", "5.1.4", "7.1.4", "9.1.6"] {
        let layout = executable_layout(name);
        let mut descriptor = dynamic_point(0.5, 0.5, 0.0);
        descriptor.extent = Some([16_384.0 / 32_768.0; 3]);
        let target = layout.project(&descriptor).expect("extent target");
        assert_eq!(target.len(), layout.active_channel_count(), "{name}");
        assert_unit_l2(&target);
    }
}

#[test]
fn fixed_and_named_routes_use_identity_registry_and_missing_routes_fail() {
    let layout = layout()
        .with_route_vectors(vec![
            SpatialRouteVector {
                identity: "fixed".to_owned(),
                vector: vec![1.0, 0.0],
            },
            SpatialRouteVector {
                identity: "named".to_owned(),
                vector: vec![0.0, 1.0],
            },
        ])
        .expect("route registry");

    let fixed = descriptor(SpatialSourceClass::FixedLayout, "fixed", Vec::new());
    let named = descriptor(SpatialSourceClass::NamedLayout, "named", Vec::new());
    assert_eq!(layout.project(&fixed).unwrap(), vec![1.0, 0.0]);
    assert_eq!(layout.project(&named).unwrap(), vec![0.0, 1.0]);

    let missing = descriptor(SpatialSourceClass::FixedLayout, "missing", Vec::new());
    assert_eq!(
        layout.project(&missing),
        Err(openjoc_scene::SpatialProjectionError::MissingRoute(
            "missing".to_owned()
        ))
    );
}

fn discrete_route_layout() -> SpatialLayout {
    let channels = ["FL", "FR", "FC", "Ls", "Rs"]
        .into_iter()
        .map(|identity| SpatialLayoutChannel {
            identity: identity.to_owned(),
            enabled: true,
            lfe: false,
        })
        .collect::<Vec<_>>();
    let nodes = (0..channels.len())
        .map(|index| {
            let mut vector = vec![0.0; channels.len()];
            vector[index] = 1.0;
            SpatialLayoutNode {
                knot_indices: vec![index],
                vector,
            }
        })
        .collect::<Vec<_>>();
    SpatialLayout::new(
        channels,
        vec![(0..5).map(|index| index as f64).collect()],
        nodes,
        Vec::new(),
    )
    .expect("valid direct-route layout")
}

#[test]
fn fixed_neutral_key_preserves_supplied_non_unit_route_and_ignores_position() {
    let layout = discrete_route_layout()
        .with_route_vectors(vec![SpatialRouteVector {
            identity: "fixed/6/5".to_owned(),
            vector: vec![0.25, 0.5, 0.0, 0.0, 0.0],
        }])
        .expect("route registry");
    let mut fixed = descriptor(
        SpatialSourceClass::FixedLayout,
        "fixed/6/5",
        vec![0.0, 0.0, 0.0],
    );
    let first = layout.project(&fixed).expect("fixed route");
    fixed.coordinates = vec![1.0, 1.0, 1.0];
    let second = layout
        .project(&fixed)
        .expect("fixed route remains discrete");
    assert_eq!(first, vec![0.25, 0.5, 0.0, 0.0, 0.0]);
    assert_eq!(second, first);
}

#[test]
fn named_direct_route_is_explicit_and_missing_route_does_not_use_point_fallback() {
    let layout = discrete_route_layout()
        .with_route_vectors(vec![SpatialRouteVector {
            identity: "named/0".to_owned(),
            vector: vec![0.0, 0.0, 1.0, 0.0, 0.0],
        }])
        .expect("route registry");
    let named = descriptor(
        SpatialSourceClass::NamedLayout,
        "named/0",
        vec![0.0, 0.0, 0.0],
    );
    assert_eq!(
        layout.project(&named).expect("named direct route"),
        vec![0.0, 0.0, 1.0, 0.0, 0.0]
    );

    let missing = descriptor(
        SpatialSourceClass::NamedLayout,
        "named/1",
        vec![0.5, 0.5, 0.0],
    );
    assert!(layout.project(&missing).is_err());
}

#[test]
fn named_fallback_red_case_resolves_an_ordinary_pair_without_geometry() {
    let layout = executable_layout("5.1")
        .with_route_vectors(Vec::new())
        .expect("empty route registry");
    let named = SpatialDescriptor::named(NamedTargetId::new(4).unwrap(), vec![1.0, 1.0, 1.0]);

    let target = layout
        .project(&named)
        .expect("Named fallback should resolve the missing Lb route");

    assert_eq!(target.len(), 5);
    assert!((target[active_index(&layout, "FL")] - 0.75).abs() < 1.0e-12);
    assert!(
        target
            .iter()
            .enumerate()
            .all(|(index, value)| index == active_index(&layout, "FL") || *value == 0.0)
    );
}

#[test]
fn named_fallback_red_case_uses_guarded_l2_for_the_716_upper_triple() {
    let layout = executable_layout("7.1.6")
        .with_route_vectors(Vec::new())
        .expect("empty route registry");
    let named = SpatialDescriptor::named(NamedTargetId::new(8).unwrap(), vec![0.0; 3]);

    let target = layout
        .project(&named)
        .expect("Named upper fallback should resolve");
    let expected = [0.755416341480983, 0.534793795986174, 0.3786050010210978];
    for (identity, value) in ["Ltf", "Ltm", "Ltr"].into_iter().zip(expected) {
        assert!((target[active_index(&layout, identity)] - value).abs() < 1.0e-8);
    }
    assert_unit_l2(&target);
    assert_eq!(target.len(), 13);
}

#[test]
fn named_fallback_red_case_recomputes_after_layout_epoch_and_keeps_lfe_out() {
    let first = executable_layout("9.1.4")
        .with_route_vectors(Vec::new())
        .expect("empty route registry");
    let second = executable_layout("9.1.6")
        .with_route_vectors(Vec::new())
        .expect("empty route registry");
    let named = SpatialDescriptor::named(NamedTargetId::new(8).unwrap(), vec![0.0; 3]);

    let first_target = first.project(&named).expect("first fallback target");
    let second_target = second.project(&named).expect("second fallback target");

    assert_ne!(first_target, second_target);
    assert_eq!(first_target.len(), 13);
    assert_eq!(second_target.len(), 15);
    assert!(first_target[active_index(&first, "Ltf")] > 0.0);
    assert!(second_target[active_index(&second, "Ltm")] > 0.0);
    assert_eq!(first.active_channel_count(), 13);
    assert_eq!(second.active_channel_count(), 15);
}

fn matrix_direct_ids(layout: &str) -> &'static [u8] {
    match layout {
        "2.0" => &[0, 1],
        "5.1" => &[0, 1, 2],
        "5.1.2" => &[0, 1, 2, 9, 15],
        "5.1.4" => &[0, 1, 2, 9, 10, 15],
        "7.1" => &[0, 1, 2, 4, 5],
        "7.1.2" => &[0, 1, 2, 4, 5, 9, 15],
        "7.1.4" => &[0, 1, 2, 4, 5, 9, 10, 15],
        "7.1.6" => &[0, 1, 2, 4, 5, 6, 7, 10, 11],
        "9.1" | "9.1.4" => &[0, 1, 2, 4, 5, 6, 7, 14, 15],
        "9.1.2" | "9.1.6" => &[0, 1, 2, 4, 5, 6, 7, 10, 11, 14, 15],
        "22.2" => &[0, 1, 2, 14, 15],
        _ => &[],
    }
}

#[test]
fn named_route_disposition_matrix_is_exhaustive_and_deterministic() {
    let mut direct_count = 0;
    let mut fallback_count = 0;
    let mut unsupported_count = 0;
    for layout_name in SPEAKER_LAYOUT_PRESET_NAMES {
        let base = executable_layout(layout_name);
        let direct_ids = matrix_direct_ids(layout_name);
        let routes = direct_ids
            .iter()
            .map(|id| {
                SpatialRouteVector::named(
                    NamedTargetId::new(*id).unwrap(),
                    vec![0.0; base.active_channel_count()],
                )
            })
            .collect();
        let layout = base
            .with_route_vectors(routes)
            .expect("matrix route registry");
        for id in 0..16 {
            let target = NamedTargetId::new(id).unwrap();
            let status = layout.named_route_status(target);
            if id == 3 {
                assert_eq!(
                    status,
                    SpatialRouteStatus::Unsupported,
                    "{layout_name}/named/{id}"
                );
                unsupported_count += 1;
            } else if direct_ids.contains(&id) {
                assert_eq!(
                    status,
                    SpatialRouteStatus::DirectReady,
                    "{layout_name}/named/{id}"
                );
                direct_count += 1;
            } else if id >= 4 {
                assert_eq!(
                    status,
                    SpatialRouteStatus::FallbackReady,
                    "{layout_name}/named/{id}"
                );
                fallback_count += 1;
            } else {
                assert_eq!(
                    status,
                    SpatialRouteStatus::FallbackWithheld,
                    "{layout_name}/named/{id}"
                );
            }
        }
    }
    assert_eq!(direct_count, 90);
    assert_eq!(fallback_count, 104);
    assert_eq!(unsupported_count, 13);
}

#[test]
fn named_fallback_q12_and_product_rules_are_exact() {
    assert_eq!(named_fallback_q12(0), 4096);
    assert_eq!(named_fallback_q12(1), 4339);
    assert_eq!(named_fallback_q12(-5), 3072);
    assert_eq!(named_fallback_q12(-128), 0);
    assert_eq!(named_fallback_gain(1), 1.059326171875);
    assert_eq!(named_fallback_product_q12(4868, 3072), 3651);
    assert_eq!(named_fallback_product_q12(217, 7715), 408);
}

#[test]
fn named_fallback_parameter_tuple_is_a_clean_adapter_input() {
    let mut codewords = [0_i16; 24];
    codewords[22] = 0;
    let parameters = NamedFallbackParameterTuple::from_codewords(codewords);
    let layout = executable_layout("5.1")
        .with_route_vectors(Vec::new())
        .expect("empty route registry")
        .with_named_fallback_parameters(parameters);
    let target = layout
        .project(&SpatialDescriptor::named(
            NamedTargetId::new(15).unwrap(),
            vec![1.0, 1.0, 1.0],
        ))
        .expect("tuple-driven single gain");
    assert_eq!(target[active_index(&layout, "FR")], 1.0);
}

#[test]
fn named_fallback_guarded_l2_handles_one_and_zero_survivors() {
    let one_survivor = executable_layout("5.1.2")
        .with_route_vectors(Vec::new())
        .expect("empty route registry");
    let one_target = one_survivor
        .project(&SpatialDescriptor::named(
            NamedTargetId::new(8).unwrap(),
            vec![0.0; 3],
        ))
        .expect("single active top candidate");
    assert_eq!(one_target[active_index(&one_survivor, "TFL")], 1.0);
    assert_unit_l2(&one_target);

    let zero_survivors = executable_layout("5.1")
        .with_route_vectors(Vec::new())
        .expect("empty route registry");
    assert!(matches!(
        zero_survivors.project(&SpatialDescriptor::named(
            NamedTargetId::new(8).unwrap(),
            vec![0.0; 3],
        )),
        Err(openjoc_scene::SpatialProjectionError::UnsupportedRoute {
            status: SpatialRouteStatus::Unsupported,
            ..
        })
    ));
}

#[test]
fn named_unsupported_lfe_cells_fail_closed_for_every_public_layout() {
    for layout_name in SPEAKER_LAYOUT_PRESET_NAMES {
        let layout = executable_layout(layout_name)
            .with_route_vectors(Vec::new())
            .expect("empty route registry");
        let result = layout.project(&SpatialDescriptor::named(
            NamedTargetId::new(3).unwrap(),
            vec![0.0; 3],
        ));
        assert!(
            matches!(
                result,
                Err(openjoc_scene::SpatialProjectionError::UnsupportedRoute {
                    status: SpatialRouteStatus::Unsupported,
                    ..
                })
            ),
            "{layout_name}/named/3"
        );
    }
}

#[test]
fn named_direct_rows_remain_unmodified_and_coordinates_are_ignored() {
    let target = NamedTargetId::new(0).unwrap();
    let row = vec![0.25, 0.5, 0.0, 0.0, 0.0];
    let layout = executable_layout("5.1")
        .with_route_vectors(vec![SpatialRouteVector::named(target, row.clone())])
        .expect("direct route registry");
    let mut named = SpatialDescriptor::named(target, vec![0.0, 0.0, 0.0]);
    assert_eq!(layout.project(&named).unwrap(), row);
    named.coordinates = vec![1.0, 1.0, 1.0];
    assert_eq!(layout.project(&named).unwrap(), row);
}

#[test]
fn neutral_fixed_and_named_domains_are_closed_and_status_is_explicit() {
    assert!(FixedRouteKey::new(4, 1).is_none());
    assert!(FixedRouteKey::new(6, 4).is_none());
    assert!(FixedRouteKey::new(6, 13).is_none());
    assert_eq!(FixedRouteKey::new(6, 5).unwrap().identity(), "fixed/6/5");
    assert_eq!(NamedTargetId::new(15).unwrap().canonical_route_slot(), 12);
    assert!(NamedTargetId::new(16).is_none());

    let layout = discrete_route_layout()
        .with_route_vectors(vec![SpatialRouteVector::named(
            NamedTargetId::new(0).unwrap(),
            vec![1.0, 0.0, 0.0, 0.0, 0.0],
        )])
        .expect("route registry");
    let direct = SpatialDescriptor::named(NamedTargetId::new(0).unwrap(), vec![0.0; 3]);
    let withheld = SpatialDescriptor::named(NamedTargetId::new(1).unwrap(), vec![0.0; 3]);
    assert_eq!(
        layout.route_status(&direct),
        SpatialRouteStatus::DirectReady
    );
    assert_eq!(
        layout.route_status(&withheld),
        SpatialRouteStatus::FallbackWithheld
    );
}

#[test]
fn named_direct_route_does_not_extrapolate_to_unadmitted_layout_shape() {
    let layout = high_channel_layout(15)
        .with_route_vectors(vec![SpatialRouteVector::named(
            NamedTargetId::new(0).unwrap(),
            vec![1.0; 15],
        )])
        .expect("route registry");
    let named = SpatialDescriptor::named(NamedTargetId::new(0).unwrap(), vec![0.0; 3]);
    assert_eq!(layout.route_status(&named), SpatialRouteStatus::Unsupported);
    assert!(layout.project(&named).is_err());
}

#[test]
fn discrete_dynamic_controls_fail_closed_without_point_or_route_composition() {
    let layout = discrete_route_layout()
        .with_route_vectors(vec![SpatialRouteVector::fixed(
            FixedRouteKey::new(6, 5).unwrap(),
            vec![1.0, 0.0, 0.0, 0.0, 0.0],
        )])
        .expect("route registry");
    let mut fixed = SpatialDescriptor::fixed(FixedRouteKey::new(6, 5).unwrap(), vec![0.0; 3]);
    fixed.extent = Some([1.0, 1.0, 1.0]);
    assert!(layout.project(&fixed).is_err());
}

#[test]
fn withheld_named_route_clears_outputs_and_cannot_reuse_a_prior_vector() {
    let layout = discrete_route_layout()
        .with_route_vectors(vec![SpatialRouteVector::named(
            NamedTargetId::new(0).unwrap(),
            vec![1.0, 0.0, 0.0, 0.0, 0.0],
        )])
        .expect("route registry");
    let topology = SpatialTopologySnapshot {
        dynamic_records: vec![SpatialBindingRecord {
            descriptor: SpatialDescriptor::named(NamedTargetId::new(0).unwrap(), vec![0.0; 3]),
            scalar: 1.0,
            active: true,
        }],
        ..SpatialTopologySnapshot::default()
    };
    let input = [1.0];
    let coordinates = vec![input.as_slice()];
    let mut bridge = JocSpatialBridge::new();
    let mut initial = vec![vec![0.0; 1]; 5];
    let mut initial_refs = initial
        .iter_mut()
        .map(Vec::as_mut_slice)
        .collect::<Vec<_>>();
    bridge
        .render_coordinates(
            &coordinates,
            Some(&topology),
            None,
            &layout,
            0,
            48_000,
            &mut initial_refs,
        )
        .expect("direct named route");
    drop(initial_refs);
    assert_eq!(initial[0], vec![1.0]);

    let invalid = SpatialCoordinateUpdate {
        ordinal: 0,
        descriptor: Some(SpatialDescriptorPatch {
            identity: Some("named/1".to_owned()),
            ..SpatialDescriptorPatch::default()
        }),
        scalar: None,
        active: None,
    };
    let mut outputs = vec![vec![99.0; 1]; 5];
    let mut output_refs = outputs
        .iter_mut()
        .map(Vec::as_mut_slice)
        .collect::<Vec<_>>();
    assert!(
        bridge
            .render_coordinates(
                &coordinates,
                None,
                Some(std::slice::from_ref(&invalid)),
                &layout,
                0,
                48_000,
                &mut output_refs,
            )
            .is_err()
    );
    drop(output_refs);
    assert!(outputs.iter().all(|output| output == &[0.0]));

    let mut retry = vec![vec![99.0; 1]; 5];
    let mut retry_refs = retry.iter_mut().map(Vec::as_mut_slice).collect::<Vec<_>>();
    assert!(
        bridge
            .render_coordinates(
                &coordinates,
                None,
                None,
                &layout,
                0,
                48_000,
                &mut retry_refs,
            )
            .is_err()
    );
    drop(retry_refs);
    assert!(retry.iter().all(|output| output == &[0.0]));
}

#[test]
fn fixed_named_dynamic_switch_keeps_existing_q32_scheduler() {
    let layout = layout()
        .with_route_vectors(vec![
            SpatialRouteVector {
                identity: "fixed".to_owned(),
                vector: vec![0.0, 1.0],
            },
            SpatialRouteVector {
                identity: "named".to_owned(),
                vector: vec![1.0, 0.0],
            },
        ])
        .expect("route registry");
    let topology = SpatialTopologySnapshot {
        dynamic_records: vec![SpatialBindingRecord {
            descriptor: descriptor(SpatialSourceClass::DynamicPoint, "dynamic", vec![0.0]),
            scalar: 1.0,
            active: true,
        }],
        ..SpatialTopologySnapshot::default()
    };
    let initial_input = [1.0];
    let initial_coordinates = vec![initial_input.as_slice()];
    let mut bridge = JocSpatialBridge::new();
    let mut initial = vec![vec![0.0; 1]; 2];
    let mut initial_refs = initial
        .iter_mut()
        .map(Vec::as_mut_slice)
        .collect::<Vec<_>>();
    bridge
        .render_coordinates(
            &initial_coordinates,
            Some(&topology),
            None,
            &layout,
            0,
            48_000,
            &mut initial_refs,
        )
        .expect("initial dynamic route");
    drop(initial_refs);
    assert_eq!(initial, vec![vec![1.0], vec![0.0]]);

    let switch = SpatialCoordinateUpdate {
        ordinal: 0,
        descriptor: Some(SpatialDescriptorPatch {
            source_class: Some(SpatialSourceClass::FixedLayout),
            identity: Some("fixed".to_owned()),
            ..SpatialDescriptorPatch::default()
        }),
        scalar: None,
        active: None,
    };
    let switched_input = vec![1.0; 64];
    let switched_coordinates = vec![switched_input.as_slice()];
    let mut switched = vec![vec![0.0; 64]; 2];
    let mut switched_refs = switched
        .iter_mut()
        .map(Vec::as_mut_slice)
        .collect::<Vec<_>>();
    bridge
        .render_coordinates(
            &switched_coordinates,
            None,
            Some(std::slice::from_ref(&switch)),
            &layout,
            64,
            48_000,
            &mut switched_refs,
        )
        .expect("fixed class switch");
    drop(switched_refs);
    assert_eq!(switched[0][0], 1.0);
    assert_eq!(switched[1][0], 0.0);
    assert!(switched[1].iter().any(|value| *value > 0.0));
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
fn bridge_channel_lock_acquire_release_and_switch_use_existing_q32_targets() {
    let layout = layout();
    let mut locked_descriptor = descriptor(SpatialSourceClass::DynamicPoint, "point", vec![0.0]);
    locked_descriptor.channel_lock = true;
    let topology = SpatialTopologySnapshot {
        dynamic_records: vec![SpatialBindingRecord {
            descriptor: locked_descriptor,
            scalar: 1.0,
            active: true,
        }],
        ..SpatialTopologySnapshot::default()
    };
    let input = [vec![1.0]];
    let coordinates = input.iter().map(Vec::as_slice).collect::<Vec<_>>();
    let mut bridge = JocSpatialBridge::new();

    let mut first = vec![vec![0.0; 1]; 2];
    let mut first_refs = first.iter_mut().map(Vec::as_mut_slice).collect::<Vec<_>>();
    bridge
        .render_coordinates(
            &coordinates,
            Some(&topology),
            None,
            &layout,
            0,
            48_000,
            &mut first_refs,
        )
        .expect("locked acquisition");
    drop(first_refs);
    assert_eq!(first, vec![vec![1.0], vec![0.0]]);

    let release = SpatialCoordinateUpdate {
        ordinal: 0,
        descriptor: Some(SpatialDescriptorPatch {
            coordinates: Some(vec![0.5]),
            ..SpatialDescriptorPatch::default()
        }),
        scalar: None,
        active: None,
    };
    let mut ordinary = vec![vec![0.0; 1]; 2];
    let mut ordinary_refs = ordinary
        .iter_mut()
        .map(Vec::as_mut_slice)
        .collect::<Vec<_>>();
    bridge
        .render_coordinates(
            &coordinates,
            None,
            Some(std::slice::from_ref(&release)),
            &layout,
            0,
            48_000,
            &mut ordinary_refs,
        )
        .expect("locked release");
    drop(ordinary_refs);
    let root = 0.5_f64.sqrt();
    assert!((ordinary[0][0] - root).abs() < 1.0e-12);
    assert!((ordinary[1][0] - root).abs() < 1.0e-12);

    let reacquire = SpatialCoordinateUpdate {
        ordinal: 0,
        descriptor: Some(SpatialDescriptorPatch {
            coordinates: Some(vec![1.0]),
            ..SpatialDescriptorPatch::default()
        }),
        scalar: None,
        active: None,
    };
    let mut switched = vec![vec![0.0; 1]; 2];
    let mut switched_refs = switched
        .iter_mut()
        .map(Vec::as_mut_slice)
        .collect::<Vec<_>>();
    bridge
        .render_coordinates(
            &coordinates,
            None,
            Some(std::slice::from_ref(&reacquire)),
            &layout,
            0,
            48_000,
            &mut switched_refs,
        )
        .expect("current-candidate switch");
    drop(switched_refs);
    assert_eq!(switched, vec![vec![0.0], vec![1.0]]);
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

const QMAX_CLEAN: f64 = 32_767.0 / 32_768.0;
const TOP_INNER_LEFT_CLEAN: f64 = 0.241_943_359_375;
const TOP_INNER_RIGHT_CLEAN: f64 = 0.758_056_640_625;
const TOP_FRONT_Y_CLEAN: f64 = 0.241_943_359_375;
const TOP_REAR_Y_CLEAN: f64 = 0.758_056_640_625;

fn clean_channels(labels: &[&str], lfe: Option<&str>) -> Vec<SpatialLayoutChannel> {
    labels
        .iter()
        .map(|identity| SpatialLayoutChannel {
            identity: (*identity).to_owned(),
            enabled: true,
            lfe: lfe == Some(*identity),
        })
        .collect()
}

fn clean_anchor(identity: &str, x: f64, y: f64, z: f64) -> SpatialLayoutAnchor {
    SpatialLayoutAnchor {
        identity: identity.to_owned(),
        x,
        y,
        z,
    }
}

fn clean_row(y: f64, anchors: Vec<SpatialLayoutAnchor>) -> SpatialLayoutRow {
    SpatialLayoutRow { y, anchors }
}

fn clean_bed(rows: Vec<SpatialLayoutRow>) -> SpatialLayoutLayer {
    SpatialLayoutLayer { z: 0.0, rows }
}

fn clean_upper(rows: Vec<SpatialLayoutRow>) -> SpatialLayoutLayer {
    SpatialLayoutLayer {
        z: QMAX_CLEAN,
        rows,
    }
}

fn executable_layout(name: &str) -> SpatialLayout {
    if SPEAKER_LAYOUT_PRESET_NAMES.contains(&name) {
        return SpeakerLayoutPreset::for_name(name)
            .expect("canonical public executable preset")
            .layout;
    }
    let (labels, layers) = match name {
        "2.0" => (
            vec!["FL", "FR"],
            vec![clean_bed(vec![clean_row(
                0.0,
                vec![
                    clean_anchor("FL", 0.0, 0.0, 0.0),
                    clean_anchor("FR", QMAX_CLEAN, 0.0, 0.0),
                ],
            )])],
        ),
        "3.1" => (
            vec!["FL", "FR", "FC", "LFE"],
            vec![clean_bed(vec![clean_row(
                0.0,
                vec![
                    clean_anchor("FL", 0.0, 0.0, 0.0),
                    clean_anchor("FC", 0.5, 0.0, 0.0),
                    clean_anchor("FR", QMAX_CLEAN, 0.0, 0.0),
                ],
            )])],
        ),
        "5.1" => (
            vec!["FL", "FR", "FC", "LFE", "Ls", "Rs"],
            vec![clean_bed(vec![
                clean_row(
                    0.0,
                    vec![
                        clean_anchor("FL", 0.0, 0.0, 0.0),
                        clean_anchor("FC", 0.5, 0.0, 0.0),
                        clean_anchor("FR", QMAX_CLEAN, 0.0, 0.0),
                    ],
                ),
                clean_row(
                    0.5,
                    vec![
                        clean_anchor("Ls", 0.0, 0.5, 0.0),
                        clean_anchor("Rs", QMAX_CLEAN, 0.5, 0.0),
                    ],
                ),
            ])],
        ),
        "5.1.2" => (
            vec!["FL", "FR", "FC", "LFE", "Ls", "Rs", "TFL", "TFR"],
            vec![
                executable_layout("5.1").topology().layers[0].clone(),
                clean_upper(vec![clean_row(
                    0.5,
                    vec![
                        clean_anchor("TFL", TOP_INNER_LEFT_CLEAN, 0.5, QMAX_CLEAN),
                        clean_anchor("TFR", TOP_INNER_RIGHT_CLEAN, 0.5, QMAX_CLEAN),
                    ],
                )]),
            ],
        ),
        "5.1.4" => (
            vec![
                "FL", "FR", "FC", "LFE", "Ls", "Rs", "TFL", "TFR", "TBL", "TBR",
            ],
            vec![
                executable_layout("5.1").topology().layers[0].clone(),
                clean_upper(vec![
                    clean_row(
                        TOP_FRONT_Y_CLEAN,
                        vec![
                            clean_anchor(
                                "TFL",
                                TOP_INNER_LEFT_CLEAN,
                                TOP_FRONT_Y_CLEAN,
                                QMAX_CLEAN,
                            ),
                            clean_anchor(
                                "TFR",
                                TOP_INNER_RIGHT_CLEAN,
                                TOP_FRONT_Y_CLEAN,
                                QMAX_CLEAN,
                            ),
                        ],
                    ),
                    clean_row(
                        TOP_REAR_Y_CLEAN,
                        vec![
                            clean_anchor("TBL", TOP_INNER_LEFT_CLEAN, TOP_REAR_Y_CLEAN, QMAX_CLEAN),
                            clean_anchor(
                                "TBR",
                                TOP_INNER_RIGHT_CLEAN,
                                TOP_REAR_Y_CLEAN,
                                QMAX_CLEAN,
                            ),
                        ],
                    ),
                ]),
            ],
        ),
        "7.1" => (
            vec!["FL", "FR", "FC", "LFE", "Lb", "Rb", "Ls", "Rs"],
            vec![clean_bed(vec![
                clean_row(
                    0.0,
                    vec![
                        clean_anchor("FL", 0.0, 0.0, 0.0),
                        clean_anchor("FC", 0.5, 0.0, 0.0),
                        clean_anchor("FR", QMAX_CLEAN, 0.0, 0.0),
                    ],
                ),
                clean_row(
                    0.5,
                    vec![
                        clean_anchor("Ls", 0.0, 0.5, 0.0),
                        clean_anchor("Rs", QMAX_CLEAN, 0.5, 0.0),
                    ],
                ),
                clean_row(
                    QMAX_CLEAN,
                    vec![
                        clean_anchor("Lb", 0.0, QMAX_CLEAN, 0.0),
                        clean_anchor("Rb", QMAX_CLEAN, QMAX_CLEAN, 0.0),
                    ],
                ),
            ])],
        ),
        "7.1.2" => (
            vec![
                "FL", "FR", "FC", "LFE", "Lb", "Rb", "Ls", "Rs", "TFL", "TFR",
            ],
            vec![
                executable_layout("7.1").topology().layers[0].clone(),
                clean_upper(vec![clean_row(
                    0.5,
                    vec![
                        clean_anchor("TFL", TOP_INNER_LEFT_CLEAN, 0.5, QMAX_CLEAN),
                        clean_anchor("TFR", TOP_INNER_RIGHT_CLEAN, 0.5, QMAX_CLEAN),
                    ],
                )]),
            ],
        ),
        "7.1.4" => (
            vec![
                "FL", "FR", "FC", "LFE", "Lb", "Rb", "Ls", "Rs", "TFL", "TFR", "TBL", "TBR",
            ],
            vec![
                executable_layout("7.1").topology().layers[0].clone(),
                clean_upper(vec![
                    clean_row(
                        TOP_FRONT_Y_CLEAN,
                        vec![
                            clean_anchor(
                                "TFL",
                                TOP_INNER_LEFT_CLEAN,
                                TOP_FRONT_Y_CLEAN,
                                QMAX_CLEAN,
                            ),
                            clean_anchor(
                                "TFR",
                                TOP_INNER_RIGHT_CLEAN,
                                TOP_FRONT_Y_CLEAN,
                                QMAX_CLEAN,
                            ),
                        ],
                    ),
                    clean_row(
                        TOP_REAR_Y_CLEAN,
                        vec![
                            clean_anchor("TBL", TOP_INNER_LEFT_CLEAN, TOP_REAR_Y_CLEAN, QMAX_CLEAN),
                            clean_anchor(
                                "TBR",
                                TOP_INNER_RIGHT_CLEAN,
                                TOP_REAR_Y_CLEAN,
                                QMAX_CLEAN,
                            ),
                        ],
                    ),
                ]),
            ],
        ),
        other => panic!("unknown executable fixture {other}"),
    };
    let lfe = labels.iter().find(|label| **label == "LFE").copied();
    SpatialLayout::from_topology(
        clean_channels(&labels, lfe),
        SpatialLayoutTopology {
            layers,
            aliases: Vec::new(),
        },
        Vec::new(),
    )
    .expect("clean executable topology")
}

fn single_record_topology(descriptor: SpatialDescriptor) -> SpatialTopologySnapshot {
    SpatialTopologySnapshot {
        dynamic_records: vec![SpatialBindingRecord {
            descriptor,
            scalar: 1.0,
            active: true,
        }],
        ..SpatialTopologySnapshot::default()
    }
}

fn render_block(
    bridge: &mut JocSpatialBridge,
    layout: &SpatialLayout,
    topology: Option<&SpatialTopologySnapshot>,
    updates: Option<&[SpatialCoordinateUpdate]>,
    duration_samples: u64,
    block_length: usize,
) -> Vec<Vec<f64>> {
    let input = vec![1.0; block_length];
    let mut storage = vec![vec![0.0; block_length]; layout.active_channel_count()];
    let mut outputs = storage
        .iter_mut()
        .map(Vec::as_mut_slice)
        .collect::<Vec<_>>();
    bridge
        .render_coordinates(
            &[input.as_slice()],
            topology,
            updates,
            layout,
            duration_samples,
            48_000,
            &mut outputs,
        )
        .expect("spatial target render");
    drop(outputs);
    storage
}

fn dynamic_point(x: f64, y: f64, z: f64) -> SpatialDescriptor {
    SpatialDescriptor::new(SpatialSourceClass::DynamicPoint, "point", vec![x, y, z])
}

fn active_index(layout: &SpatialLayout, identity: &str) -> usize {
    layout
        .channels()
        .iter()
        .filter(|channel| channel.enabled && !channel.lfe)
        .position(|channel| channel.identity == identity)
        .expect("active fixture channel")
}

fn assert_unit_l2(vector: &[f64]) {
    assert!(
        vector
            .iter()
            .all(|value| value.is_finite() && *value >= 0.0)
    );
    let power = vector.iter().map(|value| value * value).sum::<f64>();
    assert!((power - 1.0).abs() < 2.0e-12, "power={power}");
}

#[test]
fn clean_executable_layouts_use_one_projector_and_exact_anchor_identity() {
    for name in [
        "2.0", "3.1", "5.1", "5.1.2", "5.1.4", "7.1", "7.1.2", "7.1.4",
    ] {
        let layout = executable_layout(name);
        assert_eq!(layout.coordinate_dimension_count(), 3, "{name}");
        for layer in &layout.topology().layers {
            for row in &layer.rows {
                for anchor in &row.anchors {
                    let projected = layout
                        .project(&dynamic_point(anchor.x, anchor.y, anchor.z))
                        .expect("exact clean anchor");
                    let expected = active_index(&layout, &anchor.identity);
                    assert!(
                        projected
                            .iter()
                            .enumerate()
                            .all(|(index, value)| if index == expected {
                                (*value - 1.0).abs() < 2.0e-12
                            } else {
                                value.abs() < 2.0e-12
                            }),
                        "{name} anchor {} projected as {projected:?}",
                        anchor.identity
                    );
                    assert_unit_l2(&projected);
                }
            }
        }
    }
}

#[test]
fn clean_xyz_sweeps_distinguish_depth_height_and_lfe_ownership() {
    let layout = executable_layout("7.1.4");
    let front = layout.project(&dynamic_point(0.5, 0.0, 0.0)).unwrap();
    assert!((front[active_index(&layout, "FC")] - 1.0).abs() < 2.0e-12);
    let side = layout
        .project(&dynamic_point(QMAX_CLEAN / 2.0, 0.5, 0.0))
        .unwrap();
    assert!(side[active_index(&layout, "FL")].abs() < 2.0e-12);
    assert!((side[active_index(&layout, "Ls")] - 0.5_f64.sqrt()).abs() < 2.0e-12);
    assert!((side[active_index(&layout, "Rs")] - 0.5_f64.sqrt()).abs() < 2.0e-12);
    let rear = layout
        .project(&dynamic_point(QMAX_CLEAN / 2.0, QMAX_CLEAN, 0.0))
        .unwrap();
    assert!(rear[active_index(&layout, "Lb")] > 0.0);
    assert!(rear[active_index(&layout, "Rb")] > 0.0);
    assert!(rear[active_index(&layout, "Ls")].abs() < 2.0e-12);
    let upper = layout
        .project(&dynamic_point(QMAX_CLEAN / 2.0, 0.5, 0.5))
        .unwrap();
    assert!(upper[active_index(&layout, "FC")] > 0.0);
    assert!(upper[active_index(&layout, "TFL")] > 0.0);
    assert_unit_l2(&upper);

    let bed_only = executable_layout("7.1");
    let no_upper = bed_only
        .project(&dynamic_point(0.5, 0.5, QMAX_CLEAN))
        .unwrap();
    assert_eq!(no_upper.len(), 7);
    assert_unit_l2(&no_upper);
}

#[test]
fn clean_boundaries_and_mirror_symmetry_are_gain_continuous() {
    let layout = executable_layout("7.1");
    let left = layout
        .project(&dynamic_point(QMAX_CLEAN * 0.25, 0.5, 0.0))
        .unwrap();
    let right = layout
        .project(&dynamic_point(QMAX_CLEAN * 0.75, 0.5, 0.0))
        .unwrap();
    assert!(
        (left[active_index(&layout, "Ls")] - right[active_index(&layout, "Rs")]).abs() < 2.0e-12
    );
    assert!(
        (left[active_index(&layout, "Rs")] - right[active_index(&layout, "Ls")]).abs() < 2.0e-12
    );
    let at = layout.project(&dynamic_point(0.5, 0.5, 0.0)).unwrap();
    let before = layout
        .project(&dynamic_point(0.5, 0.5 - 1.0e-8, 0.0))
        .unwrap();
    let after = layout
        .project(&dynamic_point(0.5, 0.5 + 1.0e-8, 0.0))
        .unwrap();
    assert!(before.iter().zip(&at).all(|(a, b)| (a - b).abs() < 2.0e-7));
    assert!(after.iter().zip(&at).all(|(a, b)| (a - b).abs() < 2.0e-7));
    for vector in [left, right, at, before, after] {
        assert_unit_l2(&vector);
    }
}

#[test]
fn clean_one_and_three_upper_rows_and_wide_anchors_use_the_same_data_law() {
    let one_row = executable_layout("5.1.2");
    let low_y = one_row
        .project(&dynamic_point(TOP_INNER_LEFT_CLEAN, 0.0, QMAX_CLEAN))
        .unwrap();
    let high_y = one_row
        .project(&dynamic_point(TOP_INNER_LEFT_CLEAN, QMAX_CLEAN, QMAX_CLEAN))
        .unwrap();
    assert_eq!(low_y, high_y);

    let three_rows = SpatialLayout::from_topology(
        clean_channels(&["B", "UF", "UM", "UR"], None),
        SpatialLayoutTopology {
            layers: vec![
                clean_bed(vec![clean_row(0.0, vec![clean_anchor("B", 0.5, 0.0, 0.0)])]),
                clean_upper(vec![
                    clean_row(0.0, vec![clean_anchor("UF", 0.5, 0.0, QMAX_CLEAN)]),
                    clean_row(0.5, vec![clean_anchor("UM", 0.5, 0.5, QMAX_CLEAN)]),
                    clean_row(
                        QMAX_CLEAN,
                        vec![clean_anchor("UR", 0.5, QMAX_CLEAN, QMAX_CLEAN)],
                    ),
                ]),
            ],
            aliases: Vec::new(),
        },
        Vec::new(),
    )
    .unwrap();
    let middle = three_rows
        .project(&dynamic_point(0.5, 0.5, QMAX_CLEAN))
        .unwrap();
    assert_eq!(middle[active_index(&three_rows, "UM")], 1.0);
    let between = three_rows
        .project(&dynamic_point(0.5, 0.25, QMAX_CLEAN))
        .unwrap();
    assert!(between[active_index(&three_rows, "UF")] > 0.0);
    assert!(between[active_index(&three_rows, "UM")] > 0.0);
    assert_unit_l2(&between);

    let wide = SpatialLayout::from_topology(
        clean_channels(&["FL", "Lw", "Rw", "FR"], None),
        SpatialLayoutTopology {
            layers: vec![clean_bed(vec![clean_row(
                0.0,
                vec![
                    clean_anchor("FL", 0.0, 0.0, 0.0),
                    clean_anchor("Lw", 0.25, 0.0, 0.0),
                    clean_anchor("Rw", 0.75, 0.0, 0.0),
                    clean_anchor("FR", QMAX_CLEAN, 0.0, 0.0),
                ],
            )])],
            aliases: Vec::new(),
        },
        Vec::new(),
    )
    .unwrap();
    let wide_point = wide.project(&dynamic_point(0.25, 0.0, 0.0)).unwrap();
    assert_eq!(wide_point[active_index(&wide, "Lw")], 1.0);
    assert_unit_l2(&wide_point);
}

#[test]
fn clean_topology_validation_and_multilayer_projection_are_generic() {
    let duplicate_x = SpatialLayout::from_topology(
        clean_channels(&["A", "B"], None),
        SpatialLayoutTopology {
            layers: vec![clean_bed(vec![clean_row(
                0.0,
                vec![
                    clean_anchor("A", 0.0, 0.0, 0.0),
                    clean_anchor("B", 0.0, 0.0, 0.0),
                ],
            )])],
            aliases: Vec::new(),
        },
        Vec::new(),
    );
    assert!(duplicate_x.is_err());

    let four_layers = SpatialLayout::from_topology(
        clean_channels(&["A", "B", "C", "D"], None),
        SpatialLayoutTopology {
            layers: (0..4)
                .map(|index| {
                    let z = index as f64 / 3.0;
                    SpatialLayoutLayer {
                        z,
                        rows: vec![clean_row(
                            0.0,
                            vec![clean_anchor(["A", "B", "C", "D"][index], 0.5, 0.0, z)],
                        )],
                    }
                })
                .collect(),
            aliases: Vec::new(),
        },
        Vec::new(),
    )
    .unwrap();
    assert_eq!(four_layers.topology().layers.len(), 4);
    let projected = four_layers
        .project(&dynamic_point(0.5, 0.0, 0.5))
        .expect("generic four-layer interpolation");
    assert_unit_l2(&projected);
    assert!(projected[active_index(&four_layers, "B")] > 0.0);
    assert!(projected[active_index(&four_layers, "C")] > 0.0);
}

#[test]
#[ignore = "manual release performance harness"]
fn region_preparation_and_control_performance_harness() {
    use std::time::Instant;

    const FRAME_COUNT: usize = 128;
    const SAMPLES_PER_FRAME: usize = 1_536;
    let layout = executable_layout("7.1.4");
    let states = [
        RegionSemanticState {
            horizontal: RegionHorizontalState::NoConstraints,
            top_bottom: RegionTopBottomState::Include,
        },
        RegionSemanticState {
            horizontal: RegionHorizontalState::BackExcluded,
            top_bottom: RegionTopBottomState::Include,
        },
        RegionSemanticState {
            horizontal: RegionHorizontalState::SideExcluded,
            top_bottom: RegionTopBottomState::Include,
        },
        RegionSemanticState {
            horizontal: RegionHorizontalState::CentreAndBack,
            top_bottom: RegionTopBottomState::Include,
        },
        RegionSemanticState {
            horizontal: RegionHorizontalState::ScreenOnly,
            top_bottom: RegionTopBottomState::Include,
        },
        RegionSemanticState {
            horizontal: RegionHorizontalState::SurroundOnly,
            top_bottom: RegionTopBottomState::Include,
        },
        RegionSemanticState {
            horizontal: RegionHorizontalState::NoConstraints,
            top_bottom: RegionTopBottomState::Exclude,
        },
        RegionSemanticState {
            horizontal: RegionHorizontalState::BackExcluded,
            top_bottom: RegionTopBottomState::Exclude,
        },
        RegionSemanticState {
            horizontal: RegionHorizontalState::SideExcluded,
            top_bottom: RegionTopBottomState::Exclude,
        },
        RegionSemanticState {
            horizontal: RegionHorizontalState::CentreAndBack,
            top_bottom: RegionTopBottomState::Exclude,
        },
        RegionSemanticState {
            horizontal: RegionHorizontalState::ScreenOnly,
            top_bottom: RegionTopBottomState::Exclude,
        },
        RegionSemanticState {
            horizontal: RegionHorizontalState::SurroundOnly,
            top_bottom: RegionTopBottomState::Exclude,
        },
    ];
    let mut selector = RegionTopologySelector::new();
    let start = Instant::now();
    for _ in 0..1_000 {
        for state in states {
            std::hint::black_box(selector.select(&layout, state).expect("region topology"));
        }
    }
    let preparation_seconds = start.elapsed().as_secs_f64();

    let mut benchmark_descriptor = dynamic_point(0.25, 0.5, 0.0);
    benchmark_descriptor.extent = Some([0.25; 3]);
    let topology = SpatialTopologySnapshot {
        dynamic_records: vec![SpatialBindingRecord {
            descriptor: benchmark_descriptor,
            scalar: 1.0,
            active: true,
        }],
        ..SpatialTopologySnapshot::default()
    };
    let input = vec![1.0; SAMPLES_PER_FRAME];
    let mut bridge = JocSpatialBridge::new();
    let mut storage = vec![vec![0.0; SAMPLES_PER_FRAME]; layout.active_channel_count()];
    let mut outputs = storage
        .iter_mut()
        .map(Vec::as_mut_slice)
        .collect::<Vec<_>>();
    bridge
        .render_coordinates(
            &[input.as_slice()],
            Some(&topology),
            None,
            &layout,
            0,
            48_000,
            &mut outputs,
        )
        .expect("initial benchmark snapshot");
    let start = Instant::now();
    for frame in 0..FRAME_COUNT {
        let horizontal = if frame % 2 == 0 {
            RegionHorizontalState::ScreenOnly
        } else {
            RegionHorizontalState::SurroundOnly
        };
        let update = SpatialCoordinateUpdate {
            ordinal: 0,
            descriptor: Some(SpatialDescriptorPatch {
                zones: Some(Some(region_zones(
                    horizontal,
                    RegionTopBottomState::Include,
                ))),
                ..SpatialDescriptorPatch::default()
            }),
            scalar: None,
            active: None,
        };
        bridge
            .render_coordinates(
                &[input.as_slice()],
                None,
                Some(std::slice::from_ref(&update)),
                &layout,
                SAMPLES_PER_FRAME as u64,
                48_000,
                &mut outputs,
            )
            .expect("benchmark region event");
    }
    let render_seconds = start.elapsed().as_secs_f64();
    std::hint::black_box(&storage);
    println!(
        "region_performance preparation_seconds={preparation_seconds:.6} cached_entries={} render_seconds={render_seconds:.6} frames={FRAME_COUNT} samples_per_frame={SAMPLES_PER_FRAME}",
        selector.cached_topology_count()
    );
}

#[test]
#[ignore = "manual release performance harness"]
fn semantic_pair_target_update_performance_harness() {
    use std::time::Instant;

    const ITERATIONS: usize = 100_000;
    let layout = three_anchor_pair_layout();
    let mut point = descriptor(SpatialSourceClass::DynamicPoint, "point", vec![0.25]);
    let mut pair = point.clone();
    pair.pair_span_q15 = Some(16_384);

    let point_start = Instant::now();
    let mut point_checksum = 0.0;
    for index in 0..ITERATIONS {
        point.coordinates[0] = (index % 32_768) as f64 / 32_768.0;
        point_checksum += layout.project(&point).expect("point benchmark target")[0];
    }
    let point_elapsed = point_start.elapsed();

    let pair_start = Instant::now();
    let mut pair_checksum = 0.0;
    for index in 0..ITERATIONS {
        pair.coordinates[0] = (index % 32_768) as f64 / 32_768.0;
        pair_checksum += layout.project(&pair).expect("Pair benchmark target")[0];
    }
    let pair_elapsed = pair_start.elapsed();

    let topology = single_record_topology(pair.clone());
    let mut bridge = JocSpatialBridge::new();
    let input = [1.0_f64];
    let mut output = vec![vec![0.0; 1]; layout.active_channel_count()];
    let mut output_refs = output.iter_mut().map(Vec::as_mut_slice).collect::<Vec<_>>();
    let render_start = Instant::now();
    for index in 0..ITERATIONS {
        let update = SpatialCoordinateUpdate {
            ordinal: 0,
            descriptor: Some(SpatialDescriptorPatch {
                coordinates: Some(vec![(index % 32_768) as f64 / 32_768.0]),
                ..SpatialDescriptorPatch::default()
            }),
            scalar: None,
            active: None,
        };
        bridge
            .render_coordinates(
                std::slice::from_ref(&input.as_slice()),
                if index == 0 { Some(&topology) } else { None },
                (index > 0).then_some(std::slice::from_ref(&update)),
                &layout,
                0,
                48_000,
                &mut output_refs,
            )
            .expect("Pair render benchmark workload");
    }
    let render_elapsed = render_start.elapsed();
    println!(
        "semantic_pair_performance iterations={ITERATIONS} point_seconds={:.6} pair_seconds={:.6} relative_overhead={:.3} render_control_seconds={:.6} point_checksum={point_checksum} pair_checksum={pair_checksum}",
        point_elapsed.as_secs_f64(),
        pair_elapsed.as_secs_f64(),
        pair_elapsed.as_secs_f64() / point_elapsed.as_secs_f64(),
        render_elapsed.as_secs_f64(),
    );
}

#[test]
#[ignore = "manual release performance harness"]
fn channel_lock_target_update_performance_harness() {
    use std::time::Instant;
    const ITERATIONS: usize = 100_000;

    let layout = executable_layout("7.1.4");
    let mut standalone = dynamic_point(0.5, 0.5, 0.0);
    standalone.channel_lock = true;
    let mut lock_extent = standalone.clone();
    lock_extent.extent = Some([0.75; 3]);
    let mut region_lock = standalone.clone();
    region_lock.zones = Some(region_zones(
        RegionHorizontalState::ScreenOnly,
        RegionTopBottomState::Include,
    ));
    let mut triple = region_lock.clone();
    triple.extent = Some([0.75; 3]);

    for (label, mut descriptor) in [
        ("channel_lock", standalone),
        ("channel_lock_extent_state", lock_extent),
        ("region_channel_lock", region_lock),
        ("region_extent_channel_lock", triple),
    ] {
        let start = Instant::now();
        let mut checksum = 0.0;
        for index in 0..ITERATIONS {
            descriptor.coordinates[0] = (index % 32_768) as f64 / 32_768.0;
            let outcome = layout
                .project_with_outcome(&descriptor)
                .expect("ChannelLock composition target update");
            checksum += outcome.target[0];
        }
        let elapsed = start.elapsed();
        eprintln!(
            "channel_lock_performance case={label} iterations={ITERATIONS} elapsed_seconds={:.6} checksum={checksum}",
            elapsed.as_secs_f64()
        );
        assert!(checksum.is_finite());
    }
}
