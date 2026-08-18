//! Clean semantic E-AC-3 dialnorm conversion and calibrated program gain.
//!
//! The scalar is prepared at metadata-update granularity and is carried with
//! the decoded program frame. It is deliberately separate from E-AC-3 DRC.

/// Dialnorm's internal behavioral mode.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DialnormMode {
    /// The first public policy: the calibrated digital branch.
    #[default]
    Default,
    /// The neutral analog branch; dialnorm contributes unity.
    Analog,
    /// The calibrated digital branch.
    Digital,
}

/// Prepared semantic state for one E-AC-3 program boundary.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DialnormState {
    mode: DialnormMode,
    encoded: u8,
    effective_value: u8,
    linear_gain: f64,
}

impl Default for DialnormState {
    fn default() -> Self {
        Self::new(DialnormMode::Default, 31)
    }
}

impl DialnormState {
    /// Converts one parsed five-bit value into a prepared scalar.
    ///
    /// The parser supplies the closed `0..=31` domain. Code `0` is the
    /// reserved unity fallback and is never interpreted as mute or reuse.
    #[must_use]
    pub fn new(mode: DialnormMode, encoded: u8) -> Self {
        debug_assert!(encoded <= 31, "dialnorm is a five-bit value");
        let effective_value = if encoded == 0 { 31 } else { encoded };
        let linear_gain = match mode {
            DialnormMode::Analog => 1.0,
            DialnormMode::Default | DialnormMode::Digital => {
                10.0_f64.powf((f64::from(effective_value) - 31.0) / 20.0)
            }
        };
        Self {
            mode,
            encoded,
            effective_value,
            linear_gain,
        }
    }

    /// Commits a new independent-syncframe value using the current mode.
    pub fn update(&mut self, encoded: u8) {
        *self = Self::new(self.mode, encoded);
    }

    /// Returns the current mode.
    #[must_use]
    pub const fn mode(self) -> DialnormMode {
        self.mode
    }

    /// Returns the raw five-bit value carried by the accepted syncframe.
    #[must_use]
    pub const fn encoded_value(self) -> u8 {
        self.encoded
    }

    /// Returns the semantic effective value in the `1..=31` domain.
    #[must_use]
    pub const fn effective_value(self) -> u8 {
        self.effective_value
    }

    /// Returns the prepared finite linear amplitude multiplier.
    #[must_use]
    pub const fn linear_gain(self) -> f64 {
        self.linear_gain
    }

    /// Applies the already prepared scalar to one f64 signal plane.
    pub fn apply_to_samples(self, samples: &mut [f64]) {
        for sample in samples {
            *sample *= self.linear_gain;
        }
    }

    /// Restores the canonical new-session state.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_value_maps_reserved_zero_to_unity_baseline() {
        assert_eq!(
            DialnormState::new(DialnormMode::Digital, 0).effective_value(),
            31
        );
        assert_eq!(
            DialnormState::new(DialnormMode::Digital, 31).linear_gain(),
            1.0
        );
    }

    #[test]
    fn analog_is_independent_of_encoded_value() {
        for encoded in 0..=31 {
            assert_eq!(
                DialnormState::new(DialnormMode::Analog, encoded).linear_gain(),
                1.0
            );
        }
    }
}
