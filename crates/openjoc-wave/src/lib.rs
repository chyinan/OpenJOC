// pattern: Functional Core

//! Checked reference serialization for reconstructed object WAV stems.

use std::fmt;

/// WAV serialization failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WaveError {
    InvalidSampleRate,
    NonFiniteSample { index: usize },
    SizeOverflow,
    InvalidRiff,
    Truncated,
    MissingFormat,
    MissingData,
    UnsupportedFormat { format: u16, bits: u16 },
    InvalidFormat,
}

impl fmt::Display for WaveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSampleRate => formatter.write_str("invalid WAV sample rate"),
            Self::NonFiniteSample { index } => {
                write!(formatter, "non-finite WAV sample at index {index}")
            }
            Self::SizeOverflow => formatter.write_str("WAV data exceeds RIFF size limits"),
            Self::InvalidRiff => formatter.write_str("invalid RIFF/WAVE header"),
            Self::Truncated => formatter.write_str("truncated RIFF/WAVE chunk"),
            Self::MissingFormat => formatter.write_str("WAV format chunk is missing"),
            Self::MissingData => formatter.write_str("WAV data chunk is missing"),
            Self::UnsupportedFormat { format, bits } => {
                write!(
                    formatter,
                    "unsupported WAV format {format} with {bits} bits"
                )
            }
            Self::InvalidFormat => formatter.write_str("inconsistent WAV format fields"),
        }
    }
}

/// Decoded channel-major WAV PCM.
#[derive(Clone, Debug, PartialEq)]
pub struct WavePcm {
    pub sample_rate: u32,
    pub channels: Vec<Vec<f64>>,
}

#[derive(Clone, Copy)]
struct WaveFormat {
    encoding: u16,
    channels: usize,
    sample_rate: u32,
    block_align: usize,
    bits: u16,
}

/// Decodes checked PCM/IEEE-float RIFF/WAVE data to channel-major f64 PCM.
///
/// PCM 16/24/32-bit, IEEE float 32/64-bit, and their extensible-header forms
/// are supported. Unknown chunks are skipped within the declared RIFF bound.
///
/// # Errors
/// Returns [`WaveError`] for malformed, truncated, inconsistent, unsupported,
/// or non-finite audio data.
pub fn decode(bytes: &[u8]) -> Result<WavePcm, WaveError> {
    if bytes.get(0..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"WAVE") {
        return Err(WaveError::InvalidRiff);
    }
    let riff_size = read_u32(bytes, 4)?;
    let riff_end = usize::try_from(riff_size)
        .map_err(|_| WaveError::SizeOverflow)?
        .checked_add(8)
        .ok_or(WaveError::SizeOverflow)?;
    if riff_end > bytes.len() || riff_end < 12 {
        return Err(WaveError::Truncated);
    }
    let mut format = None;
    let mut data = None;
    let mut position = 12_usize;
    while position < riff_end {
        let header_end = position.checked_add(8).ok_or(WaveError::SizeOverflow)?;
        if header_end > riff_end {
            return Err(WaveError::Truncated);
        }
        let size =
            usize::try_from(read_u32(bytes, position + 4)?).map_err(|_| WaveError::SizeOverflow)?;
        let chunk_end = header_end
            .checked_add(size)
            .ok_or(WaveError::SizeOverflow)?;
        if chunk_end > riff_end {
            return Err(WaveError::Truncated);
        }
        match bytes.get(position..position + 4) {
            Some(b"fmt ") => format = Some(parse_format(&bytes[header_end..chunk_end])?),
            Some(b"data") => data = Some(&bytes[header_end..chunk_end]),
            _ => {}
        }
        position = chunk_end
            .checked_add(size % 2)
            .ok_or(WaveError::SizeOverflow)?;
        if position > riff_end {
            return Err(WaveError::Truncated);
        }
    }
    let format = format.ok_or(WaveError::MissingFormat)?;
    let data = data.ok_or(WaveError::MissingData)?;
    decode_samples(format, data)
}

fn parse_format(bytes: &[u8]) -> Result<WaveFormat, WaveError> {
    if bytes.len() < 16 {
        return Err(WaveError::Truncated);
    }
    let original_encoding = read_u16(bytes, 0)?;
    let channels = usize::from(read_u16(bytes, 2)?);
    let sample_rate = read_u32(bytes, 4)?;
    let block_align = usize::from(read_u16(bytes, 12)?);
    let bits = read_u16(bytes, 14)?;
    let encoding = if original_encoding == 0xfffe {
        if bytes.len() < 40 || read_u16(bytes, 16)? < 22 || read_u16(bytes, 18)? != bits {
            return Err(WaveError::InvalidFormat);
        }
        u16::try_from(read_u32(bytes, 24)?).map_err(|_| WaveError::InvalidFormat)?
    } else {
        original_encoding
    };
    if channels == 0 || sample_rate == 0 || bits == 0 || bits % 8 != 0 {
        return Err(WaveError::InvalidFormat);
    }
    let bytes_per_sample = usize::from(bits / 8);
    if channels
        .checked_mul(bytes_per_sample)
        .ok_or(WaveError::SizeOverflow)?
        != block_align
    {
        return Err(WaveError::InvalidFormat);
    }
    if !matches!((encoding, bits), (1, 16 | 24 | 32) | (3, 32 | 64)) {
        return Err(WaveError::UnsupportedFormat {
            format: encoding,
            bits,
        });
    }
    Ok(WaveFormat {
        encoding,
        channels,
        sample_rate,
        block_align,
        bits,
    })
}

fn decode_samples(format: WaveFormat, data: &[u8]) -> Result<WavePcm, WaveError> {
    if data.len() % format.block_align != 0 {
        return Err(WaveError::InvalidFormat);
    }
    let frames = data.len() / format.block_align;
    let bytes_per_sample = usize::from(format.bits / 8);
    let mut channels = vec![Vec::with_capacity(frames); format.channels];
    for frame in 0..frames {
        for (channel, output) in channels.iter_mut().enumerate() {
            let offset = frame
                .checked_mul(format.block_align)
                .and_then(|value| value.checked_add(channel * bytes_per_sample))
                .ok_or(WaveError::SizeOverflow)?;
            let sample = decode_sample(format, &data[offset..offset + bytes_per_sample])?;
            if !sample.is_finite() {
                return Err(WaveError::NonFiniteSample { index: frame });
            }
            output.push(sample);
        }
    }
    Ok(WavePcm {
        sample_rate: format.sample_rate,
        channels,
    })
}

fn decode_sample(format: WaveFormat, bytes: &[u8]) -> Result<f64, WaveError> {
    Ok(match (format.encoding, format.bits) {
        (1, 16) => {
            f64::from(i16::from_le_bytes(
                bytes.try_into().map_err(|_| WaveError::Truncated)?,
            )) / 32_768.0
        }
        (1, 24) => {
            let mut value =
                i32::from(bytes[0]) | (i32::from(bytes[1]) << 8) | (i32::from(bytes[2]) << 16);
            if value & 0x0080_0000 != 0 {
                value |= !0x00ff_ffff;
            }
            f64::from(value) / 8_388_608.0
        }
        (1, 32) => {
            f64::from(i32::from_le_bytes(
                bytes.try_into().map_err(|_| WaveError::Truncated)?,
            )) / 2_147_483_648.0
        }
        (3, 32) => f64::from(f32::from_le_bytes(
            bytes.try_into().map_err(|_| WaveError::Truncated)?,
        )),
        (3, 64) => f64::from_le_bytes(bytes.try_into().map_err(|_| WaveError::Truncated)?),
        _ => {
            return Err(WaveError::UnsupportedFormat {
                format: format.encoding,
                bits: format.bits,
            });
        }
    })
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, WaveError> {
    Ok(u16::from_le_bytes(
        bytes
            .get(offset..offset + 2)
            .ok_or(WaveError::Truncated)?
            .try_into()
            .map_err(|_| WaveError::Truncated)?,
    ))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, WaveError> {
    Ok(u32::from_le_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or(WaveError::Truncated)?
            .try_into()
            .map_err(|_| WaveError::Truncated)?,
    ))
}

impl std::error::Error for WaveError {}

/// Encodes mono samples as a 64-bit IEEE-float RIFF/WAVE stream.
///
/// This preserves reconstructed amplitudes without integer clipping or
/// quantization.
///
/// # Errors
/// Returns [`WaveError`] for a zero rate, non-finite sample, or RIFF size
/// overflow.
pub fn encode_f64_mono(sample_rate: u32, samples: &[f64]) -> Result<Vec<u8>, WaveError> {
    if let Some(index) = samples.iter().position(|sample| !sample.is_finite()) {
        return Err(WaveError::NonFiniteSample { index });
    }
    encode_f64_planar(sample_rate, 1, samples.len(), |_, frame| samples[frame])
}

/// Encodes channel-major samples as a 64-bit IEEE-float RIFF/WAVE stream.
///
/// # Errors
/// Returns [`WaveError`] for zero/mismatched channels, invalid rate,
/// non-finite samples, or RIFF size overflow.
pub fn encode_f64_channels(sample_rate: u32, channels: &[Vec<f64>]) -> Result<Vec<u8>, WaveError> {
    let frames = channels.first().map_or(0, Vec::len);
    if channels.is_empty() || channels.iter().any(|channel| channel.len() != frames) {
        return Err(WaveError::InvalidFormat);
    }
    for channel in channels {
        if let Some(index) = channel.iter().position(|sample| !sample.is_finite()) {
            return Err(WaveError::NonFiniteSample { index });
        }
    }
    encode_f64_planar(sample_rate, channels.len(), frames, |channel, frame| {
        channels[channel][frame]
    })
}

fn encode_f64_planar(
    sample_rate: u32,
    channels: usize,
    frames: usize,
    sample: impl Fn(usize, usize) -> f64,
) -> Result<Vec<u8>, WaveError> {
    if sample_rate == 0 {
        return Err(WaveError::InvalidSampleRate);
    }
    let channel_count = u16::try_from(channels).map_err(|_| WaveError::SizeOverflow)?;
    let block_align = channel_count
        .checked_mul(8)
        .ok_or(WaveError::SizeOverflow)?;
    let sample_count = frames
        .checked_mul(channels)
        .ok_or(WaveError::SizeOverflow)?;
    let sample_count = u32::try_from(sample_count).map_err(|_| WaveError::SizeOverflow)?;
    let data_size = sample_count.checked_mul(8).ok_or(WaveError::SizeOverflow)?;
    let riff_size = data_size.checked_add(36).ok_or(WaveError::SizeOverflow)?;
    let byte_rate = sample_rate
        .checked_mul(u32::from(block_align))
        .ok_or(WaveError::SizeOverflow)?;
    let capacity = usize::try_from(riff_size)
        .map_err(|_| WaveError::SizeOverflow)?
        .checked_add(8)
        .ok_or(WaveError::SizeOverflow)?;

    let mut wav = Vec::with_capacity(capacity);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&riff_size.to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&3_u16.to_le_bytes());
    wav.extend_from_slice(&channel_count.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&64_u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());
    for frame in 0..frames {
        for channel in 0..channels {
            wav.extend_from_slice(&sample(channel, frame).to_le_bytes());
        }
    }
    Ok(wav)
}
