use openjoc_eac3::{
    Eac3Error, JocAddbsi, StreamType, index_syncframes, parse_bsi, parse_joc_addbsi,
    parse_syncframe_header,
};

#[derive(Default)]
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
