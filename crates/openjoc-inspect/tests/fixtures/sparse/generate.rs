// Run from this directory with rustc +1.95.0 --edition 2024 generate.rs
// and execute the resulting binary to regenerate sparse.ec3.
#[allow(dead_code)]
#[path = "../../support/mod.rs"]
mod support;

fn access_unit(sequence: u64, initial: u64, first_delta: u64, width: u8) -> Vec<u8> {
    let mut bits = Vec::new();
    for (value, size) in [
        (0, 3),
        (0, 6),
        (0, 3), // five inputs, one object, no extension
        (4, 3),
        (0, 5),
        (sequence, 10),
        (1, 1),
        (1, 3),
        (1, 1), // present, three bands, sparse
        (0, 1),
        (0, 1),
        (0, 1), // coarse, smooth, one data point
        (initial, 3),
        (first_delta, width),
        (0, 1), // final delta is zero
    ] {
        support::push(&mut bits, value, size);
    }
    for _ in 0..3 {
        support::push(&mut bits, 0b100, 3); // coarse sparse coefficient delta +1
    }
    let mut frame = support::five_channel_audio_frame(&support::joc_emdf(
        &support::active_object_oamd(true),
        &support::pack(bits),
    ));
    let end = frame.len() - 2;
    let crc = support::crc16(&frame[2..end]);
    frame[end..].copy_from_slice(&crc.to_be_bytes());
    frame
}

fn main() {
    // Five-channel index codewords: 0 selects delta 0, 110 selects delta +1.
    let stream = [access_unit(0, 0, 0, 1), access_unit(1, 3, 0b110, 3)].concat();
    std::fs::write("sparse.ec3", stream).expect("write synthetic fixture");
}
