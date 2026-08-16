// pattern: Functional Core

//! Direct f64 complex QMF reference transform from ETSI TS 103 420 clause 7.

use num_complex::Complex64;
use std::f64::consts::PI;
use std::sync::OnceLock;

/// Number of complex QMF subbands mandated by clause 7.4.
pub const QMF_BANDS: usize = 64;
/// Fixed causal latency of the normative analysis/synthesis identity path.
pub const QMF_ROUNDTRIP_LATENCY_SAMPLES: usize = 577;
/// Length of the normative prototype and analysis state.
pub const QMF_LENGTH: usize = 640;
const SYNTHESIS_LENGTH: usize = 2 * QMF_LENGTH;
const PHASE_TABLE_LENGTH: usize = QMF_BANDS * (2 * QMF_BANDS);

#[allow(dead_code)]
mod generated {
    include!("generated_etsi_tables.rs");
}

fn prototype_f64() -> &'static [f64] {
    static PROTOTYPE: OnceLock<Box<[f64]>> = OnceLock::new();
    PROTOTYPE.get_or_init(|| {
        generated::PROT64
            .iter()
            .copied()
            .map(f64::from)
            .collect::<Vec<_>>()
            .into_boxed_slice()
    })
}

fn analysis_phases() -> &'static [Complex64] {
    // These are the exact `from_polar(1.0, angle)` factors from the analysis
    // equation. Only their construction moves out of the block loop.
    static PHASES: OnceLock<Box<[Complex64]>> = OnceLock::new();
    PHASES.get_or_init(|| {
        let mut phases = vec![Complex64::ZERO; PHASE_TABLE_LENGTH].into_boxed_slice();
        for (index, phase) in phases.iter_mut().enumerate() {
            let subband = index / (2 * QMF_BANDS);
            let folded_index = index % (2 * QMF_BANDS);
            let angle =
                PI * (subband as f64 + 0.5) * (folded_index as f64 - 0.5) / QMF_BANDS as f64;
            *phase = Complex64::from_polar(1.0, angle);
        }
        phases
    })
}

fn synthesis_phases() -> &'static [Complex64] {
    // Store synthesis phases in the loop's [sample][subband] traversal order;
    // this preserves each equation while keeping the inner loop contiguous.
    static PHASES: OnceLock<Box<[Complex64]>> = OnceLock::new();
    PHASES.get_or_init(|| {
        let mut phases = vec![Complex64::ZERO; PHASE_TABLE_LENGTH].into_boxed_slice();
        for (index, phase) in phases.iter_mut().enumerate() {
            let sample_index = index / QMF_BANDS;
            let subband = index % QMF_BANDS;
            // Clause 7.3 defines N[j,k] with the synthesis index centered at
            // j - 128 + 1/2, i.e. the equivalent integer term (2*j - 255).
            let angle = PI / 256.0 * (2 * subband + 1) as f64 * (2.0 * sample_index as f64 - 255.0);
            *phase = Complex64::from_polar(1.0, angle);
        }
        phases
    })
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
        let prototype = prototype_f64();
        for (index, value) in folded.iter_mut().enumerate() {
            for fold in 0..QMF_LENGTH / (2 * QMF_BANDS) {
                let window_index = index + fold * 2 * QMF_BANDS;
                *value += self.analysis_state[window_index] * prototype[window_index];
            }
        }

        let mut subbands = [Complex64::ZERO; QMF_BANDS];
        let phases = analysis_phases();
        for (subband, output) in subbands.iter_mut().enumerate() {
            for (index, sample) in folded.iter().copied().enumerate() {
                let phase = phases[subband * (2 * QMF_BANDS) + index];
                *output += Complex64::new(sample * phase.re, sample * phase.im);
            }
        }
        subbands
    }

    /// Applies clauses 7.3 pseudocode 13 through 17 to one complex timeslot.
    #[must_use]
    pub fn synthesize(&mut self, subbands: &[Complex64; QMF_BANDS]) -> [f64; QMF_BANDS] {
        self.synthesis_state
            .copy_within(0..SYNTHESIS_LENGTH - 2 * QMF_BANDS, 2 * QMF_BANDS);

        let phases = synthesis_phases();
        for (index, state_sample) in self.synthesis_state[..2 * QMF_BANDS].iter_mut().enumerate() {
            let mut sample = 0.0;
            for (subband, value) in subbands.iter().enumerate() {
                sample += (value * phases[index * QMF_BANDS + subband]).re / 64.0;
            }
            *state_sample = sample;
        }

        let mut pcm = [0.0; QMF_BANDS];
        let prototype = prototype_f64();
        for (timeslot, output) in pcm.iter_mut().enumerate() {
            for fold in 0..QMF_LENGTH / QMF_BANDS {
                let window_index = fold * QMF_BANDS + timeslot;
                let synthesis_index = if fold % 2 == 0 {
                    2 * fold * QMF_BANDS + timeslot
                } else {
                    2 * (fold - 1) * QMF_BANDS + 3 * QMF_BANDS + timeslot
                };
                *output += self.synthesis_state[synthesis_index] * prototype[window_index];
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
