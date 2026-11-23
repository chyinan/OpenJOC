// pattern: Imperative Shell

//! Discrete speaker export through the browser bridge: the decoded programme
//! channels must reach the caller unmodified, not downmixed to stereo.

use openjoc_api::{
    DialnormMode, DownmixPolicy, DrcPolicy, OpenJocConfig, OpenJocPacket, OpenJocPcmFrame,
    OpenJocSession, OpenJocStatus, RenderMode, ValidationProfile,
};
use openjoc_eac3::{AccessUnitParse, parse_access_unit_bounds};
use openjoc_wasm::{Decoder, DecoderStatus, speaker_layout_for_code};

const FIXTURE: &[u8] = include_bytes!("../testdata/joc.ec3");

fn collect_session(session: &mut OpenJocSession, frames: &mut Vec<OpenJocPcmFrame>) {
    while let Some(frame) = session.receive_frame() {
        frames.push(frame);
    }
}

fn decode_native_speaker(layout: &str) -> Vec<OpenJocPcmFrame> {
    let config = OpenJocConfig {
        render_mode: RenderMode::Speaker,
        speaker_layout: layout.to_owned(),
        downmix: DownmixPolicy::Auto,
        drc: DrcPolicy::Line,
        validation_profile: ValidationProfile::Auto,
        ..OpenJocConfig::default()
    };
    let mut session = OpenJocSession::new(config).expect("native speaker session");
    let mut pending = FIXTURE.to_vec();
    let mut frames = Vec::new();
    let mut eos = false;
    loop {
        if pending.is_empty() {
            break;
        }
        match parse_access_unit_bounds(&pending, eos).expect("native AU framing") {
            AccessUnitParse::NeedMore => eos = true,
            AccessUnitParse::Complete(length) => {
                let packet: Vec<u8> = pending.drain(..length).collect();
                let status = session
                    .push_packet(OpenJocPacket {
                        data: &packet,
                        pts_samples: None,
                        discontinuity: false,
                        preroll: false,
                    })
                    .expect("native packet");
                assert_ne!(status, OpenJocStatus::OutputPending);
                collect_session(&mut session, &mut frames);
                eos = false;
            }
        }
    }
    session.drain().expect("native drain");
    collect_session(&mut session, &mut frames);
    frames
}

fn decode_bridge_speaker(layout: &'static str) -> Vec<OpenJocPcmFrame> {
    let mut decoder =
        Decoder::new_with_speaker_layout(DialnormMode::Default, layout).expect("speaker decoder");
    let mut frames = Vec::new();
    for chunk in FIXTURE.chunks(97) {
        assert_ne!(decoder.push_bytes(chunk), DecoderStatus::Error);
        while let Some(frame) = decoder.receive_pcm() {
            frames.push(frame);
        }
    }
    loop {
        let status = decoder.flush().expect("bridge drain");
        while let Some(frame) = decoder.receive_pcm() {
            frames.push(frame);
        }
        if status == DecoderStatus::EndOfStream {
            break;
        }
    }
    frames
}

#[test]
fn preset_codes_are_stable_and_reject_unknown_codes() {
    assert_eq!(speaker_layout_for_code(0), Some("5.1"));
    assert_eq!(speaker_layout_for_code(1), Some("7.1"));
    assert_eq!(speaker_layout_for_code(3), Some("7.1.4"));
    assert_eq!(speaker_layout_for_code(5), Some("22.2"));
    assert_eq!(speaker_layout_for_code(6), None);
    assert_eq!(speaker_layout_for_code(u32::MAX), None);
}

#[test]
fn speaker_bridge_emits_one_pcm_frame_per_programme_channel() {
    let frames = decode_bridge_speaker("5.1");
    assert!(!frames.is_empty(), "fixture produced no PCM");
    for frame in &frames {
        assert_eq!(frame.channel_count, 6, "5.1 must carry six channels");
        assert_eq!(
            frame.interleaved_f32.len(),
            frame.sample_count * frame.channel_count,
            "interleaved buffer must cover every channel"
        );
    }
}

#[test]
fn speaker_bridge_pcm_is_bit_identical_to_native_openjoc_speaker() {
    assert_eq!(decode_bridge_speaker("5.1"), decode_native_speaker("5.1"));
}

#[test]
fn speaker_output_differs_from_the_stereo_downmix() {
    let speaker = decode_bridge_speaker("5.1");
    let mut stereo = Decoder::new().expect("stereo decoder");
    let mut stereo_frames = Vec::new();
    for chunk in FIXTURE.chunks(97) {
        assert_ne!(stereo.push_bytes(chunk), DecoderStatus::Error);
        while let Some(frame) = stereo.receive_pcm() {
            stereo_frames.push(frame);
        }
    }
    loop {
        let status = stereo.flush().expect("stereo drain");
        while let Some(frame) = stereo.receive_pcm() {
            stereo_frames.push(frame);
        }
        if status == DecoderStatus::EndOfStream {
            break;
        }
    }
    assert_eq!(stereo_frames[0].channel_count, 2);
    assert_ne!(speaker[0].channel_count, stereo_frames[0].channel_count);

    // A discrete centre channel is the concrete difference: the stereo downmix
    // folds FC into L/R and therefore cannot carry it as its own channel. Later
    // frames are used because the first ones are latency-compensated silence.
    let centre = speaker
        .iter()
        .flat_map(|frame| {
            frame
                .interleaved_f32
                .chunks_exact(frame.channel_count)
                .map(move |channels| channels[2])
        })
        .any(|sample| sample.abs() > 1.0e-6);
    assert!(centre, "the fifth-channel programme kept no centre energy");
    assert!(stereo_frames.iter().all(|frame| frame.channel_count == 2));
}

#[test]
fn status_reports_discrete_speaker_rendering_and_channel_count() {
    let mut decoder =
        Decoder::new_with_speaker_layout(DialnormMode::Default, "5.1").expect("speaker decoder");
    for chunk in FIXTURE.chunks(97) {
        assert_ne!(decoder.push_bytes(chunk), DecoderStatus::Error);
        while decoder.receive_pcm().is_some() {}
    }
    let _ = decoder.flush();
    let status = decoder.status();
    assert_eq!(status.renderer, "Speaker (Discrete)");
    assert_eq!(status.virtual_layout, Some("5.1"));
    assert_eq!(status.output_channels, 6);
    assert_eq!(status.sample_rate, Some(48_000));
}
