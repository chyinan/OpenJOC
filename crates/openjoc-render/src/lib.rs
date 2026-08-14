//! Explicit-scene speaker rendering for OpenJOC.
//!
//! This crate deliberately accepts only caller-supplied [`ExplicitSpatialSource`]
//! values. It has no dependency on the decoder or metadata scene crates, so a
//! `ReconstructionBasis` row cannot be silently promoted to an authored object
//! source while [`SemanticBindingState`](https://docs.rs/openjoc-scene) remains
//! unresolved.
//!
//! J5R1 implements a front-horizontal, equal-power FL/FR panner. J5R2 adds a
//! separate explicit-scene two-dimensional speaker-layout renderer using
//! checked public VBAP-style pair mathematics. J5R3 adds sample-accurate,
//! absolute-timeline azimuth and gain trajectories on top of both renderers.
//! J5R4 adds explicit caller-declared three-dimensional speaker topology and
//! checked 3x3 VBAP triplet gains. Elevation, distance, room acoustics,
//! occlusion, HRTF, and JOC semantic binding remain explicit non-features of
//! this foundation.

use std::fmt;

/// The fixed output channel order for the J5R1 speaker renderer.
pub const STEREO_CHANNEL_ORDER: [&str; 2] = ["FL", "FR"];

/// An opaque caller-provided identity for one explicit spatial source.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SourceId(u64);

impl SourceId {
    /// Creates an opaque source identity from a caller-owned value.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the caller-owned identity value.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Cartesian position relative to the listener.
///
/// The initial renderer uses `x` and `y` for front-horizontal azimuth only:
/// `+X` is right, `-X` is left, `+Y` is front, `-Y` is rear, and `+Z` is up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CartesianPosition {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl CartesianPosition {
    /// Creates a Cartesian position without silently normalizing or clamping it.
    #[must_use]
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }
}

/// One borrowed mono PCM block with explicit caller-supplied spatial semantics.
#[derive(Clone, Copy, Debug)]
pub struct ExplicitSpatialSource<'a> {
    id: SourceId,
    samples: &'a [f64],
    position: CartesianPosition,
    gain: f64,
}

impl<'a> ExplicitSpatialSource<'a> {
    /// Creates a source after validating its position, gain, and PCM block.
    ///
    /// The samples are borrowed; the renderer does not retain or copy them.
    pub fn new(
        id: SourceId,
        samples: &'a [f64],
        position: CartesianPosition,
        gain: f64,
    ) -> Result<Self, RenderError> {
        validate_position(position)?;
        validate_gain(gain)?;
        if let Some(sample_index) = samples.iter().position(|sample| !sample.is_finite()) {
            return Err(RenderError::NonFiniteSourceSample { id, sample_index });
        }
        Ok(Self {
            id,
            samples,
            position,
            gain,
        })
    }

    /// Returns the opaque caller-provided source identity.
    #[must_use]
    pub const fn id(self) -> SourceId {
        self.id
    }

    /// Returns the borrowed mono PCM block.
    #[must_use]
    pub const fn samples(self) -> &'a [f64] {
        self.samples
    }

    /// Returns the explicit Cartesian position.
    #[must_use]
    pub const fn position(self) -> CartesianPosition {
        self.position
    }

    /// Returns the explicit linear source gain.
    #[must_use]
    pub const fn gain(self) -> f64 {
        self.gain
    }
}

/// A borrowed explicit scene composed of caller-supplied source blocks.
#[derive(Clone, Copy, Debug)]
pub struct ExplicitSpatialScene<'a> {
    sample_rate_hz: u32,
    sources: &'a [ExplicitSpatialSource<'a>],
}

impl<'a> ExplicitSpatialScene<'a> {
    /// Creates a scene with an explicit sample rate and source identity set.
    pub fn new(
        sample_rate_hz: u32,
        sources: &'a [ExplicitSpatialSource<'a>],
    ) -> Result<Self, RenderError> {
        if sample_rate_hz == 0 {
            return Err(RenderError::InvalidSampleRate);
        }
        for (index, source) in sources.iter().enumerate() {
            if sources[index + 1..]
                .iter()
                .any(|other| other.id == source.id)
            {
                return Err(RenderError::DuplicateSourceId { id: source.id });
            }
        }
        Ok(Self {
            sample_rate_hz,
            sources,
        })
    }

    /// Returns the scene sample rate.
    #[must_use]
    pub const fn sample_rate_hz(self) -> u32 {
        self.sample_rate_hz
    }

    /// Returns the borrowed explicit source blocks.
    #[must_use]
    pub const fn sources(self) -> &'a [ExplicitSpatialSource<'a>] {
        self.sources
    }
}

/// Equal-power gains for the fixed FL/FR output order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StereoGains {
    pub left: f64,
    pub right: f64,
}

/// A validated spatial state used by a sample-accurate two-dimensional
/// trajectory.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpatialState2d {
    position: CartesianPosition,
    gain: f64,
}

impl SpatialState2d {
    /// Creates a finite directional state with a finite linear gain.
    pub fn new(position: CartesianPosition, gain: f64) -> Result<Self, RenderError> {
        validate_position(position)?;
        validate_gain(gain)?;
        if horizontal_magnitude_squared(position) <= 0.0 {
            return Err(RenderError::UndefinedHorizontalDirection);
        }
        Ok(Self { position, gain })
    }

    /// Returns the Cartesian state position.
    #[must_use]
    pub const fn position(self) -> CartesianPosition {
        self.position
    }

    /// Returns the finite linear source gain.
    #[must_use]
    pub const fn gain(self) -> f64 {
        self.gain
    }
}

/// Explicit horizontal azimuth path selection between trajectory keyframes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AzimuthPath2d {
    /// Select the shortest signed angular path; exact antipodes are rejected.
    Shortest,
    /// Select the non-negative wrapped path.
    Increasing,
    /// Select the non-positive wrapped path.
    Decreasing,
}

/// One inclusive, linearly interpolated sample-timeline trajectory segment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrajectorySegment2d {
    start_sample: u64,
    end_sample: u64,
    start_state: SpatialState2d,
    end_state: SpatialState2d,
    azimuth_path: AzimuthPath2d,
}

impl TrajectorySegment2d {
    /// Creates a segment whose endpoint samples are both included.
    pub fn new(
        start_sample: u64,
        end_sample: u64,
        start_state: SpatialState2d,
        end_state: SpatialState2d,
        azimuth_path: AzimuthPath2d,
    ) -> Result<Self, RenderError> {
        if end_sample <= start_sample {
            return Err(RenderError::InvalidTrajectorySegment {
                start_sample,
                end_sample,
            });
        }
        let span = end_sample - start_sample;
        if span > MAX_EXACT_INTERPOLATION_SPAN {
            return Err(RenderError::TrajectorySpanTooLarge { span });
        }
        let start_angle = azimuth(start_state.position)?;
        let end_angle = azimuth(end_state.position)?;
        let delta = azimuth_delta(start_angle, end_angle, azimuth_path)?;
        if !delta.is_finite() {
            return Err(RenderError::InvalidAzimuthPath);
        }
        Ok(Self {
            start_sample,
            end_sample,
            start_state,
            end_state,
            azimuth_path,
        })
    }

    /// Returns the inclusive first sample index.
    #[must_use]
    pub const fn start_sample(self) -> u64 {
        self.start_sample
    }

    /// Returns the inclusive final sample index.
    #[must_use]
    pub const fn end_sample(self) -> u64 {
        self.end_sample
    }

    /// Returns the exact start state.
    #[must_use]
    pub const fn start_state(self) -> SpatialState2d {
        self.start_state
    }

    /// Returns the exact end state.
    #[must_use]
    pub const fn end_state(self) -> SpatialState2d {
        self.end_state
    }

    /// Returns the explicit azimuth path policy.
    #[must_use]
    pub const fn azimuth_path(self) -> AzimuthPath2d {
        self.azimuth_path
    }

    fn evaluate(self, sample: u64) -> Result<SpatialState2d, RenderError> {
        if sample < self.start_sample || sample > self.end_sample {
            return Err(RenderError::TrajectorySampleOutOfRange { sample });
        }
        if sample == self.start_sample {
            return Ok(self.start_state);
        }
        if sample == self.end_sample {
            return Ok(self.end_state);
        }
        if self.start_state == self.end_state {
            return Ok(self.start_state);
        }
        let span = self.end_sample - self.start_sample;
        let offset = sample - self.start_sample;
        let t = offset as f64 / span as f64;
        let start_angle = azimuth(self.start_state.position)?;
        let delta = azimuth_delta(
            start_angle,
            azimuth(self.end_state.position)?,
            self.azimuth_path,
        )?;
        let angle = normalize_azimuth(start_angle + t * delta);
        let z = self
            .start_state
            .position
            .z
            .mul_add(1.0 - t, self.end_state.position.z * t);
        let gain = self
            .start_state
            .gain
            .mul_add(1.0 - t, self.end_state.gain * t);
        SpatialState2d::new(CartesianPosition::new(angle.sin(), angle.cos(), z), gain)
    }
}

/// A contiguous, validated piecewise-linear source trajectory.
#[derive(Clone, Debug)]
pub struct SourceTrajectory2d {
    segments: Vec<TrajectorySegment2d>,
}

impl SourceTrajectory2d {
    /// Creates a non-empty contiguous trajectory.
    pub fn new(segments: Vec<TrajectorySegment2d>) -> Result<Self, RenderError> {
        segments
            .first()
            .copied()
            .ok_or(RenderError::EmptyTrajectory)?;
        for pair in segments.windows(2) {
            let previous = pair[0];
            let next = pair[1];
            if next.start_sample() != previous.end_sample() {
                return Err(RenderError::NonContiguousTrajectory {
                    previous_end: previous.end_sample(),
                    next_start: next.start_sample(),
                });
            }
            if next.start_state() != previous.end_state() {
                return Err(RenderError::DiscontinuousTrajectory {
                    boundary: next.start_sample(),
                });
            }
        }
        Ok(Self { segments })
    }

    /// Returns the inclusive trajectory domain.
    #[must_use]
    pub fn domain(&self) -> (u64, u64) {
        (
            self.segments[0].start_sample(),
            self.segments[self.segments.len() - 1].end_sample(),
        )
    }

    /// Returns the validated segments.
    #[must_use]
    pub fn segments(&self) -> &[TrajectorySegment2d] {
        &self.segments
    }

    /// Evaluates the state at an absolute trajectory sample index.
    pub fn evaluate(&self, sample: u64) -> Result<SpatialState2d, RenderError> {
        let (start, end) = self.domain();
        if sample < start || sample > end {
            return Err(RenderError::TrajectorySampleOutOfRange { sample });
        }
        let index = self
            .segments
            .partition_point(|segment| segment.end_sample() < sample);
        self.segments[index.min(self.segments.len() - 1)].evaluate(sample)
    }

    fn validate_block(&self, start: u64, length: usize) -> Result<(), RenderError> {
        let end_exclusive = start
            .checked_add(length as u64)
            .ok_or(RenderError::SampleIndexOverflow)?;
        let (domain_start, domain_end) = self.domain();
        if length > 0 && (start < domain_start || end_exclusive - 1 > domain_end) {
            return Err(RenderError::TrajectoryBlockOutOfRange {
                start,
                length,
                domain_start,
                domain_end,
            });
        }
        if length == 0 && (start < domain_start || start > domain_end.saturating_add(1)) {
            return Err(RenderError::TrajectoryBlockOutOfRange {
                start,
                length,
                domain_start,
                domain_end,
            });
        }
        Ok(())
    }
}

/// A borrowed mono PCM block bound to an absolute source trajectory range.
#[derive(Clone, Copy, Debug)]
pub struct TrajectorySourceBlock<'a> {
    id: SourceId,
    samples: &'a [f64],
    trajectory: &'a SourceTrajectory2d,
    block_start_sample: u64,
}

impl<'a> TrajectorySourceBlock<'a> {
    /// Creates a block after validating samples and its absolute range.
    pub fn new(
        id: SourceId,
        samples: &'a [f64],
        trajectory: &'a SourceTrajectory2d,
        block_start_sample: u64,
    ) -> Result<Self, RenderError> {
        trajectory.validate_block(block_start_sample, samples.len())?;
        if let Some(sample_index) = samples.iter().position(|sample| !sample.is_finite()) {
            return Err(RenderError::NonFiniteSourceSample { id, sample_index });
        }
        Ok(Self {
            id,
            samples,
            trajectory,
            block_start_sample,
        })
    }

    /// Returns the opaque source identity.
    #[must_use]
    pub const fn id(self) -> SourceId {
        self.id
    }

    /// Returns the borrowed source samples.
    #[must_use]
    pub const fn samples(self) -> &'a [f64] {
        self.samples
    }

    /// Returns the referenced trajectory.
    #[must_use]
    pub const fn trajectory(self) -> &'a SourceTrajectory2d {
        self.trajectory
    }

    /// Returns the absolute sample index of the first sample in this block.
    #[must_use]
    pub const fn block_start_sample(self) -> u64 {
        self.block_start_sample
    }
}

/// An opaque caller-provided identity for one output speaker.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SpeakerId(u64);

impl SpeakerId {
    /// Creates an opaque speaker identity from a caller-owned value.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the caller-owned identity value.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// One full-range horizontal speaker direction in the caller's output order.
///
/// The horizontal projection of `position` defines the speaker direction. The
/// `z` component is retained for coordinate completeness but is ignored by
/// the two-dimensional renderer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Speaker2d {
    id: SpeakerId,
    position: CartesianPosition,
}

impl Speaker2d {
    /// Creates a speaker after validating its finite, nonzero horizontal
    /// direction.
    pub fn new(id: SpeakerId, position: CartesianPosition) -> Result<Self, RenderError> {
        validate_position(position)?;
        if horizontal_magnitude_squared(position) <= 0.0 {
            return Err(RenderError::UndefinedSpeakerDirection { id });
        }
        Ok(Self { id, position })
    }

    /// Returns the opaque speaker identity.
    #[must_use]
    pub const fn id(self) -> SpeakerId {
        self.id
    }

    /// Returns the speaker's Cartesian direction.
    #[must_use]
    pub const fn position(self) -> CartesianPosition {
        self.position
    }
}

/// A deterministic adjacent-speaker pair selected by a two-dimensional layout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpeakerPair2d {
    first_index: usize,
    second_index: usize,
}

impl SpeakerPair2d {
    /// Returns the first public output index in angular order.
    #[must_use]
    pub const fn first_index(self) -> usize {
        self.first_index
    }

    /// Returns the second public output index in angular order.
    #[must_use]
    pub const fn second_index(self) -> usize {
        self.second_index
    }
}

/// Energy-normalized gains for one selected two-dimensional speaker pair.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PairGains2d {
    pair: SpeakerPair2d,
    first: f64,
    second: f64,
}

impl PairGains2d {
    /// Returns the selected public output pair.
    #[must_use]
    pub const fn pair(self) -> SpeakerPair2d {
        self.pair
    }

    /// Returns the first pair gain.
    #[must_use]
    pub const fn first(self) -> f64 {
        self.first
    }

    /// Returns the second pair gain.
    #[must_use]
    pub const fn second(self) -> f64 {
        self.second
    }
}

/// A caller-declared horizontal speaker layout.
///
/// The input slice order is the public output-channel order. Internal azimuth
/// sorting is used only to construct adjacent panning sectors; it never
/// changes the caller-visible output-plane order. Layout construction may
/// allocate bounded speaker/pair metadata, while rendering uses no
/// per-sample allocation.
#[derive(Clone, Debug)]
pub struct SpeakerLayout2d {
    speakers: Vec<Speaker2d>,
    directions: Vec<(f64, f64, f64)>,
    sorted_indices: Vec<usize>,
    pairs: Vec<LayoutPair>,
}

#[derive(Clone, Copy, Debug)]
struct LayoutPair {
    pair: SpeakerPair2d,
    start_angle: f64,
    end_angle: f64,
}

impl SpeakerLayout2d {
    /// Builds a layout from the caller-declared public output order.
    pub fn new(speakers: Vec<Speaker2d>) -> Result<Self, RenderError> {
        if speakers.len() < 2 {
            return Err(RenderError::TooFewSpeakers {
                actual: speakers.len(),
            });
        }

        for (index, speaker) in speakers.iter().enumerate() {
            if speakers[index + 1..]
                .iter()
                .any(|other| other.id == speaker.id)
            {
                return Err(RenderError::DuplicateSpeakerId { id: speaker.id });
            }
        }

        let directions: Vec<_> = speakers
            .iter()
            .map(|speaker| normalized_horizontal(speaker.position))
            .collect::<Result<_, _>>()?;
        let mut sorted_indices: Vec<_> = (0..speakers.len()).collect();
        sorted_indices.sort_by(|&left, &right| {
            directions[left]
                .2
                .total_cmp(&directions[right].2)
                .then_with(|| left.cmp(&right))
        });

        for window in sorted_indices.windows(2) {
            let first = directions[window[0]].2;
            let second = directions[window[1]].2;
            if second - first <= ANGLE_TOLERANCE {
                return Err(RenderError::DuplicateSpeakerAzimuth {
                    first: window[0],
                    second: window[1],
                });
            }
        }

        let mut pairs = Vec::new();
        for (sorted_position, &first_index) in sorted_indices.iter().enumerate() {
            let second_index = sorted_indices[(sorted_position + 1) % sorted_indices.len()];
            let start_angle = directions[first_index].2;
            let mut end_angle = directions[second_index].2;
            if sorted_position + 1 == sorted_indices.len() {
                end_angle += TWO_PI;
            }
            let separation = end_angle - start_angle;
            if separation > ANGLE_TOLERANCE && separation < std::f64::consts::PI {
                pairs.push(LayoutPair {
                    pair: SpeakerPair2d {
                        first_index,
                        second_index,
                    },
                    start_angle,
                    end_angle,
                });
            }
        }
        if pairs.is_empty() {
            return Err(RenderError::NoUsableSpeakerPair);
        }

        Ok(Self {
            speakers,
            directions,
            sorted_indices,
            pairs,
        })
    }

    /// Returns the caller-declared speaker/output order.
    #[must_use]
    pub fn speakers(&self) -> &[Speaker2d] {
        &self.speakers
    }

    /// Returns the number of public output planes.
    #[must_use]
    pub fn speaker_count(&self) -> usize {
        self.speakers.len()
    }

    /// Returns the deterministic pair and gains for one explicit position.
    pub fn pair_gains(&self, position: CartesianPosition) -> Result<PairGains2d, RenderError> {
        validate_position(position)?;
        let (x, y, theta) = normalized_horizontal(position)?;

        for (index, &(_, _, speaker_angle)) in self.directions.iter().enumerate() {
            if angular_distance(theta, speaker_angle) <= ANGLE_TOLERANCE {
                return Ok(PairGains2d {
                    pair: SpeakerPair2d {
                        first_index: index,
                        second_index: index,
                    },
                    first: 1.0,
                    second: 0.0,
                });
            }
        }

        let mut selected = None;
        let mut normalized_theta = theta;
        if normalized_theta < self.pairs[0].start_angle {
            normalized_theta += TWO_PI;
        }
        for candidate in &self.pairs {
            let mut candidate_theta = normalized_theta;
            let start = candidate.start_angle;
            let mut end = candidate.end_angle;
            if end < start {
                end += TWO_PI;
            }
            if candidate_theta < start {
                candidate_theta += TWO_PI;
            }
            if candidate_theta >= start - ANGLE_TOLERANCE
                && candidate_theta <= end + ANGLE_TOLERANCE
                && selected.replace(*candidate).is_some()
            {
                return Err(RenderError::AmbiguousSpeakerPair);
            }
        }
        let candidate = selected.ok_or(RenderError::UnsupportedDirection { angle: theta })?;
        let first = self.directions[candidate.pair.first_index];
        let second = self.directions[candidate.pair.second_index];
        let determinant = first.0.mul_add(second.1, -(second.0 * first.1));
        if determinant.abs() <= DETERMINANT_TOLERANCE || !determinant.is_finite() {
            return Err(RenderError::SingularSpeakerPair {
                first: candidate.pair.first_index,
                second: candidate.pair.second_index,
            });
        }
        let mut first_gain = (x.mul_add(second.1, -(second.0 * y))) / determinant;
        let mut second_gain = (first.0.mul_add(y, -(x * first.1))) / determinant;
        if !first_gain.is_finite() || !second_gain.is_finite() {
            return Err(RenderError::InvalidPairGains);
        }
        if first_gain < -GAIN_TOLERANCE || second_gain < -GAIN_TOLERANCE {
            return Err(RenderError::NegativePairGain);
        }
        if first_gain < 0.0 {
            first_gain = 0.0;
        }
        if second_gain < 0.0 {
            second_gain = 0.0;
        }
        let norm = first_gain
            .mul_add(first_gain, second_gain * second_gain)
            .sqrt();
        if !norm.is_finite() || norm <= DETERMINANT_TOLERANCE {
            return Err(RenderError::InvalidPairGains);
        }
        Ok(PairGains2d {
            pair: candidate.pair,
            first: first_gain / norm,
            second: second_gain / norm,
        })
    }

    /// Returns the deterministic internally sorted public speaker indices.
    #[must_use]
    pub fn sorted_indices(&self) -> &[usize] {
        &self.sorted_indices
    }
}

/// Stateless block renderer for explicit sources and an arbitrary 2D layout.
#[derive(Clone, Debug)]
pub struct LayoutRenderer2d {
    layout: SpeakerLayout2d,
}

impl LayoutRenderer2d {
    /// Creates a renderer for a validated speaker layout.
    #[must_use]
    pub fn new(layout: SpeakerLayout2d) -> Self {
        Self { layout }
    }

    /// Returns the immutable speaker layout and public output order.
    #[must_use]
    pub const fn layout(&self) -> &SpeakerLayout2d {
        &self.layout
    }

    /// Renders one static-position block into caller-owned planar outputs.
    ///
    /// `outputs` must have one reusable plane per declared speaker, in the
    /// exact order supplied to [`SpeakerLayout2d::new`]. The renderer clears
    /// all planes on success, performs no truncation/padding, and does not
    /// allocate per sample. Output is unclipped `f64`; no LFE routing,
    /// distance model, or bass management is applied.
    pub fn render_block(
        &self,
        sources: &[ExplicitSpatialSource<'_>],
        outputs: &mut [&mut [f64]],
    ) -> Result<(), RenderError> {
        if outputs.len() != self.layout.speakers.len() {
            return Err(RenderError::SpeakerOutputCountMismatch {
                expected: self.layout.speakers.len(),
                actual: outputs.len(),
            });
        }
        let block_length = outputs.first().map_or(0, |output| output.len());
        for (index, output) in outputs.iter().enumerate() {
            if output.len() != block_length {
                return Err(RenderError::SpeakerOutputLengthMismatch {
                    speaker_index: index,
                    expected: block_length,
                    actual: output.len(),
                });
            }
        }
        for source in sources {
            if source.samples.len() != block_length {
                return Err(RenderError::SourceBlockLengthMismatch {
                    id: source.id,
                    expected: block_length,
                    actual: source.samples.len(),
                });
            }
            validate_position(source.position)?;
            validate_gain(source.gain)?;
            if let Some(sample_index) = source.samples.iter().position(|sample| !sample.is_finite())
            {
                return Err(RenderError::NonFiniteSourceSample {
                    id: source.id,
                    sample_index,
                });
            }
            self.layout.pair_gains(source.position)?;
        }

        for output in outputs.iter_mut() {
            output.fill(0.0);
        }
        for source in sources {
            let gains = self.layout.pair_gains(source.position)?;
            let first_index = gains.pair.first_index;
            let second_index = gains.pair.second_index;
            let first_gain = gains.first * source.gain;
            let second_gain = gains.second * source.gain;
            for (sample_index, &sample) in source.samples.iter().enumerate() {
                outputs[first_index][sample_index] += sample * first_gain;
                if first_index == second_index {
                    if !outputs[first_index][sample_index].is_finite() {
                        clear_outputs(outputs);
                        return Err(RenderError::NonFiniteOutput {
                            channel: OutputChannel::Speaker(first_index),
                            sample_index,
                        });
                    }
                } else {
                    outputs[second_index][sample_index] += sample * second_gain;
                    if !outputs[first_index][sample_index].is_finite() {
                        clear_outputs(outputs);
                        return Err(RenderError::NonFiniteOutput {
                            channel: OutputChannel::Speaker(first_index),
                            sample_index,
                        });
                    }
                    if !outputs[second_index][sample_index].is_finite() {
                        clear_outputs(outputs);
                        return Err(RenderError::NonFiniteOutput {
                            channel: OutputChannel::Speaker(second_index),
                            sample_index,
                        });
                    }
                }
            }
        }
        Ok(())
    }

    /// Renders absolute-sample trajectory blocks into caller-owned outputs.
    ///
    /// The trajectory is evaluated for every source sample. All source IDs,
    /// ranges, states, and panner directions are preflighted before outputs
    /// are cleared, so structural failures leave caller buffers unchanged.
    /// Numerical overflow during accumulation clears outputs and returns an
    /// error; no partially valid result is presented.
    pub fn render_trajectory_block(
        &self,
        sources: &[TrajectorySourceBlock<'_>],
        outputs: &mut [&mut [f64]],
    ) -> Result<(), RenderError> {
        validate_speaker_outputs(self.layout.speakers.len(), outputs)?;
        let block_length = outputs.first().map_or(0, |output| output.len());
        validate_trajectory_sources(sources, block_length)?;
        for source in sources {
            for offset in 0..block_length {
                let sample = source
                    .block_start_sample
                    .checked_add(offset as u64)
                    .ok_or(RenderError::SampleIndexOverflow)?;
                let state = source.trajectory.evaluate(sample)?;
                self.layout.pair_gains(state.position)?;
                validate_gain(state.gain)?;
            }
        }
        for output in outputs.iter_mut() {
            output.fill(0.0);
        }
        for source in sources {
            for (offset, &sample_value) in source.samples.iter().enumerate() {
                let sample = source.block_start_sample + offset as u64;
                let state = source.trajectory.evaluate(sample)?;
                let gains = self.layout.pair_gains(state.position)?;
                let first = gains.pair.first_index;
                let second = gains.pair.second_index;
                let first_gain = gains.first * state.gain;
                let second_gain = gains.second * state.gain;
                outputs[first][offset] += sample_value * first_gain;
                if first != second {
                    outputs[second][offset] += sample_value * second_gain;
                }
                check_speaker_outputs_finite(outputs, offset, first, second)?;
            }
        }
        Ok(())
    }
}

/// One caller-declared three-dimensional output speaker direction.
///
/// The direction is normalized only for the internal VBAP solve. Speaker
/// order is never inferred from geometry; it is the output order supplied to
/// the layout.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Speaker3d {
    id: SpeakerId,
    position: CartesianPosition,
}

impl Speaker3d {
    /// Creates a speaker with a finite, nonzero 3D direction.
    pub fn new(id: SpeakerId, position: CartesianPosition) -> Result<Self, RenderError> {
        validate_position(position)?;
        normalized_3d(position)?;
        Ok(Self { id, position })
    }

    /// Returns the opaque speaker identity.
    #[must_use]
    pub const fn id(self) -> SpeakerId {
        self.id
    }

    /// Returns the caller-declared Cartesian direction.
    #[must_use]
    pub const fn position(self) -> CartesianPosition {
        self.position
    }
}

/// One explicit 3D VBAP triplet in caller-declared speaker-ID order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpeakerTriplet3d {
    first: SpeakerId,
    second: SpeakerId,
    third: SpeakerId,
}

impl SpeakerTriplet3d {
    /// Creates a triplet. The three IDs must be distinct.
    pub fn new(first: SpeakerId, second: SpeakerId, third: SpeakerId) -> Result<Self, RenderError> {
        if first == second || first == third || second == third {
            return Err(RenderError::DuplicateTripletSpeaker);
        }
        Ok(Self {
            first,
            second,
            third,
        })
    }

    /// Returns the first declared speaker ID.
    #[must_use]
    pub const fn first(self) -> SpeakerId {
        self.first
    }

    /// Returns the second declared speaker ID.
    #[must_use]
    pub const fn second(self) -> SpeakerId {
        self.second
    }

    /// Returns the third declared speaker ID.
    #[must_use]
    pub const fn third(self) -> SpeakerId {
        self.third
    }

    const fn ids(self) -> [SpeakerId; 3] {
        [self.first, self.second, self.third]
    }
}

#[derive(Clone, Copy, Debug)]
struct TripletRecord3d {
    triplet: SpeakerTriplet3d,
    indices: [usize; 3],
    columns: [[f64; 3]; 3],
}

/// A validated explicit 3D speaker topology.
///
/// The caller supplies both the public output order and every admissible
/// triplet. The layout never constructs Delaunay triangles, a convex hull, or
/// a “best” triplet implicitly. A direction covered by more than one declared
/// triplet is accepted only when all resulting public-order gain vectors agree
/// within the fixed numerical ambiguity tolerance.
#[derive(Clone, Debug)]
pub struct SpeakerLayout3d {
    speakers: Vec<Speaker3d>,
    directions: Vec<[f64; 3]>,
    triplets: Vec<TripletRecord3d>,
    declared_triplets: Vec<SpeakerTriplet3d>,
}

impl SpeakerLayout3d {
    /// Builds an immutable topology from caller-declared speakers and triplets.
    pub fn new(
        speakers: Vec<Speaker3d>,
        triplets: Vec<SpeakerTriplet3d>,
    ) -> Result<Self, RenderError> {
        if speakers.len() < 3 {
            return Err(RenderError::TooFew3dSpeakers {
                actual: speakers.len(),
            });
        }
        for (index, speaker) in speakers.iter().enumerate() {
            if speakers[index + 1..]
                .iter()
                .any(|other| other.id == speaker.id)
            {
                return Err(RenderError::DuplicateSpeakerId { id: speaker.id });
            }
        }
        let directions: Vec<_> = speakers
            .iter()
            .map(|speaker| normalized_3d(speaker.position))
            .collect::<Result<_, _>>()?;
        for first in 0..directions.len() {
            for second in first + 1..directions.len() {
                if direction_distance_squared(directions[first], directions[second])
                    <= DIRECTION_TOLERANCE * DIRECTION_TOLERANCE
                {
                    return Err(RenderError::DuplicateSpeakerDirection { first, second });
                }
            }
        }

        let mut records = Vec::with_capacity(triplets.len());
        for triplet in triplets {
            let ids = triplet.ids();
            let mut indices = [0_usize; 3];
            for (slot, id) in ids.into_iter().enumerate() {
                indices[slot] = speakers
                    .iter()
                    .position(|speaker| speaker.id == id)
                    .ok_or(RenderError::MissingTripletSpeaker { id })?;
            }
            let mut canonical = indices;
            canonical.sort_unstable();
            if records.iter().any(|record: &TripletRecord3d| {
                let mut prior = record.indices;
                prior.sort_unstable();
                prior == canonical
            }) {
                return Err(RenderError::DuplicateTriplet);
            }
            let columns = [
                directions[indices[0]],
                directions[indices[1]],
                directions[indices[2]],
            ];
            let determinant = determinant3(columns);
            if !determinant.is_finite() || determinant.abs() <= DETERMINANT_TOLERANCE {
                return Err(RenderError::DegenerateTriplet { indices });
            }
            records.push(TripletRecord3d {
                triplet,
                indices,
                columns,
            });
        }
        if records.is_empty() {
            return Err(RenderError::NoDeclared3dTriplet);
        }
        Ok(Self {
            speakers,
            directions,
            declared_triplets: records.iter().map(|record| record.triplet).collect(),
            triplets: records,
        })
    }

    /// Returns the caller-declared public output order.
    #[must_use]
    pub fn speakers(&self) -> &[Speaker3d] {
        &self.speakers
    }

    /// Returns the number of public output speakers.
    #[must_use]
    pub fn speaker_count(&self) -> usize {
        self.speakers.len()
    }

    /// Returns the number of explicitly declared candidate triplets.
    #[must_use]
    pub fn triplet_count(&self) -> usize {
        self.triplets.len()
    }

    /// Returns the caller-declared triplets in declaration order.
    #[must_use]
    pub fn triplets(&self) -> &[SpeakerTriplet3d] {
        &self.declared_triplets
    }

    /// Resolves one position to deterministic energy-normalized triplet gains.
    pub fn gains(&self, position: CartesianPosition) -> Result<TripletGains3d, RenderError> {
        validate_position(position)?;
        let direction = normalized_3d(position)?;
        for (index, &speaker_direction) in self.directions.iter().enumerate() {
            if direction_distance_squared(direction, speaker_direction)
                <= DIRECTION_TOLERANCE * DIRECTION_TOLERANCE
            {
                return Ok(TripletGains3d {
                    triplet: None,
                    indices: [index, index, index],
                    gains: [1.0, 0.0, 0.0],
                    exact_index: Some(index),
                    speaker_count: self.speakers.len(),
                });
            }
        }

        let mut selected: Option<([usize; 3], [f64; 3], SpeakerTriplet3d)> = None;
        for record in &self.triplets {
            let mut gains = solve_3x3_for(record.columns, direction)?;
            if gains.iter().any(|gain| *gain < -GAIN_TOLERANCE) {
                continue;
            }
            for gain in &mut gains {
                if *gain < 0.0 {
                    *gain = 0.0;
                }
            }
            let norm = gains.iter().fold(0.0, |sum, gain| sum + gain * gain).sqrt();
            if !norm.is_finite() || norm <= DETERMINANT_TOLERANCE {
                continue;
            }
            for gain in &mut gains {
                *gain /= norm;
            }
            if let Some((prior_indices, prior_gains, _)) = selected {
                for speaker_index in 0..self.speakers.len() {
                    let prior = gain_at(prior_indices, prior_gains, speaker_index);
                    let current = gain_at(record.indices, gains, speaker_index);
                    if (prior - current).abs() > AMBIGUITY_TOLERANCE {
                        return Err(RenderError::Ambiguous3dCoverage);
                    }
                }
            } else {
                selected = Some((record.indices, gains, record.triplet));
            }
        }
        let (indices, gains, triplet) = selected.ok_or(RenderError::Unsupported3dDirection {
            x: direction[0],
            y: direction[1],
            z: direction[2],
        })?;
        Ok(TripletGains3d {
            triplet: Some(triplet),
            indices,
            gains,
            exact_index: None,
            speaker_count: self.speakers.len(),
        })
    }

    fn gains_into(
        &self,
        position: CartesianPosition,
        output: &mut [f64],
    ) -> Result<(), RenderError> {
        self.gains(position)?.write_full_gains(output)
    }
}

/// Energy-normalized gains for one explicit 3D triplet.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TripletGains3d {
    triplet: Option<SpeakerTriplet3d>,
    indices: [usize; 3],
    gains: [f64; 3],
    exact_index: Option<usize>,
    speaker_count: usize,
}

impl TripletGains3d {
    /// Returns the declared triplet, or `None` for an exact one-speaker hit.
    #[must_use]
    pub const fn triplet(self) -> Option<SpeakerTriplet3d> {
        self.triplet
    }

    /// Returns the three gains in declared triplet order.
    #[must_use]
    pub const fn gains(self) -> [f64; 3] {
        self.gains
    }

    /// Returns the first triplet gain.
    #[must_use]
    pub const fn first(self) -> f64 {
        self.gains[0]
    }

    /// Returns the second triplet gain.
    #[must_use]
    pub const fn second(self) -> f64 {
        self.gains[1]
    }

    /// Returns the third triplet gain.
    #[must_use]
    pub const fn third(self) -> f64 {
        self.gains[2]
    }

    /// Expands gains into the layout's caller-declared output order.
    pub fn write_full_gains(self, output: &mut [f64]) -> Result<(), RenderError> {
        if output.len() != self.speaker_count {
            return Err(RenderError::SpeakerOutputCountMismatch {
                expected: self.speaker_count,
                actual: output.len(),
            });
        }
        output.fill(0.0);
        if let Some(index) = self.exact_index {
            output[index] = 1.0;
        } else {
            for (index, gain) in self.indices.into_iter().zip(self.gains) {
                output[index] = gain;
            }
        }
        Ok(())
    }
}

/// Stateless block renderer for an explicit 3D speaker topology.
#[derive(Clone, Debug)]
pub struct LayoutRenderer3d {
    layout: SpeakerLayout3d,
}

impl LayoutRenderer3d {
    /// Creates a renderer for a validated explicit 3D topology.
    #[must_use]
    pub fn new(layout: SpeakerLayout3d) -> Self {
        Self { layout }
    }

    /// Returns the immutable explicit topology.
    #[must_use]
    pub const fn layout(&self) -> &SpeakerLayout3d {
        &self.layout
    }

    /// Renders static explicit sources into caller-owned planar outputs.
    ///
    /// All structure, source data, and 3D gain resolution are preflighted
    /// before outputs are cleared. Mixing uses one reusable speaker-gain
    /// scratch vector per call and never allocates per sample or per source
    /// sample. A numeric failure clears all outputs.
    pub fn render_block(
        &self,
        sources: &[ExplicitSpatialSource<'_>],
        outputs: &mut [&mut [f64]],
    ) -> Result<(), RenderError> {
        validate_speaker_outputs(self.layout.speaker_count(), outputs)?;
        let block_length = outputs.first().map_or(0, |output| output.len());
        for (index, source) in sources.iter().enumerate() {
            if sources[index + 1..]
                .iter()
                .any(|other| other.id == source.id)
            {
                return Err(RenderError::DuplicateSourceId { id: source.id });
            }
            if source.samples.len() != block_length {
                return Err(RenderError::SourceBlockLengthMismatch {
                    id: source.id,
                    expected: block_length,
                    actual: source.samples.len(),
                });
            }
            validate_position(source.position)?;
            validate_gain(source.gain)?;
            if let Some(sample_index) = source.samples.iter().position(|sample| !sample.is_finite())
            {
                return Err(RenderError::NonFiniteSourceSample {
                    id: source.id,
                    sample_index,
                });
            }
            self.layout.gains(source.position)?;
        }

        for output in outputs.iter_mut() {
            output.fill(0.0);
        }
        let mut full_gains = vec![0.0; self.layout.speaker_count()];
        for source in sources {
            self.layout.gains_into(source.position, &mut full_gains)?;
            for (sample_index, &sample) in source.samples.iter().enumerate() {
                for (speaker_index, output) in outputs.iter_mut().enumerate() {
                    output[sample_index] += sample * source.gain * full_gains[speaker_index];
                    if !output[sample_index].is_finite() {
                        clear_outputs(outputs);
                        return Err(RenderError::NonFiniteOutput {
                            channel: OutputChannel::Speaker(speaker_index),
                            sample_index,
                        });
                    }
                }
            }
        }
        Ok(())
    }
}

fn normalized_3d(position: CartesianPosition) -> Result<[f64; 3], RenderError> {
    validate_position(position)?;
    let norm = position
        .x
        .mul_add(
            position.x,
            position.y.mul_add(position.y, position.z * position.z),
        )
        .sqrt();
    if !norm.is_finite() || norm <= 0.0 {
        return Err(RenderError::Undefined3dSpeakerDirection);
    }
    Ok([position.x / norm, position.y / norm, position.z / norm])
}

fn direction_distance_squared(first: [f64; 3], second: [f64; 3]) -> f64 {
    let dx = first[0] - second[0];
    let dy = first[1] - second[1];
    let dz = first[2] - second[2];
    dx.mul_add(dx, dy.mul_add(dy, dz * dz))
}

fn determinant3(columns: [[f64; 3]; 3]) -> f64 {
    let [a, b, c] = columns;
    a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
        + a[2] * (b[0] * c[1] - b[1] * c[0])
}

fn solve_3x3_for(columns: [[f64; 3]; 3], vector: [f64; 3]) -> Result<[f64; 3], RenderError> {
    let determinant = determinant3(columns);
    if !determinant.is_finite() || determinant.abs() <= DETERMINANT_TOLERANCE {
        return Err(RenderError::Invalid3dGains);
    }
    let mut first = columns;
    first[0] = vector;
    let mut second = columns;
    second[1] = vector;
    let mut third = columns;
    third[2] = vector;
    let result = [
        determinant3(first) / determinant,
        determinant3(second) / determinant,
        determinant3(third) / determinant,
    ];
    if result.iter().all(|gain| gain.is_finite()) {
        Ok(result)
    } else {
        Err(RenderError::Invalid3dGains)
    }
}

fn gain_at(indices: [usize; 3], gains: [f64; 3], index: usize) -> f64 {
    indices
        .into_iter()
        .zip(gains)
        .find_map(|(candidate, gain)| (candidate == index).then_some(gain))
        .unwrap_or(0.0)
}

const ANGLE_TOLERANCE: f64 = 1.0e-12;
const DETERMINANT_TOLERANCE: f64 = 1.0e-12;
const GAIN_TOLERANCE: f64 = 1.0e-12;
const DIRECTION_TOLERANCE: f64 = 1.0e-12;
const AMBIGUITY_TOLERANCE: f64 = 1.0e-10;
const TWO_PI: f64 = 2.0 * std::f64::consts::PI;
const MAX_EXACT_INTERPOLATION_SPAN: u64 = 1_u64 << 53;

fn clear_outputs(outputs: &mut [&mut [f64]]) {
    for output in outputs.iter_mut() {
        output.fill(0.0);
    }
}

fn validate_speaker_outputs(expected: usize, outputs: &[&mut [f64]]) -> Result<(), RenderError> {
    if outputs.len() != expected {
        return Err(RenderError::SpeakerOutputCountMismatch {
            expected,
            actual: outputs.len(),
        });
    }
    let block_length = outputs.first().map_or(0, |output| output.len());
    for (speaker_index, output) in outputs.iter().enumerate() {
        if output.len() != block_length {
            return Err(RenderError::SpeakerOutputLengthMismatch {
                speaker_index,
                expected: block_length,
                actual: output.len(),
            });
        }
    }
    Ok(())
}

fn validate_trajectory_sources(
    sources: &[TrajectorySourceBlock<'_>],
    block_length: usize,
) -> Result<(), RenderError> {
    for (index, source) in sources.iter().enumerate() {
        if sources[index + 1..]
            .iter()
            .any(|other| other.id == source.id)
        {
            return Err(RenderError::DuplicateSourceId { id: source.id });
        }
        if source.samples.len() != block_length {
            return Err(RenderError::SourceBlockLengthMismatch {
                id: source.id,
                expected: block_length,
                actual: source.samples.len(),
            });
        }
        source
            .trajectory
            .validate_block(source.block_start_sample, block_length)?;
    }
    Ok(())
}

fn check_speaker_outputs_finite(
    outputs: &mut [&mut [f64]],
    sample_index: usize,
    first: usize,
    second: usize,
) -> Result<(), RenderError> {
    if !outputs[first][sample_index].is_finite() {
        clear_outputs(outputs);
        return Err(RenderError::NonFiniteOutput {
            channel: OutputChannel::Speaker(first),
            sample_index,
        });
    }
    if first != second && !outputs[second][sample_index].is_finite() {
        clear_outputs(outputs);
        return Err(RenderError::NonFiniteOutput {
            channel: OutputChannel::Speaker(second),
            sample_index,
        });
    }
    Ok(())
}

fn horizontal_magnitude_squared(position: CartesianPosition) -> f64 {
    position.x.mul_add(position.x, position.y * position.y)
}

fn normalize_azimuth(mut angle: f64) -> f64 {
    while angle >= std::f64::consts::PI {
        angle -= TWO_PI;
    }
    while angle < -std::f64::consts::PI {
        angle += TWO_PI;
    }
    angle
}

fn normalized_horizontal(position: CartesianPosition) -> Result<(f64, f64, f64), RenderError> {
    validate_position(position)?;
    let magnitude = horizontal_magnitude_squared(position).sqrt();
    if !magnitude.is_finite() || magnitude <= 0.0 {
        return Err(RenderError::UndefinedHorizontalDirection);
    }
    let x = position.x / magnitude;
    let y = position.y / magnitude;
    let theta = normalize_azimuth(position.x.atan2(position.y));
    Ok((x, y, theta))
}

fn angular_distance(first: f64, second: f64) -> f64 {
    normalize_azimuth(first - second).abs()
}

fn azimuth(position: CartesianPosition) -> Result<f64, RenderError> {
    let (_, _, angle) = normalized_horizontal(position)?;
    Ok(angle)
}

fn azimuth_delta(start: f64, end: f64, path: AzimuthPath2d) -> Result<f64, RenderError> {
    let shortest = normalize_azimuth(end - start);
    match path {
        AzimuthPath2d::Shortest => {
            if (shortest.abs() - std::f64::consts::PI).abs() <= ANGLE_TOLERANCE {
                return Err(RenderError::AmbiguousAntipodalPath);
            }
            Ok(shortest)
        }
        AzimuthPath2d::Increasing => {
            let mut delta = end - start;
            while delta < 0.0 {
                delta += TWO_PI;
            }
            while delta >= TWO_PI {
                delta -= TWO_PI;
            }
            Ok(delta)
        }
        AzimuthPath2d::Decreasing => {
            let mut delta = end - start;
            while delta > 0.0 {
                delta -= TWO_PI;
            }
            while delta <= -TWO_PI {
                delta += TWO_PI;
            }
            Ok(delta)
        }
    }
}

impl StereoGains {
    /// Computes front-horizontal equal-power gains for a position.
    pub fn for_position(position: CartesianPosition) -> Result<Self, RenderError> {
        validate_position(position)?;
        if position.y < 0.0 {
            return Err(RenderError::RearHemisphereUnsupported { y: position.y });
        }
        if position.x == 0.0 && position.y == 0.0 {
            return Err(RenderError::UndefinedHorizontalDirection);
        }
        let theta = position.x.atan2(position.y);
        let u = (theta + std::f64::consts::FRAC_PI_2) / std::f64::consts::PI;
        Ok(Self {
            left: (u * std::f64::consts::FRAC_PI_2).cos(),
            right: (u * std::f64::consts::FRAC_PI_2).sin(),
        })
    }
}

/// Channel selected by a non-finite output error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputChannel {
    Left,
    Right,
    Speaker(usize),
}

/// Explicit-scene renderer failures.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RenderError {
    InvalidSampleRate,
    NonFinitePosition {
        axis: &'static str,
        value: f64,
    },
    NonFiniteGain {
        value: f64,
    },
    NonFiniteSourceSample {
        id: SourceId,
        sample_index: usize,
    },
    UndefinedHorizontalDirection,
    RearHemisphereUnsupported {
        y: f64,
    },
    DuplicateSourceId {
        id: SourceId,
    },
    StereoOutputLengthMismatch {
        left: usize,
        right: usize,
    },
    SourceBlockLengthMismatch {
        id: SourceId,
        expected: usize,
        actual: usize,
    },
    NonFiniteOutput {
        channel: OutputChannel,
        sample_index: usize,
    },
    TooFewSpeakers {
        actual: usize,
    },
    UndefinedSpeakerDirection {
        id: SpeakerId,
    },
    Undefined3dSpeakerDirection,
    DuplicateSpeakerId {
        id: SpeakerId,
    },
    TooFew3dSpeakers {
        actual: usize,
    },
    DuplicateSpeakerDirection {
        first: usize,
        second: usize,
    },
    DuplicateTripletSpeaker,
    MissingTripletSpeaker {
        id: SpeakerId,
    },
    DuplicateTriplet,
    DegenerateTriplet {
        indices: [usize; 3],
    },
    NoDeclared3dTriplet,
    Invalid3dGains,
    Ambiguous3dCoverage,
    Unsupported3dDirection {
        x: f64,
        y: f64,
        z: f64,
    },
    DuplicateSpeakerAzimuth {
        first: usize,
        second: usize,
    },
    NoUsableSpeakerPair,
    SingularSpeakerPair {
        first: usize,
        second: usize,
    },
    InvalidPairGains,
    NegativePairGain,
    AmbiguousSpeakerPair,
    UnsupportedDirection {
        angle: f64,
    },
    SpeakerOutputCountMismatch {
        expected: usize,
        actual: usize,
    },
    SpeakerOutputLengthMismatch {
        speaker_index: usize,
        expected: usize,
        actual: usize,
    },
    EmptyTrajectory,
    InvalidTrajectorySegment {
        start_sample: u64,
        end_sample: u64,
    },
    TrajectorySpanTooLarge {
        span: u64,
    },
    InvalidAzimuthPath,
    AmbiguousAntipodalPath,
    NonContiguousTrajectory {
        previous_end: u64,
        next_start: u64,
    },
    DiscontinuousTrajectory {
        boundary: u64,
    },
    TrajectorySampleOutOfRange {
        sample: u64,
    },
    TrajectoryBlockOutOfRange {
        start: u64,
        length: usize,
        domain_start: u64,
        domain_end: u64,
    },
    SampleIndexOverflow,
}

impl fmt::Display for RenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSampleRate => formatter.write_str("sample rate must be non-zero"),
            Self::NonFinitePosition { axis, value } => {
                write!(formatter, "position axis {axis} is non-finite: {value}")
            }
            Self::NonFiniteGain { value } => {
                write!(formatter, "source gain is non-finite: {value}")
            }
            Self::NonFiniteSourceSample { id, sample_index } => write!(
                formatter,
                "source {} contains a non-finite sample at index {sample_index}",
                id.0
            ),
            Self::UndefinedHorizontalDirection => {
                formatter.write_str("horizontal direction is undefined at x=0, y=0")
            }
            Self::RearHemisphereUnsupported { y } => {
                write!(
                    formatter,
                    "rear-hemisphere source is unsupported for stereo: y={y}"
                )
            }
            Self::DuplicateSourceId { id } => write!(formatter, "duplicate source ID {}", id.0),
            Self::StereoOutputLengthMismatch { left, right } => {
                write!(
                    formatter,
                    "stereo output lengths differ: left={left}, right={right}"
                )
            }
            Self::SourceBlockLengthMismatch {
                id,
                expected,
                actual,
            } => write!(
                formatter,
                "source {} has {actual} samples; expected {expected}",
                id.0
            ),
            Self::NonFiniteOutput {
                channel,
                sample_index,
            } => write!(
                formatter,
                "rendered {channel:?} output is non-finite at sample {sample_index}"
            ),
            Self::TooFewSpeakers { actual } => {
                write!(
                    formatter,
                    "a 2D layout needs at least two speakers; got {actual}"
                )
            }
            Self::UndefinedSpeakerDirection { id } => {
                write!(
                    formatter,
                    "speaker {} has an undefined horizontal direction",
                    id.0
                )
            }
            Self::Undefined3dSpeakerDirection => {
                formatter.write_str("3D speaker direction is undefined at zero length")
            }
            Self::DuplicateSpeakerId { id } => {
                write!(formatter, "duplicate speaker ID {}", id.0)
            }
            Self::TooFew3dSpeakers { actual } => {
                write!(
                    formatter,
                    "a 3D layout needs at least three speakers; got {actual}"
                )
            }
            Self::DuplicateSpeakerDirection { first, second } => write!(
                formatter,
                "3D speakers {first} and {second} have duplicate normalized directions"
            ),
            Self::DuplicateTripletSpeaker => {
                formatter.write_str("a 3D triplet must contain three distinct speaker IDs")
            }
            Self::MissingTripletSpeaker { id } => {
                write!(formatter, "3D triplet references missing speaker {}", id.0)
            }
            Self::DuplicateTriplet => {
                formatter.write_str("3D triplets must be unique ignoring declaration order")
            }
            Self::DegenerateTriplet { indices } => write!(
                formatter,
                "3D triplet [{}, {}, {}] has a singular speaker matrix",
                indices[0], indices[1], indices[2]
            ),
            Self::NoDeclared3dTriplet => {
                formatter.write_str("3D layout requires at least one declared triplet")
            }
            Self::Invalid3dGains => formatter.write_str("3D VBAP gains are invalid or degenerate"),
            Self::Ambiguous3dCoverage => {
                formatter.write_str("multiple declared 3D triplets produce different gains")
            }
            Self::Unsupported3dDirection { x, y, z } => write!(
                formatter,
                "explicit 3D speaker topology does not cover direction ({x}, {y}, {z})"
            ),
            Self::DuplicateSpeakerAzimuth { first, second } => write!(
                formatter,
                "speakers {first} and {second} have duplicate azimuths"
            ),
            Self::NoUsableSpeakerPair => {
                formatter.write_str("layout has no usable non-singular adjacent speaker pair")
            }
            Self::SingularSpeakerPair { first, second } => write!(
                formatter,
                "speaker pair {first}/{second} is singular or opposed"
            ),
            Self::InvalidPairGains => formatter.write_str("pair gains are invalid or degenerate"),
            Self::NegativePairGain => formatter.write_str("source lies outside the selected pair"),
            Self::AmbiguousSpeakerPair => {
                formatter.write_str("source direction matches multiple speaker sectors")
            }
            Self::UnsupportedDirection { angle } => {
                write!(formatter, "speaker layout does not cover azimuth {angle}")
            }
            Self::SpeakerOutputCountMismatch { expected, actual } => write!(
                formatter,
                "speaker output plane count differs: expected {expected}, got {actual}"
            ),
            Self::SpeakerOutputLengthMismatch {
                speaker_index,
                expected,
                actual,
            } => write!(
                formatter,
                "speaker output {speaker_index} has {actual} samples; expected {expected}"
            ),
            Self::EmptyTrajectory => formatter.write_str("trajectory must contain a segment"),
            Self::InvalidTrajectorySegment {
                start_sample,
                end_sample,
            } => write!(
                formatter,
                "trajectory segment must have end > start: {start_sample}..={end_sample}"
            ),
            Self::TrajectorySpanTooLarge { span } => write!(
                formatter,
                "trajectory span {span} exceeds exact f64 interpolation bound"
            ),
            Self::InvalidAzimuthPath => formatter.write_str("azimuth path is invalid"),
            Self::AmbiguousAntipodalPath => {
                formatter.write_str("shortest azimuth path is ambiguous for antipodal endpoints")
            }
            Self::NonContiguousTrajectory {
                previous_end,
                next_start,
            } => write!(
                formatter,
                "trajectory segments are not contiguous: previous end {previous_end}, next start {next_start}"
            ),
            Self::DiscontinuousTrajectory { boundary } => write!(
                formatter,
                "trajectory keyframe state is discontinuous at sample {boundary}"
            ),
            Self::TrajectorySampleOutOfRange { sample } => {
                write!(
                    formatter,
                    "trajectory sample {sample} is outside its domain"
                )
            }
            Self::TrajectoryBlockOutOfRange {
                start,
                length,
                domain_start,
                domain_end,
            } => write!(
                formatter,
                "trajectory block [{start}, {}) is outside inclusive domain [{domain_start}, {domain_end}]",
                start.saturating_add(*length as u64)
            ),
            Self::SampleIndexOverflow => formatter.write_str("sample-index arithmetic overflowed"),
        }
    }
}

impl std::error::Error for RenderError {}

/// Stateless, block-oriented FL/FR speaker renderer.
///
/// The renderer clears caller-provided output buffers for every call, mixes
/// all supplied sources linearly, and retains no duration-sized PCM or
/// time-varying state. Consequently a static scene is partition-invariant.
#[derive(Clone, Copy, Debug, Default)]
pub struct StereoRenderer;

impl StereoRenderer {
    /// Creates a stateless stereo renderer.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Renders one block of explicit mono sources into caller-owned FL/FR buffers.
    ///
    /// The output buffers are always cleared first on success. All source blocks
    /// must have exactly the output length; no truncation or padding occurs.
    /// Output is unclipped `f64` PCM and can exceed `[-1, 1]` when sources sum.
    pub fn render_block(
        &self,
        sources: &[ExplicitSpatialSource<'_>],
        left: &mut [f64],
        right: &mut [f64],
    ) -> Result<(), RenderError> {
        if left.len() != right.len() {
            return Err(RenderError::StereoOutputLengthMismatch {
                left: left.len(),
                right: right.len(),
            });
        }
        for source in sources {
            if source.samples.len() != left.len() {
                return Err(RenderError::SourceBlockLengthMismatch {
                    id: source.id,
                    expected: left.len(),
                    actual: source.samples.len(),
                });
            }
            // Constructors validate these fields, but keep the render boundary
            // explicit in case this type gains additional constructors later.
            validate_position(source.position)?;
            validate_gain(source.gain)?;
            StereoGains::for_position(source.position)?;
        }

        left.fill(0.0);
        right.fill(0.0);
        for source in sources {
            let gains = StereoGains::for_position(source.position)?;
            let left_gain = gains.left * source.gain;
            let right_gain = gains.right * source.gain;
            for (sample_index, &sample) in source.samples.iter().enumerate() {
                left[sample_index] += sample * left_gain;
                right[sample_index] += sample * right_gain;
                if !left[sample_index].is_finite() {
                    left.fill(0.0);
                    right.fill(0.0);
                    return Err(RenderError::NonFiniteOutput {
                        channel: OutputChannel::Left,
                        sample_index,
                    });
                }
                if !right[sample_index].is_finite() {
                    left.fill(0.0);
                    right.fill(0.0);
                    return Err(RenderError::NonFiniteOutput {
                        channel: OutputChannel::Right,
                        sample_index,
                    });
                }
            }
        }
        Ok(())
    }

    /// Renders a block from a borrowed explicit scene.
    pub fn render_scene_block(
        &self,
        scene: &ExplicitSpatialScene<'_>,
        left: &mut [f64],
        right: &mut [f64],
    ) -> Result<(), RenderError> {
        self.render_block(scene.sources, left, right)
    }

    /// Renders trajectory-bound sources using absolute sample indices.
    ///
    /// Every trajectory sample is preflighted through the front-horizontal
    /// stereo domain before output buffers are cleared. This prevents an
    /// unsupported rear-path sample from exposing a partially rendered block.
    pub fn render_trajectory_block(
        &self,
        sources: &[TrajectorySourceBlock<'_>],
        left: &mut [f64],
        right: &mut [f64],
    ) -> Result<(), RenderError> {
        if left.len() != right.len() {
            return Err(RenderError::StereoOutputLengthMismatch {
                left: left.len(),
                right: right.len(),
            });
        }
        validate_trajectory_sources(sources, left.len())?;
        for source in sources {
            for offset in 0..left.len() {
                let sample = source
                    .block_start_sample
                    .checked_add(offset as u64)
                    .ok_or(RenderError::SampleIndexOverflow)?;
                let state = source.trajectory.evaluate(sample)?;
                StereoGains::for_position(state.position)?;
                validate_gain(state.gain)?;
            }
        }
        left.fill(0.0);
        right.fill(0.0);
        for source in sources {
            for (offset, &sample_value) in source.samples.iter().enumerate() {
                let sample = source.block_start_sample + offset as u64;
                let state = source.trajectory.evaluate(sample)?;
                let gains = StereoGains::for_position(state.position)?;
                let left_gain = gains.left * state.gain;
                let right_gain = gains.right * state.gain;
                left[offset] += sample_value * left_gain;
                right[offset] += sample_value * right_gain;
                if !left[offset].is_finite() {
                    left.fill(0.0);
                    right.fill(0.0);
                    return Err(RenderError::NonFiniteOutput {
                        channel: OutputChannel::Left,
                        sample_index: offset,
                    });
                }
                if !right[offset].is_finite() {
                    left.fill(0.0);
                    right.fill(0.0);
                    return Err(RenderError::NonFiniteOutput {
                        channel: OutputChannel::Right,
                        sample_index: offset,
                    });
                }
            }
        }
        Ok(())
    }
}

fn validate_position(position: CartesianPosition) -> Result<(), RenderError> {
    for (axis, value) in [("x", position.x), ("y", position.y), ("z", position.z)] {
        if !value.is_finite() {
            return Err(RenderError::NonFinitePosition { axis, value });
        }
    }
    Ok(())
}

fn validate_gain(gain: f64) -> Result<(), RenderError> {
    if gain.is_finite() {
        Ok(())
    } else {
        Err(RenderError::NonFiniteGain { value: gain })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPSILON: f64 = 1.0e-12;

    fn source(
        id: u64,
        samples: &[f64],
        x: f64,
        y: f64,
        z: f64,
        gain: f64,
    ) -> ExplicitSpatialSource<'_> {
        ExplicitSpatialSource::new(
            SourceId::new(id),
            samples,
            CartesianPosition::new(x, y, z),
            gain,
        )
        .unwrap()
    }

    fn oracle_gains(x: f64, y: f64) -> (f64, f64) {
        let theta = x.atan2(y);
        let u = (theta + std::f64::consts::FRAC_PI_2) / std::f64::consts::PI;
        (
            (u * std::f64::consts::FRAC_PI_2).cos(),
            (u * std::f64::consts::FRAC_PI_2).sin(),
        )
    }

    fn assert_pair(actual: &[f64], expected: &[f64]) {
        assert_eq!(actual.len(), expected.len());
        for (actual, expected) in actual.iter().zip(expected) {
            assert!(
                (actual - expected).abs() <= EPSILON,
                "{actual} != {expected}"
            );
        }
    }

    #[test]
    fn hard_left_center_and_hard_right_match_independent_oracle() {
        let impulse = [1.0];
        for (id, x, y) in [(1, -1.0, 1.0), (2, 0.0, 1.0), (3, 1.0, 1.0)] {
            let source = source(id, &impulse, x, y, 0.0, 1.0);
            let mut left = [0.0];
            let mut right = [0.0];
            StereoRenderer::new()
                .render_block(&[source], &mut left, &mut right)
                .unwrap();
            let (expected_left, expected_right) = oracle_gains(x, y);
            assert_pair(&left, &[expected_left]);
            assert_pair(&right, &[expected_right]);
        }
    }

    #[test]
    fn intermediate_azimuth_uses_equal_power_oracle() {
        let samples = [1.0, -0.25, 0.5];
        let source = source(4, &samples, 1.0, 1.0, 0.0, 1.0);
        let mut left = [0.0; 3];
        let mut right = [0.0; 3];
        StereoRenderer::new()
            .render_block(&[source], &mut left, &mut right)
            .unwrap();
        let (left_gain, right_gain) = oracle_gains(1.0, 1.0);
        assert_pair(&left, &[left_gain, -0.25 * left_gain, 0.5 * left_gain]);
        assert_pair(&right, &[right_gain, -0.25 * right_gain, 0.5 * right_gain]);
    }

    #[test]
    fn center_sine_and_explicit_gain_are_linear() {
        let sine: Vec<_> = (0..32)
            .map(|index| (2.0 * std::f64::consts::PI * index as f64 / 32.0).sin())
            .collect();
        let source = source(5, &sine, 0.0, 1.0, 0.0, 0.25);
        let mut left = vec![0.0; sine.len()];
        let mut right = vec![0.0; sine.len()];
        StereoRenderer::new()
            .render_block(&[source], &mut left, &mut right)
            .unwrap();
        let gain = 1.0 / 2.0_f64.sqrt() * 0.25;
        for ((actual_left, actual_right), sample) in left.iter().zip(&right).zip(&sine) {
            assert!((actual_left - sample * gain).abs() <= EPSILON);
            assert!((actual_right - sample * gain).abs() <= EPSILON);
        }
    }

    #[test]
    fn two_sources_accumulate_without_clipping() {
        let first = source(6, &[0.8, 0.8], 0.0, 1.0, 0.0, 1.0);
        let second = source(7, &[0.8, 0.8], 0.0, 1.0, 0.0, 1.0);
        let mut left = [0.0; 2];
        let mut right = [0.0; 2];
        StereoRenderer::new()
            .render_block(&[first, second], &mut left, &mut right)
            .unwrap();
        let expected = 1.6 / 2.0_f64.sqrt();
        assert_pair(&left, &[expected, expected]);
        assert_pair(&right, &[expected, expected]);
    }

    #[test]
    fn energy_is_normalized_for_public_positions() {
        for (x, y) in [(-1.0, 1.0), (-0.5, 1.0), (0.0, 1.0), (0.5, 1.0), (1.0, 1.0)] {
            let gains = StereoGains::for_position(CartesianPosition::new(x, y, 0.0)).unwrap();
            assert!(
                (gains.left.mul_add(gains.left, gains.right * gains.right) - 1.0).abs() < EPSILON
            );
        }
    }

    #[test]
    fn partitioning_reuses_buffers_and_is_byte_identical() {
        let samples = [0.1, -0.2, 0.3, -0.4, 0.5, -0.6];
        let whole_source = source(8, &samples, 0.25, 1.0, 0.0, 0.75);
        let renderer = StereoRenderer::new();
        let mut whole_left = [0.0; 6];
        let mut whole_right = [0.0; 6];
        renderer
            .render_block(&[whole_source], &mut whole_left, &mut whole_right)
            .unwrap();

        let mut partitioned_left = [99.0; 6];
        let mut partitioned_right = [99.0; 6];
        let left_ptr = partitioned_left.as_ptr();
        let right_ptr = partitioned_right.as_ptr();
        for (start, end) in [(0, 2), (2, 5), (5, 6)] {
            let part = source(8, &samples[start..end], 0.25, 1.0, 0.0, 0.75);
            renderer
                .render_block(
                    &[part],
                    &mut partitioned_left[start..end],
                    &mut partitioned_right[start..end],
                )
                .unwrap();
        }
        assert_eq!(partitioned_left.as_ptr(), left_ptr);
        assert_eq!(partitioned_right.as_ptr(), right_ptr);
        assert_eq!(whole_left, partitioned_left);
        assert_eq!(whole_right, partitioned_right);
    }

    #[test]
    fn linearity_holds_for_source_separation() {
        let first_samples = [0.25, -0.5, 0.75];
        let second_samples = [0.5, 0.25, -0.125];
        let first = source(9, &first_samples, -0.5, 1.0, 0.0, 0.5);
        let second = source(10, &second_samples, 0.5, 1.0, 0.0, 0.75);
        let mut mixed_left = [0.0; 3];
        let mut mixed_right = [0.0; 3];
        StereoRenderer::new()
            .render_block(&[first, second], &mut mixed_left, &mut mixed_right)
            .unwrap();

        let mut first_left = [0.0; 3];
        let mut first_right = [0.0; 3];
        let mut second_left = [0.0; 3];
        let mut second_right = [0.0; 3];
        StereoRenderer::new()
            .render_block(&[first], &mut first_left, &mut first_right)
            .unwrap();
        StereoRenderer::new()
            .render_block(&[second], &mut second_left, &mut second_right)
            .unwrap();
        for index in 0..3 {
            assert_eq!(mixed_left[index], first_left[index] + second_left[index]);
            assert_eq!(mixed_right[index], first_right[index] + second_right[index]);
        }
    }

    #[test]
    fn valid_finite_input_produces_finite_output() {
        let source = source(11, &[f64::MAX / 4.0], 0.0, 1.0, 0.0, 0.5);
        let mut left = [0.0];
        let mut right = [0.0];
        StereoRenderer::new()
            .render_block(&[source], &mut left, &mut right)
            .unwrap();
        assert!(left[0].is_finite());
        assert!(right[0].is_finite());
    }

    #[test]
    fn invalid_coordinates_and_unsupported_rear_are_rejected() {
        let samples = [1.0];
        assert!(matches!(
            ExplicitSpatialSource::new(
                SourceId::new(12),
                &samples,
                CartesianPosition::new(f64::NAN, 1.0, 0.0),
                1.0,
            ),
            Err(RenderError::NonFinitePosition { axis: "x", .. })
        ));
        assert!(matches!(
            StereoGains::for_position(CartesianPosition::new(0.0, 0.0, 0.0)),
            Err(RenderError::UndefinedHorizontalDirection)
        ));
        assert!(matches!(
            StereoGains::for_position(CartesianPosition::new(0.0, -1.0, 0.0)),
            Err(RenderError::RearHemisphereUnsupported { .. })
        ));
    }

    #[test]
    fn buffer_and_source_length_contracts_reject_without_stale_output() {
        let source = source(13, &[1.0, 2.0], 0.0, 1.0, 0.0, 1.0);
        let mut left = [7.0; 1];
        let mut right = [8.0; 2];
        assert!(matches!(
            StereoRenderer::new().render_block(&[source], &mut left, &mut right),
            Err(RenderError::StereoOutputLengthMismatch { .. })
        ));
        let mut left = [7.0; 1];
        let mut right = [8.0; 1];
        assert!(matches!(
            StereoRenderer::new().render_block(&[source], &mut left, &mut right),
            Err(RenderError::SourceBlockLengthMismatch { .. })
        ));
        assert_eq!(left, [7.0]);
        assert_eq!(right, [8.0]);
    }

    #[test]
    fn explicit_scene_rejects_duplicate_ids_and_keeps_binding_explicit() {
        let samples = [0.0];
        let first = source(14, &samples, -1.0, 1.0, 0.0, 1.0);
        let second = source(14, &samples, 1.0, 1.0, 0.0, 1.0);
        assert!(matches!(
            ExplicitSpatialScene::new(48_000, &[first, second]),
            Err(RenderError::DuplicateSourceId { .. })
        ));
        let explicit_id = SourceId::new(15);
        let explicit = source(explicit_id.get(), &samples, 0.0, 1.0, 0.0, 1.0);
        assert_eq!(explicit.id(), explicit_id);
        // There is intentionally no constructor accepting decoder component
        // or ReconstructionBasis types; explicit source identity is required.
    }

    #[test]
    fn renderer_manifest_stays_independent_of_decoder_scene_crates() {
        let manifest = include_str!("../Cargo.toml");
        assert!(!manifest.contains("openjoc-scene"));
        assert!(!manifest.contains("openjoc-joc"));
        assert!(!manifest.contains("DecodedJocComponents"));
    }

    #[test]
    fn overflowing_finite_mix_is_rejected_without_nonfinite_output() {
        let source = source(16, &[f64::MAX], 0.0, 1.0, 0.0, 2.0);
        let mut left = [9.0];
        let mut right = [9.0];
        assert!(matches!(
            StereoRenderer::new().render_block(&[source], &mut left, &mut right),
            Err(RenderError::NonFiniteOutput { .. })
        ));
        assert_eq!(left, [0.0]);
        assert_eq!(right, [0.0]);
    }

    fn speaker(id: u64, x: f64, y: f64) -> Speaker2d {
        Speaker2d::new(SpeakerId::new(id), CartesianPosition::new(x, y, 0.0)).unwrap()
    }

    fn cardinal_layout() -> SpeakerLayout2d {
        SpeakerLayout2d::new(vec![
            speaker(10, 0.0, 1.0),
            speaker(20, 1.0, 0.0),
            speaker(30, 0.0, -1.0),
            speaker(40, -1.0, 0.0),
        ])
        .unwrap()
    }

    fn render_planes(
        renderer: &LayoutRenderer2d,
        sources: &[ExplicitSpatialSource<'_>],
        length: usize,
    ) -> Vec<Vec<f64>> {
        let mut planes = vec![vec![0.0; length]; renderer.layout().speaker_count()];
        {
            let mut outputs: Vec<&mut [f64]> = planes.iter_mut().map(Vec::as_mut_slice).collect();
            renderer.render_block(sources, &mut outputs).unwrap();
        }
        planes
    }

    fn expected_vbap(first: (f64, f64), second: (f64, f64), source: (f64, f64)) -> (f64, f64) {
        let determinant = first.0 * second.1 - second.0 * first.1;
        let raw_first = (source.0 * second.1 - second.0 * source.1) / determinant;
        let raw_second = (first.0 * source.1 - source.0 * first.1) / determinant;
        let norm = (raw_first * raw_first + raw_second * raw_second).sqrt();
        (raw_first / norm, raw_second / norm)
    }

    #[test]
    fn two_speaker_front_sector_matches_independent_vbap_oracle() {
        let layout =
            SpeakerLayout2d::new(vec![speaker(1, -1.0, 1.0), speaker(2, 1.0, 1.0)]).unwrap();
        let gains = layout
            .pair_gains(CartesianPosition::new(0.0, 1.0, 3.0))
            .unwrap();
        let expected = expected_vbap(
            (-1.0 / 2.0_f64.sqrt(), 1.0 / 2.0_f64.sqrt()),
            (1.0 / 2.0_f64.sqrt(), 1.0 / 2.0_f64.sqrt()),
            (0.0, 1.0),
        );
        assert_eq!(gains.pair().first_index(), 0);
        assert_eq!(gains.pair().second_index(), 1);
        assert!((gains.first() - expected.0).abs() <= EPSILON);
        assert!((gains.second() - expected.1).abs() <= EPSILON);
        assert!(
            (gains.first() * gains.first() + gains.second() * gains.second() - 1.0).abs()
                <= EPSILON
        );
    }

    #[test]
    fn public_output_order_survives_internal_azimuth_sort() {
        let layout = SpeakerLayout2d::new(vec![
            speaker(20, 1.0, 0.0),
            speaker(10, 0.0, 1.0),
            speaker(40, -1.0, 0.0),
            speaker(30, 0.0, -1.0),
        ])
        .unwrap();
        assert_eq!(layout.speakers()[0].id(), SpeakerId::new(20));
        assert_eq!(layout.speakers()[1].id(), SpeakerId::new(10));
        assert_eq!(layout.sorted_indices(), &[3, 2, 1, 0]);
        let source = source(50, &[1.0], 0.0, 1.0, 0.0, 1.0);
        let planes = render_planes(&LayoutRenderer2d::new(layout), &[source], 1);
        assert_eq!(planes, vec![vec![0.0], vec![1.0], vec![0.0], vec![0.0]]);
    }

    #[test]
    fn full_circle_layout_renders_side_rear_and_wrap_pair() {
        let renderer = LayoutRenderer2d::new(cardinal_layout());
        let side = source(51, &[1.0], 1.0, 0.0, 0.0, 1.0);
        let rear = source(52, &[1.0], 0.0, -1.0, 0.0, 1.0);
        let wrap = source(53, &[1.0], 1.0, -1.0, 0.0, 1.0);
        let side_planes = render_planes(&renderer, &[side], 1);
        assert_eq!(
            side_planes,
            vec![vec![0.0], vec![1.0], vec![0.0], vec![0.0]]
        );
        let rear_planes = render_planes(&renderer, &[rear], 1);
        assert_eq!(
            rear_planes,
            vec![vec![0.0], vec![0.0], vec![1.0], vec![0.0]]
        );
        let wrap_planes = render_planes(&renderer, &[wrap], 1);
        let half = 1.0 / 2.0_f64.sqrt();
        assert_eq!(wrap_planes[0], vec![0.0]);
        assert!((wrap_planes[1][0] - half).abs() <= EPSILON);
        assert!((wrap_planes[2][0] - half).abs() <= EPSILON);
        assert_eq!(wrap_planes[3], vec![0.0]);
    }

    #[test]
    fn partial_layout_rejects_uncovered_rear_direction() {
        let layout = SpeakerLayout2d::new(vec![
            speaker(60, -1.0, 1.0),
            speaker(61, 0.0, 1.0),
            speaker(62, 1.0, 1.0),
        ])
        .unwrap();
        assert!(matches!(
            layout.pair_gains(CartesianPosition::new(0.0, -1.0, 0.0)),
            Err(RenderError::UnsupportedDirection { .. })
        ));
    }

    #[test]
    fn irregular_five_speaker_layout_has_deterministic_angular_sweep() {
        let layout = SpeakerLayout2d::new(vec![
            speaker(70, 0.0, 1.0),
            speaker(71, 0.95, 0.31),
            speaker(72, 0.59, -0.81),
            speaker(73, -0.81, -0.59),
            speaker(74, -0.95, 0.31),
        ])
        .unwrap();
        for step in 0..720 {
            let theta = -std::f64::consts::PI + TWO_PI * step as f64 / 720.0;
            let position = CartesianPosition::new(theta.sin(), theta.cos(), 10.0);
            let first = layout.pair_gains(position).unwrap();
            let second = layout.pair_gains(position).unwrap();
            assert_eq!(first, second);
            assert!(first.first().is_finite() && first.second().is_finite());
            assert!(first.first() >= -EPSILON && first.second() >= -EPSILON);
            assert!(
                (first.first() * first.first() + first.second() * first.second() - 1.0).abs()
                    <= EPSILON
            );
        }
    }

    #[test]
    fn pair_boundary_is_continuous_at_shared_speaker() {
        let layout = cardinal_layout();
        let epsilon = 1.0e-8;
        for theta in [
            -std::f64::consts::FRAC_PI_2,
            0.0,
            std::f64::consts::FRAC_PI_2,
        ] {
            let before = layout
                .pair_gains(CartesianPosition::new(
                    (theta - epsilon).sin(),
                    (theta - epsilon).cos(),
                    0.0,
                ))
                .unwrap();
            let after = layout
                .pair_gains(CartesianPosition::new(
                    (theta + epsilon).sin(),
                    (theta + epsilon).cos(),
                    0.0,
                ))
                .unwrap();
            assert!(before.first().abs() < 1.0 || before.second().abs() < 1.0);
            assert!((before.first() - after.first()).abs() < 1.0);
        }
    }

    #[test]
    fn layout_validation_rejects_duplicate_ids_azimuths_and_opposed_only_layouts() {
        assert!(matches!(
            SpeakerLayout2d::new(vec![speaker(80, 0.0, 1.0), speaker(80, 1.0, 0.0)]),
            Err(RenderError::DuplicateSpeakerId { .. })
        ));
        assert!(matches!(
            SpeakerLayout2d::new(vec![speaker(81, 0.0, 1.0), speaker(82, 0.0, 2.0)]),
            Err(RenderError::DuplicateSpeakerAzimuth { .. })
        ));
        assert!(matches!(
            SpeakerLayout2d::new(vec![speaker(83, 0.0, 1.0), speaker(84, 0.0, -1.0)]),
            Err(RenderError::NoUsableSpeakerPair)
        ));
    }

    #[test]
    fn layout_block_mixes_multiple_pairs_and_is_partition_invariant() {
        let renderer = LayoutRenderer2d::new(cardinal_layout());
        let first_samples = [0.25, -0.5, 0.75, -1.0];
        let second_samples = [0.5, 0.25, -0.125, 0.25];
        let first = source(90, &first_samples, 0.0, 1.0, 0.0, 0.5);
        let second = source(91, &second_samples, 1.0, -1.0, 0.0, 0.75);
        let whole = render_planes(&renderer, &[first, second], 4);
        let mut partitioned = vec![vec![0.0; 4]; 4];
        for (start, end) in [(0, 1), (1, 3), (3, 4)] {
            let one = source(90, &first_samples[start..end], 0.0, 1.0, 0.0, 0.5);
            let two = source(91, &second_samples[start..end], 1.0, -1.0, 0.0, 0.75);
            let mut outputs: Vec<&mut [f64]> = partitioned
                .iter_mut()
                .map(|plane| &mut plane[start..end])
                .collect();
            renderer.render_block(&[one, two], &mut outputs).unwrap();
        }
        assert_eq!(whole, partitioned);
    }

    #[test]
    fn layout_output_contract_rejects_wrong_planes_without_mutation() {
        let renderer = LayoutRenderer2d::new(cardinal_layout());
        let source = source(92, &[1.0, 2.0], 0.0, 1.0, 0.0, 1.0);
        let mut too_few = vec![vec![7.0; 2]; 3];
        let mut refs: Vec<&mut [f64]> = too_few.iter_mut().map(Vec::as_mut_slice).collect();
        assert!(matches!(
            renderer.render_block(&[source], &mut refs),
            Err(RenderError::SpeakerOutputCountMismatch { .. })
        ));
        assert_eq!(too_few, vec![vec![7.0; 2]; 3]);

        let mut wrong_lengths = [vec![7.0; 2], vec![8.0; 1], vec![9.0; 2], vec![10.0; 2]];
        let mut refs: Vec<&mut [f64]> = wrong_lengths.iter_mut().map(Vec::as_mut_slice).collect();
        assert!(matches!(
            renderer.render_block(&[source], &mut refs),
            Err(RenderError::SpeakerOutputLengthMismatch { .. })
        ));
        assert_eq!(wrong_lengths[1], vec![8.0]);
    }

    #[test]
    fn two_dimensional_renderer_has_no_decoder_or_object_bridge() {
        let manifest = include_str!("../Cargo.toml");
        assert!(!manifest.contains("openjoc-eac3"));
        assert!(!manifest.contains("DecodedJocComponents"));
        assert!(Speaker2d::new(SpeakerId::new(93), CartesianPosition::new(0.0, 1.0, 9.0)).is_ok());
    }

    fn state(x: f64, y: f64, z: f64, gain: f64) -> SpatialState2d {
        SpatialState2d::new(CartesianPosition::new(x, y, z), gain).unwrap()
    }

    fn segment(
        start: u64,
        end: u64,
        start_state: SpatialState2d,
        end_state: SpatialState2d,
        path: AzimuthPath2d,
    ) -> TrajectorySegment2d {
        TrajectorySegment2d::new(start, end, start_state, end_state, path).unwrap()
    }

    fn render_stereo_trajectory(
        renderer: StereoRenderer,
        trajectory: &SourceTrajectory2d,
        samples: &[f64],
        start: u64,
    ) -> (Vec<f64>, Vec<f64>) {
        let mut left = vec![0.0; samples.len()];
        let mut right = vec![0.0; samples.len()];
        let source =
            TrajectorySourceBlock::new(SourceId::new(100), samples, trajectory, start).unwrap();
        renderer
            .render_trajectory_block(&[source], &mut left, &mut right)
            .unwrap();
        (left, right)
    }

    #[test]
    fn trajectory_evaluates_exact_endpoints_and_linear_gain_on_absolute_timeline() {
        let trajectory = SourceTrajectory2d::new(vec![segment(
            10,
            20,
            state(-1.0, 1.0, 2.0, 0.25),
            state(1.0, 1.0, 4.0, 0.75),
            AzimuthPath2d::Shortest,
        )])
        .unwrap();
        assert_eq!(
            trajectory.evaluate(10).unwrap(),
            state(-1.0, 1.0, 2.0, 0.25)
        );
        assert_eq!(trajectory.evaluate(20).unwrap(), state(1.0, 1.0, 4.0, 0.75));
        let middle = trajectory.evaluate(15).unwrap();
        assert!((middle.position().x - 0.0).abs() < EPSILON);
        assert!((middle.position().y - 1.0).abs() < EPSILON);
        assert!((middle.position().z - 3.0).abs() < EPSILON);
        assert!((middle.gain() - 0.5).abs() < EPSILON);
    }

    #[test]
    fn trajectory_render_matches_independent_absolute_sample_oracle() {
        let trajectory = SourceTrajectory2d::new(vec![segment(
            10,
            14,
            state(-1.0, 1.0, 0.0, 0.5),
            state(1.0, 1.0, 0.0, 1.0),
            AzimuthPath2d::Shortest,
        )])
        .unwrap();
        let samples = [1.0; 5];
        let (left, right) =
            render_stereo_trajectory(StereoRenderer::new(), &trajectory, &samples, 10);
        let start_theta = (-1.0_f64).atan2(1.0);
        let end_theta = 1.0_f64.atan2(1.0);
        let delta = end_theta - start_theta;
        for (offset, (&actual_left, &actual_right)) in left.iter().zip(&right).enumerate() {
            let t = offset as f64 / 4.0;
            let theta = start_theta + t * delta;
            let position_u = (theta + std::f64::consts::FRAC_PI_2) / std::f64::consts::PI;
            let expected_left = (position_u * std::f64::consts::FRAC_PI_2).cos();
            let expected_right = (position_u * std::f64::consts::FRAC_PI_2).sin();
            let expected_gain = 0.5 + t * 0.5;
            assert!((actual_left - expected_left * expected_gain).abs() < EPSILON);
            assert!((actual_right - expected_right * expected_gain).abs() < EPSILON);
        }
    }

    #[test]
    fn trajectory_validation_rejects_gaps_overlaps_discontinuities_and_bad_spans() {
        let first = segment(
            0,
            3,
            state(0.0, 1.0, 0.0, 1.0),
            state(1.0, 0.0, 0.0, 1.0),
            AzimuthPath2d::Increasing,
        );
        let mismatched = segment(
            3,
            5,
            state(-1.0, 0.0, 0.0, 1.0),
            state(0.0, -1.0, 0.0, 1.0),
            AzimuthPath2d::Increasing,
        );
        assert!(matches!(
            SourceTrajectory2d::new(vec![first, mismatched]),
            Err(RenderError::DiscontinuousTrajectory { boundary: 3 })
        ));
        let gap = segment(
            4,
            5,
            state(1.0, 0.0, 0.0, 1.0),
            state(0.0, -1.0, 0.0, 1.0),
            AzimuthPath2d::Increasing,
        );
        assert!(matches!(
            SourceTrajectory2d::new(vec![first, gap]),
            Err(RenderError::NonContiguousTrajectory { .. })
        ));
        assert!(matches!(
            TrajectorySegment2d::new(
                0,
                MAX_EXACT_INTERPOLATION_SPAN + 1,
                state(0.0, 1.0, 0.0, 1.0),
                state(1.0, 0.0, 0.0, 1.0),
                AzimuthPath2d::Increasing,
            ),
            Err(RenderError::TrajectorySpanTooLarge { .. })
        ));
    }

    #[test]
    fn shortest_antipodal_is_rejected_and_explicit_stereo_paths_are_deterministic() {
        let left = state(-1.0, 0.0, 0.0, 1.0);
        let right = state(1.0, 0.0, 0.0, 1.0);
        assert!(matches!(
            TrajectorySegment2d::new(0, 4, left, right, AzimuthPath2d::Shortest),
            Err(RenderError::AmbiguousAntipodalPath)
        ));
        let front_path =
            SourceTrajectory2d::new(vec![segment(0, 4, left, right, AzimuthPath2d::Increasing)])
                .unwrap();
        let rear_path =
            SourceTrajectory2d::new(vec![segment(0, 4, left, right, AzimuthPath2d::Decreasing)])
                .unwrap();
        let samples = [1.0; 5];
        let (front_left, front_right) =
            render_stereo_trajectory(StereoRenderer::new(), &front_path, &samples, 0);
        assert!((front_left[2] - std::f64::consts::FRAC_1_SQRT_2).abs() < EPSILON);
        assert!((front_right[2] - std::f64::consts::FRAC_1_SQRT_2).abs() < EPSILON);
        let source =
            TrajectorySourceBlock::new(SourceId::new(101), &samples, &rear_path, 0).unwrap();
        let mut left_out = [9.0; 5];
        let mut right_out = [8.0; 5];
        assert!(matches!(
            StereoRenderer::new().render_trajectory_block(&[source], &mut left_out, &mut right_out),
            Err(RenderError::RearHemisphereUnsupported { .. })
        ));
        assert_eq!(left_out, [9.0; 5]);
        assert_eq!(right_out, [8.0; 5]);
    }

    #[test]
    fn stereo_trajectory_is_byte_identical_across_absolute_block_partitions() {
        let trajectory = SourceTrajectory2d::new(vec![segment(
            0,
            15,
            state(-1.0, 1.0, 0.0, 0.25),
            state(1.0, 1.0, 0.0, 0.75),
            AzimuthPath2d::Shortest,
        )])
        .unwrap();
        let samples: Vec<f64> = (0..16).map(|i| (i as f64 - 4.0) / 7.0).collect();
        let whole = render_stereo_trajectory(StereoRenderer::new(), &trajectory, &samples, 0);
        let mut left = vec![0.0; samples.len()];
        let mut right = vec![0.0; samples.len()];
        for (start, end) in [(0_usize, 1_usize), (1, 5), (5, 6), (6, 11), (11, 16)] {
            let source = TrajectorySourceBlock::new(
                SourceId::new(100),
                &samples[start..end],
                &trajectory,
                start as u64,
            )
            .unwrap();
            StereoRenderer::new()
                .render_trajectory_block(&[source], &mut left[start..end], &mut right[start..end])
                .unwrap();
        }
        assert_eq!(left, whole.0);
        assert_eq!(right, whole.1);
    }

    #[test]
    fn layout_trajectory_supports_multiple_sources_and_partition_invariance() {
        let renderer = LayoutRenderer2d::new(cardinal_layout());
        let first_trajectory = SourceTrajectory2d::new(vec![segment(
            0,
            7,
            state(0.0, 1.0, 0.0, 0.5),
            state(1.0, 0.0, 0.0, 1.0),
            AzimuthPath2d::Increasing,
        )])
        .unwrap();
        let second_trajectory = SourceTrajectory2d::new(vec![segment(
            0,
            7,
            state(-1.0, 0.0, 0.0, 0.75),
            state(0.0, -1.0, 0.0, 0.25),
            AzimuthPath2d::Decreasing,
        )])
        .unwrap();
        let first_samples = [1.0, -0.5, 0.25, 0.75, -0.25, 0.5, 1.0, -1.0];
        let second_samples = [0.5; 8];
        let mut whole = vec![vec![0.0; 8]; 4];
        let first =
            TrajectorySourceBlock::new(SourceId::new(110), &first_samples, &first_trajectory, 0)
                .unwrap();
        let second =
            TrajectorySourceBlock::new(SourceId::new(111), &second_samples, &second_trajectory, 0)
                .unwrap();
        let mut whole_refs: Vec<&mut [f64]> = whole.iter_mut().map(Vec::as_mut_slice).collect();
        renderer
            .render_trajectory_block(&[first, second], &mut whole_refs)
            .unwrap();
        let mut partitioned = vec![vec![0.0; 8]; 4];
        for (start, end) in [(0_usize, 2_usize), (2, 3), (3, 8)] {
            let first = TrajectorySourceBlock::new(
                SourceId::new(110),
                &first_samples[start..end],
                &first_trajectory,
                start as u64,
            )
            .unwrap();
            let second = TrajectorySourceBlock::new(
                SourceId::new(111),
                &second_samples[start..end],
                &second_trajectory,
                start as u64,
            )
            .unwrap();
            let mut outputs: Vec<&mut [f64]> = partitioned
                .iter_mut()
                .map(|plane| &mut plane[start..end])
                .collect();
            renderer
                .render_trajectory_block(&[first, second], &mut outputs)
                .unwrap();
        }
        assert_eq!(partitioned, whole);
    }

    #[test]
    fn static_trajectory_is_identical_to_static_renderers() {
        let samples = [0.25, -0.5, 0.75];
        let static_source = source(120, &samples, 0.5, 1.0, 0.0, 0.8);
        let trajectory = SourceTrajectory2d::new(vec![segment(
            0,
            2,
            state(0.5, 1.0, 0.0, 0.8),
            state(0.5, 1.0, 0.0, 0.8),
            AzimuthPath2d::Shortest,
        )])
        .unwrap();
        let dynamic_source =
            TrajectorySourceBlock::new(SourceId::new(120), &samples, &trajectory, 0).unwrap();
        let mut static_left = [0.0; 3];
        let mut static_right = [0.0; 3];
        StereoRenderer::new()
            .render_block(&[static_source], &mut static_left, &mut static_right)
            .unwrap();
        let mut dynamic_left = [0.0; 3];
        let mut dynamic_right = [0.0; 3];
        StereoRenderer::new()
            .render_trajectory_block(&[dynamic_source], &mut dynamic_left, &mut dynamic_right)
            .unwrap();
        assert_eq!(static_left, dynamic_left);
        assert_eq!(static_right, dynamic_right);

        let layout = LayoutRenderer2d::new(cardinal_layout());
        let static_planes = render_planes(&layout, &[static_source], 3);
        let mut dynamic_planes = vec![vec![0.0; 3]; 4];
        let mut outputs: Vec<&mut [f64]> =
            dynamic_planes.iter_mut().map(Vec::as_mut_slice).collect();
        layout
            .render_trajectory_block(&[dynamic_source], &mut outputs)
            .unwrap();
        assert_eq!(static_planes, dynamic_planes);
    }

    #[test]
    fn trajectory_blocks_reject_duplicate_ids_and_out_of_range_without_mutation() {
        let trajectory = SourceTrajectory2d::new(vec![segment(
            5,
            7,
            state(0.0, 1.0, 0.0, 1.0),
            state(0.0, 1.0, 0.0, 1.0),
            AzimuthPath2d::Shortest,
        )])
        .unwrap();
        assert!(matches!(
            TrajectorySourceBlock::new(SourceId::new(122), &[1.0], &trajectory, 4),
            Err(RenderError::TrajectoryBlockOutOfRange { .. })
        ));
        let samples = [1.0, 1.0];
        let first =
            TrajectorySourceBlock::new(SourceId::new(123), &samples, &trajectory, 5).unwrap();
        let second =
            TrajectorySourceBlock::new(SourceId::new(123), &samples, &trajectory, 5).unwrap();
        let mut left = [7.0; 2];
        let mut right = [8.0; 2];
        assert!(matches!(
            StereoRenderer::new().render_trajectory_block(&[first, second], &mut left, &mut right),
            Err(RenderError::DuplicateSourceId { .. })
        ));
        assert_eq!(left, [7.0; 2]);
        assert_eq!(right, [8.0; 2]);
    }

    fn speaker3(id: u64, x: f64, y: f64, z: f64) -> Speaker3d {
        Speaker3d::new(SpeakerId::new(id), CartesianPosition::new(x, y, z)).unwrap()
    }

    fn triplet(first: u64, second: u64, third: u64) -> SpeakerTriplet3d {
        SpeakerTriplet3d::new(
            SpeakerId::new(first),
            SpeakerId::new(second),
            SpeakerId::new(third),
        )
        .unwrap()
    }

    fn octahedron_layout() -> SpeakerLayout3d {
        SpeakerLayout3d::new(
            vec![
                speaker3(200, 1.0, 0.0, 0.0),
                speaker3(201, -1.0, 0.0, 0.0),
                speaker3(202, 0.0, 1.0, 0.0),
                speaker3(203, 0.0, -1.0, 0.0),
                speaker3(204, 0.0, 0.0, 1.0),
                speaker3(205, 0.0, 0.0, -1.0),
            ],
            vec![
                triplet(200, 202, 204),
                triplet(200, 204, 203),
                triplet(200, 203, 205),
                triplet(200, 205, 202),
                triplet(201, 204, 202),
                triplet(201, 203, 204),
                triplet(201, 205, 203),
                triplet(201, 202, 205),
            ],
        )
        .unwrap()
    }

    fn render_planes3(
        renderer: &LayoutRenderer3d,
        sources: &[ExplicitSpatialSource<'_>],
        length: usize,
    ) -> Vec<Vec<f64>> {
        let mut planes = vec![vec![0.0; length]; renderer.layout().speaker_count()];
        let mut outputs: Vec<&mut [f64]> = planes.iter_mut().map(Vec::as_mut_slice).collect();
        renderer.render_block(sources, &mut outputs).unwrap();
        planes
    }

    fn unit_direction(x: f64, y: f64, z: f64) -> CartesianPosition {
        let norm = (x * x + y * y + z * z).sqrt();
        CartesianPosition::new(x / norm, y / norm, z / norm)
    }

    fn sum_positions(
        first: CartesianPosition,
        second: CartesianPosition,
        third: CartesianPosition,
    ) -> CartesianPosition {
        CartesianPosition::new(
            first.x + second.x + third.x,
            first.y + second.y + third.y,
            first.z + second.z + third.z,
        )
    }

    fn full_gains(layout: &SpeakerLayout3d, position: CartesianPosition) -> Vec<f64> {
        let mut result = vec![0.0; layout.speaker_count()];
        layout
            .gains(position)
            .unwrap()
            .write_full_gains(&mut result)
            .unwrap();
        result
    }

    #[test]
    fn direct_3d_matrix_oracle_solves_identity_without_renderer() {
        let result = solve_3x3_for(
            [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            [0.25, -0.5, 0.75],
        )
        .unwrap();
        assert_eq!(result, [0.25, -0.5, 0.75]);
    }

    #[test]
    fn vbap_3d_matches_independent_axis_oracle_and_normalizes_energy() {
        let layout = SpeakerLayout3d::new(
            vec![
                speaker3(210, 1.0, 0.0, 0.0),
                speaker3(211, 0.0, 1.0, 0.0),
                speaker3(212, 0.0, 0.0, 1.0),
            ],
            vec![triplet(210, 211, 212)],
        )
        .unwrap();
        let gains = layout.gains(CartesianPosition::new(1.0, 2.0, 3.0)).unwrap();
        let norm = 14.0_f64.sqrt();
        assert_eq!(gains.triplet(), Some(triplet(210, 211, 212)));
        assert!((gains.first() - 1.0 / norm).abs() <= EPSILON);
        assert!((gains.second() - 2.0 / norm).abs() <= EPSILON);
        assert!((gains.third() - 3.0 / norm).abs() <= EPSILON);
        assert!((gains.gains().iter().map(|gain| gain * gain).sum::<f64>() - 1.0).abs() <= EPSILON);
        let mut full = [99.0; 3];
        gains.write_full_gains(&mut full).unwrap();
        assert_eq!(full, [gains.first(), gains.second(), gains.third()]);

        let reordered = SpeakerLayout3d::new(
            vec![
                speaker3(212, 0.0, 0.0, 1.0),
                speaker3(210, 1.0, 0.0, 0.0),
                speaker3(211, 0.0, 1.0, 0.0),
            ],
            vec![triplet(212, 210, 211)],
        )
        .unwrap();
        let reordered_gains = reordered
            .gains(CartesianPosition::new(1.0, 2.0, 3.0))
            .unwrap();
        let mut reordered_full = [0.0; 3];
        reordered_gains
            .write_full_gains(&mut reordered_full)
            .unwrap();
        assert_eq!(
            reordered_full,
            [gains.third(), gains.first(), gains.second()]
        );
    }

    #[test]
    fn exact_cardinal_3d_hits_are_deterministic_one_hot_gains() {
        let layout = octahedron_layout();
        for (index, position) in [
            CartesianPosition::new(1.0, 0.0, 0.0),
            CartesianPosition::new(-1.0, 0.0, 0.0),
            CartesianPosition::new(0.0, 1.0, 0.0),
            CartesianPosition::new(0.0, -1.0, 0.0),
            CartesianPosition::new(0.0, 0.0, 1.0),
            CartesianPosition::new(0.0, 0.0, -1.0),
        ]
        .into_iter()
        .enumerate()
        {
            let gains = layout.gains(position).unwrap();
            let mut full = [0.0; 6];
            gains.write_full_gains(&mut full).unwrap();
            assert_eq!(full[index], 1.0);
            assert_eq!(full.iter().filter(|value| **value != 0.0).count(), 1);
        }
    }

    #[test]
    fn tetrahedron_faces_and_shared_edges_are_continuous() {
        let scale = 3.0_f64.sqrt();
        let layout = SpeakerLayout3d::new(
            vec![
                speaker3(220, 1.0, 1.0, 1.0),
                speaker3(221, -1.0, -1.0, 1.0),
                speaker3(222, -1.0, 1.0, -1.0),
                speaker3(223, 1.0, -1.0, -1.0),
            ],
            vec![
                triplet(220, 221, 222),
                triplet(220, 223, 221),
                triplet(220, 222, 223),
                triplet(221, 223, 222),
            ],
        )
        .unwrap();
        for position in [
            CartesianPosition::new(-1.0, 1.0, 1.0),
            CartesianPosition::new(1.0, -1.0, 1.0),
            CartesianPosition::new(-1.0, -1.0, -1.0),
            CartesianPosition::new(1.0, 1.0, -1.0),
        ] {
            let gains = layout.gains(position).unwrap();
            assert!(
                gains
                    .gains()
                    .iter()
                    .all(|gain| (*gain - 1.0 / scale).abs() <= EPSILON)
            );
        }
        let edge = layout.gains(CartesianPosition::new(0.0, 0.0, 1.0)).unwrap();
        assert!((edge.first() - 2.0_f64.sqrt().recip()).abs() <= EPSILON);
        assert!((edge.second() - 2.0_f64.sqrt().recip()).abs() <= EPSILON);
        assert!(edge.third().abs() <= EPSILON);
    }

    #[test]
    fn irregular_closed_topology_and_dense_face_sweep_are_deterministic() {
        let a = unit_direction(1.0, 0.2, 0.3);
        let b = unit_direction(-0.4, 1.0, 0.1);
        let c = unit_direction(0.2, -0.3, 1.0);
        let d = unit_direction(-0.8, -0.6, -0.7);
        let layout = SpeakerLayout3d::new(
            vec![
                speaker3(224, a.x, a.y, a.z),
                speaker3(225, b.x, b.y, b.z),
                speaker3(226, c.x, c.y, c.z),
                speaker3(227, d.x, d.y, d.z),
            ],
            vec![
                triplet(224, 225, 226),
                triplet(224, 227, 225),
                triplet(224, 226, 227),
                triplet(225, 227, 226),
            ],
        )
        .unwrap();
        for (first, second, third) in [(a, b, c), (a, d, b), (a, c, d), (b, d, c)] {
            let gains = layout.gains(sum_positions(first, second, third)).unwrap();
            assert!(
                gains
                    .gains()
                    .iter()
                    .all(|gain| (*gain - 1.0 / 3.0_f64.sqrt()).abs() <= EPSILON)
            );
        }
        let faces = [(a, b, c), (a, d, b), (a, c, d), (b, d, c)];
        let mut samples = 0;
        for (first, second, third) in faces {
            for i in 1..=8 {
                for j in 1..=8 - i {
                    let k = 9 - i - j;
                    let position = CartesianPosition::new(
                        first.x * i as f64 + second.x * j as f64 + third.x * k as f64,
                        first.y * i as f64 + second.y * j as f64 + third.y * k as f64,
                        first.z * i as f64 + second.z * j as f64 + third.z * k as f64,
                    );
                    let gains = layout.gains(position).unwrap();
                    assert!(
                        gains
                            .gains()
                            .iter()
                            .all(|gain| gain.is_finite() && *gain >= 0.0)
                    );
                    assert!(
                        (gains.gains().iter().map(|gain| gain * gain).sum::<f64>() - 1.0).abs()
                            <= EPSILON
                    );
                    samples += 1;
                }
            }
        }
        assert_eq!(samples, 112);
    }

    #[test]
    fn vertex_approaches_converge_to_one_hot_exact_speaker() {
        let layout = octahedron_layout();
        for position in [
            CartesianPosition::new(1.0, 1.0e-6, 1.0e-6),
            CartesianPosition::new(1.0, 1.0e-6, -1.0e-6),
            CartesianPosition::new(1.0, -1.0e-6, 1.0e-6),
            CartesianPosition::new(1.0, -1.0e-6, -1.0e-6),
        ] {
            let gains = full_gains(&layout, position);
            assert!(gains[0] > 0.999999);
            assert!(gains[1..].iter().all(|gain| *gain < 0.001));
        }
    }

    #[test]
    fn partial_topology_rejects_uncovered_direction_and_overlap_is_ambiguous() {
        let partial = SpeakerLayout3d::new(
            vec![
                speaker3(230, 1.0, 0.0, 0.0),
                speaker3(231, 0.0, 1.0, 0.0),
                speaker3(232, 0.0, 0.0, 1.0),
            ],
            vec![triplet(230, 231, 232)],
        )
        .unwrap();
        assert!(matches!(
            partial.gains(CartesianPosition::new(-1.0, 0.0, 0.0)),
            Err(RenderError::Unsupported3dDirection { .. })
        ));

        let ambiguous = SpeakerLayout3d::new(
            vec![
                speaker3(240, 1.0, 0.0, 0.0),
                speaker3(241, 0.0, 1.0, 0.0),
                speaker3(242, 0.0, 0.0, 1.0),
                speaker3(243, 0.0, 1.0, 1.0),
            ],
            vec![triplet(240, 241, 242), triplet(240, 241, 243)],
        )
        .unwrap();
        assert!(matches!(
            ambiguous.gains(CartesianPosition::new(0.2, 1.0, 0.5)),
            Err(RenderError::Ambiguous3dCoverage)
        ));
    }

    #[test]
    fn layout_3d_validation_rejects_implicit_or_degenerate_topology() {
        assert!(matches!(
            Speaker3d::new(SpeakerId::new(250), CartesianPosition::new(0.0, 0.0, 0.0)),
            Err(RenderError::Undefined3dSpeakerDirection)
        ));
        assert!(matches!(
            SpeakerLayout3d::new(
                vec![speaker3(251, 1.0, 0.0, 0.0), speaker3(252, 0.0, 1.0, 0.0)],
                vec![]
            ),
            Err(RenderError::TooFew3dSpeakers { .. })
        ));
        assert!(matches!(
            SpeakerTriplet3d::new(
                SpeakerId::new(253),
                SpeakerId::new(253),
                SpeakerId::new(254)
            ),
            Err(RenderError::DuplicateTripletSpeaker)
        ));
        let duplicate_direction = SpeakerLayout3d::new(
            vec![
                speaker3(255, 1.0, 0.0, 0.0),
                speaker3(256, 2.0, 0.0, 0.0),
                speaker3(257, 0.0, 1.0, 0.0),
            ],
            vec![triplet(255, 256, 257)],
        );
        assert!(matches!(
            duplicate_direction,
            Err(RenderError::DuplicateSpeakerDirection { .. })
        ));
        let degenerate = SpeakerLayout3d::new(
            vec![
                speaker3(258, 1.0, 0.0, 0.0),
                speaker3(259, 0.0, 1.0, 0.0),
                speaker3(260, 1.0, 1.0, 0.0),
            ],
            vec![triplet(258, 259, 260)],
        );
        assert!(matches!(
            degenerate,
            Err(RenderError::DegenerateTriplet { .. })
        ));
    }

    #[test]
    fn renderer_3d_is_linear_partition_invariant_and_atomic() {
        let renderer = LayoutRenderer3d::new(
            SpeakerLayout3d::new(
                vec![
                    speaker3(270, 1.0, 0.0, 0.0),
                    speaker3(271, 0.0, 1.0, 0.0),
                    speaker3(272, 0.0, 0.0, 1.0),
                ],
                vec![triplet(270, 271, 272)],
            )
            .unwrap(),
        );
        let samples = [0.25, -0.5, 0.75, -1.0];
        let full_source = source(273, &samples, 1.0, 2.0, 3.0, 0.5);
        let whole = render_planes3(&renderer, &[full_source], samples.len());
        let mut partitioned = vec![vec![0.0; samples.len()]; 3];
        for (start, end) in [(0, 1), (1, 3), (3, 4)] {
            let part = source(273, &samples[start..end], 1.0, 2.0, 3.0, 0.5);
            let mut outputs: Vec<&mut [f64]> = partitioned
                .iter_mut()
                .map(|plane| &mut plane[start..end])
                .collect();
            renderer.render_block(&[part], &mut outputs).unwrap();
        }
        assert_eq!(whole, partitioned);

        let invalid = source(274, &[1.0], -1.0, 0.0, 0.0, 1.0);
        let mut stale = vec![vec![7.0; 1]; 3];
        let mut outputs: Vec<&mut [f64]> = stale.iter_mut().map(Vec::as_mut_slice).collect();
        assert!(matches!(
            renderer.render_block(&[invalid], &mut outputs),
            Err(RenderError::Unsupported3dDirection { .. })
        ));
        assert_eq!(stale, vec![vec![7.0; 1]; 3]);
    }

    #[test]
    fn renderer_3d_has_no_decoder_or_semantic_bridge() {
        let manifest = include_str!("../Cargo.toml");
        assert!(!manifest.contains("openjoc-scene"));
        assert!(!manifest.contains("openjoc-joc"));
        assert!(!manifest.contains("DecodedJocComponents"));
        assert!(Speaker3d::new(SpeakerId::new(280), CartesianPosition::new(0.0, 0.0, 1.0)).is_ok());
    }
}
