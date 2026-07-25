// pattern: Functional Core

use crate::{QuantMode, Slope};
use num_complex::Complex64;
use std::fmt;

const QMF_SUBBANDS: usize = 64;

/// The eight parameter-band resolutions from TS 103 420 table 50.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JocBandCount {
    One,
    Three,
    Five,
    Seven,
    Nine,
    Twelve,
    Fifteen,
    TwentyThree,
}

impl JocBandCount {
    /// All allowed values, in table 50 signalling order.
    pub const ALL: [Self; 8] = [
        Self::One,
        Self::Three,
        Self::Five,
        Self::Seven,
        Self::Nine,
        Self::Twelve,
        Self::Fifteen,
        Self::TwentyThree,
    ];

    /// Returns the number of parameter bands.
    #[must_use]
    pub const fn value(self) -> u8 {
        match self {
            Self::One => 1,
            Self::Three => 3,
            Self::Five => 5,
            Self::Seven => 7,
            Self::Nine => 9,
            Self::Twelve => 12,
            Self::Fifteen => 15,
            Self::TwentyThree => 23,
        }
    }
}

impl TryFrom<u8> for JocBandCount {
    type Error = ReconstructionError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::One),
            3 => Ok(Self::Three),
            5 => Ok(Self::Five),
            7 => Ok(Self::Seven),
            9 => Ok(Self::Nine),
            12 => Ok(Self::Twelve),
            15 => Ok(Self::Fifteen),
            23 => Ok(Self::TwentyThree),
            value => Err(ReconstructionError::InvalidBandCount { count: value }),
        }
    }
}

/// Validation failures in the normative reconstruction stages.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReconstructionError {
    InvalidBandCount { count: u8 },
    InvalidSubband { subband: u8 },
    InvalidChannel { index: u8, channel_count: u8 },
    InvalidChannelCount { count: usize },
    QuantizedValueOutOfRange { value: u16, steps: u16 },
    DimensionMismatch { context: &'static str },
    InvalidDataPointCount { count: usize },
    InvalidTimeslotCount { count: usize },
}

impl fmt::Display for ReconstructionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBandCount { count } => write!(formatter, "invalid JOC band count {count}"),
            Self::InvalidSubband { subband } => write!(formatter, "invalid QMF subband {subband}"),
            Self::InvalidChannel {
                index,
                channel_count,
            } => write!(
                formatter,
                "invalid channel index {index} for {channel_count} channels"
            ),
            Self::InvalidChannelCount { count } => {
                write!(formatter, "invalid channel count {count}")
            }
            Self::QuantizedValueOutOfRange { value, steps } => {
                write!(formatter, "quantized value {value} exceeds {steps} steps")
            }
            Self::DimensionMismatch { context } => {
                write!(formatter, "dimension mismatch in {context}")
            }
            Self::InvalidDataPointCount { count } => {
                write!(formatter, "invalid data point count {count}")
            }
            Self::InvalidTimeslotCount { count } => {
                write!(formatter, "invalid timeslot count {count}")
            }
        }
    }
}

impl std::error::Error for ReconstructionError {}

/// Output of clause 6.6.5, including the state carried into the next frame.
#[derive(Clone, Debug, PartialEq)]
pub struct InterpolatedMatrix {
    /// Matrix layout is `[timeslot][channel][subband]`.
    pub matrix: Vec<Vec<[f64; QMF_SUBBANDS]>>,
    /// Previous matrix layout is `[channel][subband]`.
    pub next_previous: Vec<[f64; QMF_SUBBANDS]>,
}

/// Applies sparse differential decoding exactly as clause 6.6.2 pseudocode 2.
///
/// # Errors
///
/// Returns [`ReconstructionError`] for invalid channels, dimensions, or symbols.
pub fn reconstruct_sparse(
    channel_count: u8,
    mode: QuantMode,
    initial_channel: u8,
    channel_deltas: &[u16],
    vector_symbols: &[u16],
) -> Result<Vec<Vec<u16>>, ReconstructionError> {
    if !matches!(channel_count, 5 | 7) {
        return Err(ReconstructionError::InvalidChannelCount {
            count: usize::from(channel_count),
        });
    }
    if initial_channel >= channel_count {
        return Err(ReconstructionError::InvalidChannel {
            index: initial_channel,
            channel_count,
        });
    }
    if vector_symbols.is_empty() || channel_deltas.len() + 1 != vector_symbols.len() {
        return Err(ReconstructionError::DimensionMismatch {
            context: "sparse differential data",
        });
    }
    let steps = mode.steps();
    validate_symbols(vector_symbols, steps)?;
    for &delta in channel_deltas {
        if delta >= u16::from(channel_count) {
            return Err(ReconstructionError::InvalidChannel {
                index: u8::try_from(delta).unwrap_or(u8::MAX),
                channel_count,
            });
        }
    }

    let bands = vector_symbols.len();
    let offset = match mode {
        QuantMode::Coarse96 => 50,
        QuantMode::Fine192 => 100,
    };
    let mut raw_channels = Vec::with_capacity(bands);
    raw_channels.push(u16::from(initial_channel));
    raw_channels.extend_from_slice(channel_deltas);
    let mut matrix = vec![vec![offset; bands]; usize::from(channel_count)];
    for band in 0..bands {
        let selected = if band == 0 {
            raw_channels[0]
        } else {
            (raw_channels[band - 1] + raw_channels[band]) % u16::from(channel_count)
        };
        let selected = usize::from(selected);
        matrix[selected][band] = if band == 0 {
            (offset + vector_symbols[band]) % steps
        } else {
            (matrix[selected][band - 1] + vector_symbols[band]) % steps
        };
    }
    Ok(matrix)
}

/// Applies full-matrix differential decoding as clause 6.6.2 pseudocode 3.
///
/// # Errors
///
/// Returns [`ReconstructionError`] for empty/ragged matrices or invalid symbols.
pub fn reconstruct_full(
    mode: QuantMode,
    matrix_symbols: &[Vec<u16>],
) -> Result<Vec<Vec<u16>>, ReconstructionError> {
    let Some(first) = matrix_symbols.first() else {
        return Err(ReconstructionError::InvalidChannelCount { count: 0 });
    };
    if first.is_empty()
        || matrix_symbols
            .iter()
            .any(|channel| channel.len() != first.len())
    {
        return Err(ReconstructionError::DimensionMismatch {
            context: "full differential data",
        });
    }
    let steps = mode.steps();
    let offset = steps / 2;
    let mut output = Vec::with_capacity(matrix_symbols.len());
    for symbols in matrix_symbols {
        validate_symbols(symbols, steps)?;
        let mut channel = Vec::with_capacity(symbols.len());
        for (band, symbol) in symbols.iter().copied().enumerate() {
            let previous = if band == 0 { offset } else { channel[band - 1] };
            channel.push((previous + symbol) % steps);
        }
        output.push(channel);
    }
    Ok(output)
}

/// Applies clause 6.6.4 pseudocode 5 for one quantized coefficient.
///
/// # Errors
///
/// Returns [`ReconstructionError`] if `quantized` is outside the selected mode.
pub fn dequantize(quantized: u16, mode: QuantMode) -> Result<f64, ReconstructionError> {
    let steps = mode.steps();
    if quantized >= steps {
        return Err(ReconstructionError::QuantizedValueOutOfRange {
            value: quantized,
            steps,
        });
    }
    Ok((f64::from(quantized) - f64::from(steps) / 2.0) * 820.0
        / (4096.0 * (1.0 + f64::from(mode.index()))))
}

/// Maps one QMF subband through the exact grouped ranges of table 54.
///
/// # Errors
///
/// Returns [`ReconstructionError`] for subband indices above 63.
pub fn qmf_subband_to_parameter_band(
    count: JocBandCount,
    subband: u8,
) -> Result<u8, ReconstructionError> {
    if subband >= 64 {
        return Err(ReconstructionError::InvalidSubband { subband });
    }
    let widths: &[u8] = match count {
        JocBandCount::One => &[64],
        JocBandCount::Three => &[3, 11, 50],
        JocBandCount::Five => &[1, 2, 6, 14, 41],
        JocBandCount::Seven => &[1, 1, 2, 4, 6, 9, 41],
        JocBandCount::Nine => &[1, 1, 1, 2, 2, 2, 5, 9, 41],
        JocBandCount::Twelve => &[1, 1, 1, 1, 2, 2, 3, 3, 4, 5, 12, 29],
        JocBandCount::Fifteen => &[1, 1, 1, 1, 1, 1, 1, 1, 1, 2, 3, 4, 5, 12, 29],
        JocBandCount::TwentyThree => &[
            1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 4, 5, 6, 7, 16,
        ],
    };
    let mut boundary = 0_u8;
    for (band, width) in widths.iter().copied().enumerate() {
        boundary += width;
        if subband < boundary {
            return Ok(u8::try_from(band).unwrap_or(u8::MAX));
        }
    }
    Err(ReconstructionError::InvalidSubband { subband })
}

/// Applies clause 6.6.5 and returns both this frame's matrix and next state.
///
/// # Errors
///
/// Returns [`ReconstructionError`] for invalid point, channel, band, offset, or
/// previous-state dimensions.
#[allow(clippy::too_many_lines)]
pub fn interpolate_matrix(
    points: &[Vec<Vec<f64>>],
    previous: &[[f64; QMF_SUBBANDS]],
    slope: Slope,
    offsets: &[Option<u8>],
    band_count: JocBandCount,
    timeslots: usize,
) -> Result<InterpolatedMatrix, ReconstructionError> {
    if !matches!(points.len(), 1 | 2) {
        return Err(ReconstructionError::InvalidDataPointCount {
            count: points.len(),
        });
    }
    if timeslots == 0 {
        return Err(ReconstructionError::InvalidTimeslotCount { count: timeslots });
    }
    let timeslot_denominator = f64::from(
        u32::try_from(timeslots)
            .map_err(|_| ReconstructionError::InvalidTimeslotCount { count: timeslots })?,
    );
    if offsets.len() != points.len() || points.iter().any(|point| point.len() != previous.len()) {
        return Err(ReconstructionError::DimensionMismatch {
            context: "interpolation points",
        });
    }
    let bands = usize::from(band_count.value());
    if points
        .iter()
        .flat_map(|point| point.iter())
        .any(|channel| channel.len() != bands)
    {
        return Err(ReconstructionError::DimensionMismatch {
            context: "interpolation parameter bands",
        });
    }
    if slope == Slope::Steep && offsets.iter().any(Option::is_none) {
        return Err(ReconstructionError::DimensionMismatch {
            context: "steep interpolation offsets",
        });
    }

    let channels = previous.len();
    let mut matrix = vec![vec![[0.0; QMF_SUBBANDS]; channels]; timeslots];
    for channel in 0..channels {
        for subband in 0..QMF_SUBBANDS {
            let parameter_band = usize::from(qmf_subband_to_parameter_band(
                band_count,
                u8::try_from(subband).unwrap_or(u8::MAX),
            )?);
            for (timeslot, slot) in matrix.iter_mut().enumerate() {
                let timeslot_one =
                    f64::from(u32::try_from(timeslot + 1).map_err(|_| {
                        ReconstructionError::InvalidTimeslotCount { count: timeslots }
                    })?);
                slot[channel][subband] = match slope {
                    Slope::Smooth if points.len() == 1 => {
                        let delta = points[0][channel][parameter_band] - previous[channel][subband];
                        previous[channel][subband] + timeslot_one * delta / timeslot_denominator
                    }
                    Slope::Smooth => {
                        let midpoint = timeslots / 2;
                        if timeslot < midpoint {
                            let delta =
                                points[0][channel][parameter_band] - previous[channel][subband];
                            let midpoint_f64 =
                                f64::from(u32::try_from(midpoint).map_err(|_| {
                                    ReconstructionError::InvalidTimeslotCount { count: timeslots }
                                })?);
                            previous[channel][subband] + timeslot_one * delta / midpoint_f64
                        } else {
                            let delta = points[1][channel][parameter_band]
                                - points[0][channel][parameter_band];
                            let second_position = f64::from(
                                u32::try_from(timeslot - midpoint + 1).map_err(|_| {
                                    ReconstructionError::InvalidTimeslotCount { count: timeslots }
                                })?,
                            );
                            let second_length =
                                f64::from(u32::try_from(timeslots - midpoint).map_err(|_| {
                                    ReconstructionError::InvalidTimeslotCount { count: timeslots }
                                })?);
                            points[0][channel][parameter_band]
                                + second_position * delta / second_length
                        }
                    }
                    Slope::Steep if points.len() == 1 => {
                        if timeslot < usize::from(offsets[0].unwrap_or(u8::MAX)) {
                            previous[channel][subband]
                        } else {
                            points[0][channel][parameter_band]
                        }
                    }
                    Slope::Steep => {
                        if timeslot < usize::from(offsets[0].unwrap_or(u8::MAX)) {
                            previous[channel][subband]
                        } else if timeslot < usize::from(offsets[1].unwrap_or(u8::MAX)) {
                            points[0][channel][parameter_band]
                        } else {
                            points[1][channel][parameter_band]
                        }
                    }
                };
            }
        }
    }
    let last = points
        .last()
        .ok_or(ReconstructionError::InvalidDataPointCount { count: 0 })?;
    let mut next_previous = vec![[0.0; QMF_SUBBANDS]; channels];
    for channel in 0..channels {
        for subband in 0..QMF_SUBBANDS {
            let parameter_band = usize::from(qmf_subband_to_parameter_band(
                band_count,
                u8::try_from(subband).unwrap_or(u8::MAX),
            )?);
            next_previous[channel][subband] = last[channel][parameter_band];
        }
    }
    Ok(InterpolatedMatrix {
        matrix,
        next_previous,
    })
}

/// Applies clause 6.6.6 complex QMF matrix multiplication.
///
/// Inputs use `[channel][timeslot][subband]`; matrices use
/// `[object][channel][timeslot][subband]`.
///
/// # Errors
///
/// Returns [`ReconstructionError`] for any mismatched channel/timeslot shape.
pub fn reconstruct_objects(
    inputs: &[Vec<[Complex64; QMF_SUBBANDS]>],
    matrices: &[Vec<Vec<[f64; QMF_SUBBANDS]>>],
) -> Result<Vec<Vec<[Complex64; QMF_SUBBANDS]>>, ReconstructionError> {
    let Some(first_input) = inputs.first() else {
        return Err(ReconstructionError::InvalidChannelCount { count: 0 });
    };
    let timeslots = first_input.len();
    if inputs.iter().any(|channel| channel.len() != timeslots)
        || matrices.iter().any(|object| {
            object.len() != inputs.len() || object.iter().any(|channel| channel.len() != timeslots)
        })
    {
        return Err(ReconstructionError::DimensionMismatch {
            context: "object reconstruction",
        });
    }
    let mut outputs = vec![vec![[Complex64::ZERO; QMF_SUBBANDS]; timeslots]; matrices.len()];
    for (object_index, object) in matrices.iter().enumerate() {
        for (channel_index, channel_matrix) in object.iter().enumerate() {
            for (timeslot, coefficients) in channel_matrix.iter().enumerate() {
                for (subband, coefficient) in coefficients.iter().copied().enumerate() {
                    outputs[object_index][timeslot][subband] +=
                        inputs[channel_index][timeslot][subband] * coefficient;
                }
            }
        }
    }
    Ok(outputs)
}

fn validate_symbols(symbols: &[u16], steps: u16) -> Result<(), ReconstructionError> {
    for &value in symbols {
        if value >= steps {
            return Err(ReconstructionError::QuantizedValueOutOfRange { value, steps });
        }
    }
    Ok(())
}
