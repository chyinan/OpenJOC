use openjoc_joc::{
    JocParseError, JocPayloadData, QuantMode, Slope, all_huffman_tables, parse_joc_payload,
};

fn push_bits(bits: &mut Vec<bool>, value: u64, width: u8) {
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

fn codeword_for(nodes: &[[i16; 2]], wanted: u16) -> Vec<bool> {
    fn visit(nodes: &[[i16; 2]], node: usize, wanted: u16, path: &mut Vec<bool>) -> bool {
        for branch in 0..2 {
            path.push(branch != 0);
            let child = nodes[node][branch];
            if child > 0 {
                if visit(nodes, usize::try_from(child).expect("node"), wanted, path) {
                    return true;
                }
            } else if u16::try_from(-i32::from(child) - 1) == Ok(wanted) {
                return true;
            }
            path.pop();
        }
        false
    }
    let mut path = Vec::new();
    assert!(visit(nodes, 0, wanted, &mut path));
    path
}

#[test]
fn parses_and_retains_a_complete_full_matrix_object() {
    let mut bits = Vec::new();
    push_bits(&mut bits, 0, 3); // 5-channel downmix
    push_bits(&mut bits, 0, 6); // one object
    push_bits(&mut bits, 0, 3); // no extension
    push_bits(&mut bits, 2, 3);
    push_bits(&mut bits, 17, 5);
    push_bits(&mut bits, 42, 10);
    push_bits(&mut bits, 1, 1); // present
    push_bits(&mut bits, 0, 3); // one band
    push_bits(&mut bits, 0, 1); // full matrix
    push_bits(&mut bits, 0, 1); // 96 steps
    push_bits(&mut bits, 0, 1); // smooth
    push_bits(&mut bits, 0, 1); // one data point
    let table = all_huffman_tables()[0];
    let codeword = codeword_for(table.nodes, 48);
    for _ in 0..5 {
        bits.extend_from_slice(&codeword);
    }

    let frame = parse_joc_payload(&pack(bits)).expect("valid JOC payload");

    assert_eq!(frame.header.downmix_index, 0);
    assert_eq!(frame.header.channel_count, 5);
    assert_eq!(frame.header.object_count_bits, 0);
    assert_eq!(frame.clip_gain_x_bits, 2);
    assert_eq!(frame.clip_gain_y_bits, 17);
    assert_eq!(frame.sequence_count, 42);
    let object = &frame.objects[0];
    assert!(object.present);
    assert_eq!(object.band_count, Some(1));
    assert_eq!(object.quant_mode, Some(QuantMode::Coarse96));
    assert_eq!(object.slope, Some(Slope::Smooth));
    let JocPayloadData::Full { matrix_symbols } = &object.data_points[0].payload else {
        panic!("expected full-matrix payload");
    };
    assert_eq!(matrix_symbols.len(), 5);
    assert!(matrix_symbols.iter().all(|channel| channel[0].symbol == 48));
    assert!(
        matrix_symbols
            .iter()
            .all(|channel| channel[0].bits == codeword)
    );
}

#[test]
fn parses_sparse_fine_two_point_steep_data_and_offsets() {
    let mut bits = Vec::new();
    push_bits(&mut bits, 1, 3); // 7-channel downmix
    push_bits(&mut bits, 0, 6);
    push_bits(&mut bits, 0, 3);
    push_bits(&mut bits, 0, 3 + 5 + 10);
    push_bits(&mut bits, 1, 1);
    push_bits(&mut bits, 1, 3); // three bands
    push_bits(&mut bits, 1, 1); // sparse
    push_bits(&mut bits, 1, 1); // 192 steps
    push_bits(&mut bits, 1, 1); // steep
    push_bits(&mut bits, 1, 1); // two points
    push_bits(&mut bits, 1, 5); // offsets are coded +1
    push_bits(&mut bits, 3, 5);
    let tables = all_huffman_tables();
    let index_one = codeword_for(tables[5].nodes, 1);
    let index_two = codeword_for(tables[5].nodes, 2);
    let vectors = [
        codeword_for(tables[3].nodes, 0),
        codeword_for(tables[3].nodes, 100),
        codeword_for(tables[3].nodes, 191),
    ];
    for initial_channel in [0, 6] {
        push_bits(&mut bits, initial_channel, 3);
        bits.extend_from_slice(&index_one);
        bits.extend_from_slice(&index_two);
        for vector in &vectors {
            bits.extend_from_slice(vector);
        }
    }

    let frame = parse_joc_payload(&pack(bits)).expect("valid sparse payload");
    let object = &frame.objects[0];
    assert_eq!(object.band_count, Some(3));
    assert_eq!(object.quant_mode, Some(QuantMode::Fine192));
    assert_eq!(object.slope, Some(Slope::Steep));
    assert_eq!(object.data_points.len(), 2);
    assert_eq!(object.data_points[0].offset_timeslot, Some(2));
    assert_eq!(object.data_points[1].offset_timeslot, Some(4));
    for (point, expected_channel) in object.data_points.iter().zip([0, 6]) {
        let JocPayloadData::Sparse {
            initial_channel,
            channel_deltas,
            vector_symbols,
        } = &point.payload
        else {
            panic!("expected sparse payload");
        };
        assert_eq!(*initial_channel, expected_channel);
        assert_eq!(
            channel_deltas
                .iter()
                .map(|code| code.symbol)
                .collect::<Vec<_>>(),
            [1, 2]
        );
        assert_eq!(
            vector_symbols
                .iter()
                .map(|code| code.symbol)
                .collect::<Vec<_>>(),
            [0, 100, 191]
        );
    }
}

#[test]
fn absent_object_has_no_conditional_fields_or_data() {
    let mut bits = Vec::new();
    push_bits(&mut bits, 1, 3);
    push_bits(&mut bits, 0, 6);
    push_bits(&mut bits, 0, 3);
    push_bits(&mut bits, 0, 3 + 5 + 10);
    push_bits(&mut bits, 0, 1);

    let frame = parse_joc_payload(&pack(bits)).expect("valid absent object");
    assert!(!frame.objects[0].present);
    assert!(frame.objects[0].data_points.is_empty());
}

#[test]
fn rejects_reserved_header_values_and_more_than_sixteen_objects() {
    for (downmix, objects, extension, expected) in [
        (5, 0, 0, JocParseError::ReservedDownmix { index: 5 }),
        (0, 16, 0, JocParseError::TooManyObjects { count: 17 }),
        (0, 0, 1, JocParseError::ReservedExtension { index: 1 }),
    ] {
        let mut bits = Vec::new();
        push_bits(&mut bits, downmix, 3);
        push_bits(&mut bits, objects, 6);
        push_bits(&mut bits, extension, 3);
        assert_eq!(parse_joc_payload(&pack(bits)), Err(expected));
    }
}

#[test]
fn rejects_nonzero_padding() {
    let mut bits = Vec::new();
    push_bits(&mut bits, 0, 3);
    push_bits(&mut bits, 0, 6);
    push_bits(&mut bits, 0, 3);
    push_bits(&mut bits, 0, 3 + 5 + 10);
    push_bits(&mut bits, 0, 1);
    while bits.len() % 8 != 7 {
        bits.push(false);
    }
    bits.push(true);

    assert_eq!(
        parse_joc_payload(&pack(bits)),
        Err(JocParseError::NonZeroPadding)
    );
}
