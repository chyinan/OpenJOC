use crate::ReconstructionBasis;
use openjoc_qmf::QMF_ROUNDTRIP_LATENCY_SAMPLES;
use std::{collections::VecDeque, fmt};

/// Metadata carried with one renderer-input reconstruction interval.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReconstructionTimelineMetadata {
    pub sample_rate: u32,
    pub logical_start_sample: u64,
    pub logical_end_sample: u64,
    pub qmf_latency_samples: usize,
    pub reset_epoch: u64,
    pub topology_epoch: u64,
    pub pre_roll_valid: bool,
    pub tail_flush_valid: bool,
}

/// One aligned Base/ReconstructionBasis interval owned by reconstruction output
/// assembly. The frame index remains the source frame index; the logical range
/// is not shifted to conceal the filterbank latency.
#[derive(Clone, Debug, PartialEq)]
pub struct AlignedReconstructionOutput {
    pub frame_index: u64,
    pub timeline: ReconstructionTimelineMetadata,
    pub base_full_band_pcm: Vec<Vec<f64>>,
    pub reconstruction_basis: ReconstructionBasis,
    pub lfe_pcm: Option<Vec<f64>>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ReconstructionTimelineError {
    Finished,
    InvalidSampleRange {
        start_sample: u64,
        end_sample: u64,
    },
    FrameIndexDiscontinuity {
        expected: u64,
        actual: u64,
    },
    SampleTimelineDiscontinuity {
        expected: u64,
        actual: u64,
    },
    SampleRateMismatch {
        expected: u32,
        actual: u32,
    },
    EmptyBaseCoordinates,
    BaseFrameLengthMismatch {
        channel: usize,
        expected: usize,
        actual: usize,
    },
    ReconstructionFrameLengthMismatch {
        row: usize,
        expected: usize,
        actual: usize,
    },
    LfeFrameLengthMismatch {
        expected: usize,
        actual: usize,
    },
    BaseTopologyChanged {
        expected: usize,
        actual: usize,
    },
    ReconstructionTopologyChanged {
        expected: usize,
        actual: usize,
    },
    NonFiniteBase {
        channel: usize,
        sample: usize,
    },
    NonFiniteReconstruction {
        row: usize,
        sample: usize,
    },
    NonFiniteLfe {
        sample: usize,
    },
    TailLengthMismatch {
        row: usize,
        expected: usize,
        actual: usize,
    },
    MissingTail {
        pending_frames: usize,
    },
}

impl fmt::Display for ReconstructionTimelineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Finished => formatter.write_str("reconstruction timeline is already finished"),
            Self::InvalidSampleRange {
                start_sample,
                end_sample,
            } => write!(
                formatter,
                "invalid reconstruction range [{start_sample},{end_sample})"
            ),
            Self::FrameIndexDiscontinuity { expected, actual } => {
                write!(
                    formatter,
                    "expected reconstruction frame {expected}, received {actual}"
                )
            }
            Self::SampleTimelineDiscontinuity { expected, actual } => write!(
                formatter,
                "expected reconstruction sample {expected}, received {actual}"
            ),
            Self::SampleRateMismatch { expected, actual } => {
                write!(
                    formatter,
                    "reconstruction sample rate changed from {expected} to {actual} Hz"
                )
            }
            Self::EmptyBaseCoordinates => formatter.write_str("Base coordinate PCM is empty"),
            Self::BaseFrameLengthMismatch {
                channel,
                expected,
                actual,
            } => write!(
                formatter,
                "Base coordinate {channel} has {actual} samples; expected {expected}"
            ),
            Self::ReconstructionFrameLengthMismatch {
                row,
                expected,
                actual,
            } => write!(
                formatter,
                "ReconstructionBasis row {row} has {actual} samples; expected {expected}"
            ),
            Self::LfeFrameLengthMismatch { expected, actual } => {
                write!(formatter, "LFE has {actual} samples; expected {expected}")
            }
            Self::BaseTopologyChanged { expected, actual } => write!(
                formatter,
                "Base coordinate topology changed from {expected} to {actual} channels"
            ),
            Self::ReconstructionTopologyChanged { expected, actual } => write!(
                formatter,
                "Reconstruction topology changed from {expected} to {actual} rows"
            ),
            Self::NonFiniteBase { channel, sample } => {
                write!(
                    formatter,
                    "Base coordinate {channel} has non-finite sample {sample}"
                )
            }
            Self::NonFiniteReconstruction { row, sample } => write!(
                formatter,
                "ReconstructionBasis row {row} has non-finite sample {sample}"
            ),
            Self::NonFiniteLfe { sample } => {
                write!(formatter, "LFE has non-finite sample {sample}")
            }
            Self::TailLengthMismatch {
                row,
                expected,
                actual,
            } => write!(
                formatter,
                "reconstruction tail row {row} has {actual} samples; expected {expected}"
            ),
            Self::MissingTail { pending_frames } => write!(
                formatter,
                "reconstruction tail ended with {pending_frames} pending frame(s)"
            ),
        }
    }
}

impl std::error::Error for ReconstructionTimelineError {}

#[derive(Clone, Debug)]
struct PendingFrame {
    frame_index: u64,
    sample_rate: u32,
    start_sample: u64,
    end_sample: u64,
    base_full_band_pcm: Vec<Vec<f64>>,
    lfe_pcm: Option<Vec<f64>>,
}

/// Bounded reconstruction-output timeline state.
///
/// QMF output sample `t` represents input programme sample `t-D`. The timeline
/// therefore retains decoded ReconstructionBasis PCM until the future QMF
/// output covering each pending Base interval is available. It never retains
/// more than the fixed latency plus the current pending frame material.
#[derive(Clone, Debug, Default)]
pub struct ReconstructionOutputTimeline {
    sample_rate: Option<u32>,
    next_input_frame: u64,
    next_input_sample: u64,
    base_channel_count: Option<usize>,
    reconstruction_row_count: Option<usize>,
    reconstruction_start_sample: Option<u64>,
    reconstruction_end_sample: u64,
    reconstruction_rows: Vec<VecDeque<f64>>,
    pending_frames: VecDeque<PendingFrame>,
    reset_epoch: u64,
    topology_epoch: u64,
    tail_start_sample: Option<u64>,
    finished: bool,
    peak_buffered_samples: usize,
}

impl ReconstructionOutputTimeline {
    /// Creates empty reconstruction-owned alignment state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the declared QMF latency used by this timeline.
    #[must_use]
    pub const fn qmf_latency_samples() -> usize {
        QMF_ROUNDTRIP_LATENCY_SAMPLES
    }

    /// Returns the current discontinuity epoch.
    #[must_use]
    pub const fn reset_epoch(&self) -> u64 {
        self.reset_epoch
    }

    /// Returns the current coordinate-topology epoch.
    #[must_use]
    pub const fn topology_epoch(&self) -> u64 {
        self.topology_epoch
    }

    /// Returns the maximum PCM sample slots retained by the timeline so far.
    #[must_use]
    pub const fn peak_buffered_samples(&self) -> usize {
        self.peak_buffered_samples
    }

    /// Returns an approximate upper bound for currently allocated PCM state.
    #[must_use]
    pub fn persistent_state_bytes(&self) -> usize {
        let reconstruction = self
            .reconstruction_rows
            .iter()
            .map(VecDeque::len)
            .sum::<usize>();
        let pending = self
            .pending_frames
            .iter()
            .map(|frame| {
                frame.base_full_band_pcm.iter().map(Vec::len).sum::<usize>()
                    + frame.lfe_pcm.as_ref().map_or(0, Vec::len)
            })
            .sum::<usize>();
        (reconstruction + pending) * std::mem::size_of::<f64>()
    }

    /// Adds one decoded frame and returns any complete aligned intervals.
    ///
    /// `reconstruction_basis` is the raw causal QMF output. Its sample range
    /// is the same physical range as the Base input, while its programme-time
    /// meaning is delayed by [`Self::qmf_latency_samples`].
    #[allow(clippy::too_many_arguments)]
    pub fn push_frame(
        &mut self,
        frame_index: u64,
        sample_rate: u32,
        start_sample: u64,
        end_sample: u64,
        base_full_band_pcm: &[Vec<f64>],
        reconstruction_basis: &ReconstructionBasis,
        lfe_pcm: Option<&[f64]>,
        discontinuity: bool,
    ) -> Result<Vec<AlignedReconstructionOutput>, ReconstructionTimelineError> {
        if self.finished {
            return Err(ReconstructionTimelineError::Finished);
        }
        if end_sample < start_sample {
            return Err(ReconstructionTimelineError::InvalidSampleRange {
                start_sample,
                end_sample,
            });
        }
        let frame_samples = usize::try_from(end_sample - start_sample).unwrap_or(usize::MAX);
        validate_frame_pcm(
            base_full_band_pcm,
            reconstruction_basis,
            lfe_pcm,
            frame_samples,
        )?;
        if base_full_band_pcm.is_empty() {
            return Err(ReconstructionTimelineError::EmptyBaseCoordinates);
        }

        if discontinuity {
            self.reset_for_discontinuity(frame_index, start_sample);
        } else if self.sample_rate.is_some() {
            if self.next_input_frame != frame_index {
                return Err(ReconstructionTimelineError::FrameIndexDiscontinuity {
                    expected: self.next_input_frame,
                    actual: frame_index,
                });
            }
            if self.next_input_sample != start_sample {
                return Err(ReconstructionTimelineError::SampleTimelineDiscontinuity {
                    expected: self.next_input_sample,
                    actual: start_sample,
                });
            }
        }
        if let Some(expected) = self.sample_rate {
            if expected != sample_rate {
                return Err(ReconstructionTimelineError::SampleRateMismatch {
                    expected,
                    actual: sample_rate,
                });
            }
        } else {
            self.sample_rate = Some(sample_rate);
        }

        let base_count = base_full_band_pcm.len();
        let reconstruction_count = reconstruction_basis.rows.len();
        if let Some(expected) = self.base_channel_count {
            if expected != base_count {
                return Err(ReconstructionTimelineError::BaseTopologyChanged {
                    expected,
                    actual: base_count,
                });
            }
        } else {
            self.base_channel_count = Some(base_count);
        }
        if let Some(expected) = self.reconstruction_row_count {
            if expected != reconstruction_count {
                return Err(ReconstructionTimelineError::ReconstructionTopologyChanged {
                    expected,
                    actual: reconstruction_count,
                });
            }
        } else {
            self.reconstruction_row_count = Some(reconstruction_count);
        }
        if self.reconstruction_rows.len() != reconstruction_count {
            self.reconstruction_rows = (0..reconstruction_count).map(|_| VecDeque::new()).collect();
        }

        if self.reconstruction_start_sample.is_none() {
            self.reconstruction_start_sample = Some(start_sample);
            self.reconstruction_end_sample = start_sample;
        }
        for (queue, row) in self
            .reconstruction_rows
            .iter_mut()
            .zip(&reconstruction_basis.rows)
        {
            queue.extend(row.iter().copied());
        }
        self.reconstruction_end_sample = self
            .reconstruction_end_sample
            .saturating_add(u64::try_from(frame_samples).unwrap_or(u64::MAX));
        self.pending_frames.push_back(PendingFrame {
            frame_index,
            sample_rate,
            start_sample,
            end_sample,
            base_full_band_pcm: base_full_band_pcm.to_vec(),
            lfe_pcm: lfe_pcm.map(ToOwned::to_owned),
        });
        self.next_input_frame = frame_index.saturating_add(1);
        self.next_input_sample = end_sample;
        self.update_peak_buffered_samples();
        Ok(self.drain_ready())
    }

    /// Adds the QMF reconstruction tail and emits the final pending intervals.
    /// The tail is exactly the declared QMF latency for each reconstruction row.
    pub fn finish(
        &mut self,
        reconstruction_tail: &ReconstructionBasis,
    ) -> Result<Vec<AlignedReconstructionOutput>, ReconstructionTimelineError> {
        if self.finished {
            return Err(ReconstructionTimelineError::Finished);
        }
        let expected_tail = Self::qmf_latency_samples();
        for (row_index, row) in reconstruction_tail.rows.iter().enumerate() {
            if row.len() != expected_tail {
                return Err(ReconstructionTimelineError::TailLengthMismatch {
                    row: row_index,
                    expected: expected_tail,
                    actual: row.len(),
                });
            }
            if let Some(sample) = row.iter().position(|value| !value.is_finite()) {
                return Err(ReconstructionTimelineError::NonFiniteReconstruction {
                    row: row_index,
                    sample,
                });
            }
        }
        if reconstruction_tail.rows.len()
            != self
                .reconstruction_row_count
                .unwrap_or(reconstruction_tail.rows.len())
        {
            return Err(ReconstructionTimelineError::ReconstructionTopologyChanged {
                expected: self.reconstruction_row_count.unwrap_or(0),
                actual: reconstruction_tail.rows.len(),
            });
        }
        self.tail_start_sample = Some(self.reconstruction_end_sample);
        for (queue, row) in self
            .reconstruction_rows
            .iter_mut()
            .zip(&reconstruction_tail.rows)
        {
            queue.extend(row.iter().copied());
        }
        self.reconstruction_end_sample = self
            .reconstruction_end_sample
            .saturating_add(u64::try_from(expected_tail).unwrap_or(u64::MAX));
        let output = self.drain_ready();
        self.finished = true;
        if !self.pending_frames.is_empty() {
            return Err(ReconstructionTimelineError::MissingTail {
                pending_frames: self.pending_frames.len(),
            });
        }
        Ok(output)
    }

    /// Clears all delayed Base/RB state for a new reconstruction sequence.
    pub fn reset(&mut self) {
        self.sample_rate = None;
        self.next_input_frame = 0;
        self.next_input_sample = 0;
        self.base_channel_count = None;
        self.reconstruction_row_count = None;
        self.reconstruction_start_sample = None;
        self.reconstruction_end_sample = 0;
        self.reconstruction_rows.clear();
        self.pending_frames.clear();
        self.reset_epoch = self.reset_epoch.saturating_add(1);
        self.topology_epoch = self.topology_epoch.saturating_add(1);
        self.tail_start_sample = None;
        self.finished = false;
        self.peak_buffered_samples = 0;
    }

    fn reset_for_discontinuity(&mut self, frame_index: u64, start_sample: u64) {
        let sample_rate = self.sample_rate;
        let next_topology_epoch = self.topology_epoch.saturating_add(1);
        self.pending_frames.clear();
        self.reconstruction_rows.clear();
        self.base_channel_count = None;
        self.reconstruction_row_count = None;
        self.reconstruction_start_sample = Some(start_sample);
        self.reconstruction_end_sample = start_sample;
        self.next_input_frame = frame_index;
        self.next_input_sample = start_sample;
        self.tail_start_sample = None;
        self.reset_epoch = self.reset_epoch.saturating_add(1);
        self.topology_epoch = next_topology_epoch;
        self.sample_rate = sample_rate;
    }

    fn drain_ready(&mut self) -> Vec<AlignedReconstructionOutput> {
        let latency = u64::try_from(Self::qmf_latency_samples()).unwrap_or(u64::MAX);
        let Some(reconstruction_start) = self.reconstruction_start_sample else {
            return Vec::new();
        };
        let mut output = Vec::new();
        while self.pending_frames.front().is_some_and(|pending| {
            pending
                .end_sample
                .checked_add(latency)
                .is_some_and(|end| end <= self.reconstruction_end_sample)
        }) {
            let pending = self.pending_frames.pop_front().expect("front was present");
            let physical_start = pending.start_sample.saturating_add(latency);
            let current_reconstruction_start = self
                .reconstruction_start_sample
                .unwrap_or(reconstruction_start);
            let offset =
                usize::try_from(physical_start.saturating_sub(current_reconstruction_start))
                    .unwrap_or(usize::MAX);
            let sample_count =
                usize::try_from(pending.end_sample - pending.start_sample).unwrap_or(usize::MAX);
            let reconstruction_basis = ReconstructionBasis {
                rows: self
                    .reconstruction_rows
                    .iter()
                    .map(|row| {
                        row.iter()
                            .skip(offset)
                            .take(sample_count)
                            .copied()
                            .collect()
                    })
                    .collect(),
            };
            let physical_end =
                physical_start.saturating_add(u64::try_from(sample_count).unwrap_or(0));
            let tail_flush_valid = self
                .tail_start_sample
                .is_some_and(|tail_start| physical_end > tail_start);
            output.push(AlignedReconstructionOutput {
                frame_index: pending.frame_index,
                timeline: ReconstructionTimelineMetadata {
                    sample_rate: pending.sample_rate,
                    logical_start_sample: pending.start_sample,
                    logical_end_sample: pending.end_sample,
                    qmf_latency_samples: Self::qmf_latency_samples(),
                    reset_epoch: self.reset_epoch,
                    topology_epoch: self.topology_epoch,
                    pre_roll_valid: true,
                    tail_flush_valid,
                },
                base_full_band_pcm: pending.base_full_band_pcm,
                reconstruction_basis,
                lfe_pcm: pending.lfe_pcm,
            });
            let discard_until = self
                .pending_frames
                .front()
                .map_or(self.reconstruction_end_sample, |next| {
                    next.start_sample.saturating_add(latency)
                });
            let discard = usize::try_from(
                discard_until.saturating_sub(
                    self.reconstruction_start_sample
                        .unwrap_or(reconstruction_start),
                ),
            )
            .unwrap_or(usize::MAX)
            .min(self.reconstruction_rows.first().map_or(0, VecDeque::len));
            for row in &mut self.reconstruction_rows {
                row.drain(..discard);
            }
            self.reconstruction_start_sample = Some(
                self.reconstruction_start_sample
                    .unwrap_or(reconstruction_start)
                    .saturating_add(u64::try_from(discard).unwrap_or(0)),
            );
        }
        self.update_peak_buffered_samples();
        output
    }

    fn update_peak_buffered_samples(&mut self) {
        let reconstruction = self
            .reconstruction_rows
            .iter()
            .map(VecDeque::len)
            .max()
            .unwrap_or(0);
        let pending = self
            .pending_frames
            .iter()
            .map(|frame| frame.base_full_band_pcm.first().map_or(0, Vec::len))
            .sum::<usize>();
        self.peak_buffered_samples = self
            .peak_buffered_samples
            .max(reconstruction.saturating_add(pending));
    }
}

fn validate_frame_pcm(
    base: &[Vec<f64>],
    reconstruction: &ReconstructionBasis,
    lfe: Option<&[f64]>,
    expected: usize,
) -> Result<(), ReconstructionTimelineError> {
    for (channel, pcm) in base.iter().enumerate() {
        if pcm.len() != expected {
            return Err(ReconstructionTimelineError::BaseFrameLengthMismatch {
                channel,
                expected,
                actual: pcm.len(),
            });
        }
        if let Some(sample) = pcm.iter().position(|value| !value.is_finite()) {
            return Err(ReconstructionTimelineError::NonFiniteBase { channel, sample });
        }
    }
    for (row, pcm) in reconstruction.rows.iter().enumerate() {
        if pcm.len() != expected {
            return Err(
                ReconstructionTimelineError::ReconstructionFrameLengthMismatch {
                    row,
                    expected,
                    actual: pcm.len(),
                },
            );
        }
        if let Some(sample) = pcm.iter().position(|value| !value.is_finite()) {
            return Err(ReconstructionTimelineError::NonFiniteReconstruction { row, sample });
        }
    }
    if let Some(pcm) = lfe {
        if pcm.len() != expected {
            return Err(ReconstructionTimelineError::LfeFrameLengthMismatch {
                expected,
                actual: pcm.len(),
            });
        }
        if let Some(sample) = pcm.iter().position(|value| !value.is_finite()) {
            return Err(ReconstructionTimelineError::NonFiniteLfe { sample });
        }
    }
    Ok(())
}
