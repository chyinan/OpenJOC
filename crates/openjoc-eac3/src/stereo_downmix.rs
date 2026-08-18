//! ETSI TS 102 366 clause 6.8 stereo downmix matrices.
//!
//! The matrix is kept in the E-AC-3 crate so every renderer uses one canonical
//! implementation.  It deliberately separates unscaled public equations from
//! the uniform overload-protection scale applied to the complete matrix.

use crate::{ChannelLocation, DecodedAccessUnitPcm, DownmixMetadata};
use std::fmt;

/// Requested 2.0 channel downmix policy.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum StereoDownmixMode {
    /// Select Lt/Rt for `dmixmod == 01`, otherwise Lo/Ro.
    #[default]
    Auto,
    /// Conventional stereo downmix.
    LoRo,
    /// Matrix-surround stereo downmix.
    LtRt,
}

impl StereoDownmixMode {
    /// Stable public spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::LoRo => "loro",
            Self::LtRt => "ltrt",
        }
    }
}

/// One Base-channel row in a stereo downmix matrix.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StereoDownmixRow {
    location: ChannelLocation,
    unscaled_left: f64,
    unscaled_right: f64,
    left: f64,
    right: f64,
}

impl StereoDownmixRow {
    /// Base channel location for this row.
    #[must_use]
    pub const fn location(self) -> ChannelLocation {
        self.location
    }

    /// Unscaled left-output coefficient from the ETSI equation.
    #[must_use]
    pub const fn unscaled_left(self) -> f64 {
        self.unscaled_left
    }

    /// Unscaled right-output coefficient from the ETSI equation.
    #[must_use]
    pub const fn unscaled_right(self) -> f64 {
        self.unscaled_right
    }

    /// Final left-output coefficient after uniform overload protection.
    #[must_use]
    pub const fn left(self) -> f64 {
        self.left
    }

    /// Final right-output coefficient after uniform overload protection.
    #[must_use]
    pub const fn right(self) -> f64 {
        self.right
    }

    /// Absolute sum of this row's final coefficients.
    #[must_use]
    pub fn absolute_coefficient_sum(self) -> f64 {
        self.left.abs() + self.right.abs()
    }

    /// Absolute sum of this row's unscaled coefficients.
    #[must_use]
    pub fn unscaled_absolute_coefficient_sum(self) -> f64 {
        self.unscaled_left.abs() + self.unscaled_right.abs()
    }

    /// Squared sum of this row's final coefficients.
    #[must_use]
    pub fn squared_coefficient_sum(self) -> f64 {
        self.left.mul_add(self.left, self.right * self.right)
    }

    /// Squared sum of this row's unscaled coefficients.
    #[must_use]
    pub fn unscaled_squared_coefficient_sum(self) -> f64 {
        self.unscaled_left.mul_add(
            self.unscaled_left,
            self.unscaled_right * self.unscaled_right,
        )
    }
}

/// Uniformly scaled Lo/Ro or Lt/Rt matrix for one decoded Base topology.
#[derive(Clone, Debug, PartialEq)]
pub struct StereoDownmixMatrix {
    selected_mode: StereoDownmixMode,
    rows: Vec<StereoDownmixRow>,
    unscaled_lfe: Option<f64>,
    lfe: Option<f64>,
    unscaled_maximum_coherent_sum: f64,
    scale: f64,
}

impl StereoDownmixMatrix {
    /// Selected policy after resolving `Auto`.
    #[must_use]
    pub const fn selected_mode(&self) -> StereoDownmixMode {
        self.selected_mode
    }

    /// Final Base-channel rows in the supplied channel order.
    #[must_use]
    pub fn rows(&self) -> &[StereoDownmixRow] {
        &self.rows
    }

    /// Final metadata-selected LFE coefficient, if LFE fold-down is enabled.
    #[must_use]
    pub const fn lfe_coefficient(&self) -> Option<f64> {
        self.lfe
    }

    /// Unscaled metadata-selected LFE coefficient, if enabled.
    #[must_use]
    pub const fn unscaled_lfe_coefficient(&self) -> Option<f64> {
        self.unscaled_lfe
    }

    /// Uniform overload-protection scale applied to every matrix coefficient.
    #[must_use]
    pub const fn overload_protection_scale(&self) -> f64 {
        self.scale
    }

    /// Maximum coherent full-scale sum before overload protection.
    ///
    /// This includes the LFE coefficient when metadata explicitly admits LFE
    /// fold-down.  With LFE excluded, the ordinary default 5.1 cases are the
    /// ETSI 2.414 (Lo/Ro) and 3.121 (Lt/Rt) examples.
    #[must_use]
    pub const fn unscaled_maximum_coherent_sum(&self) -> f64 {
        self.unscaled_maximum_coherent_sum
    }

    /// Absolute coefficient sums for the final left and right output rows.
    #[must_use]
    pub fn output_absolute_coefficient_sums(&self) -> [f64; 2] {
        let [left, right] = self.output_sums(false);
        [left, right]
    }

    /// Absolute coefficient sums for the unscaled left and right output rows.
    #[must_use]
    pub fn unscaled_output_absolute_coefficient_sums(&self) -> [f64; 2] {
        let [left, right] = self.output_sums(true);
        [left, right]
    }

    /// Squared coefficient sums for the final left and right output rows.
    #[must_use]
    pub fn output_squared_coefficient_sums(&self) -> [f64; 2] {
        let [left, right] = self.output_squared_sums(false);
        [left, right]
    }

    /// Squared coefficient sums for the unscaled left and right output rows.
    #[must_use]
    pub fn unscaled_output_squared_coefficient_sums(&self) -> [f64; 2] {
        let [left, right] = self.output_squared_sums(true);
        [left, right]
    }

    /// Maximum coherent full-scale sum after overload protection.
    #[must_use]
    pub fn maximum_coherent_sum(&self) -> f64 {
        let [left, right] = self.output_absolute_coefficient_sums();
        left.max(right)
    }

    fn output_sums(&self, unscaled: bool) -> [f64; 2] {
        let [left, right] = self.rows.iter().fold([0.0, 0.0], |mut sums, row| {
            if unscaled {
                sums[0] += row.unscaled_left().abs();
                sums[1] += row.unscaled_right().abs();
            } else {
                sums[0] += row.left().abs();
                sums[1] += row.right().abs();
            }
            sums
        });
        let lfe = if unscaled {
            self.unscaled_lfe.map_or(0.0, f64::abs)
        } else {
            self.lfe.map_or(0.0, f64::abs)
        };
        [left + lfe, right + lfe]
    }

    fn output_squared_sums(&self, unscaled: bool) -> [f64; 2] {
        let [left, right] = self.rows.iter().fold([0.0, 0.0], |mut sums, row| {
            if unscaled {
                sums[0] += row.unscaled_left() * row.unscaled_left();
                sums[1] += row.unscaled_right() * row.unscaled_right();
            } else {
                sums[0] += row.left() * row.left();
                sums[1] += row.right() * row.right();
            }
            sums
        });
        let lfe = if unscaled {
            self.unscaled_lfe.map_or(0.0, |value| value * value)
        } else {
            self.lfe.map_or(0.0, |value| value * value)
        };
        [left + lfe, right + lfe]
    }

    /// Applies this matrix to Base PCM and accumulates into stereo output.
    pub fn apply(
        &self,
        base: &DecodedAccessUnitPcm,
        active: &mut [Vec<f64>],
    ) -> Result<(), StereoDownmixError> {
        if active.len() != 2 {
            return Err(StereoDownmixError::InvalidOutputChannelCount {
                actual: active.len(),
            });
        }
        if base.channel_locations.len() != base.channels.len()
            || self.rows.len() != base.channels.len()
        {
            return Err(StereoDownmixError::ChannelCountMismatch {
                locations: base.channel_locations.len(),
                channels: base.channels.len(),
                matrix_rows: self.rows.len(),
            });
        }
        let sample_count = usize::from(base.samples);
        for output in active.iter() {
            if output.len() != sample_count {
                return Err(StereoDownmixError::OutputSampleCountMismatch {
                    expected: sample_count,
                    actual: output.len(),
                });
            }
        }
        for channel in &base.channels {
            if channel.len() != sample_count {
                return Err(StereoDownmixError::InputSampleCountMismatch {
                    expected: sample_count,
                    actual: channel.len(),
                });
            }
        }
        for (row, (location, channel)) in self
            .rows
            .iter()
            .zip(base.channel_locations.iter().copied().zip(&base.channels))
        {
            if row.location != location {
                return Err(StereoDownmixError::MatrixTopologyMismatch {
                    expected: row.location,
                    actual: location,
                });
            }
            for (sample, value) in channel.iter().copied().enumerate() {
                active[0][sample] += row.left * value;
                active[1][sample] += row.right * value;
            }
        }
        if let (Some(lfe), Some(coefficient)) = (base.lfe.as_deref(), self.lfe) {
            if lfe.len() != sample_count {
                return Err(StereoDownmixError::InputSampleCountMismatch {
                    expected: sample_count,
                    actual: lfe.len(),
                });
            }
            for (sample, value) in lfe.iter().copied().enumerate() {
                active[0][sample] += coefficient * value;
                active[1][sample] += coefficient * value;
            }
        }
        Ok(())
    }
}

/// Matrix construction/application failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StereoDownmixError {
    /// The output accumulation buffer must have exactly FL and FR planes.
    InvalidOutputChannelCount { actual: usize },
    /// Base PCM, channel locations, and matrix rows must have matching counts.
    ChannelCountMismatch {
        locations: usize,
        channels: usize,
        matrix_rows: usize,
    },
    /// A Base channel location is outside the public 6.8 matrix.
    UnsupportedChannel { location: ChannelLocation },
    /// A matrix was applied to a different Base topology.
    MatrixTopologyMismatch {
        expected: ChannelLocation,
        actual: ChannelLocation,
    },
    /// A Base or LFE input plane has the wrong sample count.
    InputSampleCountMismatch { expected: usize, actual: usize },
    /// A stereo output plane has the wrong sample count.
    OutputSampleCountMismatch { expected: usize, actual: usize },
}

impl fmt::Display for StereoDownmixError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidOutputChannelCount { actual } => {
                write!(
                    formatter,
                    "stereo downmix needs two output channels, got {actual}"
                )
            }
            Self::ChannelCountMismatch {
                locations,
                channels,
                matrix_rows,
            } => write!(
                formatter,
                "stereo downmix topology mismatch: {locations} locations, {channels} PCM channels, {matrix_rows} matrix rows"
            ),
            Self::UnsupportedChannel { location } => write!(
                formatter,
                "E-AC-3 stereo downmix does not admit Base channel {}",
                location.label()
            ),
            Self::MatrixTopologyMismatch { expected, actual } => write!(
                formatter,
                "stereo downmix matrix expects Base channel {}, got {}",
                expected.label(),
                actual.label()
            ),
            Self::InputSampleCountMismatch { expected, actual } => write!(
                formatter,
                "stereo downmix input has {actual} samples, expected {expected}"
            ),
            Self::OutputSampleCountMismatch { expected, actual } => write!(
                formatter,
                "stereo downmix output has {actual} samples, expected {expected}"
            ),
        }
    }
}

impl std::error::Error for StereoDownmixError {}

/// Builds the ETSI clause 6.8 matrix for one Base channel configuration.
///
/// The selected matrix is scaled per actual configuration: all coefficients,
/// including an explicitly admitted LFE coefficient, are multiplied by one
/// common factor so no output-row absolute coefficient sum exceeds one. This
/// is the minimum attenuation for the supplied matrix and preserves every
/// relative center/surround/phase relationship.
pub fn stereo_downmix_matrix(
    requested: StereoDownmixMode,
    metadata: DownmixMetadata,
    locations: &[ChannelLocation],
) -> Result<StereoDownmixMatrix, StereoDownmixError> {
    const DEFAULT_LEVEL: f64 = 0.707;
    const CENTER_LEVELS: [f64; 8] = [1.414, 1.189, 1.0, 0.841, 0.707, 0.595, 0.5, 0.0];
    const SURROUND_LEVELS: [f64; 8] = [f64::NAN, f64::NAN, f64::NAN, 0.841, 0.707, 0.595, 0.5, 0.0];
    let selected = match requested {
        StereoDownmixMode::Auto => match metadata.dmixmod {
            Some(1) => StereoDownmixMode::LtRt,
            _ => StereoDownmixMode::LoRo,
        },
        explicit => explicit,
    };
    let center = match selected {
        StereoDownmixMode::LoRo => mix_level(
            metadata.loro_center_mix_level,
            CENTER_LEVELS,
            DEFAULT_LEVEL,
            DEFAULT_LEVEL,
        ),
        StereoDownmixMode::LtRt => mix_level(
            metadata.ltrt_center_mix_level,
            CENTER_LEVELS,
            DEFAULT_LEVEL,
            DEFAULT_LEVEL,
        ),
        StereoDownmixMode::Auto => unreachable!("Auto is resolved above"),
    };
    let surround = match selected {
        StereoDownmixMode::LoRo => mix_level(
            metadata.loro_surround_mix_level,
            SURROUND_LEVELS,
            DEFAULT_LEVEL,
            0.841,
        ),
        StereoDownmixMode::LtRt => mix_level(
            metadata.ltrt_surround_mix_level,
            SURROUND_LEVELS,
            DEFAULT_LEVEL,
            0.841,
        ),
        StereoDownmixMode::Auto => unreachable!("Auto is resolved above"),
    };
    let unscaled_lfe = metadata.lfe_mix_level_code.map(|code| {
        let db = 10.0 - f64::from(code) - 4.5;
        10.0_f64.powf(db / 20.0)
    });
    let mut unscaled_rows = Vec::with_capacity(locations.len());
    for &location in locations {
        let (left, right) = match location {
            ChannelLocation::Left => (1.0, 0.0),
            ChannelLocation::Right => (0.0, 1.0),
            ChannelLocation::Centre => (center, center),
            ChannelLocation::LeftSurround => match selected {
                StereoDownmixMode::LoRo => (surround, 0.0),
                StereoDownmixMode::LtRt => (-surround, surround),
                StereoDownmixMode::Auto => unreachable!("Auto is resolved above"),
            },
            ChannelLocation::RightSurround => match selected {
                StereoDownmixMode::LoRo => (0.0, surround),
                StereoDownmixMode::LtRt => (-surround, surround),
                StereoDownmixMode::Auto => unreachable!("Auto is resolved above"),
            },
            ChannelLocation::Other(3) => match selected {
                StereoDownmixMode::LoRo => (0.7 * surround, 0.7 * surround),
                StereoDownmixMode::LtRt => (-surround, surround),
                StereoDownmixMode::Auto => unreachable!("Auto is resolved above"),
            },
            unsupported => {
                return Err(StereoDownmixError::UnsupportedChannel {
                    location: unsupported,
                });
            }
        };
        unscaled_rows.push((location, left, right));
    }
    let (maximum_left_sum, maximum_right_sum) = unscaled_rows
        .iter()
        .fold((0.0, 0.0), |(left_sum, right_sum), (_, left, right)| {
            (left_sum + left.abs(), right_sum + right.abs())
        });
    let maximum_full_band_sum = maximum_left_sum.max(maximum_right_sum);
    let maximum_sum = maximum_full_band_sum + unscaled_lfe.map_or(0.0, f64::abs);
    let scale = if maximum_sum > 1.0 {
        1.0 / maximum_sum
    } else {
        1.0
    };
    let rows = unscaled_rows
        .into_iter()
        .map(
            |(location, unscaled_left, unscaled_right)| StereoDownmixRow {
                location,
                unscaled_left,
                unscaled_right,
                left: unscaled_left * scale,
                right: unscaled_right * scale,
            },
        )
        .collect();
    Ok(StereoDownmixMatrix {
        selected_mode: selected,
        rows,
        unscaled_lfe,
        lfe: unscaled_lfe.map(|value| value * scale),
        unscaled_maximum_coherent_sum: maximum_sum,
        scale,
    })
}

fn mix_level(code: Option<u8>, table: [f64; 8], default: f64, reserved: f64) -> f64 {
    match code {
        Some(code @ 0..=7) => {
            let value = table[usize::from(code)];
            if value.is_finite() { value } else { reserved }
        }
        _ => default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DialnormState;

    const FIVE_CHANNELS: [ChannelLocation; 5] = [
        ChannelLocation::Left,
        ChannelLocation::Right,
        ChannelLocation::Centre,
        ChannelLocation::LeftSurround,
        ChannelLocation::RightSurround,
    ];

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1.0e-12,
            "{actual} != {expected}"
        );
    }

    fn base(value: f64, lfe: Option<f64>, metadata: DownmixMetadata) -> DecodedAccessUnitPcm {
        DecodedAccessUnitPcm {
            sample_rate: 48_000,
            samples: 1,
            channel_locations: FIVE_CHANNELS.to_vec(),
            channels: vec![vec![value]; 5],
            lfe_location: lfe.map(|_| ChannelLocation::Lfe(0)),
            lfe: lfe.map(|value| vec![value]),
            downmix: metadata,
            dialnorm: DialnormState::default(),
        }
    }

    #[test]
    fn default_loro_uses_etsi_unscaled_rows_and_2414_scale() {
        let matrix = stereo_downmix_matrix(
            StereoDownmixMode::LoRo,
            DownmixMetadata::default(),
            &FIVE_CHANNELS,
        )
        .unwrap();
        assert_eq!(matrix.selected_mode(), StereoDownmixMode::LoRo);
        assert_close(matrix.unscaled_maximum_coherent_sum(), 2.414);
        assert_close(matrix.overload_protection_scale(), 1.0 / 2.414);
        assert_close(matrix.rows()[2].unscaled_left(), 0.707);
        assert_close(matrix.rows()[2].left(), 0.707 / 2.414);
        assert_close(matrix.rows()[3].unscaled_right(), 0.0);
        assert_close(matrix.maximum_coherent_sum(), 1.0);
    }

    #[test]
    fn default_ltrt_uses_etsi_signed_rows_and_3121_scale() {
        let matrix = stereo_downmix_matrix(
            StereoDownmixMode::LtRt,
            DownmixMetadata::default(),
            &FIVE_CHANNELS,
        )
        .unwrap();
        assert_eq!(matrix.selected_mode(), StereoDownmixMode::LtRt);
        assert_close(matrix.unscaled_maximum_coherent_sum(), 3.121);
        assert_close(matrix.overload_protection_scale(), 1.0 / 3.121);
        assert_close(matrix.rows()[3].unscaled_left(), -0.707);
        assert_close(matrix.rows()[4].unscaled_right(), 0.707);
        assert_close(matrix.maximum_coherent_sum(), 1.0);
    }

    #[test]
    fn auto_uses_the_same_matrix_as_the_selected_forced_policy() {
        let metadata = DownmixMetadata {
            dmixmod: Some(1),
            ..DownmixMetadata::default()
        };
        let auto =
            stereo_downmix_matrix(StereoDownmixMode::Auto, metadata, &FIVE_CHANNELS).unwrap();
        let forced =
            stereo_downmix_matrix(StereoDownmixMode::LtRt, metadata, &FIVE_CHANNELS).unwrap();
        assert_eq!(auto, forced);
    }

    #[test]
    fn coherent_full_scale_rows_are_overload_protected() {
        for mode in [StereoDownmixMode::LoRo, StereoDownmixMode::LtRt] {
            let matrix =
                stereo_downmix_matrix(mode, DownmixMetadata::default(), &FIVE_CHANNELS).unwrap();
            for value in [1.0, -1.0] {
                let mut active = vec![vec![0.0], vec![0.0]];
                matrix
                    .apply(&base(value, None, DownmixMetadata::default()), &mut active)
                    .unwrap();
                assert!(
                    active
                        .iter()
                        .flatten()
                        .all(|sample| sample.abs() <= 1.0 + 1.0e-12)
                );
            }
            assert_close(matrix.maximum_coherent_sum(), 1.0);
        }
    }

    #[test]
    fn every_normative_base_contributor_is_checked_individually() {
        for mode in [StereoDownmixMode::LoRo, StereoDownmixMode::LtRt] {
            let matrix =
                stereo_downmix_matrix(mode, DownmixMetadata::default(), &FIVE_CHANNELS).unwrap();
            for channel_index in 0..FIVE_CHANNELS.len() {
                let mut input = base(0.0, None, DownmixMetadata::default());
                input.channels[channel_index][0] = 1.0;
                let mut active = vec![vec![0.0], vec![0.0]];
                matrix.apply(&input, &mut active).unwrap();
                assert!(active.iter().flatten().all(|sample| sample.abs() <= 1.0));
            }
        }
    }

    #[test]
    fn lfe_is_scaled_only_when_metadata_admits_fold_down() {
        let no_lfe = stereo_downmix_matrix(
            StereoDownmixMode::LoRo,
            DownmixMetadata::default(),
            &FIVE_CHANNELS,
        )
        .unwrap();
        assert_eq!(no_lfe.lfe_coefficient(), None);
        let metadata = DownmixMetadata {
            lfe_mix_level_code: Some(31),
            ..DownmixMetadata::default()
        };
        let with_lfe =
            stereo_downmix_matrix(StereoDownmixMode::LoRo, metadata, &FIVE_CHANNELS).unwrap();
        assert!(with_lfe.lfe_coefficient().unwrap() < with_lfe.unscaled_lfe_coefficient().unwrap());
        let mut active = vec![vec![0.0], vec![0.0]];
        with_lfe
            .apply(&base(0.0, Some(1.0), metadata), &mut active)
            .unwrap();
        assert_close(active[0][0], with_lfe.lfe_coefficient().unwrap());
        assert_eq!(active[0], active[1]);
    }
}
