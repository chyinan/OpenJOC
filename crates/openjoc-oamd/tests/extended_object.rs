use openjoc_oamd::{
    ExtendedObjectElement, OamdError, ObjectClass, Position3, decode_object_divergence_code,
    decode_object_divergence_table, parse_extended_object_element, parse_object_element,
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

fn two_block_dynamic_object() -> openjoc_oamd::ObjectElement {
    let mut bits = Vec::new();
    push(&mut bits, 0, 2);
    push(&mut bits, 1, 3); // two blocks
    for factor in [0, 1] {
        push(&mut bits, factor, 6);
        push(&mut bits, 0, 2);
    }
    push(&mut bits, 1, 1); // reserved data absent

    push(&mut bits, 0, 1); // active block 0
    push(&mut bits, 0, 2); // gain 0 dB
    push(&mut bits, 1, 1); // default priority
    push(&mut bits, 0, 6); // absolute X
    push(&mut bits, 62, 6); // absolute Y
    push(&mut bits, 1, 1); // positive Z
    push(&mut bits, 0, 4);
    push(&mut bits, 0, 1); // inside room
    push(&mut bits, 0, 3); // no zones
    push(&mut bits, 0, 1);
    push(&mut bits, 0, 2); // zero size
    push(&mut bits, 0, 1); // room anchor
    push(&mut bits, 0, 1); // channel unlocked
    push(&mut bits, 0, 1); // no additional data

    push(&mut bits, 0, 1); // active block 1
    push(&mut bits, 2, 2); // reuse basic
    push(&mut bits, 3, 2); // mixed render update
    push(&mut bits, 0b1000, 4); // position only
    push(&mut bits, 1, 1); // differential
    push(&mut bits, 0b111, 3); // X delta -1
    push(&mut bits, 0, 3);
    push(&mut bits, 0, 3);
    push(&mut bits, 0, 1); // inside room
    push(&mut bits, 0, 1); // channel unlocked
    push(&mut bits, 0, 1); // no additional data

    parse_object_element(&pack(bits), &[ObjectClass::Dynamic]).expect("object element")
}

#[test]
fn decodes_every_normative_divergence_table_entry() {
    let coarse = [0.500_755, 0.608_529, 0.704_833, 1.0];
    for (code, expected) in coarse.into_iter().enumerate() {
        assert_eq!(
            decode_object_divergence_table(u8::try_from(code).expect("two-bit code")),
            Ok(expected)
        );
    }

    let fine = [
        0.0, 0.004_026, 0.007_16, 0.012_731, 0.020_173, 0.028_485, 0.040_21, 0.050_582, 0.063_601,
        0.079_914, 0.100_299, 0.125_666, 0.140_532, 0.157_027, 0.175_282, 0.195_417, 0.217_536,
        0.241_718, 0.268_002, 0.296_377, 0.326_766, 0.359_017, 0.392_895, 0.428_081, 0.464_184,
        0.500_755, 0.537_316, 0.573_389, 0.608_529, 0.642_346, 0.674_524, 0.704_833, 0.733_123,
        0.759_32, 0.783_416, 0.805_451, 0.825_506, 0.843_686, 0.860_112, 0.874_914, 0.888_222,
        0.900_168, 0.910_875, 0.920_461, 0.929_035, 0.936_698, 0.943_544, 0.949_656, 0.955_112,
        0.959_98, 0.964_322, 0.968_195, 0.974_729, 0.979_923, 0.984_05, 0.987_33, 0.989_935,
        0.992_874, 0.994_955, 0.996_817, 0.998_21, 0.998_993, 1.0,
    ];
    assert_eq!(
        decode_object_divergence_code(0),
        Err(OamdError::ReservedObjectDivergenceCode)
    );
    for (offset, expected) in fine.into_iter().enumerate() {
        assert_eq!(
            decode_object_divergence_code(
                u8::try_from(offset + 1).expect("six-bit divergence code")
            ),
            Ok(expected)
        );
    }
}

#[test]
fn parses_divergence_reuse_and_extended_precision_in_object_block_order() {
    let objects = two_block_dynamic_object();
    let mut bits = Vec::new();
    push(&mut bits, 1, 1); // divergence block present
    push(&mut bits, 1, 1); // block 0 divergence present
    push(&mut bits, 2, 2); // fine code
    push(&mut bits, 26, 6); // 0.500755
    push(&mut bits, 1, 1); // block 1 divergence present
    push(&mut bits, 1, 2); // reuse previous block
    push(&mut bits, 1, 1); // extended precision block present
    push(&mut bits, 1, 1); // block 0 extension present
    push(&mut bits, 0b101, 3); // X and Z
    push(&mut bits, 1, 2); // X semantics +2
    push(&mut bits, 2, 2); // Z semantics -1
    push(&mut bits, 1, 1); // block 1 extension present
    push(&mut bits, 0b100, 3); // X only
    push(&mut bits, 1, 2); // +2, applied before lower clamp
    let meaningful_bits = bits.len();

    assert_eq!(
        parse_extended_object_element(&pack(bits), &objects, &[ObjectClass::Dynamic]),
        Ok(ExtendedObjectElement {
            divergence: Some(vec![vec![0.500_755, 0.500_755]]),
            extended_precision: Some(vec![vec![[Some(1), None, Some(2)], [Some(1), None, None],]]),
            consumed_bits: meaningful_bits,
        })
    );
}

#[test]
fn applies_extended_precision_before_differential_clamping() {
    let mut objects = two_block_dynamic_object();
    let extension = ExtendedObjectElement {
        divergence: None,
        extended_precision: Some(vec![vec![
            [None; 3],
            [Some(1), None, None], // +2 fifth-steps
        ]]),
        consumed_bits: 0,
    };
    extension
        .apply_positions(&mut objects)
        .expect("extended positions");
    assert_eq!(
        objects.objects[0][1].render.position,
        Position3 {
            x: 0.0, // max(0, -1/62 + 2/(62*5))
            y: 1.0,
            z: 0.0,
        }
    );
}

#[test]
fn rejects_reserved_mode_and_reuse_without_a_previous_block() {
    let mut one_block = two_block_dynamic_object();
    one_block.timing.blocks.truncate(1);
    one_block.objects[0].truncate(1);

    let mut reserved = Vec::new();
    push(&mut reserved, 1, 1);
    push(&mut reserved, 1, 1);
    push(&mut reserved, 3, 2);
    assert_eq!(
        parse_extended_object_element(&pack(reserved), &one_block, &[ObjectClass::Dynamic]),
        Err(OamdError::ReservedObjectDivergenceMode)
    );

    let mut reuse = Vec::new();
    push(&mut reuse, 1, 1);
    push(&mut reuse, 1, 1);
    push(&mut reuse, 1, 2);
    assert_eq!(
        parse_extended_object_element(&pack(reuse), &one_block, &[ObjectClass::Dynamic]),
        Err(OamdError::MissingPreviousObjectDivergence)
    );
}

#[test]
fn coarse_absent_inactive_and_bed_divergence_follow_block_semantics() {
    let mut dynamic = two_block_dynamic_object();
    dynamic.timing.blocks.truncate(1);
    dynamic.objects[0].truncate(1);

    let mut coarse = Vec::new();
    push(&mut coarse, 1, 1); // divergence block
    push(&mut coarse, 1, 1); // divergence present
    push(&mut coarse, 0, 2); // coarse table mode
    push(&mut coarse, 3, 2); // 1.0
    push(&mut coarse, 0, 1); // no precision block
    assert_eq!(
        parse_extended_object_element(&pack(coarse), &dynamic, &[ObjectClass::Dynamic])
            .expect("coarse divergence")
            .divergence,
        Some(vec![vec![1.0]])
    );

    let mut absent = Vec::new();
    push(&mut absent, 1, 1);
    push(&mut absent, 0, 1); // object divergence absent means zero
    push(&mut absent, 0, 1);
    assert_eq!(
        parse_extended_object_element(&pack(absent), &dynamic, &[ObjectClass::Dynamic])
            .expect("absent divergence")
            .divergence,
        Some(vec![vec![0.0]])
    );

    let mut inactive_and_bed = dynamic.clone();
    inactive_and_bed.objects = vec![
        vec![{
            let mut update = dynamic.objects[0][0].clone();
            update.active = false;
            update
        }],
        dynamic.objects[0].clone(),
    ];
    let mut implicit_zero = Vec::new();
    push(&mut implicit_zero, 1, 1); // no per-block bits for either object
    push(&mut implicit_zero, 0, 1);
    assert_eq!(
        parse_extended_object_element(
            &pack(implicit_zero),
            &inactive_and_bed,
            &[ObjectClass::Dynamic, ObjectClass::BedOrIsf],
        )
        .expect("implicit zero divergence")
        .divergence,
        Some(vec![vec![0.0], vec![0.0]])
    );
}
