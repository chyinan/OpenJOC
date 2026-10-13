//! Native-only, opt-in observations of existing decoder call boundaries.

use super::{Eac3DecodeStageTiming, ReconstructionStageTiming};
use std::time::{Duration, Instant};

/// Detailed observations for one successfully accepted access unit.
///
/// Available on non-WASM targets. Collection is opt-in through
/// [`crate::OpenJocSession::enable_decode_stage_timing`]. This record is not an
/// accumulator, does not include rendering or delivery, and does not change
/// the existing aggregate [`crate::OpenJocStageTiming`] contract.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OpenJocDecodeStageTiming {
    /// Input sample offset within the current stream, before this AU.
    pub input_start_sample: u64,
    /// Input samples accepted from this AU, not emitted output or drain samples.
    pub input_sample_count: u16,
    /// Input sample rate for this AU.
    pub sample_rate: u32,
    /// Wall time of the existing E-AC-3 PCM decoder call alone.
    /// Metadata selection, topology validation and rendering are excluded.
    pub eac3_decode: Duration,
    /// Wall time of the existing owned JOC frame decoder call alone.
    /// Earlier metadata validation/binding classification and later rendering
    /// or delivery are excluded.
    pub joc_reconstruction: Duration,
    /// Existing E-AC-3 implementation-stage times and deterministic workload.
    pub eac3_stages: Eac3DecodeStageTiming,
    /// Existing JOC reconstruction implementation-stage times.
    pub reconstruction_stages: ReconstructionStageTiming,
}

#[derive(Debug, Default)]
pub(super) struct DecodeTimingState {
    pub(super) enabled: bool,
    pub(super) last: Option<OpenJocDecodeStageTiming>,
}

pub(super) fn start(enabled: bool) -> Option<Instant> {
    start_with(enabled, Instant::now)
}

pub(super) fn finish(start: Option<Instant>) -> Option<Duration> {
    finish_with(start, |start| start.elapsed())
}

fn start_with<T>(enabled: bool, clock: impl FnOnce() -> T) -> Option<T> {
    enabled.then(clock)
}

fn finish_with<T>(start: Option<T>, elapsed: impl FnOnce(T) -> Duration) -> Option<Duration> {
    start.map(elapsed)
}

#[cfg(test)]
mod tests {
    use super::{finish_with, start_with};
    use std::{cell::Cell, time::Duration};

    #[test]
    fn disabled_timer_never_reads_a_clock() {
        let reads = Cell::new(0);
        let start = start_with(false, || {
            reads.set(reads.get() + 1);
            0_u64
        });
        assert_eq!(
            finish_with(start, |_| {
                reads.set(reads.get() + 1);
                Duration::ZERO
            }),
            None
        );
        assert_eq!(reads.get(), 0);
    }

    #[test]
    fn enabled_timer_brackets_only_the_selected_call() {
        let clock = Cell::new(100_u64);
        let reads = Cell::new(0);
        let start = start_with(true, || {
            reads.set(reads.get() + 1);
            clock.get()
        });
        clock.set(107);
        let elapsed = finish_with(start, |start| {
            reads.set(reads.get() + 1);
            Duration::from_nanos(clock.get() - start)
        });
        // Work after the selected call cannot alter its already-finished record.
        clock.set(1_000);
        assert_eq!(elapsed, Some(Duration::from_nanos(7)));
        assert_eq!(reads.get(), 2);
    }
}
