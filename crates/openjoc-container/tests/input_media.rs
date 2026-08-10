use openjoc_container::{
    InputMediaKind, IsoBmffSample, SeekableIsoBmffEc3Reader, detect_media,
    parse_audio_probe_output, parse_packet_probe_output,
};
use std::io::{Cursor, Read};

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
