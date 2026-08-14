use openjoc_oamd::{OamdDecoderConfig, ReferenceScreen};
use openjoc_scene::{
    BaseFullBandCoordinate, BridgeError, JocFrameInput, JocSpatialBridge,
    JocSpatialOperatorUnresolvedReason, PayloadDecoder, PayloadDecoderConfig,
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

fn absent_joc_payload(sequence_count: u16) -> Vec<u8> {
    let mut bits = Vec::new();
    push(&mut bits, 0, 3);
    push(&mut bits, 0, 6);
    push(&mut bits, 0, 3);
    push(&mut bits, 0, 3 + 5);
    push(&mut bits, u64::from(sequence_count), 10);
    push(&mut bits, 0, 1);
    pack(bits)
}

fn inactive_oamd_payload() -> Vec<u8> {
    let mut bits = Vec::new();
    push(&mut bits, 0, 2);
    push(&mut bits, 0, 5);
    push(&mut bits, 1, 1);
    push(&mut bits, 0, 1);
    push(&mut bits, 0, 1);
    push(&mut bits, 1, 4);
    push(&mut bits, 1, 4);
    push(&mut bits, 2, 4);
    push(&mut bits, 0, 1);
    push(&mut bits, 0, 1);
    push(&mut bits, 0, 2);
    push(&mut bits, 0, 3);
    push(&mut bits, 0, 6);
    push(&mut bits, 0, 2);
    push(&mut bits, 1, 1);
    push(&mut bits, 1, 1);
    push(&mut bits, 0, 1);
    push(&mut bits, 0, 7);
    pack(bits)
}

fn decoder() -> PayloadDecoder {
    PayloadDecoder::new(PayloadDecoderConfig {
        reference_screen: Some(ReferenceScreen {
            bottom_left: openjoc_oamd::Position3 {
                x: 0.1,
                y: 0.0,
                z: -0.5,
            },
            width: 0.8,
            height: 1.0,
        }),
        oamd: OamdDecoderConfig::default(),
    })
}

fn coordinates() -> [BaseFullBandCoordinate; 5] {
    [
        BaseFullBandCoordinate::Left,
        BaseFullBandCoordinate::Right,
        BaseFullBandCoordinate::Centre,
        BaseFullBandCoordinate::LeftSurround,
        BaseFullBandCoordinate::RightSurround,
    ]
}

#[test]
fn bridge_exposes_absolute_timeline_and_hard_unresolved_gate() {
    let joc = absent_joc_payload(0);
    let oamd = inactive_oamd_payload();
    let base = vec![vec![0.0; 64]; 5];
    let mut payload_decoder = decoder();
    let frame = payload_decoder
        .decode_frame(JocFrameInput {
            sample_rate: 48_000,
            downmix_pcm: &base,
            base_lfe_pcm: None,
            joc_payload: &joc,
            oamd_payload: &oamd,
            frame_index: 0,
        })
        .expect("synthetic payload is valid");
    assert_eq!(frame.sample_range.start_sample, 0);
    assert_eq!(frame.sample_range.end_sample, 64);

    let next_joc = absent_joc_payload(1);
    let next_frame = payload_decoder
        .decode_frame(JocFrameInput {
            sample_rate: 48_000,
            downmix_pcm: &base,
            base_lfe_pcm: None,
            joc_payload: &next_joc,
            oamd_payload: &oamd,
            frame_index: 1,
        })
        .expect("second synthetic payload is valid");
    assert_eq!(next_frame.sample_range.start_sample, 64);
    assert_eq!(next_frame.sample_range.end_sample, 128);

    let bridge = JocSpatialBridge;
    let labels = coordinates();
    let spatial = bridge
        .frame(&frame, &labels, &base, None)
        .expect("codec-domain dimensions are valid");
    assert_eq!(spatial.dynamic_metadata_count(), 1);
    assert_eq!(spatial.reconstruction_basis_count(), 1);
    assert!(spatial.operator_state.is_unresolved());
    assert_eq!(
        spatial.semantic_binding,
        openjoc_scene::SemanticBindingState::Unresolved
    );
    assert!(matches!(
        spatial.require_resolved_operator(),
        Err(BridgeError::OperatorUnresolved {
            reason: JocSpatialOperatorUnresolvedReason::ReconstructionEquationNotEstablished
        })
    ));
}

#[test]
fn bridge_rejects_dimension_and_nonfinite_failures_before_output() {
    let joc = absent_joc_payload(0);
    let oamd = inactive_oamd_payload();
    let base = vec![vec![0.0; 64]; 5];
    let frame = decoder()
        .decode_frame(JocFrameInput {
            sample_rate: 48_000,
            downmix_pcm: &base,
            base_lfe_pcm: None,
            joc_payload: &joc,
            oamd_payload: &oamd,
            frame_index: 0,
        })
        .expect("synthetic payload is valid");
    let bridge = JocSpatialBridge;
    let labels = coordinates();
    let too_short = vec![vec![0.0; 63]; 5];
    assert!(matches!(
        bridge.frame(&frame, &labels, &too_short, None),
        Err(BridgeError::BaseFrameLengthMismatch { .. })
    ));
    let mut nonfinite = base.clone();
    nonfinite[2][4] = f64::NAN;
    assert!(matches!(
        bridge.frame(&frame, &labels, &nonfinite, None),
        Err(BridgeError::NonFiniteBase {
            channel: 2,
            sample: 4
        })
    ));
}

#[test]
fn synthetic_operator_is_linear_and_partition_invariant_without_joc_claim() {
    let input = [vec![1.0, 2.0, 3.0, 4.0], vec![5.0, 6.0, 7.0, 8.0]];
    let matrix = [[2.0, -1.0], [0.5, 3.0]];
    let apply = |channels: &[Vec<f64>]| {
        (0..channels[0].len())
            .map(|sample| {
                [
                    matrix[0][0] * channels[0][sample] + matrix[0][1] * channels[1][sample],
                    matrix[1][0] * channels[0][sample] + matrix[1][1] * channels[1][sample],
                ]
            })
            .collect::<Vec<_>>()
    };
    let whole = apply(&input);
    let mut partitioned = apply(
        &input[..]
            .iter()
            .map(|c| c[..2].to_vec())
            .collect::<Vec<_>>(),
    );
    partitioned.extend(apply(
        &input[..]
            .iter()
            .map(|c| c[2..].to_vec())
            .collect::<Vec<_>>(),
    ));
    assert_eq!(whole, partitioned);
    assert_eq!(whole[0], [-3.0, 15.5]);
    assert_eq!(whole[3], [0.0, 26.0]);
}
