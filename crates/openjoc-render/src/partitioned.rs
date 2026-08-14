//! Fixed-size uniform partitioned binaural convolution.
//!
//! This module is intentionally a second, explicit implementation family next
//! to [`crate::BinauralRenderer`].  It uses overlap-add with a frequency-domain
//! delay line.  All buffers are allocated by the constructor; rendering only
//! reuses those buffers and never retains duration-proportional PCM history.

use std::sync::Arc;

use rustfft::{Fft, FftPlanner, num_complex::Complex};

use crate::{BinauralSourceBlock, HrirBank, RenderError, StaticBinauralSource};

/// Fixed configuration for a uniform partitioned convolution backend.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UniformPartitionedConfig {
    partition_size: usize,
}

impl UniformPartitionedConfig {
    /// Creates a configuration with a power-of-two partition size.
    pub fn new(partition_size: usize) -> Result<Self, RenderError> {
        if partition_size == 0 || !partition_size.is_power_of_two() {
            return Err(RenderError::InvalidPartitionSize {
                size: partition_size,
            });
        }
        Ok(Self { partition_size })
    }

    /// Returns the fixed input/output partition size.
    #[must_use]
    pub const fn partition_size(self) -> usize {
        self.partition_size
    }

    /// Returns the fixed transform size (`2 * partition_size`).
    pub fn fft_size(self) -> Result<usize, RenderError> {
        self.partition_size
            .checked_mul(2)
            .ok_or(RenderError::PartitionFftSizeOverflow)
    }
}

struct PartitionedSource {
    definition: StaticBinauralSource,
    left_spectra: Vec<Vec<Complex<f64>>>,
    right_spectra: Vec<Vec<Complex<f64>>>,
}

/// A static-source binaural renderer using fixed uniform FFT partitions.
///
/// The transform is causal overlap-add.  One complete input partition is
/// consumed and one complete output partition is emitted on each call.  The
/// reported algorithmic latency is the fixed partition size; the mathematical
/// output remains aligned with the direct causal FIR oracle once that explicit
/// block scheduling boundary is accounted for by the caller.
#[allow(clippy::struct_excessive_bools)]
pub struct PartitionedBinauralRenderer {
    sample_rate_hz: u32,
    config: UniformPartitionedConfig,
    fft_size: usize,
    sources: Vec<PartitionedSource>,
    max_partition_count: usize,
    max_tap_count: usize,
    pending_left: Vec<Vec<f64>>,
    pending_right: Vec<Vec<f64>>,
    work_left: Vec<Vec<f64>>,
    work_right: Vec<Vec<f64>>,
    forward: Arc<dyn Fft<f64>>,
    inverse: Arc<dyn Fft<f64>>,
    input_spectrum: Vec<Complex<f64>>,
    product_spectrum: Vec<Complex<f64>>,
    has_input: bool,
    tail_started: bool,
    finished: bool,
    requires_reset: bool,
    tail_block_index: usize,
    tail_offset: usize,
    tail_remaining: usize,
}

/// Explicit name for the fixed uniform convolution core.
///
/// The renderer and convolver intentionally share one implementation so the
/// lifecycle, source ordering, and tail contract cannot diverge.
pub type UniformPartitionedConvolver = PartitionedBinauralRenderer;

impl PartitionedBinauralRenderer {
    /// Creates a fixed source set and precomputes all HRIR partition spectra.
    #[allow(clippy::needless_pass_by_value)]
    pub fn new(
        sample_rate_hz: u32,
        config: UniformPartitionedConfig,
        bank: HrirBank,
        sources: Vec<StaticBinauralSource>,
    ) -> Result<Self, RenderError> {
        if sample_rate_hz == 0 {
            return Err(RenderError::InvalidSampleRate);
        }
        if bank.sample_rate_hz() != sample_rate_hz {
            return Err(RenderError::HrirSampleRateMismatch {
                expected: sample_rate_hz,
                actual: bank.sample_rate_hz(),
            });
        }
        if sources.is_empty() {
            return Err(RenderError::EmptyBinauralSourceSet);
        }
        for first in 0..sources.len() {
            for second in first + 1..sources.len() {
                if sources[first].id() == sources[second].id() {
                    return Err(RenderError::DuplicateSourceId {
                        id: sources[first].id(),
                    });
                }
            }
        }
        let fft_size = config.fft_size()?;
        let mut planner = FftPlanner::<f64>::new();
        let forward = planner.plan_fft_forward(fft_size);
        let inverse = planner.plan_fft_inverse(fft_size);
        let mut registered = Vec::with_capacity(sources.len());
        let mut max_partition_count = 0usize;
        let mut max_tap_count = 0usize;
        for definition in sources {
            let entry = bank
                .entries()
                .iter()
                .find(|entry| entry.id() == definition.hrir_entry())
                .ok_or(RenderError::UnknownHrirEntry {
                    id: definition.hrir_entry(),
                })?;
            if !same_direction(definition.direction(), entry.direction()) {
                return Err(RenderError::HrirEntryDirectionMismatch {
                    source: definition.id(),
                    entry: definition.hrir_entry(),
                });
            }
            let taps = entry.pair().tap_count();
            max_tap_count = max_tap_count.max(taps);
            let partition_count = taps
                .checked_add(config.partition_size - 1)
                .ok_or(RenderError::PartitionStateSizeOverflow)?
                / config.partition_size;
            max_partition_count = max_partition_count.max(partition_count);
            registered.push(PartitionedSource {
                definition,
                left_spectra: partition_spectra(
                    entry.pair().left_taps(),
                    config.partition_size,
                    fft_size,
                    &forward,
                ),
                right_spectra: partition_spectra(
                    entry.pair().right_taps(),
                    config.partition_size,
                    fft_size,
                    &forward,
                ),
            });
        }
        let queue_len = max_partition_count
            .checked_add(1)
            .ok_or(RenderError::PartitionStateSizeOverflow)?;
        let make_queue = || vec![vec![0.0; config.partition_size]; queue_len];
        Ok(Self {
            sample_rate_hz,
            config,
            fft_size,
            sources: registered,
            max_partition_count,
            max_tap_count,
            pending_left: make_queue(),
            pending_right: make_queue(),
            work_left: make_queue(),
            work_right: make_queue(),
            forward,
            inverse,
            input_spectrum: vec![Complex::new(0.0, 0.0); fft_size],
            product_spectrum: vec![Complex::new(0.0, 0.0); fft_size],
            has_input: false,
            tail_started: false,
            finished: false,
            requires_reset: false,
            tail_block_index: 0,
            tail_offset: 0,
            tail_remaining: 0,
        })
    }

    /// Returns the exact sample rate.
    #[must_use]
    pub const fn sample_rate_hz(&self) -> u32 {
        self.sample_rate_hz
    }

    /// Returns the fixed partition size.
    #[must_use]
    pub const fn partition_size(&self) -> usize {
        self.config.partition_size
    }

    /// Returns the fixed transform size.
    #[must_use]
    pub const fn fft_size(&self) -> usize {
        self.fft_size
    }

    /// Returns the explicit scheduling latency in samples.
    #[must_use]
    pub const fn algorithmic_latency_samples(&self) -> usize {
        self.config.partition_size
    }

    /// Returns the largest number of HRIR frequency partitions held by state.
    #[must_use]
    pub const fn max_internal_frequency_partitions(&self) -> usize {
        self.max_partition_count
    }

    /// Returns the bounded number of pending time-domain samples allocated by
    /// the renderer, independent of elapsed input duration.
    #[must_use]
    pub const fn max_pending_output_samples(&self) -> usize {
        (self.max_partition_count + 1) * self.config.partition_size
    }

    /// Returns the registered source count.
    #[must_use]
    pub fn source_count(&self) -> usize {
        self.sources.len()
    }

    /// Returns registered sources in deterministic order.
    #[must_use = "the registered source definitions are needed to inspect the renderer contract"]
    pub fn sources(&self) -> impl Iterator<Item = StaticBinauralSource> + '_ {
        self.sources.iter().map(|source| source.definition)
    }

    /// Returns the exact remaining causal tail after finalization.
    #[must_use]
    pub const fn remaining_tail_samples(&self) -> usize {
        self.tail_remaining
    }

    /// Returns whether tail draining has begun.
    #[must_use]
    pub const fn tail_started(&self) -> bool {
        self.tail_started
    }

    /// Returns whether the complete tail has been drained.
    #[must_use]
    pub const fn is_finished(&self) -> bool {
        self.finished
    }

    /// Renders exactly one complete fixed-size input partition.
    pub fn render_partition(
        &mut self,
        blocks: &[BinauralSourceBlock<'_>],
        left: &mut [f64],
        right: &mut [f64],
    ) -> Result<(), RenderError> {
        self.ensure_input_state()?;
        self.validate_blocks(blocks, self.partition_size())?;
        validate_outputs(left, right, self.partition_size())?;
        if let Err(error) = self.process_candidate(blocks) {
            left.fill(0.0);
            right.fill(0.0);
            return Err(error);
        }
        left.copy_from_slice(&self.work_left[0]);
        right.copy_from_slice(&self.work_right[0]);
        self.commit_shifted_work();
        self.has_input = true;
        Ok(())
    }

    /// Finalizes with one partial input partition and exposes its valid output.
    ///
    /// `valid_samples` may be zero.  The caller must provide one block per
    /// source, each with exactly `valid_samples` samples.  Zero padding is
    /// internal and never becomes part of the returned input-aligned output.
    pub fn finish_input(
        &mut self,
        blocks: &[BinauralSourceBlock<'_>],
        valid_samples: usize,
        left: &mut [f64],
        right: &mut [f64],
    ) -> Result<(), RenderError> {
        self.ensure_input_state()?;
        if valid_samples > self.partition_size() {
            return Err(RenderError::PartitionedInputLengthMismatch {
                expected: self.partition_size(),
                actual: valid_samples,
            });
        }
        self.validate_blocks(blocks, valid_samples)?;
        validate_outputs(left, right, valid_samples)?;
        if valid_samples == 0 {
            if !self.has_input {
                left.fill(0.0);
                right.fill(0.0);
                self.finished = true;
                self.tail_started = true;
                return Ok(());
            }
            left.fill(0.0);
            right.fill(0.0);
            self.begin_tail(0);
            return Ok(());
        }
        if let Err(error) = self.process_candidate(blocks) {
            left.fill(0.0);
            right.fill(0.0);
            return Err(error);
        }
        left.copy_from_slice(&self.work_left[0][..valid_samples]);
        right.copy_from_slice(&self.work_right[0][..valid_samples]);
        self.commit_unshifted_work();
        self.has_input = true;
        self.begin_tail(valid_samples);
        Ok(())
    }

    /// Drains at most the exact remaining tail length.
    pub fn drain_tail_block(
        &mut self,
        left: &mut [f64],
        right: &mut [f64],
    ) -> Result<(), RenderError> {
        if self.requires_reset {
            return Err(RenderError::PartitionedRequiresReset);
        }
        if self.finished {
            return Err(RenderError::PartitionedAlreadyFinished);
        }
        if !self.tail_started {
            return Err(RenderError::PartitionedInputAfterFinish);
        }
        if left.len() != right.len() {
            return Err(RenderError::BinauralOutputLengthMismatch {
                left: left.len(),
                right: right.len(),
            });
        }
        if left.len() > self.tail_remaining {
            return Err(RenderError::PartitionedTailOutputLengthMismatch {
                requested: left.len(),
                remaining: self.tail_remaining,
            });
        }
        if slices_overlap(left, right) {
            return Err(RenderError::BinauralOutputAliased);
        }
        left.fill(0.0);
        right.fill(0.0);
        let mut written = 0usize;
        while written < left.len() {
            if self.tail_block_index >= self.pending_left.len() {
                return self.numeric_failure(left, right, 0);
            }
            if self.tail_offset == self.partition_size() {
                self.tail_block_index += 1;
                self.tail_offset = 0;
                continue;
            }
            let available = self.partition_size() - self.tail_offset;
            let count = available.min(left.len() - written).min(self.tail_remaining);
            let end = self.tail_offset + count;
            left[written..written + count]
                .copy_from_slice(&self.pending_left[self.tail_block_index][self.tail_offset..end]);
            right[written..written + count]
                .copy_from_slice(&self.pending_right[self.tail_block_index][self.tail_offset..end]);
            written += count;
            self.tail_offset = end;
            self.tail_remaining -= count;
        }
        if left.iter().any(|sample| !sample.is_finite())
            || right.iter().any(|sample| !sample.is_finite())
        {
            return self.numeric_failure(left, right, 0);
        }
        if self.tail_remaining == 0 {
            self.finished = true;
        }
        Ok(())
    }

    /// Clears all frequency-domain delay state while preserving registration.
    pub fn reset(&mut self) {
        clear_queue(&mut self.pending_left);
        clear_queue(&mut self.pending_right);
        clear_queue(&mut self.work_left);
        clear_queue(&mut self.work_right);
        self.input_spectrum.fill(Complex::new(0.0, 0.0));
        self.product_spectrum.fill(Complex::new(0.0, 0.0));
        self.has_input = false;
        self.tail_started = false;
        self.finished = false;
        self.requires_reset = false;
        self.tail_block_index = 0;
        self.tail_offset = 0;
        self.tail_remaining = 0;
    }

    fn ensure_input_state(&self) -> Result<(), RenderError> {
        if self.requires_reset {
            return Err(RenderError::PartitionedRequiresReset);
        }
        if self.finished {
            return Err(RenderError::PartitionedAlreadyFinished);
        }
        if self.tail_started {
            return Err(RenderError::PartitionedInputAfterFinish);
        }
        Ok(())
    }

    fn validate_blocks(
        &self,
        blocks: &[BinauralSourceBlock<'_>],
        expected_length: usize,
    ) -> Result<(), RenderError> {
        if blocks.len() != self.sources.len() {
            return Err(RenderError::BinauralSourceCountMismatch {
                expected: self.sources.len(),
                actual: blocks.len(),
            });
        }
        for (index, block) in blocks.iter().enumerate() {
            if block.samples().len() != expected_length {
                return Err(RenderError::PartitionedInputLengthMismatch {
                    expected: expected_length,
                    actual: block.samples().len(),
                });
            }
            if self
                .sources
                .iter()
                .all(|source| source.definition.id() != block.id())
            {
                return Err(RenderError::UnknownBinauralSource { id: block.id() });
            }
            if blocks[..index]
                .iter()
                .any(|previous| previous.id() == block.id())
            {
                return Err(RenderError::DuplicateBinauralSource { id: block.id() });
            }
            if let Some(sample_index) = block
                .samples()
                .iter()
                .position(|sample| !sample.is_finite())
            {
                return Err(RenderError::NonFiniteSourceSample {
                    id: block.id(),
                    sample_index,
                });
            }
        }
        for source in &self.sources {
            if blocks
                .iter()
                .all(|block| block.id() != source.definition.id())
            {
                return Err(RenderError::MissingBinauralSource {
                    id: source.definition.id(),
                });
            }
        }
        Ok(())
    }

    fn process_candidate(&mut self, blocks: &[BinauralSourceBlock<'_>]) -> Result<(), RenderError> {
        copy_queue(&mut self.work_left, &self.pending_left);
        copy_queue(&mut self.work_right, &self.pending_right);
        // Temporarily move the immutable source table out of `self` so the
        // preallocated FFT/work fields can be mutably borrowed without
        // cloning any spectra or allocating during a render call.
        let sources = std::mem::take(&mut self.sources);
        let result = (|| {
            for source in &sources {
                let block = blocks
                    .iter()
                    .find(|block| block.id() == source.definition.id())
                    .ok_or(RenderError::MissingBinauralSource {
                        id: source.definition.id(),
                    })?;
                self.input_spectrum.fill(Complex::new(0.0, 0.0));
                for (index, sample) in block.samples().iter().enumerate() {
                    self.input_spectrum[index].re = *sample * source.definition.gain();
                }
                self.forward.process(&mut self.input_spectrum);
                for partition in 0..source.left_spectra.len() {
                    self.frequency_accumulate(&source.left_spectra[partition], partition, false)?;
                    self.frequency_accumulate(&source.right_spectra[partition], partition, true)?;
                }
            }
            if self
                .work_left
                .iter()
                .flatten()
                .chain(self.work_right.iter().flatten())
                .any(|sample| !sample.is_finite())
            {
                return self.numeric_failure(&mut [], &mut [], 0);
            }
            Ok(())
        })();
        self.sources = sources;
        result
    }

    fn frequency_accumulate(
        &mut self,
        hrir: &[Complex<f64>],
        partition: usize,
        right: bool,
    ) -> Result<(), RenderError> {
        for (product, (input, impulse)) in self
            .product_spectrum
            .iter_mut()
            .zip(self.input_spectrum.iter().zip(hrir))
        {
            *product = *input * *impulse;
        }
        self.inverse.process(&mut self.product_spectrum);
        let scale = self.fft_size as f64;
        for index in 0..self.fft_size {
            let sample = self.product_spectrum[index].re / scale;
            if !sample.is_finite() {
                return self.numeric_failure(&mut [], &mut [], index);
            }
            let (queue, offset) = if index < self.partition_size() {
                (partition, index)
            } else {
                (partition + 1, index - self.partition_size())
            };
            if right {
                self.work_right[queue][offset] += sample;
            } else {
                self.work_left[queue][offset] += sample;
            }
        }
        Ok(())
    }

    fn commit_shifted_work(&mut self) {
        let last = self.pending_left.len() - 1;
        for index in 0..last {
            self.pending_left[index].copy_from_slice(&self.work_left[index + 1]);
            self.pending_right[index].copy_from_slice(&self.work_right[index + 1]);
        }
        self.pending_left[last].fill(0.0);
        self.pending_right[last].fill(0.0);
    }

    fn commit_unshifted_work(&mut self) {
        copy_queue(&mut self.pending_left, &self.work_left);
        copy_queue(&mut self.pending_right, &self.work_right);
    }

    fn begin_tail(&mut self, valid_samples: usize) {
        self.tail_started = true;
        self.tail_block_index = 0;
        self.tail_offset = valid_samples;
        self.tail_remaining = self.max_tap_count.saturating_sub(1);
        if self.tail_remaining == 0 {
            self.finished = true;
        }
    }

    fn numeric_failure(
        &mut self,
        left: &mut [f64],
        right: &mut [f64],
        sample_index: usize,
    ) -> Result<(), RenderError> {
        left.fill(0.0);
        right.fill(0.0);
        self.requires_reset = true;
        Err(RenderError::NonFiniteOutput {
            channel: crate::OutputChannel::Left,
            sample_index,
        })
    }
}

fn partition_spectra(
    taps: &[f64],
    partition_size: usize,
    fft_size: usize,
    forward: &Arc<dyn Fft<f64>>,
) -> Vec<Vec<Complex<f64>>> {
    taps.chunks(partition_size)
        .map(|chunk| {
            let mut spectrum = vec![Complex::new(0.0, 0.0); fft_size];
            for (index, tap) in chunk.iter().enumerate() {
                spectrum[index].re = *tap;
            }
            forward.process(&mut spectrum);
            spectrum
        })
        .collect()
}

fn copy_queue(destination: &mut [Vec<f64>], source: &[Vec<f64>]) {
    for (destination, source) in destination.iter_mut().zip(source) {
        destination.copy_from_slice(source);
    }
}

fn clear_queue(queue: &mut [Vec<f64>]) {
    for block in queue {
        block.fill(0.0);
    }
}

fn validate_outputs(left: &[f64], right: &[f64], expected: usize) -> Result<(), RenderError> {
    if left.len() != right.len() {
        return Err(RenderError::BinauralOutputLengthMismatch {
            left: left.len(),
            right: right.len(),
        });
    }
    if left.len() != expected {
        return Err(RenderError::PartitionedInputLengthMismatch {
            expected,
            actual: left.len(),
        });
    }
    if slices_overlap(left, right) {
        return Err(RenderError::BinauralOutputAliased);
    }
    Ok(())
}

fn slices_overlap(left: &[f64], right: &[f64]) -> bool {
    let left_start = left.as_ptr() as usize;
    let left_end = left_start.saturating_add(left.len().saturating_mul(std::mem::size_of::<f64>()));
    let right_start = right.as_ptr() as usize;
    let right_end =
        right_start.saturating_add(right.len().saturating_mul(std::mem::size_of::<f64>()));
    left_start < right_end && right_start < left_end
}

fn same_direction(left: [f64; 3], right: [f64; 3]) -> bool {
    left.iter()
        .zip(right)
        .all(|(left, right)| (*left - right).abs() <= 1.0e-12)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BinauralRenderer, CartesianPosition, HrirEntry, HrirPair, SourceId};

    fn bank(taps: Vec<f64>) -> HrirBank {
        HrirBank::new(
            48_000,
            vec![
                HrirEntry::new(
                    crate::HrirEntryId::new(1),
                    CartesianPosition::new(0.0, 1.0, 0.0),
                    HrirPair::new(48_000, taps.clone(), taps).unwrap(),
                )
                .unwrap(),
            ],
        )
        .unwrap()
    }

    fn source() -> StaticBinauralSource {
        StaticBinauralSource::new(
            SourceId::new(1),
            CartesianPosition::new(0.0, 1.0, 0.0),
            1.0,
            crate::HrirEntryId::new(1),
        )
        .unwrap()
    }

    fn render_direct(taps: &[f64], input: &[f64]) -> Vec<f64> {
        let mut renderer =
            BinauralRenderer::new(48_000, bank(taps.to_owned()), vec![source()]).unwrap();
        let mut output = Vec::new();
        for chunk in input.chunks(31) {
            let mut left = vec![0.0; chunk.len()];
            let mut right = vec![0.0; chunk.len()];
            renderer
                .render_block(
                    &[BinauralSourceBlock::new(SourceId::new(1), chunk)],
                    &mut left,
                    &mut right,
                )
                .unwrap();
            output.extend(left);
        }
        let mut remaining = renderer.remaining_tail_samples();
        while remaining > 0 {
            let amount = remaining.min(19);
            let mut left = vec![0.0; amount];
            let mut right = vec![0.0; amount];
            renderer.drain_tail_block(&mut left, &mut right).unwrap();
            output.extend(left);
            remaining = renderer.remaining_tail_samples();
        }
        output
    }

    fn render_partitioned(
        taps: Vec<f64>,
        input: &[f64],
        partition_size: usize,
        drain: usize,
    ) -> Vec<f64> {
        let mut renderer = PartitionedBinauralRenderer::new(
            48_000,
            UniformPartitionedConfig::new(partition_size).unwrap(),
            bank(taps),
            vec![source()],
        )
        .unwrap();
        let mut output = Vec::new();
        let mut offset = 0;
        while offset + partition_size <= input.len() {
            let chunk = &input[offset..offset + partition_size];
            let mut left = vec![0.0; partition_size];
            let mut right = vec![0.0; partition_size];
            renderer
                .render_partition(
                    &[BinauralSourceBlock::new(SourceId::new(1), chunk)],
                    &mut left,
                    &mut right,
                )
                .unwrap();
            output.extend(left);
            offset += partition_size;
        }
        let partial = &input[offset..];
        let mut left = vec![0.0; partial.len()];
        let mut right = vec![0.0; partial.len()];
        renderer
            .finish_input(
                &[BinauralSourceBlock::new(SourceId::new(1), partial)],
                partial.len(),
                &mut left,
                &mut right,
            )
            .unwrap();
        output.extend(left);
        while renderer.remaining_tail_samples() > 0 {
            let amount = renderer.remaining_tail_samples().min(drain);
            let mut left = vec![0.0; amount];
            let mut right = vec![0.0; amount];
            renderer.drain_tail_block(&mut left, &mut right).unwrap();
            output.extend(left);
        }
        output
    }

    #[test]
    fn partitioned_matches_direct_for_multiple_hrir_lengths() {
        let input: Vec<f64> = (0..137).map(|index| ((index * 17) as f64).sin()).collect();
        for length in [1, 3, 17, 31, 32, 33, 63, 64, 65, 129] {
            let taps: Vec<f64> = (0..length)
                .map(|index| ((index + 1) as f64).recip() * if index % 2 == 0 { 1.0 } else { -0.5 })
                .collect();
            let expected = render_direct(&taps, &input);
            let actual = render_partitioned(taps, &input, 16, 7);
            assert_eq!(expected.len(), actual.len());
            for (expected, actual) in expected.iter().zip(actual) {
                assert!(
                    (expected - actual).abs() < 1.0e-10,
                    "{expected} vs {actual}"
                );
            }
        }
    }

    #[test]
    fn tail_chunking_is_invariant_and_reset_reuses_state() {
        let input: Vec<f64> = (0..71).map(|index| (index as f64 * 0.13).cos()).collect();
        let taps: Vec<f64> = (0..37).map(|index| 0.01 * (index as f64 + 1.0)).collect();
        let one = render_partitioned(taps.clone(), &input, 16, 1);
        let many = render_partitioned(taps.clone(), &input, 16, 1000);
        assert_eq!(one, many);
        let expected = render_direct(&taps, &input);
        assert_eq!(one.len(), expected.len());
        assert!(
            one.iter()
                .zip(expected)
                .all(|(a, b)| (a - b).abs() < 1.0e-10)
        );
    }

    #[test]
    fn fixed_latency_and_lifecycle_are_explicit() {
        let mut renderer = PartitionedBinauralRenderer::new(
            48_000,
            UniformPartitionedConfig::new(8).unwrap(),
            bank(vec![1.0; 19]),
            vec![source()],
        )
        .unwrap();
        assert_eq!(renderer.partition_size(), 8);
        assert_eq!(renderer.fft_size(), 16);
        assert_eq!(renderer.algorithmic_latency_samples(), 8);
        assert_eq!(renderer.max_internal_frequency_partitions(), 3);
        assert_eq!(renderer.max_pending_output_samples(), 32);
        let mut left = vec![0.0; 3];
        let mut right = vec![0.0; 3];
        renderer
            .finish_input(
                &[BinauralSourceBlock::new(SourceId::new(1), &[1.0, 2.0, 3.0])],
                3,
                &mut left,
                &mut right,
            )
            .unwrap();
        assert_eq!(renderer.remaining_tail_samples(), 18);
        renderer
            .drain_tail_block(&mut [0.0; 18], &mut [0.0; 18])
            .unwrap();
        assert!(renderer.is_finished());
        assert!(matches!(
            renderer.finish_input(&[], 0, &mut [], &mut []),
            Err(RenderError::PartitionedAlreadyFinished)
        ));
        renderer.reset();
        assert!(!renderer.is_finished());
    }

    #[test]
    fn several_fixed_partition_sizes_match_the_direct_oracle() {
        let input: Vec<f64> = (0..513)
            .map(|index| {
                let index = index as f64;
                (index * 0.071).sin() + 0.25 * (index * 0.013).cos()
            })
            .collect();
        let taps: Vec<f64> = (0..257)
            .map(|index| {
                let index = index as f64;
                0.002 * (index + 1.0).sin() / (index + 1.0)
            })
            .collect();
        let expected = render_direct(&taps, &input);
        for partition_size in [4, 8, 16, 32] {
            let actual = render_partitioned(taps.clone(), &input, partition_size, 3);
            assert_eq!(actual.len(), expected.len());
            assert!(
                actual
                    .iter()
                    .zip(&expected)
                    .all(|(actual, expected)| (actual - expected).abs() < 1.0e-10)
            );
        }
    }

    #[test]
    fn multiple_sources_keep_registered_order_and_gain_semantics() {
        let bank = HrirBank::new(
            48_000,
            vec![
                HrirEntry::new(
                    crate::HrirEntryId::new(1),
                    CartesianPosition::new(0.0, 1.0, 0.0),
                    HrirPair::new(48_000, vec![1.0, 0.25, -0.1], vec![0.5, 0.0, 0.2]).unwrap(),
                )
                .unwrap(),
                HrirEntry::new(
                    crate::HrirEntryId::new(2),
                    CartesianPosition::new(1.0, 0.0, 0.0),
                    HrirPair::new(
                        48_000,
                        vec![0.2, -0.3, 0.1, 0.05],
                        vec![1.0, 0.1, 0.0, -0.2],
                    )
                    .unwrap(),
                )
                .unwrap(),
            ],
        )
        .unwrap();
        let sources = vec![
            StaticBinauralSource::new(
                SourceId::new(10),
                CartesianPosition::new(0.0, 1.0, 0.0),
                0.75,
                crate::HrirEntryId::new(1),
            )
            .unwrap(),
            StaticBinauralSource::new(
                SourceId::new(20),
                CartesianPosition::new(1.0, 0.0, 0.0),
                -0.5,
                crate::HrirEntryId::new(2),
            )
            .unwrap(),
        ];
        let first: Vec<f64> = (0..37).map(|index| (index as f64 * 0.3).sin()).collect();
        let second: Vec<f64> = (0..37).map(|index| (index as f64 * 0.17).cos()).collect();
        let mut direct = BinauralRenderer::new(48_000, bank.clone(), sources.clone()).unwrap();
        let mut expected_left = Vec::new();
        let mut expected_right = Vec::new();
        for (first_chunk, second_chunk) in first.chunks(11).zip(second.chunks(11)) {
            let mut left = vec![0.0; first_chunk.len()];
            let mut right = vec![0.0; first_chunk.len()];
            direct
                .render_block(
                    &[
                        BinauralSourceBlock::new(SourceId::new(20), second_chunk),
                        BinauralSourceBlock::new(SourceId::new(10), first_chunk),
                    ],
                    &mut left,
                    &mut right,
                )
                .unwrap();
            expected_left.extend(left);
            expected_right.extend(right);
        }
        while direct.remaining_tail_samples() > 0 {
            let n = direct.remaining_tail_samples().min(9);
            let mut left = vec![0.0; n];
            let mut right = vec![0.0; n];
            direct.drain_tail_block(&mut left, &mut right).unwrap();
            expected_left.extend(left);
            expected_right.extend(right);
        }
        let mut partitioned = PartitionedBinauralRenderer::new(
            48_000,
            UniformPartitionedConfig::new(8).unwrap(),
            bank,
            sources,
        )
        .unwrap();
        let mut actual_left = Vec::new();
        let mut actual_right = Vec::new();
        let mut offset = 0;
        while offset + 8 <= first.len() {
            let mut left = vec![0.0; 8];
            let mut right = vec![0.0; 8];
            partitioned
                .render_partition(
                    &[
                        BinauralSourceBlock::new(SourceId::new(20), &second[offset..offset + 8]),
                        BinauralSourceBlock::new(SourceId::new(10), &first[offset..offset + 8]),
                    ],
                    &mut left,
                    &mut right,
                )
                .unwrap();
            actual_left.extend(left);
            actual_right.extend(right);
            offset += 8;
        }
        let n = first.len() - offset;
        let mut left = vec![0.0; n];
        let mut right = vec![0.0; n];
        partitioned
            .finish_input(
                &[
                    BinauralSourceBlock::new(SourceId::new(20), &second[offset..]),
                    BinauralSourceBlock::new(SourceId::new(10), &first[offset..]),
                ],
                n,
                &mut left,
                &mut right,
            )
            .unwrap();
        actual_left.extend(left);
        actual_right.extend(right);
        while partitioned.remaining_tail_samples() > 0 {
            let n = partitioned.remaining_tail_samples().min(5);
            let mut left = vec![0.0; n];
            let mut right = vec![0.0; n];
            partitioned.drain_tail_block(&mut left, &mut right).unwrap();
            actual_left.extend(left);
            actual_right.extend(right);
        }
        assert!(
            actual_left
                .iter()
                .zip(expected_left)
                .all(|(actual, expected)| (actual - expected).abs() < 1.0e-10)
        );
        assert!(
            actual_right
                .iter()
                .zip(expected_right)
                .all(|(actual, expected)| (actual - expected).abs() < 1.0e-10)
        );
    }

    #[test]
    fn fft_normalization_and_failure_preflight_are_explicit() {
        assert!(matches!(
            UniformPartitionedConfig::new(0),
            Err(RenderError::InvalidPartitionSize { size: 0 })
        ));
        assert!(matches!(
            UniformPartitionedConfig::new(3),
            Err(RenderError::InvalidPartitionSize { size: 3 })
        ));
        let config = UniformPartitionedConfig::new(8).unwrap();
        let mut planner = FftPlanner::<f64>::new();
        let forward = planner.plan_fft_forward(config.fft_size().unwrap());
        let inverse = planner.plan_fft_inverse(config.fft_size().unwrap());
        let mut round_trip: Vec<Complex<f64>> = (0..16)
            .map(|index| Complex::new((index as f64 * 0.17).sin(), 0.0))
            .collect();
        let expected = round_trip.clone();
        forward.process(&mut round_trip);
        inverse.process(&mut round_trip);
        let scale = config.fft_size().unwrap() as f64;
        assert!(round_trip.iter().zip(expected).all(|(actual, expected)| {
            (actual.re / scale - expected.re).abs() < 1.0e-12 && (actual.im / scale).abs() < 1.0e-12
        }));

        let mut renderer =
            PartitionedBinauralRenderer::new(48_000, config, bank(vec![f64::MAX]), vec![source()])
                .unwrap();
        let mut left = vec![7.0; 8];
        let mut right = vec![9.0; 8];
        let result = renderer.render_partition(
            &[BinauralSourceBlock::new(SourceId::new(1), &[f64::MAX; 8])],
            &mut left,
            &mut right,
        );
        assert!(matches!(result, Err(RenderError::NonFiniteOutput { .. })));
        assert!(left.iter().all(|sample| *sample == 0.0));
        assert!(right.iter().all(|sample| *sample == 0.0));
        assert!(matches!(
            renderer.render_partition(
                &[BinauralSourceBlock::new(SourceId::new(1), &[0.0; 8])],
                &mut left,
                &mut right,
            ),
            Err(RenderError::PartitionedRequiresReset)
        ));
        renderer.reset();
        renderer
            .render_partition(
                &[BinauralSourceBlock::new(SourceId::new(1), &[0.0; 8])],
                &mut left,
                &mut right,
            )
            .unwrap();
    }
}
