use openjoc_oamd::{
    OamdDecoderConfig, OamdElement, OamdError, OamdParseProfile, Position3 as OamdPosition3,
    ReferenceScreen,
};
use openjoc_scene::{
    JocFrameInput, PayloadDecodeError, PayloadDecoder, PayloadDecoderConfig, Position, Position3,
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
    push(&mut bits, 0, 3); // five-channel downmix
    push(&mut bits, 0, 6); // one object
    push(&mut bits, 0, 3); // no extension
    push(&mut bits, 0, 3 + 5); // clip fields
    push(&mut bits, u64::from(sequence_count), 10);
    push(&mut bits, 0, 1); // object side information absent
    pack(bits)
}

fn inactive_oamd_payload() -> Vec<u8> {
    let mut bits = Vec::new();
    push(&mut bits, 0, 2); // syntax version
    push(&mut bits, 0, 5); // one object
    push(&mut bits, 1, 1); // dynamic-only
    push(&mut bits, 0, 1); // no LFE
    push(&mut bits, 0, 1); // no alternate data
    push(&mut bits, 1, 4); // one element
    push(&mut bits, 1, 4); // object element ID
    push(&mut bits, 2, 4); // three-byte body minus one
    push(&mut bits, 0, 1); // variable-size continuation false
    push(&mut bits, 0, 1); // discard unknown false
    push(&mut bits, 0, 2); // sample offset zero
    push(&mut bits, 0, 3); // one metadata block
    push(&mut bits, 0, 6); // block start zero
    push(&mut bits, 0, 2); // no ramp
    push(&mut bits, 1, 1); // reserved data absent
    push(&mut bits, 1, 1); // object inactive
    push(&mut bits, 0, 1); // no additional data
    push(&mut bits, 0, 7); // zero padding to the declared three-byte window
    pack(bits)
}

fn opaque_trim_oamd_payload() -> Vec<u8> {
    let mut bits = Vec::new();
    push(&mut bits, 0, 2); // syntax version
    push(&mut bits, 0, 5); // one object
    push(&mut bits, 1, 1); // dynamic-only
    push(&mut bits, 0, 1); // no LFE
    push(&mut bits, 0, 1); // no alternate data
    push(&mut bits, 2, 4); // two elements
    push(&mut bits, 1, 4); // object element ID
    push(&mut bits, 2, 4); // three-byte body minus one
    push(&mut bits, 0, 1); // variable-size continuation false
    push(&mut bits, 0, 1); // discard unknown false
    push(&mut bits, 0, 2); // sample offset zero
    push(&mut bits, 0, 3); // one metadata block
    push(&mut bits, 0, 6); // block start zero
    push(&mut bits, 0, 2); // no ramp
    push(&mut bits, 1, 1); // reserved data absent
    push(&mut bits, 1, 1); // object inactive
    push(&mut bits, 0, 1); // no additional data
    push(&mut bits, 0, 7); // object body padding
    push(&mut bits, 2, 4); // trim element ID
    push(&mut bits, 0, 4); // one-byte body minus one
    push(&mut bits, 0, 1); // variable-size continuation false
    push(&mut bits, 0b0110_0000, 8); // discard, raw warp 3, remaining opaque bits
    pack(bits)
}

#[test]
fn decodes_raw_payloads_and_downmix_into_an_object_scene() {
    let joc = absent_joc_payload(0);
    let oamd = inactive_oamd_payload();
    let downmix = vec![vec![1.0; 64]; 5];
    let mut decoder = PayloadDecoder::new(PayloadDecoderConfig {
        reference_screen: Some(ReferenceScreen {
            bottom_left: OamdPosition3 {
                x: 0.1,
                y: 0.0,
                z: -0.5,
            },
            width: 0.8,
            height: 1.0,
        }),
        oamd: OamdDecoderConfig::default(),
    });

    let frame_output = decoder
        .decode_frame(JocFrameInput {
            sample_rate: 48_000,
            downmix_pcm: &downmix,
            joc_payload: &joc,
            oamd_payload: &oamd,
            frame_index: 0,
        })
        .expect("valid payload frame");
    assert_eq!(frame_output.joc.header.object_count, 1);
    assert!(
        frame_output.decoded.object_pcm[0]
            .iter()
            .all(|sample| *sample == 0.0)
    );

    let scene = decoder.finish().expect("complete scene");
    assert_eq!(scene.duration_samples, 64);
    assert_eq!(scene.objects[0].pcm, vec![0.0; 64]);
    assert!(!scene.metadata_timeline[0].active);
    assert_eq!(
        scene.metadata_timeline[0].position,
        Position::Room(Position3 {
            x: 0.5,
            y: 0.5,
            z: 0.0,
        })
    );
}

#[test]
fn consumes_a_decoded_frame_through_a_borrowed_sink() {
    let joc = absent_joc_payload(0);
    let oamd = inactive_oamd_payload();
    let downmix = vec![vec![1.0; 64]; 5];
    let mut decoder = PayloadDecoder::new(PayloadDecoderConfig {
        reference_screen: None,
        oamd: OamdDecoderConfig::default(),
    });
    let mut observed = Vec::new();

    decoder
        .decode_frame_with(
            JocFrameInput {
                sample_rate: 48_000,
                downmix_pcm: &downmix,
                joc_payload: &joc,
                oamd_payload: &oamd,
                frame_index: 0,
            },
            |frame| {
                observed.push((
                    frame.joc.header.object_count,
                    frame.decoded.object_pcm.len(),
                ));
                Ok::<(), PayloadDecodeError>(())
            },
        )
        .expect("valid payload frame");

    assert_eq!(observed, vec![(1, 1)]);
    assert_eq!(
        decoder.finish().expect("complete scene").duration_samples,
        64
    );
}

#[test]
fn rejected_payload_frame_does_not_advance_decoder_or_scene_state() {
    let joc_zero = absent_joc_payload(0);
    let joc_one = absent_joc_payload(1);
    let oamd = inactive_oamd_payload();
    let downmix = vec![vec![1.0; 64]; 5];
    let config = PayloadDecoderConfig {
        reference_screen: Some(ReferenceScreen {
            bottom_left: OamdPosition3 {
                x: 0.1,
                y: 0.0,
                z: -0.5,
            },
            width: 0.8,
            height: 1.0,
        }),
        oamd: OamdDecoderConfig::default(),
    };
    let mut decoder = PayloadDecoder::new(config);
    decoder
        .decode_frame(JocFrameInput {
            sample_rate: 48_000,
            downmix_pcm: &downmix,
            joc_payload: &joc_zero,
            oamd_payload: &oamd,
            frame_index: 0,
        })
        .expect("first frame");

    assert!(
        decoder
            .decode_frame(JocFrameInput {
                sample_rate: 48_000,
                downmix_pcm: &downmix,
                joc_payload: &joc_one,
                oamd_payload: &[0],
                frame_index: 1,
            })
            .is_err()
    );
    decoder
        .decode_frame(JocFrameInput {
            sample_rate: 48_000,
            downmix_pcm: &downmix,
            joc_payload: &joc_one,
            oamd_payload: &oamd,
            frame_index: 1,
        })
        .expect("retry same frame");

    let scene = decoder.finish().expect("two committed frames");
    assert_eq!(scene.duration_samples, 128);
    assert_eq!(scene.metadata_timeline[1].start_sample, 64);
}

#[test]
fn explicit_vendor_profile_retains_opaque_trim_and_keeps_joc_chain_separate() {
    let joc = absent_joc_payload(0);
    let oamd = opaque_trim_oamd_payload();
    let downmix = vec![vec![1.0; 64]; 5];
    let config = PayloadDecoderConfig {
        reference_screen: None,
        oamd: OamdDecoderConfig {
            trim_configuration_count: Some(std::num::NonZeroU8::new(1).expect("nonzero")),
        },
    };
    let mut strict = PayloadDecoder::new(config);
    assert!(matches!(
        strict
            .decode_frame(JocFrameInput {
                sample_rate: 48_000,
                downmix_pcm: &downmix,
                joc_payload: &joc,
                oamd_payload: &oamd,
                frame_index: 0,
            })
            .expect_err("strict raw warp 3 rejection"),
        PayloadDecodeError::Oamd(OamdError::ReservedWarpMode { code: 3 })
    ));

    let mut vendor = PayloadDecoder::with_oamd_profile(config, OamdParseProfile::DolbyVendorCompat);
    let frame = vendor
        .decode_frame(JocFrameInput {
            sample_rate: 48_000,
            downmix_pcm: &downmix,
            joc_payload: &joc,
            oamd_payload: &oamd,
            frame_index: 0,
        })
        .expect("vendor opaque trim retention");
    assert!(matches!(
        frame.oamd.elements[1].element,
        OamdElement::OpaqueObservedKnownElement(_)
    ));
    assert_eq!(frame.joc.header.object_count, 1);
    assert!(vendor.finish().is_ok());
}
