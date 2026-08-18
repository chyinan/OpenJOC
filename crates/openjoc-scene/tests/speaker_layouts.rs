use openjoc_scene::{
    SPEAKER_LAYOUT_PRESET_NAMES, SpatialDescriptor, SpatialSourceClass, SpeakerLayoutPreset,
    speaker_channel_mask_for_labels, speaker_layout_5_1_4, speaker_layout_7_1_2,
    speaker_layout_7_1_6,
};

const QMAX: f64 = 32_767.0 / 32_768.0;
const TOP_INNER_LEFT: f64 = 0.241_943_359_375;
const TOP_INNER_RIGHT: f64 = 0.758_056_640_625;
const TOP_FRONT_Y: f64 = 0.241_943_359_375;
const TOP_REAR_Y: f64 = 0.758_056_640_625;

fn point(x: f64, y: f64, z: f64) -> SpatialDescriptor {
    SpatialDescriptor::new(SpatialSourceClass::DynamicPoint, "fixture", vec![x, y, z])
}

fn active_index(preset: &SpeakerLayoutPreset, identity: &str) -> usize {
    preset
        .layout
        .channels()
        .iter()
        .filter(|channel| channel.enabled && !channel.lfe)
        .position(|channel| channel.identity == identity)
        .expect("active speaker identity")
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
fn public_presets_have_the_admitted_names_and_backend_contracts() {
    assert_eq!(
        SPEAKER_LAYOUT_PRESET_NAMES,
        [
            "2.0", "5.1", "5.1.2", "5.1.4", "7.1", "7.1.2", "7.1.4", "7.1.6", "9.1", "9.1.2",
            "9.1.4", "9.1.6",
        ]
    );
    let stereo = SpeakerLayoutPreset::for_name("2.0").expect("2.0 public preset");
    assert_eq!(stereo.channel_labels(), vec!["FL", "FR"]);
    assert_eq!(stereo.channel_count(), 2);
    assert_eq!(stereo.lfe_index(), None);
    assert_eq!(stereo.wav_channel_mask(), Some(0x0000_0003));
    let expected = [
        ("5.1", 6, 0x0000_060f),
        ("5.1.2", 8, 0x0000_560f),
        ("5.1.4", 10, 0x0002_d60f),
        ("7.1", 8, 0x0000_063f),
        ("7.1.2", 10, 0x0000_563f),
        ("7.1.4", 12, 0x0002_d63f),
    ];
    for (name, count, mask) in expected {
        let preset = SpeakerLayoutPreset::for_name(name).expect("public preset");
        assert_eq!(preset.name, name);
        assert_eq!(preset.channel_count(), count);
        assert_eq!(preset.lfe_index(), Some(3));
        assert_eq!(preset.wav_channel_mask(), Some(mask));
        assert_eq!(
            speaker_channel_mask_for_labels(&preset.labels).expect("standard mask"),
            mask
        );
    }
    let preset = SpeakerLayoutPreset::for_name("7.1.6").expect("7.1.6 public preset");
    assert_eq!(preset.channel_count(), 14);
    assert_eq!(preset.lfe_index(), Some(3));
    assert_eq!(preset.wav_channel_mask(), None);
    assert_eq!(
        preset.channel_labels(),
        vec![
            "FL", "FR", "FC", "LFE", "Lb", "Rb", "Ls", "Rs", "Ltf", "Rtf", "Ltm", "Rtm", "Ltr",
            "Rtr",
        ]
    );
    assert!(speaker_channel_mask_for_labels(&preset.labels).is_err());
    for (name, count, labels) in [
        (
            "9.1",
            10,
            vec!["FL", "FR", "FC", "LFE", "Lb", "Rb", "Ls", "Rs", "Lw", "Rw"],
        ),
        (
            "9.1.2",
            12,
            vec![
                "FL", "FR", "FC", "LFE", "Lb", "Rb", "Ls", "Rs", "Lw", "Rw", "Ltm", "Rtm",
            ],
        ),
        (
            "9.1.4",
            14,
            vec![
                "FL", "FR", "FC", "LFE", "Lb", "Rb", "Ls", "Rs", "Lw", "Rw", "Ltf", "Rtf", "Ltr",
                "Rtr",
            ],
        ),
        (
            "9.1.6",
            16,
            vec![
                "FL", "FR", "FC", "LFE", "Lb", "Rb", "Ls", "Rs", "Lw", "Rw", "Ltf", "Rtf", "Ltm",
                "Rtm", "Ltr", "Rtr",
            ],
        ),
    ] {
        let preset = SpeakerLayoutPreset::for_name(name).expect("9.1-family preset");
        assert_eq!(preset.channel_count(), count);
        assert_eq!(preset.lfe_index(), Some(3));
        assert_eq!(preset.wav_channel_mask(), None);
        assert_eq!(preset.channel_labels(), labels);
    }
    assert!(SpeakerLayoutPreset::for_name("22.2").is_err());
}

#[test]
fn two_point_zero_reuses_generic_full_xyz_projection() {
    let preset = SpeakerLayoutPreset::for_name("2.0").expect("2.0 preset");
    let left = preset.layout.project(&point(0.0, 0.0, 0.0)).unwrap();
    let right = preset.layout.project(&point(QMAX, 0.0, 0.0)).unwrap();
    let center = preset.layout.project(&point(0.5, 0.0, 0.0)).unwrap();
    let rear = preset.layout.project(&point(0.5, QMAX, 0.0)).unwrap();
    let height = preset.layout.project(&point(0.5, 0.0, QMAX)).unwrap();
    let negative_z = preset.layout.project(&point(0.5, 0.0, -QMAX)).unwrap();
    assert_eq!(left, vec![1.0, 0.0]);
    assert_eq!(right, vec![0.0, 1.0]);
    assert_unit_l2(&center);
    assert_unit_l2(&rear);
    assert_unit_l2(&height);
    assert_unit_l2(&negative_z);
    assert!(center[0] > 0.0 && center[1] > 0.0);
    assert_eq!(rear, center);
    assert_eq!(height, center);
    assert_eq!(negative_z, center);
}

#[test]
fn nine_one_wide_row_uses_exact_q15_geometry_and_generic_interpolation() {
    const WIDE_Y: f64 = 5_285.0 / 32_768.0;
    const WIDE_RIGHT_X: f64 = 32_767.0 / 32_768.0;
    let preset = SpeakerLayoutPreset::for_name("9.1").expect("9.1 preset");
    let bed = &preset.layout.topology().layers[0];
    assert_eq!(bed.rows.len(), 4);
    assert_eq!(
        bed.rows.iter().map(|row| row.y).collect::<Vec<_>>(),
        [0.0, WIDE_Y, 0.5, QMAX]
    );
    assert_eq!(bed.rows[1].anchors[0].identity, "Lw");
    assert_eq!(bed.rows[1].anchors[1].identity, "Rw");
    assert_eq!(bed.rows[1].anchors[0].x, 0.0);
    assert_eq!(bed.rows[1].anchors[1].x, WIDE_RIGHT_X);
    assert_eq!(bed.rows[1].anchors[0].z, 0.0);
    assert_eq!(bed.rows[1].anchors[1].z, 0.0);
    assert_eq!(
        bed.rows[1].anchors[0].x + bed.rows[1].anchors[1].x,
        WIDE_RIGHT_X
    );

    let left = preset
        .layout
        .project(&point(0.0, WIDE_Y, 0.0))
        .expect("Lw anchor");
    let right = preset
        .layout
        .project(&point(WIDE_RIGHT_X, WIDE_Y, 0.0))
        .expect("Rw anchor");
    assert_eq!(left[active_index(&preset, "Lw")], 1.0);
    assert_eq!(right[active_index(&preset, "Rw")], 1.0);
    assert!(left[active_index(&preset, "Rw")].abs() < 2.0e-12);
    assert!(right[active_index(&preset, "Lw")].abs() < 2.0e-12);

    let wide_midpoint = preset
        .layout
        .project(&point(0.5, WIDE_Y, 0.0))
        .expect("Wide-row midpoint");
    assert!(wide_midpoint[active_index(&preset, "Lw")] > 0.0);
    assert!(wide_midpoint[active_index(&preset, "Rw")] > 0.0);
    assert_unit_l2(&wide_midpoint);

    for y in [f64::midpoint(0.0, WIDE_Y), f64::midpoint(WIDE_Y, 0.5)] {
        let transition = preset
            .layout
            .project(&point(0.5, y, 0.0))
            .expect("Wide Y transition");
        assert!(
            transition[active_index(&preset, "FC")] > 0.0
                || transition[active_index(&preset, "Ls")] > 0.0
        );
        assert!(
            transition[active_index(&preset, "Lw")] > 0.0
                || transition[active_index(&preset, "Rw")] > 0.0
        );
        assert_unit_l2(&transition);
    }
}

#[test]
fn nine_one_family_preserves_bed_and_upper_topology_data_relationships() {
    let seven = SpeakerLayoutPreset::for_name("7.1").expect("7.1 preset");
    let nine = SpeakerLayoutPreset::for_name("9.1").expect("9.1 preset");
    let seven_rows = &seven.layout.topology().layers[0].rows;
    let nine_rows = &nine.layout.topology().layers[0].rows;
    assert_eq!(nine_rows[0], seven_rows[0]);
    assert_eq!(nine_rows[2], seven_rows[1]);
    assert_eq!(nine_rows[3], seven_rows[2]);

    let nine_six = SpeakerLayoutPreset::for_name("9.1.6").expect("9.1.6 preset");
    let seven_six = SpeakerLayoutPreset::for_name("7.1.6").expect("7.1.6 preset");
    assert_eq!(
        nine_six.layout.topology().layers[1]
            .rows
            .iter()
            .map(|row| (
                row.y,
                row.anchors
                    .iter()
                    .map(|anchor| (anchor.x, anchor.z))
                    .collect::<Vec<_>>()
            ))
            .collect::<Vec<_>>(),
        seven_six.layout.topology().layers[1]
            .rows
            .iter()
            .map(|row| (
                row.y,
                row.anchors
                    .iter()
                    .map(|anchor| (anchor.x, anchor.z))
                    .collect::<Vec<_>>()
            ))
            .collect::<Vec<_>>()
    );

    let left = nine
        .layout
        .project(&point(0.125, 5_285.0 / 32_768.0, 0.0))
        .unwrap();
    let right = nine
        .layout
        .project(&point(32_767.0 / 32_768.0 - 0.125, 5_285.0 / 32_768.0, 0.0))
        .unwrap();
    assert!((left[active_index(&nine, "Lw")] - right[active_index(&nine, "Rw")]).abs() < 2.0e-12);
    assert!((left[active_index(&nine, "Rw")] - right[active_index(&nine, "Lw")]).abs() < 2.0e-12);
    assert_unit_l2(&left);
    assert_unit_l2(&right);
}

#[test]
fn seven_one_six_uses_three_upper_rows_and_preserves_generic_xyz_projection() {
    let preset = SpeakerLayoutPreset::for_name("7.1.6").expect("7.1.6 preset");
    let topology = preset.layout.topology();
    assert_eq!(topology.layers.len(), 2);
    assert_eq!(topology.layers[0].rows.len(), 3);
    assert_eq!(topology.layers[1].rows.len(), 3);
    assert_eq!(
        topology.layers[0]
            .rows
            .iter()
            .map(|row| row.y)
            .collect::<Vec<_>>(),
        [0.0, 0.5, QMAX]
    );
    assert_eq!(
        topology.layers[1]
            .rows
            .iter()
            .map(|row| row.y)
            .collect::<Vec<_>>(),
        [TOP_FRONT_Y, 0.5, TOP_REAR_Y]
    );

    for layer in &topology.layers {
        for row in &layer.rows {
            for anchor in &row.anchors {
                let projected = preset
                    .layout
                    .project(&point(anchor.x, anchor.y, anchor.z))
                    .expect("7.1.6 clean anchor projection");
                let index = active_index(&preset, &anchor.identity);
                assert!(
                    projected.iter().enumerate().all(|(candidate, value)| {
                        if candidate == index {
                            (*value - 1.0).abs() < 2.0e-12
                        } else {
                            value.abs() < 2.0e-12
                        }
                    }),
                    "{} projected as {projected:?}",
                    anchor.identity
                );
            }
        }
    }

    let upper_front_middle = preset
        .layout
        .project(&point(
            TOP_INNER_LEFT,
            f64::midpoint(TOP_FRONT_Y, 0.5),
            QMAX,
        ))
        .expect("front-to-middle upper transition");
    assert!(upper_front_middle[active_index(&preset, "Ltf")] > 0.0);
    assert!(upper_front_middle[active_index(&preset, "Ltm")] > 0.0);
    assert_eq!(
        preset
            .layout
            .project(&point(TOP_INNER_LEFT, 0.5, QMAX))
            .unwrap()[active_index(&preset, "Ltm")],
        1.0
    );
    let upper_middle_rear = preset
        .layout
        .project(&point(TOP_INNER_LEFT, f64::midpoint(0.5, TOP_REAR_Y), QMAX))
        .expect("middle-to-rear upper transition");
    assert!(upper_middle_rear[active_index(&preset, "Ltm")] > 0.0);
    assert!(upper_middle_rear[active_index(&preset, "Ltr")] > 0.0);

    let z_composed = preset
        .layout
        .project(&point(0.5, 0.5, 0.5))
        .expect("7.1.6 bed/top composition");
    assert!(z_composed[active_index(&preset, "FC")] > 0.0);
    assert!(z_composed[active_index(&preset, "Ltm")] > 0.0);
    assert_unit_l2(&z_composed);
    assert_eq!(preset.layout.active_channel_count(), 13);
    assert_eq!(preset.layout.channels()[3].identity, "LFE");
    assert!(preset.layout.channels()[3].lfe);
}

#[test]
fn seven_one_six_inserts_only_the_middle_upper_row_over_seven_one_four_geometry() {
    let four = SpeakerLayoutPreset::for_name("7.1.4").expect("7.1.4 preset");
    let six = speaker_layout_7_1_6().expect("7.1.6 helper");
    assert_eq!(four.layout.topology().layers[0], six.topology().layers[0]);
    let four_upper = &four.layout.topology().layers[1].rows;
    let six_upper = &six.topology().layers[1].rows;
    assert_eq!(four_upper.len(), 2);
    assert_eq!(six_upper.len(), 3);
    for (outer, inserted) in [(0, 0), (1, 2)] {
        assert_eq!(four_upper[outer].y, six_upper[inserted].y);
        assert_eq!(
            four_upper[outer]
                .anchors
                .iter()
                .map(|anchor| (anchor.x, anchor.y, anchor.z))
                .collect::<Vec<_>>(),
            six_upper[inserted]
                .anchors
                .iter()
                .map(|anchor| (anchor.x, anchor.y, anchor.z))
                .collect::<Vec<_>>()
        );
    }
    assert_eq!(six_upper[1].y, 0.5);
    assert_eq!(
        six_upper[1]
            .anchors
            .iter()
            .map(|anchor| anchor.identity.as_str())
            .collect::<Vec<_>>(),
        ["Ltm", "Rtm"]
    );
}

#[test]
fn five_one_four_uses_clean_two_row_upper_topology_and_generic_xyz_projection() {
    let preset = SpeakerLayoutPreset::for_name("5.1.4").expect("5.1.4 preset");
    assert_eq!(preset.layout.topology().layers.len(), 2);
    assert_eq!(preset.layout.topology().layers[0].rows.len(), 2);
    assert_eq!(preset.layout.topology().layers[1].rows.len(), 2);
    assert_eq!(
        preset.layout.topology().layers[1]
            .rows
            .iter()
            .map(|row| row.y)
            .collect::<Vec<_>>(),
        [TOP_FRONT_Y, TOP_REAR_Y]
    );
    assert_eq!(
        preset.layout.topology().layers[1].rows[0]
            .anchors
            .iter()
            .map(|anchor| anchor.x)
            .collect::<Vec<_>>(),
        [TOP_INNER_LEFT, TOP_INNER_RIGHT]
    );

    for layer in &preset.layout.topology().layers {
        for row in &layer.rows {
            for anchor in &row.anchors {
                let projected = preset
                    .layout
                    .project(&point(anchor.x, anchor.y, anchor.z))
                    .expect("clean anchor projection");
                let index = active_index(&preset, &anchor.identity);
                assert!(
                    projected.iter().enumerate().all(|(candidate, value)| {
                        if candidate == index {
                            (*value - 1.0).abs() < 2.0e-12
                        } else {
                            value.abs() < 2.0e-12
                        }
                    }),
                    "{} projected as {projected:?}",
                    anchor.identity
                );
            }
        }
    }

    let upper_middle = preset
        .layout
        .project(&point(0.5, 0.5, QMAX))
        .expect("upper interior point");
    for label in ["TFL", "TFR", "TBL", "TBR"] {
        assert!(upper_middle[active_index(&preset, label)] > 0.0, "{label}");
    }
    assert_unit_l2(&upper_middle);

    let left = preset
        .layout
        .project(&point(0.3, 0.5, QMAX))
        .expect("left upper point");
    let right = preset
        .layout
        .project(&point(0.7, 0.5, QMAX))
        .expect("right upper point");
    for (left_label, right_label) in [("TFL", "TFR"), ("TBL", "TBR")] {
        assert!(
            (left[active_index(&preset, left_label)] - right[active_index(&preset, right_label)])
                .abs()
                < 2.0e-12
        );
        assert!(
            (left[active_index(&preset, right_label)] - right[active_index(&preset, left_label)])
                .abs()
                < 2.0e-12
        );
    }

    let z_composed = preset
        .layout
        .project(&point(0.5, 0.0, 0.5))
        .expect("bed/top composition");
    assert!(z_composed[active_index(&preset, "FC")] > 0.0);
    assert!(z_composed[active_index(&preset, "TFL")] > 0.0);
    assert_unit_l2(&z_composed);
    assert_eq!(preset.layout.active_channel_count(), 9);
}

#[test]
fn seven_one_two_has_complete_bed_and_one_row_upper_y_degeneration() {
    let preset = SpeakerLayoutPreset::for_name("7.1.2").expect("7.1.2 preset");
    assert_eq!(preset.layout.topology().layers[0].rows.len(), 3);
    assert_eq!(preset.layout.topology().layers[1].rows.len(), 1);
    assert_eq!(
        preset.layout.topology().layers[0]
            .rows
            .iter()
            .map(|row| row.y)
            .collect::<Vec<_>>(),
        [0.0, 0.5, QMAX]
    );
    assert_eq!(
        preset.layout.topology().layers[1].rows[0].anchors[0].identity,
        "TFL"
    );
    assert_eq!(
        preset.layout.topology().layers[1].rows[0].anchors[1].identity,
        "TFR"
    );

    let upper_front = preset
        .layout
        .project(&point(TOP_INNER_LEFT, 0.0, QMAX))
        .expect("upper row at front depth");
    let upper_rear = preset
        .layout
        .project(&point(TOP_INNER_LEFT, QMAX, QMAX))
        .expect("upper row at rear depth");
    assert_eq!(upper_front, upper_rear);
    assert!(upper_front[active_index(&preset, "TFL")] > 0.0);
    assert!(upper_front[active_index(&preset, "TFR")] < 2.0e-12);
    assert_unit_l2(&upper_front);

    let bed_front = preset
        .layout
        .project(&point(0.5, 0.0, 0.0))
        .expect("front bed point");
    let bed_rear = preset
        .layout
        .project(&point(0.5, QMAX, 0.0))
        .expect("rear bed point");
    assert!(bed_front[active_index(&preset, "FC")] > 0.99);
    assert!(bed_rear[active_index(&preset, "Lb")] > 0.0);
    assert!(bed_rear[active_index(&preset, "Rb")] > 0.0);
    assert_unit_l2(&bed_rear);
}

#[test]
fn named_layout_helpers_return_the_same_canonical_data_instances() {
    let four = speaker_layout_5_1_4().expect("5.1.4 helper");
    let four_by_name = SpeakerLayoutPreset::for_name("5.1.4").expect("5.1.4 name");
    assert_eq!(four, four_by_name.layout);

    let two = speaker_layout_7_1_2().expect("7.1.2 helper");
    let two_by_name = SpeakerLayoutPreset::for_name("7.1.2").expect("7.1.2 name");
    assert_eq!(two, two_by_name.layout);
}
