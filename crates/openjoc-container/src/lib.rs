// pattern: Imperative Shell

//! Input-media boundary for OpenJOC.
//!
//! This crate deliberately stops at container demuxing. AC-3/E-AC-3 syncframes,
//! EMDF, JOC, and OAMD are parsed by the OpenJOC codec crates after this
//! boundary. FFmpeg is used only as an external ISO BMFF demuxer with audio
//! stream copy; it is not an audio decoder or a normative reference.

use crate::cmaf::{CmafError, CmafJocTrack};
use openjoc_eac3::{
    AccessUnitIndex, Eac3Error, GENERAL_MAX_ACCESS_UNIT_BYTES, GENERAL_MAX_DEPENDENT_SUBSTREAMS,
    StreamType, SyncframeHeader, SyncframeIndexEntry, group_access_units, index_syncframes,
    parse_syncframe_header,
};
use std::{
    fmt,
    fs::{self, File},
    io::{self, BufRead, BufReader, Read},
    path::Path,
    process::{Child, ChildStderr, ChildStdout, Command, Stdio},
    thread,
};

pub mod cmaf;

/// Maximum elementary-stream size accepted by the demux boundary.
pub const DEFAULT_MAX_EAC3_BYTES: usize = 512 * 1024 * 1024;

/// File-signature classification performed before any E-AC-3 parsing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputMediaKind {
    RawEac3,
    IsoBmff,
    Unknown,
}

/// Bounded E-AC-3 bytes ready for independent OpenJOC parsing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Eac3Input {
    pub kind: InputMediaKind,
    pub bytes: Vec<u8>,
}

/// Deterministic high-watermark counters for incremental raw E-AC-3 framing.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RawEac3ReaderStats {
    pub frames_emitted: usize,
    pub max_input_carry_bytes: usize,
    pub max_frame_bytes: usize,
}

/// One complete, locally indexed raw E-AC-3 access unit.
///
/// The byte and frame vectors are intentionally bounded to one access unit;
/// callers can hand them to the codec decoder and release them before reading
/// the next unit.  Offsets in `frames` are relative to `bytes`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RawEac3AccessUnit {
    pub bytes: Vec<u8>,
    pub frames: Vec<SyncframeIndexEntry>,
    pub unit: AccessUnitIndex,
}

/// Deterministic high-watermarks for an incremental raw E-AC-3 AU consumer.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RawEac3AccessUnitReaderStats {
    pub frames_emitted: usize,
    pub access_units_emitted: usize,
    pub max_input_carry_bytes: usize,
    pub max_frame_bytes: usize,
    pub max_complete_frames_retained: usize,
    pub max_au_bytes: usize,
    pub max_au_frames: usize,
    pub max_lookahead_frames: usize,
}

/// Reader-based raw E-AC-3 syncframe framer.
///
/// The reader requests no more than the bytes needed for the current header or
/// declared frame.  It therefore never retains a complete programme or a
/// second frame merely because the underlying `Read` implementation returned a
/// large chunk.  AU grouping remains a separate bounded consumer concern.
pub struct RawEac3FrameReader<R> {
    reader: R,
    carry: Vec<u8>,
    offset: usize,
    eof: bool,
    max_frame_bytes: usize,
    stats: RawEac3ReaderStats,
}

impl<R: Read> RawEac3FrameReader<R> {
    /// Creates a framer with an explicit maximum declared syncframe size.
    #[must_use]
    pub fn new(reader: R, max_frame_bytes: usize) -> Self {
        Self {
            reader,
            carry: Vec::new(),
            offset: 0,
            eof: false,
            max_frame_bytes,
            stats: RawEac3ReaderStats::default(),
        }
    }

    /// Reads the next complete syncframe, or `None` at an exact frame-boundary EOF.
    ///
    /// A framing error is terminal for this reader: callers must discard the
    /// reader rather than treating a later call as a recovery/reset operation.
    pub fn next_frame(&mut self) -> Result<Option<Vec<u8>>, InputMediaError> {
        const HEADER_PROBE_BYTES: usize = 8;
        while self.carry.len() < HEADER_PROBE_BYTES {
            if !self.read_more(HEADER_PROBE_BYTES - self.carry.len())? {
                if self.carry.is_empty() {
                    return Ok(None);
                }
                return Err(InputMediaError::TruncatedRawEac3 {
                    offset: self.offset,
                    available: self.carry.len(),
                });
            }
        }
        let header =
            parse_syncframe_header(&self.carry).map_err(InputMediaError::InvalidDemuxedEac3)?;
        if header.frame_size > self.max_frame_bytes {
            return Err(InputMediaError::DemuxOutputTooLarge {
                limit: self.max_frame_bytes,
            });
        }
        while self.carry.len() < header.frame_size {
            if !self.read_more(header.frame_size - self.carry.len())? {
                return Err(InputMediaError::InvalidDemuxedEac3(
                    Eac3Error::TruncatedFrame {
                        offset: self.offset,
                        declared: header.frame_size,
                        available: self.carry.len(),
                    },
                ));
            }
        }
        let frame = self.carry.drain(..header.frame_size).collect::<Vec<_>>();
        self.offset = self.offset.checked_add(header.frame_size).ok_or(
            InputMediaError::DemuxOutputTooLarge {
                limit: self.max_frame_bytes,
            },
        )?;
        self.stats.frames_emitted = self.stats.frames_emitted.saturating_add(1);
        self.stats.max_frame_bytes = self.stats.max_frame_bytes.max(frame.len());
        Ok(Some(frame))
    }

    /// Returns deterministic retained-input high-watermarks.
    #[must_use]
    pub const fn stats(&self) -> RawEac3ReaderStats {
        self.stats
    }

    fn read_more(&mut self, requested: usize) -> Result<bool, InputMediaError> {
        if self.eof || requested == 0 {
            return Ok(false);
        }
        let start = self.carry.len();
        let end = start
            .checked_add(requested)
            .ok_or(InputMediaError::DemuxOutputTooLarge {
                limit: self.max_frame_bytes,
            })?;
        self.carry.resize(end, 0);
        let mut read = 0;
        while read < requested {
            let count = self
                .reader
                .read(&mut self.carry[start + read..start + requested])
                .map_err(|source| InputMediaError::Io {
                    operation: "read raw E-AC-3 frame",
                    source,
                })?;
            if count == 0 {
                self.eof = true;
                self.carry.truncate(start + read);
                break;
            }
            read += count;
        }
        self.stats.max_input_carry_bytes = self.stats.max_input_carry_bytes.max(self.carry.len());
        Ok(read != 0)
    }
}

impl<R> RawEac3FrameReader<R> {
    /// Returns the underlying reader after framing has stopped.
    #[must_use]
    pub fn into_inner(self) -> R {
        self.reader
    }
}

/// Sequential six-block access-unit consumer built on [`RawEac3FrameReader`].
///
/// A single frame of lookahead is retained to detect the next complete-unit
/// boundary. Short syncframes are accumulated by programme-set timing; no
/// programme-wide frame or AU index is built.
pub struct RawEac3AccessUnitReader<R> {
    frame_reader: RawEac3FrameReader<R>,
    lookahead: Option<Vec<u8>>,
    stats: RawEac3AccessUnitReaderStats,
}

impl<R: Read> RawEac3AccessUnitReader<R> {
    /// Creates a sequential AU reader with an explicit syncframe size bound.
    #[must_use]
    pub fn new(reader: R, max_frame_bytes: usize) -> Self {
        Self {
            frame_reader: RawEac3FrameReader::new(reader, max_frame_bytes),
            lookahead: None,
            stats: RawEac3AccessUnitReaderStats::default(),
        }
    }

    /// Reads one complete AU, or `None` at an exact frame-boundary EOF.
    pub fn next_access_unit(&mut self) -> Result<Option<RawEac3AccessUnit>, InputMediaError> {
        let Some(first) = self.take_frame()? else {
            return Ok(None);
        };
        let first_header = raw_frame_header(&first)?;
        if !matches!(
            first_header.stream_type,
            StreamType::LegacyIndependent | StreamType::Independent
        ) || first_header.substream_id != 0
        {
            return Err(InputMediaError::InvalidDemuxedEac3(
                Eac3Error::MissingIndependentSubstreamZero { frame: 0 },
            ));
        }

        let mut bytes = first;
        let mut frames = vec![SyncframeIndexEntry {
            offset: 0,
            header: first_header,
        }];
        if first_header.audio_blocks < 6 {
            if first_header.stream_type != StreamType::Independent {
                return Err(InputMediaError::InvalidDemuxedEac3(
                    Eac3Error::UnsupportedJocAudioBlockCount {
                        actual: first_header.audio_blocks,
                    },
                ));
            }
            self.read_short_access_unit(&mut bytes, &mut frames, first_header)?;
        } else {
            while let Some(next) = self.take_frame()? {
                let header = raw_frame_header(&next)?;
                if header.stream_type != StreamType::Dependent && header.substream_id == 0 {
                    self.lookahead = Some(next);
                    self.stats.max_lookahead_frames = self.stats.max_lookahead_frames.max(1);
                    break;
                }
                append_raw_frame(&mut bytes, &mut frames, &next, header)?;
            }
        }

        let units = group_access_units(&frames).map_err(InputMediaError::InvalidDemuxedEac3)?;
        let unit = units
            .into_iter()
            .next()
            .ok_or(InputMediaError::InvalidDemuxedEac3(
                Eac3Error::InvalidAccessUnitRange,
            ))?;
        if unit.frame_count != frames.len() {
            return Err(InputMediaError::InvalidDemuxedEac3(
                Eac3Error::InvalidAccessUnitRange,
            ));
        }

        self.stats.access_units_emitted = self.stats.access_units_emitted.saturating_add(1);
        self.stats.max_au_bytes = self.stats.max_au_bytes.max(bytes.len());
        self.stats.max_au_frames = self.stats.max_au_frames.max(frames.len());
        self.stats.max_complete_frames_retained = self
            .stats
            .max_complete_frames_retained
            .max(frames.len() + usize::from(self.lookahead.is_some()));
        Ok(Some(RawEac3AccessUnit {
            bytes,
            frames,
            unit,
        }))
    }

    /// Returns deterministic sequential-reader high-watermarks.
    #[must_use]
    pub const fn stats(&self) -> RawEac3AccessUnitReaderStats {
        self.stats
    }

    fn read_short_access_unit(
        &mut self,
        bytes: &mut Vec<u8>,
        frames: &mut Vec<SyncframeIndexEntry>,
        first: SyncframeHeader,
    ) -> Result<(), InputMediaError> {
        let mut total_blocks = first.audio_blocks;
        let mut current_independent = first;
        let mut expected_dependent = 0_u8;
        let mut expected_dependent_count = None;
        loop {
            let Some(next) = self.take_frame()? else {
                if total_blocks == 6
                    && (expected_dependent_count.is_none()
                        || expected_dependent_count == Some(usize::from(expected_dependent)))
                {
                    return Ok(());
                }
                return Err(InputMediaError::InvalidDemuxedEac3(
                    Eac3Error::InvalidAccessUnitRange,
                ));
            };
            let header = raw_frame_header(&next)?;
            if header.stream_type == StreamType::Dependent {
                if usize::from(expected_dependent) >= GENERAL_MAX_DEPENDENT_SUBSTREAMS
                    || header.substream_id != expected_dependent
                {
                    return Err(InputMediaError::InvalidDemuxedEac3(
                        Eac3Error::NonsequentialDependentSubstream {
                            expected: expected_dependent,
                            actual: header.substream_id,
                        },
                    ));
                }
                if header.sample_rate != current_independent.sample_rate
                    || header.audio_blocks != current_independent.audio_blocks
                {
                    return Err(InputMediaError::InvalidDemuxedEac3(
                        Eac3Error::SubstreamTimingMismatch {
                            frame: frames.len(),
                        },
                    ));
                }
                expected_dependent = expected_dependent.saturating_add(1);
                append_raw_frame(bytes, frames, &next, header)?;
                continue;
            }
            if header.stream_type != StreamType::Independent || header.substream_id != 0 {
                return Err(InputMediaError::InvalidDemuxedEac3(
                    Eac3Error::MissingIndependentSubstreamZero {
                        frame: frames.len(),
                    },
                ));
            }
            if let Some(expected) = expected_dependent_count {
                if expected != usize::from(expected_dependent) {
                    return Err(InputMediaError::InvalidDemuxedEac3(
                        Eac3Error::SubstreamTimingMismatch {
                            frame: frames.len(),
                        },
                    ));
                }
            } else {
                expected_dependent_count = Some(usize::from(expected_dependent));
            }
            if total_blocks == 6 {
                self.lookahead = Some(next);
                self.stats.max_lookahead_frames = self.stats.max_lookahead_frames.max(1);
                return Ok(());
            }
            let next_total = total_blocks.checked_add(header.audio_blocks).ok_or(
                InputMediaError::InvalidDemuxedEac3(Eac3Error::FrameSizeOverflow),
            )?;
            if next_total > 6 {
                return Err(InputMediaError::InvalidDemuxedEac3(
                    Eac3Error::SubstreamTimingMismatch {
                        frame: frames.len(),
                    },
                ));
            }
            total_blocks = next_total;
            current_independent = header;
            expected_dependent = 0;
            append_raw_frame(bytes, frames, &next, header)?;
        }
    }

    fn take_frame(&mut self) -> Result<Option<Vec<u8>>, InputMediaError> {
        let frame = match self.lookahead.take() {
            Some(frame) => Some(frame),
            None => self.frame_reader.next_frame()?,
        };
        let frame_reader_stats = self.frame_reader.stats();
        self.stats.frames_emitted = frame_reader_stats.frames_emitted;
        self.stats.max_input_carry_bytes = frame_reader_stats.max_input_carry_bytes;
        self.stats.max_frame_bytes = frame_reader_stats.max_frame_bytes;
        Ok(frame)
    }
}

fn append_raw_frame(
    bytes: &mut Vec<u8>,
    frames: &mut Vec<SyncframeIndexEntry>,
    frame: &[u8],
    header: SyncframeHeader,
) -> Result<(), InputMediaError> {
    let new_len =
        bytes
            .len()
            .checked_add(frame.len())
            .ok_or(InputMediaError::DemuxOutputTooLarge {
                limit: GENERAL_MAX_ACCESS_UNIT_BYTES,
            })?;
    if new_len > GENERAL_MAX_ACCESS_UNIT_BYTES {
        return Err(InputMediaError::DemuxOutputTooLarge {
            limit: GENERAL_MAX_ACCESS_UNIT_BYTES,
        });
    }
    let offset = bytes.len();
    bytes.extend_from_slice(frame);
    frames.push(SyncframeIndexEntry { offset, header });
    Ok(())
}

/// Lightweight header information useful to a bounded AU consumer.
pub fn raw_frame_header(frame: &[u8]) -> Result<SyncframeHeader, InputMediaError> {
    parse_syncframe_header(frame).map_err(InputMediaError::InvalidDemuxedEac3)
}

/// Container/input boundary failure.
#[derive(Debug)]
pub enum InputMediaError {
    Io {
        operation: &'static str,
        source: io::Error,
    },
    EmptyInput,
    UnsupportedSignature,
    MissingAudioTrack,
    MultipleAudioTracks {
        count: usize,
    },
    NoMatchingAudioTrack {
        codec: String,
    },
    ProbeFailed {
        detail: String,
    },
    MalformedProbeRow {
        row: String,
    },
    DemuxFailed {
        detail: String,
    },
    DemuxOutputTooLarge {
        limit: usize,
    },
    MalformedPacketProbeRow {
        row: String,
    },
    EmptyDemuxOutput,
    TruncatedRawEac3 {
        offset: usize,
        available: usize,
    },
    InvalidDemuxedEac3(Eac3Error),
    InvalidCmaf(CmafError),
}

impl fmt::Display for InputMediaError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { operation, source } => write!(formatter, "failed to {operation}: {source}"),
            Self::EmptyInput => formatter.write_str("input file is empty"),
            Self::UnsupportedSignature => formatter
                .write_str("unsupported input media signature; expected raw E-AC-3 or ISO BMFF"),
            Self::MissingAudioTrack => formatter.write_str("ISO BMFF container has no audio track"),
            Self::MultipleAudioTracks { count } => write!(
                formatter,
                "ISO BMFF container has {count} audio tracks; exactly one E-AC-3 track is required"
            ),
            Self::NoMatchingAudioTrack { codec } => write!(
                formatter,
                "ISO BMFF container has no matching E-AC-3 audio track (found codec {codec})"
            ),
            Self::ProbeFailed { detail } => {
                write!(formatter, "failed to inspect ISO BMFF tracks: {detail}")
            }
            Self::MalformedProbeRow { row } => {
                write!(
                    formatter,
                    "FFprobe returned malformed audio-track row: {row}"
                )
            }
            Self::DemuxFailed { detail } => {
                write!(
                    formatter,
                    "failed to stream-copy E-AC-3 from ISO BMFF: {detail}"
                )
            }
            Self::DemuxOutputTooLarge { limit } => write!(
                formatter,
                "demuxed E-AC-3 stream exceeds the configured {limit}-byte bound"
            ),
            Self::MalformedPacketProbeRow { row } => {
                write!(
                    formatter,
                    "FFprobe returned malformed ISO BMFF packet row: {row}"
                )
            }
            Self::EmptyDemuxOutput => {
                formatter.write_str("ISO BMFF E-AC-3 demux produced an empty elementary stream")
            }
            Self::TruncatedRawEac3 { offset, available } => write!(
                formatter,
                "truncated raw E-AC-3 input at byte {offset}: only {available} header bytes available"
            ),
            Self::InvalidDemuxedEac3(error) => {
                write!(formatter, "demuxed E-AC-3 stream is invalid: {error}")
            }
            Self::InvalidCmaf(error) => write!(formatter, "CMAF sample is invalid: {error}"),
        }
    }
}

impl std::error::Error for InputMediaError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::InvalidDemuxedEac3(error) => Some(error),
            Self::InvalidCmaf(error) => Some(error),
            _ => None,
        }
    }
}

impl From<Eac3Error> for InputMediaError {
    fn from(error: Eac3Error) -> Self {
        Self::InvalidDemuxedEac3(error)
    }
}

/// Classifies a byte prefix without interpreting codec fields.
pub fn detect_media(bytes: &[u8]) -> InputMediaKind {
    if bytes.starts_with(&[0x0b, 0x77]) {
        return InputMediaKind::RawEac3;
    }
    if bytes.get(4..8) == Some(b"ftyp") {
        return InputMediaKind::IsoBmff;
    }
    if bytes.len() >= 8 && is_iso_bmff_box_type(&bytes[4..8]) {
        return InputMediaKind::IsoBmff;
    }
    InputMediaKind::Unknown
}

fn is_iso_bmff_box_type(bytes: &[u8]) -> bool {
    matches!(
        bytes,
        b"moov" | b"mdat" | b"free" | b"skip" | b"wide" | b"uuid" | b"sidx" | b"styp"
    )
}

/// Parses FFprobe's `index,codec_name` CSV output for audio streams.
pub fn parse_audio_probe_output(output: &str) -> Result<Vec<(usize, String)>, InputMediaError> {
    let mut tracks = Vec::new();
    for row in output.lines().map(str::trim).filter(|row| !row.is_empty()) {
        let fields = row.split(',').map(str::trim).collect::<Vec<_>>();
        let index = fields.first().and_then(|field| field.parse::<usize>().ok());
        let codec = fields.get(1).copied().filter(|field| !field.is_empty());
        if fields.len() < 2
            || fields.iter().skip(2).any(|field| !field.is_empty())
            || index.is_none()
            || codec.is_none()
        {
            return Err(InputMediaError::MalformedProbeRow {
                row: row.to_owned(),
            });
        }
        let (Some(index), Some(codec)) = (index, codec) else {
            return Err(InputMediaError::MalformedProbeRow {
                row: row.to_owned(),
            });
        };
        tracks.push((index, codec.to_owned()));
    }
    Ok(tracks)
}

/// One compressed ISO BMFF packet location for the selected E-AC-3 track.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IsoBmffSample {
    pub offset: u64,
    pub size: usize,
}

/// Deterministic ownership counters for a seekable ISO BMFF reader.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SeekableIsoBmffReaderStats {
    pub samples_delivered: usize,
    pub sample_count: usize,
    pub max_current_sample_bytes: usize,
    pub max_samples_simultaneously_retained: usize,
    pub derived_sample_index_entries: usize,
    pub cursor_state_entries: usize,
}

/// A bounded cursor over FFprobe's packet rows.
///
/// The packet rows are consumed one line at a time. OpenJOC does not retain a
/// second per-sample descriptor vector; any native ISO BMFF table memory held
/// inside FFprobe remains external and is reported separately.
pub struct IsoBmffSampleCursor {
    child: Child,
    rows: BufReader<ChildStdout>,
    stderr: Option<ChildStderr>,
    selected_stream: usize,
    line: String,
    emitted: usize,
    finished: bool,
}

impl IsoBmffSampleCursor {
    fn new(
        child: Child,
        stdout: ChildStdout,
        stderr: Option<ChildStderr>,
        selected_stream: usize,
    ) -> Self {
        Self {
            child,
            rows: BufReader::new(stdout),
            stderr,
            selected_stream,
            line: String::new(),
            emitted: 0,
            finished: false,
        }
    }

    fn next_sample(&mut self) -> Result<Option<IsoBmffSample>, InputMediaError> {
        if self.finished {
            return Ok(None);
        }
        loop {
            self.line.clear();
            let read =
                self.rows
                    .read_line(&mut self.line)
                    .map_err(|source| InputMediaError::Io {
                        operation: "read FFprobe packet row",
                        source,
                    })?;
            if read == 0 {
                self.finished = true;
                let mut stderr = Vec::new();
                if let Some(mut stream) = self.stderr.take() {
                    stream
                        .read_to_end(&mut stderr)
                        .map_err(|source| InputMediaError::Io {
                            operation: "read FFprobe packet diagnostics",
                            source,
                        })?;
                }
                let status = self.child.wait().map_err(|source| InputMediaError::Io {
                    operation: "wait for FFprobe packet probe",
                    source,
                })?;
                if !status.success() {
                    return Err(InputMediaError::ProbeFailed {
                        detail: command_detail(&stderr),
                    });
                }
                if self.emitted == 0 {
                    return Err(InputMediaError::EmptyDemuxOutput);
                }
                return Ok(None);
            }
            if self.line.trim().is_empty() {
                continue;
            }
            let sample = parse_packet_probe_row(&self.line, self.selected_stream)?;
            self.emitted = self.emitted.saturating_add(1);
            return Ok(Some(sample));
        }
    }
}

impl Drop for IsoBmffSampleCursor {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

enum SampleDelivery {
    Indexed {
        samples: Vec<IsoBmffSample>,
        next: usize,
    },
    Cursor(IsoBmffSampleCursor),
}

/// Seekable ISO BMFF E-AC-3 sample reader.
///
/// [`Self::from_cursor`] is the ordinary sequential path: it retains only the
/// bounded cursor and one current sample. [`Self::new`] remains an explicit
/// indexed/capture adapter for callers that already own sample descriptors.
pub struct SeekableIsoBmffEc3Reader<R> {
    source: R,
    delivery: SampleDelivery,
    file_len: u64,
    max_sample_bytes: usize,
    current: Vec<u8>,
    current_offset: usize,
    stats: SeekableIsoBmffReaderStats,
    cursor_delivery: bool,
}

impl<R: Read + io::Seek> SeekableIsoBmffEc3Reader<R> {
    /// Creates an explicit indexed/capture reader from sample descriptors.
    pub fn new(
        mut source: R,
        samples: Vec<IsoBmffSample>,
        max_sample_bytes: usize,
    ) -> Result<Self, InputMediaError> {
        let file_len = source
            .seek(io::SeekFrom::End(0))
            .map_err(|source| InputMediaError::Io {
                operation: "seek ISO BMFF input",
                source,
            })?;
        source
            .seek(io::SeekFrom::Start(0))
            .map_err(|source| InputMediaError::Io {
                operation: "rewind ISO BMFF input",
                source,
            })?;
        let stats = SeekableIsoBmffReaderStats {
            sample_count: samples.len(),
            derived_sample_index_entries: samples.len(),
            ..SeekableIsoBmffReaderStats::default()
        };
        Ok(Self {
            source,
            delivery: SampleDelivery::Indexed { samples, next: 0 },
            file_len,
            max_sample_bytes,
            current: Vec::new(),
            current_offset: 0,
            stats,
            cursor_delivery: false,
        })
    }

    /// Creates the ordinary sequential reader with bounded cursor state.
    pub fn from_cursor(
        mut source: R,
        cursor: IsoBmffSampleCursor,
        max_sample_bytes: usize,
    ) -> Result<Self, InputMediaError> {
        let file_len = source
            .seek(io::SeekFrom::End(0))
            .map_err(|source| InputMediaError::Io {
                operation: "seek ISO BMFF input",
                source,
            })?;
        source
            .seek(io::SeekFrom::Start(0))
            .map_err(|source| InputMediaError::Io {
                operation: "rewind ISO BMFF input",
                source,
            })?;
        Ok(Self {
            source,
            delivery: SampleDelivery::Cursor(cursor),
            file_len,
            max_sample_bytes,
            current: Vec::new(),
            current_offset: 0,
            stats: SeekableIsoBmffReaderStats {
                cursor_state_entries: 1,
                ..SeekableIsoBmffReaderStats::default()
            },
            cursor_delivery: true,
        })
    }

    /// Reads the next compressed sample and releases the previous sample.
    pub fn next_sample(&mut self) -> Result<Option<Vec<u8>>, InputMediaError> {
        self.current.clear();
        self.current_offset = 0;
        let sample = match &mut self.delivery {
            SampleDelivery::Indexed { samples, next } => {
                let sample = samples.get(*next).copied();
                *next = next.saturating_add(1);
                sample
            }
            SampleDelivery::Cursor(cursor) => cursor.next_sample()?,
        };
        let Some(sample) = sample else {
            return Ok(None);
        };
        if sample.size > self.max_sample_bytes {
            return Err(InputMediaError::DemuxOutputTooLarge {
                limit: self.max_sample_bytes,
            });
        }
        let end = sample
            .offset
            .checked_add(u64::try_from(sample.size).unwrap_or(u64::MAX))
            .ok_or(InputMediaError::MalformedPacketProbeRow {
                row: format!("offset={} size={}", sample.offset, sample.size),
            })?;
        if end > self.file_len {
            return Err(InputMediaError::MalformedPacketProbeRow {
                row: format!(
                    "sample exceeds file bounds: offset={} size={}",
                    sample.offset, sample.size
                ),
            });
        }
        self.source
            .seek(io::SeekFrom::Start(sample.offset))
            .map_err(|source| InputMediaError::Io {
                operation: "seek ISO BMFF sample",
                source,
            })?;
        let mut bytes = vec![0_u8; sample.size];
        self.source
            .read_exact(&mut bytes)
            .map_err(|source| InputMediaError::Io {
                operation: "read ISO BMFF sample",
                source,
            })?;
        self.stats.samples_delivered = self.stats.samples_delivered.saturating_add(1);
        if self.cursor_delivery {
            self.stats.sample_count = self.stats.samples_delivered;
        }
        self.stats.max_current_sample_bytes = self.stats.max_current_sample_bytes.max(bytes.len());
        self.stats.max_samples_simultaneously_retained = 1;
        Ok(Some(bytes))
    }

    /// Reads and validates the next CMAF JOC sample without modifying its
    /// compressed bytes.
    pub fn next_cmaf_sample(
        &mut self,
        track: &CmafJocTrack,
    ) -> Result<Option<Vec<u8>>, InputMediaError> {
        let Some(bytes) = self.next_sample()? else {
            return Ok(None);
        };
        track
            .validate_sample(&bytes)
            .map_err(InputMediaError::InvalidCmaf)?;
        Ok(Some(bytes))
    }

    /// Returns deterministic ownership counters.
    #[must_use]
    pub const fn stats(&self) -> SeekableIsoBmffReaderStats {
        self.stats
    }
}

impl<R: Read + io::Seek> Read for SeekableIsoBmffEc3Reader<R> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        loop {
            if self.current_offset < self.current.len() {
                let count = output
                    .len()
                    .min(self.current.len().saturating_sub(self.current_offset));
                output[..count].copy_from_slice(
                    &self.current[self.current_offset..self.current_offset + count],
                );
                self.current_offset += count;
                return Ok(count);
            }
            match self.next_sample() {
                Ok(Some(sample)) => {
                    self.current = sample;
                    self.current_offset = 0;
                }
                Ok(None) => return Ok(0),
                Err(error) => return Err(io::Error::other(error.to_string())),
            }
        }
    }
}

/// Opens a seekable ISO BMFF E-AC-3 reader using a lazy FFprobe packet cursor.
pub fn open_seekable_iso_bmff(
    path: &Path,
    ffprobe: &Path,
    max_sample_bytes: usize,
) -> Result<SeekableIsoBmffEc3Reader<File>, InputMediaError> {
    let cursor = probe_iso_bmff_sample_cursor(path, ffprobe)?;
    let source = File::open(path).map_err(|source| InputMediaError::Io {
        operation: "open ISO BMFF input",
        source,
    })?;
    SeekableIsoBmffEc3Reader::from_cursor(source, cursor, max_sample_bytes)
}

fn parse_packet_probe_row(
    row: &str,
    selected_stream: usize,
) -> Result<IsoBmffSample, InputMediaError> {
    let fields = row.trim().split(',').map(str::trim).collect::<Vec<_>>();
    let stream = fields.first().and_then(|field| field.parse::<usize>().ok());
    let size = fields.get(1).and_then(|field| field.parse::<usize>().ok());
    let offset = fields.get(2).and_then(|field| field.parse::<u64>().ok());
    if fields.len() < 3 || stream != Some(selected_stream) || size.is_none() || offset.is_none() {
        return Err(InputMediaError::MalformedPacketProbeRow {
            row: row.trim().to_owned(),
        });
    }
    let (Some(offset), Some(size)) = (offset, size) else {
        return Err(InputMediaError::MalformedPacketProbeRow {
            row: row.trim().to_owned(),
        });
    };
    Ok(IsoBmffSample { offset, size })
}

/// Parses FFprobe packet rows in its stable `stream_index,size,pos` order.
pub fn parse_packet_probe_output(
    output: &str,
    selected_stream: usize,
) -> Result<Vec<IsoBmffSample>, InputMediaError> {
    let mut samples = Vec::new();
    for row in output.lines().map(str::trim).filter(|row| !row.is_empty()) {
        samples.push(parse_packet_probe_row(row, selected_stream)?);
    }
    if samples.is_empty() {
        return Err(InputMediaError::EmptyDemuxOutput);
    }
    Ok(samples)
}

fn probe_iso_bmff_sample_cursor(
    path: &Path,
    ffprobe: &Path,
) -> Result<IsoBmffSampleCursor, InputMediaError> {
    let probe = Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-select_streams",
            "a",
            "-show_entries",
            "stream=index,codec_name",
            "-of",
            "csv=p=0:s=,",
        ])
        .arg(path)
        .output()
        .map_err(|source| InputMediaError::Io {
            operation: "run FFprobe",
            source,
        })?;
    if !probe.status.success() {
        return Err(InputMediaError::ProbeFailed {
            detail: command_detail(&probe.stderr),
        });
    }
    let tracks = parse_audio_probe_output(&String::from_utf8_lossy(&probe.stdout))?;
    let (stream_index, codec) = match tracks.as_slice() {
        [] => return Err(InputMediaError::MissingAudioTrack),
        [track] => track,
        tracks => {
            return Err(InputMediaError::MultipleAudioTracks {
                count: tracks.len(),
            });
        }
    };
    if codec != "eac3" {
        return Err(InputMediaError::NoMatchingAudioTrack {
            codec: codec.clone(),
        });
    }
    let stream_selector = stream_index.to_string();
    let mut packet_probe = Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-select_streams",
            stream_selector.as_str(),
            "-show_packets",
            "-show_entries",
            "packet=stream_index,size,pos",
            "-of",
            "csv=p=0:s=,",
        ])
        .arg(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| InputMediaError::Io {
            operation: "run FFprobe packet probe",
            source,
        })?;
    let stdout = packet_probe
        .stdout
        .take()
        .ok_or_else(|| InputMediaError::ProbeFailed {
            detail: "FFprobe packet stdout was not piped".to_owned(),
        })?;
    let stderr = packet_probe.stderr.take();
    Ok(IsoBmffSampleCursor::new(
        packet_probe,
        stdout,
        stderr,
        *stream_index,
    ))
}

/// Reads raw E-AC-3 or demuxes one E-AC-3 track from ISO BMFF.
pub fn load_eac3(path: &Path) -> Result<Eac3Input, InputMediaError> {
    load_eac3_with_tools(
        path,
        Path::new("ffprobe"),
        Path::new("ffmpeg"),
        DEFAULT_MAX_EAC3_BYTES,
    )
}

/// Testable/configurable form of [`load_eac3`].
pub fn load_eac3_with_tools(
    path: &Path,
    ffprobe: &Path,
    ffmpeg: &Path,
    max_bytes: usize,
) -> Result<Eac3Input, InputMediaError> {
    let prefix = read_prefix(path)?;
    let kind = detect_media(&prefix);
    match kind {
        InputMediaKind::RawEac3 => {
            let bytes = read_bounded_file(path, max_bytes)?;
            Ok(Eac3Input { kind, bytes })
        }
        InputMediaKind::IsoBmff => {
            let bytes = demux_iso_bmff(path, ffprobe, ffmpeg, max_bytes)?;
            Ok(Eac3Input { kind, bytes })
        }
        InputMediaKind::Unknown => Err(InputMediaError::UnsupportedSignature),
    }
}

fn read_prefix(path: &Path) -> Result<[u8; 12], InputMediaError> {
    let mut file = File::open(path).map_err(|source| InputMediaError::Io {
        operation: "open input file",
        source,
    })?;
    let mut prefix = [0_u8; 12];
    let mut read = 0;
    while read < prefix.len() {
        let count = file
            .read(&mut prefix[read..])
            .map_err(|source| InputMediaError::Io {
                operation: "read input signature",
                source,
            })?;
        if count == 0 {
            break;
        }
        read += count;
    }
    if read == 0 {
        return Err(InputMediaError::EmptyInput);
    }
    Ok(prefix)
}

fn read_bounded_file(path: &Path, max_bytes: usize) -> Result<Vec<u8>, InputMediaError> {
    let metadata = fs::metadata(path).map_err(|source| InputMediaError::Io {
        operation: "stat input file",
        source,
    })?;
    let length = usize::try_from(metadata.len()).unwrap_or(usize::MAX);
    if length > max_bytes {
        return Err(InputMediaError::DemuxOutputTooLarge { limit: max_bytes });
    }
    let bytes = fs::read(path).map_err(|source| InputMediaError::Io {
        operation: "read input file",
        source,
    })?;
    if bytes.len() > max_bytes {
        return Err(InputMediaError::DemuxOutputTooLarge { limit: max_bytes });
    }
    Ok(bytes)
}

fn validate_eac3(bytes: &[u8]) -> Result<(), InputMediaError> {
    if bytes.is_empty() {
        return Err(InputMediaError::EmptyDemuxOutput);
    }
    let frames = index_syncframes(bytes).map_err(InputMediaError::InvalidDemuxedEac3)?;
    if frames.is_empty() {
        return Err(InputMediaError::EmptyDemuxOutput);
    }
    group_access_units(&frames).map_err(InputMediaError::InvalidDemuxedEac3)?;
    Ok(())
}

fn demux_iso_bmff(
    path: &Path,
    ffprobe: &Path,
    ffmpeg: &Path,
    max_bytes: usize,
) -> Result<Vec<u8>, InputMediaError> {
    let probe = Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-select_streams",
            "a",
            "-show_entries",
            "stream=index,codec_name",
            "-of",
            "csv=p=0:s=,",
        ])
        .arg(path)
        .output()
        .map_err(|source| InputMediaError::Io {
            operation: "run FFprobe",
            source,
        })?;
    if !probe.status.success() {
        return Err(InputMediaError::ProbeFailed {
            detail: command_detail(&probe.stderr),
        });
    }
    let tracks = parse_audio_probe_output(&String::from_utf8_lossy(&probe.stdout))?;
    let (stream_index, codec) = match tracks.as_slice() {
        [] => return Err(InputMediaError::MissingAudioTrack),
        [track] => track,
        tracks => {
            return Err(InputMediaError::MultipleAudioTracks {
                count: tracks.len(),
            });
        }
    };
    if codec != "eac3" {
        return Err(InputMediaError::NoMatchingAudioTrack {
            codec: codec.clone(),
        });
    }
    let mut command = Command::new(ffmpeg);
    command
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(path)
        .args(["-map"])
        .arg(format!("0:{stream_index}"))
        .args(["-c:a", "copy", "-f", "eac3", "pipe:1"]);
    let output = run_bounded(command, max_bytes)?;
    if !output.status.success() {
        return Err(InputMediaError::DemuxFailed {
            detail: command_detail(&output.stderr),
        });
    }
    if output.stdout.is_empty() {
        return Err(InputMediaError::EmptyDemuxOutput);
    }
    if output.stdout.len() > max_bytes {
        return Err(InputMediaError::DemuxOutputTooLarge { limit: max_bytes });
    }
    validate_eac3(&output.stdout)?;
    Ok(output.stdout)
}

struct CommandOutput {
    status: std::process::ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

fn run_bounded(mut command: Command, max_bytes: usize) -> Result<CommandOutput, InputMediaError> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| InputMediaError::Io {
            operation: "run FFmpeg",
            source,
        })?;
    let stdout = child.stdout.take().ok_or_else(|| InputMediaError::Io {
        operation: "capture FFmpeg output",
        source: io::Error::other("FFmpeg stdout was not piped"),
    })?;
    let stderr = child.stderr.take().ok_or_else(|| InputMediaError::Io {
        operation: "capture FFmpeg diagnostics",
        source: io::Error::other("FFmpeg stderr was not piped"),
    })?;
    let stderr_thread = thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stderr.take(1024 * 1024).read_to_end(&mut bytes);
        bytes
    });
    let read_limit = max_bytes
        .checked_add(1)
        .ok_or(InputMediaError::DemuxOutputTooLarge { limit: max_bytes })?;
    let mut bytes = Vec::new();
    stdout
        .take(u64::try_from(read_limit).unwrap_or(u64::MAX))
        .read_to_end(&mut bytes)
        .map_err(|source| InputMediaError::Io {
            operation: "read FFmpeg output",
            source,
        })?;
    if bytes.len() > max_bytes {
        let _ = child.kill();
    }
    let status = child.wait().map_err(|source| InputMediaError::Io {
        operation: "wait for FFmpeg",
        source,
    })?;
    let stderr = stderr_thread.join().map_err(|_| InputMediaError::Io {
        operation: "collect FFmpeg diagnostics",
        source: io::Error::other("FFmpeg diagnostic reader panicked"),
    })?;
    if bytes.len() > max_bytes {
        return Err(InputMediaError::DemuxOutputTooLarge { limit: max_bytes });
    }
    Ok(CommandOutput {
        status,
        stdout: bytes,
        stderr,
    })
}

fn command_detail(stderr: &[u8]) -> String {
    let detail = String::from_utf8_lossy(stderr).trim().to_owned();
    if detail.is_empty() {
        "the external tool returned no diagnostics".to_owned()
    } else {
        detail
    }
}
