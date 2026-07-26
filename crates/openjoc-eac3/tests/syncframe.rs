use openjoc_eac3::{
    Eac3Error, JocAddbsi, StreamType, extract_aux_emdf, extract_aux_joc_access_unit,
    extract_auxdata, group_access_units, index_syncframes, parse_bsi, parse_joc_addbsi,
    parse_syncframe_header, validate_complexity_index,
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
