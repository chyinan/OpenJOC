use openjoc_api::{OpenJocConfig, OpenJocPacket, OpenJocSession};
use openjoc_ffmpeg::{BridgeErrorKind, FfmpegDecoder, PacketRef, Rational, ReceiveOutcome};
use openjoc_scene::{SpeakerGeometry, SpeakerLayout};

fn config(name: &str, speakers: Vec<SpeakerGeometry>) -> OpenJocConfig {
    OpenJocConfig {
        // Deliberately retain the superseded preset, whose channel count/order
        // must not determine the transport of a validated explicit definition.
        speaker_layout: "5.1".to_owned(),
        speaker_layout_definition: Some(SpeakerLayout::custom(name, speakers).unwrap()),
        ..OpenJocConfig::default()
    }
}

fn reversed_config(name: &str) -> OpenJocConfig {
    config(
        name,
        vec![
            SpeakerGeometry::full_range("FR", 45.0, 0.0),
            SpeakerGeometry::full_range("FL", -45.0, 0.0),
            SpeakerGeometry::full_range("FC", 0.0, 0.0),
            SpeakerGeometry::lfe("LFE", 0.0, -20.0),
            SpeakerGeometry::full_range("Rs", 135.0, 0.0),
            SpeakerGeometry::full_range("Ls", -135.0, 0.0),
        ],
    )
}

#[test]
fn custom_order_overrides_preset_metadata_and_preserves_decoded_pcm() {
    for name in ["studio", "5.1"] {
        let config = reversed_config(name);
        let mut direct = OpenJocSession::new(config.clone()).unwrap();
        let mut bridge = FfmpegDecoder::new(config).unwrap();
        let layout = bridge.channel_layout().clone();
        assert_eq!(layout.name, name);
        assert_eq!(layout.standard_layout, None);
        assert!(layout.custom);
        assert_eq!(layout.openjoc_order, ["FR", "FL", "FC", "LFE", "Rs", "Ls"]);
        assert_eq!(layout.ffmpeg_order, ["FR", "FL", "FC", "LFE", "SR", "SL"]);
        assert_eq!(layout.permutation, [0, 1, 2, 3, 4, 5]);
        let mut expected = Vec::new();
        let mut actual = Vec::new();
        for au in include_bytes!("fixtures/timestamps.ec3")
            .chunks_exact(4096)
            .take(6)
        {
            direct
                .push_packet(OpenJocPacket {
                    data: au,
                    pts_samples: None,
                    discontinuity: false,
                    preroll: false,
                })
                .unwrap();
            bridge
                .send_packet(PacketRef {
                    data: au,
                    pts: None,
                    dts: None,
                    duration: None,
                    time_base: Rational::SAMPLE_TIME_BASE,
                    stream_index: 0,
                    discontinuity: false,
                    preroll: false,
                })
                .unwrap();
            while let Some(frame) = direct.receive_frame() {
                expected.extend(frame.interleaved_f32);
            }
            while let ReceiveOutcome::Frame(frame) = bridge.receive_frame().unwrap() {
                assert_eq!(frame.channel_layout, layout);
                actual.extend(frame.interleaved_f32);
            }
        }
        direct.drain().unwrap();
        bridge.drain().unwrap();
        while let Some(frame) = direct.receive_frame() {
            expected.extend(frame.interleaved_f32);
        }
        while let ReceiveOutcome::Frame(frame) = bridge.receive_frame().unwrap() {
            actual.extend(frame.interleaved_f32);
        }
        assert!(actual.iter().any(|sample| sample.abs() > 0.01));
        assert_eq!(actual, expected);
    }
}

#[test]
fn custom_channel_count_overrides_superseded_preset_count() {
    let bridge = FfmpegDecoder::new(config(
        "pair",
        vec![
            SpeakerGeometry::full_range("FR", 45.0, 0.0),
            SpeakerGeometry::full_range("FL", -45.0, 0.0),
        ],
    ))
    .unwrap();
    assert_eq!(bridge.channel_layout().ffmpeg_order, ["FR", "FL"]);
    assert_eq!(bridge.channel_layout().permutation, [0, 1]);
}

#[test]
fn unsupported_transport_is_rejected_before_any_audio_is_submitted() {
    let cases = [
        vec![
            SpeakerGeometry::full_range("A", -45.0, 0.0),
            SpeakerGeometry::full_range("B", 45.0, 0.0),
        ],
        vec![
            SpeakerGeometry::full_range("Ls", -45.0, 0.0),
            SpeakerGeometry::full_range("SiL", 45.0, 0.0),
        ],
        vec![
            SpeakerGeometry::full_range("FL", -45.0, 0.0),
            SpeakerGeometry::full_range("FR", 45.0, 0.0),
            SpeakerGeometry::lfe("FC", 0.0, -20.0),
        ],
        vec![
            SpeakerGeometry::full_range("FL", -45.0, 0.0),
            SpeakerGeometry::full_range("LFE", 45.0, 0.0),
        ],
    ];
    for speakers in cases {
        let config = config("valid-rust-layout", speakers);
        assert!(OpenJocSession::new(config.clone()).is_ok());
        let error = FfmpegDecoder::new(config).unwrap_err();
        assert_eq!(error.kind, BridgeErrorKind::InvalidConfig);
    }
}

#[cfg(feature = "ffmpeg")]
#[test]
fn native_avframe_preserves_custom_channel_ids_and_pcm_order() {
    use openjoc_ffmpeg::{AvFrame, FfmpegFrame};
    let decoder = FfmpegDecoder::new(reversed_config("studio")).unwrap();
    let pcm = vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6];
    let frame = AvFrame::from_frame(&FfmpegFrame {
        format: "AV_SAMPLE_FMT_FLT",
        sample_rate: 48_000,
        nb_samples: 1,
        pts: None,
        duration: 1,
        channel_layout: decoder.channel_layout().clone(),
        interleaved_f32: pcm.clone(),
    })
    .unwrap();
    // Stable AVChannel IDs: FR, FL, FC, LFE, SR, SL.
    assert_eq!(frame.channel_ids(), [1, 0, 2, 3, 10, 9]);
    assert_eq!(frame.interleaved_f32(), pcm);
}
