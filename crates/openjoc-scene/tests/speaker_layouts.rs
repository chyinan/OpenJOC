use openjoc_scene::{
    SPEAKER_LAYOUT_PRESET_NAMES, SpatialDescriptor, SpatialSourceClass, SpeakerLayoutPreset,
    speaker_channel_mask_for_labels, speaker_layout_5_1_4, speaker_layout_7_1_2,
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
fn public_presets_have_only_the_six_admitted_names_and_explicit_wav_contracts() {
    assert_eq!(
        SPEAKER_LAYOUT_PRESET_NAMES,
        ["5.1", "5.1.2", "5.1.4", "7.1", "7.1.2", "7.1.4"]
    );
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
        assert_eq!(preset.wav_channel_mask(), mask);
        assert_eq!(
            speaker_channel_mask_for_labels(&preset.labels).expect("standard mask"),
            mask
        );
    }
    assert!(SpeakerLayoutPreset::for_name("9.1.6").is_err());
    assert!(SpeakerLayoutPreset::for_name("22.2").is_err());
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
