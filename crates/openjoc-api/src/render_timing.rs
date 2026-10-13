//! Native-only observations of existing render and PCM conversion boundaries.

use super::{OpenJocPcmFrame, RenderedBlock};
use std::time::Duration;

/// The successful session operation described by a renderer observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpenJocRenderOperation {
    /// An accepted AU. Input counts are independent of delayed output counts.
    Push {
        /// Input sample offset before this AU in the current stream.
        input_start_sample: u64,
        /// Samples accepted from this AU, per channel.
        input_sample_count: u16,
        /// Input sample rate.
        sample_rate: u32,
    },
    /// The first successful drain transition, possibly with no output.
    Drain,
    /// One successful bounded binaural pull returning a PCM frame.
    BinauralPull,
}

/// Native-only, opt-in observations for the latest successful render operation.
///
/// These stages do not partition all session work and are not interchangeable
/// with the legacy CLI report stages. In particular, binaural timing includes
/// lazy engine initialization when it occurs. A deferred-binaural push queues
/// speaker blocks; later pulls perform their binaural processing and conversion.
/// No adapter copy, file writing, reporting or caller delivery is included.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpenJocRenderStageTiming {
    /// Operation whose successful work is described by this record.
    pub operation: OpenJocRenderOperation,
    /// Existing bridge-control assembly calls, excluding calibration.
    pub bridge_control_assembly: Duration,
    /// Boundary preparation, spatial segment rendering and stereo base mixing.
    /// Channel composition, calibration and final linked gain are excluded.
    pub spatial_bridge_render: Duration,
    /// Entire existing binaural render/range/tail calls, including any lazy
    /// engine initialization, orientation handling and output allocation.
    pub binaural_render: Duration,
    /// Existing PCM frame conversion calls: descriptors, validation,
    /// interleaving, f32 narrowing and PTS construction; queueing is excluded.
    pub pcm_conversion: Duration,
    /// Blocks returned by speaker rendering in this operation, including tails.
    pub speaker_blocks: u64,
    /// Speaker output samples per channel, not accepted input or host delivery.
    pub speaker_samples: u64,
    /// PCM frames successfully converted in this operation.
    pub pcm_blocks: u64,
    /// Successfully converted PCM samples per channel.
    pub pcm_samples: u64,
}

#[derive(Debug, Default)]
pub(super) struct SpeakerTimingState {
    pub(super) enabled: bool,
    pub(super) bridge_control_assembly: Duration,
    pub(super) spatial_bridge_render: Duration,
}

impl SpeakerTimingState {
    pub(super) fn clear(&mut self) {
        self.bridge_control_assembly = Duration::ZERO;
        self.spatial_bridge_render = Duration::ZERO;
    }
}

#[derive(Debug, Default)]
pub(super) struct RenderTimingState {
    pub(super) enabled: bool,
    pub(super) last: Option<OpenJocRenderStageTiming>,
    pub(super) current: RenderTimingScratch,
}

#[derive(Debug, Default)]
pub(super) struct RenderTimingScratch {
    pub(super) binaural_render: Duration,
    pub(super) pcm_conversion: Duration,
    speaker_blocks: u64,
    speaker_samples: u64,
    pcm_blocks: u64,
    pcm_samples: u64,
}

impl RenderTimingState {
    pub(super) fn begin(&mut self) {
        self.current = RenderTimingScratch::default();
    }

    pub(super) fn clear(&mut self) {
        self.last = None;
        self.begin();
    }

    pub(super) fn record_speaker(&mut self, blocks: &[RenderedBlock]) {
        if self.enabled {
            for block in blocks {
                self.current.speaker_blocks = self.current.speaker_blocks.saturating_add(1);
                self.current.speaker_samples = self
                    .current
                    .speaker_samples
                    .saturating_add(u64::try_from(block.sample_count).unwrap_or(u64::MAX));
            }
        }
    }

    pub(super) fn record_pcm(&mut self, pcm: &OpenJocPcmFrame, elapsed: Option<Duration>) {
        if let Some(elapsed) = elapsed {
            self.current.pcm_conversion = self.current.pcm_conversion.saturating_add(elapsed);
            self.current.pcm_blocks = self.current.pcm_blocks.saturating_add(1);
            self.current.pcm_samples = self
                .current
                .pcm_samples
                .saturating_add(u64::try_from(pcm.sample_count).unwrap_or(u64::MAX));
        }
    }

    pub(super) fn record_binaural(&mut self, elapsed: Option<Duration>) {
        if let Some(elapsed) = elapsed {
            self.current.binaural_render = self.current.binaural_render.saturating_add(elapsed);
        }
    }

    pub(super) fn publish(
        &mut self,
        operation: OpenJocRenderOperation,
        speaker: &SpeakerTimingState,
    ) {
        if self.enabled {
            let current = std::mem::take(&mut self.current);
            self.last = Some(OpenJocRenderStageTiming {
                operation,
                bridge_control_assembly: speaker.bridge_control_assembly,
                spatial_bridge_render: speaker.spatial_bridge_render,
                binaural_render: current.binaural_render,
                pcm_conversion: current.pcm_conversion,
                speaker_blocks: current.speaker_blocks,
                speaker_samples: current.speaker_samples,
                pcm_blocks: current.pcm_blocks,
                pcm_samples: current.pcm_samples,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_duration_success_is_a_record_and_partial_work_is_discarded() {
        let mut state = RenderTimingState {
            enabled: true,
            ..RenderTimingState::default()
        };
        let speaker = SpeakerTimingState::default();
        state.publish(OpenJocRenderOperation::Drain, &speaker);
        let saved = state.last.clone().unwrap();
        state.current.binaural_render = Duration::from_nanos(7);
        state.current.pcm_samples = 13;
        // A failed operation never calls publish. The next operation starts clean.
        state.begin();
        assert_eq!(state.last, Some(saved));
        state.publish(OpenJocRenderOperation::BinauralPull, &speaker);
        let latest = state.last.take().unwrap();
        assert_eq!(latest.operation, OpenJocRenderOperation::BinauralPull);
        assert_eq!(latest.binaural_render, Duration::ZERO);
        assert_eq!(latest.pcm_samples, 0);
        assert!(state.last.take().is_none());
    }

    #[test]
    fn disabled_publication_is_empty_and_clear_retains_enablement() {
        let mut state = RenderTimingState::default();
        let speaker = SpeakerTimingState::default();
        state.publish(OpenJocRenderOperation::Drain, &speaker);
        assert!(state.last.is_none());
        state.enabled = true;
        state.publish(OpenJocRenderOperation::Drain, &speaker);
        state.clear();
        assert!(state.last.is_none());
        assert!(state.enabled);
        assert_eq!(state.current.pcm_blocks, 0);
    }
}
