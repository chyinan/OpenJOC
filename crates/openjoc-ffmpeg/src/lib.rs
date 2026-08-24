//! External FFmpeg-facing packet/frame bridge for OpenJOC.
//!
//! This crate is not a libavcodec plugin. Its core accepts borrowed compressed
//! packet bytes plus an `AVStream.time_base` equivalent, assembles bounded
//! E-AC-3 access units, positively admits JOC, and drives `OpenJocSession`.
//! With the `ffmpeg` feature it also owns real, public-API-allocated AVFrames
//! and exposes a small libavformat demux helper.

#![cfg_attr(feature = "ffmpeg", allow(unsafe_code))]

use openjoc_api::{
    FINAL_LINKED_GAIN_LATENCY_SAMPLES, OpenJocConfig, OpenJocPacket, OpenJocPcmFrame,
    OpenJocSession, QMF_LATENCY_SAMPLES, RenderMode,
};
use openjoc_eac3::{
    AccessUnitIndex, StreamType, group_access_units, index_syncframes, parse_audio_frame,
    parse_joc_access_unit, parse_syncframe_header, validate_joc_access_unit,
};
use openjoc_emdf::JocValidationProfile;
use openjoc_scene::SpeakerLayoutPreset;
use sha2::{Digest, Sha256};
use std::{
    collections::VecDeque,
    fmt,
    panic::{AssertUnwindSafe, catch_unwind},
    time::{Duration, Instant},
};

#[cfg(feature = "ffmpeg")]
mod ffi;
#[cfg(feature = "ffmpeg")]
pub use ffi::{
    AvFrame, DemuxPacket, Demuxer, FfmpegLibraryVersions, LibraryVersion, ReceiveAvOutcome,
};

/// OpenJOC's fixed decoded sample rate and the AVFrame time base used here.
pub const SAMPLE_RATE: u32 = 48_000;
/// `AV_NOPTS_VALUE`, represented without requiring libavutil at core-build time.
pub const AV_NOPTS_VALUE: i64 = i64::MIN;
/// E-AC-3 syncframes are bounded to 4096 bytes by the coded frame-size field.
pub const MAX_SYNCFRAME_BYTES: usize = 4096;
/// OpenJOC admits I0 plus at most D0 per access unit.
pub const MAX_ACCESS_UNIT_BYTES: usize = MAX_SYNCFRAME_BYTES * 2;
/// Maximum compressed data retained across calls. This is 16 maximum AUs.
pub const MAX_COMPRESSED_STAGING_BYTES: usize = MAX_ACCESS_UNIT_BYTES * 16;

/// A checked rational number. FFmpeg packet timestamps use the stream time base.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rational {
    pub numerator: i32,
    pub denominator: i32,
}

impl Rational {
    pub const SAMPLE_TIME_BASE: Self = Self {
        numerator: 1,
        denominator: 48_000,
    };

    pub const fn new(numerator: i32, denominator: i32) -> Self {
        Self {
            numerator,
            denominator,
        }
    }

    fn validate(self) -> Result<(), BridgeError> {
        if self.numerator <= 0 || self.denominator <= 0 {
            return Err(BridgeError::new(
                BridgeErrorKind::InvalidTimestamp,
                format!(
                    "invalid rational time base {}/{}",
                    self.numerator, self.denominator
                ),
            ));
        }
        Ok(())
    }
}

/// Equivalent to FFmpeg's nearest, ties-away-from-zero `av_rescale_q` policy,
/// but reports overflow and invalid rationals instead of saturating.
pub fn rescale_q_checked(
    value: i64,
    source: Rational,
    destination: Rational,
) -> Result<i64, BridgeError> {
    if value == AV_NOPTS_VALUE {
        return Err(BridgeError::new(
            BridgeErrorKind::InvalidTimestamp,
            "AV_NOPTS_VALUE must be handled as an absent timestamp",
        ));
    }
    source.validate()?;
    destination.validate()?;
    let numerator = i128::from(value)
        .checked_mul(i128::from(source.numerator))
        .and_then(|scaled| scaled.checked_mul(i128::from(destination.denominator)))
        .ok_or_else(|| {
            BridgeError::new(
                BridgeErrorKind::InvalidTimestamp,
                "timestamp numerator overflow",
            )
        })?;
    let denominator = i128::from(source.denominator)
        .checked_mul(i128::from(destination.numerator))
        .ok_or_else(|| {
            BridgeError::new(
                BridgeErrorKind::InvalidTimestamp,
                "timestamp denominator overflow",
            )
        })?;
    let negative = numerator.is_negative();
    let magnitude = numerator.abs();
    let mut rounded = magnitude / denominator;
    let remainder = magnitude % denominator;
    if remainder.saturating_mul(2) >= denominator {
        rounded = rounded.checked_add(1).ok_or_else(|| {
            BridgeError::new(
                BridgeErrorKind::InvalidTimestamp,
                "timestamp rounding overflow",
            )
        })?;
    }
    let signed = if negative { -rounded } else { rounded };
    i64::try_from(signed).map_err(|_| {
        BridgeError::new(
            BridgeErrorKind::InvalidTimestamp,
            "rescaled timestamp is outside i64",
        )
    })
}

/// Which compressed timestamp may anchor the presentation timeline.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TimestampPolicy {
    /// Use presentation timestamps only. Missing PTS stays missing.
    #[default]
    PtsOnly,
    /// Explicitly permit DTS only when PTS is absent.
    PtsThenDts,
}

/// The timestamp selected for a packet/AU trace.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimestampSource {
    Pts,
    DtsFallback,
    Synthesized,
    Absent,
}

/// Borrowed input corresponding to the relevant public AVPacket fields.
/// Bytes are copied only if they must survive this call.
#[derive(Clone, Copy, Debug)]
pub struct PacketRef<'a> {
    pub data: &'a [u8],
    pub pts: Option<i64>,
    pub dts: Option<i64>,
    pub duration: Option<i64>,
    pub time_base: Rational,
    pub stream_index: i32,
    pub discontinuity: bool,
    pub preroll: bool,
}

/// Positive admission state. Ordinary E-AC-3 is not an OpenJOC fallback.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum JocClassification {
    #[default]
    Unknown,
    ConfirmedJoc,
    ConfirmedNonJoc,
    InvalidOrUnsupported,
}

impl JocClassification {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "UNKNOWN",
            Self::ConfirmedJoc => "CONFIRMED_JOC",
            Self::ConfirmedNonJoc => "CONFIRMED_NON_JOC",
            Self::InvalidOrUnsupported => "INVALID_OR_UNSUPPORTED",
        }
    }
}

/// Bounded, decode-free classifier for a compressed E-AC-3 stream.
///
/// This shares the packet-to-access-unit parser and positive JOC admission
/// rules with [`FfmpegDecoder`], but never creates an `OpenJocSession`. It is
/// intended for framework adapters that must choose a decoder before sending
/// the first packet to a renderer.
#[derive(Debug, Default)]
pub struct JocClassifier {
    staging: Vec<u8>,
    classification: JocClassification,
    inspected_bytes: usize,
}

impl JocClassifier {
    /// Creates an empty stream classifier.
    pub const fn new() -> Self {
        Self {
            staging: Vec::new(),
            classification: JocClassification::Unknown,
            inspected_bytes: 0,
        }
    }

    /// Returns the current positive-admission state.
    pub const fn classification(&self) -> JocClassification {
        self.classification
    }

    /// Returns compressed bytes retained while waiting for a complete AU.
    pub fn staged_bytes(&self) -> usize {
        self.staging.len()
    }

    /// Returns compressed bytes examined by the classifier.
    pub const fn inspected_bytes(&self) -> usize {
        self.inspected_bytes
    }

    /// Discards the current bounded probe state.
    pub fn reset(&mut self) {
        self.staging.clear();
        self.classification = JocClassification::Unknown;
        self.inspected_bytes = 0;
    }

    /// Supplies a borrowed compressed chunk without decoding or rendering it.
    pub fn send_chunk(&mut self, data: &[u8]) -> Result<JocClassification, BridgeError> {
        if data.is_empty() {
            return Err(BridgeError::new(
                BridgeErrorKind::InvalidData,
                "empty classifier chunk",
            ));
        }
        if self.classification != JocClassification::Unknown {
            return Ok(self.classification);
        }

        let new_len = self.staging.len().checked_add(data.len()).ok_or_else(|| {
            BridgeError::new(BridgeErrorKind::InvalidData, "classifier staging overflow")
        })?;
        if new_len > MAX_COMPRESSED_STAGING_BYTES {
            self.classification = JocClassification::InvalidOrUnsupported;
            return Err(BridgeError::new(
                BridgeErrorKind::OutputPending,
                format!("classifier staging would exceed {MAX_COMPRESSED_STAGING_BYTES} bytes"),
            ));
        }
        self.staging.extend_from_slice(data);

        let size = match parse_access_unit(&self.staging, false)? {
            AccessUnitParse::NeedMore => return Ok(JocClassification::Unknown),
            AccessUnitParse::Complete(size) => size,
        };
        let inspected = inspect_complete_access_unit(&self.staging[..size])?;
        self.inspected_bytes = self.inspected_bytes.saturating_add(size);
        self.staging.drain(..size);
        self.classification = inspected.classification;
        Ok(self.classification)
    }

    /// Closes the bounded probe and classifies a final complete AU when the
    /// stream does not contain a following independent syncframe.
    pub fn finish(&mut self) -> Result<JocClassification, BridgeError> {
        if self.classification != JocClassification::Unknown {
            return Ok(self.classification);
        }
        let size = match parse_access_unit(&self.staging, true)? {
            AccessUnitParse::NeedMore => return Ok(JocClassification::Unknown),
            AccessUnitParse::Complete(size) => size,
        };
        let inspected = inspect_complete_access_unit(&self.staging[..size])?;
        self.inspected_bytes = self.inspected_bytes.saturating_add(size);
        self.staging.drain(..size);
        self.classification = inspected.classification;
        Ok(self.classification)
    }
}

/// Non-error send/drain status with bounded backpressure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BridgeStatus {
    Ok,
    NeedMoreInput,
    FrameAvailable,
    WouldBlock,
    EndOfStream,
    NotJoc,
}

/// Result of one receive call.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, PartialEq)]
pub enum ReceiveOutcome {
    Frame(FfmpegFrame),
    NeedMoreInput,
    EndOfStream,
    NotJoc,
}

/// Stable error category suitable for a future native libavcodec mapping.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BridgeErrorKind {
    InvalidConfig,
    InvalidData,
    InvalidTimestamp,
    Unsupported,
    OutputPending,
    EndOfStream,
    Ffmpeg,
    InternalPanic,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BridgeError {
    pub kind: BridgeErrorKind,
    pub message: String,
}

impl BridgeError {
    pub fn new(kind: BridgeErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl fmt::Display for BridgeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.message)
    }
}

impl std::error::Error for BridgeError {}

/// One deterministic packet-to-AU audit record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccessUnitTrace {
    pub index: u64,
    pub byte_length: usize,
    pub sha256: String,
    pub pts_samples: Option<i64>,
    pub timestamp_source: TimestampSource,
    pub sample_count: u16,
    pub sample_rate: u32,
    pub independent_frame_count: usize,
    pub dependent_frame_count: usize,
}

/// Cumulative instance-local stage timings for integration benchmarking.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BridgeTimings {
    pub packet_staging_nanos: u128,
    pub au_assembly_and_admission_nanos: u128,
    pub openjoc_session_nanos: u128,
    pub channel_reorder_nanos: u128,
    pub avframe_allocation_nanos: u128,
}

impl BridgeTimings {
    fn add_packet_staging(&mut self, elapsed: Duration) {
        self.packet_staging_nanos = self.packet_staging_nanos.saturating_add(elapsed.as_nanos());
    }

    fn add_assembly(&mut self, elapsed: Duration) {
        self.au_assembly_and_admission_nanos = self
            .au_assembly_and_admission_nanos
            .saturating_add(elapsed.as_nanos());
    }

    fn add_session(&mut self, elapsed: Duration) {
        self.openjoc_session_nanos = self
            .openjoc_session_nanos
            .saturating_add(elapsed.as_nanos());
    }

    fn add_reorder(&mut self, elapsed: Duration) {
        self.channel_reorder_nanos = self
            .channel_reorder_nanos
            .saturating_add(elapsed.as_nanos());
    }
}

/// FFmpeg channel semantics and the exact transport permutation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FfmpegChannelLayout {
    pub name: String,
    pub standard_layout: Option<String>,
    pub custom: bool,
    pub openjoc_order: Vec<String>,
    pub ffmpeg_order: Vec<String>,
    /// For each FFmpeg output channel, the matching OpenJOC input index.
    pub permutation: Vec<usize>,
}

impl FfmpegChannelLayout {
    pub fn inverse_permutation(&self) -> Result<Vec<usize>, BridgeError> {
        let mut inverse = vec![usize::MAX; self.permutation.len()];
        for (output, &input) in self.permutation.iter().enumerate() {
            let slot = inverse.get_mut(input).ok_or_else(|| {
                BridgeError::new(
                    BridgeErrorKind::InvalidConfig,
                    "channel permutation index is outside the layout",
                )
            })?;
            if *slot != usize::MAX {
                return Err(BridgeError::new(
                    BridgeErrorKind::InvalidConfig,
                    "channel permutation contains a duplicate input",
                ));
            }
            *slot = output;
        }
        if inverse.contains(&usize::MAX) {
            return Err(BridgeError::new(
                BridgeErrorKind::InvalidConfig,
                "channel permutation omits an input",
            ));
        }
        Ok(inverse)
    }
}

/// Owned packed/interleaved float output with AVFrame-compatible semantics.
#[derive(Clone, Debug, PartialEq)]
pub struct FfmpegFrame {
    pub format: &'static str,
    pub sample_rate: u32,
    pub nb_samples: usize,
    /// In [`Rational::SAMPLE_TIME_BASE`], without latency pre-shifting.
    pub pts: Option<i64>,
    /// In [`Rational::SAMPLE_TIME_BASE`].
    pub duration: i64,
    pub channel_layout: FfmpegChannelLayout,
    pub interleaved_f32: Vec<f32>,
}

#[derive(Clone, Copy, Debug)]
struct Boundary {
    byte_offset: usize,
    timestamp: Option<i64>,
    source: TimestampSource,
    preroll: bool,
}

#[derive(Clone, Copy, Debug)]
enum Timeline {
    Unset,
    Timed { next_pts: i64 },
    Untimed,
}

enum AccessUnitParse {
    NeedMore,
    Complete(usize),
}

enum PumpResult {
    Idle,
    Frame,
    NotJoc,
    Eof,
}

/// Reusable, instance-owned external decoder-like wrapper.
pub struct FfmpegDecoder {
    config: OpenJocConfig,
    config_descriptor: String,
    config_fingerprint: String,
    layout: FfmpegChannelLayout,
    timestamp_policy: TimestampPolicy,
    session: Option<OpenJocSession>,
    staging: Vec<u8>,
    boundaries: VecDeque<Boundary>,
    output: VecDeque<FfmpegFrame>,
    traces: Vec<AccessUnitTrace>,
    classification: JocClassification,
    timeline: Timeline,
    next_au_index: u64,
    drain_requested: bool,
    session_drained: bool,
    poisoned: bool,
    timings: BridgeTimings,
}

impl fmt::Debug for FfmpegDecoder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FfmpegDecoder")
            .field("layout", &self.layout.name)
            .field("classification", &self.classification)
            .field("staging_bytes", &self.staging.len())
            .field("output_frames", &self.output.len())
            .field("drain_requested", &self.drain_requested)
            .finish_non_exhaustive()
    }
}

impl FfmpegDecoder {
    pub fn new(config: OpenJocConfig) -> Result<Self, BridgeError> {
        Self::with_timestamp_policy(config, TimestampPolicy::PtsOnly)
    }

    pub fn with_timestamp_policy(
        config: OpenJocConfig,
        timestamp_policy: TimestampPolicy,
    ) -> Result<Self, BridgeError> {
        config
            .validate()
            .map_err(|error| BridgeError::new(BridgeErrorKind::InvalidConfig, error.to_string()))?;
        let layout = channel_layout_for_config(&config)?;
        let config_descriptor = config.effective_config_descriptor();
        let config_fingerprint = config.effective_config_fingerprint();
        Ok(Self {
            config,
            config_descriptor,
            config_fingerprint,
            layout,
            timestamp_policy,
            session: None,
            staging: Vec::new(),
            boundaries: VecDeque::new(),
            output: VecDeque::new(),
            traces: Vec::new(),
            classification: JocClassification::Unknown,
            timeline: Timeline::Unset,
            next_au_index: 0,
            drain_requested: false,
            session_drained: false,
            poisoned: false,
            timings: BridgeTimings::default(),
        })
    }

    #[must_use]
    pub const fn classification(&self) -> JocClassification {
        self.classification
    }

    #[must_use]
    pub fn channel_layout(&self) -> &FfmpegChannelLayout {
        &self.layout
    }

    #[must_use]
    pub fn effective_config_descriptor(&self) -> &str {
        &self.config_descriptor
    }

    #[must_use]
    pub fn effective_config_fingerprint(&self) -> &str {
        &self.config_fingerprint
    }

    #[must_use]
    pub fn latency_samples(&self) -> usize {
        if self.config.render_mode == RenderMode::Binaural {
            QMF_LATENCY_SAMPLES
        } else {
            QMF_LATENCY_SAMPLES + FINAL_LINKED_GAIN_LATENCY_SAMPLES
        }
    }

    #[must_use]
    pub fn latency_time(&self) -> (i64, Rational) {
        (
            i64::try_from(self.latency_samples()).unwrap_or(i64::MAX),
            Rational::SAMPLE_TIME_BASE,
        )
    }

    #[must_use]
    pub fn staged_bytes(&self) -> usize {
        self.staging.len()
    }

    #[must_use]
    pub fn queued_frames(&self) -> usize {
        self.output.len()
    }

    #[must_use]
    pub const fn timings(&self) -> BridgeTimings {
        self.timings
    }

    pub fn take_traces(&mut self) -> Vec<AccessUnitTrace> {
        std::mem::take(&mut self.traces)
    }

    pub fn send_packet(&mut self, packet: PacketRef<'_>) -> Result<BridgeStatus, BridgeError> {
        self.guard(|decoder| decoder.send_packet_inner(packet))
    }

    pub fn receive_frame(&mut self) -> Result<ReceiveOutcome, BridgeError> {
        self.guard(Self::receive_frame_inner)
    }

    pub fn drain(&mut self) -> Result<BridgeStatus, BridgeError> {
        self.guard(Self::drain_inner)
    }

    /// Discards compressed staging, PCM, DSP history, and timing state.
    pub fn flush(&mut self) {
        self.reset_inner();
    }

    /// Alias with explicit seek/discontinuity intent.
    pub fn reset(&mut self) {
        self.reset_inner();
    }

    fn guard<T>(
        &mut self,
        operation: impl FnOnce(&mut Self) -> Result<T, BridgeError>,
    ) -> Result<T, BridgeError> {
        if self.poisoned {
            return Err(BridgeError::new(
                BridgeErrorKind::InternalPanic,
                "OpenJOC FFmpeg wrapper is poisoned after an internal panic",
            ));
        }
        if let Ok(result) = catch_unwind(AssertUnwindSafe(|| operation(self))) {
            result
        } else {
            self.poisoned = true;
            Err(BridgeError::new(
                BridgeErrorKind::InternalPanic,
                "OpenJOC FFmpeg wrapper contained an internal panic",
            ))
        }
    }

    fn send_packet_inner(&mut self, packet: PacketRef<'_>) -> Result<BridgeStatus, BridgeError> {
        if self.drain_requested || self.session_drained {
            return Err(BridgeError::new(
                BridgeErrorKind::EndOfStream,
                "cannot send input after drain; reset the wrapper first",
            ));
        }
        if self.classification == JocClassification::ConfirmedNonJoc {
            return Ok(BridgeStatus::NotJoc);
        }
        if !self.output.is_empty() {
            return Ok(BridgeStatus::WouldBlock);
        }
        if packet.discontinuity {
            self.reset_inner();
        }
        if packet.data.is_empty() {
            return Err(BridgeError::new(
                BridgeErrorKind::InvalidData,
                "empty AVPacket payload",
            ));
        }
        packet.time_base.validate()?;
        let new_len = self
            .staging
            .len()
            .checked_add(packet.data.len())
            .ok_or_else(|| {
                BridgeError::new(BridgeErrorKind::InvalidData, "compressed staging overflow")
            })?;
        if new_len > MAX_COMPRESSED_STAGING_BYTES {
            return Err(BridgeError::new(
                BridgeErrorKind::OutputPending,
                format!("compressed staging would exceed {MAX_COMPRESSED_STAGING_BYTES} bytes"),
            ));
        }
        let (raw_timestamp, source) = match (packet.pts, packet.dts, self.timestamp_policy) {
            (Some(pts), _, _) => (Some(pts), TimestampSource::Pts),
            (None, Some(dts), TimestampPolicy::PtsThenDts) => {
                (Some(dts), TimestampSource::DtsFallback)
            }
            _ => (None, TimestampSource::Absent),
        };
        let staging_started = Instant::now();
        let timestamp = raw_timestamp
            .map(|value| rescale_q_checked(value, packet.time_base, Rational::SAMPLE_TIME_BASE))
            .transpose()?;
        self.boundaries.push_back(Boundary {
            byte_offset: self.staging.len(),
            timestamp,
            source,
            preroll: packet.preroll,
        });
        self.staging.extend_from_slice(packet.data);
        self.timings.add_packet_staging(staging_started.elapsed());
        match self.pump()? {
            PumpResult::Frame => Ok(BridgeStatus::FrameAvailable),
            PumpResult::NotJoc => Ok(BridgeStatus::NotJoc),
            PumpResult::Eof => Ok(BridgeStatus::EndOfStream),
            PumpResult::Idle => Ok(BridgeStatus::NeedMoreInput),
        }
    }

    fn receive_frame_inner(&mut self) -> Result<ReceiveOutcome, BridgeError> {
        if let Some(frame) = self.output.pop_front() {
            return Ok(ReceiveOutcome::Frame(frame));
        }
        match self.pump()? {
            PumpResult::Frame => self.output.pop_front().map_or_else(
                || Ok(ReceiveOutcome::NeedMoreInput),
                |frame| Ok(ReceiveOutcome::Frame(frame)),
            ),
            PumpResult::NotJoc => Ok(ReceiveOutcome::NotJoc),
            PumpResult::Eof => Ok(ReceiveOutcome::EndOfStream),
            PumpResult::Idle => Ok(ReceiveOutcome::NeedMoreInput),
        }
    }

    fn drain_inner(&mut self) -> Result<BridgeStatus, BridgeError> {
        self.drain_requested = true;
        if !self.output.is_empty() {
            return Ok(BridgeStatus::WouldBlock);
        }
        match self.pump()? {
            PumpResult::Frame => Ok(BridgeStatus::FrameAvailable),
            PumpResult::NotJoc => Ok(BridgeStatus::NotJoc),
            PumpResult::Eof => Ok(BridgeStatus::EndOfStream),
            PumpResult::Idle => Ok(BridgeStatus::NeedMoreInput),
        }
    }

    fn pump(&mut self) -> Result<PumpResult, BridgeError> {
        if !self.output.is_empty() {
            return Ok(PumpResult::Frame);
        }
        if self.classification == JocClassification::ConfirmedNonJoc {
            return Ok(PumpResult::NotJoc);
        }
        loop {
            if !self.staging.is_empty() {
                let assembly_started = Instant::now();
                let size = match parse_access_unit(&self.staging, self.drain_requested)? {
                    AccessUnitParse::NeedMore => {
                        self.timings.add_assembly(assembly_started.elapsed());
                        return Ok(PumpResult::Idle);
                    }
                    AccessUnitParse::Complete(size) => size,
                };
                let bytes = self.staging[..size].to_vec();
                let inspection = inspect_complete_access_unit(&bytes)?;
                match inspection.classification {
                    JocClassification::ConfirmedJoc => {
                        if self.classification == JocClassification::Unknown {
                            self.classification = JocClassification::ConfirmedJoc;
                        }
                    }
                    JocClassification::ConfirmedNonJoc => {
                        if self.classification == JocClassification::ConfirmedJoc {
                            self.classification = JocClassification::InvalidOrUnsupported;
                            return Err(BridgeError::new(
                                BridgeErrorKind::InvalidData,
                                "JOC metadata disappeared after stream admission",
                            ));
                        }
                        self.classification = JocClassification::ConfirmedNonJoc;
                        self.staging.clear();
                        self.boundaries.clear();
                        return Ok(PumpResult::NotJoc);
                    }
                    JocClassification::InvalidOrUnsupported | JocClassification::Unknown => {
                        self.classification = JocClassification::InvalidOrUnsupported;
                        return Err(BridgeError::new(
                            BridgeErrorKind::InvalidData,
                            "E-AC-3 access unit is malformed or uses an unsupported JOC profile",
                        ));
                    }
                }
                let (pts_samples, timestamp_source) =
                    self.resolve_timestamp(size, inspection.unit.samples)?;
                let preroll = self
                    .boundaries
                    .front()
                    .is_some_and(|boundary| boundary.byte_offset == 0 && boundary.preroll);
                self.timings.add_assembly(assembly_started.elapsed());
                let session_started = Instant::now();
                if self.session.is_none() {
                    self.session =
                        Some(OpenJocSession::new(self.config.clone()).map_err(|error| {
                            BridgeError::new(BridgeErrorKind::InvalidConfig, error.to_string())
                        })?);
                }
                let session = self.session.as_mut().expect("session was initialized");
                session
                    .push_packet(OpenJocPacket {
                        data: &bytes,
                        pts_samples,
                        discontinuity: false,
                        preroll,
                    })
                    .map_err(|error| map_openjoc_error(&error))?;
                let mut frames = Vec::new();
                while let Some(frame) = session.receive_frame() {
                    frames.push(frame);
                }
                self.timings.add_session(session_started.elapsed());
                self.traces.push(AccessUnitTrace {
                    index: self.next_au_index,
                    byte_length: bytes.len(),
                    sha256: sha256_hex(&bytes),
                    pts_samples,
                    timestamp_source,
                    sample_count: inspection.unit.samples,
                    sample_rate: inspection.unit.sample_rate,
                    independent_frame_count: inspection.independent_frame_count,
                    dependent_frame_count: inspection.dependent_frame_count,
                });
                self.next_au_index = self.next_au_index.saturating_add(1);
                self.consume_staging(size);
                let reorder_started = Instant::now();
                for frame in frames {
                    self.output.push_back(reorder_frame(frame, &self.layout)?);
                }
                self.timings.add_reorder(reorder_started.elapsed());
                if !self.output.is_empty() {
                    return Ok(PumpResult::Frame);
                }
                continue;
            }

            if !self.drain_requested {
                return Ok(PumpResult::Idle);
            }
            if self.session_drained {
                return Ok(PumpResult::Eof);
            }
            let Some(session) = self.session.as_mut() else {
                self.session_drained = true;
                return Ok(PumpResult::Eof);
            };
            let session_started = Instant::now();
            session.drain().map_err(|error| map_openjoc_error(&error))?;
            let mut frames = Vec::new();
            while let Some(frame) = session.receive_frame() {
                frames.push(frame);
            }
            self.timings.add_session(session_started.elapsed());
            self.session_drained = true;
            let reorder_started = Instant::now();
            for frame in frames {
                self.output.push_back(reorder_frame(frame, &self.layout)?);
            }
            self.timings.add_reorder(reorder_started.elapsed());
            return if self.output.is_empty() {
                Ok(PumpResult::Eof)
            } else {
                Ok(PumpResult::Frame)
            };
        }
    }

    fn resolve_timestamp(
        &mut self,
        au_size: usize,
        samples: u16,
    ) -> Result<(Option<i64>, TimestampSource), BridgeError> {
        let start = self
            .boundaries
            .front()
            .copied()
            .filter(|entry| entry.byte_offset == 0);
        let explicit = start.and_then(|entry| entry.timestamp);
        let (pts, source) = match (self.timeline, explicit) {
            (Timeline::Unset, Some(value)) => (Some(value), start.unwrap().source),
            (Timeline::Unset | Timeline::Untimed, None) => (None, TimestampSource::Absent),
            (Timeline::Timed { next_pts }, Some(value)) if next_pts == value => {
                (Some(value), start.unwrap().source)
            }
            (Timeline::Timed { next_pts }, None) => (Some(next_pts), TimestampSource::Synthesized),
            (Timeline::Timed { next_pts }, Some(value)) => {
                return Err(BridgeError::new(
                    BridgeErrorKind::InvalidTimestamp,
                    format!("packet PTS maps to sample {value}, expected {next_pts}"),
                ));
            }
            (Timeline::Untimed, Some(_)) => {
                return Err(BridgeError::new(
                    BridgeErrorKind::InvalidTimestamp,
                    "a timestamp cannot begin after an untimestamped stream segment",
                ));
            }
        };
        for boundary in self
            .boundaries
            .iter()
            .filter(|entry| entry.byte_offset > 0 && entry.byte_offset < au_size)
        {
            if let Some(value) = boundary.timestamp {
                if pts != Some(value) {
                    return Err(BridgeError::new(
                        BridgeErrorKind::InvalidTimestamp,
                        "a packet fragment carries a conflicting mid-AU timestamp",
                    ));
                }
            }
        }
        self.timeline = if let Some(pts) = pts {
            Timeline::Timed {
                next_pts: pts.checked_add(i64::from(samples)).ok_or_else(|| {
                    BridgeError::new(
                        BridgeErrorKind::InvalidTimestamp,
                        "sample timeline overflow",
                    )
                })?,
            }
        } else {
            Timeline::Untimed
        };
        Ok((pts, source))
    }

    fn consume_staging(&mut self, bytes: usize) {
        self.staging.drain(..bytes);
        self.boundaries.retain(|entry| entry.byte_offset >= bytes);
        for boundary in &mut self.boundaries {
            boundary.byte_offset -= bytes;
        }
    }

    fn reset_inner(&mut self) {
        if let Some(session) = self.session.as_mut() {
            session.reset();
        }
        self.session = None;
        self.staging.clear();
        self.boundaries.clear();
        self.output.clear();
        self.traces.clear();
        self.classification = JocClassification::Unknown;
        self.timeline = Timeline::Unset;
        self.next_au_index = 0;
        self.drain_requested = false;
        self.session_drained = false;
        self.poisoned = false;
        self.timings = BridgeTimings::default();
    }
}

struct InspectedAccessUnit {
    classification: JocClassification,
    unit: AccessUnitIndex,
    independent_frame_count: usize,
    dependent_frame_count: usize,
}

/// Classifies exactly one complete access unit through OpenJOC's public parser
/// and both admitted validation profiles.
pub fn classify_complete_access_unit(bytes: &[u8]) -> JocClassification {
    inspect_complete_access_unit(bytes).map_or(JocClassification::InvalidOrUnsupported, |value| {
        value.classification
    })
}

fn inspect_complete_access_unit(bytes: &[u8]) -> Result<InspectedAccessUnit, BridgeError> {
    let frames = index_syncframes(bytes)
        .map_err(|error| BridgeError::new(BridgeErrorKind::InvalidData, error.to_string()))?;
    let units = group_access_units(&frames)
        .map_err(|error| BridgeError::new(BridgeErrorKind::InvalidData, error.to_string()))?;
    let Some(unit) = units.first().copied() else {
        return Err(BridgeError::new(
            BridgeErrorKind::InvalidData,
            "input contains no E-AC-3 access unit",
        ));
    };
    if units.len() != 1 || unit.first_frame != 0 || unit.frame_count != frames.len() {
        return Err(BridgeError::new(
            BridgeErrorKind::InvalidData,
            "classification requires exactly one complete access unit",
        ));
    }
    for entry in &frames {
        let frame = bytes.get(entry.offset..).ok_or_else(|| {
            BridgeError::new(
                BridgeErrorKind::InvalidData,
                "syncframe offset is outside input",
            )
        })?;
        parse_audio_frame(frame)
            .map_err(|error| BridgeError::new(BridgeErrorKind::InvalidData, error.to_string()))?;
    }
    let parsed = parse_joc_access_unit(bytes, &frames, unit)
        .map_err(|error| BridgeError::new(BridgeErrorKind::InvalidData, error.to_string()))?;
    let classification = match parsed {
        None => JocClassification::ConfirmedNonJoc,
        Some(parsed) => {
            let strict = validate_joc_access_unit(&parsed, JocValidationProfile::EtsiStrict);
            let compatible =
                validate_joc_access_unit(&parsed, JocValidationProfile::ObservedVendorCompat);
            if strict.is_ok() || compatible.is_ok() {
                JocClassification::ConfirmedJoc
            } else {
                JocClassification::InvalidOrUnsupported
            }
        }
    };
    let independent_frame_count = frames
        .iter()
        .filter(|entry| entry.header.stream_type == StreamType::Independent)
        .count();
    Ok(InspectedAccessUnit {
        classification,
        unit,
        independent_frame_count,
        dependent_frame_count: frames.len().saturating_sub(independent_frame_count),
    })
}

fn parse_access_unit(bytes: &[u8], eos: bool) -> Result<AccessUnitParse, BridgeError> {
    if bytes.len() < 8 {
        return if eos {
            Err(BridgeError::new(
                BridgeErrorKind::InvalidData,
                "truncated E-AC-3 syncframe header at EOF",
            ))
        } else {
            Ok(AccessUnitParse::NeedMore)
        };
    }
    let first = parse_syncframe_header(bytes)
        .map_err(|error| BridgeError::new(BridgeErrorKind::InvalidData, error.to_string()))?;
    if first.stream_type != StreamType::Independent || first.substream_id != 0 {
        return Err(BridgeError::new(
            BridgeErrorKind::Unsupported,
            "access unit does not start with independent substream zero",
        ));
    }
    if first.frame_size > MAX_SYNCFRAME_BYTES || first.sample_rate != SAMPLE_RATE {
        return Err(BridgeError::new(
            BridgeErrorKind::Unsupported,
            format!(
                "unsupported E-AC-3 frame size/rate: {} bytes at {} Hz",
                first.frame_size, first.sample_rate
            ),
        ));
    }
    if bytes.len() < first.frame_size {
        return if eos {
            Err(BridgeError::new(
                BridgeErrorKind::InvalidData,
                "truncated independent syncframe at EOF",
            ))
        } else {
            Ok(AccessUnitParse::NeedMore)
        };
    }
    if bytes.len() == first.frame_size {
        return if eos {
            Ok(AccessUnitParse::Complete(first.frame_size))
        } else {
            Ok(AccessUnitParse::NeedMore)
        };
    }
    if bytes.len() < first.frame_size + 8 {
        return if eos {
            Err(BridgeError::new(
                BridgeErrorKind::InvalidData,
                "partial second syncframe header at EOF",
            ))
        } else {
            Ok(AccessUnitParse::NeedMore)
        };
    }
    let second = parse_syncframe_header(&bytes[first.frame_size..])
        .map_err(|error| BridgeError::new(BridgeErrorKind::InvalidData, error.to_string()))?;
    if second.stream_type == StreamType::Independent && second.substream_id == 0 {
        return Ok(AccessUnitParse::Complete(first.frame_size));
    }
    if second.stream_type != StreamType::Dependent || second.substream_id != 0 {
        return Err(BridgeError::new(
            BridgeErrorKind::Unsupported,
            "unsupported substream order after independent substream zero",
        ));
    }
    if second.frame_size > MAX_SYNCFRAME_BYTES
        || second.sample_rate != SAMPLE_RATE
        || second.audio_blocks != first.audio_blocks
    {
        return Err(BridgeError::new(
            BridgeErrorKind::Unsupported,
            "dependent substream timing or size does not match I0",
        ));
    }
    let total = first
        .frame_size
        .checked_add(second.frame_size)
        .ok_or_else(|| {
            BridgeError::new(BridgeErrorKind::InvalidData, "access-unit size overflow")
        })?;
    if bytes.len() < total {
        return if eos {
            Err(BridgeError::new(
                BridgeErrorKind::InvalidData,
                "truncated dependent syncframe at EOF",
            ))
        } else {
            Ok(AccessUnitParse::NeedMore)
        };
    }
    Ok(AccessUnitParse::Complete(total))
}

fn channel_layout_for_config(config: &OpenJocConfig) -> Result<FfmpegChannelLayout, BridgeError> {
    if config.render_mode == RenderMode::Binaural {
        return build_layout(
            "binaural",
            Some("binaural"),
            &["Left Ear", "Right Ear"],
            &["BIL", "BIR"],
        );
    }
    let layout_name = if config.render_mode == RenderMode::Stereo {
        "2.0"
    } else {
        config.speaker_layout.as_str()
    };
    let preset = SpeakerLayoutPreset::for_name(layout_name)
        .map_err(|error| BridgeError::new(BridgeErrorKind::InvalidConfig, error.to_string()))?;
    let labels = preset.channel_labels();
    let (standard, channels): (Option<&str>, Vec<&str>) = match layout_name {
        "2.0" => (Some("stereo"), vec!["FL", "FR"]),
        "5.1" => (Some("5.1(side)"), vec!["FL", "FR", "FC", "LFE", "SL", "SR"]),
        "5.1.2" => (
            Some("5.1.2"),
            vec!["FL", "FR", "FC", "LFE", "SL", "SR", "TFL", "TFR"],
        ),
        "5.1.4" => (
            Some("5.1.4"),
            vec![
                "FL", "FR", "FC", "LFE", "SL", "SR", "TFL", "TFR", "TBL", "TBR",
            ],
        ),
        "7.1" => (
            Some("7.1"),
            vec!["FL", "FR", "FC", "LFE", "BL", "BR", "SL", "SR"],
        ),
        "7.1.2" => (
            Some("7.1.2"),
            vec![
                "FL", "FR", "FC", "LFE", "BL", "BR", "SL", "SR", "TFL", "TFR",
            ],
        ),
        "7.1.4" => (
            Some("7.1.4"),
            vec![
                "FL", "FR", "FC", "LFE", "BL", "BR", "SL", "SR", "TFL", "TFR", "TBL", "TBR",
            ],
        ),
        "22.2" => (
            Some("22.2"),
            vec![
                "FL", "FR", "FC", "LFE", "BL", "BR", "FLC", "FRC", "BC", "SL", "SR", "TC", "TFL",
                "TFC", "TFR", "TBL", "TBC", "TBR", "LFE2", "TSL", "TSR", "BFC", "BFL", "BFR",
            ],
        ),
        _ => (
            None,
            labels
                .iter()
                .map(|label| ffmpeg_channel_for_label(label))
                .collect::<Result<Vec<_>, _>>()?,
        ),
    };
    build_layout(layout_name, standard, &labels, &channels)
}

fn build_layout(
    name: &str,
    standard_layout: Option<&str>,
    openjoc_labels: &[impl AsRef<str>],
    ffmpeg_channels: &[impl AsRef<str>],
) -> Result<FfmpegChannelLayout, BridgeError> {
    if openjoc_labels.len() != ffmpeg_channels.len() {
        return Err(BridgeError::new(
            BridgeErrorKind::InvalidConfig,
            "FFmpeg and OpenJOC channel counts differ",
        ));
    }
    let mut permutation = Vec::with_capacity(ffmpeg_channels.len());
    for channel in ffmpeg_channels {
        let channel = channel.as_ref();
        let input = openjoc_labels
            .iter()
            .position(|label| ffmpeg_channel_for_label(label.as_ref()) == Ok(channel))
            .ok_or_else(|| {
                BridgeError::new(
                    BridgeErrorKind::InvalidConfig,
                    format!("FFmpeg channel {channel} has no OpenJOC semantic peer"),
                )
            })?;
        permutation.push(input);
    }
    let layout = FfmpegChannelLayout {
        name: name.to_owned(),
        standard_layout: standard_layout.map(str::to_owned),
        custom: standard_layout.is_none(),
        openjoc_order: openjoc_labels
            .iter()
            .map(|value| value.as_ref().to_owned())
            .collect(),
        ffmpeg_order: ffmpeg_channels
            .iter()
            .map(|value| value.as_ref().to_owned())
            .collect(),
        permutation,
    };
    let _ = layout.inverse_permutation()?;
    Ok(layout)
}

fn ffmpeg_channel_for_label(label: &str) -> Result<&'static str, BridgeError> {
    match label {
        "FL" => Ok("FL"),
        "FR" => Ok("FR"),
        "FC" => Ok("FC"),
        "LFE" | "LFE1" => Ok("LFE"),
        "LFE2" => Ok("LFE2"),
        "Ls" | "SiL" => Ok("SL"),
        "Rs" | "SiR" => Ok("SR"),
        "Lb" | "BL" => Ok("BL"),
        "Rb" | "BR" => Ok("BR"),
        "Lw" => Ok("WL"),
        "Rw" => Ok("WR"),
        "FLc" => Ok("FLC"),
        "FRc" => Ok("FRC"),
        "BC" => Ok("BC"),
        "TFL" | "TpFL" | "Ltf" => Ok("TFL"),
        "TFR" | "TpFR" | "Rtf" => Ok("TFR"),
        "Ltm" | "TpSiL" => Ok("TSL"),
        "Rtm" | "TpSiR" => Ok("TSR"),
        "TBL" | "TpBL" | "Ltr" => Ok("TBL"),
        "TBR" | "TpBR" | "Rtr" => Ok("TBR"),
        "TpFC" => Ok("TFC"),
        "TpC" => Ok("TC"),
        "TpBC" => Ok("TBC"),
        "BtFC" => Ok("BFC"),
        "BtFL" => Ok("BFL"),
        "BtFR" => Ok("BFR"),
        "Left Ear" => Ok("BIL"),
        "Right Ear" => Ok("BIR"),
        other => Err(BridgeError::new(
            BridgeErrorKind::Unsupported,
            format!("OpenJOC channel identity {other} has no FFmpeg AVChannel mapping"),
        )),
    }
}

fn reorder_frame(
    frame: OpenJocPcmFrame,
    layout: &FfmpegChannelLayout,
) -> Result<FfmpegFrame, BridgeError> {
    if frame.sample_rate != SAMPLE_RATE
        || frame.channel_count != layout.permutation.len()
        || frame.interleaved_f32.len() != frame.sample_count.saturating_mul(frame.channel_count)
    {
        return Err(BridgeError::new(
            BridgeErrorKind::InvalidData,
            "OpenJOC PCM does not match the configured FFmpeg frame shape",
        ));
    }
    let identity = layout
        .permutation
        .iter()
        .copied()
        .eq(0..layout.permutation.len());
    let interleaved_f32 = if identity {
        frame.interleaved_f32
    } else {
        let mut output = Vec::with_capacity(frame.interleaved_f32.len());
        for sample in 0..frame.sample_count {
            let base = sample * frame.channel_count;
            for &input in &layout.permutation {
                output.push(frame.interleaved_f32[base + input]);
            }
        }
        output
    };
    Ok(FfmpegFrame {
        format: "AV_SAMPLE_FMT_FLT",
        sample_rate: frame.sample_rate,
        nb_samples: frame.sample_count,
        pts: frame.pts_samples,
        duration: i64::try_from(frame.sample_count).map_err(|_| {
            BridgeError::new(BridgeErrorKind::InvalidData, "frame duration exceeds i64")
        })?,
        channel_layout: layout.clone(),
        interleaved_f32,
    })
}

fn map_openjoc_error(error: &openjoc_api::OpenJocError) -> BridgeError {
    use openjoc_api::OpenJocError;
    let kind = match error {
        OpenJocError::InvalidConfig(_) => BridgeErrorKind::InvalidConfig,
        OpenJocError::Unsupported(_) | OpenJocError::FormatChanged { .. } => {
            BridgeErrorKind::Unsupported
        }
        OpenJocError::OutputPending => BridgeErrorKind::OutputPending,
        OpenJocError::AlreadyDrained => BridgeErrorKind::EndOfStream,
        OpenJocError::TimestampDiscontinuity { .. } => BridgeErrorKind::InvalidTimestamp,
        _ => BridgeErrorKind::InvalidData,
    };
    BridgeError::new(kind, error.to_string())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut result = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(result, "{byte:02x}");
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use openjoc_api::{BinauralConfig, ValidationProfile};
    use std::collections::HashSet;
    use std::thread;
    #[cfg(feature = "ffmpeg")]
    use std::{fs, process::Command, time::SystemTime};

    #[derive(Default)]
    struct Bits(Vec<bool>);

    impl Bits {
        fn push(&mut self, value: u64, width: u8) {
            for shift in (0..width).rev() {
                self.0.push(value & (1_u64 << shift) != 0);
            }
        }

        fn set(&mut self, position: usize, value: u64, width: u8) {
            for index in 0..usize::from(width) {
                let shift = usize::from(width) - index - 1;
                self.0[position + index] = (value >> shift) & 1 != 0;
            }
        }

        fn padded_bytes(mut self) -> Vec<u8> {
            while self.0.len() % 8 != 0 {
                self.0.push(false);
            }
            let size = self.0.len() / 8;
            self.bytes(size)
        }

        fn bytes(self, size: usize) -> Vec<u8> {
            let mut bytes = vec![0_u8; size];
            for (index, bit) in self.0.into_iter().enumerate() {
                if bit {
                    bytes[index / 8] |= 0x80 >> (index % 8);
                }
            }
            bytes
        }
    }

    fn push(bits: &mut Vec<bool>, value: u64, width: u8) {
        for shift in (0..width).rev() {
            bits.push(value & (1_u64 << shift) != 0);
        }
    }

    fn pack(mut bits: Vec<bool>) -> Vec<u8> {
        while bits.len() % 8 != 0 {
            bits.push(false);
        }
        let mut bytes = vec![0_u8; bits.len() / 8];
        for (index, bit) in bits.into_iter().enumerate() {
            if bit {
                bytes[index / 8] |= 0x80 >> (index % 8);
            }
        }
        bytes
    }

    fn joc_emdf(oamd: &[u8], joc: &[u8]) -> Vec<u8> {
        let mut container = Bits::default();
        container.push(0, 2);
        container.push(0, 3);
        for (id, payload) in [(11, oamd), (14, joc)] {
            container.push(id, 5);
            container.push(0, 1);
            container.push(0, 1);
            container.push(1, 1);
            container.push(1, 2);
            container.push(0, 1);
            container.push(1, 1);
            container.push(0, 8);
            container.push(0, 1);
            container.push(1, 1);
            container.push(0, 1);
            container.push(0, 1);
            container.push(0, 5);
            container.push(0, 2);
            container.push(u64::try_from(payload.len()).expect("payload length"), 8);
            container.push(0, 1);
            for byte in payload {
                container.push(u64::from(*byte), 8);
            }
        }
        container.push(0, 5);
        container.push(1, 2);
        container.push(0, 2);
        container.push(0, 8);
        let container = container.padded_bytes();
        let mut emdf = vec![0x58, 0x38];
        emdf.extend_from_slice(
            &u16::try_from(container.len())
                .expect("container length")
                .to_be_bytes(),
        );
        emdf.extend_from_slice(&container);
        emdf
    }

    fn inactive_oamd() -> Vec<u8> {
        let mut bits = Vec::new();
        for (value, width) in [
            (0, 2),
            (0, 5),
            (1, 1),
            (0, 1),
            (0, 1),
            (1, 4),
            (1, 4),
            (2, 4),
            (0, 1),
            (0, 1),
            (0, 2),
            (0, 3),
            (0, 6),
            (0, 2),
            (1, 1),
            (1, 1),
            (0, 1),
            (0, 7),
        ] {
            push(&mut bits, value, width);
        }
        pack(bits)
    }

    fn five_channel_audio_frame_with_exponent_codes(
        emdf: &[u8],
        joc_extension: bool,
        exponent_delta_codes: [u8; 5],
        lfe_grouped_mantissa_codes: Option<[[u8; 3]; 6]>,
    ) -> Vec<u8> {
        assert!(exponent_delta_codes.iter().all(|code| *code < 125));
        assert!(
            lfe_grouped_mantissa_codes
                .iter()
                .flatten()
                .flatten()
                .all(|code| *code < 27)
        );
        let lfe_on = lfe_grouped_mantissa_codes.is_some();
        let size = 4096;
        let mut bits = Bits::default();
        for (value, width) in [
            (0x0b77, 16),
            (0, 2),
            (0, 3),
            (2047, 11),
            (0, 2),
            (3, 2),
            (7, 3),
            (u64::from(lfe_on), 1),
            (16, 5),
            (31, 5),
            (0, 1),
            (0, 1),
            (0, 1),
            (u64::from(joc_extension), 1),
        ] {
            bits.push(value, width);
        }
        if joc_extension {
            bits.push(1, 6);
            bits.push(0x01, 8);
            bits.push(1, 8);
        }
        bits.push(1, 1);
        bits.push(0, 1);
        bits.push(if lfe_on { 2 } else { 0 }, 2);
        bits.push(0, 1);
        bits.push(0, 7);
        bits.push(0, 1);
        for _ in 1..6 {
            bits.push(0, 1);
        }
        for block in 0..6 {
            for _ in 0..5 {
                bits.push(u64::from(block == 0), 2);
            }
        }
        if lfe_on {
            bits.push(1, 1);
            for _ in 1..6 {
                bits.push(0, 1);
            }
        }
        for _ in 0..5 {
            bits.push(0, 5);
        }
        if !lfe_on {
            bits.push(0, 6);
            bits.push(0, 4);
        }
        bits.push(0, 1);
        bits.push(0, 1);
        bits.push(0, 1);
        for _ in 0..5 {
            bits.push(0, 6);
        }
        for exponent_delta_code in exponent_delta_codes {
            bits.push(if lfe_on { 15 } else { 0 }, 4);
            for _ in 0..24 {
                bits.push(u64::from(exponent_delta_code), 7);
            }
            bits.push(0, 2);
        }
        if let Some(lfe_codes) = lfe_grouped_mantissa_codes {
            bits.push(0, 4); // LFE initial exponent
            bits.push(62, 7);
            bits.push(62, 7);
            bits.push(5, 6); // coarse SNR: beds BAP 0, LFE BAP 1
            for _ in 0..5 {
                bits.push(0, 4); // five bed fine SNR codes
            }
            bits.push(15, 4); // LFE fine SNR code
            bits.push(0, 1); // converter SNR offset absent
            for code in lfe_codes[0] {
                bits.push(u64::from(code), 5);
            }
            for block_codes in &lfe_codes[1..] {
                bits.push(0, 1); // dynamic range absent
                bits.push(0, 1); // SPX strategy reused
                bits.push(0, 1); // SNR offsets reused
                bits.push(0, 1); // converter SNR offset absent
                for code in block_codes {
                    bits.push(u64::from(*code), 5);
                }
            }
        } else {
            bits.push(0, 1);
            for _ in 1..6 {
                bits.push(0, 1);
                bits.push(0, 1);
                bits.push(0, 1);
            }
        }
        bits.0.resize(size * 8, false);
        if joc_extension {
            let auxdatae_position = size * 8 - 18;
            let length_position = auxdatae_position - 14;
            bits.set(
                length_position,
                u64::try_from(emdf.len() * 8).expect("EMDF bit length"),
                14,
            );
            bits.set(auxdatae_position, 1, 1);
            let start = length_position - emdf.len() * 8;
            for (index, byte) in emdf.iter().copied().enumerate() {
                bits.set(start + index * 8, u64::from(byte), 8);
            }
        }
        bits.bytes(size)
    }

    fn five_channel_audio_frame(emdf: &[u8], joc_extension: bool) -> Vec<u8> {
        five_channel_audio_frame_with_exponent_codes(emdf, joc_extension, [62; 5], None)
    }

    fn synthetic_joc_frame() -> Vec<u8> {
        five_channel_audio_frame(&joc_emdf(&inactive_oamd(), &one_object_joc()), true)
    }

    fn active_object_oamd(x: u8, y: u8, z: i8) -> Vec<u8> {
        assert!(x <= 62 && y <= 62 && (-15..=15).contains(&z));
        let positions = [
            (x, y, z),
            (15, 4, 15),
            (47, 5, 12),
            (13, 55, 10),
            (50, 52, 14),
        ];

        let mut body = Bits::default();
        body.push(0, 1); // discard_unknown
        body.push(0, 2); // sample offset
        body.push(
            u64::try_from(positions.len() - 1).expect("OAMD block count"),
            3,
        );
        for factor in [0, 9, 18, 27, 36] {
            body.push(factor, 6);
            body.push(0, 2); // zero ramp
        }
        body.push(1, 1); // no reserved object-element data
        for (block, (x, y, z)) in positions.into_iter().enumerate() {
            body.push(0, 1); // active object
            if block == 0 {
                body.push(0, 2); // 0 dB object gain
                body.push(1, 1); // default priority
            } else {
                body.push(2, 2); // reuse basic information
                body.push(3, 2); // update selected render information
                body.push(8, 4); // position only
                body.push(0, 1); // absolute position
            }
            body.push(u64::from(x), 6);
            body.push(u64::from(y), 6);
            body.push(u64::from(z >= 0), 1);
            body.push(u64::from(z.unsigned_abs()), 4);
            body.push(0, 1); // inside-room distance
            if block == 0 {
                body.push(0, 3); // include horizontal zones
                body.push(1, 1); // include elevation zone
                body.push(0, 2); // point object
                body.push(0, 1); // room anchored
            }
            body.push(0, 1); // channel lock disabled
            body.push(0, 1); // no additional table data
        }
        let body = body.padded_bytes();
        assert!(!body.is_empty() && body.len() <= 31);

        let mut payload = Bits::default();
        payload.push(0, 2); // syntax version
        payload.push(0, 5); // one object
        payload.push(1, 1); // dynamic-only programme assignment
        payload.push(0, 1); // no LFE object
        payload.push(0, 1); // no alternate object data
        payload.push(1, 4); // one element
        payload.push(1, 4); // object element
        let size_minus_one = body.len() - 1;
        if size_minus_one < 16 {
            payload.push(u64::try_from(size_minus_one).expect("OAMD body size"), 4);
            payload.push(0, 1);
        } else {
            payload.push(0, 4);
            payload.push(1, 1);
            payload.push(
                u64::try_from(size_minus_one - 16).expect("continued OAMD body size"),
                4,
            );
            payload.push(0, 1);
        }
        for byte in body {
            payload.push(u64::from(byte), 8);
        }
        payload.padded_bytes()
    }

    fn synthetic_joc_fingerprint_stream() -> Vec<u8> {
        const POSITIONS: [(u8, u8, i8); 12] = [
            (15, 4, 15),
            (53, 7, 3),
            (4, 11, -12),
            (60, 39, -4),
            (9, 26, 8),
            (42, 61, -9),
            (29, 4, 15),
            (56, 20, 1),
            (14, 47, -15),
            (37, 15, 11),
            (2, 33, -2),
            (48, 52, 6),
        ];
        let mut stream = Vec::with_capacity(POSITIONS.len() * 4096);
        for (index, (x, y, z)) in POSITIONS.into_iter().enumerate() {
            let _index = u8::try_from(index).expect("bounded fingerprint frame index");
            // Each base channel receives a distinct, valid grouped D15
            // exponent path. Every non-zero path returns to exponent zero
            // within one group, so all 24 groups remain in range. With the
            // syntax's enabled zero-BAP dither this yields five distinct,
            // bounded bed excitation time series without private media.
            let exponent_delta_codes = [62, 82, 86, 102, 106];
            let lfe_codes = std::array::from_fn(|block| {
                std::array::from_fn(|group| {
                    u8::try_from((index * 7 + block * 3 + group) % 27)
                        .expect("bounded LFE grouped mantissa code")
                })
            });
            let frame = five_channel_audio_frame_with_exponent_codes(
                &joc_emdf(&active_object_oamd(x, y, z), &one_object_joc()),
                true,
                exponent_delta_codes,
                Some(lfe_codes),
            );
            stream.extend_from_slice(&frame);
        }
        stream
    }

    const SYNTHETIC_JOC_LIFECYCLE_FRAME_COUNT: usize = 128;

    fn synthetic_joc_lifecycle_stream() -> Vec<u8> {
        const JOC_SEQUENCE_MAX: usize = 1023;
        const JOC_SEQUENCE_FIRST: usize = 1;
        let frame_count = SYNTHETIC_JOC_LIFECYCLE_FRAME_COUNT;
        let mut stream = Vec::with_capacity(frame_count * 4096);
        for index in 0..frame_count {
            let x = u8::try_from((index * 17 + 15) % 63).expect("bounded lifecycle x");
            let y = u8::try_from((index * 29 + 4) % 63).expect("bounded lifecycle y");
            let z = i8::try_from((index * 11) % 31).expect("bounded lifecycle z") - 15;
            let lfe_codes = std::array::from_fn(|block| {
                std::array::from_fn(|group| {
                    u8::try_from((index * 13 + block * 5 + group * 3) % 27)
                        .expect("bounded lifecycle LFE code")
                })
            });
            let sequence_count = u16::try_from(index % JOC_SEQUENCE_MAX + JOC_SEQUENCE_FIRST)
                .expect("bounded lifecycle JOC sequence count");
            stream.extend_from_slice(&five_channel_audio_frame_with_exponent_codes(
                &joc_emdf(
                    &active_object_oamd(x, y, z),
                    &one_object_joc_with_sequence(sequence_count),
                ),
                true,
                [62, 82, 86, 102, 106],
                Some(lfe_codes),
            ));
        }
        stream
    }

    fn assert_lifecycle_payload_sequence_contract() {
        use openjoc_scene::{JocFrameInput, PayloadDecoder, PayloadDecoderConfig};

        let mut decoder = PayloadDecoder::streaming(PayloadDecoderConfig {
            reference_screen: None,
            oamd: Default::default(),
        });
        let downmix = vec![vec![0.0_f64; 1536]; 5];
        let mut callbacks = 0usize;
        for index in 0..SYNTHETIC_JOC_LIFECYCLE_FRAME_COUNT {
            let sequence_count = u16::try_from(index % 1023 + 1).expect("bounded JOC sequence");
            let joc_payload = one_object_joc_with_sequence(sequence_count);
            let parsed = openjoc_joc::parse_joc_payload(&joc_payload).expect("parse lifecycle JOC");
            assert_eq!(parsed.sequence_count, sequence_count);
            let oamd_payload = active_object_oamd(
                u8::try_from((index * 17 + 15) % 63).expect("bounded lifecycle x"),
                u8::try_from((index * 29 + 4) % 63).expect("bounded lifecycle y"),
                i8::try_from((index * 11) % 31).expect("bounded lifecycle z") - 15,
            );
            decoder
                .decode_frame_with(
                    JocFrameInput {
                        sample_rate: SAMPLE_RATE,
                        downmix_pcm: &downmix,
                        base_lfe_pcm: None,
                        joc_payload: &joc_payload,
                        oamd_payload: &oamd_payload,
                        frame_index: u64::try_from(index).expect("bounded frame index"),
                    },
                    |frame| {
                        callbacks += 1;
                        assert_eq!(frame.frame_index, u64::try_from(index).unwrap());
                        assert_eq!(frame.sample_range.start_sample, (index * 1536) as u64);
                        assert_eq!(frame.sample_range.end_sample, ((index + 1) * 1536) as u64);
                        assert!(!frame.decoded.state_reset);
                        assert!(!frame.decoded.reconstruction_basis.rows.is_empty());
                        assert!(
                            frame
                                .decoded
                                .reconstruction_basis
                                .rows
                                .iter()
                                .all(|row| row.len() == 1536
                                    && row.iter().all(|value| value.is_finite()))
                        );
                        Ok::<(), openjoc_scene::PayloadDecodeError>(())
                    },
                )
                .expect("decode lifecycle payload frame");
        }
        assert_eq!(callbacks, SYNTHETIC_JOC_LIFECYCLE_FRAME_COUNT);
        decoder
            .finish_streaming()
            .expect("finish lifecycle payload stream");
    }

    fn assert_lifecycle_fixture_streaming_contract(stream: &[u8]) {
        assert_lifecycle_payload_sequence_contract();
        let config = OpenJocConfig {
            render_mode: RenderMode::Stereo,
            speaker_layout: "2.0".to_owned(),
            validation_profile: ValidationProfile::EtsiStrict,
            ..OpenJocConfig::default()
        };
        let mut session = OpenJocSession::new(config).expect("lifecycle fixture session");
        let expected_access_units = SYNTHETIC_JOC_LIFECYCLE_FRAME_COUNT;
        assert_eq!(stream.len(), expected_access_units * 4096);
        let expected_samples = expected_access_units * 1536 + FINAL_LINKED_GAIN_LATENCY_SAMPLES;

        for segment_origin in [0_i64, 480_000_i64] {
            let mut output = Vec::new();
            for (index, frame) in stream.chunks_exact(4096).enumerate() {
                session
                    .push_packet(OpenJocPacket {
                        data: frame,
                        pts_samples: Some(
                            segment_origin + i64::try_from(index * 1536).expect("lifecycle PTS"),
                        ),
                        discontinuity: false,
                        preroll: false,
                    })
                    .expect("decode lifecycle fixture access unit");
                while let Some(frame) = session.receive_frame() {
                    output.push(frame);
                }
            }
            session.drain().expect("drain lifecycle fixture");
            while let Some(frame) = session.receive_frame() {
                output.push(frame);
            }

            assert!(!output.is_empty(), "lifecycle fixture produced no PCM");
            assert!(output.iter().all(|frame| {
                frame.sample_rate == SAMPLE_RATE
                    && frame.channel_count == 2
                    && frame.interleaved_f32.len() == frame.sample_count * frame.channel_count
                    && frame.interleaved_f32.iter().all(|value| value.is_finite())
            }));
            assert_eq!(
                output.iter().map(|frame| frame.sample_count).sum::<usize>(),
                expected_samples,
                "continuous lifecycle input must conserve programme samples plus the declared linked-gain tail"
            );
            let mut expected_pts = segment_origin;
            for frame in &output {
                assert_eq!(
                    frame.pts_samples,
                    Some(expected_pts),
                    "lifecycle PCM PTS must be contiguous within a segment"
                );
                expected_pts += i64::try_from(frame.sample_count).expect("bounded PCM frame");
            }
            assert_eq!(
                expected_pts,
                segment_origin + i64::try_from(expected_samples).expect("bounded sample count")
            );
            assert!(session.is_drained());
            assert!(session.receive_frame().is_none());

            session.reset();
        }
    }

    fn decode_policy_channel_fingerprints(
        stream: &[u8],
        render_mode: RenderMode,
        layout: &str,
    ) -> Vec<String> {
        assert_eq!(stream.len() % 4096, 0);
        let config = OpenJocConfig {
            render_mode,
            speaker_layout: layout.to_owned(),
            validation_profile: ValidationProfile::EtsiStrict,
            ..OpenJocConfig::default()
        };
        let mut session = OpenJocSession::new(config).expect("fingerprint fixture policy session");
        let mut channels: Option<Vec<Vec<u8>>> = None;
        for (index, frame) in stream.chunks_exact(4096).enumerate() {
            session
                .push_packet(OpenJocPacket {
                    data: frame,
                    pts_samples: Some(
                        i64::try_from(index * 1536).expect("fingerprint fixture PTS"),
                    ),
                    discontinuity: false,
                    preroll: false,
                })
                .expect("decode fingerprint fixture access unit");
            while let Some(frame) = session.receive_frame() {
                let output = channels.get_or_insert_with(|| vec![Vec::new(); frame.channel_count]);
                assert_eq!(output.len(), frame.channel_count);
                assert_eq!(
                    frame.interleaved_f32.len(),
                    frame.sample_count * frame.channel_count
                );
                for sample in frame.interleaved_f32.chunks_exact(frame.channel_count) {
                    for (channel, value) in sample.iter().enumerate() {
                        assert!(value.is_finite());
                        output[channel].extend_from_slice(&value.to_bits().to_le_bytes());
                    }
                }
            }
        }
        session.drain().expect("drain fingerprint fixture");
        while let Some(frame) = session.receive_frame() {
            let output = channels.get_or_insert_with(|| vec![Vec::new(); frame.channel_count]);
            assert_eq!(output.len(), frame.channel_count);
            for sample in frame.interleaved_f32.chunks_exact(frame.channel_count) {
                for (channel, value) in sample.iter().enumerate() {
                    assert!(value.is_finite());
                    output[channel].extend_from_slice(&value.to_bits().to_le_bytes());
                }
            }
        }
        channels
            .expect("fingerprint fixture must produce PCM")
            .into_iter()
            .map(|bytes| {
                assert!(!bytes.is_empty());
                sha256_hex(&bytes)
            })
            .collect()
    }

    fn assert_fingerprint_fixture_distinguishes_every_policy(stream: &[u8]) {
        const POLICIES: [(RenderMode, &str, usize); 7] = [
            (RenderMode::Stereo, "2.0", 2),
            (RenderMode::Speaker, "5.1", 6),
            (RenderMode::Speaker, "7.1", 8),
            (RenderMode::Speaker, "5.1.2", 8),
            (RenderMode::Speaker, "5.1.4", 10),
            (RenderMode::Speaker, "7.1.2", 10),
            (RenderMode::Speaker, "7.1.4", 12),
        ];
        for (mode, layout, channel_count) in POLICIES {
            let first = decode_policy_channel_fingerprints(stream, mode, layout);
            let second = decode_policy_channel_fingerprints(stream, mode, layout);
            assert_eq!(
                first, second,
                "{layout} channel fingerprints must be stable"
            );
            assert_eq!(first.len(), channel_count, "{layout} channel count");
            assert_eq!(
                first.iter().collect::<HashSet<_>>().len(),
                channel_count,
                "{layout} channel fingerprints must be pairwise distinct: {first:?}"
            );
        }
    }

    #[test]
    fn fingerprint_fixture_snr_separates_bed_dither_from_lfe_mantissas() {
        let parameters = openjoc_eac3::BitAllocationParameters {
            slow_decay_code: 2,
            fast_decay_code: 1,
            slow_gain_code: 1,
            db_per_bit_code: 2,
            floor_code: 7,
        };
        const EXPONENT_CODES: [u8; 5] = [62, 82, 86, 102, 106];
        for code in EXPONENT_CODES {
            let exponents = openjoc_eac3::decode_exponents(15, &[code; 24], 1, 73)
                .expect("bed exponent fixture");
            let baps = openjoc_eac3::compute_element_bap(
                &exponents, 0, 73, parameters, 4, 5, 0, 0, None, None,
            )
            .expect("bed BAP fixture");
            assert!(baps.iter().all(|bap| *bap == 0));
        }
        let lfe_baps =
            openjoc_eac3::compute_element_bap(&[0; 7], 0, 7, parameters, 4, 5, 15, 0, None, None)
                .expect("LFE BAP fixture");
        assert_eq!(lfe_baps, [1; 7]);
    }

    #[test]
    fn export_synthetic_joc_fixture_when_requested() {
        let Some(path) = std::env::var_os("OPENJOC_SYNTHETIC_JOC_PATH") else {
            return;
        };
        let frame = synthetic_joc_frame();
        assert_eq!(
            sha256_hex(&frame),
            "54b48754b915cef97c13752de5eace4a219da6599cdfcf26f92b5b6fffc6e3e4",
            "the established single-AU compatibility fixture must remain byte-stable"
        );
        let mut stream = Vec::with_capacity(frame.len() * 8);
        for _ in 0..8 {
            stream.extend_from_slice(&frame);
        }
        std::fs::write(path, stream).expect("write requested synthetic JOC fixture");
    }

    #[test]
    fn export_synthetic_joc_fingerprint_fixture_when_requested() {
        let Some(path) = std::env::var_os("OPENJOC_FINGERPRINT_JOC_PATH") else {
            return;
        };
        let stream = synthetic_joc_fingerprint_stream();
        assert_fingerprint_fixture_distinguishes_every_policy(&stream);
        std::fs::write(path, stream).expect("write requested fingerprint JOC fixture");
    }

    #[test]
    fn export_synthetic_joc_lifecycle_fixture_when_requested() {
        let Some(path) = std::env::var_os("OPENJOC_LIFECYCLE_JOC_PATH") else {
            return;
        };
        let stream = synthetic_joc_lifecycle_stream();
        assert_lifecycle_fixture_streaming_contract(&stream);
        std::fs::write(path, stream).expect("write requested lifecycle JOC fixture");
    }

    fn huffman_codeword_for(nodes: &[[i16; 2]], wanted: u16) -> Vec<bool> {
        fn visit(nodes: &[[i16; 2]], node: usize, wanted: u16, path: &mut Vec<bool>) -> bool {
            for branch in 0..2 {
                path.push(branch != 0);
                let child = nodes[node][branch];
                if child > 0 {
                    if visit(
                        nodes,
                        usize::try_from(child).expect("Huffman node"),
                        wanted,
                        path,
                    ) {
                        return true;
                    }
                } else if u16::try_from(-i32::from(child) - 1) == Ok(wanted) {
                    return true;
                }
                path.pop();
            }
            false
        }
        let mut path = Vec::new();
        assert!(visit(nodes, 0, wanted, &mut path));
        path
    }

    fn one_object_joc_with_sequence(sequence_count: u16) -> Vec<u8> {
        assert!(sequence_count <= 1023);
        let mut bits = Vec::new();
        for (value, width) in [
            (0, 3),
            (0, 6),
            (0, 3),
            (2, 3),
            (17, 5),
            (u64::from(sequence_count), 10),
        ] {
            push(&mut bits, value, width);
        }
        push(&mut bits, 1, 1);
        push(&mut bits, 0, 3);
        push(&mut bits, 0, 1);
        push(&mut bits, 0, 1);
        push(&mut bits, 0, 1);
        push(&mut bits, 0, 1);
        let codeword = huffman_codeword_for(openjoc_joc::all_huffman_tables()[0].nodes, 48);
        for _ in 0..5 {
            bits.extend_from_slice(&codeword);
        }
        pack(bits)
    }

    fn one_object_joc() -> Vec<u8> {
        one_object_joc_with_sequence(42)
    }

    fn ordinary_eac3_frame() -> Vec<u8> {
        five_channel_audio_frame(&[], false)
    }

    fn packet(data: &[u8], pts: Option<i64>) -> PacketRef<'_> {
        PacketRef {
            data,
            pts,
            dts: pts,
            duration: Some(1536),
            time_base: Rational::SAMPLE_TIME_BASE,
            stream_index: 0,
            discontinuity: false,
            preroll: false,
        }
    }

    fn indexed_syncframe(stream_type: u8, size: usize) -> Vec<u8> {
        let mut bytes = vec![0_u8; size];
        let mut cursor = 0;
        for (value, width) in [
            (0x0b77_u64, 16_usize),
            (u64::from(stream_type), 2),
            (0, 3),
            (u64::try_from(size / 2 - 1).expect("frame words"), 11),
            (0, 2),
            (3, 2),
        ] {
            for shift in (0..width).rev() {
                if value & (1_u64 << shift) != 0 {
                    bytes[cursor / 8] |= 0x80 >> (cursor % 8);
                }
                cursor += 1;
            }
        }
        bytes
    }

    fn collect_wrapper(
        config: OpenJocConfig,
        first: &[u8],
        second: &[u8],
    ) -> (FfmpegDecoder, Vec<FfmpegFrame>) {
        let mut decoder = FfmpegDecoder::new(config).expect("wrapper");
        let split = first.len() / 3;
        assert_eq!(
            decoder
                .send_packet(packet(&first[..split], Some(0)))
                .expect("first fragment"),
            BridgeStatus::NeedMoreInput
        );
        assert_eq!(
            decoder
                .send_packet(packet(&first[split..], None))
                .expect("second fragment"),
            BridgeStatus::NeedMoreInput
        );
        assert!(matches!(
            decoder
                .send_packet(packet(second, Some(1536)))
                .expect("grouped boundary"),
            BridgeStatus::NeedMoreInput | BridgeStatus::FrameAvailable
        ));
        let mut output = Vec::new();
        loop {
            match decoder.receive_frame().expect("receive before drain") {
                ReceiveOutcome::Frame(frame) => output.push(frame),
                ReceiveOutcome::NeedMoreInput => break,
                other => panic!("unexpected receive state {other:?}"),
            }
        }
        let _ = decoder.drain().expect("request drain");
        loop {
            match decoder.receive_frame().expect("receive drain") {
                ReceiveOutcome::Frame(frame) => output.push(frame),
                ReceiveOutcome::EndOfStream => break,
                ReceiveOutcome::NeedMoreInput => {}
                ReceiveOutcome::NotJoc => panic!("synthetic fixture classified non-JOC"),
            }
        }
        (decoder, output)
    }

    fn collect_direct(config: OpenJocConfig, first: &[u8], second: &[u8]) -> Vec<OpenJocPcmFrame> {
        let mut session = OpenJocSession::new(config).expect("direct session");
        let mut output = Vec::new();
        for (index, data) in [first, second].into_iter().enumerate() {
            session
                .push_packet(OpenJocPacket {
                    data,
                    pts_samples: Some(i64::try_from(index * 1536).expect("PTS")),
                    discontinuity: false,
                    preroll: false,
                })
                .expect("direct push");
            while let Some(frame) = session.receive_frame() {
                output.push(frame);
            }
        }
        let _ = session.drain().expect("direct drain");
        while let Some(frame) = session.receive_frame() {
            output.push(frame);
        }
        output
    }

    fn wrapper_in_openjoc_order(frames: &[FfmpegFrame]) -> Vec<(Option<i64>, Vec<f32>)> {
        frames
            .iter()
            .map(|frame| {
                let inverse = frame
                    .channel_layout
                    .inverse_permutation()
                    .expect("inverse permutation");
                let channels = inverse.len();
                let mut pcm = Vec::with_capacity(frame.interleaved_f32.len());
                for sample in 0..frame.nb_samples {
                    let base = sample * channels;
                    for &output_channel in &inverse {
                        pcm.push(frame.interleaved_f32[base + output_channel]);
                    }
                }
                (frame.pts, pcm)
            })
            .collect()
    }

    #[test]
    fn rational_timestamp_conversion_is_exact_and_drift_free() {
        for (time_base, step) in [
            (Rational::new(1, 48_000), 1536_i64),
            (Rational::new(1, 90_000), 2880),
            (Rational::new(1, 1000), 32),
            (Rational::new(1001, 30_000), 960),
        ] {
            for index in [0_i64, 1, 10, 10_000, 1_000_000] {
                let timestamp = index * step;
                let converted = rescale_q_checked(timestamp, time_base, Rational::SAMPLE_TIME_BASE)
                    .expect("rescale");
                let expected = rescale_q_checked(timestamp, time_base, Rational::SAMPLE_TIME_BASE)
                    .expect("independent rescale");
                assert_eq!(converted, expected);
                if time_base != Rational::new(1001, 30_000) {
                    assert_eq!(converted, index * 1536);
                }
            }
        }
        assert_eq!(
            rescale_q_checked(-2880, Rational::new(1, 90_000), Rational::SAMPLE_TIME_BASE)
                .expect("negative rescale"),
            -1536
        );
        assert!(
            rescale_q_checked(
                AV_NOPTS_VALUE,
                Rational::new(1, 90_000),
                Rational::SAMPLE_TIME_BASE
            )
            .is_err()
        );
    }

    #[test]
    fn assembler_handles_independent_dependent_fragmentation_and_grouping() {
        let independent = indexed_syncframe(0, 16);
        let dependent = indexed_syncframe(1, 16);
        let next = indexed_syncframe(0, 16);
        assert!(matches!(
            parse_access_unit(&independent, false).expect("pending independent"),
            AccessUnitParse::NeedMore
        ));
        assert!(matches!(
            parse_access_unit(&[independent.clone(), dependent.clone()].concat(), false)
                .expect("I0 plus D0"),
            AccessUnitParse::Complete(32)
        ));
        assert!(matches!(
            parse_access_unit(&[independent.clone(), next].concat(), false)
                .expect("next AU proves boundary"),
            AccessUnitParse::Complete(16)
        ));
        assert!(parse_access_unit(&[independent, dependent[..4].to_vec()].concat(), true).is_err());
    }

    #[test]
    fn every_public_layout_has_exact_semantic_roundtrip() {
        for name in openjoc_scene::SPEAKER_LAYOUT_PRESET_NAMES {
            let decoder = FfmpegDecoder::new(OpenJocConfig {
                render_mode: RenderMode::Speaker,
                speaker_layout: name.to_owned(),
                ..OpenJocConfig::default()
            })
            .expect("layout wrapper");
            let layout = decoder.channel_layout();
            assert_eq!(layout.openjoc_order.len(), layout.ffmpeg_order.len());
            let inverse = layout.inverse_permutation().expect("semantic inverse");
            for (input, &output) in inverse.iter().enumerate() {
                assert_eq!(layout.permutation[output], input);
                assert_eq!(
                    ffmpeg_channel_for_label(&layout.openjoc_order[input]).expect("mapped channel"),
                    layout.ffmpeg_order[output]
                );
            }
        }
        let binaural = FfmpegDecoder::new(OpenJocConfig {
            render_mode: RenderMode::Binaural,
            speaker_layout: "7.1.4".to_owned(),
            binaural: Some(BinauralConfig::builtin_generic("7.1.4")),
            ..OpenJocConfig::default()
        })
        .expect("binaural wrapper");
        assert_eq!(binaural.channel_layout().ffmpeg_order, ["BIL", "BIR"]);
        assert_eq!(
            binaural.channel_layout().standard_layout.as_deref(),
            Some("binaural")
        );
    }

    #[test]
    fn twenty_two_two_uses_verified_ffmpeg_native_order() {
        let decoder = FfmpegDecoder::new(OpenJocConfig {
            render_mode: RenderMode::Speaker,
            speaker_layout: "22.2".to_owned(),
            ..OpenJocConfig::default()
        })
        .expect("22.2 wrapper");
        assert_eq!(
            decoder.channel_layout().permutation,
            [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 10, 11, 15, 12, 14, 13, 16, 20, 17, 9, 18, 19, 21, 22,
                23
            ]
        );
        assert_eq!(
            decoder.channel_layout().standard_layout.as_deref(),
            Some("22.2")
        );
    }

    #[test]
    fn admission_distinguishes_joc_ordinary_and_invalid_data() {
        let joc = synthetic_joc_frame();
        let ordinary = ordinary_eac3_frame();
        assert_eq!(
            classify_complete_access_unit(&joc),
            JocClassification::ConfirmedJoc
        );
        assert_eq!(
            classify_complete_access_unit(&ordinary),
            JocClassification::ConfirmedNonJoc
        );
        assert_eq!(
            classify_complete_access_unit(&[0xde, 0xad, 0xbe, 0xef]),
            JocClassification::InvalidOrUnsupported
        );

        let mut classifier = JocClassifier::new();
        let mut result = JocClassification::Unknown;
        for chunk in joc.chunks(5) {
            result = classifier.send_chunk(chunk).expect("classify JOC chunk");
            if result != JocClassification::Unknown {
                break;
            }
        }
        if result == JocClassification::Unknown {
            result = classifier.finish().expect("finish JOC classification");
        }
        assert_eq!(result, JocClassification::ConfirmedJoc);
        assert_eq!(classifier.inspected_bytes(), joc.len());

        let mut classifier = JocClassifier::new();
        assert_eq!(
            classifier
                .send_chunk(&ordinary)
                .expect("classify ordinary chunk"),
            JocClassification::Unknown
        );
        assert_eq!(
            classifier.finish().expect("finish ordinary classification"),
            JocClassification::ConfirmedNonJoc
        );
        assert_eq!(classifier.staged_bytes(), 0);

        let mut decoder = FfmpegDecoder::new(OpenJocConfig::default()).expect("wrapper");
        assert_eq!(
            decoder.send_packet(packet(&ordinary, Some(0))),
            Ok(BridgeStatus::NeedMoreInput)
        );
        assert_eq!(decoder.drain(), Ok(BridgeStatus::NotJoc));
        assert_eq!(decoder.classification(), JocClassification::ConfirmedNonJoc);
        assert!(decoder.session.is_none());
        assert!(decoder.output.is_empty());
    }

    #[test]
    fn partial_eof_bad_sync_and_staging_limit_fail_without_panics() {
        let joc = synthetic_joc_frame();
        let mut partial = FfmpegDecoder::new(OpenJocConfig::default()).expect("wrapper");
        partial
            .send_packet(packet(&joc[..100], Some(0)))
            .expect("partial send");
        let error = partial.drain().expect_err("partial EOF");
        assert_eq!(error.kind, BridgeErrorKind::InvalidData);

        let mut bad = FfmpegDecoder::new(OpenJocConfig::default()).expect("wrapper");
        let error = bad
            .send_packet(packet(&[0_u8; 16], Some(0)))
            .expect_err("bad sync");
        assert_eq!(error.kind, BridgeErrorKind::InvalidData);

        let mut bounded = FfmpegDecoder::new(OpenJocConfig::default()).expect("wrapper");
        let oversized = vec![0_u8; MAX_COMPRESSED_STAGING_BYTES + 1];
        let error = bounded
            .send_packet(packet(&oversized, Some(0)))
            .expect_err("staging bound");
        assert_eq!(error.kind, BridgeErrorKind::OutputPending);
    }

    #[test]
    fn flush_discards_partial_bytes_output_timing_and_classification() {
        let frame = synthetic_joc_frame();
        let mut decoder = FfmpegDecoder::new(OpenJocConfig::default()).expect("wrapper");
        decoder
            .send_packet(packet(&frame[..100], Some(48_000)))
            .expect("partial send");
        assert_eq!(decoder.staged_bytes(), 100);
        decoder.reset();
        assert_eq!(decoder.staged_bytes(), 0);
        assert_eq!(decoder.queued_frames(), 0);
        assert_eq!(decoder.classification(), JocClassification::Unknown);
        assert!(matches!(decoder.timeline, Timeline::Unset));
    }

    #[test]
    fn grouped_packets_preserve_bounded_send_receive_backpressure() {
        let frame = synthetic_joc_frame();
        let grouped = [frame.clone(), frame.clone(), frame.clone()].concat();
        let mut decoder = FfmpegDecoder::new(OpenJocConfig::default()).expect("wrapper");
        assert!(matches!(
            decoder
                .send_packet(packet(&grouped, Some(0)))
                .expect("grouped packet"),
            BridgeStatus::NeedMoreInput | BridgeStatus::FrameAvailable
        ));
        let mut received = 0;
        loop {
            match decoder.receive_frame().expect("receive") {
                ReceiveOutcome::Frame(_) => received += 1,
                ReceiveOutcome::NeedMoreInput => break,
                other => panic!("unexpected receive state {other:?}"),
            }
        }
        let _ = decoder.drain().expect("drain");
        loop {
            match decoder.receive_frame().expect("drain receive") {
                ReceiveOutcome::Frame(_) => received += 1,
                ReceiveOutcome::EndOfStream => break,
                ReceiveOutcome::NeedMoreInput => {}
                ReceiveOutcome::NotJoc => panic!("JOC became non-JOC"),
            }
        }
        assert!(received > 0);
        assert_eq!(decoder.take_traces().len(), 3);

        let mut blocked = FfmpegDecoder::new(OpenJocConfig::default()).expect("wrapper");
        blocked.output.push_back(FfmpegFrame {
            format: "AV_SAMPLE_FMT_FLT",
            sample_rate: SAMPLE_RATE,
            nb_samples: 1,
            pts: Some(0),
            duration: 1,
            channel_layout: blocked.layout.clone(),
            interleaved_f32: vec![0.0; blocked.layout.ffmpeg_order.len()],
        });
        assert_eq!(
            blocked
                .send_packet(packet(&frame, Some(0)))
                .expect("backpressure"),
            BridgeStatus::WouldBlock
        );
        assert_eq!(blocked.staged_bytes(), 0);
    }

    #[test]
    fn timestamp_discontinuity_and_explicit_dts_fallback_are_distinct() {
        let frame = synthetic_joc_frame();
        let mut conflicting = FfmpegDecoder::new(OpenJocConfig::default()).expect("wrapper");
        conflicting
            .send_packet(packet(&frame, Some(0)))
            .expect("first packet");
        conflicting
            .send_packet(packet(&frame, Some(2000)))
            .expect("second packet");
        let error = conflicting
            .send_packet(packet(&frame, Some(4000)))
            .expect_err("conflicting timestamp");
        assert_eq!(error.kind, BridgeErrorKind::InvalidTimestamp);

        let mut fallback = FfmpegDecoder::with_timestamp_policy(
            OpenJocConfig::default(),
            TimestampPolicy::PtsThenDts,
        )
        .expect("DTS wrapper");
        for (index, dts) in [0_i64, 1536, 3072].into_iter().enumerate() {
            let status = fallback
                .send_packet(PacketRef {
                    data: &frame,
                    pts: None,
                    dts: Some(dts),
                    duration: Some(1536),
                    time_base: Rational::SAMPLE_TIME_BASE,
                    stream_index: 0,
                    discontinuity: false,
                    preroll: index == 0,
                })
                .expect("DTS packet");
            if status == BridgeStatus::FrameAvailable {
                while matches!(
                    fallback.receive_frame().expect("receive DTS"),
                    ReceiveOutcome::Frame(_)
                ) {}
            }
        }
        let _ = fallback.drain().expect("DTS drain");
        while !matches!(
            fallback.receive_frame().expect("DTS drain receive"),
            ReceiveOutcome::EndOfStream
        ) {}
        let traces = fallback.take_traces();
        assert_eq!(traces.len(), 3);
        assert!(
            traces
                .iter()
                .all(|trace| trace.timestamp_source == TimestampSource::DtsFallback)
        );
    }

    #[test]
    fn wrapper_pcm_matches_direct_session_for_binaural_and_speakers() {
        let first = synthetic_joc_frame();
        let second = synthetic_joc_frame();
        let configs = [
            OpenJocConfig {
                render_mode: RenderMode::Binaural,
                speaker_layout: "7.1.4".to_owned(),
                binaural: Some(BinauralConfig::builtin_generic("7.1.4")),
                validation_profile: ValidationProfile::EtsiStrict,
                ..OpenJocConfig::default()
            },
            OpenJocConfig {
                render_mode: RenderMode::Speaker,
                speaker_layout: "7.1.4".to_owned(),
                validation_profile: ValidationProfile::EtsiStrict,
                ..OpenJocConfig::default()
            },
            OpenJocConfig {
                render_mode: RenderMode::Speaker,
                speaker_layout: "22.2".to_owned(),
                validation_profile: ValidationProfile::EtsiStrict,
                ..OpenJocConfig::default()
            },
        ];
        for config in configs {
            let direct_fingerprint = config.effective_config_fingerprint();
            let direct = collect_direct(config.clone(), &first, &second);
            let (mut wrapper, bridged) = collect_wrapper(config, &first, &second);
            assert_eq!(wrapper.effective_config_fingerprint(), direct_fingerprint);
            assert_eq!(wrapper.classification(), JocClassification::ConfirmedJoc);
            let direct_pcm: Vec<_> = direct
                .iter()
                .map(|frame| (frame.pts_samples, frame.interleaved_f32.clone()))
                .collect();
            assert_eq!(wrapper_in_openjoc_order(&bridged), direct_pcm);
            let trace = wrapper.take_traces();
            let common =
                openjoc_api::trace_access_units(&[first.clone(), second.clone()].concat(), Some(0))
                    .expect("shared trace");
            assert_eq!(trace.len(), common.len());
            for (bridge, direct) in trace.iter().zip(common) {
                assert_eq!(bridge.sha256, direct.sha256);
                assert_eq!(bridge.pts_samples, direct.pts_samples);
                assert_eq!(bridge.byte_length, direct.byte_length);
            }
        }
    }

    #[test]
    fn independent_instances_can_decode_on_separate_threads() {
        let stream = synthetic_joc_frame();
        let tasks: Vec<_> = (0..2)
            .map(|_| {
                let first = stream.clone();
                let second = stream.clone();
                thread::spawn(move || {
                    let (_, frames) = collect_wrapper(OpenJocConfig::default(), &first, &second);
                    frames
                        .into_iter()
                        .flat_map(|frame| frame.interleaved_f32)
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        let outputs: Vec<_> = tasks
            .into_iter()
            .map(|task| task.join().expect("decode thread"))
            .collect();
        assert_eq!(outputs[0], outputs[1]);
    }

    #[cfg(feature = "ffmpeg")]
    #[test]
    fn actual_avframe_owns_packed_float_and_truthful_binaural_layout() {
        let layout = channel_layout_for_config(&OpenJocConfig {
            render_mode: RenderMode::Binaural,
            speaker_layout: "7.1.4".to_owned(),
            binaural: Some(BinauralConfig::builtin_generic("7.1.4")),
            ..OpenJocConfig::default()
        })
        .expect("layout");
        let samples = vec![0.25_f32, -0.25, 0.5, -0.5];
        let frame = FfmpegFrame {
            format: "AV_SAMPLE_FMT_FLT",
            sample_rate: SAMPLE_RATE,
            nb_samples: 2,
            pts: Some(1234),
            duration: 2,
            channel_layout: layout,
            interleaved_f32: samples.clone(),
        };
        let avframe = AvFrame::from_frame(&frame).expect("AVFrame");
        assert!(avframe.is_packed_float());
        assert_eq!(avframe.sample_rate(), 48_000);
        assert_eq!(avframe.nb_samples(), 2);
        assert_eq!(avframe.channel_count(), 2);
        assert_eq!(avframe.pts(), Some(1234));
        assert_eq!(avframe.duration(), 2);
        assert_eq!(avframe.interleaved_f32(), samples);
        assert_eq!(
            avframe.layout_description().expect("description"),
            "binaural"
        );
        assert_ne!(avframe.as_ptr(), std::ptr::null());
    }

    #[cfg(feature = "ffmpeg")]
    fn decode_demuxed(path: &str) -> (JocClassification, usize, usize) {
        let mut demuxer = Demuxer::open(path).expect("open synthetic container");
        let target = demuxer.target_stream_index();
        let mut decoder = FfmpegDecoder::new(OpenJocConfig::default()).expect("wrapper");
        let mut packets = 0;
        let mut frames = 0;
        while let Some(packet) = demuxer.read_packet().expect("read packet") {
            if packet.stream_index != target {
                continue;
            }
            packets += 1;
            let input = PacketRef {
                data: packet.data,
                pts: packet.pts,
                dts: packet.dts,
                duration: packet.duration,
                time_base: packet.time_base,
                stream_index: packet.stream_index,
                discontinuity: false,
                preroll: false,
            };
            assert_ne!(
                decoder.send_packet(input).expect("send packet"),
                BridgeStatus::NotJoc
            );
            loop {
                match decoder.receive_avframe().expect("receive AVFrame") {
                    ReceiveAvOutcome::Frame(frame) => {
                        assert!(frame.is_packed_float());
                        frames += 1;
                    }
                    ReceiveAvOutcome::NeedMoreInput => break,
                    ReceiveAvOutcome::EndOfStream => panic!("EOF before drain"),
                    ReceiveAvOutcome::NotJoc => panic!("synthetic JOC classified ordinary"),
                }
            }
        }
        let _ = decoder.drain().expect("drain");
        loop {
            match decoder.receive_avframe().expect("receive drain AVFrame") {
                ReceiveAvOutcome::Frame(frame) => {
                    assert!(frame.is_packed_float());
                    frames += 1;
                }
                ReceiveAvOutcome::EndOfStream => break,
                ReceiveAvOutcome::NeedMoreInput => {}
                ReceiveAvOutcome::NotJoc => panic!("synthetic JOC classified ordinary"),
            }
        }
        (decoder.classification(), packets, frames)
    }

    #[cfg(feature = "ffmpeg")]
    fn classify_demuxed_non_joc(path: &str) -> JocClassification {
        let mut demuxer = Demuxer::open(path).expect("open ordinary container");
        let target = demuxer.target_stream_index();
        let mut decoder = FfmpegDecoder::new(OpenJocConfig::default()).expect("wrapper");
        while let Some(packet) = demuxer.read_packet().expect("read packet") {
            if packet.stream_index != target {
                continue;
            }
            let status = decoder
                .send_packet(PacketRef {
                    data: packet.data,
                    pts: packet.pts,
                    dts: packet.dts,
                    duration: packet.duration,
                    time_base: packet.time_base,
                    stream_index: packet.stream_index,
                    discontinuity: false,
                    preroll: false,
                })
                .expect("classify ordinary packet");
            if status == BridgeStatus::NotJoc {
                return decoder.classification();
            }
            assert_eq!(
                decoder.receive_frame().expect("ordinary receive"),
                ReceiveOutcome::NeedMoreInput
            );
        }
        assert_eq!(
            decoder.drain().expect("ordinary drain"),
            BridgeStatus::NotJoc
        );
        decoder.classification()
    }

    #[cfg(feature = "ffmpeg")]
    #[test]
    fn libavformat_demuxes_raw_and_container_controls_without_libavcodec_decode() {
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "openjoc-ffmpeg-demux-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&directory).expect("create temp directory");
        let raw = directory.join("synthetic.ec3");
        let stream = [
            synthetic_joc_frame(),
            synthetic_joc_frame(),
            synthetic_joc_frame(),
        ]
        .concat();
        fs::write(&raw, stream).expect("write synthetic raw stream");
        let raw_result = decode_demuxed(raw.to_str().expect("UTF-8 temp path"));
        assert_eq!(raw_result.0, JocClassification::ConfirmedJoc);
        assert!(raw_result.1 >= 1);
        assert!(raw_result.2 >= 1);

        let ordinary = directory.join("ordinary.ec3");
        let encoded = Command::new("ffmpeg")
            .env_remove("DYLD_LIBRARY_PATH")
            .env_remove("LD_LIBRARY_PATH")
            .args([
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "anullsrc=r=48000:cl=stereo",
                "-t",
                "0.12",
                "-c:a",
                "eac3",
                "-f",
                "eac3",
            ])
            .arg(&ordinary)
            .status()
            .expect("encode ordinary E-AC-3 control");
        assert!(encoded.success());
        assert_eq!(
            classify_demuxed_non_joc(ordinary.to_str().expect("UTF-8 temp path")),
            JocClassification::ConfirmedNonJoc
        );

        for (extension, format) in [("mp4", "mp4"), ("mkv", "matroska")] {
            let output = directory.join(format!("synthetic.{extension}"));
            let status = Command::new("ffmpeg")
                .env_remove("DYLD_LIBRARY_PATH")
                .env_remove("LD_LIBRARY_PATH")
                .args(["-v", "error", "-y", "-f", "eac3", "-i"])
                .arg(&ordinary)
                .args(["-map", "0:a:0", "-c:a", "copy", "-f", format])
                .arg(&output)
                .status()
                .expect("run FFmpeg remux");
            assert!(status.success(), "FFmpeg could not create {format} fixture");
            let mut seekable = Demuxer::open(output.to_str().expect("UTF-8 temp path"))
                .expect("open seekable container");
            seekable.seek(0).expect("seek container to stream start");
            assert!(seekable.read_packet().expect("read after seek").is_some());
            assert_eq!(
                classify_demuxed_non_joc(output.to_str().expect("UTF-8 temp path")),
                JocClassification::ConfirmedNonJoc
            );
        }
        fs::remove_dir_all(&directory).expect("remove temp directory");
    }
}
