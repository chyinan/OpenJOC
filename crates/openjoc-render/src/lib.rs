//! Explicit-scene speaker rendering for OpenJOC.
//!
//! This crate deliberately accepts only caller-supplied [`ExplicitSpatialSource`]
//! values. It has no dependency on the decoder or metadata scene crates, so a
//! `ReconstructionBasis` row cannot be silently promoted to an authored object
//! source while [`SemanticBindingState`](https://docs.rs/openjoc-scene) remains
//! unresolved.
//!
//! J5R1 implements a front-horizontal, equal-power FL/FR panner. Elevation,
//! distance, room acoustics, occlusion, HRTF, and JOC semantic binding are
//! explicit non-features of this initial foundation.

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
}
