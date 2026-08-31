// pattern: Functional Core

use num_complex::Complex64;
use openjoc_joc::{
    HuffmanCodeword, JocDataPoint, JocDecodeError, JocDecoderState, JocFrame, JocHeader,
    JocObjectFrame, JocPayloadData, QuantMode, ReconstructionStageTiming, Slope,
    all_huffman_tables,
};
use openjoc_qmf::ReferenceQmf64F64;

fn push_bits(bits: &mut Vec<bool>, value: u64, width: u8) {
    for shift in (0..width).rev() {
        bits.push(value & (1_u64 << shift) != 0);
    }
}

fn pack_bits(mut bits: Vec<bool>) -> Vec<u8> {
    while bits.len() % 8 != 0 {
        bits.push(false);
    }
    let mut bytes = vec![0_u8; bits.len() / 8];
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
                if visit(
                    nodes,
                    usize::try_from(child).expect("Huffman node"),
                    wanted,
                    path,
                ) {
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

fn synthetic_payload(
    downmix_index: u8,
    channel_count: u8,
    sparse: bool,
    sequence_count: u16,
    object_present: bool,
) -> Vec<u8> {
    assert!(sequence_count <= 1023);
    let tables = all_huffman_tables();
    let mut bits = Vec::new();
    push_bits(&mut bits, u64::from(downmix_index), 3);
    push_bits(&mut bits, 0, 6); // one object
    push_bits(&mut bits, 0, 3); // no extension
    push_bits(&mut bits, 0, 3); // clip gain X
    push_bits(&mut bits, 0, 5); // clip gain Y
    push_bits(&mut bits, u64::from(sequence_count), 10);
    push_bits(&mut bits, u64::from(object_present), 1);
    if !object_present {
        return pack_bits(bits);
    }
    push_bits(&mut bits, if sparse { 3 } else { 0 }, 3); // seven or one band
    push_bits(&mut bits, u64::from(sparse), 1);
    push_bits(&mut bits, 0, 1); // coarse 96-step quantizer
    push_bits(&mut bits, 0, 1); // smooth interpolation
    push_bits(&mut bits, 0, 1); // one data point

    if sparse {
        push_bits(&mut bits, 0, 3); // initial channel
        let index_table = tables[4 + usize::from(channel_count == 7)];
        let mut previous_raw = 0_u8;
        for band in 1..7_u8 {
            let raw = if band < channel_count {
                (band + channel_count - previous_raw) % channel_count
            } else {
                0
            };
            bits.extend(codeword_for(index_table.nodes, u16::from(raw)));
            previous_raw = raw;
        }
        for _ in 0..7 {
            bits.extend(codeword_for(tables[2].nodes, 1));
        }
    } else {
        for channel in 0..channel_count {
            bits.extend(codeword_for(tables[0].nodes, 49 + u16::from(channel)));
        }
    }
    pack_bits(bits)
}

fn synthetic_inputs(channel_count: u8) -> Vec<Vec<[Complex64; 64]>> {
    (0..usize::from(channel_count))
        .map(|channel| {
            vec![
                [Complex64::new(channel as f64 + 1.0, 0.25); 64],
                [Complex64::new(channel as f64 + 2.0, -0.5); 64],
            ]
        })
        .collect()
}

fn single_timeslot_inputs(channel_count: u8) -> Vec<Vec<[Complex64; 64]>> {
    (0..usize::from(channel_count))
        .map(|channel| vec![[Complex64::new(channel as f64 + 1.0, 0.25); 64]])
        .collect()
}

fn full_object(symbol: u16) -> JocObjectFrame {
    JocObjectFrame {
        present: true,
        band_index: Some(0),
        band_count: Some(1),
        sparse: Some(false),
        quant_mode: Some(QuantMode::Coarse96),
        slope: Some(Slope::Smooth),
        data_points: vec![JocDataPoint {
            offset_timeslot: None,
            payload: JocPayloadData::Full {
                matrix_symbols: (0..5)
                    .map(|_| {
                        vec![HuffmanCodeword {
                            bits: vec![],
                            symbol,
                        }]
                    })
                    .collect(),
            },
        }],
    }
}

fn absent_object() -> JocObjectFrame {
    JocObjectFrame {
        present: false,
        band_index: None,
        band_count: None,
        sparse: None,
        quant_mode: None,
        slope: None,
        data_points: vec![],
    }
}

fn frame(sequence_count: u16, object: JocObjectFrame) -> JocFrame {
    JocFrame {
        header: JocHeader {
            downmix_index: 0,
            channel_count: 5,
            object_count_bits: 0,
            object_count: 1,
            extension_index: 0,
        },
        clip_gain_x_bits: 0,
        clip_gain_y_bits: 0,
        sequence_count,
        objects: vec![object],
    }
}

fn multi_frame(sequence_count: u16, objects: Vec<JocObjectFrame>) -> JocFrame {
    let object_count = u8::try_from(objects.len()).expect("test object count");
    JocFrame {
        header: JocHeader {
            downmix_index: 0,
            channel_count: 5,
            object_count_bits: object_count - 1,
            object_count,
            extension_index: 0,
        },
        clip_gain_x_bits: 0,
        clip_gain_y_bits: 0,
        sequence_count,
        objects,
    }
}

fn inputs() -> Vec<Vec<[Complex64; 64]>> {
    (1..=5)
        .map(|value| vec![[Complex64::new(f64::from(value), 0.0); 64]])
        .collect()
}

#[test]
fn frame_pipeline_reuses_absent_state_and_resets_on_sequence_zero() {
    let mut state = JocDecoderState::new();
    let first = state
        .decode_frame(&frame(1, full_object(5)), &inputs())
        .expect("present frame");
    let coefficient = 5.0 * 820.0 / 4096.0;
    assert!((first.reconstruction_qmf[0][0][0].re - 15.0 * coefficient).abs() < 1.0e-12);
    assert!(!first.state_reset);
    assert_eq!(
        first.stages[0].as_ref().expect("stages").quantized[0][0][0],
        53
    );

    let reused = state
        .decode_frame(&frame(2, absent_object()), &inputs())
        .expect("absent frame reuses state");
    assert!(
        (reused.reconstruction_qmf[0][0][0].re - first.reconstruction_qmf[0][0][0].re).abs()
            < 1.0e-12
    );
    assert!(reused.stages[0].is_none());
    assert!(!reused.state_reset);

    let reset = state
        .decode_frame(&frame(0, absent_object()), &inputs())
        .expect("sequence zero resets state");
    assert!(reset.state_reset);
    assert!(
        reset.reconstruction_qmf[0][0]
            .iter()
            .all(|sample| *sample == Complex64::ZERO)
    );
}

#[test]
fn three_object_state_keeps_an_absent_middle_ordinal_through_decode_and_reset() {
    let mut state = JocDecoderState::new();
    let first = state
        .decode_frame(
            &multi_frame(1, vec![full_object(1), full_object(5), full_object(10)]),
            &inputs(),
        )
        .expect("three present objects");
    assert_eq!(first.stages.len(), 3);
    assert_eq!(
        first
            .stages
            .iter()
            .map(|stage| stage.as_ref().map(|stage| stage.quantized[0][0][0]))
            .collect::<Vec<_>>(),
        [Some(49), Some(53), Some(58)]
    );

    let next = state
        .decode_frame(
            &multi_frame(2, vec![full_object(2), absent_object(), full_object(11)]),
            &inputs(),
        )
        .expect("absent middle object reuses its own state");
    assert_eq!(
        next.stages.iter().map(Option::is_some).collect::<Vec<_>>(),
        [true, false, true]
    );
    assert_eq!(
        next.reconstruction_qmf[1], first.reconstruction_qmf[1],
        "the absent middle ordinal must retain matrix slot 1"
    );
    assert_ne!(next.reconstruction_qmf[0], first.reconstruction_qmf[0]);
    assert_ne!(next.reconstruction_qmf[2], first.reconstruction_qmf[2]);

    let reset = state
        .decode_frame(
            &multi_frame(0, vec![absent_object(), absent_object(), absent_object()]),
            &inputs(),
        )
        .expect("sequence reset");
    assert!(reset.state_reset);
    assert_eq!(reset.reconstruction_qmf.len(), 3);
    assert!(reset.reconstruction_qmf.iter().all(|row| {
        row.iter()
            .flatten()
            .all(|sample| *sample == Complex64::ZERO)
    }));
}

#[test]
fn discontinuous_sequence_resets_interpolation_origin() {
    let mut state = JocDecoderState::new();
    state
        .decode_frame(&frame(7, full_object(5)), &inputs())
        .expect("initial frame");
    let discontinuous = state
        .decode_frame(&frame(9, full_object(5)), &inputs())
        .expect("discontinuous frame");

    assert!(discontinuous.state_reset);
    let target = 5.0 * 820.0 / 4096.0;
    assert!(
        (discontinuous.stages[0]
            .as_ref()
            .expect("stages")
            .interpolated[0][0][0]
            - target)
            .abs()
            < 1.0e-12
    );
}

#[test]
fn reconstruction_qmf_is_synthesized_to_basis_rows_with_continuous_state() {
    let mut state = JocDecoderState::new();
    let first = state
        .decode_frame(&frame(1, full_object(5)), &inputs())
        .expect("first frame");
    let second = state
        .decode_frame(&frame(2, absent_object()), &inputs())
        .expect("continuous frame");

    let mut reference = ReferenceQmf64F64::new();
    let expected_first = reference.synthesize(&first.reconstruction_qmf[0][0]);
    let expected_second = reference.synthesize(&second.reconstruction_qmf[0][0]);
    assert_eq!(
        first.reconstruction_basis.rows,
        vec![expected_first.to_vec()]
    );
    assert_eq!(
        second.reconstruction_basis.rows,
        vec![expected_second.to_vec()]
    );

    let reset = state
        .decode_frame(&frame(0, absent_object()), &inputs())
        .expect("reset frame");
    assert_eq!(reset.reconstruction_basis.rows, vec![vec![0.0; 64]]);
}

#[test]
fn downmix_pcm_is_analyzed_before_object_reconstruction() {
    let joc_frame = frame(1, full_object(5));
    let downmix_pcm = (1..=5)
        .map(|channel| {
            (0..128)
                .map(|sample| f64::from(channel * 128 + sample) / 1024.0)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    let mut integrated = JocDecoderState::new();
    let actual = integrated
        .decode_pcm_frame(&joc_frame, &downmix_pcm)
        .expect("PCM frame");

    let mut analyzers = vec![ReferenceQmf64F64::new(); 5];
    let qmf = downmix_pcm
        .iter()
        .zip(&mut analyzers)
        .map(|(channel, analyzer)| {
            channel
                .chunks_exact(64)
                .map(|chunk| analyzer.analyze(chunk.try_into().expect("64 samples")))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let mut staged = JocDecoderState::new();
    let expected = staged.decode_frame(&joc_frame, &qmf).expect("QMF frame");

    assert_eq!(actual.reconstruction_qmf, expected.reconstruction_qmf);
    assert_eq!(actual.reconstruction_basis, expected.reconstruction_basis);
}

#[test]
fn downmix_pcm_rejects_partial_qmf_blocks_without_advancing_state() {
    let mut state = JocDecoderState::new();
    let invalid = vec![vec![0.0; 65]; 5];
    assert!(matches!(
        state.decode_pcm_frame(&frame(1, full_object(5)), &invalid),
        Err(JocDecodeError::InputSampleCountNotQmfAligned { samples: 65 })
    ));

    let valid = vec![vec![0.0; 64]; 5];
    let decoded = state
        .decode_pcm_frame(&frame(1, full_object(5)), &valid)
        .expect("state remains usable after rejected PCM");
    assert!(!decoded.state_reset);
}

#[test]
fn reconstruction_timing_is_opt_in_and_tracks_qmf_stages() {
    let mut state = JocDecoderState::new();
    state.enable_reconstruction_timing();
    state
        .decode_frame(&frame(1, full_object(5)), &inputs())
        .expect("timed frame");

    let timing = state.take_reconstruction_timing();
    assert!(timing.coefficient_decode > std::time::Duration::ZERO);
    assert!(timing.dequantization > std::time::Duration::ZERO);
    assert!(timing.interpolation > std::time::Duration::ZERO);
    assert!(timing.matrix_reconstruction > std::time::Duration::ZERO);
    assert!(timing.qmf_synthesis > std::time::Duration::ZERO);
    assert_eq!(
        state.take_reconstruction_timing(),
        ReconstructionStageTiming::default()
    );
}

#[test]
fn idx234_full_and_sparse_payloads_decode_the_declared_input_dimensions() {
    for (downmix_index, channel_count) in [(2, 7), (3, 5), (4, 7)] {
        for sparse in [false, true] {
            let payload = synthetic_payload(downmix_index, channel_count, sparse, 1, true);
            let inputs = synthetic_inputs(channel_count);
            let mut decoder_state = JocDecoderState::new();
            let (frame, decoded) = decoder_state
                .decode_payload(&payload, &inputs)
                .expect("synthetic idx2/idx3/idx4 payload");

            assert_eq!(frame.header.downmix_index, downmix_index);
            assert_eq!(frame.header.channel_count, channel_count);
            assert_eq!(decoded.reconstruction_basis.rows.len(), 1);
            assert!(
                decoded.reconstruction_basis.rows[0]
                    .iter()
                    .all(|sample| sample.is_finite())
            );
            let object_stages = decoded.stages[0].as_ref().expect("present object stages");
            assert_eq!(object_stages.quantized[0].len(), usize::from(channel_count));
            if sparse {
                assert_eq!(object_stages.quantized[0][0].len(), 7);
                assert!(
                    object_stages.quantized[0]
                        .iter()
                        .all(|bands| bands.iter().any(|value| *value != 50))
                );
            } else {
                assert!(
                    object_stages.quantized[0]
                        .iter()
                        .all(|bands| bands.len() == 1 && bands[0] != 50)
                );
            }

            for channel in 0..usize::from(channel_count) {
                let mut altered_inputs = inputs.clone();
                for sample in altered_inputs[channel]
                    .iter_mut()
                    .flat_map(|timeslot| timeslot.iter_mut())
                {
                    *sample = Complex64::ZERO;
                }
                let mut altered_state = JocDecoderState::new();
                let (_, altered) = altered_state
                    .decode_payload(&payload, &altered_inputs)
                    .expect("altered synthetic input");
                assert_ne!(
                    altered.reconstruction_qmf[0], decoded.reconstruction_qmf[0],
                    "idx {downmix_index} {channel_count}-channel input {channel} is causal"
                );
            }
        }
    }
}

#[test]
fn phase_shift_indices_do_not_rotate_pcm_again() {
    for (base_index, phase_index, channel_count) in [(0, 3, 5), (2, 4, 7)] {
        let inputs = synthetic_inputs(channel_count);
        let mut base_state = JocDecoderState::new();
        let (_, base) = base_state
            .decode_payload(
                &synthetic_payload(base_index, channel_count, false, 1, true),
                &inputs,
            )
            .expect("base synthetic payload");
        let mut phase_state = JocDecoderState::new();
        let (_, phase) = phase_state
            .decode_payload(
                &synthetic_payload(phase_index, channel_count, false, 1, true),
                &inputs,
            )
            .expect("phase-index synthetic payload");

        assert_eq!(
            base.reconstruction_qmf, phase.reconstruction_qmf,
            "idx {phase_index} is signaling-only at the JOC decoder stage"
        );
        assert_eq!(base.reconstruction_basis, phase.reconstruction_basis);
    }
}

#[test]
fn idx234_payload_state_reuses_absent_objects_resets_and_resets_on_topology_change() {
    for (downmix_index, channel_count) in [(2, 7), (3, 5), (4, 7)] {
        let inputs = single_timeslot_inputs(channel_count);
        let mut state = JocDecoderState::new();
        let (first_frame, first) = state
            .decode_payload(
                &synthetic_payload(downmix_index, channel_count, false, 1, true),
                &inputs,
            )
            .expect("present idx2/idx3/idx4 payload");
        assert_eq!(first_frame.header.downmix_index, downmix_index);
        assert_eq!(first_frame.header.channel_count, channel_count);
        assert_eq!(first_frame.sequence_count, 1);
        assert!(first_frame.objects[0].present);

        let (reused_frame, reused) = state
            .decode_payload(
                &synthetic_payload(downmix_index, channel_count, false, 2, false),
                &inputs,
            )
            .expect("absent idx2/idx3/idx4 payload");

        assert_eq!(reused_frame.sequence_count, 2);
        assert!(!reused_frame.objects[0].present);
        assert_eq!(reused.reconstruction_qmf, first.reconstruction_qmf);
        assert!(reused.stages[0].is_none());
        assert!(!reused.state_reset);

        let (reset_frame, reset) = state
            .decode_payload(
                &synthetic_payload(downmix_index, channel_count, false, 0, false),
                &inputs,
            )
            .expect("sequence-zero idx2/idx3/idx4 payload");
        assert_eq!(reset_frame.sequence_count, 0);
        assert!(!reset_frame.objects[0].present);
        assert!(reset.state_reset);
        assert!(
            reset
                .reconstruction_qmf
                .iter()
                .flatten()
                .flatten()
                .all(|sample| *sample == Complex64::ZERO)
        );
        assert!(
            reset
                .reconstruction_basis
                .rows
                .iter()
                .flatten()
                .all(|sample| *sample == 0.0)
        );
    }

    let mut transition_state = JocDecoderState::new();
    transition_state
        .decode_payload(
            &synthetic_payload(3, 5, false, 1, true),
            &single_timeslot_inputs(5),
        )
        .expect("5-channel idx3 transition source");
    let (_, transitioned) = transition_state
        .decode_payload(
            &synthetic_payload(2, 7, false, 2, true),
            &single_timeslot_inputs(7),
        )
        .expect("7-channel idx2 transition target");
    assert!(transitioned.state_reset);
}
