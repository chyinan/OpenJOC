// pattern: Imperative Shell

//! Input-media boundary for OpenJOC.
//!
//! This crate deliberately stops at container demuxing. E-AC-3 syncframes,
//! EMDF, JOC, and OAMD are parsed by the OpenJOC codec crates after this
//! boundary. FFmpeg is used only as an external ISO BMFF demuxer with audio
//! stream copy; it is not an audio decoder or a normative reference.

use openjoc_eac3::{
    Eac3Error, SyncframeHeader, group_access_units, index_syncframes, parse_syncframe_header,
};
use std::{
    fmt,
    fs::{self, File},
    io::{self, Read},
    path::Path,
    process::{Command, Stdio},
    thread,
};

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
        self.carry.resize(start + requested, 0);
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
    EmptyDemuxOutput,
    TruncatedRawEac3 {
        offset: usize,
        available: usize,
    },
    InvalidDemuxedEac3(Eac3Error),
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
        }
    }
}

impl std::error::Error for InputMediaError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::InvalidDemuxedEac3(error) => Some(error),
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
