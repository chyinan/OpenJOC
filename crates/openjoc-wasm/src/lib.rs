// pattern: Imperative Shell

//! Thin browser-facing orchestration around the existing OpenJOC session.

mod ffi;
mod performance;
mod stream;

use openjoc_api::{
    DialnormMode, DownmixPolicy, DrcPolicy, OpenJocConfig, OpenJocError, OpenJocPcmFrame,
    OpenJocSession, OpenJocStatus, RenderMode, ValidationProfile,
};
use performance::{TimingSample, summarize};
use std::collections::VecDeque;
use stream::{ElementaryStreamFramer, FramingError, FramingStatus};

const MAX_QUEUED_PCM_FRAMES: usize = 8;
const MAX_TIMING_SAMPLES: usize = 4096;

/// Result categories intentionally stay small and stable at the browser boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailureCategory {
    InvalidInput,
    Decode,
    Render,
    Lifecycle,
    Internal,
}

/// Non-error result of a browser bridge operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecoderStatus {
    NeedMoreInput,
    FrameAvailable,
    OutputPending,
    EndOfStream,
    Error,
}

/// Bounded user-facing bridge error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecoderError {
    pub category: FailureCategory,
    pub detail: String,
}

/// Stable status snapshot consumed by the local player and browser QA.
#[derive(Clone, Debug, PartialEq)]
pub struct DecoderStatusSnapshot {
    pub decoder: &'static str,
    pub input: &'static str,
    pub profile: Option<String>,
    pub objects: Option<u16>,
    pub complexity_index: Option<u8>,
    pub renderer: &'static str,
    pub sample_rate: Option<u32>,
    pub output_channels: usize,
    pub queued_audio_ms: f64,
    pub underrun_count: u64,
    pub preroll_ms: u32,
    pub native_dolby_decoder_used: bool,
    pub decoded_access_units: usize,
    pub output_frames: usize,
    pub output_samples: u64,
    pub error: Option<DecoderError>,
    pub decode_mean_ms: f64,
    pub decode_p95_ms: f64,
    pub decode_max_ms: f64,
    pub render_mean_ms: f64,
    pub render_p95_ms: f64,
    pub render_max_ms: f64,
    pub total_mean_ms: f64,
    pub total_p95_ms: f64,
    pub total_max_ms: f64,
    pub realtime_factor: Option<f64>,
}

/// Stateful, bounded raw-E-AC-3/CMAF-sample-to-Stereo bridge.
pub struct Decoder {
    session: OpenJocSession,
    framer: ElementaryStreamFramer,
    output: VecDeque<OpenJocPcmFrame>,
    current_pcm: Option<OpenJocPcmFrame>,
    profile: Option<String>,
    objects: Option<u16>,
    complexity_index: Option<u8>,
    decoded_access_units: usize,
    output_frames: usize,
    output_samples: u64,
    timing_samples: Vec<TimingSample>,
    input_seen: bool,
    end_of_stream: bool,
    last_error: Option<DecoderError>,
}

impl Decoder {
    /// Creates the fixed Phase-0 Stereo (Speakers) session.
    pub fn new() -> Result<Self, DecoderError> {
        Self::new_with_dialnorm(DialnormMode::Default)
    }

    /// Creates the fixed Stereo session with the existing OpenJOC dialnorm policy.
    pub fn new_with_dialnorm(dialnorm: DialnormMode) -> Result<Self, DecoderError> {
        let config = OpenJocConfig {
            render_mode: RenderMode::Stereo,
            speaker_layout: String::from("2.0"),
            downmix: DownmixPolicy::Auto,
            drc: DrcPolicy::Line,
            dialnorm,
            validation_profile: ValidationProfile::Auto,
            ..OpenJocConfig::default()
        };
        let mut session = OpenJocSession::new(config).map_err(|error| map_openjoc_error(&error))?;
        session.enable_stage_timing();
        Ok(Self {
            session,
            framer: ElementaryStreamFramer::new(),
            output: VecDeque::new(),
            current_pcm: None,
            profile: None,
            objects: None,
            complexity_index: None,
            decoded_access_units: 0,
            output_frames: 0,
            output_samples: 0,
            timing_samples: Vec::new(),
            input_seen: false,
            end_of_stream: false,
            last_error: None,
        })
    }

    /// Adds raw E-AC-3 bytes and advances at most until one PCM frame is ready.
    pub fn push_bytes(&mut self, bytes: &[u8]) -> DecoderStatus {
        if self.end_of_stream {
            return DecoderStatus::EndOfStream;
        }
        if !self.output.is_empty() || self.current_pcm.is_some() {
            return DecoderStatus::OutputPending;
        }
        if !bytes.is_empty() {
            self.input_seen = true;
            if let Err(error) = self.framer.push(bytes) {
                return self.fail(error.into());
            }
        }
        self.pump(false)
    }

    /// Adds one complete CMAF audio sample with its media-timeline PTS.
    pub fn push_packet(
        &mut self,
        bytes: &[u8],
        pts_samples: Option<i64>,
        discontinuity: bool,
        preroll: bool,
    ) -> DecoderStatus {
        if self.end_of_stream {
            return DecoderStatus::EndOfStream;
        }
        if !self.output.is_empty() || self.current_pcm.is_some() {
            return DecoderStatus::OutputPending;
        }
        if bytes.is_empty() {
            return self.fail(DecoderError {
                category: FailureCategory::InvalidInput,
                detail: "CMAF sample is empty".to_owned(),
            });
        }
        if bytes.len() > openjoc_eac3::GENERAL_MAX_ACCESS_UNIT_BYTES {
            return self.fail(DecoderError {
                category: FailureCategory::InvalidInput,
                detail: "CMAF sample exceeds the bounded access-unit limit".to_owned(),
            });
        }
        self.input_seen = true;
        match self.decode_access_unit(bytes, pts_samples, discontinuity, preroll) {
            Ok(()) => {
                if self.output.is_empty() {
                    DecoderStatus::NeedMoreInput
                } else {
                    DecoderStatus::FrameAvailable
                }
            }
            Err(error) => self.fail(error),
        }
    }

    /// Returns one owned interleaved Float32 PCM frame.
    pub fn receive_pcm(&mut self) -> Option<OpenJocPcmFrame> {
        self.current_pcm.take().or_else(|| self.output.pop_front())
    }

    /// Completes input, flushes the existing OpenJOC delay line, and exposes its tail.
    pub fn flush(&mut self) -> Result<DecoderStatus, DecoderError> {
        if self.end_of_stream {
            return Ok(if self.output.is_empty() {
                DecoderStatus::EndOfStream
            } else {
                DecoderStatus::FrameAvailable
            });
        }
        if !self.input_seen {
            let error = DecoderError {
                category: FailureCategory::InvalidInput,
                detail: "empty E-AC-3 JOC stream".to_owned(),
            };
            self.fail(error.clone());
            return Err(error);
        }
        if !self.output.is_empty() || self.current_pcm.is_some() {
            return Ok(DecoderStatus::OutputPending);
        }
        let status = self.pump(true);
        if let Some(error) = self.last_error.clone() {
            Err(error)
        } else {
            Ok(status)
        }
    }

    /// Resets all stream, PCM, and diagnostic state while retaining the Stereo configuration.
    pub fn reset(&mut self) {
        self.session.reset();
        self.framer.reset();
        self.output.clear();
        self.current_pcm = None;
        self.profile = None;
        self.objects = None;
        self.complexity_index = None;
        self.decoded_access_units = 0;
        self.output_frames = 0;
        self.output_samples = 0;
        self.timing_samples.clear();
        self.input_seen = false;
        self.end_of_stream = false;
        self.last_error = None;
    }

    /// Returns a bounded status snapshot for UI and machine QA.
    #[must_use]
    pub fn status(&self) -> DecoderStatusSnapshot {
        let info = self.session.output_info();
        let queued_samples = self
            .output
            .iter()
            .map(|frame| frame.sample_count as u64)
            .chain(
                self.current_pcm
                    .iter()
                    .map(|frame| frame.sample_count as u64),
            )
            .sum::<u64>();
        let queued_audio_ms = info.sample_rate.map_or(0.0, |sample_rate| {
            queued_samples as f64 * 1000.0 / f64::from(sample_rate)
        });
        let performance = summarize(&self.timing_samples);
        DecoderStatusSnapshot {
            decoder: "OpenJOC",
            input: "E-AC-3 JOC",
            profile: self.profile.clone(),
            objects: self.objects,
            complexity_index: self.complexity_index,
            renderer: "Stereo (Speakers)",
            sample_rate: info.sample_rate,
            output_channels: 2,
            queued_audio_ms,
            underrun_count: 0,
            preroll_ms: 128,
            native_dolby_decoder_used: false,
            decoded_access_units: self.decoded_access_units,
            output_frames: self.output_frames,
            output_samples: self.output_samples,
            error: self.last_error.clone(),
            decode_mean_ms: performance.decode_mean_ms,
            decode_p95_ms: performance.decode_p95_ms,
            decode_max_ms: performance.decode_max_ms,
            render_mean_ms: performance.render_mean_ms,
            render_p95_ms: performance.render_p95_ms,
            render_max_ms: performance.render_max_ms,
            total_mean_ms: performance.total_mean_ms,
            total_p95_ms: performance.total_p95_ms,
            total_max_ms: performance.total_max_ms,
            realtime_factor: performance.realtime_factor,
        }
    }

    fn pump(&mut self, eos: bool) -> DecoderStatus {
        loop {
            let next = match self.framer.next(eos) {
                Ok(status) => status,
                Err(error) => return self.fail(error.into()),
            };
            match next {
                FramingStatus::NeedMoreInput => return DecoderStatus::NeedMoreInput,
                FramingStatus::AccessUnit(packet) => {
                    if let Err(error) = self.decode_access_unit(&packet, None, false, false) {
                        return self.fail(error);
                    }
                    if !self.output.is_empty() {
                        return DecoderStatus::FrameAvailable;
                    }
                }
                FramingStatus::EndOfStream => {
                    if !eos {
                        return DecoderStatus::NeedMoreInput;
                    }
                    match self.session.drain() {
                        Ok(OpenJocStatus::OutputPending) => {
                            return DecoderStatus::OutputPending;
                        }
                        Ok(_) => {}
                        Err(error) => return self.fail(map_openjoc_error(&error)),
                    }
                    if let Err(error) = self.collect_session_output() {
                        return self.fail(error);
                    }
                    self.end_of_stream = true;
                    return if self.output.is_empty() {
                        DecoderStatus::EndOfStream
                    } else {
                        DecoderStatus::FrameAvailable
                    };
                }
            }
        }
    }

    fn decode_access_unit(
        &mut self,
        packet: &[u8],
        pts_samples: Option<i64>,
        discontinuity: bool,
        preroll: bool,
    ) -> Result<(), DecoderError> {
        let status = self
            .session
            .push_packet(openjoc_api::OpenJocPacket {
                data: packet,
                pts_samples,
                discontinuity,
                preroll,
            })
            .map_err(|error| map_openjoc_error(&error))?;
        if status == OpenJocStatus::OutputPending {
            return Err(DecoderError {
                category: FailureCategory::Internal,
                detail: "OpenJOC output backpressure was not drained".to_owned(),
            });
        }
        let stage_timing = self.session.take_stage_timing();
        self.collect_session_output()?;
        let diagnostics = self.session.diagnostics();
        self.profile = diagnostics.profile.map(str::to_owned);
        self.objects = diagnostics.object_count;
        self.complexity_index = diagnostics.complexity_index;
        self.decoded_access_units = self.decoded_access_units.saturating_add(1);
        if self.timing_samples.len() < MAX_TIMING_SAMPLES
            && self.timing_samples.try_reserve(1).is_ok()
        {
            let sample_rate = self.session.output_info().sample_rate.unwrap_or(48_000);
            self.timing_samples.push(TimingSample {
                decode: stage_timing.decode.as_secs_f64() * 1000.0,
                render: stage_timing.render.as_secs_f64() * 1000.0,
                total: stage_timing.total.as_secs_f64() * 1000.0,
                audio: 1536.0 * 1000.0 / f64::from(sample_rate),
            });
        }
        Ok(())
    }

    fn collect_session_output(&mut self) -> Result<(), DecoderError> {
        while let Some(frame) = self.session.receive_frame() {
            if self.output.len() >= MAX_QUEUED_PCM_FRAMES {
                return Err(DecoderError {
                    category: FailureCategory::Internal,
                    detail: "PCM queue exceeded the bounded browser bridge limit".to_owned(),
                });
            }
            self.output_samples = self
                .output_samples
                .saturating_add(frame.sample_count as u64);
            self.output_frames = self.output_frames.saturating_add(1);
            self.output.push_back(frame);
        }
        Ok(())
    }

    fn fail(&mut self, error: DecoderError) -> DecoderStatus {
        self.last_error = Some(error);
        self.end_of_stream = true;
        DecoderStatus::Error
    }

    pub(crate) fn mark_internal_failure(&mut self) {
        self.fail(DecoderError {
            category: FailureCategory::Internal,
            detail: "OpenJOC WASM bridge trapped while processing input".to_owned(),
        });
    }

    pub(crate) fn last_error(&self) -> Option<&DecoderError> {
        self.last_error.as_ref()
    }

    pub(crate) fn error_category_code(&self) -> Option<u32> {
        self.last_error.as_ref().map(|error| match error.category {
            FailureCategory::InvalidInput => 0,
            FailureCategory::Decode => 1,
            FailureCategory::Render => 2,
            FailureCategory::Lifecycle => 3,
            FailureCategory::Internal => 4,
        })
    }

    pub(crate) fn profile_bytes(&self) -> Option<&[u8]> {
        self.profile.as_deref().map(str::as_bytes)
    }

    pub(crate) fn object_count(&self) -> Option<u16> {
        self.objects
    }

    pub(crate) fn downmix_index(&self) -> Option<u8> {
        self.session.diagnostics().downmix_index
    }

    pub(crate) fn complexity_index(&self) -> Option<u8> {
        self.session.diagnostics().complexity_index
    }

    pub(crate) fn performance_summary(&self) -> performance::PerformanceSummary {
        summarize(&self.timing_samples)
    }

    pub(crate) fn expose_current_pcm(&mut self) -> Option<&OpenJocPcmFrame> {
        if self.current_pcm.is_none() {
            self.current_pcm = self.output.pop_front();
        }
        self.current_pcm.as_ref()
    }

    pub(crate) fn consume_current_pcm(&mut self) -> bool {
        self.current_pcm.take().is_some()
    }
}

fn map_openjoc_error(error: &OpenJocError) -> DecoderError {
    let detail = bound_detail(&error.to_string());
    let category = match &error {
        OpenJocError::InvalidConfig(_)
        | OpenJocError::InvalidPacket(_)
        | OpenJocError::EmptyStream => FailureCategory::InvalidInput,
        OpenJocError::Decode(_) => FailureCategory::Decode,
        OpenJocError::Render(_) => FailureCategory::Render,
        OpenJocError::AlreadyDrained
        | OpenJocError::OutputPending
        | OpenJocError::FormatChanged { .. }
        | OpenJocError::TimestampDiscontinuity { .. }
        | OpenJocError::ProfileChanged => FailureCategory::Lifecycle,
        OpenJocError::Unsupported(_) => FailureCategory::Internal,
    };
    DecoderError { category, detail }
}

fn bound_detail(detail: &str) -> String {
    detail.chars().take(256).collect()
}

impl From<FramingError> for DecoderError {
    fn from(error: FramingError) -> Self {
        DecoderError {
            category: FailureCategory::InvalidInput,
            detail: bound_detail(&error.to_string()),
        }
    }
}
