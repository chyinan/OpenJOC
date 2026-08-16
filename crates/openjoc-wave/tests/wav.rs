use openjoc_wave::{
    Clipping, Dither, SampleFormat, WaveEncodeOptions, WaveError, WaveWriter, decode,
    encode_channels, encode_f64_channels, encode_f64_mono,
};
use std::io::{self, Cursor, Seek, SeekFrom, Write};

struct FailingSeekWriter {
    bytes: Vec<u8>,
}

impl Write for FailingSeekWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Seek for FailingSeekWriter {
    fn seek(&mut self, _position: SeekFrom) -> io::Result<u64> {
        Err(io::Error::other("seek intentionally failed"))
    }
}

#[test]
fn encodes_mono_ieee_float_wave_without_pcm_quantization() {
    let wav = encode_f64_mono(48_000, &[0.25, -0.5, 1.0]).expect("valid WAV");

    assert_eq!(&wav[0..4], b"RIFF");
    assert_eq!(
        u32::from_le_bytes(wav[4..8].try_into().expect("RIFF size")),
        60
    );
    assert_eq!(&wav[8..12], b"WAVE");
    assert_eq!(&wav[12..16], b"fmt ");
    assert_eq!(
        u16::from_le_bytes(wav[20..22].try_into().expect("format")),
        3
    );
    assert_eq!(
        u16::from_le_bytes(wav[22..24].try_into().expect("channels")),
        1
    );
    assert_eq!(
        u32::from_le_bytes(wav[24..28].try_into().expect("rate")),
        48_000
    );
    assert_eq!(
        u16::from_le_bytes(wav[34..36].try_into().expect("bits")),
        64
    );
    assert_eq!(&wav[36..40], b"data");
    assert_eq!(
        u32::from_le_bytes(wav[40..44].try_into().expect("data size")),
        24
    );
    let samples = wav[44..]
        .chunks_exact(8)
        .map(|bytes| f64::from_le_bytes(bytes.try_into().expect("sample")))
        .collect::<Vec<_>>();
    assert_eq!(samples, vec![0.25, -0.5, 1.0]);
}

#[test]
fn incremental_writer_matches_capture_encoding_and_patches_sizes() {
    let options = WaveEncodeOptions {
        sample_format: SampleFormat::F64,
        clipping: Clipping::Reject,
        dither: Dither::None,
    };
    let expected =
        encode_f64_channels(48_000, &[vec![0.25, -0.5], vec![0.75, -1.0]]).expect("capture WAV");
    let mut writer = WaveWriter::new(Cursor::new(Vec::new()), 48_000, 2, options).expect("writer");
    writer
        .write_channels(&[&[0.25_f64][..], &[0.75_f64][..]])
        .expect("first chunk");
    writer
        .write_interleaved(&[-0.5, -1.0])
        .expect("second chunk");
    let actual = writer.finish().expect("finalize").into_inner();
    assert_eq!(actual, expected);
    assert_eq!(
        decode(&actual).expect("decode").channels[0],
        vec![0.25, -0.5]
    );
}

#[test]
fn extensible_speaker_writer_emits_standard_mask_and_preserves_plane_order() {
    let mask = 0x0002_d63f;
    let options = WaveEncodeOptions {
        sample_format: SampleFormat::F32,
        clipping: Clipping::Reject,
        dither: Dither::None,
    };
    let samples = (0..12).map(|value| value as f64).collect::<Vec<_>>();
    let mut writer =
        WaveWriter::new_with_speaker_mask(Cursor::new(Vec::new()), 48_000, 12, options, mask)
            .expect("valid 7.1.4 speaker mask");
    writer
        .write_interleaved(&samples)
        .expect("interleaved frame");
    let bytes = writer.finish().expect("finalize").into_inner();

    assert_eq!(u32::from_le_bytes(bytes[16..20].try_into().unwrap()), 40);
    assert_eq!(
        u16::from_le_bytes(bytes[20..22].try_into().unwrap()),
        0xfffe
    );
    assert_eq!(u16::from_le_bytes(bytes[36..38].try_into().unwrap()), 22);
    assert_eq!(u16::from_le_bytes(bytes[38..40].try_into().unwrap()), 32);
    assert_eq!(u32::from_le_bytes(bytes[40..44].try_into().unwrap()), mask);
    assert_eq!(
        &bytes[44..60],
        &[
            3, 0, 0, 0, 0, 0, 0x10, 0, 0x80, 0, 0, 0xaa, 0, 0x38, 0x9b, 0x71
        ]
    );
    assert_eq!(&bytes[60..64], b"data");
    assert_eq!(u32::from_le_bytes(bytes[64..68].try_into().unwrap()), 48);

    let decoded = decode(&bytes).expect("extensible WAV decodes");
    assert_eq!(decoded.channel_mask, Some(mask));
    assert_eq!(decoded.channels[0][0], 0.0);
    assert_eq!(decoded.channels[11][0], 11.0);

    let basic = encode_channels(48_000, std::slice::from_ref(&samples), options)
        .expect("basic reference WAV");
    assert_eq!(&bytes[68..], &basic[44..]);
}

#[test]
fn extensible_speaker_writer_rejects_nonstandard_or_mismatched_masks() {
    let options = WaveEncodeOptions {
        sample_format: SampleFormat::F32,
        clipping: Clipping::Reject,
        dither: Dither::None,
    };
    assert!(matches!(
        WaveWriter::new_with_speaker_mask(Cursor::new(Vec::new()), 48_000, 2, options, 0),
        Err(WaveError::InvalidChannelMask {
            channels: 2,
            mask: 0
        })
    ));
    assert!(matches!(
        WaveWriter::new_with_speaker_mask(Cursor::new(Vec::new()), 48_000, 1, options, 1 << 31),
        Err(WaveError::InvalidChannelMask { channels: 1, mask }) if mask == 1 << 31
    ));
}

#[test]
fn incremental_writer_propagates_finalization_io_error() {
    let options = WaveEncodeOptions {
        sample_format: SampleFormat::F32,
        clipping: Clipping::Reject,
        dither: Dither::None,
    };
    let mut writer = WaveWriter::new(FailingSeekWriter { bytes: Vec::new() }, 48_000, 1, options)
        .expect("header write");
    writer.write_interleaved(&[0.25]).expect("sample write");
    assert!(matches!(
        writer.finish(),
        Err(WaveError::Io {
            kind: io::ErrorKind::Other,
        })
    ));
}

#[test]
fn decodes_the_reference_f64_wave_without_sample_loss() {
    let bytes = encode_f64_mono(48_000, &[0.25, -0.5, 1.0]).expect("valid WAV");
    let wave = decode(&bytes).expect("reference WAV decodes");

    assert_eq!(wave.sample_rate, 48_000);
    assert_eq!(wave.channels, vec![vec![0.25, -0.5, 1.0]]);
}

#[test]
fn roundtrips_multichannel_f64_wave_for_downmix_input() {
    let expected = vec![vec![0.25, 0.5], vec![-0.25, -0.5]];
    let bytes = encode_f64_channels(48_000, &expected).expect("stereo f64 WAV");
    let wave = decode(&bytes).expect("stereo f64 WAV decodes");

    assert_eq!(wave.sample_rate, 48_000);
    assert_eq!(wave.channels, expected);
}

#[test]
fn encodes_explicit_f32_reference_and_integer_sample_formats() {
    let channels = vec![vec![-1.0, 0.0, 1.0]];
    let f32_wav = encode_channels(
        48_000,
        &channels,
        WaveEncodeOptions {
            sample_format: SampleFormat::F32,
            clipping: Clipping::Reject,
            dither: Dither::None,
        },
    )
    .expect("f32 WAV");
    assert_eq!(u16::from_le_bytes(f32_wav[20..22].try_into().unwrap()), 3);
    assert_eq!(u16::from_le_bytes(f32_wav[34..36].try_into().unwrap()), 32);

    for (format, bits) in [
        (SampleFormat::F64, 64),
        (SampleFormat::S24, 24),
        (SampleFormat::S16, 16),
    ] {
        let wav = encode_channels(
            48_000,
            &channels,
            WaveEncodeOptions {
                sample_format: format,
                clipping: Clipping::Reject,
                dither: Dither::None,
            },
        )
        .expect("explicit WAV");
        assert_eq!(u16::from_le_bytes(wav[34..36].try_into().unwrap()), bits);
    }
}

#[test]
fn integer_output_requires_explicit_clipping_policy() {
    let channels = vec![vec![1.25]];
    let error = encode_channels(
        48_000,
        &channels,
        WaveEncodeOptions {
            sample_format: SampleFormat::S16,
            clipping: Clipping::Reject,
            dither: Dither::None,
        },
    )
    .expect_err("out-of-range integer sample");
    assert_eq!(error, WaveError::OutOfRangeSample { index: 0 });

    let wav = encode_channels(
        48_000,
        &channels,
        WaveEncodeOptions {
            sample_format: SampleFormat::S16,
            clipping: Clipping::Hard,
            dither: Dither::None,
        },
    )
    .expect("explicit hard clipping");
    assert_eq!(&wav[44..46], &i16::MAX.to_le_bytes());
}

#[test]
fn explicit_triangular_dither_is_reproducible() {
    let channels = vec![vec![0.25; 32]];
    let options = WaveEncodeOptions {
        sample_format: SampleFormat::S16,
        clipping: Clipping::Reject,
        dither: Dither::Triangular { seed: 7 },
    };
    let first = encode_channels(48_000, &channels, options).expect("dithered WAV");
    let second = encode_channels(48_000, &channels, options).expect("dithered WAV");
    assert_eq!(first, second);
}

#[test]
fn decodes_interleaved_stereo_pcm16_to_channel_major_f64() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&44_u32.to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&48_000_u32.to_le_bytes());
    bytes.extend_from_slice(&192_000_u32.to_le_bytes());
    bytes.extend_from_slice(&4_u16.to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&8_u32.to_le_bytes());
    for sample in [i16::MAX, i16::MIN, 0, 16_384] {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }

    let wave = decode(&bytes).expect("PCM16 WAV");
    assert_eq!(wave.channels.len(), 2);
    assert_eq!(wave.channels[0], vec![f64::from(i16::MAX) / 32768.0, 0.0]);
    assert_eq!(wave.channels[1], vec![-1.0, 0.5]);
}

#[test]
fn rejects_invalid_rate_and_nonfinite_samples() {
    assert_eq!(encode_f64_mono(0, &[]), Err(WaveError::InvalidSampleRate));
    assert_eq!(
        encode_f64_mono(48_000, &[f64::NAN]),
        Err(WaveError::NonFiniteSample { index: 0 })
    );
}
