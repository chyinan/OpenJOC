//! J1R27 parser-level SPX reuse, replacement, disable and reset regressions.
//!
//! The bit writer below is a bounded public-syntax fixture builder.  It does
//! not model a real encoder or a real carrier; it exercises the same parser
//! entry point used by production decoding and makes each state transition
//! explicit in the test vector.

use openjoc_eac3::decode_audio_blocks;

#[derive(Clone, Default)]
struct Bits(Vec<bool>);

impl Bits {
    fn push(&mut self, value: u64, width: u8) {
        for shift in (0..width).rev() {
            self.0.push((value >> shift) & 1 != 0);
        }
    }

    fn bytes(mut self, length: usize) -> Vec<u8> {
        self.0.resize(length * 8, false);
        self.0
            .chunks(8)
            .map(|chunk| {
                chunk
                    .iter()
                    .fold(0_u8, |value, bit| (value << 1) | u8::from(*bit))
            })
            .collect()
    }
}

const A_COORDINATES: [(u8, u8); 4] = [(1, 0), (2, 1), (3, 2), (4, 3)];
const B_COORDINATES: [(u8, u8); 4] = [(12, 3), (11, 2), (10, 1), (9, 0)];
const BAND_STRUCTURE: [bool; 6] = [false, true, false, true, true, false];

fn spx_config(bits: &mut Bits, coordinates: [(u8, u8); 4], structure_exists: bool) {
    bits.push(2, 2); // spxstrtf
    bits.push(0, 3); // spxbegf => begin sub-band 2
    bits.push(3, 3); // spxendf => end sub-band 8
    bits.push(u64::from(structure_exists), 1); // spxbndstrce
    if structure_exists {
        for value in BAND_STRUCTURE {
            bits.push(u64::from(value), 1);
        }
    }
    bits.push(17, 5); // spxblnd
    bits.push(2, 2); // mstrspxco
    for (exponent, mantissa) in coordinates {
        bits.push(u64::from(exponent), 4);
        bits.push(u64::from(mantissa), 2);
    }
}

fn block_zero(bits: &mut Bits, coordinates: [(u8, u8); 4]) {
    bits.push(0, 1); // dynamic range absent
    bits.push(1, 1); // spxinu: enabled (spxstre is implicit in block zero)
    spx_config(bits, coordinates, true);
    bits.push(10, 4); // channel absolute exponent
    for _ in 0..16 {
        bits.push(62, 7); // neutral D15 groups through SPX begin bin 49
    }
    bits.push(1, 2); // gain range
    bits.push(0, 1); // converter SNR offset absent
}

fn block_reuse(bits: &mut Bits) {
    block_reuse_with_start(bits, 2);
}

fn block_reuse_with_start(bits: &mut Bits, start_copy_frequency_code: u8) {
    bits.push(0, 1); // dynamic range absent
    bits.push(1, 1); // spxstre: new block syntax follows
    bits.push(1, 1); // spxinu: enabled
    spx_config_header_with_start(bits, start_copy_frequency_code, false);
    bits.push(0, 1); // spxcoe: reuse previous channel coordinates
    bits.push(0, 1); // converter SNR offset absent
}

fn spx_config_header_with_start(
    bits: &mut Bits,
    start_copy_frequency_code: u8,
    structure_exists: bool,
) {
    bits.push(u64::from(start_copy_frequency_code), 2); // spxstrtf
    bits.push(0, 3); // spxbegf
    bits.push(3, 3); // spxendf
    bits.push(u64::from(structure_exists), 1); // spxbndstrce
    if structure_exists {
        for value in BAND_STRUCTURE {
            bits.push(u64::from(value), 1);
        }
    }
}

fn block_replace(bits: &mut Bits, coordinates: [(u8, u8); 4]) {
    bits.push(0, 1); // dynamic range absent
    bits.push(1, 1); // spxstre
    bits.push(1, 1); // spxinu
    spx_config_header_with_start(bits, 1, true);
    bits.push(1, 1); // spxcoe: explicit replacement coordinates
    bits.push(17, 5); // spxblnd
    bits.push(2, 2); // mstrspxco
    for (exponent, mantissa) in coordinates {
        bits.push(u64::from(exponent), 4);
        bits.push(u64::from(mantissa), 2);
    }
    bits.push(0, 1); // converter SNR offset absent
}

fn block_reenable(bits: &mut Bits, coordinates: [(u8, u8); 4]) {
    bits.push(0, 1); // dynamic range absent
    bits.push(1, 1); // spxstre
    bits.push(1, 1); // spxinu
    // firstspxcos is set by the preceding disable, so spxcoe is implicit 1.
    spx_config(bits, coordinates, true);
    bits.push(0, 1); // converter SNR offset absent
}

fn block_disable(bits: &mut Bits) {
    bits.push(0, 1); // dynamic range absent
    bits.push(1, 1); // spxstre
    bits.push(0, 1); // spxinu: disabled for this block
    bits.push(0, 1); // converter SNR offset absent
}

fn six_block_header(bits: &mut Bits) {
    bits.push(0x0b77, 16);
    bits.push(0, 2); // independent
    bits.push(0, 3); // substream id
    bits.push(2047, 11); // 4096-byte frame
    bits.push(0, 2); // 48 kHz
    bits.push(3, 2); // six audio blocks
    bits.push(1, 3); // mono
    bits.push(0, 1); // no LFE
    bits.push(16, 5); // E-AC-3 version
    bits.push(31, 5); // dialnorm
    bits.push(0, 1); // no compression metadata
    bits.push(0, 1); // no mixing metadata
    bits.push(0, 1); // no informational metadata
    bits.push(0, 1); // no addbsi

    bits.push(1, 1); // exponent strategy exists
    bits.push(0, 1); // AHT syntax absent
    bits.push(0, 2); // frame SNR strategy
    bits.push(0, 1); // transient processing
    bits.push(0, 7); // block-switch/dither/BA/fast/delta/skip/SPX attenuation syntax
    bits.push(1, 2); // block 0 channel D15
    for _ in 1..6 {
        bits.push(0, 2); // following blocks reuse channel exponent strategy
    }
    bits.push(0, 5); // converter exponent strategy
    bits.push(0, 6); // frame coarse SNR offset
    bits.push(0, 4); // frame fine SNR offset
    bits.push(0, 1); // no block-start information
}

fn six_block_spx_frame() -> Vec<u8> {
    let mut bits = Bits::default();
    six_block_header(&mut bits);
    block_zero(&mut bits, A_COORDINATES);
    block_reuse(&mut bits);
    block_reuse(&mut bits);
    block_replace(&mut bits, B_COORDINATES);
    block_reuse_with_start(&mut bits, 1);
    block_disable(&mut bits);

    bits.bytes(4096)
}

fn six_block_no_spx_then_reuse() -> Vec<u8> {
    let mut bits = Bits::default();
    six_block_header(&mut bits);
    bits.push(0, 1); // block 0 dynamic range absent
    bits.push(0, 1); // block 0: SPX not in use
    bits.push(0, 6); // chbwcod = 0 => end mantissa 73
    bits.push(15, 4); // channel absolute exponent
    for _ in 0..24 {
        bits.push(62, 7); // neutral D15 groups through mantissa 73
    }
    bits.push(1, 2); // gain range
    bits.push(0, 1); // converter SNR offset absent
    bits.push(0, 1); // block 1 dynamic range absent
    bits.push(0, 1); // block 1: spxstre=0 without a previous SPX state
    bits.bytes(4096)
}

fn six_block_disable_reenable_frame() -> Vec<u8> {
    let mut bits = Bits::default();
    six_block_header(&mut bits);
    block_zero(&mut bits, A_COORDINATES);
    block_disable(&mut bits);
    block_reenable(&mut bits, B_COORDINATES);
    block_reuse(&mut bits);
    block_reuse(&mut bits);
    block_reuse(&mut bits);
    bits.bytes(4096)
}

#[test]
fn parser_preserves_reuses_replaces_and_disables_spx_state_by_block() {
    let bytes = six_block_spx_frame();
    let blocks = decode_audio_blocks(&bytes, &[0.5; 6 * 49]).expect("six-block SPX sequence");
    assert_eq!(blocks.len(), 6);

    let a = blocks[0]
        .prefix
        .spectral_extension
        .clone()
        .expect("explicit state A");
    assert_eq!(blocks[1].prefix.spectral_extension, Some(a.clone()));
    assert_eq!(blocks[2].prefix.spectral_extension, Some(a.clone()));

    let b = blocks[3]
        .prefix
        .spectral_extension
        .clone()
        .expect("explicit replacement state B");
    assert_ne!(a, b);
    assert_eq!(blocks[4].prefix.spectral_extension, Some(b));
    assert!(blocks[5].prefix.spectral_extension.is_none());
}

#[test]
fn inactive_block_without_prior_spx_does_not_resurrect_state() {
    let blocks = decode_audio_blocks(&six_block_no_spx_then_reuse(), &[0.5; 6 * 73])
        .expect("inactive SPX sequence");
    assert!(
        blocks
            .iter()
            .all(|block| block.prefix.spectral_extension.is_none())
    );
}

#[test]
fn disable_requires_fresh_state_on_reenable_and_state_is_frame_local() {
    let blocks = decode_audio_blocks(&six_block_disable_reenable_frame(), &[0.5; 6 * 49])
        .expect("disable/re-enable sequence");
    assert!(blocks[1].prefix.spectral_extension.is_none());
    let reenabled = blocks[2]
        .prefix
        .spectral_extension
        .as_ref()
        .expect("explicit state after disable");
    assert_eq!(reenabled.start_copy_frequency_code, 2);
    assert_eq!(blocks[3].prefix.spectral_extension, Some(reenabled.clone()));

    let next_frame = decode_audio_blocks(&six_block_no_spx_then_reuse(), &[0.5; 6 * 73])
        .expect("independent next frame");
    assert!(
        next_frame
            .iter()
            .all(|block| block.prefix.spectral_extension.is_none())
    );
}

#[test]
fn repeated_state_sequences_have_stable_bounded_results() {
    let bytes = six_block_spx_frame();
    let first = decode_audio_blocks(&bytes, &[0.5; 6 * 49]).expect("first sequence");
    for _ in 0..256 {
        let repeated = decode_audio_blocks(&bytes, &[0.5; 6 * 49]).expect("repeated sequence");
        assert_eq!(repeated, first);
    }
}
