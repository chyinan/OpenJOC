// pattern: Functional Core

use crate::{
    JocBandCount, JocFrame, JocParseError, JocPayloadData, ReconstructionError, dequantize,
    interpolate_matrix, parse_joc_payload, reconstruct_full, reconstruct_objects,
    reconstruct_sparse,
};
use num_complex::Complex64;
use openjoc_qmf::ReferenceQmf64F64;
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    time::{Duration, Instant},
};

const QMF_SUBBANDS: usize = 64;

/// Opt-in timing for the reconstruction stages that make up one JOC decode.
///
/// This is diagnostic state only. It is disabled on the normal render path and
/// therefore does not add clock reads to ordinary decoding.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReconstructionStageTiming {
    pub payload_parsing: Duration,
    pub coefficient_decode: Duration,
    pub dequantization: Duration,
    pub qmf_analysis: Duration,
    pub interpolation: Duration,
    pub matrix_reconstruction: Duration,
    pub qmf_synthesis: Duration,
    pub output_assembly: Duration,
    pub buffer_initialization: Duration,
}

impl ReconstructionStageTiming {
    pub fn add_assign(&mut self, other: &Self) {
        self.payload_parsing += other.payload_parsing;
        self.coefficient_decode += other.coefficient_decode;
        self.dequantization += other.dequantization;
        self.qmf_analysis += other.qmf_analysis;
        self.interpolation += other.interpolation;
        self.matrix_reconstruction += other.matrix_reconstruction;
        self.qmf_synthesis += other.qmf_synthesis;
        self.output_assembly += other.output_assembly;
        self.buffer_initialization += other.buffer_initialization;
    }
}

/// Retained values at each normative matrix reconstruction stage.
#[derive(Clone, Debug, PartialEq)]
pub struct ReconstructionRowStages {
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
    /// QMF output indexed by reconstruction row, not authored object.
    pub reconstruction_qmf: Vec<Vec<[Complex64; QMF_SUBBANDS]>>,
    /// PCM rows emitted by the JOC reconstruction basis.
    pub reconstruction_basis: ReconstructionBasis,
    pub stages: Vec<Option<ReconstructionRowStages>>,
    pub state_reset: bool,
}

/// PCM emitted by JOC reconstruction before any semantic authored-object
/// binding. A row has only a structural index; it intentionally carries no
/// authored object identity.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct ReconstructionBasis {
    pub rows: Vec<Vec<f64>>,
}

/// Stable identity for one decoder reconstruction-basis coordinate.
///
/// This index is local to the decoded basis. It is deliberately not an
/// authored-object ID, OAMD slot, or output-channel identity.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ReconstructionBasisRowIndex(pub usize);

/// Borrowed PCM for one decoder reconstruction-basis coordinate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReconstructionBasisRow<'a> {
    pub index: ReconstructionBasisRowIndex,
    pub pcm: &'a [f64],
}

impl ReconstructionBasis {
    /// Iterates rows in deterministic decoder-coordinate order.
    ///
    /// The returned indices carry no authored-object semantics.
    pub fn iter_rows(&self) -> impl ExactSizeIterator<Item = ReconstructionBasisRow<'_>> {
        self.rows
            .iter()
            .enumerate()
            .map(|(index, pcm)| ReconstructionBasisRow {
                index: ReconstructionBasisRowIndex(index),
                pcm,
            })
    }
}

/// Failures joining syntax, differential, interpolation, and object stages.
#[derive(Debug)]
pub enum JocDecodeError {
    Parse(JocParseError),
    Reconstruction(ReconstructionError),
    HeaderObjectCount { header: u8, actual: usize },
    InputChannelCount { expected: usize, actual: usize },
    InputTimeslotMismatch,
    InputSampleCountNotQmfAligned { samples: usize },
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
            Self::InputSampleCountNotQmfAligned { samples } => write!(
                formatter,
                "JOC input contains {samples} samples, not a multiple of 64"
            ),
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
    synthesis_states: Vec<ReferenceQmf64F64>,
    analysis_states: Vec<ReferenceQmf64F64>,
    reconstruction_timing_enabled: bool,
    last_reconstruction_timing: ReconstructionStageTiming,
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

    /// Enables collection of one reconstruction-stage timing record per
    /// successful decode. Disabled by default so ordinary decoding has no
    /// profiling clock overhead.
    pub fn enable_reconstruction_timing(&mut self) {
        self.reconstruction_timing_enabled = true;
        self.last_reconstruction_timing = ReconstructionStageTiming::default();
    }

    /// Takes the most recent stage record, or an all-zero record when timing is
    /// disabled or no frame has completed.
    pub fn take_reconstruction_timing(&mut self) -> ReconstructionStageTiming {
        std::mem::take(&mut self.last_reconstruction_timing)
    }

    fn begin_reconstruction_timing(&mut self) {
        if self.reconstruction_timing_enabled {
            self.last_reconstruction_timing = ReconstructionStageTiming::default();
        }
    }

    fn record_timing(
        &mut self,
        stage: fn(&mut ReconstructionStageTiming) -> &mut Duration,
        start: Option<Instant>,
    ) {
        if let Some(start) = start {
            *stage(&mut self.last_reconstruction_timing) += start.elapsed();
        }
    }

    fn timing_start(&self) -> Option<Instant> {
        self.reconstruction_timing_enabled.then(Instant::now)
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
        self.begin_reconstruction_timing();
        let parse_start = self.timing_start();
        let frame = parse_joc_payload(payload)?;
        self.record_timing(|timing| &mut timing.payload_parsing, parse_start);
        let decoded = self.decode_frame_inner(&frame, inputs)?;
        Ok((frame, decoded))
    }

    /// Analyzes channel-major PCM and decodes one retained JOC frame end to end.
    ///
    /// Each channel must contain the same number of samples, divisible by 64.
    /// Analysis history is committed only when reconstruction also succeeds.
    ///
    /// # Errors
    ///
    /// Returns [`JocDecodeError`] for inconsistent PCM or JOC dimensions.
    pub fn decode_pcm_frame(
        &mut self,
        frame: &JocFrame,
        downmix_pcm: &[Vec<f64>],
    ) -> Result<DecodedJocFrame, JocDecodeError> {
        self.begin_reconstruction_timing();
        let expected_channels = usize::from(frame.header.channel_count);
        if downmix_pcm.len() != expected_channels {
            return Err(JocDecodeError::InputChannelCount {
                expected: expected_channels,
                actual: downmix_pcm.len(),
            });
        }
        let samples = downmix_pcm.first().map_or(0, Vec::len);
        if downmix_pcm.iter().any(|channel| channel.len() != samples) {
            return Err(JocDecodeError::InputTimeslotMismatch);
        }
        if samples % QMF_SUBBANDS != 0 {
            return Err(JocDecodeError::InputSampleCountNotQmfAligned { samples });
        }

        let reset =
            self.requires_state_reset(frame) || self.analysis_states.len() != expected_channels;
        let buffer_start = self.timing_start();
        let mut analysis_states = if reset {
            vec![ReferenceQmf64F64::new(); expected_channels]
        } else {
            self.analysis_states.clone()
        };
        self.record_timing(|timing| &mut timing.buffer_initialization, buffer_start);
        let analysis_start = self.timing_start();
        let inputs = downmix_pcm
            .iter()
            .zip(&mut analysis_states)
            .map(|(channel, analysis)| {
                channel
                    .chunks_exact(QMF_SUBBANDS)
                    .map(|chunk| {
                        let mut block = [0.0; QMF_SUBBANDS];
                        block.copy_from_slice(chunk);
                        analysis.analyze(&block)
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        self.record_timing(|timing| &mut timing.qmf_analysis, analysis_start);
        let decoded = self.decode_frame_inner(frame, &inputs)?;
        self.analysis_states = analysis_states;
        Ok(decoded)
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
        self.begin_reconstruction_timing();
        self.decode_frame_inner(frame, inputs)
    }

    fn decode_frame_inner(
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

        let state_reset = self.requires_state_reset(frame);
        let buffer_start = self.timing_start();
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
        self.record_timing(|timing| &mut timing.buffer_initialization, buffer_start);

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
                let coefficient_start = self.timing_start();
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
                self.record_timing(|timing| &mut timing.coefficient_decode, coefficient_start);
                quantized.push(matrix);
            }
            let dequantization_start = self.timing_start();
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
            self.record_timing(|timing| &mut timing.dequantization, dequantization_start);
            let offsets = object
                .data_points
                .iter()
                .map(|point| point.offset_timeslot)
                .collect::<Vec<_>>();
            let interpolation_start = self.timing_start();
            let interpolation = interpolate_matrix(
                &dequantized,
                &previous[object_index],
                slope,
                &offsets,
                band_count,
                timeslots,
            )?;
            self.record_timing(|timing| &mut timing.interpolation, interpolation_start);
            previous[object_index] = interpolation.next_previous;
            object_matrices.push(transpose_interpolated(
                &interpolation.matrix,
                expected_channels,
            ));
            stages.push(Some(ReconstructionRowStages {
                quantized,
                dequantized,
                interpolated: interpolation.matrix,
            }));
        }
        let matrix_start = self.timing_start();
        let reconstruction_qmf = reconstruct_objects(inputs, &object_matrices)?;
        self.record_timing(|timing| &mut timing.matrix_reconstruction, matrix_start);
        let buffer_start = self.timing_start();
        let mut synthesis_states =
            if state_reset || self.synthesis_states.len() != frame.objects.len() {
                vec![ReferenceQmf64F64::new(); frame.objects.len()]
            } else {
                self.synthesis_states.clone()
            };
        self.record_timing(|timing| &mut timing.buffer_initialization, buffer_start);
        let mut reconstruction_rows = Vec::with_capacity(reconstruction_qmf.len());
        for (timeslots, synthesis) in reconstruction_qmf.iter().zip(&mut synthesis_states) {
            let mut pcm = Vec::with_capacity(timeslots.len() * QMF_SUBBANDS);
            let synthesis_start = self.timing_start();
            for timeslot in timeslots {
                pcm.extend_from_slice(&synthesis.synthesize(timeslot));
            }
            self.record_timing(|timing| &mut timing.qmf_synthesis, synthesis_start);
            let output_start = self.timing_start();
            reconstruction_rows.push(pcm);
            self.record_timing(|timing| &mut timing.output_assembly, output_start);
        }
        self.previous_sequence = Some(frame.sequence_count);
        self.channel_count = Some(frame.header.channel_count);
        self.previous_matrices = previous;
        self.synthesis_states = synthesis_states;
        Ok(DecodedJocFrame {
            reconstruction_qmf,
            reconstruction_basis: ReconstructionBasis {
                rows: reconstruction_rows,
            },
            stages,
            state_reset,
        })
    }

    fn requires_state_reset(&self, frame: &JocFrame) -> bool {
        let sequence_reset = self.previous_sequence.is_some_and(|previous| {
            let expected = if previous == 1023 { 1 } else { previous + 1 };
            frame.sequence_count == 0 || frame.sequence_count != expected
        });
        let configuration_reset = self.previous_sequence.is_some()
            && (self.channel_count != Some(frame.header.channel_count)
                || self.previous_matrices.len() != frame.objects.len());
        sequence_reset || configuration_reset
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
