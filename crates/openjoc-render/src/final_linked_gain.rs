//! Causal common gain for the final speaker-domain output.
//!
//! This stage intentionally lives in the shared renderer crate.  The CLI and
//! headless API both feed it the same combined speaker-channel planes, so the
//! state machine cannot drift between adapters.

use std::fmt;

/// The admitted 48-kHz processing block used by the public speaker adapters.
pub const FINAL_LINKED_GAIN_BLOCK_SAMPLES: usize = 32;
const MAX_WINDOW_SAMPLES: usize = 40;
const SCALE_BRANCH_CONSTANT: f32 = 0.84140015;
const SCALE_MULTIPLIER: f32 = 1.188495;
const OUTPUT_SCALE: f32 = 1.0 / 256.0;

/// Availability of a clean 48-kHz parameter family.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FinalLinkedGainAvailability {
    /// The complete family is admitted by the current public adapter.
    Ready,
    /// Static parameters exist, but the current public adapter does not admit
    /// this processing size for the new fidelity path.
    Constrained,
    /// No authorized parameter family exists for this configuration.
    Withheld,
}

/// Failures at the shared FinalLinkedGain boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FinalLinkedGainError {
    /// Only the authorized 48-kHz family is available.
    UnsupportedSampleRate { sample_rate: u32 },
    /// The block is unavailable or constrained by the clean implementation
    /// boundary.  Constrained rows are deliberately not guessed.
    UnsupportedBlockLength {
        sample_rate: u32,
        block_length: usize,
        availability: FinalLinkedGainAvailability,
    },
    /// A channel plane was not the same length as its peers.
    ChannelLengthMismatch {
        channel: usize,
        expected: usize,
        actual: usize,
    },
    /// The render boundary supplied a non-finite or non-binary32 sample.
    NonFiniteSample { channel: usize, sample: usize },
    /// The input did not contain complete processing blocks.
    IncompleteBlock {
        sample_count: usize,
        block_length: usize,
    },
    /// The channel set changed without rebuilding the state owner.
    ChannelSetMismatch { expected: usize, actual: usize },
}

impl fmt::Display for FinalLinkedGainError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedSampleRate { sample_rate } => {
                write!(
                    formatter,
                    "FinalLinkedGain only admits 48 kHz, received {sample_rate} Hz"
                )
            }
            Self::UnsupportedBlockLength {
                sample_rate,
                block_length,
                availability,
            } => write!(
                formatter,
                "FinalLinkedGain block length {block_length} at {sample_rate} Hz is {availability:?}"
            ),
            Self::ChannelLengthMismatch {
                channel,
                expected,
                actual,
            } => write!(
                formatter,
                "FinalLinkedGain channel {channel} has {actual} samples; expected {expected}"
            ),
            Self::NonFiniteSample { channel, sample } => write!(
                formatter,
                "FinalLinkedGain received a non-finite sample at channel {channel}, index {sample}"
            ),
            Self::IncompleteBlock {
                sample_count,
                block_length,
            } => write!(
                formatter,
                "FinalLinkedGain received {sample_count} samples, not complete {block_length}-sample blocks"
            ),
            Self::ChannelSetMismatch { expected, actual } => write!(
                formatter,
                "FinalLinkedGain channel set has {actual} entries; expected {expected}"
            ),
        }
    }
}

impl std::error::Error for FinalLinkedGainError {}

/// Returns the clean availability classification for a sample-rate/block pair.
#[must_use]
pub const fn final_linked_gain_availability(
    sample_rate: u32,
    block_length: usize,
) -> FinalLinkedGainAvailability {
    if sample_rate != 48_000 {
        return FinalLinkedGainAvailability::Withheld;
    }
    match block_length {
        32 | 40 => FinalLinkedGainAvailability::Ready,
        64 | 128 => FinalLinkedGainAvailability::Constrained,
        _ => FinalLinkedGainAvailability::Withheld,
    }
}

#[derive(Clone, Copy, Debug)]
struct Coefficients {
    c0: f32,
    c1: f32,
    c2: f32,
    c3: f32,
    c4: f32,
}

#[allow(clippy::excessive_precision)]
impl Coefficients {
    const fn for_block_length(block_length: usize) -> Option<Self> {
        match block_length {
            32 => Some(Self {
                c0: 0.988525390625,
                c1: 0.011474609375,
                c2: 0.997100830078125,
                c3: 0.002899169921875,
                c4: 0.911712646484375,
            }),
            40 => Some(Self {
                c0: 0.98565673828125,
                c1: 0.01434326171875,
                c2: 0.99639892578125,
                c3: 0.00360107421875,
                c4: 0.890899658203125,
            }),
            _ => None,
        }
    }
}

/// The shared stateful FinalLinkedGain engine.
#[derive(Clone, Debug)]
pub struct FinalLinkedGain {
    sample_rate: u32,
    block_length: usize,
    ring_depth: usize,
    coefficients: Coefficients,
    window: [f32; MAX_WINDOW_SAMPLES],
    active_channels: Vec<bool>,
    audio_history: Vec<Vec<f32>>,
    previous_audio_history: Vec<Vec<f32>>,
    previous_peak: f32,
    previous_gain: f32,
    scale: f32,
    applied_gain: f32,
    smoothing: f32,
    ring: [f32; 3],
    cursor: usize,
}

impl FinalLinkedGain {
    /// Creates one independent engine for an admitted 48-kHz processing row.
    pub fn new(
        sample_rate: u32,
        block_length: usize,
        active_channels: &[bool],
    ) -> Result<Self, FinalLinkedGainError> {
        if sample_rate != 48_000 {
            return Err(FinalLinkedGainError::UnsupportedSampleRate { sample_rate });
        }
        let availability = final_linked_gain_availability(sample_rate, block_length);
        let Some(coefficients) = Coefficients::for_block_length(block_length) else {
            return Err(FinalLinkedGainError::UnsupportedBlockLength {
                sample_rate,
                block_length,
                availability,
            });
        };
        if availability != FinalLinkedGainAvailability::Ready {
            return Err(FinalLinkedGainError::UnsupportedBlockLength {
                sample_rate,
                block_length,
                availability,
            });
        }
        let mut engine = Self {
            sample_rate,
            block_length,
            ring_depth: if block_length == 32 { 3 } else { 2 },
            coefficients,
            window: [0.0; MAX_WINDOW_SAMPLES],
            active_channels: active_channels.to_vec(),
            audio_history: active_channels
                .iter()
                .map(|_| vec![0.0; block_length])
                .collect(),
            previous_audio_history: active_channels
                .iter()
                .map(|_| vec![0.0; block_length])
                .collect(),
            previous_peak: 0.0,
            previous_gain: 1.0,
            scale: 1.0,
            applied_gain: 1.0,
            smoothing: 1.0,
            ring: [1.0; 3],
            cursor: 0,
        };
        engine.window = generate_window(block_length);
        Ok(engine)
    }

    /// Returns the selected semantic block length.
    #[must_use]
    pub const fn block_length(&self) -> usize {
        self.block_length
    }

    /// Returns the selected sample rate.
    #[must_use]
    pub const fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Reconfigures the active channel set and resets all streaming state.
    pub fn reconfigure(
        &mut self,
        sample_rate: u32,
        block_length: usize,
        active_channels: &[bool],
    ) -> Result<(), FinalLinkedGainError> {
        if self.sample_rate == sample_rate
            && self.block_length == block_length
            && self.active_channels == active_channels
        {
            return Ok(());
        }
        *self = Self::new(sample_rate, block_length, active_channels)?;
        Ok(())
    }

    /// Resets peak, gain, ring, smoothing, and audio history state.
    pub fn reset(&mut self) {
        self.previous_peak = 0.0;
        self.previous_gain = 1.0;
        self.scale = 1.0;
        self.applied_gain = 1.0;
        self.smoothing = 1.0;
        self.ring = [1.0; 3];
        self.cursor = 0;
        for history in &mut self.audio_history {
            history.fill(0.0);
        }
        for history in &mut self.previous_audio_history {
            history.fill(0.0);
        }
    }

    /// Applies the common state machine to complete channel-major PCM blocks.
    ///
    /// The caller retains ownership of the channel planes.  Samples are
    /// narrowed to binary32 at the stage boundary, matching the clean state
    /// contract, and are written back as finite `f64` values for the existing
    /// renderer container APIs.
    pub fn process(&mut self, channels: &mut [Vec<f64>]) -> Result<(), FinalLinkedGainError> {
        if channels.len() != self.active_channels.len() {
            return Err(FinalLinkedGainError::ChannelSetMismatch {
                expected: self.active_channels.len(),
                actual: channels.len(),
            });
        }
        let sample_count = channels.first().map_or(0, Vec::len);
        for (channel_index, channel) in channels.iter().enumerate() {
            if channel.len() != sample_count {
                return Err(FinalLinkedGainError::ChannelLengthMismatch {
                    channel: channel_index,
                    expected: sample_count,
                    actual: channel.len(),
                });
            }
            for (sample_index, &sample) in channel.iter().enumerate() {
                if !sample.is_finite() || !(sample as f32).is_finite() {
                    return Err(FinalLinkedGainError::NonFiniteSample {
                        channel: channel_index,
                        sample: sample_index,
                    });
                }
            }
        }
        if !self.active_channels.iter().any(|active| *active) {
            return Ok(());
        }
        if sample_count % self.block_length != 0 {
            return Err(FinalLinkedGainError::IncompleteBlock {
                sample_count,
                block_length: self.block_length,
            });
        }
        for block_start in (0..sample_count).step_by(self.block_length) {
            self.process_block(channels, block_start);
        }
        Ok(())
    }

    /// Emits the one-block causal tail by processing an all-zero input block.
    ///
    /// This is a stream drain operation, not a reset.  The returned block is
    /// the final captured audio history; the engine remains resettable and its
    /// zero input is captured as the next history block.
    pub fn drain(&mut self) -> Result<Vec<Vec<f64>>, FinalLinkedGainError> {
        let mut channels = self
            .active_channels
            .iter()
            .map(|_| vec![0.0; self.block_length])
            .collect::<Vec<_>>();
        self.process(&mut channels)?;
        Ok(channels)
    }

    fn process_block(&mut self, channels: &mut [Vec<f64>], block_start: usize) {
        let mut peak = 0.0_f32;
        for (channel_index, active) in self.active_channels.iter().copied().enumerate() {
            if active {
                for &sample in
                    &channels[channel_index][block_start..block_start + self.block_length]
                {
                    peak = peak.max(sample as f32).max(-(sample as f32));
                }
            }
        }

        let peak_hold = self.previous_peak.max(peak);
        let product = f32_mul(peak_hold, self.scale);
        let candidate = if product <= 1.0 {
            1.0
        } else {
            f64_to_f32(1.0 / f64::from(product))
        };
        let target = f32_mul(self.scale, candidate);

        let mut history_min = candidate;
        for &value in &self.ring[..self.ring_depth] {
            history_min = history_min.min(value);
        }
        self.ring[self.cursor] = candidate;
        self.cursor = (self.cursor + 1) % self.ring_depth;

        let old_gain = self.applied_gain;
        let (q, next_smoothing, next_gain) = if old_gain <= target {
            let q = f32_add(
                f32_sub(history_min, f32_mul(self.coefficients.c4, history_min)),
                f32_mul(self.coefficients.c4, self.smoothing),
            );
            let rising = f32_mul(self.scale, q);
            (q, q, target.min(old_gain.max(rising)))
        } else {
            (candidate, candidate, target)
        };

        let next_scale = if q < SCALE_BRANCH_CONSTANT {
            let scaled_q = f32_mul(
                f32_mul(f32_mul(self.scale, q), SCALE_MULTIPLIER),
                self.coefficients.c1,
            );
            f32_add(scaled_q, f32_mul(self.scale, self.coefficients.c0))
        } else {
            f32_add(
                f32_mul(self.scale, self.coefficients.c2),
                self.coefficients.c3,
            )
        };

        self.previous_peak = peak;
        self.previous_gain = old_gain;
        self.scale = next_scale;
        self.applied_gain = next_gain;
        self.smoothing = next_smoothing;

        let old_norm = f32_mul(old_gain, OUTPUT_SCALE);
        let new_norm = f32_mul(next_gain, OUTPUT_SCALE);
        for (channel_index, active) in self.active_channels.iter().copied().enumerate() {
            if !active {
                continue;
            }
            {
                let previous_history = &self.previous_audio_history[channel_index];
                let history = &mut self.audio_history[channel_index];
                // Capture the current block before writing the delayed output;
                // the previous-history plane supplies Xhist for this write.
                for (sample_index, source) in channels[channel_index]
                    [block_start..block_start + self.block_length]
                    .iter()
                    .enumerate()
                {
                    history[sample_index] = *source as f32;
                }
                for (sample_index, destination) in channels[channel_index]
                    [block_start..block_start + self.block_length]
                    .iter_mut()
                    .enumerate()
                {
                    let old_term = f32_mul(
                        f32_mul(
                            self.window[self.block_length - 1 - sample_index],
                            previous_history[sample_index],
                        ),
                        old_norm,
                    );
                    let new_term = f32_mul(
                        f32_mul(previous_history[sample_index], new_norm),
                        self.window[sample_index],
                    );
                    *destination = f64::from(f32_mul(f32_add(old_term, new_term), 256.0));
                }
            }
            self.previous_audio_history[channel_index]
                .copy_from_slice(&self.audio_history[channel_index]);
        }
    }
}

fn generate_window(block_length: usize) -> [f32; MAX_WINDOW_SAMPLES] {
    let mut window = [0.0; MAX_WINDOW_SAMPLES];
    for (index, value) in window.iter_mut().take(block_length).enumerate() {
        let angle = std::f64::consts::PI * (index as f64 + 0.5) / (2.0 * block_length as f64);
        let quantized = (32_768.0 * angle.sin().powi(2)).round();
        *value = (quantized as f32) / 32_768.0;
    }
    window
}

#[inline(never)]
fn f32_add(left: f32, right: f32) -> f32 {
    left + right
}

#[inline(never)]
fn f32_sub(left: f32, right: f32) -> f32 {
    left - right
}

#[inline(never)]
fn f32_mul(left: f32, right: f32) -> f32 {
    left * right
}

#[inline(never)]
fn f64_to_f32(value: f64) -> f32 {
    value as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn constant_block(length: usize, value: f64) -> Vec<Vec<f64>> {
        vec![vec![value; length]]
    }

    fn assert_close(actual: f64, expected: f64) {
        assert_eq!(
            actual.to_bits(),
            expected.to_bits(),
            "{actual} != {expected}"
        );
    }

    #[test]
    fn availability_keeps_constrained_rows_closed() {
        assert_eq!(
            final_linked_gain_availability(48_000, 32),
            FinalLinkedGainAvailability::Ready
        );
        assert_eq!(
            final_linked_gain_availability(48_000, 40),
            FinalLinkedGainAvailability::Ready
        );
        assert_eq!(
            final_linked_gain_availability(48_000, 64),
            FinalLinkedGainAvailability::Constrained
        );
        assert_eq!(
            final_linked_gain_availability(48_000, 128),
            FinalLinkedGainAvailability::Constrained
        );
        for block_length in [64, 128] {
            assert!(matches!(
                FinalLinkedGain::new(48_000, block_length, &[true]),
                Err(FinalLinkedGainError::UnsupportedBlockLength {
                    availability: FinalLinkedGainAvailability::Constrained,
                    ..
                })
            ));
        }
        assert!(matches!(
            FinalLinkedGain::new(48_000, 256, &[true]),
            Err(FinalLinkedGainError::UnsupportedBlockLength {
                availability: FinalLinkedGainAvailability::Withheld,
                ..
            })
        ));
    }

    #[test]
    fn window_generator_matches_authorized_endpoints() {
        let thirty_two = FinalLinkedGain::new(48_000, 32, &[true]).unwrap();
        assert_close(f64::from(thirty_two.window[0]), 0.0006103515625);
        assert_close(f64::from(thirty_two.window[16]), 0.5245361328125);
        assert_close(f64::from(thirty_two.window[31]), 0.9993896484375);
        let forty = FinalLinkedGain::new(48_000, 40, &[true]).unwrap();
        assert_close(f64::from(forty.window[0]), 0.000396728515625);
        assert_close(f64::from(forty.window[20]), 0.519622802734375);
        assert_close(f64::from(forty.window[39]), 0.999603271484375);
    }

    #[test]
    fn clean_32_fixtures_cover_threshold_attack_and_history() {
        let mut below = FinalLinkedGain::new(48_000, 32, &[true]).unwrap();
        let mut input = constant_block(32, 0.5);
        below.process(&mut input).unwrap();
        assert_eq!(below.applied_gain, 1.0);
        assert_eq!(input[0][0], 0.0);

        let mut exact = FinalLinkedGain::new(48_000, 32, &[true]).unwrap();
        let mut input = constant_block(32, 1.0);
        exact.process(&mut input).unwrap();
        assert_eq!(exact.applied_gain, 1.0);

        let mut above = FinalLinkedGain::new(48_000, 32, &[true]).unwrap();
        let mut input = constant_block(32, 2.0);
        above.process(&mut input).unwrap();
        assert_eq!(above.applied_gain, 0.5);
        assert_eq!(above.smoothing, 0.5);
        assert_close(f64::from(above.scale), 0.9953441619873047);

        let mut step = FinalLinkedGain::new(48_000, 32, &[true]).unwrap();
        let mut first = constant_block(32, 0.5);
        step.process(&mut first).unwrap();
        let mut second = constant_block(32, 2.0);
        step.process(&mut second).unwrap();
        assert_close(second[0][0], 0.499847412109375);
        assert_close(second[0][31], 0.250152587890625);
    }

    #[test]
    fn clean_32_state_trace_covers_recovery_repeat_peak_and_reset() {
        let mut engine = FinalLinkedGain::new(48_000, 32, &[true]).unwrap();
        let values = [0.5, 2.0, 2.0, 2.0, 0.5, 0.5, 0.5, 0.5, 0.5];
        let mut outputs = Vec::new();
        for value in values {
            let mut block = constant_block(32, value);
            engine.process(&mut block).unwrap();
            outputs.push((engine.applied_gain, engine.scale, engine.smoothing));
        }
        assert_eq!(outputs[1].0, 0.5);
        assert_close(f64::from(outputs[2].0), 0.5);
        assert_close(f64::from(outputs[8].0), 0.5307021737098694);
        assert_close(f64::from(outputs[8].1), 0.9642758369445801);
        assert_close(f64::from(outputs[8].2), 0.5481625199317932);
        engine.reset();
        assert_eq!(engine.previous_peak, 0.0);
        assert_eq!(engine.previous_gain, 1.0);
        assert_eq!(engine.scale, 1.0);
        assert_eq!(engine.applied_gain, 1.0);
        assert_eq!(engine.smoothing, 1.0);
        assert_eq!(engine.ring, [1.0; 3]);
        assert_eq!(engine.cursor, 0);
        assert!(
            engine
                .audio_history
                .iter()
                .all(|history| history.iter().all(|&v| v == 0.0))
        );
    }

    #[test]
    fn clean_impulse_fixture_uses_the_previous_audio_history() {
        let mut engine = FinalLinkedGain::new(48_000, 32, &[true]).unwrap();
        let mut first = constant_block(32, 0.25);
        engine.process(&mut first).unwrap();
        let mut impulse = constant_block(32, 0.25);
        impulse[0][0] = 2.0;
        engine.process(&mut impulse).unwrap();
        assert_close(impulse[0][0], 0.2499237060546875);
        let mut recovery = constant_block(32, 0.25);
        engine.process(&mut recovery).unwrap();
        assert_close(recovery[0][0], 1.0);
    }

    #[test]
    fn clean_40_family_uses_its_own_coefficients_and_window() {
        let mut engine = FinalLinkedGain::new(48_000, 40, &[true]).unwrap();
        let mut first = constant_block(40, 0.5);
        engine.process(&mut first).unwrap();
        let mut second = constant_block(40, 2.0);
        engine.process(&mut second).unwrap();
        assert_eq!(engine.ring_depth, 2);
        assert_eq!(engine.applied_gain, 0.5);
        assert_close(f64::from(engine.scale), 0.9941802024841309);
        assert_close(second[0][0], 0.49990081787109375);
        assert_close(second[0][39], 0.25009918212890625);
        for value in [2.0, 2.0, 0.5, 0.5, 0.5, 0.5] {
            let mut block = constant_block(40, value);
            engine.process(&mut block).unwrap();
        }
        assert_close(f64::from(engine.applied_gain), 0.5370312929153442);
        assert_close(f64::from(engine.scale), 0.9609397649765015);
    }

    #[test]
    fn linking_lfe_and_permutation_share_one_envelope() {
        let mut engine = FinalLinkedGain::new(48_000, 32, &[true, true]).unwrap();
        let mut block = vec![vec![0.25; 32], vec![2.0; 32]];
        engine.process(&mut block).unwrap();
        assert_eq!(engine.applied_gain, 0.5);
        let mut next = vec![vec![0.25; 32], vec![2.0; 32]];
        engine.process(&mut next).unwrap();
        let mut low = FinalLinkedGain::new(48_000, 32, &[true, true]).unwrap();
        let mut low_block = vec![vec![2.0; 32], vec![0.25; 32]];
        low.process(&mut low_block).unwrap();
        let mut low_next = vec![vec![2.0; 32], vec![0.25; 32]];
        low.process(&mut low_next).unwrap();
        assert_eq!(low.applied_gain, engine.applied_gain);
        assert_eq!(low_next[0], next[1]);
        assert!(low_next[1].iter().all(|&sample| sample == 0.125));
    }

    #[test]
    fn drain_emits_history_without_lookahead_or_hard_clipping() {
        let mut engine = FinalLinkedGain::new(48_000, 32, &[true]).unwrap();
        let mut input = constant_block(32, 2.0);
        engine.process(&mut input).unwrap();
        assert!(input[0].iter().all(|&sample| sample == 0.0));
        let tail = engine.drain().unwrap();
        assert!(tail[0][16] > 0.9 && tail[0][16] < 1.1);
        assert!(tail[0].iter().all(|&sample| sample == 1.0));
    }

    #[test]
    fn incomplete_blocks_are_fail_closed() {
        let mut engine = FinalLinkedGain::new(48_000, 32, &[true]).unwrap();
        let mut input = constant_block(31, 0.5);
        assert!(matches!(
            engine.process(&mut input),
            Err(FinalLinkedGainError::IncompleteBlock { .. })
        ));
    }
}
