//! Bounded direct-FIR binaural updates prepared away from the render path.

use crate::{
    BinauralResourceIdentity, BinauralSourceBlock, PreparedBinauralKernel, PreparedBinauralUpdate,
    RenderError, SourceId, update_binaural_history, validate_binaural_outputs, validate_gain,
};

/// Maximum accepted taps in one dynamic HRIR.
pub const MAX_DYNAMIC_BINAURAL_HRIR_TAPS: usize = 8_192;
/// Maximum accepted samples in one dynamic render or tail block.
pub const MAX_DYNAMIC_BINAURAL_BLOCK_SAMPLES: usize = 16_384;
/// Maximum registered dynamic source count.
pub const MAX_DYNAMIC_BINAURAL_SOURCES: usize = 128;
/// Default shared linear cross-fade length (5 ms at 48 kHz).
pub const DEFAULT_DYNAMIC_BINAURAL_TRANSITION_SAMPLES: usize = 240;
const MAX_DYNAMIC_HISTORY_BYTES: usize = 16 * 1024 * 1024;

/// One registered dynamic binaural source and its fixed linear gain.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DynamicBinauralSource {
    id: SourceId,
    gain: f64,
}

impl DynamicBinauralSource {
    /// Creates one registered source after validating its gain.
    pub fn new(id: SourceId, gain: f64) -> Result<Self, RenderError> {
        validate_gain(gain)?;
        Ok(Self { id, gain })
    }

    /// Returns the source identity.
    #[must_use]
    pub const fn id(self) -> SourceId {
        self.id
    }

    /// Returns the fixed linear source gain.
    #[must_use]
    pub const fn gain(self) -> f64 {
        self.gain
    }
}

/// A successful prepared-update admission and any request it replaced.
#[derive(Debug)]
pub struct BinauralUpdateAcceptance {
    /// Sequence accepted from the prepared update.
    pub accepted_sequence: u64,
    /// Sequence of a not-yet-started target replaced by this update, if any.
    pub superseded_sequence: Option<u64>,
    /// Replaced filter allocations. Release them away from the render path.
    pub retired_kernels: Vec<PreparedBinauralKernel>,
}

/// Rejected update and its original prepared resources, which remain caller-owned.
#[derive(Debug)]
pub struct BinauralUpdateApplyFailure {
    /// Reason the renderer rejected the update.
    pub error: RenderError,
    /// Original update. It is not consumed on validation or lifecycle failure.
    pub update: PreparedBinauralUpdate,
}

/// Failed reset request and the initial update it did not install.
#[derive(Debug)]
pub struct BinauralResetFailure {
    /// Reason the renderer rejected the reset.
    pub error: RenderError,
    /// Original initial update; still owned by the caller after failure.
    pub update: PreparedBinauralUpdate,
}

impl std::fmt::Display for BinauralUpdateApplyFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.error)
    }
}

impl std::error::Error for BinauralUpdateApplyFailure {}

impl std::fmt::Display for BinauralResetFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.error)
    }
}

impl std::error::Error for BinauralResetFailure {}

/// The most recent update that actually began affecting rendered samples.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppliedBinauralUpdate {
    /// Sequence of the update whose target kernel is in use or cross-fading.
    pub sequence: u64,
    /// First logical sample rendered with a non-zero contribution from it.
    pub logical_start_sample: u64,
}

#[derive(Clone, Debug)]
struct DynamicSourceState {
    id: SourceId,
    gain: f64,
    history: Vec<f64>,
    tail_remaining: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DynamicLifecycle {
    Active,
    ReconstructionTail,
    Draining,
    Finished,
    NumericFailure,
    EpochExhausted,
}

/// Direct-FIR renderer with bounded, off-path prepared-kernel updates.
///
/// The registered source set, gain and resource identity stay fixed. Filter
/// changes use the same input history for both kernels and a shared linear
/// output cross-fade. One extra latest-wins target may wait for an active fade.
#[derive(Clone, Debug)]
pub struct DynamicBinauralRenderer {
    sample_rate_hz: u32,
    resource_identity: BinauralResourceIdentity,
    stream_epoch: u64,
    max_filter_taps: usize,
    max_block_samples: usize,
    transition_samples: usize,
    sources: Vec<DynamicSourceState>,
    current_kernels: Vec<PreparedBinauralKernel>,
    target_kernels: Vec<PreparedBinauralKernel>,
    target_sequence: Option<u64>,
    pending_kernels: Vec<PreparedBinauralKernel>,
    pending_sequence: Option<u64>,
    current_sequence: u64,
    last_accepted_sequence: u64,
    last_applied_update: Option<AppliedBinauralUpdate>,
    transition_progress: Option<usize>,
    logical_sample_count: u64,
    lifecycle: DynamicLifecycle,
}

impl DynamicBinauralRenderer {
    /// Creates the renderer from its initial complete kernel set.
    ///
    /// `max_filter_taps` is the explicit history and filter admission bound.
    /// `max_block_samples` is the largest processing call the caller will use;
    /// it bounds per-call CPU and any wrappers that stage a source block.
    pub fn new(
        sample_rate_hz: u32,
        initial_update: PreparedBinauralUpdate,
        sources: Vec<DynamicBinauralSource>,
        max_filter_taps: usize,
        max_block_samples: usize,
        transition_samples: usize,
    ) -> Result<Self, RenderError> {
        if sample_rate_hz == 0 {
            return Err(RenderError::InvalidSampleRate);
        }
        if max_filter_taps == 0 {
            return Err(RenderError::BinauralDynamicResourceLimit);
        }
        if max_filter_taps > MAX_DYNAMIC_BINAURAL_HRIR_TAPS
            || max_block_samples > MAX_DYNAMIC_BINAURAL_BLOCK_SAMPLES
        {
            return Err(RenderError::BinauralDynamicResourceLimit);
        }
        if max_block_samples == 0 {
            return Err(RenderError::BinauralInvalidBlockLimit);
        }
        if transition_samples == 0 {
            return Err(RenderError::BinauralInvalidTransitionLength);
        }
        if transition_samples > MAX_DYNAMIC_BINAURAL_HRIR_TAPS {
            return Err(RenderError::BinauralDynamicResourceLimit);
        }
        let (resource_identity, stream_epoch, sequence, update_rate, kernels) =
            initial_update.into_parts();
        if update_rate != sample_rate_hz {
            return Err(RenderError::HrirSampleRateMismatch {
                expected: sample_rate_hz,
                actual: update_rate,
            });
        }
        validate_dynamic_source_set(&sources)?;
        validate_dynamic_kernel_set(
            sample_rate_hz,
            max_filter_taps,
            sources.len(),
            |index| sources[index].id,
            &kernels,
        )?;

        let history_len = max_filter_taps
            .checked_sub(1)
            .ok_or(RenderError::BinauralDynamicResourceLimit)?;
        let history_bytes = sources
            .len()
            .checked_mul(history_len)
            .and_then(|samples| samples.checked_mul(std::mem::size_of::<f64>()))
            .ok_or(RenderError::BinauralDynamicResourceLimit)?;
        if history_bytes > MAX_DYNAMIC_HISTORY_BYTES {
            return Err(RenderError::BinauralDynamicResourceLimit);
        }

        let dynamic_sources = sources
            .into_iter()
            .map(|source| DynamicSourceState {
                id: source.id,
                gain: source.gain,
                history: vec![0.0; history_len],
                tail_remaining: 0,
            })
            .collect();
        Ok(Self {
            sample_rate_hz,
            resource_identity,
            stream_epoch,
            max_filter_taps,
            max_block_samples,
            transition_samples,
            sources: dynamic_sources,
            current_kernels: kernels,
            target_kernels: Vec::new(),
            target_sequence: None,
            pending_kernels: Vec::new(),
            pending_sequence: None,
            current_sequence: sequence,
            last_accepted_sequence: sequence,
            last_applied_update: None,
            transition_progress: None,
            logical_sample_count: 0,
            lifecycle: DynamicLifecycle::Active,
        })
    }

    /// Returns the renderer's exact PCM sample rate.
    #[must_use]
    pub const fn sample_rate_hz(&self) -> u32 {
        self.sample_rate_hz
    }

    /// Returns the fixed non-LFE source count.
    #[must_use]
    pub fn source_count(&self) -> usize {
        self.sources.len()
    }

    /// Returns the stream epoch captured by the current kernel set.
    #[must_use]
    pub const fn stream_epoch(&self) -> u64 {
        self.stream_epoch
    }

    /// Returns the latest accepted sequence, including a queued target.
    #[must_use]
    pub const fn latest_accepted_sequence(&self) -> u64 {
        self.last_accepted_sequence
    }

    /// Returns the most recently applied sequence and its actual sample start.
    #[must_use]
    pub const fn last_applied_update(&self) -> Option<AppliedBinauralUpdate> {
        self.last_applied_update
    }

    /// Returns the next target sequence not yet affecting rendered audio.
    #[must_use]
    pub const fn pending_sequence(&self) -> Option<u64> {
        if self.transition_progress.is_some() {
            self.pending_sequence
        } else {
            self.target_sequence
        }
    }

    /// Returns the logical count of rendered or drained samples in this epoch.
    #[must_use]
    pub const fn logical_sample_count(&self) -> u64 {
        self.logical_sample_count
    }

    /// Returns bytes retained by current, transition, and one latest pending kernel set.
    #[must_use]
    pub fn hrir_kernel_storage_bytes(&self) -> usize {
        [
            &self.current_kernels,
            &self.target_kernels,
            &self.pending_kernels,
        ]
        .into_iter()
        .flat_map(|kernels| kernels.iter())
        .map(|kernel| {
            (kernel.pair().left_taps().len() + kernel.pair().right_taps().len())
                * std::mem::size_of::<f64>()
        })
        .sum()
    }

    /// Returns bytes retained by shared input histories.
    #[must_use]
    pub fn hrir_history_storage_bytes(&self) -> usize {
        self.sources
            .iter()
            .map(|source| source.history.capacity() * std::mem::size_of::<f64>())
            .sum()
    }

    /// Returns the largest remaining causal FIR tail in samples.
    #[must_use]
    pub fn remaining_tail_samples(&self) -> usize {
        self.sources
            .iter()
            .map(|source| source.tail_remaining)
            .max()
            .unwrap_or(0)
    }

    /// Accepts a complete off-path update without doing direction search or FIR work.
    ///
    /// If a transition is already active, this retains only the latest waiting
    /// update and reports the replaced waiting sequence, if there was one.
    pub fn apply_prepared(
        &mut self,
        update: PreparedBinauralUpdate,
    ) -> Result<BinauralUpdateAcceptance, BinauralUpdateApplyFailure> {
        if let Err(error) = self.ensure_renderable() {
            return Err(BinauralUpdateApplyFailure { error, update });
        }
        let resource = update.resource_identity();
        let epoch = update.stream_epoch();
        let sequence = update.sequence();
        let sample_rate = update.sample_rate_hz();
        if resource != self.resource_identity {
            return Err(BinauralUpdateApplyFailure {
                error: RenderError::BinauralResourceIdentityMismatch,
                update,
            });
        }
        if epoch != self.stream_epoch {
            return Err(BinauralUpdateApplyFailure {
                error: RenderError::BinauralUpdateEpochMismatch {
                    expected: self.stream_epoch,
                    actual: epoch,
                },
                update,
            });
        }
        if sequence <= self.last_accepted_sequence {
            return Err(BinauralUpdateApplyFailure {
                error: RenderError::BinauralUpdateSequenceNotIncreasing {
                    previous: self.last_accepted_sequence,
                    actual: sequence,
                },
                update,
            });
        }
        if sample_rate != self.sample_rate_hz {
            return Err(BinauralUpdateApplyFailure {
                error: RenderError::HrirSampleRateMismatch {
                    expected: self.sample_rate_hz,
                    actual: sample_rate,
                },
                update,
            });
        }
        if let Err(error) = validate_dynamic_kernel_set(
            self.sample_rate_hz,
            self.max_filter_taps,
            self.sources.len(),
            |index| self.sources[index].id,
            update.kernels(),
        ) {
            return Err(BinauralUpdateApplyFailure { error, update });
        }
        let (_, _, _, _, kernels) = update.into_parts();
        let superseded_sequence = if self.transition_progress.is_some() {
            self.pending_sequence.replace(sequence)
        } else {
            self.target_sequence.replace(sequence)
        };
        let retired_kernels = if self.transition_progress.is_some() {
            std::mem::replace(&mut self.pending_kernels, kernels)
        } else {
            std::mem::replace(&mut self.target_kernels, kernels)
        };
        self.last_accepted_sequence = sequence;
        Ok(BinauralUpdateAcceptance {
            accepted_sequence: sequence,
            superseded_sequence,
            retired_kernels,
        })
    }

    /// Renders one equal-length mono block per registered source.
    ///
    /// A zero-length call does not apply a queued update. The first non-empty
    /// block records the true logical sample where its target begins to affect
    /// the output. Input-history and static identity-path accumulation order
    /// match [`crate::BinauralRenderer::render_block`].
    pub fn render_block(
        &mut self,
        blocks: &[BinauralSourceBlock<'_>],
        left: &mut [f64],
        right: &mut [f64],
    ) -> Result<(), RenderError> {
        self.ensure_renderable()?;
        self.render_block_inner(blocks, left, right)
    }

    /// Begins drain mode: active fades may finish, while a queued target cannot
    /// start during decoder-produced reconstruction tail blocks.
    pub fn begin_drain(&mut self) -> Result<(), RenderError> {
        match self.lifecycle {
            DynamicLifecycle::Active | DynamicLifecycle::ReconstructionTail => {
                self.lifecycle = DynamicLifecycle::ReconstructionTail;
                Ok(())
            }
            DynamicLifecycle::Draining => Ok(()),
            DynamicLifecycle::Finished => Err(RenderError::BinauralAlreadyFinished),
            DynamicLifecycle::NumericFailure | DynamicLifecycle::EpochExhausted => {
                Err(RenderError::BinauralRequiresReset)
            }
        }
    }

    /// Renders decoder-produced reconstruction tail after [`Self::begin_drain`].
    /// A waiting pose remains pending; only a fade already in progress advances.
    pub fn render_reconstruction_tail_block(
        &mut self,
        blocks: &[BinauralSourceBlock<'_>],
        left: &mut [f64],
        right: &mut [f64],
    ) -> Result<(), RenderError> {
        if self.lifecycle != DynamicLifecycle::ReconstructionTail {
            return Err(RenderError::BinauralInputAfterTailStart);
        }
        self.render_block_inner(blocks, left, right)
    }

    fn render_block_inner(
        &mut self,
        blocks: &[BinauralSourceBlock<'_>],
        left: &mut [f64],
        right: &mut [f64],
    ) -> Result<(), RenderError> {
        validate_binaural_outputs(left, right)?;
        if left.len() > self.max_block_samples {
            return Err(RenderError::BinauralBlockLimitExceeded {
                actual: left.len(),
                maximum: self.max_block_samples,
            });
        }
        let block_length = left.len();
        validate_binaural_blocks(&self.sources, blocks, block_length)?;
        if block_length == 0 {
            left.fill(0.0);
            right.fill(0.0);
            return Ok(());
        }
        let sample_count =
            u64::try_from(block_length).map_err(|_| RenderError::SampleIndexOverflow)?;
        self.logical_sample_count
            .checked_add(sample_count)
            .ok_or(RenderError::SampleIndexOverflow)?;
        self.render_samples(Some(blocks), left, right)
    }

    /// Drains no more than the remaining effective FIR tail.
    ///
    /// An active transition continues by sample count. A not-yet-started
    /// target remains pending and does not extend the tail.
    pub fn drain_tail_block(
        &mut self,
        left: &mut [f64],
        right: &mut [f64],
    ) -> Result<(), RenderError> {
        if matches!(
            self.lifecycle,
            DynamicLifecycle::NumericFailure | DynamicLifecycle::EpochExhausted
        ) {
            return Err(RenderError::BinauralRequiresReset);
        }
        if self.lifecycle == DynamicLifecycle::Finished {
            return Err(RenderError::BinauralAlreadyFinished);
        }
        validate_binaural_outputs(left, right)?;
        if left.len() > self.max_block_samples {
            return Err(RenderError::BinauralBlockLimitExceeded {
                actual: left.len(),
                maximum: self.max_block_samples,
            });
        }
        let requested = left.len();
        let remaining = self.remaining_tail_samples();
        if requested > remaining {
            return Err(RenderError::TailOutputLengthMismatch {
                requested,
                remaining,
            });
        }
        let sample_count =
            u64::try_from(requested).map_err(|_| RenderError::SampleIndexOverflow)?;
        self.logical_sample_count
            .checked_add(sample_count)
            .ok_or(RenderError::SampleIndexOverflow)?;
        self.lifecycle = DynamicLifecycle::Draining;
        if requested == 0 {
            left.fill(0.0);
            right.fill(0.0);
            if self.remaining_tail_samples() == 0 {
                self.lifecycle = DynamicLifecycle::Finished;
            }
            return Ok(());
        }
        self.render_samples(None, left, right)?;
        for source in &mut self.sources {
            source.tail_remaining = source.tail_remaining.saturating_sub(requested);
        }
        if self.remaining_tail_samples() == 0 {
            self.lifecycle = DynamicLifecycle::Finished;
        }
        Ok(())
    }

    /// Resets histories and lifecycle state while installing the next epoch's initial kernel set.
    ///
    /// The supplied update must use exactly the next epoch. Preparing identity
    /// kernels remains the caller's off-render responsibility.
    pub fn reset_to(
        &mut self,
        initial_update: PreparedBinauralUpdate,
    ) -> Result<Vec<Vec<PreparedBinauralKernel>>, BinauralResetFailure> {
        if self.lifecycle == DynamicLifecycle::EpochExhausted {
            return Err(BinauralResetFailure {
                error: RenderError::BinauralRequiresReset,
                update: initial_update,
            });
        }
        let Some(next_epoch) = self.stream_epoch.checked_add(1) else {
            self.lifecycle = DynamicLifecycle::EpochExhausted;
            return Err(BinauralResetFailure {
                error: RenderError::BinauralStreamEpochOverflow,
                update: initial_update,
            });
        };
        let resource = initial_update.resource_identity();
        let epoch = initial_update.stream_epoch();
        if resource != self.resource_identity {
            return Err(BinauralResetFailure {
                error: RenderError::BinauralResourceIdentityMismatch,
                update: initial_update,
            });
        }
        if epoch != next_epoch {
            return Err(BinauralResetFailure {
                error: RenderError::BinauralUpdateEpochMismatch {
                    expected: next_epoch,
                    actual: epoch,
                },
                update: initial_update,
            });
        }
        let sequence = initial_update.sequence();
        let sample_rate = initial_update.sample_rate_hz();
        if sample_rate != self.sample_rate_hz {
            return Err(BinauralResetFailure {
                error: RenderError::HrirSampleRateMismatch {
                    expected: self.sample_rate_hz,
                    actual: sample_rate,
                },
                update: initial_update,
            });
        }
        if let Err(error) = validate_dynamic_kernel_set(
            self.sample_rate_hz,
            self.max_filter_taps,
            self.sources.len(),
            |index| self.sources[index].id,
            initial_update.kernels(),
        ) {
            return Err(BinauralResetFailure {
                error,
                update: initial_update,
            });
        }
        let (_, _, _, _, kernels) = initial_update.into_parts();
        for source in &mut self.sources {
            source.history.fill(0.0);
            source.tail_remaining = 0;
        }
        let retired_current = std::mem::replace(&mut self.current_kernels, kernels);
        let retired_target = std::mem::take(&mut self.target_kernels);
        self.target_sequence = None;
        let retired_pending = std::mem::take(&mut self.pending_kernels);
        self.pending_sequence = None;
        self.stream_epoch = next_epoch;
        self.current_sequence = sequence;
        self.last_accepted_sequence = sequence;
        self.last_applied_update = None;
        self.transition_progress = None;
        self.logical_sample_count = 0;
        self.lifecycle = DynamicLifecycle::Active;
        Ok(vec![retired_current, retired_target, retired_pending])
    }

    fn ensure_renderable(&self) -> Result<(), RenderError> {
        match self.lifecycle {
            DynamicLifecycle::Active => Ok(()),
            DynamicLifecycle::ReconstructionTail | DynamicLifecycle::Draining => {
                Err(RenderError::BinauralInputAfterTailStart)
            }
            DynamicLifecycle::Finished => Err(RenderError::BinauralAlreadyFinished),
            DynamicLifecycle::NumericFailure | DynamicLifecycle::EpochExhausted => {
                Err(RenderError::BinauralRequiresReset)
            }
        }
    }

    fn render_samples(
        &mut self,
        blocks: Option<&[BinauralSourceBlock<'_>]>,
        left: &mut [f64],
        right: &mut [f64],
    ) -> Result<(), RenderError> {
        left.fill(0.0);
        right.fill(0.0);
        for offset in 0..left.len() {
            if blocks.is_some() && self.lifecycle == DynamicLifecycle::Active {
                let logical_start = self
                    .logical_sample_count
                    .checked_add(offset as u64)
                    .ok_or(RenderError::SampleIndexOverflow)?;
                self.begin_target_if_ready(logical_start);
            } else if blocks.is_some()
                && self.lifecycle == DynamicLifecycle::ReconstructionTail
                && self.last_applied_update.is_none()
            {
                let logical_start = self
                    .logical_sample_count
                    .checked_add(offset as u64)
                    .ok_or(RenderError::SampleIndexOverflow)?;
                self.last_applied_update = Some(AppliedBinauralUpdate {
                    sequence: self.current_sequence,
                    logical_start_sample: logical_start,
                });
            }
            let transitioning = self.transition_progress.is_some();
            let mut old_left = 0.0;
            let mut old_right = 0.0;
            let mut new_left = 0.0;
            let mut new_right = 0.0;
            for (source_index, source) in self.sources.iter().enumerate() {
                if blocks.is_none() && offset >= source.tail_remaining {
                    continue;
                }
                let input_block =
                    blocks.and_then(|blocks| blocks.iter().find(|block| block.id() == source.id));
                let current_pair = self.current_kernels[source_index].pair();
                let target_pair = transitioning.then(|| self.target_kernels[source_index].pair());
                let history_len = source.history.len();
                for tap_index in 0..current_pair.tap_count() {
                    let input = if blocks.is_some() && tap_index <= offset {
                        input_block.map_or(0.0, |block| {
                            block.samples()[offset - tap_index] * source.gain
                        })
                    } else if tap_index <= offset {
                        0.0
                    } else {
                        source.history[history_len - (tap_index - offset)]
                    };
                    old_left += input * current_pair.left_taps()[tap_index];
                    old_right += input * current_pair.right_taps()[tap_index];
                }
                if let Some(target_pair) = target_pair {
                    for tap_index in 0..target_pair.tap_count() {
                        let input = if blocks.is_some() && tap_index <= offset {
                            input_block.map_or(0.0, |block| {
                                block.samples()[offset - tap_index] * source.gain
                            })
                        } else if tap_index <= offset {
                            0.0
                        } else {
                            source.history[history_len - (tap_index - offset)]
                        };
                        new_left += input * target_pair.left_taps()[tap_index];
                        new_right += input * target_pair.right_taps()[tap_index];
                    }
                }
            }
            if transitioning {
                let progress = self.transition_progress.unwrap_or(0);
                let alpha = ((progress + 1) as f64 / self.transition_samples as f64).min(1.0);
                let old_weight = 1.0 - alpha;
                left[offset] = old_left * old_weight + new_left * alpha;
                right[offset] = old_right * old_weight + new_right * alpha;
            } else {
                left[offset] = old_left;
                right[offset] = old_right;
            }
            if !left[offset].is_finite() {
                return self.numeric_failure(left, right, crate::OutputChannel::Left, offset);
            }
            if !right[offset].is_finite() {
                return self.numeric_failure(left, right, crate::OutputChannel::Right, offset);
            }
            if transitioning {
                let progress = self.transition_progress.unwrap_or(0) + 1;
                if progress >= self.transition_samples {
                    self.finish_transition();
                } else {
                    self.transition_progress = Some(progress);
                }
            }
        }
        match blocks {
            Some(blocks) => {
                for (source_index, source) in self.sources.iter_mut().enumerate() {
                    let block = blocks
                        .iter()
                        .find(|block| block.id() == source.id)
                        .ok_or(RenderError::MissingBinauralSource { id: source.id })?;
                    update_binaural_history(&mut source.history, block.samples(), source.gain);
                    source.tail_remaining = self.current_kernels[source_index]
                        .pair()
                        .tap_count()
                        .saturating_sub(1);
                    if self.transition_progress.is_some() {
                        source.tail_remaining = source.tail_remaining.max(
                            self.target_kernels[source_index]
                                .pair()
                                .tap_count()
                                .saturating_sub(1),
                        );
                    }
                }
            }
            None => {
                for source in &mut self.sources {
                    let history_len = source.history.len();
                    if left.len() >= history_len {
                        source.history.fill(0.0);
                    } else if !source.history.is_empty() {
                        source.history.copy_within(left.len().., 0);
                        source.history[history_len - left.len()..].fill(0.0);
                    }
                }
            }
        }
        self.logical_sample_count = self
            .logical_sample_count
            .checked_add(left.len() as u64)
            .ok_or(RenderError::SampleIndexOverflow)?;
        Ok(())
    }

    fn begin_target_if_ready(&mut self, logical_start_sample: u64) {
        if self.transition_progress.is_some() {
            return;
        }
        let Some(sequence) = self.target_sequence else {
            if self.last_applied_update.is_none() {
                self.last_applied_update = Some(AppliedBinauralUpdate {
                    sequence: self.current_sequence,
                    logical_start_sample,
                });
            }
            return;
        };
        if self
            .current_kernels
            .iter()
            .zip(&self.target_kernels)
            .all(|(current, target)| current.pair() == target.pair())
        {
            std::mem::swap(&mut self.current_kernels, &mut self.target_kernels);
            self.target_sequence = None;
            self.current_sequence = sequence;
            self.last_applied_update = Some(AppliedBinauralUpdate {
                sequence,
                logical_start_sample,
            });
            return;
        }
        self.current_sequence = sequence;
        self.last_applied_update = Some(AppliedBinauralUpdate {
            sequence,
            logical_start_sample,
        });
        self.transition_progress = Some(0);
    }

    fn finish_transition(&mut self) {
        std::mem::swap(&mut self.current_kernels, &mut self.target_kernels);
        self.target_sequence = None;
        self.transition_progress = None;
        if let Some(sequence) = self.pending_sequence.take() {
            std::mem::swap(&mut self.target_kernels, &mut self.pending_kernels);
            self.target_sequence = Some(sequence);
        }
    }

    fn numeric_failure(
        &mut self,
        left: &mut [f64],
        right: &mut [f64],
        channel: crate::OutputChannel,
        sample_index: usize,
    ) -> Result<(), RenderError> {
        left.fill(0.0);
        right.fill(0.0);
        self.lifecycle = DynamicLifecycle::NumericFailure;
        Err(RenderError::NonFiniteOutput {
            channel,
            sample_index,
        })
    }
}

fn validate_dynamic_source_set(sources: &[DynamicBinauralSource]) -> Result<(), RenderError> {
    if sources.is_empty() {
        return Err(RenderError::EmptyBinauralSourceSet);
    }
    if sources.len() > MAX_DYNAMIC_BINAURAL_SOURCES {
        return Err(RenderError::BinauralDynamicResourceLimit);
    }
    for (index, source) in sources.iter().enumerate() {
        validate_gain(source.gain)?;
        if sources[..index]
            .iter()
            .any(|previous| previous.id == source.id)
        {
            return Err(RenderError::DuplicateSourceId { id: source.id });
        }
    }
    Ok(())
}

fn validate_dynamic_kernel_set(
    expected_rate: u32,
    max_filter_taps: usize,
    source_count: usize,
    source_id_at: impl Fn(usize) -> SourceId,
    kernels: &[PreparedBinauralKernel],
) -> Result<(), RenderError> {
    if expected_rate == 0 {
        return Err(RenderError::InvalidSampleRate);
    }
    if kernels.len() != source_count {
        return Err(RenderError::BinauralUpdateSourceCountMismatch {
            expected: source_count,
            actual: kernels.len(),
        });
    }
    for (index, kernel) in kernels.iter().enumerate() {
        let expected_source = source_id_at(index);
        if kernel.source_id() != expected_source {
            return Err(RenderError::BinauralUpdateSourceMismatch {
                position: index,
                expected: expected_source,
                actual: kernel.source_id(),
            });
        }
        if kernel.pair().sample_rate_hz() != expected_rate {
            return Err(RenderError::HrirSampleRateMismatch {
                expected: expected_rate,
                actual: kernel.pair().sample_rate_hz(),
            });
        }
        if kernel.pair().tap_count() > max_filter_taps {
            return Err(RenderError::BinauralUpdateTapLimitExceeded {
                id: expected_source,
                actual: kernel.pair().tap_count(),
                maximum: max_filter_taps,
            });
        }
    }
    Ok(())
}

fn validate_binaural_blocks(
    sources: &[DynamicSourceState],
    blocks: &[BinauralSourceBlock<'_>],
    block_length: usize,
) -> Result<(), RenderError> {
    if blocks.len() != sources.len() {
        return Err(RenderError::BinauralSourceCountMismatch {
            expected: sources.len(),
            actual: blocks.len(),
        });
    }
    for (block_index, block) in blocks.iter().enumerate() {
        if block.samples().len() != block_length {
            return Err(RenderError::SourceBlockLengthMismatch {
                id: block.id(),
                expected: block_length,
                actual: block.samples().len(),
            });
        }
        if sources.iter().all(|source| source.id != block.id()) {
            return Err(RenderError::UnknownBinauralSource { id: block.id() });
        }
        if blocks[..block_index]
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
    for source in sources {
        if blocks.iter().all(|block| block.id() != source.id) {
            return Err(RenderError::MissingBinauralSource { id: source.id });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        BinauralRenderer, CartesianPosition, HrirBank, HrirEntry, HrirEntryId, HrirPair,
        StaticBinauralSource,
    };

    const RATE: u32 = 48_000;
    const ID: SourceId = SourceId::new(7);
    const ID_2: SourceId = SourceId::new(8);
    const RESOURCE: BinauralResourceIdentity = BinauralResourceIdentity::new([0x5a; 32]);

    fn pair(left: &[f64], right: &[f64]) -> HrirPair {
        HrirPair::new(RATE, left.to_vec(), right.to_vec()).unwrap()
    }

    fn update(epoch: u64, sequence: u64, response: HrirPair) -> PreparedBinauralUpdate {
        PreparedBinauralUpdate::new(
            RESOURCE,
            epoch,
            sequence,
            vec![PreparedBinauralKernel::new(ID, response)],
        )
        .unwrap()
    }

    fn renderer(
        initial: HrirPair,
        max_filter_taps: usize,
        fade_samples: usize,
    ) -> DynamicBinauralRenderer {
        DynamicBinauralRenderer::new(
            RATE,
            update(0, 0, initial),
            vec![DynamicBinauralSource::new(ID, 0.75).unwrap()],
            max_filter_taps,
            16,
            fade_samples,
        )
        .unwrap()
    }

    fn update_two(
        epoch: u64,
        sequence: u64,
        first: HrirPair,
        second: HrirPair,
    ) -> PreparedBinauralUpdate {
        PreparedBinauralUpdate::new(
            RESOURCE,
            epoch,
            sequence,
            vec![
                PreparedBinauralKernel::new(ID, first),
                PreparedBinauralKernel::new(ID_2, second),
            ],
        )
        .unwrap()
    }

    fn render_dynamic(
        renderer: &mut DynamicBinauralRenderer,
        input: &[f64],
    ) -> (Vec<f64>, Vec<f64>) {
        let mut left = vec![0.0; input.len()];
        let mut right = vec![0.0; input.len()];
        renderer
            .render_block(
                &[BinauralSourceBlock::new(ID, input)],
                &mut left,
                &mut right,
            )
            .unwrap();
        (left, right)
    }

    fn convolve_history(input: &[f64], response: &[f64], sample_index: usize) -> f64 {
        let mut value = 0.0;
        for (tap_index, tap) in response.iter().enumerate() {
            if tap_index <= sample_index {
                value += input[sample_index - tap_index] * 0.75 * tap;
            }
        }
        value
    }

    #[test]
    fn identity_path_matches_static_renderer_bit_for_bit_and_keeps_effective_tail() {
        let response = pair(&[0.4, -0.2, 0.1, 0.05], &[-0.3, 0.15, 0.2, -0.1]);
        let entry_id = HrirEntryId::new(4);
        let direction = CartesianPosition::new(0.0, 1.0, 0.0);
        let bank = HrirBank::new(
            RATE,
            vec![HrirEntry::new(entry_id, direction, response.clone()).unwrap()],
        )
        .unwrap();
        let source = StaticBinauralSource::new(ID, direction, 0.75, entry_id).unwrap();
        let mut static_renderer = BinauralRenderer::new(RATE, bank, vec![source]).unwrap();
        let mut dynamic_renderer = renderer(response, 16, 8);
        let input = (0..13)
            .map(|index| ((index * 17 % 11) as f64 - 5.0) / 7.0)
            .collect::<Vec<_>>();
        let mut static_left = Vec::new();
        let mut static_right = Vec::new();
        let mut dynamic_left = Vec::new();
        let mut dynamic_right = Vec::new();
        for range in [0..1, 1..6, 6..13] {
            let block = &input[range];
            let mut left = vec![0.0; block.len()];
            let mut right = vec![0.0; block.len()];
            static_renderer
                .render_block(
                    &[BinauralSourceBlock::new(ID, block)],
                    &mut left,
                    &mut right,
                )
                .unwrap();
            static_left.extend_from_slice(&left);
            static_right.extend_from_slice(&right);
            let (left, right) = render_dynamic(&mut dynamic_renderer, block);
            dynamic_left.extend(left);
            dynamic_right.extend(right);
        }
        assert_eq!(dynamic_left, static_left);
        assert_eq!(dynamic_right, static_right);
        assert_eq!(dynamic_renderer.remaining_tail_samples(), 3);
        assert_eq!(static_renderer.remaining_tail_samples(), 3);
        let mut static_tail_left = vec![0.0; 3];
        let mut static_tail_right = vec![0.0; 3];
        let mut dynamic_tail_left = vec![0.0; 3];
        let mut dynamic_tail_right = vec![0.0; 3];
        static_renderer
            .drain_tail_block(&mut static_tail_left, &mut static_tail_right)
            .unwrap();
        dynamic_renderer
            .drain_tail_block(&mut dynamic_tail_left, &mut dynamic_tail_right)
            .unwrap();
        assert_eq!(dynamic_tail_left, static_tail_left);
        assert_eq!(dynamic_tail_right, static_tail_right);
    }

    #[test]
    fn multisource_identity_path_matches_static_accumulation_order() {
        let first = pair(&[0.2, 0.1, -0.05, 0.03], &[0.1, -0.1, 0.2, 0.04]);
        let second = pair(&[-0.1, 0.3, 0.05], &[0.25, 0.0, -0.15]);
        let entry_a = HrirEntryId::new(11);
        let entry_b = HrirEntryId::new(12);
        let front = CartesianPosition::new(0.0, 1.0, 0.0);
        let rear = CartesianPosition::new(0.0, -1.0, 0.0);
        let bank = HrirBank::new(
            RATE,
            vec![
                HrirEntry::new(entry_a, front, first.clone()).unwrap(),
                HrirEntry::new(entry_b, rear, second.clone()).unwrap(),
            ],
        )
        .unwrap();
        let mut static_renderer = BinauralRenderer::new(
            RATE,
            bank,
            vec![
                StaticBinauralSource::new(ID, front, 0.75, entry_a).unwrap(),
                StaticBinauralSource::new(ID_2, rear, 1.25, entry_b).unwrap(),
            ],
        )
        .unwrap();
        let mut dynamic_renderer = DynamicBinauralRenderer::new(
            RATE,
            update_two(0, 0, first, second),
            vec![
                DynamicBinauralSource::new(ID, 0.75).unwrap(),
                DynamicBinauralSource::new(ID_2, 1.25).unwrap(),
            ],
            16,
            16,
            8,
        )
        .unwrap();
        let input_a = [0.2, -0.1, 0.7, 0.4, -0.3, 0.0, 0.9, -0.2];
        let input_b = [-0.4, 0.1, 0.3, -0.8, 0.5, 0.6, -0.1, 0.2];
        let mut static_left = Vec::new();
        let mut static_right = Vec::new();
        let mut dynamic_left = Vec::new();
        let mut dynamic_right = Vec::new();
        for range in [0..1, 1..5, 5..8] {
            let a = &input_a[range.clone()];
            let b = &input_b[range];
            let reversed_blocks = [
                BinauralSourceBlock::new(ID_2, b),
                BinauralSourceBlock::new(ID, a),
            ];
            let mut left = vec![0.0; a.len()];
            let mut right = vec![0.0; a.len()];
            static_renderer
                .render_block(&reversed_blocks, &mut left, &mut right)
                .unwrap();
            static_left.extend(left);
            static_right.extend(right);
            let mut left = vec![0.0; a.len()];
            let mut right = vec![0.0; a.len()];
            dynamic_renderer
                .render_block(&reversed_blocks, &mut left, &mut right)
                .unwrap();
            dynamic_left.extend(left);
            dynamic_right.extend(right);
        }
        assert_eq!(dynamic_left, static_left);
        assert_eq!(dynamic_right, static_right);
        assert_eq!(dynamic_renderer.remaining_tail_samples(), 3);
        let mut static_left = vec![0.0; 3];
        let mut static_right = vec![0.0; 3];
        let mut dynamic_left = vec![0.0; 3];
        let mut dynamic_right = vec![0.0; 3];
        static_renderer
            .drain_tail_block(&mut static_left, &mut static_right)
            .unwrap();
        dynamic_renderer
            .drain_tail_block(&mut dynamic_left, &mut dynamic_right)
            .unwrap();
        assert_eq!(dynamic_left, static_left);
        assert_eq!(dynamic_right, static_right);
    }

    #[test]
    fn updates_apply_on_the_next_nonempty_block_with_a_shared_linear_fade() {
        let mut renderer = renderer(pair(&[1.0, 0.0, 0.0], &[1.0, 0.0, 0.0]), 8, 4);
        let accepted = renderer
            .apply_prepared(update(0, 1, pair(&[0.0, 2.0, 0.0], &[0.0, 2.0, 0.0])))
            .unwrap();
        assert_eq!(accepted.accepted_sequence, 1);
        assert_eq!(renderer.last_applied_update(), None);
        let _ = render_dynamic(&mut renderer, &[]);
        assert_eq!(renderer.last_applied_update(), None);
        let (left, right) = render_dynamic(&mut renderer, &[1.0; 6]);
        assert_eq!(left, [0.5625, 1.125, 1.3125, 1.5, 1.5, 1.5]);
        assert_eq!(right, left);
        assert_eq!(
            renderer.last_applied_update(),
            Some(AppliedBinauralUpdate {
                sequence: 1,
                logical_start_sample: 0,
            })
        );
    }

    #[test]
    fn event_aligned_rendering_is_invariant_across_supported_block_partitions() {
        let input = (0..3_072)
            .map(|index| ((index * 31 % 127) as f64 - 63.0) / 80.0)
            .collect::<Vec<_>>();
        let patterns = [
            &[1][..],
            &[17][..],
            &[97][..],
            &[128][..],
            &[256][..],
            &[1536][..],
        ];
        let mut reference = None;
        for pattern in patterns {
            let mut renderer = DynamicBinauralRenderer::new(
                RATE,
                update(0, 0, pair(&[0.8, -0.1, 0.05], &[0.7, 0.2, -0.04])),
                vec![DynamicBinauralSource::new(ID, 0.75).unwrap()],
                8,
                1536,
                12,
            )
            .unwrap();
            let mut output = (Vec::new(), Vec::new());
            let mut position = 0;
            let mut block_index = 0;
            let mut update_submitted = false;
            while position < input.len() {
                if position == 1536 && !update_submitted {
                    let acceptance = renderer
                        .apply_prepared(update(
                            0,
                            1,
                            pair(&[0.6, 0.3, -0.1, 0.05], &[0.5, -0.2, 0.1, 0.03]),
                        ))
                        .unwrap();
                    drop(acceptance.retired_kernels);
                    update_submitted = true;
                }
                let requested = pattern[block_index % pattern.len()];
                let until_event = if position < 1536 {
                    1536 - position
                } else {
                    input.len() - position
                };
                let count = requested.min(until_event).min(input.len() - position);
                let mut left = vec![0.0; count];
                let mut right = vec![0.0; count];
                renderer
                    .render_block(
                        &[BinauralSourceBlock::new(
                            ID,
                            &input[position..position + count],
                        )],
                        &mut left,
                        &mut right,
                    )
                    .unwrap();
                output.0.extend(left);
                output.1.extend(right);
                position += count;
                block_index += 1;
            }
            assert!(update_submitted);
            assert_eq!(
                renderer.last_applied_update(),
                Some(AppliedBinauralUpdate {
                    sequence: 1,
                    logical_start_sample: 1536,
                })
            );
            if let Some((expected_left, expected_right)) = &reference {
                assert_eq!(&output.0, expected_left, "partition size {pattern:?}");
                assert_eq!(&output.1, expected_right, "partition size {pattern:?}");
            } else {
                reference = Some(output);
            }
        }
    }

    #[test]
    fn latest_waiting_update_replaces_only_the_pending_target() {
        let mut renderer = renderer(pair(&[1.0, 0.0, 0.0], &[1.0, 0.0, 0.0]), 8, 4);
        renderer
            .apply_prepared(update(0, 1, pair(&[0.0, 1.0, 0.0], &[0.0, 1.0, 0.0])))
            .unwrap();
        let _ = render_dynamic(&mut renderer, &[1.0]);
        renderer
            .apply_prepared(update(0, 2, pair(&[0.0, 0.0, 1.0], &[0.0, 0.0, 1.0])))
            .unwrap();
        let accepted = renderer
            .apply_prepared(update(0, 3, pair(&[2.0, 0.0, 0.0], &[2.0, 0.0, 0.0])))
            .unwrap();
        assert_eq!(accepted.superseded_sequence, Some(2));
        let _ = render_dynamic(&mut renderer, &[1.0; 3]);
        assert_eq!(renderer.logical_sample_count(), 4);
        assert_eq!(renderer.last_applied_update().unwrap().sequence, 1);
        let _ = render_dynamic(&mut renderer, &[1.0]);
        assert_eq!(
            renderer.last_applied_update(),
            Some(AppliedBinauralUpdate {
                sequence: 3,
                logical_start_sample: 4,
            })
        );
    }

    #[test]
    fn long_short_filter_transitions_preserve_causal_histories_and_delays() {
        let long = HrirPair::new_with_delays(
            RATE,
            vec![0.0, 0.0, 0.5, -0.2, 0.1, 0.04, -0.02, 0.01],
            vec![0.0, -0.1, 0.2, 0.1, 0.05, -0.03, 0.01, 0.02],
            [2, 1],
        )
        .unwrap();
        let short =
            HrirPair::new_with_delays(RATE, vec![0.3, 0.2, -0.1], vec![-0.2, 0.15, 0.05], [0, 1])
                .unwrap();
        let mut renderer = renderer(long.clone(), 16, 4);
        let mut input = vec![0.5, -0.25, 0.75];
        let _ = render_dynamic(&mut renderer, &input);
        let acceptance = renderer
            .apply_prepared(update(0, 1, short.clone()))
            .unwrap();
        drop(acceptance.retired_kernels);
        let next = [-0.3, 0.1, 0.6, -0.2];
        let (left, right) = render_dynamic(&mut renderer, &next);
        input.extend_from_slice(&next);
        for offset in 0..next.len() {
            let sample_index = 3 + offset;
            let alpha = (offset + 1) as f64 / 4.0;
            let expected_left = convolve_history(&input, long.left_taps(), sample_index)
                * (1.0 - alpha)
                + convolve_history(&input, short.left_taps(), sample_index) * alpha;
            let expected_right = convolve_history(&input, long.right_taps(), sample_index)
                * (1.0 - alpha)
                + convolve_history(&input, short.right_taps(), sample_index) * alpha;
            assert_eq!(left[offset], expected_left);
            assert_eq!(right[offset], expected_right);
        }
        assert_eq!(renderer.remaining_tail_samples(), 2);

        let acceptance = renderer.apply_prepared(update(0, 2, long.clone())).unwrap();
        drop(acceptance.retired_kernels);
        let final_input = [0.2, 0.3, 0.1, 0.7];
        let (left, right) = render_dynamic(&mut renderer, &final_input);
        let start = input.len();
        input.extend_from_slice(&final_input);
        for offset in 0..final_input.len() {
            let sample_index = start + offset;
            let alpha = (offset + 1) as f64 / 4.0;
            let expected_left = convolve_history(&input, short.left_taps(), sample_index)
                * (1.0 - alpha)
                + convolve_history(&input, long.left_taps(), sample_index) * alpha;
            let expected_right = convolve_history(&input, short.right_taps(), sample_index)
                * (1.0 - alpha)
                + convolve_history(&input, long.right_taps(), sample_index) * alpha;
            assert_eq!(left[offset], expected_left);
            assert_eq!(right[offset], expected_right);
        }
        assert_eq!(renderer.remaining_tail_samples(), 7);
    }

    #[test]
    fn update_validation_is_transactional_and_reset_rejects_old_epochs() {
        let mut renderer = renderer(pair(&[1.0, 0.0], &[1.0, 0.0]), 8, 3);
        let before = renderer.latest_accepted_sequence();
        let wrong_resource = PreparedBinauralUpdate::new(
            BinauralResourceIdentity::new([0xa5; 32]),
            0,
            1,
            vec![PreparedBinauralKernel::new(ID, pair(&[2.0], &[2.0]))],
        )
        .unwrap();
        let failure = renderer.apply_prepared(wrong_resource).unwrap_err();
        assert_eq!(failure.error, RenderError::BinauralResourceIdentityMismatch);
        assert_eq!(failure.update.sequence(), 1);
        assert_eq!(renderer.latest_accepted_sequence(), before);
        drop(
            renderer
                .reset_to(update(1, 0, pair(&[1.0, 0.0], &[1.0, 0.0])))
                .unwrap(),
        );
        assert_eq!(renderer.stream_epoch(), 1);
        assert_eq!(renderer.logical_sample_count(), 0);
        assert_eq!(renderer.remaining_tail_samples(), 0);
        assert_eq!(renderer.last_applied_update(), None);
        let failure = renderer
            .apply_prepared(update(0, 9, pair(&[3.0], &[3.0])))
            .unwrap_err();
        assert_eq!(
            failure.error,
            RenderError::BinauralUpdateEpochMismatch {
                expected: 1,
                actual: 0,
            }
        );
        assert_eq!(failure.update.sequence(), 9);
        assert_eq!(renderer.latest_accepted_sequence(), 0);
        let _ = render_dynamic(&mut renderer, &[0.0]);
        assert_eq!(
            renderer.last_applied_update(),
            Some(AppliedBinauralUpdate {
                sequence: 0,
                logical_start_sample: 0,
            })
        );
    }

    #[test]
    fn dynamic_history_capacity_does_not_extend_a_short_filter_tail() {
        let mut renderer = renderer(pair(&[1.0, 0.0], &[1.0, 0.0]), 64, 8);
        let _ = render_dynamic(&mut renderer, &[1.0]);
        assert_eq!(renderer.remaining_tail_samples(), 1);
        assert_eq!(
            renderer.hrir_history_storage_bytes(),
            63 * std::mem::size_of::<f64>()
        );
        let mut left = [0.0];
        let mut right = [0.0];
        renderer.drain_tail_block(&mut left, &mut right).unwrap();
        assert_eq!(renderer.remaining_tail_samples(), 0);
        assert_eq!(
            renderer.drain_tail_block(&mut [], &mut []).unwrap_err(),
            RenderError::BinauralAlreadyFinished
        );
    }

    #[test]
    fn drain_finishes_an_active_fade_but_does_not_start_a_waiting_target() {
        let mut renderer = renderer(
            pair(
                &[0.2, 0.1, 0.05, -0.02, 0.01, 0.005],
                &[0.1, -0.05, 0.03, 0.02, 0.01, 0.0],
            ),
            16,
            4,
        );
        renderer
            .apply_prepared(update(
                0,
                1,
                pair(&[-0.1, 0.25, 0.1, 0.05], &[0.05, 0.2, -0.1, 0.02]),
            ))
            .unwrap();
        let _ = render_dynamic(&mut renderer, &[1.0]);
        assert_eq!(renderer.pending_sequence(), None);
        let acceptance = renderer
            .apply_prepared(update(0, 2, pair(&[0.3, 0.0], &[0.0, 0.3])))
            .unwrap();
        drop(acceptance.retired_kernels);
        assert_eq!(renderer.pending_sequence(), Some(2));
        assert_eq!(renderer.remaining_tail_samples(), 5);
        let mut left = vec![0.0; 5];
        let mut right = vec![0.0; 5];
        renderer.drain_tail_block(&mut left, &mut right).unwrap();
        assert_eq!(renderer.remaining_tail_samples(), 0);
        assert_eq!(renderer.pending_sequence(), Some(2));
        assert_eq!(
            renderer.last_applied_update(),
            Some(AppliedBinauralUpdate {
                sequence: 1,
                logical_start_sample: 0,
            })
        );
        assert!(left.iter().chain(&right).all(|sample| sample.is_finite()));
    }

    #[test]
    fn reconstruction_tail_continues_active_fade_but_keeps_new_target_pending() {
        let mut renderer = DynamicBinauralRenderer::new(
            RATE,
            update(0, 0, pair(&[1.0, 0.2, 0.05], &[0.5, -0.1, 0.03])),
            vec![DynamicBinauralSource::new(ID, 0.75).unwrap()],
            8,
            256,
            240,
        )
        .unwrap();
        renderer
            .apply_prepared(update(0, 1, pair(&[0.2, 0.4, 0.1], &[0.3, 0.1, -0.05])))
            .unwrap();
        let _ = render_dynamic(&mut renderer, &[0.2; 128]);
        assert_eq!(renderer.last_applied_update().unwrap().sequence, 1);
        renderer
            .apply_prepared(update(0, 2, pair(&[-0.2, 0.5], &[0.6, -0.1])))
            .unwrap();
        assert_eq!(renderer.pending_sequence(), Some(2));
        renderer.begin_drain().unwrap();

        let tail_input = [0.1; 128];
        let mut left = vec![0.0; tail_input.len()];
        let mut right = vec![0.0; tail_input.len()];
        renderer
            .render_reconstruction_tail_block(
                &[BinauralSourceBlock::new(ID, &tail_input)],
                &mut left,
                &mut right,
            )
            .unwrap();
        assert_eq!(renderer.pending_sequence(), Some(2));
        assert_eq!(renderer.last_applied_update().unwrap().sequence, 1);
        assert!(left.iter().chain(&right).all(|sample| sample.is_finite()));
        let remaining = renderer.remaining_tail_samples();
        let mut left = vec![0.0; remaining];
        let mut right = vec![0.0; remaining];
        if remaining > 0 {
            renderer.drain_tail_block(&mut left, &mut right).unwrap();
        }
        assert_eq!(renderer.pending_sequence(), Some(2));
        assert_eq!(renderer.last_applied_update().unwrap().sequence, 1);
    }

    #[test]
    fn epoch_overflow_fails_closed_without_wrapping() {
        let initial = PreparedBinauralUpdate::new(
            RESOURCE,
            u64::MAX,
            0,
            vec![PreparedBinauralKernel::new(ID, pair(&[1.0], &[1.0]))],
        )
        .unwrap();
        let mut renderer = DynamicBinauralRenderer::new(
            RATE,
            initial,
            vec![DynamicBinauralSource::new(ID, 1.0).unwrap()],
            8,
            16,
            1,
        )
        .unwrap();
        let failure = renderer
            .reset_to(
                PreparedBinauralUpdate::new(
                    RESOURCE,
                    0,
                    0,
                    vec![PreparedBinauralKernel::new(ID, pair(&[1.0], &[1.0]))],
                )
                .unwrap(),
            )
            .unwrap_err();
        assert_eq!(failure.error, RenderError::BinauralStreamEpochOverflow);
        assert_eq!(failure.update.stream_epoch(), 0);
        let mut left = [0.0];
        let mut right = [0.0];
        assert_eq!(
            renderer
                .render_block(
                    &[BinauralSourceBlock::new(ID, &[0.0])],
                    &mut left,
                    &mut right
                )
                .unwrap_err(),
            RenderError::BinauralRequiresReset
        );
    }

    #[test]
    fn numeric_failure_can_recover_through_epoch_reset() {
        let mut renderer = renderer(pair(&[1.0e308], &[1.0e308]), 8, 4);
        let mut left = [0.0];
        let mut right = [0.0];
        assert_eq!(
            renderer
                .render_block(
                    &[BinauralSourceBlock::new(ID, &[1.0e308])],
                    &mut left,
                    &mut right,
                )
                .unwrap_err(),
            RenderError::NonFiniteOutput {
                channel: crate::OutputChannel::Left,
                sample_index: 0,
            }
        );
        assert_eq!(left, [0.0]);
        assert_eq!(right, [0.0]);
        drop(
            renderer
                .reset_to(update(1, 0, pair(&[1.0], &[1.0])))
                .unwrap(),
        );
        let (left, right) = render_dynamic(&mut renderer, &[1.0]);
        assert_eq!(left, [0.75]);
        assert_eq!(right, [0.75]);
        assert_eq!(
            renderer.last_applied_update(),
            Some(AppliedBinauralUpdate {
                sequence: 0,
                logical_start_sample: 0,
            })
        );
    }
}
