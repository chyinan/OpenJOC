//! Object positions are a visualisation side channel: opting in must not
//! change a single PCM sample, and every record must be a normalised
//! room-cube coordinate.

use openjoc_wasm::{Decoder, DecoderStatus, OBJECT_POSITION_STRIDE};

const FIXTURE: &[u8] = include_bytes!("../testdata/joc.ec3");

fn drain(decoder: &mut Decoder, values: &mut Vec<f64>, pcm: &mut Vec<f32>) {
    while let Some(frame) = decoder.receive_pcm() {
        pcm.extend_from_slice(&frame.interleaved_f32);
    }
    values.extend_from_slice(decoder.object_positions());
    decoder.clear_object_positions();
}

/// Mirrors the browser wrapper: 4 KiB pushes, drain after each, then flush.
fn decode(enable_positions: bool) -> (Vec<f64>, Vec<f32>) {
    let mut decoder = Decoder::new().expect("decoder");
    if enable_positions {
        decoder.enable_object_positions();
    }
    let mut values = Vec::new();
    let mut pcm = Vec::new();
    for chunk in FIXTURE.chunks(4096) {
        assert_ne!(decoder.push_bytes(chunk), DecoderStatus::Error);
        drain(&mut decoder, &mut values, &mut pcm);
    }
    let mut ended = false;
    for _ in 0..64 {
        let status = decoder.flush().expect("flush");
        drain(&mut decoder, &mut values, &mut pcm);
        if status == DecoderStatus::EndOfStream {
            ended = true;
            break;
        }
    }
    assert!(ended, "decoder never reached end of stream");
    (values, pcm)
}

#[test]
fn positions_are_opt_in_and_leave_pcm_unchanged() {
    let (without, pcm_without) = decode(false);
    let (with, pcm_with) = decode(true);
    assert!(without.is_empty(), "positions must stay off unless enabled");
    assert!(!pcm_without.is_empty());
    assert_eq!(
        pcm_with, pcm_without,
        "enabling positions changed the audio"
    );
    assert!(
        !with.is_empty(),
        "fixture produced no dynamic-object positions"
    );
    assert_eq!(with.len() % OBJECT_POSITION_STRIDE, 0);
}

#[test]
fn positions_are_normalised_room_coordinates_in_time_order() {
    let (values, _) = decode(true);
    let mut last_start = 0.0_f64;
    for record in values.chunks_exact(OBJECT_POSITION_STRIDE) {
        let [index, start, ramp, x, y, z, active] = record else {
            unreachable!()
        };
        assert!(*index >= 0.0 && index.fract() == 0.0, "index {index}");
        assert!(*start >= last_start, "start {start} went backwards");
        last_start = *start;
        assert!(*ramp >= 0.0);
        assert!((0.0..=1.0).contains(x), "x {x}");
        assert!((0.0..=1.0).contains(y), "y {y}");
        // Standard z is ±1; the ID 5 precision extension may add a little.
        assert!((-1.25..=1.25).contains(z), "z {z}");
        assert!(*active == 0.0 || *active == 1.0);
    }
}
