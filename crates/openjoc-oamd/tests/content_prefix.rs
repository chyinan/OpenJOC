use openjoc_oamd::{
    BedAssignment, ContentDescription, OamdContentPrefix, OamdError, parse_oamd_content_prefix,
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

#[test]
fn parses_dynamic_only_content_description() {
    let mut bits = Vec::new();
    push(&mut bits, 0, 2); // syntax version
    push(&mut bits, 2, 5); // three objects
    push(&mut bits, 1, 1); // dynamic-only
    push(&mut bits, 1, 1); // LFE present
    push(&mut bits, 0, 1); // no alternate object data
    push(&mut bits, 0, 4); // no elements in this prefix fixture

    assert_eq!(
        parse_oamd_content_prefix(&pack(bits)),
        Ok(OamdContentPrefix {
            syntax_version: 0,
            object_count: 3,
            content: ContentDescription::DynamicOnly { lfe_present: true },
            alternate_object_data_present: false,
            element_count: 0,
            consumed_bits: 14,
        })
    );
}

#[test]
fn parses_extended_counts_and_mixed_program_assignment() {
    let mut bits = Vec::new();
    push(&mut bits, 3, 2);
    push(&mut bits, 4, 3); // syntax version 7
    push(&mut bits, 31, 5);
    push(&mut bits, 0, 7); // 32 objects
    push(&mut bits, 0, 1); // mixed
    push(&mut bits, 0b0110, 4); // ISF + dynamic
    push(&mut bits, 2, 3); // 10-object ISF
    push(&mut bits, 21, 5); // 22 dynamic objects
    push(&mut bits, 1, 1); // alternate data present
    push(&mut bits, 15, 4);
    push(&mut bits, 2, 5); // 17 elements

    let prefix = parse_oamd_content_prefix(&pack(bits)).expect("mixed prefix");
    assert_eq!(prefix.syntax_version, 7);
    assert_eq!(prefix.object_count, 32);
    assert_eq!(prefix.element_count, 17);
    assert!(prefix.alternate_object_data_present);
    assert_eq!(
        prefix.content,
        ContentDescription::Mixed {
            bed_channel_distribute: None,
            beds: vec![],
            intermediate_spatial_format: Some(2),
            dynamic_objects: Some(22),
        }
    );
}

#[test]
fn retains_normative_bed_assignment_and_rejects_reserved_isf() {
    let mut bed = Vec::new();
    push(&mut bed, 0, 2);
    push(&mut bed, 1, 5); // two objects
    push(&mut bed, 0, 1);
    push(&mut bed, 0b1000, 4);
    push(&mut bed, 1, 1); // distribute
    push(&mut bed, 0, 1); // one bed
    push(&mut bed, 0, 1); // not LFE-only
    push(&mut bed, 1, 1); // standard assignment
    push(&mut bed, 0b11_0000_0000, 10);
    push(&mut bed, 0, 1);
    push(&mut bed, 0, 4);
    assert_eq!(
        parse_oamd_content_prefix(&pack(bed))
            .expect("bed prefix")
            .content,
        ContentDescription::Mixed {
            bed_channel_distribute: Some(true),
            beds: vec![BedAssignment::Standard(0b11_0000_0000)],
            intermediate_spatial_format: None,
            dynamic_objects: None,
        }
    );

    let mut reserved = Vec::new();
    push(&mut reserved, 0, 2);
    push(&mut reserved, 0, 5);
    push(&mut reserved, 0, 1);
    push(&mut reserved, 0b0100, 4);
    push(&mut reserved, 6, 3);
    assert_eq!(
        parse_oamd_content_prefix(&pack(reserved)),
        Err(OamdError::ReservedIntermediateSpatialFormat { index: 6 })
    );
}
