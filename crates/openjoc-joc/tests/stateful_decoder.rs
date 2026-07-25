use num_complex::Complex64;
use openjoc_joc::{
    HuffmanCodeword, JocDataPoint, JocDecoderState, JocFrame, JocHeader, JocObjectFrame,
    JocPayloadData, QuantMode, Slope,
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
    assert!((first.object_qmf[0][0][0].re - 15.0 * coefficient).abs() < 1.0e-12);
    assert!(!first.state_reset);
    assert_eq!(
        first.stages[0].as_ref().expect("stages").quantized[0][0][0],
        53
    );

    let reused = state
        .decode_frame(&frame(2, absent_object()), &inputs())
        .expect("absent frame reuses state");
    assert!((reused.object_qmf[0][0][0].re - first.object_qmf[0][0][0].re).abs() < 1.0e-12);
    assert!(reused.stages[0].is_none());
    assert!(!reused.state_reset);

    let reset = state
        .decode_frame(&frame(0, absent_object()), &inputs())
        .expect("sequence zero resets state");
    assert!(reset.state_reset);
    assert!(
        reset.object_qmf[0][0]
            .iter()
            .all(|sample| *sample == Complex64::ZERO)
    );
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
fn object_qmf_is_synthesized_to_pcm_with_continuous_per_object_state() {
    let mut state = JocDecoderState::new();
    let first = state
        .decode_frame(&frame(1, full_object(5)), &inputs())
        .expect("first frame");
    let second = state
        .decode_frame(&frame(2, absent_object()), &inputs())
        .expect("continuous frame");

    let mut reference = ReferenceQmf64F64::new();
    let expected_first = reference.synthesize(&first.object_qmf[0][0]);
    let expected_second = reference.synthesize(&second.object_qmf[0][0]);
    assert_eq!(first.object_pcm, vec![expected_first.to_vec()]);
    assert_eq!(second.object_pcm, vec![expected_second.to_vec()]);

    let reset = state
        .decode_frame(&frame(0, absent_object()), &inputs())
        .expect("reset frame");
    assert_eq!(reset.object_pcm, vec![vec![0.0; 64]]);
}
