// pattern: Functional Core

use openjoc_container::cmaf::{
    CmafJocTrack, CmafTrackMetadata, Ec3SpecificBox, Ec3SubstreamConfig, parse_ec3_specific_box,
};
use openjoc_container::open_seekable_iso_bmff;
use openjoc_eac3::{StreamType, parse_bsi, parse_syncframe_header};
use std::{path::Path, process::Command};

#[derive(Default)]
struct Bits(Vec<bool>);

impl Bits {
    fn push(&mut self, value: u64, width: usize) {
        for shift in (0..width).rev() {
            self.0.push(value & (1_u64 << shift) != 0);
        }
    }

    fn bytes(self) -> Vec<u8> {
        let mut bits = self.0;
        while bits.len() % 8 != 0 {
            bits.push(false);
        }
        let mut bytes = vec![0_u8; bits.len() / 8];
        for (index, bit) in bits.into_iter().enumerate() {
            if bit {
                bytes[index / 8] |= 0x80 >> (index % 8);
            }
        }
        bytes
    }
}

fn dec3_box(dependent_count: u8, chan_loc: u16, extension: bool) -> Vec<u8> {
    let mut bits = Bits::default();
    bits.push(768, 13); // data_rate
    bits.push(0, 3); // num_ind_sub: one independent substream
    bits.push(0, 2); // fscod: 48 kHz
    bits.push(16, 5); // bsid
    bits.push(0, 1); // reserved
    bits.push(0, 1); // asvc
    bits.push(0, 3); // bsmod
    bits.push(7, 3); // acmod: 5 full-band channels
    bits.push(0, 1); // lfeon
    bits.push(0, 3); // reserved
    bits.push(u64::from(dependent_count), 4);
    if dependent_count > 0 {
        bits.push(u64::from(chan_loc), 9);
    } else {
        bits.push(0, 1);
    }
    bits.push(0, 7); // reserved
    bits.push(u64::from(extension), 1); // flag_ec3_extension_type_a
    bits.push(if extension { 16 } else { 0 }, 8); // complexity_index_type_a
    let payload = bits.bytes();
    let size = u32::try_from(payload.len() + 8).expect("box size");
    let mut output = size.to_be_bytes().to_vec();
    output.extend_from_slice(b"dec3");
    output.extend_from_slice(&payload);
    output
}

fn eac3_frame(stream_type: u8, substream_id: u8, blocks: u8, size: usize) -> Vec<u8> {
    assert_eq!(size % 2, 0);
    let block_code = match blocks {
        1 => 0,
        2 => 1,
        3 => 2,
        6 => 3,
        _ => panic!("unsupported test block count"),
    };
    let mut bits = Bits::default();
    bits.push(0x0b77, 16);
    bits.push(u64::from(stream_type), 2);
    bits.push(u64::from(substream_id), 3);
    bits.push(u64::try_from(size / 2 - 1).expect("frame words"), 11);
    bits.push(0, 2); // 48 kHz
    bits.push(block_code, 2);
    bits.push(7, 3); // 5.1-compatible independent description
    bits.push(0, 1); // no LFE
    bits.push(16, 5); // E-AC-3
    bits.push(31, 5); // dialnorm
    bits.push(0, 1); // no compression
    if stream_type == 1 {
        bits.push(1, 1); // chanmap present
        bits.push(0x0200, 16); // Lrs/Rrs pair for D0
    }
    bits.push(0, 1); // no mixing metadata
    bits.push(0, 1); // no informational metadata
    if stream_type == 0 && blocks != 6 {
        bits.push(1, 1); // convsync for short-frame negative fixtures
    }
    bits.push(0, 1); // no addbsi
    let mut output = bits.bytes();
    output.resize(size, 0);
    output
}

fn valid_track(dependent_count: u8) -> CmafJocTrack {
    let config = parse_ec3_specific_box(&dec3_box(dependent_count, 2, true)).expect("dec3");
    CmafJocTrack::new(CmafTrackMetadata {
        sample_entry: *b"ec-3",
        timescale: 48_000,
        sample_rate: 48_000,
        decoder_config: Some(config),
        compatibility_brands: vec![*b"ceao"],
    })
    .expect("valid CMAF track")
}

#[test]
fn dec3_parser_retains_normative_joc_configuration() {
    let parsed = parse_ec3_specific_box(&dec3_box(1, 2, true)).expect("parse dec3");
    assert_eq!(parsed.data_rate_kbps, 768);
    assert_eq!(parsed.independent_substreams.len(), 1);
    assert_eq!(parsed.independent_substreams[0].dependent_substreams, 1);
    assert_eq!(parsed.independent_substreams[0].chan_loc, Some(2));
    assert!(parsed.flag_ec3_extension_type_a);
    assert_eq!(parsed.complexity_index_type_a, 16);
}

#[test]
fn cmaf_track_requires_ec3_entry_and_joc_dec3_extension() {
    let ordinary = parse_ec3_specific_box(&dec3_box(0, 0, false)).expect("ordinary dec3");
    let error = CmafJocTrack::new(CmafTrackMetadata {
        sample_entry: *b"mp4a",
        timescale: 48_000,
        sample_rate: 48_000,
        decoder_config: Some(ordinary.clone()),
        compatibility_brands: vec![],
    })
    .expect_err("wrong entry must fail");
    assert!(error.to_string().contains("ec-3"));

    let error = CmafJocTrack::new(CmafTrackMetadata {
        sample_entry: *b"ec-3",
        timescale: 48_000,
        sample_rate: 48_000,
        decoder_config: Some(ordinary),
        compatibility_brands: vec![],
    })
    .expect_err("ordinary E-AC-3 is not a JOC track");
    assert!(error.to_string().contains("JOC extension"));

    let error = CmafJocTrack::new(CmafTrackMetadata {
        sample_entry: *b"ec-3",
        timescale: 48_000,
        sample_rate: 48_000,
        decoder_config: None,
        compatibility_brands: vec![],
    })
    .expect_err("missing dec3 must fail");
    assert!(error.to_string().contains("dec3"));
}

#[test]
fn dec3_parser_rejects_malformed_box_and_track_rejects_core_limits() {
    assert!(parse_ec3_specific_box(&[0, 0, 0, 8, b'd', b'e', b'c', b'3']).is_err());
    let mut wrong_type = dec3_box(0, 0, true);
    wrong_type[7] = b'4';
    assert!(parse_ec3_specific_box(&wrong_type).is_err());

    let mut nonzero_complexity_without_extension = dec3_box(0, 0, false);
    *nonzero_complexity_without_extension
        .last_mut()
        .expect("complexity") = 16;
    assert!(parse_ec3_specific_box(&nonzero_complexity_without_extension).is_err());

    let mut too_large = parse_ec3_specific_box(&dec3_box(0, 0, true)).expect("dec3");
    too_large.data_rate_kbps = 3025;
    let error = CmafJocTrack::new(CmafTrackMetadata {
        sample_entry: *b"ec-3",
        timescale: 48_000,
        sample_rate: 48_000,
        decoder_config: Some(too_large),
        compatibility_brands: vec![],
    })
    .expect_err("Core bitrate limit");
    assert!(error.to_string().contains("3024"));

    let multiple = parse_ec3_specific_box(&dec3_box(2, 2, true)).expect("multiple D config");
    let error = CmafJocTrack::new(CmafTrackMetadata {
        sample_entry: *b"ec-3",
        timescale: 48_000,
        sample_rate: 48_000,
        decoder_config: Some(multiple),
        compatibility_brands: vec![],
    })
    .expect_err("D1+ config");
    assert!(error.to_string().contains("at most dependent substream D0"));
}

#[test]
fn dec3_parser_accepts_only_normative_zero_reserved_tail_bytes() {
    let mut with_reserved_tail = dec3_box(0, 0, true);
    with_reserved_tail.extend_from_slice(&[0, 0]);
    let size = u32::try_from(with_reserved_tail.len()).expect("box size");
    with_reserved_tail[..4].copy_from_slice(&size.to_be_bytes());
    assert!(parse_ec3_specific_box(&with_reserved_tail).is_ok());
    *with_reserved_tail.last_mut().expect("reserved byte") = 1;
    assert!(parse_ec3_specific_box(&with_reserved_tail).is_err());
}

#[test]
fn cmaf_sample_preserves_i0_bytes_and_duration() {
    let track = valid_track(0);
    let sample = eac3_frame(0, 0, 6, 128);
    let validated = track.validate_sample(&sample).expect("I0 sample");
    assert_eq!(validated.bytes, sample.as_slice());
    assert_eq!(validated.frame_offsets, vec![0]);
    assert_eq!(validated.audio_duration, 1536);
}

#[test]
fn cmaf_sample_preserves_i0_then_d0_order() {
    let track = valid_track(1);
    let mut sample = eac3_frame(0, 0, 6, 128);
    sample.extend_from_slice(&eac3_frame(1, 0, 6, 128));
    let validated = track.validate_sample(&sample).expect("I0+D0 sample");
    assert_eq!(validated.bytes, sample.as_slice());
    assert_eq!(validated.frame_offsets, vec![0, 128]);
    assert_eq!(validated.audio_duration, 1536);
}

#[test]
fn cmaf_sample_rejects_d1_short_blocks_type2_reversed_and_truncated() {
    let track = valid_track(1);
    let i0 = eac3_frame(0, 0, 6, 128);
    let d0 = eac3_frame(1, 0, 6, 128);

    let mut d1 = d0.clone();
    d1[2] = (d1[2] & 0xc7) | (1 << 3);
    assert!(
        track
            .validate_sample(&[i0.clone(), d0.clone(), d1].concat())
            .is_err()
    );

    let short = eac3_frame(0, 0, 1, 128);
    let short_sample = (0..6).flat_map(|_| short.clone()).collect::<Vec<_>>();
    assert!(track.validate_sample(&short_sample).is_err());

    let mut type2 = i0.clone();
    type2[2] = (type2[2] & 0x3f) | (2 << 6);
    assert!(track.validate_sample(&type2).is_err());

    assert!(track.validate_sample(&[d0, i0.clone()].concat()).is_err());
    assert!(track.validate_sample(&i0[..64]).is_err());
}

#[test]
fn cmaf_sample_metadata_is_checked_against_in_band_headers() {
    let track = valid_track(1);
    let mut sample = eac3_frame(0, 0, 6, 128);
    let mut d0 = eac3_frame(1, 0, 6, 128);
    d0[4] |= 0x40; // 44.1 kHz, contradictory to the dec3/track contract
    sample.extend_from_slice(&d0);
    assert!(track.validate_sample(&sample).is_err());

    let mut d0 = eac3_frame(1, 0, 6, 128);
    // The D0 channel map starts at bit 52 in this deterministic header; add
    // the unrepresentable Lts/Rts position to the valid 0x0200 map.
    let position = 52 + 13;
    d0[position / 8] |= 0x80 >> (position % 8);
    assert!(
        track
            .validate_sample(&[eac3_frame(0, 0, 6, 128), d0].concat())
            .is_err()
    );

    let mut empty_map = eac3_frame(1, 0, 6, 128);
    for bit in 0..16 {
        let position = 52 + bit;
        empty_map[position / 8] &= !(0x80 >> (position % 8));
    }
    assert!(
        track
            .validate_sample(&[eac3_frame(0, 0, 6, 128), empty_map].concat())
            .is_err()
    );
}

#[test]
fn test_fixture_headers_are_the_expected_eac3_shape() {
    let frame = eac3_frame(0, 0, 6, 128);
    let header = parse_syncframe_header(&frame).expect("header");
    assert_eq!(header.stream_type, StreamType::Independent);
    assert_eq!(header.audio_blocks, 6);
    assert_eq!(header.samples, 1536);
    let bsi = parse_bsi(&frame).expect("bsi");
    assert_eq!(bsi.bitstream_id, 16);
    assert_eq!(bsi.audio_coding_mode, 7);
}

fn bmff_box(kind: [u8; 4], payload: &[u8]) -> Vec<u8> {
    let size = u32::try_from(payload.len() + 8).expect("BMFF box size");
    let mut output = size.to_be_bytes().to_vec();
    output.extend_from_slice(&kind);
    output.extend_from_slice(payload);
    output
}

fn bmff_full_box(kind: [u8; 4], flags: u32, payload: &[u8]) -> Vec<u8> {
    let mut full = flags.to_be_bytes().to_vec();
    full.extend_from_slice(payload);
    bmff_box(kind, &full)
}

fn audio_sample_entry(dec3: &[u8]) -> Vec<u8> {
    let mut payload = vec![0_u8; 6];
    payload.extend_from_slice(&1_u16.to_be_bytes()); // data_reference_index
    payload.extend_from_slice(&[0_u8; 8]);
    payload.extend_from_slice(&2_u16.to_be_bytes()); // channelcount (ignored by ETSI)
    payload.extend_from_slice(&16_u16.to_be_bytes()); // samplesize (ignored by ETSI)
    payload.extend_from_slice(&[0_u8; 4]);
    payload.extend_from_slice(&(48_000_u32 << 16).to_be_bytes());
    payload.extend_from_slice(dec3);
    bmff_box(*b"ec-3", &payload)
}

fn cmaf_init_segment(dec3: &[u8]) -> Vec<u8> {
    let mut ftyp_payload = Vec::new();
    ftyp_payload.extend_from_slice(b"cmfc");
    ftyp_payload.extend_from_slice(&0_u32.to_be_bytes());
    for brand in [b"isom", b"iso6", b"cmfc", b"ceao"] {
        ftyp_payload.extend_from_slice(brand);
    }
    let ftyp = bmff_box(*b"ftyp", &ftyp_payload);

    let mut movie_header_fields = vec![0_u8; 96];
    movie_header_fields[8..12].copy_from_slice(&48_000_u32.to_be_bytes());
    let movie_header_box = bmff_full_box(*b"mvhd", 0, &movie_header_fields);

    let mut tkhd_fields = vec![0_u8; 80];
    tkhd_fields[8..12].copy_from_slice(&1_u32.to_be_bytes());
    let track_header_box = bmff_full_box(*b"tkhd", 0x0000_0007, &tkhd_fields);

    let mut media_header_fields = vec![0_u8; 20];
    media_header_fields[8..12].copy_from_slice(&48_000_u32.to_be_bytes());
    let media_header_box = bmff_full_box(*b"mdhd", 0, &media_header_fields);

    let mut hdlr_fields = vec![0_u8; 20];
    hdlr_fields[4..8].copy_from_slice(b"soun");
    hdlr_fields.extend_from_slice(b"SoundHandler\0");
    let handler_box = bmff_full_box(*b"hdlr", 0, &hdlr_fields);

    let smhd = bmff_full_box(*b"smhd", 0, &[0_u8; 4]);
    let url = bmff_full_box(*b"url ", 1, &[]);
    let mut dref_fields = 1_u32.to_be_bytes().to_vec();
    dref_fields.extend_from_slice(&url);
    let dref = bmff_full_box(*b"dref", 0, &dref_fields);
    let dinf = bmff_box(*b"dinf", &dref);

    let entry = audio_sample_entry(dec3);
    let mut stsd_fields = 1_u32.to_be_bytes().to_vec();
    stsd_fields.extend_from_slice(&entry);
    let stsd = bmff_full_box(*b"stsd", 0, &stsd_fields);
    let stbl = bmff_box(*b"stbl", &stsd);
    let minf = bmff_box(*b"minf", &[smhd, dinf, stbl].concat());
    let mdia = bmff_box(*b"mdia", &[media_header_box, handler_box, minf].concat());
    let trak = bmff_box(*b"trak", &[track_header_box, mdia].concat());

    let mut trex_fields = Vec::new();
    trex_fields.extend_from_slice(&1_u32.to_be_bytes());
    trex_fields.extend_from_slice(&1_u32.to_be_bytes());
    trex_fields.extend_from_slice(&1536_u32.to_be_bytes());
    trex_fields.extend_from_slice(&0_u32.to_be_bytes());
    trex_fields.extend_from_slice(&0_u32.to_be_bytes());
    let trex = bmff_full_box(*b"trex", 0, &trex_fields);
    let mvex = bmff_box(*b"mvex", &trex);
    let moov = bmff_box(*b"moov", &[movie_header_box, trak, mvex].concat());
    [ftyp, moov].concat()
}

fn cmaf_fragment(samples: &[Vec<u8>], sequence: u32, decode_time: u64) -> Vec<u8> {
    let mfhd = bmff_full_box(*b"mfhd", 0, &sequence.to_be_bytes());
    let mut tfhd_fields = 1_u32.to_be_bytes().to_vec();
    tfhd_fields.extend_from_slice(&1536_u32.to_be_bytes());
    let tfhd = bmff_full_box(*b"tfhd", 0x0002_0008, &tfhd_fields);
    let tfdt = bmff_full_box(*b"tfdt", 0x0100_0000, &decode_time.to_be_bytes());
    let mut trun_fields = Vec::new();
    trun_fields.extend_from_slice(
        &u32::try_from(samples.len())
            .expect("sample count")
            .to_be_bytes(),
    );
    trun_fields.extend_from_slice(&0_u32.to_be_bytes()); // patched after moof size is known
    for sample in samples {
        trun_fields.extend_from_slice(&1536_u32.to_be_bytes());
        trun_fields.extend_from_slice(
            &u32::try_from(sample.len())
                .expect("sample size")
                .to_be_bytes(),
        );
    }
    let trun = bmff_full_box(*b"trun", 0x0000_0301, &trun_fields);
    let traf = bmff_box(*b"traf", &[tfhd.clone(), tfdt.clone(), trun].concat());
    let moof = bmff_box(*b"moof", &[mfhd.clone(), traf].concat());
    let data_offset = u32::try_from(moof.len() + 8).expect("media data offset");

    let mut patched_trun_fields = Vec::new();
    patched_trun_fields.extend_from_slice(
        &u32::try_from(samples.len())
            .expect("sample count")
            .to_be_bytes(),
    );
    patched_trun_fields.extend_from_slice(&data_offset.to_be_bytes());
    for sample in samples {
        patched_trun_fields.extend_from_slice(&1536_u32.to_be_bytes());
        patched_trun_fields.extend_from_slice(
            &u32::try_from(sample.len())
                .expect("sample size")
                .to_be_bytes(),
        );
    }
    let patched_trun = bmff_full_box(*b"trun", 0x0000_0301, &patched_trun_fields);
    let patched_traf = bmff_box(*b"traf", &[tfhd, tfdt, patched_trun].concat());
    let patched_moof = bmff_box(*b"moof", &[mfhd, patched_traf].concat());
    let mdat = bmff_box(*b"mdat", &samples.concat());
    [patched_moof, mdat].concat()
}

#[test]
fn fragmented_cmaf_fixture_roundtrips_two_samples_through_ffprobe() {
    if Command::new("ffprobe").arg("-version").status().is_err() {
        eprintln!("skipping fragmented CMAF fixture: ffprobe is unavailable");
        return;
    }
    let dec3 = dec3_box(0, 0, true);
    let sample = eac3_frame(0, 0, 6, 128);
    let init = cmaf_init_segment(&dec3);
    assert!(init.windows(4).any(|window| window == b"ec-3"));
    assert!(init.windows(4).any(|window| window == b"dec3"));
    assert!(init.windows(4).any(|window| window == b"ceao"));
    let track = valid_track(0);
    let first_fragment = cmaf_fragment(std::slice::from_ref(&sample), 1, 0);
    let second_fragment = cmaf_fragment(std::slice::from_ref(&sample), 2, 1536);
    let file = [init, first_fragment, second_fragment].concat();

    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("openjoc-cmaf-fixture-{nonce}.mp4"));
    std::fs::write(&path, file).expect("write CMAF fixture");
    let timing = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "a:0",
            "-show_entries",
            "packet=pts,dts,duration,size",
            "-of",
            "csv=p=0:nk=1",
        ])
        .arg(&path)
        .output()
        .expect("probe CMAF timing");
    assert!(timing.status.success());
    let timing_rows = String::from_utf8_lossy(&timing.stdout);
    assert!(
        timing_rows
            .lines()
            .any(|row| row.starts_with("0,0,N/A,128"))
    );
    assert!(
        timing_rows
            .lines()
            .any(|row| row.starts_with("1536,1536,N/A,128"))
    );
    let mut ffprobe_reader = open_seekable_iso_bmff(&path, Path::new("ffprobe"), 4096)
        .expect("open fragmented CMAF fixture");
    assert_eq!(
        ffprobe_reader.next_cmaf_sample(&track).expect("sample 0"),
        Some(sample.clone())
    );
    assert_eq!(
        ffprobe_reader.next_cmaf_sample(&track).expect("sample 1"),
        Some(sample)
    );
    assert_eq!(
        ffprobe_reader
            .next_cmaf_sample(&track)
            .expect("fragment EOF"),
        None
    );
    std::fs::remove_file(path).expect("remove CMAF fixture");
}

#[allow(dead_code)]
fn _types_keep_the_expected_public_shape(_: Ec3SpecificBox, _: Ec3SubstreamConfig) {}
