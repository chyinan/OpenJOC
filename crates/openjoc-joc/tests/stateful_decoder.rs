use num_complex::Complex64;
use openjoc_joc::{
    HuffmanCodeword, JocDataPoint, JocDecodeError, JocDecoderState, JocFrame, JocHeader,
    JocObjectFrame, JocPayloadData, QuantMode, ReconstructionStageTiming, Slope,
};
use openjoc_qmf::ReferenceQmf64F64;

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
