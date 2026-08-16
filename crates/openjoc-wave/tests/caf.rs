use openjoc_wave::{
    CafChannelDescription, CafWriter, Clipping, Dither, SampleFormat, WaveEncodeOptions,
};
use std::{
    fs,
    io::{Cursor, Read, Seek},
};

#[derive(Debug)]
struct ParsedCaf {
    sample_rate: f64,
    format_flags: u32,
    channels: u32,
    bits_per_channel: u32,
    descriptions: Vec<CafChannelDescription>,
    data: Vec<u8>,
}

fn parse_caf(bytes: &[u8]) -> ParsedCaf {
    assert_eq!(&bytes[..4], b"caff");
    assert_eq!(u16::from_be_bytes(bytes[4..6].try_into().unwrap()), 1);
    assert_eq!(u16::from_be_bytes(bytes[6..8].try_into().unwrap()), 0);

    let mut position = 8;
    let mut parsed = ParsedCaf {
        sample_rate: 0.0,
        format_flags: 0,
        channels: 0,
        bits_per_channel: 0,
        descriptions: Vec::new(),
        data: Vec::new(),
    };
    while position < bytes.len() {
        let chunk_type = &bytes[position..position + 4];
        let size = i64::from_be_bytes(bytes[position + 4..position + 12].try_into().unwrap());
        assert!(size >= 0);
        let size = usize::try_from(size).unwrap();
        let start = position + 12;
        let end = start + size;
        assert!(end <= bytes.len());
        let payload = &bytes[start..end];
        match chunk_type {
            b"desc" => {
                assert_eq!(size, 32);
                parsed.sample_rate = f64::from_be_bytes(payload[..8].try_into().unwrap());
                assert_eq!(&payload[8..12], b"lpcm");
                parsed.format_flags = u32::from_be_bytes(payload[12..16].try_into().unwrap());
                let bytes_per_packet = u32::from_be_bytes(payload[16..20].try_into().unwrap());
                assert_ne!(bytes_per_packet, 0);
                assert_eq!(u32::from_be_bytes(payload[20..24].try_into().unwrap()), 1);
                parsed.channels = u32::from_be_bytes(payload[24..28].try_into().unwrap());
                parsed.bits_per_channel = u32::from_be_bytes(payload[28..32].try_into().unwrap());
            }
            b"chan" => {
                assert!(size >= 12);
                assert_eq!(u32::from_be_bytes(payload[..4].try_into().unwrap()), 0);
                assert_eq!(u32::from_be_bytes(payload[4..8].try_into().unwrap()), 0);
                let count = usize::try_from(u32::from_be_bytes(payload[8..12].try_into().unwrap()))
                    .unwrap();
                assert_eq!(size, 12 + count * 20);
                parsed.descriptions = (0..count)
                    .map(|index| {
                        let offset = 12 + index * 20;
                        CafChannelDescription {
                            label: u32::from_be_bytes(
                                payload[offset..offset + 4].try_into().unwrap(),
                            ),
                            flags: u32::from_be_bytes(
                                payload[offset + 4..offset + 8].try_into().unwrap(),
                            ),
                            coordinates: [
                                f32::from_be_bytes(
                                    payload[offset + 8..offset + 12].try_into().unwrap(),
                                ),
                                f32::from_be_bytes(
                                    payload[offset + 12..offset + 16].try_into().unwrap(),
                                ),
                                f32::from_be_bytes(
                                    payload[offset + 16..offset + 20].try_into().unwrap(),
                                ),
                            ],
                        }
                    })
                    .collect();
            }
            b"data" => {
                assert!(payload.len() >= 4);
                assert_eq!(u32::from_be_bytes(payload[..4].try_into().unwrap()), 0);
                parsed.data.extend_from_slice(&payload[4..]);
            }
            _ => {}
        }
        position = end;
    }
    parsed
}

fn options(sample_format: SampleFormat) -> WaveEncodeOptions {
    WaveEncodeOptions {
        sample_format,
        clipping: Clipping::Reject,
        dither: Dither::None,
    }
}

#[test]
fn caf_roundtrip_parser_verifies_semantic_metadata_and_float32_payload() {
    let descriptions = [
        CafChannelDescription {
            label: 1,
            flags: 0,
            coordinates: [0.0; 3],
        },
        CafChannelDescription {
            label: 100,
            flags: 1,
            coordinates: [-0.516_113_3, 0.0, 0.999_969_5],
        },
        CafChannelDescription {
            label: 4,
            flags: 0,
            coordinates: [0.0; 3],
        },
    ];
    let mut writer = CafWriter::new(
        Cursor::new(Vec::new()),
        48_000,
        descriptions.len(),
        options(SampleFormat::F32),
        &descriptions,
    )
    .unwrap();
    writer
        .write_channels(&[&[0.25, -0.5][..], &[1.0, 0.0][..], &[-1.0, 0.75][..]])
        .unwrap();
    assert_eq!(writer.frames(), 2);
    let bytes = writer.finish().unwrap().into_inner();
    if let Some(path) = std::env::var_os("OPENJOC_CAF_VALIDATION_PATH") {
        fs::write(path, &bytes).unwrap();
    }
    let parsed = parse_caf(&bytes);

    assert_eq!(parsed.sample_rate, 48_000.0);
    assert_eq!(parsed.format_flags, 0b11);
    assert_eq!(parsed.channels, 3);
    assert_eq!(parsed.bits_per_channel, 32);
    assert_eq!(parsed.descriptions, descriptions);
    assert_eq!(parsed.data.len(), 3 * 2 * 4);
    let mut samples = Vec::new();
    let mut cursor = Cursor::new(parsed.data);
    while cursor.stream_position().unwrap() < cursor.get_ref().len() as u64 {
        let mut sample = [0_u8; 4];
        cursor.read_exact(&mut sample).unwrap();
        samples.push(f32::from_le_bytes(sample) as f64);
    }
    assert_eq!(samples, vec![0.25, 1.0, -1.0, -0.5, 0.0, 0.75]);
}

#[test]
fn caf_sample_format_matrix_has_declared_flags_and_little_endian_payload() {
    for (sample_format, bits, bytes_per_sample) in [
        (SampleFormat::F32, 32, 4),
        (SampleFormat::F64, 64, 8),
        (SampleFormat::S16, 16, 2),
        (SampleFormat::S24, 24, 3),
    ] {
        let descriptions = [
            CafChannelDescription {
                label: 1,
                flags: 0,
                coordinates: [0.0; 3],
            },
            CafChannelDescription {
                label: 2,
                flags: 0,
                coordinates: [0.0; 3],
            },
        ];
        let mut writer = CafWriter::new(
            Cursor::new(Vec::new()),
            44_100,
            2,
            options(sample_format),
            &descriptions,
        )
        .unwrap();
        writer.write_channels(&[&[-0.5][..], &[0.5][..]]).unwrap();
        let bytes = writer.finish().unwrap().into_inner();
        let parsed = parse_caf(&bytes);
        assert_eq!(parsed.bits_per_channel, bits);
        assert_eq!(
            parsed.format_flags,
            if matches!(sample_format, SampleFormat::F32 | SampleFormat::F64) {
                0b11
            } else {
                0b10
            }
        );
        assert_eq!(parsed.data.len(), 2 * bytes_per_sample);
    }
}
