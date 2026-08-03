use openjoc_eac3::{
    CouplingInformation, Eac3Error, JocAddbsi, StreamType, block_start_information_length,
    channel_end_mantissa, channel_exponent_group_count, decode_exponents, decode_first_audio_block,
    decode_frame_exponent_strategy, extract_aux_emdf, extract_aux_joc_access_unit, extract_auxdata,
    group_access_units, index_syncframes, parse_audio_frame, parse_bsi,
    parse_first_audio_block_prefix, parse_joc_addbsi, parse_syncframe_header, spx_subband_range,
    validate_complexity_index,
};

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

    fn set(&mut self, position: usize, value: u64, width: u8) {
        for (index, shift) in (0..width).rev().enumerate() {
            self.0[position + index] = (value >> shift) & 1 != 0;
        }
    }

    fn padded_bytes(self) -> Vec<u8> {
        let length = self.0.len().div_ceil(8);
        self.bytes(length)
    }
}

fn frame(stream_type: u8, substream_id: u8, size: usize, fscod: u8, blocks: u8) -> Vec<u8> {
    assert_eq!(size % 2, 0);
    let mut bits = Bits::default();
    bits.push(0x0b77, 16);
    bits.push(u64::from(stream_type), 2);
    bits.push(u64::from(substream_id), 3);
    bits.push(u64::try_from(size / 2 - 1).expect("frame words"), 11);
    bits.push(u64::from(fscod), 2);
    bits.push(u64::from(blocks), 2);
    bits.bytes(size)
}

#[test]
fn parses_every_stream_rate_and_block_code() {
    let rates = [48_000, 44_100, 32_000];
    let block_counts = [1, 2, 3, 6];
    for (fscod, sample_rate) in rates.into_iter().enumerate() {
        for (code, blocks) in block_counts.into_iter().enumerate() {
            let bytes = frame(
                1,
                5,
                64,
                u8::try_from(fscod).expect("rate code"),
                u8::try_from(code).expect("block code"),
            );
            let header = parse_syncframe_header(&bytes).expect("valid header");
            assert_eq!(header.stream_type, StreamType::Dependent);
            assert_eq!(header.substream_id, 5);
            assert_eq!(header.frame_size, 64);
            assert_eq!(header.sample_rate, sample_rate);
            assert_eq!(header.audio_blocks, blocks);
            assert_eq!(header.samples, u16::from(blocks) * 256);
        }
    }
}

#[test]
fn indexes_complete_frames_without_scanning_inside_payloads() {
    let first = frame(0, 0, 32, 0, 3);
    let second = frame(1, 0, 48, 0, 3);
    let bytes = [first, second].concat();
    let entries = index_syncframes(&bytes).expect("two frames");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].offset, 0);
    assert_eq!(entries[1].offset, 32);
    assert_eq!(entries[1].header.frame_size, 48);
}

#[test]
fn rejects_reserved_headers_bad_sync_and_declared_truncation() {
    assert_eq!(
        parse_syncframe_header(&frame(3, 0, 16, 0, 0)),
        Err(Eac3Error::ReservedStreamType)
    );
    assert_eq!(
        parse_syncframe_header(&frame(0, 0, 16, 3, 0)),
        Err(Eac3Error::ReservedSampleRate)
    );
    let mut bad_sync = frame(0, 0, 16, 0, 0);
    bad_sync[0] = 0;
    assert_eq!(
        parse_syncframe_header(&bad_sync),
        Err(Eac3Error::InvalidSyncword { actual: 0x0077 })
    );
    let truncated = frame(0, 0, 32, 0, 0);
    assert_eq!(
        index_syncframes(&truncated[..16]),
        Err(Eac3Error::TruncatedFrame {
            offset: 0,
            declared: 32,
            available: 16,
        })
    );
}

#[test]
fn parses_and_bounds_the_type_a_addbsi_extension() {
    assert_eq!(
        parse_joc_addbsi(&[0x01, 0x10]),
        Ok(JocAddbsi {
            complexity_index: 16,
        })
    );
    assert_eq!(
        parse_joc_addbsi(&[0x00, 0x10]),
        Err(Eac3Error::MissingJocExtensionFlag)
    );
    assert_eq!(
        parse_joc_addbsi(&[0x03, 0x10]),
        Err(Eac3Error::NonzeroReservedData)
    );
    assert_eq!(
        parse_joc_addbsi(&[0x01, 0x11]),
        Err(Eac3Error::ComplexityIndexOutOfRange { actual: 17 })
    );
    assert_eq!(
        parse_joc_addbsi(&[0x01]),
        Err(Eac3Error::InvalidAddbsiLength { actual: 1 })
    );
}

#[test]
fn complexity_index_equals_the_oamd_program_object_count() {
    assert_eq!(validate_complexity_index(0, 0), Ok(()));
    assert_eq!(validate_complexity_index(16, 16), Ok(()));
    assert_eq!(
        validate_complexity_index(7, 8),
        Err(Eac3Error::ComplexityIndexMismatch {
            complexity: 7,
            objects: 8,
        })
    );
    assert_eq!(
        validate_complexity_index(0, 17),
        Err(Eac3Error::ComplexityIndexMismatch {
            complexity: 0,
            objects: 17,
        })
    );
}

#[test]
fn parses_bsi_conditionals_to_extract_addbsi_without_scanning() {
    let mut bits = Bits::default();
    bits.push(0x0b77, 16);
    bits.push(0, 2); // independent
    bits.push(0, 3);
    bits.push(31, 11); // 64-byte frame
    bits.push(0, 2); // 48 kHz
    bits.push(3, 2); // 6 blocks
    bits.push(2, 3); // stereo
    bits.push(0, 1); // no LFE
    bits.push(16, 5); // E-AC-3 version
    bits.push(31, 5); // dialnorm
    bits.push(0, 1); // no compression word
    bits.push(0, 1); // no mixing metadata
    bits.push(0, 1); // no informational metadata
    bits.push(1, 1); // addbsi exists
    bits.push(1, 6); // 2 bytes
    bits.push(0x01, 8);
    bits.push(0x05, 8);
    let bytes = bits.bytes(64);

    let bsi = parse_bsi(&bytes).expect("valid complete BSI");
    assert_eq!(bsi.audio_coding_mode, 2);
    assert!(!bsi.lfe_on);
    assert_eq!(bsi.bitstream_id, 16);
    assert_eq!(bsi.addbsi.as_deref(), Some(&[0x01, 0x05][..]));
    assert_eq!(
        parse_joc_addbsi(bsi.addbsi.as_deref().expect("addbsi")),
        Ok(JocAddbsi {
            complexity_index: 5,
        })
    );
}

#[test]
fn mixing_option_four_length_includes_the_mixdeflen_field() {
    let mut bits = Bits::default();
    bits.push(0x0b77, 16);
    bits.push(0, 2);
    bits.push(0, 3);
    bits.push(31, 11);
    bits.push(0, 2);
    bits.push(3, 2);
    bits.push(2, 3);
    bits.push(0, 1);
    bits.push(16, 5);
    bits.push(31, 5);
    bits.push(0, 1); // compre
    bits.push(1, 1); // mixmdate
    bits.push(0, 1); // pgmscle
    bits.push(0, 1); // extpgmscle
    bits.push(3, 2); // mixdef option 4
    bits.push(0, 5); // 2-byte mixdata, including these five bits
    bits.push(0, 11); // mixdata2e, mixdata3e, and zero fill
    bits.push(0, 1); // frmmixcfginfoe
    bits.push(0, 1); // infomdate
    bits.push(1, 1); // addbsie
    bits.push(1, 6);
    bits.push(0x01, 8);
    bits.push(0x04, 8);

    let bsi = parse_bsi(&bits.bytes(64)).expect("bounded option-four mixdata");
    assert_eq!(bsi.addbsi, Some(vec![0x01, 0x04]));
}

#[test]
fn parses_one_block_audio_frame_state_and_exact_block_offset() {
    let mut bits = Bits::default();
    bits.push(0x0b77, 16);
    bits.push(0, 2); // independent
    bits.push(0, 3);
    bits.push(63, 11); // 128-byte frame
    bits.push(0, 2); // 48 kHz
    bits.push(0, 2); // one block
    bits.push(1, 3); // mono
    bits.push(0, 1); // no LFE
    bits.push(16, 5);
    bits.push(31, 5);
    bits.push(0, 1); // compre
    bits.push(0, 1); // mixmdate
    bits.push(0, 1); // infomdate
    bits.push(0, 1); // convsync
    bits.push(0, 1); // addbsie

    bits.push(0, 2); // frame SNR strategy
    bits.push(0, 1); // transient processing
    bits.push(1, 1); // block-switch syntax
    bits.push(0, 1); // dither syntax
    bits.push(0, 1); // bit-allocation syntax
    bits.push(1, 1); // frame fast-gain syntax
    bits.push(1, 1); // delta-bit-allocation syntax
    bits.push(1, 1); // skip-field syntax
    bits.push(0, 1); // SPX attenuation syntax
    bits.push(1, 2); // channel exponent strategy D15
    bits.push(0, 1); // converter exponent strategy absent
    bits.push(32, 6); // frame coarse SNR offset
    bits.push(7, 4); // frame fine SNR offset
    let expected_offset = bits.0.len();
    let bytes = bits.bytes(128);

    let frame = parse_audio_frame(&bytes).expect("valid audio-frame syntax");
    assert_eq!(frame.full_bandwidth_channels, 1);
    assert_eq!(frame.snr_offset_strategy, 0);
    assert!(frame.syntax.block_switch());
    assert!(!frame.syntax.dither());
    assert!(!frame.syntax.bit_allocation());
    assert!(frame.syntax.frame_fast_gain());
    assert!(frame.syntax.delta_bit_allocation());
    assert!(frame.syntax.skip_field());
    assert!(!frame.syntax.spx_attenuation());
    assert_eq!(frame.coupling_in_use, [false]);
    assert_eq!(frame.channel_exponent_strategy, vec![vec![1]]);
    assert_eq!(frame.frame_coarse_snr_code, Some(32));
    assert_eq!(frame.frame_fine_snr_code, Some(7));
    assert_eq!(frame.audio_blocks_offset_bits, expected_offset);
}

#[test]
fn parses_first_audio_block_through_spectral_extension_coordinates() {
    let mut bits = Bits::default();
    bits.push(0x0b77, 16);
    bits.push(0, 2); // independent
    bits.push(0, 3);
    bits.push(63, 11); // 128-byte frame
    bits.push(0, 2); // 48 kHz
    bits.push(0, 2); // one block
    bits.push(1, 3); // mono
    bits.push(0, 1); // no LFE
    bits.push(16, 5);
    bits.push(31, 5);
    bits.push(0, 1); // compre
    bits.push(0, 1); // mixmdate
    bits.push(0, 1); // infomdate
    bits.push(0, 1); // convsync
    bits.push(0, 1); // addbsie
    bits.push(0, 2); // frame SNR strategy
    bits.push(0, 1); // transient processing
    bits.push(1, 1); // block-switch syntax
    bits.push(0, 1); // dither syntax disabled
    bits.push(0, 5); // remaining syntax flags
    bits.push(1, 2); // channel D15
    bits.push(0, 1); // converter exponent strategy absent
    bits.push(0, 10); // frame SNR offsets

    bits.push(1, 1); // block switch
    bits.push(1, 1); // dynamic range exists
    bits.push(0xa5, 8); // dynamic range
    bits.push(1, 1); // SPX in use (strategy is implicit in block zero)
    bits.push(2, 2); // start copy frequency code
    bits.push(0, 3); // begin subband 2
    bits.push(3, 3); // end subband 9
    bits.push(1, 1); // band structure exists
    for value in [false, true, false, true, true, false] {
        bits.push(u64::from(value), 1); // subbands 3 through 8
    }
    bits.push(17, 5); // blend
    bits.push(2, 2); // master coordinate
    for (exponent, mantissa) in [(1, 0), (2, 1), (3, 2), (4, 3)] {
        bits.push(exponent, 4);
        bits.push(mantissa, 2);
    }
    bits.push(10, 4); // channel absolute exponent
    for _ in 0..16 {
        bits.push(62, 7); // neutral D15 groups through SPX begin bin 49
    }
    bits.push(1, 2); // gain range
    bits.push(0, 1); // converter SNR offset absent
    let expected_offset = bits.0.len();

    let prefix = parse_first_audio_block_prefix(&bits.bytes(128)).expect("valid block prefix");
    assert_eq!(prefix.block_switch, vec![true]);
    assert_eq!(prefix.dither, vec![true]);
    assert_eq!(prefix.dynamic_range, Some(0xa5));
    assert_eq!(prefix.dynamic_range_2, None);
    let spx = prefix.spectral_extension.expect("SPX state");
    assert_eq!(spx.channel_in_use, vec![true]);
    assert_eq!(spx.start_copy_frequency_code, 2);
    assert_eq!((spx.begin_subband, spx.end_subband), (2, 9));
    assert_eq!(spx.band_count, 4);
    let coordinate = spx.coordinates[0].as_ref().expect("channel coordinate");
    assert_eq!(coordinate.blend, 17);
    assert_eq!(coordinate.master, 2);
    assert_eq!(coordinate.bands, vec![(1, 0), (2, 1), (3, 2), (4, 3)]);
    assert_eq!(prefix.channel_bandwidth_codes, vec![None]);
    let exponents = prefix.channel_exponents[0]
        .as_ref()
        .expect("channel exponents");
    assert_eq!((exponents.start_mantissa, exponents.end_mantissa), (0, 49));
    assert_eq!(exponents.decoded, vec![10; 49]);
    assert_eq!(exponents.gain_range, Some(1));
    assert_eq!(prefix.next_offset_bits, expected_offset);
}

#[test]
#[allow(clippy::too_many_lines)]
fn parses_first_audio_block_standard_coupling_coordinates() {
    let mut bits = Bits::default();
    bits.push(0x0b77, 16);
    bits.push(0, 2); // independent
    bits.push(0, 3);
    bits.push(63, 11); // 128-byte frame
    bits.push(0, 2); // 48 kHz
    bits.push(0, 2); // one block
    bits.push(2, 3); // stereo
    bits.push(0, 1); // no LFE
    bits.push(16, 5);
    bits.push(31, 5);
    bits.push(0, 1); // compre
    bits.push(0, 1); // mixmdate
    bits.push(0, 1); // infomdate
    bits.push(0, 1); // convsync
    bits.push(0, 1); // addbsie
    bits.push(0, 2); // frame SNR strategy
    bits.push(0, 1); // transient processing
    bits.push(0, 7); // compact syntax flags
    bits.push(1, 1); // coupling in use
    bits.push(1, 2); // coupling D15
    bits.push(1, 2); // left D15
    bits.push(1, 2); // right D15
    bits.push(0, 1); // converter exponent strategy absent
    bits.push(0, 10); // frame SNR offsets

    bits.push(0, 1); // dynamic range absent
    bits.push(0, 1); // SPX not in use
    bits.push(0, 1); // standard coupling
    bits.push(1, 1); // phase flags in use
    bits.push(0, 4); // coupling begin frequency
    bits.push(2, 4); // coupling end frequency: five subbands
    bits.push(1, 1); // band structure exists
    for value in [false, true, false, true] {
        bits.push(u64::from(value), 1);
    }
    for (master, coordinates) in [
        (1, [(1, 2), (3, 4), (5, 6)]),
        (2, [(7, 8), (9, 10), (11, 12)]),
    ] {
        bits.push(master, 2);
        for (exponent, mantissa) in coordinates {
            bits.push(exponent, 4);
            bits.push(mantissa, 4);
        }
    }
    bits.push(0b101, 3); // one phase flag per coupling band
    bits.push(0b10, 2); // two rematrix flags for standard cplbegf zero
    bits.push(5, 4); // coupling absolute exponent, decoded as 10
    for _ in 0..20 {
        bits.push(62, 7); // 60 coupled mantissas
    }
    for gain_range in [1, 2] {
        bits.push(10, 4);
        for _ in 0..12 {
            bits.push(62, 7); // channel end mantissa is coupling start 37
        }
        bits.push(gain_range, 2);
    }
    bits.push(0, 1); // converter SNR offset absent
    bits.push(3, 3); // first coupling fast leak
    bits.push(5, 3); // first coupling slow leak
    let expected_offset = bits.0.len();

    let bytes = bits.bytes(128);
    let prefix = parse_first_audio_block_prefix(&bytes).expect("standard coupling");
    let coupling = match prefix.coupling.expect("coupling state") {
        CouplingInformation::Standard(value) => value,
        CouplingInformation::Enhanced(_) => panic!("expected standard coupling"),
    };
    assert_eq!(coupling.channel_in_use, vec![true, true]);
    assert!(coupling.phase_flags_in_use);
    assert_eq!(coupling.begin_frequency_code, 0);
    assert_eq!(coupling.end_frequency_code, 2);
    assert_eq!(coupling.subband_count, 5);
    assert_eq!(coupling.band_count, 3);
    let left = coupling.coordinates[0].as_ref().expect("left coordinates");
    assert_eq!(left.master, 1);
    assert_eq!(left.bands, vec![(1, 2), (3, 4), (5, 6)]);
    let right = coupling.coordinates[1].as_ref().expect("right coordinates");
    assert_eq!(right.master, 2);
    assert_eq!(right.bands, vec![(7, 8), (9, 10), (11, 12)]);
    assert_eq!(coupling.phase_flags, vec![true, false, true]);
    assert_eq!(prefix.rematrix_flags, vec![true, false]);
    let coupling_exponents = prefix
        .coupling_exponents
        .as_ref()
        .expect("coupling exponents");
    assert_eq!(
        (
            coupling_exponents.start_mantissa,
            coupling_exponents.end_mantissa
        ),
        (37, 97)
    );
    assert_eq!(coupling_exponents.decoded, vec![10; 60]);
    assert_eq!(
        prefix.channel_exponents[0].as_ref().expect("left").decoded,
        vec![10; 37]
    );
    assert_eq!(
        prefix.channel_exponents[1].as_ref().expect("right").decoded,
        vec![10; 37]
    );
    let leakage = prefix.coupling_leak.expect("coupling leakage");
    assert_eq!((leakage.fast_code, leakage.slow_code), (3, 5));
    assert_eq!(prefix.next_offset_bits, expected_offset);

    let decoded =
        decode_first_audio_block(&bytes, &[0.0; 74]).expect("standard coupling mantissas");
    assert_eq!(
        decoded
            .channel_baps
            .iter()
            .map(Vec::len)
            .collect::<Vec<_>>(),
        vec![37, 37]
    );
    assert_eq!(
        decoded.coupling_bap.as_ref().expect("coupling BAP").len(),
        60
    );
    assert_eq!(
        decoded
            .coupling_mantissas
            .as_ref()
            .expect("coupling mantissas")
            .len(),
        60
    );
    assert_eq!(decoded.mantissa_end_offset_bits, expected_offset);
}

#[test]
#[allow(clippy::too_many_lines)]
fn parses_first_audio_block_enhanced_coupling_coordinates() {
    let mut bits = Bits::default();
    bits.push(0x0b77, 16);
    bits.push(0, 2); // independent
    bits.push(0, 3);
    bits.push(127, 11); // 256-byte frame
    bits.push(0, 2); // 48 kHz
    bits.push(0, 2); // one block
    bits.push(3, 3); // three front channels
    bits.push(0, 1); // no LFE
    bits.push(16, 5);
    bits.push(31, 5);
    bits.push(0, 1); // compre
    bits.push(0, 1); // mixmdate
    bits.push(0, 1); // infomdate
    bits.push(0, 1); // convsync
    bits.push(0, 1); // addbsie
    bits.push(0, 2); // frame SNR strategy
    bits.push(0, 1); // transient processing
    bits.push(0, 7); // compact syntax flags
    bits.push(1, 1); // coupling in use
    bits.push(1, 2); // coupling D15
    for _ in 0..3 {
        bits.push(1, 2); // channel D15
    }
    bits.push(0, 1); // converter exponent strategy absent
    bits.push(0, 10); // frame SNR offsets

    bits.push(0, 1); // dynamic range absent
    bits.push(0, 1); // SPX not in use
    bits.push(1, 1); // enhanced coupling
    bits.push(0b101, 3); // channels 0 and 2 participate
    bits.push(3, 4); // begin subband 5
    bits.push(4, 4); // end subband 11
    bits.push(1, 1); // band structure exists
    bits.push(0, 1); // subband 9 starts a band
    bits.push(1, 1); // subband 10 merges: five bands total
    bits.push(0, 1); // leading reserved bit
    for amplitude in [1, 2, 3, 4, 5] {
        bits.push(amplitude, 5); // first participating channel
    }
    for amplitude in [6, 7, 8, 9, 10] {
        bits.push(amplitude, 5); // later participating channel
    }
    bits.push(0, 36); // 9 * (necplbnd - 1) reserved bits
    bits.push(0, 1); // trailing later-channel reserved bit
    bits.push(0, 6); // bandwidth code for uncoupled channel 1
    bits.push(5, 4); // coupling absolute exponent, decoded as 10
    for _ in 0..24 {
        bits.push(62, 7); // enhanced coupling bins 49 through 120
    }
    for (groups, gain_range) in [(16, 0), (24, 1), (16, 2)] {
        bits.push(10, 4);
        for _ in 0..groups {
            bits.push(62, 7);
        }
        bits.push(gain_range, 2);
    }
    bits.push(0, 1); // converter SNR offset absent
    bits.push(2, 3); // first coupling fast leak
    bits.push(6, 3); // first coupling slow leak
    let expected_offset = bits.0.len();

    let prefix = parse_first_audio_block_prefix(&bits.bytes(256)).expect("enhanced coupling");
    let coupling = match prefix.coupling.expect("coupling state") {
        CouplingInformation::Enhanced(value) => value,
        CouplingInformation::Standard(_) => panic!("expected enhanced coupling"),
    };
    assert_eq!(coupling.channel_in_use, vec![true, false, true]);
    assert_eq!(coupling.begin_subband, 5);
    assert_eq!(coupling.end_subband, 11);
    assert_eq!(coupling.band_count, 5);
    assert_eq!(coupling.amplitudes[0], Some(vec![1, 2, 3, 4, 5]));
    assert_eq!(coupling.amplitudes[1], None);
    assert_eq!(coupling.amplitudes[2], Some(vec![6, 7, 8, 9, 10]));
    assert_eq!(prefix.channel_bandwidth_codes, vec![None, Some(0), None]);
    let coupling_exponents = prefix
        .coupling_exponents
        .as_ref()
        .expect("coupling exponents");
    assert_eq!(
        (
            coupling_exponents.start_mantissa,
            coupling_exponents.end_mantissa
        ),
        (49, 121)
    );
    assert_eq!(coupling_exponents.decoded, vec![10; 72]);
    assert_eq!(
        prefix.channel_exponents[0].as_ref().expect("left").decoded,
        vec![10; 49]
    );
    assert_eq!(
        prefix.channel_exponents[1]
            .as_ref()
            .expect("centre")
            .decoded,
        vec![10; 73]
    );
    assert_eq!(
        prefix.channel_exponents[2].as_ref().expect("right").decoded,
        vec![10; 49]
    );
    let leakage = prefix.coupling_leak.expect("coupling leakage");
    assert_eq!((leakage.fast_code, leakage.slow_code), (2, 6));
    assert_eq!(prefix.next_offset_bits, expected_offset);
}

#[test]
#[allow(clippy::too_many_lines)]
fn parses_uncoupled_channel_and_lfe_exponents() {
    let mut bits = Bits::default();
    bits.push(0x0b77, 16);
    bits.push(0, 2); // independent
    bits.push(0, 3);
    bits.push(127, 11); // 256-byte frame
    bits.push(0, 2); // 48 kHz
    bits.push(0, 2); // one block
    bits.push(1, 3); // mono
    bits.push(1, 1); // LFE
    bits.push(16, 5);
    bits.push(31, 5);
    bits.push(0, 1); // compre
    bits.push(0, 1); // mixmdate
    bits.push(0, 1); // infomdate
    bits.push(0, 1); // convsync
    bits.push(0, 1); // addbsie
    bits.push(2, 2); // per-element block SNR strategy
    bits.push(0, 1); // transient processing
    bits.push(0, 1); // block-switch syntax
    bits.push(0, 1); // dither syntax
    bits.push(1, 1); // bit-allocation syntax
    bits.push(1, 1); // frame fast-gain syntax
    bits.push(1, 1); // delta-bit-allocation syntax
    bits.push(1, 1); // skip-field syntax
    bits.push(0, 1); // SPX attenuation syntax
    bits.push(1, 2); // channel D15
    bits.push(1, 1); // LFE D15
    bits.push(0, 1); // converter exponent strategy absent

    bits.push(0, 1); // dynamic range absent
    bits.push(0, 1); // SPX not in use
    bits.push(0, 6); // channel bandwidth code: end mantissa 73
    bits.push(10, 4); // channel absolute exponent
    for _ in 0..24 {
        bits.push(62, 7);
    }
    bits.push(3, 2); // channel gain range
    bits.push(8, 4); // LFE absolute exponent
    bits.push(62, 7);
    bits.push(62, 7);
    bits.push(1, 1); // new bit-allocation parameters
    bits.push(3, 2); // slow decay
    bits.push(2, 2); // fast decay
    bits.push(1, 2); // slow gain
    bits.push(0, 2); // dB per bit
    bits.push(5, 3); // floor
    bits.push(33, 6); // coarse SNR offset
    bits.push(5, 4); // channel fine SNR offset
    bits.push(7, 4); // LFE fine SNR offset
    bits.push(1, 1); // new fast-gain codes
    bits.push(3, 3); // channel fast gain
    bits.push(6, 3); // LFE fast gain
    bits.push(1, 1); // converter SNR offset exists
    bits.push(0x155, 10); // converter SNR offset
    bits.push(1, 1); // delta-bit-allocation information exists
    bits.push(1, 2); // channel: new information follows
    bits.push(1, 3); // two channel delta segments
    for (offset, length, delta) in [(3, 4, 5), (17, 9, 2)] {
        bits.push(offset, 5);
        bits.push(length, 4);
        bits.push(delta, 3);
    }
    bits.push(1, 1); // skip length exists
    bits.push(2, 9); // two skipped bytes
    bits.push(0xabcd, 16); // skipped data
    let expected_offset = bits.0.len();

    let bytes = bits.bytes(256);
    let prefix = parse_first_audio_block_prefix(&bytes).expect("channel and LFE exponents");
    assert_eq!(prefix.channel_bandwidth_codes, vec![Some(0)]);
    let channel = prefix.channel_exponents[0]
        .as_ref()
        .expect("channel exponents");
    assert_eq!((channel.start_mantissa, channel.end_mantissa), (0, 73));
    assert_eq!(channel.decoded, vec![10; 73]);
    assert_eq!(channel.gain_range, Some(3));
    let lfe = prefix.lfe_exponents.as_ref().expect("LFE exponents");
    assert_eq!((lfe.start_mantissa, lfe.end_mantissa), (0, 7));
    assert_eq!(lfe.decoded, vec![8; 7]);
    assert_eq!(lfe.gain_range, None);
    let bit_allocation = prefix
        .bit_allocation_parameters
        .expect("bit-allocation parameters");
    assert_eq!(bit_allocation.slow_decay_code, 3);
    assert_eq!(bit_allocation.fast_decay_code, 2);
    assert_eq!(bit_allocation.slow_gain_code, 1);
    assert_eq!(bit_allocation.db_per_bit_code, 0);
    assert_eq!(bit_allocation.floor_code, 5);
    let snr = prefix.snr_offsets.expect("SNR offsets");
    assert_eq!(snr.coarse_code, 33);
    assert_eq!(snr.coupling_fine_code, None);
    assert_eq!(snr.channel_fine_codes, vec![5]);
    assert_eq!(snr.lfe_fine_code, Some(7));
    let fast_gain = prefix.fast_gain_codes.expect("fast-gain codes");
    assert_eq!(fast_gain.coupling, None);
    assert_eq!(fast_gain.channels, vec![3]);
    assert_eq!(fast_gain.lfe, Some(6));
    assert_eq!(prefix.converter_snr_offset, Some(0x155));
    let delta = prefix.delta_bit_allocation.expect("delta allocation");
    assert_eq!(delta.coupling, None);
    assert_eq!(delta.channels[0].strategy, 1);
    assert_eq!(delta.channels[0].segments.len(), 2);
    assert_eq!(
        (
            delta.channels[0].segments[0].offset,
            delta.channels[0].segments[0].length,
            delta.channels[0].segments[0].delta
        ),
        (3, 4, 5)
    );
    assert_eq!(
        (
            delta.channels[0].segments[1].offset,
            delta.channels[0].segments[1].length,
            delta.channels[0].segments[1].delta
        ),
        (17, 9, 2)
    );
    assert_eq!(
        prefix.skip_field,
        Some(openjoc_eac3::AuxiliaryData {
            bit_len: 16,
            bytes: vec![0xab, 0xcd],
        })
    );
    assert_eq!(prefix.next_offset_bits, expected_offset);

    let decoded =
        decode_first_audio_block(&bytes, &[0.0; 73]).expect("conventional first-block mantissas");
    assert_eq!(decoded.channel_baps[0].len(), 73);
    assert_eq!(decoded.channel_mantissas[0].len(), 73);
    assert_eq!(decoded.lfe_bap.as_ref().expect("LFE BAP").len(), 7);
    assert_eq!(
        decoded.lfe_mantissas.as_ref().expect("LFE mantissas").len(),
        7
    );
    assert!(decoded.mantissa_end_offset_bits > decoded.prefix.next_offset_bits);
}

#[test]
fn parses_six_block_coupling_lfe_converter_and_optional_frame_data() {
    let mut bits = Bits::default();
    bits.push(0x0b77, 16);
    bits.push(0, 2);
    bits.push(0, 3);
    bits.push(127, 11); // 256-byte frame
    bits.push(0, 2);
    bits.push(3, 2); // six blocks
    bits.push(7, 3); // 3/2 mode, five full-bandwidth channels
    bits.push(1, 1); // LFE
    bits.push(16, 5);
    bits.push(31, 5);
    bits.push(0, 1); // compre
    bits.push(0, 1); // mixmdate
    bits.push(0, 1); // infomdate
    bits.push(0, 1); // addbsie

    bits.push(1, 1); // per-block exponent strategies
    bits.push(0, 1); // no AHT
    bits.push(2, 2); // channel-specific SNR strategy
    bits.push(1, 1); // transient processing
    bits.push(0, 1); // block switch syntax
    bits.push(1, 1); // dither syntax
    bits.push(1, 1); // bit allocation syntax
    bits.push(0, 1); // frame fast gain syntax
    bits.push(1, 1); // delta bit allocation syntax
    bits.push(1, 1); // skip field syntax
    bits.push(1, 1); // SPX attenuation syntax
    bits.push(1, 1); // coupling in block 0
    for _ in 1..6 {
        bits.push(0, 1); // reuse coupling-in-use state
    }
    for block in 0..6 {
        bits.push(u64::from(block == 0), 2); // coupling D15 then reuse
        for _ in 0..5 {
            bits.push(u64::from(block == 0), 2); // channel D15 then reuse
        }
    }
    bits.push(0b10_0000, 6); // LFE D15 then reuse
    for _ in 0..5 {
        bits.push(0, 5); // converter frame strategy
    }
    bits.push(1, 1); // channel 0 transient data
    bits.push(341, 10);
    bits.push(85, 8);
    for _ in 1..5 {
        bits.push(0, 1);
    }
    bits.push(1, 1); // channel 0 SPX attenuation
    bits.push(17, 5);
    for _ in 1..5 {
        bits.push(0, 1);
    }
    bits.push(1, 1); // block start information present
    bits.push((1_u64 << 55) - 1, 55);
    let expected_offset = bits.0.len();
    let bytes = bits.bytes(256);

    let frame = parse_audio_frame(&bytes).expect("complete six-block frame state");
    assert_eq!(frame.full_bandwidth_channels, 5);
    assert_eq!(frame.coupling_in_use, [true; 6]);
    assert_eq!(frame.coupling_exponent_strategy, vec![1, 0, 0, 0, 0, 0]);
    assert_eq!(frame.channel_exponent_strategy[0], [1; 5]);
    assert!(
        frame.channel_exponent_strategy[1..]
            .iter()
            .all(|strategies| strategies == &[0; 5])
    );
    assert_eq!(
        frame.lfe_exponent_strategy,
        [true, false, false, false, false, false]
    );
    assert_eq!(
        frame.block_start_information,
        Some(openjoc_eac3::AuxiliaryData {
            bit_len: 55,
            bytes: vec![0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xfe],
        })
    );
    assert_eq!(frame.audio_blocks_offset_bits, expected_offset);
}

#[test]
fn decodes_every_frame_exponent_strategy_table_row() {
    let rows = [
        [1, 0, 0, 0, 0, 0],
        [1, 0, 0, 0, 0, 3],
        [1, 0, 0, 0, 2, 0],
        [1, 0, 0, 0, 3, 3],
        [2, 0, 0, 2, 0, 0],
        [2, 0, 0, 2, 0, 3],
        [2, 0, 0, 3, 2, 0],
        [2, 0, 0, 3, 3, 3],
        [2, 0, 1, 0, 0, 0],
        [2, 0, 2, 0, 0, 3],
        [2, 0, 2, 0, 2, 0],
        [2, 0, 2, 0, 3, 3],
        [2, 0, 3, 2, 0, 0],
        [2, 0, 3, 2, 0, 3],
        [2, 0, 3, 3, 2, 0],
        [2, 0, 3, 3, 3, 3],
        [3, 1, 0, 0, 0, 0],
        [3, 1, 0, 0, 0, 3],
        [3, 2, 0, 0, 2, 0],
        [3, 2, 0, 0, 3, 3],
        [3, 2, 0, 2, 0, 0],
        [3, 2, 0, 2, 0, 3],
        [3, 2, 0, 3, 2, 0],
        [3, 2, 0, 3, 3, 3],
        [3, 3, 1, 0, 0, 0],
        [3, 3, 2, 0, 0, 3],
        [3, 3, 2, 0, 2, 0],
        [3, 3, 2, 0, 3, 3],
        [3, 3, 3, 2, 0, 0],
        [3, 3, 3, 2, 0, 3],
        [3, 3, 3, 3, 2, 0],
        [3, 3, 3, 3, 3, 3],
    ];
    for (code, expected) in rows.into_iter().enumerate() {
        assert_eq!(
            decode_frame_exponent_strategy(u8::try_from(code).expect("code")),
            Ok(expected)
        );
    }
    assert_eq!(
        decode_frame_exponent_strategy(32),
        Err(Eac3Error::InvalidFrameExponentStrategy { actual: 32 })
    );
}

#[test]
fn derives_six_block_channel_strategies_from_frame_code() {
    let mut bits = Bits::default();
    bits.push(0x0b77, 16);
    bits.push(0, 2);
    bits.push(0, 3);
    bits.push(63, 11);
    bits.push(0, 2);
    bits.push(3, 2);
    bits.push(1, 3); // mono: no coupling syntax
    bits.push(0, 1);
    bits.push(16, 5);
    bits.push(31, 5);
    bits.push(0, 1); // compre
    bits.push(0, 1); // mixmdate
    bits.push(0, 1); // infomdate
    bits.push(0, 1); // addbsie

    bits.push(0, 1); // frame-based exponent strategy
    bits.push(0, 1); // no AHT
    let snr_position = bits.0.len();
    bits.push(2, 2); // channel-specific SNR strategy
    bits.push(0, 7); // transient through skip-field syntax disabled
    bits.push(0, 1); // SPX attenuation
    bits.push(30, 5); // channel frame strategy
    bits.push(0, 5); // converter frame strategy
    bits.push(0, 1); // no block-start information
    let expected_offset = bits.0.len();
    let mut reserved = bits.clone();
    reserved.set(snr_position, 3, 2);
    let bytes = bits.bytes(128);

    let frame = parse_audio_frame(&bytes).expect("frame-based strategies");
    assert_eq!(
        frame.channel_exponent_strategy,
        vec![vec![3], vec![3], vec![3], vec![3], vec![2], vec![0]]
    );
    assert_eq!(frame.audio_blocks_offset_bits, expected_offset);
    assert_eq!(
        parse_audio_frame(&reserved.bytes(128)),
        Err(Eac3Error::ReservedSnrOffsetStrategy)
    );
}

#[test]
fn computes_the_normative_block_start_information_length() {
    assert_eq!(block_start_information_length(128, 1), Ok(0));
    assert_eq!(block_start_information_length(128, 6), Ok(50));
    assert_eq!(block_start_information_length(130, 6), Ok(55));
    assert_eq!(block_start_information_length(256, 3), Ok(22));
    assert_eq!(
        block_start_information_length(0, 6),
        Err(Eac3Error::InvalidBlockStartDimensions {
            frame_size: 0,
            audio_blocks: 6,
        })
    );
    assert_eq!(
        block_start_information_length(128, 4),
        Err(Eac3Error::InvalidBlockStartDimensions {
            frame_size: 128,
            audio_blocks: 4,
        })
    );
}

#[test]
fn derives_channel_mantissa_and_exponent_group_counts() {
    assert_eq!(channel_end_mantissa(0), Ok(73));
    assert_eq!(channel_end_mantissa(60), Ok(253));
    assert_eq!(
        channel_end_mantissa(61),
        Err(Eac3Error::InvalidChannelBandwidthCode { actual: 61 })
    );

    assert_eq!(channel_exponent_group_count(73, 1), Ok(24));
    assert_eq!(channel_exponent_group_count(73, 2), Ok(12));
    assert_eq!(channel_exponent_group_count(73, 3), Ok(6));
    assert_eq!(
        channel_exponent_group_count(73, 0),
        Err(Eac3Error::InvalidExponentStrategy { actual: 0 })
    );
}

#[test]
fn decodes_grouped_exponents_for_every_strategy() {
    assert_eq!(decode_exponents(10, &[62, 62], 1, 7), Ok(vec![10; 7]));
    assert_eq!(decode_exponents(10, &[62], 2, 7), Ok(vec![10; 7]));
    assert_eq!(decode_exponents(10, &[62], 3, 13), Ok(vec![10; 13]));

    assert_eq!(decode_exponents(6, &[0], 1, 4), Ok(vec![6, 4, 2, 0]));
    assert_eq!(decode_exponents(0, &[124], 1, 4), Ok(vec![0, 2, 4, 6]));
}

#[test]
fn rejects_malformed_grouped_exponents() {
    assert_eq!(
        decode_exponents(10, &[], 1, 0),
        Err(Eac3Error::InvalidExponentDimensions { end_mantissa: 0 })
    );
    assert_eq!(
        decode_exponents(10, &[125], 1, 4),
        Err(Eac3Error::InvalidGroupedExponent { actual: 125 })
    );
    assert_eq!(
        decode_exponents(1, &[0], 1, 4),
        Err(Eac3Error::ExponentOutOfRange { actual: -1 })
    );
    assert_eq!(
        decode_exponents(10, &[], 1, 4),
        Err(Eac3Error::ExponentGroupCountMismatch {
            expected: 1,
            actual: 0,
        })
    );
}

#[test]
fn derives_every_spectral_extension_subband_boundary() {
    let expected_begin = [2, 3, 4, 5, 6, 7, 9, 11];
    let expected_end = [5, 6, 7, 9, 11, 13, 15, 17];
    for (begin_code, &begin) in expected_begin.iter().enumerate() {
        for (end_code, &end) in expected_end.iter().enumerate() {
            let begin_code = u8::try_from(begin_code).expect("three-bit begin code");
            let end_code = u8::try_from(end_code).expect("three-bit end code");
            let actual = spx_subband_range(begin_code, end_code);
            if begin < end {
                assert_eq!(actual, Ok((begin, end)));
            } else {
                assert_eq!(
                    actual,
                    Err(Eac3Error::InvalidSpectralExtensionRange { begin, end })
                );
            }
        }
    }
}

#[test]
fn rejects_spectral_extension_codes_wider_than_the_normative_fields() {
    assert_eq!(
        spx_subband_range(8, 0),
        Err(Eac3Error::InvalidSpectralExtensionCode {
            begin_code: 8,
            end_code: 0,
        })
    );
    assert_eq!(
        spx_subband_range(0, 8),
        Err(Eac3Error::InvalidSpectralExtensionCode {
            begin_code: 0,
            end_code: 8,
        })
    );
}

#[test]
fn parses_aht_flags_only_for_single_exponent_regions() {
    let mut bits = Bits::default();
    bits.push(0x0b77, 16);
    bits.push(0, 2);
    bits.push(0, 3);
    bits.push(63, 11);
    bits.push(0, 2);
    bits.push(3, 2);
    bits.push(1, 3); // mono
    bits.push(0, 1);
    bits.push(16, 5);
    bits.push(31, 5);
    bits.push(0, 1); // compre
    bits.push(0, 1); // mixmdate
    bits.push(0, 1); // infomdate
    bits.push(0, 1); // addbsie

    bits.push(1, 1); // per-block exponent strategies
    bits.push(1, 1); // AHT syntax
    bits.push(1, 2); // SNR strategy
    bits.push(0, 8); // frame syntax flags
    for block in 0..6 {
        bits.push(u64::from(block == 0), 2);
    }
    bits.push(0, 5); // converter exponent strategy
    bits.push(1, 1); // mono channel uses AHT (one exponent region)
    bits.push(0, 1); // no block-start information
    let expected_offset = bits.0.len();
    let bytes = bits.bytes(128);

    let frame = parse_audio_frame(&bytes).expect("AHT frame flags");
    assert!(!frame.coupling_aht_in_use);
    assert_eq!(frame.channel_aht_in_use, [true]);
    assert!(!frame.lfe_aht_in_use);
    assert_eq!(frame.audio_blocks_offset_bits, expected_offset);
}

#[test]
fn groups_sequential_independent_and_dependent_substreams_into_access_units() {
    let frames = [
        frame(0, 0, 16, 0, 3),
        frame(1, 0, 16, 0, 3),
        frame(1, 1, 16, 0, 3),
        frame(0, 1, 16, 0, 3),
        frame(0, 0, 16, 0, 3),
        frame(1, 0, 16, 0, 3),
        frame(1, 1, 16, 0, 3),
        frame(0, 1, 16, 0, 3),
    ]
    .concat();
    let indexed = index_syncframes(&frames).expect("indexed frames");
    let units = group_access_units(&indexed).expect("valid substream sequence");
    assert_eq!(units.len(), 2);
    assert_eq!(units[0].first_frame, 0);
    assert_eq!(units[0].frame_count, 4);
    assert_eq!(units[1].first_frame, 4);
    assert_eq!(units[1].frame_count, 4);
    assert_eq!(units[0].sample_rate, 48_000);
    assert_eq!(units[0].samples, 1536);
}

#[test]
fn rejects_nonsequential_substreams_and_timing_mismatch() {
    let bad_dependent = [frame(0, 0, 16, 0, 3), frame(1, 1, 16, 0, 3)].concat();
    assert_eq!(
        group_access_units(&index_syncframes(&bad_dependent).expect("headers")),
        Err(Eac3Error::NonsequentialDependentSubstream {
            expected: 0,
            actual: 1,
        })
    );

    let bad_independent = [frame(0, 0, 16, 0, 3), frame(0, 2, 16, 0, 3)].concat();
    assert_eq!(
        group_access_units(&index_syncframes(&bad_independent).expect("headers")),
        Err(Eac3Error::NonsequentialIndependentSubstream {
            expected: 1,
            actual: 2,
        })
    );

    let bad_timing = [frame(0, 0, 16, 0, 3), frame(1, 0, 16, 1, 3)].concat();
    assert_eq!(
        group_access_units(&index_syncframes(&bad_timing).expect("headers")),
        Err(Eac3Error::SubstreamTimingMismatch { frame: 1 })
    );
}

fn auxdata_frame(auxdatae: bool, declared_bits: u16, payload: &[u8]) -> Vec<u8> {
    let mut bits = Bits::default();
    bits.push(0x0b77, 16);
    bits.push(0, 2);
    bits.push(0, 3);
    bits.push(7, 11); // 16 bytes
    bits.push(0, 2);
    bits.push(0, 2);
    bits.0.resize(128, false);
    if auxdatae {
        bits.set(96, u64::from(declared_bits), 14);
        bits.set(110, 1, 1);
        let payload_start = 96 - payload.len() * 8;
        for (index, byte) in payload.iter().copied().enumerate() {
            bits.set(payload_start + index * 8, u64::from(byte), 8);
        }
    }
    bits.bytes(16)
}

#[test]
fn extracts_forward_ordered_auxdata_from_the_frame_end() {
    let payload = [0x58, 0x38, 0x00, 0x00];
    let extracted = extract_auxdata(&auxdata_frame(true, 32, &payload))
        .expect("valid auxdata")
        .expect("present auxdata");
    assert_eq!(extracted.bit_len, 32);
    assert_eq!(extracted.bytes, payload);

    assert_eq!(extract_auxdata(&auxdata_frame(false, 0, &[])), Ok(None));
    assert_eq!(
        extract_auxdata(&auxdata_frame(true, 100, &[])),
        Err(Eac3Error::AuxDataLengthOutOfRange {
            declared: 100,
            available: 96,
        })
    );
}

#[test]
fn parses_a_bounded_emdf_container_directly_from_auxdata() {
    let mut container = Bits::default();
    container.push(0, 2); // EMDF version
    container.push(0, 3); // key
    container.push(0, 5); // terminator
    container.push(1, 2); // primary protection: 8 bits
    container.push(0, 2); // no secondary protection
    container.push(0, 8);
    let container = container.bytes(3);
    let mut emdf = vec![0x58, 0x38, 0, 3];
    emdf.extend_from_slice(&container);
    let frame = auxdata_frame(true, 56, &emdf);

    let parsed = extract_aux_emdf(&frame)
        .expect("valid carrier")
        .expect("EMDF present");
    assert_eq!(parsed.container.version, 0);
    assert!(parsed.container.payloads.is_empty());
    assert_eq!(parsed.bytes_consumed, 7);
}

fn joc_emdf() -> Vec<u8> {
    let mut container = Bits::default();
    container.push(0, 2);
    container.push(0, 3);
    for (id, payload) in [(11, 0xa5), (14, 0x5a)] {
        container.push(id, 5);
        container.push(0, 1); // no sample offset
        container.push(0, 1); // no duration
        container.push(1, 1); // group ID
        container.push(1, 2);
        container.push(0, 1); // variable-bits stop
        container.push(1, 1); // codec data present
        container.push(0, 8); // reserved codec data
        container.push(0, 1); // retain unknown payload
        container.push(1, 1); // frame aligned
        container.push(0, 1); // create duplicate
        container.push(0, 1); // remove duplicate
        container.push(0, 5); // priority
        container.push(0, 2); // proc_allowed
        container.push(1, 8); // one payload byte
        container.push(0, 1); // variable-bits stop
        container.push(payload, 8);
    }
    container.push(0, 5);
    container.push(1, 2);
    container.push(0, 2);
    container.push(0, 8);
    let container = container.padded_bytes();
    let mut emdf = vec![0x58, 0x38];
    emdf.extend_from_slice(
        &u16::try_from(container.len())
            .expect("container length")
            .to_be_bytes(),
    );
    emdf.extend_from_slice(&container);
    emdf
}

fn joc_carrier_frame(stream_type: u8, substream_id: u8, emdf: Option<&[u8]>) -> Vec<u8> {
    let size = 64;
    let mut bits = Bits::default();
    bits.push(0x0b77, 16);
    bits.push(u64::from(stream_type), 2);
    bits.push(u64::from(substream_id), 3);
    bits.push(31, 11);
    bits.push(0, 2);
    bits.push(3, 2);
    bits.push(2, 3);
    bits.push(0, 1);
    bits.push(16, 5);
    bits.push(31, 5);
    bits.push(0, 1);
    if stream_type == 1 {
        bits.push(0, 1); // no custom channel map
    }
    bits.push(0, 1);
    bits.push(0, 1);
    bits.push(u64::from(emdf.is_some()), 1);
    if emdf.is_some() {
        bits.push(1, 6);
        bits.push(0x01, 8);
        bits.push(2, 8);
    }
    bits.0.resize(size * 8, false);
    if let Some(emdf) = emdf {
        let length_position = size * 8 - 32;
        bits.set(
            length_position,
            u64::try_from(emdf.len() * 8).expect("EMDF bits"),
            14,
        );
        bits.set(size * 8 - 18, 1, 1);
        let start = length_position - emdf.len() * 8;
        for (index, byte) in emdf.iter().copied().enumerate() {
            bits.set(start + index * 8, u64::from(byte), 8);
        }
    }
    bits.bytes(size)
}

#[test]
fn extracts_joc_profile_from_the_last_dependent_substream() {
    let emdf = joc_emdf();
    let bytes = [
        joc_carrier_frame(0, 0, None),
        joc_carrier_frame(1, 0, None),
        joc_carrier_frame(1, 1, Some(&emdf)),
    ]
    .concat();
    let frames = index_syncframes(&bytes).expect("frames");
    let units = group_access_units(&frames).expect("unit");
    let metadata = extract_aux_joc_access_unit(&bytes, &frames, units[0])
        .expect("valid JOC carrier")
        .expect("JOC metadata");
    assert_eq!(metadata.carrier_frame, 2);
    assert_eq!(metadata.complexity_index, 2);
    assert_eq!(metadata.oamd, [0xa5]);
    assert_eq!(metadata.joc, [0x5a]);
}

#[test]
fn rejects_joc_profile_before_the_last_dependent_substream() {
    let emdf = joc_emdf();
    let bytes = [
        joc_carrier_frame(0, 0, None),
        joc_carrier_frame(1, 0, Some(&emdf)),
        joc_carrier_frame(1, 1, None),
    ]
    .concat();
    let frames = index_syncframes(&bytes).expect("frames");
    let unit = group_access_units(&frames).expect("unit")[0];
    assert_eq!(
        extract_aux_joc_access_unit(&bytes, &frames, unit),
        Err(Eac3Error::InvalidJocCarrierPlacement {
            carrier_frame: 1,
            required_frame: 2,
        })
    );
}
