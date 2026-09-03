// pattern: Functional Core

use openjoc_api::{
    BinauralConfig, DialnormMode, DownmixPolicy, DrcPolicy, OpenJocConfig, OpenJocPacket,
    OpenJocPcmFrame, OpenJocSession, OpenJocStatus, RenderMode, ValidationProfile,
};
use openjoc_eac3::{AccessUnitParse, parse_access_unit_bounds};
use openjoc_wasm::{Decoder, WasmRenderer};

const FIXTURES: &[&[u8]] = &[
    include_bytes!("../testdata/joc.ec3"),
    include_bytes!("../testdata/joc.lifecycle.ec3"),
];

fn binaural_config(dialnorm: DialnormMode) -> OpenJocConfig {
    OpenJocConfig {
        render_mode: RenderMode::Binaural,
        speaker_layout: String::from("7.1.4"),
        downmix: DownmixPolicy::Auto,
        drc: DrcPolicy::Line,
        dialnorm,
        validation_profile: ValidationProfile::Auto,
        binaural: Some(BinauralConfig::builtin_generic("7.1.4")),
        ..OpenJocConfig::default()
    }
}

fn collect_session(session: &mut OpenJocSession, frames: &mut Vec<OpenJocPcmFrame>) {
    while let Some(frame) = session.receive_frame() {
        frames.push(frame);
    }
}

fn decode_native(fixture: &[u8]) -> Vec<OpenJocPcmFrame> {
    let mut session = OpenJocSession::new(binaural_config(DialnormMode::Default))
        .expect("native binaural session");
    let mut pending = fixture.to_vec();
    let mut frames = Vec::new();
    let mut eos = false;
    while !pending.is_empty() {
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

fn decode_bridge(fixture: &[u8]) -> Vec<OpenJocPcmFrame> {
    let mut decoder =
        Decoder::new_with_dialnorm_and_renderer(DialnormMode::Default, WasmRenderer::Binaural)
            .expect("WASM bridge binaural decoder");
    let mut frames = Vec::new();
    for chunk in fixture.chunks(97) {
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
fn browser_bridge_binaural_pcm_is_bit_identical_to_native_openjoc() {
    for fixture in FIXTURES {
        let native = decode_native(fixture);
        let bridge = decode_bridge(fixture);
        assert_eq!(bridge, native);
        assert!(bridge.iter().all(|frame| {
            frame.sample_rate == 48_000
                && frame.channel_count == 2
                && frame.render_mode == RenderMode::Binaural
        }));
    }
}
