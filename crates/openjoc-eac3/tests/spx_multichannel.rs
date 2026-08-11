//! J1R28 multi-channel SPX participation, isolation, and error regressions.
//!
//! These are bounded synthetic public-syntax frames. They exercise the
//! production audio-block parser and keep real-carrier and full-PCM fidelity
//! claims outside this test's scope.

use openjoc_bitio::BitError;
use openjoc_eac3::{
    Eac3Error, InternalBasePolicy, SpectralExtensionCoordinates, SpectralExtensionInformation,
    decode_audio_blocks, decode_audio_blocks_with_diagnostic_trace,
    decode_audio_blocks_with_parsed_frame, parse_audio_frame, parse_first_audio_block_prefix,
    synthesize_spectral_extension,
};

#[derive(Clone, Default)]
struct Bits(Vec<bool>);

impl Bits {
    fn push(&mut self, value: u64, width: u8) {
        for shift in (0..width).rev() {
            self.0.push((value >> shift) & 1 != 0);
        }
    }

    fn len(&self) -> usize {
        self.0.len()
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

const STATE_A: [(u8, u8); 4] = [(1, 0), (2, 1), (3, 2), (4, 3)];
const STATE_B: [(u8, u8); 4] = [(12, 3), (11, 2), (10, 1), (9, 0)];
const STATE_C: [(u8, u8); 4] = [(5, 1), (6, 2), (7, 3), (8, 0)];
const STATE_D: [(u8, u8); 4] = [(14, 0), (13, 1), (12, 2), (11, 3)];
const BAND_STRUCTURE: [bool; 6] = [false, true, false, true, true, false];

fn coordinates(bits: &mut Bits, values: [(u8, u8); 4]) {
    bits.push(17, 5); // spxblnd
    bits.push(2, 2); // mstrspxco
    for (exponent, mantissa) in values {
        bits.push(u64::from(exponent), 4);
        bits.push(u64::from(mantissa), 2);
    }
}

fn shared_spx_config(bits: &mut Bits, structure_exists: bool) {
    bits.push(1, 2); // spxstrtf: copy sub-band below begin sub-band
    bits.push(0, 3); // spxbegf => begin sub-band 2
    bits.push(3, 3); // spxendf => end sub-band 9
    bits.push(u64::from(structure_exists), 1); // spxbndstrce
    if structure_exists {
        for value in BAND_STRUCTURE {
            bits.push(u64::from(value), 1);
        }
    }
}

fn stereo_six_block_header(bits: &mut Bits) {
    bits.push(0x0b77, 16);
    bits.push(0, 2); // independent
    bits.push(0, 3); // substream id
    bits.push(2047, 11); // 4096-byte frame
    bits.push(0, 2); // 48 kHz
    bits.push(3, 2); // six audio blocks
    bits.push(2, 3); // stereo
    bits.push(0, 1); // no LFE
    bits.push(16, 5); // E-AC-3 version
    bits.push(31, 5); // dialnorm
    bits.push(0, 1); // no compression metadata
    bits.push(0, 1); // no mixing metadata
    bits.push(0, 1); // no informational metadata
    bits.push(0, 1); // no addbsi

    bits.push(1, 1); // per-block exponent strategies
    bits.push(0, 1); // AHT syntax absent
    bits.push(0, 2); // frame SNR strategy
    bits.push(0, 1); // transient processing absent
    bits.push(0, 7); // compact syntax flags

    bits.push(0, 1); // block 0 coupling not in use
    for _ in 1..6 {
        bits.push(0, 1); // following blocks reuse coupling-off state
    }
    for block in 0..6 {
        for _ in 0..2 {
            bits.push(u64::from(block == 0), 2); // D15 then reuse
        }
    }
    bits.push(0, 10); // converter exponent strategies for two channels
    bits.push(0, 6); // frame coarse SNR offset
    bits.push(0, 4); // frame fine SNR offset
    bits.push(0, 1); // no block-start information
}

fn first_stereo_spx_block(
    bits: &mut Bits,
    participants: [bool; 2],
    values: [Option<[(u8, u8); 4]>; 2],
) -> [usize; 3] {
    bits.push(1, 1); // dynamic-range word present: aligns truncation boundaries
    bits.push(0, 8); // unity dynamic range
    bits.push(1, 1); // spxinu; strategy is implicit in block zero
    let participation_bit = bits.len();
    for participant in participants {
        bits.push(u64::from(participant), 1); // chinspx[ch]
    }
    shared_spx_config(bits, true);
    let first_coordinate_bit = bits.len();
    for value in values.into_iter().flatten() {
        coordinates(bits, value);
    }
    let second_coordinate_bit = if participants[0] {
        first_coordinate_bit + 31
    } else {
        first_coordinate_bit
    };
    for _ in 0..3 {
        bits.push(0, 1); // stereo rematrix flags
    }
    for participant in participants {
        if !participant {
            bits.push(0, 6); // non-SPX channel bandwidth: 73 mantissas
        }
    }
    for participant in participants {
        bits.push(if participant { 10 } else { 15 }, 4); // channel absolute exponent
        let groups = if participant { 16 } else { 24 };
        for _ in 0..groups {
            bits.push(62, 7); // neutral D15 groups through active bandwidth
        }
        bits.push(1, 2); // gain range
    }
    bits.push(0, 1); // converter SNR offset absent
    [
        participation_bit,
        first_coordinate_bit,
        second_coordinate_bit,
    ]
}

fn following_spx_header(bits: &mut Bits, participants: [bool; 2]) {
    bits.push(0, 1); // dynamic range absent
    bits.push(1, 1); // spxstre
    bits.push(1, 1); // spxinu
    for participant in participants {
        bits.push(u64::from(participant), 1); // chinspx[ch]
    }
    shared_spx_config(bits, false);
}

fn following_block_tail(bits: &mut Bits) {
    bits.push(0, 1); // rematrix flags reused
    bits.push(0, 1); // converter SNR offset absent
}

fn stereo_multichannel_frame() -> Vec<u8> {
    let mut bits = Bits::default();
    stereo_six_block_header(&mut bits);

    first_stereo_spx_block(&mut bits, [true, true], [Some(STATE_A), Some(STATE_B)]);

    following_spx_header(&mut bits, [true, true]);
    bits.push(0, 1); // A reuses A
    bits.push(0, 1); // B reuses B
    following_block_tail(&mut bits);

    following_spx_header(&mut bits, [true, false]);
    bits.push(1, 1); // A receives explicit C
    coordinates(&mut bits, STATE_C);
    following_block_tail(&mut bits);

    following_spx_header(&mut bits, [false, true]);
    // B was absent in block 2, so firstspxcos[B] makes spxcoe implicit 1.
    coordinates(&mut bits, STATE_D);
    following_block_tail(&mut bits);

    following_spx_header(&mut bits, [true, true]);
    // A was absent in block 3 and must be explicit; B can reuse D.
    coordinates(&mut bits, STATE_C);
    bits.push(0, 1); // B reuses D
    following_block_tail(&mut bits);

    following_spx_header(&mut bits, [true, true]);
    bits.push(0, 1); // A reuses C
    bits.push(1, 1); // B receives explicit replacement B
    coordinates(&mut bits, STATE_B);
    following_block_tail(&mut bits);

    bits.bytes(4096)
}

fn stereo_first_block_fixture() -> (Vec<u8>, [usize; 3]) {
    let mut bits = Bits::default();
    stereo_six_block_header(&mut bits);
    let markers = first_stereo_spx_block(&mut bits, [true, true], [Some(STATE_A), Some(STATE_B)]);
    (bits.bytes(4096), markers)
}

fn stereo_a_only_first_block_fixture() -> Vec<u8> {
    let mut bits = Bits::default();
    stereo_six_block_header(&mut bits);
    first_stereo_spx_block(&mut bits, [true, false], [Some(STATE_A), None]);
    bits.bytes(4096)
}

fn stereo_disabled_first_block_fixture() -> Vec<u8> {
    let mut bits = Bits::default();
    stereo_six_block_header(&mut bits);
    bits.push(0, 1); // dynamic range absent
    bits.push(0, 1); // spxinu=0
    for _ in 0..4 {
        bits.push(0, 1); // stereo rematrix flags without SPX
    }
    for _ in 0..2 {
        bits.push(0, 6); // channel bandwidth: 73 mantissas
    }
    for _ in 0..2 {
        bits.push(15, 4); // channel absolute exponent
        for _ in 0..24 {
            bits.push(62, 7); // neutral D15 groups through mantissa 73
        }
        bits.push(1, 2); // gain range
    }
    bits.push(0, 1); // converter SNR offset absent
    bits.bytes(4096)
}

fn set_bit(bytes: &mut [u8], bit: usize, value: bool) {
    let mask = 0x80 >> (bit % 8);
    if value {
        bytes[bit / 8] |= mask;
    } else {
        bytes[bit / 8] &= !mask;
    }
}

fn declared_prefix(bytes: &[u8], byte_len: usize) -> Vec<u8> {
    assert!(byte_len >= 4 && byte_len <= bytes.len() && byte_len % 2 == 0);
    let mut truncated = bytes[..byte_len].to_vec();
    let frame_size_code = u16::try_from(byte_len / 2 - 1).expect("bounded frame size");
    for index in 0..11 {
        let value = (frame_size_code >> (10 - index)) & 1 != 0;
        set_bit(&mut truncated, 21 + index, value);
    }
    truncated
}

fn coordinate(
    value: &SpectralExtensionInformation,
    channel: usize,
) -> &SpectralExtensionCoordinates {
    value.coordinates[channel]
        .as_ref()
        .expect("participating channel coordinate")
}

fn assert_coordinate(actual: &SpectralExtensionCoordinates, expected: [(u8, u8); 4]) {
    assert_eq!(actual.blend, 17);
    assert_eq!(actual.master, 2);
    assert_eq!(actual.bands, expected);
}

#[test]
fn parser_isolates_multichannel_spx_state_across_participation_changes() {
    let bytes = stereo_multichannel_frame();
    let blocks = decode_audio_blocks(&bytes, &[0.5; 6 * 2 * 49])
        .expect("bounded stereo multi-channel SPX sequence");
    assert_eq!(blocks.len(), 6);

    let states = blocks
        .iter()
        .map(|block| block.prefix.spectral_extension.as_ref().expect("SPX state"))
        .collect::<Vec<_>>();

    assert_eq!(states[0].channel_in_use, vec![true, true]);
    assert_coordinate(coordinate(states[0], 0), STATE_A);
    assert_coordinate(coordinate(states[0], 1), STATE_B);
    assert_eq!(states[1].coordinates, states[0].coordinates);

    assert_eq!(states[2].channel_in_use, vec![true, false]);
    assert_coordinate(coordinate(states[2], 0), STATE_C);
    assert!(states[2].coordinates[1].is_none());

    assert_eq!(states[3].channel_in_use, vec![false, true]);
    assert!(states[3].coordinates[0].is_none());
    assert_coordinate(coordinate(states[3], 1), STATE_D);

    assert_eq!(states[4].channel_in_use, vec![true, true]);
    assert_coordinate(coordinate(states[4], 0), STATE_C);
    assert_coordinate(coordinate(states[4], 1), STATE_D);

    assert_eq!(states[5].channel_in_use, vec![true, true]);
    assert_coordinate(coordinate(states[5], 0), STATE_C);
    assert_coordinate(coordinate(states[5], 1), STATE_B);

    for state in &states[1..] {
        assert_eq!(state.begin_subband, states[0].begin_subband);
        assert_eq!(state.end_subband, states[0].end_subband);
        assert_eq!(state.band_structure, states[0].band_structure);
        assert_eq!(state.band_count, states[0].band_count);
    }
}

#[test]
fn parser_distinguishes_stereo_disabled_and_first_a_only_participation() {
    let disabled = parse_first_audio_block_prefix(&stereo_disabled_first_block_fixture())
        .expect("stereo SPX-disabled prefix");
    assert!(disabled.spectral_extension.is_none());

    let a_only = parse_first_audio_block_prefix(&stereo_a_only_first_block_fixture())
        .expect("stereo A-only SPX prefix");
    let state = a_only.spectral_extension.expect("A-only SPX state");
    assert_eq!(state.channel_in_use, vec![true, false]);
    assert_coordinate(coordinate(&state, 0), STATE_A);
    assert!(state.coordinates[1].is_none());
    assert_eq!(a_only.channel_bandwidth_codes, vec![None, Some(0)]);
}

#[test]
fn preparsed_and_diagnostic_paths_preserve_identical_multichannel_spx_state() {
    let bytes = stereo_multichannel_frame();
    let dither = [0.5; 6 * 2 * 49];
    let direct = decode_audio_blocks(&bytes, &dither).expect("direct decode");
    let frame = parse_audio_frame(&bytes).expect("frame syntax");
    let preparsed = decode_audio_blocks_with_parsed_frame(
        &bytes,
        &frame,
        &dither,
        InternalBasePolicy::CurrentDefault,
    )
    .expect("preparsed decode");
    let mut trace = Vec::new();
    let diagnostic = decode_audio_blocks_with_diagnostic_trace(
        &bytes,
        &dither,
        InternalBasePolicy::CurrentDefault,
        &mut trace,
    )
    .expect("diagnostic decode");
    assert_eq!(preparsed, direct);
    assert_eq!(diagnostic, direct);
}

#[test]
fn parser_rejects_truncated_participation_and_per_channel_coordinates() {
    let (bytes, markers) = stereo_first_block_fixture();
    assert_eq!(markers, [127, 144, 175]);

    for (label, byte_len) in [
        ("second participation flag", 16),
        ("first channel coordinate", 18),
        ("second channel coordinate", 22),
    ] {
        let error =
            parse_first_audio_block_prefix(&declared_prefix(&bytes, byte_len)).expect_err(label);
        assert!(
            matches!(error, Eac3Error::Bit(BitError::EndOfInput { .. })),
            "{label}: {error:?}"
        );
    }
}

#[test]
fn failed_multichannel_spx_parse_cannot_poison_a_fresh_decode() {
    let valid = stereo_multichannel_frame();
    let baseline = decode_audio_blocks(&valid, &[0.5; 6 * 2 * 49]).expect("baseline");
    let (first_block, _) = stereo_first_block_fixture();
    let malformed = declared_prefix(&first_block, 22);
    assert!(matches!(
        parse_first_audio_block_prefix(&malformed),
        Err(Eac3Error::Bit(BitError::EndOfInput { .. }))
    ));
    let recovered = decode_audio_blocks(&valid, &[0.5; 6 * 2 * 49]).expect("fresh recovery");
    assert_eq!(recovered, baseline);
}

#[test]
fn invalid_spx_coordinate_dimensions_are_checked_without_indexing() {
    let blocks = decode_audio_blocks(&stereo_multichannel_frame(), &[0.5; 6 * 2 * 49])
        .expect("multi-channel state");
    let information = blocks[0]
        .prefix
        .spectral_extension
        .as_ref()
        .expect("SPX information");
    let mut wrong = coordinate(information, 0).clone();
    wrong.bands.pop();
    assert_eq!(
        synthesize_spectral_extension(&vec![0.25; 49], information, &wrong, None, &vec![0.0; 180]),
        Err(Eac3Error::InvalidSpectralExtensionCoordinateDimensions {
            expected: 4,
            actual: 3,
        })
    );
}

#[test]
fn repeated_multichannel_participation_sequences_are_exact_and_bounded() {
    let bytes = stereo_multichannel_frame();
    let dither = [0.5; 6 * 2 * 49];
    let expected = decode_audio_blocks(&bytes, &dither).expect("initial sequence");
    for _ in 0..256 {
        assert_eq!(
            decode_audio_blocks(&bytes, &dither).expect("repeated sequence"),
            expected
        );
    }

    // A separately parsed frame starts from explicit coordinates again; no
    // channel state from the prior call is reachable across the frame/API boundary.
    assert_eq!(
        decode_audio_blocks(&bytes, &dither).expect("independent next frame"),
        expected
    );
}
