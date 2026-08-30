// pattern: Functional Core

use openjoc_eac3::{
    AudioPcmSynthesizer, ChannelLocation, InternalBasePolicy, JocAccessUnitPcmDecoder, StreamType,
    decode_audio_blocks, decode_audio_frame_pcm, decode_audio_frame_pcm_with_policy,
    group_access_units, index_syncframes, parse_bsi, parse_syncframe_header,
};

#[derive(Default)]
struct Bits(Vec<bool>);

impl Bits {
    fn push(&mut self, value: u64, width: u8) {
        for shift in (0..width).rev() {
            self.0.push(value & (1_u64 << shift) != 0);
        }
    }

    fn bytes(self, size: usize) -> Vec<u8> {
        let mut bytes = vec![0_u8; size];
        for (index, bit) in self.0.into_iter().enumerate() {
            if bit {
                bytes[index / 8] |= 0x80 >> (index % 8);
            }
        }
        bytes
    }
}

fn crc16_update(mut register: u16, input: bool) -> u16 {
    let feedback = ((register >> 15) != 0) ^ input;
    register <<= 1;
    if feedback {
        register ^= 0x8005;
    }
    register
}

fn crc16(bytes: &[u8]) -> u16 {
    bytes.iter().fold(0_u16, |mut register, byte| {
        for shift in (0..8).rev() {
            register = crc16_update(register, byte & (1 << shift) != 0);
        }
        register
    })
}

fn crc16_reverse(register: u16, input: bool) -> u16 {
    for top in [false, true] {
        let feedback = top ^ input;
        let shifted = register ^ if feedback { 0x8005 } else { 0 };
        if shifted & 1 != 0 {
            continue;
        }
        let previous = (shifted >> 1) | (u16::from(top) << 15);
        if crc16_update(previous, input) == register {
            return previous;
        }
    }
    panic!("CRC reverse transition must exist");
}

fn crc16_reverse_bytes(bytes: &[u8], mut target: u16) -> u16 {
    for byte in bytes.iter().rev() {
        for shift in 0..8 {
            target = crc16_reverse(target, byte & (1 << shift) != 0);
        }
    }
    target
}

fn two_bytes_for_target(target: u16) -> [u8; 2] {
    for value in 0..=u16::MAX {
        let bytes = value.to_be_bytes();
        if crc16(&bytes) == target {
            return bytes;
        }
    }
    panic!("two-byte CRC preimage must exist");
}

fn finalize_ac3_crc(frame: &mut [u8]) {
    let frame_words = frame.len() / 2;
    let five_eighth_bytes = ((frame_words >> 1) + (frame_words >> 3)) * 2;
    frame[2] = 0;
    frame[3] = 0;
    let required_after_crc1 = crc16_reverse_bytes(&frame[4..five_eighth_bytes], 0);
    frame[2..4].copy_from_slice(&two_bytes_for_target(required_after_crc1));
    assert_eq!(crc16(&frame[2..five_eighth_bytes]), 0);

    let end = frame.len();
    frame[end - 2] = 0;
    frame[end - 1] = 0;
    let prefix_state = frame[2..end - 2].iter().fold(0_u16, |mut register, byte| {
        for shift in (0..8).rev() {
            register = crc16_update(register, byte & (1 << shift) != 0);
        }
        register
    });
    let crc2 = (0..=u16::MAX)
        .find(|value| {
            value
                .to_be_bytes()
                .iter()
                .fold(prefix_state, |mut register, byte| {
                    for shift in (0..8).rev() {
                        register = crc16_update(register, byte & (1 << shift) != 0);
                    }
                    register
                })
                == 0
        })
        .expect("CRC2 preimage");
    frame[end - 2..].copy_from_slice(&crc2.to_be_bytes());
    assert_eq!(crc16(&frame[2..]), 0);
}

fn legacy_ac3_header_frame(bsid: u8) -> Vec<u8> {
    let mut bits = Bits::default();
    bits.push(0x0b77, 16);
    bits.push(0, 16); // crc1 is outside acquisition-header semantics
    bits.push(0, 2); // 48 kHz
    bits.push(0, 6); // 32 kbit/s => 64 words => 128 bytes
    bits.push(u64::from(bsid), 5);
    bits.bytes(128)
}

fn dependent_d0_header_frame() -> Vec<u8> {
    let mut bits = Bits::default();
    bits.push(0x0b77, 16);
    bits.push(1, 2); // dependent
    bits.push(0, 3); // D0
    bits.push(63, 11); // 128 bytes
    bits.push(0, 2); // 48 kHz
    bits.push(3, 2); // six blocks
    bits.push(2, 3); // stereo
    bits.push(0, 1); // no LFE
    bits.push(16, 5); // E-AC-3 syntax
    bits.bytes(128)
}

fn decodable_dependent_rear_pair_frame() -> Vec<u8> {
    const SIZE: usize = 4096;
    let mut bits = Bits::default();
    for (value, width) in [
        (0x0b77, 16),
        (1, 2),
        (0, 3),
        (2047, 11),
        (0, 2),
        (3, 2),
        (2, 3),
        (0, 1),
        (16, 5),
        (31, 5),
        (0, 1),
        (1, 1),
        (0x0200, 16),
        (0, 1),
        (0, 1),
        (0, 1),
        (1, 1),
        (0, 1),
        (0, 2),
        (0, 1),
        (0, 7),
        (0, 1),
    ] {
        bits.push(value, width);
    }
    for _ in 1..6 {
        bits.push(0, 1);
    }
    for block in 0..6 {
        for _ in 0..2 {
            bits.push(u64::from(block == 0), 2);
        }
    }
    bits.push(0, 6);
    bits.push(0, 4);
    bits.push(0, 1);
    bits.push(0, 1);
    bits.push(0, 1);
    for _ in 0..4 {
        bits.push(0, 1);
    }
    for _ in 0..2 {
        bits.push(0, 6);
    }
    for exponent_delta_code in [82_u8, 86] {
        bits.push(15, 4);
        for _ in 0..24 {
            bits.push(u64::from(exponent_delta_code), 7);
        }
        bits.push(0, 2);
    }
    bits.push(0, 1);
    for _ in 1..6 {
        bits.push(0, 1);
        bits.push(0, 1);
        bits.push(0, 1);
        bits.push(0, 1);
    }
    bits.bytes(SIZE)
}

fn legacy_ac3_bsi_frame() -> Vec<u8> {
    let mut bits = Bits::default();
    bits.push(0x0b77, 16);
    bits.push(0, 16); // crc1
    bits.push(0, 2); // 48 kHz
    bits.push(0, 6); // 128-byte frame
    bits.push(8, 5); // original AC-3 syntax
    bits.push(0, 3); // complete main service
    bits.push(7, 3); // 3/2
    bits.push(0, 2); // centre -3 dB
    bits.push(0, 2); // surround -3 dB
    bits.push(1, 1); // LFE
    bits.push(31, 5); // dialnorm
    bits.push(0, 1); // compression metadata absent
    bits.push(0, 1); // language code absent
    bits.push(0, 1); // audio production info absent
    bits.push(0, 1); // copyright
    bits.push(1, 1); // original bitstream
    bits.push(0, 1); // timecode 1 absent
    bits.push(0, 1); // timecode 2 absent
    bits.push(0, 1); // addbsi absent
    bits.bytes(128)
}

fn legacy_ac3_bsid6_bsi_frame(xbsi1: bool, xbsi2: bool) -> Vec<u8> {
    let mut bits = Bits::default();
    bits.push(0x0b77, 16);
    bits.push(0, 16); // crc1
    bits.push(0, 2); // 48 kHz
    bits.push(0, 6); // 128-byte frame
    bits.push(6, 5); // Annex-D alternate BSI syntax
    bits.push(0, 3); // complete main service
    bits.push(7, 3); // 3/2
    bits.push(0, 2); // centre -3 dB
    bits.push(0, 2); // surround -3 dB
    bits.push(1, 1); // LFE
    bits.push(31, 5); // dialnorm
    bits.push(0, 1); // compression metadata absent
    bits.push(0, 1); // language code absent
    bits.push(0, 1); // audio production info absent
    bits.push(0, 1); // copyright
    bits.push(1, 1); // original bitstream
    bits.push(u64::from(xbsi1), 1);
    if xbsi1 {
        bits.push(1, 2); // Lt/Rt preferred
        bits.push(6, 3); // Lt/Rt centre -6 dB
        bits.push(5, 3); // Lt/Rt surround -4.5 dB
        bits.push(3, 3); // Lo/Ro centre -1.5 dB
        bits.push(4, 3); // Lo/Ro surround -3 dB
    }
    bits.push(u64::from(xbsi2), 1);
    if xbsi2 {
        bits.push(2, 2); // Surround EX indicated (informational)
        bits.push(1, 2); // Headphone mode not indicated (informational)
        bits.push(1, 1); // HDCD A/D converter (informational)
        bits.push(0x5a, 8); // reserved xbsi2 payload
        bits.push(1, 1); // encoder information (informational)
    }
    bits.push(0, 1); // addbsi absent
    bits.bytes(128)
}

fn legacy_ac3_zero_dialnorm_bsi_frame() -> Vec<u8> {
    let mut frame = legacy_ac3_bsi_frame();
    // dialnorm begins at bit 56 for the 3/2 BSI above.
    for bit in 56..61 {
        frame[bit / 8] &= !(0x80 >> (bit % 8));
    }
    frame
}

fn decodable_legacy_ac3_frame() -> Vec<u8> {
    decodable_ac3_frame_for(7, true, None, false)
}

fn decodable_bsid6_legacy_ac3_frame() -> Vec<u8> {
    let mut frame = decodable_legacy_ac3_frame();
    for bit in 40..45 {
        frame[bit / 8] &= !(0x80 >> (bit % 8));
    }
    frame[42 / 8] |= 0x80 >> (42 % 8); // 00110
    frame[43 / 8] |= 0x80 >> (43 % 8);
    finalize_ac3_crc(&mut frame);
    frame
}

fn decodable_legacy_ac3_frame_with_dynrng(dynamic_range: Option<u8>) -> Vec<u8> {
    decodable_ac3_frame_for(7, true, dynamic_range, false)
}

fn decodable_ac3_frame_for(
    acmod: u8,
    lfe_on: bool,
    dynamic_range: Option<u8>,
    delta_reuse: bool,
) -> Vec<u8> {
    const SIZE: usize = 2560;
    let channels = [2_usize, 1, 2, 3, 3, 4, 4, 5][usize::from(acmod)];
    let exponent_codes = [62_u8, 82, 86, 102, 106];
    let mut bits = Bits::default();
    bits.push(0x0b77, 16);
    bits.push(0, 16); // patched crc1
    bits.push(0, 2); // 48 kHz
    bits.push(36, 6); // 640 kbit/s, 2,560 bytes
    bits.push(8, 5);
    bits.push(0, 3); // complete main
    bits.push(u64::from(acmod), 3);
    if acmod & 1 != 0 && acmod != 1 {
        bits.push(0, 2); // centre -3 dB
    }
    if acmod & 4 != 0 {
        bits.push(0, 2); // surround -3 dB
    }
    if acmod == 2 {
        bits.push(0, 2); // matrix-surround mode not indicated
    }
    bits.push(u64::from(lfe_on), 1);
    bits.push(31, 5); // dialnorm
    bits.push(0, 1); // compr absent
    bits.push(0, 1); // language absent
    bits.push(0, 1); // production info absent
    bits.push(0, 1); // copyright
    bits.push(1, 1); // original bitstream
    bits.push(0, 1); // timecode 1 absent
    bits.push(0, 1); // timecode 2 absent
    bits.push(0, 1); // addbsi absent

    for block in 0..6 {
        for channel in 0..channels {
            bits.push(u64::from(block == 0 && channel == 0), 1);
        }
        for _ in 0..channels {
            bits.push(1, 1); // dither enabled
        }
        bits.push(u64::from(block == 0 && dynamic_range.is_some()), 1);
        if block == 0 {
            if let Some(code) = dynamic_range {
                bits.push(u64::from(code), 8);
            }
        }
        bits.push(u64::from(block == 0), 1); // coupling strategy
        if block == 0 {
            bits.push(0, 1); // coupling off
        }
        if acmod == 2 {
            bits.push(u64::from(block == 0), 1);
            if block == 0 {
                for _ in 0..4 {
                    bits.push(0, 1);
                }
            }
        }
        for _ in 0..channels {
            bits.push(u64::from(block == 0), 2); // D15 then reuse
        }
        if lfe_on {
            bits.push(u64::from(block == 0), 1);
        }
        if block == 0 {
            for _ in 0..channels {
                bits.push(0, 6); // chbwcod
            }
            for &code in &exponent_codes[..channels] {
                bits.push(15, 4);
                for _ in 0..24 {
                    bits.push(u64::from(code), 7);
                }
                bits.push(0, 2);
            }
            if lfe_on {
                bits.push(0, 4);
                bits.push(62, 7);
                bits.push(62, 7);
            }
        }
        bits.push(u64::from(block == 0), 1); // bit allocation info
        if block == 0 {
            bits.push(2, 2);
            bits.push(1, 2);
            bits.push(1, 2);
            bits.push(2, 2);
            bits.push(7, 3);
        }
        bits.push(u64::from(block == 0), 1); // SNR offsets
        if block == 0 {
            bits.push(5, 6);
            for _ in 0..channels {
                bits.push(0, 4);
                bits.push(4, 3);
            }
            if lfe_on {
                bits.push(15, 4);
                bits.push(4, 3);
            }
        }
        if delta_reuse && block <= 1 {
            bits.push(1, 1);
            for _ in 0..channels {
                bits.push(if block == 0 { 2 } else { 0 }, 2);
            }
        } else {
            bits.push(0, 1);
        }
        bits.push(0, 1); // skip field absent
        if lfe_on {
            for group in 0..3 {
                bits.push(u64::try_from((block * 3 + group) % 27).unwrap(), 5);
            }
        }
    }
    let mut frame = bits.bytes(SIZE);
    finalize_ac3_crc(&mut frame);
    frame
}

fn coupled_stereo_ac3_frame() -> Vec<u8> {
    const SIZE: usize = 2560;
    let mut bits = Bits::default();
    for (value, width) in [
        (0x0b77, 16),
        (0, 16),
        (0, 2),
        (36, 6),
        (8, 5),
        (0, 3),
        (2, 3),
        (0, 2),
        (0, 1),
        (31, 5),
        (0, 1),
        (0, 1),
        (0, 1),
        (0, 1),
        (1, 1),
        (0, 1),
        (0, 1),
        (0, 1),
    ] {
        bits.push(value, width);
    }
    for block in 0..6 {
        for _ in 0..2 {
            bits.push(0, 1); // long transform
            bits.push(1, 1); // dither
        }
        bits.push(0, 1); // dynrng absent
        bits.push(u64::from(block == 0), 1); // coupling strategy
        if block == 0 {
            bits.push(1, 1); // coupling in use
            bits.push(1, 1); // L in coupling
            bits.push(1, 1); // R in coupling
            bits.push(0, 1); // phase flags absent
            bits.push(0, 4); // coupling begin
            bits.push(0, 4); // coupling end => three subbands
            bits.push(0, 1);
            bits.push(0, 1);
        }
        for _ in 0..2 {
            bits.push(u64::from(block == 0), 1); // coupling coordinates
            if block == 0 {
                bits.push(0, 2); // master coordinate
                for _ in 0..3 {
                    bits.push(0, 4);
                    bits.push(0, 4);
                }
            }
        }
        bits.push(u64::from(block == 0), 1); // rematrix strategy
        if block == 0 {
            bits.push(0, 1);
            bits.push(0, 1);
        }
        bits.push(u64::from(block == 0), 2); // coupling exponents
        for _ in 0..2 {
            bits.push(u64::from(block <= 1), 2); // channel exponents
        }
        if block == 0 {
            bits.push(0, 4); // coupling absolute exponent
            for _ in 0..12 {
                bits.push(62, 7);
            }
        }
        if block <= 1 {
            for _ in 0..2 {
                bits.push(15, 4);
                for _ in 0..12 {
                    bits.push(62, 7);
                }
                bits.push(0, 2);
            }
        }
        bits.push(u64::from(block == 0), 1);
        if block == 0 {
            for (value, width) in [(2, 2), (1, 2), (1, 2), (2, 2), (7, 3)] {
                bits.push(value, width);
            }
        }
        bits.push(u64::from(block == 0), 1);
        if block == 0 {
            bits.push(0, 6);
            bits.push(0, 4);
            bits.push(4, 3);
            for _ in 0..2 {
                bits.push(0, 4);
                bits.push(4, 3);
            }
            bits.push(1, 1); // coupling leak initialization
            bits.push(0, 3);
            bits.push(0, 3);
        } else {
            bits.push(0, 1); // coupling leak reuse
        }
        bits.push(0, 1); // no delta bit allocation
        bits.push(0, 1); // no skip field
    }
    let mut frame = bits.bytes(SIZE);
    finalize_ac3_crc(&mut frame);
    frame
}

#[test]
fn annex_j_bsid8_core_header_is_typed_as_legacy_independent_i0() {
    let header = parse_syncframe_header(&legacy_ac3_header_frame(8)).expect("AC-3 header");

    assert_eq!(header.stream_type, StreamType::LegacyIndependent);
    assert_eq!(header.substream_id, 0);
    assert_eq!(header.frame_size, 128);
    assert_eq!(header.sample_rate, 48_000);
    assert_eq!(header.audio_blocks, 6);
    assert_eq!(header.samples, 1536);
    assert!(parse_syncframe_header(&legacy_ac3_header_frame(9)).is_err());
}

#[test]
fn annex_j_legacy_i0_and_eac3_d0_form_one_timed_access_unit() {
    let stream = [legacy_ac3_header_frame(6), dependent_d0_header_frame()].concat();
    let frames = index_syncframes(&stream).expect("mixed syncframe index");
    let units = group_access_units(&frames).expect("mixed access unit");

    assert_eq!(frames.len(), 2);
    assert_eq!(units.len(), 1);
    assert_eq!(units[0].frame_count, 2);
    assert_eq!(units[0].sample_rate, 48_000);
    assert_eq!(units[0].samples, 1536);
}

#[test]
fn original_syntax_bsi_is_normalized_into_existing_programme_semantics() {
    let bsi = parse_bsi(&legacy_ac3_bsi_frame()).expect("AC-3 BSI");

    assert_eq!(bsi.header.stream_type, StreamType::LegacyIndependent);
    assert_eq!(bsi.bitstream_id, 8);
    assert_eq!(bsi.audio_coding_mode, 7);
    assert!(bsi.lfe_on);
    assert_eq!(bsi.dialnorm, 31);
    assert_eq!(bsi.downmix.loro_center_mix_level, Some(4));
    assert_eq!(bsi.downmix.loro_surround_mix_level, Some(4));
    assert_eq!(bsi.downmix.ltrt_center_mix_level, Some(4));
    assert_eq!(bsi.downmix.ltrt_surround_mix_level, Some(4));
    assert_eq!(bsi.channel_map, None);
    assert_eq!(bsi.addbsi, None);
}

#[test]
fn bsid6_annex_d_extended_bsi_is_parsed_without_losing_downmix_ownership() {
    let extended = parse_bsi(&legacy_ac3_bsid6_bsi_frame(true, true)).expect("bsid6 xBSI");
    assert_eq!(extended.bitstream_id, 6);
    assert_eq!(extended.downmix.dmixmod, Some(1));
    assert_eq!(extended.downmix.ltrt_center_mix_level, Some(6));
    assert_eq!(extended.downmix.ltrt_surround_mix_level, Some(5));
    assert_eq!(extended.downmix.loro_center_mix_level, Some(3));
    assert_eq!(extended.downmix.loro_surround_mix_level, Some(4));
    assert_eq!(extended.addbsi, None);

    let absent = parse_bsi(&legacy_ac3_bsid6_bsi_frame(false, false)).expect("absent xBSI");
    assert_eq!(absent.bitstream_id, 6);
    assert_eq!(absent.downmix.loro_center_mix_level, Some(4));
    assert_eq!(absent.downmix.loro_surround_mix_level, Some(4));
    assert_eq!(absent.addbsi, None);
}

#[test]
fn reserved_zero_dialnorm_uses_the_normative_minus_31_db_fallback() {
    let bsi = parse_bsi(&legacy_ac3_zero_dialnorm_bsi_frame()).expect("reserved dialnorm fallback");
    assert_eq!(bsi.dialnorm, 31);
}

#[test]
fn reserved_mix_codes_use_the_normative_reproduction_fallbacks() {
    let mut surround = legacy_ac3_bsi_frame();
    for bit in 51..55 {
        surround[bit / 8] |= 0x80 >> (bit % 8);
    }
    let bsi = parse_bsi(&surround).expect("reserved centre/surround fallback");
    assert_eq!(bsi.downmix.loro_center_mix_level, Some(5)); // -4.5 dB
    assert_eq!(bsi.downmix.loro_surround_mix_level, Some(6)); // -6 dB

    let mut stereo = decodable_ac3_frame_for(2, false, None, false);
    stereo[51 / 8] |= 0x80 >> (51 % 8);
    stereo[52 / 8] |= 0x80 >> (52 % 8);
    finalize_ac3_crc(&mut stereo);
    let bsi = parse_bsi(&stereo).expect("reserved dsurmod fallback");
    assert_eq!(bsi.downmix.dmixmod, Some(2)); // not indicated -> Lo/Ro default
}

#[test]
fn original_syntax_crc_corruption_is_rejected_before_audio_decode() {
    assert!(decode_audio_blocks(&legacy_ac3_bsi_frame(), &vec![0.25; 32_768]).is_err());
}

#[test]
fn original_syntax_pcm_reuses_shared_transform_and_emits_finite_1536_samples() {
    let frame = decodable_legacy_ac3_frame();
    let blocks = decode_audio_blocks(&frame, &vec![0.25; 32_768]).expect("AC-3 blocks");
    assert!(blocks[0].prefix.block_switch[0]);
    let mut synthesizer = AudioPcmSynthesizer::new();
    let pcm = decode_audio_frame_pcm(&frame, &vec![0.25; 32_768], &mut synthesizer)
        .expect("native AC-3 PCM");

    assert_eq!(pcm.channels.len(), 5);
    assert!(pcm.channels.iter().all(|channel| channel.len() == 1536));
    assert!(pcm.lfe.as_ref().is_some_and(|lfe| lfe.len() == 1536));
    assert!(
        pcm.channels
            .iter()
            .flatten()
            .all(|sample| sample.is_finite())
    );
    assert!(
        pcm.lfe
            .as_ref()
            .unwrap()
            .iter()
            .all(|sample| sample.is_finite())
    );
    assert!(pcm.channels.iter().flatten().any(|sample| *sample != 0.0));
}

#[test]
fn annex_j_bsid6_core_without_extended_bsi_decodes_natively() {
    let frame = decodable_bsid6_legacy_ac3_frame();
    let bsi = parse_bsi(&frame).expect("bsid6 BSI");
    assert_eq!(bsi.bitstream_id, 6);
    let pcm = decode_audio_frame_pcm(&frame, &vec![0.25; 32_768], &mut AudioPcmSynthesizer::new())
        .expect("native bsid6 core");
    assert_eq!(pcm.channels.len(), 5);
    assert!(pcm.channels.iter().all(|channel| channel.len() == 1536));
}

#[test]
fn mixed_core_and_d0_preserve_core_compatibility_and_assemble_rear_seven_inputs() {
    let stream = [
        decodable_legacy_ac3_frame(),
        decodable_dependent_rear_pair_frame(),
    ]
    .concat();
    let frames = index_syncframes(&stream).expect("mixed frames");
    let unit = group_access_units(&frames).expect("mixed unit")[0];
    let mut decoder = JocAccessUnitPcmDecoder::new();
    let planes = decoder
        .decode_pcm_planes_with_policy(
            &stream,
            &frames,
            unit,
            &vec![0.25; 32_768],
            InternalBasePolicy::CodecCore,
        )
        .expect("mixed native decode");

    assert_eq!(
        planes.compatibility_pcm.channel_locations,
        [
            ChannelLocation::Left,
            ChannelLocation::Right,
            ChannelLocation::Centre,
            ChannelLocation::LeftSurround,
            ChannelLocation::RightSurround,
        ]
    );
    assert_eq!(
        planes.joc_input_pcm.channel_locations,
        [
            ChannelLocation::Left,
            ChannelLocation::Right,
            ChannelLocation::Centre,
            ChannelLocation::LeftSurround,
            ChannelLocation::RightSurround,
            ChannelLocation::LeftBack,
            ChannelLocation::RightBack,
        ]
    );
    assert_eq!(planes.compatibility_pcm.channels.len(), 5);
    assert_eq!(planes.joc_input_pcm.channels.len(), 7);
    assert_ne!(
        planes.compatibility_pcm.channels[0].as_ptr(),
        planes.joc_input_pcm.channels[0].as_ptr()
    );
}

#[test]
fn original_syntax_standard_coupling_is_decoded_by_the_shared_reconstruction_path() {
    let frame = coupled_stereo_ac3_frame();
    let blocks = decode_audio_blocks(&frame, &vec![0.25; 32_768]).expect("coupled blocks");
    assert_eq!(
        blocks[0].prefix.channel_exponents[0]
            .as_ref()
            .unwrap()
            .end_mantissa,
        37,
        "coupled channel coefficients stop at the coupling start bin"
    );
    let mut synthesizer = AudioPcmSynthesizer::new();
    let pcm = decode_audio_frame_pcm(&frame, &vec![0.25; 32_768], &mut synthesizer)
        .expect("coupled AC-3 PCM");

    assert_eq!(pcm.channels.len(), 2);
    assert!(pcm.channels.iter().all(|channel| channel.len() == 1536));
    assert!(
        pcm.channels
            .iter()
            .flatten()
            .all(|sample| sample.is_finite())
    );
}

#[test]
fn original_syntax_dynamic_range_is_applied_once_under_the_selected_policy() {
    let frame = decodable_legacy_ac3_frame_with_dynrng(Some(0x20));
    let dither = vec![0.25; 32_768];
    let line = decode_audio_frame_pcm_with_policy(
        &frame,
        &dither,
        &mut AudioPcmSynthesizer::new(),
        InternalBasePolicy::CurrentDefault,
    )
    .expect("line DRC");
    let core = decode_audio_frame_pcm_with_policy(
        &frame,
        &dither,
        &mut AudioPcmSynthesizer::new(),
        InternalBasePolicy::CodecCore,
    )
    .expect("DRC-disabled core");

    assert_ne!(line.channels, core.channels);
    assert_eq!(line.channels.len(), core.channels.len());
    assert!(
        line.channels
            .iter()
            .flatten()
            .all(|sample| sample.is_finite())
    );
}

#[test]
fn every_annex_j_admitted_non_dual_mono_acmod_decodes_with_canonical_dimensions() {
    let expected_channels = [1_usize, 2, 3, 3, 4, 4, 5];
    for (acmod, expected) in (1_u8..=7).zip(expected_channels) {
        let frame = decodable_ac3_frame_for(acmod, false, None, false);
        let pcm =
            decode_audio_frame_pcm(&frame, &vec![0.25; 32_768], &mut AudioPcmSynthesizer::new())
                .unwrap_or_else(|error| panic!("acmod {acmod}: {error}"));
        assert_eq!(pcm.channels.len(), expected, "acmod {acmod}");
        assert!(pcm.channels.iter().all(|channel| channel.len() == 1536));
        assert!(pcm.lfe.is_none());
    }
}

#[test]
fn original_syntax_delta_bit_allocation_reuse_is_stateful_and_bounded() {
    let frame = decodable_ac3_frame_for(2, false, None, true);
    let pcm = decode_audio_frame_pcm(&frame, &vec![0.25; 32_768], &mut AudioPcmSynthesizer::new())
        .expect("delta-allocation reuse");

    assert_eq!(pcm.channels.len(), 2);
    assert!(
        pcm.channels
            .iter()
            .flatten()
            .all(|sample| sample.is_finite())
    );
}

#[test]
fn original_syntax_tdac_reset_and_failed_frame_recovery_are_atomic() {
    let frame = decodable_legacy_ac3_frame();
    let dither = vec![0.25; 32_768];
    let mut continuing = AudioPcmSynthesizer::new();
    decode_audio_frame_pcm(&frame, &dither, &mut continuing).expect("first frame");
    let expected_state = continuing.clone();

    let mut malformed = frame.clone();
    malformed[64] ^= 1;
    assert!(decode_audio_frame_pcm(&malformed, &dither, &mut continuing).is_err());
    let recovered = decode_audio_frame_pcm(&frame, &dither, &mut continuing).expect("recovery");
    let control = decode_audio_frame_pcm(&frame, &dither, &mut expected_state.clone())
        .expect("failure-free control");
    assert_eq!(recovered, control);

    continuing.reset();
    let after_reset =
        decode_audio_frame_pcm(&frame, &dither, &mut continuing).expect("reset frame");
    let fresh = decode_audio_frame_pcm(&frame, &dither, &mut AudioPcmSynthesizer::new())
        .expect("fresh frame");
    assert_eq!(after_reset, fresh);
}
