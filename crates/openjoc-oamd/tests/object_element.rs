use openjoc_oamd::{
    Distance, Extent3, Gain, MetadataBlockTiming, MetadataTiming, ObjectBasicInfo, ObjectClass,
    ObjectElement, ObjectRenderInfo, ObjectUpdate, Position3, StandardPositionBits, ZoneConstraint,
    parse_object_element,
};

fn push(bits: &mut Vec<bool>, value: u64, width: u8) {
    for shift in (0..width).rev() {
        bits.push(value & (1_u64 << shift) != 0);
    }
}

fn pack(mut bits: Vec<bool>) -> Vec<u8> {
    while bits.len() % 8 != 0 {
        bits.push(false);
    }
    let mut bytes = vec![0; bits.len() / 8];
    for (index, bit) in bits.into_iter().enumerate() {
        if bit {
            bytes[index / 8] |= 0x80 >> (index % 8);
        }
    }
    bytes
}

fn assert_close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1.0e-12,
        "{actual} != {expected}"
    );
}

fn full_dynamic_block(bits: &mut Vec<bool>, gain_index: u8, gain_bits: Option<u8>) {
    push(bits, 0, 1); // active
    push(bits, gain_index.into(), 2);
    if let Some(gain_bits) = gain_bits {
        push(bits, gain_bits.into(), 6);
    }
    push(bits, 0, 1); // explicit priority
    push(bits, 16, 5);
    push(bits, 31, 6); // absolute X
    push(bits, 62, 6); // absolute Y
    push(bits, 1, 1); // positive Z
    push(bits, 15, 4);
    push(bits, 1, 1); // distance specified
    push(bits, 0, 1); // finite
    push(bits, 2, 4); // factor 1.6
    push(bits, 2, 3); // side zone excluded
    push(bits, 0, 1); // elevation excluded
    push(bits, 2, 2); // independent size
    push(bits, 1, 5);
    push(bits, 2, 5);
    push(bits, 3, 5);
    push(bits, 1, 1); // screen anchored
    push(bits, 3, 3); // factor 0.5
    push(bits, 2, 2); // depth factor 1
    push(bits, 1, 1); // channel lock
    push(bits, 0, 1); // no additional table data
}

fn expected_full(gain: Gain) -> ObjectUpdate {
    use ZoneConstraint::{Exclude, Include};
    ObjectUpdate {
        active: true,
        basic: ObjectBasicInfo {
            gain,
            priority: 0.5,
        },
        render: ObjectRenderInfo {
            position: Position3 {
                x: 0.5,
                y: 1.0,
                z: 1.0,
            },
            standard_position: StandardPositionBits {
                x: 31,
                y: 62,
                z: 15,
            },
            distance: Distance::Finite(1.6),
            zones: [Include, Exclude, Include, Include, Include, Exclude],
            size: Extent3 {
                width: 1.0 / 31.0,
                depth: 2.0 / 31.0,
                height: 3.0 / 31.0,
            },
            screen_anchor: true,
            screen_factor: 0.5,
            depth_factor: 1.0,
            channel_lock: true,
        },
        additional_table_data: None,
    }
}

#[test]
fn parses_a_full_dynamic_object_update() {
    let mut bits = Vec::new();
    push(&mut bits, 0, 2); // sample offset 0
    push(&mut bits, 0, 3); // one block
    push(&mut bits, 0, 6); // block offset
    push(&mut bits, 0, 2); // no ramp
    push(&mut bits, 1, 1); // reserved data absent
    full_dynamic_block(&mut bits, 2, Some(20));
    let meaningful_bits = bits.len();

    assert_eq!(
        parse_object_element(&pack(bits), &[ObjectClass::Dynamic]),
        Ok(ObjectElement {
            timing: MetadataTiming {
                sample_offset: 0,
                blocks: vec![MetadataBlockTiming {
                    start_sample: 0,
                    ramp_duration: 0,
                }],
            },
            objects: vec![vec![expected_full(Gain::Decibels(-6))]],
            consumed_bits: meaningful_bits,
        })
    );
}

#[test]
fn applies_reuse_mixed_update_and_previous_object_gain() {
    let mut bits = Vec::new();
    push(&mut bits, 0, 2);
    push(&mut bits, 1, 3); // two blocks
    for factor in [0, 1] {
        push(&mut bits, factor, 6);
        push(&mut bits, 0, 2);
    }
    push(&mut bits, 1, 1);

    full_dynamic_block(&mut bits, 0, None);
    push(&mut bits, 0, 1); // active, second block
    push(&mut bits, 2, 2); // basic full reuse
    push(&mut bits, 3, 2); // render mixed
    push(&mut bits, 0b0001, 4); // screen group only
    push(&mut bits, 0, 1); // room anchor
    push(&mut bits, 0, 1); // channel lock is always signalled
    push(&mut bits, 0, 1); // no additional table data

    full_dynamic_block(&mut bits, 3, None); // gain from previous object in block 0
    push(&mut bits, 0, 1);
    push(&mut bits, 2, 2);
    push(&mut bits, 2, 2); // render full reuse
    push(&mut bits, 0, 1);

    let decoded = parse_object_element(&pack(bits), &[ObjectClass::Dynamic, ObjectClass::Dynamic])
        .expect("object element");
    assert_eq!(decoded.objects[0][0], expected_full(Gain::Decibels(0)));
    assert_eq!(decoded.objects[0][1].basic, decoded.objects[0][0].basic);
    assert!(!decoded.objects[0][1].render.screen_anchor);
    assert_close(decoded.objects[0][1].render.screen_factor, 0.0);
    assert!(!decoded.objects[0][1].render.channel_lock);
    assert_eq!(decoded.objects[1][0], expected_full(Gain::Decibels(0)));
    assert_eq!(decoded.objects[1][1].basic, decoded.objects[1][0].basic);
    assert_eq!(
        decoded.objects[1][1].render.position,
        decoded.objects[1][0].render.position
    );
}

#[test]
fn inactive_and_bed_objects_use_normative_defaults() {
    let mut bits = Vec::new();
    push(&mut bits, 0, 2);
    push(&mut bits, 0, 3);
    push(&mut bits, 0, 6);
    push(&mut bits, 0, 2);
    push(&mut bits, 1, 1);
    push(&mut bits, 1, 1); // inactive dynamic object
    push(&mut bits, 0, 1); // no additional data
    push(&mut bits, 0, 1); // active bed object
    push(&mut bits, 0, 2); // gain 0 dB
    push(&mut bits, 1, 1); // default priority
    push(&mut bits, 0, 1); // no additional data

    let decoded = parse_object_element(&pack(bits), &[ObjectClass::Dynamic, ObjectClass::BedOrIsf])
        .expect("defaults");
    assert!(!decoded.objects[0][0].active);
    assert_eq!(decoded.objects[0][0].basic.gain, Gain::NegativeInfinity);
    assert_close(decoded.objects[0][0].basic.priority, 0.0);
    assert_eq!(decoded.objects[0][0].render, ObjectRenderInfo::DEFAULT);
    assert_eq!(decoded.objects[1][0].render, ObjectRenderInfo::DEFAULT);
}

#[test]
fn rejects_truncation_and_reserved_syntax() {
    assert!(parse_object_element(&[0], &[ObjectClass::Dynamic]).is_err());

    let mut reserved_offset = Vec::new();
    push(&mut reserved_offset, 3, 2);
    assert!(parse_object_element(&pack(reserved_offset), &[]).is_err());

    let mut reserved_header = Vec::new();
    push(&mut reserved_header, 0, 2);
    push(&mut reserved_header, 0, 3);
    push(&mut reserved_header, 0, 6);
    push(&mut reserved_header, 0, 2);
    push(&mut reserved_header, 0, 1); // reserved data follows
    push(&mut reserved_header, 1, 5); // must be zero
    assert!(parse_object_element(&pack(reserved_header), &[]).is_err());
}

#[test]
fn retains_additional_table_data_inside_its_declared_bound() {
    let mut bits = Vec::new();
    push(&mut bits, 0, 2);
    push(&mut bits, 0, 3);
    push(&mut bits, 0, 6);
    push(&mut bits, 0, 2);
    push(&mut bits, 1, 1);
    push(&mut bits, 1, 1); // inactive object
    push(&mut bits, 1, 1); // additional table data exists
    push(&mut bits, 0, 4); // one-byte bounded window
    push(&mut bits, 0xa5, 8);
    let decoded =
        parse_object_element(&pack(bits), &[ObjectClass::Dynamic]).expect("additional data");
    assert_eq!(
        decoded.objects[0][0].additional_table_data,
        Some(vec![0xa5])
    );

    let mut truncated = Vec::new();
    push(&mut truncated, 0, 2);
    push(&mut truncated, 0, 3);
    push(&mut truncated, 0, 6);
    push(&mut truncated, 0, 2);
    push(&mut truncated, 1, 1);
    push(&mut truncated, 1, 1);
    push(&mut truncated, 1, 1);
    push(&mut truncated, 1, 4); // declares two bytes
    push(&mut truncated, 0xa5, 8); // only one byte follows
    assert!(parse_object_element(&pack(truncated), &[ObjectClass::Dynamic]).is_err());
}
