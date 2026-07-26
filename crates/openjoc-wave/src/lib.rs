// pattern: Functional Core

//! Checked reference serialization for reconstructed object WAV stems.

use std::fmt;

/// WAV serialization failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WaveError {
    InvalidSampleRate,
    NonFiniteSample { index: usize },
    SizeOverflow,
}

impl fmt::Display for WaveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSampleRate => formatter.write_str("invalid WAV sample rate"),
            Self::NonFiniteSample { index } => {
                write!(formatter, "non-finite WAV sample at index {index}")
            }
            Self::SizeOverflow => formatter.write_str("WAV data exceeds RIFF size limits"),
        }
    }
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
    if sample_rate == 0 {
        return Err(WaveError::InvalidSampleRate);
    }
    if let Some(index) = samples.iter().position(|sample| !sample.is_finite()) {
        return Err(WaveError::NonFiniteSample { index });
    }
    let sample_count = u32::try_from(samples.len()).map_err(|_| WaveError::SizeOverflow)?;
    let data_size = sample_count.checked_mul(8).ok_or(WaveError::SizeOverflow)?;
    let riff_size = data_size.checked_add(36).ok_or(WaveError::SizeOverflow)?;
    let byte_rate = sample_rate.checked_mul(8).ok_or(WaveError::SizeOverflow)?;
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
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&8_u16.to_le_bytes());
    wav.extend_from_slice(&64_u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());
    for sample in samples {
        wav.extend_from_slice(&sample.to_le_bytes());
    }
    Ok(wav)
}
