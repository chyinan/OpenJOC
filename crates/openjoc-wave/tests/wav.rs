use openjoc_wave::{WaveError, encode_f64_mono};

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
fn rejects_invalid_rate_and_nonfinite_samples() {
    assert_eq!(encode_f64_mono(0, &[]), Err(WaveError::InvalidSampleRate));
    assert_eq!(
        encode_f64_mono(48_000, &[f64::NAN]),
        Err(WaveError::NonFiniteSample { index: 0 })
    );
}
