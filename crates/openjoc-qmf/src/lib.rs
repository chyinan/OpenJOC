// pattern: Functional Core

//! Direct f64 complex QMF reference transform from ETSI TS 103 420 clause 7.

use num_complex::Complex64;
use std::f64::consts::PI;

/// Number of complex QMF subbands mandated by clause 7.4.
pub const QMF_BANDS: usize = 64;
/// Length of the normative prototype and analysis state.
pub const QMF_LENGTH: usize = 640;
const SYNTHESIS_LENGTH: usize = 2 * QMF_LENGTH;

#[allow(dead_code)]
mod generated {
    include!(concat!(env!("OUT_DIR"), "/etsi_tables.rs"));
}

/// Stateful, direct-equation f64 implementation of the clause 7 transform pair.
#[derive(Clone, Debug)]
pub struct ReferenceQmf64F64 {
    analysis_state: [f64; QMF_LENGTH],
    synthesis_state: [f64; SYNTHESIS_LENGTH],
}

impl ReferenceQmf64F64 {
    /// Creates a transform with both normative history buffers initialized to zero.
    #[must_use]
    pub fn new() -> Self {
        Self {
            analysis_state: [0.0; QMF_LENGTH],
            synthesis_state: [0.0; SYNTHESIS_LENGTH],
        }
    }

    /// Clears both analysis and synthesis history, for stream discontinuities.
    pub fn reset(&mut self) {
        self.analysis_state.fill(0.0);
        self.synthesis_state.fill(0.0);
    }

    /// Applies clauses 7.2 pseudocode 8 through 12 to one 64-sample block.
    #[must_use]
    pub fn analyze(&mut self, pcm: &[f64; QMF_BANDS]) -> [Complex64; QMF_BANDS] {
        self.analysis_state
            .copy_within(0..QMF_LENGTH - QMF_BANDS, QMF_BANDS);
        for (destination, sample) in self.analysis_state[..QMF_BANDS]
            .iter_mut()
            .zip(pcm.iter().rev())
        {
            *destination = *sample;
        }

        let mut folded = [0.0; 2 * QMF_BANDS];
        for (index, value) in folded.iter_mut().enumerate() {
            for fold in 0..QMF_LENGTH / (2 * QMF_BANDS) {
                let window_index = index + fold * 2 * QMF_BANDS;
                *value +=
                    self.analysis_state[window_index] * f64::from(generated::PROT64[window_index]);
            }
        }

        let mut subbands = [Complex64::ZERO; QMF_BANDS];
        for (subband, output) in (0_u32..).zip(subbands.iter_mut()) {
            for (index, sample) in (0_u32..).zip(folded.iter().copied()) {
                let angle = PI * (f64::from(subband) + 0.5) * (f64::from(index) - 0.5) / 64.0;
                *output += Complex64::from_polar(sample, angle);
            }
        }
        subbands
    }

    /// Applies clauses 7.3 pseudocode 13 through 17 to one complex timeslot.
    #[must_use]
    pub fn synthesize(&mut self, subbands: &[Complex64; QMF_BANDS]) -> [f64; QMF_BANDS] {
        self.synthesis_state
            .copy_within(0..SYNTHESIS_LENGTH - 2 * QMF_BANDS, 2 * QMF_BANDS);

        for (index, state_sample) in (0_u32..).zip(self.synthesis_state[..2 * QMF_BANDS].iter_mut())
        {
            let mut sample = 0.0;
            for (subband, value) in (0_u32..).zip(subbands.iter()) {
                let angle =
                    PI / 256.0 * f64::from(2 * subband + 1) * (2.0 * f64::from(index) - 129.0);
                sample += (value * Complex64::from_polar(1.0, angle)).re / 64.0;
            }
            *state_sample = sample;
        }

        let mut pcm = [0.0; QMF_BANDS];
        for (timeslot, output) in pcm.iter_mut().enumerate() {
            for fold in 0..QMF_LENGTH / QMF_BANDS {
                let window_index = fold * QMF_BANDS + timeslot;
                let synthesis_index = if fold % 2 == 0 {
                    2 * fold * QMF_BANDS + timeslot
                } else {
                    2 * (fold - 1) * QMF_BANDS + 3 * QMF_BANDS + timeslot
                };
                *output += self.synthesis_state[synthesis_index]
                    * f64::from(generated::PROT64[window_index]);
            }
        }
        pcm
    }
}

impl Default for ReferenceQmf64F64 {
    fn default() -> Self {
        Self::new()
    }
}
