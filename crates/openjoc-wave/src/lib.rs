// pattern: Functional Core

//! Checked reference serialization for reconstructed object WAV stems.

use std::{fmt, io, io::Seek, io::SeekFrom, io::Write};

/// WAV serialization failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WaveError {
    Io { kind: io::ErrorKind },
    InvalidSampleRate,
    NonFiniteSample { index: usize },
    OutOfRangeSample { index: usize },
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
            Self::Io { kind } => write!(formatter, "WAV I/O error: {kind}"),
            Self::InvalidSampleRate => formatter.write_str("invalid WAV sample rate"),
            Self::NonFiniteSample { index } => {
                write!(formatter, "non-finite WAV sample at index {index}")
            }
            Self::OutOfRangeSample { index } => {
                write!(
                    formatter,
                    "WAV integer sample is outside [-1, 1] at index {index}"
                )
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

/// Output sample representation for a WAV sink.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SampleFormat {
    /// 32-bit IEEE float. This is the normal user-facing output format.
    F32,
    /// 64-bit IEEE float. This is the explicit reference format.
    F64,
    /// Signed little-endian 24-bit PCM.
    S24,
    /// Signed little-endian 16-bit PCM.
    S16,
}

/// Policy for normalized samples that cannot be represented by integer PCM.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Clipping {
    /// Return [`WaveError::OutOfRangeSample`] instead of changing the sample.
    Reject,
    /// Clamp integer PCM input to the representable normalized range.
    Hard,
}

/// Explicit quantization dither policy for integer PCM output.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Dither {
    /// Do not add dither.
    None,
    /// Add deterministic triangular-PDF one-LSB dither derived from `seed`.
    Triangular { seed: u64 },
}

/// Explicit WAV output policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WaveEncodeOptions {
    pub sample_format: SampleFormat,
    pub clipping: Clipping,
    pub dither: Dither,
}

/// Incremental seekable WAV writer.
///
/// The header is written once with placeholder sizes, samples are appended in
/// bounded chunks, and RIFF/data sizes are patched during [`Self::finish`].
/// This is intentionally seekable-only; callers writing to non-seekable output
/// must use an explicitly supported container instead of emitting invalid WAV.
pub struct WaveWriter<W> {
    writer: W,
    sample_rate: u32,
    channels: usize,
    options: WaveEncodeOptions,
    data_bytes: u64,
    frames: u64,
    sample_index: usize,
}

impl<W: Write + Seek> WaveWriter<W> {
    /// Creates a writer and emits a placeholder 44-byte RIFF/WAVE header.
    pub fn new(
        mut writer: W,
        sample_rate: u32,
        channels: usize,
        options: WaveEncodeOptions,
    ) -> Result<Self, WaveError> {
        let channel_count = validate_writer_format(sample_rate, channels, options)?;
        write_placeholder_header(&mut writer, sample_rate, channel_count, options)?;
        Ok(Self {
            writer,
            sample_rate,
            channels,
            options,
            data_bytes: 0,
            frames: 0,
            sample_index: 0,
        })
    }

    /// Appends interleaved samples for one or more complete frames.
    pub fn write_interleaved(&mut self, samples: &[f64]) -> Result<(), WaveError> {
        if samples.len() % self.channels != 0 {
            return Err(WaveError::InvalidFormat);
        }
        let mut encoded = Vec::with_capacity(
            samples
                .len()
                .checked_mul(self.options.sample_format.bytes_per_sample())
                .ok_or(WaveError::SizeOverflow)?,
        );
        for &sample in samples {
            encode_sample(&mut encoded, sample, self.options, self.sample_index)?;
            self.sample_index = self
                .sample_index
                .checked_add(1)
                .ok_or(WaveError::SizeOverflow)?;
        }
        self.writer.write_all(&encoded).map_err(io_error)?;
        self.data_bytes = self
            .data_bytes
            .checked_add(u64::try_from(encoded.len()).map_err(|_| WaveError::SizeOverflow)?)
            .ok_or(WaveError::SizeOverflow)?;
        self.frames = self
            .frames
            .checked_add(
                u64::try_from(samples.len() / self.channels)
                    .map_err(|_| WaveError::SizeOverflow)?,
            )
            .ok_or(WaveError::SizeOverflow)?;
        Ok(())
    }

    /// Appends a channel-major chunk without retaining prior chunks.
    pub fn write_channels(&mut self, channels: &[&[f64]]) -> Result<(), WaveError> {
        if channels.len() != self.channels {
            return Err(WaveError::InvalidFormat);
        }
        let frames = channels.first().map_or(0, |channel| channel.len());
        if channels.iter().any(|channel| channel.len() != frames) {
            return Err(WaveError::InvalidFormat);
        }
        let mut interleaved = Vec::with_capacity(
            frames
                .checked_mul(self.channels)
                .ok_or(WaveError::SizeOverflow)?,
        );
        for frame in 0..frames {
            for channel in channels {
                interleaved.push(channel[frame]);
            }
        }
        self.write_interleaved(&interleaved)
    }

    /// Finalizes RIFF/data sizes, flushes, and returns the underlying writer.
    pub fn finish(mut self) -> Result<W, WaveError> {
        let data_size = u32::try_from(self.data_bytes).map_err(|_| WaveError::SizeOverflow)?;
        let riff_size = data_size.checked_add(36).ok_or(WaveError::SizeOverflow)?;
        self.writer.seek(SeekFrom::Start(4)).map_err(io_error)?;
        self.writer
            .write_all(&riff_size.to_le_bytes())
            .map_err(io_error)?;
        self.writer.seek(SeekFrom::Start(40)).map_err(io_error)?;
        self.writer
            .write_all(&data_size.to_le_bytes())
            .map_err(io_error)?;
        self.writer.seek(SeekFrom::End(0)).map_err(io_error)?;
        self.writer.flush().map_err(io_error)?;
        Ok(self.writer)
    }

    /// Number of complete sample frames appended so far.
    #[must_use]
    pub const fn frames(&self) -> u64 {
        self.frames
    }

    /// Bytes of encoded sample data appended so far.
    #[must_use]
    pub const fn data_bytes(&self) -> u64 {
        self.data_bytes
    }

    /// Sample rate retained by this writer.
    #[must_use]
    pub const fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
}

#[allow(clippy::needless_pass_by_value)]
fn io_error(error: io::Error) -> WaveError {
    WaveError::Io { kind: error.kind() }
}

fn validate_writer_format(
    sample_rate: u32,
    channels: usize,
    options: WaveEncodeOptions,
) -> Result<u16, WaveError> {
    if sample_rate == 0 || channels == 0 {
        return Err(WaveError::InvalidFormat);
    }
    let channel_count = u16::try_from(channels).map_err(|_| WaveError::SizeOverflow)?;
    channel_count
        .checked_mul(
            u16::try_from(options.sample_format.bytes_per_sample())
                .map_err(|_| WaveError::SizeOverflow)?,
        )
        .ok_or(WaveError::SizeOverflow)?;
    Ok(channel_count)
}

fn write_placeholder_header<W: Write>(
    writer: &mut W,
    sample_rate: u32,
    channels: u16,
    options: WaveEncodeOptions,
) -> Result<(), WaveError> {
    let block_align = channels
        .checked_mul(
            u16::try_from(options.sample_format.bytes_per_sample())
                .map_err(|_| WaveError::SizeOverflow)?,
        )
        .ok_or(WaveError::SizeOverflow)?;
    let byte_rate = sample_rate
        .checked_mul(u32::from(block_align))
        .ok_or(WaveError::SizeOverflow)?;
    writer.write_all(b"RIFF").map_err(io_error)?;
    writer.write_all(&0_u32.to_le_bytes()).map_err(io_error)?;
    writer.write_all(b"WAVEfmt ").map_err(io_error)?;
    writer.write_all(&16_u32.to_le_bytes()).map_err(io_error)?;
    writer
        .write_all(&options.sample_format.encoding().to_le_bytes())
        .map_err(io_error)?;
    writer
        .write_all(&channels.to_le_bytes())
        .map_err(io_error)?;
    writer
        .write_all(&sample_rate.to_le_bytes())
        .map_err(io_error)?;
    writer
        .write_all(&byte_rate.to_le_bytes())
        .map_err(io_error)?;
    writer
        .write_all(&block_align.to_le_bytes())
        .map_err(io_error)?;
    writer
        .write_all(&options.sample_format.bits().to_le_bytes())
        .map_err(io_error)?;
    writer.write_all(b"data").map_err(io_error)?;
    writer.write_all(&0_u32.to_le_bytes()).map_err(io_error)
}

/// Encodes channel-major samples with an explicit format and quantization policy.
///
/// Float output preserves finite values and never clips or dithers. Integer
/// output accepts normalized samples in `[-1, 1]`; [`Clipping::Reject`] reports
/// an out-of-range value and [`Clipping::Hard`] clamps it. Dither is applied only
/// to integer output and is deterministic for a given seed and sample index.
pub fn encode_channels(
    sample_rate: u32,
    channels: &[Vec<f64>],
    options: WaveEncodeOptions,
) -> Result<Vec<u8>, WaveError> {
    let frames = channels.first().map_or(0, Vec::len);
    if channels.is_empty() || channels.iter().any(|channel| channel.len() != frames) {
        return Err(WaveError::InvalidFormat);
    }
    encode_planar(
        sample_rate,
        channels.len(),
        frames,
        options,
        |channel, frame| channels[channel][frame],
    )
}

/// Encodes mono samples as a 64-bit IEEE-float RIFF/WAVE stream.
///
/// This preserves reconstructed amplitudes without integer clipping or
/// quantization.
///
/// # Errors
/// Returns [`WaveError`] for a zero rate, non-finite sample, or RIFF size
/// overflow.
pub fn encode_f64_mono(sample_rate: u32, samples: &[f64]) -> Result<Vec<u8>, WaveError> {
    encode_planar(
        sample_rate,
        1,
        samples.len(),
        WaveEncodeOptions {
            sample_format: SampleFormat::F64,
            clipping: Clipping::Reject,
            dither: Dither::None,
        },
        |_, frame| samples[frame],
    )
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
    encode_channels(
        sample_rate,
        channels,
        WaveEncodeOptions {
            sample_format: SampleFormat::F64,
            clipping: Clipping::Reject,
            dither: Dither::None,
        },
    )
}

fn encode_planar(
    sample_rate: u32,
    channels: usize,
    frames: usize,
    options: WaveEncodeOptions,
    sample: impl Fn(usize, usize) -> f64,
) -> Result<Vec<u8>, WaveError> {
    if sample_rate == 0 {
        return Err(WaveError::InvalidSampleRate);
    }
    let channel_count = u16::try_from(channels).map_err(|_| WaveError::SizeOverflow)?;
    let bytes_per_sample = options.sample_format.bytes_per_sample();
    let block_align = channel_count
        .checked_mul(u16::try_from(bytes_per_sample).map_err(|_| WaveError::SizeOverflow)?)
        .ok_or(WaveError::SizeOverflow)?;
    let sample_count = frames
        .checked_mul(channels)
        .ok_or(WaveError::SizeOverflow)?;
    let sample_count = u32::try_from(sample_count).map_err(|_| WaveError::SizeOverflow)?;
    let data_size = sample_count
        .checked_mul(u32::try_from(bytes_per_sample).map_err(|_| WaveError::SizeOverflow)?)
        .ok_or(WaveError::SizeOverflow)?;
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
    wav.extend_from_slice(&options.sample_format.encoding().to_le_bytes());
    wav.extend_from_slice(&channel_count.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&options.sample_format.bits().to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());
    for frame in 0..frames {
        for channel in 0..channels {
            let index = frame
                .checked_mul(channels)
                .and_then(|value| value.checked_add(channel))
                .ok_or(WaveError::SizeOverflow)?;
            encode_sample(&mut wav, sample(channel, frame), options, index)?;
        }
    }
    Ok(wav)
}

impl SampleFormat {
    const fn encoding(self) -> u16 {
        match self {
            Self::F32 | Self::F64 => 3,
            Self::S24 | Self::S16 => 1,
        }
    }

    const fn bits(self) -> u16 {
        match self {
            Self::F32 => 32,
            Self::F64 => 64,
            Self::S24 => 24,
            Self::S16 => 16,
        }
    }

    const fn bytes_per_sample(self) -> usize {
        (self.bits() / 8) as usize
    }
}

fn encode_sample(
    output: &mut Vec<u8>,
    value: f64,
    options: WaveEncodeOptions,
    index: usize,
) -> Result<(), WaveError> {
    if !value.is_finite() {
        return Err(WaveError::NonFiniteSample { index });
    }
    match options.sample_format {
        SampleFormat::F32 => output.extend_from_slice(&(value as f32).to_le_bytes()),
        SampleFormat::F64 => output.extend_from_slice(&value.to_le_bytes()),
        SampleFormat::S16 => {
            let value = quantize_integer(value, 16, options, index)?;
            output.extend_from_slice(&value.to_le_bytes()[..2]);
        }
        SampleFormat::S24 => {
            let value = quantize_integer(value, 24, options, index)?;
            output.extend_from_slice(&value.to_le_bytes()[..3]);
        }
    }
    Ok(())
}

fn quantize_integer(
    value: f64,
    bits: u32,
    options: WaveEncodeOptions,
    index: usize,
) -> Result<i32, WaveError> {
    let normalized = if (-1.0..=1.0).contains(&value) {
        value
    } else {
        match options.clipping {
            Clipping::Reject => return Err(WaveError::OutOfRangeSample { index }),
            Clipping::Hard => value.clamp(-1.0, 1.0),
        }
    };
    let scale = f64::from(1_u32 << (bits - 1));
    let dither = match options.dither {
        Dither::None => 0.0,
        Dither::Triangular { seed } => triangular_dither(seed, index),
    };
    let minimum = -(1_i32 << (bits - 1));
    let maximum = (1_i32 << (bits - 1)) - 1;
    Ok((normalized * scale + dither)
        .round()
        .clamp(f64::from(minimum), f64::from(maximum)) as i32)
}

fn triangular_dither(seed: u64, index: usize) -> f64 {
    let mut state = (seed as u32).wrapping_add(index as u32).wrapping_add(1);
    let first = f64::from(next_dither(&mut state)) / 4_294_967_296.0;
    let second = f64::from(next_dither(&mut state)) / 4_294_967_296.0;
    first - second
}

fn next_dither(state: &mut u32) -> u32 {
    *state ^= *state << 13;
    *state ^= *state >> 17;
    *state ^= *state << 5;
    *state
}
