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
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        if position == SeekFrom::Current(0) {
            return Ok(self.bytes.len() as u64);
        }
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

#[test]
fn f32_output_rejects_finite_overflow_without_clipping() {
    let maximum = f64::from(f32::MAX);
    for clipping in [Clipping::Reject, Clipping::Hard] {
        let options = WaveEncodeOptions {
            sample_format: SampleFormat::F32,
            clipping,
            dither: Dither::None,
        };
        for value in [2.0 * maximum, -2.0 * maximum] {
            assert!(matches!(
                encode_channels(48_000, &[vec![0.0, value]], options),
                Err(WaveError::OutOfRangeSample { index: 1 })
            ));
            let mut writer = WaveWriter::new(Cursor::new(Vec::new()), 48_000, 1, options).unwrap();
            writer.write_interleaved(&[0.0]).unwrap();
            assert!(matches!(
                writer.write_channels(&[&[value]]),
                Err(WaveError::OutOfRangeSample { index: 1 })
            ));
            assert_eq!(writer.frames(), 1);
            assert_eq!(
                decode(&writer.finish().unwrap().into_inner())
                    .unwrap()
                    .channels[0],
                [0.0]
            );
        }
        let bytes = encode_channels(48_000, &[vec![maximum, -maximum]], options).unwrap();
        assert_eq!(decode(&bytes).unwrap().channels[0], [maximum, -maximum]);
    }
    let bytes = encode_f64_mono(48_000, &[2.0 * maximum, -2.0 * maximum]).unwrap();
    assert_eq!(
        decode(&bytes).unwrap().channels[0],
        [2.0 * maximum, -2.0 * maximum]
    );
}

#[test]
fn wav_chunk_padding_roundtrips_batch_and_streaming_formats() {
    for format in [
        SampleFormat::S24,
        SampleFormat::S16,
        SampleFormat::F32,
        SampleFormat::F64,
    ] {
        let bytes_per_sample = match format {
            SampleFormat::S24 => 3,
            SampleFormat::S16 => 2,
            SampleFormat::F32 => 4,
            SampleFormat::F64 => 8,
        };
        let options = WaveEncodeOptions {
            sample_format: format,
            clipping: Clipping::Reject,
            dither: Dither::None,
        };
        for channel_count in 1..=5 {
            for frame_count in 0..=3 {
                let channels = (0..channel_count)
                    .map(|channel| {
                        (0..frame_count)
                            .map(|frame| ((channel + frame) % 5) as f64 * 0.25 - 0.5)
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>();
                let data_size = channel_count * frame_count * bytes_per_sample;
                let batch = encode_channels(48_000, &channels, options).unwrap();
                assert_wav_data_padding(&batch, 44, data_size, &channels);
                for extensible in [false, true] {
                    let mut writer = if extensible {
                        WaveWriter::new_with_speaker_mask(
                            Cursor::new(Vec::new()),
                            48_000,
                            channel_count,
                            options,
                            (1 << channel_count) - 1,
                        )
                        .unwrap()
                    } else {
                        WaveWriter::new(Cursor::new(Vec::new()), 48_000, channel_count, options)
                            .unwrap()
                    };
                    writer.write_interleaved(&[]).unwrap();
                    for frame in 0..frame_count {
                        if frame % 2 == 0 {
                            let planes = channels
                                .iter()
                                .map(|channel| &channel[frame..=frame])
                                .collect::<Vec<_>>();
                            writer.write_channels(&planes).unwrap();
                        } else {
                            let interleaved = channels
                                .iter()
                                .map(|channel| channel[frame])
                                .collect::<Vec<_>>();
                            writer.write_interleaved(&interleaved).unwrap();
                        }
                        assert_eq!(
                            writer.data_bytes(),
                            ((frame + 1) * channel_count * bytes_per_sample) as u64
                        );
                    }
                    assert_eq!(writer.frames(), frame_count as u64);
                    let bytes = writer.finish().unwrap().into_inner();
                    assert_wav_data_padding(
                        &bytes,
                        if extensible { 68 } else { 44 },
                        data_size,
                        &channels,
                    );
                    if !extensible {
                        assert_eq!(bytes, batch);
                    }
                }
            }
        }
    }
}

fn assert_wav_data_padding(
    bytes: &[u8],
    data_start: usize,
    data_size: usize,
    channels: &[Vec<f64>],
) {
    let padding = data_size % 2;
    assert_eq!(bytes.len(), data_start + data_size + padding);
    assert_eq!(
        u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize,
        bytes.len() - 8
    );
    assert_eq!(&bytes[data_start - 8..data_start - 4], b"data");
    assert_eq!(
        u32::from_le_bytes(bytes[data_start - 4..data_start].try_into().unwrap()) as usize,
        data_size
    );
    if padding != 0 {
        assert_eq!(bytes.last(), Some(&0));
    }
    assert_eq!(decode(bytes).unwrap().channels, channels);
}

#[test]
fn positioned_writers_preserve_prefix_suffix_and_return_at_wav_end() {
    for format in [
        SampleFormat::S24,
        SampleFormat::S16,
        SampleFormat::F32,
        SampleFormat::F64,
    ] {
        let options = WaveEncodeOptions {
            sample_format: format,
            clipping: Clipping::Reject,
            dither: Dither::None,
        };
        for channel_count in 1..=3 {
            for frames in 0..=3 {
                for extensible in [false, true] {
                    let encode = |sink: Cursor<Vec<u8>>| {
                        let mut writer = if extensible {
                            WaveWriter::new_with_speaker_mask(
                                sink,
                                48_000,
                                channel_count,
                                options,
                                (1 << channel_count) - 1,
                            )
                            .unwrap()
                        } else {
                            WaveWriter::new(sink, 48_000, channel_count, options).unwrap()
                        };
                        for frame in 0..frames {
                            let samples = (0..channel_count)
                                .map(|channel| ((frame + channel) % 5) as f64 * 0.25 - 0.5)
                                .collect::<Vec<_>>();
                            if frame % 2 == 0 {
                                writer.write_interleaved(&samples).unwrap();
                            } else {
                                let planes =
                                    samples.iter().map(std::slice::from_ref).collect::<Vec<_>>();
                                writer.write_channels(&planes).unwrap();
                            }
                        }
                        writer.finish().unwrap()
                    };
                    let reference = encode(Cursor::new(Vec::new()));
                    assert_eq!(reference.position(), reference.get_ref().len() as u64);
                    let expected = reference.into_inner();
                    for origin in [1, 16, 37] {
                        for suffix_len in [0, 19] {
                            let mut original = vec![0x55; origin];
                            // Exercise both appending to a prefix and replacing an
                            // owned span followed by unrelated existing data.
                            if suffix_len != 0 {
                                original.extend(vec![0xcc; expected.len()]);
                                original.extend(vec![0x77; suffix_len]);
                            }
                            let mut sink = Cursor::new(original);
                            sink.set_position(origin as u64);
                            let result = encode(sink);
                            let wav_end = origin + expected.len();
                            assert_eq!(result.position(), wav_end as u64);
                            let bytes = result.into_inner();
                            assert_eq!(&bytes[..origin], vec![0x55; origin]);
                            assert_eq!(&bytes[origin..wav_end], expected);
                            assert_eq!(&bytes[wav_end..], vec![0x77; suffix_len]);
                            assert_eq!(
                                decode(&bytes[origin..wav_end]).unwrap().channels,
                                decode(&expected).unwrap().channels
                            );
                        }
                    }
                }
            }
        }
    }
}

struct OffsetOnlySink {
    position: u64,
    writes: usize,
}

impl Write for OffsetOnlySink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.position = self
            .position
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| io::Error::other("sink overflow"))?;
        self.writes += 1;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Seek for OffsetOnlySink {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        self.position = match from {
            SeekFrom::Start(position) => position,
            SeekFrom::Current(0) => self.position,
            _ => return Err(io::Error::other("unexpected seek")),
        };
        Ok(self.position)
    }
}

#[test]
fn writer_positions_use_checked_arithmetic_before_io() {
    let options = WaveEncodeOptions {
        sample_format: SampleFormat::S24,
        clipping: Clipping::Reject,
        dither: Dither::None,
    };
    for extensible in [false, true] {
        let header_len = if extensible { 68 } else { 44 };
        let mut sink = OffsetOnlySink {
            position: u64::MAX - header_len + 1,
            writes: 0,
        };
        let result = if extensible {
            WaveWriter::new_with_speaker_mask(&mut sink, 48_000, 1, options, 1)
        } else {
            WaveWriter::new(&mut sink, 48_000, 1, options)
        };
        assert!(matches!(result, Err(WaveError::SizeOverflow)));
        assert_eq!(sink.writes, 0);
        for room in [0, 3] {
            let mut sink = OffsetOnlySink {
                position: u64::MAX - header_len - room,
                writes: 0,
            };
            let mut writer = if extensible {
                WaveWriter::new_with_speaker_mask(&mut sink, 48_000, 1, options, 1).unwrap()
            } else {
                WaveWriter::new(&mut sink, 48_000, 1, options).unwrap()
            };
            if room == 0 {
                assert!(matches!(
                    writer.write_interleaved(&[0.25]),
                    Err(WaveError::SizeOverflow)
                ));
                assert_eq!(writer.frames(), 0);
                assert_eq!(writer.data_bytes(), 0);
                assert_eq!(writer.finish().unwrap().position, u64::MAX);
            } else {
                writer.write_interleaved(&[0.25]).unwrap();
                assert!(matches!(writer.finish(), Err(WaveError::SizeOverflow)));
                assert_eq!(sink.position, u64::MAX);
            }
        }
    }
}
