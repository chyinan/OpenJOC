// pattern: Functional Core

use crate::{
    JocBandCount, JocFrame, JocParseError, JocPayloadData, ReconstructionError, dequantize,
    interpolate_matrix, parse_joc_payload, reconstruct_full, reconstruct_objects,
    reconstruct_sparse,
};
use num_complex::Complex64;
use std::fmt;

const QMF_SUBBANDS: usize = 64;

/// Retained values at each normative matrix reconstruction stage.
#[derive(Clone, Debug, PartialEq)]
pub struct ObjectReconstructionStages {
    /// `[data_point][channel][parameter_band]`.
    pub quantized: Vec<Vec<Vec<u16>>>,
    /// `[data_point][channel][parameter_band]`.
    pub dequantized: Vec<Vec<Vec<f64>>>,
    /// `[timeslot][channel][subband]`.
    pub interpolated: Vec<Vec<[f64; QMF_SUBBANDS]>>,
}

/// Output of one stateful JOC frame decode.
#[derive(Clone, Debug, PartialEq)]
pub struct DecodedJocFrame {
    pub object_qmf: Vec<Vec<[Complex64; QMF_SUBBANDS]>>,
    pub stages: Vec<Option<ObjectReconstructionStages>>,
    pub state_reset: bool,
}

/// Failures joining syntax, differential, interpolation, and object stages.
#[derive(Debug)]
pub enum JocDecodeError {
    Parse(JocParseError),
    Reconstruction(ReconstructionError),
    HeaderObjectCount { header: u8, actual: usize },
    InputChannelCount { expected: usize, actual: usize },
    InputTimeslotMismatch,
    MissingObjectField { object: usize, field: &'static str },
    PayloadModeMismatch { object: usize },
}

impl fmt::Display for JocDecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(error) => write!(formatter, "failed to parse JOC payload: {error}"),
            Self::Reconstruction(error) => {
                write!(formatter, "failed to reconstruct JOC frame: {error}")
            }
            Self::HeaderObjectCount { header, actual } => {
                write!(
                    formatter,
                    "JOC header declares {header} objects but contains {actual}"
                )
            }
            Self::InputChannelCount { expected, actual } => {
                write!(
                    formatter,
                    "JOC frame requires {expected} channels but received {actual}"
                )
            }
            Self::InputTimeslotMismatch => {
                formatter.write_str("JOC input channel timeslots differ")
            }
            Self::MissingObjectField { object, field } => {
                write!(formatter, "present JOC object {object} is missing {field}")
            }
            Self::PayloadModeMismatch { object } => {
                write!(
                    formatter,
                    "JOC object {object} payload does not match sparse flag"
                )
            }
        }
    }
}

impl std::error::Error for JocDecodeError {}

impl From<JocParseError> for JocDecodeError {
    fn from(value: JocParseError) -> Self {
        Self::Parse(value)
    }
}

impl From<ReconstructionError> for JocDecodeError {
    fn from(value: ReconstructionError) -> Self {
        Self::Reconstruction(value)
    }
}

/// Cross-frame state required by clause 6.6.5 and sequence splice detection.
#[derive(Clone, Debug, Default)]
pub struct JocDecoderState {
    previous_sequence: Option<u16>,
    channel_count: Option<u8>,
    previous_matrices: Vec<Vec<[f64; QMF_SUBBANDS]>>,
}

impl JocDecoderState {
    /// Creates a decoder with the normative all-zero previous matrix.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Clears sequence and matrix history for an external discontinuity.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Parses and decodes one complete JOC payload against input channel QMF.
    ///
    /// # Errors
    ///
    /// Returns [`JocDecodeError`] for syntax or reconstruction failures.
    pub fn decode_payload(
        &mut self,
        payload: &[u8],
        inputs: &[Vec<[Complex64; QMF_SUBBANDS]>],
    ) -> Result<(JocFrame, DecodedJocFrame), JocDecodeError> {
        let frame = parse_joc_payload(payload)?;
        let decoded = self.decode_frame(&frame, inputs)?;
        Ok((frame, decoded))
    }

    /// Decodes a retained JOC frame and commits state only after full success.
    ///
    /// # Errors
    ///
    /// Returns [`JocDecodeError`] for inconsistent model or QMF dimensions.
    #[allow(clippy::too_many_lines)]
    pub fn decode_frame(
        &mut self,
        frame: &JocFrame,
        inputs: &[Vec<[Complex64; QMF_SUBBANDS]>],
    ) -> Result<DecodedJocFrame, JocDecodeError> {
        if usize::from(frame.header.object_count) != frame.objects.len() {
            return Err(JocDecodeError::HeaderObjectCount {
                header: frame.header.object_count,
                actual: frame.objects.len(),
            });
        }
        let expected_channels = usize::from(frame.header.channel_count);
        if inputs.len() != expected_channels {
            return Err(JocDecodeError::InputChannelCount {
                expected: expected_channels,
                actual: inputs.len(),
            });
        }
        let timeslots = inputs.first().map_or(0, Vec::len);
        if inputs.iter().any(|channel| channel.len() != timeslots) {
            return Err(JocDecodeError::InputTimeslotMismatch);
        }

        let sequence_reset = self.previous_sequence.is_some_and(|previous| {
            let expected = if previous == 1023 { 1 } else { previous + 1 };
            frame.sequence_count == 0 || frame.sequence_count != expected
        });
        let configuration_reset = self.previous_sequence.is_some()
            && (self.channel_count != Some(frame.header.channel_count)
                || self.previous_matrices.len() != frame.objects.len());
        let state_reset = sequence_reset || configuration_reset;
        let mut previous = if state_reset
            || self.previous_matrices.len() != frame.objects.len()
            || self
                .previous_matrices
                .iter()
                .any(|object| object.len() != expected_channels)
        {
            vec![vec![[0.0; QMF_SUBBANDS]; expected_channels]; frame.objects.len()]
        } else {
            self.previous_matrices.clone()
        };

        let mut stages = Vec::with_capacity(frame.objects.len());
        let mut object_matrices = Vec::with_capacity(frame.objects.len());
        for (object_index, object) in frame.objects.iter().enumerate() {
            if !object.present {
                let interpolated = (0..timeslots)
                    .map(|_| previous[object_index].clone())
                    .collect::<Vec<_>>();
                object_matrices.push(transpose_interpolated(&interpolated, expected_channels));
                stages.push(None);
                continue;
            }
            let mode = required(object.quant_mode, object_index, "quantization mode")?;
            let band_count_value = required(object.band_count, object_index, "band count")?;
            let band_count = JocBandCount::try_from(band_count_value)?;
            let slope = required(object.slope, object_index, "slope")?;
            let sparse = required(object.sparse, object_index, "sparse flag")?;
            let mut quantized = Vec::with_capacity(object.data_points.len());
            for data_point in &object.data_points {
                let matrix = match (&data_point.payload, sparse) {
                    (
                        JocPayloadData::Sparse {
                            initial_channel,
                            channel_deltas,
                            vector_symbols,
                        },
                        true,
                    ) => reconstruct_sparse(
                        frame.header.channel_count,
                        mode,
                        *initial_channel,
                        &channel_deltas
                            .iter()
                            .map(|code| code.symbol)
                            .collect::<Vec<_>>(),
                        &vector_symbols
                            .iter()
                            .map(|code| code.symbol)
                            .collect::<Vec<_>>(),
                    )?,
                    (JocPayloadData::Full { matrix_symbols }, false) => reconstruct_full(
                        mode,
                        &matrix_symbols
                            .iter()
                            .map(|channel| channel.iter().map(|code| code.symbol).collect())
                            .collect::<Vec<Vec<_>>>(),
                    )?,
                    _ => {
                        return Err(JocDecodeError::PayloadModeMismatch {
                            object: object_index,
                        });
                    }
                };
                quantized.push(matrix);
            }
            let dequantized = quantized
                .iter()
                .map(|point| {
                    point
                        .iter()
                        .map(|channel| {
                            channel
                                .iter()
                                .map(|value| dequantize(*value, mode))
                                .collect::<Result<Vec<_>, _>>()
                        })
                        .collect::<Result<Vec<_>, _>>()
                })
                .collect::<Result<Vec<_>, _>>()?;
            let offsets = object
                .data_points
                .iter()
                .map(|point| point.offset_timeslot)
                .collect::<Vec<_>>();
            let interpolation = interpolate_matrix(
                &dequantized,
                &previous[object_index],
                slope,
                &offsets,
                band_count,
                timeslots,
            )?;
            previous[object_index] = interpolation.next_previous;
            object_matrices.push(transpose_interpolated(
                &interpolation.matrix,
                expected_channels,
            ));
            stages.push(Some(ObjectReconstructionStages {
                quantized,
                dequantized,
                interpolated: interpolation.matrix,
            }));
        }
        let object_qmf = reconstruct_objects(inputs, &object_matrices)?;
        self.previous_sequence = Some(frame.sequence_count);
        self.channel_count = Some(frame.header.channel_count);
        self.previous_matrices = previous;
        Ok(DecodedJocFrame {
            object_qmf,
            stages,
            state_reset,
        })
    }
}

fn required<T: Copy>(
    value: Option<T>,
    object: usize,
    field: &'static str,
) -> Result<T, JocDecodeError> {
    value.ok_or(JocDecodeError::MissingObjectField { object, field })
}

fn transpose_interpolated(
    interpolated: &[Vec<[f64; QMF_SUBBANDS]>],
    channels: usize,
) -> Vec<Vec<[f64; QMF_SUBBANDS]>> {
    let mut transposed = vec![Vec::with_capacity(interpolated.len()); channels];
    for timeslot in interpolated {
        for (channel, coefficients) in timeslot.iter().enumerate() {
            transposed[channel].push(*coefficients);
        }
    }
    transposed
}
