use openjoc_wave::{
    Clipping, Dither, SampleFormat, WaveEncodeOptions, WaveError, decode, encode_channels,
    encode_f64_channels, encode_f64_mono,
};

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
