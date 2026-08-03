use openjoc_container::{InputMediaKind, detect_media, parse_audio_probe_output};

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
