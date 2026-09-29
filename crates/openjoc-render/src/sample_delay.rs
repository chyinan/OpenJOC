// pattern: Functional Core

/// A fixed mono delay for aligning bypass audio with an added FIR delay.
#[derive(Debug)]
pub struct SampleDelay {
    samples: Vec<f64>,
    cursor: usize,
    remaining: usize,
}

impl SampleDelay {
    /// Returns the configured delay, independent of queued input or draining.
    #[must_use]
    pub fn delay_samples(&self) -> usize {
        self.samples.len()
    }

    /// Creates a zero-initialized delay. Zero samples is an exact bypass.
    #[must_use]
    pub fn new(samples: usize) -> Self {
        Self {
            samples: vec![0.0; samples],
            cursor: 0,
            remaining: 0,
        }
    }

    /// Delays one input sample, retaining the end of the stream for draining.
    pub fn process_sample(&mut self, sample: f64) -> f64 {
        self.remaining = self.samples.len();
        self.exchange(sample)
    }

    /// Emits the next tail sample, or zero after the tail has ended.
    pub fn drain_sample(&mut self) -> f64 {
        if self.remaining == 0 {
            return 0.0;
        }
        self.remaining -= 1;
        self.exchange(0.0)
    }

    /// Returns the number of samples needed to flush the delayed input.
    #[must_use]
    pub const fn remaining_samples(&self) -> usize {
        self.remaining
    }

    /// Clears all history for a new stream.
    pub fn reset(&mut self) {
        self.samples.fill(0.0);
        self.cursor = 0;
        self.remaining = 0;
    }

    fn exchange(&mut self, sample: f64) -> f64 {
        if self.samples.is_empty() {
            return sample;
        }
        let output = std::mem::replace(&mut self.samples[self.cursor], sample);
        self.cursor = (self.cursor + 1) % self.samples.len();
        output
    }
}
