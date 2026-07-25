use openjoc_oamd::{
    MetadataBlockTiming, MetadataTimelineState, MetadataTiming, TimedMetadataBlock,
    parse_metadata_timing,
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
    for (i, bit) in bits.into_iter().enumerate() {
        if bit {
            bytes[i / 8] |= 0x80 >> (i % 8);
        }
    }
    bytes
}

#[test]
fn decodes_multiple_update_offsets_and_all_ramp_forms() {
    let mut bits = Vec::new();
    push(&mut bits, 1, 2); // table sample offset
    push(&mut bits, 2, 2); // 18 samples
    push(&mut bits, 3, 3); // four blocks
    push(&mut bits, 0, 6);
    push(&mut bits, 0, 2);
    push(&mut bits, 1, 6);
    push(&mut bits, 1, 2);
    push(&mut bits, 2, 6);
    push(&mut bits, 2, 2);
    push(&mut bits, 3, 6);
    push(&mut bits, 3, 2);
    push(&mut bits, 1, 1);
    push(&mut bits, 15, 4);

    assert_eq!(
        parse_metadata_timing(&pack(bits)),
        Ok(MetadataTiming {
            sample_offset: 18,
            blocks: vec![
                MetadataBlockTiming {
                    start_sample: 18,
                    ramp_duration: 0
                },
                MetadataBlockTiming {
                    start_sample: 50,
                    ramp_duration: 512
                },
                MetadataBlockTiming {
                    start_sample: 82,
                    ramp_duration: 1536
                },
                MetadataBlockTiming {
                    start_sample: 114,
                    ramp_duration: 2048
                },
            ],
        })
    );
}

#[test]
fn frame_offset_advances_by_1536_and_failed_frames_are_atomic() {
    let mut state = MetadataTimelineState::new();
    assert_eq!(
        state.decode_frame(&[0, 0]).expect("first frame"),
        vec![TimedMetadataBlock {
            start_sample: 0,
            frame_offset: 0,
            ramp_duration: 0,
        }]
    );
    assert!(state.decode_frame(&[0b1100_0000]).is_err());
    assert_eq!(
        state.decode_frame(&[0, 0]).expect("second valid frame")[0].frame_offset,
        1536
    );
    state.reset();
    assert_eq!(
        state.decode_frame(&[0, 0]).expect("reset frame")[0].frame_offset,
        0
    );
}
