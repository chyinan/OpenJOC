use openjoc_api::{
    DownmixPolicy, DrcPolicy, OpenJocConfig, OpenJocPacket, OpenJocPcmFrame, OpenJocSession,
    OpenJocStatus, RenderMode, ValidationProfile,
};
use openjoc_eac3::{AccessUnitParse, parse_access_unit_bounds};
use openjoc_wasm::Decoder;

const FIXTURE: &[u8] = include_bytes!("../testdata/joc.ec3");

fn phase0_config() -> OpenJocConfig {
    OpenJocConfig {
        render_mode: RenderMode::Stereo,
        speaker_layout: String::from("2.0"),
        downmix: DownmixPolicy::Auto,
        drc: DrcPolicy::Line,
        validation_profile: ValidationProfile::Auto,
        ..OpenJocConfig::default()
    }
}

fn collect_session(session: &mut OpenJocSession, frames: &mut Vec<OpenJocPcmFrame>) {
    while let Some(frame) = session.receive_frame() {
        frames.push(frame);
    }
}

fn decode_native() -> Vec<OpenJocPcmFrame> {
    let mut session = OpenJocSession::new(phase0_config()).expect("native session");
    let mut pending = FIXTURE.to_vec();
    let mut frames = Vec::new();

    let mut eos = false;
    loop {
        if pending.is_empty() {
            break;
        }
        match parse_access_unit_bounds(&pending, eos).expect("native AU framing") {
            AccessUnitParse::NeedMore => {
                eos = true;
            }
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
    assert!(pending.is_empty());
    session.drain().expect("native drain");
    collect_session(&mut session, &mut frames);
    frames
}

fn decode_bridge() -> Vec<OpenJocPcmFrame> {
    let mut decoder = Decoder::new().expect("bridge decoder");
    let mut frames = Vec::new();
    for chunk in FIXTURE.chunks(97) {
        assert_ne!(
            decoder.push_bytes(chunk),
            openjoc_wasm::DecoderStatus::Error
        );
        while let Some(frame) = decoder.receive_pcm() {
            frames.push(frame);
        }
    }
    loop {
        let status = decoder.flush().expect("bridge drain");
        while let Some(frame) = decoder.receive_pcm() {
            frames.push(frame);
        }
        if status == openjoc_wasm::DecoderStatus::EndOfStream {
            break;
        }
    }
    frames
}

#[test]
fn browser_bridge_pcm_is_bit_identical_to_native_openjoc_stereo() {
    assert_eq!(decode_bridge(), decode_native());
}
