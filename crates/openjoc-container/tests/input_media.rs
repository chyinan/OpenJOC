use openjoc_container::cmaf::{
    CmafJocTrack, CmafTrackMetadata, Ec3SpecificBox, Ec3SubstreamConfig,
};
use openjoc_container::{
    InputMediaError, InputMediaKind, IsoBmffSample, SeekableIsoBmffEc3Reader, detect_media,
    load_eac3_with_tools, parse_audio_probe_output, parse_packet_probe_output,
};
use openjoc_eac3::index_syncframes;
use std::{
    fs,
    io::{Cursor, Read},
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

fn push_bits(bits: &mut Vec<bool>, value: u64, width: usize) {
    for shift in (0..width).rev() {
        bits.push(value & (1_u64 << shift) != 0);
    }
}

fn cmaf_i0_sample() -> Vec<u8> {
    let size = 128;
    let mut bits = Vec::new();
    push_bits(&mut bits, 0x0b77, 16);
    push_bits(&mut bits, 0, 2);
    push_bits(&mut bits, 0, 3);
    push_bits(
        &mut bits,
        u64::try_from(size / 2 - 1).expect("frame words"),
        11,
    );
    push_bits(&mut bits, 0, 2);
    push_bits(&mut bits, 3, 2);
    push_bits(&mut bits, 7, 3);
    push_bits(&mut bits, 0, 1);
    push_bits(&mut bits, 16, 5);
    push_bits(&mut bits, 31, 5);
    push_bits(&mut bits, 0, 1);
    push_bits(&mut bits, 0, 1);
    push_bits(&mut bits, 0, 1);
    push_bits(&mut bits, 0, 1);
    let mut frame = vec![0_u8; size];
    for (index, bit) in bits.into_iter().enumerate() {
        if bit {
            frame[index / 8] |= 0x80 >> (index % 8);
        }
    }
    frame
}

fn cmaf_track() -> CmafJocTrack {
    CmafJocTrack::new(CmafTrackMetadata {
        sample_entry: *b"ec-3",
        timescale: 48_000,
        sample_rate: 48_000,
        decoder_config: Some(Ec3SpecificBox {
            data_rate_kbps: 768,
            independent_substreams: vec![Ec3SubstreamConfig {
                fscod: 0,
                bsid: 16,
                asvc: false,
                bsmod: 0,
                acmod: 7,
                lfe_on: false,
                dependent_substreams: 0,
                chan_loc: None,
            }],
            flag_ec3_extension_type_a: true,
            complexity_index_type_a: 16,
        }),
        compatibility_brands: vec![*b"ceao"],
    })
    .expect("CMAF track")
}

#[test]
fn detects_raw_eac3_by_syncword_signature() {
    assert_eq!(detect_media(&[0x0b, 0x77, 0, 0]), InputMediaKind::RawEac3);
}

#[test]
fn detects_iso_bmff_by_ftyp_box_signature() {
    assert_eq!(
        detect_media(&[0, 0, 0, 24, b'f', b't', b'y', b'p']),
        InputMediaKind::IsoBmff
    );
}

#[test]
fn does_not_mislabel_unknown_input_as_eac3() {
    assert_eq!(detect_media(&[0, 0, 0, 0]), InputMediaKind::Unknown);
}

#[test]
fn parses_exactly_one_eac3_audio_track() {
    let tracks = parse_audio_probe_output("0,eac3\n").expect("one track");
    assert_eq!(tracks, vec![(0, "eac3".to_owned())]);
}

#[test]
fn rejects_malformed_probe_rows() {
    assert!(parse_audio_probe_output("not-a-track\n").is_err());
}

#[test]
fn parses_packet_rows_and_rejects_other_streams() {
    let samples =
        parse_packet_probe_output("0,4,8\n0,2,12,Skip Samples,1\n", 0).expect("packet rows");
    assert_eq!(
        samples,
        vec![
            IsoBmffSample { offset: 8, size: 4 },
            IsoBmffSample {
                offset: 12,
                size: 2
            },
        ]
    );
    assert!(parse_packet_probe_output("1,4,8\n", 0).is_err());
}

#[test]
fn seekable_reader_delivers_one_sample_at_a_time_and_supports_read() {
    let source = b"headerAAABBBBtail".to_vec();
    let samples = vec![
        IsoBmffSample { offset: 6, size: 3 },
        IsoBmffSample { offset: 9, size: 4 },
    ];
    let mut reader =
        SeekableIsoBmffEc3Reader::new(Cursor::new(source), samples, 8).expect("seekable reader");
    assert_eq!(
        reader.next_sample().expect("sample 0"),
        Some(b"AAA".to_vec())
    );
    assert_eq!(
        reader.next_sample().expect("sample 1"),
        Some(b"BBBB".to_vec())
    );
    assert_eq!(reader.next_sample().expect("EOF"), None);
    let stats = reader.stats();
    assert_eq!(stats.samples_delivered, 2);
    assert_eq!(stats.sample_count, 2);
    assert_eq!(stats.max_current_sample_bytes, 4);
    assert_eq!(stats.max_samples_simultaneously_retained, 1);
    assert_eq!(stats.derived_sample_index_entries, 2);
    assert_eq!(stats.cursor_state_entries, 0);

    let source = b"headerAAABBBBtail".to_vec();
    let samples = vec![
        IsoBmffSample { offset: 6, size: 3 },
        IsoBmffSample { offset: 9, size: 4 },
    ];
    let mut reader =
        SeekableIsoBmffEc3Reader::new(Cursor::new(source), samples, 8).expect("seekable reader");
    let mut delivered = Vec::new();
    reader.read_to_end(&mut delivered).expect("read samples");
    assert_eq!(delivered, b"AAABBBB");
}

#[test]
fn seekable_reader_rejects_sample_beyond_file_bounds() {
    let samples = vec![IsoBmffSample { offset: 9, size: 4 }];
    let mut reader = SeekableIsoBmffEc3Reader::new(Cursor::new(b"short".to_vec()), samples, 8)
        .expect("seekable reader");
    assert!(reader.next_sample().is_err());
}

#[test]
fn seekable_reader_rejects_sample_larger_than_limit_before_allocation() {
    let samples = vec![IsoBmffSample {
        offset: 0,
        size: 65,
    }];
    let mut reader =
        SeekableIsoBmffEc3Reader::new(Cursor::new(vec![0_u8; 128]), samples, 64).expect("reader");
    assert!(matches!(
        reader.next_sample(),
        Err(InputMediaError::DemuxOutputTooLarge { limit: 64 })
    ));
}

#[test]
fn seekable_reader_rejects_offset_overflow_without_allocation() {
    let samples = vec![IsoBmffSample {
        offset: u64::MAX,
        size: usize::MAX,
    }];
    let mut reader =
        SeekableIsoBmffEc3Reader::new(Cursor::new(vec![0_u8; 1]), samples, 64).expect("reader");
    assert!(matches!(
        reader.next_sample(),
        Err(InputMediaError::DemuxOutputTooLarge { limit: 64 })
    ));
}

#[test]
fn seekable_reader_validates_cmaf_sample_before_returning_exact_bytes() {
    let sample = cmaf_i0_sample();
    let source = [b"prefix".as_slice(), sample.as_slice(), b"suffix"].concat();
    let mut reader = SeekableIsoBmffEc3Reader::new(
        Cursor::new(source),
        vec![IsoBmffSample {
            offset: 6,
            size: sample.len(),
        }],
        4096,
    )
    .expect("reader");
    assert_eq!(
        reader.next_cmaf_sample(&cmaf_track()).expect("CMAF sample"),
        Some(sample)
    );
}

#[test]
fn malformed_raw_file_stays_bounded_and_codec_parser_rejects_it() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("openjoc-malformed-raw-{nonce}.ec3"));
    fs::write(&path, [0x0b, 0x77, 0, 0]).expect("write malformed raw");
    let loaded = load_eac3_with_tools(
        &path,
        Path::new("missing-ffprobe"),
        Path::new("missing-ffmpeg"),
        64,
    )
    .expect("raw loading is intentionally a bounded byte boundary");
    assert_eq!(loaded.kind, InputMediaKind::RawEac3);
    assert!(index_syncframes(&loaded.bytes).is_err());
    fs::remove_file(path).expect("remove test file");
}
